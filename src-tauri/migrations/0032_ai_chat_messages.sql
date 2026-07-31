CREATE TABLE IF NOT EXISTS ai_chat_messages (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    message_id   TEXT NOT NULL UNIQUE,
    session_id   TEXT NOT NULL,
    branch_id    TEXT NOT NULL,
    user_id      TEXT NOT NULL,
    role         TEXT NOT NULL CHECK (role IN ('user','assistant','system_event')),
    content      TEXT NOT NULL,
    message_type TEXT NOT NULL CHECK (message_type IN ('text','action_card','error')),
    created_at   TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_ai_chat_messages_owner
    ON ai_chat_messages(branch_id, user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_ai_chat_messages_session
    ON ai_chat_messages(session_id, message_id);

DELETE FROM ai_feedback
WHERE NOT EXISTS (
    SELECT 1 FROM ai_chat_messages
    WHERE ai_chat_messages.message_id = ai_feedback.message_id
);

DELETE FROM ai_feedback
WHERE rowid NOT IN (
    SELECT MAX(rowid) FROM ai_feedback GROUP BY user_id, message_id
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_ai_feedback_user_message
    ON ai_feedback(user_id, message_id);
