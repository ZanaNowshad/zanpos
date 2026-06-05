-- stock_levels: derived cache, one row per product per branch
CREATE TABLE stock_levels (
    stock_level_id TEXT PRIMARY KEY,
    product_id TEXT NOT NULL REFERENCES products(product_id),
    branch_id TEXT NOT NULL,
    quantity_on_hand TEXT NOT NULL DEFAULT '0',
    last_movement_at TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    sync_status TEXT NOT NULL DEFAULT 'pending',
    sync_attempts INTEGER NOT NULL DEFAULT 0,
    UNIQUE(product_id, branch_id)
);

-- stock_movements: immutable inventory ledger
CREATE TABLE stock_movements (
    movement_id TEXT PRIMARY KEY,
    product_id TEXT NOT NULL REFERENCES products(product_id),
    branch_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    origin_device_id TEXT NOT NULL DEFAULT '',
    movement_type TEXT NOT NULL,
    quantity_delta TEXT NOT NULL,
    quantity_after TEXT NOT NULL,
    reference_type TEXT,
    reference_id TEXT,
    notes TEXT,
    created_by_user_id TEXT,
    created_at TEXT NOT NULL,
    sync_status TEXT NOT NULL DEFAULT 'pending',
    sync_attempts INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_stock_levels_product ON stock_levels(product_id);
CREATE INDEX idx_stock_levels_branch ON stock_levels(branch_id);
CREATE INDEX idx_stock_movements_product ON stock_movements(product_id);
CREATE INDEX idx_stock_movements_created ON stock_movements(created_at);
