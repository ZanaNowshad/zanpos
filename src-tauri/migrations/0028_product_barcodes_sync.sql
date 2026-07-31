ALTER TABLE product_barcodes ADD COLUMN updated_at TEXT NOT NULL DEFAULT '';
ALTER TABLE product_barcodes ADD COLUMN sync_status TEXT NOT NULL DEFAULT 'pending';
ALTER TABLE product_barcodes ADD COLUMN sync_attempts INTEGER NOT NULL DEFAULT 0;

UPDATE product_barcodes
SET barcode_id = COALESCE(NULLIF(barcode_id, ''), 'PBC-' || lower(hex(randomblob(12)))),
    updated_at = COALESCE(NULLIF(updated_at, ''), NULLIF(created_at, ''), datetime('now'));

CREATE UNIQUE INDEX IF NOT EXISTS idx_product_barcodes_id ON product_barcodes(barcode_id);
CREATE INDEX IF NOT EXISTS idx_product_barcodes_sync_status ON product_barcodes(sync_status);
CREATE INDEX IF NOT EXISTS idx_product_barcodes_updated_at ON product_barcodes(updated_at);

INSERT OR IGNORE INTO sync_watermark (table_name) VALUES ('product_barcodes');
