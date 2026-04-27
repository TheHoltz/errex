use std::path::Path;

use chrono::{DateTime, Utc};
use errex_proto::{Event, Fingerprint, Issue, IssueStatus};
use serde::{Deserialize, Serialize};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;

use crate::error::DaemonError;

/// JSON shape returned by `/api/projects`. Lives next to the query that
/// produces it so adding a column is a one-file change.
#[derive(Debug, Clone, Serialize)]
pub struct ProjectSummary {
    pub project: String,
    pub issue_count: usize,
}

/// Admin-side project record: name + ingest token + optional webhook URL.
/// Used by the CLI (`errexd project list/add/rotate/set-webhook`) and to
/// validate `sentry_key` on ingest when `ERREXD_REQUIRE_AUTH=true`.
#[derive(Debug, Clone, Serialize)]
pub struct Project {
    pub name: String,
    pub token: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub webhook_url: Option<String>,
    /// HTTP status from the most recent webhook delivery, or 0 for transport
    /// failure. `None` means no delivery has been attempted yet.
    pub last_webhook_status: Option<i16>,
    pub last_webhook_at: Option<DateTime<Utc>>,
}

/// Counts returned by `delete_project` and `delete_preview`. Both cascade the
/// same way; preview just doesn't commit. Returned to the SPA so the
/// confirmation modal can show "permanently deletes N events and M issues"
/// before the user types the project name.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct DeleteSummary {
    pub events_deleted: i64,
    pub issues_deleted: i64,
}

/// 24-hour activity rollup for a single project. Computed on demand from
/// `events` joined to `issues`; no aggregate table.
#[derive(Debug, Clone, Serialize)]
pub struct ActivityStats {
    pub events_24h: i64,
    pub unique_issues_24h: i64,
    pub last_event_at: Option<DateTime<Utc>>,
    /// 24 hourly counts, oldest → newest. Index 23 is the bucket containing
    /// `now`; index 0 is the bucket 23 hours earlier. Sparkline-ready.
    pub hourly_buckets: Vec<i64>,
}

/// Closed enum, validated by a CHECK in the migration AND on the way in via
/// `from_db_str` (defaults to the SAFER value on garbage — viewer, not
/// admin — so a corrupt row never escalates privileges).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    Viewer,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::Viewer => "viewer",
        }
    }
    pub fn from_db_str(s: &str) -> Self {
        match s {
            "admin" => Role::Admin,
            // Defensively default to the LEAST-privileged role on unexpected
            // input. A column corruption must never silently escalate.
            _ => Role::Viewer,
        }
    }
    pub fn is_admin(self) -> bool {
        matches!(self, Role::Admin)
    }
}

/// Public user view. Excludes `password_hash` by construction — that field
/// only ever leaves the store via `get_user_password_hash`, which is called
/// exactly once per login attempt.
#[derive(Debug, Clone, Serialize)]
pub struct User {
    pub username: String,
    pub role: Role,
    pub created_at: DateTime<Utc>,
    pub last_login_at: Option<DateTime<Utc>>,
    pub last_login_ip: Option<String>,
    pub deactivated_at: Option<DateTime<Utc>>,
}

/// Server-side session row. The `id` is the cookie value; in admin views
/// returned by `list_user_sessions` we still surface it because the team
/// page needs to be able to revoke individual sessions by id.
#[derive(Debug, Clone, Serialize)]
pub struct Session {
    pub id: String,
    pub username: String,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
}

/// Thin handle around a sqlx pool with WAL configured.
///
/// All writes go through the digest task (single-writer invariant). Reads
/// can come from anywhere — `/api/issues`, `/api/issues/:id/event`, the
/// startup hydrator.
#[derive(Debug, Clone)]
pub struct Store {
    pool: SqlitePool,
}

/// Result of an upsert: the full `Issue` row as it stands after the write.
/// `created` is true when this was the first event with that fingerprint —
/// callers use it to choose `IssueCreated` vs `IssueUpdated` on the wire.
/// `regressed` is true when the upsert flipped a `resolved` issue back to
/// `unresolved` (a fresh event for something we thought was fixed).
#[derive(Debug, Clone)]
pub struct UpsertResult {
    pub issue: Issue,
    pub created: bool,
    pub regressed: bool,
}

/// A persisted event with its raw payload kept verbatim. The renderer can
/// re-parse this as the proto evolves without a re-ingest. Serialized
/// directly to JSON by the `/api/issues/:id/event` endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct StoredEvent {
    pub event_id: String,
    pub received_at: DateTime<Utc>,
    pub payload: serde_json::Value,
}

