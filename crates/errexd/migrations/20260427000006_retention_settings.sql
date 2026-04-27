-- Retention settings live in a single-row table so the SPA's settings UI
-- can persist limits without a config file edit + restart. The CLI flag
-- (`ERREXD_RETENTION_DAYS`) remains the source of truth for the *event
-- age* horizon at boot; the UI value, if non-zero, takes precedence at
-- runtime so an operator can tighten retention without redeploy.
--
-- All numeric columns: 0 means "unlimited" so a fresh row is a no-op
-- relative to current behavior. The retention task reads this row each
-- tick (cheap) so a UI change takes effect on the next tick.

CREATE TABLE IF NOT EXISTS retention_settings (
    id                       INTEGER PRIMARY KEY CHECK (id = 1),
    events_per_issue_max     INTEGER NOT NULL DEFAULT 0,
    issues_per_project_max   INTEGER NOT NULL DEFAULT 0,
    event_retention_days     INTEGER NOT NULL DEFAULT 0,
    updated_at               TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP)
);

INSERT OR IGNORE INTO retention_settings (id) VALUES (1);
