-- Phase 10b: customers, product images, thermal printer config

-- ── Customers ─────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS customers (
  customer_id    TEXT PRIMARY KEY,
  branch_id      TEXT NOT NULL REFERENCES branches(branch_id),
  name           TEXT NOT NULL,
  phone          TEXT,
  email          TEXT,
  loyalty_points INTEGER NOT NULL DEFAULT 0,
  created_at     TEXT NOT NULL DEFAULT (datetime('now')),
  notes          TEXT
);

CREATE INDEX IF NOT EXISTS idx_customers_branch ON customers(branch_id);
CREATE INDEX IF NOT EXISTS idx_customers_name   ON customers(name);
CREATE INDEX IF NOT EXISTS idx_customers_phone  ON customers(phone);

-- ── Add customer_id to sales ──────────────────────────────────────────────────
ALTER TABLE sales ADD COLUMN customer_id TEXT REFERENCES customers(customer_id);

-- ── Add image_path to products ────────────────────────────────────────────────
ALTER TABLE products ADD COLUMN image_path TEXT;

-- ── Thermal printer config ────────────────────────────────────────────────────
INSERT OR IGNORE INTO app_config(key,value) VALUES('thermal_printer_port','');
INSERT OR IGNORE INTO app_config(key,value) VALUES('thermal_printer_baud','9600');
INSERT OR IGNORE INTO app_config(key,value) VALUES('thermal_printer_enabled','0');
