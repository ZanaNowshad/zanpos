-- Phase 6: Store setup configuration
-- Adds configurable store details and first-run wizard tracking

-- ── Extend branches with store contact + receipt customisation ────────────────
ALTER TABLE branches ADD COLUMN phone         TEXT;
ALTER TABLE branches ADD COLUMN receipt_header TEXT;   -- printed at top of receipt
ALTER TABLE branches ADD COLUMN receipt_footer TEXT;   -- printed at bottom of receipt
ALTER TABLE branches ADD COLUMN tax_number    TEXT;    -- VAT / tax registration number

-- ── App-level key-value config (single device settings) ──────────────────────
CREATE TABLE IF NOT EXISTS app_config (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL,
    updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Wizard starts not-complete so first launch triggers setup
INSERT OR IGNORE INTO app_config(key, value) VALUES ('setup_complete', '0');
