-- Expiry belongs to a received lot, represented by its receive movement.
-- Existing movements remain non-expiring and require no backfill.
ALTER TABLE stock_movements ADD COLUMN expiry_date TEXT;
ALTER TABLE stock_movements ADD COLUMN lot_quantity_received TEXT;
ALTER TABLE stock_movements ADD COLUMN lot_quantity_remaining TEXT;

CREATE INDEX idx_stock_movements_expiry_lots
ON stock_movements(branch_id, expiry_date, product_id)
WHERE movement_type = 'receive'
  AND expiry_date IS NOT NULL;
