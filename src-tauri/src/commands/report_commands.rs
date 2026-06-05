use crate::commands::rbac;
use crate::db::repositories::audit_hash;
use crate::db::repositories::report_repo;
use crate::domain::report::TodaySummary;
use crate::errors::{AppError, AppResult};
use crate::sync::scope::report_scope;
use crate::AppState;
use serde::Serialize;
use sqlx::{Row, SqlitePool};
use tauri::State;

// ─── Range report types ───────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct RangeSummary {
    pub from_date: String,
    pub to_date: String,
    pub transaction_count: i64,
    pub gross_total_minor: i64,
    pub discount_total_minor: i64,
    pub tax_total_minor: i64,
    pub net_total_minor: i64,
    pub cash_total_minor: i64,
    pub card_total_minor: i64,
    pub refund_count: i64,
    pub refund_total_minor: i64,
    pub pending_delivery_count: i64,
    pub pending_delivery_minor: i64,
}

#[derive(Debug, Serialize)]
pub struct TopProduct {
    pub product_name: String,
    pub total_quantity: String,
    pub revenue_minor: i64,
    pub transaction_count: i64,
}

#[derive(Debug, Serialize)]
pub struct SaleListRow {
    pub sale_id: String,
    pub receipt_number: String,
    pub sold_at: String,
    pub cashier_name: String,
    pub net_total_minor: i64,
    pub discount_total_minor: i64,
    pub status: String,
    pub payment_methods: String,
}

/// Paginated wrapper for the sales list.
/// `total` is the count of ALL matching rows (ignoring limit/offset) so the
/// caller can display page counts and detect truncation — fixes F-BIZ-002 / F-INT-001.
#[derive(Debug, Serialize)]
pub struct SaleListPage {
    pub items: Vec<SaleListRow>,
    pub total: i64,
    pub offset: i64,
    pub limit: i64,
}

#[tauri::command]
pub async fn report_today(
    actor_user_id: String,
    branch_id: String,
    business_date: String,
    state: State<'_, AppState>,
) -> Result<TodaySummary, AppError> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;
    report_repo::today_summary(&state.db, &branch_id, &business_date).await
}

// ─── Date-range commands ──────────────────────────────────────────────────────

