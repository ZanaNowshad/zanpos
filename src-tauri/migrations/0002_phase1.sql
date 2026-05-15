-- Phase 1: held_carts, refunds; update seed PINs to PLAIN format

CREATE TABLE IF NOT EXISTS held_carts (
    held_cart_id    TEXT PRIMARY KEY,
    branch_id       TEXT NOT NULL,
    device_id       TEXT NOT NULL,
    shift_id        TEXT NOT NULL,
    cashier_user_id TEXT NOT NULL,
    cart_json       TEXT NOT NULL,
    held_at         TEXT NOT NULL,
    note            TEXT
);

CREATE INDEX IF NOT EXISTS idx_held_carts_device ON held_carts(device_id);

CREATE TABLE IF NOT EXISTS refunds (
    refund_id             TEXT PRIMARY KEY,
    original_sale_id      TEXT NOT NULL REFERENCES sales(sale_id),
    refund_receipt_number TEXT NOT NULL,
    reason                TEXT NOT NULL DEFAULT '',
    refund_total_minor    INTEGER NOT NULL,
    currency              TEXT NOT NULL DEFAULT 'BHD',
    created_by_user_id    TEXT NOT NULL REFERENCES users(user_id),
    created_at            TEXT NOT NULL,
    sync_status           TEXT NOT NULL DEFAULT 'pending',
    idempotency_key       TEXT NOT NULL UNIQUE
);

CREATE TABLE IF NOT EXISTS refund_items (
    refund_item_id        TEXT PRIMARY KEY,
    refund_id             TEXT NOT NULL REFERENCES refunds(refund_id),
    sale_item_id          TEXT NOT NULL REFERENCES sale_items(sale_item_id),
    product_name_snapshot TEXT NOT NULL,
    quantity              TEXT NOT NULL,
    unit_price_minor      INTEGER NOT NULL DEFAULT 0,
    refund_amount_minor   INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_refunds_original_sale ON refunds(original_sale_id);
CREATE INDEX IF NOT EXISTS idx_refund_items_refund   ON refund_items(refund_id);

-- Update placeholder PINs: admin=0000, cashier1=1234
UPDATE users SET pin_hash = 'PLAIN:0000' WHERE user_id = '01JUSER000000000000ADMIN1';
UPDATE users SET pin_hash = 'PLAIN:1234' WHERE user_id = '01JUSER000000000000CASH01';
