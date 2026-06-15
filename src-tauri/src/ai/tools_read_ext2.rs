//! Phase-3 extension read tools — analytics, supplier/PO queries, system info.

use crate::domain::money;
use crate::errors::{AppError, AppResult};
use sqlx::{Row, SqlitePool};

fn s_i64(r: &sqlx::sqlite::SqliteRow, col: &str) -> i64 {
    r.try_get::<i64, _>(col).unwrap_or(0)
}
fn s_str(r: &sqlx::sqlite::SqliteRow, col: &str) -> String {
    r.try_get::<Option<String>, _>(col)
        .ok()
        .flatten()
        .unwrap_or_default()
}

fn period_days(input: &serde_json::Value, default: i64) -> i64 {
    input
        .get("period_days")
        .or_else(|| input.get("days"))
        .and_then(|v| v.as_i64())
        .unwrap_or(default)
        .clamp(1, 1825)
}
fn limit_i(input: &serde_json::Value, default: i64) -> i64 {
    input
        .get("limit")
        .and_then(|v| v.as_i64())
        .unwrap_or(default)
        .clamp(1, 500)
}

// ── Dispatch ───────────────────────────────────────────────────────────────────

pub async fn execute(
    pool: &SqlitePool,
    tool_name: &str,
    input: &serde_json::Value,
    _branch_id: &str,
    currency_exp: u32,
) -> AppResult<String> {
    let fmt = |n: i64| money::format_minor(n, currency_exp);
    match tool_name {
        "get_dead_stock" => dead_stock(pool, input, &fmt).await,
        "get_discount_by_cashier" => discount_by_cashier(pool, input, &fmt).await,
        "get_customer_purchase_history" => customer_purchase_history(pool, input, &fmt).await,
        "get_revenue_by_payment_method" => revenue_by_payment_method(pool, input, &fmt).await,
        "get_profit_margin_report" => profit_margin(pool, input, &fmt).await,
        "get_shelf_label_gap" => shelf_label_gap(pool, &fmt).await,
        "get_product_sales_rank" => product_sales_rank(pool, input, &fmt).await,
        "get_hourly_heatmap" => hourly_heatmap(pool, input).await,
        "get_category_performance" => category_performance(pool, input, &fmt).await,
        "get_cash_discrepancy_log" => cash_discrepancy_log(pool, input, &fmt).await,
        "get_void_rate_by_cashier" => void_rate_by_cashier(pool, input).await,
        "get_peak_hours" => peak_hours(pool, input).await,
        "get_customer_visit_frequency" => customer_visit_frequency(pool, input).await,
        "get_average_basket_by_time" => average_basket_by_time(pool, input, &fmt).await,
        "get_tax_collected_report" => tax_collected_report(pool, input, &fmt).await,
        "get_unused_products" => unused_products(pool, input).await,
        "get_category_mix_analysis" => category_mix_analysis(pool, input, &fmt).await,
        "get_sales_by_device" => sales_by_device(pool, input, &fmt).await,
        "get_revenue_forecast" => revenue_forecast(pool, &fmt).await,
        "get_customer_ltv" => customer_ltv(pool, input, &fmt).await,
        "get_churn_risk" => churn_risk(pool, input).await,
        "get_day_of_week_comparison" => day_of_week_comparison(pool, input, &fmt).await,
        "get_month_over_month_growth" => month_over_month_growth(pool, input, &fmt).await,
        "get_new_vs_returning" => new_vs_returning(pool, input, &fmt).await,
        "get_void_report" => void_report(pool, input, &fmt).await,
        "get_loyalty_summary" => loyalty_summary(pool, input, &fmt).await,
        "get_customer_segments" => customer_segments(pool).await,
        "get_top_spenders" => top_spenders(pool, input, &fmt).await,
        "get_lapsed_customers" => lapsed_customers(pool, input).await,
        "get_customer_outstanding_balance" => customer_outstanding_balance(pool, &fmt).await,
        "get_active_deliveries_map" => active_deliveries_map(pool, &fmt).await,
        "get_delivery_performance" => delivery_performance(pool, input).await,
        "get_delivery_payment_outstanding" => delivery_payment_outstanding(pool, &fmt).await,
        "get_product_versions" => product_versions(pool, input, &fmt).await,
        "get_tax_filing_summary" => tax_filing_summary(pool, input, &fmt).await,
        "validate_tax_config" => validate_tax_config(pool).await,
        "get_z_report_archive" => z_report_archive(pool, input, &fmt).await,
        "get_low_stock_with_velocity" => low_stock_with_velocity(pool, input).await,
        "list_suppliers" => list_suppliers(pool, input).await,
        "list_purchase_orders" => list_purchase_orders(pool, input, &fmt).await,
        "get_database_size" => database_size().await,
        "get_table_row_counts" => table_row_counts(pool).await,
        "export_product_catalog" => export_product_catalog(pool, input, &fmt).await,
        "get_customer_notes" => customer_notes(pool, input).await,
        "get_user_permissions" => user_permissions(pool, input).await,
        "find_products_without_barcode" => products_without_barcode(pool).await,
        "search_sales_by_customer" => search_sales_by_customer(pool, input, &fmt).await,
        "get_shift_performance" => shift_performance(pool, input, &fmt).await,
        other => {
            crate::ai::tools_read_ext3::execute(pool, other, input, _branch_id, currency_exp).await
        }
    }
}

// ── Implementations ────────────────────────────────────────────────────────────

async fn dead_stock(
    pool: &SqlitePool,
    input: &serde_json::Value,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let days = period_days(input, 30);
    let lim = limit_i(input, 50);
    let rows = sqlx::query(
        "SELECT p.product_id, p.name, p.price_minor, COALESCE(p.stock_quantity, 0) AS stock
         FROM products p WHERE p.is_active = 1
         AND NOT EXISTS (
             SELECT 1 FROM sale_items si JOIN sales s ON s.sale_id = si.sale_id
             WHERE si.product_id = p.product_id AND s.sold_at >= date('now','-'||?||' days')
               AND s.status != 'voided'
         )
         ORDER BY stock DESC LIMIT ?",
    )
    .bind(days)
    .bind(lim)
    .fetch_all(pool)
    .await?;
    if rows.is_empty() {
        return Ok(format!("[DB] No dead stock found in last {days} days."));
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            format!(
                "  {} | BHD {} | stock: {}",
                s_str(r, "name"),
                fmt(s_i64(r, "price_minor")),
                s_i64(r, "stock")
            )
        })
        .collect();
    Ok(format!(
        "[DB] {} dead-stock items (no sales in {days} days):\n{}",
        rows.len(),
        lines.join("\n")
    ))
}

async fn discount_by_cashier(
    pool: &SqlitePool,
    input: &serde_json::Value,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let from = input.get("from").and_then(|v| v.as_str()).unwrap_or("2000-01-01");
    let to = input.get("to").and_then(|v| v.as_str()).unwrap_or("2999-12-31");
    let rows = sqlx::query(
        "SELECT COALESCE(u.display_name,'(deleted)') AS cashier,
                COUNT(s.sale_id) AS tx, SUM(s.discount_total_minor) AS disc
         FROM sales s LEFT JOIN users u ON u.user_id = s.cashier_user_id
         WHERE s.business_date BETWEEN ? AND ? AND s.status != 'voided'
         GROUP BY s.cashier_user_id ORDER BY disc DESC",
    )
    .bind(from).bind(to)
    .fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok("[DB] No discount data for that period.".into());
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            format!(
                "  {} | {} sales | BHD {} discount",
                s_str(r, "cashier"),
                s_i64(r, "tx"),
                fmt(s_i64(r, "disc"))
            )
        })
        .collect();
    Ok(format!(
        "[DB] Discounts by cashier ({from} → {to}):\n{}",
        lines.join("\n")
    ))
}

