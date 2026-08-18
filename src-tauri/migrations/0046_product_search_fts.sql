-- Full-text search index over the product catalogue.
--
-- Product lookup ran `name LIKE '%term%'`, and a leading wildcard cannot use an
-- index, so every search was a full scan of the products table. That is the
-- hot path for both the POS product picker and ZanAI's product read tools.
--
-- `contentless_delete=1` lets rows be deleted from an external-content table
-- (requires SQLite 3.43+, which ships with the bundled sqlx/rusqlite build).
CREATE VIRTUAL TABLE product_search USING fts5(
    name,
    sku,
    barcode,
    content = 'products',
    content_rowid = 'rowid',
    tokenize = 'unicode61 remove_diacritics 2'
);

-- Seed from the existing catalogue. Soft-deleted rows stay out of the index.
INSERT INTO product_search (rowid, name, sku, barcode)
SELECT rowid, name, COALESCE(sku, ''), COALESCE(barcode, '')
FROM products
WHERE deleted_at IS NULL;

-- Keep the index in step with the table. External-content FTS5 tables are not
-- updated automatically; without these triggers the index silently goes stale.
CREATE TRIGGER product_search_ai AFTER INSERT ON products BEGIN
    INSERT INTO product_search (rowid, name, sku, barcode)
    SELECT new.rowid, new.name, COALESCE(new.sku, ''), COALESCE(new.barcode, '')
    WHERE new.deleted_at IS NULL;
END;

CREATE TRIGGER product_search_ad AFTER DELETE ON products BEGIN
    INSERT INTO product_search (product_search, rowid, name, sku, barcode)
    VALUES ('delete', old.rowid, old.name, COALESCE(old.sku, ''), COALESCE(old.barcode, ''));
END;

-- An update can also be a soft delete or an undelete, so the row is removed and
-- re-inserted only when it should be present afterwards.
CREATE TRIGGER product_search_au AFTER UPDATE ON products BEGIN
    INSERT INTO product_search (product_search, rowid, name, sku, barcode)
    VALUES ('delete', old.rowid, old.name, COALESCE(old.sku, ''), COALESCE(old.barcode, ''));
    INSERT INTO product_search (rowid, name, sku, barcode)
    SELECT new.rowid, new.name, COALESCE(new.sku, ''), COALESCE(new.barcode, '')
    WHERE new.deleted_at IS NULL;
END;
