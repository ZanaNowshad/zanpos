use crate::db::repositories::audit_hash;
use crate::domain::shift::Shift;
use crate::errors::{AppError, AppResult};
use sqlx::{Row, SqlitePool};
use ulid::Ulid;

fn row_to_shift(row: &sqlx::sqlite::SqliteRow) -> Shift {
    Shift {
        shift_id: row.get("shift_id"),
        branch_id: row.get("branch_id"),
        device_id: row.get("device_id"),
        cashier_user_id: row.get("cashier_user_id"),
        cashier_name: row.get("cashier_name"),
        opened_at: row.get("opened_at"),
        closed_at: row.get("closed_at"),
        opening_cash_minor: row.get("opening_cash_minor"),
        status: row.get("status"),
    }
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
    const CASHIER: &str = "01JUSER000000000000CASH01";

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
        pool
    }

    // ── T4. Expected cash formula is correct including all components ─────────
    // Verifies: opening + cash_sales - cash_refunds + paid_in - paid_out - safe_drop
    #[tokio::test]
    async fn test_close_shift_expected_cash_formula() {
        let pool = make_pool().await;

        // Open a shift with BHD 10.000 opening float (10_000 minor)
        let shift = open_shift(&pool, BRANCH, DEVICE, CASHIER, 10_000)
            .await
            .expect("open shift");
        let shift_id = &shift.shift_id;

        // Insert a cash sale of BHD 5.000 (5_000 minor)
        let sale_id = ulid::Ulid::new().to_string();
        sqlx::query(
            "INSERT INTO sales (sale_id, receipt_number, branch_id, device_id, origin_device_id, shift_id,
             cashier_user_id, status, gross_total_minor, discount_total_minor,
             tax_total_minor, net_total_minor, currency, business_date,
             sold_at, created_offline, idempotency_key, sync_status)
             VALUES (?,?,?,?,?,?,?,'completed',5000,0,0,5000,'BHD','2026-01-01',
                     datetime('now'),0,'sale-t4-1','pending')",
        )
        .bind(&sale_id)
        .bind("MAIN-POS01-00000001")
        .bind(BRANCH)
        .bind(DEVICE)
        .bind(DEVICE)
        .bind(shift_id)
        .bind(CASHIER)
        .execute(&pool)
        .await
        .expect("insert sale");

        sqlx::query(
            "INSERT INTO payments
             (payment_id, sale_id, origin_device_id, payment_method, amount_minor, currency,
              recorded_by_user_id, recorded_at, sync_status)
             VALUES (?,?,?,'cash',5000,'BHD',?,datetime('now'),'pending')",
        )
        .bind(ulid::Ulid::new().to_string())
        .bind(&sale_id)
        .bind(DEVICE)
        .bind(CASHIER)
        .execute(&pool)
        .await
        .expect("insert payment");

        // Insert a refund of BHD 1.000 (1_000 minor) on that sale
        let refund_id = ulid::Ulid::new().to_string();
        sqlx::query(
            "INSERT INTO refunds
             (refund_id, original_sale_id, origin_device_id, refund_receipt_number, reason,
              return_reason_code, refund_total_minor, currency,
              created_by_user_id, created_at, sync_status, idempotency_key)
             VALUES (?,?,?,?,?,?,1000,'BHD',?,datetime('now'),'pending',?)",
        )
        .bind(&refund_id)
        .bind(&sale_id)
        .bind(DEVICE)
        .bind("MAIN-POS01-REF-00000001")
        .bind("changed mind")
        .bind("customer_return")
        .bind(CASHIER)
        .bind(format!("idem-ref-{refund_id}"))
        .execute(&pool)
        .await
        .expect("insert refund");

        // Paid In: BHD 2.000 (2_000 minor)
        sqlx::query(
            "INSERT INTO cash_events
             (cash_event_id, shift_id, branch_id, device_id, event_type,
              amount_minor, note, created_by_user_id, created_at)
             VALUES (?,?,'01JBRANCH0000000000000001','01JDEVICE0000000000000001',
                     'paid_in',2000,'test',?,datetime('now'))",
        )
        .bind(ulid::Ulid::new().to_string())
        .bind(shift_id)
        .bind(CASHIER)
        .execute(&pool)
        .await
        .expect("paid_in");

        // Safe Drop: BHD 3.000 (3_000 minor)
        sqlx::query(
            "INSERT INTO cash_events
             (cash_event_id, shift_id, branch_id, device_id, event_type,
              amount_minor, note, created_by_user_id, created_at)
             VALUES (?,?,'01JBRANCH0000000000000001','01JDEVICE0000000000000001',
                     'safe_drop',3000,'bag #1',?,datetime('now'))",
        )
        .bind(ulid::Ulid::new().to_string())
        .bind(shift_id)
        .bind(CASHIER)
        .execute(&pool)
        .await
        .expect("safe_drop");

        // Close the shift with a counted amount (doesn't matter for formula test)
        let closed = close_shift(&pool, shift_id, Some(12_000), None)
            .await
            .expect("close shift");

        // Expected formula:
        //   opening (10_000) + cash_sales (5_000) - refunds (1_000)
        //   + paid_in (2_000) - paid_out (0) - safe_drop (3_000) = 13_000
        let expected: i64 =
            sqlx::query_scalar("SELECT expected_cash_minor FROM shifts WHERE shift_id = ?")
                .bind(&closed.shift_id)
                .fetch_one(&pool)
                .await
                .expect("fetch expected");

        assert_eq!(
            expected, 13_000,
            "expected_cash_minor must equal 10000+5000-1000+2000-0-3000=13000"
        );
    }

    // ── T12. Second open shift on same device is rejected ─────────────────────
    #[tokio::test]
    async fn test_concurrent_open_shift_blocked() {
        let pool = make_pool().await;

        open_shift(&pool, BRANCH, DEVICE, CASHIER, 0)
            .await
            .expect("first shift");

        let err = open_shift(&pool, BRANCH, DEVICE, CASHIER, 0)
            .await
            .unwrap_err();

        assert!(
            matches!(err, AppError::Conflict(_)),
            "second open shift must be blocked: got {err:?}"
        );
    }
}

