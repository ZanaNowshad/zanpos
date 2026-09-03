use crate::commands::rbac;
use crate::db::repositories::audit_hash;

use crate::errors::{AppError, AppResult};
use crate::AppState;
use serde::Serialize;
use sqlx::Row;
/// Cash event commands — Paid-In / Paid-Out manual cash drawer adjustments
/// and the full cash drawer reconciliation summary.
use tauri::State;
use ulid::Ulid;

// ─── Response types ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct CashEventRow {
    pub cash_event_id: String,
    pub shift_id: String,
    pub event_type: String, // "paid_in" | "paid_out"
    pub amount_minor: i64,
    pub note: Option<String>,
    pub created_by_user_id: String,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct CashDrawerSummary {
    pub opening_minor: i64,
    pub cash_sales_minor: i64,
    pub pending_delivery_cash_minor: i64,
    pub cash_refunds_minor: i64,
    pub paid_in_minor: i64,
    pub paid_out_minor: i64,
    pub safe_drop_minor: i64,
    pub expected_minor: i64,
    pub counted_minor: Option<i64>,
    pub variance_minor: Option<i64>,
    pub events: Vec<CashEventRow>,
}

#[derive(Debug, Serialize)]
pub struct NoSaleRow {
    pub no_sale_id: String,
    pub shift_id: String,
    pub actor_user_id: String,
    pub note: Option<String>,
    pub created_at: String,
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

async fn resolve_branch_device(state: &AppState) -> AppResult<(String, String)> {
    crate::db::helpers::active_branch_and_device(&state.db).await
}

fn row_to_event(r: &sqlx::sqlite::SqliteRow) -> CashEventRow {
    CashEventRow {
        cash_event_id: r.get("cash_event_id"),
        shift_id: r.get("shift_id"),
        event_type: r.get("event_type"),
        amount_minor: r.get("amount_minor"),
        note: r.get("note"),
        created_by_user_id: r.get("created_by_user_id"),
        created_at: r.get("created_at"),
    }
}

// ─── Commands ─────────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn cash_event_create(
    shift_id: String,
    event_type: String,
    amount_minor: i64,
    note: Option<String>,
    created_by_user_id: String,
    state: State<'_, AppState>,
) -> AppResult<CashEventRow> {
    // Validate event_type
    if event_type != "paid_in" && event_type != "paid_out" && event_type != "safe_drop" {
        return Err(AppError::Validation(
            "event_type must be 'paid_in', 'paid_out', or 'safe_drop'".into(),
        ));
    }
    if amount_minor <= 0 {
        return Err(AppError::Validation("amount_minor must be positive".into()));
    }
    // Paid-out and safe-drop events require a note/reason for audit purposes.
    // This is enforced server-side so that direct IPC calls cannot bypass the requirement.
    if (event_type == "paid_out" || event_type == "safe_drop")
        && note.as_deref().map(str::trim).unwrap_or("").is_empty()
    {
        return Err(AppError::Validation(
            "A reason is required for Paid Out and Safe Drop events".into(),
        ));
    }

    // Cash drawer adjustments (paid_in/paid_out/safe_drop) are manager-level operations.
    // The created_by_user_id is verified against the DB role.
    rbac::manager_or_owner(&state.db, &created_by_user_id).await?;

    let (branch_id, device_id) = resolve_branch_device(&state).await?;
    let cash_event_id = Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let mut tx = state.db.begin().await?;

    // BUG-6: Reject cash events on a closed shift.
    // A cash event on a closed shift would corrupt the closing balance that was
    // already computed and stored in expected_cash_minor at close time.
    // Status check is INSIDE the tx so no other connection can commit a close
    // between the check and the INSERT (single-connection WAL write lock).
    let shift_status: Option<String> =
        sqlx::query_scalar("SELECT status FROM shifts WHERE shift_id = ?")
            .bind(&shift_id)
            .fetch_optional(&mut *tx)
            .await?;
    match shift_status.as_deref() {
        Some("open") => {}
        Some(_) => {
            return Err(AppError::Conflict(
                "Cannot add cash event to a closed shift".into(),
            ))
        }
        None => return Err(AppError::NotFound(format!("Shift {} not found", shift_id))),
    }

    sqlx::query(
        "INSERT INTO cash_events
           (cash_event_id, shift_id, branch_id, device_id, origin_device_id, event_type,
            amount_minor, note, created_by_user_id, created_at, updated_at,
            sync_status, sync_attempts)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', 0)",
    )
    .bind(&cash_event_id)
    .bind(&shift_id)
    .bind(&branch_id)
    .bind(&device_id)
    .bind(&device_id)
    .bind(&event_type)
    .bind(amount_minor)
    .bind(note.as_deref())
    .bind(&created_by_user_id)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    // Audit trail for cash drawer event with hash chain
    let audit_id = Ulid::new().to_string();
    let after_json = serde_json::json!({
        "cash_event_id": &cash_event_id,
        "shift_id": &shift_id,
        "event_type": &event_type,
        "amount_minor": amount_minor,
        "note": note.as_deref(),
    })
    .to_string();
    let prev_hash = audit_hash::fetch_last_hash(&state.db, &device_id)
        .await
        .unwrap_or_default();
    let hash = audit_hash::compute_audit_hash(&audit_hash::AuditHashInput {
        audit_log_id: &audit_id,
        event_type: &format!("CASH_{}", event_type.to_uppercase()),
        entity_type: "shift",
        entity_id: &shift_id,
        actor_user_id: &created_by_user_id,
        actor_type: "user",
        created_at: &now,
        before_json: None,
        after_json: Some(&after_json),
        reason: None,
        previous_hash: &prev_hash,
    });
    sqlx::query(
        "INSERT INTO audit_logs
           (audit_log_id, event_type, entity_type, entity_id,
            actor_user_id, actor_type, device_id, origin_device_id, branch_id, after_json, created_at, hash, previous_hash)
         VALUES (?, ?, 'shift', ?, ?, 'user', ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&audit_id)
    .bind(format!("CASH_{}", event_type.to_uppercase()))
    .bind(&shift_id)
    .bind(&created_by_user_id)
    .bind(&device_id)
    .bind(&device_id)
    .bind(&branch_id)
    .bind(&after_json)
    .bind(&now)
    .bind(&hash)
    .bind(if prev_hash.is_empty() {
        None
    } else {
        Some(prev_hash.clone())
    })
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    let row = sqlx::query(
        "SELECT cash_event_id, shift_id, event_type, amount_minor, note,
                created_by_user_id, created_at
         FROM cash_events WHERE cash_event_id = ?",
    )
    .bind(&cash_event_id)
    .fetch_one(&state.db)
    .await?;

    Ok(row_to_event(&row))
}

#[tauri::command]
pub async fn cash_events_list(
    actor_user_id: String,
    shift_id: String,
    state: State<'_, AppState>,
) -> AppResult<Vec<CashEventRow>> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;
    let rows = sqlx::query(
        "SELECT cash_event_id, shift_id, event_type, amount_minor, note,
                created_by_user_id, created_at
         FROM cash_events WHERE shift_id = ? ORDER BY created_at",
    )
    .bind(&shift_id)
    .fetch_all(&state.db)
    .await?;

    Ok(rows.iter().map(row_to_event).collect())
}

