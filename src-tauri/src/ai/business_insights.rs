use crate::errors::{AppError, AppResult};
use chrono::{Datelike, NaiveDate};
use serde::Serialize;
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;

#[derive(Debug, Serialize)]
pub struct MarginErosionRow {
    pub product_id: String,
    pub product_name: String,
    pub selling_price_minor: i64,
    pub old_cost_minor: i64,
    pub new_cost_minor: i64,
    pub margin_drop_basis_points: i64,
    pub current_margin_basis_points: i64,
}

#[derive(Debug, Serialize)]
pub struct MarginErosionReport {
    pub rows: Vec<MarginErosionRow>,
    pub unknown_cost_line_count: i64,
}

#[derive(Debug, Serialize)]
pub struct SeasonalDemandRow {
    pub product_id: String,
    pub product_name: String,
    pub historical_average_quantity: f64,
    pub quantity_on_hand: f64,
    pub suggested_reorder_quantity: f64,
}

#[derive(Debug, Serialize)]
pub struct CashFlowForecast {
    pub delivery_cod_receivable_minor: i64,
    pub purchase_commitments_minor: i64,
    pub net_known_position_minor: i64,
    pub unknown_cost_po_line_count: i64,
}

pub fn margin_drop_basis_points(
    selling_price_minor: i64,
    old_cost_minor: i64,
    new_cost_minor: i64,
) -> Option<(i64, i64)> {
    if selling_price_minor <= 0 || new_cost_minor <= old_cost_minor {
        return None;
    }
    let drop = (new_cost_minor - old_cost_minor).saturating_mul(10_000) / selling_price_minor;
    let current =
        (selling_price_minor - new_cost_minor).saturating_mul(10_000) / selling_price_minor;
    Some((drop, current))
}

pub fn prior_year_period(
    from: NaiveDate,
    to: NaiveDate,
    years_back: i32,
) -> AppResult<(NaiveDate, NaiveDate)> {
    if years_back <= 0 || years_back > 5 || to < from {
        return Err(AppError::Validation(
            "Historical comparison must use 1..=5 prior years and a valid period".into(),
        ));
    }
    fn shift(date: NaiveDate, years: i32) -> AppResult<NaiveDate> {
        let year = date.year() - years;
        date.with_year(year)
            .or_else(|| NaiveDate::from_ymd_opt(year, date.month(), 28))
            .ok_or_else(|| AppError::Validation("Historical period is outside range".into()))
    }
    Ok((shift(from, years_back)?, shift(to, years_back)?))
}

pub async fn margin_erosion(
    pool: &SqlitePool,
    branch_id: &str,
    threshold_basis_points: i64,
) -> AppResult<MarginErosionReport> {
    if !(1..=10_000).contains(&threshold_basis_points) {
        return Err(AppError::Validation(
            "Margin erosion threshold must be 1..=10000 basis points".into(),
        ));
    }
    let rows = sqlx::query(
        "WITH latest_cost AS (
           SELECT h.*
           FROM product_cost_history h
           WHERE h.created_at = (
             SELECT MAX(h2.created_at) FROM product_cost_history h2
             WHERE h2.product_id = h.product_id
           )
         )
         SELECT p.product_id, p.name, lc.old_cost_minor, lc.new_cost_minor,
                pp.price_minor
         FROM latest_cost lc
         JOIN products p ON p.product_id = lc.product_id
         JOIN product_prices pp ON pp.price_id = (
           SELECT pp2.price_id FROM product_prices pp2
           WHERE pp2.product_id = p.product_id
             AND (pp2.branch_id = ? OR pp2.branch_id IS NULL)
             AND pp2.price_type = 'selling'
             AND pp2.effective_from <= datetime('now')
             AND (pp2.effective_to IS NULL OR pp2.effective_to > datetime('now'))
           ORDER BY CASE WHEN pp2.branch_id = ? THEN 0 ELSE 1 END,
                    pp2.effective_from DESC, pp2.created_at DESC
           LIMIT 1
         )
         WHERE p.is_active = 1 AND p.deleted_at IS NULL
           AND lc.old_cost_minor IS NOT NULL
           AND lc.new_cost_minor > lc.old_cost_minor
           AND NOT EXISTS (
             SELECT 1 FROM product_prices changed
             WHERE changed.product_id = p.product_id
               AND changed.created_at > lc.created_at
           )",
    )
    .bind(branch_id)
    .bind(branch_id)
    .fetch_all(pool)
    .await?;
    let mut erosion_rows = Vec::new();
    for row in rows {
        let price: i64 = row.get("price_minor");
        let old_cost: i64 = row.get("old_cost_minor");
        let new_cost: i64 = row.get("new_cost_minor");
        let Some((drop, current)) = margin_drop_basis_points(price, old_cost, new_cost) else {
            continue;
        };
        if drop < threshold_basis_points {
            continue;
        }
        erosion_rows.push(MarginErosionRow {
            product_id: row.get("product_id"),
            product_name: row.get("name"),
            selling_price_minor: price,
            old_cost_minor: old_cost,
            new_cost_minor: new_cost,
            margin_drop_basis_points: drop,
            current_margin_basis_points: current,
        });
    }
    erosion_rows.sort_by_key(|row| std::cmp::Reverse(row.margin_drop_basis_points));
    let unknown_cost_line_count = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM sale_items si JOIN sales s ON s.sale_id = si.sale_id
         WHERE s.branch_id = ? AND si.voided = 0
           AND si.cost_minor_snapshot IS NULL
           AND datetime(s.sold_at) >= datetime('now', '-30 days')",
    )
    .bind(branch_id)
    .fetch_one(pool)
    .await?;
    Ok(MarginErosionReport {
        rows: erosion_rows,
        unknown_cost_line_count,
    })
}