async fn customer_purchase_history(
    pool: &SqlitePool,
    input: &serde_json::Value,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let cid = input
        .get("customer_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("customer_id required".into()))?;
    let lim = limit_i(input, 20);
    let rows = sqlx::query(
        "SELECT receipt_number, sold_at, net_total_minor, status FROM sales
         WHERE customer_id = ? AND status != 'voided' ORDER BY sold_at DESC LIMIT ?",
    )
    .bind(cid).bind(lim)
    .fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok(format!("[DB] No sales found for customer {cid}."));
    }
    let total: i64 = rows.iter().map(|r| s_i64(r, "net_total_minor")).sum();
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            format!(
                "  #{} | {} | BHD {}",
                s_str(r, "receipt_number"),
                s_str(r, "sold_at"),
                fmt(s_i64(r, "net_total_minor"))
            )
        })
        .collect();
    Ok(format!(
        "[DB] {} sales for customer {cid} | Lifetime: BHD {}:\n{}",
        rows.len(),
        fmt(total),
        lines.join("\n")
    ))
}

async fn revenue_by_payment_method(
    pool: &SqlitePool,
    input: &serde_json::Value,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let from = input.get("from").and_then(|v| v.as_str()).unwrap_or("2000-01-01");
    let to = input.get("to").and_then(|v| v.as_str()).unwrap_or("2999-12-31");
    let rows = sqlx::query(
        "SELECT p.payment_method, SUM(p.amount_minor) AS total, COUNT(DISTINCT s.sale_id) AS tx
         FROM payments p JOIN sales s ON s.sale_id = p.sale_id
         WHERE s.business_date BETWEEN ? AND ? AND s.status != 'voided'
         GROUP BY p.payment_method ORDER BY total DESC",
    )
    .bind(from).bind(to)
    .fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok("[DB] No payment data for that period.".into());
    }
    let grand: i64 = rows.iter().map(|r| s_i64(r, "total")).sum();
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            let t = s_i64(r, "total");
            let pct = if grand > 0 { t * 100 / grand } else { 0 };
            format!(
                "  {} | BHD {} | {} tx | {}%",
                s_str(r, "payment_method"),
                fmt(t),
                s_i64(r, "tx"),
                pct
            )
        })
        .collect();
    Ok(format!(
        "[DB] Revenue by payment method ({from} → {to}) | Grand total: BHD {}:\n{}",
        fmt(grand),
        lines.join("\n")
    ))
}

async fn profit_margin(
    pool: &SqlitePool,
    input: &serde_json::Value,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let days = period_days(input, 30);
    let lim = limit_i(input, 20);
    let rows = sqlx::query(
        "SELECT p.name,
                SUM(si.quantity * si.unit_price_minor) AS revenue,
                SUM(si.quantity * COALESCE(p.cost_minor, 0)) AS cost
         FROM sale_items si
         JOIN products p ON p.product_id = si.product_id
         JOIN sales s ON s.sale_id = si.sale_id
         WHERE s.sold_at >= date('now','-'||?||' days') AND s.status != 'voided'
           AND p.cost_minor IS NOT NULL AND p.cost_minor > 0
         GROUP BY si.product_id
         ORDER BY (SUM(si.quantity * si.unit_price_minor) - SUM(si.quantity * COALESCE(p.cost_minor, 0))) DESC
         LIMIT ?",
    )
    .bind(days).bind(lim)
    .fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok("[DB] No margin data — set cost_minor on products to enable this report.".into());
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            let rev = s_i64(r, "revenue");
            let cost = s_i64(r, "cost");
            let margin = rev - cost;
            let pct = if rev > 0 { margin * 100 / rev } else { 0 };
            format!(
                "  {} | rev BHD {} | cost BHD {} | margin BHD {} ({}%)",
                s_str(r, "name"),
                fmt(rev),
                fmt(cost),
                fmt(margin),
                pct
            )
        })
        .collect();
    Ok(format!(
        "[DB] Profit margin report (last {days} days):\n{}",
        lines.join("\n")
    ))
}

async fn shelf_label_gap(pool: &SqlitePool, fmt: &impl Fn(i64) -> String) -> AppResult<String> {
    let rows = sqlx::query(
        "SELECT name, price_minor, cost_minor FROM products
         WHERE is_active = 1 AND cost_minor IS NOT NULL AND cost_minor > price_minor
         ORDER BY (cost_minor - price_minor) DESC LIMIT 50",
    )
    .fetch_all(pool)
    .await?;
    if rows.is_empty() {
        return Ok("[DB] No products found where cost > price. Margin health OK.".into());
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            let price = s_i64(r, "price_minor");
            let cost = s_i64(r, "cost_minor");
            format!(
                "  {} | price BHD {} | cost BHD {} | LOSS BHD {}",
                s_str(r, "name"),
                fmt(price),
                fmt(cost),
                fmt(cost - price)
            )
        })
        .collect();
    Ok(format!(
        "[DB] {} product(s) with cost > selling price — URGENT repricing needed:\n{}",
        rows.len(),
        lines.join("\n")
    ))
}

async fn product_sales_rank(
    pool: &SqlitePool,
    input: &serde_json::Value,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let days = period_days(input, 30);
    let lim = limit_i(input, 10);
    let sort = input
        .get("sort_by")
        .and_then(|v| v.as_str())
        .unwrap_or("revenue");
    let order_col = if sort == "units" { "units" } else { "revenue" };
    let rows = sqlx::query(
        &format!("SELECT p.name,
                CAST(SUM(si.quantity) AS INTEGER) AS units,
                SUM(si.quantity * si.unit_price_minor) AS revenue
         FROM sale_items si JOIN products p ON p.product_id = si.product_id
         JOIN sales s ON s.sale_id = si.sale_id
         WHERE s.sold_at >= date('now','-'||?||' days') AND s.status != 'voided'
         GROUP BY si.product_id ORDER BY {order_col} DESC LIMIT ?"),
    )
    .bind(days).bind(lim)
    .fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok(format!("[DB] No sales data in last {days} days."));
    }
    let lines: Vec<String> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            format!(
                "  {}. {} | {} units | BHD {}",
                i + 1,
                s_str(r, "name"),
                s_i64(r, "units"),
                fmt(s_i64(r, "revenue"))
            )
        })
        .collect();
    Ok(format!(
        "[DB] Top {} products by {} (last {days} days):\n{}",
        lim,
        sort,
        lines.join("\n")
    ))
}

async fn hourly_heatmap(pool: &SqlitePool, input: &serde_json::Value) -> AppResult<String> {
    let days = period_days(input, 28);
    let rows = sqlx::query(
        "SELECT strftime('%H', sold_at, 'localtime') AS hr,
                strftime('%w', sold_at, 'localtime') AS dow,
                COUNT(*) AS tx
         FROM sales WHERE sold_at >= date('now','-'||?||' days') AND status != 'voided'
         GROUP BY hr, dow ORDER BY hr, dow",
    )
    .bind(days)
    .fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok("[DB] No transaction data available.".into());
    }
    let days_label = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    let mut map: std::collections::HashMap<(String, String), i64> = std::collections::HashMap::new();
    let mut hours: Vec<String> = Vec::new();
    for r in &rows {
        let hr = s_str(r, "hr");
        let dow = s_str(r, "dow");
        if !hours.contains(&hr) {
            hours.push(hr.clone());
        }
        map.insert((hr, dow), s_i64(r, "tx"));
    }
    hours.sort();
    let header = format!("Hr   | {}", days_label.join(" | "));
    let lines: Vec<String> = hours
        .iter()
        .map(|hr| {
            let cols: Vec<String> = (0..7)
                .map(|d| {
                    let cnt = map.get(&(hr.clone(), d.to_string())).copied().unwrap_or(0);
                    format!("{:>3}", cnt)
                })
                .collect();
            format!("  {}h | {}", hr, cols.join(" | "))
        })
        .collect();
    Ok(format!(
        "[DB] Hourly transaction heatmap (last {days} days):\n  {header}\n{}",
        lines.join("\n")
    ))
}

