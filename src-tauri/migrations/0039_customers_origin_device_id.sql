-- ── Migration 0039: customers origin_device_id ───────────────────────────────
-- The customers table was missed in migration 0032. Add origin_device_id with
-- backfill and triggers matching the pattern on other transactional tables.

ALTER TABLE customers ADD COLUMN IF NOT EXISTS origin_device_id TEXT NOT NULL DEFAULT '';
UPDATE customers SET origin_device_id = (SELECT value FROM app_config WHERE key = 'device_id')
  WHERE origin_device_id = '' OR origin_device_id IS NULL;
CREATE TRIGGER IF NOT EXISTS chk_customers_origin_nn_insert
BEFORE INSERT ON customers
BEGIN
    SELECT RAISE(ABORT, 'customers.origin_device_id must not be empty')
    WHERE NEW.origin_device_id IS NULL OR NEW.origin_device_id = '';
END;
CREATE TRIGGER IF NOT EXISTS chk_customers_origin_nn_update
BEFORE UPDATE OF origin_device_id ON customers
BEGIN
    SELECT RAISE(ABORT, 'customers.origin_device_id must not be empty')
    WHERE NEW.origin_device_id IS NULL OR NEW.origin_device_id = '';
END;
CREATE INDEX IF NOT EXISTS idx_customers_origin ON customers(origin_device_id);
