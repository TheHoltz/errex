//! Persistence layer integration tests.
//!
//! Each test boots a fresh in-tempdir SQLite DB. We deliberately do NOT
//! mock the store: the whole point of embedding SQLite is that it's cheap
//! to spin up real ones, and "test against the real type" catches a class
//! of bugs (column types, parameter binding, transaction edge cases) that
//! mocks paper over.
//!
//! These tests pin every public method on `Store` so any future refactor
//! that breaks the contract trips a red bar before reaching prod.

// `#[path]`-imported error/store modules export items that are exercised by
// other test binaries (api.rs uses Role::is_admin, the api handlers use
// DaemonError::Crypto). Inside this binary's compilation unit those look
// dead; the silencing is per-crate, not a real warning.
#![allow(dead_code)]

use std::path::PathBuf;

use chrono::{Duration, Utc};
use errex_proto::{
    Event, ExceptionContainer, ExceptionInfo, Fingerprint, Frame, IssueStatus, Level, Stacktrace,
};
use uuid::Uuid;

#[path = "../src/error.rs"]
mod error;
#[path = "../src/store.rs"]
mod store;

use store::{Role, Store};

// ----- helpers -----

fn unique_tempdir() -> PathBuf {
    // PID + nanos so parallel cargo test workers don't collide.
    let p = std::env::temp_dir().join(format!(
        "errexd-store-{}-{}",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).expect("create tempdir");
    p
}

async fn fresh_store() -> (Store, PathBuf) {
    let dir = unique_tempdir();
    let db = dir.join("errex.db");
    let store = Store::open(&db).await.expect("open");
    store.migrate().await.expect("migrate");
    (store, dir)
}

fn sample_event(ty: &str, value: &str, function: &str, lineno: u32) -> Event {
    Event {
        event_id: Uuid::new_v4(),
        timestamp: Utc::now(),
        platform: Some("javascript".into()),
        level: Some(Level::Error),
        environment: Some("prod".into()),
        release: Some("1.0.0".into()),
        server_name: None,
        message: None,
        exception: Some(ExceptionContainer {
            values: vec![ExceptionInfo {
                ty: Some(ty.into()),
                value: Some(value.into()),
                module: None,
                stacktrace: Some(Stacktrace {
                    frames: vec![Frame {
                        filename: Some("app.js".into()),
                        function: Some(function.into()),
                        module: None,
                        lineno: Some(lineno),
                        colno: None,
                        in_app: Some(true),
                    }],
                }),
            }],
        }),
        breadcrumbs: None,
        tags: None,
        contexts: None,
        extra: None,
        user: None,
        request: None,
    }
}

fn fp(s: &str) -> Fingerprint {
    Fingerprint::new(s.to_string())
}

// ----- open / migrate -----

#[tokio::test]
async fn migrate_creates_issues_table() {
    let (store, _dir) = fresh_store().await;
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM issues")
        .fetch_one(store.pool())
        .await
        .expect("query issues");
    assert_eq!(row.0, 0);
}

#[tokio::test]
async fn migrate_creates_events_table() {
    let (store, _dir) = fresh_store().await;
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM events")
        .fetch_one(store.pool())
        .await
        .expect("query events");
    assert_eq!(row.0, 0);
}

#[tokio::test]
async fn migrate_is_idempotent() {
    let (store, dir) = fresh_store().await;
    // Run a second time — must not fail or duplicate rows.
    store.migrate().await.expect("second migrate");
    drop(store);
    let _ = std::fs::remove_dir_all(&dir);
}

// ----- upsert_issue -----

#[tokio::test]
async fn upsert_issue_creates_new_row() {
    let (store, _dir) = fresh_store().await;
    let now = Utc::now();
    let r = store
        .upsert_issue(
            "p",
            &fp("abc"),
            "TypeError: x",
            Some("f in app.js"),
            Some("error"),
            now,
        )
        .await
        .unwrap();
    assert!(r.created, "first upsert must report created=true");
    assert_eq!(r.issue.event_count, 1);
    assert_eq!(r.issue.title, "TypeError: x");
    assert_eq!(r.issue.project, "p");
    assert_eq!(r.issue.fingerprint.as_str(), "abc");
    assert_eq!(r.issue.first_seen, now);
    assert_eq!(r.issue.last_seen, now);
}

#[tokio::test]
async fn upsert_issue_updates_existing_increments_count_and_bumps_last_seen() {
    let (store, _dir) = fresh_store().await;
    let t0 = Utc::now();
    let t1 = t0 + Duration::seconds(30);

    let first = store
        .upsert_issue("p", &fp("abc"), "T: x", None, None, t0)
        .await
        .unwrap();
    let second = store
        .upsert_issue("p", &fp("abc"), "T: x", None, None, t1)
        .await
        .unwrap();

    assert!(!second.created, "second upsert must report created=false");
    assert_eq!(second.issue.id, first.issue.id, "id must be stable");
    assert_eq!(second.issue.event_count, 2);
    assert_eq!(
        second.issue.first_seen, t0,
        "first_seen frozen at first upsert"
    );
    assert_eq!(second.issue.last_seen, t1, "last_seen tracks newest");
}

#[tokio::test]
async fn upsert_issue_does_not_overwrite_metadata_on_repeat() {
    // The first sighting's title/culprit/level are canonical; subsequent
    // events with refined metadata must not overwrite them. Otherwise UI
    // would flicker between titles when grouped events disagree.
    let (store, _dir) = fresh_store().await;
    let t0 = Utc::now();
    let _ = store
        .upsert_issue(
            "p",
            &fp("abc"),
            "First Title",
            Some("first culprit"),
            Some("error"),
            t0,
        )
        .await
        .unwrap();
    let r = store
        .upsert_issue(
            "p",
            &fp("abc"),
            "Second Title",
            Some("second culprit"),
            Some("warning"),
            t0 + Duration::seconds(1),
        )
        .await
        .unwrap();
    assert_eq!(r.issue.title, "First Title");
    assert_eq!(r.issue.culprit.as_deref(), Some("first culprit"));
    assert_eq!(r.issue.level.as_deref(), Some("error"));
}

#[tokio::test]
async fn upsert_issue_isolates_by_project() {
    let (store, _dir) = fresh_store().await;
    let now = Utc::now();
    let a = store
        .upsert_issue("proj-a", &fp("abc"), "T", None, None, now)
        .await
        .unwrap();
    let b = store
        .upsert_issue("proj-b", &fp("abc"), "T", None, None, now)
        .await
        .unwrap();
    assert!(
        a.created && b.created,
        "same fingerprint in different projects must create distinct issues"
    );
    assert_ne!(a.issue.id, b.issue.id);
}

#[tokio::test]
async fn upsert_issue_isolates_by_fingerprint() {
    let (store, _dir) = fresh_store().await;
    let now = Utc::now();
    let a = store
        .upsert_issue("p", &fp("abc"), "T", None, None, now)
        .await
        .unwrap();
    let b = store
        .upsert_issue("p", &fp("xyz"), "T", None, None, now)
        .await
        .unwrap();
    assert_ne!(a.issue.id, b.issue.id);
}

// ----- insert_event -----

#[tokio::test]
async fn insert_event_persists_payload() {
    let (store, _dir) = fresh_store().await;
    let now = Utc::now();
    let upsert = store
        .upsert_issue("p", &fp("abc"), "T", None, None, now)
        .await
        .unwrap();
    let ev = sample_event("TypeError", "boom", "f", 12);
    store.insert_event(upsert.issue.id, &ev).await.unwrap();

    let stored = store
        .latest_event(upsert.issue.id)
        .await
        .unwrap()
        .expect("event must be retrievable");
    assert_eq!(stored.event_id, ev.event_id.to_string());
    // The payload round-trips verbatim — exception type recoverable.
    let ty = stored
        .payload
        .get("exception")
        .and_then(|e| e.get("values"))
        .and_then(|v| v.get(0))
        .and_then(|v| v.get("type"))
        .and_then(|v| v.as_str());
    assert_eq!(ty, Some("TypeError"));
}

#[tokio::test]
async fn insert_event_is_idempotent_on_duplicate_event_id() {
    // Sentry SDKs retry on transient failures; an event_id that arrives
    // twice must not double-count.
    let (store, _dir) = fresh_store().await;
    let now = Utc::now();
    let upsert = store
        .upsert_issue("p", &fp("abc"), "T", None, None, now)
        .await
        .unwrap();
    let ev = sample_event("E", "v", "f", 1);
    store.insert_event(upsert.issue.id, &ev).await.unwrap();
    store.insert_event(upsert.issue.id, &ev).await.unwrap();
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM events WHERE issue_id = ?")
        .bind(upsert.issue.id)
        .fetch_one(store.pool())
        .await
        .unwrap();
    assert_eq!(row.0, 1, "duplicate event_id must collapse to a single row");
}

// ----- latest_event -----

#[tokio::test]
async fn latest_event_returns_none_when_no_events() {
    let (store, _dir) = fresh_store().await;
    let upsert = store
        .upsert_issue("p", &fp("abc"), "T", None, None, Utc::now())
        .await
        .unwrap();
    let res = store.latest_event(upsert.issue.id).await.unwrap();
    assert!(res.is_none());
}

#[tokio::test]
async fn latest_event_returns_none_for_unknown_issue_id() {
    let (store, _dir) = fresh_store().await;
    let res = store.latest_event(999_999).await.unwrap();
    assert!(res.is_none());
}

#[tokio::test]
async fn latest_event_returns_most_recent() {
    let (store, _dir) = fresh_store().await;
    let upsert = store
        .upsert_issue("p", &fp("abc"), "T", None, None, Utc::now())
        .await
        .unwrap();

    let e1 = sample_event("E", "first", "f1", 1);
    store.insert_event(upsert.issue.id, &e1).await.unwrap();
    // Brief sleep so received_at strictly orders. SQLite's CURRENT_TIMESTAMP
    // resolution is seconds; tied timestamps fall back to `id DESC` per the
    // query, which still favors "most recent insert" — but exercising the
    // primary order keeps the contract honest.
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    let e2 = sample_event("E", "second", "f2", 2);
    store.insert_event(upsert.issue.id, &e2).await.unwrap();

    let stored = store.latest_event(upsert.issue.id).await.unwrap().unwrap();
    let value = stored
        .payload
        .get("exception")
        .and_then(|e| e.get("values"))
        .and_then(|v| v.get(0))
        .and_then(|v| v.get("value"))
        .and_then(|v| v.as_str());
    assert_eq!(value, Some("second"));
}

// ----- load_issues / list_issues_by_project / project_summaries -----

#[tokio::test]
async fn load_issues_orders_newest_first() {
    let (store, _dir) = fresh_store().await;
    let t0 = Utc::now();
    let _a = store
        .upsert_issue("p", &fp("a"), "Old", None, None, t0)
        .await
        .unwrap();
    let _b = store
        .upsert_issue(
            "p",
            &fp("b"),
            "Newer",
            None,
            None,
            t0 + Duration::seconds(10),
        )
        .await
        .unwrap();
    let _c = store
        .upsert_issue(
            "p",
            &fp("c"),
            "Newest",
            None,
            None,
            t0 + Duration::seconds(20),
        )
        .await
        .unwrap();
    let list = store.load_issues().await.unwrap();
    let titles: Vec<_> = list.iter().map(|i| i.title.as_str()).collect();
    assert_eq!(titles, vec!["Newest", "Newer", "Old"]);
}

#[tokio::test]
async fn list_issues_by_project_filters() {
    let (store, _dir) = fresh_store().await;
    let now = Utc::now();
    let _ = store
        .upsert_issue("p1", &fp("a"), "in p1", None, None, now)
        .await
        .unwrap();
    let _ = store
        .upsert_issue("p2", &fp("a"), "in p2", None, None, now)
        .await
        .unwrap();
    let _ = store
        .upsert_issue("p1", &fp("b"), "also p1", None, None, now)
        .await
        .unwrap();
    let only_p1 = store.list_issues_by_project("p1").await.unwrap();
    assert_eq!(only_p1.len(), 2);
    assert!(only_p1.iter().all(|i| i.project == "p1"));
}

#[tokio::test]
async fn list_issues_by_project_returns_empty_for_unknown() {
    let (store, _dir) = fresh_store().await;
    let res = store
        .list_issues_by_project("does-not-exist")
        .await
        .unwrap();
    assert!(res.is_empty());
}

#[tokio::test]
async fn project_summaries_groups_and_counts() {
    let (store, _dir) = fresh_store().await;
    let now = Utc::now();
    let _ = store
        .upsert_issue("alpha", &fp("a"), "T", None, None, now)
        .await
        .unwrap();
    let _ = store
        .upsert_issue("alpha", &fp("b"), "T", None, None, now)
        .await
        .unwrap();
    let _ = store
        .upsert_issue("beta", &fp("c"), "T", None, None, now)
        .await
        .unwrap();
    // Repeat ingest of an existing fingerprint must NOT inflate the count.
    let _ = store
        .upsert_issue("alpha", &fp("a"), "T", None, None, now)
        .await
        .unwrap();

    let summaries = store.project_summaries().await.unwrap();
    let map: std::collections::HashMap<_, _> = summaries
        .iter()
        .map(|s| (s.project.clone(), s.issue_count))
        .collect();
    assert_eq!(map.get("alpha").copied(), Some(2));
    assert_eq!(map.get("beta").copied(), Some(1));
}

#[tokio::test]
async fn project_summaries_is_alphabetical() {
    let (store, _dir) = fresh_store().await;
    let now = Utc::now();
    let _ = store
        .upsert_issue("zeta", &fp("a"), "T", None, None, now)
        .await
        .unwrap();
    let _ = store
        .upsert_issue("alpha", &fp("a"), "T", None, None, now)
        .await
        .unwrap();
    let _ = store
        .upsert_issue("mike", &fp("a"), "T", None, None, now)
        .await
        .unwrap();
    let summaries = store.project_summaries().await.unwrap();
    let projects: Vec<_> = summaries.iter().map(|s| s.project.as_str()).collect();
    assert_eq!(projects, vec!["alpha", "mike", "zeta"]);
}

// ----- status -----

#[tokio::test]
async fn newly_created_issue_has_unresolved_status() {
    let (store, _dir) = fresh_store().await;
    let r = store
        .upsert_issue("p", &fp("a"), "T", None, None, Utc::now())
        .await
        .unwrap();
    assert_eq!(r.issue.status, IssueStatus::Unresolved);
    assert!(!r.regressed, "create must not flag regression");
}

#[tokio::test]
async fn set_status_persists_and_round_trips() {
    let (store, _dir) = fresh_store().await;
    let r = store
        .upsert_issue("p", &fp("a"), "T", None, None, Utc::now())
        .await
        .unwrap();
    store
        .set_status(r.issue.id, IssueStatus::Resolved)
        .await
        .unwrap();
    let issues = store.load_issues().await.unwrap();
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].status, IssueStatus::Resolved);
}

