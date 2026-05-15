/// Supabase central PostgreSQL schema.
/// Applied automatically via Management API on first store setup.
/// All DDL uses IF NOT EXISTS / CREATE OR REPLACE so re-running is safe.
pub const CENTRAL_SCHEMA_SQL: &str = r#"

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
    created_at              TEXT NOT NULL,
    updated_at              TEXT NOT NULL,
    version                 BIGINT NOT NULL DEFAULT 1
);

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
    version            BIGINT NOT NULL DEFAULT 1
);

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
    cashier_user_id       TEXT NOT NULL,
    opened_at             TEXT NOT NULL,
    closed_at             TEXT,
    opening_cash_minor    BIGINT NOT NULL DEFAULT 0,
    counted_cash_minor    BIGINT,
    status                TEXT NOT NULL DEFAULT 'open',
    close_notes           TEXT,
    updated_at            TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS sales (
    sale_id              TEXT PRIMARY KEY,
    receipt_number       TEXT NOT NULL,
    branch_id            TEXT NOT NULL,
    device_id            TEXT NOT NULL,
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
    idempotency_key      TEXT NOT NULL UNIQUE
);

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
    voided                BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE TABLE IF NOT EXISTS payments (
    payment_id           TEXT PRIMARY KEY,
    sale_id              TEXT NOT NULL,
    payment_method       TEXT NOT NULL,
    amount_minor         BIGINT NOT NULL,
    currency             TEXT NOT NULL DEFAULT 'BHD',
    status               TEXT NOT NULL DEFAULT 'approved',
    external_reference   TEXT,
    tendered_minor       BIGINT,
    change_minor         BIGINT,
    recorded_by_user_id  TEXT NOT NULL,
    recorded_at          TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS refunds (
    refund_id             TEXT PRIMARY KEY,
    original_sale_id      TEXT NOT NULL,
    refund_receipt_number TEXT NOT NULL,
    reason                TEXT NOT NULL DEFAULT '',
    refund_total_minor    BIGINT NOT NULL,
    currency              TEXT NOT NULL DEFAULT 'BHD',
    created_by_user_id    TEXT NOT NULL,
    created_at            TEXT NOT NULL,
    idempotency_key       TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS refund_items (
    refund_item_id        TEXT PRIMARY KEY,
    refund_id             TEXT NOT NULL,
    sale_item_id          TEXT NOT NULL,
    product_name_snapshot TEXT NOT NULL,
    quantity              TEXT NOT NULL,
    unit_price_minor      BIGINT NOT NULL DEFAULT 0,
    refund_amount_minor   BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS audit_logs (
    audit_log_id  TEXT PRIMARY KEY,
    event_type    TEXT NOT NULL,
    entity_type   TEXT NOT NULL,
    entity_id     TEXT NOT NULL,
    actor_user_id TEXT,
    actor_type    TEXT NOT NULL DEFAULT 'user',
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

-- ── apply_sync_event RPC ──────────────────────────────────────────────────────
-- Called by every device push. Inserts into sync_events log (idempotent) and
-- applies LWW upsert or append-only insert to the entity table.
CREATE OR REPLACE FUNCTION apply_sync_event(
    p_entity_type   TEXT,
    p_operation     TEXT,
    p_payload       JSONB,
    p_idem_key      TEXT
) RETURNS void
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
            WHEN 'sale_item'    THEN v_entity_id := p_payload->>'sale_item_id';
            WHEN 'refund_item'  THEN v_entity_id := p_payload->>'refund_item_id';
            WHEN 'audit_log'    THEN v_entity_id := p_payload->>'audit_log_id';
            WHEN 'product_price' THEN v_entity_id := p_payload->>'price_id';
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
        COALESCE(p_payload->>'payload_hash', ''),
        p_idem_key,
        COALESCE((p_payload->>'local_sequence')::BIGINT, 0),
        COALESCE(p_payload->>'created_at', NOW()::TEXT)
    ON CONFLICT (idempotency_key) DO NOTHING;

    -- Apply to entity table
    CASE p_entity_type

        WHEN 'product' THEN
            INSERT INTO products (
                product_id, category_id, name, sku, barcode, description,
                track_inventory, allow_decimal_quantity, is_active, tax_rule_id,
                cost_minor, currency, created_at, updated_at, version
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
                p_payload->>'created_at', p_payload->>'updated_at',
                COALESCE((p_payload->>'version')::BIGINT, 1)
            )
            ON CONFLICT (product_id) DO UPDATE SET
                name                   = EXCLUDED.name,
                sku                    = EXCLUDED.sku,
                barcode                = EXCLUDED.barcode,
                description            = EXCLUDED.description,
                track_inventory        = EXCLUDED.track_inventory,
                allow_decimal_quantity = EXCLUDED.allow_decimal_quantity,
                is_active              = EXCLUDED.is_active,
                tax_rule_id            = EXCLUDED.tax_rule_id,
                cost_minor             = EXCLUDED.cost_minor,
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
                shift_id, branch_id, device_id, cashier_user_id,
                opened_at, closed_at, opening_cash_minor, counted_cash_minor,
                status, close_notes, updated_at
            )
            VALUES (
                p_payload->>'shift_id', p_payload->>'branch_id', p_payload->>'device_id',
                p_payload->>'cashier_user_id', p_payload->>'opened_at', p_payload->>'closed_at',
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
                sale_id, receipt_number, branch_id, device_id, shift_id,
                cashier_user_id, status, gross_total_minor, discount_total_minor,
                tax_total_minor, net_total_minor, currency, business_date,
                sold_at, created_offline, idempotency_key
            )
            VALUES (
                p_payload->>'sale_id', p_payload->>'receipt_number',
                p_payload->>'branch_id', p_payload->>'device_id', p_payload->>'shift_id',
                p_payload->>'cashier_user_id',
                COALESCE(p_payload->>'status', 'completed'),
                COALESCE((p_payload->>'gross_total_minor')::BIGINT, 0),
                COALESCE((p_payload->>'discount_total_minor')::BIGINT, 0),
                COALESCE((p_payload->>'tax_total_minor')::BIGINT, 0),
                COALESCE((p_payload->>'net_total_minor')::BIGINT, 0),
                COALESCE(p_payload->>'currency', 'BHD'),
                p_payload->>'business_date', p_payload->>'sold_at',
                COALESCE((p_payload->>'created_offline')::BOOLEAN, FALSE),
                p_payload->>'idempotency_key'
            )
            ON CONFLICT (sale_id) DO NOTHING;

        WHEN 'sale_item' THEN
            INSERT INTO sale_items (
                sale_item_id, sale_id, product_id, product_name_snapshot,
                sku_snapshot, barcode_snapshot, quantity, unit_price_minor,
                line_discount_minor, tax_rule_snapshot, tax_amount_minor,
                line_total_minor, note, voided
            )
            VALUES (
                p_payload->>'sale_item_id', p_payload->>'sale_id',
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
                payment_id, sale_id, payment_method, amount_minor, currency,
                status, external_reference, tendered_minor, change_minor,
                recorded_by_user_id, recorded_at
            )
            VALUES (
                p_payload->>'payment_id', p_payload->>'sale_id',
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
                refund_id, original_sale_id, refund_receipt_number, reason,
                refund_total_minor, currency, created_by_user_id, created_at, idempotency_key
            )
            VALUES (
                p_payload->>'refund_id', p_payload->>'original_sale_id',
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
                refund_item_id, refund_id, sale_item_id, product_name_snapshot,
                quantity, unit_price_minor, refund_amount_minor
            )
            VALUES (
                p_payload->>'refund_item_id', p_payload->>'refund_id',
                p_payload->>'sale_item_id', p_payload->>'product_name_snapshot',
                p_payload->>'quantity',
                COALESCE((p_payload->>'unit_price_minor')::BIGINT, 0),
                COALESCE((p_payload->>'refund_amount_minor')::BIGINT, 0)
            )
            ON CONFLICT (refund_item_id) DO NOTHING;

        WHEN 'audit_log' THEN
            INSERT INTO audit_logs (
                audit_log_id, event_type, entity_type, entity_id,
                actor_user_id, actor_type, ai_action_id, device_id, branch_id,
                before_json, after_json, reason, created_at, hash, previous_hash
            )
            VALUES (
                p_payload->>'audit_log_id', p_payload->>'event_type',
                p_payload->>'entity_type', p_payload->>'entity_id',
                p_payload->>'actor_user_id',
                COALESCE(p_payload->>'actor_type', 'user'),
                p_payload->>'ai_action_id', p_payload->>'device_id', p_payload->>'branch_id',
                p_payload->>'before_json', p_payload->>'after_json', p_payload->>'reason',
                p_payload->>'created_at',
                COALESCE(p_payload->>'hash', ''),
                p_payload->>'previous_hash'
            )
            ON CONFLICT (audit_log_id) DO NOTHING;

        ELSE
            -- Unknown entity type: still logged in sync_events, just skip entity apply
            NULL;
    END CASE;
END;
$$;

"#;
