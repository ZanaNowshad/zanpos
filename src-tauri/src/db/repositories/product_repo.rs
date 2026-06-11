use crate::domain::product::{Product, ProductWithPrice};
use crate::errors::AppResult;
use sqlx::{Row, SqlitePool};

const PRODUCT_QUERY: &str = r#"
    SELECT
        p.product_id,
        p.category_id,
        p.name,
        p.sku,
        p.barcode,
        p.description,
        p.track_inventory,
        p.allow_decimal_quantity,
        p.is_active,
        p.tax_rule_id,
        p.cost_minor,
        p.currency,
        p.version,
        p.created_at,
        p.updated_at,
        p.reorder_point,
        p.image_path,
        p.default_supplier_id,
        c.name AS category_name,
        COALESCE(pp.price_minor, 0) AS price_minor,
        COALESCE(t.rate_basis_points, 0) AS tax_rate_basis_points,
        COALESCE(t.inclusive, 0) AS tax_inclusive,
        sl.quantity_on_hand AS quantity_on_hand
    FROM products p
    JOIN categories c ON c.category_id = p.category_id AND c.is_active = 1 AND c.deleted_at IS NULL
    LEFT JOIN product_prices pp ON pp.product_id = p.product_id
        AND pp.branch_id IS NULL
        AND pp.price_type = 'selling'
        AND datetime(pp.effective_from) <= datetime('now')
        AND (pp.effective_to IS NULL OR datetime(pp.effective_to) > datetime('now'))
    LEFT JOIN tax_rules t ON t.tax_rule_id = p.tax_rule_id AND t.is_active = 1
    LEFT JOIN (SELECT product_id, quantity_on_hand FROM stock_levels WHERE branch_id = (SELECT branch_id FROM branches WHERE is_active = 1 ORDER BY created_at LIMIT 1)) sl ON sl.product_id = p.product_id
    WHERE p.is_active = 1 AND p.deleted_at IS NULL
"#;

fn row_to_product(row: &sqlx::sqlite::SqliteRow) -> ProductWithPrice {
    let track: i64 = row.get("track_inventory");
    let decimal: i64 = row.get("allow_decimal_quantity");
    let active: i64 = row.get("is_active");
    let inclusive: i64 = row.get("tax_inclusive");
    ProductWithPrice {
        product: Product {
            product_id: row.get("product_id"),
            category_id: row.get("category_id"),
            name: row.get("name"),
            sku: row.get("sku"),
            barcode: row.get("barcode"),
            description: row.get("description"),
            track_inventory: track != 0,
            allow_decimal_quantity: decimal != 0,
            is_active: active != 0,
            tax_rule_id: row.get("tax_rule_id"),
            cost_minor: row.get("cost_minor"),
            currency: row.get("currency"),
            version: row.get("version"),
            created_at: row.try_get("created_at").unwrap_or_default(),
            updated_at: row.try_get("updated_at").unwrap_or_default(),
            reorder_point: row.try_get("reorder_point").unwrap_or(0),
            image_path: row.try_get("image_path").unwrap_or(None),
            default_supplier_id: row.try_get("default_supplier_id").unwrap_or(None),
        },
        price_minor: row.get("price_minor"),
        tax_rate_basis_points: row.get("tax_rate_basis_points"),
        tax_inclusive: inclusive != 0,
        category_name: row.get("category_name"),
        quantity_on_hand: row.try_get("quantity_on_hand").unwrap_or(None),
    }
}

pub async fn search_products(
    pool: &SqlitePool,
    query: &str,
    limit: i64,
) -> AppResult<Vec<ProductWithPrice>> {
    // SQLite LIKE is case-insensitive for ASCII — no need for lower() wrapping.
    // Removing lower() allows the idx_products_name index to be used directly.
    let escaped = query
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let pattern = format!("%{}%", escaped);
    let sql = format!(
        "{} AND (p.name LIKE ? ESCAPE '\\' OR p.sku LIKE ? ESCAPE '\\' OR p.barcode LIKE ? ESCAPE '\\') ORDER BY p.name LIMIT ?",
        PRODUCT_QUERY
    );
    let rows = sqlx::query(&sql)
        .bind(&pattern)
        .bind(&pattern)
        .bind(&pattern)
        .bind(limit)
        .fetch_all(pool)
        .await?;

    Ok(rows.iter().map(row_to_product).collect())
}