/// Inner function — callable from both `cash_drawer_summary` and `cash_x_report`.
/// Inner drawer summary — testable without AppState, like its EOD sibling.
///
/// The expected-cash formula lives in three places: here, `shift_repo::close_shift`
/// and `report_eod_cashup_inner`. They are kept in step by hand, and a shop only
/// finds out they have drifted when the drawer count disagrees with the report.
pub(crate) async fn drawer_summary_inner(
    pool: &sqlx::SqlitePool,
    shift_id: &str,
) -> AppResult<CashDrawerSummary> {
    let shift_row =
        sqlx::query("SELECT opening_cash_minor, counted_cash_minor FROM shifts WHERE shift_id = ?")
            .bind(shift_id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Shift {} not found", shift_id)))?;

    let opening_minor: i64 = shift_row.get("opening_cash_minor");
    let counted_minor: Option<i64> = shift_row.get("counted_cash_minor");

    // M5: Run the 6 independent aggregation queries concurrently.
    // SQLite WAL allows concurrent reads; tokio::join! avoids serial round-trips.
    let (
        cash_sales_minor,
        pending_delivery_cash_minor,
        cash_refunds_minor,
        paid_in_minor,
        paid_out_minor,
        safe_drop_minor,
    ) = tokio::try_join!(
        async {
            sqlx::query_scalar::<_, i64>(
                "SELECT COALESCE(SUM(p.amount_minor), 0)
                 FROM payments p
                 JOIN sales s ON s.sale_id = p.sale_id
                 WHERE s.shift_id = ? AND p.payment_method = 'cash' AND s.status != 'voided'
                   AND (s.is_delivery = 0 OR EXISTS (
                       SELECT 1 FROM delivery_orders d WHERE d.sale_id = s.sale_id AND d.payment_status = 'paid'
                   ))",
            )
            .bind(shift_id)
            .fetch_one(pool)
            .await
            .map_err(AppError::from)
        },
        async {
            sqlx::query_scalar::<_, i64>(
                "SELECT COALESCE(SUM(p.amount_minor), 0)
                 FROM payments p
                 JOIN sales s ON s.sale_id = p.sale_id
                 WHERE s.shift_id = ? AND p.payment_method = 'cash' AND s.status != 'voided'
                   AND s.is_delivery = 1
                   AND NOT EXISTS (
                       SELECT 1 FROM delivery_orders d
                       WHERE d.sale_id = s.sale_id AND d.payment_status = 'paid'
                   )",
            )
            .bind(shift_id)
            .fetch_one(pool)
            .await
            .map_err(AppError::from)
        },
        async {
            // Only deduct the cash portion of refunds: proportionally scale
            // refund_total by (cash_paid / sale_total) for split-payment sales.
            // The old EXISTS-based approach overcounted (BUG-REPORTS-3 fix).
            //
            // The `exchange_credit` carve-out matters as much as the cash ratio:
            // when a returned item's value goes towards a replacement rather than
            // back across the counter, no notes leave the drawer. Deducting the
            // whole refund made the X-report and drawer summary read short by the
            // credited amount for the rest of the shift, while `close_shift` —
            // which does carve it out — disagreed until the shift ended, at which
            // point the number moved for no reason the manager could see.
            sqlx::query_scalar::<_, i64>(
                "SELECT COALESCE(SUM(
                    CASE WHEN s.net_total_minor <= 0 THEN 0
                    ELSE MIN(
                        (SELECT COALESCE(SUM(p2.amount_minor), 0)
                         FROM payments p2
                         WHERE p2.sale_id = s.sale_id AND p2.payment_method = 'cash'),
                        s.net_total_minor
                    ) * MAX(
                        r.refund_total_minor - COALESCE((
                            SELECT SUM(ep.amount_minor)
                            FROM payments ep
                            WHERE ep.payment_method = 'exchange_credit'
                              AND ep.external_reference = r.refund_id
                        ), 0),
                        0
                    ) / s.net_total_minor
                    END
                ), 0)
                 FROM refunds r
                 JOIN sales s ON s.sale_id = r.original_sale_id
                 WHERE s.shift_id = ?",
            )
            .bind(shift_id)
            .fetch_one(pool)
            .await
            .map_err(AppError::from)
        },
        async {
            sqlx::query_scalar::<_, i64>(
                "SELECT COALESCE(SUM(amount_minor), 0)
                 FROM cash_events WHERE shift_id = ? AND event_type = 'paid_in'",
            )
            .bind(shift_id)
            .fetch_one(pool)
            .await
            .map_err(AppError::from)
        },
        async {
            sqlx::query_scalar::<_, i64>(
                "SELECT COALESCE(SUM(amount_minor), 0)
                 FROM cash_events WHERE shift_id = ? AND event_type = 'paid_out'",
            )
            .bind(shift_id)
            .fetch_one(pool)
            .await
            .map_err(AppError::from)
        },
        async {
            sqlx::query_scalar::<_, i64>(
                "SELECT COALESCE(SUM(amount_minor), 0)
                 FROM cash_events WHERE shift_id = ? AND event_type = 'safe_drop'",
            )
            .bind(shift_id)
            .fetch_one(pool)
            .await
            .map_err(AppError::from)
        },
    )?;

    let expected_minor = opening_minor + cash_sales_minor - cash_refunds_minor + paid_in_minor
        - paid_out_minor
        - safe_drop_minor;
    let variance_minor = counted_minor.map(|c| c - expected_minor);

    let event_rows = sqlx::query(
        "SELECT cash_event_id, shift_id, event_type, amount_minor, note,
                created_by_user_id, created_at
         FROM cash_events WHERE shift_id = ? ORDER BY created_at",
    )
    .bind(shift_id)
    .fetch_all(pool)
    .await?;

    Ok(CashDrawerSummary {
        opening_minor,
        cash_sales_minor,
        pending_delivery_cash_minor,
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
    actor_user_id: String,
    shift_id: String,
    state: State<'_, AppState>,
) -> AppResult<CashDrawerSummary> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;
    drawer_summary_inner(&state.db, &shift_id).await
}

