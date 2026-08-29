-- Barcodes the hub can never hand out.
--
-- `product_barcodes.updated_at` was added in 0028 as `NOT NULL DEFAULT ''`, and
-- 0028 backfilled the rows that existed then. No insert path has set it since —
-- all four write `(barcode_id, product_id, barcode, created_at)` and leave
-- `updated_at` to the default. So every barcode created since that migration
-- carries an empty string.
--
-- The hub serves a pull with:
--
--     strftime('%Y-%m-%dT%H:%M:%f', updated_at) > strftime(..., :watermark)
--
-- `strftime` on `''` is NULL, and `NULL > anything` is NULL, which is not true.
-- The row is never served. Not "served late", not "served once" — never, to any
-- terminal, no matter how many times somebody presses Resync All, because
-- resetting the watermark does not change a predicate that cannot match.
--
-- That is why a terminal shows 28,054 products, 28,119 prices and 0 barcodes,
-- and why a full resync does not move the number.
--
-- `roles` carries the same `DEFAULT ''` from 0004 and is fixed here too. It has
-- been invisible in the same way; nobody noticed because roles are seeded
-- identically on every terminal and almost never change.

UPDATE product_barcodes
   SET updated_at = COALESCE(NULLIF(TRIM(updated_at), ''),
                             NULLIF(TRIM(created_at), ''),
                             datetime('now')),
       -- Re-queued so the corrected timestamp reaches the hub. Without this the
       -- terminal holds a fixed row the hub still cannot serve to its siblings.
       sync_status = 'pending'
 WHERE TRIM(COALESCE(updated_at, '')) = '';

UPDATE roles
   SET updated_at = COALESCE(NULLIF(TRIM(updated_at), ''),
                             NULLIF(TRIM(created_at), ''),
                             datetime('now'))
 WHERE TRIM(COALESCE(updated_at, '')) = '';
