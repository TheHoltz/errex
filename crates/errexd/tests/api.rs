//! HTTP API integration tests.
//!
//! We build the router with a real `AppState` (real Store + a mpsc channel
//! whose receiver is kept alive by the test) and fire requests through
//! `tower::ServiceExt::oneshot`. No live network listener — that means tests
//! run instantly even on Mac sandboxed runners.

// `#[path]`-imported source modules each have items unused in the API test
// scope (digest::run, ingest::serve, store helpers we don't exercise here).
// Silence per-binary dead_code rather than tagging each fn — the main bin
// and `tests/store.rs` both prove these are used.
#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::Utc;
use errex_proto::{Fingerprint, IssueStatus};
use http_body_util::BodyExt;
use serde_json::Value;
use tokio::sync::mpsc;
use tower::util::ServiceExt;

#[path = "../src/auth.rs"]
mod auth;
#[path = "../src/crypto.rs"]
mod crypto;
#[path = "../src/digest.rs"]
mod digest;
#[path = "../src/error.rs"]
mod error;
#[path = "../src/fingerprint.rs"]
mod fingerprint;
#[path = "../src/ingest.rs"]
mod ingest;
#[path = "../src/lockout.rs"]
mod lockout;
#[path = "../src/rate_limit.rs"]
mod rate_limit;
#[path = "../src/spa.rs"]
mod spa;
#[path = "../src/store.rs"]
mod store;
#[path = "../src/webhook.rs"]
mod webhook;
#[path = "../src/ws.rs"]
mod ws;

use ingest::AppState;
use store::Store;

