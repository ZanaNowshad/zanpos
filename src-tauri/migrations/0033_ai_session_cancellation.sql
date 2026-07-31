CREATE TABLE ai_sessions_new (
    session_id         TEXT PRIMARY KEY,
    branch_id          TEXT NOT NULL,
    user_id            TEXT NOT NULL,
    provider           TEXT NOT NULL,
    model              TEXT NOT NULL,
    status             TEXT NOT NULL DEFAULT 'active'
                       CHECK (status IN ('active','ended','error','cancelled')),
    total_turns        INTEGER NOT NULL DEFAULT 0,
    tokens_in          INTEGER NOT NULL DEFAULT 0,
    tokens_out         INTEGER NOT NULL DEFAULT 0,
    cost_estimate_usd  REAL NOT NULL DEFAULT 0.0,
    total_latency_ms   INTEGER NOT NULL DEFAULT 0,
    started_at         TEXT NOT NULL,
    ended_at           TEXT
);

INSERT INTO ai_sessions_new
SELECT session_id, branch_id, user_id, provider, model, status, total_turns,
       tokens_in, tokens_out, cost_estimate_usd, total_latency_ms, started_at, ended_at
FROM ai_sessions;

CREATE TABLE ai_usage_log_new (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id  TEXT NOT NULL REFERENCES ai_sessions_new(session_id),
    turn        INTEGER NOT NULL,
    tokens_in   INTEGER NOT NULL DEFAULT 0,
    tokens_out  INTEGER NOT NULL DEFAULT 0,
    latency_ms  INTEGER NOT NULL DEFAULT 0,
    provider    TEXT NOT NULL,
    model       TEXT NOT NULL,
    logged_at   TEXT NOT NULL
);

INSERT INTO ai_usage_log_new
SELECT id, session_id, turn, tokens_in, tokens_out, latency_ms, provider, model, logged_at
FROM ai_usage_log;

DROP TABLE ai_usage_log;
DROP TABLE ai_sessions;
ALTER TABLE ai_sessions_new RENAME TO ai_sessions;
ALTER TABLE ai_usage_log_new RENAME TO ai_usage_log;

CREATE INDEX idx_sessions_branch ON ai_sessions(branch_id, started_at DESC);
CREATE INDEX idx_sessions_user ON ai_sessions(user_id);
CREATE INDEX idx_usage_session ON ai_usage_log(session_id, turn);

DELETE FROM app_config WHERE key = 'ai_chat_timeout_secs';
