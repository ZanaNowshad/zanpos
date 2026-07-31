-- Sync consistency ledger + catalog identity hardening.

CREATE TABLE IF NOT EXISTS sync_table_health (
    table_name            TEXT PRIMARY KEY,
    last_push_success_at  TEXT,
    last_pull_success_at  TEXT,
    last_error_at         TEXT,
    last_error            TEXT,
    last_failed_row_id    TEXT,
    retry_count           INTEGER NOT NULL DEFAULT 0,
    table_checksum        TEXT,
    updated_at            TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS sync_conflicts (
    conflict_id     TEXT PRIMARY KEY,
    conflict_type   TEXT NOT NULL,
    table_name      TEXT NOT NULL,
    entity_id       TEXT,
    severity        TEXT NOT NULL DEFAULT 'warning',
    title           TEXT NOT NULL,
    detail          TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'open',
    created_at      TEXT NOT NULL,
    resolved_at     TEXT
);

CREATE INDEX IF NOT EXISTS idx_sync_conflicts_status ON sync_conflicts(status, created_at);
CREATE INDEX IF NOT EXISTS idx_sync_table_health_updated ON sync_table_health(updated_at);

CREATE TRIGGER IF NOT EXISTS trg_products_sku_unique_insert
BEFORE INSERT ON products
WHEN NEW.deleted_at IS NULL
 AND NEW.is_active = 1
 AND NEW.sku IS NOT NULL
 AND TRIM(NEW.sku) <> ''
BEGIN
  SELECT RAISE(ABORT, 'sku already in use by another active product')
  WHERE EXISTS (
    SELECT 1 FROM products p
    WHERE p.deleted_at IS NULL
      AND p.is_active = 1
      AND p.product_id <> NEW.product_id
      AND LOWER(TRIM(p.sku)) = LOWER(TRIM(NEW.sku))
  );
END;

CREATE TRIGGER IF NOT EXISTS trg_products_sku_unique_update
BEFORE UPDATE OF sku, is_active, deleted_at ON products
WHEN NEW.deleted_at IS NULL
 AND NEW.is_active = 1
 AND NEW.sku IS NOT NULL
 AND TRIM(NEW.sku) <> ''
BEGIN
  SELECT RAISE(ABORT, 'sku already in use by another active product')
  WHERE EXISTS (
    SELECT 1 FROM products p
    WHERE p.deleted_at IS NULL
      AND p.is_active = 1
      AND p.product_id <> NEW.product_id
      AND LOWER(TRIM(p.sku)) = LOWER(TRIM(NEW.sku))
  );
END;
