//! Duplicate-product detection and resolution.
//!
//! Shared by the AI `find_duplicate_products` / `merge_products` tools and the
//! back-office "Duplicate Products" admin command, so both paths use identical
//! merge semantics (stock combined, movements transferred, source archived).

use crate::errors::{AppError, AppResult};
use serde::Serialize;
use sqlx::{Row, SqlitePool};
use std::collections::{HashMap, HashSet};

// ── Scan result types ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct DuplicateProduct {
    pub product_id: String,
    pub name: String,
    pub sku: Option<String>,
    pub barcode: Option<String>,
    pub category_name: String,
    pub price_minor: i64,
    pub is_active: bool,
    pub total_stock: f64,
    pub image_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DuplicateGroup {
    /// "Exact name", "Same barcode", "Same SKU", or "Similar product".
    pub match_type: String,
    /// The shared value the products collide on (name/barcode/sku).
    pub match_key: String,
    pub reason: String,
    pub confidence: i64,
    pub products: Vec<DuplicateProduct>,
}

#[derive(Debug, Serialize)]
pub struct MergeOutcome {
    pub source_name: String,
    pub target_name: String,
}

// Selling-price join shared by every scan query (current, branch-default price).
const PRICE_JOIN: &str = "LEFT JOIN product_prices pp ON pp.product_id = p.product_id \
     AND pp.branch_id IS NULL AND pp.price_type = 'selling' \
     AND datetime(pp.effective_from) <= datetime('now') \
     AND (pp.effective_to IS NULL OR datetime(pp.effective_to) > datetime('now'))";

fn rows_to_groups(rows: &[sqlx::sqlite::SqliteRow], match_type: &str) -> Vec<DuplicateGroup> {
    let mut groups: Vec<DuplicateGroup> = Vec::new();
    for r in rows {
        let gkey: String = r.try_get("gkey").unwrap_or_default();
        let active: i64 = r.try_get("is_active").unwrap_or(0);
        let product = DuplicateProduct {
            product_id: r.get("product_id"),
            name: r.get("name"),
            sku: r.get("sku"),
            barcode: r.get("barcode"),
            category_name: r.get("category_name"),
            price_minor: r.try_get("price_minor").unwrap_or(0),
            is_active: active != 0,
            total_stock: r.try_get("total_stock").unwrap_or(0.0),
            image_path: r.get("image_path"),
        };
        // Rows arrive ordered by gkey, so the current group is always the last one.
        if groups.last().map(|g| g.match_key.as_str()) == Some(gkey.as_str()) {
            groups.last_mut().unwrap().products.push(product);
        } else {
            groups.push(DuplicateGroup {
                match_type: match_type.into(),
                match_key: gkey,
                reason: match_type.into(),
                confidence: if match_type == "Same barcode" {
                    100
                } else {
                    92
                },
                products: vec![product],
            });
        }
    }
    groups
}

