-- Phase 2: Admin Chat + AI Tool Executor

CREATE TABLE IF NOT EXISTS app_config (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Seed: empty API key placeholder
INSERT OR IGNORE INTO app_config (key, value) VALUES ('anthropic_api_key', '');

CREATE TABLE IF NOT EXISTS ai_actions (
    action_id          TEXT PRIMARY KEY,
    session_user_id    TEXT NOT NULL REFERENCES users(user_id),
    tool_name          TEXT NOT NULL,
    tool_input_json    TEXT NOT NULL,
    tool_input_hash    TEXT NOT NULL,
    preview_text       TEXT NOT NULL,
    status             TEXT NOT NULL DEFAULT 'prepared',
    -- prepared | confirmed | executing | executed | failed | cancelled | expired
    confirmation_token TEXT NOT NULL,
    prepared_at        TEXT NOT NULL,
    confirmed_at       TEXT,
    executed_at        TEXT,
    expires_at         TEXT NOT NULL,
    result_json        TEXT,
    error_message      TEXT
);

CREATE INDEX IF NOT EXISTS idx_ai_actions_user   ON ai_actions(session_user_id);
CREATE INDEX IF NOT EXISTS idx_ai_actions_status ON ai_actions(status);

CREATE TABLE IF NOT EXISTS undo_records (
    undo_id              TEXT PRIMARY KEY,
    action_id            TEXT NOT NULL REFERENCES ai_actions(action_id),
    entity_type          TEXT NOT NULL,
    entity_id            TEXT NOT NULL,
    snapshot_json        TEXT NOT NULL,
    rollback_tool        TEXT NOT NULL,
    rollback_input_json  TEXT NOT NULL,
    status               TEXT NOT NULL DEFAULT 'available',
    -- available | undone | expired
    created_at           TEXT NOT NULL,
    undone_at            TEXT,
    undone_by_user_id    TEXT
);

CREATE INDEX IF NOT EXISTS idx_undo_records_action ON undo_records(action_id);