fn unique_tempdir() -> PathBuf {
    let p = std::env::temp_dir().join(format!(
        "errexd-api-{}-{}",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("create tempdir");
    p
}

async fn fixture() -> (axum::Router, Store, PathBuf) {
    fixture_full(false, 0, 200, None).await
}

async fn fixture_with_auth(require_auth: bool) -> (axum::Router, Store, PathBuf) {
    fixture_full(require_auth, 0, 200, None).await
}

async fn fixture_with_rate_limit(per_min: u32, burst: u32) -> (axum::Router, Store, PathBuf) {
    fixture_full(false, per_min, burst, None).await
}

async fn fixture_with_admin(setup_token: &str) -> (axum::Router, Store, PathBuf) {
    fixture_full(false, 0, 200, Some(setup_token.to_string())).await
}

async fn fixture_full(
    require_auth: bool,
    rate_limit_per_min: u32,
    rate_limit_burst: u32,
    setup_token: Option<String>,
) -> (axum::Router, Store, PathBuf) {
    let dir = unique_tempdir();
    let store = Store::open(&dir.join("errex.db")).await.unwrap();
    store.migrate().await.unwrap();

    let (tx, rx) = mpsc::channel(16);
    let (fanout_tx, _fanout_rx) = tokio::sync::broadcast::channel(8);
    let state = Arc::new(AppState {
        events: tx,
        store: store.clone(),
        fanout: fanout_tx,
        require_auth,
        rate_limiter: Arc::new(rate_limit::RateLimiter::new(
            rate_limit_per_min,
            rate_limit_burst,
        )),
        setup_token,
        public_url: "http://test.local:9090".to_string(),
        // Tests don't need cookies' Secure flag since we never round-trip
        // through a real browser; dev_mode keeps the cookie attribute set
        // simple so the helper-based assertion is stable.
        dev_mode: true,
    });
    let router = ingest::build_router(state);
    tokio::spawn(async move {
        let mut rx = rx;
        while rx.recv().await.is_some() {}
    });
    (router, store, dir)
}

async fn body_json(resp: axum::response::Response) -> Value {
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

// ----- /api/projects -----

#[tokio::test]
async fn list_projects_returns_empty_array_when_no_issues() {
    let (router, _store, _dir) = fixture().await;
    let res = router
        .oneshot(
            Request::builder()
                .uri("/api/projects")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    assert_eq!(v, serde_json::json!([]));
}

#[tokio::test]
async fn list_projects_groups_by_project() {
    let (router, store, _dir) = fixture().await;
    let now = Utc::now();
    store
        .upsert_issue("alpha", &Fingerprint::new("a"), "T", None, None, now)
        .await
        .unwrap();
    store
        .upsert_issue("beta", &Fingerprint::new("b"), "T", None, None, now)
        .await
        .unwrap();
    let res = router
        .oneshot(
            Request::builder()
                .uri("/api/projects")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let v = body_json(res).await;
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 2);
}

// ----- /api/issues -----

#[tokio::test]
async fn list_issues_includes_status_field() {
    // The SPA's filter UI requires `status` on every Issue. Pin it on the
    // wire path so a refactor that drops the column from the SELECT trips
    // a red bar.
    let (router, store, _dir) = fixture().await;
    store
        .upsert_issue("p", &Fingerprint::new("a"), "T", None, None, Utc::now())
        .await
        .unwrap();
    let res = router
        .oneshot(
            Request::builder()
                .uri("/api/issues")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let v = body_json(res).await;
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(
        arr[0].get("status").and_then(|s| s.as_str()),
        Some("unresolved")
    );
}

// ----- PUT /api/issues/:id/status -----

#[tokio::test]
async fn put_status_updates_issue() {
    let (router, store, _dir) = fixture().await;
    let r = store
        .upsert_issue("p", &Fingerprint::new("a"), "T", None, None, Utc::now())
        .await
        .unwrap();

    let res = router
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/issues/{}/status", r.issue.id))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"status":"resolved"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    assert_eq!(v.get("status").and_then(|s| s.as_str()), Some("resolved"));
    assert_eq!(v.get("id").and_then(|s| s.as_i64()), Some(r.issue.id));

    // Persisted: a follow-up GET reflects the new status.
    let issues = store.load_issues().await.unwrap();
    assert_eq!(issues[0].status, IssueStatus::Resolved);
}

#[tokio::test]
async fn put_status_returns_404_for_unknown_id() {
    let (router, _store, _dir) = fixture().await;
    let res = router
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/issues/999999/status")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"status":"resolved"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn put_status_rejects_unknown_status() {
    let (router, store, _dir) = fixture().await;
    let r = store
        .upsert_issue("p", &Fingerprint::new("a"), "T", None, None, Utc::now())
        .await
        .unwrap();
    let res = router
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!("/api/issues/{}/status", r.issue.id))
                .header("content-type", "application/json")
                .body(Body::from(r#"{"status":"banana"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

// ----- DSN auth -----

fn sample_envelope_body() -> Body {
    let now = chrono::Utc::now().to_rfc3339();
    let payload = format!(
        r#"{{"event_id":"00000000000000000000000000000001","sent_at":"{now}"}}
{{"type":"event"}}
{{"timestamp":"{now}","exception":{{"values":[{{"type":"E","value":"v"}}]}}}}
"#
    );
    Body::from(payload)
}

#[tokio::test]
async fn ingest_without_auth_required_accepts_anonymous_post() {
    let (router, _store, _dir) = fixture_with_auth(false).await;
    let res = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/anyproject/envelope/")
                .body(sample_envelope_body())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn ingest_with_auth_required_rejects_missing_token() {
    let (router, store, _dir) = fixture_with_auth(true).await;
    let _ = store.create_project("p").await.unwrap();
    let res = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/p/envelope/")
                .body(sample_envelope_body())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn ingest_with_auth_required_accepts_valid_x_sentry_auth_header() {
    let (router, store, _dir) = fixture_with_auth(true).await;
    let p = store.create_project("p").await.unwrap();
    // Sentry SDKs send a comma-separated key=value header. We accept any
    // formatting as long as `sentry_key=<token>` appears.
    let header = format!("Sentry sentry_version=7, sentry_key={}", p.token);
    let res = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/p/envelope/")
                .header("x-sentry-auth", header)
                .body(sample_envelope_body())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn ingest_with_auth_required_accepts_query_param_token() {
    let (router, store, _dir) = fixture_with_auth(true).await;
    let p = store.create_project("p").await.unwrap();
    let res = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/p/envelope/?sentry_key={}", p.token))
                .body(sample_envelope_body())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn ingest_with_auth_required_rejects_token_for_wrong_project() {
    let (router, store, _dir) = fixture_with_auth(true).await;
    let alpha = store.create_project("alpha").await.unwrap();
    store.create_project("beta").await.unwrap();
    let res = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/beta/envelope/") // posting to beta with alpha's token
                .header(
                    "x-sentry-auth",
                    format!("Sentry sentry_key={}", alpha.token),
                )
                .body(sample_envelope_body())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn ingest_with_auth_required_rejects_unknown_project() {
    let (router, _store, _dir) = fixture_with_auth(true).await;
    let res = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/ghost/envelope/")
                .header("x-sentry-auth", "Sentry sentry_key=anything")
                .body(sample_envelope_body())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

// ----- rate limit -----

#[tokio::test]
async fn ingest_returns_429_when_rate_limit_exceeded() {
    // 60/min = 1/sec, burst of 2 → 3rd request inside the same second denies.
    let (router, _store, _dir) = fixture_with_rate_limit(60, 2).await;

    async fn fire(router: &axum::Router) -> StatusCode {
        router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/p/envelope/")
                    .body(sample_envelope_body())
                    .unwrap(),
            )
            .await
            .unwrap()
            .status()
    }

    assert_eq!(fire(&router).await, StatusCode::OK);
    assert_eq!(fire(&router).await, StatusCode::OK);
    assert_eq!(
        fire(&router).await,
        StatusCode::TOO_MANY_REQUESTS,
        "3rd within burst window must be 429"
    );
}

#[tokio::test]
async fn ingest_with_rate_limit_zero_is_unlimited() {
    let (router, _store, _dir) = fixture_with_rate_limit(0, 0).await;
    for _ in 0..50 {
        let res = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/p/envelope/")
                    .body(sample_envelope_body())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }
}

// ----- admin endpoints -----
//
// Auth is now cookie-based (`errex_session`) gated on an admin role. We mint
// the session directly via Store helpers so tests don't need to round-trip
// through the login endpoint, and the Bearer-header path is gone.

const ADMIN_TOKEN: &str = "admin-secret-xyz"; // setup secret (env-var equivalent)

/// Creates a user + session in `store` and returns the cookie string the
/// test should send with subsequent admin requests.
async fn signed_in_cookie(store: &Store, role: store::Role) -> String {
    let username = match role {
        store::Role::Admin => "test-admin",
        store::Role::Viewer => "test-viewer",
    };
    // Hash anything; tests never log in via password — they mint a session
    // straight onto a fresh user.
    store
        .create_user(username, "stored-hash", role)
        .await
        .unwrap();
    let session_id = format!(
        "test-session-{}-{}",
        username,
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    );
    store
        .create_session(&session_id, username, Some("127.0.0.1"), None)
        .await
        .unwrap();
    format!("errex_session={session_id}")
}

async fn admin_cookie(store: &Store) -> String {
    signed_in_cookie(store, store::Role::Admin).await
}

async fn admin_get(
    router: &axum::Router,
    path: &str,
    cookie: Option<&str>,
) -> axum::response::Response {
    let mut req = Request::builder().method("GET").uri(path);
    if let Some(c) = cookie {
        req = req.header("cookie", c);
    }
    router
        .clone()
        .oneshot(req.body(Body::empty()).unwrap())
        .await
        .unwrap()
}

async fn admin_send(
    router: &axum::Router,
    method: &str,
    path: &str,
    cookie: Option<&str>,
    body: &str,
) -> axum::response::Response {
    let mut req = Request::builder().method(method).uri(path);
    if let Some(c) = cookie {
        req = req.header("cookie", c);
    }
    req = req.header("content-type", "application/json");
    router
        .clone()
        .oneshot(req.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap()
}

#[tokio::test]
async fn admin_endpoints_reject_missing_cookie() {
    let (router, _store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let res = admin_get(&router, "/api/admin/projects", None).await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_endpoints_reject_unknown_session() {
    let (router, _store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let res = admin_get(&router, "/api/admin/projects", Some("errex_session=ghost")).await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_endpoints_reject_viewer_role_with_403() {
    // The cookie is valid but the user is only a viewer — admin endpoints
    // require admin. This is the "I logged in but I'm not allowed" case.
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let viewer_cookie = signed_in_cookie(&store, store::Role::Viewer).await;
    let res = admin_get(&router, "/api/admin/projects", Some(&viewer_cookie)).await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn admin_list_projects_returns_array_with_dsn() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let cookie = admin_cookie(&store).await;
    let p = store.create_project("alpha").await.unwrap();
    let res = admin_get(&router, "/api/admin/projects", Some(&cookie)).await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    let proj = &arr[0];
    assert_eq!(proj.get("name").and_then(|n| n.as_str()), Some("alpha"));
    assert_eq!(
        proj.get("token").and_then(|n| n.as_str()),
        Some(p.token.as_str())
    );
    let dsn = proj.get("dsn").and_then(|d| d.as_str()).unwrap();
    assert!(
        dsn.contains("alpha"),
        "dsn must contain project name: {dsn}"
    );
    assert!(dsn.contains(&p.token), "dsn must contain token");
    assert!(
        dsn.starts_with("http://test.local:9090/"),
        "dsn must use public_url"
    );
}

#[tokio::test]
async fn admin_create_project_returns_dsn_and_persists() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let cookie = admin_cookie(&store).await;
    let res = admin_send(
        &router,
        "POST",
        "/api/admin/projects",
        Some(&cookie),
        r#"{"name":"new-app"}"#,
    )
    .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let v = body_json(res).await;
    assert_eq!(v.get("name").and_then(|n| n.as_str()), Some("new-app"));
    assert!(v.get("token").is_some());
    assert!(v.get("dsn").is_some());

    // Persisted: a follow-up GET sees it.
    let p = store.project_by_name("new-app").await.unwrap();
    assert!(p.is_some());
}

#[tokio::test]
async fn admin_create_project_rejects_duplicate_name() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let cookie = admin_cookie(&store).await;
    store.create_project("dup").await.unwrap();
    let res = admin_send(
        &router,
        "POST",
        "/api/admin/projects",
        Some(&cookie),
        r#"{"name":"dup"}"#,
    )
    .await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn admin_create_project_rejects_empty_name() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let cookie = admin_cookie(&store).await;
    let res = admin_send(
        &router,
        "POST",
        "/api/admin/projects",
        Some(&cookie),
        r#"{"name":""}"#,
    )
    .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn admin_set_webhook_persists_url() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let cookie = admin_cookie(&store).await;
    store.create_project("p").await.unwrap();
    let res = admin_send(
        &router,
        "PUT",
        "/api/admin/projects/p/webhook",
        Some(&cookie),
        r#"{"url":"https://hooks.slack.com/x"}"#,
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let p = store.project_by_name("p").await.unwrap().unwrap();
    assert_eq!(p.webhook_url.as_deref(), Some("https://hooks.slack.com/x"));
}

#[tokio::test]
async fn admin_set_webhook_with_null_clears() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let cookie = admin_cookie(&store).await;
    store.create_project("p").await.unwrap();
    store
        .set_project_webhook("p", Some("https://x"))
        .await
        .unwrap();
    let res = admin_send(
        &router,
        "PUT",
        "/api/admin/projects/p/webhook",
        Some(&cookie),
        r#"{"url":null}"#,
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let p = store.project_by_name("p").await.unwrap().unwrap();
    assert!(p.webhook_url.is_none());
}

#[tokio::test]
async fn admin_set_webhook_returns_404_for_unknown_project() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let cookie = admin_cookie(&store).await;
    let res = admin_send(
        &router,
        "PUT",
        "/api/admin/projects/ghost/webhook",
        Some(&cookie),
        r#"{"url":"https://x"}"#,
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn admin_rotate_token_returns_new_token_and_invalidates_old() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let cookie = admin_cookie(&store).await;
    let original = store.create_project("p").await.unwrap();
    let res = admin_send(
        &router,
        "POST",
        "/api/admin/projects/p/rotate",
        Some(&cookie),
        "",
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    let new_token = v.get("token").and_then(|t| t.as_str()).unwrap();
    assert_ne!(new_token, original.token);
    assert!(
        store
            .project_by_token(&original.token)
            .await
            .unwrap()
            .is_none(),
        "old token must no longer match"
    );
}

#[tokio::test]
async fn admin_rotate_token_returns_404_for_unknown_project() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let cookie = admin_cookie(&store).await;
    let res = admin_send(
        &router,
        "POST",
        "/api/admin/projects/ghost/rotate",
        Some(&cookie),
        "",
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

// ----- admin: activity stats -----

async fn seed_one_event(store: &Store, project: &str, fingerprint: &str) -> i64 {
    use chrono::Utc;
    use errex_proto::{Event, Level};
    let r = store
        .upsert_issue(
            project,
            &Fingerprint::new(fingerprint.to_string()),
            "T",
            None,
            None,
            Utc::now(),
        )
        .await
        .unwrap();
    let ev = Event {
        event_id: uuid::Uuid::new_v4(),
        timestamp: Utc::now(),
        platform: Some("javascript".into()),
        level: Some(Level::Error),
        environment: None,
        release: None,
        server_name: None,
        message: None,
        exception: None,
        breadcrumbs: None,
        tags: None,
        contexts: None,
        extra: None,
        user: None,
        request: None,
    };
    store.insert_event(r.issue.id, &ev).await.unwrap();
    r.issue.id
}

#[tokio::test]
async fn admin_activity_returns_zeros_for_empty_project() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let cookie = admin_cookie(&store).await;
    store.create_project("p").await.unwrap();
    let res = admin_get(&router, "/api/admin/projects/p/activity", Some(&cookie)).await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    assert_eq!(v.get("events_24h").and_then(|n| n.as_i64()), Some(0));
    assert_eq!(v.get("unique_issues_24h").and_then(|n| n.as_i64()), Some(0));
    assert!(v.get("last_event_at").map(|x| x.is_null()).unwrap_or(false));
    let buckets = v.get("hourly_buckets").and_then(|b| b.as_array()).unwrap();
    assert_eq!(buckets.len(), 24);
}

#[tokio::test]
async fn admin_activity_counts_events_in_window() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let cookie = admin_cookie(&store).await;
    store.create_project("p").await.unwrap();
    seed_one_event(&store, "p", "a").await;
    seed_one_event(&store, "p", "a").await;
    seed_one_event(&store, "p", "b").await;
    let res = admin_get(&router, "/api/admin/projects/p/activity", Some(&cookie)).await;
    let v = body_json(res).await;
    assert_eq!(v.get("events_24h").and_then(|n| n.as_i64()), Some(3));
    assert_eq!(v.get("unique_issues_24h").and_then(|n| n.as_i64()), Some(2));
    assert!(v.get("last_event_at").and_then(|x| x.as_str()).is_some());
}

#[tokio::test]
async fn admin_activity_returns_404_for_unknown_project() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let cookie = admin_cookie(&store).await;
    let res = admin_get(&router, "/api/admin/projects/ghost/activity", Some(&cookie)).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

// ----- admin: destroy preview -----

#[tokio::test]
async fn admin_destroy_preview_returns_counts_without_deleting() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let cookie = admin_cookie(&store).await;
    store.create_project("p").await.unwrap();
    seed_one_event(&store, "p", "a").await;
    seed_one_event(&store, "p", "a").await;
    seed_one_event(&store, "p", "b").await;
    let res = admin_get(
        &router,
        "/api/admin/projects/p/destroy-preview",
        Some(&cookie),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    assert_eq!(v.get("issues_deleted").and_then(|n| n.as_i64()), Some(2));
    assert_eq!(v.get("events_deleted").and_then(|n| n.as_i64()), Some(3));
    // Preview must NOT delete anything.
    assert!(store.project_by_name("p").await.unwrap().is_some());
    assert_eq!(store.list_issues_by_project("p").await.unwrap().len(), 2);
}

#[tokio::test]
async fn admin_destroy_preview_returns_404_for_unknown_project() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let cookie = admin_cookie(&store).await;
    let res = admin_get(
        &router,
        "/api/admin/projects/ghost/destroy-preview",
        Some(&cookie),
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

// ----- admin: delete -----

#[tokio::test]
async fn admin_delete_project_returns_summary_and_removes_rows() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let cookie = admin_cookie(&store).await;
    store.create_project("p").await.unwrap();
    seed_one_event(&store, "p", "a").await;
    let res = admin_send(
        &router,
        "DELETE",
        "/api/admin/projects/p",
        Some(&cookie),
        "",
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    assert_eq!(v.get("issues_deleted").and_then(|n| n.as_i64()), Some(1));
    assert_eq!(v.get("events_deleted").and_then(|n| n.as_i64()), Some(1));
    assert!(store.project_by_name("p").await.unwrap().is_none());
}

#[tokio::test]
async fn admin_delete_project_returns_404_for_unknown_project() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let cookie = admin_cookie(&store).await;
    let res = admin_send(
        &router,
        "DELETE",
        "/api/admin/projects/ghost",
        Some(&cookie),
        "",
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn admin_delete_project_requires_admin_role() {
    // A signed-in viewer should NOT be able to delete projects. Confirms
    // the role gate fires (403, not 401) and that the project survives the
    // rejected request.
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let viewer = signed_in_cookie(&store, store::Role::Viewer).await;
    store.create_project("p").await.unwrap();
    let res = admin_send(
        &router,
        "DELETE",
        "/api/admin/projects/p",
        Some(&viewer),
        "",
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    assert!(store.project_by_name("p").await.unwrap().is_some());
}

// ----- admin: rename -----

#[tokio::test]
async fn admin_rename_project_changes_name_and_cascades_to_issues() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let cookie = admin_cookie(&store).await;
    let original = store.create_project("old").await.unwrap();
    seed_one_event(&store, "old", "a").await;
    let res = admin_send(
        &router,
        "PATCH",
        "/api/admin/projects/old",
        Some(&cookie),
        r#"{"name":"new"}"#,
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    assert_eq!(v.get("name").and_then(|n| n.as_str()), Some("new"));
    // Token must be preserved across rename — a rename is not a security event.
    assert_eq!(
        v.get("token").and_then(|t| t.as_str()),
        Some(original.token.as_str())
    );
    assert!(store.project_by_name("old").await.unwrap().is_none());
    assert!(store.project_by_name("new").await.unwrap().is_some());
    assert_eq!(
        store.list_issues_by_project("new").await.unwrap().len(),
        1,
        "issues must move to the new name"
    );
}

#[tokio::test]
async fn admin_rename_project_to_existing_name_returns_409() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let cookie = admin_cookie(&store).await;
    store.create_project("a").await.unwrap();
    store.create_project("b").await.unwrap();
    let res = admin_send(
        &router,
        "PATCH",
        "/api/admin/projects/a",
        Some(&cookie),
        r#"{"name":"b"}"#,
    )
    .await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
    // Both must still exist with their original names.
    assert!(store.project_by_name("a").await.unwrap().is_some());
    assert!(store.project_by_name("b").await.unwrap().is_some());
}

#[tokio::test]
async fn admin_rename_project_returns_404_for_unknown_source() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let cookie = admin_cookie(&store).await;
    let res = admin_send(
        &router,
        "PATCH",
        "/api/admin/projects/ghost",
        Some(&cookie),
        r#"{"name":"anything"}"#,
    )
    .await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn admin_rename_project_rejects_empty_name() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let cookie = admin_cookie(&store).await;
    store.create_project("p").await.unwrap();
    let res = admin_send(
        &router,
        "PATCH",
        "/api/admin/projects/p",
        Some(&cookie),
        r#"{"name":""}"#,
    )
    .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    assert!(store.project_by_name("p").await.unwrap().is_some());
}

// ----- admin: list exposes new webhook health columns -----

#[tokio::test]
async fn admin_list_projects_includes_webhook_health_fields() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let cookie = admin_cookie(&store).await;
    store.create_project("p").await.unwrap();
    store.record_webhook_attempt("p", 200).await;
    let res = admin_get(&router, "/api/admin/projects", Some(&cookie)).await;
    let v = body_json(res).await;
    let arr = v.as_array().unwrap();
    let p = arr
        .iter()
        .find(|p| p.get("name").and_then(|n| n.as_str()) == Some("p"))
        .unwrap();
    assert_eq!(
        p.get("last_webhook_status").and_then(|n| n.as_i64()),
        Some(200)
    );
    assert!(p.get("last_webhook_at").and_then(|s| s.as_str()).is_some());
}

// ----- /api/auth/setup -----
//
// Setup wizard: only fires while the users table is empty AND the operator
// presents the env-var setup token. We test all four arms (happy + the
// three rejection paths).

const STRONG_PW: &str = "super-secure-passphrase";

async fn auth_send(router: &axum::Router, path: &str, body: &str) -> axum::response::Response {
    router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(path)
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
}

fn extract_set_cookie(resp: &axum::response::Response) -> Option<String> {
    resp.headers()
        .get_all("set-cookie")
        .iter()
        .find_map(|v| v.to_str().ok().map(str::to_string))
}

fn cookie_value_from_set(set_cookie: &str) -> String {
    // Extract `errex_session=<value>` (the part before the first `;`).
    set_cookie
        .split(';')
        .next()
        .map(str::trim)
        .unwrap_or_default()
        .to_string()
}

#[tokio::test]
async fn setup_creates_first_admin_and_signs_them_in() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let body =
        format!(r#"{{"token":"{ADMIN_TOKEN}","username":"daisy","password":"{STRONG_PW}"}}"#);
    let res = auth_send(&router, "/api/auth/setup", &body).await;
    assert_eq!(res.status(), StatusCode::CREATED);
    // Cookie is issued so the operator doesn't have to re-sign-in.
    let set = extract_set_cookie(&res).expect("setup must issue a session cookie");
    assert!(set.starts_with("errex_session="));
    // User is persisted as admin.
    let user = store.get_user("daisy").await.unwrap().unwrap();
    assert_eq!(user.role, store::Role::Admin);
}

#[tokio::test]
async fn setup_rejects_wrong_token() {
    let (router, _store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let body = format!(r#"{{"token":"WRONG","username":"daisy","password":"{STRONG_PW}"}}"#);
    let res = auth_send(&router, "/api/auth/setup", &body).await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn setup_rejects_weak_password() {
    let (router, _store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let body = format!(r#"{{"token":"{ADMIN_TOKEN}","username":"daisy","password":"short"}}"#);
    let res = auth_send(&router, "/api/auth/setup", &body).await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn setup_returns_409_after_first_user_exists() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    store
        .create_user("seed", "stored-hash", store::Role::Admin)
        .await
        .unwrap();
    let body =
        format!(r#"{{"token":"{ADMIN_TOKEN}","username":"daisy","password":"{STRONG_PW}"}}"#);
    let res = auth_send(&router, "/api/auth/setup", &body).await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn setup_returns_503_when_no_setup_token_configured() {
    // Without ERREXD_ADMIN_TOKEN there is no way to authorise the wizard.
    // The endpoint must refuse, not silently allow open setup.
    let (router, _store, _dir) = fixture().await;
    let body = format!(r#"{{"token":"anything","username":"daisy","password":"{STRONG_PW}"}}"#);
    let res = auth_send(&router, "/api/auth/setup", &body).await;
    assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
}

// ----- /api/auth/login -----

async fn create_real_user(store: &Store, username: &str, password: &str, role: store::Role) {
    let hash = crypto::hash_password(password).unwrap();
    store.create_user(username, &hash, role).await.unwrap();
}

#[tokio::test]
async fn login_with_valid_credentials_sets_cookie() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    create_real_user(&store, "daisy", STRONG_PW, store::Role::Admin).await;
    let body = format!(r#"{{"username":"daisy","password":"{STRONG_PW}"}}"#);
    let res = auth_send(&router, "/api/auth/login", &body).await;
    assert_eq!(res.status(), StatusCode::OK);
    let set = extract_set_cookie(&res).expect("login must Set-Cookie");
    assert!(set.contains("errex_session="));
    assert!(set.contains("HttpOnly"));
    assert!(set.contains("SameSite=Strict"));
}

#[tokio::test]
async fn login_with_wrong_password_returns_401_and_records_failure() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    create_real_user(&store, "daisy", STRONG_PW, store::Role::Admin).await;
    let body = r#"{"username":"daisy","password":"wrong-password-attempt"}"#;
    let res = auth_send(&router, "/api/auth/login", body).await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let n = store
        .count_recent_failures_for_username("daisy", Utc::now() - chrono::Duration::minutes(15))
        .await
        .unwrap();
    assert_eq!(n, 1, "failed login must be recorded for lockout accounting");
}

#[tokio::test]
async fn login_with_unknown_user_returns_401_without_revealing_existence() {
    let (router, _store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let body = format!(r#"{{"username":"ghost","password":"{STRONG_PW}"}}"#);
    let res = auth_send(&router, "/api/auth/login", &body).await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn login_with_deactivated_user_returns_401() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    create_real_user(&store, "daisy", STRONG_PW, store::Role::Admin).await;
    store.set_user_deactivated("daisy", true).await.unwrap();
    let body = format!(r#"{{"username":"daisy","password":"{STRONG_PW}"}}"#);
    let res = auth_send(&router, "/api/auth/login", &body).await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn login_returns_429_when_user_lockout_threshold_hit() {
    // Spec: 5 failures in 15 min → 429 with Retry-After.
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    create_real_user(&store, "daisy", STRONG_PW, store::Role::Admin).await;
    for _ in 0..5 {
        store
            .record_attempt(Some("daisy"), "127.0.0.1", false)
            .await;
    }
    let body = format!(r#"{{"username":"daisy","password":"{STRONG_PW}"}}"#);
    let res = auth_send(&router, "/api/auth/login", &body).await;
    assert_eq!(res.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(
        res.headers().contains_key("retry-after"),
        "lockout response must include Retry-After"
    );
}

#[tokio::test]
async fn login_updates_last_login_at_and_ip_on_success() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    create_real_user(&store, "daisy", STRONG_PW, store::Role::Admin).await;
    let body = format!(r#"{{"username":"daisy","password":"{STRONG_PW}"}}"#);
    let res = auth_send(&router, "/api/auth/login", &body).await;
    assert_eq!(res.status(), StatusCode::OK);
    let u = store.get_user("daisy").await.unwrap().unwrap();
    assert!(u.last_login_at.is_some());
}

// ----- /api/auth/logout -----

#[tokio::test]
async fn logout_clears_cookie_and_revokes_session() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let cookie = admin_cookie(&store).await;

    // Sanity: /me works before logout.
    let res = admin_get(&router, "/api/auth/me", Some(&cookie)).await;
    assert_eq!(res.status(), StatusCode::OK);

    let res = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/logout")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    let set = extract_set_cookie(&res).expect("logout must Set-Cookie to clear it");
    assert!(set.contains("Max-Age=0"));

    // Same cookie no longer works.
    let res = admin_get(&router, "/api/auth/me", Some(&cookie)).await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn logout_with_no_cookie_is_idempotent() {
    let (router, _store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let res = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/auth/logout")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
}

// ----- /api/auth/me -----

#[tokio::test]
async fn me_returns_username_and_role_for_valid_session() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let cookie = admin_cookie(&store).await;
    let res = admin_get(&router, "/api/auth/me", Some(&cookie)).await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    assert_eq!(
        v.get("username").and_then(|s| s.as_str()),
        Some("test-admin")
    );
    assert_eq!(v.get("role").and_then(|s| s.as_str()), Some("admin"));
}

#[tokio::test]
async fn me_returns_401_when_no_cookie() {
    let (router, _store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let res = admin_get(&router, "/api/auth/me", None).await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn me_round_trips_with_value_extracted_from_login_cookie() {
    // Drives the full login-then-me flow via the wire: confirms the cookie
    // header round-trips correctly between Set-Cookie writer and Cookie
    // reader, no leading-space or quoting bugs.
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    create_real_user(&store, "daisy", STRONG_PW, store::Role::Admin).await;
    let body = format!(r#"{{"username":"daisy","password":"{STRONG_PW}"}}"#);
    let login = auth_send(&router, "/api/auth/login", &body).await;
    let cookie = cookie_value_from_set(&extract_set_cookie(&login).unwrap());
    let me = admin_get(&router, "/api/auth/me", Some(&cookie)).await;
    assert_eq!(me.status(), StatusCode::OK);
    let v = body_json(me).await;
    assert_eq!(v.get("username").and_then(|s| s.as_str()), Some("daisy"));
}

// ----- /api/admin/users -----

#[tokio::test]
async fn admin_list_users_requires_admin_session() {
    let (router, _store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let res = admin_get(&router, "/api/admin/users", None).await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_list_users_returns_serialised_view_without_password() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let cookie = admin_cookie(&store).await; // creates "test-admin"
    let res = admin_get(&router, "/api/admin/users", Some(&cookie)).await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    let arr = v.as_array().unwrap();
    assert!(arr
        .iter()
        .any(|u| u.get("username").and_then(|s| s.as_str()) == Some("test-admin")));
    let raw = serde_json::to_string(&v).unwrap();
    assert!(
        !raw.contains("password"),
        "user list must never carry password fields"
    );
}

#[tokio::test]
async fn admin_create_user_persists_and_returns_201() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let cookie = admin_cookie(&store).await;
    let res = admin_send(
        &router,
        "POST",
        "/api/admin/users",
        Some(&cookie),
        &format!(r#"{{"username":"new-viewer","password":"{STRONG_PW}","role":"viewer"}}"#),
    )
    .await;
    assert_eq!(res.status(), StatusCode::CREATED);
    let u = store.get_user("new-viewer").await.unwrap().unwrap();
    assert_eq!(u.role, store::Role::Viewer);
}

#[tokio::test]
async fn admin_create_user_rejects_weak_password() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let cookie = admin_cookie(&store).await;
    let res = admin_send(
        &router,
        "POST",
        "/api/admin/users",
        Some(&cookie),
        r#"{"username":"x","password":"short","role":"viewer"}"#,
    )
    .await;
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn admin_create_user_rejects_duplicate_username_with_409() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let cookie = admin_cookie(&store).await;
    let body = format!(r#"{{"username":"daisy","password":"{STRONG_PW}","role":"admin"}}"#);
    admin_send(&router, "POST", "/api/admin/users", Some(&cookie), &body).await;
    let res = admin_send(&router, "POST", "/api/admin/users", Some(&cookie), &body).await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn admin_get_user_returns_404_for_unknown() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let cookie = admin_cookie(&store).await;
    let res = admin_get(&router, "/api/admin/users/ghost", Some(&cookie)).await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn admin_patch_user_changes_role() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let cookie = admin_cookie(&store).await; // test-admin (admin role)
    create_real_user(&store, "daisy", STRONG_PW, store::Role::Viewer).await;
    let res = admin_send(
        &router,
        "PATCH",
        "/api/admin/users/daisy",
        Some(&cookie),
        r#"{"role":"admin"}"#,
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let u = store.get_user("daisy").await.unwrap().unwrap();
    assert_eq!(u.role, store::Role::Admin);
}

#[tokio::test]
async fn admin_patch_user_can_change_password_and_lets_login_use_new_value() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let cookie = admin_cookie(&store).await;
    create_real_user(&store, "daisy", STRONG_PW, store::Role::Admin).await;
    let new_pw = "shiny-new-passphrase";
    let res = admin_send(
        &router,
        "PATCH",
        "/api/admin/users/daisy",
        Some(&cookie),
        &format!(r#"{{"password":"{new_pw}"}}"#),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    // Old password no longer works.
    let old = auth_send(
        &router,
        "/api/auth/login",
        &format!(r#"{{"username":"daisy","password":"{STRONG_PW}"}}"#),
    )
    .await;
    assert_eq!(old.status(), StatusCode::UNAUTHORIZED);
    // New password works.
    let new = auth_send(
        &router,
        "/api/auth/login",
        &format!(r#"{{"username":"daisy","password":"{new_pw}"}}"#),
    )
    .await;
    assert_eq!(new.status(), StatusCode::OK);
}

#[tokio::test]
async fn admin_patch_user_deactivate_revokes_sessions() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let cookie = admin_cookie(&store).await;
    create_real_user(&store, "daisy", STRONG_PW, store::Role::Viewer).await;
    store
        .create_session("sess", "daisy", None, None)
        .await
        .unwrap();
    let res = admin_send(
        &router,
        "PATCH",
        "/api/admin/users/daisy",
        Some(&cookie),
        r#"{"deactivated":true}"#,
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    assert!(store.session_for_id("sess").await.unwrap().is_none());
}

#[tokio::test]
async fn admin_patch_user_refuses_to_demote_last_admin() {
    // Critical safety: the team page must not let an admin click themselves
    // out of the role and lock everyone out.
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let cookie = admin_cookie(&store).await; // test-admin = the only admin
    let res = admin_send(
        &router,
        "PATCH",
        "/api/admin/users/test-admin",
        Some(&cookie),
        r#"{"role":"viewer"}"#,
    )
    .await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
    let u = store.get_user("test-admin").await.unwrap().unwrap();
    assert_eq!(u.role, store::Role::Admin, "role must not have changed");
}

#[tokio::test]
async fn admin_patch_user_refuses_to_deactivate_last_admin() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let cookie = admin_cookie(&store).await;
    let res = admin_send(
        &router,
        "PATCH",
        "/api/admin/users/test-admin",
        Some(&cookie),
        r#"{"deactivated":true}"#,
    )
    .await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn admin_delete_user_removes_user() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let cookie = admin_cookie(&store).await;
    create_real_user(&store, "daisy", STRONG_PW, store::Role::Viewer).await;
    let res = admin_send(
        &router,
        "DELETE",
        "/api/admin/users/daisy",
        Some(&cookie),
        "",
    )
    .await;
    assert_eq!(res.status(), StatusCode::NO_CONTENT);
    assert!(store.get_user("daisy").await.unwrap().is_none());
}

#[tokio::test]
async fn admin_delete_user_refuses_last_admin() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let cookie = admin_cookie(&store).await;
    let res = admin_send(
        &router,
        "DELETE",
        "/api/admin/users/test-admin",
        Some(&cookie),
        "",
    )
    .await;
    assert_eq!(res.status(), StatusCode::CONFLICT);
    assert!(store.get_user("test-admin").await.unwrap().is_some());
}

#[tokio::test]
async fn admin_list_user_sessions_returns_active_sessions() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let cookie = admin_cookie(&store).await;
    create_real_user(&store, "daisy", STRONG_PW, store::Role::Viewer).await;
    store
        .create_session("a", "daisy", Some("1.1.1.1"), None)
        .await
        .unwrap();
    store
        .create_session("b", "daisy", Some("2.2.2.2"), None)
        .await
        .unwrap();
    let res = admin_get(&router, "/api/admin/users/daisy/sessions", Some(&cookie)).await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    assert_eq!(v.as_array().map(Vec::len), Some(2));
}

#[tokio::test]
async fn admin_revoke_user_sessions_returns_count() {
    let (router, store, _dir) = fixture_with_admin(ADMIN_TOKEN).await;
    let cookie = admin_cookie(&store).await;
    create_real_user(&store, "daisy", STRONG_PW, store::Role::Viewer).await;
    store
        .create_session("a", "daisy", None, None)
        .await
        .unwrap();
    store
        .create_session("b", "daisy", None, None)
        .await
        .unwrap();
    store
        .create_session("c", "daisy", None, None)
        .await
        .unwrap();
    let res = admin_send(
        &router,
        "POST",
        "/api/admin/users/daisy/sessions/revoke-all",
        Some(&cookie),
        "",
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    assert_eq!(v.get("sessions_revoked").and_then(|n| n.as_i64()), Some(3));
}

// ----- /ws/:project -----
//
// The fan-out WebSocket lives on the same listener as the HTTP API. These
// tests bind a real ephemeral port (oneshot can't drive an upgrade) and
// drive a tungstenite client against it. They pin the production-critical
// invariant that brought us here: the daemon must answer the upgrade on
// the HTTP port — the SPA builds its WS URL from `location.host`, so any
// regression where /ws falls through to the SPA fallback (HTTP 200 +
// index.html) breaks every browser instantly.

use futures_util::{SinkExt, StreamExt};
use std::net::SocketAddr;
use tokio_tungstenite::tungstenite::Message;

async fn spawn_server(router: axum::Router) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let make = router.into_make_service_with_connect_info::<SocketAddr>();
    tokio::spawn(async move {
        axum::serve(listener, make).await.unwrap();
    });
    addr
}

async fn next_text(
    ws: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
) -> serde_json::Value {
    let msg = ws.next().await.expect("stream ended").expect("ws error");
    match msg {
        Message::Text(t) => serde_json::from_str(&t).expect("valid json"),
        other => panic!("expected text frame, got {:?}", other),
    }
}

#[tokio::test]
async fn ws_handshake_returns_hello_then_snapshot() {
    let (router, store, _dir) = fixture().await;
    store
        .upsert_issue(
            "alpha",
            &Fingerprint::new("a"),
            "Boom",
            None,
            None,
            Utc::now(),
        )
        .await
        .unwrap();

    let addr = spawn_server(router).await;
    let url = format!("ws://{}/ws/alpha", addr);
    let (mut ws, _resp) = tokio_tungstenite::connect_async(&url)
        .await
        .expect("upgrade");

    let hello = next_text(&mut ws).await;
    assert_eq!(hello.get("type").and_then(|s| s.as_str()), Some("hello"));
    assert!(hello.get("server_version").is_some());

    let snap = next_text(&mut ws).await;
    assert_eq!(snap.get("type").and_then(|s| s.as_str()), Some("snapshot"));
    let issues = snap
        .get("issues")
        .and_then(|i| i.as_array())
        .expect("snapshot issues array");
    assert_eq!(issues.len(), 1);
    assert_eq!(
        issues[0].get("project").and_then(|s| s.as_str()),
        Some("alpha")
    );
}

#[tokio::test]
async fn ws_ping_keepalive_does_not_close() {
    // Client-side Ping is documented as a no-op heartbeat. If the daemon
    // ever started replying with Close on unknown messages, the keepalive
    // would tear sockets down — pin it.
    let (router, _store, _dir) = fixture().await;
    let addr = spawn_server(router).await;
    let url = format!("ws://{}/ws/alpha", addr);
    let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();

    let _hello = next_text(&mut ws).await;
    let _snap = next_text(&mut ws).await;

    ws.send(Message::Text(r#"{"type":"ping"}"#.into()))
        .await
        .unwrap();

    // Round-trip a ws-level ping/pong to prove the connection is still
    // alive without racing on absence-of-message.
    ws.send(Message::Ping(vec![])).await.unwrap();
    let pong = ws.next().await.expect("pong").expect("ws ok");
    assert!(matches!(pong, Message::Pong(_)), "got {:?}", pong);
}

// ----- /health -----

#[tokio::test]
async fn health_returns_ok() {
    let (router, _store, _dir) = fixture().await;
    let res = router
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let v = body_json(res).await;
    assert_eq!(v.get("status").and_then(|s| s.as_str()), Some("ok"));
}
