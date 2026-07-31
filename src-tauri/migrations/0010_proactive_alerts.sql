CREATE TABLE proactive_alerts (
    alert_id             TEXT PRIMARY KEY,
    branch_id            TEXT NOT NULL,
    alert_type           TEXT NOT NULL,
    severity             TEXT NOT NULL CHECK (severity IN ('info','warning','critical')),
    title                TEXT NOT NULL,
    description          TEXT NOT NULL,
    detail_json          TEXT,
    detected_at          TEXT NOT NULL,
    dismissed_at         TEXT,
    dismissed_by_user_id TEXT,
    created_at           TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_alerts_branch ON proactive_alerts(branch_id, detected_at DESC);
CREATE INDEX idx_alerts_undismissed ON proactive_alerts(branch_id) WHERE dismissed_at IS NULL;

CREATE TABLE proactive_watermark (
    rule_name    TEXT PRIMARY KEY,
    last_checked TEXT NOT NULL
);