#[tauri::command]
pub async fn report_date_range(
    actor_user_id: String,
    branch_id: String,
    from_date: String,
    to_date: String,
    state: State<'_, AppState>,
) -> Result<RangeSummary, AppError> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;
    // M6: Run the 5 independent range aggregation queries concurrently.
    // E: apply the device-scope filter uniformly: scope='all' short-circuits
    // the OR; scope='origin' requires origin_device_id to match this device.
    let pool = &state.db;
    let (scope, origin_device_id) = report_scope(pool).await;
    let scope_str = scope.as_str();
    let (sales_row, cash, card, pending_row, refund_row) = tokio::try_join!(
        sqlx::query(
            "SELECT COUNT(*) AS cnt,
                    COALESCE(SUM(s.gross_total_minor),    0) AS gross,
                    COALESCE(SUM(s.discount_total_minor), 0) AS discount,
                    COALESCE(SUM(s.tax_total_minor),      0) AS tax,
                    COALESCE(SUM(s.net_total_minor),      0) AS net
             FROM sales s
             WHERE s.branch_id = ? AND s.business_date BETWEEN ? AND ?
               AND s.status != 'voided'
               AND (s.is_delivery = 0 OR EXISTS (
                   SELECT 1 FROM delivery_orders d WHERE d.sale_id = s.sale_id AND d.payment_status = 'paid'
               ))
               AND (? = 'all' OR s.origin_device_id = ?)",
        )
        .bind(&branch_id).bind(&from_date).bind(&to_date)
        .bind(scope_str).bind(&origin_device_id)
        .fetch_one(pool),
        sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(SUM(p.amount_minor), 0)
             FROM payments p JOIN sales s ON s.sale_id = p.sale_id
             WHERE s.branch_id = ? AND s.business_date BETWEEN ? AND ?
               AND s.status != 'voided' AND p.payment_method = 'cash'
               AND (s.is_delivery = 0 OR EXISTS (
                   SELECT 1 FROM delivery_orders d WHERE d.sale_id = s.sale_id AND d.payment_status = 'paid'
               ))
               AND (? = 'all' OR s.origin_device_id = ?)",
        )
        .bind(&branch_id).bind(&from_date).bind(&to_date)
        .bind(scope_str).bind(&origin_device_id)
        .fetch_one(pool),
        sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(SUM(p.amount_minor), 0)
             FROM payments p JOIN sales s ON s.sale_id = p.sale_id
             WHERE s.branch_id = ? AND s.business_date BETWEEN ? AND ?
               AND s.status != 'voided' AND p.payment_method = 'card'
               AND (s.is_delivery = 0 OR EXISTS (
                   SELECT 1 FROM delivery_orders d WHERE d.sale_id = s.sale_id AND d.payment_status = 'paid'
               ))
               AND (? = 'all' OR s.origin_device_id = ?)",
        )
        .bind(&branch_id).bind(&from_date).bind(&to_date)
        .bind(scope_str).bind(&origin_device_id)
        .fetch_one(pool),
        sqlx::query(
            "SELECT COUNT(*) AS cnt, COALESCE(SUM(s.net_total_minor), 0) AS total
             FROM sales s
             WHERE s.branch_id = ? AND s.business_date BETWEEN ? AND ?
               AND s.status != 'voided' AND s.is_delivery = 1
               AND NOT EXISTS (
                   SELECT 1 FROM delivery_orders d
                   WHERE d.sale_id = s.sale_id AND d.payment_status = 'paid'
               )
               AND (? = 'all' OR s.origin_device_id = ?)",
        )
        .bind(&branch_id).bind(&from_date).bind(&to_date)
        .bind(scope_str).bind(&origin_device_id)
        .fetch_one(pool),
        sqlx::query(
            "SELECT COUNT(*) AS cnt, COALESCE(SUM(r.refund_total_minor), 0) AS total
             FROM refunds r JOIN sales s ON s.sale_id = r.original_sale_id
             WHERE s.branch_id = ? AND s.business_date BETWEEN ? AND ?
               AND (? = 'all' OR r.origin_device_id = ?)",
        )
        .bind(&branch_id).bind(&from_date).bind(&to_date)
        .bind(scope_str).bind(&origin_device_id)
        .fetch_one(pool),
    )?;

    let (pending_count, pending_minor): (i64, i64) = (pending_row.get("cnt"), pending_row.get("total"));

    Ok(RangeSummary {
        from_date,
        to_date,
        transaction_count: sales_row.get("cnt"),
        gross_total_minor: sales_row.get("gross"),
        discount_total_minor: sales_row.get("discount"),
        tax_total_minor: sales_row.get("tax"),
        net_total_minor: sales_row.get("net"),
        cash_total_minor: cash,
        card_total_minor: card,
        refund_count: refund_row.get("cnt"),
        refund_total_minor: refund_row.get("total"),
        pending_delivery_count: pending_count,
        pending_delivery_minor: pending_minor,
    })
}

#[tauri::command]
pub async fn report_top_products(
    actor_user_id: String,
    branch_id: String,
    from_date: String,
    to_date: String,
    state: State<'_, AppState>,
) -> Result<Vec<TopProduct>, AppError> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;
    let pool = &state.db;
    let (scope, origin_device_id) = report_scope(pool).await;
    let rows = sqlx::query(
        "SELECT si.product_name_snapshot AS product_name,
                CAST(SUM(CAST(si.quantity AS REAL)) AS TEXT) AS total_quantity,
                SUM(si.line_total_minor)    AS revenue_minor,
                COUNT(DISTINCT si.sale_id)  AS transaction_count
         FROM sale_items si
         JOIN sales s ON s.sale_id = si.sale_id
         WHERE s.branch_id = ? AND s.business_date BETWEEN ? AND ?
           AND s.status != 'voided' AND si.voided = 0
           AND (? = 'all' OR s.origin_device_id = ?)
         GROUP BY si.product_name_snapshot
         ORDER BY revenue_minor DESC
         LIMIT 15",
    )
    .bind(&branch_id)
    .bind(&from_date)
    .bind(&to_date)
    .bind(scope.as_str())
    .bind(&origin_device_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| TopProduct {
            product_name: r.get("product_name"),
            total_quantity: r.get("total_quantity"),
            revenue_minor: r.get("revenue_minor"),
            transaction_count: r.get("transaction_count"),
        })
        .collect())
}

