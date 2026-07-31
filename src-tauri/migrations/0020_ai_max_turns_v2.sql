-- Raise ai_max_turns for existing installs.
-- The 120s SSE chunk timeout fix means long bulk ops no longer die mid-run.
-- Raise default from 25 → 50; only updates installs that haven't been set above 25.
INSERT INTO app_config (key, value, updated_at)
VALUES ('ai_max_turns', '50', datetime('now'))
ON CONFLICT (key) DO UPDATE
    SET value = '50', updated_at = datetime('now')
    WHERE CAST(value AS INTEGER) <= 25;