async fn category_performance(
    pool: &SqlitePool,
    input: &serde_json::Value,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let from = input.get("from").and_then(|v| v.as_str()).unwrap_or("2000-01-01");
    let to = input.get("to").and_then(|v| v.as_str()).unwrap_or("2999-12-31");
    let rows = sqlx::query(
        "SELECT COALESCE(c.name,'(uncategorised)') AS cat,
                SUM(si.quantity * si.unit_price_minor) AS revenue,
                COUNT(DISTINCT s.sale_id) AS tx
         FROM sale_items si
         JOIN products p ON p.product_id = si.product_id
         LEFT JOIN categories c ON c.category_id = p.category_id
         JOIN sales s ON s.sale_id = si.sale_id
         WHERE s.business_date BETWEEN ? AND ? AND s.status != 'voided'
         GROUP BY c.category_id ORDER BY revenue DESC",
    )
    .bind(from).bind(to)
    .fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok("[DB] No sales data for that period.".into());
    }
    let grand: i64 = rows.iter().map(|r| s_i64(r, "revenue")).sum();
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            let rev = s_i64(r, "revenue");
            let pct = if grand > 0 { rev * 100 / grand } else { 0 };
            format!(
                "  {} | BHD {} | {} tx | {}%",
                s_str(r, "cat"),
                fmt(rev),
                s_i64(r, "tx"),
                pct
            )
        })
        .collect();
    Ok(format!(
        "[DB] Category performance ({from} → {to}) | Total: BHD {}:\n{}",
        fmt(grand),
        lines.join("\n")
    ))
}

async fn cash_discrepancy_log(
    pool: &SqlitePool,
    input: &serde_json::Value,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let min_gap = input
        .get("min_gap_minor")
        .and_then(|v| v.as_i64())
        .unwrap_or(500);
    let lim = limit_i(input, 20);
    let rows = sqlx::query(
        "SELECT shift_id, opening_cash_minor, counted_cash_minor, opened_at
         FROM shifts
         WHERE counted_cash_minor IS NOT NULL
           AND ABS(counted_cash_minor - opening_cash_minor) >= ?
         ORDER BY ABS(counted_cash_minor - opening_cash_minor) DESC LIMIT ?",
    )
    .bind(min_gap).bind(lim)
    .fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok(format!(
            "[DB] No shifts with cash discrepancy > BHD {}.",
            fmt(min_gap)
        ));
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            let opening = s_i64(r, "opening_cash_minor");
            let counted = s_i64(r, "counted_cash_minor");
            let gap = counted - opening;
            format!(
                "  {} | opening BHD {} | counted BHD {} | gap BHD {} | {}",
                s_str(r, "shift_id"),
                fmt(opening),
                fmt(counted),
                fmt(gap.abs()),
                if gap < 0 { "SHORT" } else { "OVER" }
            )
        })
        .collect();
    Ok(format!(
        "[DB] {} shift(s) with discrepancy > BHD {}:\n{}",
        rows.len(),
        fmt(min_gap),
        lines.join("\n")
    ))
}

async fn void_rate_by_cashier(
    pool: &SqlitePool,
    input: &serde_json::Value,
) -> AppResult<String> {
    let from = input.get("from").and_then(|v| v.as_str()).unwrap_or("2000-01-01");
    let to = input.get("to").and_then(|v| v.as_str()).unwrap_or("2999-12-31");
    let rows = sqlx::query(
        "SELECT COALESCE(u.display_name,'(deleted)') AS cashier,
                COUNT(*) AS total,
                SUM(CASE WHEN s.status='voided' THEN 1 ELSE 0 END) AS voided
         FROM sales s LEFT JOIN users u ON u.user_id = s.cashier_user_id
         WHERE s.business_date BETWEEN ? AND ?
         GROUP BY s.cashier_user_id ORDER BY voided DESC",
    )
    .bind(from).bind(to)
    .fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok("[DB] No sales data for that period.".into());
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            let total = s_i64(r, "total");
            let voided = s_i64(r, "voided");
            let rate = if total > 0 { voided * 100 / total } else { 0 };
            format!(
                "  {} | {} total | {} voided | {}%",
                s_str(r, "cashier"),
                total,
                voided,
                rate
            )
        })
        .collect();
    Ok(format!(
        "[DB] Void rate by cashier ({from} → {to}):\n{}",
        lines.join("\n")
    ))
}

async fn peak_hours(pool: &SqlitePool, input: &serde_json::Value) -> AppResult<String> {
    let days = period_days(input, 28);
    let rows = sqlx::query(
        "SELECT strftime('%H', sold_at, 'localtime') AS hr, COUNT(*) AS tx
         FROM sales WHERE sold_at >= date('now','-'||?||' days') AND status != 'voided'
         GROUP BY hr ORDER BY tx DESC LIMIT 5",
    )
    .bind(days)
    .fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok("[DB] No transaction data.".into());
    }
    let lines: Vec<String> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            format!(
                "  {}. {}:00–{}:59 | {} transactions",
                i + 1,
                s_str(r, "hr"),
                s_str(r, "hr"),
                s_i64(r, "tx")
            )
        })
        .collect();
    Ok(format!(
        "[DB] Top 5 peak hours (last {days} days):\n{}",
        lines.join("\n")
    ))
}

async fn customer_visit_frequency(
    pool: &SqlitePool,
    input: &serde_json::Value,
) -> AppResult<String> {
    let min_visits = input
        .get("min_visits")
        .and_then(|v| v.as_i64())
        .unwrap_or(2);
    let lim = limit_i(input, 20);
    let rows = sqlx::query(
        "SELECT COALESCE(c.name, s.customer_id) AS cust,
                COUNT(DISTINCT s.sale_id) AS visits,
                MIN(s.sold_at) AS first_visit,
                MAX(s.sold_at) AS last_visit,
                julianday(MAX(s.sold_at)) - julianday(MIN(s.sold_at)) AS span_days
         FROM sales s LEFT JOIN customers c ON c.customer_id = s.customer_id
         WHERE s.customer_id IS NOT NULL AND s.status != 'voided'
         GROUP BY s.customer_id HAVING visits >= ?
         ORDER BY (julianday(MAX(s.sold_at)) - julianday(MIN(s.sold_at))) / MAX(visits-1,1) DESC
         LIMIT ?",
    )
    .bind(min_visits).bind(lim)
    .fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok("[DB] No customers with multiple visits yet.".into());
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            let visits = s_i64(r, "visits");
            let span: f64 = r.try_get::<f64, _>("span_days").unwrap_or(0.0);
            let avg_days = if visits > 1 {
                span / (visits - 1) as f64
            } else {
                0.0
            };
            format!(
                "  {} | {} visits | avg {:.1} days between visits",
                s_str(r, "cust"),
                visits,
                avg_days
            )
        })
        .collect();
    Ok(format!(
        "[DB] Customer visit frequency (min {min_visits} visits):\n{}",
        lines.join("\n")
    ))
}

async fn average_basket_by_time(
    pool: &SqlitePool,
    input: &serde_json::Value,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let days = period_days(input, 28);
    let rows = sqlx::query(
        "SELECT strftime('%w', sold_at, 'localtime') AS dow,
                AVG(net_total_minor) AS avg_basket, COUNT(*) AS tx
         FROM sales WHERE sold_at >= date('now','-'||?||' days') AND status != 'voided'
         GROUP BY dow ORDER BY dow",
    )
    .bind(days)
    .fetch_all(pool).await?;
    let days_label = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    if rows.is_empty() {
        return Ok("[DB] No data.".into());
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            let dow: usize = s_str(r, "dow").parse().unwrap_or(0);
            let avg: i64 = r.try_get::<f64, _>("avg_basket").unwrap_or(0.0) as i64;
            format!(
                "  {} | avg BHD {} | {} tx",
                days_label.get(dow).unwrap_or(&"?"),
                fmt(avg),
                s_i64(r, "tx")
            )
        })
        .collect();
    Ok(format!(
        "[DB] Avg basket by day of week (last {days} days):\n{}",
        lines.join("\n")
    ))
}

async fn tax_collected_report(
    pool: &SqlitePool,
    input: &serde_json::Value,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let from = input.get("from").and_then(|v| v.as_str()).unwrap_or("2000-01-01");
    let to = input.get("to").and_then(|v| v.as_str()).unwrap_or("2999-12-31");
    let rows = sqlx::query(
        "SELECT COALESCE(tr.name,'No tax rule') AS rule,
                COALESCE(tr.rate_pct,0) AS rate,
                SUM(s.tax_total_minor) AS tax_collected,
                COUNT(DISTINCT s.sale_id) AS tx
         FROM sales s
         LEFT JOIN sale_items si ON si.sale_id = s.sale_id
         LEFT JOIN products p ON p.product_id = si.product_id
         LEFT JOIN tax_rules tr ON tr.tax_rule_id = p.tax_rule_id
         WHERE s.business_date BETWEEN ? AND ? AND s.status != 'voided'
         GROUP BY tr.tax_rule_id ORDER BY tax_collected DESC",
    )
    .bind(from).bind(to)
    .fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok("[DB] No tax data for that period.".into());
    }
    let total: i64 = rows.iter().map(|r| s_i64(r, "tax_collected")).sum();
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            format!(
                "  {} ({}%) | BHD {} tax | {} tx",
                s_str(r, "rule"),
                s_i64(r, "rate"),
                fmt(s_i64(r, "tax_collected")),
                s_i64(r, "tx")
            )
        })
        .collect();
    Ok(format!(
        "[DB] Tax collected ({from} → {to}) | Total: BHD {}:\n{}",
        fmt(total),
        lines.join("\n")
    ))
}

