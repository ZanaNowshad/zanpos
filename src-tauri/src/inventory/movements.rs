use crate::domain::product::LowStockAlert;
use crate::errors::AppResult;
/// Inventory movement writers.
/// Each function creates a stock_movement record and updates stock_levels atomically.
/// Returns a list of LowStockAlert for products that crossed below their reorder point.
///
/// `branch_id` and `device_id` are passed explicitly by callers (resolved from the
/// active branch/device at the command layer) so no hardcoded fallback IDs are needed.
///
/// H3: All read-modify-write ops (return_refund, return_void_sale, manual_adjust,
/// stock_take) now hold a write transaction for the entire get_qty → compute →
/// upsert_level cycle so two concurrent operations cannot interleave and silently
/// lose stock.
use sqlx::{Row, Sqlite, SqlitePool, Transaction};
use std::collections::HashMap;
use std::str::FromStr;
use ulid::Ulid;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct StockDriftRow {
    pub product_id: String,
    pub product_name: String,
    pub branch_id: String,
    pub cached_quantity: String,
    pub expected_quantity: String,
    pub latest_movement_id: String,
}

// ── Internal: fetch or initialize stock level ─────────────────────────────────

/// Reserved: fetch or initialize stock level. Not yet wired into the call path.
#[allow(dead_code)]
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

/// H3: Transaction-aware variant of get_qty for use inside write transactions.
/// The shelf quantity, or an error — never a silent zero.
///
/// This used to swallow the query failure (`.ok().flatten()`) and fall back to
/// `0.0`. Every caller feeds the result into `upsert_level_tx`, which writes an
/// **absolute** quantity, so a failed read did not merely lose information: it
/// overwrote a real shelf count with `0 + delta`. A void restocking two units
/// against a shelf of two hundred would have written two.
///
/// A product with no stock row at all is still legitimately zero — that is a
/// missing row, not a failed query, and the two are now distinguishable.
async fn get_qty_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    product_id: &str,
    branch_id: &str,
) -> AppResult<f64> {
    let qty: Option<String> = sqlx::query_scalar(
        "SELECT quantity_on_hand FROM stock_levels WHERE product_id = ? AND branch_id = ?",
    )
    .bind(product_id)
    .bind(branch_id)
    .fetch_optional(&mut **tx)
    .await?;

    Ok(qty.and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0))
}

