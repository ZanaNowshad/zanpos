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

// ─── Integrity check ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn db_integrity_check(state: State<'_, AppState>) -> Result<String, AppError> {
    let result: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&state.db)
        .await?;
    Ok(result)
}
