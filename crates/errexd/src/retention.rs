//! Background retention task: purges event payloads older than the
//! configured horizon. Runs on a 1-hour cadence — frequent enough that a
//! freshly-set retention takes effect within an hour, infrequent enough
//! that we don't churn the DB during normal operation.
//!
//! Issue rows are intentionally NOT purged. A long-tail issue from months
//! ago whose events have aged out is still useful as historical context;
//! only the raw payloads are heavy enough to justify deletion.

use std::time::Duration;

use chrono::Utc;
use tokio::time::{interval, MissedTickBehavior};

use crate::store::Store;

/// Spawn the retention task. `days == 0` is a no-op so the task isn't even
/// scheduled — keeping the daemon completely idle when retention is off.
pub async fn run(store: Store, days: u32) {
    if days == 0 {
        tracing::info!("retention disabled (ERREXD_RETENTION_DAYS=0)");
        // Sleep forever; the supervisor expects this future to never resolve.
        std::future::pending::<()>().await;
        return;
    }

    let mut tick = interval(Duration::from_secs(60 * 60));
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);
    tracing::info!(days, "retention task started");

    loop {
        tick.tick().await;
        let event_cutoff = Utc::now() - chrono::Duration::days(days as i64);
        match store.purge_events_older_than(event_cutoff).await {
            Ok(0) => tracing::debug!("retention: nothing to purge"),
            Ok(n) => {
                tracing::info!(deleted = n, cutoff = %event_cutoff, "retention: purged events")
            }
            Err(err) => tracing::warn!(%err, "retention: purge failed; will retry next tick"),
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
}
