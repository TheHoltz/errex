//! Wire-format pinning tests.
//!
//! errexd and the SPA agree on JSON shape via `serde`. Any silent rename or
//! type change here breaks every connected client. These tests serialize and
//! deserialize each surface type and assert the literal field names + values
//! the SPA reads.
//!
//! When you change a wire type *intentionally*, update these tests in the
//! same commit so reviewers see the wire impact.

use chrono::{DateTime, TimeZone, Utc};
use errex_proto::{
    ClientMessage, Event, ExceptionContainer, ExceptionInfo, Fingerprint, Frame, Issue,
    IssueStatus, Level, ServerMessage, Stacktrace,
};
use serde_json::{json, Value};
use uuid::Uuid;

fn fixed_ts() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap()
}

// ----- Event -----

#[test]
fn event_deserializes_minimal_payload() {
    // The smallest payload Sentry SDKs may send: just timestamp + an
    // exception. Defaults must fill in everything else.
    let raw = json!({
        "timestamp": "2026-01-02T03:04:05Z",
        "exception": { "values": [{ "type": "TypeError", "value": "x" }] },
    });
    let ev: Event = serde_json::from_value(raw).expect("minimal event must parse");
    assert_eq!(ev.platform, None);
    assert_eq!(ev.level, None);
    assert_eq!(ev.message, None);
    let ex = ev.primary_exception().unwrap();
    assert_eq!(ex.ty.as_deref(), Some("TypeError"));
}

#[test]
fn event_deserializes_full_payload() {
    let raw = json!({
        "event_id": "11111111111111111111111111111111",
        "timestamp": "2026-01-02T03:04:05Z",
        "platform": "javascript",
        "level": "error",
        "environment": "prod",
        "release": "1.0.0",
        "server_name": "edge-1",
        "message": "boom",
        "exception": { "values": [{
            "type": "TypeError",
            "value": "x is not a function",
            "stacktrace": { "frames": [
                { "function": "f", "filename": "a.js", "lineno": 12, "in_app": true }
            ]}
        }]},
        "breadcrumbs": { "values": [{ "category": "navigation", "message": "/checkout" }] },
        "tags": { "browser": "chrome" },
        "contexts": { "browser": { "name": "Chrome" } },
        "extra": { "session_id": "abc" },
        "user": { "id": "42" },
        "request": { "url": "https://example.com" }
    });
    let ev: Event = serde_json::from_value(raw).expect("full event must parse");
    assert_eq!(ev.platform.as_deref(), Some("javascript"));
    assert_eq!(ev.level, Some(Level::Error));
    assert_eq!(ev.environment.as_deref(), Some("prod"));
    assert_eq!(ev.release.as_deref(), Some("1.0.0"));
    assert_eq!(ev.server_name.as_deref(), Some("edge-1"));
    // The optional bag fields preserve their JSON shape verbatim.
    assert!(ev.breadcrumbs.is_some());
    assert!(ev.tags.is_some());
    assert!(ev.contexts.is_some());
    assert!(ev.extra.is_some());
    assert!(ev.user.is_some());
    assert!(ev.request.is_some());
}

#[test]
fn event_unknown_fields_are_ignored() {
    let raw = json!({
        "timestamp": "2026-01-02T03:04:05Z",
        "platform": "node",
        "future_field_we_dont_know_about": "should not break parsing",
    });
    let ev: Event = serde_json::from_value(raw).expect("unknown fields must not fail");
    assert_eq!(ev.platform.as_deref(), Some("node"));
}

#[test]
fn event_round_trip_preserves_passthrough_fields() {
    // Round-trip is the contract that lets `latest_event` return a payload
    // the SPA can render — breadcrumbs and tags must come back intact even
    // though we don't model them as typed structs.
    let original = json!({
        "event_id": "11111111111111111111111111111111",
        "timestamp": "2026-01-02T03:04:05Z",
        "tags": { "k1": "v1", "k2": "v2" },
        "breadcrumbs": { "values": [{ "category": "x" }, { "category": "y" }] },
    });
    let ev: Event = serde_json::from_value(original.clone()).unwrap();
    let back: Value = serde_json::to_value(&ev).unwrap();
    assert_eq!(back.get("tags"), original.get("tags"));
    assert_eq!(back.get("breadcrumbs"), original.get("breadcrumbs"));
}

#[test]
fn level_serializes_as_lowercase_string() {
    // The SPA's IssueLevel union is lowercase; any rename here breaks it.
    let cases = [
        (Level::Debug, "debug"),
        (Level::Info, "info"),
        (Level::Warning, "warning"),
        (Level::Error, "error"),
        (Level::Fatal, "fatal"),
    ];
    for (l, expected) in cases {
        let v = serde_json::to_value(l).unwrap();
        assert_eq!(v, Value::String(expected.into()));
    }
}

#[test]
fn frame_renames_in_app_correctly() {
    let raw = json!({ "function": "f", "in_app": true });
    let f: Frame = serde_json::from_value(raw).unwrap();
    assert_eq!(f.function.as_deref(), Some("f"));
    assert_eq!(f.in_app, Some(true));
}

#[test]
fn exception_renames_type_field() {
    // `type` is a Rust keyword so the proto uses `ty` internally with a
    // serde rename. The wire still reads `type` — pin that.
    let raw = json!({ "type": "RuntimeError", "value": "boom" });
    let info: ExceptionInfo = serde_json::from_value(raw).unwrap();
    assert_eq!(info.ty.as_deref(), Some("RuntimeError"));
    assert_eq!(info.value.as_deref(), Some("boom"));

    let back = serde_json::to_value(&info).unwrap();
    assert_eq!(
        back.get("type").and_then(|v| v.as_str()),
        Some("RuntimeError")
    );
    assert!(
        back.get("ty").is_none(),
        "must not leak the Rust field name"
    );
}