pub async fn seasonal_demand_plan(
    pool: &SqlitePool,
    branch_id: &str,
    from: &str,
    to: &str,
    comparison_years: i32,
) -> AppResult<Vec<SeasonalDemandRow>> {
    let from_date = NaiveDate::parse_from_str(from, "%Y-%m-%d")
        .map_err(|_| AppError::Validation("from must be YYYY-MM-DD".into()))?;
    let to_date = NaiveDate::parse_from_str(to, "%Y-%m-%d")
        .map_err(|_| AppError::Validation("to must be YYYY-MM-DD".into()))?;
    if to_date < from_date || (to_date - from_date).num_days() > 92 {
        return Err(AppError::Validation(
            "Seasonal planning period must be 1..=93 days".into(),
        ));
    }
    if !(1..=3).contains(&comparison_years) {
        return Err(AppError::Validation(
            "comparison_years must be between 1 and 3".into(),
        ));
    }

    let mut history: HashMap<String, (String, f64)> = HashMap::new();
    for years_back in 1..=comparison_years {
        let (prior_from, prior_to) = prior_year_period(from_date, to_date, years_back)?;
        let rows = sqlx::query(
            "SELECT p.product_id, p.name,
                    COALESCE(SUM(CAST(si.quantity AS REAL)), 0) AS quantity
             FROM sale_items si
             JOIN sales s ON s.sale_id = si.sale_id
             JOIN products p ON p.product_id = si.product_id
             WHERE s.branch_id = ? AND si.voided = 0
               AND date(s.business_date) BETWEEN ? AND ?
             GROUP BY p.product_id, p.name",
        )
        .bind(branch_id)
        .bind(prior_from.to_string())
        .bind(prior_to.to_string())
        .fetch_all(pool)
        .await?;
        for row in rows {
            let product_id: String = row.get("product_id");
            let product_name: String = row.get("name");
            let quantity: f64 = row.get("quantity");
            let entry = history.entry(product_id).or_insert((product_name, 0.0));
            entry.1 += quantity;
        }
    }

    let stock_rows = sqlx::query(
        "SELECT p.product_id, COALESCE(CAST(sl.quantity_on_hand AS REAL), 0) AS quantity
         FROM products p
         LEFT JOIN stock_levels sl ON sl.product_id = p.product_id AND sl.branch_id = ?
         WHERE p.is_active = 1 AND p.deleted_at IS NULL",
    )
    .bind(branch_id)
    .fetch_all(pool)
    .await?;
    let stock: HashMap<String, f64> = stock_rows
        .into_iter()
        .map(|row| (row.get("product_id"), row.get("quantity")))
        .collect();
    let mut plan: Vec<_> = history
        .into_iter()
        .map(|(product_id, (product_name, total))| {
            let average = total / f64::from(comparison_years);
            let on_hand = stock.get(&product_id).copied().unwrap_or(0.0);
            SeasonalDemandRow {
                product_id,
                product_name,
                historical_average_quantity: average,
                quantity_on_hand: on_hand,
                suggested_reorder_quantity: (average - on_hand).max(0.0).ceil(),
            }
        })
        .collect();
    plan.sort_by(|left, right| {
        right
            .suggested_reorder_quantity
            .total_cmp(&left.suggested_reorder_quantity)
    });
    Ok(plan)
}

