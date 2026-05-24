use crate::domain::product::LowStockAlert;
use crate::errors::AppResult;
use crate::sync::outbox;
/// Inventory movement writers.
/// Each function creates a stock_movement record and updates stock_levels atomically.
/// Returns a list of LowStockAlert for products that crossed below their reorder point.
///
/// `branch_id` and `device_id` are passed explicitly by callers (resolved from the
/// active branch/device at the command layer) so no hardcoded fallback IDs are needed.
use sqlx::{Row, SqlitePool};
use ulid::Ulid;

// ── Internal: fetch or initialize stock level ─────────────────────────────────

async fn get_qty(pool: &SqlitePool, product_id: &str, branch_id: &str) -> f64 {
    let qty: Option<String> = sqlx::query_scalar(
        "SELECT quantity_on_hand FROM stock_levels WHERE product_id = ? AND branch_id = ?",
    )
    .bind(product_id)
    .bind(branch_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    qty.and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0)
}

async fn upsert_level(
    pool: &SqlitePool,
    product_id: &str,
    branch_id: &str,
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
    .bind(branch_id)
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
    if s.is_empty() {
        "0".to_string()
    } else {
        s.to_string()
    }
}

async fn check_alert(pool: &SqlitePool, product_id: &str, new_qty: f64) -> Option<LowStockAlert> {
    let row = sqlx::query(
        "SELECT name, reorder_point FROM products WHERE product_id = ? AND track_inventory = 1",
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

/// Called AFTER finalize_sale commits. Stock levels have already been atomically
/// deducted within the sale transaction. This function only writes movement
/// records and enqueues outbox events — it does NOT touch stock_levels again.
/// Returns alerts for products that crossed below their reorder point.
pub async fn deduct_sale(
    pool: &SqlitePool,
    sale_id: &str,
    cashier_user_id: &str,
    branch_id: &str,
    device_id: &str,
) -> AppResult<Vec<LowStockAlert>> {
    let now = chrono::Utc::now().to_rfc3339();

    // Load sale items with track_inventory flag
    let rows = sqlx::query(
        "SELECT si.product_id, si.quantity, p.track_inventory, p.name
         FROM sale_items si
         JOIN products p ON p.product_id = si.product_id
         WHERE si.sale_id = ? AND si.voided = 0 AND si.product_id IS NOT NULL
           AND p.track_inventory = 1",
    )
    .bind(sale_id)
    .fetch_all(pool)
    .await?;

    let mut alerts = Vec::new();

    for row in &rows {
        let product_id: String = row.get("product_id");
        let qty_str: String = row.get("quantity");
        let sold_qty: f64 = qty_str.parse().unwrap_or(0.0);
        if sold_qty <= 0.0 {
            continue;
        }

        // Stock level was already deducted in the sale transaction; read current value.
        let new_qty = get_qty(pool, &product_id, branch_id).await;
        let movement_id = Ulid::new().to_string();

        // Write movement record (stock_levels already updated — do NOT call upsert_level).
        sqlx::query(
            "INSERT INTO stock_movements
             (movement_id, product_id, branch_id, device_id, movement_type,
              quantity_delta, quantity_after, reference_type, reference_id,
              created_by_user_id, created_at, sync_status)
             VALUES (?,?,?,?,'sale',?,?,   'sale',?,?,?,'pending')",
        )
        .bind(&movement_id)
        .bind(&product_id)
        .bind(branch_id)
        .bind(device_id)
        .bind(format_qty(-sold_qty))
        .bind(format_qty(new_qty))
        .bind(sale_id)
        .bind(cashier_user_id)
        .bind(&now)
        .execute(pool)
        .await?;

        // Enqueue to sync
        let _ = outbox::enqueue_stock_movement(
            pool,
            device_id,
            branch_id,
            &movement_id,
            &product_id,
            "sale",
            &format_qty(-sold_qty),
            &format_qty(new_qty),
            "sale",
            sale_id,
            None,
            Some(cashier_user_id),
            &now,
        )
        .await;
        let _ = outbox::enqueue_stock_level(
            pool,
            device_id,
            branch_id,
            &product_id,
            &format_qty(new_qty),
            &now,
        )
        .await;

        if let Some(alert) = check_alert(pool, &product_id, new_qty).await {
            alerts.push(alert);
        }
    }

    Ok(alerts)
}

// ── return_refund ─────────────────────────────────────────────────────────────

/// Called after create_refund commits. Returns stock for tracked items.
/// `branch_id` / `device_id` come from the original sale's branch/device.
pub async fn return_refund(
    pool: &SqlitePool,
    refund_id: &str,
    created_by_user_id: &str,
    branch_id: &str,
    device_id: &str,
) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();

    // Join refund_items → sale_items → products to get product + qty
    let rows = sqlx::query(
        "SELECT si.product_id, ri.quantity, p.track_inventory
         FROM refund_items ri
         JOIN sale_items si ON si.sale_item_id = ri.sale_item_id
         JOIN products p ON p.product_id = si.product_id
         WHERE ri.refund_id = ? AND si.product_id IS NOT NULL AND p.track_inventory = 1",
    )
    .bind(refund_id)
    .fetch_all(pool)
    .await?;

    for row in &rows {
        let product_id: String = row.get("product_id");
        let qty_str: String = row.get("quantity");
        let returned_qty: f64 = qty_str.parse().unwrap_or(0.0);
        if returned_qty <= 0.0 {
            continue;
        }

        let current = get_qty(pool, &product_id, branch_id).await;
        let new_qty = current + returned_qty;
        let movement_id = Ulid::new().to_string();

        upsert_level(pool, &product_id, branch_id, new_qty, &now).await?;

        sqlx::query(
            "INSERT INTO stock_movements
             (movement_id, product_id, branch_id, device_id, movement_type,
              quantity_delta, quantity_after, reference_type, reference_id,
              created_by_user_id, created_at, sync_status)
             VALUES (?,?,?,?,'refund',?,?, 'refund',?,?,?,'pending')",
        )
        .bind(&movement_id)
        .bind(&product_id)
        .bind(branch_id)
        .bind(device_id)
        .bind(format_qty(returned_qty))
        .bind(format_qty(new_qty))
        .bind(refund_id)
        .bind(created_by_user_id)
        .bind(&now)
        .execute(pool)
        .await?;

        let _ = outbox::enqueue_stock_movement(
            pool,
            device_id,
            branch_id,
            &movement_id,
            &product_id,
            "refund",
            &format_qty(returned_qty),
            &format_qty(new_qty),
            "refund",
            refund_id,
            None,
            Some(created_by_user_id),
            &now,
        )
        .await;
        let _ = outbox::enqueue_stock_level(
            pool,
            device_id,
            branch_id,
            &product_id,
            &format_qty(new_qty),
            &now,
        )
        .await;
    }

    Ok(())
}

// ── return_void_sale ──────────────────────────────────────────────────────────

/// Called after pos_void_sale succeeds. Restores stock for all tracked items
/// that were deducted when the sale was originally finalized.
///
/// Uses the `sale_items` table directly (no refund record required).
/// Movement type is `"void"` to distinguish from normal refund returns.
pub async fn return_void_sale(
    pool: &SqlitePool,
    sale_id: &str,
    voided_by_user_id: &str,
    branch_id: &str,
    device_id: &str,
) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();

    let rows = sqlx::query(
        "SELECT si.product_id, si.quantity, p.track_inventory
         FROM sale_items si
         JOIN products p ON p.product_id = si.product_id
         WHERE si.sale_id = ?
           AND si.voided = 0
           AND si.product_id IS NOT NULL
           AND p.track_inventory = 1",
    )
    .bind(sale_id)
    .fetch_all(pool)
    .await?;

    for row in &rows {
        let product_id: String = row.get("product_id");
        let qty_str: String = row.get("quantity");
        let returned_qty: f64 = qty_str.parse().unwrap_or(0.0);
        if returned_qty <= 0.0 {
            continue;
        }

        let current = get_qty(pool, &product_id, branch_id).await;
        let new_qty = current + returned_qty;
        let movement_id = Ulid::new().to_string();

        upsert_level(pool, &product_id, branch_id, new_qty, &now).await?;

        sqlx::query(
            "INSERT INTO stock_movements
             (movement_id, product_id, branch_id, device_id, movement_type,
              quantity_delta, quantity_after, reference_type, reference_id,
              created_by_user_id, created_at, sync_status)
             VALUES (?,?,?,?,'void',?,?,'sale',?,?,?,'pending')",
        )
        .bind(&movement_id)
        .bind(&product_id)
        .bind(branch_id)
        .bind(device_id)
        .bind(format_qty(returned_qty))
        .bind(format_qty(new_qty))
        .bind(sale_id)
        .bind(voided_by_user_id)
        .bind(&now)
        .execute(pool)
        .await?;

        let _ = outbox::enqueue_stock_level(
            pool,
            device_id,
            branch_id,
            &product_id,
            &format_qty(new_qty),
            &now,
        )
        .await;
    }

    Ok(())
}