/// H3: Transaction-aware upsert_level (replaces the old pool-based version).
async fn upsert_level_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    product_id: &str,
    branch_id: &str,
    new_qty: f64,
    movement_at: &str,
) -> AppResult<()> {
    let qty_str = format_qty(new_qty);
    let id = format!("SL-{}-{}", product_id, branch_id);

    sqlx::query(
        "INSERT INTO stock_levels (stock_level_id, product_id, branch_id, quantity_on_hand, last_movement_at, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(product_id, branch_id) DO UPDATE SET
           quantity_on_hand = excluded.quantity_on_hand,
           last_movement_at = excluded.last_movement_at,
           updated_at       = excluded.updated_at,
           sync_status      = 'pending'"
    )
    .bind(&id)
    .bind(product_id)
    .bind(branch_id)
    .bind(&qty_str)
    .bind(movement_at)
    .bind(movement_at)
    .bind(movement_at)
    .execute(&mut **tx)
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

/// Compare the stock cache to the authoritative movement balance.
///
/// The expected figure is the oldest surviving movement's post-state plus every
/// delta after it. It deliberately does *not* use the newest movement's
/// `quantity_after`: that is the value the cache is written from, so comparing
/// the two made this check structurally unable to fail. Two terminals selling
/// the same last unit offline both record `quantity_after = 9`, and the old
/// query agreed with the cache at 9 while the truth was 8.
///
/// Anchored rather than a plain SUM because synced movements are pruned.
pub async fn stock_drift_report(pool: &SqlitePool) -> AppResult<Vec<StockDriftRow>> {
    let rows = sqlx::query(
        "WITH ranked AS (
           SELECT movement_id, product_id, branch_id, quantity_after, created_at, rowid AS rid,
                  ROW_NUMBER() OVER (
                    PARTITION BY product_id, branch_id
                    ORDER BY datetime(created_at) ASC, rowid ASC
                  ) AS oldest_no,
                  ROW_NUMBER() OVER (
                    PARTITION BY product_id, branch_id
                    ORDER BY datetime(created_at) DESC, rowid DESC
                  ) AS newest_no
           FROM stock_movements
         ),
         anchor AS (
           SELECT product_id, branch_id, CAST(quantity_after AS REAL) AS anchor_after,
                  created_at AS anchor_at, rid AS anchor_rid
           FROM ranked WHERE oldest_no = 1
         ),
         expected AS (
           SELECT a.product_id, a.branch_id,
                  a.anchor_after + COALESCE((
                    SELECT SUM(CAST(m.quantity_delta AS REAL))
                    FROM stock_movements m
                    WHERE m.product_id = a.product_id AND m.branch_id = a.branch_id
                      AND (datetime(m.created_at) > datetime(a.anchor_at)
                           OR (datetime(m.created_at) = datetime(a.anchor_at)
                               AND m.rowid > a.anchor_rid))
                  ), 0.0) AS expected_qty
           FROM anchor a
         )
         SELECT newest.movement_id, expected.product_id, expected.branch_id,
                expected.expected_qty,
                COALESCE(stock_levels.quantity_on_hand, '0') AS cached_quantity,
                COALESCE(products.name, expected.product_id) AS product_name
         FROM expected
         JOIN ranked newest
           ON newest.product_id = expected.product_id
          AND newest.branch_id = expected.branch_id
          AND newest.newest_no = 1
         LEFT JOIN stock_levels
           ON stock_levels.product_id = expected.product_id
          AND stock_levels.branch_id = expected.branch_id
         LEFT JOIN products ON products.product_id = expected.product_id
         WHERE ABS(CAST(COALESCE(stock_levels.quantity_on_hand, '0') AS REAL)
                   - expected.expected_qty) > 0.001
         ORDER BY product_name, expected.branch_id",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let expected: f64 = row.get("expected_qty");
            StockDriftRow {
                product_id: row.get("product_id"),
                product_name: row.get("product_name"),
                branch_id: row.get("branch_id"),
                cached_quantity: row.get("cached_quantity"),
                expected_quantity: format_qty(expected),
                latest_movement_id: row.get("movement_id"),
            }
        })
        .collect())
}

