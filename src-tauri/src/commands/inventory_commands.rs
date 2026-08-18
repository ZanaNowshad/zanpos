use crate::commands::{rbac, sync_commands};
use crate::db::repositories::audit_hash;
use crate::errors::{AppError, AppResult};
use crate::inventory::stock_repo::{self, StockLevel, StockLevelPage, StockMovementRow};
use crate::AppState;
use rust_decimal::Decimal;
use sqlx::Row;
use std::str::FromStr;
use tauri::State;
use ulid::Ulid;

/// Resolve the active branch_id from the database at runtime.
/// Replaces the old compile-time constant so multi-branch or post-wizard IDs work.
async fn active_branch_id(state: &AppState) -> AppResult<String> {
    let row = sqlx::query(
        "SELECT branch_id FROM branches WHERE is_active = 1 ORDER BY created_at LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("No active branch configured".into()))?;
    Ok(row.get("branch_id"))
}

/// Resolve the active device_id from the database at runtime.
async fn active_device_id(state: &AppState) -> AppResult<String> {
    crate::device_identity::current(&state.db).await
}

// ── Decimal quantity helper (H9/H10) ─────────────────────────────────────────────

/// Parse a decimal quantity string, rejecting non-numeric input.
/// Uses `rust_decimal::Decimal` for exact base-10 arithmetic — no float
/// contamination.
fn parse_qty(s: &str) -> AppResult<Decimal> {
    Decimal::from_str(s.trim())
        .map_err(|_| AppError::Validation(format!("Invalid quantity: {}", s)))
}

// ── inventory_get_levels ──────────────────────────────────────────────────────

#[tauri::command]
pub async fn inventory_get_levels(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<StockLevel>, AppError> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;
    let branch_id = active_branch_id(&state).await?;
    stock_repo::get_all_levels(&state.db, &branch_id).await
}

// ── inventory_get_levels_paged ────────────────────────────────────────────────

#[tauri::command]
pub async fn inventory_get_levels_paged(
    actor_user_id: String,
    search: Option<String>,
    offset: Option<i64>,
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<StockLevelPage, AppError> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;
    let branch_id = active_branch_id(&state).await?;
    let limit = limit.unwrap_or(100).min(500);
    let offset = offset.unwrap_or(0).max(0);
    stock_repo::get_levels_paged(&state.db, &branch_id, search.as_deref(), offset, limit).await
}

// ── inventory_get_low_stock ───────────────────────────────────────────────────

#[tauri::command]
pub async fn inventory_get_low_stock(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<StockLevel>, AppError> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;
    let branch_id = active_branch_id(&state).await?;
    stock_repo::get_low_stock(&state.db, &branch_id).await
}

// ── inventory_get_movements ───────────────────────────────────────────────────

#[tauri::command]
pub async fn inventory_get_movements(
    actor_user_id: String,
    state: State<'_, AppState>,
    product_id: String,
) -> Result<Vec<StockMovementRow>, AppError> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;
    stock_repo::get_movements(&state.db, &product_id).await
}

// ── inventory_receive_stock ───────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct ReceiveStockInput {
    pub product_id: String,
    pub quantity: String, // decimal string
    pub expiry_date: Option<String>,
    pub notes: Option<String>,
    pub received_by_user_id: String,
}