#[tokio::test]
async fn set_status_returns_updated_issue() {
    let (store, _dir) = fresh_store().await;
    let r = store
        .upsert_issue("p", &fp("a"), "T", None, None, Utc::now())
        .await
        .unwrap();
    let updated = store
        .set_status(r.issue.id, IssueStatus::Muted)
        .await
        .unwrap();
    assert_eq!(updated.id, r.issue.id);
    assert_eq!(updated.status, IssueStatus::Muted);
}

#[tokio::test]
async fn set_status_on_unknown_issue_is_an_error() {
    let (store, _dir) = fresh_store().await;
    let res = store.set_status(999_999, IssueStatus::Resolved).await;
    assert!(res.is_err(), "missing issue must surface as Err");
}

#[tokio::test]
async fn ingesting_event_for_resolved_issue_regresses_to_unresolved() {
    let (store, _dir) = fresh_store().await;
    let t0 = Utc::now();
    let r1 = store
        .upsert_issue("p", &fp("a"), "T", None, None, t0)
        .await
        .unwrap();
    store
        .set_status(r1.issue.id, IssueStatus::Resolved)
        .await
        .unwrap();
    let r2 = store
        .upsert_issue("p", &fp("a"), "T", None, None, t0 + Duration::seconds(60))
        .await
        .unwrap();
    assert_eq!(r2.issue.status, IssueStatus::Unresolved);
    assert!(
        r2.regressed,
        "upsert into resolved issue must flag regression"
    );
}

