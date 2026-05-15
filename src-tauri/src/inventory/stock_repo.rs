/// Read-only stock queries — used by inventory commands and AI tools.
use sqlx::{SqlitePool, Row};
use serde::Serialize;
use crate::errors::AppResult;

const BRANCH_ID: &str = "01JBRANCH0000000000000001";

#[derive(Debug, Serialize)]
pub struct StockLevel {
    pub product_id:      String,
    pub product_name:    String,
    pub sku:             Option<String>,
    pub category_name:   String,
    pub quantity_on_hand: String,
    pub reorder_point:   i64,
    pub is_low_stock:    bool,
    pub is_out_of_stock: bool,
    pub track_inventory: bool,
}

#[derive(Debug, Serialize)]
pub struct StockMovementRow {
    pub movement_id:     String,
    pub movement_type:   String,
    pub quantity_delta:  String,
    pub quantity_after:  String,
    pub reference_type:  Option<String>,
    pub reference_id:    Option<String>,
    pub notes:           Option<String>,
    pub created_at:      String,
}

/// All tracked products with their current stock level.
pub async fn get_all_levels(pool: &SqlitePool) -> AppResult<Vec<StockLevel>> {
    let rows = sqlx::query(
        "SELECT p.product_id, p.name, p.sku, p.reorder_point, p.track_inventory,
                c.name AS category_name,
                COALESCE(sl.quantity_on_hand, '0') AS quantity_on_hand
         FROM products p
         JOIN categories c ON c.category_id = p.category_id
         LEFT JOIN stock_levels sl ON sl.product_id = p.product_id AND sl.branch_id = ?
         WHERE p.is_active = 1 AND p.track_inventory = 1
         ORDER BY p.name"
    )
    .bind(BRANCH_ID)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(|r| {
        let qty_str: String = r.get("quantity_on_hand");
        let qty: f64 = qty_str.parse().unwrap_or(0.0);
        let reorder_point: i64 = r.get("reorder_point");
        let track: i64 = r.get("track_inventory");
        StockLevel {
            product_id:      r.get("product_id"),
            product_name:    r.get("name"),
            sku:             r.get("sku"),
            category_name:   r.get("category_name"),
            quantity_on_hand: qty_str,
            reorder_point,
            is_low_stock:    qty <= reorder_point as f64,
            is_out_of_stock: qty <= 0.0,
            track_inventory: track != 0,
        }
    }).collect())
}

/// Products at or below reorder point.
pub async fn get_low_stock(pool: &SqlitePool) -> AppResult<Vec<StockLevel>> {
    let all = get_all_levels(pool).await?;
    Ok(all.into_iter().filter(|s| s.is_low_stock).collect())
}

/// Recent movements for one product (last 50).
pub async fn get_movements(pool: &SqlitePool, product_id: &str) -> AppResult<Vec<StockMovementRow>> {
    let rows = sqlx::query(
        "SELECT movement_id, movement_type, quantity_delta, quantity_after,
                reference_type, reference_id, notes, created_at
         FROM stock_movements
         WHERE product_id = ?
         ORDER BY created_at DESC
         LIMIT 50"
    )
    .bind(product_id)
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(|r| StockMovementRow {
        movement_id:    r.get("movement_id"),
        movement_type:  r.get("movement_type"),
        quantity_delta: r.get("quantity_delta"),
        quantity_after: r.get("quantity_after"),
        reference_type: r.get("reference_type"),
        reference_id:   r.get("reference_id"),
        notes:          r.get("notes"),
        created_at:     r.get("created_at"),
    }).collect())
}
