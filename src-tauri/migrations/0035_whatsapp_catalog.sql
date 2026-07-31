-- Map POS products ↔ WhatsApp Business catalog products.
-- One row per POS product that has been pushed to WhatsApp. Lets the sync
-- engine detect drift (products.updated_at > synced_at) and clean up
-- WhatsApp-only products that ZANPOS created.
CREATE TABLE IF NOT EXISTS wa_catalog_products (
    product_id     TEXT NOT NULL,
    wa_product_id  TEXT NOT NULL,          -- WhatsApp's catalog product ID
    wa_image_url   TEXT,                    -- first WhatsApp-hosted image URL
    synced_at      TEXT NOT NULL,
    last_error     TEXT,
    PRIMARY KEY (product_id),
    UNIQUE (wa_product_id)
);
CREATE INDEX IF NOT EXISTS idx_wa_catalog_wa_id ON wa_catalog_products(wa_product_id);