async fn unused_products(pool: &SqlitePool, input: &serde_json::Value) -> AppResult<String> {
    let days = period_days(input, 60);
    let rows = sqlx::query(
        "SELECT p.product_id, p.name, COALESCE(p.stock_quantity,0) AS stock
         FROM products p WHERE p.is_active = 1
           AND COALESCE(p.stock_quantity,0) = 0
           AND NOT EXISTS (
               SELECT 1 FROM sale_items si JOIN sales s ON s.sale_id = si.sale_id
               WHERE si.product_id = p.product_id AND s.sold_at >= date('now','-'||?||' days')
                 AND s.status != 'voided'
           )
         ORDER BY p.name LIMIT 50",
    )
    .bind(days)
    .fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok(format!(
            "[DB] No unused products found (no sales AND zero stock in {days} days)."
        ));
    }
    let names: Vec<String> = rows.iter().map(|r| s_str(r, "name")).collect();
    Ok(format!(
        "[DB] {} unused product(s) — no sales in {days} days, zero stock:\n  {}",
        rows.len(),
        names.join("\n  ")
    ))
}

async fn category_mix_analysis(
    pool: &SqlitePool,
    input: &serde_json::Value,
    _fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let months = input.get("months").and_then(|v| v.as_i64()).unwrap_or(2).clamp(1, 24);
    let rows = sqlx::query(
        "SELECT COALESCE(c.name,'(uncategorised)') AS cat,
                strftime('%Y-%m', s.sold_at, 'localtime') AS mo,
                SUM(si.quantity * si.unit_price_minor) AS revenue
         FROM sale_items si
         JOIN products p ON p.product_id = si.product_id
         LEFT JOIN categories c ON c.category_id = p.category_id
         JOIN sales s ON s.sale_id = si.sale_id
         WHERE s.sold_at >= date('now','-'||?||' months') AND s.status != 'voided'
         GROUP BY c.category_id, mo ORDER BY mo DESC, revenue DESC",
    )
    .bind(months)
    .fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok("[DB] No category mix data.".into());
    }
    let mut out: Vec<String> = Vec::new();
    let mut cur_mo = String::new();
    let mut mo_total: i64 = 0;
    let mut mo_rows: Vec<(String, i64)> = Vec::new();
    let flush = |mo: &str, total: i64, rows: &[(String, i64)]| -> Vec<String> {
        let mut v = vec![format!("  [{mo}] total BHD {}", money::format_minor(total, 3))];
        for (cat, rev) in rows {
            let pct = if total > 0 { rev * 100 / total } else { 0 };
            v.push(format!("    {} {}%", cat, pct));
        }
        v
    };
    for r in &rows {
        let mo = s_str(r, "mo");
        let cat = s_str(r, "cat");
        let rev = s_i64(r, "revenue");
        if mo != cur_mo {
            if !cur_mo.is_empty() {
                out.extend(flush(&cur_mo, mo_total, &mo_rows));
            }
            cur_mo = mo;
            mo_total = 0;
            mo_rows.clear();
        }
        mo_total += rev;
        mo_rows.push((cat, rev));
    }
    if !cur_mo.is_empty() {
        out.extend(flush(&cur_mo, mo_total, &mo_rows));
    }
    Ok(format!("[DB] Category mix last {months} month(s):\n{}", out.join("\n")))
}

async fn sales_by_device(
    pool: &SqlitePool,
    input: &serde_json::Value,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let from = input.get("from").and_then(|v| v.as_str()).unwrap_or("2000-01-01");
    let to = input.get("to").and_then(|v| v.as_str()).unwrap_or("2999-12-31");
    let rows = sqlx::query(
        "SELECT COALESCE(d.name, s.device_id, '(unknown)') AS device,
                COUNT(DISTINCT s.sale_id) AS tx,
                SUM(s.net_total_minor) AS revenue
         FROM sales s LEFT JOIN devices d ON d.device_id = s.device_id
         WHERE s.business_date BETWEEN ? AND ? AND s.status != 'voided'
         GROUP BY s.device_id ORDER BY revenue DESC",
    )
    .bind(from).bind(to)
    .fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok("[DB] No device sales data for that period.".into());
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            format!(
                "  {} | {} tx | BHD {}",
                s_str(r, "device"),
                s_i64(r, "tx"),
                fmt(s_i64(r, "revenue"))
            )
        })
        .collect();
    Ok(format!(
        "[DB] Sales by device ({from} → {to}):\n{}",
        lines.join("\n")
    ))
}

async fn revenue_forecast(pool: &SqlitePool, fmt: &impl Fn(i64) -> String) -> AppResult<String> {
    let daily_avg: i64 = sqlx::query_scalar(
        "SELECT COALESCE(AVG(daily_rev),0) FROM (
             SELECT business_date, SUM(net_total_minor) AS daily_rev FROM sales
             WHERE business_date >= date('now','-90 days') AND status != 'voided'
             GROUP BY business_date
         )",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0.0) as i64;

    let (best_dow, worst_dow): (String, String) = {
        let rows = sqlx::query(
            "SELECT strftime('%w', sold_at, 'localtime') AS dow, AVG(net_total_minor) AS avg
             FROM sales WHERE sold_at >= date('now','-90 days') AND status != 'voided'
             GROUP BY dow ORDER BY avg DESC",
        )
        .fetch_all(pool)
        .await
        .unwrap_or_default();
        let days = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
        let best = rows.first().map(|r| {
            let d: usize = s_str(r, "dow").parse().unwrap_or(0);
            days.get(d).unwrap_or(&"?").to_string()
        }).unwrap_or("?".into());
        let worst = rows.last().map(|r| {
            let d: usize = s_str(r, "dow").parse().unwrap_or(0);
            days.get(d).unwrap_or(&"?").to_string()
        }).unwrap_or("?".into());
        (best, worst)
    };

    Ok(format!(
        "[DB] Revenue forecast (based on 90-day trailing avg):\n  Daily avg: BHD {}\n  7-day projection: BHD {}\n  30-day projection: BHD {}\n  Best day: {best_dow} | Worst day: {worst_dow}",
        fmt(daily_avg),
        fmt(daily_avg * 7),
        fmt(daily_avg * 30)
    ))
}

async fn customer_ltv(
    pool: &SqlitePool,
    input: &serde_json::Value,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let lim = limit_i(input, 20);
    let rows = sqlx::query(
        "SELECT COALESCE(c.name, s.customer_id) AS cust,
                COUNT(DISTINCT s.sale_id) AS visits,
                SUM(s.net_total_minor) AS ltv,
                MIN(s.sold_at) AS first_visit,
                MAX(s.sold_at) AS last_visit
         FROM sales s LEFT JOIN customers c ON c.customer_id = s.customer_id
         WHERE s.customer_id IS NOT NULL AND s.status != 'voided'
         GROUP BY s.customer_id ORDER BY ltv DESC LIMIT ?",
    )
    .bind(lim)
    .fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok("[DB] No customer purchase data.".into());
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            let ltv = s_i64(r, "ltv");
            let visits = s_i64(r, "visits");
            let avg = if visits > 0 { ltv / visits } else { 0 };
            format!(
                "  {} | BHD {} lifetime | {} visits | avg BHD {} | last: {}",
                s_str(r, "cust"),
                fmt(ltv),
                visits,
                fmt(avg),
                s_str(r, "last_visit")
            )
        })
        .collect();
    Ok(format!(
        "[DB] Top {} customers by lifetime value:\n{}",
        lim,
        lines.join("\n")
    ))
}

