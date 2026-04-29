//! Outbound webhook delivery for issue alerts.
//!
//! The digest task pushes `Trigger`s onto an mpsc channel; this task drains
//! them, looks up the project's webhook URL, and POSTs a Slack/Discord/Teams-
//! compatible JSON payload. Failures are logged and dropped — webhook
//! delivery is best-effort, not transactional. (If you need durable
//! delivery, fan out to your own queue.)
//!
//! Lightweight: a single shared hyper Client (HTTP/1, rustls + webpki-roots,
//! ring crypto) and one task. No reqwest — the builder/multipart/cookie/
//! redirect machinery was dead weight at idle for the fire-and-forget JSON
//! POSTs we actually issue.

use std::net::IpAddr;
use std::time::Duration;

use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::{Method, Request};
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use url::Url;

use errex_proto::{Issue, IssueStatus};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::store::Store;

type WebhookClient = Client<HttpsConnector<HttpConnector>, Full<Bytes>>;

fn build_client() -> WebhookClient {
    let mut http = HttpConnector::new();
    // Bound the TCP handshake — webhooks are not a load-bearing path,
    // so a slow connect must not stall the task. Mirrors the prior
    // `reqwest::ClientBuilder::connect_timeout(2s)`.
    http.set_connect_timeout(Some(Duration::from_secs(2)));
    http.enforce_http(false);

    let https: HttpsConnector<HttpConnector> = HttpsConnectorBuilder::new()
        .with_webpki_roots()
        .https_or_http()
        .enable_http1()
        .wrap_connector(http);

    Client::builder(TokioExecutor::new())
        .pool_idle_timeout(Duration::from_secs(30))
        .build(https)
}

/// Total per-call deadline, including DNS, TCP, TLS, and the response.
/// Webhooks redirect to `https://hooks.slack.com/...` style endpoints
/// where 10 s is generous; anything slower is failing somewhere.
const SEND_TIMEOUT: Duration = Duration::from_secs(10);

async fn post_json(client: &WebhookClient, url: &str, body: &Value) -> Result<u16, String> {
    let body_bytes = serde_json::to_vec(body).map_err(|e| format!("serialize body: {e}"))?;
    let req = Request::builder()
        .method(Method::POST)
        .uri(url)
        .header("content-type", "application/json")
        .header("user-agent", concat!("errexd/", env!("CARGO_PKG_VERSION")))
        .body(Full::new(Bytes::from(body_bytes)))
        .map_err(|e| format!("build request: {e}"))?;

    let send = client.request(req);
    let resp = match tokio::time::timeout(SEND_TIMEOUT, send).await {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => return Err(format!("send: {e}")),
        Err(_) => return Err("timeout".into()),
    };
    let status = resp.status().as_u16();
    // Drain the body so the connection can be reused. Best-effort —
    // we don't surface body content anywhere.
    let _ = resp.into_body().collect().await;
    Ok(status)
}

/// Validate a configured webhook URL before storing it. The webhook task
/// will POST to whatever lands here, so this is the SSRF gate: a hostile
/// admin (or hijacked admin cookie) that could otherwise pivot to AWS
/// IMDS, GCP metadata, internal services on the same host, or RFC1918
/// neighbors is rejected up front.
///
/// Checks (synchronous, no DNS to keep tests hermetic):
///   * scheme must be `http` or `https`
///   * URL must not embed userinfo (`http://user:pass@…`)
///   * if the host is an IP literal, it must not be in a private,
///     loopback, link-local, multicast, broadcast, or unspecified range
///   * if the host is a name, common loopback / internal-zone TLDs are
///     refused (`localhost`, `.local`, `.internal`, `.lan`, `.intranet`,
///     `.corp`, `.home.arpa`)
///
/// Known limitation: a public hostname whose authoritative DNS returns
/// an RFC1918 address (DNS rebinding) still slips through this check.
/// Mitigation belongs in the delivery path (custom resolver pinning the
/// validated IP) — flagged for follow-up.
pub fn validate_url(raw: &str) -> Result<(), &'static str> {
    let url = Url::parse(raw).map_err(|_| "invalid url")?;
    match url.scheme() {
        "http" | "https" => {}
        _ => return Err("scheme must be http or https"),
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("userinfo not allowed in webhook url");
    }
    let host = url.host_str().ok_or("missing host")?;
    // host_str() returns IPv6 hosts wrapped in `[…]` literals; strip them
    // so `parse::<IpAddr>()` recognises the address.
    let host_for_parse = host.trim_start_matches('[').trim_end_matches(']');
    if let Ok(ip) = host_for_parse.parse::<IpAddr>() {
        if ip_is_private_or_local(ip) {
            return Err("private, loopback, or link-local addresses not allowed");
        }
        return Ok(());
    }
    let lower = host.to_ascii_lowercase();
    if lower == "localhost" || hostname_in_internal_zone(&lower) {
        return Err("loopback / internal-zone hostnames not allowed");
    }
    Ok(())
}

fn hostname_in_internal_zone(host: &str) -> bool {
    const INTERNAL_SUFFIXES: &[&str] = &[
        ".localhost",
        ".local",
        ".internal",
        ".lan",
        ".intranet",
        ".corp",
        ".home.arpa",
    ];
    INTERNAL_SUFFIXES.iter().any(|s| host.ends_with(s))
}

