use tauri::State;
use sqlx::Row;
use serde::Serialize;
use crate::domain::report::TodaySummary;
use crate::db::repositories::report_repo;
use crate::errors::AppError;
use crate::AppState;

// ─── Range report types ───────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct RangeSummary {
    pub from_date:            String,
    pub to_date:              String,
    pub transaction_count:    i64,
    pub gross_total_minor:    i64,
    pub discount_total_minor: i64,
    pub tax_total_minor:      i64,
    pub net_total_minor:      i64,
    pub cash_total_minor:     i64,
    pub card_total_minor:     i64,
    pub refund_count:         i64,
    pub refund_total_minor:   i64,
}

#[derive(Debug, Serialize)]
pub struct TopProduct {
    pub product_name:      String,
    pub total_quantity:    String,
    pub revenue_minor:     i64,
    pub transaction_count: i64,
}

#[derive(Debug, Serialize)]
pub struct SaleListRow {
    pub sale_id:              String,
    pub receipt_number:       String,
    pub sold_at:              String,
    pub cashier_name:         String,
    pub net_total_minor:      i64,
    pub discount_total_minor: i64,
    pub status:               String,
    pub payment_methods:      String,
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
    from_date:  String,
    to_date:    String,
    state: State<'_, AppState>,
) -> Result<RangeSummary, AppError> {
    let sales_row = sqlx::query(
        "SELECT COUNT(*) AS cnt,
                COALESCE(SUM(gross_total_minor),    0) AS gross,
                COALESCE(SUM(discount_total_minor), 0) AS discount,
                COALESCE(SUM(tax_total_minor),      0) AS tax,
                COALESCE(SUM(net_total_minor),      0) AS net
         FROM sales
         WHERE branch_id = ? AND business_date BETWEEN ? AND ?
           AND status != 'voided'"
    )
    .bind(&branch_id).bind(&from_date).bind(&to_date)
    .fetch_one(&state.db).await?;

    let cash: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(p.amount_minor), 0)
         FROM payments p JOIN sales s ON s.sale_id = p.sale_id
         WHERE s.branch_id = ? AND s.business_date BETWEEN ? AND ?
           AND p.payment_method = 'cash'"
    )
    .bind(&branch_id).bind(&from_date).bind(&to_date)
    .fetch_one(&state.db).await?;

    let card: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(p.amount_minor), 0)
         FROM payments p JOIN sales s ON s.sale_id = p.sale_id
         WHERE s.branch_id = ? AND s.business_date BETWEEN ? AND ?
           AND p.payment_method = 'card'"
    )
    .bind(&branch_id).bind(&from_date).bind(&to_date)
    .fetch_one(&state.db).await?;

    let refund_row = sqlx::query(
        "SELECT COUNT(*) AS cnt, COALESCE(SUM(r.refund_total_minor), 0) AS total
         FROM refunds r JOIN sales s ON s.sale_id = r.original_sale_id
         WHERE s.branch_id = ? AND s.business_date BETWEEN ? AND ?"
    )
    .bind(&branch_id).bind(&from_date).bind(&to_date)
    .fetch_one(&state.db).await?;

    Ok(RangeSummary {
        from_date, to_date,
        transaction_count:    sales_row.get("cnt"),
        gross_total_minor:    sales_row.get("gross"),
        discount_total_minor: sales_row.get("discount"),
        tax_total_minor:      sales_row.get("tax"),
        net_total_minor:      sales_row.get("net"),
        cash_total_minor:     cash,
        card_total_minor:     card,
        refund_count:         refund_row.get("cnt"),
        refund_total_minor:   refund_row.get("total"),
    })
}

#[tauri::command]
pub async fn report_top_products(
    branch_id: String,
    from_date:  String,
    to_date:    String,
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
         LIMIT 15"
    )
    .bind(&branch_id).bind(&from_date).bind(&to_date)
    .fetch_all(&state.db).await?;

    Ok(rows.iter().map(|r| TopProduct {
        product_name:      r.get("product_name"),
        total_quantity:    r.get("total_quantity"),
        revenue_minor:     r.get("revenue_minor"),
        transaction_count: r.get("transaction_count"),
    }).collect())
}

