use crate::db::repositories::report_repo;
use crate::domain::report::TodaySummary;
use crate::errors::{AppError, AppResult};
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

#[tauri::command]
pub async fn report_today(
    branch_id: String,
    business_date: String,
    state: State<'_, AppState>,
) -> Result<TodaySummary, AppError> {
    report_repo::today_summary(&state.db, &branch_id, &business_date).await
}

// ─── Date-range commands ──────────────────────────────────────────────────────

#[tauri::command]
pub async fn report_date_range(
    branch_id: String,
    from_date: String,
    to_date: String,
    state: State<'_, AppState>,
) -> Result<RangeSummary, AppError> {
    let sales_row = sqlx::query(
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
           ))",
    )
    .bind(&branch_id)
    .bind(&from_date)
    .bind(&to_date)
    .fetch_one(&state.db)
    .await?;

    let cash: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(p.amount_minor), 0)
         FROM payments p JOIN sales s ON s.sale_id = p.sale_id
         WHERE s.branch_id = ? AND s.business_date BETWEEN ? AND ?
           AND p.payment_method = 'cash'
           AND (s.is_delivery = 0 OR EXISTS (
               SELECT 1 FROM delivery_orders d WHERE d.sale_id = s.sale_id AND d.payment_status = 'paid'
           ))",
    )
    .bind(&branch_id)
    .bind(&from_date)
    .bind(&to_date)
    .fetch_one(&state.db)
    .await?;

    let card: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(p.amount_minor), 0)
         FROM payments p JOIN sales s ON s.sale_id = p.sale_id
         WHERE s.branch_id = ? AND s.business_date BETWEEN ? AND ?
           AND p.payment_method = 'card'
           AND (s.is_delivery = 0 OR EXISTS (
               SELECT 1 FROM delivery_orders d WHERE d.sale_id = s.sale_id AND d.payment_status = 'paid'
           ))",
    )
    .bind(&branch_id)
    .bind(&from_date)
    .bind(&to_date)
    .fetch_one(&state.db)
    .await?;

    let (pending_count, pending_minor): (i64, i64) = {
        let row = sqlx::query(
            "SELECT COUNT(*) AS cnt, COALESCE(SUM(s.net_total_minor), 0) AS total
             FROM sales s
             WHERE s.branch_id = ? AND s.business_date BETWEEN ? AND ?
               AND s.status != 'voided'
               AND s.is_delivery = 1
               AND NOT EXISTS (
                   SELECT 1 FROM delivery_orders d
                   WHERE d.sale_id = s.sale_id AND d.payment_status = 'paid'
               )",
        )
        .bind(&branch_id)
        .bind(&from_date)
        .bind(&to_date)
        .fetch_one(&state.db)
        .await?;
        (row.get("cnt"), row.get("total"))
    };

    let refund_row = sqlx::query(
        "SELECT COUNT(*) AS cnt, COALESCE(SUM(r.refund_total_minor), 0) AS total
         FROM refunds r JOIN sales s ON s.sale_id = r.original_sale_id
         WHERE s.branch_id = ? AND s.business_date BETWEEN ? AND ?",
    )
    .bind(&branch_id)
    .bind(&from_date)
    .bind(&to_date)
    .fetch_one(&state.db)
    .await?;

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
    branch_id: String,
    from_date: String,
    to_date: String,
    state: State<'_, AppState>,
) -> Result<Vec<TopProduct>, AppError> {
    let rows = sqlx::query(
        "SELECT si.product_name_snapshot AS product_name,
                CAST(SUM(CAST(si.quantity AS REAL)) AS TEXT) AS total_quantity,
                SUM(si.line_total_minor)    AS revenue_minor,
                COUNT(DISTINCT si.sale_id)  AS transaction_count
         FROM sale_items si
         JOIN sales s ON s.sale_id = si.sale_id
         WHERE s.branch_id = ? AND s.business_date BETWEEN ? AND ?
           AND s.status != 'voided' AND si.voided = 0
         GROUP BY si.product_name_snapshot
         ORDER BY revenue_minor DESC
         LIMIT 15",
    )
    .bind(&branch_id)
    .bind(&from_date)
    .bind(&to_date)
    .fetch_all(&state.db)
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
    branch_id: String,
    from_date: String,
    to_date: String,
    state: State<'_, AppState>,
) -> Result<Vec<SaleListRow>, AppError> {
    let rows = sqlx::query(
        "SELECT s.sale_id, s.receipt_number, s.sold_at,
                s.net_total_minor, s.discount_total_minor, s.status,
                u.display_name AS cashier_name,
                GROUP_CONCAT(DISTINCT p.payment_method) AS payment_methods
         FROM sales s
         JOIN users u ON u.user_id = s.cashier_user_id
         LEFT JOIN payments p ON p.sale_id = s.sale_id
         WHERE s.branch_id = ? AND s.business_date BETWEEN ? AND ?
         GROUP BY s.sale_id
         ORDER BY s.sold_at DESC
         LIMIT 200",
    )
    .bind(&branch_id)
    .bind(&from_date)
    .bind(&to_date)
    .fetch_all(&state.db)
    .await?;

    Ok(rows
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
        .collect())
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
    branch_id: String,
    from_date: String,
    to_date: String,
    state: State<'_, AppState>,
) -> Result<Vec<CashierSummaryRow>, AppError> {
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
             GROUP BY s.cashier_user_id
         ),
         refund_totals AS (
             SELECT s.cashier_user_id,
                    COUNT(r.refund_id)                         AS refund_count,
                    COALESCE(SUM(r.refund_total_minor), 0)     AS refund_total
             FROM sales s
             JOIN refunds r ON r.original_sale_id = s.sale_id
             WHERE s.branch_id = ? AND s.business_date BETWEEN ? AND ?
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
         GROUP BY s.cashier_user_id
         ORDER BY net_total_minor DESC",
    )
    .bind(&branch_id) // payment_totals
    .bind(&from_date)
    .bind(&to_date)
    .bind(&branch_id) // refund_totals
    .bind(&from_date)
    .bind(&to_date)
    .bind(&branch_id) // main WHERE
    .bind(&from_date)
    .bind(&to_date)
    .fetch_all(&state.db)
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
    let shifts = sqlx::query(
        "SELECT s.shift_id, COALESCE(u.display_name,'Unknown') AS cashier_name,
                s.opened_at, s.closed_at,
                s.opening_cash_minor, s.counted_cash_minor
         FROM shifts s
         LEFT JOIN users u ON u.user_id = s.cashier_user_id
         WHERE s.branch_id = ? AND DATE(s.opened_at) = ?
         ORDER BY s.opened_at ASC",
    )
    .bind(branch_id)
    .bind(date)
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

        let cash_refunds: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(r.refund_total_minor),0) FROM refunds r
             JOIN sales s ON s.sale_id=r.original_sale_id WHERE s.shift_id=?",
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
    branch_id: String,
    date: String,
    state: State<'_, AppState>,
) -> Result<EodCashupReport, AppError> {
    report_eod_cashup_inner(&state.db, &branch_id, &date, &date).await
}