/// C-2 fix: read-modify-write is wrapped in a transaction to prevent races.
/// H-9/H-10 fix: quantity arithmetic uses exact Decimal, not f64/CAST AS REAL.
/// H-29 fix: audit log entry is written after commit.
/// M-6 fix: reference_type and movement_type are 'receive' (not 'manual_receive').
#[tauri::command]
pub async fn inventory_receive_stock(
    input: ReceiveStockInput,
    state: State<'_, AppState>,
) -> Result<StockLevel, AppError> {
    rbac::manager_or_owner(&state.db, &input.received_by_user_id).await?;

    // Parse and validate quantity
    let qty = parse_qty(&input.quantity)?;
    if qty <= Decimal::ZERO {
        return Err(AppError::Validation("Quantity must be positive".into()));
    }
    let expiry_date = crate::inventory::lots::validate_expiry_date(input.expiry_date.as_deref())?;

    let branch_id = active_branch_id(&state).await?;
    let device_id = active_device_id(&state).await?;
    let now = chrono::Utc::now().to_rfc3339();
    let stock_level_id = format!("SL-{}-{}", input.product_id, branch_id);

    // Build audit snapshots *before* entering the transaction so we capture
    // the pre-mutation state.
    let old_qty_read: Option<String> = sqlx::query_scalar(
        "SELECT quantity_on_hand FROM stock_levels WHERE product_id = ? AND branch_id = ?",
    )
    .bind(&input.product_id)
    .bind(&branch_id)
    .fetch_optional(&state.db)
    .await?;
    let old_qty_str = old_qty_read.as_deref().unwrap_or("0");
    let before_json = serde_json::json!({"quantity_on_hand": old_qty_str}).to_string();

    // ── Transaction: read old qty → compute new → upsert → movement → sync_status ──
    let mut tx = state.db.begin().await?;

    // Re-read inside tx for correctness (the snapshot above is for audit only)
    let old_qty_in_tx: Option<String> = sqlx::query_scalar(
        "SELECT quantity_on_hand FROM stock_levels WHERE product_id = ? AND branch_id = ?",
    )
    .bind(&input.product_id)
    .bind(&branch_id)
    .fetch_optional(&mut *tx)
    .await?;
    let old_qty = old_qty_in_tx.as_deref().unwrap_or("0");
    let old_qty_dec = Decimal::from_str(old_qty).unwrap_or(Decimal::ZERO);

    // Exact Decimal arithmetic — no CAST AS REAL (H-9/H-10)
    let new_qty_dec = old_qty_dec + qty;
    let new_qty_str = new_qty_dec.to_string();
    let delta_str = qty.to_string();

    // Upsert stock_levels with the computed string (include PK so ON CONFLICT fires)
    let movement_id = Ulid::new().to_string();
    sqlx::query(
        "INSERT INTO stock_levels (stock_level_id, product_id, branch_id, quantity_on_hand, created_at, updated_at, last_movement_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(product_id, branch_id) DO UPDATE SET
           sync_status = 'pending',
           quantity_on_hand = excluded.quantity_on_hand,
           updated_at = excluded.updated_at,
           last_movement_at = excluded.last_movement_at",
    )
    .bind(&stock_level_id)
    .bind(&input.product_id)
    .bind(&branch_id)
    .bind(&new_qty_str)
    .bind(&now)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    // Record movement — M-6: reference_type and movement_type are 'receive'
    sqlx::query(
        "INSERT INTO stock_movements
           (movement_id, product_id, branch_id, device_id, origin_device_id, movement_type, quantity_delta,
            quantity_after, reference_type, notes, created_by_user_id, created_at, sync_status,
            expiry_date, lot_quantity_received, lot_quantity_remaining)
         VALUES (?, ?, ?, ?, ?, 'receive', ?, ?, 'receive', ?, ?, ?, 'pending', ?, ?, ?)",
    )
    .bind(&movement_id)
    .bind(&input.product_id)
    .bind(&branch_id)
    .bind(&device_id)
    .bind(&device_id)
    .bind(&delta_str)
    .bind(&new_qty_str)
    .bind(&input.notes)
    .bind(&input.received_by_user_id)
    .bind(&now)
    .bind(expiry_date)
    .bind(&delta_str)
    .bind(&delta_str)
    .execute(&mut *tx)
    .await?;

    // sync_status='pending' is set by column DEFAULT — sync worker picks it up

    tx.commit().await?;
    sync_commands::schedule_immediate_sync(&state);

    // ── Audit log AFTER commit (H-29) ──
    let after_json = serde_json::json!({"quantity_on_hand": new_qty_str}).to_string();
    if let Err(e) = audit_hash::insert_audit_entry(
        &state.db,
        "STOCK_RECEIVED",
        "stock_level",
        &input.product_id,
        &input.received_by_user_id,
        "user",
        &device_id,
        &branch_id,
        Some(&before_json),
        Some(&after_json),
        input.notes.as_deref(),
    )
    .await
    {
        tracing::warn!("Failed to write audit entry: {}", e);
    }

    // Return updated level
    let levels = stock_repo::get_all_levels(&state.db, &branch_id).await?;
    levels
        .into_iter()
        .find(|l| l.product_id == input.product_id)
        .ok_or_else(|| AppError::NotFound(format!("Product {} not found", input.product_id)))
}

