//! Single-writer digest task.
//!
//! Owns the only "write side" of SQLite. HTTP handlers shovel `IngestEvent`s
//! into a channel; this task drains them, fingerprints, persists, and
//! broadcasts the resulting `IssueCreated`/`IssueUpdated` to subscribers.
//!
//! There is intentionally no in-memory cache — SQLite WAL reads are
//! sub-millisecond and an in-RAM mirror would just double the footprint.
//! The WS server queries the store directly when a client connects.

use chrono::{DateTime, Utc};
use errex_proto::{Event, Fingerprint, Level, ServerMessage};
use tokio::sync::{broadcast, mpsc};

use crate::error::DaemonError;
use crate::fingerprint;
use crate::store::{BatchUpsertInput, Store, UpsertResult};
use crate::webhook;

/// Max events per digest batch. The single-COMMIT-per-batch is what makes
/// batching worth it under WAL+synchronous=NORMAL: one fsync amortized
/// over up to N events.
///
/// Stress-tested at 32 — increasing it bumps the saturation throughput
/// but enlarges the worst-case tail when a checkpoint stalls a big tx.
const BATCH_SIZE: usize = 16;

/// What the ingest layer hands to the digest task.
///
/// Everything that requires touching the raw `Event` (fingerprint,
/// title, level, culprit, JSON serialization, event_id stringification)
/// is precomputed in the HTTP handler via [`prepare`]. The digest task
/// then only does SQLite I/O. Per-event JSON serialization showed up on
/// the digest's hot path under saturation; moving it off the writer
/// lifts the throughput plateau.
#[derive(Debug, Clone)]
pub struct IngestEvent {
    pub project: String,
    pub fingerprint: Fingerprint,
    pub title: String,
    pub level: Option<String>,
    pub culprit: Option<String>,
    pub exception_type: String,
    pub first_frame: String,
    pub received_at: DateTime<Utc>,
    /// SDK event_id pre-stringified so the writer doesn't pay
    /// `Uuid::to_string` per insert.
    pub event_id: String,
    /// Pre-serialized event JSON for the `events.payload` column.
    pub payload: String,
}

/// Build a digest-ready record from a raw `Event` parsed off the wire.
/// Cheap (small string allocs + one fingerprint hash + one JSON
/// serialize); intentionally runs in the HTTP handler instead of the
/// digest task so the digest's only blocking work is SQLite I/O.
pub fn prepare(project: String, event: Event) -> IngestEvent {
    let fingerprint = fingerprint::derive(&event);
    let title = event.title();
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

    let event_id = event.event_id.simple().to_string();
    // `serde_json::to_string` is infallible here — `Event` is a struct of
    // strings/numbers/nested options, no map keys whose Display can
    // fail. The `expect` documents the invariant rather than papering
    // over a real error.
    let payload =
        serde_json::to_string(&event).expect("Event always serializes (no non-string map keys)");

    IngestEvent {
        project,
        fingerprint,
        title,
        level,
        culprit,
        exception_type,
        first_frame,
        received_at: Utc::now(),
        event_id,
        payload,
    }
}

pub async fn run(
    store: Store,
    mut events: mpsc::Receiver<IngestEvent>,
    fanout: broadcast::Sender<ServerMessage>,
    webhooks: mpsc::Sender<webhook::Trigger>,
) -> Result<(), DaemonError> {
    tracing::info!("digest task started");

    let mut buf: Vec<IngestEvent> = Vec::with_capacity(BATCH_SIZE);
    loop {
        // Block on the first event in this batch.
        let first = match events.recv().await {
            Some(e) => e,
            None => break,
        };
        buf.push(first);

        // Opportunistically drain whatever else is already queued. We
        // intentionally do NOT wait — under low load the channel is
        // empty after the first recv and we dispatch a batch of one
        // immediately. Under sustained load the channel is rarely
        // empty after the previous batch's commit, so each batch fills
        // close to BATCH_SIZE without us synthesizing latency.
        while buf.len() < BATCH_SIZE {
            match events.try_recv() {
                Ok(e) => buf.push(e),
                Err(_) => break,
            }
        }

        process_batch(&store, &fanout, &webhooks, &mut buf).await;
    }

    tracing::info!("digest task exiting (ingest channel closed)");
    Ok(())
}

