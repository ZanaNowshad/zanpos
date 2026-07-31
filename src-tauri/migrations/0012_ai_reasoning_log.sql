-- P1-05: Store AI reasoning/chain-of-thought for audit and explainability.
-- Populated by the non-streaming tool loop (OpenAI/DeepSeek reasoning models).
CREATE TABLE IF NOT EXISTS ai_reasoning_log (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id  TEXT,
    branch_id   TEXT,
    user_id     TEXT,
    turn        INTEGER NOT NULL DEFAULT 0,
    tool_name   TEXT,
    reasoning   TEXT NOT NULL,
    logged_at   TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_reasoning_session ON ai_reasoning_log (session_id, turn);
