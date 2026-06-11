-- 0004_missing_columns.sql
-- Fixes gaps found during setup-migration audit (2026-06-09):
--   1. roles.updated_at  — missing updated_at column (sync watermark cannot advance)
--   2. roles sync columns — sync_status / sync_attempts missing (roles are syncable data)
--   3. cash_events.updated_at — missing; sync pull watermark filter would stall
--
-- HUMAN DECISIONS (cannot be auto-applied without verifying existing data):
--   A. UNIQUE(device_code) globally — current schema has UNIQUE(branch_id, device_code).
--      A single-store POS sharing codes across branches would be rejected.
--      Run AFTER confirming no duplicates:
--        CREATE UNIQUE INDEX idx_devices_code_global ON devices(device_code);
--
--   B. UNIQUE partial index on customers(phone) — no partial unique exists.
--      SQLite syntax: CREATE UNIQUE INDEX idx_customers_phone_uniq ON customers(phone) WHERE phone IS NOT NULL;
--      Run AFTER confirming no duplicate phone numbers exist in existing data.
--
--   C. product_barcodes has no sync_status / sync_attempts and is not in sync_watermark.
--      If barcode data should be synced to Supabase, add:
--        ALTER TABLE product_barcodes ADD COLUMN updated_at TEXT NOT NULL DEFAULT (datetime('now'));
--        ALTER TABLE product_barcodes ADD COLUMN sync_status TEXT NOT NULL DEFAULT 'pending';
--        ALTER TABLE product_barcodes ADD COLUMN sync_attempts INTEGER NOT NULL DEFAULT 0;
--        INSERT OR IGNORE INTO sync_watermark (table_name) VALUES ('product_barcodes');
--      If barcodes are local-only, no action needed.

-- ── 1. roles: add updated_at ──────────────────────────────────────────────────
-- roles is a seed-only table historically, but the sync watermark table includes
-- no entry for it — still safe to add updated_at so any future sync path can
-- filter by it without a schema mismatch error.

ALTER TABLE roles ADD COLUMN updated_at TEXT NOT NULL DEFAULT '';

-- Backfill existing rows using created_at as a historically correct baseline.
UPDATE roles SET updated_at = COALESCE(NULLIF(created_at, ''), datetime('now'));

CREATE INDEX IF NOT EXISTS idx_roles_updated_at ON roles(updated_at);

-- ── 2. roles: add sync columns ────────────────────────────────────────────────
-- roles is seeded but could be extended (custom roles) and pulled from Supabase.
-- Without sync_status the sync worker cannot push new/updated role rows.

ALTER TABLE roles ADD COLUMN sync_status TEXT NOT NULL DEFAULT 'pending';
ALTER TABLE roles ADD COLUMN sync_attempts INTEGER NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS idx_roles_sync_status ON roles(sync_status);

-- ── 3. cash_events: updated_at already exists (verified in 0001_initial.sql).
--    sync_status + sync_attempts are also present. The previously duplicated
--    0004_cash_events_updated_at.sql has been removed — no ALTER needed here.