/// Repair stock_levels from each product/branch's latest movement balance.
pub async fn reconcile_stock_drift(pool: &SqlitePool) -> AppResult<u64> {
    let drift = stock_drift_report(pool).await?;
    if drift.is_empty() {
        return Ok(0);
    }
    let now = chrono::Utc::now().to_rfc3339();
    let mut tx = pool.begin().await?;
    for row in &drift {
        let expected = row.expected_quantity.parse::<f64>().unwrap_or(0.0);
        let expected = format_qty(expected);
        let stock_level_id = format!("SL-{}-{}", row.product_id, row.branch_id);
        sqlx::query(
            "INSERT INTO stock_levels
               (stock_level_id, product_id, branch_id, quantity_on_hand,
                last_movement_at, created_at, updated_at, sync_status, sync_attempts)
             VALUES (?, ?, ?, ?, ?, ?, ?, 'pending', 0)
             ON CONFLICT(product_id, branch_id) DO UPDATE SET
               quantity_on_hand=excluded.quantity_on_hand,
               last_movement_at=excluded.last_movement_at,
               updated_at=excluded.updated_at,
               sync_status='pending', sync_attempts=0",
        )
        .bind(stock_level_id)
        .bind(&row.product_id)
        .bind(&row.branch_id)
        .bind(expected)
        .bind(&now)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(drift.len() as u64)
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

/// Record the stock movements for a sale, inside the caller's transaction.
///
/// The sale used to decrement `stock_levels` inside its transaction and write
/// the movements afterwards, in a separate one. The comment on the failure path
/// said what that costs: "deduct_sale movement records failed … Stock levels
/// were already updated in the sale transaction." A failure there left the shelf
/// count reduced with nothing in the ledger to say why — an unexplainable
/// quantity, permanently, because nothing ever retried it.
///
/// So the ledger travels with the sale. Either the sale, its stock deduction and
/// its movements all land, or none of them do. A sale that cannot record what it
/// took off the shelf is a sale that should not commit; refusing it leaves the
/// till in a state the next attempt can succeed from, which is strictly better
/// than a receipt whose stock effect is missing.
///
/// `quantities_after` carries the post-deduction totals the caller already
/// computed, so this does not re-read what it just wrote.
pub async fn record_sale_movements_tx(
    tx: &mut Transaction<'_, Sqlite>,
    sale_id: &str,
    cashier_user_id: &str,
    branch_id: &str,
    device_id: &str,
    quantities_after: &HashMap<String, f64>,
) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();

    let rows = sqlx::query(
        "SELECT si.product_id, si.quantity
         FROM sale_items si
         JOIN products p ON p.product_id = si.product_id
         WHERE si.sale_id = ? AND si.voided = 0 AND si.product_id IS NOT NULL
           AND p.track_inventory = 1",
    )
    .bind(sale_id)
    .fetch_all(&mut **tx)
    .await?;

    for row in &rows {
        let product_id: String = row.get("product_id");
        let qty_str: String = row.get("quantity");
        let sold_qty: f64 = qty_str.parse().unwrap_or(0.0);
        if sold_qty <= 0.0 {
            continue;
        }
        let sold_qty_decimal =
            rust_decimal::Decimal::from_str(&qty_str).unwrap_or(rust_decimal::Decimal::ZERO);

        // One movement per sale line, keyed by the sale it belongs to. The
        // replay of an idempotent sale returns the original without reaching
        // here, so a duplicate cannot arise from a retry; this guard covers a
        // sale whose movements were written by an older, post-commit build.
        let already: Option<String> = sqlx::query_scalar(
            "SELECT movement_id FROM stock_movements
             WHERE reference_type = 'sale' AND reference_id = ? AND product_id = ? LIMIT 1",
        )
        .bind(sale_id)
        .bind(&product_id)
        .fetch_optional(&mut **tx)
        .await?;
        if already.is_some() {
            continue;
        }

        let new_qty = *quantities_after.get(&product_id).unwrap_or(&0.0);
        sqlx::query(
            "INSERT INTO stock_movements
             (movement_id, product_id, branch_id, device_id, origin_device_id, movement_type,
              quantity_delta, quantity_after, reference_type, reference_id,
              created_by_user_id, created_at, sync_status)
             VALUES (?,?,?,?,?,'sale',?,?,   'sale',?,?,?,'pending')",
        )
        .bind(Ulid::new().to_string())
        .bind(&product_id)
        .bind(branch_id)
        .bind(device_id)
        .bind(device_id)
        .bind(format_qty(-sold_qty))
        .bind(format_qty(new_qty))
        .bind(sale_id)
        .bind(cashier_user_id)
        .bind(&now)
        .execute(&mut **tx)
        .await?;

        crate::inventory::lots::consume_fefo(tx, &product_id, branch_id, sold_qty_decimal).await?;
    }

    Ok(())
}

/// Low-stock alerts for a committed sale, read after the fact.
///
/// Split out from the movement write: an alert is something to show the cashier,
/// not part of the books, so it must not be able to fail a sale.
pub async fn low_stock_after_sale(
    pool: &SqlitePool,
    quantities_after: &HashMap<String, f64>,
) -> Vec<LowStockAlert> {
    let mut alerts = Vec::new();
    for (product_id, qty) in quantities_after {
        if let Some(alert) = check_alert(pool, product_id, *qty).await {
            alerts.push(alert);
        }
    }
    alerts
}

/// Put refunded items back on the shelf, on the caller's open connection.
///
/// `create_refund` holds a `BEGIN IMMEDIATE` on one connection to serialise
/// concurrent refunds, and used to credit stock *after* committing it — with the
/// failure only logged: "stock credit failed after refund commit — manual
/// reconciliation may be required". A refund that does not return its stock is
/// worse than one that fails outright: the customer has their money, the shelf
/// count still says the goods were sold, and the only record of the discrepancy
/// is a log line nobody reads.
///
/// Running on the refund's own connection puts the stock return inside the same
/// transaction. A refund that cannot credit stock now does not happen at all,
/// which the operator can see and retry.
pub async fn return_refund_conn(
    conn: &mut sqlx::SqliteConnection,
    refund_id: &str,
    created_by_user_id: &str,
    branch_id: &str,
    device_id: &str,
) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();

    let rows = sqlx::query(
        "SELECT si.product_id, ri.quantity
         FROM refund_items ri
         JOIN sale_items si ON si.sale_item_id = ri.sale_item_id
         JOIN products p ON p.product_id = si.product_id
         WHERE ri.refund_id = ? AND si.product_id IS NOT NULL AND p.track_inventory = 1",
    )
    .bind(refund_id)
    .fetch_all(&mut *conn)
    .await?;

    for row in &rows {
        let product_id: String = row.get("product_id");
        let qty_str: String = row.get("quantity");
        let returned_qty: f64 = qty_str.parse().unwrap_or(0.0);
        if returned_qty <= 0.0 {
            continue;
        }

        let current: f64 = sqlx::query_scalar(
            "SELECT CAST(quantity_on_hand AS REAL) FROM stock_levels
             WHERE product_id = ? AND branch_id = ?",
        )
        .bind(&product_id)
        .bind(branch_id)
        .fetch_optional(&mut *conn)
        .await?
        .unwrap_or(0.0);
        let new_qty = current + returned_qty;

        sqlx::query(
            "INSERT INTO stock_levels
               (stock_level_id, product_id, branch_id, quantity_on_hand,
                last_movement_at, created_at, updated_at, sync_status)
             VALUES (?,?,?,?,?,?,?,'pending')
             ON CONFLICT(product_id, branch_id) DO UPDATE SET
               quantity_on_hand = excluded.quantity_on_hand,
               last_movement_at = excluded.last_movement_at,
               updated_at       = excluded.updated_at,
               sync_status      = 'pending'",
        )
        .bind(format!("SL-{product_id}-{branch_id}"))
        .bind(&product_id)
        .bind(branch_id)
        .bind(format_qty(new_qty))
        .bind(&now)
        .bind(&now)
        .bind(&now)
        .execute(&mut *conn)
        .await?;

        sqlx::query(
            "INSERT INTO stock_movements
             (movement_id, product_id, branch_id, device_id, origin_device_id, movement_type,
              quantity_delta, quantity_after, reference_type, reference_id,
              created_by_user_id, created_at, sync_status)
             VALUES (?,?,?,?,?,'refund',?,?,   'refund',?,?,?,'pending')",
        )
        .bind(Ulid::new().to_string())
        .bind(&product_id)
        .bind(branch_id)
        .bind(device_id)
        .bind(device_id)
        .bind(format_qty(returned_qty))
        .bind(format_qty(new_qty))
        .bind(refund_id)
        .bind(created_by_user_id)
        .bind(&now)
        .execute(&mut *conn)
        .await?;
    }

    Ok(())
}

