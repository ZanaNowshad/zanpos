CREATE TABLE shifts (
    shift_id TEXT PRIMARY KEY,
    branch_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    origin_device_id TEXT NOT NULL DEFAULT '',
    cashier_user_id TEXT NOT NULL,
    opened_at TEXT NOT NULL,
    closed_at TEXT,
    opening_cash_minor INTEGER NOT NULL DEFAULT 0,
    counted_cash_minor INTEGER,
    status TEXT NOT NULL DEFAULT 'open',
    close_notes TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    sync_status TEXT NOT NULL DEFAULT 'pending',
    sync_attempts INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE cash_events (
    cash_event_id TEXT PRIMARY KEY,
    shift_id TEXT NOT NULL REFERENCES shifts(shift_id),
    event_type TEXT NOT NULL,
    amount_minor INTEGER,
    note TEXT,
    created_by_user_id TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE INDEX idx_shifts_device ON shifts(device_id);
CREATE INDEX idx_shifts_status ON shifts(status);
CREATE INDEX idx_cash_events_shift ON cash_events(shift_id);