/// Persist a batch of `IngestEvent`s under one transaction, then broadcast
/// one ServerMessage per result. Empties `buf` when done. Per-batch DB
/// errors are logged and the batch is dropped — the next batch starts
/// fresh, the digest task does not exit on transient SQLite errors.
async fn process_batch(
    store: &Store,
    fanout: &broadcast::Sender<ServerMessage>,
    webhooks: &mpsc::Sender<webhook::Trigger>,
    buf: &mut Vec<IngestEvent>,
) {
    if buf.is_empty() {
        return;
    }
    let inputs: Vec<BatchUpsertInput<'_>> = buf
        .iter()
        .map(|rec| BatchUpsertInput {
            project: &rec.project,
            fp: &rec.fingerprint,
            title: &rec.title,
            culprit: rec.culprit.as_deref(),
            level: rec.level.as_deref(),
            now: rec.received_at,
            event_id: &rec.event_id,
            payload: &rec.payload,
        })
        .collect();

    let results: Vec<UpsertResult> = match store.upsert_batch_with_events(&inputs).await {
        Ok(r) => r,
        Err(err) => {
            // The whole batch was rolled back. Log loudly and drop them
            // — better than crashing the digest task and stranding every
            // subsequent event on the channel.
            tracing::error!(%err, batch_size = buf.len(), "digest: upsert_batch failed");
            buf.clear();
            return;
        }
    };

    for (rec, result) in buf.iter().zip(results) {
        if result.regressed {
            tracing::warn!(
                project = %rec.project,
                issue_id = result.issue.id,
                fingerprint = %rec.fingerprint,
                "issue regressed: {}",
                rec.title,
            );
        } else {
            tracing::info!(
                project = %rec.project,
                issue_id = result.issue.id,
                fingerprint = %rec.fingerprint,
                exception = %rec.exception_type,
                frame = %rec.first_frame,
                "ingest: {}",
                rec.title,
            );
        }

        if result.created {
            let _ = webhooks.try_send(webhook::Trigger {
                issue: result.issue.clone(),
                kind: webhook::TriggerKind::NewIssue,
            });
        } else if result.regressed {
            let _ = webhooks.try_send(webhook::Trigger {
                issue: result.issue.clone(),
                kind: webhook::TriggerKind::Regression,
            });
        }

        let msg = if result.created {
            ServerMessage::IssueCreated {
                issue: result.issue,
            }
        } else {
            ServerMessage::IssueUpdated {
                issue: result.issue,
            }
        };
        let _ = fanout.send(msg);
    }
    buf.clear();
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

#[cfg(test)]
mod tests {
    use super::*;

    fn event_with_exception(ty: &str, func: &str, lineno: u32) -> Event {
        let raw = format!(
            r#"{{"timestamp":"2026-01-01T00:00:00Z","level":"error","exception":{{"values":[{{"type":"{ty}","value":"v","stacktrace":{{"frames":[{{"function":"{func}","filename":"src/x.rs","lineno":{lineno},"in_app":true}}]}}}}]}}}}"#
        );
        serde_json::from_str(&raw).expect("valid event JSON")
    }

    #[test]
    fn prepare_fills_derived_fields() {
        let ev = event_with_exception("Boom", "f", 42);
        let rec = prepare("p1".into(), ev);
        assert_eq!(rec.project, "p1");
        assert_eq!(rec.exception_type, "Boom");
        assert_eq!(rec.first_frame, "f:42");
        assert!(rec.title.contains("Boom"));
        assert_eq!(rec.level.as_deref(), Some("error"));
        // Same input → same fingerprint (algorithm is deterministic).
        let ev2 = event_with_exception("Boom", "f", 42);
        let rec2 = prepare("p1".into(), ev2);
        assert_eq!(rec.fingerprint, rec2.fingerprint);
    }

    #[test]
    fn prepare_handles_event_without_exception() {
        let raw = r#"{"timestamp":"2026-01-01T00:00:00Z","message":"hi"}"#;
        let ev: Event = serde_json::from_str(raw).expect("valid event JSON");
        let rec = prepare("p1".into(), ev);
        assert_eq!(rec.exception_type, "<no exception>");
        assert_eq!(rec.first_frame, "<no frames>");
        assert_eq!(rec.level, None);
        assert_eq!(rec.title, "hi");
    }
}