pub async fn cash_flow_forecast(pool: &SqlitePool, branch_id: &str) -> AppResult<CashFlowForecast> {
    let delivery_cod_receivable_minor: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(amount_minor), 0)
         FROM delivery_orders
         WHERE branch_id = ? AND expected_payment_method = 'cash'
           AND payment_status = 'unpaid' AND delivery_status != 'cancelled'",
    )
    .bind(branch_id)
    .fetch_one(pool)
    .await?;
    let commitment: f64 = sqlx::query_scalar(
        "SELECT CAST(COALESCE(SUM(
           MAX(pol.ordered_qty - pol.received_qty, 0) * pol.unit_cost_minor
         ), 0) AS REAL)
         FROM purchase_order_lines pol
         JOIN purchase_orders po ON po.po_id = pol.po_id
         WHERE po.status IN ('ordered', 'partial')",
    )
    .fetch_one(pool)
    .await?;
    let unknown_cost_po_line_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM purchase_order_lines pol
         JOIN purchase_orders po ON po.po_id = pol.po_id
         WHERE po.status IN ('ordered', 'partial')
           AND pol.ordered_qty > pol.received_qty
           AND pol.unit_cost_minor <= 0",
    )
    .fetch_one(pool)
    .await?;
    let purchase_commitments_minor = commitment.round() as i64;
    Ok(CashFlowForecast {
        delivery_cod_receivable_minor,
        purchase_commitments_minor,
        net_known_position_minor: delivery_cod_receivable_minor - purchase_commitments_minor,
        unknown_cost_po_line_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        pool
    }

    #[test]
    fn margin_erosion_uses_basis_points_and_rejects_invalid_prices() {
        assert_eq!(
            margin_drop_basis_points(1_000, 400, 550),
            Some((1_500, 4_500))
        );
        assert_eq!(margin_drop_basis_points(0, 400, 550), None);
        assert_eq!(margin_drop_basis_points(1_000, 550, 400), None);
    }

    #[test]
    fn prior_year_period_uses_the_requested_period_without_a_religious_calendar() {
        let from = NaiveDate::from_ymd_opt(2026, 2, 20).unwrap();
        let to = NaiveDate::from_ymd_opt(2026, 3, 20).unwrap();

        assert_eq!(
            prior_year_period(from, to, 1).unwrap(),
            (
                NaiveDate::from_ymd_opt(2025, 2, 20).unwrap(),
                NaiveDate::from_ymd_opt(2025, 3, 20).unwrap()
            )
        );
        assert!(prior_year_period(from, to, 0).is_err());
    }

    #[tokio::test]
    async fn insight_queries_are_valid_on_an_empty_store() {
        let pool = pool().await;

        let margin = margin_erosion(&pool, "B1", 500).await.unwrap();
        let seasonal = seasonal_demand_plan(&pool, "B1", "2026-02-20", "2026-03-20", 2)
            .await
            .unwrap();

        assert!(margin.rows.is_empty());
        assert_eq!(margin.unknown_cost_line_count, 0);
        assert!(seasonal.is_empty());
    }

    #[tokio::test]
    async fn cash_flow_forecast_is_zero_on_an_empty_store() {
        let pool = pool().await;

        let forecast = cash_flow_forecast(&pool, "B1").await.unwrap();

        assert_eq!(forecast.delivery_cod_receivable_minor, 0);
        assert_eq!(forecast.purchase_commitments_minor, 0);
        assert_eq!(forecast.net_known_position_minor, 0);
        assert_eq!(forecast.unknown_cost_po_line_count, 0);
    }
}
