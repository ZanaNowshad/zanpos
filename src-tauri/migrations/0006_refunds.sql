CREATE TABLE refunds (
    refund_id TEXT PRIMARY KEY,
    original_sale_id TEXT NOT NULL,
    origin_device_id TEXT NOT NULL DEFAULT '',
    refund_receipt_number TEXT NOT NULL UNIQUE,
    reason TEXT NOT NULL DEFAULT '',
    refund_total_minor INTEGER NOT NULL,
    currency TEXT NOT NULL DEFAULT 'BHD',
    created_by_user_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    sync_status TEXT NOT NULL DEFAULT 'pending',
    sync_attempts INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE refund_items (
    refund_item_id TEXT PRIMARY KEY,
    refund_id TEXT NOT NULL REFERENCES refunds(refund_id),
    origin_device_id TEXT NOT NULL DEFAULT '',
    sale_item_id TEXT NOT NULL,
    product_name_snapshot TEXT NOT NULL,
    quantity TEXT NOT NULL,
    unit_price_minor INTEGER NOT NULL DEFAULT 0,
    refund_amount_minor INTEGER NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    sync_status TEXT NOT NULL DEFAULT 'pending',
    sync_attempts INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_refunds_sale ON refunds(original_sale_id);
CREATE INDEX idx_refund_items_refund ON refund_items(refund_id);
