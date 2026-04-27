-- Per-project webhook for new-issue and regression alerts. Nullable: a
-- project without a URL set is silently skipped at notification time.
-- Format is opaque to the daemon — Slack/Discord/Teams "Incoming Webhook"
-- URLs all accept the same JSON shape (text + attachments).

ALTER TABLE projects ADD COLUMN webhook_url TEXT;
