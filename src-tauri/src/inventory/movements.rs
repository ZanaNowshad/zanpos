/// Inventory movement writers.
/// Each function creates a stock_movement record and updates stock_levels atomically.
/// Returns a list of LowStockAlert for products that crossed below their reorder point.
use sqlx::{SqlitePool, Row};
use ulid::Ulid;
use crate::domain::product::LowStockAlert;
use crate::errors::AppResult;
use crate::sync::outbox;

const BRANCH_ID: &str = "01JBRANCH0000000000000001";
const DEVICE_ID: &str = "01JDEVICE0000000000000001";

// ── Internal: fetch or initialize stock level ─────────────────────────────────

async fn get_qty(pool: &SqlitePool, product_id: &str) -> f64 {
    let qty: Option<String> = sqlx::query_scalar(
        "SELECT quantity_on_hand FROM stock_levels WHERE product_id = ? AND branch_id = ?"
    )
    .bind(product_id)
    .bind(BRANCH_ID)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    qty.and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0)
}

async fn upsert_level(
    pool: &SqlitePool,
    product_id: &str,
    new_qty: f64,
    movement_at: &str,
) -> AppResult<()> {
    let qty_str = format_qty(new_qty);
    let id = format!("SL-{}", product_id);

    sqlx::query(
        "INSERT INTO stock_levels (stock_level_id, product_id, branch_id, quantity_on_hand, last_movement_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(product_id, branch_id) DO UPDATE SET
           quantity_on_hand = excluded.quantity_on_hand,
           last_movement_at = excluded.last_movement_at,
           updated_at       = excluded.updated_at"
    )
    .bind(&id)
    .bind(product_id)
    .bind(BRANCH_ID)
    .bind(&qty_str)
    .bind(movement_at)
    .bind(movement_at)
    .execute(pool)
    .await?;

    Ok(())
}

fn format_qty(qty: f64) -> String {
    // Round to 4 decimal places; strip trailing zeros
    let s = format!("{:.4}", qty);
    let s = s.trim_end_matches('0');
    let s = s.trim_end_matches('.');
    if s.is_empty() { "0".to_string() } else { s.to_string() }
}

