//! Outbound webhook delivery for issue alerts.
//!
//! The digest task pushes `Trigger`s onto an mpsc channel; this task drains
//! them, looks up the project's webhook URL, and POSTs a Slack/Discord/Teams-
//! compatible JSON payload. Failures are logged and dropped — webhook
//! delivery is best-effort, not transactional. (If you need durable
//! delivery, fan out to your own queue.)
//!
//! Lightweight: a single shared `reqwest::Client` (HTTP/2 keepalive, rustls
//! TLS) and one task. No per-call allocations beyond the JSON body.

use std::time::Duration;

use errex_proto::{Issue, IssueStatus};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::store::Store;

/// What the digest task fires when a webhook-worthy event happens.
#[derive(Debug, Clone)]
pub struct Trigger {
    pub issue: Issue,
    pub kind: TriggerKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerKind {
    /// First occurrence of a fingerprint — always notify.
    NewIssue,
    /// Resolved issue saw a fresh event — also notify.
    Regression,
}

impl TriggerKind {
    pub fn label(self) -> &'static str {
        match self {
            TriggerKind::NewIssue => "New issue",
            TriggerKind::Regression => "Regression",
        }
    }
    pub fn slack_color(self) -> &'static str {
        // Slack/Discord both interpret these named colors.
        match self {
            TriggerKind::NewIssue => "danger",
            TriggerKind::Regression => "warning",
        }
    }
}

/// Build a Slack-compatible message body. Discord and Teams' "Slack
/// compatible" webhook endpoints accept the same shape.
pub fn build_payload(t: &Trigger, public_url: &str) -> Value {
    let issue = &t.issue;
    let title = format!("{}: {}", t.kind.label(), issue.title);
    let link = format!("{}/issues/{}", public_url.trim_end_matches('/'), issue.id);
    let mut fields = vec![
        json!({"title": "Project", "value": issue.project, "short": true}),
        json!({"title": "Events",  "value": issue.event_count.to_string(), "short": true}),
    ];
    if let Some(level) = &issue.level {
        fields.push(json!({"title": "Level", "value": level, "short": true}));
    }
    if let Some(culprit) = &issue.culprit {
        fields.push(json!({"title": "Culprit", "value": culprit, "short": false}));
    }
    json!({
        "text": title,
        "attachments": [{
            "title": issue.title,
            "title_link": link,
            "color": t.kind.slack_color(),
            "fields": fields,
            "fallback": title,
            "ts": issue.last_seen.timestamp(),
        }]
    })
}