// ── inventory_adjust_stock ────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct AdjustStockInput {
    pub product_id: String,
    pub new_quantity: String, // absolute quantity (count correction)
    pub notes: Option<String>,
    pub adjusted_by_user_id: String,
}

/// C-2 fix: transaction guards the read-modify-write.
/// H-9/H-10 fix: Decimal arithmetic, no CAST AS REAL.
/// H-29 fix: audit log entry after commit.
/// M-7 fix: movement_type and reference_type are 'manual_adjust' (not 'adjustment'/'count_correction').
#[tauri::command]
pub async fn inventory_adjust_stock(
    input: AdjustStockInput,
    state: State<'_, AppState>,
) -> Result<StockLevel, AppError> {
    rbac::manager_or_owner(&state.db, &input.adjusted_by_user_id).await?;

    // Parse target quantity
    let new_qty = parse_qty(&input.new_quantity)?;
    if new_qty < Decimal::ZERO {
        return Err(AppError::Validation("Quantity cannot be negative".into()));
    }

    let branch_id = active_branch_id(&state).await?;
    let device_id = active_device_id(&state).await?;
    let now = chrono::Utc::now().to_rfc3339();
    let stock_level_id = format!("SL-{}-{}", input.product_id, branch_id);

    // Pre-tx snapshot for audit
    let old_qty_read: Option<String> = sqlx::query_scalar(
        "SELECT quantity_on_hand FROM stock_levels WHERE product_id = ? AND branch_id = ?",
    )
    .bind(&input.product_id)
    .bind(&branch_id)
    .fetch_optional(&state.db)
    .await?;
    let old_qty_str = old_qty_read.as_deref().unwrap_or("0");
    let before_json = serde_json::json!({"quantity_on_hand": old_qty_str}).to_string();

    // ── Transaction ──
    let mut tx = state.db.begin().await?;

    let old_qty_in_tx: Option<String> = sqlx::query_scalar(
        "SELECT quantity_on_hand FROM stock_levels WHERE product_id = ? AND branch_id = ?",
    )
    .bind(&input.product_id)
    .bind(&branch_id)
    .fetch_optional(&mut *tx)
    .await?;
    let old_qty = old_qty_in_tx.as_deref().unwrap_or("0");
    let old_qty_dec = Decimal::from_str(old_qty).unwrap_or(Decimal::ZERO);

    // Compute delta using Decimal arithmetic
    let delta_dec = new_qty - old_qty_dec;
    let new_qty_str = new_qty.to_string();
    let delta_str = delta_dec.to_string();

    // Upsert stock_levels (include PK)
    let movement_id = Ulid::new().to_string();
    sqlx::query(
        "INSERT INTO stock_levels (stock_level_id, product_id, branch_id, quantity_on_hand, created_at, updated_at, last_movement_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT(product_id, branch_id) DO UPDATE SET
           sync_status = 'pending',
           quantity_on_hand = excluded.quantity_on_hand,
           updated_at = excluded.updated_at,
           last_movement_at = excluded.last_movement_at",
    )
    .bind(&stock_level_id)
    .bind(&input.product_id)
    .bind(&branch_id)
    .bind(&new_qty_str)
    .bind(&now)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    // M-7: movement_type and reference_type are 'manual_adjust'
    sqlx::query(
        "INSERT INTO stock_movements
           (movement_id, product_id, branch_id, device_id, origin_device_id, movement_type, quantity_delta,
            quantity_after, reference_type, notes, created_by_user_id, created_at, sync_status)
         VALUES (?, ?, ?, ?, ?, 'manual_adjust', ?, ?, 'manual_adjust', ?, ?, ?, 'pending')",
    )
    .bind(&movement_id)
    .bind(&input.product_id)
    .bind(&branch_id)
    .bind(&device_id)
    .bind(&device_id)
    .bind(&delta_str)
    .bind(&new_qty_str)
    .bind(&input.notes)
    .bind(&input.adjusted_by_user_id)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    // sync_status='pending' is set by column DEFAULT — sync worker picks it up

    tx.commit().await?;
    sync_commands::schedule_immediate_sync(&state);

    // ── Audit log AFTER commit (H-29) ──
    let after_json = serde_json::json!({"quantity_on_hand": new_qty_str}).to_string();
    if let Err(e) = audit_hash::insert_audit_entry(
        &state.db,
        "STOCK_ADJUSTED",
        "stock_level",
        &input.product_id,
        &input.adjusted_by_user_id,
        "user",
        &device_id,
        &branch_id,
        Some(&before_json),
        Some(&after_json),
        input.notes.as_deref(),
    )
    .await
    {
        tracing::warn!("Failed to write audit entry: {}", e);
    }

    let levels = stock_repo::get_all_levels(&state.db, &branch_id).await?;
    levels
        .into_iter()
        .find(|l| l.product_id == input.product_id)
        .ok_or_else(|| AppError::NotFound(format!("Product {} not found", input.product_id)))
}

