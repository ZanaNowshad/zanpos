/// Supabase central PostgreSQL schema.
/// Applied automatically via Management API on first store setup.
/// All DDL uses IF NOT EXISTS / CREATE OR REPLACE so re-running is safe.
pub const CENTRAL_SCHEMA_SQL: &str = r#"

-- ── Store / branch registry ───────────────────────────────────────────────────
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

-- ── Global event log ──────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS sync_events (
    global_sequence  BIGSERIAL PRIMARY KEY,
    device_id        TEXT NOT NULL,
    branch_id        TEXT NOT NULL,
    entity_type      TEXT NOT NULL,
    entity_id        TEXT NOT NULL,
    operation        TEXT NOT NULL,
    payload_json     JSONB NOT NULL,
    payload_hash     TEXT NOT NULL,
    idempotency_key  TEXT NOT NULL UNIQUE,
    local_sequence   BIGINT NOT NULL,
    created_at       TEXT NOT NULL,
    received_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_se_sequence ON sync_events (global_sequence);
CREATE INDEX IF NOT EXISTS idx_se_device   ON sync_events (device_id, local_sequence);
CREATE INDEX IF NOT EXISTS idx_se_entity   ON sync_events (entity_type, entity_id);

-- ── Mutable catalog entities (last-write-wins) ────────────────────────────────
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
    -- F-MED-10: reorder_point and image_path were missing from the central schema
    reorder_point           BIGINT NOT NULL DEFAULT 0,
    image_path              TEXT,
    created_at              TEXT NOT NULL,
    updated_at              TEXT NOT NULL,
    version                 BIGINT NOT NULL DEFAULT 1
);
ALTER TABLE products ADD COLUMN IF NOT EXISTS default_supplier_id TEXT;

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
    -- The apply_sync_event RPC writes updated_at; the column must exist or every
    -- tax_rule push 400s ("column updated_at does not exist"). Idempotent add below
    -- patches projects migrated before this column was added to the CREATE TABLE.
    updated_at         TEXT,
    version            BIGINT NOT NULL DEFAULT 1
);
ALTER TABLE tax_rules ADD COLUMN IF NOT EXISTS updated_at TEXT;

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

-- ── Append-only entities ──────────────────────────────────────────────────────
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

-- ── Inventory tables ─────────────────────────────────────────────────────────
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

-- ── Customers — F-MED-02 ─────────────────────────────────────────────────────
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

-- ── Devices (terminal registry) — multi-terminal visibility ───────────────────
-- Materializes device records so every terminal's "Devices" page can list all
-- terminals on the store. Without this, device events flowed through sync_events
-- but were never queryable as a table.
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

-- ── Delivery orders — F-HIGH-02 ───────────────────────────────────────────────
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

-- ── apply_sync_event RPC ──────────────────────────────────────────────────────
-- Called by every device push. Inserts into sync_events log (idempotent) and
-- applies LWW upsert or append-only insert to the entity table.
CREATE OR REPLACE FUNCTION apply_sync_event(
    p_entity_type   TEXT,
    p_operation     TEXT,
    p_payload       JSONB,
    p_idem_key      TEXT,
    p_payload_hash  TEXT DEFAULT '',
    p_local_seq     BIGINT DEFAULT 0
) RETURNS TEXT
LANGUAGE plpgsql
SECURITY DEFINER
AS $$
DECLARE
    v_entity_id TEXT;
