-- Prevent future active catalog barcode collisions without breaking stores that
-- already contain duplicates. Existing duplicates remain visible in the merge
-- wizard; new inserts/updates are rejected at the database boundary.

CREATE TRIGGER IF NOT EXISTS trg_products_barcode_unique_insert
BEFORE INSERT ON products
WHEN NEW.deleted_at IS NULL
 AND NEW.is_active = 1
 AND NEW.barcode IS NOT NULL
 AND TRIM(NEW.barcode) <> ''
BEGIN
  SELECT RAISE(ABORT, 'barcode already in use by another active product')
  WHERE EXISTS (
    SELECT 1 FROM products p
    WHERE p.deleted_at IS NULL
      AND p.is_active = 1
      AND p.product_id <> NEW.product_id
      AND p.barcode = NEW.barcode
  )
  OR EXISTS (
    SELECT 1
    FROM product_barcodes pb
    JOIN products p ON p.product_id = pb.product_id
    WHERE p.deleted_at IS NULL
      AND p.is_active = 1
      AND p.product_id <> NEW.product_id
      AND pb.barcode = NEW.barcode
  );
END;

CREATE TRIGGER IF NOT EXISTS trg_products_barcode_unique_update
BEFORE UPDATE OF barcode, is_active, deleted_at ON products
WHEN NEW.deleted_at IS NULL
 AND NEW.is_active = 1
 AND NEW.barcode IS NOT NULL
 AND TRIM(NEW.barcode) <> ''
BEGIN
  SELECT RAISE(ABORT, 'barcode already in use by another active product')
  WHERE EXISTS (
    SELECT 1 FROM products p
    WHERE p.deleted_at IS NULL
      AND p.is_active = 1
      AND p.product_id <> NEW.product_id
      AND p.barcode = NEW.barcode
  )
  OR EXISTS (
    SELECT 1
    FROM product_barcodes pb
    JOIN products p ON p.product_id = pb.product_id
    WHERE p.deleted_at IS NULL
      AND p.is_active = 1
      AND p.product_id <> NEW.product_id
      AND pb.barcode = NEW.barcode
  );
END;

CREATE TRIGGER IF NOT EXISTS trg_product_barcodes_unique_insert
BEFORE INSERT ON product_barcodes
BEGIN
  SELECT RAISE(ABORT, 'barcode already in use by another active product')
  WHERE EXISTS (
    SELECT 1 FROM products p
    WHERE p.deleted_at IS NULL
      AND p.is_active = 1
      AND p.product_id <> NEW.product_id
      AND p.barcode = NEW.barcode
  );
END;

CREATE TRIGGER IF NOT EXISTS trg_product_barcodes_unique_update
BEFORE UPDATE OF barcode, product_id ON product_barcodes
BEGIN
  SELECT RAISE(ABORT, 'barcode already in use by another active product')
  WHERE EXISTS (
    SELECT 1 FROM products p
    WHERE p.deleted_at IS NULL
      AND p.is_active = 1
      AND p.product_id <> NEW.product_id
      AND p.barcode = NEW.barcode
  );
END;
