use crate::domain::report::TodaySummary;
use crate::errors::AppResult;
use crate::sync::scope::report_scope;
use sqlx::{Row, SqlitePool};

pub async fn today_summary(
    pool: &SqlitePool,
    branch_id: &str,
    business_date: &str,
) -> AppResult<TodaySummary> {
    let (scope, origin_device_id) = report_scope(pool).await;

    let sales_row = sqlx::query(
        "SELECT COUNT(*) as cnt,
                COALESCE(SUM(s.gross_total_minor), 0) as gross,
                COALESCE(SUM(s.discount_total_minor), 0) as discount,
                COALESCE(SUM(s.tax_total_minor), 0) as tax,
                COALESCE(SUM(s.net_total_minor), 0) as net
         FROM sales s
         WHERE s.branch_id = ? AND s.business_date = ? AND s.status != 'voided'
           AND (s.is_delivery = 0 OR EXISTS (
               SELECT 1 FROM delivery_orders d WHERE d.sale_id = s.sale_id AND d.payment_status = 'paid'
           ))
           AND (? = 'all' OR s.origin_device_id = ?)",
    )
    .bind(branch_id)
    .bind(business_date)
    .bind(scope.as_str())
    .bind(&origin_device_id)
    .fetch_one(pool)
    .await?;

    let cash_total: i64 = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(SUM(p.amount_minor), 0)
         FROM payments p JOIN sales s ON s.sale_id = p.sale_id
         WHERE s.branch_id = ? AND s.business_date = ? AND p.payment_method = 'cash'
           AND s.status != 'voided'
           AND (s.is_delivery = 0 OR EXISTS (
               SELECT 1 FROM delivery_orders d WHERE d.sale_id = s.sale_id AND d.payment_status = 'paid'
           ))
           AND (? = 'all' OR s.origin_device_id = ?)",
    )
    .bind(branch_id)
    .bind(business_date)
    .bind(scope.as_str())
    .bind(&origin_device_id)
    .fetch_one(pool)
    .await?;

    let card_total: i64 = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(SUM(p.amount_minor), 0)
         FROM payments p JOIN sales s ON s.sale_id = p.sale_id
         WHERE s.branch_id = ? AND s.business_date = ? AND p.payment_method = 'card'
           AND s.status != 'voided'
           AND (s.is_delivery = 0 OR EXISTS (
               SELECT 1 FROM delivery_orders d WHERE d.sale_id = s.sale_id AND d.payment_status = 'paid'
           ))
           AND (? = 'all' OR s.origin_device_id = ?)",
    )
    .bind(branch_id)
    .bind(business_date)
    .bind(scope.as_str())
    .bind(&origin_device_id)
    .fetch_one(pool)
    .await?;

    let (pending_delivery_count, pending_delivery_minor): (i64, i64) = {
        let row = sqlx::query(
            "SELECT COUNT(*) as cnt, COALESCE(SUM(s.net_total_minor), 0) as total
             FROM sales s
             WHERE s.branch_id = ? AND s.business_date = ? AND s.status != 'voided'
               AND s.is_delivery = 1
               AND NOT EXISTS (
                   SELECT 1 FROM delivery_orders d
                   WHERE d.sale_id = s.sale_id AND d.payment_status = 'paid'
               )
               AND (? = 'all' OR s.origin_device_id = ?)",
        )
        .bind(branch_id)
        .bind(business_date)
        .bind(scope.as_str())
        .bind(&origin_device_id)
        .fetch_one(pool)
        .await?;
        (row.get("cnt"), row.get("total"))
    };

    let refund_row = sqlx::query(
        "SELECT COUNT(*) as cnt, COALESCE(SUM(r.refund_total_minor), 0) as total
         FROM refunds r JOIN sales s ON s.sale_id = r.original_sale_id
         WHERE s.branch_id = ? AND s.business_date = ?
           AND (? = 'all' OR r.origin_device_id = ?)",
    )
    .bind(branch_id)
    .bind(business_date)
    .bind(scope.as_str())
    .bind(&origin_device_id)
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
        pending_delivery_count,
        pending_delivery_minor,
    })
}