// ─── Integrity check ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn db_integrity_check(state: State<'_, AppState>) -> Result<String, AppError> {
    let result: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&state.db)
        .await?;
    Ok(result)
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
        // Seed adequate stock so finalize_sale in test setup doesn't fail on stock-out.
        sqlx::query("UPDATE stock_levels SET quantity_on_hand = '1000' WHERE branch_id = ?")
            .bind(BRANCH)
            .execute(&pool)
            .await
            .expect("seed stock");
        pool
    }

    async fn insert_shift(pool: &SqlitePool) -> String {
        let shift_id = ulid::Ulid::new().to_string();
        sqlx::query(
            "INSERT INTO shifts (shift_id, branch_id, device_id, cashier_user_id, opened_at, status)
             VALUES (?, ?, ?, ?, datetime('now'), 'open')",
        )
        .bind(&shift_id)
        .bind(BRANCH)
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
        sale_repo::finalize_sale(&pool, &cart, payments, "idem-t10-eod", None, false, None)
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
             (sale_id, receipt_number, branch_id, device_id, shift_id,
              cashier_user_id, status, gross_total_minor, discount_total_minor,
              tax_total_minor, net_total_minor, currency, business_date,
              sold_at, created_offline, idempotency_key, sync_status)
             VALUES (?,'MAIN-POS01-T11',?,?,?,?,'completed',100,0,0,100,'BHD',
                     '2026-01-01',datetime('now'),0,'idem-t11','pending')",
        )
        .bind(&sale_id)
        .bind(BRANCH)
        .bind(DEVICE)
        .bind(&shift_id)
        .bind(CASHIER)
        .execute(&pool)
        .await
        .expect("insert sale");

        // Try inserting a payment with an invalid method — must fail CHECK constraint
        let err = sqlx::query(
            "INSERT INTO payments
             (payment_id, sale_id, payment_method, amount_minor, currency,
              recorded_by_user_id, recorded_at, sync_status)
             VALUES (?,?,'bribe',100,'BHD',?,datetime('now'),'pending')",
        )
        .bind(ulid::Ulid::new().to_string())
        .bind(&sale_id)
        .bind(CASHIER)
        .execute(&pool)
        .await;

        assert!(
            err.is_err(),
            "invalid payment_method 'bribe' must violate CHECK constraint"
        );
    }
}