fn ip_is_private_or_local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_multicast()
                || v4.is_unspecified()
                || v4.is_broadcast()
                // CGNAT 100.64.0.0/10 — common cloud carrier-internal range
                || (v4.octets()[0] == 100 && (v4.octets()[1] & 0b1100_0000) == 64)
        }
        IpAddr::V6(v6) => {
            let segs = v6.segments();
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                // unique local fc00::/7
                || (segs[0] & 0xfe00) == 0xfc00
                // link-local fe80::/10
                || (segs[0] & 0xffc0) == 0xfe80
        }
    }
}

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
    // Hyper Client does not follow redirects on its own — that matches the
    // SSRF policy: a 30x pointing at an internal address would re-do the
    // pivot the configured-URL validator just refused. Followers can update
    // their endpoint instead of relying on us to chase 302s.
    let client = build_client();

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
        let res = post_json(&client, &url, &body).await;
        // Surface the most recent delivery outcome on the project row so the
        // /projects/[name] console can render "last delivery: 200 · 12s ago"
        // (or 404, or "never delivered") without a separate history table.
        // Status 0 is the "transport failure" sentinel — see the migration.
        let status_code = match &res {
            Ok(code) => *code,
            Err(_) => 0,
        };
        store
            .record_webhook_attempt(&trigger.issue.project, status_code)
            .await;
        match res {
            Ok(code) if (200..300).contains(&code) => {
                tracing::info!(
                    project = %trigger.issue.project,
                    issue_id = trigger.issue.id,
                    kind = ?trigger.kind,
                    "webhook delivered",
                );
            }
            Ok(code) => {
                tracing::warn!(
                    project = %trigger.issue.project,
                    issue_id = trigger.issue.id,
                    status = code,
                    "webhook returned non-2xx",
                );
            }
            Err(err) => {
                tracing::warn!(err, project = %trigger.issue.project, "webhook send failed");
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

    // ----- validate_url -----
    //
    // SSRF gate. Each rejected case below is a real-world pivot we don't
    // want a hostile (or hijacked) admin to be able to fire from the
    // webhook config endpoint.

    #[test]
    fn validate_url_accepts_public_https() {
        assert!(validate_url("https://hooks.slack.com/services/ABC/DEF").is_ok());
        assert!(validate_url("https://discord.com/api/webhooks/x/y").is_ok());
        assert!(validate_url("http://example.com:8080/hook").is_ok());
    }

    #[test]
    fn validate_url_rejects_loopback_ipv4() {
        assert!(validate_url("http://127.0.0.1/hook").is_err());
        assert!(validate_url("http://127.0.0.1:9090/hook").is_err());
    }

    #[test]
    fn validate_url_rejects_aws_imds() {
        // 169.254.169.254 — the most common cloud-metadata SSRF target.
        assert!(validate_url("http://169.254.169.254/latest/meta-data/").is_err());
    }

    #[test]
    fn validate_url_rejects_rfc1918_ipv4() {
        assert!(validate_url("http://10.0.0.1/").is_err());
        assert!(validate_url("http://192.168.1.50/x").is_err());
        assert!(validate_url("http://172.16.0.1/x").is_err());
    }

    #[test]
    fn validate_url_rejects_unspecified_and_broadcast() {
        assert!(validate_url("http://0.0.0.0/").is_err());
        assert!(validate_url("http://255.255.255.255/").is_err());
    }

    #[test]
    fn validate_url_rejects_loopback_ipv6() {
        assert!(validate_url("http://[::1]/").is_err());
    }

    #[test]
    fn validate_url_rejects_unique_local_ipv6() {
        assert!(validate_url("http://[fc00::1]/").is_err());
        assert!(validate_url("http://[fd12:3456::1]/").is_err());
    }

    #[test]
    fn validate_url_rejects_link_local_ipv6() {
        assert!(validate_url("http://[fe80::1]/").is_err());
    }

    #[test]
    fn validate_url_rejects_localhost_hostname() {
        assert!(validate_url("http://localhost/x").is_err());
        assert!(validate_url("http://localhost:9090/x").is_err());
    }

    #[test]
    fn validate_url_rejects_internal_zone_tlds() {
        assert!(validate_url("http://api.internal/").is_err());
        assert!(validate_url("http://server.local/").is_err());
        assert!(validate_url("http://x.lan/").is_err());
        assert!(validate_url("http://app.intranet/").is_err());
        assert!(validate_url("http://a.b.localhost/").is_err());
    }

    #[test]
    fn validate_url_rejects_userinfo() {
        assert!(validate_url("http://user:pass@example.com/").is_err());
        assert!(validate_url("http://user@example.com/").is_err());
    }

    #[test]
    fn validate_url_rejects_non_http_schemes() {
        assert!(validate_url("file:///etc/passwd").is_err());
        assert!(validate_url("ftp://example.com/").is_err());
        assert!(validate_url("gopher://example.com/").is_err());
        // `data:` URIs would never reach here as a webhook, but a careless
        // operator paste shouldn't be allowed to misconfigure either.
        assert!(validate_url("data:text/plain,hi").is_err());
    }

    #[test]
    fn validate_url_rejects_unparseable_input() {
        assert!(validate_url("not a url").is_err());
        assert!(validate_url("").is_err());
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