#[tauri::command]
pub async fn report_sales_list(
    actor_user_id: String,
    branch_id: String,
    from_date: String,
    to_date: String,
    offset: Option<i64>,
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<SaleListPage, AppError> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;
    let limit = limit.unwrap_or(200).clamp(1, 500);
    let offset = offset.unwrap_or(0).max(0);

    let pool = &state.db;
    let (scope, origin_device_id) = report_scope(pool).await;

    // M13: COUNT must use identical JOINs/WHERE as the data query to avoid
    // pagination totals diverging from actual row counts. Use LEFT JOIN users
    // in both to count even if the cashier account was later deleted.
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT s.sale_id)
         FROM sales s
         LEFT JOIN users u ON u.user_id = s.cashier_user_id
         WHERE s.branch_id = ? AND s.business_date BETWEEN ? AND ?
           AND (? = 'all' OR s.origin_device_id = ?)",
    )
    .bind(&branch_id)
    .bind(&from_date)
    .bind(&to_date)
    .bind(scope.as_str())
    .bind(&origin_device_id)
    .fetch_one(pool)
    .await?;

    let rows = sqlx::query(
        "SELECT s.sale_id, s.receipt_number, s.sold_at,
                s.net_total_minor, s.discount_total_minor, s.status,
                COALESCE(u.display_name, '(deleted)') AS cashier_name,
                GROUP_CONCAT(DISTINCT p.payment_method) AS payment_methods
         FROM sales s
         LEFT JOIN users u ON u.user_id = s.cashier_user_id
         LEFT JOIN payments p ON p.sale_id = s.sale_id
         WHERE s.branch_id = ? AND s.business_date BETWEEN ? AND ?
           AND (? = 'all' OR s.origin_device_id = ?)
         GROUP BY s.sale_id
         ORDER BY s.sold_at DESC
         LIMIT ? OFFSET ?",
    )
    .bind(&branch_id)
    .bind(&from_date)
    .bind(&to_date)
    .bind(scope.as_str())
    .bind(&origin_device_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    let items = rows
        .iter()
        .map(|r| {
            let methods: Option<String> = r.get("payment_methods");
            SaleListRow {
                sale_id: r.get("sale_id"),
                receipt_number: r.get("receipt_number"),
                sold_at: r.get("sold_at"),
                cashier_name: r.get("cashier_name"),
                net_total_minor: r.get("net_total_minor"),
                discount_total_minor: r.get("discount_total_minor"),
                status: r.get("status"),
                payment_methods: methods.unwrap_or_default(),
            }
        })
        .collect();

    Ok(SaleListPage { items, total, offset, limit })
}

// ─── Sales by cashier ─────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct CashierSummaryRow {
    pub cashier_user_id: String,
    pub cashier_name: String,
    pub transaction_count: i64,
    pub net_total_minor: i64,
    pub cash_total_minor: i64,
    pub card_total_minor: i64,
    pub discount_total_minor: i64,
    pub refund_count: i64,
    pub refund_total_minor: i64,
}