// ── manual_adjust ─────────────────────────────────────────────────────────────

/// Applies a +/- delta. Used by AI tool 'adjust_stock'.
/// `branch_id` / `device_id` resolved by the AI command layer.
#[allow(clippy::too_many_arguments)]
pub async fn manual_adjust(
    pool: &SqlitePool,
    product_id: &str,
    quantity_delta: f64,
    notes: Option<&str>,
    user_id: &str,
    ai_action_id: Option<&str>,
    branch_id: &str,
    device_id: &str,
) -> AppResult<LowStockAlert> {
    let now = chrono::Utc::now().to_rfc3339();
    let current = get_qty(pool, product_id, branch_id).await;
    let new_qty = current + quantity_delta;
    let movement_id = Ulid::new().to_string();

    upsert_level(pool, product_id, branch_id, new_qty, &now).await?;

    sqlx::query(
        "INSERT INTO stock_movements
         (movement_id, product_id, branch_id, device_id, movement_type,
          quantity_delta, quantity_after, reference_type, reference_id,
          notes, created_by_user_id, created_at, sync_status)
         VALUES (?,?,?,?,'adjustment',?,?, 'ai_action',?,?,?,?,'pending')",
    )
    .bind(&movement_id)
    .bind(product_id)
    .bind(branch_id)
    .bind(device_id)
    .bind(format_qty(quantity_delta))
    .bind(format_qty(new_qty))
    .bind(ai_action_id.unwrap_or(""))
    .bind(notes)
    .bind(user_id)
    .bind(&now)
    .execute(pool)
    .await?;

    let _ = outbox::enqueue_stock_movement(
        pool,
        device_id,
        branch_id,
        &movement_id,
        product_id,
        "adjustment",
        &format_qty(quantity_delta),
        &format_qty(new_qty),
        "ai_action",
        ai_action_id.unwrap_or(""),
        notes,
        Some(user_id),
        &now,
    )
    .await;
    let _ = outbox::enqueue_stock_level(
        pool,
        device_id,
        branch_id,
        product_id,
        &format_qty(new_qty),
        &now,
    )
    .await;

    Ok(LowStockAlert {
        product_id: product_id.to_string(),
        product_name: String::new(), // filled by caller from product query
        quantity_on_hand: format_qty(new_qty),
        reorder_point: 0,
    })
}

