//! Fingerprint derivation for grouping similar events.
//!
//! Strategy is deliberately naive for the scaffold: hash the exception type
//! plus the topmost in-app frame (function + filename). Real Sentry-style
//! grouping needs more nuance (module normalization, frame-skip rules,
//! message templating), and that work belongs in a dedicated module.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use errex_proto::{Event, Fingerprint};

pub fn derive(event: &Event) -> Fingerprint {
    let mut h = DefaultHasher::new();

    if let Some(ex) = event.primary_exception() {
        ex.ty.hash(&mut h);
        if let Some(frame) = ex.first_frame() {
            frame.function.hash(&mut h);
            frame.filename.hash(&mut h);
        }
    } else if let Some(msg) = &event.message {
        msg.hash(&mut h);
    } else {
        // Nothing distinguishing — fall back to event id so each event is its
        // own group rather than collapsing all unknown events together.
        event.event_id.hash(&mut h);
    }

    Fingerprint::new(format!("{:016x}", h.finish()))
}

#[cfg(test)]
mod tests {
    //! Fingerprint behavior tests. We pin the *contract* (what produces
    //! equal vs distinct fingerprints) rather than concrete hash values,
    //! because `DefaultHasher` is intentionally not stable across Rust
    //! versions — a SHA-256 / blake3 cutover later will change values
    //! without breaking any of these assertions.
    //!
    //! Stability across Rust versions is itself a TODO (see issue tracker):
    //! once the daemon stores issues persistently across upgrades, the
    //! fingerprint algorithm must be deterministic across compiler bumps
    //! or grouping would silently fragment.

    use super::*;
    use chrono::Utc;
    use errex_proto::{ExceptionContainer, ExceptionInfo, Frame, Stacktrace};
    use uuid::Uuid;

    fn ev_with_exception(ty: &str, function: &str, filename: &str, lineno: u32) -> Event {
        Event {
            event_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            platform: None,
            level: None,
            environment: None,
            release: None,
            server_name: None,
            message: None,
            exception: Some(ExceptionContainer {
                values: vec![ExceptionInfo {
                    ty: Some(ty.into()),
                    value: None,
                    module: None,
                    stacktrace: Some(Stacktrace {
                        frames: vec![Frame {
                            filename: Some(filename.into()),
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

    fn ev_message(msg: &str) -> Event {
        Event {
            event_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            platform: None,
            level: None,
            environment: None,
            release: None,
            server_name: None,
            message: Some(msg.into()),
            exception: None,
            breadcrumbs: None,
            tags: None,
            contexts: None,
            extra: None,
            user: None,
            request: None,
        }
    }

    fn ev_empty() -> Event {
        Event {
            event_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            platform: None,
            level: None,
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
        }
    }

    // ----- shape -----

    #[test]
    fn output_is_16_hex_chars() {
        let fp = derive(&ev_with_exception("E", "f", "a.js", 1));
        let s = fp.as_str();
        assert_eq!(s.len(), 16);
        assert!(
            s.chars().all(|c| c.is_ascii_hexdigit()),
            "non-hex char: {s}"
        );
    }

    #[test]
    fn deterministic_across_calls() {
        let a = derive(&ev_with_exception("E", "f", "a.js", 1));
        let b = derive(&ev_with_exception("E", "f", "a.js", 1));
        assert_eq!(a, b, "same inputs must hash identically");
    }

    // ----- grouping (same fingerprint) -----

    #[test]
    fn lineno_and_event_id_do_not_affect_grouping() {
        // Two events with identical type+function+filename but different
        // lineno and event_id must group together. Otherwise every event
        // would be its own issue and the daemon's value vanishes.
        let a = derive(&ev_with_exception(
            "TypeError",
            "checkout",
            "src/pay.ts",
            10,
        ));
        let b = derive(&ev_with_exception(
            "TypeError",
            "checkout",
            "src/pay.ts",
            273,
        ));
        assert_eq!(a, b);
    }

    // ----- distinction (different fingerprints) -----

    #[test]
    fn different_exception_types_produce_different_fingerprints() {
        let a = derive(&ev_with_exception("TypeError", "f", "a.js", 1));
        let b = derive(&ev_with_exception("ReferenceError", "f", "a.js", 1));
        assert_ne!(a, b);
    }

    #[test]
    fn different_functions_produce_different_fingerprints() {
        let a = derive(&ev_with_exception("E", "alpha", "a.js", 1));
        let b = derive(&ev_with_exception("E", "beta", "a.js", 1));
        assert_ne!(a, b);
    }

    #[test]
    fn different_filenames_produce_different_fingerprints() {
        let a = derive(&ev_with_exception("E", "f", "a.js", 1));
        let b = derive(&ev_with_exception("E", "f", "b.js", 1));
        assert_ne!(a, b);
    }

    // ----- fallbacks -----

    #[test]
    fn message_only_events_group_by_message() {
        let a = derive(&ev_message("Database connection lost"));
        let b = derive(&ev_message("Database connection lost"));
        let c = derive(&ev_message("Different message"));
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn empty_event_falls_back_to_event_id() {
        // No exception, no message: each event must be its own group rather
        // than collapsing all "unknown" events into a single noisy bucket.
        let a = derive(&ev_empty());
        let b = derive(&ev_empty());
        assert_ne!(a, b, "two empty events must produce distinct fingerprints");
    }

    #[test]
    fn exception_takes_precedence_over_message() {
        // If an event has BOTH a message and an exception, the exception
        // is the grouping key. (The message often differs per occurrence
        // even when the exception is the same.)
        let mut with_ex = ev_with_exception("E", "f", "a.js", 1);
        with_ex.message = Some("changing message 1".into());
        let mut with_ex2 = ev_with_exception("E", "f", "a.js", 1);
        with_ex2.message = Some("changing message 2".into());
        assert_eq!(derive(&with_ex), derive(&with_ex2));
    }
}
