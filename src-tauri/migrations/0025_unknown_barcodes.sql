-- Migration 0025: Ghost barcode lookup table
-- Records barcodes that failed product lookup so managers can
-- resolve them later via online barcode databases.

CREATE TABLE IF NOT EXISTS unknown_barcodes (
    id              TEXT PRIMARY KEY NOT NULL,
    barcode         TEXT NOT NULL UNIQUE,
    scan_count      INTEGER NOT NULL DEFAULT 1,
    first_seen_at   INTEGER NOT NULL,
    last_seen_at    INTEGER NOT NULL,
    status          TEXT NOT NULL DEFAULT 'pending'
                        CHECK(status IN ('pending','found','not_found','dismissed')),
    product_name    TEXT,
    brand           TEXT,
    category        TEXT,
    image_url       TEXT,
    raw_json        TEXT
);

CREATE INDEX IF NOT EXISTS idx_unknown_barcodes_status
    ON unknown_barcodes(status);