fn compact_name(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn prices_are_close(a: i64, b: i64) -> bool {
    let diff = (a - b).abs();
    diff <= 20 || diff * 100 <= a.max(b).max(1) * 5
}

fn push_similar_name_groups(out: &mut Vec<DuplicateGroup>, rows: &[sqlx::sqlite::SqliteRow]) {
    let mut seen: HashSet<String> = out
        .iter()
        .flat_map(|g| {
            let ids: Vec<&str> = g.products.iter().map(|p| p.product_id.as_str()).collect();
            ids.windows(2)
                .map(|w| format!("{}|{}", w[0].min(w[1]), w[0].max(w[1])))
                .collect::<Vec<_>>()
        })
        .collect();
    let mut by_compact: HashMap<String, Vec<DuplicateProduct>> = HashMap::new();
    for r in rows {
        let product = DuplicateProduct {
            product_id: r.get("product_id"),
            name: r.get("name"),
            sku: r.get("sku"),
            barcode: r.get("barcode"),
            category_name: r.get("category_name"),
            price_minor: r.try_get("price_minor").unwrap_or(0),
            is_active: r.try_get::<i64, _>("is_active").unwrap_or(0) != 0,
            total_stock: r.try_get("total_stock").unwrap_or(0.0),
            image_path: r.get("image_path"),
        };
        let key = compact_name(&product.name);
        if key.len() >= 4 {
            by_compact.entry(key).or_default().push(product);
        }
    }

    for (key, products) in by_compact {
        if products.len() < 2 {
            continue;
        }
        let close = products.iter().enumerate().any(|(idx, a)| {
            products
                .iter()
                .skip(idx + 1)
                .any(|b| prices_are_close(a.price_minor, b.price_minor))
        });
        if !close {
            continue;
        }
        let mut ids: Vec<&str> = products.iter().map(|p| p.product_id.as_str()).collect();
        ids.sort_unstable();
        let pair_key = ids.windows(2).next().map(|w| format!("{}|{}", w[0], w[1]));
        if pair_key.as_ref().is_some_and(|k| seen.contains(k)) {
            continue;
        }
        if let Some(k) = pair_key {
            seen.insert(k);
        }
        out.push(DuplicateGroup {
            match_type: "Similar product".into(),
            match_key: key,
            reason: "Names normalize to the same product text and prices are close".into(),
            confidence: 86,
            products,
        });
    }
}

/// Scan the whole catalog for duplicate products.
///
/// Three independent passes — identical (case-insensitive) name, shared barcode,
/// shared SKU — each returning the colliding products with enough detail for the
/// admin to decide which to keep. A product may legitimately appear in more than
/// one section (e.g. same name *and* same barcode).
pub async fn find_duplicate_groups(
    pool: &SqlitePool,
    include_inactive: bool,
) -> AppResult<Vec<DuplicateGroup>> {
    let active_outer = if include_inactive {
        "p.deleted_at IS NULL"
    } else {
        "p.is_active = 1 AND p.deleted_at IS NULL"
    };
    let active_inner = if include_inactive {
        "deleted_at IS NULL"
    } else {
        "is_active = 1 AND deleted_at IS NULL"
    };

    let select_cols = "p.product_id, p.name, p.sku, p.barcode, c.name AS category_name, \
         p.is_active, p.image_path, COALESCE(pp.price_minor, 0) AS price_minor, \
         (SELECT COALESCE(SUM(CAST(sl.quantity_on_hand AS REAL)), 0) FROM stock_levels sl \
            WHERE sl.product_id = p.product_id) AS total_stock";

    let mut out: Vec<DuplicateGroup> = Vec::new();

    // ── 1. Exact name (case-insensitive) ────────────────────────────────────────
    let name_sql = format!(
        "SELECT {select_cols}, LOWER(TRIM(p.name)) AS gkey \
         FROM products p JOIN categories c ON c.category_id = p.category_id {PRICE_JOIN} \
         WHERE {active_outer} AND LOWER(TRIM(p.name)) IN ( \
             SELECT LOWER(TRIM(name)) FROM products WHERE {active_inner} \
             GROUP BY LOWER(TRIM(name)) HAVING COUNT(*) > 1 ) \
         ORDER BY gkey, p.name"
    );
    let name_rows = sqlx::query(&name_sql).fetch_all(pool).await?;
    out.extend(rows_to_groups(&name_rows, "Exact name"));

    // ── 2. Same barcode ─────────────────────────────────────────────────────────
    let bc_sql = format!(
        "SELECT {select_cols}, p.barcode AS gkey \
         FROM products p JOIN categories c ON c.category_id = p.category_id {PRICE_JOIN} \
         WHERE {active_outer} AND p.barcode IS NOT NULL AND TRIM(p.barcode) <> '' \
           AND p.barcode IN ( \
             SELECT barcode FROM products WHERE {active_inner} \
               AND barcode IS NOT NULL AND TRIM(barcode) <> '' \
             GROUP BY barcode HAVING COUNT(*) > 1 ) \
         ORDER BY gkey, p.name"
    );
    let bc_rows = sqlx::query(&bc_sql).fetch_all(pool).await?;
    out.extend(rows_to_groups(&bc_rows, "Same barcode"));

    // ── 3. Same SKU ─────────────────────────────────────────────────────────────
    let sku_sql = format!(
        "SELECT {select_cols}, p.sku AS gkey \
         FROM products p JOIN categories c ON c.category_id = p.category_id {PRICE_JOIN} \
         WHERE {active_outer} AND p.sku IS NOT NULL AND TRIM(p.sku) <> '' \
           AND p.sku IN ( \
             SELECT sku FROM products WHERE {active_inner} \
               AND sku IS NOT NULL AND TRIM(sku) <> '' \
             GROUP BY sku HAVING COUNT(*) > 1 ) \
         ORDER BY gkey, p.name"
    );
    let sku_rows = sqlx::query(&sku_sql).fetch_all(pool).await?;
    out.extend(rows_to_groups(&sku_rows, "Same SKU"));

    let similar_sql = format!(
        "SELECT {select_cols} \
         FROM products p JOIN categories c ON c.category_id = p.category_id {PRICE_JOIN} \
         WHERE {active_outer} \
         ORDER BY p.name"
    );
    let similar_rows = sqlx::query(&similar_sql).fetch_all(pool).await?;
    push_similar_name_groups(&mut out, &similar_rows);

    Ok(out)
}

/// Soft-delete (archive) a product: marks it inactive + deleted, leaving history
/// intact and recoverable. Returns the product's name for the audit/UI message.
pub async fn soft_delete_product(pool: &SqlitePool, product_id: &str) -> AppResult<String> {
    let name: Option<String> =
        sqlx::query_scalar("SELECT name FROM products WHERE product_id = ? AND deleted_at IS NULL")
            .bind(product_id)
            .fetch_optional(pool)
            .await?
            .flatten();
    let name = name.ok_or_else(|| {
        AppError::Validation(format!("Product {product_id} not found or already deleted"))
    })?;

    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE products SET is_active = 0, deleted_at = ?, updated_at = ?, sync_status = 'pending' \
         WHERE product_id = ?",
    )
    .bind(&now)
    .bind(&now)
    .bind(product_id)
    .execute(pool)
    .await?;
    Ok(name)
}