#[tauri::command]
pub async fn report_sales_list(
    branch_id: String,
    from_date:  String,
    to_date:    String,
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
         LIMIT 200"
    )
    .bind(&branch_id).bind(&from_date).bind(&to_date)
    .fetch_all(&state.db).await?;

    Ok(rows.iter().map(|r| {
        let methods: Option<String> = r.get("payment_methods");
        SaleListRow {
            sale_id:              r.get("sale_id"),
            receipt_number:       r.get("receipt_number"),
            sold_at:              r.get("sold_at"),
            cashier_name:         r.get("cashier_name"),
            net_total_minor:      r.get("net_total_minor"),
            discount_total_minor: r.get("discount_total_minor"),
            status:               r.get("status"),
            payment_methods:      methods.unwrap_or_default(),
        }
    }).collect())
}

// ─── Sales by cashier ─────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct CashierSummaryRow {
    pub cashier_user_id:      String,
    pub cashier_name:         String,
    pub transaction_count:    i64,
    pub net_total_minor:      i64,
    pub cash_total_minor:     i64,
    pub card_total_minor:     i64,
    pub discount_total_minor: i64,
    pub refund_count:         i64,
    pub refund_total_minor:   i64,
}

/// Sales totals grouped by cashier for a date range.
#[tauri::command]
pub async fn report_by_cashier(
    branch_id: String,
    from_date:  String,
    to_date:    String,
    state: State<'_, AppState>,
) -> Result<Vec<CashierSummaryRow>, AppError> {
    let rows = sqlx::query(
        "SELECT s.cashier_user_id,
                COALESCE(u.display_name, s.cashier_user_id) AS cashier_name,
                COUNT(s.sale_id)                            AS transaction_count,
                COALESCE(SUM(s.net_total_minor),      0)    AS net_total_minor,
                COALESCE(SUM(s.discount_total_minor), 0)    AS discount_total_minor
         FROM sales s
         LEFT JOIN users u ON u.user_id = s.cashier_user_id
         WHERE s.branch_id = ? AND s.business_date BETWEEN ? AND ?
           AND s.status != 'voided'
         GROUP BY s.cashier_user_id
         ORDER BY net_total_minor DESC"
    )
    .bind(&branch_id).bind(&from_date).bind(&to_date)
    .fetch_all(&state.db).await?;

    let mut result = Vec::new();
    for r in &rows {
        let uid: String = r.get("cashier_user_id");

        let cash: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(p.amount_minor),0) FROM payments p
             JOIN sales s ON s.sale_id=p.sale_id
             WHERE s.branch_id=? AND s.business_date BETWEEN ? AND ?
               AND s.cashier_user_id=? AND p.payment_method='cash' AND s.status!='voided'"
        ).bind(&branch_id).bind(&from_date).bind(&to_date).bind(&uid)
         .fetch_one(&state.db).await?;

        let card: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(p.amount_minor),0) FROM payments p
             JOIN sales s ON s.sale_id=p.sale_id
             WHERE s.branch_id=? AND s.business_date BETWEEN ? AND ?
               AND s.cashier_user_id=? AND p.payment_method='card' AND s.status!='voided'"
        ).bind(&branch_id).bind(&from_date).bind(&to_date).bind(&uid)
         .fetch_one(&state.db).await?;

        let refund_cnt: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM refunds ref
             JOIN sales s ON s.sale_id=ref.original_sale_id
             WHERE s.branch_id=? AND s.business_date BETWEEN ? AND ?
               AND s.cashier_user_id=?"
        ).bind(&branch_id).bind(&from_date).bind(&to_date).bind(&uid)
         .fetch_one(&state.db).await?;

        let refund_total: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(ref.refund_total_minor),0) FROM refunds ref
             JOIN sales s ON s.sale_id=ref.original_sale_id
             WHERE s.branch_id=? AND s.business_date BETWEEN ? AND ?
               AND s.cashier_user_id=?"
        ).bind(&branch_id).bind(&from_date).bind(&to_date).bind(&uid)
         .fetch_one(&state.db).await?;

        result.push(CashierSummaryRow {
            cashier_user_id:      uid,
            cashier_name:         r.get("cashier_name"),
            transaction_count:    r.get("transaction_count"),
            net_total_minor:      r.get("net_total_minor"),
            cash_total_minor:     cash,
            card_total_minor:     card,
            discount_total_minor: r.get("discount_total_minor"),
            refund_count:         refund_cnt,
            refund_total_minor:   refund_total,
        });
    }
    Ok(result)
}

// ─── End-of-day cash-up report ────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct EodShiftRow {
    pub shift_id:         String,
    pub cashier_name:     String,
    pub opened_at:        String,
    pub closed_at:        Option<String>,
    pub opening_minor:    i64,
    pub cash_sales_minor: i64,
    pub safe_drop_minor:  i64,
    pub paid_in_minor:    i64,
    pub paid_out_minor:   i64,
    pub expected_minor:   i64,
    pub counted_minor:    Option<i64>,
    pub variance_minor:   Option<i64>,
    pub net_sales_minor:  i64,
}

