-- Append-only cost history for supplier bill/catalog import updates.
-- Keeps a margin/audit trail when products.cost_minor changes.
CREATE TABLE product_cost_history (
    cost_history_id TEXT PRIMARY KEY,
    product_id      TEXT NOT NULL REFERENCES products(product_id),
    old_cost_minor  INTEGER,
    new_cost_minor  INTEGER NOT NULL,
    supplier_id     TEXT,
    source          TEXT NOT NULL,
    actor_user_id   TEXT,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL,
    sync_status     TEXT NOT NULL DEFAULT 'pending',
    sync_attempts   INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_product_cost_history_product ON product_cost_history(product_id, created_at);
CREATE INDEX idx_product_cost_history_sync_status ON product_cost_history(sync_status);