BEGIN
    -- Derive entity_id from the payload using the primary key convention
    v_entity_id := p_payload->>(p_entity_type || '_id');
    -- Some entities have non-standard PKs
    IF v_entity_id IS NULL THEN
        CASE p_entity_type
            WHEN 'sale_item'      THEN v_entity_id := p_payload->>'sale_item_id';
            WHEN 'refund_item'    THEN v_entity_id := p_payload->>'refund_item_id';
            WHEN 'audit_log'      THEN v_entity_id := p_payload->>'audit_log_id';
            WHEN 'product_price'  THEN v_entity_id := p_payload->>'price_id';
            WHEN 'delivery_order' THEN v_entity_id := p_payload->>'delivery_id';
            WHEN 'device'         THEN v_entity_id := p_payload->>'device_id';
            ELSE v_entity_id := 'unknown';
        END CASE;
    END IF;

    -- Insert into global event log (idempotent)
    INSERT INTO sync_events (
        device_id, branch_id, entity_type, entity_id,
        operation, payload_json, payload_hash,
        idempotency_key, local_sequence, created_at
    )
    SELECT
        COALESCE(p_payload->>'device_id', 'unknown'),
        COALESCE(p_payload->>'branch_id', 'unknown'),
        p_entity_type,
        v_entity_id,
        p_operation,
        p_payload,
        COALESCE(NULLIF(p_payload_hash, ''), p_payload->>'payload_hash', ''),
        p_idem_key,
        COALESCE(NULLIF(p_local_seq, 0), (p_payload->>'local_sequence')::BIGINT, 0),
        COALESCE(p_payload->>'created_at', NOW()::TEXT)
    ON CONFLICT (idempotency_key) DO NOTHING;

    -- Apply to entity table
    CASE p_entity_type

        WHEN 'product' THEN
            INSERT INTO products (
                product_id, category_id, name, sku, barcode, description,
                track_inventory, allow_decimal_quantity, is_active, tax_rule_id,
                cost_minor, currency, reorder_point, image_path, default_supplier_id,
                created_at, updated_at, version
            )
            VALUES (
                p_payload->>'product_id', p_payload->>'category_id', p_payload->>'name',
                p_payload->>'sku', p_payload->>'barcode', p_payload->>'description',
                (p_payload->>'track_inventory')::BOOLEAN,
                (p_payload->>'allow_decimal_quantity')::BOOLEAN,
                (p_payload->>'is_active')::BOOLEAN,
                p_payload->>'tax_rule_id',
                (p_payload->>'cost_minor')::BIGINT,
                COALESCE(p_payload->>'currency', 'BHD'),
                COALESCE((p_payload->>'reorder_point')::BIGINT, 0),
                p_payload->>'image_path',
                p_payload->>'default_supplier_id',
                p_payload->>'created_at', p_payload->>'updated_at',
                COALESCE((p_payload->>'version')::BIGINT, 1)
            )
            ON CONFLICT (product_id) DO UPDATE SET
                category_id            = EXCLUDED.category_id,
                name                   = EXCLUDED.name,
                sku                    = EXCLUDED.sku,
                barcode                = EXCLUDED.barcode,
                description            = EXCLUDED.description,
                track_inventory        = EXCLUDED.track_inventory,
                allow_decimal_quantity = EXCLUDED.allow_decimal_quantity,
                is_active              = EXCLUDED.is_active,
                tax_rule_id            = EXCLUDED.tax_rule_id,
                cost_minor             = EXCLUDED.cost_minor,
                currency               = EXCLUDED.currency,
                reorder_point          = EXCLUDED.reorder_point,
                image_path             = EXCLUDED.image_path,
                default_supplier_id    = EXCLUDED.default_supplier_id,
                updated_at             = EXCLUDED.updated_at,
                version                = EXCLUDED.version
            WHERE products.updated_at < EXCLUDED.updated_at;

        WHEN 'product_price' THEN
            INSERT INTO product_prices (
                price_id, product_id, branch_id, price_type, price_minor, currency,
                effective_from, effective_to, created_by_user_id, created_by_ai_action_id, created_at
            )
            VALUES (
                p_payload->>'price_id', p_payload->>'product_id', p_payload->>'branch_id',
                COALESCE(p_payload->>'price_type', 'selling'),
                (p_payload->>'price_minor')::BIGINT,
                COALESCE(p_payload->>'currency', 'BHD'),
                p_payload->>'effective_from', p_payload->>'effective_to',
                p_payload->>'created_by_user_id', p_payload->>'created_by_ai_action_id',
                p_payload->>'created_at'
            )
            ON CONFLICT (price_id) DO NOTHING;

        WHEN 'shift' THEN
            INSERT INTO shifts (
                shift_id, branch_id, device_id, origin_device_id, cashier_user_id,
                opened_at, closed_at, opening_cash_minor, counted_cash_minor,
                status, close_notes, updated_at
            )
            VALUES (
                p_payload->>'shift_id', p_payload->>'branch_id', p_payload->>'device_id',
                COALESCE(NULLIF(p_payload->>'origin_device_id', ''), p_payload->>'device_id'),
                p_payload->>'cashier_user_id',
                p_payload->>'opened_at', p_payload->>'closed_at',
                COALESCE((p_payload->>'opening_cash_minor')::BIGINT, 0),
                (p_payload->>'counted_cash_minor')::BIGINT,
                COALESCE(p_payload->>'status', 'open'),
                p_payload->>'close_notes',
                COALESCE(p_payload->>'updated_at', p_payload->>'opened_at', '')
            )
            ON CONFLICT (shift_id) DO UPDATE SET
                closed_at          = EXCLUDED.closed_at,
                counted_cash_minor = EXCLUDED.counted_cash_minor,
                status             = EXCLUDED.status,
                close_notes        = EXCLUDED.close_notes,
                updated_at         = EXCLUDED.updated_at
            WHERE shifts.updated_at < EXCLUDED.updated_at;

        WHEN 'sale' THEN
            INSERT INTO sales (
                sale_id, receipt_number, branch_id, device_id, origin_device_id, shift_id,
                cashier_user_id, status, gross_total_minor, discount_total_minor,
                tax_total_minor, net_total_minor, currency, business_date,
                sold_at, created_offline, idempotency_key, customer_id, is_delivery
            )
            VALUES (
                p_payload->>'sale_id', p_payload->>'receipt_number',
                p_payload->>'branch_id', p_payload->>'device_id',
                COALESCE(NULLIF(p_payload->>'origin_device_id', ''), p_payload->>'device_id'),
                p_payload->>'shift_id',
                p_payload->>'cashier_user_id',
                COALESCE(p_payload->>'status', 'completed'),
                COALESCE((p_payload->>'gross_total_minor')::BIGINT, 0),
                COALESCE((p_payload->>'discount_total_minor')::BIGINT, 0),
                COALESCE((p_payload->>'tax_total_minor')::BIGINT, 0),
                COALESCE((p_payload->>'net_total_minor')::BIGINT, 0),
                COALESCE(p_payload->>'currency', 'BHD'),
                p_payload->>'business_date', p_payload->>'sold_at',
                COALESCE((p_payload->>'created_offline')::BOOLEAN, FALSE),
                p_payload->>'idempotency_key',
                NULLIF(p_payload->>'customer_id', ''),
                COALESCE((p_payload->>'is_delivery')::BOOLEAN, FALSE)
            )
            ON CONFLICT (sale_id) DO NOTHING;

        WHEN 'sale_item' THEN
            INSERT INTO sale_items (
                sale_item_id, sale_id, origin_device_id, product_id,
                product_name_snapshot, sku_snapshot, barcode_snapshot, quantity,
                unit_price_minor, line_discount_minor, tax_rule_snapshot,
                tax_amount_minor, line_total_minor, note, voided
            )
            VALUES (
                p_payload->>'sale_item_id', p_payload->>'sale_id',
                COALESCE(NULLIF(p_payload->>'origin_device_id', ''), COALESCE(p_payload->>'device_id', 'unknown')),
                p_payload->>'product_id', p_payload->>'product_name_snapshot',
                p_payload->>'sku_snapshot', p_payload->>'barcode_snapshot',
                p_payload->>'quantity',
                COALESCE((p_payload->>'unit_price_minor')::BIGINT, 0),
                COALESCE((p_payload->>'line_discount_minor')::BIGINT, 0),
                COALESCE(p_payload->>'tax_rule_snapshot', '{}'),
                COALESCE((p_payload->>'tax_amount_minor')::BIGINT, 0),
                COALESCE((p_payload->>'line_total_minor')::BIGINT, 0),
                p_payload->>'note',
                COALESCE((p_payload->>'voided')::BOOLEAN, FALSE)
            )
            ON CONFLICT (sale_item_id) DO NOTHING;

        WHEN 'payment' THEN
            INSERT INTO payments (
                payment_id, sale_id, origin_device_id, payment_method, amount_minor, currency,
                status, external_reference, tendered_minor, change_minor,
                recorded_by_user_id, recorded_at
            )
            VALUES (
                p_payload->>'payment_id', p_payload->>'sale_id',
                COALESCE(NULLIF(p_payload->>'origin_device_id', ''), p_payload->>'device_id'),
                p_payload->>'payment_method',
                COALESCE((p_payload->>'amount_minor')::BIGINT, 0),
                COALESCE(p_payload->>'currency', 'BHD'),
                COALESCE(p_payload->>'status', 'approved'),
                p_payload->>'external_reference',
                (p_payload->>'tendered_minor')::BIGINT,
                (p_payload->>'change_minor')::BIGINT,
                p_payload->>'recorded_by_user_id', p_payload->>'recorded_at'
            )
            ON CONFLICT (payment_id) DO NOTHING;

        WHEN 'refund' THEN
            INSERT INTO refunds (
                refund_id, original_sale_id, origin_device_id, refund_receipt_number, reason,
                refund_total_minor, currency, created_by_user_id, created_at, idempotency_key
            )
            VALUES (
                p_payload->>'refund_id', p_payload->>'original_sale_id',
                COALESCE(NULLIF(p_payload->>'origin_device_id', ''), p_payload->>'device_id'),
                p_payload->>'refund_receipt_number',
                COALESCE(p_payload->>'reason', ''),
                COALESCE((p_payload->>'refund_total_minor')::BIGINT, 0),
                COALESCE(p_payload->>'currency', 'BHD'),
                p_payload->>'created_by_user_id', p_payload->>'created_at',
                p_payload->>'idempotency_key'
            )
            ON CONFLICT (refund_id) DO NOTHING;

        WHEN 'refund_item' THEN
            INSERT INTO refund_items (
                refund_item_id, refund_id, origin_device_id, sale_item_id, product_name_snapshot,
                quantity, unit_price_minor, refund_amount_minor
            )
            VALUES (
                p_payload->>'refund_item_id', p_payload->>'refund_id',
                COALESCE(NULLIF(p_payload->>'origin_device_id', ''), p_payload->>'device_id'),
                p_payload->>'sale_item_id', p_payload->>'product_name_snapshot',
                p_payload->>'quantity',
                COALESCE((p_payload->>'unit_price_minor')::BIGINT, 0),
                COALESCE((p_payload->>'refund_amount_minor')::BIGINT, 0)
            )
            ON CONFLICT (refund_item_id) DO NOTHING;

        WHEN 'audit_log' THEN
            INSERT INTO audit_logs (
                audit_log_id, event_type, entity_type, entity_id,
                actor_user_id, actor_type, ai_action_id, device_id, origin_device_id, branch_id,
                before_json, after_json, reason, created_at, hash, previous_hash
            )
            VALUES (
                p_payload->>'audit_log_id', p_payload->>'event_type',
                p_payload->>'entity_type', p_payload->>'entity_id',
                p_payload->>'actor_user_id',
                COALESCE(p_payload->>'actor_type', 'user'),
                p_payload->>'ai_action_id', p_payload->>'device_id',
                COALESCE(NULLIF(p_payload->>'origin_device_id', ''), NULLIF(p_payload->>'device_id', '')),
                p_payload->>'branch_id',
                p_payload->>'before_json', p_payload->>'after_json', p_payload->>'reason',
                p_payload->>'created_at',
                COALESCE(p_payload->>'hash', ''),
                p_payload->>'previous_hash'
            )
            ON CONFLICT (audit_log_id) DO NOTHING;

        WHEN 'stock_level' THEN
            INSERT INTO stock_levels (
                stock_level_id, product_id, branch_id, quantity_on_hand, last_movement_at, updated_at
            )
            -- H4: stock_level_id must include branch_id to match the local SQLite format
            -- 'SL-{product_id}' would collide for the same product across branches.
            VALUES (
                'SL-' || (p_payload->>'product_id') || '-' || COALESCE(p_payload->>'branch_id', 'unknown'),
                p_payload->>'product_id',
                COALESCE(p_payload->>'branch_id', 'unknown'),
                COALESCE(p_payload->>'quantity_on_hand', '0'),
                p_payload->>'last_movement_at',
                p_payload->>'updated_at'
            )
            ON CONFLICT (product_id, branch_id) DO UPDATE SET
                quantity_on_hand = EXCLUDED.quantity_on_hand,
                last_movement_at = EXCLUDED.last_movement_at,
                updated_at       = EXCLUDED.updated_at
            WHERE stock_levels.updated_at < EXCLUDED.updated_at;

        WHEN 'stock_movement' THEN
            INSERT INTO stock_movements (
                movement_id, product_id, branch_id, device_id, origin_device_id, movement_type,
                quantity_delta, quantity_after, reference_type, reference_id,
                notes, created_by_user_id, created_at
            )
            VALUES (
                p_payload->>'movement_id', p_payload->>'product_id',
                COALESCE(p_payload->>'branch_id', 'unknown'),
                COALESCE(p_payload->>'device_id', 'unknown'),
                COALESCE(NULLIF(p_payload->>'origin_device_id', ''), p_payload->>'device_id'),
                p_payload->>'movement_type',
                p_payload->>'quantity_delta', p_payload->>'quantity_after',
                p_payload->>'reference_type', p_payload->>'reference_id',
                p_payload->>'notes', p_payload->>'created_by_user_id',
                p_payload->>'created_at'
            )
            ON CONFLICT (movement_id) DO NOTHING;

        -- F-HIGH-06: category, tax_rule, user were missing — catalog never materialized in central DB
        WHEN 'category' THEN
            INSERT INTO categories (
                category_id, parent_category_id, name, sort_order, is_active, created_at, updated_at, version
            )
            VALUES (
                p_payload->>'category_id', p_payload->>'parent_category_id',
                p_payload->>'name',
                COALESCE((p_payload->>'sort_order')::BIGINT, 0),
                COALESCE((p_payload->>'is_active')::BOOLEAN, TRUE),
                p_payload->>'created_at', p_payload->>'updated_at',
                COALESCE((p_payload->>'version')::BIGINT, 1)
            )
            ON CONFLICT (category_id) DO UPDATE SET
                name               = EXCLUDED.name,
                sort_order         = EXCLUDED.sort_order,
                is_active          = EXCLUDED.is_active,
                parent_category_id = EXCLUDED.parent_category_id,
                updated_at         = EXCLUDED.updated_at,
                version            = EXCLUDED.version
            WHERE categories.updated_at < EXCLUDED.updated_at;

        WHEN 'tax_rule' THEN
            INSERT INTO tax_rules (
                tax_rule_id, name, rate_basis_points, inclusive, is_active,
                effective_from, effective_to, updated_at, version
            )
            VALUES (
                p_payload->>'tax_rule_id', p_payload->>'name',
                COALESCE((p_payload->>'rate_basis_points')::BIGINT, 0),
                COALESCE((p_payload->>'inclusive')::BOOLEAN, FALSE),
                COALESCE((p_payload->>'is_active')::BOOLEAN, TRUE),
                p_payload->>'effective_from', p_payload->>'effective_to',
                COALESCE(p_payload->>'updated_at', p_payload->>'effective_from'),
                COALESCE((p_payload->>'version')::BIGINT, 1)
            )
            ON CONFLICT (tax_rule_id) DO UPDATE SET
                name              = EXCLUDED.name,
                rate_basis_points = EXCLUDED.rate_basis_points,
                inclusive         = EXCLUDED.inclusive,
                is_active         = EXCLUDED.is_active,
                effective_to      = EXCLUDED.effective_to,
                updated_at        = EXCLUDED.updated_at,
                version           = EXCLUDED.version
            WHERE COALESCE(tax_rules.updated_at, tax_rules.effective_from) < EXCLUDED.updated_at;

        WHEN 'user' THEN
            INSERT INTO users (
                user_id, display_name, username, role_id, branch_scope,
                is_active, created_at, updated_at, version
            )
            VALUES (
                p_payload->>'user_id', p_payload->>'display_name', p_payload->>'username',
                p_payload->>'role_id', COALESCE(p_payload->>'branch_scope', '[]'),
                COALESCE((p_payload->>'is_active')::BOOLEAN, TRUE),
                p_payload->>'created_at', p_payload->>'updated_at',
                COALESCE((p_payload->>'version')::BIGINT, 1)
            )
            ON CONFLICT (user_id) DO UPDATE SET
                display_name = EXCLUDED.display_name,
                username     = EXCLUDED.username,
                role_id      = EXCLUDED.role_id,
                branch_scope = EXCLUDED.branch_scope,
                is_active    = EXCLUDED.is_active,
                updated_at   = EXCLUDED.updated_at,
                version      = EXCLUDED.version
            WHERE users.updated_at < EXCLUDED.updated_at;

        -- F-MED-02: customers table was missing from central DB
        WHEN 'customer' THEN
            INSERT INTO customers (
                customer_id, branch_id, name, phone, email, loyalty_points, notes,
                created_at, updated_at
            )
            VALUES (
                p_payload->>'customer_id', p_payload->>'branch_id', p_payload->>'name',
                p_payload->>'phone', p_payload->>'email',
                COALESCE((p_payload->>'loyalty_points')::BIGINT, 0),
                p_payload->>'notes', p_payload->>'created_at',
                COALESCE(p_payload->>'updated_at', p_payload->>'created_at')
            )
            ON CONFLICT (customer_id) DO UPDATE SET
                name           = EXCLUDED.name,
                phone          = EXCLUDED.phone,
                email          = EXCLUDED.email,
                loyalty_points = GREATEST(customers.loyalty_points, EXCLUDED.loyalty_points),
                notes          = EXCLUDED.notes,
                updated_at     = EXCLUDED.updated_at
            WHERE customers.updated_at < EXCLUDED.updated_at;

        -- Device registry — materialize so terminals can list each other.
        WHEN 'device' THEN
            INSERT INTO devices (
                device_id, branch_id, device_code, name, status, is_active, updated_at
            )
            VALUES (
                p_payload->>'device_id', p_payload->>'branch_id',
                p_payload->>'device_code', p_payload->>'name',
                COALESCE(p_payload->>'status', 'online'),
                COALESCE((p_payload->>'is_active')::BOOLEAN, TRUE),
                COALESCE(p_payload->>'updated_at', NOW()::TEXT)
            )
            ON CONFLICT (device_id) DO UPDATE SET
                name        = EXCLUDED.name,
                device_code = EXCLUDED.device_code,
                status      = EXCLUDED.status,
                is_active   = EXCLUDED.is_active,
                updated_at  = EXCLUDED.updated_at
            WHERE devices.updated_at IS NULL
               OR devices.updated_at < EXCLUDED.updated_at;

        -- F-HIGH-02: delivery_order — materialize from outbox payload (local column names)
        WHEN 'delivery_order' THEN
            INSERT INTO delivery_orders (
                delivery_id, sale_id, receipt_number, branch_id, device_id, origin_device_id,
                customer_id, customer_name, contact_number, address_text, house_number, area,
                delivery_status, delivery_staff_name, delivery_note,
                expected_payment_method, payment_status,
                amount_minor, currency, paid_confirmed_at,
                created_by_user_id, created_at, updated_at
            )
            VALUES (
                p_payload->>'delivery_id', p_payload->>'sale_id',
                COALESCE(p_payload->>'receipt_number', ''),
                p_payload->>'branch_id', p_payload->>'device_id',
                COALESCE(NULLIF(p_payload->>'origin_device_id', ''), p_payload->>'device_id'),
                p_payload->>'customer_id', p_payload->>'customer_name',
                COALESCE(p_payload->>'contact_number', ''),
                p_payload->>'address_text',
                p_payload->>'house_number', p_payload->>'area',
                COALESCE(p_payload->>'delivery_status', 'pending'),
                p_payload->>'delivery_staff_name', p_payload->>'delivery_note',
                COALESCE(p_payload->>'expected_payment_method', 'cash'),
                COALESCE(p_payload->>'payment_status', 'unpaid'),
                COALESCE((p_payload->>'amount_minor')::BIGINT, 0),
                COALESCE(p_payload->>'currency', 'BHD'),
                p_payload->>'paid_confirmed_at',
                COALESCE(p_payload->>'created_by_user_id', ''),
                p_payload->>'created_at', p_payload->>'updated_at'
            )
            ON CONFLICT (delivery_id) DO UPDATE SET
                delivery_status         = EXCLUDED.delivery_status,
                delivery_staff_name     = EXCLUDED.delivery_staff_name,
                delivery_note           = EXCLUDED.delivery_note,
                payment_status          = EXCLUDED.payment_status,
                expected_payment_method = EXCLUDED.expected_payment_method,
                paid_confirmed_at       = EXCLUDED.paid_confirmed_at,
                updated_at              = EXCLUDED.updated_at
            WHERE delivery_orders.updated_at < EXCLUDED.updated_at;

        ELSE
            -- Unknown entity type: still logged in sync_events, just skip entity apply
            NULL;
    END CASE;
    RETURN 'ok';