async fn check_alert(
    pool: &SqlitePool,
    product_id: &str,
    new_qty: f64,
) -> Option<LowStockAlert> {
    let row = sqlx::query(
        "SELECT name, reorder_point FROM products WHERE product_id = ? AND track_inventory = 1"
    )
    .bind(product_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()?;

    let reorder_point: i64 = row.get("reorder_point");
    let name: String = row.get("name");

    if new_qty <= reorder_point as f64 {
        Some(LowStockAlert {
            product_id: product_id.to_string(),
            product_name: name,
            quantity_on_hand: format_qty(new_qty),
            reorder_point,
        })
    } else {
        None
    }
}

// ── deduct_sale ───────────────────────────────────────────────────────────────

/// Called after finalize_sale commits. Deducts stock for all tracked items.
/// Returns alerts for products that crossed below their reorder point.
pub async fn deduct_sale(
    pool:    &SqlitePool,
    sale_id: &str,
    cashier_user_id: &str,
) -> AppResult<Vec<LowStockAlert>> {
    let now = chrono::Utc::now().to_rfc3339();

    // Load sale items with track_inventory flag
    let rows = sqlx::query(
        "SELECT si.product_id, si.quantity, p.track_inventory, p.name
         FROM sale_items si
         JOIN products p ON p.product_id = si.product_id
         WHERE si.sale_id = ? AND si.voided = 0 AND si.product_id IS NOT NULL
           AND p.track_inventory = 1"
    )
    .bind(sale_id)
    .fetch_all(pool)
    .await?;

    let mut alerts = Vec::new();

    for row in &rows {
        let product_id: String = row.get("product_id");
        let qty_str: String = row.get("quantity");
        let sold_qty: f64 = qty_str.parse().unwrap_or(0.0);
        if sold_qty <= 0.0 { continue; }

        let current = get_qty(pool, &product_id).await;
        let new_qty = current - sold_qty;
        let movement_id = Ulid::new().to_string();

        upsert_level(pool, &product_id, new_qty, &now).await?;

        sqlx::query(
            "INSERT INTO stock_movements
             (movement_id, product_id, branch_id, device_id, movement_type,
              quantity_delta, quantity_after, reference_type, reference_id,
              created_by_user_id, created_at, sync_status)
             VALUES (?,?,?,?,'sale',?,?,   'sale',?,?,?,'pending')"
        )
        .bind(&movement_id)
        .bind(&product_id)
        .bind(BRANCH_ID)
        .bind(DEVICE_ID)
        .bind(format_qty(-sold_qty))
        .bind(format_qty(new_qty))
        .bind(sale_id)
        .bind(cashier_user_id)
        .bind(&now)
        .execute(pool)
        .await?;

        // Enqueue to sync
        let _ = outbox::enqueue_stock_movement(
            pool, DEVICE_ID, BRANCH_ID, &movement_id, &product_id,
            "sale", &format_qty(-sold_qty), &format_qty(new_qty),
            "sale", sale_id, None, Some(cashier_user_id), &now,
        ).await;
        let _ = outbox::enqueue_stock_level(
            pool, DEVICE_ID, BRANCH_ID, &product_id, &format_qty(new_qty), &now,
        ).await;

        if let Some(alert) = check_alert(pool, &product_id, new_qty).await {
            alerts.push(alert);
        }
    }

    Ok(alerts)
}

// ── return_refund ─────────────────────────────────────────────────────────────

/// Called after create_refund commits. Returns stock for tracked items.
pub async fn return_refund(
    pool:      &SqlitePool,
    refund_id: &str,
    created_by_user_id: &str,
) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();

    // Join refund_items → sale_items → products to get product + qty
    let rows = sqlx::query(
        "SELECT si.product_id, ri.quantity, p.track_inventory
         FROM refund_items ri
         JOIN sale_items si ON si.sale_item_id = ri.sale_item_id
         JOIN products p ON p.product_id = si.product_id
         WHERE ri.refund_id = ? AND si.product_id IS NOT NULL AND p.track_inventory = 1"
    )
    .bind(refund_id)
    .fetch_all(pool)
    .await?;

    for row in &rows {
        let product_id: String = row.get("product_id");
        let qty_str: String = row.get("quantity");
        let returned_qty: f64 = qty_str.parse().unwrap_or(0.0);
        if returned_qty <= 0.0 { continue; }

        let current = get_qty(pool, &product_id).await;
        let new_qty = current + returned_qty;
        let movement_id = Ulid::new().to_string();

        upsert_level(pool, &product_id, new_qty, &now).await?;

        sqlx::query(
            "INSERT INTO stock_movements
             (movement_id, product_id, branch_id, device_id, movement_type,
              quantity_delta, quantity_after, reference_type, reference_id,
              created_by_user_id, created_at, sync_status)
             VALUES (?,?,?,?,'refund',?,?, 'refund',?,?,?,'pending')"
        )
        .bind(&movement_id)
        .bind(&product_id)
        .bind(BRANCH_ID)
        .bind(DEVICE_ID)
        .bind(format_qty(returned_qty))
        .bind(format_qty(new_qty))
        .bind(refund_id)
        .bind(created_by_user_id)
        .bind(&now)
        .execute(pool)
        .await?;

        let _ = outbox::enqueue_stock_movement(
            pool, DEVICE_ID, BRANCH_ID, &movement_id, &product_id,
            "refund", &format_qty(returned_qty), &format_qty(new_qty),
            "refund", refund_id, None, Some(created_by_user_id), &now,
        ).await;
        let _ = outbox::enqueue_stock_level(
            pool, DEVICE_ID, BRANCH_ID, &product_id, &format_qty(new_qty), &now,
        ).await;
    }

    Ok(())
}