pub async fn get_active_shift(pool: &SqlitePool, device_id: &str) -> AppResult<Option<Shift>> {
    let row = sqlx::query(
        "SELECT s.shift_id, s.branch_id, s.device_id, s.cashier_user_id,
                s.opened_at, s.closed_at, s.opening_cash_minor, s.status,
                u.display_name as cashier_name
         FROM shifts s JOIN users u ON u.user_id = s.cashier_user_id
         WHERE s.device_id = ? AND s.status = 'open'
         ORDER BY s.opened_at DESC LIMIT 1",
    )
    .bind(device_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.as_ref().map(row_to_shift))
}

pub async fn open_shift(
    pool: &SqlitePool,
    branch_id: &str,
    device_id: &str,
    cashier_user_id: &str,
    opening_cash_minor: i64,
) -> AppResult<Shift> {
    let existing = get_active_shift(pool, device_id).await?;
    if existing.is_some() {
        return Err(AppError::Conflict(
            "A shift is already open for this device".into(),
        ));
    }

    let shift_id = Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    // business_date uses local time so EOD reports show the correct calendar day
    // for Bahrain (UTC+3) even when a shift is opened after midnight local time.
    let business_date = chrono::Local::now().format("%Y-%m-%d").to_string();

    sqlx::query(
        "INSERT INTO shifts (shift_id, branch_id, device_id, origin_device_id, cashier_user_id, opened_at,
                             opening_cash_minor, business_date, status, sync_status, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'open', 'pending', ?)"
    )
    .bind(&shift_id).bind(branch_id).bind(device_id).bind(device_id)
    .bind(cashier_user_id).bind(&now).bind(opening_cash_minor)
    .bind(&business_date).bind(&now)
    .execute(pool)
    .await?;

    let row = sqlx::query(
        "SELECT s.shift_id, s.branch_id, s.device_id, s.cashier_user_id,
                s.opened_at, s.closed_at, s.opening_cash_minor, s.status,
                u.display_name as cashier_name
         FROM shifts s JOIN users u ON u.user_id = s.cashier_user_id
         WHERE s.shift_id = ?",
    )
    .bind(&shift_id)
    .fetch_one(pool)
    .await?;

    let shift = row_to_shift(&row);
    tracing::info!("Shift opened: {}", shift_id);

    // Audit trail for shift open with hash chain
    let audit_id = Ulid::new().to_string();
    let after_json = serde_json::json!({
        "shift_id": &shift_id,
        "opening_cash_minor": opening_cash_minor,
    })
    .to_string();
    let prev_hash = audit_hash::fetch_last_hash(pool, device_id)
        .await
        .unwrap_or_default();
    let hash = audit_hash::compute_audit_hash(&audit_hash::AuditHashInput {
        audit_log_id: &audit_id,
        event_type: "shift.opened",
        entity_type: "shift",
        entity_id: &shift_id,
        actor_user_id: cashier_user_id,
        actor_type: "user",
        created_at: &now,
        before_json: None,
        after_json: Some(&after_json),
        reason: None,
        previous_hash: &prev_hash,
    });
    let _ = sqlx::query(
        "INSERT INTO audit_logs
           (audit_log_id, event_type, entity_type, entity_id,
            actor_user_id, actor_type, device_id, origin_device_id, branch_id, after_json, created_at, hash, previous_hash)
         VALUES (?, 'shift.opened', 'shift', ?, ?, 'user', ?, ?, ?, ?, ?, ?)",
    )
    .bind(&audit_id)
    .bind(&shift_id)
    .bind(cashier_user_id)
    .bind(device_id)
    .bind(device_id)
    .bind(branch_id)
    .bind(&after_json)
    .bind(&now)
    .bind(&hash)
    .bind(if prev_hash.is_empty() {
        None
    } else {
        Some(prev_hash.clone())
    })
    .execute(pool)
    .await;

    // sync_status='pending' is set by column DEFAULT — sync worker picks it up
    Ok(shift)
}

