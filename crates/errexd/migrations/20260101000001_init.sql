-- Initial schema. Issues are the deduplicated grouping; events are the raw
-- ingest log keyed by issue. payload stores the event JSON verbatim so we
-- can re-render frames as the renderer evolves without a re-ingest.

CREATE TABLE IF NOT EXISTS issues (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    project       TEXT NOT NULL,
    fingerprint   TEXT NOT NULL,
    title         TEXT NOT NULL,
    culprit       TEXT,
    level         TEXT,
    event_count   INTEGER NOT NULL DEFAULT 0,
    first_seen    TEXT NOT NULL,
    last_seen     TEXT NOT NULL,
    UNIQUE(project, fingerprint)
);

CREATE INDEX IF NOT EXISTS idx_issues_project_last_seen
    ON issues(project, last_seen DESC);

CREATE TABLE IF NOT EXISTS events (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    issue_id    INTEGER NOT NULL REFERENCES issues(id) ON DELETE CASCADE,
    event_id    TEXT NOT NULL UNIQUE,
    payload     TEXT NOT NULL,
    received_at TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP)
);

CREATE INDEX IF NOT EXISTS idx_events_issue_received
    ON events(issue_id, received_at DESC);
