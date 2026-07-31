-- AI feedback: thumbs up/down ratings on assistant messages for RLHF-style improvement.
CREATE TABLE ai_feedback (
    feedback_id  TEXT PRIMARY KEY,
    session_id   TEXT NOT NULL,
    user_id      TEXT NOT NULL,
    message_id   TEXT NOT NULL,
    rating       TEXT NOT NULL CHECK (rating IN ('up', 'down')),
    comment      TEXT,
    created_at   TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_ai_feedback_session ON ai_feedback(session_id);
CREATE INDEX idx_ai_feedback_message ON ai_feedback(message_id);