/// Sales totals grouped by cashier for a date range.
/// Single CTE query — avoids the prior N+1 (4 queries per cashier).
#[tauri::command]
pub async fn report_by_cashier(
    actor_user_id: String,
    branch_id: String,
    from_date: String,
    to_date: String,
    state: State<'_, AppState>,
) -> Result<Vec<CashierSummaryRow>, AppError> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;
    let pool = &state.db;
    let (scope, origin_device_id) = report_scope(pool).await;
    let scope_str = scope.as_str();
    // payment_totals: cash and card per cashier, pre-aggregated before joining
    // to avoid inflating sale-level sums when a sale has multiple payment rows.
    // refund_totals: similarly pre-aggregated to avoid double-counting on sales
    // with multiple refunds.
    let rows = sqlx::query(
        "WITH payment_totals AS (
             SELECT s.cashier_user_id,
                    COALESCE(SUM(CASE WHEN p.payment_method = 'cash' THEN p.amount_minor ELSE 0 END), 0) AS cash_total,
                    COALESCE(SUM(CASE WHEN p.payment_method = 'card' THEN p.amount_minor ELSE 0 END), 0) AS card_total
             FROM sales s
             JOIN payments p ON p.sale_id = s.sale_id
             WHERE s.branch_id = ? AND s.business_date BETWEEN ? AND ?
               AND s.status != 'voided'
               AND (s.is_delivery = 0 OR EXISTS (SELECT 1 FROM delivery_orders d WHERE d.sale_id = s.sale_id AND d.payment_status = 'paid'))
               AND (? = 'all' OR s.origin_device_id = ?)
             GROUP BY s.cashier_user_id
         ),
         refund_totals AS (
             SELECT s.cashier_user_id,
                    COUNT(r.refund_id)                         AS refund_count,
                    COALESCE(SUM(r.refund_total_minor), 0)     AS refund_total
             FROM sales s
             JOIN refunds r ON r.original_sale_id = s.sale_id
             WHERE s.branch_id = ? AND s.business_date BETWEEN ? AND ?
               AND (? = 'all' OR r.origin_device_id = ?)
             GROUP BY s.cashier_user_id
         )
         SELECT s.cashier_user_id,
                COALESCE(u.display_name, s.cashier_user_id)   AS cashier_name,
                COUNT(s.sale_id)                               AS transaction_count,
                COALESCE(SUM(s.net_total_minor),      0)       AS net_total_minor,
                COALESCE(SUM(s.discount_total_minor), 0)       AS discount_total_minor,
                COALESCE(pt.cash_total,  0)                    AS cash_total_minor,
                COALESCE(pt.card_total,  0)                    AS card_total_minor,
                COALESCE(rt.refund_count, 0)                   AS refund_count,
                COALESCE(rt.refund_total, 0)                   AS refund_total_minor
         FROM sales s
         LEFT JOIN users u         ON u.user_id         = s.cashier_user_id
         LEFT JOIN payment_totals pt ON pt.cashier_user_id = s.cashier_user_id
         LEFT JOIN refund_totals  rt ON rt.cashier_user_id = s.cashier_user_id
         WHERE s.branch_id = ? AND s.business_date BETWEEN ? AND ?
           AND s.status != 'voided'
           AND (s.is_delivery = 0 OR EXISTS (SELECT 1 FROM delivery_orders d WHERE d.sale_id = s.sale_id AND d.payment_status = 'paid'))
           AND (? = 'all' OR s.origin_device_id = ?)
         GROUP BY s.cashier_user_id
         ORDER BY net_total_minor DESC",
    )
    .bind(&branch_id) // payment_totals WHERE
    .bind(&from_date)
    .bind(&to_date)
    .bind(scope_str)
    .bind(&origin_device_id)
    .bind(&branch_id) // refund_totals WHERE
    .bind(&from_date)
    .bind(&to_date)
    .bind(scope_str)
    .bind(&origin_device_id)
    .bind(&branch_id) // main WHERE
    .bind(&from_date)
    .bind(&to_date)
    .bind(scope_str)
    .bind(&origin_device_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| CashierSummaryRow {
            cashier_user_id: r.get("cashier_user_id"),
            cashier_name: r.get("cashier_name"),
            transaction_count: r.get("transaction_count"),
            net_total_minor: r.get("net_total_minor"),
            cash_total_minor: r.get("cash_total_minor"),
            card_total_minor: r.get("card_total_minor"),
            discount_total_minor: r.get("discount_total_minor"),
            refund_count: r.get("refund_count"),
            refund_total_minor: r.get("refund_total_minor"),
        })
        .collect())
}

// ─── End-of-day cash-up report ────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct EodShiftRow {
    pub shift_id: String,
    pub cashier_name: String,
    pub opened_at: String,
    pub closed_at: Option<String>,
    pub opening_minor: i64,
    pub cash_sales_minor: i64,
    pub safe_drop_minor: i64,
    pub paid_in_minor: i64,
    pub paid_out_minor: i64,
    pub expected_minor: i64,
    pub counted_minor: Option<i64>,
    pub variance_minor: Option<i64>,
    pub net_sales_minor: i64,
}