async fn churn_risk(pool: &SqlitePool, input: &serde_json::Value) -> AppResult<String> {
    let days = input
        .get("days_since_last_visit")
        .and_then(|v| v.as_i64())
        .unwrap_or(30);
    let lim = limit_i(input, 20);
    let rows = sqlx::query(
        "SELECT COALESCE(c.name, s.customer_id) AS cust,
                MAX(s.sold_at) AS last_visit,
                COUNT(DISTINCT s.sale_id) AS visits,
                julianday('now') - julianday(MAX(s.sold_at)) AS days_absent
         FROM sales s LEFT JOIN customers c ON c.customer_id = s.customer_id
         WHERE s.customer_id IS NOT NULL AND s.status != 'voided'
         GROUP BY s.customer_id
         HAVING days_absent >= ?
         ORDER BY days_absent DESC LIMIT ?",
    )
    .bind(days).bind(lim)
    .fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok(format!(
            "[DB] No customers absent > {days} days. Retention looks strong."
        ));
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            let absent: i64 = r.try_get::<f64, _>("days_absent").unwrap_or(0.0) as i64;
            format!(
                "  {} | last visit: {} | {absent} days absent | {} visits",
                s_str(r, "cust"),
                s_str(r, "last_visit"),
                s_i64(r, "visits")
            )
        })
        .collect();
    Ok(format!(
        "[DB] {} at-risk customer(s) (absent >{days} days):\n{}",
        rows.len(),
        lines.join("\n")
    ))
}

async fn day_of_week_comparison(
    pool: &SqlitePool,
    input: &serde_json::Value,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let days = period_days(input, 28);
    let rows = sqlx::query(
        "SELECT strftime('%w', sold_at, 'localtime') AS dow,
                COUNT(*) AS tx, SUM(net_total_minor) AS revenue
         FROM sales WHERE sold_at >= date('now','-'||?||' days') AND status != 'voided'
         GROUP BY dow ORDER BY dow",
    )
    .bind(days)
    .fetch_all(pool).await?;
    let days_label = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    if rows.is_empty() {
        return Ok("[DB] No data.".into());
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            let dow: usize = s_str(r, "dow").parse().unwrap_or(0);
            format!(
                "  {} | {} tx | BHD {}",
                days_label.get(dow).unwrap_or(&"?"),
                s_i64(r, "tx"),
                fmt(s_i64(r, "revenue"))
            )
        })
        .collect();
    Ok(format!(
        "[DB] Revenue by day of week (last {days} days):\n{}",
        lines.join("\n")
    ))
}

async fn month_over_month_growth(
    pool: &SqlitePool,
    input: &serde_json::Value,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let months = input.get("months").and_then(|v| v.as_i64()).unwrap_or(6).clamp(2, 24);
    let rows = sqlx::query(
        "SELECT strftime('%Y-%m', sold_at, 'localtime') AS mo,
                SUM(net_total_minor) AS revenue, COUNT(*) AS tx
         FROM sales WHERE sold_at >= date('now','-'||?||' months') AND status != 'voided'
         GROUP BY mo ORDER BY mo",
    )
    .bind(months)
    .fetch_all(pool).await?;
    if rows.len() < 2 {
        return Ok("[DB] Need at least 2 months of data.".into());
    }
    let mut lines = Vec::new();
    let mut prev_rev: Option<i64> = None;
    for r in &rows {
        let rev = s_i64(r, "revenue");
        let growth = prev_rev.map(|p| if p > 0 { (rev - p) * 100 / p } else { 0 });
        lines.push(format!(
            "  {} | BHD {} | {} tx{}",
            s_str(r, "mo"),
            fmt(rev),
            s_i64(r, "tx"),
            growth.map(|g| format!(" | {}{g}% vs prev", if g >= 0 { "+" } else { "" })).unwrap_or_default()
        ));
        prev_rev = Some(rev);
    }
    Ok(format!(
        "[DB] Month-over-month revenue (last {months} months):\n{}",
        lines.join("\n")
    ))
}

async fn new_vs_returning(
    pool: &SqlitePool,
    input: &serde_json::Value,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let from = input.get("from").and_then(|v| v.as_str()).unwrap_or("2000-01-01");
    let to = input.get("to").and_then(|v| v.as_str()).unwrap_or("2999-12-31");
    let (new_rev, new_tx): (i64, i64) = sqlx::query_as(
        "SELECT COALESCE(SUM(net_total_minor),0), COUNT(*) FROM sales
         WHERE customer_id IS NOT NULL AND business_date BETWEEN ? AND ? AND status != 'voided'
         AND sold_at = (SELECT MIN(s2.sold_at) FROM sales s2 WHERE s2.customer_id = sales.customer_id)",
    )
    .bind(from).bind(to)
    .fetch_one(pool).await.unwrap_or((0, 0));
    let (total_rev, total_tx): (i64, i64) = sqlx::query_as(
        "SELECT COALESCE(SUM(net_total_minor),0), COUNT(*) FROM sales
         WHERE business_date BETWEEN ? AND ? AND status != 'voided'",
    )
    .bind(from).bind(to)
    .fetch_one(pool).await.unwrap_or((0, 0));
    let ret_rev = total_rev - new_rev;
    let ret_tx = total_tx - new_tx;
    Ok(format!(
        "[DB] New vs returning ({from} → {to}):\n  New customers: {} tx | BHD {}\n  Returning: {} tx | BHD {}\n  Total: {} tx | BHD {}",
        new_tx, fmt(new_rev),
        ret_tx, fmt(ret_rev),
        total_tx, fmt(total_rev)
    ))
}

async fn void_report(
    pool: &SqlitePool,
    input: &serde_json::Value,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let from = input.get("from").and_then(|v| v.as_str()).unwrap_or("2000-01-01");
    let to = input.get("to").and_then(|v| v.as_str()).unwrap_or("2999-12-31");
    let rows = sqlx::query(
        "SELECT s.receipt_number, s.sold_at, s.net_total_minor,
                COALESCE(u.display_name,'(deleted)') AS cashier
         FROM sales s LEFT JOIN users u ON u.user_id = s.cashier_user_id
         WHERE s.business_date BETWEEN ? AND ? AND s.status = 'voided'
         ORDER BY s.sold_at DESC LIMIT 100",
    )
    .bind(from).bind(to)
    .fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok(format!("[DB] No voided sales in {from} → {to}."));
    }
    let total: i64 = rows.iter().map(|r| s_i64(r, "net_total_minor")).sum();
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            format!(
                "  #{} | {} | {} | BHD {}",
                s_str(r, "receipt_number"),
                s_str(r, "sold_at"),
                s_str(r, "cashier"),
                fmt(s_i64(r, "net_total_minor"))
            )
        })
        .collect();
    Ok(format!(
        "[DB] {} voided sale(s) ({from} → {to}) | Total: BHD {}:\n{}",
        rows.len(),
        fmt(total),
        lines.join("\n")
    ))
}

async fn loyalty_summary(
    pool: &SqlitePool,
    input: &serde_json::Value,
    _fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let lim = limit_i(input, 10);
    let total_points: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(points_balance),0) FROM customers WHERE points_balance > 0",
    )
    .fetch_one(pool).await.unwrap_or(0);
    let customer_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM customers WHERE points_balance > 0",
    )
    .fetch_one(pool).await.unwrap_or(0);
    let rows = sqlx::query(
        "SELECT name, points_balance FROM customers WHERE points_balance > 0
         ORDER BY points_balance DESC LIMIT ?",
    )
    .bind(lim)
    .fetch_all(pool).await?;
    let rate_row: Option<String> = sqlx::query_scalar(
        "SELECT value FROM app_config WHERE key = 'loyalty_points_per_bhd'",
    )
    .fetch_optional(pool).await.ok().flatten();
    let rate = rate_row.as_deref().unwrap_or("—");
    let top: Vec<String> = rows
        .iter()
        .map(|r| {
            format!(
                "  {} | {} pts",
                s_str(r, "name"),
                s_i64(r, "points_balance")
            )
        })
        .collect();
    Ok(format!(
        "[DB] Loyalty summary:\n  Total outstanding points: {} (across {} customers)\n  Points per BHD: {}\n  Top {} customers:\n{}",
        total_points,
        customer_count,
        rate,
        lim,
        top.join("\n")
    ))
}

