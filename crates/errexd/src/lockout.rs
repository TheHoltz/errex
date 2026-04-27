//! Brute-force protection for the login endpoint.
//!
//! The decision is split into two pieces:
//!
//!   1. **`LockoutPolicy::evaluate(user_failures, ip_failures)`** — a pure
//!      function. No DB, no clock. Easy to unit-test exhaustively.
//!   2. **`Store::count_recent_failures_for_username` / `_for_ip`** — the
//!      DB queries that feed `evaluate`. Tested separately.
//!
//! Splitting the policy from the I/O lets us pin the exact threshold logic
//! without spinning up SQLite per case.
//!
//! # Why both per-account AND per-IP
//!
//! A per-account-only policy lets one IP spray N usernames at the threshold
//! limit each. A per-IP-only policy lets one attacker behind many IPs grind
//! a single account. Both together cover both threat models with the same
//! `auth_attempts` ledger.

use chrono::Duration;

#[derive(Debug, Clone, Copy)]
pub struct LockoutPolicy {
    pub user_threshold: i64,
    pub ip_threshold: i64,
    /// How far back the failure counter looks. The DB query bounds itself
    /// to this same window, so the policy only sees fresh data.
    pub window: Duration,
    /// What to put in `Retry-After` when blocked. Set equal to `window` so
    /// "give up for 15 minutes" gives the bucket time to drain naturally.
    pub cooldown: Duration,
}

impl Default for LockoutPolicy {
    fn default() -> Self {
        Self {
            user_threshold: 5,
            ip_threshold: 20,
            window: Duration::minutes(15),
            cooldown: Duration::minutes(15),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockoutDecision {
    /// Caller may proceed with the password check.
    Allow,
    /// Username has hit the per-account threshold within the window.
    BlockedByUser { retry_after_secs: i64 },
    /// IP has hit the cross-account spray threshold.
    BlockedByIp { retry_after_secs: i64 },
}

impl LockoutDecision {
    /// Convenience for the test suite — production callers branch on
    /// `retry_after_secs().is_some()` directly.
    #[cfg(test)]
    pub fn is_allowed(self) -> bool {
        matches!(self, LockoutDecision::Allow)
    }
    pub fn retry_after_secs(self) -> Option<i64> {
        match self {
            LockoutDecision::Allow => None,
            LockoutDecision::BlockedByUser { retry_after_secs }
            | LockoutDecision::BlockedByIp { retry_after_secs } => Some(retry_after_secs),
        }
    }
}

impl LockoutPolicy {
    /// Per-account check fires before per-IP check. The reason: if a
    /// targeted attacker hammers `daisy` they should see a username-shaped
    /// 429 (lets the legitimate `daisy` know "your account got attacked")
    /// rather than a generic IP-shaped one. Both responses are the same
    /// HTTP status / Retry-After to the wire — the distinction is only
    /// visible in our logs.
    pub fn evaluate(self, user_failures: i64, ip_failures: i64) -> LockoutDecision {
        let cooldown_secs = self.cooldown.num_seconds().max(1);
        if user_failures >= self.user_threshold {
            return LockoutDecision::BlockedByUser {
                retry_after_secs: cooldown_secs,
            };
        }
        if ip_failures >= self.ip_threshold {
            return LockoutDecision::BlockedByIp {
                retry_after_secs: cooldown_secs,
            };
        }
        LockoutDecision::Allow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> LockoutPolicy {
        LockoutPolicy::default()
    }

    #[test]
    fn allows_when_both_below_thresholds() {
        assert_eq!(policy().evaluate(0, 0), LockoutDecision::Allow);
        assert_eq!(policy().evaluate(4, 19), LockoutDecision::Allow);
    }

    #[test]
    fn blocks_at_user_threshold_exactly() {
        // 5 failures = block. Spec: "5 failures in 15 min → reject".
        let d = policy().evaluate(5, 0);
        assert!(matches!(d, LockoutDecision::BlockedByUser { .. }));
        assert_eq!(d.retry_after_secs(), Some(15 * 60));
    }

    #[test]
    fn blocks_at_ip_threshold_exactly() {
        let d = policy().evaluate(0, 20);
        assert!(matches!(d, LockoutDecision::BlockedByIp { .. }));
        assert_eq!(d.retry_after_secs(), Some(15 * 60));
    }

    #[test]
    fn user_threshold_takes_precedence_over_ip_when_both_breached() {
        // The order matters for the variant returned (we log username
        // shape vs IP shape differently). Per the doc-comment on
        // `evaluate`, user wins.
        let d = policy().evaluate(5, 20);
        assert!(matches!(d, LockoutDecision::BlockedByUser { .. }));
    }

    #[test]
    fn high_counts_still_block() {
        // No off-by-one above the threshold.
        assert!(!policy().evaluate(99, 0).is_allowed());
        assert!(!policy().evaluate(0, 999).is_allowed());
    }

    #[test]
    fn custom_policy_thresholds_are_honoured() {
        let p = LockoutPolicy {
            user_threshold: 3,
            ip_threshold: 100,
            window: Duration::minutes(5),
            cooldown: Duration::minutes(5),
        };
        assert!(p.evaluate(2, 0).is_allowed());
        assert!(!p.evaluate(3, 0).is_allowed());
        assert_eq!(p.evaluate(3, 0).retry_after_secs(), Some(300));
    }

    #[test]
    fn retry_after_is_at_least_one_second() {
        // Edge case: if someone sets cooldown to a sub-second value, we
        // still need a positive integer for the Retry-After header.
        let p = LockoutPolicy {
            cooldown: Duration::milliseconds(0),
            ..Default::default()
        };
        assert_eq!(p.evaluate(99, 0).retry_after_secs(), Some(1));
    }

    #[test]
    fn allow_carries_no_retry_after() {
        assert_eq!(LockoutDecision::Allow.retry_after_secs(), None);
    }
}
