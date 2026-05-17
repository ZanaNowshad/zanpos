-- Migration 0009: Cash Events (Paid-In / Paid-Out) + Multi-Barcode per Product

-- ── Cash Events ────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS cash_events (
  cash_event_id       TEXT PRIMARY KEY,
  shift_id            TEXT NOT NULL REFERENCES shifts(shift_id),
  branch_id           TEXT NOT NULL,
  device_id           TEXT NOT NULL,
  event_type          TEXT NOT NULL CHECK(event_type IN ('paid_in','paid_out')),
  amount_minor        INTEGER NOT NULL CHECK(amount_minor > 0),
  note                TEXT,
  created_by_user_id  TEXT NOT NULL,
  created_at          TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_cash_events_shift ON cash_events(shift_id);

-- ── Product Barcodes (multiple barcodes per product) ───────────────────────────
CREATE TABLE IF NOT EXISTS product_barcodes (
  barcode_id  TEXT PRIMARY KEY,
  product_id  TEXT NOT NULL REFERENCES products(product_id) ON DELETE CASCADE,
  barcode     TEXT NOT NULL,
  created_at  TEXT NOT NULL DEFAULT (datetime('now')),
  UNIQUE(barcode)
);

CREATE INDEX IF NOT EXISTS idx_product_barcodes_product ON product_barcodes(product_id);
CREATE INDEX IF NOT EXISTS idx_product_barcodes_barcode ON product_barcodes(barcode);

-- Migrate existing barcodes from products.barcode to product_barcodes
INSERT OR IGNORE INTO product_barcodes (barcode_id, product_id, barcode, created_at)
SELECT 'PBC-' || product_id, product_id, barcode, created_at
FROM products
WHERE barcode IS NOT NULL AND barcode != '';
