-- ghost_record() upserts unknown barcodes via `ON CONFLICT(barcode) DO UPDATE`,
-- but unknown_barcodes had only `id` as PRIMARY KEY — no UNIQUE on `barcode`.
-- SQLite rejected the whole statement at prepare time with
-- "ON CONFLICT clause does not match any PRIMARY KEY or UNIQUE constraint",
-- so every failed-scan recording (fire-and-forget from the till) errored and
-- the Unknown Barcodes panel never populated.
--
-- The table is normally empty (the insert always failed), but dedup defensively
-- before adding the unique index so this migration is safe on any DB state.

DELETE FROM unknown_barcodes
WHERE rowid NOT IN (SELECT MIN(rowid) FROM unknown_barcodes GROUP BY barcode);

CREATE UNIQUE INDEX IF NOT EXISTS idx_unknown_barcodes_barcode
    ON unknown_barcodes(barcode);