impl Store {
    pub async fn open(db_path: &Path) -> Result<Self, DaemonError> {
        // WAL gives us non-blocking readers alongside the single writer task.
        // `synchronous=NORMAL` is the standard pairing with WAL: durable across
        // app crashes, only loses writes on a host power loss.
        let opts = SqliteConnectOptions::new()
            .filename(db_path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true);

        // 4 connections is plenty: 1 writer (digest task) + a few concurrent
        // readers from /api routes and WS snapshot loads. Self-host pequeno
        // does not need 8.
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(opts)
            .await?;

        Ok(Self { pool })
    }

    /// Read-side handle for tests and future query helpers.
    #[allow(dead_code)]
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn migrate(&self) -> Result<(), DaemonError> {
        sqlx::migrate!("./migrations").run(&self.pool).await?;
        Ok(())
    }

    /// Insert-or-update an issue keyed by `(project, fingerprint)`. When the
    /// row already exists the event_count is incremented and `last_seen` is
    /// bumped; the title/culprit/level fields are NOT overwritten because the
    /// first sighting is the canonical one (subsequent events refining the
    /// title would be confusing without a UI affordance).
    pub async fn upsert_issue(
        &self,
        project: &str,
        fp: &Fingerprint,
        title: &str,
        culprit: Option<&str>,
        level: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<UpsertResult, DaemonError> {
        let mut tx = self.pool.begin().await?;
        let now_str = now.to_rfc3339();
        let fp_str = fp.as_str();

        // Step 1: try INSERT. If the unique key already matches, the conflict
        // clause becomes a no-op so we can detect it via row_count below.
        let inserted = sqlx::query(
            "INSERT INTO issues (project, fingerprint, title, culprit, level, status, \
             event_count, first_seen, last_seen) \
             VALUES (?, ?, ?, ?, ?, 'unresolved', 1, ?, ?) \
             ON CONFLICT(project, fingerprint) DO NOTHING",
        )
        .bind(project)
        .bind(fp_str)
        .bind(title)
        .bind(culprit)
        .bind(level)
        .bind(&now_str)
        .bind(&now_str)
        .execute(&mut *tx)
        .await?
        .rows_affected();

        let created = inserted == 1;
        let mut regressed = false;
        if !created {
            // Existing row: read current status so we can detect regression
            // (resolved → new event → flip to unresolved). Muted/Ignored stay
            // put — the user explicitly silenced the noise.
            let prev_status: String = sqlx::query_scalar(
                "SELECT status FROM issues WHERE project = ? AND fingerprint = ?",
            )
            .bind(project)
            .bind(fp_str)
            .fetch_one(&mut *tx)
            .await?;

            if IssueStatus::from_db_str(&prev_status) == IssueStatus::Resolved {
                regressed = true;
                sqlx::query(
                    "UPDATE issues \
                     SET event_count = event_count + 1, last_seen = ?, status = 'unresolved' \
                     WHERE project = ? AND fingerprint = ?",
                )
                .bind(&now_str)
                .bind(project)
                .bind(fp_str)
                .execute(&mut *tx)
                .await?;
            } else {
                sqlx::query(
                    "UPDATE issues SET event_count = event_count + 1, last_seen = ? \
                     WHERE project = ? AND fingerprint = ?",
                )
                .bind(&now_str)
                .bind(project)
                .bind(fp_str)
                .execute(&mut *tx)
                .await?;
            }
        }

        // Read back the post-write row so callers get DB-assigned id and the
        // updated event_count without a second roundtrip.
        let row: IssueRow = sqlx::query_as(
            "SELECT id, project, fingerprint, title, culprit, level, status, \
             event_count, first_seen, last_seen \
             FROM issues WHERE project = ? AND fingerprint = ?",
        )
        .bind(project)
        .bind(fp_str)
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(UpsertResult {
            issue: row.into(),
            created,
            regressed,
        })
    }

    /// Set the triage status for an issue, returning the updated row.
    /// Errors when the issue does not exist (callers should validate the
    /// id from the URL before calling).
    pub async fn set_status(
        &self,
        issue_id: i64,
        status: IssueStatus,
    ) -> Result<Issue, DaemonError> {
        let mut tx = self.pool.begin().await?;
        let updated = sqlx::query("UPDATE issues SET status = ? WHERE id = ?")
            .bind(status.as_db_str())
            .bind(issue_id)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        if updated == 0 {
            return Err(DaemonError::NotFound(format!("issue {issue_id}")));
        }
        let row: IssueRow = sqlx::query_as(
            "SELECT id, project, fingerprint, title, culprit, level, status, \
             event_count, first_seen, last_seen FROM issues WHERE id = ?",
        )
        .bind(issue_id)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(row.into())
    }

    /// Append the raw event payload for an issue. `event_id` is the SDK-side
    /// UUID; UNIQUE constraint silently swallows duplicate POSTs (Sentry SDKs
    /// retry on transient failures and we don't want double-counting).
    pub async fn insert_event(&self, issue_id: i64, event: &Event) -> Result<(), DaemonError> {
        let payload = serde_json::to_string(event)?;
        sqlx::query(
            "INSERT INTO events (issue_id, event_id, payload) VALUES (?, ?, ?) \
             ON CONFLICT(event_id) DO NOTHING",
        )
        .bind(issue_id)
        .bind(event.event_id.to_string())
        .bind(payload)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Load every issue, newest-first by `last_seen`. Used by the WS snapshot
    /// and the `/api/issues` endpoint when no project filter is applied.
    pub async fn load_issues(&self) -> Result<Vec<Issue>, DaemonError> {
        let rows: Vec<IssueRow> = sqlx::query_as(
            "SELECT id, project, fingerprint, title, culprit, level, status, \
             event_count, first_seen, last_seen \
             FROM issues ORDER BY last_seen DESC",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// Issues for a single project, newest-first.
    pub async fn list_issues_by_project(&self, project: &str) -> Result<Vec<Issue>, DaemonError> {
        let rows: Vec<IssueRow> = sqlx::query_as(
            "SELECT id, project, fingerprint, title, culprit, level, status, \
             event_count, first_seen, last_seen \
             FROM issues WHERE project = ? ORDER BY last_seen DESC",
        )
        .bind(project)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// Distinct project names with issue counts. Done in SQL with GROUP BY
    /// rather than loading every row into RAM.
    pub async fn project_summaries(&self) -> Result<Vec<ProjectSummary>, DaemonError> {
        #[derive(sqlx::FromRow)]
        struct Row {
            project: String,
            issue_count: i64,
        }
        let rows: Vec<Row> = sqlx::query_as(
            "SELECT project, COUNT(*) AS issue_count FROM issues \
             GROUP BY project ORDER BY project",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| ProjectSummary {
                project: r.project,
                issue_count: r.issue_count.max(0) as usize,
            })
            .collect())
    }

    /// Create a new project record with a freshly-generated ingest token.
    /// Errors if the project name already exists. The token is derived from
    /// `Uuid::new_v4()` (122 bits of entropy) — sufficient for self-host
    /// without dragging in `rand` or `getrandom` crates directly.
    pub async fn create_project(&self, name: &str) -> Result<Project, DaemonError> {
        let token = generate_token();
        let now = Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO projects (name, token, created_at) VALUES (?, ?, ?)")
            .bind(name)
            .bind(&token)
            .bind(&now)
            .execute(&self.pool)
            .await?;
        Ok(Project {
            name: name.to_string(),
            token,
            created_at: parse_or_now(&now),
            last_used_at: None,
            webhook_url: None,
            last_webhook_status: None,
            last_webhook_at: None,
        })
    }

    /// Set or clear the webhook URL for an existing project.
    pub async fn set_project_webhook(
        &self,
        name: &str,
        url: Option<&str>,
    ) -> Result<(), DaemonError> {
        let updated = sqlx::query("UPDATE projects SET webhook_url = ? WHERE name = ?")
            .bind(url)
            .bind(name)
            .execute(&self.pool)
            .await?
            .rows_affected();
        if updated == 0 {
            return Err(DaemonError::NotFound(format!("project {name}")));
        }
        Ok(())
    }

    pub async fn project_by_name(&self, name: &str) -> Result<Option<Project>, DaemonError> {
        let row: Option<ProjectRow> = sqlx::query_as(
            "SELECT name, token, created_at, last_used_at, webhook_url, last_webhook_status, last_webhook_at \
             FROM projects WHERE name = ?",
        )
        .bind(name)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    pub async fn project_by_token(&self, token: &str) -> Result<Option<Project>, DaemonError> {
        let row: Option<ProjectRow> = sqlx::query_as(
            "SELECT name, token, created_at, last_used_at, webhook_url, last_webhook_status, last_webhook_at \
             FROM projects WHERE token = ?",
        )
        .bind(token)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    pub async fn list_admin_projects(&self) -> Result<Vec<Project>, DaemonError> {
        let rows: Vec<ProjectRow> = sqlx::query_as(
            "SELECT name, token, created_at, last_used_at, webhook_url, last_webhook_status, last_webhook_at \
             FROM projects ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn rotate_token(&self, name: &str) -> Result<Project, DaemonError> {
        let token = generate_token();
        let updated = sqlx::query("UPDATE projects SET token = ? WHERE name = ?")
            .bind(&token)
            .bind(name)
            .execute(&self.pool)
            .await?
            .rows_affected();
        if updated == 0 {
            return Err(DaemonError::NotFound(format!("project {name}")));
        }
        // We could RETURNING but sqlx-sqlite makes that fiddly; one extra
        // round-trip is fine for an admin op.
        self.project_by_name(name)
            .await?
            .ok_or_else(|| DaemonError::NotFound(format!("project {name} (post-rotate)")))
    }

    /// Bumps `last_used_at` for telemetry. Best-effort — a missing row or
    /// transient SQLite contention is logged and swallowed.
    /// Used by ingest auth path; tests/store.rs doesn't exercise it
    /// directly (covered via the api.rs auth tests instead).
    #[allow(dead_code)]
    pub async fn touch_project_used(&self, name: &str) {
        if let Err(err) = sqlx::query("UPDATE projects SET last_used_at = ? WHERE name = ?")
            .bind(Utc::now().to_rfc3339())
            .bind(name)
            .execute(&self.pool)
            .await
        {
            tracing::debug!(%err, %name, "touch_project_used failed");
        }
    }

    /// Delete events whose `received_at` predates `cutoff`. Issue rows are
    /// preserved on purpose — long-tail issues whose payloads have aged out
    /// are still useful as historical context (count, first/last seen).
    /// Returns the number of rows deleted.
    pub async fn purge_events_older_than(&self, cutoff: DateTime<Utc>) -> Result<u64, DaemonError> {
        let res = sqlx::query("DELETE FROM events WHERE received_at < ?")
            .bind(cutoff.to_rfc3339())
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected())
    }

    /// Most recent event payload for an issue. Drives the StackTrace /
    /// Breadcrumbs / Tags panes; returns None for issues seen before
    /// persistence was enabled (or whose events have been purged by retention).
    pub async fn latest_event(&self, issue_id: i64) -> Result<Option<StoredEvent>, DaemonError> {
        let row: Option<EventRow> = sqlx::query_as(
            "SELECT event_id, payload, received_at FROM events \
             WHERE issue_id = ? ORDER BY received_at DESC, id DESC LIMIT 1",
        )
        .bind(issue_id)
        .fetch_optional(&self.pool)
        .await?;
        row.map(StoredEvent::try_from).transpose()
    }

    /// Count what `delete_project` would destroy, without committing. The
    /// SPA calls this to populate the type-to-confirm modal copy.
    pub async fn delete_preview(&self, name: &str) -> Result<DeleteSummary, DaemonError> {
        if self.project_by_name(name).await?.is_none() {
            return Err(DaemonError::NotFound(format!("project {name}")));
        }
        let (issues_deleted, events_deleted) = self.count_project_rows(name).await?;
        Ok(DeleteSummary {
            events_deleted,
            issues_deleted,
        })
    }

    /// Hard-delete a project and everything it owns: its issues, the events
    /// belonging to those issues (via FK cascade), and the project row
    /// itself. All in one transaction so a partial failure leaves no
    /// dangling rows. Returns counts for the SPA's "deleted N events and M
    /// issues" toast.
    pub async fn delete_project(&self, name: &str) -> Result<DeleteSummary, DaemonError> {
        if self.project_by_name(name).await?.is_none() {
            return Err(DaemonError::NotFound(format!("project {name}")));
        }
        // Snapshot counts before the delete so the return value is accurate
        // (rows_affected on a CASCADE doesn't include the cascaded children).
        let (issues_deleted, events_deleted) = self.count_project_rows(name).await?;

        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM issues WHERE project = ?")
            .bind(name)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM projects WHERE name = ?")
            .bind(name)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;

        Ok(DeleteSummary {
            events_deleted,
            issues_deleted,
        })
    }

    /// Issue + event counts for a project. Used by both delete paths so the
    /// preview and the actual delete return consistent numbers.
    async fn count_project_rows(&self, name: &str) -> Result<(i64, i64), DaemonError> {
        let issues_deleted: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM issues WHERE project = ?")
                .bind(name)
                .fetch_one(&self.pool)
                .await?;
        let events_deleted: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM events e \
             JOIN issues i ON e.issue_id = i.id \
             WHERE i.project = ?",
        )
        .bind(name)
        .fetch_one(&self.pool)
        .await?;
        Ok((issues_deleted.0, events_deleted.0))
    }

    /// Rename a project: updates the `projects` row AND fans out to the
    /// denormalized `issues.project` column. Single transaction. Same-name
    /// rename is a successful no-op (so the SPA doesn't have to special-case
    /// "user pressed save without changes").
    pub async fn rename_project(&self, old: &str, new: &str) -> Result<Project, DaemonError> {
        if old == new {
            return self
                .project_by_name(old)
                .await?
                .ok_or_else(|| DaemonError::NotFound(format!("project {old}")));
        }
        if self.project_by_name(old).await?.is_none() {
            return Err(DaemonError::NotFound(format!("project {old}")));
        }
        let mut tx = self.pool.begin().await?;
        // The PRIMARY KEY on projects.name will trip a unique violation if
        // `new` already exists — we surface that via the SQLx error path so
        // the API can map it to 409.
        sqlx::query("UPDATE projects SET name = ? WHERE name = ?")
            .bind(new)
            .bind(old)
            .execute(&mut *tx)
            .await?;
        sqlx::query("UPDATE issues SET project = ? WHERE project = ?")
            .bind(new)
            .bind(old)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        self.project_by_name(new)
            .await?
            .ok_or_else(|| DaemonError::NotFound(format!("project {new} (post-rename)")))
    }

    /// Best-effort write of the most recent webhook delivery outcome. Called
    /// by the webhook task after each POST. A missing row (project deleted
    /// while a webhook was in flight) or transient SQLite contention is
    /// logged and swallowed — webhook health is a UX nicety, not a system
    /// invariant.
    pub async fn record_webhook_attempt(&self, name: &str, status: u16) {
        let res = sqlx::query(
            "UPDATE projects SET last_webhook_status = ?, last_webhook_at = ? WHERE name = ?",
        )
        .bind(status as i64)
        .bind(Utc::now().to_rfc3339())
        .bind(name)
        .execute(&self.pool)
        .await;
        if let Err(err) = res {
            tracing::debug!(%err, %name, "record_webhook_attempt failed");
        }
    }

    /// 24-hour activity rollup for a project. One round-trip per metric;
    /// indexed lookups (`idx_events_issue_received` for the bucket scan,
    /// `idx_issues_project_last_seen` for the issue join) keep this in the
    /// single-digit-ms range on a SQLite of any reasonable size.
    pub async fn activity_stats(
        &self,
        project: &str,
        now: DateTime<Utc>,
    ) -> Result<ActivityStats, DaemonError> {
        let cutoff = now - chrono::Duration::hours(24);
        let cutoff_str = cutoff.to_rfc3339();

        // Total events in the window.
        let events_24h: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM events e \
             JOIN issues i ON e.issue_id = i.id \
             WHERE i.project = ? AND e.received_at >= ?",
        )
        .bind(project)
        .bind(&cutoff_str)
        .fetch_one(&self.pool)
        .await?;

        // Distinct issues touched in the window.
        let unique_issues_24h: (i64,) = sqlx::query_as(
            "SELECT COUNT(DISTINCT e.issue_id) FROM events e \
             JOIN issues i ON e.issue_id = i.id \
             WHERE i.project = ? AND e.received_at >= ?",
        )
        .bind(project)
        .bind(&cutoff_str)
        .fetch_one(&self.pool)
        .await?;

        // Most recent event timestamp (ever, not just within the window). The
        // sub-line under the project name uses this so a "you haven't sent
        // anything in 3 days" project still shows its real last-seen time.
        let last_event_at: Option<(String,)> = sqlx::query_as(
            "SELECT MAX(e.received_at) FROM events e \
             JOIN issues i ON e.issue_id = i.id \
             WHERE i.project = ? AND e.received_at IS NOT NULL",
        )
        .bind(project)
        .fetch_optional(&self.pool)
        .await?;
        let last_event_at = last_event_at
            .and_then(|(s,)| Some(s).filter(|s| !s.is_empty()))
            .map(|s| parse_or_now(&s));

        // Per-hour buckets. We pull every event timestamp in the window and
        // bucket in Rust — simpler and more portable than juggling SQLite's
        // `strftime` modulo arithmetic, and the row count is bounded by a
        // single project's 24h traffic (small for self-host).
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT e.received_at FROM events e \
             JOIN issues i ON e.issue_id = i.id \
             WHERE i.project = ? AND e.received_at >= ?",
        )
        .bind(project)
        .bind(&cutoff_str)
        .fetch_all(&self.pool)
        .await?;

        let mut hourly_buckets = vec![0i64; 24];
        for (ts,) in rows {
            let dt = match DateTime::parse_from_rfc3339(&ts) {
                Ok(dt) => dt.with_timezone(&Utc),
                Err(_) => continue,
            };
            // Bucket index: oldest = 0, newest = 23. `now` lands in 23.
            let hours_ago = (now - dt).num_hours();
            if !(0..24).contains(&hours_ago) {
                continue;
            }
            let idx = (23 - hours_ago) as usize;
            hourly_buckets[idx] += 1;
        }

        Ok(ActivityStats {
            events_24h: events_24h.0,
            unique_issues_24h: unique_issues_24h.0,
            last_event_at,
            hourly_buckets,
        })
    }

    // ----- users -----

    /// Total user rows. The setup wizard reads this to gate the env-token
    /// bootstrap path: empty table = bootstrap allowed, anything else = no.
    pub async fn user_count(&self) -> Result<i64, DaemonError> {
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
            .fetch_one(&self.pool)
            .await?;
        Ok(row.0)
    }

    /// Insert a new user. Caller is responsible for hashing the password
    /// (this layer is crypto-agnostic so callers can pre-validate
    /// strength). Errors with NOT NULL violation if username exists.
    pub async fn create_user(
        &self,
        username: &str,
        password_hash: &str,
        role: Role,
    ) -> Result<User, DaemonError> {
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO users (username, password_hash, role, created_at) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(username)
        .bind(password_hash)
        .bind(role.as_str())
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(User {
            username: username.to_string(),
            role,
            created_at: parse_or_now(&now),
            last_login_at: None,
            last_login_ip: None,
            deactivated_at: None,
        })
    }

    pub async fn list_users(&self) -> Result<Vec<User>, DaemonError> {
        let rows: Vec<UserRow> = sqlx::query_as(
            "SELECT username, role, created_at, last_login_at, last_login_ip, deactivated_at \
             FROM users ORDER BY username",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn get_user(&self, username: &str) -> Result<Option<User>, DaemonError> {
        let row: Option<UserRow> = sqlx::query_as(
            "SELECT username, role, created_at, last_login_at, last_login_ip, deactivated_at \
             FROM users WHERE username = ?",
        )
        .bind(username)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(Into::into))
    }

    /// Hash retrieval is its own method so it never accidentally goes out
    /// over the wire as part of the public `User` shape.
    pub async fn get_user_password_hash(
        &self,
        username: &str,
    ) -> Result<Option<String>, DaemonError> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT password_hash FROM users WHERE username = ?")
                .bind(username)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(|(h,)| h))
    }

    pub async fn set_user_password(
        &self,
        username: &str,
        password_hash: &str,
    ) -> Result<(), DaemonError> {
        let updated = sqlx::query("UPDATE users SET password_hash = ? WHERE username = ?")
            .bind(password_hash)
            .bind(username)
            .execute(&self.pool)
            .await?
            .rows_affected();
        if updated == 0 {
            return Err(DaemonError::NotFound(format!("user {username}")));
        }
        Ok(())
    }

    pub async fn set_user_role(&self, username: &str, role: Role) -> Result<(), DaemonError> {
        let updated = sqlx::query("UPDATE users SET role = ? WHERE username = ?")
            .bind(role.as_str())
            .bind(username)
            .execute(&self.pool)
            .await?
            .rows_affected();
        if updated == 0 {
            return Err(DaemonError::NotFound(format!("user {username}")));
        }
        Ok(())
    }

    /// Deactivation is reversible (`set_user_deactivated(name, false)` to
    /// re-enable) and on flip-to-true revokes every session — otherwise a
    /// freshly-deactivated user would keep their cookie until expiry.
    pub async fn set_user_deactivated(
        &self,
        username: &str,
        deactivated: bool,
    ) -> Result<(), DaemonError> {
        let mut tx = self.pool.begin().await?;
        let value = if deactivated {
            Some(Utc::now().to_rfc3339())
        } else {
            None
        };
        let updated = sqlx::query("UPDATE users SET deactivated_at = ? WHERE username = ?")
            .bind(value.as_deref())
            .bind(username)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        if updated == 0 {
            return Err(DaemonError::NotFound(format!("user {username}")));
        }
        if deactivated {
            sqlx::query("DELETE FROM sessions WHERE username = ?")
                .bind(username)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Hard-delete. The `sessions` FK has ON DELETE CASCADE so sessions go
    /// with the user. `auth_attempts` does NOT cascade — historical brute
    /// force evidence stays useful even after the target user is gone.
    pub async fn delete_user(&self, username: &str) -> Result<(), DaemonError> {
        let updated = sqlx::query("DELETE FROM users WHERE username = ?")
            .bind(username)
            .execute(&self.pool)
            .await?
            .rows_affected();
        if updated == 0 {
            return Err(DaemonError::NotFound(format!("user {username}")));
        }
        Ok(())
    }

    /// Records a successful login on the user row. Best-effort — we don't
    /// fail a login because telemetry didn't write.
    pub async fn touch_user_login(&self, username: &str, ip: Option<&str>) {
        let res =
            sqlx::query("UPDATE users SET last_login_at = ?, last_login_ip = ? WHERE username = ?")
                .bind(Utc::now().to_rfc3339())
                .bind(ip)
                .bind(username)
                .execute(&self.pool)
                .await;
        if let Err(err) = res {
            tracing::debug!(%err, %username, "touch_user_login failed");
        }
    }

    /// Counts admins NOT counting deactivated rows. Used as the lockout
    /// guard: refuse to demote/delete/deactivate the last admin so an
    /// errant click can't lock everyone out of the daemon.
    pub async fn count_active_admins(&self) -> Result<i64, DaemonError> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM users WHERE role = 'admin' AND deactivated_at IS NULL",
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    // ----- sessions -----

    pub async fn create_session(
        &self,
        id: &str,
        username: &str,
        ip: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<Session, DaemonError> {
        let now = Utc::now();
        let now_str = now.to_rfc3339();
        sqlx::query(
            "INSERT INTO sessions (id, username, created_at, last_seen_at, ip, user_agent) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(id)
        .bind(username)
        .bind(&now_str)
        .bind(&now_str)
        .bind(ip)
        .bind(user_agent)
        .execute(&self.pool)
        .await?;
        Ok(Session {
            id: id.to_string(),
            username: username.to_string(),
            created_at: now,
            last_seen_at: now,
            ip: ip.map(str::to_string),
            user_agent: user_agent.map(str::to_string),
        })
    }

    /// Returns `(session, role, deactivated)` so the auth middleware can
    /// reject deactivated-but-still-cookied users in one round-trip.
    pub async fn session_for_id(
        &self,
        id: &str,
    ) -> Result<Option<(Session, Role, bool)>, DaemonError> {
        let row: Option<SessionWithUserRow> = sqlx::query_as(
            "SELECT s.id, s.username, s.created_at, s.last_seen_at, s.ip, s.user_agent, \
                    u.role, u.deactivated_at \
             FROM sessions s JOIN users u ON u.username = s.username \
             WHERE s.id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| {
            let role = Role::from_db_str(&r.role);
            let deactivated = r.deactivated_at.is_some();
            (
                Session {
                    id: r.id,
                    username: r.username,
                    created_at: parse_or_now(&r.created_at),
                    last_seen_at: parse_or_now(&r.last_seen_at),
                    ip: r.ip,
                    user_agent: r.user_agent,
                },
                role,
                deactivated,
            )
        }))
    }

    pub async fn touch_session(&self, id: &str) {
        let res = sqlx::query("UPDATE sessions SET last_seen_at = ? WHERE id = ?")
            .bind(Utc::now().to_rfc3339())
            .bind(id)
            .execute(&self.pool)
            .await;
        if let Err(err) = res {
            tracing::debug!(%err, "touch_session failed");
        }
    }

    pub async fn revoke_session(&self, id: &str) -> Result<(), DaemonError> {
        sqlx::query("DELETE FROM sessions WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Revokes every session for a user. Returns the number revoked so the
    /// admin UI can show "5 sessions ended" instead of guessing.
    pub async fn revoke_user_sessions(&self, username: &str) -> Result<u64, DaemonError> {
        let res = sqlx::query("DELETE FROM sessions WHERE username = ?")
            .bind(username)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected())
    }

    pub async fn list_user_sessions(&self, username: &str) -> Result<Vec<Session>, DaemonError> {
        let rows: Vec<SessionRow> = sqlx::query_as(
            "SELECT id, username, created_at, last_seen_at, ip, user_agent \
             FROM sessions WHERE username = ? ORDER BY last_seen_at DESC",
        )
        .bind(username)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| Session {
                id: r.id,
                username: r.username,
                created_at: parse_or_now(&r.created_at),
                last_seen_at: parse_or_now(&r.last_seen_at),
                ip: r.ip,
                user_agent: r.user_agent,
            })
            .collect())
    }

    /// Sweeps sessions whose `last_seen_at` predates `cutoff`. Called by
    /// the retention task on the same schedule as event purges. Returns the
    /// number of sessions removed.
    pub async fn purge_sessions_idle_since(
        &self,
        cutoff: DateTime<Utc>,
    ) -> Result<u64, DaemonError> {
        let res = sqlx::query("DELETE FROM sessions WHERE last_seen_at < ?")
            .bind(cutoff.to_rfc3339())
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected())
    }

    // ----- auth attempts -----

    pub async fn record_attempt(&self, username: Option<&str>, ip: &str, success: bool) {
        let res = sqlx::query(
            "INSERT INTO auth_attempts (username, ip, ts, success) VALUES (?, ?, ?, ?)",
        )
        .bind(username)
        .bind(ip)
        .bind(Utc::now().to_rfc3339())
        .bind(success as i64)
        .execute(&self.pool)
        .await;
        if let Err(err) = res {
            tracing::debug!(%err, "record_attempt failed");
        }
    }

    /// Number of FAILED attempts against this username since `since`.
    pub async fn count_recent_failures_for_username(
        &self,
        username: &str,
        since: DateTime<Utc>,
    ) -> Result<i64, DaemonError> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM auth_attempts \
             WHERE username = ? AND success = 0 AND ts >= ?",
        )
        .bind(username)
        .bind(since.to_rfc3339())
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    /// Number of FAILED attempts from this IP since `since`. Counts across
    /// all usernames so a spray attack can't dilute its rate.
    pub async fn count_recent_failures_for_ip(
        &self,
        ip: &str,
        since: DateTime<Utc>,
    ) -> Result<i64, DaemonError> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM auth_attempts \
             WHERE ip = ? AND success = 0 AND ts >= ?",
        )
        .bind(ip)
        .bind(since.to_rfc3339())
        .fetch_one(&self.pool)
        .await?;
        Ok(row.0)
    }

    pub async fn prune_auth_attempts_older_than(
        &self,
        cutoff: DateTime<Utc>,
    ) -> Result<u64, DaemonError> {
        let res = sqlx::query("DELETE FROM auth_attempts WHERE ts < ?")
            .bind(cutoff.to_rfc3339())
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected())
    }
}

