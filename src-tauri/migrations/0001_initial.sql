-- ZANPOS Phase 0 initial schema
-- All monetary values stored as INTEGER minor units (BHD uses 3 decimal places: 1.000 BHD = 1000)
-- All IDs are ULID strings
-- All timestamps are UTC ISO-8601 text

PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

-- ─── Branches ────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS branches (
    branch_id    TEXT PRIMARY KEY,
    branch_code  TEXT NOT NULL UNIQUE,
    name         TEXT NOT NULL,
    address      TEXT,
    timezone     TEXT NOT NULL DEFAULT 'Asia/Bahrain',
    currency     TEXT NOT NULL DEFAULT 'BHD',
    is_active    INTEGER NOT NULL DEFAULT 1,
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL,
    version      INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS idx_branches_active ON branches(is_active);
CREATE INDEX IF NOT EXISTS idx_branches_code   ON branches(branch_code);

-- ─── Devices ─────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS devices (
    device_id          TEXT PRIMARY KEY,
    branch_id          TEXT NOT NULL REFERENCES branches(branch_id),
    device_code        TEXT NOT NULL,
    name               TEXT NOT NULL,
    app_version        TEXT NOT NULL DEFAULT '0.1.0',
    last_seen_at       TEXT,
    last_sync_at       TEXT,
    status             TEXT NOT NULL DEFAULT 'offline',  -- online/offline/stale/error
    printer_status     TEXT NOT NULL DEFAULT 'unknown',  -- unknown/ok/error
    cash_drawer_status TEXT NOT NULL DEFAULT 'unknown',
    scanner_status     TEXT NOT NULL DEFAULT 'unknown',
    local_db_status    TEXT NOT NULL DEFAULT 'ok',       -- ok/warning/error
    sync_error_count   INTEGER NOT NULL DEFAULT 0,
    is_active          INTEGER NOT NULL DEFAULT 1,
    UNIQUE(branch_id, device_code)
);

-- ─── Categories ──────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS categories (
    category_id        TEXT PRIMARY KEY,
    parent_category_id TEXT REFERENCES categories(category_id),
    name               TEXT NOT NULL,
    sort_order         INTEGER NOT NULL DEFAULT 0,
    is_active          INTEGER NOT NULL DEFAULT 1,
    created_at         TEXT NOT NULL,
    updated_at         TEXT NOT NULL,
    version            INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS idx_categories_parent ON categories(parent_category_id);
CREATE INDEX IF NOT EXISTS idx_categories_active ON categories(is_active);

-- ─── Tax Rules ───────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS tax_rules (
    tax_rule_id       TEXT PRIMARY KEY,
    name              TEXT NOT NULL,
    rate_basis_points INTEGER NOT NULL DEFAULT 0,  -- 10% = 1000 basis points
    inclusive         INTEGER NOT NULL DEFAULT 0,  -- 0=exclusive, 1=inclusive
    is_active         INTEGER NOT NULL DEFAULT 1,
    effective_from    TEXT NOT NULL,
    effective_to      TEXT,
    version           INTEGER NOT NULL DEFAULT 1,
    CHECK(rate_basis_points >= 0)
);

-- ─── Products ────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS products (
    product_id             TEXT PRIMARY KEY,
    category_id            TEXT NOT NULL REFERENCES categories(category_id),
    name                   TEXT NOT NULL,
    sku                    TEXT UNIQUE,
    barcode                TEXT UNIQUE,
    description            TEXT,
    track_inventory        INTEGER NOT NULL DEFAULT 1,
    allow_decimal_quantity INTEGER NOT NULL DEFAULT 0,
    is_active              INTEGER NOT NULL DEFAULT 1,
    tax_rule_id            TEXT REFERENCES tax_rules(tax_rule_id),
    default_supplier_id    TEXT,
    cost_minor             INTEGER CHECK(cost_minor IS NULL OR cost_minor >= 0),
    currency               TEXT NOT NULL DEFAULT 'BHD',
    created_at             TEXT NOT NULL,
    updated_at             TEXT NOT NULL,
    version                INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS idx_products_name     ON products(name);
CREATE INDEX IF NOT EXISTS idx_products_sku      ON products(sku) WHERE sku IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_products_barcode  ON products(barcode) WHERE barcode IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_products_category ON products(category_id);
CREATE INDEX IF NOT EXISTS idx_products_active   ON products(is_active);

-- ─── Product Prices (append-only history) ─────────────────────────────────
CREATE TABLE IF NOT EXISTS product_prices (
    price_id                TEXT PRIMARY KEY,
    product_id              TEXT NOT NULL REFERENCES products(product_id),
    branch_id               TEXT,  -- NULL = global default price
    price_type              TEXT NOT NULL DEFAULT 'selling',  -- selling/default/promo
    price_minor             INTEGER NOT NULL,
    currency                TEXT NOT NULL DEFAULT 'BHD',
    effective_from          TEXT NOT NULL,
    effective_to            TEXT,
    created_by_user_id      TEXT NOT NULL,
    created_by_ai_action_id TEXT,
    created_at              TEXT NOT NULL,
    CHECK(price_minor >= 0)
);

CREATE INDEX IF NOT EXISTS idx_prices_product_branch_effective ON product_prices(product_id, branch_id, effective_from);
CREATE INDEX IF NOT EXISTS idx_prices_effective_from           ON product_prices(effective_from);

-- ─── Roles & Permissions ──────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS roles (
    role_id       TEXT PRIMARY KEY,
    name          TEXT NOT NULL UNIQUE,
    description   TEXT,
    is_system_role INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS permissions (
    permission_id  TEXT PRIMARY KEY,
    permission_key TEXT NOT NULL UNIQUE,
    description    TEXT
);

CREATE TABLE IF NOT EXISTS role_permissions (
    role_id        TEXT NOT NULL REFERENCES roles(role_id),
    permission_id  TEXT NOT NULL REFERENCES permissions(permission_id),
    PRIMARY KEY(role_id, permission_id)
);

-- ─── Users / Staff ───────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS users (
    user_id       TEXT PRIMARY KEY,
    display_name  TEXT NOT NULL,
    username      TEXT NOT NULL UNIQUE,
    pin_hash      TEXT,
    password_hash TEXT,
    role_id       TEXT NOT NULL REFERENCES roles(role_id),
    branch_scope  TEXT NOT NULL DEFAULT '[]',  -- JSON array of branch_ids; empty = all
    is_active     INTEGER NOT NULL DEFAULT 1,
    last_login_at TEXT,
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL,
    version       INTEGER NOT NULL DEFAULT 1
);

CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);
CREATE INDEX IF NOT EXISTS idx_users_role     ON users(role_id);
CREATE INDEX IF NOT EXISTS idx_users_active   ON users(is_active);

-- ─── Shifts ──────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS shifts (
    shift_id              TEXT PRIMARY KEY,
    branch_id             TEXT NOT NULL REFERENCES branches(branch_id),
    device_id             TEXT NOT NULL REFERENCES devices(device_id),
    cashier_user_id       TEXT NOT NULL REFERENCES users(user_id),
    opened_at             TEXT NOT NULL,
    closed_at             TEXT,
    opening_cash_minor    INTEGER NOT NULL DEFAULT 0,
    expected_cash_minor   INTEGER,
    counted_cash_minor    INTEGER,
    cash_difference_minor INTEGER,
    status                TEXT NOT NULL DEFAULT 'open',  -- open/closed/corrected
    close_notes           TEXT,
    sync_status           TEXT NOT NULL DEFAULT 'pending'  -- pending/synced/conflict
);

CREATE INDEX IF NOT EXISTS idx_shifts_branch_date ON shifts(branch_id, opened_at);
CREATE INDEX IF NOT EXISTS idx_shifts_cashier     ON shifts(cashier_user_id);
CREATE INDEX IF NOT EXISTS idx_shifts_status      ON shifts(status);

-- ─── Sales (immutable once completed) ────────────────────────────────────────
CREATE TABLE IF NOT EXISTS sales (
    sale_id              TEXT PRIMARY KEY,
    receipt_number       TEXT NOT NULL,
    branch_id            TEXT NOT NULL REFERENCES branches(branch_id),
    device_id            TEXT NOT NULL REFERENCES devices(device_id),
    shift_id             TEXT NOT NULL REFERENCES shifts(shift_id),
    cashier_user_id      TEXT NOT NULL REFERENCES users(user_id),
    status               TEXT NOT NULL DEFAULT 'completed',  -- completed/voided/partially_refunded/refunded
    gross_total_minor    INTEGER NOT NULL DEFAULT 0,
    discount_total_minor INTEGER NOT NULL DEFAULT 0,
    tax_total_minor      INTEGER NOT NULL DEFAULT 0,
    net_total_minor      INTEGER NOT NULL DEFAULT 0,
    currency             TEXT NOT NULL DEFAULT 'BHD',
    business_date        TEXT NOT NULL,  -- YYYY-MM-DD local business date
    sold_at              TEXT NOT NULL,  -- UTC
    created_offline      INTEGER NOT NULL DEFAULT 0,
    idempotency_key      TEXT NOT NULL UNIQUE,
    sync_status          TEXT NOT NULL DEFAULT 'pending',  -- pending/synced/conflict
    CHECK(net_total_minor >= 0)
);

CREATE INDEX IF NOT EXISTS idx_sales_receipt    ON sales(receipt_number);
CREATE INDEX IF NOT EXISTS idx_sales_branch_date ON sales(branch_id, business_date);
CREATE INDEX IF NOT EXISTS idx_sales_cashier    ON sales(cashier_user_id);
CREATE INDEX IF NOT EXISTS idx_sales_shift      ON sales(shift_id);
CREATE INDEX IF NOT EXISTS idx_sales_sync_status ON sales(sync_status);

-- ─── Sale Items (immutable) ───────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS sale_items (
    sale_item_id         TEXT PRIMARY KEY,
    sale_id              TEXT NOT NULL REFERENCES sales(sale_id),
    product_id           TEXT REFERENCES products(product_id),  -- nullable for custom items
    product_name_snapshot TEXT NOT NULL,
    sku_snapshot         TEXT,
    barcode_snapshot     TEXT,
    quantity             TEXT NOT NULL,  -- stored as text to preserve decimal precision
    unit_price_minor     INTEGER NOT NULL,
    line_discount_minor  INTEGER NOT NULL DEFAULT 0,
    tax_rule_snapshot    TEXT NOT NULL DEFAULT '{}',  -- JSON snapshot
    tax_amount_minor     INTEGER NOT NULL DEFAULT 0,
    line_total_minor     INTEGER NOT NULL DEFAULT 0,
    note                 TEXT,
    voided               INTEGER NOT NULL DEFAULT 0,
    void_reason          TEXT
);

CREATE INDEX IF NOT EXISTS idx_sale_items_sale    ON sale_items(sale_id);
CREATE INDEX IF NOT EXISTS idx_sale_items_product ON sale_items(product_id);

-- ─── Payments (immutable) ─────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS payments (
    payment_id           TEXT PRIMARY KEY,
    sale_id              TEXT NOT NULL REFERENCES sales(sale_id),
    payment_method       TEXT NOT NULL,  -- cash/card/wallet/other
    amount_minor         INTEGER NOT NULL,
    currency             TEXT NOT NULL DEFAULT 'BHD',
    status               TEXT NOT NULL DEFAULT 'approved',  -- approved/declined/failed/pending/refunded
    external_reference   TEXT,
    tendered_minor       INTEGER,  -- cash only
    change_minor         INTEGER,  -- cash only
    recorded_by_user_id  TEXT NOT NULL REFERENCES users(user_id),
    recorded_at          TEXT NOT NULL,
    sync_status          TEXT NOT NULL DEFAULT 'pending',
    CHECK(amount_minor >= 0)
);

CREATE INDEX IF NOT EXISTS idx_payments_sale        ON payments(sale_id);
CREATE INDEX IF NOT EXISTS idx_payments_method_date ON payments(payment_method, recorded_at);

-- ─── Audit Logs (append-only) ─────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS audit_logs (
    audit_log_id  TEXT PRIMARY KEY,
    event_type    TEXT NOT NULL,   -- e.g. sale.created, product.price.changed
    entity_type   TEXT NOT NULL,
    entity_id     TEXT NOT NULL,
    actor_user_id TEXT,
    actor_type    TEXT NOT NULL DEFAULT 'user',  -- user/ai_agent/system
    ai_action_id  TEXT,
    device_id     TEXT,
    branch_id     TEXT,
    before_json   TEXT,
    after_json    TEXT,
    reason        TEXT,
    created_at    TEXT NOT NULL,
    hash          TEXT NOT NULL,
    previous_hash TEXT
);

CREATE INDEX IF NOT EXISTS idx_audit_entity     ON audit_logs(entity_type, entity_id);
CREATE INDEX IF NOT EXISTS idx_audit_actor      ON audit_logs(actor_user_id);
CREATE INDEX IF NOT EXISTS idx_audit_created_at ON audit_logs(created_at);

-- ─── Sync Queue (outbox) ──────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS sync_queue (
    sync_event_id      TEXT PRIMARY KEY,
    device_id          TEXT NOT NULL,
    branch_id          TEXT NOT NULL,
    entity_type        TEXT NOT NULL,  -- sale/payment/product/shift/etc.
    entity_id          TEXT NOT NULL,
    operation          TEXT NOT NULL,  -- create/update/append
    payload_json       TEXT NOT NULL,
    payload_hash       TEXT NOT NULL,
    idempotency_key    TEXT NOT NULL UNIQUE,
    local_sequence     INTEGER NOT NULL,
    created_at         TEXT NOT NULL,
    status             TEXT NOT NULL DEFAULT 'pending',  -- pending/sending/synced/failed/conflict
    attempt_count      INTEGER NOT NULL DEFAULT 0,
    last_attempt_at    TEXT,
    last_error         TEXT
);

CREATE INDEX IF NOT EXISTS idx_sync_queue_status   ON sync_queue(status);
CREATE INDEX IF NOT EXISTS idx_sync_queue_sequence ON sync_queue(local_sequence);
CREATE INDEX IF NOT EXISTS idx_sync_queue_entity   ON sync_queue(entity_type, entity_id);

-- ─── Sync State ───────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS sync_state (
    sync_state_id                 TEXT PRIMARY KEY,
    device_id                     TEXT NOT NULL UNIQUE,
    last_pushed_sequence          INTEGER NOT NULL DEFAULT 0,
    last_pulled_central_sequence  INTEGER NOT NULL DEFAULT 0,
    last_successful_sync_at       TEXT,
    server_watermark              TEXT
);

-- ─── Seed data: default branch, device, role, and admin user ─────────────────
INSERT OR IGNORE INTO branches(branch_id, branch_code, name, timezone, currency, is_active, created_at, updated_at)
VALUES('01JBRANCH0000000000000001', 'MAIN', 'Main Branch', 'Asia/Bahrain', 'BHD', 1,
       datetime('now'), datetime('now'));

INSERT OR IGNORE INTO devices(device_id, branch_id, device_code, name, status)
VALUES('01JDEVICE0000000000000001', '01JBRANCH0000000000000001', 'POS01', 'POS Terminal 1', 'online');

INSERT OR IGNORE INTO roles(role_id, name, description, is_system_role)
VALUES
    ('01JROLE00000000000OWNER001', 'owner',      'Full business control',     1),
    ('01JROLE00000000000MGMT001',  'manager',    'Branch operational control', 1),
    ('01JROLE00000000000CASH001',  'cashier',    'POS checkout operations',   1),
    ('01JROLE00000000000RPRT001',  'accountant', 'Read-only reports access',  1);

-- Default admin user: username=admin, PIN=0000 (hashed placeholder for Phase 0)
INSERT OR IGNORE INTO users(user_id, display_name, username, pin_hash, role_id, branch_scope, is_active, created_at, updated_at)
VALUES('01JUSER000000000000ADMIN1', 'Admin', 'admin',
       '$argon2id$placeholder$admin0000',
       '01JROLE00000000000OWNER001', '[]', 1, datetime('now'), datetime('now'));

INSERT OR IGNORE INTO users(user_id, display_name, username, pin_hash, role_id, branch_scope, is_active, created_at, updated_at)
VALUES('01JUSER000000000000CASH01', 'Cashier 1', 'cashier1',
       '$argon2id$placeholder$cash0001',
       '01JROLE00000000000CASH001', '[]', 1, datetime('now'), datetime('now'));

-- Default tax rule: 10% VAT exclusive
INSERT OR IGNORE INTO tax_rules(tax_rule_id, name, rate_basis_points, inclusive, is_active, effective_from)
VALUES('01JTAX000000000000VAT001', 'VAT 10%', 1000, 0, 1, '2024-01-01T00:00:00Z');

INSERT OR IGNORE INTO tax_rules(tax_rule_id, name, rate_basis_points, inclusive, is_active, effective_from)
VALUES('01JTAX000000000000ZERO01', 'Zero Rate', 0, 0, 1, '2024-01-01T00:00:00Z');

-- Default categories
INSERT OR IGNORE INTO categories(category_id, name, sort_order, is_active, created_at, updated_at)
VALUES
    ('01JCAT000000000000DRINK01', 'Drinks',    1, 1, datetime('now'), datetime('now')),
    ('01JCAT000000000000FOOD001', 'Food',      2, 1, datetime('now'), datetime('now')),
    ('01JCAT000000000000MISC001', 'Misc',      3, 1, datetime('now'), datetime('now'));

-- Sample products for Phase 0 testing
INSERT OR IGNORE INTO products(product_id, category_id, name, sku, barcode, track_inventory, is_active, tax_rule_id, currency, created_at, updated_at)
VALUES
    ('01JPROD00000000000COLA001', '01JCAT000000000000DRINK01', 'Coca-Cola 330ml', 'COLA-330', '5449000000996', 1, 1, '01JTAX000000000000VAT001', 'BHD', datetime('now'), datetime('now')),
    ('01JPROD00000000000PEPS001', '01JCAT000000000000DRINK01', 'Pepsi 330ml',     'PEPS-330', '6281006480443', 1, 1, '01JTAX000000000000VAT001', 'BHD', datetime('now'), datetime('now')),
    ('01JPROD00000000000WATR001', '01JCAT000000000000DRINK01', 'Water 500ml',     'WATR-500', '6281001511222', 1, 1, '01JTAX000000000000ZERO01', 'BHD', datetime('now'), datetime('now'));

-- Sample prices (minor units: 1.000 BHD = 1000)
INSERT OR IGNORE INTO product_prices(price_id, product_id, price_type, price_minor, currency, effective_from, created_by_user_id, created_at)
VALUES
    ('01JPRICE0000000000COLA001', '01JPROD00000000000COLA001', 'selling', 400, 'BHD', '2024-01-01T00:00:00Z', '01JUSER000000000000ADMIN1', datetime('now')),
    ('01JPRICE0000000000PEPS001', '01JPROD00000000000PEPS001', 'selling', 400, 'BHD', '2024-01-01T00:00:00Z', '01JUSER000000000000ADMIN1', datetime('now')),
    ('01JPRICE0000000000WATR001', '01JPROD00000000000WATR001', 'selling', 250, 'BHD', '2024-01-01T00:00:00Z', '01JUSER000000000000ADMIN1', datetime('now'));

INSERT OR IGNORE INTO sync_state(sync_state_id, device_id)
VALUES('01JSYNCS0000000000000001', '01JDEVICE0000000000000001');