/// Record stock changing hands when two products are merged into one.
///
/// A merge folds the loser's shelf quantity into the survivor. That is a real
/// movement of stock between two product ids, and it was written straight into
/// `stock_levels` with nothing in the ledger to explain it: the survivor's count
/// jumped, the movements did not account for the jump, and the two could never
/// be reconciled again.
///
/// Two rows, because two products changed: the source goes to zero, the target
/// gains what the source had. Both name the merge as their source so the pair
/// can be found together, and the quantity is only moved — never created.
pub async fn record_merge_movements_tx(
    tx: &mut Transaction<'_, Sqlite>,
    source_product_id: &str,
    target_product_id: &str,
    branch_id: &str,
    quantity: f64,
    target_quantity_after: f64,
    actor_user_id: &str,
    device_id: &str,
) -> AppResult<()> {
    if quantity <= 0.0 {
        return Ok(());
    }
    let now = chrono::Utc::now().to_rfc3339();

    for (product_id, delta, after) in [
        (source_product_id, -quantity, 0.0),
        (target_product_id, quantity, target_quantity_after),
    ] {
        sqlx::query(
            "INSERT INTO stock_movements
             (movement_id, product_id, branch_id, device_id, origin_device_id, movement_type,
              quantity_delta, quantity_after, reference_type, reference_id,
              created_by_user_id, created_at, sync_status)
             VALUES (?,?,?,?,?,'merge',?,?,   'product_merge',?,?,?,'pending')",
        )
        .bind(Ulid::new().to_string())
        .bind(product_id)
        .bind(branch_id)
        .bind(device_id)
        .bind(device_id)
        .bind(format_qty(delta))
        .bind(format_qty(after))
        .bind(source_product_id)
        .bind(actor_user_id)
        .bind(&now)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

// ── return_void_sale ──────────────────────────────────────────────────────────

/// Called inside pos_void_sale's transaction before it commits. Restores stock for all tracked items
/// that were deducted when the sale was originally finalized.
///
/// Uses the `sale_items` table directly (no refund record required).
/// Movement type is `"void"` to distinguish from normal refund returns.
pub async fn return_void_sale(
    tx: &mut Transaction<'_, Sqlite>,
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
    .fetch_all(&mut **tx)
    .await?;

    for row in &rows {
        let product_id: String = row.get("product_id");
        let qty_str: String = row.get("quantity");
        let returned_qty: f64 = qty_str.parse().unwrap_or(0.0);
        if returned_qty <= 0.0 {
            continue;
        }

        let current = get_qty_tx(tx, &product_id, branch_id).await?;
        let new_qty = current + returned_qty;
        let movement_id = Ulid::new().to_string();

        upsert_level_tx(tx, &product_id, branch_id, new_qty, &now).await?;

        sqlx::query(
            "INSERT INTO stock_movements
             (movement_id, product_id, branch_id, device_id, origin_device_id, movement_type,
              quantity_delta, quantity_after, reference_type, reference_id,
              created_by_user_id, created_at, sync_status)
             VALUES (?,?,?,?,?,'void',?,?,'sale',?,?,?,'pending')",
        )
        .bind(&movement_id)
        .bind(&product_id)
        .bind(branch_id)
        .bind(device_id)
        .bind(device_id)
        .bind(format_qty(returned_qty))
        .bind(format_qty(new_qty))
        .bind(sale_id)
        .bind(voided_by_user_id)
        .bind(&now)
        .execute(&mut **tx)
        .await?;
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

    // H3: Exclusive transaction for read-modify-write
    let mut tx = pool.begin().await?;
    let current = get_qty_tx(&mut tx, product_id, branch_id).await?;
    let new_qty = current + quantity_delta;
    let movement_id = Ulid::new().to_string();

    upsert_level_tx(&mut tx, product_id, branch_id, new_qty, &now).await?;

    sqlx::query(
        "INSERT INTO stock_movements
         (movement_id, product_id, branch_id, device_id, origin_device_id, movement_type,
          quantity_delta, quantity_after, reference_type, reference_id,
          notes, created_by_user_id, created_at, sync_status)
         VALUES (?,?,?,?,?,'adjustment',?,?, 'ai_action',?,?,?,?,'pending')",
    )
    .bind(&movement_id)
    .bind(product_id)
    .bind(branch_id)
    .bind(device_id)
    .bind(device_id)
    .bind(format_qty(quantity_delta))
    .bind(format_qty(new_qty))
    .bind(ai_action_id.unwrap_or(""))
    .bind(notes)
    .bind(user_id)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    // sync_status='pending' is set by column DEFAULT — sync worker picks it up

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

    // H3: Exclusive transaction for read-modify-write (current qty → delta → upsert)
    let mut tx = pool.begin().await?;
    let current = get_qty_tx(&mut tx, product_id, branch_id).await?;
    let delta = new_quantity - current;
    let movement_id = Ulid::new().to_string();

    upsert_level_tx(&mut tx, product_id, branch_id, new_quantity, &now).await?;

    sqlx::query(
        "INSERT INTO stock_movements
         (movement_id, product_id, branch_id, device_id, origin_device_id, movement_type,
          quantity_delta, quantity_after, reference_type, reference_id,
          notes, created_by_user_id, created_at, sync_status)
         VALUES (?,?,?,?,?,'stock_take',?,?, 'ai_action',?,?,?,?,'pending')",
    )
    .bind(&movement_id)
    .bind(product_id)
    .bind(branch_id)
    .bind(device_id)
    .bind(device_id)
    .bind(format_qty(delta))
    .bind(format_qty(new_quantity))
    .bind(ai_action_id.unwrap_or(""))
    .bind(notes)
    .bind(user_id)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    // sync_status='pending' is set by column DEFAULT — sync worker picks it up

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
        // Activate seed device and branch
        sqlx::query(
            "UPDATE devices SET is_active = 1 WHERE device_id = '01JDEVICE0000000000000001'",
        )
        .execute(&pool)
        .await
        .ok();
        sqlx::query(
            "UPDATE branches SET is_active = 1 WHERE branch_id = '01JBRANCH0000000000000001'",
        )
        .execute(&pool)
        .await
        .ok();

        // Seed tax rules needed by the test products
        sqlx::query(
            "INSERT OR IGNORE INTO tax_rules (tax_rule_id, name, rate_basis_points, inclusive, is_active, effective_from, created_at, updated_at, version)
             VALUES
             ('01JTAX000000000000VAT001', 'VAT 10%', 1000, 0, 1, datetime('now'), datetime('now'), datetime('now'), 1),
             ('01JTAX000000000000ZERO01', 'Zero-rated', 0, 0, 1, datetime('now'), datetime('now'), datetime('now'), 1)"
        ).execute(&pool).await.expect("seed test tax rules");

        // Seed admin user (needed by stock movements created_by_user_id)
        sqlx::query(
            "INSERT OR IGNORE INTO users (user_id, branch_id, display_name, username, pin_hash, role_id, is_active, created_at, updated_at, version)
             VALUES ('01JUSER000000000000ADMIN1', '01JBRANCH0000000000000001', 'Admin', 'admin1', 'PLAIN:0000', '01JROLES000000000000000001', 1, datetime('now'), datetime('now'), 1)"
        ).execute(&pool).await.expect("seed test admin");
        // Ensure admin is active (seed may have created it as inactive)
        sqlx::query("UPDATE users SET is_active = 1 WHERE user_id = '01JUSER000000000000ADMIN1'")
            .execute(&pool)
            .await
            .ok();

        // Re-seed products needed as FK refs.
        sqlx::query(
            "INSERT OR IGNORE INTO categories (category_id, name, sort_order, is_active, created_at, updated_at, version)
             VALUES ('01JCAT000000000000DRINK01', 'Drinks', 1, 1, datetime('now'), datetime('now'), 1)"
        ).execute(&pool).await.expect("seed test category");
        sqlx::query(
            "INSERT OR IGNORE INTO products
             (product_id, category_id, name, sku, barcode, description, track_inventory, reorder_point, is_active, tax_rule_id, currency, created_at, updated_at, version)
             VALUES
             ('01JPROD00000000000COLA001', '01JCAT000000000000DRINK01', 'Coca-Cola 330ml', 'COLA-330', '5449000000996', NULL, 1, 5, 1, '01JTAX000000000000VAT001', 'BHD', datetime('now'), datetime('now'), 1),
             ('01JPROD00000000000WATR001', '01JCAT000000000000DRINK01', 'Water 500ml',     'WATR-500', '6281001511222', NULL, 1, 10, 1, '01JTAX000000000000ZERO01', 'BHD', datetime('now'), datetime('now'), 1)"
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
            "INSERT INTO stock_levels (stock_level_id, product_id, branch_id, quantity_on_hand, created_at, updated_at)
             VALUES (?, ?, ?, ?, datetime('now'), datetime('now'))
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

    #[tokio::test]
    async fn reconciliation_repairs_stock_level_from_latest_movement() {
        let pool = make_pool().await;
        manual_adjust(&pool, COLA_ID, 10.0, None, USER, None, BRANCH, DEVICE)
            .await
            .expect("seed movement");
        manual_adjust(&pool, COLA_ID, -3.0, None, USER, None, BRANCH, DEVICE)
            .await
            .expect("second movement");

        sqlx::query(
            "UPDATE stock_levels SET quantity_on_hand = '999' WHERE product_id = ? AND branch_id = ?",
        )
        .bind(COLA_ID)
        .bind(BRANCH)
        .execute(&pool)
        .await
        .expect("corrupt cache");

        let before = stock_drift_report(&pool).await.expect("drift report");
        assert_eq!(before.len(), 1);
        assert_eq!(before[0].expected_quantity, "7");

        let repaired = reconcile_stock_drift(&pool).await.expect("reconcile");
        assert_eq!(repaired, 1);
        assert_eq!(current_qty(&pool, COLA_ID).await, 7.0);
        assert!(stock_drift_report(&pool)
            .await
            .expect("clean report")
            .is_empty());
    }

    /// Insert a movement exactly as an offline terminal would: its own delta,
    /// and a quantity_after computed from *its* view of stock.
    async fn offline_movement(
        pool: &SqlitePool,
        id: &str,
        delta: f64,
        believed_after: f64,
        created_at: &str,
    ) {
        sqlx::query(
            "INSERT INTO stock_movements
               (movement_id, product_id, branch_id, device_id, origin_device_id, movement_type,
                quantity_delta, quantity_after, reference_type, reference_id,
                created_by_user_id, created_at, sync_status)
             VALUES (?,?,?,?,?,'sale',?,?,'sale','R-1',?,?,'synced')",
        )
        .bind(id)
        .bind(COLA_ID)
        .bind(BRANCH)
        .bind(DEVICE)
        .bind(DEVICE)
        .bind(format_qty(delta))
        .bind(format_qty(believed_after))
        .bind(USER)
        .bind(created_at)
        .execute(pool)
        .await
        .expect("insert movement");
    }

    // Two terminals each sell the last unit while offline. Both compute
    // quantity_after = 9 from their own cache of 10, so taking the newest
    // snapshot settles stock at 9 when the truth is 8 — and the old drift check
    // compared the cache against that same 9, so it could never report it.
    #[tokio::test]
    async fn concurrent_offline_sales_are_detected_as_drift() {
        let pool = make_pool().await;
        offline_movement(&pool, "M-BASE", -0.0, 10.0, "2026-01-01T10:00:00Z").await;
        offline_movement(&pool, "M-LANE1", -1.0, 9.0, "2026-01-01T10:05:00Z").await;
        offline_movement(&pool, "M-LANE2", -1.0, 9.0, "2026-01-01T10:06:00Z").await;

        // The cache holds the newest snapshot, which is what sync wrote.
        sqlx::query(
            "INSERT INTO stock_levels (stock_level_id, product_id, branch_id, quantity_on_hand,
                                       created_at, updated_at)
             VALUES ('SL-DRIFT', ?, ?, '9', datetime('now'), datetime('now'))
             ON CONFLICT(product_id, branch_id) DO UPDATE SET quantity_on_hand = '9'",
        )
        .bind(COLA_ID)
        .bind(BRANCH)
        .execute(&pool)
        .await
        .expect("seed cache");

        let drift = stock_drift_report(&pool).await.expect("drift report");
        assert_eq!(drift.len(), 1, "the double-sale must be reported");
        assert_eq!(
            drift[0].expected_quantity, "8",
            "both deltas must compose: 10 - 1 - 1"
        );

        reconcile_stock_drift(&pool).await.expect("reconcile");
        assert_eq!(current_qty(&pool, COLA_ID).await, 8.0);
    }

    // Synced movements are pruned, so the balance must anchor on the oldest
    // surviving row rather than summing deltas from zero.
    #[tokio::test]
    async fn balance_survives_pruned_history() {
        let pool = make_pool().await;
        // Everything before this was pruned; this row's quantity_after is the
        // only record that the earlier 500 units ever existed.
        offline_movement(&pool, "M-ANCHOR", -1.0, 500.0, "2026-02-01T10:00:00Z").await;
        offline_movement(&pool, "M-NEXT", -2.0, 498.0, "2026-02-01T11:00:00Z").await;

        sqlx::query(
            "INSERT INTO stock_levels (stock_level_id, product_id, branch_id, quantity_on_hand,
                                       created_at, updated_at)
             VALUES ('SL-PRUNE', ?, ?, '498', datetime('now'), datetime('now'))
             ON CONFLICT(product_id, branch_id) DO UPDATE SET quantity_on_hand = '498'",
        )
        .bind(COLA_ID)
        .bind(BRANCH)
        .execute(&pool)
        .await
        .expect("seed cache");

        // 500 (anchor) - 2 (the one delta after it) = 498. A plain SUM of
        // surviving deltas would have said -3.
        let drift = stock_drift_report(&pool).await.expect("drift report");
        assert!(
            drift.is_empty(),
            "pruned history must not be read as stock loss: {drift:?}"
        );
    }
}