fn generate_token() -> String {
    // `Uuid::new_v4().simple()` → 32 lowercase hex chars, 122 bits of
    // entropy. We don't want hyphens (some operators paste this into env
    // files where hyphens trigger word-wrap), so use the simple form.
    uuid::Uuid::new_v4().simple().to_string()
}

fn parse_or_now(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

// ----- Row types isolate sqlx FromRow from the public proto types. -----

#[derive(Debug, sqlx::FromRow)]
struct IssueRow {
    id: i64,
    project: String,
    fingerprint: String,
    title: String,
    culprit: Option<String>,
    level: Option<String>,
    status: String,
    event_count: i64,
    first_seen: String,
    last_seen: String,
}

impl From<IssueRow> for Issue {
    fn from(r: IssueRow) -> Self {
        // Timestamps were written as RFC3339; on read we parse leniently and
        // fall back to "now" rather than crash if the column is corrupt.
        let parse = |s: &str| {
            DateTime::parse_from_rfc3339(s)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now())
        };
        Issue {
            id: r.id,
            project: r.project,
            fingerprint: Fingerprint::new(r.fingerprint),
            title: r.title,
            culprit: r.culprit,
            level: r.level,
            status: IssueStatus::from_db_str(&r.status),
            event_count: r.event_count,
            first_seen: parse(&r.first_seen),
            last_seen: parse(&r.last_seen),
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct ProjectRow {
    name: String,
    token: String,
    created_at: String,
    last_used_at: Option<String>,
    webhook_url: Option<String>,
    last_webhook_status: Option<i64>,
    last_webhook_at: Option<String>,
}

impl From<ProjectRow> for Project {
    fn from(r: ProjectRow) -> Self {
        Project {
            name: r.name,
            token: r.token,
            created_at: parse_or_now(&r.created_at),
            last_used_at: r.last_used_at.as_deref().map(parse_or_now),
            webhook_url: r.webhook_url,
            // SQLite returns INTEGER as i64; status fits comfortably in i16
            // (0..599) so we narrow with saturation.
            last_webhook_status: r
                .last_webhook_status
                .map(|n| n.clamp(0, i16::MAX as i64) as i16),
            last_webhook_at: r.last_webhook_at.as_deref().map(parse_or_now),
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct EventRow {
    event_id: String,
    payload: String,
    received_at: String,
}

#[derive(Debug, sqlx::FromRow)]
struct UserRow {
    username: String,
    role: String,
    created_at: String,
    last_login_at: Option<String>,
    last_login_ip: Option<String>,
    deactivated_at: Option<String>,
}

impl From<UserRow> for User {
    fn from(r: UserRow) -> Self {
        User {
            username: r.username,
            role: Role::from_db_str(&r.role),
            created_at: parse_or_now(&r.created_at),
            last_login_at: r.last_login_at.as_deref().map(parse_or_now),
            last_login_ip: r.last_login_ip,
            deactivated_at: r.deactivated_at.as_deref().map(parse_or_now),
        }
    }
}

#[derive(Debug, sqlx::FromRow)]
struct SessionRow {
    id: String,
    username: String,
    created_at: String,
    last_seen_at: String,
    ip: Option<String>,
    user_agent: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct SessionWithUserRow {
    id: String,
    username: String,
    created_at: String,
    last_seen_at: String,
    ip: Option<String>,
    user_agent: Option<String>,
    role: String,
    deactivated_at: Option<String>,
}

impl TryFrom<EventRow> for StoredEvent {
    type Error = DaemonError;
    fn try_from(r: EventRow) -> Result<Self, DaemonError> {
        Ok(StoredEvent {
            event_id: r.event_id,
            payload: serde_json::from_str(&r.payload)?,
            received_at: DateTime::parse_from_rfc3339(&r.received_at)
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
        })
    }
}
