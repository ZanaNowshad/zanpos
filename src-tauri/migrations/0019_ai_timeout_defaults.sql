-- Raise AI streaming timeouts for existing installs.
-- Old defaults (120s stream, 60s chat) cut off long bulk operations.
-- New defaults: 1800s (30 min) — no constraint on realistic workloads.
INSERT INTO app_config (key, value, updated_at)
VALUES ('ai_stream_timeout_secs', '1800', datetime('now'))
ON CONFLICT (key) DO UPDATE
    SET value = '1800', updated_at = datetime('now')
    WHERE CAST(value AS INTEGER) <= 120;

INSERT INTO app_config (key, value, updated_at)
VALUES ('ai_chat_timeout_secs', '1800', datetime('now'))
ON CONFLICT (key) DO UPDATE
    SET value = '1800', updated_at = datetime('now')
    WHERE CAST(value AS INTEGER) <= 60;
