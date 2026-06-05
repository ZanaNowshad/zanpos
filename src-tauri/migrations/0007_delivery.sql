CREATE TABLE delivery_orders (
    delivery_id TEXT PRIMARY KEY,
    sale_id TEXT NOT NULL REFERENCES sales(sale_id),
    receipt_number TEXT NOT NULL,
    branch_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    origin_device_id TEXT NOT NULL DEFAULT '',
    customer_id TEXT,
    customer_name TEXT,
    contact_number TEXT NOT NULL,
    address_text TEXT,
    house_number TEXT,
    area TEXT,
    delivery_status TEXT NOT NULL DEFAULT 'pending',
    delivery_staff_name TEXT,
    delivery_note TEXT,
    expected_payment_method TEXT NOT NULL DEFAULT 'cash',
    payment_status TEXT NOT NULL DEFAULT 'unpaid',
    amount_minor INTEGER NOT NULL DEFAULT 0,
    currency TEXT NOT NULL DEFAULT 'BHD',
    paid_confirmed_by_user_id TEXT,
    paid_confirmed_at TEXT,
    payment_reference TEXT,
    payment_note TEXT,
    created_by_user_id TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    sync_status TEXT NOT NULL DEFAULT 'pending',
    sync_attempts INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_delivery_branch ON delivery_orders(branch_id, delivery_status);
CREATE INDEX idx_delivery_sale ON delivery_orders(sale_id);
