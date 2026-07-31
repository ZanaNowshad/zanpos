use crate::errors::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogBranch {
    pub id: String,
    pub name: String,
    pub address: Option<String>,
    pub phone: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogCategory {
    pub id: String,
    pub name: String,
    pub name_ar: Option<String>,
    pub sort_order: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogProduct {
    pub id: String,
    pub category_id: String,
    pub name: String,
    pub name_ar: Option<String>,
    pub description: Option<String>,
    pub description_ar: Option<String>,
    pub price_minor: i64,
    pub currency: String,
    pub image_url: Option<String>,
    pub quantity_decimals: u8,
    /// Coarse public state only. Exact inventory quantities never leave the POS.
    pub availability: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogSnapshot {
    pub schema_version: u32,
    pub generated_at: String,
    pub branch: CatalogBranch,
    pub currency: String,
    pub currency_decimals: u8,
    pub categories: Vec<CatalogCategory>,
    pub products: Vec<CatalogProduct>,
}

pub fn currency_decimals(currency: &str) -> u8 {
    match currency.to_ascii_uppercase().as_str() {
        "BHD" | "IQD" | "JOD" | "KWD" | "LYD" | "OMR" | "TND" => 3,
        "BIF" | "CLP" | "DJF" | "GNF" | "ISK" | "JPY" | "KMF" | "KRW" | "PYG" | "RWF" | "UGX"
        | "VND" | "VUV" | "XAF" | "XOF" | "XPF" => 0,
        _ => 2,
    }
}

fn public_image_url(value: Option<String>) -> Option<String> {
    value.filter(|url| {
        let lower = url.to_ascii_lowercase();
        lower.starts_with("https://") || lower.starts_with("http://localhost:")
    })
}

pub async fn build_catalog_snapshot(pool: &SqlitePool) -> AppResult<CatalogSnapshot> {
    let branch = sqlx::query(
        "SELECT branch_id, name, currency, address, phone
         FROM branches WHERE is_active=1 AND deleted_at IS NULL
         ORDER BY created_at LIMIT 1",
    )
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::Validation("No active branch is configured".into()))?;
    let branch_id: String = branch.get("branch_id");
    let currency: String = branch.get("currency");

    let category_rows = sqlx::query(
        "SELECT DISTINCT c.category_id, c.name, c.sort_order
         FROM categories c
         JOIN products p ON p.category_id=c.category_id
         JOIN storefront_products sp ON sp.product_id=p.product_id AND sp.is_visible=1
         WHERE c.is_active=1 AND c.deleted_at IS NULL
           AND p.is_active=1 AND p.deleted_at IS NULL
         ORDER BY c.sort_order, c.name",
    )
    .fetch_all(pool)
    .await?;

    let product_rows = sqlx::query(
        "SELECT p.product_id, p.category_id, p.name, sp.name_ar, p.description,
                sp.description_ar,
                pp.price_minor, pp.currency, sp.public_image_url,
                p.track_inventory, p.allow_decimal_quantity, sl.quantity_on_hand
         FROM products p
         JOIN categories c ON c.category_id=p.category_id
         JOIN storefront_products sp ON sp.product_id=p.product_id AND sp.is_visible=1
         JOIN product_prices pp ON pp.price_id=(
             SELECT candidate.price_id FROM product_prices candidate
             WHERE candidate.product_id=p.product_id
               AND candidate.price_type='selling'
               AND (candidate.branch_id=? OR candidate.branch_id IS NULL)
               AND candidate.effective_from <= strftime('%Y-%m-%dT%H:%M:%fZ','now')
               AND (candidate.effective_to IS NULL OR
                    candidate.effective_to > strftime('%Y-%m-%dT%H:%M:%fZ','now'))
             ORDER BY CASE WHEN candidate.branch_id=? THEN 0 ELSE 1 END,
                      candidate.effective_from DESC, candidate.price_id DESC
             LIMIT 1
         )
         LEFT JOIN stock_levels sl ON sl.product_id=p.product_id AND sl.branch_id=?
         WHERE p.is_active=1 AND p.deleted_at IS NULL
           AND c.is_active=1 AND c.deleted_at IS NULL
         ORDER BY c.sort_order, p.name, p.product_id",
    )
    .bind(&branch_id)
    .bind(&branch_id)
    .bind(&branch_id)
    .fetch_all(pool)
    .await?;

    let categories = category_rows
        .into_iter()
        .map(|row| CatalogCategory {
            id: row.get("category_id"),
            name: row.get("name"),
            name_ar: None,
            sort_order: row.get("sort_order"),
        })
        .collect();
    let products = product_rows
        .into_iter()
        .map(|row| {
            let tracked: i64 = row.get("track_inventory");
            let allows_decimal: i64 = row.get("allow_decimal_quantity");
            let quantity = row
                .try_get::<Option<String>, _>("quantity_on_hand")
                .ok()
                .flatten()
                .and_then(|v| v.parse::<f64>().ok())
                .unwrap_or(0.0);
            CatalogProduct {
                id: row.get("product_id"),
                category_id: row.get("category_id"),
                name: row.get("name"),
                name_ar: row.get("name_ar"),
                description: row.get("description"),
                description_ar: row.get("description_ar"),
                price_minor: row.get("price_minor"),
                currency: row.get("currency"),
                image_url: public_image_url(row.get("public_image_url")),
                quantity_decimals: if allows_decimal == 0 { 0 } else { 3 },
                availability: if tracked == 0 || quantity > 0.0 {
                    "available".into()
                } else {
                    "out_of_stock".into()
                },
            }
        })
        .collect();

    Ok(CatalogSnapshot {
        schema_version: 1,
        generated_at: chrono::Utc::now().to_rfc3339(),
        branch: CatalogBranch {
            id: branch_id,
            name: branch.get("name"),
            address: branch.get("address"),
            phone: branch.get("phone"),
        },
        currency_decimals: currency_decimals(&currency),
        currency,
        categories,
        products,
    })
}
