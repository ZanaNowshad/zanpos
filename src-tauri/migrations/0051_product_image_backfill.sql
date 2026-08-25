-- Attempt state for the automatic product-image fetch.
--
-- The catalogue itself is the work list: anything with no `image_path` and a
-- barcode to search on needs an image. This table holds only what the catalogue
-- cannot say — how many times we have already tried and when it is fair to try
-- again — so a product that has no findable image stops being retried instead
-- of being picked up on every pass forever.
--
-- Deliberately NOT synced. It is device-local work state, not business data.
-- The *result* of a successful fetch is `products.image_path`, which does sync,
-- so a terminal that fills an image stops every other terminal from looking for
-- it again. Syncing the attempt counters instead would mean one device's
-- failures suppressing another device's chance to succeed on a better
-- connection, which is the opposite of useful.
--
-- No foreign key to `products`. A product deleted mid-backfill would otherwise
-- fail the delete or orphan this row; the worker only ever reads rows through a
-- join against live products, so a leftover row is inert and gets cleaned up
-- with the product it names.
CREATE TABLE product_image_attempts (
    product_id      TEXT PRIMARY KEY,
    -- How many times the search has been asked about this product. The worker
    -- gives up after a small number so a product with genuinely no online
    -- image — most unbranded and local goods — costs a handful of requests
    -- once rather than a request per pass forever.
    attempts        INTEGER NOT NULL DEFAULT 0,
    -- RFC3339. Backoff grows with attempts so a rate-limited provider is not
    -- hammered, and a transient network failure still resolves on its own.
    next_attempt_at TEXT NOT NULL,
    -- Kept for the catalogue to explain itself: a manager who sees a product
    -- with no picture should be able to find out whether nothing was found or
    -- the lookup never got through.
    last_error      TEXT,
    -- Set once the search succeeded, so the row records an outcome rather than
    -- disappearing. Useful when an image is later cleared by hand — the
    -- product becomes eligible again and the history is still there.
    resolved_at     TEXT,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

-- The worker's only query orders by due time over unresolved rows.
CREATE INDEX idx_product_image_attempts_due
    ON product_image_attempts(next_attempt_at)
    WHERE resolved_at IS NULL;