/// Merge `source` into `target`: combine stock per branch, re-point stock
/// movements, optionally reassign sale history, then archive the source.
///
/// This is the single source of truth for product merges — the AI
/// `merge_products` tool and the admin command both call it.
pub async fn merge_products(
    pool: &SqlitePool,
    source_id: &str,
    target_id: &str,
    transfer_history: bool,
) -> AppResult<MergeOutcome> {
    if source_id == target_id {
        return Err(AppError::Validation(
            "source and target product must be different".into(),
        ));
    }

    // Validate both products exist and the source is not already archived.
    let source_name: Option<String> =
        sqlx::query_scalar("SELECT name FROM products WHERE product_id = ? AND deleted_at IS NULL")
            .bind(source_id)
            .fetch_optional(pool)
            .await?
            .flatten();
    let source_name = source_name.ok_or_else(|| {
        AppError::Validation(format!(
            "Source product {source_id} not found or already deleted"
        ))
    })?;

    let target_name: Option<String> =
        sqlx::query_scalar("SELECT name FROM products WHERE product_id = ? AND deleted_at IS NULL")
            .bind(target_id)
            .fetch_optional(pool)
            .await?
            .flatten();
    let target_name = target_name.ok_or_else(|| {
        AppError::Validation(format!(
            "Target product {target_id} not found or already deleted"
        ))
    })?;

    let now = chrono::Utc::now().to_rfc3339();

    // ── Merge stock levels — re-point or sum each source row into the target ────
    let source_stock: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT stock_level_id, branch_id, quantity_on_hand FROM stock_levels WHERE product_id = ?",
    )
    .bind(source_id)
    .fetch_all(pool)
    .await?;

    for (sl_id, branch_id, qty_str) in &source_stock {
        let source_qty: f64 = qty_str.parse().unwrap_or(0.0);
        let updated = sqlx::query(
            "UPDATE stock_levels SET \
               quantity_on_hand = CAST(CAST(quantity_on_hand AS REAL) + ? AS TEXT), \
               last_movement_at = ?, updated_at = ?, sync_status = 'pending' \
             WHERE product_id = ? AND branch_id = ?",
        )
        .bind(source_qty)
        .bind(&now)
        .bind(&now)
        .bind(target_id)
        .bind(branch_id)
        .execute(pool)
        .await?
        .rows_affected();

        if updated > 0 {
            // Target already had stock in this branch — drop the merged source row.
            sqlx::query("DELETE FROM stock_levels WHERE stock_level_id = ?")
                .bind(sl_id)
                .execute(pool)
                .await?;
        } else {
            // Target had no row for this branch — re-point the source row.
            sqlx::query(
                "UPDATE stock_levels SET product_id = ?, updated_at = ?, sync_status = 'pending' \
                 WHERE stock_level_id = ?",
            )
            .bind(target_id)
            .bind(&now)
            .bind(sl_id)
            .execute(pool)
            .await?;
        }
    }

    // ── Transfer stock movements ────────────────────────────────────────────────
    sqlx::query("UPDATE stock_movements SET product_id = ? WHERE product_id = ?")
        .bind(target_id)
        .bind(source_id)
        .execute(pool)
        .await?;

    // ── Optionally reassign sale history ────────────────────────────────────────
    if transfer_history {
        sqlx::query("UPDATE sale_items SET product_id = ? WHERE product_id = ?")
            .bind(target_id)
            .bind(source_id)
            .execute(pool)
            .await?;
    }

    // ── Archive the source ──────────────────────────────────────────────────────
    sqlx::query(
        "UPDATE products SET is_active = 0, deleted_at = ?, updated_at = ?, sync_status = 'pending' \
         WHERE product_id = ?",
    )
    .bind(&now)
    .bind(&now)
    .bind(source_id)
    .execute(pool)
    .await?;

    Ok(MergeOutcome {
        source_name,
        target_name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::SqlitePool;

    async fn make_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("migrations");

        sqlx::query(
            "INSERT OR IGNORE INTO categories
             (category_id, name, sort_order, is_active, created_at, updated_at, version)
             VALUES ('CAT-DEDUP', 'Dedup Test', 1, 1, datetime('now'), datetime('now'), 1)",
        )
        .execute(&pool)
        .await
        .expect("seed category");
        pool
    }

    async fn insert_product(
        pool: &SqlitePool,
        id: &str,
        name: &str,
        barcode: Option<&str>,
        price_minor: i64,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO products
             (product_id, category_id, name, sku, barcode, track_inventory, is_active,
              currency, created_at, updated_at, version)
             VALUES (?, 'CAT-DEDUP', ?, NULL, ?, 1, 1, 'BHD', ?, ?, 1)",
        )
        .bind(id)
        .bind(name)
        .bind(barcode)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await?;

        sqlx::query(
            "INSERT INTO product_prices
             (price_id, product_id, price_type, price_minor, currency, effective_from,
              created_by_user_id, created_at)
             VALUES (?, ?, 'selling', ?, 'BHD', ?, 'TEST', ?)",
        )
        .bind(format!("PRICE-{id}"))
        .bind(id)
        .bind(price_minor)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await?;
        Ok(())
    }

    #[tokio::test]
    async fn active_products_cannot_share_the_same_barcode() {
        let pool = make_pool().await;
        insert_product(&pool, "P-ONE", "Cola", Some("6290001112223"), 1000)
            .await
            .expect("first product");

        let duplicate =
            insert_product(&pool, "P-TWO", "Cola Copy", Some("6290001112223"), 1000).await;

        assert!(
            duplicate.is_err(),
            "database must reject a second active product with the same barcode"
        );
    }

    #[tokio::test]
    async fn scanner_flags_similar_names_with_close_prices() {
        let pool = make_pool().await;
        insert_product(&pool, "P-ONE", "Coca Cola 330ml", Some("111"), 1000)
            .await
            .expect("first product");
        insert_product(&pool, "P-TWO", "Coca-Cola 330 ml", Some("222"), 1010)
            .await
            .expect("second product");

        let groups = find_duplicate_groups(&pool, false)
            .await
            .expect("scan duplicates");

        assert!(
            groups.iter().any(|g| {
                g.match_type == "Similar product"
                    && g.products.iter().any(|p| p.product_id == "P-ONE")
                    && g.products.iter().any(|p| p.product_id == "P-TWO")
            }),
            "similar names with near-identical prices must be shown to the merge wizard"
        );
    }
}
