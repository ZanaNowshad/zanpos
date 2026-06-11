-- Fix: product_prices was missing updated_at column.
--
-- The v2 sync pull watermark filter is `updated_at=gt.{watermark}` for every
-- synced table.  Without this column:
--   (a) pull rows from Supabase fail to INSERT locally (column mismatch)
--   (b) pull watermark (max_ts) never advances → infinite re-fetch of all rows
--   (c) push payload omits updated_at → Supabase upsert fails if schema has it NOT NULL
--
-- After this migration:
--   • New product_prices rows inserted without specifying updated_at get the current
--     timestamp via the column DEFAULT.
--   • Existing rows are backfilled to created_at so that pull watermarks advance
--     correctly from the first full pull cycle.

ALTER TABLE product_prices ADD COLUMN updated_at TEXT NOT NULL DEFAULT (datetime('now'));

-- Backfill existing rows: use created_at as the historically accurate updated_at.
UPDATE product_prices SET updated_at = created_at;

CREATE INDEX IF NOT EXISTS idx_product_prices_updated_at ON product_prices(updated_at);