/// Cursor-paginated product search for large catalogs.
/// Uses `product_id > ?` ordering to avoid OFFSET re-scan on large catalogs.
/// `after_id`: exclusive lower bound (pass `None` for first page).
/// `page_size`: caller must cap.
pub async fn search_products_paginated(
    pool: &SqlitePool,
    query: &str,
    after_id: Option<&str>,
    page_size: u32,
) -> AppResult<Vec<ProductWithPrice>> {
    let escaped = query
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    let pattern = format!("%{}%", escaped);

    let sql = if after_id.is_some() {
        format!(
            "{} AND (p.name LIKE ? ESCAPE '\\' OR p.sku LIKE ? ESCAPE '\\' OR p.barcode LIKE ? ESCAPE '\\') \
             AND p.product_id > ? ORDER BY p.product_id LIMIT ?",
            PRODUCT_QUERY
        )
    } else {
        format!(
            "{} AND (p.name LIKE ? ESCAPE '\\' OR p.sku LIKE ? ESCAPE '\\' OR p.barcode LIKE ? ESCAPE '\\') \
             ORDER BY p.product_id LIMIT ?",
            PRODUCT_QUERY
        )
    };

    let rows = if let Some(cursor) = after_id {
        sqlx::query(&sql)
            .bind(&pattern)
            .bind(&pattern)
            .bind(&pattern)
            .bind(cursor)
            .bind(page_size as i64)
            .fetch_all(pool)
            .await?
    } else {
        sqlx::query(&sql)
            .bind(&pattern)
            .bind(&pattern)
            .bind(&pattern)
            .bind(page_size as i64)
            .fetch_all(pool)
            .await?
    };

    Ok(rows.iter().map(row_to_product).collect())
}

pub async fn get_product_by_barcode(
    pool: &SqlitePool,
    barcode: &str,
) -> AppResult<Option<ProductWithPrice>> {
    let sql = format!(
        "{} AND (p.barcode = ? OR p.product_id IN (SELECT product_id FROM product_barcodes WHERE barcode = ?)) LIMIT 1",
        PRODUCT_QUERY
    );
    let row = sqlx::query(&sql)
        .bind(barcode)
        .bind(barcode)
        .fetch_optional(pool)
        .await?;

    Ok(row.as_ref().map(row_to_product))
}

pub async fn get_product_by_id(
    pool: &SqlitePool,
    product_id: &str,
) -> AppResult<Option<ProductWithPrice>> {
    let sql = format!("{} AND p.product_id = ?", PRODUCT_QUERY);
    let row = sqlx::query(&sql)
        .bind(product_id)
        .fetch_optional(pool)
        .await?;

    Ok(row.as_ref().map(row_to_product))
}

/// Cursor-paginated active product listing.
/// `after_id`: exclusive lower bound on `product_id` (pass `None` for the first page).
/// `page_size`: number of rows to return, caller is responsible for capping.
pub async fn list_all_active(
    pool: &SqlitePool,
    after_id: Option<&str>,
    page_size: u32,
) -> AppResult<Vec<ProductWithPrice>> {
    // Cursor: product_id is a ULID so lexicographic order == insertion order.
    // Using WHERE p.product_id > ? avoids OFFSET re-scanning all previous rows.
    let sql = if after_id.is_some() {
        format!(
            "{} AND p.product_id > ? ORDER BY p.product_id LIMIT ?",
            PRODUCT_QUERY
        )
    } else {
        format!("{} ORDER BY p.product_id LIMIT ?", PRODUCT_QUERY)
    };

    let rows = if let Some(cursor) = after_id {
        sqlx::query(&sql)
            .bind(cursor)
            .bind(page_size as i64)
            .fetch_all(pool)
            .await?
    } else {
        sqlx::query(&sql)
            .bind(page_size as i64)
            .fetch_all(pool)
            .await?
    };

    Ok(rows.iter().map(row_to_product).collect())
}
