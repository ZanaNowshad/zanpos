-- sync_watermark: tracks last_pulled_at per table
CREATE TABLE sync_watermark (
    table_name TEXT PRIMARY KEY,
    last_pulled_at TEXT NOT NULL DEFAULT '1970-01-01T00:00:00Z',
    last_pushed_at TEXT NOT NULL DEFAULT '1970-01-01T00:00:00Z'
);

-- Seed watermarks for all syncable tables
INSERT INTO sync_watermark (table_name) VALUES
    ('branches'), ('devices'), ('users'), ('products'), ('categories'),
    ('tax_rules'), ('product_prices'), ('customers'), ('sales'),
    ('sale_items'), ('payments'), ('refunds'), ('refund_items'),
    ('stock_movements'), ('stock_levels'), ('audit_logs'),
    ('shifts'), ('delivery_orders'), ('app_config');

-- held_carts: local-only, never synced
CREATE TABLE held_carts (
    held_cart_id TEXT PRIMARY KEY,
    cart_json TEXT NOT NULL,
    note TEXT,
    cashier_user_id TEXT NOT NULL,
    held_at TEXT NOT NULL
);

-- ai_chat_history: local-only
CREATE TABLE ai_chat_history (
    message_id TEXT PRIMARY KEY,
    session_user_id TEXT NOT NULL,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    created_at TEXT NOT NULL
);

-- ai_actions: local confirmation workflow
CREATE TABLE ai_actions (
    action_id TEXT PRIMARY KEY,
    session_user_id TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    tool_input_json TEXT NOT NULL,
    tool_input_hash TEXT NOT NULL,
    preview_text TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    confirmation_token TEXT,
    prepared_at TEXT NOT NULL,
    confirmed_at TEXT,
    executed_at TEXT,
    expires_at TEXT NOT NULL,
    result_json TEXT,
    error_message TEXT
);

-- import_history: migration/import tracking (local only)
CREATE TABLE import_history (
    import_id TEXT PRIMARY KEY,
    source_path TEXT,
    rows_imported INTEGER NOT NULL DEFAULT 0,
    created_by_user_id TEXT NOT NULL,
    created_at TEXT NOT NULL
);