END;
$$;

-- ── Permissions ───────────────────────────────────────────────────────────────
-- Fresh Supabase projects ship with a locked-down public schema. Without explicit
-- grants, PostgREST returns "403 permission denied for schema public" on every
-- table read and every RPC call — the clean-install blocker. These grants are
-- idempotent and cover tables, sequences, and routines created by this migration.
-- ALTER DEFAULT PRIVILEGES ensures future tables/functions (created by re-running
-- the migration) also get the right grants.
GRANT USAGE ON SCHEMA public TO anon, authenticated, service_role;
GRANT ALL ON ALL TABLES IN SCHEMA public TO anon, authenticated, service_role;
GRANT ALL ON ALL SEQUENCES IN SCHEMA public TO anon, authenticated, service_role;
GRANT ALL ON ALL ROUTINES IN SCHEMA public TO anon, authenticated, service_role;

-- Revoke destructive permissions on append-only tables
REVOKE UPDATE, DELETE ON audit_logs FROM anon;
REVOKE UPDATE, DELETE ON audit_logs FROM authenticated;
GRANT SELECT, INSERT ON audit_logs TO anon;
GRANT SELECT, INSERT ON audit_logs TO authenticated;

ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON TABLES   TO anon, authenticated, service_role;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON SEQUENCES TO anon, authenticated, service_role;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON ROUTINES  TO anon, authenticated, service_role;

"#;
