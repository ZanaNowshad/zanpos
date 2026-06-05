-- categories
CREATE TABLE categories (
    category_id TEXT PRIMARY KEY,
    parent_category_id TEXT,
    name TEXT NOT NULL,
    sort_order INTEGER NOT NULL DEFAULT 0,
    is_active INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT,
    version INTEGER NOT NULL DEFAULT 1,
    sync_status TEXT NOT NULL DEFAULT 'pending',
    sync_attempts INTEGER NOT NULL DEFAULT 0
);

-- tax_rules
CREATE TABLE tax_rules (
    tax_rule_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    rate_basis_points INTEGER NOT NULL DEFAULT 0,
    inclusive INTEGER NOT NULL DEFAULT 0,
    is_active INTEGER NOT NULL DEFAULT 1,
    effective_from TEXT NOT NULL,
    effective_to TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT,
    version INTEGER NOT NULL DEFAULT 1,
    sync_status TEXT NOT NULL DEFAULT 'pending',
    sync_attempts INTEGER NOT NULL DEFAULT 0
);

-- products
CREATE TABLE products (
    product_id TEXT PRIMARY KEY,
    category_id TEXT NOT NULL REFERENCES categories(category_id),
    name TEXT NOT NULL,
    sku TEXT,
    barcode TEXT,
    description TEXT,
    track_inventory INTEGER NOT NULL DEFAULT 1,
    allow_decimal_quantity INTEGER NOT NULL DEFAULT 0,
    is_active INTEGER NOT NULL DEFAULT 1,
    tax_rule_id TEXT REFERENCES tax_rules(tax_rule_id),
    cost_minor INTEGER,
    currency TEXT NOT NULL DEFAULT 'BHD',
    reorder_point INTEGER NOT NULL DEFAULT 0,
    image_path TEXT,
    default_supplier_id TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    deleted_at TEXT,
    version INTEGER NOT NULL DEFAULT 1,
    sync_status TEXT NOT NULL DEFAULT 'pending',
    sync_attempts INTEGER NOT NULL DEFAULT 0
);

-- product_prices: append-only price history
CREATE TABLE product_prices (
    price_id TEXT PRIMARY KEY,
    product_id TEXT NOT NULL REFERENCES products(product_id),
    branch_id TEXT,
    price_type TEXT NOT NULL DEFAULT 'selling',
    price_minor INTEGER NOT NULL,
    currency TEXT NOT NULL DEFAULT 'BHD',
    effective_from TEXT NOT NULL,
    effective_to TEXT,
    created_by_user_id TEXT NOT NULL,
    created_by_ai_action_id TEXT,
    created_at TEXT NOT NULL,
    sync_status TEXT NOT NULL DEFAULT 'pending',
    sync_attempts INTEGER NOT NULL DEFAULT 0
);

-- product_barcodes: additional barcodes per product
CREATE TABLE product_barcodes (
    barcode_id TEXT,
    product_id TEXT NOT NULL REFERENCES products(product_id),
    barcode TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (product_id, barcode)
);

CREATE INDEX idx_products_category ON products(category_id);
CREATE INDEX idx_products_barcode ON products(barcode);
CREATE INDEX idx_products_name ON products(name);
CREATE INDEX idx_products_sku ON products(sku);
CREATE INDEX idx_prices_product ON product_prices(product_id);
CREATE INDEX idx_prices_effective ON product_prices(product_id, effective_from);
CREATE INDEX idx_tax_rules_active ON tax_rules(is_active);
