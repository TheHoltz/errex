//! Background retention task: bounds payload age, events-per-issue, and
//! issues-per-project. Runs on a 1-hour cadence — frequent enough that a
//! UI-driven setting change takes effect within an hour, infrequent enough
//! that we don't churn the DB during normal operation.
//!
//! Settings are read from the `retention_settings` row each tick (cheap)
//! so an operator can tighten retention via the SPA without redeploying
//! the daemon. The CLI flag (`ERREXD_RETENTION_DAYS`) remains the boot
//! default for *event age* — it kicks in when the DB-level
//! `event_retention_days` is 0 (the migration default).

use std::time::Duration;

use chrono::Utc;
use tokio::time::{interval, MissedTickBehavior};

use crate::store::Store;

/// Spawn the retention task. `days == 0` AND no DB-level overrides ⇒ the
/// task still runs (so a UI-set bound takes effect), but each individual
/// purge becomes a no-op until something is configured.
pub async fn run(store: Store, days: u32) {
    let mut tick = interval(Duration::from_secs(60 * 60));
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    tracing::info!(boot_days = days, "retention task started");

    loop {
        tick.tick().await;
        run_one_tick(&store, days).await;
    }
}

/// One purge sweep. Extracted so tests can drive it deterministically
/// without waiting on the hourly interval.
pub async fn run_one_tick(store: &Store, boot_days: u32) {
    // Pull the live settings each tick so a UI change takes effect
    // without restart. Failure here is fatal-to-the-tick only — the next
    // hour gets a fresh chance.
    let settings = match store.get_retention_settings().await {
        Ok(s) => s,
        Err(err) => {
            tracing::warn!(%err, "retention: failed to read settings; skipping tick");
            return;
        }
    };

    // Effective event-age horizon: DB-level wins when set, else fall back
    // to the boot config. Both 0 ⇒ event-age purge is disabled.
    let effective_days = if settings.event_retention_days > 0 {
        settings.event_retention_days
    } else {
        boot_days as i64
    };
    if effective_days > 0 {
        let event_cutoff = Utc::now() - chrono::Duration::days(effective_days);
        match store.purge_events_older_than(event_cutoff).await {
            Ok(0) => tracing::debug!("retention: nothing aged out"),
            Ok(n) => {
                tracing::info!(deleted = n, cutoff = %event_cutoff, "retention: purged aged events")
            }
            Err(err) => tracing::warn!(%err, "retention: age purge failed"),
        }
    }

    if settings.events_per_issue_max > 0 {
        match store
            .purge_excess_events_per_issue(settings.events_per_issue_max)
            .await
        {
            Ok(0) => tracing::debug!("retention: events_per_issue cap met"),
            Ok(n) => tracing::info!(
                deleted = n,
                max = settings.events_per_issue_max,
                "retention: trimmed events per issue"
            ),
            Err(err) => tracing::warn!(%err, "retention: events_per_issue purge failed"),
        }
    }

    if settings.issues_per_project_max > 0 {
        match store
            .purge_excess_issues_per_project(settings.issues_per_project_max)
            .await
        {
            Ok(0) => tracing::debug!("retention: issues_per_project cap met"),
            Ok(n) => tracing::info!(
                deleted = n,
                max = settings.issues_per_project_max,
                "retention: trimmed issues per project"
            ),
            Err(err) => tracing::warn!(%err, "retention: issues_per_project purge failed"),
        }
    }

    // Auth attempts are bounded by the lockout window (15 min) — keeping
    // anything older than 24h is purely for forensic context and quickly
    // becomes noise. The same hourly tick handles it; we don't need a
    // dedicated task for two extra DELETEs.
    let attempts_cutoff = Utc::now() - chrono::Duration::hours(24);
    match store.prune_auth_attempts_older_than(attempts_cutoff).await {
        Ok(0) => {}
        Ok(n) => tracing::debug!(deleted = n, "retention: pruned auth attempts"),
        Err(err) => tracing::warn!(%err, "retention: auth-attempts purge failed"),
    }

    // Sessions: drop any whose `last_seen_at` is past the sliding-cookie
    // expiry. The browser would refuse the cookie at this point anyway,
    // but evicting the row keeps the table from accumulating ghost
    // sessions for users who just closed their tab and never came back.
    let session_cutoff = Utc::now() - chrono::Duration::days(30);
    match store.purge_sessions_idle_since(session_cutoff).await {
        Ok(0) => {}
        Ok(n) => tracing::debug!(deleted = n, "retention: purged idle sessions"),
        Err(err) => tracing::warn!(%err, "retention: session purge failed"),
    }
}
