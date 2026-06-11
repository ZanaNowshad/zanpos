-- Fix: stock_movements and audit_logs were missing updated_at columns.
--
-- The v2 sync pull watermark filter is `updated_at=gt.{watermark}` for every
-- synced table. Without this column on either table:
--   (a) If Supabase also lacks updated_at → pull fails with HTTP 400 (permanent
--       error), stops retrying after MAX_ATTEMPTS → movements and audit_logs
--       silently stop syncing forever
--   (b) If Supabase has updated_at but local SQLite does not → apply_append_only
--       attempts INSERT with updated_at value from the Supabase JSON → SQLite
--       returns "table has no column named updated_at" → hit_failure=true,
--       watermark halts permanently
--
-- After this migration:
--   • New rows inserted without specifying updated_at get datetime('now') via DEFAULT
--   • Existing rows are backfilled to created_at for historically correct watermarks
--   • Pull watermark can now advance correctly for both tables

ALTER TABLE stock_movements ADD COLUMN updated_at TEXT NOT NULL DEFAULT (datetime('now'));
UPDATE stock_movements SET updated_at = created_at;
CREATE INDEX IF NOT EXISTS idx_stock_movements_updated_at ON stock_movements(updated_at);

ALTER TABLE audit_logs ADD COLUMN updated_at TEXT NOT NULL DEFAULT (datetime('now'));
UPDATE audit_logs SET updated_at = created_at;
CREATE INDEX IF NOT EXISTS idx_audit_logs_updated_at ON audit_logs(updated_at);
