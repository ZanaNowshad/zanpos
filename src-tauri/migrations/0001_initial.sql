-- ZANPOS v2 — Single clean schema.
-- One file creates everything. No incremental patches, no ALTER TABLE, no legacy baggage.
-- Designed for fresh installs. Old DBs are auto-detected and replaced on startup.

-- ══════════════════════════════════════════════════════════════════════════════
-- CORE: branches, roles, devices, users, app_config
-- ══════════════════════════════════════════════════════════════════════════════

CREATE TABLE branches (
    branch_id      TEXT PRIMARY KEY,
    branch_code    TEXT NOT NULL,
    name           TEXT NOT NULL,
    currency       TEXT NOT NULL DEFAULT 'BHD',
    timezone       TEXT NOT NULL DEFAULT 'Asia/Bahrain',
    address        TEXT,
    phone          TEXT,
    receipt_header TEXT,
    receipt_footer TEXT,
    tax_number     TEXT,
    cr_number      TEXT,
    is_active      INTEGER NOT NULL DEFAULT 1,
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL,
    deleted_at     TEXT,
    version        INTEGER NOT NULL DEFAULT 1,
    sync_status    TEXT NOT NULL DEFAULT 'pending',
    sync_attempts  INTEGER NOT NULL DEFAULT 0
);
CREATE UNIQUE INDEX idx_branches_code ON branches (branch_code) WHERE is_active;

-- sync_status index for fast push queries
CREATE INDEX idx_branches_sync_status ON branches(sync_status);

CREATE TABLE roles (
    role_id    TEXT PRIMARY KEY,
    name       TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL
);

CREATE TABLE devices (
    device_id        TEXT PRIMARY KEY,
    branch_id        TEXT NOT NULL REFERENCES branches(branch_id),
    device_code      TEXT NOT NULL,
    name             TEXT NOT NULL,
    status           TEXT NOT NULL DEFAULT 'online',
    is_active        INTEGER NOT NULL DEFAULT 1,
    next_receipt_seq INTEGER NOT NULL DEFAULT 1,
    last_seen_at     TEXT,
    created_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL,
    deleted_at       TEXT,
    version          INTEGER NOT NULL DEFAULT 1,
    sync_status      TEXT NOT NULL DEFAULT 'pending',
    sync_attempts    INTEGER NOT NULL DEFAULT 0,
    UNIQUE(branch_id, device_code)
);
CREATE INDEX idx_devices_branch ON devices(branch_id);

-- sync_status index for fast push queries
CREATE INDEX idx_devices_sync_status ON devices(sync_status);

CREATE TABLE users (
    user_id              TEXT PRIMARY KEY,
    branch_id            TEXT NOT NULL,
    display_name         TEXT NOT NULL,
    username             TEXT NOT NULL UNIQUE,
    pin_hash             TEXT NOT NULL,
    role_id              TEXT NOT NULL REFERENCES roles(role_id),
    branch_scope         TEXT NOT NULL DEFAULT '[]',
    is_active            INTEGER NOT NULL DEFAULT 1,
    failed_pin_attempts  INTEGER NOT NULL DEFAULT 0,
    locked_until         TEXT,
    last_login_at        TEXT,
    created_at           TEXT NOT NULL,
    updated_at           TEXT NOT NULL,
    deleted_at           TEXT,
    version              INTEGER NOT NULL DEFAULT 1,
    sync_status          TEXT NOT NULL DEFAULT 'pending',
    sync_attempts        INTEGER NOT NULL DEFAULT 0
    -- pin_hash is NEVER synced — stripped from push payload, excluded from pull SET,
    -- absent from central PostgreSQL
);
CREATE INDEX idx_users_username ON users(username);
CREATE INDEX idx_users_role ON users(role_id);

-- sync_status index for fast push queries
CREATE INDEX idx_users_sync_status ON users(sync_status);

CREATE TABLE app_config (
    key        TEXT PRIMARY KEY,
    value      TEXT NOT NULL DEFAULT '',
    updated_at TEXT NOT NULL
);

-- ══════════════════════════════════════════════════════════════════════════════
-- CATALOG: categories, tax_rules, products, product_prices, product_barcodes
-- ══════════════════════════════════════════════════════════════════════════════

