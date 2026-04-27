-- Per-project webhook delivery health. The webhook task updates these two
-- columns after each POST attempt so the projects console can surface a
-- "last delivery: 200 · 12s ago" badge without storing a full delivery log.
--
-- `last_webhook_status` semantics:
--   NULL      → no delivery has ever been attempted for this project.
--   1xx-5xx   → the HTTP status code from the most recent attempt.
--   0         → transport-level failure (timeout, DNS, TLS) — no HTTP code.
--
-- We deliberately keep one row per project (not a history table) to honour
-- errex's lightweight constraint: webhook health is a "is it working
-- right now" signal, not an audit log.

ALTER TABLE projects ADD COLUMN last_webhook_status INTEGER;
ALTER TABLE projects ADD COLUMN last_webhook_at TEXT;
