CREATE TABLE audit_logs (
    audit_log_id TEXT PRIMARY KEY,
    event_type TEXT NOT NULL,
    entity_type TEXT NOT NULL,
    entity_id TEXT NOT NULL,
    actor_user_id TEXT,
    actor_type TEXT NOT NULL DEFAULT 'user',
    ai_action_id TEXT,
    device_id TEXT,
    origin_device_id TEXT NOT NULL DEFAULT '',
    branch_id TEXT,
    before_json TEXT,
    after_json TEXT,
    reason TEXT,
    created_at TEXT NOT NULL,
    hash TEXT NOT NULL,
    previous_hash TEXT,
    override_used INTEGER NOT NULL DEFAULT 0,
    sync_status TEXT NOT NULL DEFAULT 'pending',
    sync_attempts INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE ghost_barcodes (
    ghost_id TEXT PRIMARY KEY,
    barcode TEXT NOT NULL,
    recorded_by_user_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'unresolved',
    product_name TEXT,
    product_id TEXT,
    resolved_by_user_id TEXT,
    resolved_at TEXT,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_audit_entity ON audit_logs(entity_type, entity_id);
CREATE INDEX idx_audit_device ON audit_logs(device_id);
CREATE INDEX idx_audit_created ON audit_logs(created_at);
CREATE INDEX idx_ghost_barcode ON ghost_barcodes(barcode);
