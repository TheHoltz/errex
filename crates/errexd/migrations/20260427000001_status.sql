-- Add a triage status to issues. Existing rows default to 'unresolved'.
-- Values are validated by the application layer (errex_proto::IssueStatus)
-- because SQLite CHECK constraints can't reference an enum across migrations.

ALTER TABLE issues ADD COLUMN status TEXT NOT NULL DEFAULT 'unresolved';

CREATE INDEX IF NOT EXISTS idx_issues_status_last_seen
    ON issues(status, last_seen DESC);