#[derive(Debug, Serialize)]
pub struct EodCashupReport {
    pub date: String,
    pub shifts: Vec<EodShiftRow>,
    pub total_net_minor: i64,
    pub total_cash_minor: i64,
    pub total_counted_minor: Option<i64>,
    pub total_variance_minor: Option<i64>,
}

/// Inner EOD cashup implementation — testable without AppState.
pub(crate) async fn report_eod_cashup_inner(
    pool: &SqlitePool,
    branch_id: &str,
    date: &str,
    _date_to: &str,
) -> AppResult<EodCashupReport> {
    // Use business_date (local date populated at shift-open time) rather than
    // DATE(opened_at) which is UTC and misattributes Bahrain late-night shifts
    // (opened between midnight and 03:00 local / 21:00–00:00 UTC) to the wrong day.
    // Fallback: DATE(opened_at, '+3 hours') covers rows backfilled from 0021.
    let (scope, origin_device_id) = report_scope(pool).await;
    let scope_str = scope.as_str();
    let shifts = sqlx::query(
        "SELECT s.shift_id, COALESCE(u.display_name,'Unknown') AS cashier_name,
                s.opened_at, s.closed_at,
                s.opening_cash_minor, s.counted_cash_minor
         FROM shifts s
         LEFT JOIN users u ON u.user_id = s.cashier_user_id
         WHERE s.branch_id = ?
           AND COALESCE(s.business_date, DATE(s.opened_at, '+3 hours')) = ?
           AND (? = 'all' OR s.origin_device_id = ?)
         ORDER BY s.opened_at ASC",
    )
    .bind(branch_id)
    .bind(date)
    .bind(scope_str)
    .bind(&origin_device_id)
    .fetch_all(pool)
    .await?;

    let mut rows: Vec<EodShiftRow> = Vec::new();
    let mut total_net: i64 = 0;
    let mut total_cash: i64 = 0;
    let mut total_counted: Option<i64> = Some(0);
    let mut all_counted = true;

    for sh in &shifts {
        let shift_id: String = sh.get("shift_id");
        let opening: i64 = sh.get("opening_cash_minor");
        let counted: Option<i64> = sh.get("counted_cash_minor");

        let net_sales: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(s.net_total_minor),0) FROM sales s
             WHERE s.shift_id=? AND s.status!='voided'
               AND (s.is_delivery = 0 OR EXISTS (
                   SELECT 1 FROM delivery_orders d WHERE d.sale_id = s.sale_id AND d.payment_status = 'paid'
               ))",
        ).bind(&shift_id).fetch_one(pool).await?;

        let cash_sales: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(p.amount_minor),0) FROM payments p
             JOIN sales s ON s.sale_id=p.sale_id
             WHERE s.shift_id=? AND p.payment_method='cash' AND s.status!='voided'
               AND (s.is_delivery = 0 OR EXISTS (
                   SELECT 1 FROM delivery_orders d WHERE d.sale_id = s.sale_id AND d.payment_status = 'paid'
               ))",
        )
        .bind(&shift_id)
        .fetch_one(pool)
        .await?;

        let safe_drop: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount_minor),0) FROM cash_events WHERE shift_id=? AND event_type='safe_drop'"
        ).bind(&shift_id).fetch_one(pool).await?;

        let paid_in: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount_minor),0) FROM cash_events WHERE shift_id=? AND event_type='paid_in'"
        ).bind(&shift_id).fetch_one(pool).await?;

        let paid_out: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount_minor),0) FROM cash_events WHERE shift_id=? AND event_type='paid_out'"
        ).bind(&shift_id).fetch_one(pool).await?;

        // Only deduct refunds that were paid in cash — card refunds don't reduce
        // the physical cash in the drawer.
        let cash_refunds: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(r.refund_total_minor),0) FROM refunds r
             JOIN sales s ON s.sale_id=r.original_sale_id
             WHERE s.shift_id=?
               AND EXISTS (
                   SELECT 1 FROM payments p
                   WHERE p.sale_id = s.sale_id AND p.payment_method = 'cash'
               )",
        )
        .bind(&shift_id)
        .fetch_one(pool)
        .await?;

        let expected = opening + cash_sales - cash_refunds + paid_in - paid_out - safe_drop;
        let variance = counted.map(|c| c - expected);

        total_net += net_sales;
        total_cash += cash_sales;
        if let Some(c) = counted {
            if let Some(ref mut tc) = total_counted {
                *tc += c;
            }
        } else {
            all_counted = false;
        }

        rows.push(EodShiftRow {
            shift_id,
            cashier_name: sh.get("cashier_name"),
            opened_at: sh.get("opened_at"),
            closed_at: sh.get("closed_at"),
            opening_minor: opening,
            cash_sales_minor: cash_sales,
            safe_drop_minor: safe_drop,
            paid_in_minor: paid_in,
            paid_out_minor: paid_out,
            expected_minor: expected,
            counted_minor: counted,
            variance_minor: variance,
            net_sales_minor: net_sales,
        });
    }

    if !all_counted {
        total_counted = None;
    }
    // Overall variance: sum of per-shift variances where available
    let total_variance_minor = rows
        .iter()
        .filter_map(|r| r.variance_minor)
        .reduce(|a, b| a + b);

    Ok(EodCashupReport {
        date: date.to_string(),
        shifts: rows,
        total_net_minor: total_net,
        total_cash_minor: total_cash,
        total_counted_minor: total_counted,
        total_variance_minor,
    })
}

