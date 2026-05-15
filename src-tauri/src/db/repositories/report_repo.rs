use sqlx::{SqlitePool, Row};
use crate::domain::report::TodaySummary;
use crate::errors::AppResult;

pub async fn today_summary(
    pool: &SqlitePool,
    branch_id: &str,
    business_date: &str,
) -> AppResult<TodaySummary> {
    let sales_row = sqlx::query(
        "SELECT COUNT(*) as cnt,
                COALESCE(SUM(gross_total_minor), 0) as gross,
                COALESCE(SUM(discount_total_minor), 0) as discount,
                COALESCE(SUM(tax_total_minor), 0) as tax,
                COALESCE(SUM(net_total_minor), 0) as net
         FROM sales WHERE branch_id = ? AND business_date = ? AND status != 'voided'"
    )
    .bind(branch_id).bind(business_date)
    .fetch_one(pool)
    .await?;

    let cash_total: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(p.amount_minor), 0)
         FROM payments p JOIN sales s ON s.sale_id = p.sale_id
         WHERE s.branch_id = ? AND s.business_date = ? AND p.payment_method = 'cash'"
    )
    .bind(branch_id).bind(business_date)
    .fetch_one(pool)
    .await?;

    let card_total: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(p.amount_minor), 0)
         FROM payments p JOIN sales s ON s.sale_id = p.sale_id
         WHERE s.branch_id = ? AND s.business_date = ? AND p.payment_method = 'card'"
    )
    .bind(branch_id).bind(business_date)
    .fetch_one(pool)
    .await?;

    let refund_row = sqlx::query(
        "SELECT COUNT(*) as cnt, COALESCE(SUM(r.refund_total_minor), 0) as total
         FROM refunds r JOIN sales s ON s.sale_id = r.original_sale_id
         WHERE s.branch_id = ? AND s.business_date = ?"
    )
    .bind(branch_id).bind(business_date)
    .fetch_one(pool)
    .await?;

    Ok(TodaySummary {
        business_date: business_date.to_string(),
        transaction_count: sales_row.get("cnt"),
        gross_total_minor: sales_row.get("gross"),
        discount_total_minor: sales_row.get("discount"),
        tax_total_minor: sales_row.get("tax"),
        net_total_minor: sales_row.get("net"),
        cash_total_minor: cash_total,
        card_total_minor: card_total,
        refund_count: refund_row.get("cnt"),
        refund_total_minor: refund_row.get("total"),
    })
}
