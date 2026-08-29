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

/// Build an FTS5 MATCH expression from free user text.
///
/// FTS5 has its own query syntax, so raw input cannot be passed through: a bare
/// `-` or `"` is a syntax error, not a no-match. Every token is quoted (which
/// makes it a literal) and given a `*` prefix so typing "alma" still finds
/// "Almarai". Returns None when nothing usable survives, which is the caller's
/// signal to fall back.
fn fts_match_query(raw: &str) -> Option<String> {
    let terms: Vec<String> = raw
        .split(|c: char| !c.is_alphanumeric())
        .filter(|term| !term.is_empty())
        .map(|term| format!("\"{}\"*", term.replace('"', "")))
        .collect();
    (!terms.is_empty()).then(|| terms.join(" "))
}

/// Full-text product search backed by the `product_search` FTS5 index.
///
/// This is the indexed replacement for the `LIKE '%term%'` scan. FTS5 matches
/// whole-token prefixes rather than arbitrary substrings, so a mid-word query
/// ("lmara") finds nothing here where LIKE would have matched — callers that
/// need substring semantics fall back to [`search_products`].
pub async fn search_products_fts(
    pool: &SqlitePool,
    query: &str,
    limit: i64,
) -> AppResult<Vec<ProductWithPrice>> {
    let Some(match_query) = fts_match_query(query) else {
        return Ok(Vec::new());
    };
    let sql = format!(
        "{} AND p.rowid IN (SELECT rowid FROM product_search WHERE product_search MATCH ?)          ORDER BY p.name LIMIT ?",
        PRODUCT_QUERY
    );
    let rows = sqlx::query(&sql)
        .bind(&match_query)
        .bind(limit)
        .fetch_all(pool)
        .await?;

    Ok(rows.iter().map(row_to_product).collect())
}

/// Indexed search first, falling back to the scan only when the index returns
/// nothing — so a mid-word query still behaves exactly as it did before, while
/// the common prefix query never touches the full table.
pub async fn search_products_indexed(
    pool: &SqlitePool,
    query: &str,
    limit: i64,
) -> AppResult<Vec<ProductWithPrice>> {
    let hits = search_products_fts(pool, query, limit).await?;
    if !hits.is_empty() {
        return Ok(hits);
    }
    search_products(pool, query, limit).await
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
        "{} AND (p.barcode = ? OR p.product_id IN (SELECT product_id FROM product_barcodes WHERE barcode = ? AND deleted_at IS NULL)) LIMIT 1",
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

#[cfg(test)]
mod fts_tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn pool_with_products(names: &[(&str, &str)]) -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let now = "2026-01-01T00:00:00Z";
        sqlx::query("INSERT INTO categories (category_id, name, sort_order, is_active, created_at, updated_at) VALUES ('cat1','General',0,1,?,?)")
            .bind(now).bind(now).execute(&pool).await.unwrap();
        for (i, (name, sku)) in names.iter().enumerate() {
            sqlx::query(
                "INSERT INTO products (product_id, category_id, name, sku, is_active, created_at, updated_at) \
                 VALUES (?,'cat1',?,?,1,?,?)",
            )
            .bind(format!("p{i}"))
            .bind(name)
            .bind(sku)
            .bind(now)
            .bind(now)
            .execute(&pool)
            .await
            .unwrap();
        }
        pool
    }

    #[test]
    fn free_text_becomes_a_safe_prefix_match() {
        // Quoting makes each term a literal, so FTS5 operators in user input are
        // data rather than syntax; the trailing * keeps prefix search working.
        assert_eq!(fts_match_query("alma"), Some("\"alma\"*".into()));
        assert_eq!(
            fts_match_query("fresh milk"),
            Some("\"fresh\"* \"milk\"*".into())
        );
        // A bare operator would be an FTS5 syntax error if passed through.
        assert_eq!(fts_match_query("-"), None);
        assert_eq!(fts_match_query("   "), None);
        assert_eq!(fts_match_query("\"OR\""), Some("\"OR\"*".into()));
    }

    #[tokio::test]
    async fn index_finds_products_by_token_prefix() {
        let pool =
            pool_with_products(&[("Almarai Fresh Milk", "SKU1"), ("Basmati Rice", "SKU2")]).await;

        let hits = search_products_fts(&pool, "alma", 10).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].product.name, "Almarai Fresh Milk");

        // Second token also matches — index covers the whole name.
        let hits = search_products_fts(&pool, "milk", 10).await.unwrap();
        assert_eq!(hits.len(), 1);
    }

    #[tokio::test]
    async fn triggers_keep_the_index_in_step_with_the_table() {
        let pool = pool_with_products(&[("Almarai Fresh Milk", "SKU1")]).await;

        // Rename: the old term must stop matching and the new one start.
        sqlx::query("UPDATE products SET name='Nadec Laban' WHERE product_id='p0'")
            .execute(&pool)
            .await
            .unwrap();
        assert!(search_products_fts(&pool, "almarai", 10)
            .await
            .unwrap()
            .is_empty());
        assert_eq!(
            search_products_fts(&pool, "nadec", 10).await.unwrap().len(),
            1
        );

        // Soft delete removes it from the index.
        sqlx::query("UPDATE products SET deleted_at='2026-01-02T00:00:00Z' WHERE product_id='p0'")
            .execute(&pool)
            .await
            .unwrap();
        assert!(search_products_fts(&pool, "nadec", 10)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn mid_word_queries_still_work_through_the_fallback() {
        let pool = pool_with_products(&[("Almarai Fresh Milk", "SKU1")]).await;

        // FTS5 matches token prefixes, so a mid-word fragment misses the index...
        assert!(search_products_fts(&pool, "lmarai", 10)
            .await
            .unwrap()
            .is_empty());
        // ...but the combined path preserves the old substring behaviour.
        let hits = search_products_indexed(&pool, "lmarai", 10).await.unwrap();
        assert_eq!(hits.len(), 1);
    }
}

/// Retire a barcode so the removal reaches every other terminal.
///
/// `product_barcodes` has no `is_active` to fall back on, so the tombstone is
/// the only marker a deletion has — and three code paths used to hard-`DELETE`,
/// which left nothing to push and let the hub hand its copy back on the next
/// pull. The barcode came back, and the table reported divergent until it did.
pub async fn soft_delete_barcode(pool: &SqlitePool, barcode_id: &str) -> AppResult<u64> {
    let now = chrono::Utc::now().to_rfc3339();
    Ok(sqlx::query(
        "UPDATE product_barcodes
            SET deleted_at = ?, updated_at = ?, sync_status = 'pending'
          WHERE barcode_id = ? AND deleted_at IS NULL",
    )
    .bind(&now)
    .bind(&now)
    .bind(barcode_id)
    .execute(pool)
    .await?
    .rows_affected())
}

/// Retire every barcode belonging to a product, for when the product itself goes.
///
/// Same tombstone, different handle. Deleting a product without this leaves its
/// codes scanning on every other till until that terminal happens to pull the
/// product row — and on a till that scans faster than it syncs, that is a
/// customer standing at the counter with an item the screen says does not exist.
pub async fn soft_delete_barcodes_for_product(
    pool: &SqlitePool,
    product_id: &str,
) -> AppResult<u64> {
    let now = chrono::Utc::now().to_rfc3339();
    Ok(sqlx::query(
        "UPDATE product_barcodes
            SET deleted_at = ?, updated_at = ?, sync_status = 'pending'
          WHERE product_id = ? AND deleted_at IS NULL",
    )
    .bind(&now)
    .bind(&now)
    .bind(product_id)
    .execute(pool)
    .await?
    .rows_affected())
}