pub async fn close_shift(
    pool: &SqlitePool,
    shift_id: &str,
    counted_cash_minor: Option<i64>,
    notes: Option<String>,
) -> AppResult<Shift> {
    let now = chrono::Utc::now().to_rfc3339();

    // Expected cash = opening + cash_sales - cash_refunds + paid_in - paid_out - safe_drop.
    // This is the authoritative formula used everywhere (drawer_summary, EOD report).
    // Previously only opening + cash_sales was computed here — that was incorrect.
    // Only deduct refunds where the original sale had a cash payment component.
    // Card/wallet refunds do not reduce the physical cash drawer balance.
    // Unpaid deliveries are excluded from expected cash — the physical cash drawer
    // does not contain cash that hasn't been collected yet, matching drawer_summary_inner.
    let expected: Option<i64> = sqlx::query_scalar(
        "SELECT s.opening_cash_minor
              + COALESCE((SELECT SUM(p.amount_minor)
                          FROM payments p
                          JOIN sales sa ON sa.sale_id = p.sale_id
                          WHERE sa.shift_id = ? AND p.payment_method = 'cash'
                            AND sa.status != 'voided'
                            AND (sa.is_delivery = 0 OR EXISTS (
                                SELECT 1 FROM delivery_orders d
                                WHERE d.sale_id = sa.sale_id AND d.payment_status = 'paid'
                            ))
                         ), 0)
              - COALESCE((SELECT SUM(r.refund_total_minor)
                          FROM refunds r
                          JOIN sales sa ON sa.sale_id = r.original_sale_id
                          WHERE sa.shift_id = ?
                            AND EXISTS (
                                SELECT 1 FROM payments p2
                                WHERE p2.sale_id = sa.sale_id AND p2.payment_method = 'cash'
                            )), 0)
              + COALESCE((SELECT SUM(ce.amount_minor)
                          FROM cash_events ce
                          WHERE ce.shift_id = ? AND ce.event_type = 'paid_in'), 0)
              - COALESCE((SELECT SUM(ce.amount_minor)
                          FROM cash_events ce
                          WHERE ce.shift_id = ? AND ce.event_type = 'paid_out'), 0)
              - COALESCE((SELECT SUM(ce.amount_minor)
                          FROM cash_events ce
                          WHERE ce.shift_id = ? AND ce.event_type = 'safe_drop'), 0)
         FROM shifts s WHERE s.shift_id = ?",
    )
    .bind(shift_id) // cash_sales
    .bind(shift_id) // cash_refunds (cash-paid sales only)
    .bind(shift_id) // paid_in
    .bind(shift_id) // paid_out
    .bind(shift_id) // safe_drop
    .bind(shift_id) // FROM shifts WHERE shift_id
    .fetch_optional(pool)
    .await?
    .flatten();

    let diff = match (counted_cash_minor, expected) {
        (Some(counted), Some(exp)) => Some(counted - exp),
        _ => None,
    };

    let affected = sqlx::query(
        "UPDATE shifts SET status = 'closed', closed_at = ?, counted_cash_minor = ?,
         expected_cash_minor = ?, cash_difference_minor = ?, close_notes = ?, updated_at = ?
         WHERE shift_id = ? AND status = 'open'",
    )
    .bind(&now)
    .bind(counted_cash_minor)
    .bind(expected)
    .bind(diff)
    .bind(&notes)
    .bind(&now)
    .bind(shift_id)
    .execute(pool)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::Conflict(
            "Shift is already closed or does not exist".into(),
        ));
    }

    let row = sqlx::query(
        "SELECT s.shift_id, s.branch_id, s.device_id, s.cashier_user_id,
                s.opened_at, s.closed_at, s.opening_cash_minor, s.status,
                u.display_name as cashier_name
         FROM shifts s JOIN users u ON u.user_id = s.cashier_user_id
         WHERE s.shift_id = ?",
    )
    .bind(shift_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Shift not found".into()))?;

    let shift = row_to_shift(&row);
    tracing::info!("Shift closed: {}", shift_id);

    // Audit trail for shift close with hash chain
    let audit_id = Ulid::new().to_string();
    let after_json = serde_json::json!({
        "shift_id": shift_id,
        "counted_cash_minor": counted_cash_minor,
        "expected_cash_minor": expected,
        "cash_difference_minor": diff,
    })
    .to_string();
    let prev_hash = audit_hash::fetch_last_hash(pool, &shift.device_id)
        .await
        .unwrap_or_default();
    let hash = audit_hash::compute_audit_hash(&audit_hash::AuditHashInput {
        audit_log_id: &audit_id,
        event_type: "shift.closed",
        entity_type: "shift",
        entity_id: shift_id,
        actor_user_id: &shift.cashier_user_id,
        actor_type: "user",
        created_at: &now,
        before_json: None,
        after_json: Some(&after_json),
        reason: None,
        previous_hash: &prev_hash,
    });
    let _ = sqlx::query(
        "INSERT INTO audit_logs
           (audit_log_id, event_type, entity_type, entity_id,
            actor_user_id, actor_type, device_id, origin_device_id, branch_id, after_json, created_at, hash, previous_hash)
         VALUES (?, 'shift.closed', 'shift', ?, ?, 'user', ?, ?, ?, ?, ?, ?)",
    )
    .bind(&audit_id)
    .bind(shift_id)
    .bind(&shift.cashier_user_id)
    .bind(&shift.device_id)
    .bind(&shift.device_id)
    .bind(&shift.branch_id)
    .bind(&after_json)
    .bind(&now)
    .bind(&hash)
    .bind(if prev_hash.is_empty() {
        None
    } else {
        Some(prev_hash.clone())
    })
    .execute(pool)
    .await;

    // sync_status='pending' is set by column DEFAULT — sync worker picks it up
    Ok(shift)
}