#[tokio::test]
async fn upsert_keeps_muted_status() {
    let (store, _dir) = fresh_store().await;
    let r1 = store
        .upsert_issue("p", &fp("a"), "T", None, None, Utc::now())
        .await
        .unwrap();
    store
        .set_status(r1.issue.id, IssueStatus::Muted)
        .await
        .unwrap();
    let r2 = store
        .upsert_issue("p", &fp("a"), "T", None, None, Utc::now())
        .await
        .unwrap();
    assert_eq!(r2.issue.status, IssueStatus::Muted);
    assert!(!r2.regressed);
}

#[tokio::test]
async fn upsert_keeps_ignored_status() {
    let (store, _dir) = fresh_store().await;
    let r1 = store
        .upsert_issue("p", &fp("a"), "T", None, None, Utc::now())
        .await
        .unwrap();
    store
        .set_status(r1.issue.id, IssueStatus::Ignored)
        .await
        .unwrap();
    let r2 = store
        .upsert_issue("p", &fp("a"), "T", None, None, Utc::now())
        .await
        .unwrap();
    assert_eq!(r2.issue.status, IssueStatus::Ignored);
}

// ----- projects (admin / DSN tokens) -----

#[tokio::test]
async fn create_project_returns_record_with_nonempty_token() {
    let (store, _dir) = fresh_store().await;
    let p = store.create_project("my-app").await.unwrap();
    assert_eq!(p.name, "my-app");
    assert!(
        p.token.len() >= 16,
        "token must have meaningful entropy: got {}",
        p.token
    );
}

#[tokio::test]
async fn create_project_rejects_duplicate_name() {
    let (store, _dir) = fresh_store().await;
    store.create_project("dup").await.unwrap();
    let res = store.create_project("dup").await;
    assert!(res.is_err(), "second create with same name must fail");
}

#[tokio::test]
async fn project_by_name_returns_none_for_unknown() {
    let (store, _dir) = fresh_store().await;
    let p = store.project_by_name("ghost").await.unwrap();
    assert!(p.is_none());
}

#[tokio::test]
async fn project_by_token_returns_matching_project() {
    let (store, _dir) = fresh_store().await;
    let created = store.create_project("alpha").await.unwrap();
    let by_token = store.project_by_token(&created.token).await.unwrap();
    assert!(by_token.is_some());
    assert_eq!(by_token.unwrap().name, "alpha");
}

#[tokio::test]
async fn project_by_token_returns_none_for_unknown_token() {
    let (store, _dir) = fresh_store().await;
    let _ = store.create_project("p").await.unwrap();
    let res = store.project_by_token("not-a-real-token").await.unwrap();
    assert!(res.is_none());
}

#[tokio::test]
async fn list_admin_projects_orders_alphabetically() {
    let (store, _dir) = fresh_store().await;
    store.create_project("zeta").await.unwrap();
    store.create_project("alpha").await.unwrap();
    store.create_project("mike").await.unwrap();
    let list = store.list_admin_projects().await.unwrap();
    let names: Vec<_> = list.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, vec!["alpha", "mike", "zeta"]);
}