/// Record a no-sale drawer-open event (audit trail only — no monetary effect).
/// Inserts into `no_sale_events` and writes a NO_SALE audit_log entry.
#[tauri::command]
pub async fn cash_no_sale(
    shift_id: String,
    actor_user_id: String,
    note: Option<String>,
    state: State<'_, AppState>,
) -> AppResult<NoSaleRow> {
    // Any active user may trigger a no-sale (it is audited), but anonymous callers
    // (empty string, unknown user) must be rejected to prevent forged audit entries.
    crate::commands::rbac::require_any_role(&state.db, &actor_user_id).await?;
    let (branch_id, device_id) = resolve_branch_device(&state).await?;
    let no_sale_id = Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let mut tx = state.db.begin().await?;

    sqlx::query(
        "INSERT INTO no_sale_events
           (no_sale_id, shift_id, branch_id, device_id, actor_user_id, note, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&no_sale_id)
    .bind(&shift_id)
    .bind(&branch_id)
    .bind(&device_id)
    .bind(&actor_user_id)
    .bind(note.as_deref())
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    // Audit trail with hash chain
    let log_id = Ulid::new().to_string();
    let prev_hash = audit_hash::fetch_last_hash(&state.db, &device_id)
        .await
        .unwrap_or_default();
    let hash = audit_hash::compute_audit_hash(&audit_hash::AuditHashInput {
        audit_log_id: &log_id,
        event_type: "NO_SALE",
        entity_type: "shift",
        entity_id: &shift_id,
        actor_user_id: &actor_user_id,
        actor_type: "user",
        created_at: &now,
        before_json: None,
        after_json: None,
        reason: None,
        previous_hash: &prev_hash,
    });
    sqlx::query(
        "INSERT INTO audit_logs
           (audit_log_id, event_type, entity_type, entity_id,
            actor_user_id, actor_type, device_id, origin_device_id, branch_id, created_at, hash, previous_hash)
         VALUES (?, 'NO_SALE', 'shift', ?, ?, 'user', ?, ?, ?, ?, ?, ?)",
    )
    .bind(&log_id)
    .bind(&shift_id)
    .bind(&actor_user_id)
    .bind(&device_id)
    .bind(&device_id)
    .bind(&branch_id)
    .bind(&now)
    .bind(&hash)
    .bind(if prev_hash.is_empty() {
        None
    } else {
        Some(prev_hash.clone())
    })
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(NoSaleRow {
        no_sale_id,
        shift_id,
        actor_user_id,
        note,
        created_at: now,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Unit tests for cash_event_create validation logic
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    // T14: safe_drop and paid_out require a non-empty note (backend enforced)
    // These tests exercise the validation path that fires BEFORE the DB write.
    // We simulate the validation locally since cash_event_create is a Tauri command
    // that requires AppState; extract and test the rules directly.

    fn validate_cash_event(
        event_type: &str,
        amount_minor: i64,
        note: Option<&str>,
    ) -> Result<(), String> {
        if event_type != "paid_in" && event_type != "paid_out" && event_type != "safe_drop" {
            return Err("invalid event_type".into());
        }
        if amount_minor <= 0 {
            return Err("amount_minor must be positive".into());
        }
        if (event_type == "paid_out" || event_type == "safe_drop")
            && note.map(str::trim).unwrap_or("").is_empty()
        {
            return Err("A reason is required for Paid Out and Safe Drop events".into());
        }
        Ok(())
    }

    #[test]
    fn test_safe_drop_without_note_rejected() {
        let err = validate_cash_event("safe_drop", 1000, None).unwrap_err();
        assert!(
            err.contains("reason"),
            "safe_drop must require a reason: {err}"
        );
    }

    #[test]
    fn test_safe_drop_with_empty_note_rejected() {
        let err = validate_cash_event("safe_drop", 1000, Some("   ")).unwrap_err();
        assert!(
            err.contains("reason"),
            "whitespace-only note must be rejected: {err}"
        );
    }

    #[test]
    fn test_safe_drop_with_note_accepted() {
        validate_cash_event("safe_drop", 1000, Some("bag #3")).unwrap();
    }

    #[test]
    fn test_paid_out_without_note_rejected() {
        let err = validate_cash_event("paid_out", 500, None).unwrap_err();
        assert!(
            err.contains("reason"),
            "paid_out must require a reason: {err}"
        );
    }

    #[test]
    fn test_paid_in_without_note_accepted() {
        // paid_in note is optional
        validate_cash_event("paid_in", 2000, None).unwrap();
    }

    #[test]
    fn test_zero_amount_rejected() {
        let err = validate_cash_event("paid_in", 0, None).unwrap_err();
        assert!(
            err.contains("positive"),
            "zero amount must be rejected: {err}"
        );
    }

    #[test]
    fn test_negative_amount_rejected() {
        let err = validate_cash_event("paid_in", -100, None).unwrap_err();
        assert!(
            err.contains("positive"),
            "negative amount must be rejected: {err}"
        );
    }
}

/// X-Report: mid-shift drawer snapshot without closing the shift.
/// Logs an audit event and returns the current reconciliation totals.
/// Requires manager or owner role.
#[tauri::command]
pub async fn cash_x_report(
    shift_id: String,
    actor_user_id: String,
    state: State<'_, AppState>,
) -> AppResult<CashDrawerSummary> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;

    let summary = drawer_summary_inner(&state.db, &shift_id).await?;

    // Audit trail for X-Report generation with hash chain
    let log_id = Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let (branch_id, device_id) = resolve_branch_device(&state).await.unwrap_or_default();
    let prev_hash = audit_hash::fetch_last_hash(&state.db, &device_id)
        .await
        .unwrap_or_default();
    let hash = audit_hash::compute_audit_hash(&audit_hash::AuditHashInput {
        audit_log_id: &log_id,
        event_type: "X_REPORT",
        entity_type: "shift",
        entity_id: &shift_id,
        actor_user_id: &actor_user_id,
        actor_type: "user",
        created_at: &now,
        before_json: None,
        after_json: None,
        reason: None,
        previous_hash: &prev_hash,
    });
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "INSERT INTO audit_logs
           (audit_log_id, event_type, entity_type, entity_id,
            actor_user_id, actor_type, device_id, origin_device_id, branch_id, created_at, hash, previous_hash)
         VALUES (?, 'X_REPORT', 'shift', ?, ?, 'user', ?, ?, ?, ?, ?, ?)",
    )
    .bind(&log_id)
    .bind(&shift_id)
    .bind(&actor_user_id)
    .bind(&device_id)
    .bind(&device_id)
    .bind(&branch_id)
    .bind(&now)
    .bind(&hash)
    .bind(if prev_hash.is_empty() {
        None
    } else {
        Some(prev_hash.clone())
    })
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(summary)
}