async fn customer_segments(pool: &SqlitePool) -> AppResult<String> {
    let one_time: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM (
             SELECT customer_id FROM sales WHERE customer_id IS NOT NULL AND status != 'voided'
             GROUP BY customer_id HAVING COUNT(*) = 1
         )",
    )
    .fetch_one(pool).await.unwrap_or(0);
    let regular: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM (
             SELECT customer_id FROM sales WHERE customer_id IS NOT NULL AND status != 'voided'
             GROUP BY customer_id HAVING COUNT(*) BETWEEN 2 AND 9
         )",
    )
    .fetch_one(pool).await.unwrap_or(0);
    let loyal: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM (
             SELECT customer_id FROM sales WHERE customer_id IS NOT NULL AND status != 'voided'
             GROUP BY customer_id HAVING COUNT(*) >= 10
         )",
    )
    .fetch_one(pool).await.unwrap_or(0);
    let total = one_time + regular + loyal;
    Ok(format!(
        "[DB] Customer segments (by visit count):\n  One-time (1 visit): {one_time} | {:.0}%\n  Regular (2–9 visits): {regular} | {:.0}%\n  Loyal (10+ visits): {loyal} | {:.0}%\n  Total with purchases: {total}",
        if total > 0 { one_time as f64 * 100.0 / total as f64 } else { 0.0 },
        if total > 0 { regular as f64 * 100.0 / total as f64 } else { 0.0 },
        if total > 0 { loyal as f64 * 100.0 / total as f64 } else { 0.0 },
    ))
}

async fn top_spenders(
    pool: &SqlitePool,
    input: &serde_json::Value,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let lim = limit_i(input, 10);
    let rows = sqlx::query(
        "SELECT COALESCE(c.name, s.customer_id) AS cust,
                SUM(s.net_total_minor) AS ltv, COUNT(DISTINCT s.sale_id) AS visits
         FROM sales s LEFT JOIN customers c ON c.customer_id = s.customer_id
         WHERE s.customer_id IS NOT NULL AND s.status != 'voided'
         GROUP BY s.customer_id ORDER BY ltv DESC LIMIT ?",
    )
    .bind(lim)
    .fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok("[DB] No customer purchase data yet.".into());
    }
    let lines: Vec<String> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            format!(
                "  {}. {} | BHD {} | {} visits",
                i + 1,
                s_str(r, "cust"),
                fmt(s_i64(r, "ltv")),
                s_i64(r, "visits")
            )
        })
        .collect();
    Ok(format!(
        "[DB] Top {} spenders:\n{}",
        lim,
        lines.join("\n")
    ))
}

async fn lapsed_customers(
    pool: &SqlitePool,
    input: &serde_json::Value,
) -> AppResult<String> {
    let days = period_days(input, 30);
    let lim = limit_i(input, 20);
    let rows = sqlx::query(
        "SELECT COALESCE(c.name, s.customer_id) AS cust,
                MAX(s.sold_at) AS last_visit,
                julianday('now') - julianday(MAX(s.sold_at)) AS absent
         FROM sales s LEFT JOIN customers c ON c.customer_id = s.customer_id
         WHERE s.customer_id IS NOT NULL AND s.status != 'voided'
         GROUP BY s.customer_id HAVING absent > ?
         ORDER BY absent DESC LIMIT ?",
    )
    .bind(days).bind(lim)
    .fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok(format!(
            "[DB] No lapsed customers (none absent > {days} days)."
        ));
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            let absent: i64 = r.try_get::<f64, _>("absent").unwrap_or(0.0) as i64;
            format!(
                "  {} | last: {} | {absent} days absent",
                s_str(r, "cust"),
                s_str(r, "last_visit")
            )
        })
        .collect();
    Ok(format!(
        "[DB] {} lapsed customer(s) (not visited in >{days} days):\n{}",
        rows.len(),
        lines.join("\n")
    ))
}

async fn customer_outstanding_balance(
    pool: &SqlitePool,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let rows = sqlx::query(
        "SELECT COALESCE(c.name, d.contact_number) AS cust,
                COUNT(*) AS orders, SUM(d.amount_minor) AS outstanding
         FROM delivery_orders d
         LEFT JOIN customers c ON c.customer_id = d.customer_id
         WHERE d.payment_status = 'unpaid' AND d.delivery_status = 'delivered'
         GROUP BY d.customer_id, d.contact_number
         ORDER BY outstanding DESC LIMIT 50",
    )
    .fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok("[DB] No outstanding delivery balances.".into());
    }
    let total: i64 = rows.iter().map(|r| s_i64(r, "outstanding")).sum();
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            format!(
                "  {} | {} order(s) | BHD {}",
                s_str(r, "cust"),
                s_i64(r, "orders"),
                fmt(s_i64(r, "outstanding"))
            )
        })
        .collect();
    Ok(format!(
        "[DB] Outstanding delivery balances | Total: BHD {}:\n{}",
        fmt(total),
        lines.join("\n")
    ))
}

async fn active_deliveries_map(
    pool: &SqlitePool,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let rows = sqlx::query(
        "SELECT d.delivery_id, d.receipt_number, d.delivery_status,
                d.delivery_staff_name, d.contact_number, d.address_text,
                d.amount_minor, d.payment_status
         FROM delivery_orders d
         WHERE d.delivery_status IN ('pending','dispatched')
         ORDER BY d.created_at DESC LIMIT 100",
    )
    .fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok("[DB] No active deliveries in progress.".into());
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            format!(
                "  #{} | {} | {} | Rider: {} | Phone: {} | {} | BHD {}",
                s_str(r, "receipt_number"),
                s_str(r, "delivery_status"),
                s_str(r, "address_text"),
                s_str(r, "delivery_staff_name"),
                s_str(r, "contact_number"),
                s_str(r, "payment_status"),
                fmt(s_i64(r, "amount_minor"))
            )
        })
        .collect();
    Ok(format!(
        "[DB] {} active delivery/deliveries:\n{}",
        rows.len(),
        lines.join("\n")
    ))
}

async fn delivery_performance(
    pool: &SqlitePool,
    input: &serde_json::Value,
) -> AppResult<String> {
    let from = input.get("from").and_then(|v| v.as_str()).unwrap_or("2000-01-01");
    let to = input.get("to").and_then(|v| v.as_str()).unwrap_or("2999-12-31");
    let rows = sqlx::query(
        "SELECT delivery_staff_name,
                COUNT(*) AS deliveries,
                AVG(julianday(updated_at) - julianday(created_at)) * 24 * 60 AS avg_mins
         FROM delivery_orders
         WHERE delivery_status = 'delivered'
           AND date(created_at) BETWEEN ? AND ?
         GROUP BY delivery_staff_name ORDER BY avg_mins ASC",
    )
    .bind(from).bind(to)
    .fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok("[DB] No completed deliveries for that period.".into());
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            let avg: f64 = r.try_get::<f64, _>("avg_mins").unwrap_or(0.0);
            format!(
                "  {} | {} deliveries | avg {:.0} min",
                s_str(r, "delivery_staff_name"),
                s_i64(r, "deliveries"),
                avg
            )
        })
        .collect();
    Ok(format!(
        "[DB] Delivery performance ({from} → {to}):\n{}",
        lines.join("\n")
    ))
}

async fn delivery_payment_outstanding(
    pool: &SqlitePool,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let rows = sqlx::query(
        "SELECT receipt_number, contact_number, delivery_staff_name,
                amount_minor, created_at FROM delivery_orders
         WHERE delivery_status = 'delivered' AND payment_status = 'unpaid'
         ORDER BY created_at DESC LIMIT 100",
    )
    .fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok("[DB] No outstanding delivery payments.".into());
    }
    let total: i64 = rows.iter().map(|r| s_i64(r, "amount_minor")).sum();
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            format!(
                "  #{} | {} | Rider: {} | BHD {} | Created: {}",
                s_str(r, "receipt_number"),
                s_str(r, "contact_number"),
                s_str(r, "delivery_staff_name"),
                fmt(s_i64(r, "amount_minor")),
                s_str(r, "created_at")
            )
        })
        .collect();
    Ok(format!(
        "[DB] {} unpaid delivery orders | Total: BHD {}:\n{}",
        rows.len(),
        fmt(total),
        lines.join("\n")
    ))
}

