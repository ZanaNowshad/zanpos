-- Purchasing must sync across every terminal in the same hub.
ALTER TABLE suppliers ADD COLUMN sync_status TEXT NOT NULL DEFAULT 'pending';
ALTER TABLE suppliers ADD COLUMN sync_attempts INTEGER NOT NULL DEFAULT 0;

ALTER TABLE purchase_orders ADD COLUMN sync_status TEXT NOT NULL DEFAULT 'pending';
ALTER TABLE purchase_orders ADD COLUMN sync_attempts INTEGER NOT NULL DEFAULT 0;

ALTER TABLE purchase_order_lines ADD COLUMN updated_at TEXT;
ALTER TABLE purchase_order_lines ADD COLUMN sync_status TEXT NOT NULL DEFAULT 'pending';
ALTER TABLE purchase_order_lines ADD COLUMN sync_attempts INTEGER NOT NULL DEFAULT 0;

UPDATE purchase_order_lines SET updated_at = created_at WHERE updated_at IS NULL OR updated_at = '';

CREATE INDEX idx_suppliers_sync_status ON suppliers(sync_status);
CREATE INDEX idx_purchase_orders_sync_status ON purchase_orders(sync_status);
CREATE INDEX idx_purchase_order_lines_sync_status ON purchase_order_lines(sync_status);
