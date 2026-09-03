//! Folding one product row into another.
//!
//! Two rows for the same item is an ordinary way for a catalogue to go wrong —
//! an import that ran twice, a barcode typed onto a new product instead of an
//! existing one. Merging them means the survivor has to inherit everything the
//! other one was carrying: its stock, its movements, optionally its sale history,
//! and its barcodes. Miss one and the tidy-up quietly costs the shop something.
//!
//! All of it is one transaction. These writes are dependent — credit the target's
//! stock, then fail before dropping the source's row, and that stock is counted
//! twice.

use super::super::repositories::product_dedup_repo::MergeOutcome;
use crate::errors::{AppError, AppResult};
use sqlx::SqlitePool;

/// Merge `source` into `target`: combine stock per branch, re-point stock
/// movements and barcodes, optionally reassign sale history, then archive the
/// source.
///
/// The single source of truth for product merges — the AI `merge_products` tool
/// and the admin command both call it.
pub async fn merge_products(
    pool: &SqlitePool,
    source_id: &str,
    target_id: &str,
    transfer_history: bool,
    actor_user_id: &str,
) -> AppResult<MergeOutcome> {
    if source_id == target_id {
        return Err(AppError::Validation(
            "source and target product must be different".into(),
        ));
    }

    // One transaction: these five dependent writes each ran straight against the
    // pool. Stopping between any two corrupts the catalogue — credit the target,
    // fail before dropping the source stock row, and that stock is counted twice.
    let mut tx = pool.begin().await?;

    // Validate both products exist and the source is not already archived.
    let source_name: Option<String> =
        sqlx::query_scalar("SELECT name FROM products WHERE product_id = ? AND deleted_at IS NULL")
            .bind(source_id)
            .fetch_optional(&mut *tx)
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
            .fetch_optional(&mut *tx)
            .await?
            .flatten();
    let target_name = target_name.ok_or_else(|| {
        AppError::Validation(format!(
            "Target product {target_id} not found or already deleted"
        ))
    })?;

    let now = chrono::Utc::now().to_rfc3339();
    // Which terminal performed the merge, for the movement rows' attribution.
    let device_id = crate::device_identity::current(pool)
        .await
        .unwrap_or_default();

    // ── Merge stock levels — re-point or sum each source row into the target ────
    let source_stock: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT stock_level_id, branch_id, quantity_on_hand FROM stock_levels WHERE product_id = ?",
    )
    .bind(source_id)
    .fetch_all(&mut *tx)
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
        .execute(&mut *tx)
        .await?
        .rows_affected();

        if updated > 0 {
            // Target already had stock in this branch — drop the merged source row.
            sqlx::query("DELETE FROM stock_levels WHERE stock_level_id = ?")
                .bind(sl_id)
                .execute(&mut *tx)
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
            .execute(&mut *tx)
            .await?;
        }

        // The shelf quantity just moved from one product to another. Recorded as
        // a movement pair so the target's jump is explainable and both products
        // still reconcile against their ledgers; without it the survivor's count
        // permanently exceeds what its movements can account for.
        let target_after: f64 = sqlx::query_scalar(
            "SELECT CAST(quantity_on_hand AS REAL) FROM stock_levels \
             WHERE product_id = ? AND branch_id = ?",
        )
        .bind(target_id)
        .bind(branch_id)
        .fetch_optional(&mut *tx)
        .await?
        .unwrap_or(source_qty);

        crate::inventory::movements::record_merge_movements_tx(
            &mut tx,
            source_id,
            target_id,
            branch_id,
            source_qty,
            target_after,
            actor_user_id,
            &device_id,
        )
        .await?;
    }

    // ── The source's movements stay with the source ─────────────────────────────
    //
    // They used to be re-pointed at the target, which rewrote history: a sale
    // that took two off product A's shelf came to claim it had taken them off
    // product B's. It also made the target's quantity unexplainable. The ledger
    // balance is the earliest surviving movement's post-state plus every later
    // delta, and splicing a second product's history into that stream describes
    // two different shelves as if they were one — the sum stops matching either.
    //
    // The transfer of stock is recorded above as its own movement pair instead:
    // the source goes to zero, the target gains what the source had. That is
    // what actually happened, both ledgers still balance, and each movement
    // still names the product it really moved.

    // ── Optionally reassign sale history ────────────────────────────────────────
    if transfer_history {
        sqlx::query("UPDATE sale_items SET product_id = ? WHERE product_id = ?")
            .bind(target_id)
            .bind(source_id)
            .execute(&mut *tx)
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
    .execute(&mut *tx)
    .await?;

    // ── Carry the source's barcodes across ──────────────────────────────────────
    //
    // The point of merging two rows for the same item is that either code rings
    // up the survivor. This step was missing: the codes stayed pointed at the row
    // that was just archived, and `get_product_by_barcode` filters on active
    // products, so they stopped resolving to anything. The shop tidied its
    // catalogue and found out at the till that half its barcodes had gone dead.
    //
    // After the archive, not before: the uniqueness triggers refuse a code that
    // is the live primary barcode of another *active* product, and until the
    // source is archived it is exactly that.
    //
    // `OR IGNORE` on the re-point and the insert: if the target already carries
    // the code there is nothing to move, and the source row goes with its
    // product. Nothing is lost, because the target already answers to it.
    sqlx::query(
        "UPDATE OR IGNORE product_barcodes SET product_id = ?, updated_at = ?, \
                 sync_status = 'pending' WHERE product_id = ? AND deleted_at IS NULL",
    )
    .bind(target_id)
    .bind(&now)
    .bind(source_id)
    .execute(&mut *tx)
    .await?;

    // The source's own `products.barcode` is a claim too, and it has nowhere to
    // go once the product is archived. It becomes an alias on the target.
    let source_primary: Option<String> = sqlx::query_scalar(
        "SELECT barcode FROM products WHERE product_id = ? AND barcode IS NOT NULL \
           AND TRIM(barcode) <> ''",
    )
    .bind(source_id)
    .fetch_optional(&mut *tx)
    .await?
    .flatten();

    if let Some(code) = source_primary {
        sqlx::query(
            "INSERT OR IGNORE INTO product_barcodes \
               (barcode_id, product_id, barcode, created_at, updated_at) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(ulid::Ulid::new().to_string())
        .bind(target_id)
        .bind(&code)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    Ok(MergeOutcome {
        source_name,
        target_name,
    })
}