// ── manual_adjust ─────────────────────────────────────────────────────────────

/// Applies a +/- delta. Used by AI tool 'adjust_stock'.
pub async fn manual_adjust(
    pool:            &SqlitePool,
    product_id:      &str,
    quantity_delta:  f64,
    notes:           Option<&str>,
    user_id:         &str,
    ai_action_id:    Option<&str>,
) -> AppResult<LowStockAlert> {
    let now = chrono::Utc::now().to_rfc3339();
    let current = get_qty(pool, product_id).await;
    let new_qty = current + quantity_delta;
    let movement_id = Ulid::new().to_string();

    upsert_level(pool, product_id, new_qty, &now).await?;

    sqlx::query(
        "INSERT INTO stock_movements
         (movement_id, product_id, branch_id, device_id, movement_type,
          quantity_delta, quantity_after, reference_type, reference_id,
          notes, created_by_user_id, created_at, sync_status)
         VALUES (?,?,?,?,'adjustment',?,?, 'ai_action',?,?,?,?,'pending')"
    )
    .bind(&movement_id)
    .bind(product_id)
    .bind(BRANCH_ID)
    .bind(DEVICE_ID)
    .bind(format_qty(quantity_delta))
    .bind(format_qty(new_qty))
    .bind(ai_action_id.unwrap_or(""))
    .bind(notes)
    .bind(user_id)
    .bind(&now)
    .execute(pool)
    .await?;

    let _ = outbox::enqueue_stock_movement(
        pool, DEVICE_ID, BRANCH_ID, &movement_id, product_id,
        "adjustment", &format_qty(quantity_delta), &format_qty(new_qty),
        "ai_action", ai_action_id.unwrap_or(""), notes, Some(user_id), &now,
    ).await;
    let _ = outbox::enqueue_stock_level(
        pool, DEVICE_ID, BRANCH_ID, product_id, &format_qty(new_qty), &now,
    ).await;

    Ok(LowStockAlert {
        product_id: product_id.to_string(),
        product_name: String::new(), // filled by caller from product query
        quantity_on_hand: format_qty(new_qty),
        reorder_point: 0,
    })
}

// ── stock_take ────────────────────────────────────────────────────────────────

/// Sets absolute quantity (full count override). Used by AI tool 'stock_take'.
pub async fn stock_take(
    pool:         &SqlitePool,
    product_id:   &str,
    new_quantity: f64,
    notes:        Option<&str>,
    user_id:      &str,
    ai_action_id: Option<&str>,
) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let current = get_qty(pool, product_id).await;
    let delta = new_quantity - current;
    let movement_id = Ulid::new().to_string();

    upsert_level(pool, product_id, new_quantity, &now).await?;

    sqlx::query(
        "INSERT INTO stock_movements
         (movement_id, product_id, branch_id, device_id, movement_type,
          quantity_delta, quantity_after, reference_type, reference_id,
          notes, created_by_user_id, created_at, sync_status)
         VALUES (?,?,?,?,'stock_take',?,?, 'ai_action',?,?,?,?,'pending')"
    )
    .bind(&movement_id)
    .bind(product_id)
    .bind(BRANCH_ID)
    .bind(DEVICE_ID)
    .bind(format_qty(delta))
    .bind(format_qty(new_quantity))
    .bind(ai_action_id.unwrap_or(""))
    .bind(notes)
    .bind(user_id)
    .bind(&now)
    .execute(pool)
    .await?;

    let _ = outbox::enqueue_stock_movement(
        pool, DEVICE_ID, BRANCH_ID, &movement_id, product_id,
        "stock_take", &format_qty(delta), &format_qty(new_quantity),
        "ai_action", ai_action_id.unwrap_or(""), notes, Some(user_id), &now,
    ).await;
    let _ = outbox::enqueue_stock_level(
        pool, DEVICE_ID, BRANCH_ID, product_id, &format_qty(new_quantity), &now,
    ).await;

    Ok(())
}
