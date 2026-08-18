-- Aggregated per-tool operational telemetry. No tool inputs or business data
-- are stored here: only counts, latency, failures and estimated result tokens.
CREATE TABLE ai_tool_metrics (
    tool_name          TEXT PRIMARY KEY,
    invocation_count   INTEGER NOT NULL DEFAULT 0,
    success_count      INTEGER NOT NULL DEFAULT 0,
    failure_count      INTEGER NOT NULL DEFAULT 0,
    total_latency_ms   INTEGER NOT NULL DEFAULT 0,
    last_latency_ms    INTEGER NOT NULL DEFAULT 0,
    estimated_tokens   INTEGER NOT NULL DEFAULT 0,
    last_error_at      TEXT,
    updated_at         TEXT NOT NULL
);

CREATE INDEX idx_ai_tool_metrics_failures
    ON ai_tool_metrics(failure_count DESC, updated_at DESC);
