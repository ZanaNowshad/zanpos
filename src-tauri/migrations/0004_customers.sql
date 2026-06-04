CREATE TABLE customers (
    customer_id TEXT PRIMARY KEY,
    branch_id TEXT NOT NULL,
    name TEXT NOT NULL,
    phone TEXT,
    email TEXT,
    loyalty_points INTEGER NOT NULL DEFAULT 0,
    notes TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT,
    version INTEGER NOT NULL DEFAULT 1,
    sync_status TEXT NOT NULL DEFAULT 'synced',
    sync_attempts INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_customers_branch ON customers(branch_id);
CREATE INDEX idx_customers_phone ON customers(phone);
