//! Single-writer digest task.
//!
//! Owns the only "write side" of SQLite. HTTP handlers shovel `IngestEvent`s
//! into a channel; this task drains them, fingerprints, persists, and
//! broadcasts the resulting `IssueCreated`/`IssueUpdated` to subscribers.
//!
//! There is intentionally no in-memory cache — SQLite WAL reads are
//! sub-millisecond and an in-RAM mirror would just double the footprint.
//! The WS server queries the store directly when a client connects.

use chrono::Utc;
use errex_proto::{Event, Level, ServerMessage};
use tokio::sync::{broadcast, mpsc};

use crate::error::DaemonError;
use crate::fingerprint;
use crate::store::Store;
use crate::webhook;

/// What the ingest layer hands to the digest task.
#[derive(Debug, Clone)]
pub struct IngestEvent {
    pub project: String,
    pub event: Event,
}

pub async fn run(
    store: Store,
    mut events: mpsc::Receiver<IngestEvent>,
    fanout: broadcast::Sender<ServerMessage>,
    webhooks: mpsc::Sender<webhook::Trigger>,
) -> Result<(), DaemonError> {
    tracing::info!("digest task started");

    while let Some(IngestEvent { project, event }) = events.recv().await {
        let fp = fingerprint::derive(&event);
        let title = event.title();
        let now = Utc::now();
        let level = event.level.map(level_str);

        let exception_type = event
            .primary_exception()
            .and_then(|e| e.ty.clone())
            .unwrap_or_else(|| "<no exception>".to_string());
        let first_frame = event
            .primary_exception()
            .and_then(|e| e.first_frame())
            .map(|f| {
                format!(
                    "{}:{}",
                    f.function.as_deref().unwrap_or("?"),
                    f.lineno.unwrap_or(0)
                )
            })
            .unwrap_or_else(|| "<no frames>".to_string());
        let culprit = event
            .primary_exception()
            .and_then(|e| e.first_frame())
            .map(|f| {
                let func = f.function.as_deref().unwrap_or("?");
                let file = f.filename.as_deref().unwrap_or("?");
                format!("{func} in {file}")
            });

        // Persist the issue (DB owns the id) then append the raw event.
        // Per-event DB errors are logged and swallowed: losing one event is
        // preferable to crashing the digest task and losing every subsequent
        // event because no consumer is draining the channel.
        let upsert = match store
            .upsert_issue(
                &project,
                &fp,
                &title,
                culprit.as_deref(),
                level.as_deref(),
                now,
            )
            .await
        {
            Ok(r) => r,
            Err(err) => {
                tracing::error!(%err, project = %project, fingerprint = %fp, "digest: upsert_issue failed");
                continue;
            }
        };

        if let Err(err) = store.insert_event(upsert.issue.id, &event).await {
            tracing::warn!(%err, issue_id = upsert.issue.id, "digest: insert_event failed");
        }

        // A regression (a fresh event for an issue we'd marked resolved) is
        // log-worthy on its own — surface it as a warn! so on-call notices.
        if upsert.regressed {
            tracing::warn!(
                project = %project,
                issue_id = upsert.issue.id,
                fingerprint = %fp,
                "issue regressed: {title}",
            );
        } else {
            tracing::info!(
                project = %project,
                issue_id = upsert.issue.id,
                fingerprint = %fp,
                exception = %exception_type,
                frame = %first_frame,
                "ingest: {title}",
            );
        }

        // Fire a webhook trigger before we move `upsert.issue` into the
        // ServerMessage. New issues and regressions are notification-worthy;
        // a routine count bump on an existing unresolved issue is not.
        if upsert.created {
            let _ = webhooks.try_send(webhook::Trigger {
                issue: upsert.issue.clone(),
                kind: webhook::TriggerKind::NewIssue,
            });
        } else if upsert.regressed {
            let _ = webhooks.try_send(webhook::Trigger {
                issue: upsert.issue.clone(),
                kind: webhook::TriggerKind::Regression,
            });
        }

        let msg = if upsert.created {
            ServerMessage::IssueCreated {
                issue: upsert.issue,
            }
        } else {
            ServerMessage::IssueUpdated {
                issue: upsert.issue,
            }
        };
        // `send` returns Err only when no subscribers are connected, which
        // is fine — the snapshot will catch them up later.
        let _ = fanout.send(msg);
    }

    tracing::info!("digest task exiting (ingest channel closed)");
    Ok(())
}

fn level_str(l: Level) -> String {
    match l {
        Level::Debug => "debug",
        Level::Info => "info",
        Level::Warning => "warning",
        Level::Error => "error",
        Level::Fatal => "fatal",
    }
    .to_string()
}
