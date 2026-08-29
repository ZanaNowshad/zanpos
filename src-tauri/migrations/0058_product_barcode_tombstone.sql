-- A deleted barcode has to stay deleted, and has to be reusable afterwards.
--
-- `product_barcodes` had no `deleted_at`, and three code paths removed rows
-- with a hard `DELETE`: the admin screen's remove button and two ZanAI tools.
-- A hard delete leaves nothing behind — no row to push, no tombstone to carry —
-- so the hub keeps its copy and hands it back on the next pull. The barcode
-- returns, and until it does the table reports as divergent, which is what
-- `product_barcodes` was doing in the mismatched list on the stuck terminal.
--
-- Same failure `customers` and `shifts` had, reached by a different route: not
-- a tombstone that could not travel, but no tombstone at all.
--
-- The table is rebuilt rather than altered because `barcode TEXT NOT NULL
-- UNIQUE` is a column constraint, and SQLite's implicit index for one cannot be
-- dropped. Leaving it would make the fix worse than the bug: a soft-deleted row
-- keeps its barcode value, so scanning a code onto the wrong product once would
-- make that code unusable forever. Uniqueness has to apply to live rows only,
-- which needs a partial index, which needs the constraint gone.

PRAGMA foreign_keys = OFF;

-- Dropped before the rebuild, not after: the triggers on `products` read
-- `product_barcodes`, and they fire during the copy below — at which point the
-- table they name has been dropped.
DROP TRIGGER IF EXISTS trg_products_barcode_unique_insert;
DROP TRIGGER IF EXISTS trg_products_barcode_unique_update;
DROP TRIGGER IF EXISTS trg_product_barcodes_unique_insert;
DROP TRIGGER IF EXISTS trg_product_barcodes_unique_update;


CREATE TABLE product_barcodes_new (
    barcode_id    TEXT,
    product_id    TEXT NOT NULL REFERENCES products(product_id),
    barcode       TEXT NOT NULL,
    -- Defaults that mean something. 0001 and 0028 both used `DEFAULT ''`, and an
    -- empty string is not a timestamp: the hub selects rows with
    -- `strftime(updated_at) > strftime(:watermark)`, `strftime('')` is NULL, and
    -- `NULL > x` is never true. Every barcode written since 0028 was therefore
    -- unservable to any terminal, forever, because no insert path set the column
    -- and nothing about `NOT NULL DEFAULT ''` suggested one had to.
    --
    -- Since the table is being rebuilt anyway, the default is made correct. A
    -- path that forgets the column now writes a row that syncs rather than one
    -- that silently cannot.
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    -- The only marker this table has. There is no `is_active` to fall back on,
    -- so the tombstone must sync or the deletion exists nowhere else.
    deleted_at    TEXT,
    sync_status   TEXT NOT NULL DEFAULT 'pending',
    sync_attempts INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (product_id, barcode)
);

INSERT INTO product_barcodes_new
    (barcode_id, product_id, barcode, created_at, updated_at, sync_status, sync_attempts)
SELECT barcode_id, product_id, barcode, created_at, updated_at, sync_status, sync_attempts
  FROM product_barcodes;

DROP TABLE product_barcodes;
ALTER TABLE product_barcodes_new RENAME TO product_barcodes;

CREATE UNIQUE INDEX IF NOT EXISTS idx_product_barcodes_id
    ON product_barcodes(barcode_id);
-- Live rows only: a retired barcode must be free to be scanned onto the right
-- product tomorrow.
CREATE UNIQUE INDEX IF NOT EXISTS idx_product_barcodes_live
    ON product_barcodes(barcode) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_product_barcodes_sync_status
    ON product_barcodes(sync_status);
CREATE INDEX IF NOT EXISTS idx_product_barcodes_updated_at
    ON product_barcodes(updated_at);
CREATE INDEX IF NOT EXISTS idx_product_barcodes_deleted_at
    ON product_barcodes(deleted_at);

PRAGMA foreign_keys = ON;

-- ── Uniqueness triggers, rebuilt around the tombstone ────────────────────────
--
-- 0026 guards barcode uniqueness across both `products.barcode` and
-- `product_barcodes.barcode`. They are dropped above with the table and
-- recreated here with one change: every reference to `product_barcodes` now
-- ignores tombstoned rows.
--
-- Without that the fix would be worse than the bug. A barcode scanned onto the
-- wrong product once, then removed, would keep failing every attempt to add it
-- to the right product — with an error saying it is "in use by another active
-- product" when the row it names is deleted.

CREATE TRIGGER trg_products_barcode_unique_insert
BEFORE INSERT ON products
WHEN NEW.deleted_at IS NULL
 AND NEW.is_active = 1
 AND NEW.barcode IS NOT NULL
 AND TRIM(NEW.barcode) <> ''
BEGIN
  SELECT RAISE(ABORT, 'barcode already in use by another active product')
  WHERE EXISTS (
    SELECT 1 FROM products p
    WHERE p.deleted_at IS NULL AND p.is_active = 1
      AND p.product_id <> NEW.product_id AND p.barcode = NEW.barcode
  )
  OR EXISTS (
    SELECT 1 FROM product_barcodes pb
    JOIN products p ON p.product_id = pb.product_id
    WHERE pb.deleted_at IS NULL
      AND p.deleted_at IS NULL AND p.is_active = 1
      AND p.product_id <> NEW.product_id AND pb.barcode = NEW.barcode
  );
END;

CREATE TRIGGER trg_products_barcode_unique_update
BEFORE UPDATE OF barcode, is_active, deleted_at ON products
WHEN NEW.deleted_at IS NULL
 AND NEW.is_active = 1
 AND NEW.barcode IS NOT NULL
 AND TRIM(NEW.barcode) <> ''
BEGIN
  SELECT RAISE(ABORT, 'barcode already in use by another active product')
  WHERE EXISTS (
    SELECT 1 FROM products p
    WHERE p.deleted_at IS NULL AND p.is_active = 1
      AND p.product_id <> NEW.product_id AND p.barcode = NEW.barcode
  )
  OR EXISTS (
    SELECT 1 FROM product_barcodes pb
    JOIN products p ON p.product_id = pb.product_id
    WHERE pb.deleted_at IS NULL
      AND p.deleted_at IS NULL AND p.is_active = 1
      AND p.product_id <> NEW.product_id AND pb.barcode = NEW.barcode
  );
END;

-- Only guards live inserts: restoring a tombstoned row is how a pulled deletion
-- gets undone, and that must not be refused.
CREATE TRIGGER trg_product_barcodes_unique_insert
BEFORE INSERT ON product_barcodes
WHEN NEW.deleted_at IS NULL
BEGIN
  SELECT RAISE(ABORT, 'barcode already in use by another active product')
  WHERE EXISTS (
    SELECT 1 FROM products p
    WHERE p.deleted_at IS NULL AND p.is_active = 1
      AND p.product_id <> NEW.product_id AND p.barcode = NEW.barcode
  );
END;

CREATE TRIGGER trg_product_barcodes_unique_update
BEFORE UPDATE OF barcode, product_id ON product_barcodes
WHEN NEW.deleted_at IS NULL
BEGIN
  SELECT RAISE(ABORT, 'barcode already in use by another active product')
  WHERE EXISTS (
    SELECT 1 FROM products p
    WHERE p.deleted_at IS NULL AND p.is_active = 1
      AND p.product_id <> NEW.product_id AND p.barcode = NEW.barcode
  );
END;