// ----- Issue -----

#[test]
fn issue_round_trips_through_json() {
    let issue = Issue {
        id: 42,
        project: "demo".into(),
        fingerprint: Fingerprint::new("abc123".to_string()),
        title: "TypeError: x".into(),
        culprit: Some("f in a.js".into()),
        level: Some("error".into()),
        status: IssueStatus::Unresolved,
        event_count: 7,
        first_seen: fixed_ts(),
        last_seen: fixed_ts(),
    };
    let v = serde_json::to_value(&issue).unwrap();
    // Names the SPA reads must be exactly these.
    for name in [
        "id",
        "project",
        "fingerprint",
        "title",
        "culprit",
        "level",
        "status",
        "event_count",
        "first_seen",
        "last_seen",
    ] {
        assert!(v.get(name).is_some(), "missing wire field: {name}");
    }
    let round: Issue = serde_json::from_value(v).unwrap();
    assert_eq!(round.id, 42);
    assert_eq!(round.event_count, 7);
    assert_eq!(round.fingerprint.as_str(), "abc123");
    assert_eq!(round.status, IssueStatus::Unresolved);
}

#[test]
fn issue_status_serializes_lowercase() {
    let cases = [
        (IssueStatus::Unresolved, "unresolved"),
        (IssueStatus::Resolved, "resolved"),
        (IssueStatus::Muted, "muted"),
        (IssueStatus::Ignored, "ignored"),
    ];
    for (s, expected) in cases {
        let v = serde_json::to_value(s).unwrap();
        assert_eq!(v, Value::String(expected.into()));
    }
}

#[test]
fn issue_status_deserializes_lowercase() {
    let s: IssueStatus = serde_json::from_value(json!("muted")).unwrap();
    assert_eq!(s, IssueStatus::Muted);
}

#[test]
fn issue_default_status_is_unresolved_when_field_missing() {
    // SDKs and older payloads won't include status; deserialization must
    // fill in `unresolved` rather than fail.
    let raw = json!({
        "id": 1,
        "project": "p",
        "fingerprint": "fp",
        "title": "T",
        "culprit": null,
        "level": null,
        "event_count": 1,
        "first_seen": "2026-01-02T03:04:05Z",
        "last_seen": "2026-01-02T03:04:05Z",
    });
    let issue: Issue = serde_json::from_value(raw).unwrap();
    assert_eq!(issue.status, IssueStatus::Unresolved);
}

#[test]
fn fingerprint_serializes_as_bare_string() {
    // `#[serde(transparent)]` keeps the wire shape as a string, not an
    // object. The SPA's type is `fingerprint: string`; an accidental
    // refactor that wraps this in `{value: "..."}` would silently break.
    let fp = Fingerprint::new("deadbeef".to_string());
    let v = serde_json::to_value(&fp).unwrap();
    assert_eq!(v, Value::String("deadbeef".into()));
}

// ----- ServerMessage / ClientMessage -----

#[test]
fn server_message_uses_snake_case_tag() {
    let hello = ServerMessage::Hello {
        server_version: "1.2.3".into(),
    };
    let v = serde_json::to_value(&hello).unwrap();
    assert_eq!(v.get("type").and_then(|x| x.as_str()), Some("hello"));
    assert_eq!(
        v.get("server_version").and_then(|x| x.as_str()),
        Some("1.2.3")
    );
}

#[test]
fn server_message_snapshot_carries_issues_array() {
    let snap = ServerMessage::Snapshot { issues: vec![] };
    let v = serde_json::to_value(&snap).unwrap();
    assert_eq!(v.get("type").and_then(|x| x.as_str()), Some("snapshot"));
    assert!(v.get("issues").map(|v| v.is_array()).unwrap_or(false));
}

#[test]
fn server_message_issue_created_and_updated_use_distinct_tags() {
    // The SPA dispatches on these strings; renaming either is a wire break.
    let issue = Issue {
        id: 1,
        project: "p".into(),
        fingerprint: Fingerprint::new("fp"),
        title: "t".into(),
        culprit: None,
        level: None,
        status: IssueStatus::Unresolved,
        event_count: 1,
        first_seen: fixed_ts(),
        last_seen: fixed_ts(),
    };
    let created = serde_json::to_value(ServerMessage::IssueCreated {
        issue: issue.clone(),
    })
    .unwrap();
    let updated = serde_json::to_value(ServerMessage::IssueUpdated { issue }).unwrap();
    assert_eq!(
        created.get("type").and_then(|x| x.as_str()),
        Some("issue_created")
    );
    assert_eq!(
        updated.get("type").and_then(|x| x.as_str()),
        Some("issue_updated")
    );
}

#[test]
fn client_message_ping_round_trips() {
    let ping = ClientMessage::Ping;
    let v = serde_json::to_value(&ping).unwrap();
    assert_eq!(v.get("type").and_then(|x| x.as_str()), Some("ping"));
    let back: ClientMessage = serde_json::from_value(v).unwrap();
    assert!(matches!(back, ClientMessage::Ping));
}

// Anchor that makes Uuid + Stacktrace + ExceptionContainer "used" if
// dependencies regress. These types must be re-exported from the crate root.
#[test]
fn types_are_reexported_from_crate_root() {
    let _ = Uuid::new_v4();
    let _ = ExceptionContainer { values: vec![] };
    let _ = Stacktrace { frames: vec![] };
}