/// Spawned task. Reads triggers, drops them silently for muted/ignored
/// issues, looks up the project's webhook URL, fires the POST.
pub async fn run(store: Store, public_url: String, mut rx: mpsc::Receiver<Trigger>) {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent(concat!("errexd/", env!("CARGO_PKG_VERSION")))
        .build()
    {
        Ok(c) => c,
        Err(err) => {
            tracing::error!(%err, "webhook: failed to build HTTP client; webhooks disabled");
            return;
        }
    };

    tracing::info!("webhook task started");
    while let Some(trigger) = rx.recv().await {
        // Don't notify on muted/ignored issues — that's the whole point of
        // those statuses. Regression of a previously-resolved issue is
        // explicitly NOT muted because the regression event already flipped
        // it to unresolved before reaching here.
        if matches!(
            trigger.issue.status,
            IssueStatus::Muted | IssueStatus::Ignored
        ) {
            continue;
        }

        let url = match store.project_by_name(&trigger.issue.project).await {
            Ok(Some(p)) => p.webhook_url,
            Ok(None) => None,
            Err(err) => {
                tracing::warn!(%err, project = %trigger.issue.project, "webhook: project lookup failed");
                continue;
            }
        };
        let Some(url) = url else { continue };

        let body = build_payload(&trigger, &public_url);
        let res = client.post(&url).json(&body).send().await;
        // Surface the most recent delivery outcome on the project row so the
        // /projects/[name] console can render "last delivery: 200 · 12s ago"
        // (or 404, or "never delivered") without a separate history table.
        // Status 0 is the "transport failure" sentinel — see the migration.
        let status_code = match &res {
            Ok(r) => r.status().as_u16(),
            Err(_) => 0,
        };
        store
            .record_webhook_attempt(&trigger.issue.project, status_code)
            .await;
        match res {
            Ok(r) if r.status().is_success() => {
                tracing::info!(
                    project = %trigger.issue.project,
                    issue_id = trigger.issue.id,
                    kind = ?trigger.kind,
                    "webhook delivered",
                );
            }
            Ok(r) => {
                tracing::warn!(
                    project = %trigger.issue.project,
                    issue_id = trigger.issue.id,
                    status = %r.status(),
                    "webhook returned non-2xx",
                );
            }
            Err(err) => {
                tracing::warn!(%err, project = %trigger.issue.project, "webhook send failed");
            }
        }
    }
    tracing::info!("webhook task exiting");
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use errex_proto::Fingerprint;

    fn issue() -> Issue {
        Issue {
            id: 42,
            project: "demo".into(),
            fingerprint: Fingerprint::new("abc"),
            title: "TypeError: x".into(),
            culprit: Some("checkout in pay.ts".into()),
            level: Some("error".into()),
            status: IssueStatus::Unresolved,
            event_count: 7,
            first_seen: Utc::now(),
            last_seen: Utc::now(),
        }
    }

    #[test]
    fn payload_for_new_issue_uses_danger_color() {
        let t = Trigger {
            issue: issue(),
            kind: TriggerKind::NewIssue,
        };
        let v = build_payload(&t, "https://errex.example.com");
        let attachments = v.get("attachments").and_then(|a| a.as_array()).unwrap();
        assert_eq!(
            attachments[0].get("color").and_then(|c| c.as_str()),
            Some("danger")
        );
        assert!(v
            .get("text")
            .and_then(|t| t.as_str())
            .unwrap()
            .starts_with("New issue:"));
    }

    #[test]
    fn payload_for_regression_uses_warning_color() {
        let t = Trigger {
            issue: issue(),
            kind: TriggerKind::Regression,
        };
        let v = build_payload(&t, "https://errex.example.com");
        let attachments = v.get("attachments").and_then(|a| a.as_array()).unwrap();
        assert_eq!(
            attachments[0].get("color").and_then(|c| c.as_str()),
            Some("warning")
        );
        assert!(v
            .get("text")
            .and_then(|t| t.as_str())
            .unwrap()
            .starts_with("Regression:"));
    }

    #[test]
    fn payload_includes_issue_link() {
        let t = Trigger {
            issue: issue(),
            kind: TriggerKind::NewIssue,
        };
        let v = build_payload(&t, "https://errex.example.com/");
        let link = v
            .pointer("/attachments/0/title_link")
            .and_then(|l| l.as_str())
            .unwrap();
        assert_eq!(link, "https://errex.example.com/issues/42");
    }

    #[test]
    fn payload_includes_project_and_event_count_fields() {
        let t = Trigger {
            issue: issue(),
            kind: TriggerKind::NewIssue,
        };
        let v = build_payload(&t, "https://errex.example.com");
        let fields = v
            .pointer("/attachments/0/fields")
            .and_then(|f| f.as_array())
            .unwrap();
        let titles: Vec<_> = fields
            .iter()
            .filter_map(|f| f.get("title").and_then(|t| t.as_str()))
            .collect();
        assert!(titles.contains(&"Project"));
        assert!(titles.contains(&"Events"));
    }

    // ----- delivery + health recording (integration) -----
    //
    // These tests boot a one-shot axum server on a random port so the real
    // reqwest client in `run` actually performs an HTTP round-trip. We then
    // observe the side-effect on the Store, which is what the projects
    // console reads.

    use crate::store::Store;
    use axum::{routing::post, Router};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU16, Ordering};
    use std::sync::Arc;
    use std::time::Duration;
    use tokio::net::TcpListener;

    fn unique_tempdir() -> PathBuf {
        let p = std::env::temp_dir().join(format!(
            "errexd-webhook-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    /// Boot a tiny HTTP server that returns the configured status code for
    /// every POST. Returns the bound URL plus a JoinHandle the test can drop
    /// to terminate the server.
    async fn spawn_mock(status: u16) -> (String, tokio::task::JoinHandle<()>) {
        let code = Arc::new(AtomicU16::new(status));
        let code_for_handler = code.clone();
        let app = Router::new().route(
            "/hook",
            post(move || {
                let code = code_for_handler.clone();
                async move {
                    axum::http::StatusCode::from_u16(code.load(Ordering::Relaxed))
                        .unwrap_or(axum::http::StatusCode::OK)
                }
            }),
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        (format!("http://{addr}/hook"), handle)
    }

    /// Build an Issue whose project name matches the one we'll create in the
    /// store, so the webhook task's project lookup succeeds.
    fn issue_for(project: &str) -> Issue {
        let mut i = issue();
        i.project = project.to_string();
        i
    }

    /// Poll the project row up to 1s waiting for `last_webhook_status` to be
    /// non-None. Returns the observed status. Avoids a hard-coded sleep.
    async fn wait_for_status(store: &Store, project: &str) -> Option<i16> {
        for _ in 0..50 {
            let p = store.project_by_name(project).await.unwrap().unwrap();
            if p.last_webhook_status.is_some() {
                return p.last_webhook_status;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        None
    }

    #[tokio::test]
    async fn successful_delivery_records_status_200() {
        let dir = unique_tempdir();
        let store = Store::open(&dir.join("errex.db")).await.unwrap();
        store.migrate().await.unwrap();
        store.create_project("p").await.unwrap();
        let (url, _server) = spawn_mock(200).await;
        store.set_project_webhook("p", Some(&url)).await.unwrap();

        let (tx, rx) = mpsc::channel(4);
        let task = tokio::spawn(run(store.clone(), "http://example".into(), rx));

        tx.send(Trigger {
            issue: issue_for("p"),
            kind: TriggerKind::NewIssue,
        })
        .await
        .unwrap();

        assert_eq!(wait_for_status(&store, "p").await, Some(200));
        drop(tx);
        let _ = task.await;
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn non_2xx_response_records_actual_status_code() {
        let dir = unique_tempdir();
        let store = Store::open(&dir.join("errex.db")).await.unwrap();
        store.migrate().await.unwrap();
        store.create_project("p").await.unwrap();
        let (url, _server) = spawn_mock(502).await;
        store.set_project_webhook("p", Some(&url)).await.unwrap();

        let (tx, rx) = mpsc::channel(4);
        let task = tokio::spawn(run(store.clone(), "http://example".into(), rx));

        tx.send(Trigger {
            issue: issue_for("p"),
            kind: TriggerKind::NewIssue,
        })
        .await
        .unwrap();

        assert_eq!(
            wait_for_status(&store, "p").await,
            Some(502),
            "console must reflect the upstream's failure status, not 200",
        );
        drop(tx);
        let _ = task.await;
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn unreachable_endpoint_records_zero() {
        let dir = unique_tempdir();
        let store = Store::open(&dir.join("errex.db")).await.unwrap();
        store.migrate().await.unwrap();
        store.create_project("p").await.unwrap();
        // Reserved address that should refuse immediately on most hosts.
        // The webhook client has a 10s timeout but a connection-refused
        // returns much faster; the test still bounds via wait_for_status.
        store
            .set_project_webhook("p", Some("http://127.0.0.1:1/hook"))
            .await
            .unwrap();

        let (tx, rx) = mpsc::channel(4);
        let task = tokio::spawn(run(store.clone(), "http://example".into(), rx));

        tx.send(Trigger {
            issue: issue_for("p"),
            kind: TriggerKind::NewIssue,
        })
        .await
        .unwrap();

        // Generous 5s for transport failure to land — refused on most hosts
        // but slower under contention or strict firewalls.
        let mut observed = None;
        for _ in 0..250 {
            let p = store.project_by_name("p").await.unwrap().unwrap();
            if p.last_webhook_status.is_some() {
                observed = p.last_webhook_status;
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(
            observed,
            Some(0),
            "transport failure must record sentinel 0"
        );

        drop(tx);
        let _ = task.await;
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn no_webhook_url_does_not_record_anything() {
        // A project with no webhook URL must leave last_webhook_status as
        // None — otherwise the console can't tell "never tried" from
        // "tried and failed".
        let dir = unique_tempdir();
        let store = Store::open(&dir.join("errex.db")).await.unwrap();
        store.migrate().await.unwrap();
        store.create_project("p").await.unwrap();

        let (tx, rx) = mpsc::channel(4);
        let task = tokio::spawn(run(store.clone(), "http://example".into(), rx));

        tx.send(Trigger {
            issue: issue_for("p"),
            kind: TriggerKind::NewIssue,
        })
        .await
        .unwrap();

        // Give the task time to drain the channel and skip the delivery.
        tokio::time::sleep(Duration::from_millis(100)).await;
        let p = store.project_by_name("p").await.unwrap().unwrap();
        assert!(p.last_webhook_status.is_none());

        drop(tx);
        let _ = task.await;
        let _ = std::fs::remove_dir_all(&dir);
    }
}