#[derive(Debug, Serialize)]
pub struct EodCashupReport {
    pub date:               String,
    pub shifts:             Vec<EodShiftRow>,
    pub total_net_minor:    i64,
    pub total_cash_minor:   i64,
    pub total_counted_minor: Option<i64>,
    pub total_variance_minor: Option<i64>,
}

/// End-of-day cash-up: all shifts for a business date with reconciliation totals.
#[tauri::command]
pub async fn report_eod_cashup(
    branch_id: String,
    date:       String,
    state: State<'_, AppState>,
) -> Result<EodCashupReport, AppError> {
    let shifts = sqlx::query(
        "SELECT s.shift_id, COALESCE(u.display_name,'Unknown') AS cashier_name,
                s.opened_at, s.closed_at,
                s.opening_cash_minor, s.counted_cash_minor
         FROM shifts s
         LEFT JOIN users u ON u.user_id = s.opened_by_user_id
         WHERE s.branch_id = ? AND DATE(s.opened_at) = ?
         ORDER BY s.opened_at ASC"
    )
    .bind(&branch_id).bind(&date)
    .fetch_all(&state.db).await?;

    let mut rows: Vec<EodShiftRow> = Vec::new();
    let mut total_net: i64 = 0;
    let mut total_cash: i64 = 0;
    let mut total_counted: Option<i64> = Some(0);
    let mut all_counted = true;

    for sh in &shifts {
        let shift_id: String          = sh.get("shift_id");
        let opening: i64              = sh.get("opening_cash_minor");
        let counted: Option<i64>      = sh.get("counted_cash_minor");

        let net_sales: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(net_total_minor),0) FROM sales WHERE shift_id=? AND status!='voided'"
        ).bind(&shift_id).fetch_one(&state.db).await?;

        let cash_sales: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(p.amount_minor),0) FROM payments p
             JOIN sales s ON s.sale_id=p.sale_id
             WHERE s.shift_id=? AND p.payment_method='cash' AND s.status!='voided'"
        ).bind(&shift_id).fetch_one(&state.db).await?;

        let safe_drop: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount_minor),0) FROM cash_events WHERE shift_id=? AND event_type='safe_drop'"
        ).bind(&shift_id).fetch_one(&state.db).await?;

        let paid_in: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount_minor),0) FROM cash_events WHERE shift_id=? AND event_type='paid_in'"
        ).bind(&shift_id).fetch_one(&state.db).await?;

        let paid_out: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount_minor),0) FROM cash_events WHERE shift_id=? AND event_type='paid_out'"
        ).bind(&shift_id).fetch_one(&state.db).await?;

        let cash_refunds: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(r.refund_total_minor),0) FROM refunds r
             JOIN sales s ON s.sale_id=r.original_sale_id WHERE s.shift_id=?"
        ).bind(&shift_id).fetch_one(&state.db).await?;

        let expected = opening + cash_sales - cash_refunds + paid_in - paid_out - safe_drop;
        let variance = counted.map(|c| c - expected);

        total_net  += net_sales;
        total_cash += cash_sales;
        if let Some(c) = counted {
            if let Some(ref mut tc) = total_counted { *tc += c; }
        } else {
            all_counted = false;
        }

        rows.push(EodShiftRow {
            shift_id,
            cashier_name:     sh.get("cashier_name"),
            opened_at:        sh.get("opened_at"),
            closed_at:        sh.get("closed_at"),
            opening_minor:    opening,
            cash_sales_minor: cash_sales,
            safe_drop_minor:  safe_drop,
            paid_in_minor:    paid_in,
            paid_out_minor:   paid_out,
            expected_minor:   expected,
            counted_minor:    counted,
            variance_minor:   variance,
            net_sales_minor:  net_sales,
        });
    }

    if !all_counted { total_counted = None; }
    // Overall variance: sum of per-shift variances where available
    let total_variance_minor = rows.iter()
        .filter_map(|r| r.variance_minor)
        .reduce(|a, b| a + b);

    Ok(EodCashupReport {
        date,
        shifts: rows,
        total_net_minor: total_net,
        total_cash_minor: total_cash,
        total_counted_minor: total_counted,
        total_variance_minor,
    })
}

// ─── Integrity check ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn db_integrity_check(state: State<'_, AppState>) -> Result<String, AppError> {
    let result: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&state.db)
        .await?;
    Ok(result)
}
