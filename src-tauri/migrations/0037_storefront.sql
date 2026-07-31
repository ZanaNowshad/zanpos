-- Public storefront publication state. Only explicitly visible products are
-- included in snapshots; local file paths and internal inventory data stay local.
CREATE TABLE storefront_products (
    product_id       TEXT PRIMARY KEY REFERENCES products(product_id),
    is_visible       INTEGER NOT NULL DEFAULT 0 CHECK (is_visible IN (0, 1)),
    public_image_url TEXT,
    name_ar          TEXT,
    description_ar   TEXT,
    featured         INTEGER NOT NULL DEFAULT 0 CHECK (featured IN (0, 1)),
    sort_order       INTEGER NOT NULL DEFAULT 0,
    last_published_hash TEXT,
    publish_error    TEXT,
    updated_at       TEXT NOT NULL
);

CREATE TABLE storefront_releases (
    release_id      TEXT PRIMARY KEY,
    version         INTEGER NOT NULL UNIQUE,
    status          TEXT NOT NULL CHECK (status IN ('staged', 'published', 'failed')),
    snapshot_json   TEXT NOT NULL,
    snapshot_sha256 TEXT NOT NULL,
    error           TEXT,
    created_at      TEXT NOT NULL,
    published_at    TEXT
);
CREATE INDEX idx_storefront_releases_status
    ON storefront_releases(status, version DESC);

ALTER TABLE wa_orders
    ADD COLUMN source TEXT NOT NULL DEFAULT 'whatsapp_catalog';
ALTER TABLE wa_orders
    ADD COLUMN external_ref TEXT;
CREATE UNIQUE INDEX idx_wa_orders_source_external_ref
    ON wa_orders(source, external_ref)
    WHERE external_ref IS NOT NULL;
