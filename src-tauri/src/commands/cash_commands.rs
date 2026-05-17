/// Cash event commands — Paid-In / Paid-Out manual cash drawer adjustments
/// and the full cash drawer reconciliation summary.
use tauri::State;
use ulid::Ulid;
use serde::Serialize;
use sqlx::Row;
use crate::db::repositories::audit_hash;
use crate::errors::{AppError, AppResult};
use crate::AppState;
use crate::commands::rbac;

// ─── Response types ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct CashEventRow {
    pub cash_event_id:       String,
    pub shift_id:            String,
    pub event_type:          String,  // "paid_in" | "paid_out"
    pub amount_minor:        i64,
    pub note:                Option<String>,
    pub created_by_user_id:  String,
    pub created_at:          String,
}

#[derive(Debug, Serialize)]
pub struct CashDrawerSummary {
    pub opening_minor:     i64,
    pub cash_sales_minor:  i64,
    pub cash_refunds_minor: i64,
    pub paid_in_minor:     i64,
    pub paid_out_minor:    i64,
    pub safe_drop_minor:   i64,
    pub expected_minor:    i64,
    pub counted_minor:     Option<i64>,
    pub variance_minor:    Option<i64>,
    pub events:            Vec<CashEventRow>,
}

#[derive(Debug, Serialize)]
pub struct NoSaleRow {
    pub no_sale_id:     String,
    pub shift_id:       String,
    pub actor_user_id:  String,
    pub note:           Option<String>,
    pub created_at:     String,
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

async fn resolve_branch_device(state: &AppState) -> AppResult<(String, String)> {
    let branch_row = sqlx::query(
        "SELECT branch_id FROM branches WHERE is_active = 1 ORDER BY created_at LIMIT 1"
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("No active branch configured".into()))?;
    let branch_id: String = branch_row.get("branch_id");

    let device_row = sqlx::query(
        "SELECT device_id FROM devices WHERE is_active = 1 ORDER BY device_code LIMIT 1"
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("No active device configured".into()))?;
    let device_id: String = device_row.get("device_id");

    Ok((branch_id, device_id))
}

fn row_to_event(r: &sqlx::sqlite::SqliteRow) -> CashEventRow {
    CashEventRow {
        cash_event_id:      r.get("cash_event_id"),
        shift_id:           r.get("shift_id"),
        event_type:         r.get("event_type"),
        amount_minor:       r.get("amount_minor"),
        note:               r.get("note"),
        created_by_user_id: r.get("created_by_user_id"),
        created_at:         r.get("created_at"),
    }
}

// ─── Commands ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn cash_event_create(
    shift_id:            String,
    event_type:          String,
    amount_minor:        i64,
    note:                Option<String>,
    created_by_user_id:  String,
    state: State<'_, AppState>,
) -> AppResult<CashEventRow> {
    // Validate event_type
    if event_type != "paid_in" && event_type != "paid_out" && event_type != "safe_drop" {
        return Err(AppError::Validation(
            "event_type must be 'paid_in', 'paid_out', or 'safe_drop'".into()
        ));
    }
    if amount_minor <= 0 {
        return Err(AppError::Validation("amount_minor must be positive".into()));
    }

    let (branch_id, device_id) = resolve_branch_device(&state).await?;
    let cash_event_id = Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO cash_events
           (cash_event_id, shift_id, branch_id, device_id, event_type,
            amount_minor, note, created_by_user_id, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&cash_event_id)
    .bind(&shift_id)
    .bind(&branch_id)
    .bind(&device_id)
    .bind(&event_type)
    .bind(amount_minor)
    .bind(note.as_deref())
    .bind(&created_by_user_id)
    .bind(&now)
    .execute(&state.db)
    .await?;

    let row = sqlx::query(
        "SELECT cash_event_id, shift_id, event_type, amount_minor, note,
                created_by_user_id, created_at
         FROM cash_events WHERE cash_event_id = ?"
    )
    .bind(&cash_event_id)
    .fetch_one(&state.db)
    .await?;

    Ok(row_to_event(&row))
}

#[tauri::command]
pub async fn cash_events_list(
    shift_id: String,
    state: State<'_, AppState>,
) -> AppResult<Vec<CashEventRow>> {
    let rows = sqlx::query(
        "SELECT cash_event_id, shift_id, event_type, amount_minor, note,
                created_by_user_id, created_at
         FROM cash_events WHERE shift_id = ? ORDER BY created_at"
    )
    .bind(&shift_id)
    .fetch_all(&state.db)
    .await?;

    Ok(rows.iter().map(row_to_event).collect())
}

