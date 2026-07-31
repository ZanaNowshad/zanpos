-- AI session & usage tracking (Phase 1.1–1.2)
CREATE TABLE ai_sessions (
    session_id         TEXT PRIMARY KEY,
    branch_id          TEXT NOT NULL,
    user_id            TEXT NOT NULL,
    provider           TEXT NOT NULL,
    model              TEXT NOT NULL,
    status             TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active','ended','error')),
    total_turns        INTEGER NOT NULL DEFAULT 0,
    tokens_in          INTEGER NOT NULL DEFAULT 0,
    tokens_out         INTEGER NOT NULL DEFAULT 0,
    cost_estimate_usd  REAL NOT NULL DEFAULT 0.0,
    total_latency_ms   INTEGER NOT NULL DEFAULT 0,
    started_at         TEXT NOT NULL,
    ended_at           TEXT
);
CREATE INDEX idx_sessions_branch ON ai_sessions(branch_id, started_at DESC);
CREATE INDEX idx_sessions_user ON ai_sessions(user_id);

CREATE TABLE ai_usage_log (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id  TEXT NOT NULL REFERENCES ai_sessions(session_id),
    turn        INTEGER NOT NULL,
    tokens_in   INTEGER NOT NULL DEFAULT 0,
    tokens_out  INTEGER NOT NULL DEFAULT 0,
    latency_ms  INTEGER NOT NULL DEFAULT 0,
    provider    TEXT NOT NULL,
    model       TEXT NOT NULL,
    logged_at   TEXT NOT NULL
);
CREATE INDEX idx_usage_session ON ai_usage_log(session_id, turn);