#[tokio::test]
async fn rotate_token_changes_token_and_invalidates_old() {
    let (store, _dir) = fresh_store().await;
    let created = store.create_project("p").await.unwrap();
    let rotated = store.rotate_token("p").await.unwrap();
    assert_ne!(created.token, rotated.token);
    assert!(store
        .project_by_token(&created.token)
        .await
        .unwrap()
        .is_none());
    assert!(store
        .project_by_token(&rotated.token)
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn rotate_token_on_unknown_project_errors() {
    let (store, _dir) = fresh_store().await;
    let res = store.rotate_token("ghost").await;
    assert!(res.is_err());
}

// ----- webhook URL -----

#[tokio::test]
async fn set_project_webhook_persists() {
    let (store, _dir) = fresh_store().await;
    store.create_project("p").await.unwrap();
    store
        .set_project_webhook("p", Some("https://hooks.slack.com/services/XXX"))
        .await
        .unwrap();
    let p = store.project_by_name("p").await.unwrap().unwrap();
    assert_eq!(
        p.webhook_url.as_deref(),
        Some("https://hooks.slack.com/services/XXX")
    );
}

#[tokio::test]
async fn set_project_webhook_to_none_clears() {
    let (store, _dir) = fresh_store().await;
    store.create_project("p").await.unwrap();
    store
        .set_project_webhook("p", Some("https://x"))
        .await
        .unwrap();
    store.set_project_webhook("p", None).await.unwrap();
    let p = store.project_by_name("p").await.unwrap().unwrap();
    assert!(p.webhook_url.is_none());
}

#[tokio::test]
async fn set_project_webhook_on_unknown_project_errors() {
    let (store, _dir) = fresh_store().await;
    let res = store.set_project_webhook("ghost", Some("https://x")).await;
    assert!(res.is_err());
}

// ----- retention -----

#[tokio::test]
async fn purge_events_older_than_deletes_only_old_rows() {
    let (store, _dir) = fresh_store().await;
    let r = store
        .upsert_issue("p", &fp("a"), "T", None, None, Utc::now())
        .await
        .unwrap();
    let issue_id = r.issue.id;

    // Insert 3 events at t=now and 2 at t=10 days ago (via direct UPDATE
    // to backdate; the API doesn't take received_at).
    for i in 0..3 {
        let mut ev = sample_event("E", &format!("recent-{i}"), "f", i);
        ev.event_id = Uuid::new_v4();
        store.insert_event(issue_id, &ev).await.unwrap();
    }
    for i in 0..2 {
        let mut ev = sample_event("E", &format!("old-{i}"), "f", 100 + i);
        ev.event_id = Uuid::new_v4();
        store.insert_event(issue_id, &ev).await.unwrap();
        sqlx::query("UPDATE events SET received_at = ? WHERE event_id = ?")
            .bind((Utc::now() - Duration::days(10)).to_rfc3339())
            .bind(ev.event_id.to_string())
            .execute(store.pool())
            .await
            .unwrap();
    }

    let cutoff = Utc::now() - Duration::days(7);
    let deleted = store.purge_events_older_than(cutoff).await.unwrap();
    assert_eq!(deleted, 2);

    let remaining: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM events")
        .fetch_one(store.pool())
        .await
        .unwrap();
    assert_eq!(remaining.0, 3);
}

#[tokio::test]
async fn purge_events_older_than_keeps_issue_row() {
    // Issue rows live longer than their events — a high-count issue from
    // months ago is still useful "context" even after retention prunes its
    // payloads.
    let (store, _dir) = fresh_store().await;
    let r = store
        .upsert_issue("p", &fp("a"), "T", None, None, Utc::now())
        .await
        .unwrap();
    let mut ev = sample_event("E", "old", "f", 1);
    ev.event_id = Uuid::new_v4();
    store.insert_event(r.issue.id, &ev).await.unwrap();
    sqlx::query("UPDATE events SET received_at = ? WHERE event_id = ?")
        .bind((Utc::now() - Duration::days(60)).to_rfc3339())
        .bind(ev.event_id.to_string())
        .execute(store.pool())
        .await
        .unwrap();

    store
        .purge_events_older_than(Utc::now() - Duration::days(7))
        .await
        .unwrap();

    let issues = store.load_issues().await.unwrap();
    assert_eq!(issues.len(), 1, "issue must survive event purge");
    assert_eq!(issues[0].id, r.issue.id);
}

#[tokio::test]
async fn purge_events_older_than_with_no_old_rows_returns_zero() {
    let (store, _dir) = fresh_store().await;
    let deleted = store
        .purge_events_older_than(Utc::now() - Duration::days(7))
        .await
        .unwrap();
    assert_eq!(deleted, 0);
}

// ----- persistence -----

#[tokio::test]
async fn data_survives_close_and_reopen() {
    let dir = unique_tempdir();
    let db = dir.join("errex.db");

    {
        let store = Store::open(&db).await.unwrap();
        store.migrate().await.unwrap();
        let upsert = store
            .upsert_issue("p", &fp("abc"), "T: persisted", None, None, Utc::now())
            .await
            .unwrap();
        let ev = sample_event("T", "persisted", "f", 1);
        store.insert_event(upsert.issue.id, &ev).await.unwrap();
        // Drop the store (and pool) to flush WAL on next reopen.
    }

    let store = Store::open(&db).await.unwrap();
    store.migrate().await.unwrap();
    let issues = store.load_issues().await.unwrap();
    assert_eq!(issues.len(), 1);
    assert_eq!(issues[0].title, "T: persisted");
    let ev = store.latest_event(issues[0].id).await.unwrap();
    assert!(ev.is_some(), "event payload must survive restart");
    let _ = std::fs::remove_dir_all(&dir);
}

// ----- delete_project (cascade) -----

#[tokio::test]
async fn delete_project_returns_summary_of_deleted_rows() {
    let (store, _dir) = fresh_store().await;
    store.create_project("doomed").await.unwrap();
    let r1 = store
        .upsert_issue("doomed", &fp("a"), "T1", None, None, Utc::now())
        .await
        .unwrap();
    let r2 = store
        .upsert_issue("doomed", &fp("b"), "T2", None, None, Utc::now())
        .await
        .unwrap();
    for i in 0..3 {
        let mut ev = sample_event("E", &format!("e{i}"), "f", i);
        ev.event_id = Uuid::new_v4();
        store.insert_event(r1.issue.id, &ev).await.unwrap();
    }
    let mut ev = sample_event("E", "lone", "f", 99);
    ev.event_id = Uuid::new_v4();
    store.insert_event(r2.issue.id, &ev).await.unwrap();

    let summary = store.delete_project("doomed").await.unwrap();
    assert_eq!(summary.issues_deleted, 2);
    assert_eq!(summary.events_deleted, 4);
}

#[tokio::test]
async fn delete_project_removes_project_row() {
    let (store, _dir) = fresh_store().await;
    store.create_project("doomed").await.unwrap();
    store.delete_project("doomed").await.unwrap();
    assert!(store.project_by_name("doomed").await.unwrap().is_none());
}

#[tokio::test]
async fn delete_project_cascades_issues_and_events() {
    let (store, _dir) = fresh_store().await;
    store.create_project("doomed").await.unwrap();
    let r = store
        .upsert_issue("doomed", &fp("a"), "T", None, None, Utc::now())
        .await
        .unwrap();
    let mut ev = sample_event("E", "v", "f", 1);
    ev.event_id = Uuid::new_v4();
    store.insert_event(r.issue.id, &ev).await.unwrap();

    store.delete_project("doomed").await.unwrap();

    let issues = store.list_issues_by_project("doomed").await.unwrap();
    assert!(issues.is_empty(), "issues must be gone");
    let leftover: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM events")
        .fetch_one(store.pool())
        .await
        .unwrap();
    assert_eq!(leftover.0, 0, "events must cascade with issues");
}

#[tokio::test]
async fn delete_project_does_not_touch_other_projects() {
    let (store, _dir) = fresh_store().await;
    store.create_project("doomed").await.unwrap();
    store.create_project("survivor").await.unwrap();
    store
        .upsert_issue("doomed", &fp("a"), "T", None, None, Utc::now())
        .await
        .unwrap();
    let r = store
        .upsert_issue("survivor", &fp("b"), "T", None, None, Utc::now())
        .await
        .unwrap();
    let mut ev = sample_event("E", "v", "f", 1);
    ev.event_id = Uuid::new_v4();
    store.insert_event(r.issue.id, &ev).await.unwrap();

    store.delete_project("doomed").await.unwrap();

    assert!(store.project_by_name("survivor").await.unwrap().is_some());
    let surviving = store.list_issues_by_project("survivor").await.unwrap();
    assert_eq!(surviving.len(), 1);
}

#[tokio::test]
async fn delete_project_unknown_returns_not_found() {
    let (store, _dir) = fresh_store().await;
    let res = store.delete_project("ghost").await;
    assert!(res.is_err());
}

#[tokio::test]
async fn delete_preview_returns_counts_without_deleting() {
    let (store, _dir) = fresh_store().await;
    store.create_project("p").await.unwrap();
    let r = store
        .upsert_issue("p", &fp("a"), "T", None, None, Utc::now())
        .await
        .unwrap();
    let mut ev = sample_event("E", "v", "f", 1);
    ev.event_id = Uuid::new_v4();
    store.insert_event(r.issue.id, &ev).await.unwrap();

    let preview = store.delete_preview("p").await.unwrap();
    assert_eq!(preview.issues_deleted, 1);
    assert_eq!(preview.events_deleted, 1);

    // Still there.
    assert!(store.project_by_name("p").await.unwrap().is_some());
    assert_eq!(store.list_issues_by_project("p").await.unwrap().len(), 1);
}

#[tokio::test]
async fn delete_preview_unknown_returns_not_found() {
    let (store, _dir) = fresh_store().await;
    let res = store.delete_preview("ghost").await;
    assert!(res.is_err());
}

// ----- rename_project -----

#[tokio::test]
async fn rename_project_changes_name_and_returns_record() {
    let (store, _dir) = fresh_store().await;
    let original = store.create_project("old-name").await.unwrap();
    let renamed = store.rename_project("old-name", "new-name").await.unwrap();
    assert_eq!(renamed.name, "new-name");
    assert_eq!(renamed.token, original.token, "token must be preserved");
    assert!(store.project_by_name("old-name").await.unwrap().is_none());
    assert!(store.project_by_name("new-name").await.unwrap().is_some());
}

#[tokio::test]
async fn rename_project_cascades_to_issues() {
    let (store, _dir) = fresh_store().await;
    store.create_project("old-name").await.unwrap();
    store
        .upsert_issue("old-name", &fp("a"), "T", None, None, Utc::now())
        .await
        .unwrap();
    store
        .upsert_issue("old-name", &fp("b"), "T2", None, None, Utc::now())
        .await
        .unwrap();

    store.rename_project("old-name", "new-name").await.unwrap();

    assert!(store
        .list_issues_by_project("old-name")
        .await
        .unwrap()
        .is_empty());
    assert_eq!(
        store
            .list_issues_by_project("new-name")
            .await
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn rename_project_unknown_source_errors() {
    let (store, _dir) = fresh_store().await;
    let res = store.rename_project("ghost", "anything").await;
    assert!(res.is_err());
}

#[tokio::test]
async fn rename_project_to_existing_name_errors() {
    let (store, _dir) = fresh_store().await;
    store.create_project("a").await.unwrap();
    store.create_project("b").await.unwrap();
    let res = store.rename_project("a", "b").await;
    assert!(res.is_err(), "rename onto existing name must fail");
    // Both must still exist with original names.
    assert!(store.project_by_name("a").await.unwrap().is_some());
    assert!(store.project_by_name("b").await.unwrap().is_some());
}

#[tokio::test]
async fn rename_project_to_same_name_is_noop_success() {
    let (store, _dir) = fresh_store().await;
    let original = store.create_project("p").await.unwrap();
    let renamed = store.rename_project("p", "p").await.unwrap();
    assert_eq!(renamed.name, "p");
    assert_eq!(renamed.token, original.token);
}

// ----- record_webhook_attempt -----

#[tokio::test]
async fn record_webhook_attempt_persists_status_and_timestamp() {
    let (store, _dir) = fresh_store().await;
    store.create_project("p").await.unwrap();
    store.record_webhook_attempt("p", 200).await;
    let p = store.project_by_name("p").await.unwrap().unwrap();
    assert_eq!(p.last_webhook_status, Some(200));
    assert!(p.last_webhook_at.is_some(), "timestamp must be recorded");
}

#[tokio::test]
async fn record_webhook_attempt_overwrites_previous() {
    let (store, _dir) = fresh_store().await;
    store.create_project("p").await.unwrap();
    store.record_webhook_attempt("p", 200).await;
    store.record_webhook_attempt("p", 502).await;
    let p = store.project_by_name("p").await.unwrap().unwrap();
    assert_eq!(p.last_webhook_status, Some(502));
}

#[tokio::test]
async fn record_webhook_attempt_zero_means_network_error() {
    // Convention: 0 = transport-level failure (no HTTP status returned).
    let (store, _dir) = fresh_store().await;
    store.create_project("p").await.unwrap();
    store.record_webhook_attempt("p", 0).await;
    let p = store.project_by_name("p").await.unwrap().unwrap();
    assert_eq!(p.last_webhook_status, Some(0));
}

#[tokio::test]
async fn record_webhook_attempt_unknown_project_is_silent_noop() {
    // Best-effort: unknown project is logged but does not panic. (We don't
    // want a webhook hot-path to crash on a race with delete_project.)
    let (store, _dir) = fresh_store().await;
    store.record_webhook_attempt("ghost", 200).await;
    // No assert needed — must just not panic.
}

// ----- activity_stats -----

#[tokio::test]
async fn activity_stats_returns_zeros_for_project_with_no_events() {
    let (store, _dir) = fresh_store().await;
    store.create_project("p").await.unwrap();
    let now = Utc::now();
    let stats = store.activity_stats("p", now).await.unwrap();
    assert_eq!(stats.events_24h, 0);
    assert_eq!(stats.unique_issues_24h, 0);
    assert!(stats.last_event_at.is_none());
    assert_eq!(stats.hourly_buckets.len(), 24);
    assert!(stats.hourly_buckets.iter().all(|&n| n == 0));
}

#[tokio::test]
async fn activity_stats_counts_events_in_last_24h() {
    let (store, _dir) = fresh_store().await;
    store.create_project("p").await.unwrap();
    let r = store
        .upsert_issue("p", &fp("a"), "T", None, None, Utc::now())
        .await
        .unwrap();
    for i in 0..5 {
        let mut ev = sample_event("E", &format!("e{i}"), "f", i);
        ev.event_id = Uuid::new_v4();
        store.insert_event(r.issue.id, &ev).await.unwrap();
    }
    let stats = store.activity_stats("p", Utc::now()).await.unwrap();
    assert_eq!(stats.events_24h, 5);
}

#[tokio::test]
async fn activity_stats_excludes_events_older_than_24h() {
    let (store, _dir) = fresh_store().await;
    store.create_project("p").await.unwrap();
    let r = store
        .upsert_issue("p", &fp("a"), "T", None, None, Utc::now())
        .await
        .unwrap();
    // Two recent, two ancient.
    for i in 0..2 {
        let mut ev = sample_event("E", &format!("recent-{i}"), "f", i);
        ev.event_id = Uuid::new_v4();
        store.insert_event(r.issue.id, &ev).await.unwrap();
    }
    for i in 0..2 {
        let mut ev = sample_event("E", &format!("old-{i}"), "f", 100 + i);
        ev.event_id = Uuid::new_v4();
        store.insert_event(r.issue.id, &ev).await.unwrap();
        sqlx::query("UPDATE events SET received_at = ? WHERE event_id = ?")
            .bind((Utc::now() - Duration::days(2)).to_rfc3339())
            .bind(ev.event_id.to_string())
            .execute(store.pool())
            .await
            .unwrap();
    }
    let stats = store.activity_stats("p", Utc::now()).await.unwrap();
    assert_eq!(stats.events_24h, 2);
}

#[tokio::test]
async fn activity_stats_counts_unique_issues_in_last_24h() {
    let (store, _dir) = fresh_store().await;
    store.create_project("p").await.unwrap();
    let r1 = store
        .upsert_issue("p", &fp("a"), "T1", None, None, Utc::now())
        .await
        .unwrap();
    let r2 = store
        .upsert_issue("p", &fp("b"), "T2", None, None, Utc::now())
        .await
        .unwrap();
    // 3 events on issue 1, 1 event on issue 2 → 2 unique issues.
    for i in 0..3 {
        let mut ev = sample_event("E", &format!("a{i}"), "f", i);
        ev.event_id = Uuid::new_v4();
        store.insert_event(r1.issue.id, &ev).await.unwrap();
    }
    let mut ev = sample_event("E", "b0", "f", 99);
    ev.event_id = Uuid::new_v4();
    store.insert_event(r2.issue.id, &ev).await.unwrap();

    let stats = store.activity_stats("p", Utc::now()).await.unwrap();
    assert_eq!(stats.unique_issues_24h, 2);
}

#[tokio::test]
async fn activity_stats_returns_last_event_timestamp() {
    let (store, _dir) = fresh_store().await;
    store.create_project("p").await.unwrap();
    let r = store
        .upsert_issue("p", &fp("a"), "T", None, None, Utc::now())
        .await
        .unwrap();
    let mut ev = sample_event("E", "v", "f", 1);
    ev.event_id = Uuid::new_v4();
    store.insert_event(r.issue.id, &ev).await.unwrap();

    let stats = store.activity_stats("p", Utc::now()).await.unwrap();
    assert!(
        stats.last_event_at.is_some(),
        "must surface most recent event timestamp"
    );
}

#[tokio::test]
async fn activity_stats_buckets_events_by_hour() {
    let (store, _dir) = fresh_store().await;
    store.create_project("p").await.unwrap();
    let r = store
        .upsert_issue("p", &fp("a"), "T", None, None, Utc::now())
        .await
        .unwrap();
    let now = Utc::now();
    // Place 1 event each at "1 hour ago" and "5 hours ago" — they must land
    // in distinct, non-zero buckets.
    let inserts = [(1i64, "h1"), (5, "h5"), (5, "h5b")];
    for (hours_ago, tag) in inserts {
        let mut ev = sample_event("E", tag, "f", hours_ago as u32);
        ev.event_id = Uuid::new_v4();
        store.insert_event(r.issue.id, &ev).await.unwrap();
        sqlx::query("UPDATE events SET received_at = ? WHERE event_id = ?")
            .bind((now - Duration::hours(hours_ago)).to_rfc3339())
            .bind(ev.event_id.to_string())
            .execute(store.pool())
            .await
            .unwrap();
    }

    let stats = store.activity_stats("p", now).await.unwrap();
    assert_eq!(stats.hourly_buckets.len(), 24);
    let total: i64 = stats.hourly_buckets.iter().sum();
    assert_eq!(total, 3, "all three events must be present in some bucket");
    let nonzero_buckets = stats.hourly_buckets.iter().filter(|&&n| n > 0).count();
    assert_eq!(
        nonzero_buckets, 2,
        "events 1h-ago and 5h-ago should be in two distinct buckets"
    );
}

#[tokio::test]
async fn activity_stats_excludes_other_projects() {
    let (store, _dir) = fresh_store().await;
    store.create_project("p").await.unwrap();
    store.create_project("other").await.unwrap();
    let r_p = store
        .upsert_issue("p", &fp("a"), "T", None, None, Utc::now())
        .await
        .unwrap();
    let r_o = store
        .upsert_issue("other", &fp("a"), "T", None, None, Utc::now())
        .await
        .unwrap();
    for i in 0..3 {
        let mut ev = sample_event("E", &format!("p{i}"), "f", i);
        ev.event_id = Uuid::new_v4();
        store.insert_event(r_p.issue.id, &ev).await.unwrap();
    }
    for i in 0..7 {
        let mut ev = sample_event("E", &format!("o{i}"), "f", i);
        ev.event_id = Uuid::new_v4();
        store.insert_event(r_o.issue.id, &ev).await.unwrap();
    }
    let stats = store.activity_stats("p", Utc::now()).await.unwrap();
    assert_eq!(stats.events_24h, 3);
    assert_eq!(stats.unique_issues_24h, 1);
}

// ----- users -----

#[tokio::test]
async fn user_count_starts_at_zero() {
    let (store, _dir) = fresh_store().await;
    assert_eq!(store.user_count().await.unwrap(), 0);
}

#[tokio::test]
async fn create_user_persists_and_returns_record() {
    let (store, _dir) = fresh_store().await;
    let u = store
        .create_user("daisy", "argon2-stored-hash", Role::Admin)
        .await
        .unwrap();
    assert_eq!(u.username, "daisy");
    assert_eq!(u.role, Role::Admin);
    assert!(u.last_login_at.is_none());
    assert!(u.deactivated_at.is_none());
    assert_eq!(store.user_count().await.unwrap(), 1);
}

#[tokio::test]
async fn create_user_rejects_duplicate_username() {
    let (store, _dir) = fresh_store().await;
    store.create_user("daisy", "h", Role::Admin).await.unwrap();
    let res = store.create_user("daisy", "h2", Role::Viewer).await;
    assert!(res.is_err());
}

#[tokio::test]
async fn list_users_orders_alphabetically() {
    let (store, _dir) = fresh_store().await;
    store.create_user("zoe", "h", Role::Viewer).await.unwrap();
    store.create_user("alex", "h", Role::Admin).await.unwrap();
    store.create_user("max", "h", Role::Viewer).await.unwrap();
    let names: Vec<_> = store
        .list_users()
        .await
        .unwrap()
        .into_iter()
        .map(|u| u.username)
        .collect();
    assert_eq!(names, vec!["alex", "max", "zoe"]);
}

#[tokio::test]
async fn get_user_returns_none_for_unknown() {
    let (store, _dir) = fresh_store().await;
    assert!(store.get_user("ghost").await.unwrap().is_none());
}

#[tokio::test]
async fn user_view_never_carries_password_hash() {
    // Defense-in-depth: the public User struct must not have a password_hash
    // field. If someone adds one, this will fail to compile (no field) —
    // the assertion is just here so the test name is greppable.
    let (store, _dir) = fresh_store().await;
    let u = store
        .create_user("daisy", "secret-hash", Role::Admin)
        .await
        .unwrap();
    let _ = u.username; // touch fields to keep the compiler honest
    let _ = u.role;
    let json = serde_json::to_string(&u).unwrap();
    assert!(
        !json.contains("password"),
        "serialized User must not leak password fields: {json}"
    );
    assert!(
        !json.contains("secret-hash"),
        "serialized User must not contain the hash itself"
    );
}

#[tokio::test]
async fn get_user_password_hash_returns_what_was_stored() {
    let (store, _dir) = fresh_store().await;
    store
        .create_user("daisy", "stored-hash", Role::Admin)
        .await
        .unwrap();
    let h = store.get_user_password_hash("daisy").await.unwrap();
    assert_eq!(h.as_deref(), Some("stored-hash"));
}

#[tokio::test]
async fn set_user_password_persists() {
    let (store, _dir) = fresh_store().await;
    store
        .create_user("daisy", "old", Role::Admin)
        .await
        .unwrap();
    store.set_user_password("daisy", "new").await.unwrap();
    assert_eq!(
        store
            .get_user_password_hash("daisy")
            .await
            .unwrap()
            .as_deref(),
        Some("new")
    );
}

#[tokio::test]
async fn set_user_password_unknown_user_errors() {
    let (store, _dir) = fresh_store().await;
    assert!(store.set_user_password("ghost", "x").await.is_err());
}

#[tokio::test]
async fn set_user_role_changes_role() {
    let (store, _dir) = fresh_store().await;
    store.create_user("daisy", "h", Role::Viewer).await.unwrap();
    store.set_user_role("daisy", Role::Admin).await.unwrap();
    let u = store.get_user("daisy").await.unwrap().unwrap();
    assert_eq!(u.role, Role::Admin);
}

#[tokio::test]
async fn deactivate_user_sets_timestamp_and_revokes_sessions() {
    let (store, _dir) = fresh_store().await;
    store.create_user("daisy", "h", Role::Viewer).await.unwrap();
    store
        .create_session("sess1", "daisy", Some("1.2.3.4"), None)
        .await
        .unwrap();
    store
        .create_session("sess2", "daisy", Some("5.6.7.8"), None)
        .await
        .unwrap();
    store.set_user_deactivated("daisy", true).await.unwrap();
    let u = store.get_user("daisy").await.unwrap().unwrap();
    assert!(u.deactivated_at.is_some());
    assert!(
        store.list_user_sessions("daisy").await.unwrap().is_empty(),
        "deactivation must revoke every existing session"
    );
}

#[tokio::test]
async fn deactivate_user_can_be_reversed() {
    let (store, _dir) = fresh_store().await;
    store.create_user("daisy", "h", Role::Viewer).await.unwrap();
    store.set_user_deactivated("daisy", true).await.unwrap();
    store.set_user_deactivated("daisy", false).await.unwrap();
    let u = store.get_user("daisy").await.unwrap().unwrap();
    assert!(u.deactivated_at.is_none());
}

#[tokio::test]
async fn delete_user_cascades_sessions() {
    let (store, _dir) = fresh_store().await;
    store.create_user("daisy", "h", Role::Viewer).await.unwrap();
    store
        .create_session("s1", "daisy", None, None)
        .await
        .unwrap();
    store.delete_user("daisy").await.unwrap();
    assert!(store.session_for_id("s1").await.unwrap().is_none());
    assert!(store.get_user("daisy").await.unwrap().is_none());
}

#[tokio::test]
async fn delete_user_unknown_errors() {
    let (store, _dir) = fresh_store().await;
    assert!(store.delete_user("ghost").await.is_err());
}

#[tokio::test]
async fn touch_user_login_updates_timestamp_and_ip() {
    let (store, _dir) = fresh_store().await;
    store.create_user("daisy", "h", Role::Admin).await.unwrap();
    store.touch_user_login("daisy", Some("9.9.9.9")).await;
    let u = store.get_user("daisy").await.unwrap().unwrap();
    assert!(u.last_login_at.is_some());
    assert_eq!(u.last_login_ip.as_deref(), Some("9.9.9.9"));
}

#[tokio::test]
async fn count_active_admins_excludes_deactivated() {
    let (store, _dir) = fresh_store().await;
    store.create_user("a1", "h", Role::Admin).await.unwrap();
    store.create_user("a2", "h", Role::Admin).await.unwrap();
    store.create_user("v", "h", Role::Viewer).await.unwrap();
    assert_eq!(store.count_active_admins().await.unwrap(), 2);
    store.set_user_deactivated("a2", true).await.unwrap();
    assert_eq!(store.count_active_admins().await.unwrap(), 1);
}

#[tokio::test]
async fn role_from_db_str_defaults_to_viewer_on_garbage() {
    // Critical: a corrupt column must NEVER silently grant admin. The
    // application enum maps unknown values to viewer (least privilege).
    assert_eq!(Role::from_db_str("admin"), Role::Admin);
    assert_eq!(Role::from_db_str("viewer"), Role::Viewer);
    assert_eq!(Role::from_db_str("ghost"), Role::Viewer);
    assert_eq!(Role::from_db_str(""), Role::Viewer);
}

// ----- sessions -----

#[tokio::test]
async fn create_session_returns_record() {
    let (store, _dir) = fresh_store().await;
    store.create_user("daisy", "h", Role::Admin).await.unwrap();
    let s = store
        .create_session("abc123", "daisy", Some("1.2.3.4"), Some("Mozilla"))
        .await
        .unwrap();
    assert_eq!(s.id, "abc123");
    assert_eq!(s.username, "daisy");
    assert_eq!(s.ip.as_deref(), Some("1.2.3.4"));
}

#[tokio::test]
async fn session_for_id_returns_role_and_deactivated_flag() {
    let (store, _dir) = fresh_store().await;
    store.create_user("daisy", "h", Role::Admin).await.unwrap();
    store
        .create_session("s1", "daisy", None, None)
        .await
        .unwrap();
    let (s, role, deactivated) = store.session_for_id("s1").await.unwrap().unwrap();
    assert_eq!(s.username, "daisy");
    assert_eq!(role, Role::Admin);
    assert!(!deactivated);
}

#[tokio::test]
async fn session_for_id_reflects_deactivation() {
    let (store, _dir) = fresh_store().await;
    store.create_user("daisy", "h", Role::Admin).await.unwrap();
    store
        .create_session("s1", "daisy", None, None)
        .await
        .unwrap();
    store.set_user_deactivated("daisy", true).await.unwrap();
    // Sessions are revoked on deactivation.
    assert!(store.session_for_id("s1").await.unwrap().is_none());
}

#[tokio::test]
async fn session_for_id_returns_none_for_unknown() {
    let (store, _dir) = fresh_store().await;
    assert!(store.session_for_id("nope").await.unwrap().is_none());
}

#[tokio::test]
async fn touch_session_advances_last_seen() {
    let (store, _dir) = fresh_store().await;
    store.create_user("daisy", "h", Role::Admin).await.unwrap();
    store
        .create_session("s1", "daisy", None, None)
        .await
        .unwrap();
    let before = store
        .session_for_id("s1")
        .await
        .unwrap()
        .unwrap()
        .0
        .last_seen_at;
    // Sleep a tick so the timestamp can move at second precision.
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    store.touch_session("s1").await;
    let after = store
        .session_for_id("s1")
        .await
        .unwrap()
        .unwrap()
        .0
        .last_seen_at;
    assert!(after > before, "touch must advance last_seen_at");
}

#[tokio::test]
async fn revoke_session_removes_only_target() {
    let (store, _dir) = fresh_store().await;
    store.create_user("daisy", "h", Role::Admin).await.unwrap();
    store
        .create_session("a", "daisy", None, None)
        .await
        .unwrap();
    store
        .create_session("b", "daisy", None, None)
        .await
        .unwrap();
    store.revoke_session("a").await.unwrap();
    assert!(store.session_for_id("a").await.unwrap().is_none());
    assert!(store.session_for_id("b").await.unwrap().is_some());
}

#[tokio::test]
async fn revoke_user_sessions_returns_count() {
    let (store, _dir) = fresh_store().await;
    store.create_user("daisy", "h", Role::Admin).await.unwrap();
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
    let n = store.revoke_user_sessions("daisy").await.unwrap();
    assert_eq!(n, 3);
    assert!(store.list_user_sessions("daisy").await.unwrap().is_empty());
}

#[tokio::test]
async fn list_user_sessions_orders_by_last_seen_desc() {
    let (store, _dir) = fresh_store().await;
    store.create_user("daisy", "h", Role::Admin).await.unwrap();
    store
        .create_session("old", "daisy", None, None)
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
    store
        .create_session("new", "daisy", None, None)
        .await
        .unwrap();
    let ids: Vec<_> = store
        .list_user_sessions("daisy")
        .await
        .unwrap()
        .into_iter()
        .map(|s| s.id)
        .collect();
    assert_eq!(ids, vec!["new", "old"]);
}

#[tokio::test]
async fn purge_sessions_idle_since_drops_only_old() {
    let (store, _dir) = fresh_store().await;
    store.create_user("daisy", "h", Role::Admin).await.unwrap();
    store
        .create_session("recent", "daisy", None, None)
        .await
        .unwrap();
    store
        .create_session("old", "daisy", None, None)
        .await
        .unwrap();
    sqlx::query("UPDATE sessions SET last_seen_at = ? WHERE id = ?")
        .bind((Utc::now() - Duration::days(60)).to_rfc3339())
        .bind("old")
        .execute(store.pool())
        .await
        .unwrap();
    let n = store
        .purge_sessions_idle_since(Utc::now() - Duration::days(30))
        .await
        .unwrap();
    assert_eq!(n, 1);
    assert!(store.session_for_id("old").await.unwrap().is_none());
    assert!(store.session_for_id("recent").await.unwrap().is_some());
}

// ----- auth_attempts -----

#[tokio::test]
async fn record_attempt_persists_with_username_and_ip() {
    let (store, _dir) = fresh_store().await;
    store.record_attempt(Some("daisy"), "1.2.3.4", false).await;
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM auth_attempts WHERE username = 'daisy' AND ip = '1.2.3.4'",
    )
    .fetch_one(store.pool())
    .await
    .unwrap();
    assert_eq!(row.0, 1);
}

#[tokio::test]
async fn record_attempt_supports_null_username() {
    let (store, _dir) = fresh_store().await;
    store.record_attempt(None, "1.2.3.4", false).await;
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM auth_attempts WHERE username IS NULL AND ip = '1.2.3.4'",
    )
    .fetch_one(store.pool())
    .await
    .unwrap();
    assert_eq!(row.0, 1);
}

#[tokio::test]
async fn count_recent_failures_for_username_excludes_successes() {
    let (store, _dir) = fresh_store().await;
    let now = Utc::now();
    for _ in 0..3 {
        store.record_attempt(Some("daisy"), "1.1.1.1", false).await;
    }
    store.record_attempt(Some("daisy"), "1.1.1.1", true).await;
    let n = store
        .count_recent_failures_for_username("daisy", now - Duration::minutes(15))
        .await
        .unwrap();
    assert_eq!(n, 3, "successful attempts must not count toward lockout");
}

#[tokio::test]
async fn count_recent_failures_for_username_respects_window() {
    let (store, _dir) = fresh_store().await;
    store.record_attempt(Some("daisy"), "1.1.1.1", false).await;
    // Backdate the row beyond the window.
    sqlx::query("UPDATE auth_attempts SET ts = ? WHERE username = 'daisy'")
        .bind((Utc::now() - Duration::hours(2)).to_rfc3339())
        .execute(store.pool())
        .await
        .unwrap();
    let n = store
        .count_recent_failures_for_username("daisy", Utc::now() - Duration::minutes(15))
        .await
        .unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn count_recent_failures_for_ip_aggregates_across_usernames() {
    let (store, _dir) = fresh_store().await;
    store.record_attempt(Some("daisy"), "9.9.9.9", false).await;
    store.record_attempt(Some("alex"), "9.9.9.9", false).await;
    store.record_attempt(None, "9.9.9.9", false).await;
    let n = store
        .count_recent_failures_for_ip("9.9.9.9", Utc::now() - Duration::minutes(15))
        .await
        .unwrap();
    assert_eq!(
        n, 3,
        "IP-bucketed lockout must count across all usernames AND null-username probes"
    );
}

#[tokio::test]
async fn prune_auth_attempts_deletes_only_old() {
    let (store, _dir) = fresh_store().await;
    store.record_attempt(Some("daisy"), "1.1.1.1", false).await;
    store.record_attempt(Some("daisy"), "1.1.1.1", false).await;
    sqlx::query("UPDATE auth_attempts SET ts = ? WHERE id = (SELECT MIN(id) FROM auth_attempts)")
        .bind((Utc::now() - Duration::days(2)).to_rfc3339())
        .execute(store.pool())
        .await
        .unwrap();
    let n = store
        .prune_auth_attempts_older_than(Utc::now() - Duration::days(1))
        .await
        .unwrap();
    assert_eq!(n, 1);
}
