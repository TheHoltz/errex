//! AI-driven triage — STUB.
//!
//! The intent: when an issue first appears, hand the event + recent context
//! to an LLM and have it produce a one-paragraph diagnosis, suspect commit,
//! and a suggested owner. Nothing wired yet.

#![allow(dead_code)]

use errex_proto::Event;

/// Placeholder for the future triage entry point.
///
/// TODO: pluggable backend (claude / local); cache per-fingerprint results;
/// rate-limit by project; surface output through the WS fan-out so TUIs can
/// render it under the issue detail pane.
pub async fn triage(_event: &Event) -> Option<String> {
    None
}
