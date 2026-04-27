-- Project registry. Used to gate ingest with `ERREXD_REQUIRE_AUTH=true`,
-- and as the admin-side source of truth for project metadata. Issues and
-- events still reference the project by its string name (denormalized) —
-- it would be a heavier migration to add an FK and not buy us much for
-- self-host scale.

CREATE TABLE IF NOT EXISTS projects (
    name         TEXT PRIMARY KEY,
    token        TEXT NOT NULL,
    created_at   TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    last_used_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_projects_token ON projects(token);
