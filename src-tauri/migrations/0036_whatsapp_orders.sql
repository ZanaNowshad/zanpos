-- Incoming WhatsApp commerce orders (customer checked out from the catalog).
-- Populated when an orderMessage arrives and Rust resolves it via the sidecar
-- GET /orders/:id endpoint. status flows new → reviewed → fulfilled/cancelled.
CREATE TABLE IF NOT EXISTS wa_orders (
    order_id       TEXT PRIMARY KEY,       -- WhatsApp order ID
    customer_jid   TEXT NOT NULL,
    customer_name  TEXT,
    message_id     TEXT NOT NULL,          -- wa_messages.id that carried the order
    status         TEXT NOT NULL DEFAULT 'new',   -- new/reviewed/fulfilled/cancelled
    raw_json       TEXT NOT NULL,          -- resolved order details JSON (line items)
    total_minor    INTEGER,                -- order total in minor units
    currency       TEXT,
    product_count  INTEGER NOT NULL DEFAULT 0,
    created_at     TEXT NOT NULL,
    reviewed_at    TEXT,
    reviewed_by    TEXT,
    linked_sale_id TEXT,                   -- set when converted to a POS sale
    version        INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX IF NOT EXISTS idx_wa_orders_status ON wa_orders(status);
CREATE INDEX IF NOT EXISTS idx_wa_orders_created ON wa_orders(created_at DESC);
