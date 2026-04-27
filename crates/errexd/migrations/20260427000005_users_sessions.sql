-- Multi-user auth: replaces the single shared `ERREXD_ADMIN_TOKEN` bearer.
-- The env var still exists, but only as a one-time setup secret consumed by
-- the onboarding wizard when `users` is empty. After the first user is
-- created, the env-var bypass is permanently disabled at the application
-- layer (no runtime flag — checked by counting rows in `users`).

-- Operators sign in here; their sessions live in `sessions`. Roles are a
-- closed enum validated at the application layer (CHECK keeps it honest).
CREATE TABLE IF NOT EXISTS users (
    username       TEXT PRIMARY KEY,
    password_hash  TEXT NOT NULL,
    role           TEXT NOT NULL CHECK (role IN ('admin','viewer')),
    created_at     TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    last_login_at  TEXT,
    last_login_ip  TEXT,
    -- Soft-delete: a deactivated user can't sign in but is preserved for
    -- audit (their sessions are revoked on deactivation, not on hard delete).
    deactivated_at TEXT
);

-- Server-side session state. Cookie value = `id` (32 bytes random hex).
-- last_seen_at advances on every authenticated request so the sliding
-- expiry window is recoverable after a daemon restart.
CREATE TABLE IF NOT EXISTS sessions (
    id           TEXT PRIMARY KEY,
    username     TEXT NOT NULL REFERENCES users(username) ON DELETE CASCADE,
    created_at   TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    last_seen_at TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    ip           TEXT,
    user_agent   TEXT
);

CREATE INDEX IF NOT EXISTS idx_sessions_username
    ON sessions(username);
CREATE INDEX IF NOT EXISTS idx_sessions_last_seen
    ON sessions(last_seen_at DESC);

-- Brute-force ledger. Each login attempt (success or fail) writes a row;
-- the lockout policy reads the trailing 15 minutes for `(username, ts)`
-- and `(ip, ts)`. Pruned hourly by the retention task — anything older
-- than 24h is dropped because the policy never looks past 15 min anyway.
-- `username` is nullable so we can still record IP-only attempts when the
-- request body was malformed (e.g. spam from a scanner).
CREATE TABLE IF NOT EXISTS auth_attempts (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    username TEXT,
    ip       TEXT NOT NULL,
    ts       TEXT NOT NULL DEFAULT (CURRENT_TIMESTAMP),
    success  INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_auth_attempts_username_ts
    ON auth_attempts(username, ts DESC);
CREATE INDEX IF NOT EXISTS idx_auth_attempts_ip_ts
    ON auth_attempts(ip, ts DESC);
