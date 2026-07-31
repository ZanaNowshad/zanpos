-- Raise the default max_turns for AI multi-step requests from 8 → 25.
-- Only updates installs that still have the old default (8); custom values are untouched.
INSERT INTO app_config (key, value, updated_at)
VALUES ('ai_max_turns', '25', datetime('now'))
ON CONFLICT (key) DO UPDATE
    SET value = '25', updated_at = datetime('now')
    WHERE CAST(value AS INTEGER) <= 8;