/// End-of-day cash-up: Tauri command wrapper around `report_eod_cashup_inner`.
#[tauri::command]
pub async fn report_eod_cashup(
    actor_user_id: String,
    branch_id: String,
    date: String,
    state: State<'_, AppState>,
) -> Result<EodCashupReport, AppError> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;
    report_eod_cashup_inner(&state.db, &branch_id, &date, &date).await
}

/// Z-report: end-of-day cash-up summary with audit trail.
/// Wraps the EOD cashup logic and records a Z_REPORT_ISSUED audit entry
/// so every Z-report issuance is tamper-evident and traceable.
#[tauri::command]
pub async fn report_z_report(
    date: String,
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<EodCashupReport, AppError> {
    // Resolve active branch — Z-report always targets the current store.
    let branch_id: String = sqlx::query_scalar(
        "SELECT branch_id FROM branches WHERE is_active = 1 ORDER BY created_at LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await?
    .flatten()
    .unwrap_or_default();

    let report = report_eod_cashup_inner(&state.db, &branch_id, &date, &date).await?;

    // Resolve device for audit trail.
    let device_id: String = sqlx::query_scalar(
        "SELECT device_id FROM devices WHERE is_active = 1 ORDER BY device_code LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await?
    .flatten()
    .unwrap_or_default();

    // Record Z-report issuance in the audit hash-chain.
    let _ = audit_hash::insert_audit_entry(
        &state.db,
        "Z_REPORT_ISSUED",
        "report",
        &date,
        &actor_user_id,
        "user",
        &device_id,
        &branch_id,
        None,
        None,
        None,
    )
    .await;

    Ok(report)
}

// ─── Integrity check ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn db_integrity_check(state: State<'_, AppState>) -> Result<String, AppError> {
    let result: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&state.db)
        .await?;
    Ok(result)
}

// ─── Reports device-scope config (Phase E) ───────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ReportsConfig {
    /// 'origin' = this device only; 'all' = every device in the branch.
    pub device_scope: String,
    /// Echoed back so the UI can display "Showing all 3 devices" or
    /// "Showing this device only" without a second roundtrip.
    pub device_count: i64,
    /// Local device id (when scope='origin' this is the filter value).
    pub local_device_id: String,
}

#[tauri::command]
pub async fn reports_config_load(state: State<'_, AppState>) -> Result<ReportsConfig, AppError> {
    let (scope, local_device_id) = report_scope(&state.db).await;
    let device_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM devices WHERE is_active = 1",
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);
    Ok(ReportsConfig {
        device_scope: scope.as_str().to_string(),
        device_count,
        local_device_id,
    })
}

#[derive(serde::Deserialize)]
pub struct SaveReportsConfigInput {
    /// 'origin' or 'all' — anything else is coerced to 'origin'.
    pub device_scope: String,
    pub actor_user_id: String,
}

