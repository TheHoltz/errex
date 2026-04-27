//! Operator-facing metrics.
//!
//! Cheap atomics + a thin `/metrics` endpoint. The goal is to surface the
//! "silent failure" surface flagged by stress testing: ingest channel
//! depth (head-of-line backpressure), webhook channel depth (alert drop
//! risk), and broadcast lag (ws clients silently missing updates). All
//! counters are zero-cost when nobody scrapes the endpoint.

use std::sync::atomic::{AtomicU64, Ordering};

/// Process-lifetime counters. Atomics + relaxed ordering: counters are
/// monotonic and snapshot reads don't need cross-counter consistency.
#[derive(Debug, Default)]
pub struct Metrics {
    /// Events accepted by the HTTP handler and pushed onto the digest channel.
    pub events_accepted: AtomicU64,
    /// Requests rejected because the per-project rate limiter denied them.
    pub events_rejected_rate_limit: AtomicU64,
    /// Total `Lagged(skipped)` events observed by WS subscribers — when
    /// non-zero, the broadcast capacity is too small for the current load
    /// or a subscriber is too slow.
    pub ws_lagged_total: AtomicU64,
}

impl Metrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn inc_accepted(&self) {
        self.events_accepted.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_rejected_rate_limit(&self) {
        self.events_rejected_rate_limit
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_ws_lagged(&self, n: u64) {
        self.ws_lagged_total.fetch_add(n, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            events_accepted: self.events_accepted.load(Ordering::Relaxed),
            events_rejected_rate_limit: self.events_rejected_rate_limit.load(Ordering::Relaxed),
            ws_lagged_total: self.ws_lagged_total.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct Snapshot {
    pub events_accepted: u64,
    pub events_rejected_rate_limit: u64,
    pub ws_lagged_total: u64,
}

/// Read RSS in kilobytes from `/proc/self/status`. Returns `None` on
/// non-Linux or if the file is unparseable. Cheap (a few-KB read) and only
/// runs when /metrics is scraped.
pub fn read_rss_kb() -> Option<u64> {
    let s = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in s.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            return rest.split_whitespace().next()?.parse().ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_increment_independently() {
        let m = Metrics::new();
        m.inc_accepted();
        m.inc_accepted();
        m.inc_rejected_rate_limit();
        m.add_ws_lagged(3);
        let snap = m.snapshot();
        assert_eq!(snap.events_accepted, 2);
        assert_eq!(snap.events_rejected_rate_limit, 1);
        assert_eq!(snap.ws_lagged_total, 3);
    }

    #[test]
    fn rss_kb_is_some_on_linux() {
        // Skip non-Linux quietly so the test suite stays portable. We only
        // ship Linux — but `cargo test` on a contributor's mac shouldn't
        // fail because /proc isn't there.
        if !std::path::Path::new("/proc/self/status").exists() {
            return;
        }
        let kb = read_rss_kb().expect("rss readable on linux");
        assert!(kb > 0, "rss should be positive, got {kb}");
    }
}