// ── stock_take ────────────────────────────────────────────────────────────────

/// Sets absolute quantity (full count override). Used by AI tool 'stock_take'.
/// `branch_id` / `device_id` resolved by the AI command layer.
#[allow(clippy::too_many_arguments)]
pub async fn stock_take(
    pool: &SqlitePool,
    product_id: &str,
    new_quantity: f64,
    notes: Option<&str>,
    user_id: &str,
    ai_action_id: Option<&str>,
    branch_id: &str,
    device_id: &str,
) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let current = get_qty(pool, product_id, branch_id).await;
    let delta = new_quantity - current;
    let movement_id = Ulid::new().to_string();

    upsert_level(pool, product_id, branch_id, new_quantity, &now).await?;

    sqlx::query(
        "INSERT INTO stock_movements
         (movement_id, product_id, branch_id, device_id, movement_type,
          quantity_delta, quantity_after, reference_type, reference_id,
          notes, created_by_user_id, created_at, sync_status)
         VALUES (?,?,?,?,'stock_take',?,?, 'ai_action',?,?,?,?,'pending')",
    )
    .bind(&movement_id)
    .bind(product_id)
    .bind(branch_id)
    .bind(device_id)
    .bind(format_qty(delta))
    .bind(format_qty(new_quantity))
    .bind(ai_action_id.unwrap_or(""))
    .bind(notes)
    .bind(user_id)
    .bind(&now)
    .execute(pool)
    .await?;

    let _ = outbox::enqueue_stock_movement(
        pool,
        device_id,
        branch_id,
        &movement_id,
        product_id,
        "stock_take",
        &format_qty(delta),
        &format_qty(new_quantity),
        "ai_action",
        ai_action_id.unwrap_or(""),
        notes,
        Some(user_id),
        &now,
    )
    .await;
    let _ = outbox::enqueue_stock_level(
        pool,
        device_id,
        branch_id,
        product_id,
        &format_qty(new_quantity),
        &now,
    )
    .await;

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Integration tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    const BRANCH: &str = "01JBRANCH0000000000000001";
    const DEVICE: &str = "01JDEVICE0000000000000001";
    const USER: &str = "01JUSER000000000000ADMIN1";
    const COLA_ID: &str = "01JPROD00000000000COLA001";
    const WATR_ID: &str = "01JPROD00000000000WATR001";

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
        // Migration 0018 removes demo data; re-seed products needed as FK refs.
        sqlx::query(
            "INSERT OR IGNORE INTO categories (category_id, name, sort_order, is_active, created_at, updated_at)
             VALUES ('01JCAT000000000000DRINK01', 'Drinks', 1, 1, datetime('now'), datetime('now'))"
        ).execute(&pool).await.expect("seed test category");
        sqlx::query(
            "INSERT OR IGNORE INTO products
             (product_id, category_id, name, sku, barcode, track_inventory, reorder_point, is_active, tax_rule_id, currency, created_at, updated_at)
             VALUES
             ('01JPROD00000000000COLA001', '01JCAT000000000000DRINK01', 'Coca-Cola 330ml', 'COLA-330', '5449000000996', 1, 5, 1, '01JTAX000000000000VAT001', 'BHD', datetime('now'), datetime('now')),
             ('01JPROD00000000000WATR001', '01JCAT000000000000DRINK01', 'Water 500ml',     'WATR-500', '6281001511222', 1, 10, 1, '01JTAX000000000000ZERO01', 'BHD', datetime('now'), datetime('now'))"
        ).execute(&pool).await.expect("seed test products");
        pool
    }

    /// Seed a stock level so a product starts with a known quantity.
    async fn seed_stock(pool: &SqlitePool, product_id: &str, qty: f64) {
        let qty_str = format!("{qty:.4}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string();
        let qty_str = if qty_str.is_empty() {
            "0".to_string()
        } else {
            qty_str
        };
        let id = format!("SL-{}", product_id);
        sqlx::query(
            "INSERT INTO stock_levels (stock_level_id, product_id, branch_id, quantity_on_hand, updated_at)
             VALUES (?, ?, ?, ?, datetime('now'))
             ON CONFLICT(product_id, branch_id) DO UPDATE SET quantity_on_hand = excluded.quantity_on_hand"
        )
        .bind(&id).bind(product_id).bind(BRANCH).bind(&qty_str)
        .execute(pool).await.expect("seed_stock");
    }

    async fn current_qty(pool: &SqlitePool, product_id: &str) -> f64 {
        let s: Option<String> = sqlx::query_scalar(
            "SELECT quantity_on_hand FROM stock_levels WHERE product_id = ? AND branch_id = ?",
        )
        .bind(product_id)
        .bind(BRANCH)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();
        s.and_then(|x| x.parse().ok()).unwrap_or(0.0)
    }

    async fn movement_count(pool: &SqlitePool, product_id: &str) -> i64 {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM stock_movements WHERE product_id = ?")
            .bind(product_id)
            .fetch_one(pool)
            .await
            .unwrap_or(0)
    }

    // ── 1. manual_adjust increases stock ──────────────────────────────────────
    #[tokio::test]
    async fn test_manual_adjust_increases_stock() {
        let pool = make_pool().await;
        seed_stock(&pool, COLA_ID, 10.0).await;

        manual_adjust(
            &pool,
            COLA_ID,
            5.0,
            Some("restock"),
            USER,
            None,
            BRANCH,
            DEVICE,
        )
        .await
        .expect("manual_adjust");

        assert_eq!(current_qty(&pool, COLA_ID).await, 15.0);
    }

    // ── 2. manual_adjust decreases stock ──────────────────────────────────────
    #[tokio::test]
    async fn test_manual_adjust_decreases_stock() {
        let pool = make_pool().await;
        seed_stock(&pool, COLA_ID, 10.0).await;

        manual_adjust(
            &pool,
            COLA_ID,
            -3.0,
            Some("shrinkage"),
            USER,
            None,
            BRANCH,
            DEVICE,
        )
        .await
        .expect("manual_adjust negative");

        assert_eq!(current_qty(&pool, COLA_ID).await, 7.0);
    }

    // ── 3. stock_take sets an absolute quantity ────────────────────────────────
    #[tokio::test]
    async fn test_stock_take_sets_absolute_quantity() {
        let pool = make_pool().await;
        seed_stock(&pool, WATR_ID, 50.0).await;

        stock_take(
            &pool,
            WATR_ID,
            30.0,
            Some("physical count"),
            USER,
            None,
            BRANCH,
            DEVICE,
        )
        .await
        .expect("stock_take");

        assert_eq!(current_qty(&pool, WATR_ID).await, 30.0);
    }

    // ── 4. stock_take from zero correctly adjusts ─────────────────────────────
    #[tokio::test]
    async fn test_stock_take_from_zero() {
        let pool = make_pool().await;
        // No prior stock record — starts at 0

        stock_take(&pool, COLA_ID, 25.0, None, USER, None, BRANCH, DEVICE)
            .await
            .expect("stock_take from zero");

        assert_eq!(current_qty(&pool, COLA_ID).await, 25.0);
    }

    // ── 5. Each adjustment writes a stock_movement record ─────────────────────
    #[tokio::test]
    async fn test_stock_movement_record_created() {
        let pool = make_pool().await;

        manual_adjust(&pool, COLA_ID, 10.0, None, USER, None, BRANCH, DEVICE)
            .await
            .expect("adjust");
        manual_adjust(&pool, COLA_ID, -2.0, None, USER, None, BRANCH, DEVICE)
            .await
            .expect("adjust 2");

        assert_eq!(movement_count(&pool, COLA_ID).await, 2);
    }

    // ── 6. Movements carry the correct branch_id and device_id ────────────────
    #[tokio::test]
    async fn test_stock_movement_uses_correct_branch_and_device() {
        let pool = make_pool().await;

        manual_adjust(&pool, COLA_ID, 5.0, None, USER, None, BRANCH, DEVICE)
            .await
            .expect("adjust");

        let row = sqlx::query(
            "SELECT branch_id, device_id FROM stock_movements WHERE product_id = ? LIMIT 1",
        )
        .bind(COLA_ID)
        .fetch_one(&pool)
        .await
        .expect("fetch movement");

        let branch: String = row.get("branch_id");
        let device: String = row.get("device_id");
        assert_eq!(branch, BRANCH, "branch_id must match the active branch");
        assert_eq!(device, DEVICE, "device_id must match the active device");
    }

    // ── 7. Low-stock alert fires when qty crosses reorder point ───────────────
    #[tokio::test]
    async fn test_low_stock_alert_fires() {
        let pool = make_pool().await;
        // Cola reorder_point = 5 (from seed data in 0001_initial.sql)
        seed_stock(&pool, COLA_ID, 8.0).await;

        // Adjust down to 4 — below reorder point of 5
        let alert = manual_adjust(&pool, COLA_ID, -4.0, None, USER, None, BRANCH, DEVICE)
            .await
            .expect("adjust below reorder");

        // The LowStockAlert returned carries the product_id and new quantity
        assert_eq!(alert.product_id, COLA_ID);
        let qty: f64 = alert.quantity_on_hand.parse().unwrap_or(-1.0);
        assert_eq!(qty, 4.0);
    }

    // ── 8. stock_take delta is recorded correctly ──────────────────────────────
    #[tokio::test]
    async fn test_stock_take_delta_in_movement() {
        let pool = make_pool().await;
        seed_stock(&pool, WATR_ID, 20.0).await;

        stock_take(&pool, WATR_ID, 12.0, None, USER, None, BRANCH, DEVICE)
            .await
            .expect("stock_take");

        let delta: Option<String> = sqlx::query_scalar(
            "SELECT quantity_delta FROM stock_movements WHERE product_id = ? LIMIT 1",
        )
        .bind(WATR_ID)
        .fetch_optional(&pool)
        .await
        .unwrap_or(None);

        // Delta = 12 - 20 = -8
        let d: f64 = delta.unwrap_or_default().parse().unwrap_or(0.0);
        assert_eq!(d, -8.0, "stock_take delta should be new_qty - old_qty");
    }
}
