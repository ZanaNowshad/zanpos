-- Phase 4: Inventory Tracking
-- Adds reorder_point to products, stock_levels (materialized), stock_movements (ledger)

-- ── Extend products with reorder threshold ────────────────────────────────────
ALTER TABLE products ADD COLUMN reorder_point INTEGER NOT NULL DEFAULT 0;

-- ── Stock Levels (one row per product+branch, materialised for fast POS reads) ─
CREATE TABLE IF NOT EXISTS stock_levels (
    stock_level_id   TEXT PRIMARY KEY,
    product_id       TEXT NOT NULL REFERENCES products(product_id),
    branch_id        TEXT NOT NULL,
    quantity_on_hand TEXT NOT NULL DEFAULT '0',   -- stored as text for decimal safety
    last_movement_at TEXT,
    updated_at       TEXT NOT NULL,
    UNIQUE(product_id, branch_id)
);

CREATE INDEX IF NOT EXISTS idx_stock_levels_product ON stock_levels(product_id);
CREATE INDEX IF NOT EXISTS idx_stock_levels_branch  ON stock_levels(branch_id);

-- ── Stock Movements (append-only ledger) ──────────────────────────────────────
CREATE TABLE IF NOT EXISTS stock_movements (
    movement_id        TEXT PRIMARY KEY,
    product_id         TEXT NOT NULL REFERENCES products(product_id),
    branch_id          TEXT NOT NULL,
    device_id          TEXT NOT NULL,
    movement_type      TEXT NOT NULL,   -- sale | refund | adjustment | stock_take | receive
    quantity_delta     TEXT NOT NULL,   -- negative = deduction
    quantity_after     TEXT NOT NULL,   -- resulting qty (audit trail)
    reference_type     TEXT,            -- 'sale' | 'refund' | 'ai_action'
    reference_id       TEXT,            -- sale_id / refund_id / ai_action_id
    notes              TEXT,
    created_by_user_id TEXT,
    created_at         TEXT NOT NULL,
    sync_status        TEXT NOT NULL DEFAULT 'pending'
);

CREATE INDEX IF NOT EXISTS idx_stock_movements_product  ON stock_movements(product_id);
CREATE INDEX IF NOT EXISTS idx_stock_movements_ref      ON stock_movements(reference_type, reference_id);
CREATE INDEX IF NOT EXISTS idx_stock_movements_created  ON stock_movements(created_at);

-- ── Seed stock_levels rows for all existing tracked products ──────────────────
INSERT OR IGNORE INTO stock_levels (stock_level_id, product_id, branch_id, quantity_on_hand, updated_at)
SELECT
    'SL-' || product_id,
    product_id,
    '01JBRANCH0000000000000001',
    '0',
    datetime('now')
FROM products WHERE track_inventory = 1;