CREATE TABLE categories (
    category_id        TEXT PRIMARY KEY,
    parent_category_id TEXT,
    name               TEXT NOT NULL,
    sort_order         INTEGER NOT NULL DEFAULT 0,
    is_active          INTEGER NOT NULL DEFAULT 1,
    created_at         TEXT NOT NULL,
    updated_at         TEXT NOT NULL,
    deleted_at         TEXT,
    version            INTEGER NOT NULL DEFAULT 1,
    sync_status        TEXT NOT NULL DEFAULT 'pending',
    sync_attempts      INTEGER NOT NULL DEFAULT 0
);

-- sync_status index for fast push queries
CREATE INDEX idx_categories_sync_status ON categories(sync_status);

CREATE TABLE tax_rules (
    tax_rule_id       TEXT PRIMARY KEY,
    name              TEXT NOT NULL,
    rate_basis_points INTEGER NOT NULL DEFAULT 0,
    inclusive         INTEGER NOT NULL DEFAULT 0,
    is_active         INTEGER NOT NULL DEFAULT 1,
    effective_from    TEXT NOT NULL,
    effective_to      TEXT,
    created_at        TEXT NOT NULL,
    updated_at        TEXT NOT NULL,
    deleted_at        TEXT,
    version           INTEGER NOT NULL DEFAULT 1,
    sync_status       TEXT NOT NULL DEFAULT 'pending',
    sync_attempts     INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_tax_rules_active ON tax_rules(is_active);

-- sync_status index for fast push queries
CREATE INDEX idx_tax_rules_sync_status ON tax_rules(sync_status);

CREATE TABLE products (
    product_id              TEXT PRIMARY KEY,
    category_id             TEXT NOT NULL REFERENCES categories(category_id),
    name                    TEXT NOT NULL,
    sku                     TEXT,
    barcode                 TEXT,
    description             TEXT,
    track_inventory         INTEGER NOT NULL DEFAULT 1,
    allow_decimal_quantity  INTEGER NOT NULL DEFAULT 0,
    is_active               INTEGER NOT NULL DEFAULT 1,
    tax_rule_id             TEXT REFERENCES tax_rules(tax_rule_id),
    cost_minor              INTEGER,
    currency                TEXT NOT NULL DEFAULT 'BHD',
    reorder_point           INTEGER NOT NULL DEFAULT 0,
    image_path              TEXT,
    default_supplier_id     TEXT,
    created_at              TEXT NOT NULL,
    updated_at              TEXT NOT NULL,
    deleted_at              TEXT,
    version                 INTEGER NOT NULL DEFAULT 1,
    sync_status             TEXT NOT NULL DEFAULT 'pending',
    sync_attempts           INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_products_category ON products(category_id);
CREATE INDEX idx_products_barcode  ON products(barcode);
CREATE INDEX idx_products_name     ON products(name);
CREATE INDEX idx_products_sku      ON products(sku);

-- sync_status index for fast push queries
CREATE INDEX idx_products_sync_status ON products(sync_status);

CREATE TABLE product_prices (
    price_id              TEXT PRIMARY KEY,
    product_id            TEXT NOT NULL REFERENCES products(product_id),
    branch_id             TEXT,
    price_type            TEXT NOT NULL DEFAULT 'selling',
    price_minor           INTEGER NOT NULL,
    currency              TEXT NOT NULL DEFAULT 'BHD',
    effective_from        TEXT NOT NULL,
    effective_to          TEXT,
    created_by_user_id    TEXT NOT NULL,
    created_by_ai_action_id TEXT,
    created_at            TEXT NOT NULL,
    sync_status           TEXT NOT NULL DEFAULT 'pending',
    sync_attempts         INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_prices_product   ON product_prices(product_id);
CREATE INDEX idx_prices_effective ON product_prices(product_id, effective_from);

-- sync_status index for fast push queries
CREATE INDEX idx_product_prices_sync_status ON product_prices(sync_status);

CREATE TABLE product_barcodes (
    barcode_id TEXT,
    product_id TEXT NOT NULL REFERENCES products(product_id),
    barcode    TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (product_id, barcode)
);

-- ══════════════════════════════════════════════════════════════════════════════
-- INVENTORY: stock_levels, stock_movements
-- ══════════════════════════════════════════════════════════════════════════════

CREATE TABLE stock_levels (
    stock_level_id   TEXT PRIMARY KEY,
    product_id       TEXT NOT NULL REFERENCES products(product_id),
    branch_id        TEXT NOT NULL,
    quantity_on_hand TEXT NOT NULL DEFAULT '0',
    last_movement_at TEXT,
    created_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL,
    sync_status      TEXT NOT NULL DEFAULT 'pending',
    sync_attempts    INTEGER NOT NULL DEFAULT 0,
    UNIQUE(product_id, branch_id)
);
CREATE INDEX idx_stock_levels_product ON stock_levels(product_id);
CREATE INDEX idx_stock_levels_branch  ON stock_levels(branch_id);

-- sync_status index for fast push queries
CREATE INDEX idx_stock_levels_sync_status ON stock_levels(sync_status);

CREATE TABLE stock_movements (
    movement_id       TEXT PRIMARY KEY,
    product_id        TEXT NOT NULL REFERENCES products(product_id),
    branch_id         TEXT NOT NULL,
    device_id         TEXT NOT NULL,
    origin_device_id  TEXT NOT NULL DEFAULT '',
    movement_type     TEXT NOT NULL,
    quantity_delta    TEXT NOT NULL,
    quantity_after    TEXT NOT NULL,
    reference_type    TEXT,
    reference_id      TEXT,
    notes             TEXT,
    created_by_user_id TEXT,
    created_at        TEXT NOT NULL,
    sync_status       TEXT NOT NULL DEFAULT 'pending',
    sync_attempts     INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_stock_movements_product ON stock_movements(product_id);
CREATE INDEX idx_stock_movements_created ON stock_movements(created_at);

-- sync_status index for fast push queries
CREATE INDEX idx_stock_movements_sync_status ON stock_movements(sync_status);

-- ══════════════════════════════════════════════════════════════════════════════
-- CUSTOMERS
-- ══════════════════════════════════════════════════════════════════════════════

CREATE TABLE customers (
    customer_id    TEXT PRIMARY KEY,
    branch_id      TEXT NOT NULL,
    origin_device_id TEXT NOT NULL DEFAULT '',
    name           TEXT NOT NULL,
    phone          TEXT,
    email          TEXT,
    loyalty_points INTEGER NOT NULL DEFAULT 0,
    notes          TEXT,
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL,
    deleted_at     TEXT,
    version        INTEGER NOT NULL DEFAULT 1,
    sync_status    TEXT NOT NULL DEFAULT 'pending',
    sync_attempts  INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_customers_branch ON customers(branch_id);
CREATE INDEX idx_customers_phone  ON customers(phone);

-- sync_status index for fast push queries
CREATE INDEX idx_customers_sync_status ON customers(sync_status);

-- ══════════════════════════════════════════════════════════════════════════════
-- SALES: sales, sale_items, payments
-- ══════════════════════════════════════════════════════════════════════════════

CREATE TABLE sales (
    sale_id             TEXT PRIMARY KEY,
    receipt_number      TEXT NOT NULL UNIQUE,
    branch_id           TEXT NOT NULL,
    device_id           TEXT NOT NULL,
    origin_device_id    TEXT NOT NULL DEFAULT '',
    shift_id            TEXT NOT NULL,
    cashier_user_id     TEXT NOT NULL,
    status              TEXT NOT NULL DEFAULT 'completed',
    gross_total_minor   INTEGER NOT NULL DEFAULT 0,
    discount_total_minor INTEGER NOT NULL DEFAULT 0,
    tax_total_minor     INTEGER NOT NULL DEFAULT 0,
    net_total_minor     INTEGER NOT NULL DEFAULT 0,
    currency            TEXT NOT NULL DEFAULT 'BHD',
    business_date       TEXT NOT NULL,
    sold_at             TEXT NOT NULL,
    created_offline     INTEGER NOT NULL DEFAULT 0,
    idempotency_key     TEXT NOT NULL UNIQUE,
    customer_id         TEXT,
    is_delivery         INTEGER NOT NULL DEFAULT 0,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    sync_status         TEXT NOT NULL DEFAULT 'pending',
    sync_attempts       INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_sales_shift         ON sales(shift_id);
CREATE INDEX idx_sales_business_date ON sales(business_date);
CREATE INDEX idx_sales_receipt       ON sales(receipt_number);

-- sync_status index for fast push queries
CREATE INDEX idx_sales_sync_status ON sales(sync_status);

CREATE TABLE sale_items (
    sale_item_id          TEXT PRIMARY KEY,
    sale_id               TEXT NOT NULL REFERENCES sales(sale_id),
    product_id            TEXT,
    product_name_snapshot TEXT NOT NULL,
    sku_snapshot          TEXT,
    barcode_snapshot      TEXT,
    quantity              TEXT NOT NULL,
    unit_price_minor      INTEGER NOT NULL,
    line_discount_minor   INTEGER NOT NULL DEFAULT 0,
    tax_rule_snapshot     TEXT NOT NULL DEFAULT '{}',
    tax_amount_minor      INTEGER NOT NULL DEFAULT 0,
    line_total_minor      INTEGER NOT NULL DEFAULT 0,
    note                  TEXT,
    voided                INTEGER NOT NULL DEFAULT 0,
    refunded_amount_minor INTEGER NOT NULL DEFAULT 0,
    origin_device_id      TEXT NOT NULL DEFAULT '',
    created_at            TEXT NOT NULL,
    updated_at            TEXT NOT NULL,
    sync_status           TEXT NOT NULL DEFAULT 'pending',
    sync_attempts         INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_sale_items_sale ON sale_items(sale_id);

-- sync_status index for fast push queries
CREATE INDEX idx_sale_items_sync_status ON sale_items(sync_status);

CREATE TABLE payments (
    payment_id          TEXT PRIMARY KEY,
    sale_id             TEXT NOT NULL REFERENCES sales(sale_id),
    origin_device_id    TEXT NOT NULL DEFAULT '',
    payment_method      TEXT NOT NULL CHECK(payment_method IN ('cash','card','wallet','other')),
    amount_minor        INTEGER NOT NULL,
    currency            TEXT NOT NULL DEFAULT 'BHD',
    status              TEXT NOT NULL DEFAULT 'approved',
    external_reference  TEXT,
    tendered_minor      INTEGER,
    change_minor        INTEGER,
    recorded_by_user_id TEXT NOT NULL,
    recorded_at         TEXT NOT NULL,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    sync_status         TEXT NOT NULL DEFAULT 'pending',
    sync_attempts       INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_payments_sale ON payments(sale_id);

-- sync_status index for fast push queries
CREATE INDEX idx_payments_sync_status ON payments(sync_status);

-- ══════════════════════════════════════════════════════════════════════════════
-- REFUNDS
-- ══════════════════════════════════════════════════════════════════════════════

CREATE TABLE refunds (
    refund_id              TEXT PRIMARY KEY,
    original_sale_id       TEXT NOT NULL,
    origin_device_id       TEXT NOT NULL DEFAULT '',
    refund_receipt_number  TEXT NOT NULL UNIQUE,
    reason                 TEXT NOT NULL DEFAULT '',
    return_reason_code     TEXT NOT NULL DEFAULT 'other',
    refund_total_minor     INTEGER NOT NULL,
    currency               TEXT NOT NULL DEFAULT 'BHD',
    created_by_user_id     TEXT NOT NULL,
    idempotency_key        TEXT NOT NULL UNIQUE,
    created_at             TEXT NOT NULL,
    updated_at             TEXT NOT NULL,
    sync_status            TEXT NOT NULL DEFAULT 'pending',
    sync_attempts          INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_refunds_sale ON refunds(original_sale_id);

-- sync_status index for fast push queries
CREATE INDEX idx_refunds_sync_status ON refunds(sync_status);

CREATE TABLE refund_items (
    refund_item_id        TEXT PRIMARY KEY,
    refund_id             TEXT NOT NULL REFERENCES refunds(refund_id),
    origin_device_id      TEXT NOT NULL DEFAULT '',
    sale_item_id          TEXT NOT NULL,
    product_name_snapshot TEXT NOT NULL,
    quantity              TEXT NOT NULL,
    unit_price_minor      INTEGER NOT NULL DEFAULT 0,
    refund_amount_minor   INTEGER NOT NULL,
    created_at            TEXT NOT NULL,
    updated_at            TEXT NOT NULL,
    sync_status           TEXT NOT NULL DEFAULT 'pending',
    sync_attempts         INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_refund_items_refund ON refund_items(refund_id);

-- sync_status index for fast push queries
CREATE INDEX idx_refund_items_sync_status ON refund_items(sync_status);

-- ══════════════════════════════════════════════════════════════════════════════
-- DELIVERY
-- ══════════════════════════════════════════════════════════════════════════════

CREATE TABLE delivery_orders (
    delivery_id              TEXT PRIMARY KEY,
    sale_id                  TEXT NOT NULL REFERENCES sales(sale_id),
    receipt_number           TEXT NOT NULL,
    branch_id                TEXT NOT NULL,
    device_id                TEXT NOT NULL,
    origin_device_id         TEXT NOT NULL DEFAULT '',
    customer_id              TEXT,
    customer_name            TEXT,
    contact_number           TEXT NOT NULL,
    address_text             TEXT,
    house_number             TEXT,
    area                     TEXT,
    delivery_status          TEXT NOT NULL DEFAULT 'pending',
    delivery_staff_name      TEXT,
    delivery_note            TEXT,
    expected_payment_method  TEXT NOT NULL DEFAULT 'cash',
    payment_status           TEXT NOT NULL DEFAULT 'unpaid',
    amount_minor             INTEGER NOT NULL DEFAULT 0,
    currency                 TEXT NOT NULL DEFAULT 'BHD',
    paid_confirmed_by_user_id TEXT,
    paid_confirmed_at        TEXT,
    payment_reference        TEXT,
    payment_note             TEXT,
    created_by_user_id       TEXT,
    created_at               TEXT NOT NULL,
    updated_at               TEXT NOT NULL,
    version                  INTEGER NOT NULL DEFAULT 1,
    sync_status              TEXT NOT NULL DEFAULT 'pending',
    sync_attempts            INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_delivery_branch ON delivery_orders(branch_id, delivery_status);
CREATE INDEX idx_delivery_sale   ON delivery_orders(sale_id);

-- sync_status index for fast push queries
CREATE INDEX idx_delivery_orders_sync_status ON delivery_orders(sync_status);

-- ══════════════════════════════════════════════════════════════════════════════
-- SHIFTS & CASH EVENTS
-- ══════════════════════════════════════════════════════════════════════════════

CREATE TABLE shifts (
    shift_id             TEXT PRIMARY KEY,
    branch_id            TEXT NOT NULL,
    device_id            TEXT NOT NULL,
    origin_device_id     TEXT NOT NULL DEFAULT '',
    cashier_user_id      TEXT NOT NULL,
    opened_at            TEXT NOT NULL,
    closed_at            TEXT,
    opening_cash_minor   INTEGER NOT NULL DEFAULT 0,
    counted_cash_minor   INTEGER,
    expected_cash_minor  INTEGER,
    cash_difference_minor INTEGER,
    business_date        TEXT,
    status               TEXT NOT NULL DEFAULT 'open',
    close_notes          TEXT,
    created_at           TEXT NOT NULL,
    updated_at           TEXT NOT NULL,
    deleted_at           TEXT,
    version              INTEGER NOT NULL DEFAULT 1,
    sync_status          TEXT NOT NULL DEFAULT 'pending',
    sync_attempts        INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_shifts_device ON shifts(device_id);
CREATE INDEX idx_shifts_status ON shifts(status);

-- sync_status index for fast push queries
CREATE INDEX idx_shifts_sync_status ON shifts(sync_status);
-- Prevent ghost shifts: only one open shift per device at any time (T01)
CREATE UNIQUE INDEX IF NOT EXISTS idx_shifts_one_open_per_device ON shifts(device_id) WHERE status = 'open';

CREATE TABLE cash_events (
    cash_event_id     TEXT PRIMARY KEY,
    shift_id          TEXT NOT NULL REFERENCES shifts(shift_id),
    branch_id         TEXT,
    device_id         TEXT,
    event_type        TEXT NOT NULL,
    amount_minor      INTEGER,
    note              TEXT,
    created_by_user_id TEXT NOT NULL,
    created_at        TEXT NOT NULL
);
CREATE INDEX idx_cash_events_shift ON cash_events(shift_id);

-- ══════════════════════════════════════════════════════════════════════════════
-- AUDIT & GHOST BARCODES
-- ══════════════════════════════════════════════════════════════════════════════

CREATE TABLE audit_logs (
    audit_log_id    TEXT PRIMARY KEY,
    event_type      TEXT NOT NULL,
    entity_type     TEXT NOT NULL,
    entity_id       TEXT NOT NULL,
    actor_user_id   TEXT,
    actor_type      TEXT NOT NULL DEFAULT 'user',
    ai_action_id    TEXT,
    device_id       TEXT,
    origin_device_id TEXT NOT NULL DEFAULT '',
    branch_id       TEXT,
    before_json     TEXT,
    after_json      TEXT,
    reason          TEXT,
    created_at      TEXT NOT NULL,
    hash            TEXT NOT NULL,
    previous_hash   TEXT,
    override_used   INTEGER NOT NULL DEFAULT 0,
    sync_status     TEXT NOT NULL DEFAULT 'pending',
    sync_attempts   INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_audit_entity  ON audit_logs(entity_type, entity_id);
CREATE INDEX idx_audit_device  ON audit_logs(device_id);
CREATE INDEX idx_audit_created ON audit_logs(created_at);

-- sync_status index for fast push queries
CREATE INDEX idx_audit_logs_sync_status ON audit_logs(sync_status);

CREATE TABLE ghost_barcodes (
    ghost_id             TEXT PRIMARY KEY,
    barcode              TEXT NOT NULL,
    recorded_by_user_id  TEXT NOT NULL,
    status               TEXT NOT NULL DEFAULT 'unresolved',
    product_name         TEXT,
    product_id           TEXT,
    resolved_by_user_id  TEXT,
    resolved_at          TEXT,
    created_at           TEXT NOT NULL
);
CREATE INDEX idx_ghost_barcode ON ghost_barcodes(barcode);

-- Legacy alias for ghost_barcodes (some code references unknown_barcodes)
CREATE TABLE IF NOT EXISTS unknown_barcodes (
    id                  TEXT PRIMARY KEY,
    barcode             TEXT NOT NULL,
    scan_count          INTEGER NOT NULL DEFAULT 1,
    first_seen_at       TEXT,
    last_seen_at        TEXT,
    recorded_by_user_id TEXT,
    status              TEXT DEFAULT 'pending',
    product_name        TEXT,
    product_id          TEXT,
    resolved_by_user_id TEXT,
    resolved_at         TEXT
);

-- ══════════════════════════════════════════════════════════════════════════════
-- SYNC WATERMARK
-- ══════════════════════════════════════════════════════════════════════════════

CREATE TABLE sync_watermark (
    table_name     TEXT PRIMARY KEY,
    last_pulled_at TEXT NOT NULL DEFAULT '1970-01-01T00:00:00Z',
    last_pushed_at TEXT NOT NULL DEFAULT '1970-01-01T00:00:00Z'
);

INSERT INTO sync_watermark (table_name) VALUES
    ('branches'),('devices'),('users'),('products'),('categories'),
    ('tax_rules'),('product_prices'),('customers'),('sales'),
    ('sale_items'),('payments'),('refunds'),('refund_items'),
    ('stock_movements'),('stock_levels'),('audit_logs'),
    ('shifts'),('delivery_orders'),('app_config');

-- ══════════════════════════════════════════════════════════════════════════════
-- LOCAL-ONLY TABLES (never synced)
-- ══════════════════════════════════════════════════════════════════════════════

CREATE TABLE held_carts (
    held_cart_id    TEXT PRIMARY KEY,
    branch_id       TEXT NOT NULL DEFAULT '',
    device_id       TEXT NOT NULL DEFAULT '',
    shift_id        TEXT,
    cashier_user_id TEXT NOT NULL,
    cart_json       TEXT NOT NULL,
    note            TEXT,
    held_at         TEXT NOT NULL
);

CREATE TABLE ai_chat_history (
    message_id      TEXT PRIMARY KEY,
    session_user_id TEXT NOT NULL,
    role            TEXT NOT NULL,
    content         TEXT NOT NULL,
    created_at      TEXT NOT NULL
);

CREATE TABLE ai_actions (
    action_id          TEXT PRIMARY KEY,
    session_user_id    TEXT NOT NULL,
    actor_type         TEXT NOT NULL DEFAULT 'ai',
    tool_name          TEXT NOT NULL,
    tool_input_json    TEXT NOT NULL,
    tool_input_hash    TEXT NOT NULL,
    preview_text       TEXT NOT NULL,
    status             TEXT NOT NULL DEFAULT 'pending',
    confirmation_token TEXT,
    prepared_at        TEXT NOT NULL,
    confirmed_at       TEXT,
    executed_at        TEXT,
    expires_at         TEXT NOT NULL,
    result_json        TEXT,
    error_message      TEXT
);

CREATE TABLE undo_records (
    undo_id         TEXT PRIMARY KEY,
    action_id       TEXT NOT NULL,
    before_json     TEXT NOT NULL,
    created_at      TEXT NOT NULL
);

CREATE TABLE import_history (
    import_id         TEXT PRIMARY KEY,
    source_path       TEXT,
    rows_imported     INTEGER NOT NULL DEFAULT 0,
    created_by_user_id TEXT NOT NULL,
    created_at        TEXT NOT NULL
);

CREATE TABLE no_sale_events (
    id      TEXT PRIMARY KEY,
    user_id TEXT NOT NULL,
    reason  TEXT,
    created_at TEXT NOT NULL
);

-- ══════════════════════════════════════════════════════════════════════════════
-- SEED DATA
-- ══════════════════════════════════════════════════════════════════════════════

INSERT OR IGNORE INTO roles (role_id, name, created_at) VALUES
    ('01JROLES000000000000000001', 'owner',   '2025-01-01T00:00:00Z'),
    ('01JROLES000000000000000002', 'manager', '2025-01-01T00:00:00Z'),
    ('01JROLES000000000000000003', 'cashier', '2025-01-01T00:00:00Z');

INSERT OR IGNORE INTO tax_rules (tax_rule_id, name, rate_basis_points, inclusive, is_active, effective_from, created_at, updated_at, version) VALUES
    ('01JTAX000000000000VAT001', 'VAT 10%',    1000, 0, 1, '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z', 1),
    ('01JTAX000000000000ZERO01', 'Zero-rated',  0,    0, 1, '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z', 1);

INSERT OR IGNORE INTO branches (branch_id, branch_code, name, created_at, updated_at, version)
VALUES ('01JBRANCH0000000000000001', 'MAIN', 'My Store', '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z', 1);

INSERT OR IGNORE INTO users (user_id, branch_id, display_name, username, pin_hash, role_id, is_active, created_at, updated_at, version) VALUES
    ('01JUSERS000000000000000001', '01JBRANCH0000000000000001', 'Admin',     'admin',   'PLAIN:0000', '01JROLES000000000000000001', 0, '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z', 1),
    ('01JUSER000000000000ADMIN1',  '01JBRANCH0000000000000001', 'Admin',     'admin1',  'PLAIN:0000', '01JROLES000000000000000001', 0, '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z', 1),
    ('01JUSER000000000000CASH01',  '01JBRANCH0000000000000001', 'Cashier 1', 'cashier1', 'PLAIN:0000', '01JROLES000000000000000003', 1, '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z', 1);

INSERT OR IGNORE INTO devices (device_id, branch_id, device_code, name, status, is_active, created_at, updated_at, version)
VALUES ('01JDEVICE0000000000000001', '01JBRANCH0000000000000001', 'POS01', 'Main Terminal', 'offline', 1, '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z', 1);

INSERT OR IGNORE INTO app_config (key, value, updated_at) VALUES
    ('flag_allow_negative_stock',  '1', '2025-01-01T00:00:00Z'),
    ('flag_require_discount_reason','1', '2025-01-01T00:00:00Z'),
    ('flag_cashier_can_discount',  '1', '2025-01-01T00:00:00Z'),
    ('flag_auto_print_receipt',    '0', '2025-01-01T00:00:00Z'),
    ('setup_complete',             '0', '2025-01-01T00:00:00Z'),
    ('retention_days_sales',       '90','2025-01-01T00:00:00Z'),
    ('retention_days_logs',        '30','2025-01-01T00:00:00Z');