// ── inventory_bulk_stock_take ─────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct BulkStockTakeEntry {
    pub product_id: String,
    pub new_quantity: String, // H-9/H-10: String, not f64
    pub notes: Option<String>,
}

#[derive(serde::Serialize)]
pub struct BulkStockTakeResult {
    pub updated: usize,
    pub errors: Vec<String>,
}

/// Each entry runs in its own transaction so a failure on one product does not
/// roll back others.  H-9/H-10: Decimal arithmetic.  H-29: audit logs.
/// M-8: movement_type and reference_type are 'stock_take' (not 'adjustment'/'stock_take').
#[tauri::command]
pub async fn inventory_bulk_stock_take(
    entries: Vec<BulkStockTakeEntry>,
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<BulkStockTakeResult, AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;

    if entries.is_empty() {
        return Ok(BulkStockTakeResult {
            updated: 0,
            errors: vec![],
        });
    }

    let branch_id = active_branch_id(&state).await?;
    let device_id = active_device_id(&state).await?;
    let now = chrono::Utc::now().to_rfc3339();

    let mut updated = 0usize;
    let mut errors = Vec::<String>::new();

    for entry in &entries {
        // Parse and validate with Decimal
        let new_qty = match parse_qty(&entry.new_quantity) {
            Ok(q) => q,
            Err(e) => {
                errors.push(format!("Product {}: {e}", entry.product_id));
                continue;
            }
        };
        if new_qty < Decimal::ZERO {
            errors.push(format!(
                "Product {}: quantity cannot be negative",
                entry.product_id
            ));
            continue;
        }

        let new_qty_str = new_qty.to_string();
        let stock_level_id = format!("SL-{}-{}", entry.product_id, branch_id);

        // Pre-tx snapshot for audit
        let old_qty_read: Option<String> = sqlx::query_scalar(
            "SELECT quantity_on_hand FROM stock_levels WHERE product_id = ? AND branch_id = ?",
        )
        .bind(&entry.product_id)
        .bind(&branch_id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or(None);
        let old_qty_str = old_qty_read.as_deref().unwrap_or("0");
        let before_json = serde_json::json!({"quantity_on_hand": old_qty_str}).to_string();

        // ── Transaction per entry ──
        let mut tx = match state.db.begin().await {
            Ok(tx) => tx,
            Err(e) => {
                errors.push(format!("Product {}: {e}", entry.product_id));
                continue;
            }
        };

        // Re-read inside tx
        let old_qty_in_tx: Option<String> = sqlx::query_scalar(
            "SELECT quantity_on_hand FROM stock_levels WHERE product_id = ? AND branch_id = ?",
        )
        .bind(&entry.product_id)
        .bind(&branch_id)
        .fetch_optional(&mut *tx)
        .await
        .unwrap_or(None);
        let old_qty_dec = old_qty_in_tx.as_deref().unwrap_or("0");
        let old_qty_dec = Decimal::from_str(old_qty_dec).unwrap_or(Decimal::ZERO);

        let delta_dec = new_qty - old_qty_dec;
        let delta_str = delta_dec.to_string();

        // Upsert stock level (include PK)
        let movement_id = Ulid::new().to_string();
        if let Err(e) = sqlx::query(
            "INSERT INTO stock_levels (stock_level_id, product_id, branch_id, quantity_on_hand, created_at, updated_at, last_movement_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(product_id, branch_id) DO UPDATE SET
               sync_status = 'pending',
               quantity_on_hand = excluded.quantity_on_hand,
               updated_at = excluded.updated_at,
               last_movement_at = excluded.last_movement_at",
        )
        .bind(&stock_level_id)
        .bind(&entry.product_id)
        .bind(&branch_id)
        .bind(&new_qty_str)
        .bind(&now)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await
        {
            errors.push(format!("Product {}: {e}", entry.product_id));
            let _ = tx.rollback().await;
            continue;
        }

        // M-8: movement_type and reference_type are 'stock_take'
        if let Err(e) = sqlx::query(
            "INSERT INTO stock_movements
               (movement_id, product_id, branch_id, device_id, origin_device_id, movement_type, quantity_delta,
                quantity_after, reference_type, notes, created_by_user_id, created_at, sync_status)
             VALUES (?, ?, ?, ?, ?, 'stock_take', ?, ?, 'stock_take', ?, ?, ?, 'pending')",
        )
        .bind(&movement_id)
        .bind(&entry.product_id)
        .bind(&branch_id)
        .bind(&device_id)
        .bind(&device_id)
        .bind(&delta_str)
        .bind(&new_qty_str)
        .bind(&entry.notes)
        .bind(&actor_user_id)
        .bind(&now)
        .execute(&mut *tx)
        .await
        {
            errors.push(format!("Product {}: {e}", entry.product_id));
            let _ = tx.rollback().await;
            continue;
        }

        // sync_status='pending' is set by column DEFAULT — sync worker picks it up

        if let Err(e) = tx.commit().await {
            errors.push(format!("Product {}: {e}", entry.product_id));
            continue;
        }

        // ── Audit log AFTER commit (H-29) ──
        let after_json = serde_json::json!({"quantity_on_hand": new_qty_str}).to_string();
        if let Err(e) = audit_hash::insert_audit_entry(
            &state.db,
            "STOCK_TAKE",
            "stock_level",
            &entry.product_id,
            &actor_user_id,
            "user",
            &device_id,
            &branch_id,
            Some(&before_json),
            Some(&after_json),
            entry.notes.as_deref(),
        )
        .await
        {
            tracing::warn!("Failed to write audit entry: {}", e);
        }

        updated += 1;
    }

    if updated > 0 {
        sync_commands::schedule_immediate_sync(&state);
    }

    Ok(BulkStockTakeResult { updated, errors })
}
