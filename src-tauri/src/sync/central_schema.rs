/// Supabase central PostgreSQL schema.
///
/// This schema defines the canonical business tables for a store. Every device
/// pushes its data directly to these tables and pulls changes by querying
/// `updated_at > last_sync_timestamp`.
///
/// Design principles:
///   • ``CREATE TABLE IF NOT EXISTS`` / ``ALTER TABLE ADD COLUMN IF NOT EXISTS``
///     so the script is safe to re-run.
///   • `updated_at` is the sync watermark — present on every table.
///   • `origin_device_id` on transactional tables records provenance.
///   • `idempotency_key` UNIQUE on sales and refunds prevents duplicate push.
///   • `version` is only on master-data tables (last-write-wins).
///   • `pin_hash` is NEVER stored in the users table.
///   • No sync_events log, no apply_sync_event RPC — devices write directly.
///   • No `sync_status` / `sync_attempts` / `sync_dirty` columns — those are
///     local-SQLite concerns only.
///
/// Applied via Supabase Management API on first store setup.
pub const CENTRAL_SCHEMA_SQL: &str = r#"

-- ===========================================================================
-- 1. STORE / BRANCH REGISTRY
-- ===========================================================================
-- Created first so terminals can query it during "join existing store" setup.
CREATE TABLE IF NOT EXISTS branches (
    branch_id      TEXT PRIMARY KEY,
    branch_code    TEXT NOT NULL,
    name           TEXT NOT NULL,
    currency       TEXT NOT NULL DEFAULT 'BHD',
    timezone       TEXT NOT NULL DEFAULT 'UTC',
    address        TEXT,
    phone          TEXT,
    receipt_header TEXT,
    receipt_footer TEXT,
    tax_number     TEXT,
    is_active      BOOLEAN NOT NULL DEFAULT TRUE,
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_branches_code ON branches (branch_code) WHERE is_active;
ALTER TABLE branches ADD COLUMN IF NOT EXISTS cr_number TEXT;

-- ===========================================================================
-- 2. DEVICE REGISTRY
-- ===========================================================================
-- Materializes device records so every terminal can list all terminals on the
-- store.
CREATE TABLE IF NOT EXISTS devices (
    device_id    TEXT PRIMARY KEY,
    branch_id    TEXT NOT NULL,
    device_code  TEXT NOT NULL,
    name         TEXT NOT NULL,
    status       TEXT NOT NULL DEFAULT 'online',
    is_active    BOOLEAN NOT NULL DEFAULT TRUE,
    updated_at   TEXT
);
CREATE INDEX IF NOT EXISTS idx_devices_branch ON devices (branch_id);

-- ===========================================================================
-- 3. MASTER DATA — CATALOG (last-write-wins, versioned)
-- ===========================================================================

CREATE TABLE IF NOT EXISTS categories (
    category_id         TEXT PRIMARY KEY,
    parent_category_id  TEXT,
    name                TEXT NOT NULL,
    sort_order          BIGINT NOT NULL DEFAULT 0,
    is_active           BOOLEAN NOT NULL DEFAULT TRUE,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    version             BIGINT NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS tax_rules (
    tax_rule_id        TEXT PRIMARY KEY,
    name               TEXT NOT NULL,
    rate_basis_points  BIGINT NOT NULL DEFAULT 0,
    inclusive          BOOLEAN NOT NULL DEFAULT FALSE,
    is_active          BOOLEAN NOT NULL DEFAULT TRUE,
    effective_from     TEXT NOT NULL,
    effective_to       TEXT,
    -- updated_at is required for sync watermark, patched for older projects
    -- that were created before the column was added to the CREATE TABLE.
    updated_at         TEXT,
    version            BIGINT NOT NULL DEFAULT 1
);
ALTER TABLE tax_rules ADD COLUMN IF NOT EXISTS updated_at TEXT;

CREATE TABLE IF NOT EXISTS products (
    product_id              TEXT PRIMARY KEY,
    category_id             TEXT NOT NULL,
    name                    TEXT NOT NULL,
    sku                     TEXT,
    barcode                 TEXT,
    description             TEXT,
    track_inventory         BOOLEAN NOT NULL DEFAULT TRUE,
    allow_decimal_quantity  BOOLEAN NOT NULL DEFAULT FALSE,
    is_active               BOOLEAN NOT NULL DEFAULT TRUE,
    tax_rule_id             TEXT,
    cost_minor              BIGINT,
    currency                TEXT NOT NULL DEFAULT 'BHD',
    reorder_point           BIGINT NOT NULL DEFAULT 0,
    image_path              TEXT,
    created_at              TEXT NOT NULL,
    updated_at              TEXT NOT NULL,
    version                 BIGINT NOT NULL DEFAULT 1
);
ALTER TABLE products ADD COLUMN IF NOT EXISTS default_supplier_id TEXT;

CREATE TABLE IF NOT EXISTS product_prices (
    price_id                TEXT PRIMARY KEY,
    product_id              TEXT NOT NULL,
    branch_id               TEXT,
    price_type              TEXT NOT NULL DEFAULT 'selling',
    price_minor             BIGINT NOT NULL,
    currency                TEXT NOT NULL DEFAULT 'BHD',
    effective_from          TEXT NOT NULL,
    effective_to            TEXT,
    created_by_user_id      TEXT NOT NULL,
    created_by_ai_action_id TEXT,
    created_at              TEXT NOT NULL
);

-- ===========================================================================
-- 4. USERS
-- ===========================================================================
-- pin_hash is NEVER stored here — it stays on the local device only.
CREATE TABLE IF NOT EXISTS users (
    user_id        TEXT PRIMARY KEY,
    display_name   TEXT NOT NULL,
    username       TEXT NOT NULL,
    role_id        TEXT NOT NULL,
    branch_scope   TEXT NOT NULL DEFAULT '[]',
    is_active      BOOLEAN NOT NULL DEFAULT TRUE,
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL,
    version        BIGINT NOT NULL DEFAULT 1
);

-- ===========================================================================
-- 5. CUSTOMERS
-- ===========================================================================
CREATE TABLE IF NOT EXISTS customers (
    customer_id    TEXT PRIMARY KEY,
    branch_id      TEXT NOT NULL,
    name           TEXT NOT NULL,
    phone          TEXT,
    email          TEXT,
    loyalty_points BIGINT NOT NULL DEFAULT 0,
    notes          TEXT,
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL DEFAULT ''
);
ALTER TABLE customers ADD COLUMN IF NOT EXISTS updated_at TEXT NOT NULL DEFAULT '';
UPDATE customers SET updated_at = created_at WHERE updated_at = '';
CREATE INDEX IF NOT EXISTS idx_customers_branch ON customers (branch_id);

-- ===========================================================================
-- 6. SHIFTS
-- ===========================================================================
CREATE TABLE IF NOT EXISTS shifts (
    shift_id              TEXT PRIMARY KEY,
    branch_id             TEXT NOT NULL,
    device_id             TEXT NOT NULL,
    origin_device_id      TEXT NOT NULL,
    cashier_user_id       TEXT NOT NULL,
    opened_at             TEXT NOT NULL,
    closed_at             TEXT,
    opening_cash_minor    BIGINT NOT NULL DEFAULT 0,
    counted_cash_minor    BIGINT,
    status                TEXT NOT NULL DEFAULT 'open',
    close_notes           TEXT,
    updated_at            TEXT NOT NULL DEFAULT '',
    CHECK (origin_device_id <> '')
);
ALTER TABLE shifts ADD COLUMN IF NOT EXISTS origin_device_id TEXT NOT NULL DEFAULT 'unknown';
UPDATE shifts SET origin_device_id = device_id WHERE origin_device_id = 'unknown';
ALTER TABLE shifts DROP CONSTRAINT IF EXISTS shifts_origin_nn;
ALTER TABLE shifts ADD CONSTRAINT shifts_origin_nn CHECK (origin_device_id <> '');

-- ===========================================================================
-- 7. SALES (append-mostly, idempotent via idempotency_key)
-- ===========================================================================
CREATE TABLE IF NOT EXISTS sales (
    sale_id              TEXT PRIMARY KEY,
    receipt_number       TEXT NOT NULL,
    branch_id            TEXT NOT NULL,
    device_id            TEXT NOT NULL,
    origin_device_id     TEXT NOT NULL,
    shift_id             TEXT NOT NULL,
    cashier_user_id      TEXT NOT NULL,
    status               TEXT NOT NULL DEFAULT 'completed',
    gross_total_minor    BIGINT NOT NULL DEFAULT 0,
    discount_total_minor BIGINT NOT NULL DEFAULT 0,
    tax_total_minor      BIGINT NOT NULL DEFAULT 0,
    net_total_minor      BIGINT NOT NULL DEFAULT 0,
    currency             TEXT NOT NULL DEFAULT 'BHD',
    business_date        TEXT NOT NULL,
    sold_at              TEXT NOT NULL,
    created_offline      BOOLEAN NOT NULL DEFAULT FALSE,
    idempotency_key      TEXT NOT NULL UNIQUE,
    CHECK (origin_device_id <> '')
);
ALTER TABLE sales ADD COLUMN IF NOT EXISTS origin_device_id TEXT NOT NULL DEFAULT 'unknown';
UPDATE sales SET origin_device_id = device_id WHERE origin_device_id = 'unknown';
ALTER TABLE sales DROP CONSTRAINT IF EXISTS sales_origin_nn;
ALTER TABLE sales ADD CONSTRAINT sales_origin_nn CHECK (origin_device_id <> '');
ALTER TABLE sales ADD COLUMN IF NOT EXISTS customer_id TEXT;
ALTER TABLE sales ADD COLUMN IF NOT EXISTS is_delivery BOOLEAN NOT NULL DEFAULT FALSE;

CREATE TABLE IF NOT EXISTS sale_items (
    sale_item_id          TEXT PRIMARY KEY,
    sale_id               TEXT NOT NULL,
    product_id            TEXT,
    product_name_snapshot TEXT NOT NULL,
    sku_snapshot          TEXT,
    barcode_snapshot      TEXT,
    quantity              TEXT NOT NULL,
    unit_price_minor      BIGINT NOT NULL,
    line_discount_minor   BIGINT NOT NULL DEFAULT 0,
    tax_rule_snapshot     TEXT NOT NULL DEFAULT '{}',
    tax_amount_minor      BIGINT NOT NULL DEFAULT 0,
    line_total_minor      BIGINT NOT NULL DEFAULT 0,
    note                  TEXT,
    voided                BOOLEAN NOT NULL DEFAULT FALSE,
    origin_device_id      TEXT NOT NULL,
    CHECK (origin_device_id <> '')
);
ALTER TABLE sale_items ADD COLUMN IF NOT EXISTS origin_device_id TEXT NOT NULL DEFAULT 'unknown';
UPDATE sale_items si SET origin_device_id = s.origin_device_id
  FROM sales s WHERE si.sale_id = s.sale_id AND si.origin_device_id = 'unknown';
ALTER TABLE sale_items DROP CONSTRAINT IF EXISTS sale_items_origin_nn;
ALTER TABLE sale_items ADD CONSTRAINT sale_items_origin_nn CHECK (origin_device_id <> '');

CREATE TABLE IF NOT EXISTS payments (
    payment_id           TEXT PRIMARY KEY,
    sale_id              TEXT NOT NULL,
    origin_device_id     TEXT NOT NULL,
    payment_method       TEXT NOT NULL,
    amount_minor         BIGINT NOT NULL,
    currency             TEXT NOT NULL DEFAULT 'BHD',
    status               TEXT NOT NULL DEFAULT 'approved',
    external_reference   TEXT,
    tendered_minor       BIGINT,
    change_minor         BIGINT,
    recorded_by_user_id  TEXT NOT NULL,
    recorded_at          TEXT NOT NULL,
    CHECK (origin_device_id <> '')
);
ALTER TABLE payments ADD COLUMN IF NOT EXISTS origin_device_id TEXT NOT NULL DEFAULT 'unknown';
UPDATE payments p SET origin_device_id = s.device_id
  FROM sales s WHERE p.sale_id = s.sale_id AND p.origin_device_id = 'unknown';
ALTER TABLE payments DROP CONSTRAINT IF EXISTS payments_origin_nn;
ALTER TABLE payments ADD CONSTRAINT payments_origin_nn CHECK (origin_device_id <> '');

-- ===========================================================================
-- 8. REFUNDS (append-mostly, idempotent via idempotency_key)
-- ===========================================================================
CREATE TABLE IF NOT EXISTS refunds (
    refund_id             TEXT PRIMARY KEY,
    original_sale_id      TEXT NOT NULL,
    origin_device_id      TEXT NOT NULL,
    refund_receipt_number TEXT NOT NULL,
    reason                TEXT NOT NULL DEFAULT '',
    refund_total_minor    BIGINT NOT NULL,
    currency              TEXT NOT NULL DEFAULT 'BHD',
    created_by_user_id    TEXT NOT NULL,
    created_at            TEXT NOT NULL,
    idempotency_key       TEXT NOT NULL UNIQUE,
    CHECK (origin_device_id <> '')
);
ALTER TABLE refunds ADD COLUMN IF NOT EXISTS origin_device_id TEXT NOT NULL DEFAULT 'unknown';
UPDATE refunds r SET origin_device_id = s.device_id
  FROM sales s WHERE r.original_sale_id = s.sale_id AND r.origin_device_id = 'unknown';
ALTER TABLE refunds DROP CONSTRAINT IF EXISTS refunds_origin_nn;
ALTER TABLE refunds ADD CONSTRAINT refunds_origin_nn CHECK (origin_device_id <> '');

CREATE TABLE IF NOT EXISTS refund_items (
    refund_item_id        TEXT PRIMARY KEY,
    refund_id             TEXT NOT NULL,
    origin_device_id      TEXT NOT NULL,
    sale_item_id          TEXT NOT NULL,
    product_name_snapshot TEXT NOT NULL,
    quantity              TEXT NOT NULL,
    unit_price_minor      BIGINT NOT NULL DEFAULT 0,
    refund_amount_minor   BIGINT NOT NULL,
    CHECK (origin_device_id <> '')
);
ALTER TABLE refund_items ADD COLUMN IF NOT EXISTS origin_device_id TEXT NOT NULL DEFAULT 'unknown';
UPDATE refund_items ri SET origin_device_id = r.origin_device_id
  FROM refunds r WHERE ri.refund_id = r.refund_id AND ri.origin_device_id = 'unknown';
ALTER TABLE refund_items DROP CONSTRAINT IF EXISTS refund_items_origin_nn;
ALTER TABLE refund_items ADD CONSTRAINT refund_items_origin_nn CHECK (origin_device_id <> '');

-- ===========================================================================
-- 9. INVENTORY
-- ===========================================================================
CREATE TABLE IF NOT EXISTS stock_levels (
    stock_level_id   TEXT PRIMARY KEY,
    product_id       TEXT NOT NULL,
    branch_id        TEXT NOT NULL,
    quantity_on_hand TEXT NOT NULL DEFAULT '0',
    last_movement_at TEXT,
    updated_at       TEXT NOT NULL,
    UNIQUE(product_id, branch_id)
);

CREATE TABLE IF NOT EXISTS stock_movements (
    movement_id        TEXT PRIMARY KEY,
    product_id         TEXT NOT NULL,
    branch_id          TEXT NOT NULL,
    device_id          TEXT NOT NULL,
    origin_device_id   TEXT NOT NULL,
    movement_type      TEXT NOT NULL,
    quantity_delta     TEXT NOT NULL,
    quantity_after     TEXT NOT NULL,
    reference_type     TEXT,
    reference_id       TEXT,
    notes              TEXT,
    created_by_user_id TEXT,
    created_at         TEXT NOT NULL,
    CHECK (origin_device_id <> '')
);
ALTER TABLE stock_movements ADD COLUMN IF NOT EXISTS origin_device_id TEXT NOT NULL DEFAULT 'unknown';
UPDATE stock_movements SET origin_device_id = device_id WHERE origin_device_id = 'unknown';
ALTER TABLE stock_movements DROP CONSTRAINT IF EXISTS stock_movements_origin_nn;
ALTER TABLE stock_movements ADD CONSTRAINT stock_movements_origin_nn CHECK (origin_device_id <> '');
CREATE INDEX IF NOT EXISTS idx_sm_product  ON stock_movements (product_id);
CREATE INDEX IF NOT EXISTS idx_sm_created  ON stock_movements (created_at);

-- ===========================================================================
-- 10. AUDIT LOGS (append-only)
-- ===========================================================================
CREATE TABLE IF NOT EXISTS audit_logs (
    audit_log_id    TEXT PRIMARY KEY,
    event_type      TEXT NOT NULL,
    entity_type     TEXT NOT NULL,
    entity_id       TEXT NOT NULL,
    actor_user_id   TEXT,
    actor_type      TEXT NOT NULL DEFAULT 'user',
    ai_action_id    TEXT,
    device_id       TEXT,
    origin_device_id TEXT NOT NULL,
    branch_id       TEXT,
    before_json     TEXT,
    after_json      TEXT,
    reason          TEXT,
    created_at      TEXT NOT NULL,
    hash            TEXT NOT NULL,
    previous_hash   TEXT,
    CHECK (origin_device_id <> '')
);
ALTER TABLE audit_logs ADD COLUMN IF NOT EXISTS origin_device_id TEXT NOT NULL DEFAULT 'unknown';
UPDATE audit_logs SET origin_device_id = COALESCE(NULLIF(device_id, ''), 'unknown') WHERE origin_device_id = 'unknown';
ALTER TABLE audit_logs DROP CONSTRAINT IF EXISTS audit_logs_origin_nn;
ALTER TABLE audit_logs ADD CONSTRAINT audit_logs_origin_nn CHECK (origin_device_id <> '');

-- ===========================================================================
-- 11. DELIVERY ORDERS
-- ===========================================================================
CREATE TABLE IF NOT EXISTS delivery_orders (
    delivery_id             TEXT PRIMARY KEY,
    sale_id                 TEXT NOT NULL,
    receipt_number          TEXT NOT NULL,
    branch_id               TEXT NOT NULL,
    device_id               TEXT NOT NULL,
    origin_device_id        TEXT NOT NULL,
    customer_id             TEXT,
    customer_name           TEXT,
    contact_number          TEXT NOT NULL,
    address_text            TEXT,
    house_number            TEXT,
    area                    TEXT,
    delivery_status         TEXT NOT NULL DEFAULT 'pending',
    delivery_staff_name     TEXT,
    delivery_note           TEXT,
    expected_payment_method TEXT NOT NULL DEFAULT 'cash',
    payment_status          TEXT NOT NULL DEFAULT 'unpaid',
    amount_minor            BIGINT NOT NULL DEFAULT 0,
    currency                TEXT NOT NULL DEFAULT 'BHD',
    paid_confirmed_at       TEXT,
    created_by_user_id      TEXT NOT NULL,
    created_at              TEXT NOT NULL,
    updated_at              TEXT NOT NULL,
    CHECK (origin_device_id <> '')
);
ALTER TABLE delivery_orders ADD COLUMN IF NOT EXISTS receipt_number TEXT;
ALTER TABLE delivery_orders ADD COLUMN IF NOT EXISTS contact_number TEXT;
ALTER TABLE delivery_orders ADD COLUMN IF NOT EXISTS delivery_status TEXT;
ALTER TABLE delivery_orders ADD COLUMN IF NOT EXISTS delivery_staff_name TEXT;
ALTER TABLE delivery_orders ADD COLUMN IF NOT EXISTS delivery_note TEXT;
ALTER TABLE delivery_orders ADD COLUMN IF NOT EXISTS expected_payment_method TEXT;
ALTER TABLE delivery_orders ADD COLUMN IF NOT EXISTS payment_status TEXT;
ALTER TABLE delivery_orders ADD COLUMN IF NOT EXISTS paid_confirmed_at TEXT;
ALTER TABLE delivery_orders ADD COLUMN IF NOT EXISTS created_by_user_id TEXT;
ALTER TABLE delivery_orders ADD COLUMN IF NOT EXISTS origin_device_id TEXT NOT NULL DEFAULT 'unknown';
UPDATE delivery_orders SET origin_device_id = device_id WHERE origin_device_id = 'unknown';
ALTER TABLE delivery_orders DROP CONSTRAINT IF EXISTS delivery_orders_origin_nn;
ALTER TABLE delivery_orders ADD CONSTRAINT delivery_orders_origin_nn CHECK (origin_device_id <> '');
CREATE INDEX IF NOT EXISTS idx_delivery_branch  ON delivery_orders (branch_id, delivery_status);
CREATE INDEX IF NOT EXISTS idx_delivery_sale    ON delivery_orders (sale_id);

-- ===========================================================================
-- 11b. SCHEMA ADDITIONS — columns added after initial deployment
--       Safe to re-run: ADD COLUMN IF NOT EXISTS is idempotent.
-- ===========================================================================
-- deleted_at (local soft-delete marker)
ALTER TABLE categories ADD COLUMN IF NOT EXISTS deleted_at TEXT;
ALTER TABLE tax_rules ADD COLUMN IF NOT EXISTS deleted_at TEXT;
ALTER TABLE products ADD COLUMN IF NOT EXISTS deleted_at TEXT;
ALTER TABLE devices ADD COLUMN IF NOT EXISTS deleted_at TEXT;
ALTER TABLE customers ADD COLUMN IF NOT EXISTS deleted_at TEXT;
ALTER TABLE shifts ADD COLUMN IF NOT EXISTS deleted_at TEXT;

-- tax_rules — missing created_at
ALTER TABLE tax_rules ADD COLUMN IF NOT EXISTS created_at TEXT;

-- devices — missing metadata columns
ALTER TABLE devices ADD COLUMN IF NOT EXISTS next_receipt_seq BIGINT NOT NULL DEFAULT 1;
ALTER TABLE devices ADD COLUMN IF NOT EXISTS last_seen_at TEXT;
ALTER TABLE devices ADD COLUMN IF NOT EXISTS created_at TEXT;
ALTER TABLE devices ADD COLUMN IF NOT EXISTS version BIGINT NOT NULL DEFAULT 1;

-- users — columns added for cross-device sync compatibility
ALTER TABLE users ADD COLUMN IF NOT EXISTS last_login_at TEXT;
ALTER TABLE users ADD COLUMN IF NOT EXISTS branch_id TEXT;

-- customers — missing version
ALTER TABLE customers ADD COLUMN IF NOT EXISTS version BIGINT NOT NULL DEFAULT 1;

-- shifts — missing operational columns
ALTER TABLE shifts ADD COLUMN IF NOT EXISTS expected_cash_minor BIGINT;
ALTER TABLE shifts ADD COLUMN IF NOT EXISTS cash_difference_minor BIGINT;
ALTER TABLE shifts ADD COLUMN IF NOT EXISTS business_date TEXT;
ALTER TABLE shifts ADD COLUMN IF NOT EXISTS created_at TEXT;
ALTER TABLE shifts ADD COLUMN IF NOT EXISTS version BIGINT NOT NULL DEFAULT 1;

-- sales — missing timestamps
ALTER TABLE sales ADD COLUMN IF NOT EXISTS created_at TEXT;
ALTER TABLE sales ADD COLUMN IF NOT EXISTS updated_at TEXT;

-- sale_items — missing timestamps + refund tracking
ALTER TABLE sale_items ADD COLUMN IF NOT EXISTS refunded_amount_minor BIGINT;
ALTER TABLE sale_items ADD COLUMN IF NOT EXISTS created_at TEXT;
ALTER TABLE sale_items ADD COLUMN IF NOT EXISTS updated_at TEXT;

-- payments — missing timestamps
ALTER TABLE payments ADD COLUMN IF NOT EXISTS created_at TEXT;
ALTER TABLE payments ADD COLUMN IF NOT EXISTS updated_at TEXT;

-- refunds — missing return_reason + timestamp
ALTER TABLE refunds ADD COLUMN IF NOT EXISTS return_reason_code TEXT;
ALTER TABLE refunds ADD COLUMN IF NOT EXISTS updated_at TEXT;

-- refund_items — missing timestamps
ALTER TABLE refund_items ADD COLUMN IF NOT EXISTS created_at TEXT;
ALTER TABLE refund_items ADD COLUMN IF NOT EXISTS updated_at TEXT;

-- audit_logs — missing override flag
ALTER TABLE audit_logs ADD COLUMN IF NOT EXISTS override_used BOOLEAN NOT NULL DEFAULT FALSE;

-- delivery_orders — missing metadata columns
ALTER TABLE delivery_orders ADD COLUMN IF NOT EXISTS paid_confirmed_by_user_id TEXT;
ALTER TABLE delivery_orders ADD COLUMN IF NOT EXISTS payment_reference TEXT;
ALTER TABLE delivery_orders ADD COLUMN IF NOT EXISTS payment_note TEXT;
ALTER TABLE delivery_orders ADD COLUMN IF NOT EXISTS version BIGINT NOT NULL DEFAULT 1;

-- stock_movements, audit_logs, product_prices — missing updated_at (sync watermark)
ALTER TABLE stock_movements ADD COLUMN IF NOT EXISTS updated_at TEXT;
ALTER TABLE audit_logs ADD COLUMN IF NOT EXISTS updated_at TEXT;
ALTER TABLE product_prices ADD COLUMN IF NOT EXISTS updated_at TEXT;

-- stock_levels — missing timestamp columns
ALTER TABLE stock_levels ADD COLUMN IF NOT EXISTS created_at TEXT;
ALTER TABLE stock_levels ADD COLUMN IF NOT EXISTS updated_at TEXT;
-- pin_hash is intentionally absent from central (never synced).

-- ===========================================================================
-- 11b. APP CONFIG (whitelisted store-wide business flags only)
-- ===========================================================================
-- The sync worker pushes/pulls a small allowlist of store-wide flags
-- (ALLOWED_CONFIG_KEYS) so settings converge across terminals. Device-local
-- keys (Supabase creds, printer port, sync watermarks) are never synced.
-- Defined here — before the GRANT block below — so GRANT ALL ON ALL TABLES
-- covers it. Without this table the worker logs PGRST205 every cycle.
CREATE TABLE IF NOT EXISTS app_config (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL DEFAULT '',
    updated_at TEXT NOT NULL
);

-- ===========================================================================
-- 12. PERMISSIONS
-- ===========================================================================
-- Fresh Supabase projects ship with a locked-down public schema. Without
-- explicit grants, PostgREST returns "403 permission denied for schema
-- public" on every table read — this is the clean-install blocker. These
-- grants are idempotent and cover tables, sequences, and routines.
-- ALTER DEFAULT PRIVILEGES ensures future objects also get the right grants.
GRANT USAGE ON SCHEMA public TO anon, authenticated, service_role;
GRANT ALL ON ALL TABLES IN SCHEMA public TO anon, authenticated, service_role;
GRANT ALL ON ALL SEQUENCES IN SCHEMA public TO anon, authenticated, service_role;
GRANT ALL ON ALL ROUTINES IN SCHEMA public TO anon, authenticated, service_role;

-- Audit logs are append-only: no updates or deletes allowed.
REVOKE UPDATE, DELETE ON audit_logs FROM anon;
REVOKE UPDATE, DELETE ON audit_logs FROM authenticated;
GRANT SELECT, INSERT ON audit_logs TO anon;
GRANT SELECT, INSERT ON audit_logs TO authenticated;

ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON TABLES   TO anon, authenticated, service_role;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON SEQUENCES TO anon, authenticated, service_role;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON ROUTINES  TO anon, authenticated, service_role;

"#;
