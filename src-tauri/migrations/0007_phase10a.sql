-- Phase 10a: PIN lockout, session timeout config

-- ── PIN lockout fields on users ───────────────────────────────────────────────
ALTER TABLE users ADD COLUMN failed_pin_attempts INTEGER NOT NULL DEFAULT 0;
ALTER TABLE users ADD COLUMN locked_until TEXT;  -- ISO8601 datetime or NULL

-- ── Idle session timeout (minutes) ───────────────────────────────────────────
INSERT OR IGNORE INTO app_config(key, value) VALUES ('idle_timeout_minutes', '5');