async fn product_versions(
    pool: &SqlitePool,
    input: &serde_json::Value,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let pid = input
        .get("product_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("product_id required".into()))?;
    let name: Option<String> =
        sqlx::query_scalar("SELECT name FROM products WHERE product_id = ?")
            .bind(pid)
            .fetch_optional(pool)
            .await?
            .flatten();
    let rows = sqlx::query(
        "SELECT after_json, created_at FROM audit_logs
         WHERE entity_id = ? AND event_type LIKE '%price%'
         ORDER BY created_at DESC LIMIT 20",
    )
    .bind(pid)
    .fetch_all(pool).await?;
    let current: i64 =
        sqlx::query_scalar("SELECT price_minor FROM products WHERE product_id = ?")
            .bind(pid)
            .fetch_optional(pool)
            .await?
            .flatten()
            .unwrap_or(0);
    let header = format!(
        "[DB] Price history for {} ({}):\n  Current price: BHD {}",
        name.as_deref().unwrap_or(pid),
        pid,
        fmt(current)
    );
    if rows.is_empty() {
        return Ok(format!("{header}\n  (No audit history found)"));
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|r| format!("  {} | {}", s_str(r, "created_at"), s_str(r, "after_json")))
        .collect();
    Ok(format!("{header}\n{}", lines.join("\n")))
}

async fn tax_filing_summary(
    pool: &SqlitePool,
    input: &serde_json::Value,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let from = input.get("from").and_then(|v| v.as_str()).ok_or_else(|| AppError::Validation("from required".into()))?;
    let to = input.get("to").and_then(|v| v.as_str()).ok_or_else(|| AppError::Validation("to required".into()))?;
    let total_rev: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(net_total_minor),0) FROM sales
         WHERE business_date BETWEEN ? AND ? AND status != 'voided'",
    )
    .bind(from).bind(to).fetch_one(pool).await.unwrap_or(0);
    let total_tax: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(tax_total_minor),0) FROM sales
         WHERE business_date BETWEEN ? AND ? AND status != 'voided'",
    )
    .bind(from).bind(to).fetch_one(pool).await.unwrap_or(0);
    let taxable_rev: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(s.net_total_minor),0) FROM sales s
         WHERE s.business_date BETWEEN ? AND ? AND s.status != 'voided' AND s.tax_total_minor > 0",
    )
    .bind(from).bind(to).fetch_one(pool).await.unwrap_or(0);
    let exempt_rev = total_rev - taxable_rev;
    Ok(format!(
        "[DB] Tax filing summary ({from} → {to}):\n  Total revenue: BHD {}\n  Taxable revenue: BHD {}\n  Exempt revenue: BHD {}\n  VAT collected: BHD {}\n  Net (before tax): BHD {}",
        fmt(total_rev),
        fmt(taxable_rev),
        fmt(exempt_rev),
        fmt(total_tax),
        fmt(total_rev - total_tax)
    ))
}

async fn validate_tax_config(pool: &SqlitePool) -> AppResult<String> {
    let rows = sqlx::query(
        "SELECT name, product_id FROM products
         WHERE is_active = 1 AND (tax_rule_id IS NULL OR tax_rule_id = '')
         ORDER BY name LIMIT 100",
    )
    .fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok("[DB] All active products have a tax rule configured.".into());
    }
    let names: Vec<String> = rows.iter().map(|r| s_str(r, "name")).collect();
    Ok(format!(
        "[DB] {} active product(s) missing tax rule:\n  {}",
        rows.len(),
        names.join("\n  ")
    ))
}

async fn z_report_archive(
    pool: &SqlitePool,
    input: &serde_json::Value,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let lim = limit_i(input, 30);
    let from = input.get("from").and_then(|v| v.as_str()).unwrap_or("2000-01-01");
    let to = input.get("to").and_then(|v| v.as_str()).unwrap_or("2999-12-31");
    let rows = sqlx::query(
        "SELECT business_date, COUNT(DISTINCT sale_id) AS tx,
                SUM(net_total_minor) AS revenue
         FROM sales WHERE status != 'voided' AND business_date BETWEEN ? AND ?
         GROUP BY business_date ORDER BY business_date DESC LIMIT ?",
    )
    .bind(from).bind(to).bind(lim)
    .fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok("[DB] No Z-report data for that period.".into());
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            format!(
                "  {} | {} tx | BHD {}",
                s_str(r, "business_date"),
                s_i64(r, "tx"),
                fmt(s_i64(r, "revenue"))
            )
        })
        .collect();
    Ok(format!(
        "[DB] Z-report archive ({from} → {to}):\n{}",
        lines.join("\n")
    ))
}

async fn low_stock_with_velocity(
    pool: &SqlitePool,
    input: &serde_json::Value,
) -> AppResult<String> {
    let days = period_days(input, 14);
    let rows = sqlx::query(
        "SELECT p.name, COALESCE(p.stock_quantity,0) AS stock,
                COALESCE(p.reorder_point,0) AS reorder,
                COALESCE(SUM(si.quantity),0) / ? AS daily_rate
         FROM products p
         LEFT JOIN sale_items si ON si.product_id = p.product_id
         LEFT JOIN sales s ON s.sale_id = si.sale_id
             AND s.sold_at >= date('now','-'||?||' days') AND s.status != 'voided'
         WHERE p.is_active = 1 AND p.track_stock = 1
           AND COALESCE(p.stock_quantity,0) <= COALESCE(p.reorder_point,0)
         GROUP BY p.product_id
         ORDER BY daily_rate DESC LIMIT 50",
    )
    .bind(days as f64).bind(days)
    .fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok("[DB] No low-stock items at or below reorder point.".into());
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            let rate: f64 = r.try_get::<f64, _>("daily_rate").unwrap_or(0.0);
            let stock = s_i64(r, "stock");
            let days_left = if rate > 0.0 { stock as f64 / rate } else { f64::INFINITY };
            format!(
                "  {} | stock: {} | reorder: {} | {:.1}/day | ~{:.0} day(s) left",
                s_str(r, "name"),
                stock,
                s_i64(r, "reorder"),
                rate,
                if days_left.is_finite() { days_left } else { 999.0 }
            )
        })
        .collect();
    Ok(format!(
        "[DB] Low-stock items by sales velocity (last {days} days):\n{}",
        lines.join("\n")
    ))
}

async fn list_suppliers(pool: &SqlitePool, input: &serde_json::Value) -> AppResult<String> {
    let search = input
        .get("search")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let pattern = format!("%{search}%");
    let rows = sqlx::query(
        "SELECT supplier_id, name, phone, email, contact_name, is_active FROM suppliers
         WHERE name LIKE ? ORDER BY name LIMIT 100",
    )
    .bind(&pattern)
    .fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok("[DB] No suppliers found.".into());
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            let active = s_i64(r, "is_active") == 1;
            format!(
                "  {} | {} | {} | {} | {}",
                s_str(r, "supplier_id"),
                s_str(r, "name"),
                s_str(r, "phone"),
                s_str(r, "contact_name"),
                if active { "active" } else { "inactive" }
            )
        })
        .collect();
    Ok(format!(
        "[DB] {} supplier(s):\n{}",
        rows.len(),
        lines.join("\n")
    ))
}

async fn list_purchase_orders(
    pool: &SqlitePool,
    input: &serde_json::Value,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let status = input.get("status").and_then(|v| v.as_str()).unwrap_or("%");
    let supplier_id = input.get("supplier_id").and_then(|v| v.as_str()).unwrap_or("%");
    let from = input.get("from").and_then(|v| v.as_str()).unwrap_or("2000-01-01");
    let to = input.get("to").and_then(|v| v.as_str()).unwrap_or("2999-12-31");
    let lim = limit_i(input, 20);
    let rows = sqlx::query(
        "SELECT po.po_id, po.status, po.created_at,
                COALESCE(s.name,'—') AS supplier,
                COALESCE(SUM(pol.ordered_qty * pol.unit_cost_minor),0) AS total_cost
         FROM purchase_orders po
         LEFT JOIN suppliers s ON s.supplier_id = po.supplier_id
         LEFT JOIN purchase_order_lines pol ON pol.po_id = po.po_id
         WHERE po.status LIKE ? AND po.supplier_id LIKE ?
           AND date(po.created_at) BETWEEN ? AND ?
         GROUP BY po.po_id ORDER BY po.created_at DESC LIMIT ?",
    )
    .bind(status).bind(supplier_id).bind(from).bind(to).bind(lim)
    .fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok("[DB] No purchase orders found.".into());
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            format!(
                "  {} | {} | {} | {} | BHD {}",
                s_str(r, "po_id"),
                s_str(r, "supplier"),
                s_str(r, "status"),
                s_str(r, "created_at"),
                fmt(s_i64(r, "total_cost"))
            )
        })
        .collect();
    Ok(format!(
        "[DB] {} purchase order(s):\n{}",
        rows.len(),
        lines.join("\n")
    ))
}

