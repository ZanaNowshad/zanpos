-- sales
CREATE TABLE sales (
    sale_id TEXT PRIMARY KEY,
    receipt_number TEXT NOT NULL UNIQUE,
    branch_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    origin_device_id TEXT NOT NULL DEFAULT '',
    shift_id TEXT NOT NULL,
    cashier_user_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'completed',
    gross_total_minor INTEGER NOT NULL DEFAULT 0,
    discount_total_minor INTEGER NOT NULL DEFAULT 0,
    tax_total_minor INTEGER NOT NULL DEFAULT 0,
    net_total_minor INTEGER NOT NULL DEFAULT 0,
    currency TEXT NOT NULL DEFAULT 'BHD',
    business_date TEXT NOT NULL,
    sold_at TEXT NOT NULL,
    created_offline INTEGER NOT NULL DEFAULT 0,
    customer_id TEXT,
    is_delivery INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    sync_status TEXT NOT NULL DEFAULT 'pending',
    sync_attempts INTEGER NOT NULL DEFAULT 0
);

-- sale_items
CREATE TABLE sale_items (
    sale_item_id TEXT PRIMARY KEY,
    sale_id TEXT NOT NULL REFERENCES sales(sale_id),
    product_id TEXT,
    product_name_snapshot TEXT NOT NULL,
    sku_snapshot TEXT,
    barcode_snapshot TEXT,
    quantity TEXT NOT NULL,
    unit_price_minor INTEGER NOT NULL,
    line_discount_minor INTEGER NOT NULL DEFAULT 0,
    tax_rule_snapshot TEXT NOT NULL DEFAULT '{}',
    tax_amount_minor INTEGER NOT NULL DEFAULT 0,
    line_total_minor INTEGER NOT NULL DEFAULT 0,
    note TEXT,
    voided INTEGER NOT NULL DEFAULT 0,
    origin_device_id TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    sync_status TEXT NOT NULL DEFAULT 'pending',
    sync_attempts INTEGER NOT NULL DEFAULT 0
);

-- payments
CREATE TABLE payments (
    payment_id TEXT PRIMARY KEY,
    sale_id TEXT NOT NULL REFERENCES sales(sale_id),
    origin_device_id TEXT NOT NULL DEFAULT '',
    payment_method TEXT NOT NULL,
    amount_minor INTEGER NOT NULL,
    currency TEXT NOT NULL DEFAULT 'BHD',
    status TEXT NOT NULL DEFAULT 'approved',
    external_reference TEXT,
    tendered_minor INTEGER,
    change_minor INTEGER,
    recorded_by_user_id TEXT NOT NULL,
    recorded_at TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    sync_status TEXT NOT NULL DEFAULT 'pending',
    sync_attempts INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_sales_shift ON sales(shift_id);
CREATE INDEX idx_sales_business_date ON sales(business_date);
CREATE INDEX idx_sales_receipt ON sales(receipt_number);
CREATE INDEX idx_sale_items_sale ON sale_items(sale_id);
CREATE INDEX idx_payments_sale ON payments(sale_id);