/// Inner function — callable from both `cash_drawer_summary` and `cash_x_report`.
async fn drawer_summary_inner(pool: &sqlx::SqlitePool, shift_id: &str) -> AppResult<CashDrawerSummary> {
    let shift_row = sqlx::query(
        "SELECT opening_cash_minor, counted_cash_minor FROM shifts WHERE shift_id = ?"
    )
    .bind(shift_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Shift {} not found", shift_id)))?;

    let opening_minor: i64         = shift_row.get("opening_cash_minor");
    let counted_minor: Option<i64> = shift_row.get("counted_cash_minor");

    let cash_sales_minor: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(p.amount_minor), 0)
         FROM payments p
         JOIN sales s ON s.sale_id = p.sale_id
         WHERE s.shift_id = ? AND p.payment_method = 'cash' AND s.status != 'voided'"
    )
    .bind(shift_id).fetch_one(pool).await?;

    let cash_refunds_minor: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(r.refund_total_minor), 0)
         FROM refunds r
         JOIN sales s ON s.sale_id = r.original_sale_id
         WHERE s.shift_id = ?"
    )
    .bind(shift_id).fetch_one(pool).await?;

    let paid_in_minor: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(amount_minor), 0)
         FROM cash_events WHERE shift_id = ? AND event_type = 'paid_in'"
    )
    .bind(shift_id).fetch_one(pool).await?;

    let paid_out_minor: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(amount_minor), 0)
         FROM cash_events WHERE shift_id = ? AND event_type = 'paid_out'"
    )
    .bind(shift_id).fetch_one(pool).await?;

    let safe_drop_minor: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(amount_minor), 0)
         FROM cash_events WHERE shift_id = ? AND event_type = 'safe_drop'"
    )
    .bind(shift_id).fetch_one(pool).await?;

    let expected_minor = opening_minor + cash_sales_minor - cash_refunds_minor
        + paid_in_minor - paid_out_minor - safe_drop_minor;
    let variance_minor = counted_minor.map(|c| c - expected_minor);

    let event_rows = sqlx::query(
        "SELECT cash_event_id, shift_id, event_type, amount_minor, note,
                created_by_user_id, created_at
         FROM cash_events WHERE shift_id = ? ORDER BY created_at"
    )
    .bind(shift_id)
    .fetch_all(pool)
    .await?;

    Ok(CashDrawerSummary {
        opening_minor,
        cash_sales_minor,
        cash_refunds_minor,
        paid_in_minor,
        paid_out_minor,
        safe_drop_minor,
        expected_minor,
        counted_minor,
        variance_minor,
        events: event_rows.iter().map(row_to_event).collect(),
    })
}

#[tauri::command]
pub async fn cash_drawer_summary(
    shift_id: String,
    state: State<'_, AppState>,
) -> AppResult<CashDrawerSummary> {
    drawer_summary_inner(&state.db, &shift_id).await
}

/// Record a no-sale drawer-open event (audit trail only — no monetary effect).
/// Inserts into `no_sale_events` and writes a NO_SALE audit_log entry.
#[tauri::command]
pub async fn cash_no_sale(
    shift_id:      String,
    actor_user_id: String,
    note:          Option<String>,
    state: State<'_, AppState>,
) -> AppResult<NoSaleRow> {
    let (branch_id, device_id) = resolve_branch_device(&state).await?;
    let no_sale_id = Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO no_sale_events
           (no_sale_id, shift_id, branch_id, device_id, actor_user_id, note, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)"
    )
    .bind(&no_sale_id)
    .bind(&shift_id)
    .bind(&branch_id)
    .bind(&device_id)
    .bind(&actor_user_id)
    .bind(note.as_deref())
    .bind(&now)
    .execute(&state.db)
    .await?;

    // Audit trail with hash chain
    let log_id   = Ulid::new().to_string();
    let prev_hash = audit_hash::fetch_last_hash(&state.db, &device_id).await.unwrap_or_default();
    let hash = audit_hash::compute_audit_hash(&audit_hash::AuditHashInput {
        audit_log_id: &log_id, event_type: "NO_SALE", entity_type: "shift",
        entity_id: &shift_id, actor_user_id: &actor_user_id,
        created_at: &now, after_json: None, previous_hash: &prev_hash,
    });
    sqlx::query(
        "INSERT INTO audit_logs
           (audit_log_id, event_type, entity_type, entity_id,
            actor_user_id, actor_type, device_id, created_at, hash, previous_hash)
         VALUES (?, 'NO_SALE', 'shift', ?, ?, 'user', ?, ?, ?, ?)"
    )
    .bind(&log_id)
    .bind(&shift_id)
    .bind(&actor_user_id)
    .bind(&device_id)
    .bind(&now)
    .bind(&hash)
    .bind(if prev_hash.is_empty() { None } else { Some(prev_hash.clone()) })
    .execute(&state.db)
    .await?;

    Ok(NoSaleRow {
        no_sale_id,
        shift_id,
        actor_user_id,
        note,
        created_at: now,
    })
}

/// X-Report: mid-shift drawer snapshot without closing the shift.
/// Logs an audit event and returns the current reconciliation totals.
/// Requires manager or owner role.
#[tauri::command]
pub async fn cash_x_report(
    shift_id:      String,
    actor_user_id: String,
    state: State<'_, AppState>,
) -> AppResult<CashDrawerSummary> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;

    let summary = drawer_summary_inner(&state.db, &shift_id).await?;

    // Audit trail for X-Report generation with hash chain
    let log_id = Ulid::new().to_string();
    let now    = chrono::Utc::now().to_rfc3339();
    let (_, device_id) = resolve_branch_device(&state).await.unwrap_or_default();
    let prev_hash = audit_hash::fetch_last_hash(&state.db, &device_id).await.unwrap_or_default();
    let hash = audit_hash::compute_audit_hash(&audit_hash::AuditHashInput {
        audit_log_id: &log_id, event_type: "X_REPORT", entity_type: "shift",
        entity_id: &shift_id, actor_user_id: &actor_user_id,
        created_at: &now, after_json: None, previous_hash: &prev_hash,
    });
    sqlx::query(
        "INSERT INTO audit_logs
           (audit_log_id, event_type, entity_type, entity_id,
            actor_user_id, actor_type, device_id, created_at, hash, previous_hash)
         VALUES (?, 'X_REPORT', 'shift', ?, ?, 'user', ?, ?, ?, ?)"
    )
    .bind(&log_id)
    .bind(&shift_id)
    .bind(&actor_user_id)
    .bind(&device_id)
    .bind(&now)
    .bind(&hash)
    .bind(if prev_hash.is_empty() { None } else { Some(prev_hash.clone()) })
    .execute(&state.db)
    .await?;

    Ok(summary)
}
