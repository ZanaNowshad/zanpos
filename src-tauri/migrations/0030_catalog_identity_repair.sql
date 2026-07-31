-- Legacy imports could contain the same primary barcode on multiple active
-- products before the database trigger was introduced. Keep the lowest stable
-- product_id as the owner, preserve every product, and clear only conflicting
-- barcode assignments so authoritative hub snapshots can apply everywhere.
UPDATE products
SET barcode = NULL,
    updated_at = datetime('now'),
    sync_status = 'pending',
    sync_attempts = 0
WHERE deleted_at IS NULL
  AND is_active = 1
  AND barcode IS NOT NULL
  AND TRIM(barcode) <> ''
  AND EXISTS (
    SELECT 1
    FROM products canonical
    WHERE canonical.deleted_at IS NULL
      AND canonical.is_active = 1
      AND canonical.barcode = products.barcode
      AND canonical.product_id < products.product_id
  );

-- A legacy alternate-barcode row may point at a different product than the
-- product that owns the same primary barcode. The primary assignment wins.
DELETE FROM product_barcodes
WHERE EXISTS (
  SELECT 1
  FROM products primary_owner
  WHERE primary_owner.deleted_at IS NULL
    AND primary_owner.is_active = 1
    AND primary_owner.barcode = product_barcodes.barcode
    AND primary_owner.product_id <> product_barcodes.product_id
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_products_active_barcode_unique
ON products(barcode)
WHERE deleted_at IS NULL
  AND is_active = 1
  AND barcode IS NOT NULL
  AND TRIM(barcode) <> '';