#[tauri::command]
pub async fn reports_config_save(
    input: SaveReportsConfigInput,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    // Only managers and owners can change report scope — it determines
    // whether cashiers see the whole store's takings or just their own.
    crate::commands::rbac::manager_or_owner(&state.db, &input.actor_user_id).await?;

    let normalized = match input.device_scope.to_ascii_lowercase().as_str() {
        "all" => "all",
        _     => "origin",
    };
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO app_config (key, value, updated_at) VALUES ('reports_device_scope', ?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(normalized)
    .bind(&now)
    .execute(&state.db)
    .await?;

    // sync_status='pending' is set by column DEFAULT — sync worker picks it up

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Integration tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repositories::sale_repo;
    use crate::domain::cart::{Cart, CartLine};
    use crate::domain::sale::PaymentInput;
    use sqlx::sqlite::SqlitePoolOptions;

    const BRANCH: &str = "01JBRANCH0000000000000001";
    const DEVICE: &str = "01JDEVICE0000000000000001";
    const CASHIER: &str = "01JUSER000000000000CASH01";
    const TAX_ZER: &str = "01JTAX000000000000ZERO01";

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
        sqlx::query("UPDATE devices SET is_active = 1 WHERE device_id = '01JDEVICE0000000000000001'")
            .execute(&pool).await.ok();
        sqlx::query("UPDATE branches SET is_active = 1 WHERE branch_id = '01JBRANCH0000000000000001'")
            .execute(&pool).await.ok();

        // Seed tax rules needed by the test product
        sqlx::query(
            "INSERT OR IGNORE INTO tax_rules (tax_rule_id, name, rate_basis_points, inclusive, is_active, effective_from, created_at, updated_at, version)
             VALUES ('01JTAX000000000000ZERO01', 'Zero-rated', 0, 0, 1, datetime('now'), datetime('now'), datetime('now'), 1)"
        ).execute(&pool).await.expect("seed test tax rule");

        // Seed cashier user
        sqlx::query(
            "INSERT OR IGNORE INTO users (user_id, branch_id, display_name, username, pin_hash, role_id, is_active, created_at, updated_at, version)
             VALUES ('01JUSER000000000000CASH01', '01JBRANCH0000000000000001', 'Cashier 1', 'cashier1', 'PLAIN:1234', '01JROLES000000000000000003', 1, datetime('now'), datetime('now'), 1)"
        ).execute(&pool).await.expect("seed test cashier");

        // Re-seed product + stock needed by tests.
        sqlx::query(
            "INSERT OR IGNORE INTO categories (category_id, name, sort_order, is_active, created_at, updated_at, version)
             VALUES ('01JCAT000000000000DRINK01', 'Drinks', 1, 1, datetime('now'), datetime('now'), 1)"
        ).execute(&pool).await.expect("seed test category");
        sqlx::query(
            "INSERT OR IGNORE INTO products
             (product_id, category_id, name, sku, barcode, description, track_inventory, reorder_point, is_active, tax_rule_id, currency, created_at, updated_at, version)
             VALUES ('01JPROD00000000000WATR001', '01JCAT000000000000DRINK01', 'Water 500ml', 'WATR-500', '6281001511222', NULL, 1, 10, 1, '01JTAX000000000000ZERO01', 'BHD', datetime('now'), datetime('now'), 1)"
        ).execute(&pool).await.expect("seed test product");
        sqlx::query(
            "INSERT OR IGNORE INTO stock_levels (stock_level_id, product_id, branch_id, quantity_on_hand, created_at, updated_at, sync_status, sync_attempts)
             VALUES ('SL-TEST-WATR', '01JPROD00000000000WATR001', ?, '1000', datetime('now'), datetime('now'), 'synced', 0)"
        ).bind(BRANCH).execute(&pool).await.expect("seed test stock");
        pool
    }

    async fn insert_shift(pool: &SqlitePool) -> String {
        let shift_id = ulid::Ulid::new().to_string();
        sqlx::query(
            "INSERT INTO shifts (shift_id, branch_id, device_id, origin_device_id, cashier_user_id, opened_at, status, created_at, updated_at, version, sync_status, sync_attempts)
             VALUES (?, ?, ?, ?, ?, datetime('now'), 'open', datetime('now'), datetime('now'), 1, 'pending', 0)"
        )
        .bind(&shift_id)
        .bind(BRANCH)
        .bind(DEVICE)
        .bind(DEVICE)
        .bind(CASHIER)
        .execute(pool)
        .await
        .expect("insert shift");
        shift_id
    }

    // ── T10. EOD cashup report shows correct cashier name (not empty) ─────────
    #[tokio::test]
    async fn test_eod_cashup_cashier_name_populated() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;

        // Create a water sale so there's real data
        let mut cart = Cart::new(
            BRANCH.into(),
            DEVICE.into(),
            shift_id.clone(),
            CASHIER.into(),
        );
        cart.lines.push(CartLine {
            cart_line_id: ulid::Ulid::new().to_string(),
            product_id: Some("01JPROD00000000000WATR001".into()),
            product_name: "Water 500ml".into(),
            sku: Some("WATR500".into()),
            barcode: None,
            quantity: "1".to_string(),
            unit_price_minor: 250,
            line_discount_minor: 0,
            line_discount_reason: None,
            tax_rule_id: TAX_ZER.to_string(),
            tax_rate_basis_points: 0,
            tax_inclusive: false,
            tax_amount_minor: 0,
            line_total_minor: 250,
            note: None,
            voided: false,
        });
        let payments = vec![PaymentInput {
            method: "cash".into(),
            amount_minor: 250,
            tendered_minor: Some(250),
            external_reference: None,
        }];
        sale_repo::finalize_sale(&pool, &cart, payments, "idem-t10-eod", None, false, None, false)
            .await
            .expect("finalize sale");

        // Close the shift
        crate::db::repositories::shift_repo::close_shift(&pool, &shift_id, Some(250 + 0), None)
            .await
            .expect("close shift");

        // EOD report for today
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let rows = report_eod_cashup_inner(&pool, BRANCH, &today, &today)
            .await
            .expect("eod cashup");

        assert_eq!(rows.shifts.len(), 1, "should have one shift row");
        let row = &rows.shifts[0];

        // cashier_name must be 'Cashier 1' (from seed data), not empty or 'Unknown'
        assert!(
            !row.cashier_name.is_empty(),
            "cashier_name must not be empty, got: '{}'",
            row.cashier_name
        );
        assert_ne!(
            row.cashier_name, "Unknown",
            "cashier_name must be resolved from users table, not 'Unknown'"
        );
        assert_eq!(
            row.cashier_name, "Cashier 1",
            "cashier_name must match seed data"
        );
    }

    // ── T11. payment_method CHECK constraint rejects invalid values ───────────
    #[tokio::test]
    async fn test_payment_method_check_constraint() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;

        // Insert a minimal sale to reference
        let sale_id = ulid::Ulid::new().to_string();
        sqlx::query(
            "INSERT INTO sales
             (sale_id, receipt_number, branch_id, device_id, origin_device_id, shift_id,
              cashier_user_id, status, gross_total_minor, discount_total_minor,
              tax_total_minor, net_total_minor, currency, business_date,
              sold_at, created_offline, idempotency_key, sync_status, created_at, updated_at)
             VALUES (?,'MAIN-POS01-T11',?,?,?,?,?,'completed',100,0,0,100,'BHD',
                     '2026-01-01',datetime('now'),0,'idem-t11','pending',datetime('now'),datetime('now'))",
        )
        .bind(&sale_id)
        .bind(BRANCH)
        .bind(DEVICE)
        .bind(DEVICE)
        .bind(&shift_id)
        .bind(CASHIER)
        .execute(&pool)
        .await
        .expect("insert sale");

        // Try inserting a payment with an invalid method — must fail CHECK constraint
        let err = sqlx::query(
            "INSERT INTO payments
             (payment_id, sale_id, origin_device_id, payment_method, amount_minor, currency,
              recorded_by_user_id, recorded_at, created_at, updated_at)
             VALUES (?,?,?,'bribe',100,'BHD',?,datetime('now'),datetime('now'),datetime('now'))",
        )
        .bind(ulid::Ulid::new().to_string())
        .bind(&sale_id)
        .bind(DEVICE)
        .bind(CASHIER)
        .execute(&pool)
        .await;

        assert!(
            err.is_err(),
            "invalid payment_method 'bribe' must violate CHECK constraint"
        );
    }
}