async fn database_size() -> AppResult<String> {
    let app_data = std::env::var("APPDATA").unwrap_or_default();
    let db_path = std::path::Path::new(&app_data)
        .join("com.super.zanpos")
        .join("zanpos.db");
    let wal_path = db_path.with_extension("db-wal");
    let db_size = std::fs::metadata(&db_path).map(|m| m.len()).unwrap_or(0);
    let wal_size = std::fs::metadata(&wal_path).map(|m| m.len()).unwrap_or(0);
    Ok(format!(
        "[DB] Database size:\n  Main file: {:.2} MB\n  WAL file: {:.2} MB\n  Total: {:.2} MB",
        db_size as f64 / 1_048_576.0,
        wal_size as f64 / 1_048_576.0,
        (db_size + wal_size) as f64 / 1_048_576.0
    ))
}

async fn table_row_counts(pool: &SqlitePool) -> AppResult<String> {
    let tables = [
        "products", "categories", "sales", "sale_items", "customers",
        "users", "delivery_orders", "shifts", "tax_rules", "suppliers",
        "purchase_orders", "audit_logs", "refunds", "held_carts", "cash_events",
    ];
    let mut lines = Vec::new();
    for t in &tables {
        let count: i64 =
            sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {t}"))
                .fetch_one(pool)
                .await
                .unwrap_or(0);
        lines.push(format!("  {t}: {count}"));
    }
    Ok(format!("[DB] Row counts:\n{}", lines.join("\n")))
}

async fn export_product_catalog(
    pool: &SqlitePool,
    input: &serde_json::Value,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let include_inactive = input
        .get("include_inactive")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let rows = sqlx::query(
        "SELECT p.name, p.price_minor, COALESCE(c.name,'—') AS cat,
                p.is_active, COALESCE(p.stock_quantity,0) AS stock,
                p.sku
         FROM products p LEFT JOIN categories c ON c.category_id = p.category_id
         WHERE (? = 1 OR p.is_active = 1)
         ORDER BY cat, p.name LIMIT 500",
    )
    .bind(if include_inactive { 1i64 } else { 0i64 })
    .fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok("[DB] No products found.".into());
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            format!(
                "  [{}] {} | BHD {} | stock: {} | SKU: {}{}",
                s_str(r, "cat"),
                s_str(r, "name"),
                fmt(s_i64(r, "price_minor")),
                s_i64(r, "stock"),
                s_str(r, "sku"),
                if s_i64(r, "is_active") == 0 { " [INACTIVE]" } else { "" }
            )
        })
        .collect();
    Ok(format!(
        "[DB] Product catalog ({} items):\n{}",
        rows.len(),
        lines.join("\n")
    ))
}

async fn customer_notes(pool: &SqlitePool, input: &serde_json::Value) -> AppResult<String> {
    let cid = input
        .get("customer_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("customer_id required".into()))?;
    let row = sqlx::query(
        "SELECT name, notes FROM customers WHERE customer_id = ?",
    )
    .bind(cid)
    .fetch_optional(pool).await?
    .ok_or_else(|| AppError::NotFound(format!("Customer {cid} not found")))?;
    let name = s_str(&row, "name");
    let notes = s_str(&row, "notes");
    if notes.is_empty() {
        Ok(format!("[DB] Customer {name} has no notes."))
    } else {
        Ok(format!("[DB] Notes for {name}:\n  {notes}"))
    }
}

async fn user_permissions(pool: &SqlitePool, input: &serde_json::Value) -> AppResult<String> {
    let uid = input
        .get("user_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("user_id required".into()))?;
    let row = sqlx::query(
        "SELECT display_name, role, is_active FROM users WHERE user_id = ?",
    )
    .bind(uid)
    .fetch_optional(pool).await?
    .ok_or_else(|| AppError::NotFound(format!("User {uid} not found")))?;
    let name = s_str(&row, "display_name");
    let role = s_str(&row, "role");
    let active = s_i64(&row, "is_active") == 1;
    let perms = match role.as_str() {
        "owner" => "All permissions including: users, devices, audit, settings, tax rules, syncing, voiding, discounts, delivery, end-of-day.",
        "manager" => "Sales, inventory, customers, deliveries, discounts, end-of-day, reports. Cannot manage users/devices.",
        "cashier" => "Create sales, process payments, deliveries assigned to them. No admin access.",
        other => other,
    };
    Ok(format!(
        "[DB] User: {name} ({role}) — Active: {}\nPermissions: {perms}",
        if active { "yes" } else { "no" }
    ))
}

async fn products_without_barcode(pool: &SqlitePool) -> AppResult<String> {
    let rows = sqlx::query(
        "SELECT p.product_id, p.name, p.sku FROM products p
         WHERE p.is_active = 1
           AND (p.sku IS NULL OR p.sku = '')
           AND NOT EXISTS (SELECT 1 FROM product_barcodes pb WHERE pb.product_id = p.product_id)
         ORDER BY p.name LIMIT 100",
    )
    .fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok("[DB] All active products have a barcode or SKU.".into());
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|r| format!("  {} | {}", s_str(r, "product_id"), s_str(r, "name")))
        .collect();
    Ok(format!(
        "[DB] {} active product(s) without barcode/SKU:\n{}",
        rows.len(),
        lines.join("\n")
    ))
}

async fn search_sales_by_customer(
    pool: &SqlitePool,
    input: &serde_json::Value,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let cid = input
        .get("customer_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("customer_id required".into()))?;
    let lim = limit_i(input, 20);
    let rows = sqlx::query(
        "SELECT receipt_number, sold_at, net_total_minor, status FROM sales
         WHERE customer_id = ? ORDER BY sold_at DESC LIMIT ?",
    )
    .bind(cid).bind(lim)
    .fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok(format!("[DB] No sales found for customer {cid}."));
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            format!(
                "  #{} | {} | {} | BHD {}",
                s_str(r, "receipt_number"),
                s_str(r, "sold_at"),
                s_str(r, "status"),
                fmt(s_i64(r, "net_total_minor"))
            )
        })
        .collect();
    Ok(format!(
        "[DB] {} sale(s) for customer {cid}:\n{}",
        rows.len(),
        lines.join("\n")
    ))
}

async fn shift_performance(
    pool: &SqlitePool,
    input: &serde_json::Value,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let from = input.get("from").and_then(|v| v.as_str()).unwrap_or("2000-01-01");
    let to = input.get("to").and_then(|v| v.as_str()).unwrap_or("2999-12-31");
    let lim = limit_i(input, 20);
    let rows = sqlx::query(
        "SELECT sh.shift_id, sh.opened_at,
                COALESCE(u.display_name,'(deleted)') AS cashier,
                COUNT(DISTINCT s.sale_id) AS tx,
                COALESCE(SUM(s.net_total_minor),0) AS revenue
         FROM shifts sh
         LEFT JOIN users u ON u.user_id = sh.cashier_user_id
         LEFT JOIN sales s ON s.shift_id = sh.shift_id AND s.status != 'voided'
         WHERE date(sh.opened_at) BETWEEN ? AND ?
         GROUP BY sh.shift_id ORDER BY sh.opened_at DESC LIMIT ?",
    )
    .bind(from).bind(to).bind(lim)
    .fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok("[DB] No shifts in that period.".into());
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            format!(
                "  {} | {} | {} tx | BHD {}",
                s_str(r, "cashier"),
                s_str(r, "opened_at"),
                s_i64(r, "tx"),
                fmt(s_i64(r, "revenue"))
            )
        })
        .collect();
    Ok(format!(
        "[DB] Shift performance ({from} → {to}):\n{}",
        lines.join("\n")
    ))
}
