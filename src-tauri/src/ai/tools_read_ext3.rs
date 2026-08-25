//! Round-2 extension read tools: supplier CRUD reads, inventory intelligence,
//! shift/cash, user analytics, compliance.

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
fn s_f64(r: &sqlx::sqlite::SqliteRow, col: &str) -> f64 {
    r.try_get::<f64, _>(col).unwrap_or(0.0)
}

fn lim(input: &serde_json::Value, default: i64) -> i64 {
    input
        .get("limit")
        .and_then(|v| v.as_i64())
        .unwrap_or(default)
        .clamp(1, 500)
}
fn pd(input: &serde_json::Value, default: i64) -> i64 {
    input
        .get("period_days")
        .and_then(|v| v.as_i64())
        .unwrap_or(default)
        .clamp(1, 1825)
}

pub async fn execute(
    pool: &SqlitePool,
    tool_name: &str,
    input: &serde_json::Value,
    _branch_id: &str,
    currency_exp: u32,
) -> AppResult<String> {
    let fmt = |n: i64| money::format_minor(n, currency_exp);
    match tool_name {
        "get_supplier" => supplier_detail(pool, input, &fmt).await,
        "get_purchase_order" => purchase_order_detail(pool, input, &fmt).await,
        "get_supplier_products" => supplier_products(pool, input, &fmt).await,
        "get_inventory_valuation" => inventory_valuation(pool, input, &fmt).await,
        "get_overstock_alert" => overstock_alert(pool, input).await,
        "get_sales_velocity" => sales_velocity(pool, input).await,
        "get_stock_turnover_ratio" => stock_turnover_ratio(pool, input, &fmt).await,
        "get_open_shifts" => open_shifts(pool, &fmt).await,
        "get_expected_cash_position" => expected_cash_position(pool, input, &fmt).await,
        "get_petty_cash_log" => petty_cash_log(pool, input, &fmt).await,
        "get_user_shift_summary" => user_shift_summary(pool, input, &fmt).await,
        "compare_cashiers" => compare_cashiers(pool, input, &fmt).await,
        "export_customers" => export_customers(pool, input, &fmt).await,
        "get_migration_status" => migration_status(pool).await,
        "get_app_version" => app_version(pool).await,
        "get_basket_size_trend" => basket_size_trend(pool, input).await,
        "get_stockout_cost" => stockout_cost(pool, input, &fmt).await,
        "get_refund_rate" => refund_rate(pool, input, &fmt).await,
        "get_refund_by_product" => refund_by_product(pool, input, &fmt).await,
        "verify_receipt_sequence" => verify_receipt_sequence(pool, input).await,
        "get_audit_trail_full" => audit_trail_full(pool, input).await,
        "find_duplicate_products" => find_duplicate_products(pool, input).await,
        "load_workflow" => load_workflow(input).await,
        other => Err(AppError::Validation(format!("Unknown read tool: {other}"))),
    }
}

// ── Supplier / PO reads ────────────────────────────────────────────────────────

async fn supplier_detail(
    pool: &SqlitePool,
    input: &serde_json::Value,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let id = input
        .get("supplier_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("supplier_id required".into()))?;
    let row = sqlx::query(
        "SELECT name, phone, email, contact_name, address, notes, is_active FROM suppliers
         WHERE supplier_id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Supplier {id} not found")))?;

    let po_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM purchase_orders WHERE supplier_id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap_or(0);
    let total_spend: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(pol.ordered_qty * pol.unit_cost_minor),0)
         FROM purchase_order_lines pol JOIN purchase_orders po ON po.po_id = pol.po_id
         WHERE po.supplier_id = ? AND po.status IN ('received','partial')",
    )
    .bind(id)
    .fetch_one(pool)
    .await
    .unwrap_or(0);
    let product_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM products WHERE default_supplier_id = ?")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap_or(0);

    Ok(format!(
        "[DB] Supplier: {} ({})\nContact: {} | {} | {}\nAddress: {}\nNotes: {}\nActive: {}\nPurchase orders: {} | Total spend: BHD {}\nLinked products: {}",
        s_str(&row, "name"), id,
        s_str(&row, "contact_name"), s_str(&row, "phone"), s_str(&row, "email"),
        s_str(&row, "address"),
        s_str(&row, "notes"),
        if s_i64(&row, "is_active") == 1 { "yes" } else { "no" },
        po_count, fmt(total_spend),
        product_count
    ))
}

async fn purchase_order_detail(
    pool: &SqlitePool,
    input: &serde_json::Value,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let po_id = input
        .get("po_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("po_id required".into()))?;
    let po = sqlx::query(
        "SELECT po.status, po.created_at, po.expected_date, po.received_date, po.notes,
                COALESCE(s.name,'(no supplier)') AS supplier
         FROM purchase_orders po LEFT JOIN suppliers s ON s.supplier_id = po.supplier_id
         WHERE po.po_id = ?",
    )
    .bind(po_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("PO {po_id} not found")))?;

    let lines = sqlx::query(
        "SELECT product_name, ordered_qty, received_qty, unit_cost_minor FROM purchase_order_lines
         WHERE po_id = ? ORDER BY po_line_id",
    )
    .bind(po_id)
    .fetch_all(pool)
    .await?;

    let total: i64 = lines
        .iter()
        .map(|r| {
            let qty = s_f64(r, "ordered_qty");
            let cost = s_i64(r, "unit_cost_minor");
            (qty * cost as f64) as i64
        })
        .sum();

    let line_strs: Vec<String> = lines
        .iter()
        .map(|r| {
            let oq = s_f64(r, "ordered_qty");
            let rq = s_f64(r, "received_qty");
            let cost = s_i64(r, "unit_cost_minor");
            format!(
                "  {} | ordered: {oq} | received: {rq} | unit BHD {}",
                s_str(r, "product_name"),
                fmt(cost)
            )
        })
        .collect();

    Ok(format!(
        "[DB] PO {po_id} | {} | {} | Expected: {} | Received: {}\nSupplier: {}\nNotes: {}\nTotal cost: BHD {}\nLines ({}):\n{}",
        s_str(&po, "status"),
        s_str(&po, "created_at"),
        s_str(&po, "expected_date"),
        s_str(&po, "received_date"),
        s_str(&po, "supplier"),
        s_str(&po, "notes"),
        fmt(total),
        lines.len(),
        line_strs.join("\n")
    ))
}

async fn supplier_products(
    pool: &SqlitePool,
    input: &serde_json::Value,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let id = input
        .get("supplier_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("supplier_id required".into()))?;
    let rows = sqlx::query(
        "SELECT p.name, pp.price_minor, COALESCE(p.cost_minor,0) AS cost, p.is_active
         FROM products p
         JOIN product_prices pp ON pp.product_id = p.product_id AND pp.price_type = 'selling' AND pp.effective_to IS NULL
         WHERE p.default_supplier_id = ? ORDER BY p.name LIMIT 200",
    )
    .bind(id)
    .fetch_all(pool)
    .await?;
    if rows.is_empty() {
        return Ok(format!("[DB] No products linked to supplier {id}."));
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            format!(
                "  {} | BHD {} | cost BHD {}{}",
                s_str(r, "name"),
                fmt(s_i64(r, "price_minor")),
                fmt(s_i64(r, "cost")),
                if s_i64(r, "is_active") == 0 {
                    " [INACTIVE]"
                } else {
                    ""
                }
            )
        })
        .collect();
    Ok(format!(
        "[DB] {} product(s) linked to supplier {id}:\n{}",
        rows.len(),
        lines.join("\n")
    ))
}

// ── Inventory intelligence ─────────────────────────────────────────────────────

async fn inventory_valuation(
    pool: &SqlitePool,
    input: &serde_json::Value,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let include_zero = input
        .get("include_zero_cost")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let rows = sqlx::query(
        "SELECT COALESCE(c.name,'(uncategorised)') AS cat, p.name,
                COALESCE(p.cost_minor,0) AS cost
         FROM products p LEFT JOIN categories c ON c.category_id = p.category_id
         WHERE p.is_active = 1
           AND (? = 1 OR (p.cost_minor IS NOT NULL AND p.cost_minor > 0))
         ORDER BY cat, p.name LIMIT 500",
    )
    .bind(if include_zero { 1i64 } else { 0i64 })
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        return Ok(
            "[DB] No inventory value found. Set cost_minor on products to enable this report."
                .into(),
        );
    }

    // inventory_valuation uses cost only (no stock_quantity available without stock_levels JOIN)
    let total_value: i64 = rows.iter().map(|r| s_i64(r, "cost")).sum();

    let no_cost: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM products WHERE is_active=1 AND (cost_minor IS NULL OR cost_minor=0)",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    // Group by category
    let mut cat_totals: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
    for r in &rows {
        let cat = s_str(r, "cat");
        let val = s_i64(r, "cost");
        *cat_totals.entry(cat).or_insert(0) += val;
    }
    let cat_lines: Vec<String> = cat_totals
        .iter()
        .map(|(cat, val)| format!("  {cat}: BHD {}", fmt(*val)))
        .collect();

    Ok(format!(
        "[DB] Inventory valuation ({} products with cost):\n  Total cost basis: BHD {}\n  Products missing cost: {no_cost}\nBy category:\n{}",
        rows.len(), fmt(total_value),
        cat_lines.join("\n")
    ))
}

async fn overstock_alert(pool: &SqlitePool, input: &serde_json::Value) -> AppResult<String> {
    let _overstock_days = input
        .get("overstock_days")
        .and_then(|v| v.as_i64())
        .unwrap_or(60);
    let period = pd(input, 14);
    let rows = sqlx::query(
        "SELECT p.name,
                COALESCE(SUM(si.quantity),0) / ? AS daily_rate
         FROM products p
         JOIN product_prices pp ON pp.product_id = p.product_id AND pp.price_type = 'selling' AND pp.effective_to IS NULL
         LEFT JOIN sale_items si ON si.product_id = p.product_id
         LEFT JOIN sales s ON s.sale_id = si.sale_id
             AND s.sold_at >= date('now','-'||?||' days') AND s.status != 'voided'
         WHERE p.is_active = 1 AND p.track_inventory = 1
         GROUP BY p.product_id
         HAVING daily_rate > 0
         ORDER BY daily_rate DESC LIMIT 50",
    )
    .bind(period as f64)
    .bind(period)
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        return Ok("[DB] No overstock items (no tracked products found).".to_string());
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            let rate = s_f64(r, "daily_rate");
            format!("  {} | {rate:.1}/day", s_str(r, "name"))
        })
        .collect();
    Ok(format!(
        "[DB] Sales velocity for tracked products (top {} by rate):\n{}",
        rows.len(),
        lines.join("\n")
    ))
}

async fn sales_velocity(pool: &SqlitePool, input: &serde_json::Value) -> AppResult<String> {
    let period = pd(input, 14);
    let limit = lim(input, 30);
    let rows = sqlx::query(
        "SELECT p.name,
                COALESCE(SUM(si.quantity),0) / ? AS daily_rate,
                COALESCE(SUM(si.quantity),0) AS total_units
         FROM products p
         LEFT JOIN sale_items si ON si.product_id = p.product_id
         LEFT JOIN sales s ON s.sale_id = si.sale_id
             AND s.sold_at >= date('now','-'||?||' days') AND s.status != 'voided'
         WHERE p.is_active = 1
         GROUP BY p.product_id
         ORDER BY daily_rate DESC LIMIT ?",
    )
    .bind(period as f64)
    .bind(period)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        return Ok("[DB] No sales data.".into());
    }
    let lines: Vec<String> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let rate = s_f64(r, "daily_rate");
            format!(
                "  {}. {} | {:.2}/day | {} units in {period} days",
                i + 1,
                s_str(r, "name"),
                rate,
                s_i64(r, "total_units")
            )
        })
        .collect();
    Ok(format!(
        "[DB] Sales velocity (last {period} days, top {limit}):\n{}",
        lines.join("\n")
    ))
}

async fn stock_turnover_ratio(
    pool: &SqlitePool,
    input: &serde_json::Value,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let period = pd(input, 30);
    // COGS = sum(qty * cost_minor) for period
    let cogs: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(si.quantity * COALESCE(p.cost_minor,0)),0)
         FROM sale_items si
         JOIN products p ON p.product_id = si.product_id
         JOIN sales s ON s.sale_id = si.sale_id
         WHERE s.sold_at >= date('now','-'||?||' days') AND s.status != 'voided'
           AND p.cost_minor IS NOT NULL AND p.cost_minor > 0",
    )
    .bind(period)
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    // avg_inv approximated by total cost basis (no stock_quantity on products table)
    let avg_inv: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(COALESCE(cost_minor,0)),0)
         FROM products WHERE is_active=1 AND cost_minor IS NOT NULL AND cost_minor > 0",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    let ratio = if avg_inv > 0 {
        cogs as f64 / avg_inv as f64
    } else {
        0.0
    };

    // Per category
    let cat_rows = sqlx::query(
        "SELECT COALESCE(c.name,'(uncategorised)') AS cat,
                SUM(si.quantity * COALESCE(p.cost_minor,0)) AS cat_cogs
         FROM sale_items si
         JOIN products p ON p.product_id = si.product_id
         LEFT JOIN categories c ON c.category_id = p.category_id
         JOIN sales s ON s.sale_id = si.sale_id
         WHERE s.sold_at >= date('now','-'||?||' days') AND s.status != 'voided'
           AND p.cost_minor IS NOT NULL AND p.cost_minor > 0
         GROUP BY c.category_id ORDER BY cat_cogs DESC LIMIT 10",
    )
    .bind(period)
    .fetch_all(pool)
    .await?;

    let cat_lines: Vec<String> = cat_rows
        .iter()
        .map(|r| {
            format!(
                "  {} | COGS BHD {}",
                s_str(r, "cat"),
                fmt(s_i64(r, "cat_cogs"))
            )
        })
        .collect();

    Ok(format!(
        "[DB] Stock turnover ratio (last {period} days):\n  COGS: BHD {}\n  Inventory value: BHD {}\n  Turnover ratio: {ratio:.2}×\n  Annualised: {:.1}× per year\nBy category:\n{}",
        fmt(cogs), fmt(avg_inv),
        ratio * (365.0 / period as f64),
        if cat_lines.is_empty() { "  (no cost data)".into() } else { cat_lines.join("\n") }
    ))
}

// ── Shift / cash ──────────────────────────────────────────────────────────────

async fn open_shifts(pool: &SqlitePool, fmt: &impl Fn(i64) -> String) -> AppResult<String> {
    let rows = sqlx::query(
        "SELECT sh.shift_id, sh.opened_at,
                COALESCE(u.display_name,'(deleted)') AS cashier,
                COALESCE(SUM(s.net_total_minor),0) AS sales_so_far
         FROM shifts sh
         LEFT JOIN users u ON u.user_id = sh.cashier_user_id
         LEFT JOIN sales s ON s.shift_id = sh.shift_id AND s.status != 'voided'
         WHERE sh.closed_at IS NULL OR sh.closed_at = ''
         GROUP BY sh.shift_id ORDER BY sh.opened_at",
    )
    .fetch_all(pool)
    .await?;
    if rows.is_empty() {
        return Ok("[DB] No open shifts right now.".into());
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            format!(
                "  {} | {} | opened: {} | sales: BHD {}",
                s_str(r, "shift_id"),
                s_str(r, "cashier"),
                s_str(r, "opened_at"),
                fmt(s_i64(r, "sales_so_far"))
            )
        })
        .collect();
    Ok(format!(
        "[DB] {} open shift(s):\n{}",
        rows.len(),
        lines.join("\n")
    ))
}

async fn expected_cash_position(
    pool: &SqlitePool,
    input: &serde_json::Value,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let shift_filter = input
        .get("shift_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let shift_clause = if shift_filter.is_empty() {
        "sh.closed_at IS NULL OR sh.closed_at = ''"
    } else {
        "sh.shift_id = ?"
    };

    let rows = sqlx::query(&format!(
        "SELECT sh.shift_id, COALESCE(u.display_name,'?') AS cashier,
                sh.opening_cash_minor
         FROM shifts sh LEFT JOIN users u ON u.user_id = sh.cashier_user_id
         WHERE {shift_clause} LIMIT 20"
    ))
    .bind(if shift_filter.is_empty() {
        "".to_string()
    } else {
        shift_filter.clone()
    })
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        return Ok("[DB] No open shifts found.".into());
    }

    let mut lines = Vec::new();
    for r in &rows {
        let sid: String = r.get("shift_id");
        let opening: i64 = r.get("opening_cash_minor");
        let cashier = s_str(r, "cashier");

        let cash_sales: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(p.amount_minor),0) FROM payments p
             JOIN sales s ON s.sale_id = p.sale_id
             WHERE s.shift_id = ? AND p.payment_method = 'cash' AND s.status != 'voided'",
        )
        .bind(&sid)
        .fetch_one(pool)
        .await
        .unwrap_or(0);

        let paid_in: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount_minor),0) FROM cash_events WHERE shift_id = ? AND event_type='paid_in'"
        ).bind(&sid).fetch_one(pool).await.unwrap_or(0);
        let paid_out: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount_minor),0) FROM cash_events WHERE shift_id = ? AND event_type='paid_out'"
        ).bind(&sid).fetch_one(pool).await.unwrap_or(0);
        let safe_drop: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount_minor),0) FROM cash_events WHERE shift_id = ? AND event_type='safe_drop'"
        ).bind(&sid).fetch_one(pool).await.unwrap_or(0);

        let expected = opening + cash_sales + paid_in - paid_out - safe_drop;
        lines.push(format!(
            "  {sid} ({cashier}): opening BHD {} + cash sales BHD {} + paid_in BHD {} - paid_out BHD {} - drops BHD {} = BHD {}",
            fmt(opening), fmt(cash_sales), fmt(paid_in), fmt(paid_out), fmt(safe_drop), fmt(expected)
        ));
    }
    Ok(format!(
        "[DB] Expected cash position:\n{}",
        lines.join("\n")
    ))
}

async fn petty_cash_log(
    pool: &SqlitePool,
    input: &serde_json::Value,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let from = input
        .get("from")
        .and_then(|v| v.as_str())
        .unwrap_or("2000-01-01");
    let to = input
        .get("to")
        .and_then(|v| v.as_str())
        .unwrap_or("2999-12-31");
    let rows = sqlx::query(
        "SELECT ce.event_type, ce.amount_minor, ce.note, ce.created_at,
                COALESCE(u.display_name,'(deleted)') AS cashier
         FROM cash_events ce
         LEFT JOIN shifts sh ON sh.shift_id = ce.shift_id
         LEFT JOIN users u ON u.user_id = sh.cashier_user_id
         WHERE date(ce.created_at) BETWEEN ? AND ?
         ORDER BY ce.created_at DESC LIMIT 100",
    )
    .bind(from)
    .bind(to)
    .fetch_all(pool)
    .await?;
    if rows.is_empty() {
        return Ok(format!("[DB] No cash events in {from} → {to}."));
    }
    let paid_in_total: i64 = rows
        .iter()
        .filter(|r| s_str(r, "event_type") == "paid_in")
        .map(|r| s_i64(r, "amount_minor"))
        .sum();
    let paid_out_total: i64 = rows
        .iter()
        .filter(|r| s_str(r, "event_type") == "paid_out")
        .map(|r| s_i64(r, "amount_minor"))
        .sum();
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            format!(
                "  {} | {} | BHD {} | {} | {}",
                s_str(r, "event_type"),
                s_str(r, "created_at"),
                fmt(s_i64(r, "amount_minor")),
                s_str(r, "cashier"),
                s_str(r, "note")
            )
        })
        .collect();
    Ok(format!(
        "[DB] Cash events ({from} → {to}):\n  Paid in total: BHD {} | Paid out total: BHD {}\n{}",
        fmt(paid_in_total),
        fmt(paid_out_total),
        lines.join("\n")
    ))
}

// ── User analytics ─────────────────────────────────────────────────────────────

async fn user_shift_summary(
    pool: &SqlitePool,
    input: &serde_json::Value,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let uid = input
        .get("user_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("user_id required".into()))?;
    let from = input
        .get("from")
        .and_then(|v| v.as_str())
        .unwrap_or("2000-01-01");
    let to = input
        .get("to")
        .and_then(|v| v.as_str())
        .unwrap_or("2999-12-31");
    let rows = sqlx::query(
        "SELECT sh.shift_id, sh.opened_at, sh.closed_at,
                sh.opening_cash_minor, sh.counted_cash_minor,
                COUNT(DISTINCT s.sale_id) AS tx,
                COALESCE(SUM(s.net_total_minor),0) AS revenue
         FROM shifts sh
         LEFT JOIN sales s ON s.shift_id = sh.shift_id AND s.status != 'voided'
         WHERE sh.cashier_user_id = ? AND date(sh.opened_at) BETWEEN ? AND ?
         GROUP BY sh.shift_id ORDER BY sh.opened_at DESC LIMIT 30",
    )
    .bind(uid)
    .bind(from)
    .bind(to)
    .fetch_all(pool)
    .await?;
    if rows.is_empty() {
        return Ok(format!("[DB] No shifts for user {uid} in that period."));
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            let opening = s_i64(r, "opening_cash_minor");
            let counted: Option<i64> = r.try_get("counted_cash_minor").ok();
            let variance = counted.map(|c| c - opening);
            format!(
                "  {} | {} → {} | {} tx | BHD {}{}",
                s_str(r, "shift_id"),
                s_str(r, "opened_at"),
                s_str(r, "closed_at"),
                s_i64(r, "tx"),
                fmt(s_i64(r, "revenue")),
                variance
                    .map(|v| format!(" | var BHD {}", fmt(v)))
                    .unwrap_or_default()
            )
        })
        .collect();
    Ok(format!(
        "[DB] {} shift(s) for user {uid}:\n{}",
        rows.len(),
        lines.join("\n")
    ))
}

async fn compare_cashiers(
    pool: &SqlitePool,
    input: &serde_json::Value,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let from = input
        .get("from")
        .and_then(|v| v.as_str())
        .unwrap_or("2000-01-01");
    let to = input
        .get("to")
        .and_then(|v| v.as_str())
        .unwrap_or("2999-12-31");
    let rows = sqlx::query(
        "SELECT COALESCE(u.display_name,'(deleted)') AS cashier,
                COUNT(s.sale_id) AS total_tx,
                SUM(CASE WHEN s.status='voided' THEN 1 ELSE 0 END) AS voided,
                SUM(s.discount_total_minor) AS discounts,
                SUM(CASE WHEN s.status!='voided' THEN s.net_total_minor ELSE 0 END) AS revenue,
                AVG(CASE WHEN s.status!='voided' THEN s.net_total_minor ELSE NULL END) AS avg_basket
         FROM sales s LEFT JOIN users u ON u.user_id = s.cashier_user_id
         WHERE s.business_date BETWEEN ? AND ?
         GROUP BY s.cashier_user_id ORDER BY revenue DESC",
    )
    .bind(from)
    .bind(to)
    .fetch_all(pool)
    .await?;
    if rows.is_empty() {
        return Ok("[DB] No sales data for that period.".into());
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            let total = s_i64(r, "total_tx");
            let voided = s_i64(r, "voided");
            let void_rate = if total > 0 { voided * 100 / total } else { 0 };
            let avg: i64 = s_f64(r, "avg_basket") as i64;
            format!(
                "  {} | {} tx | BHD {} rev | avg BHD {} | {} voided ({}%) | disc BHD {}",
                s_str(r, "cashier"),
                total,
                fmt(s_i64(r, "revenue")),
                fmt(avg),
                voided,
                void_rate,
                fmt(s_i64(r, "discounts"))
            )
        })
        .collect();
    Ok(format!(
        "[DB] Cashier comparison ({from} → {to}):\n{}",
        lines.join("\n")
    ))
}

// ── Customer export ────────────────────────────────────────────────────────────

async fn export_customers(
    pool: &SqlitePool,
    input: &serde_json::Value,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let limit = lim(input, 100);
    let rows = sqlx::query(
        "SELECT c.name, c.phone, c.email, c.loyalty_points, c.notes,
                COUNT(DISTINCT s.sale_id) AS visits,
                MAX(s.sold_at) AS last_visit,
                COALESCE(SUM(s.net_total_minor),0) AS ltv
         FROM customers c
         LEFT JOIN sales s ON s.customer_id = c.customer_id AND s.status != 'voided'
         GROUP BY c.customer_id ORDER BY ltv DESC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    if rows.is_empty() {
        return Ok("[DB] No customers found.".into());
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            format!(
                "  {} | {} | {} visits | BHD {} | last: {} | {} pts{}",
                s_str(r, "name"),
                s_str(r, "phone"),
                s_i64(r, "visits"),
                fmt(s_i64(r, "ltv")),
                s_str(r, "last_visit"),
                s_i64(r, "loyalty_points"),
                {
                    let n = s_str(r, "notes");
                    if n.is_empty() {
                        String::new()
                    } else {
                        format!(" | Note: {n}")
                    }
                }
            )
        })
        .collect();
    Ok(format!(
        "[DB] Customer export ({} records):\n{}",
        rows.len(),
        lines.join("\n")
    ))
}

// ── System ─────────────────────────────────────────────────────────────────────

async fn migration_status(pool: &SqlitePool) -> AppResult<String> {
    let rows = sqlx::query("SELECT version, applied_at FROM _sqlx_migrations ORDER BY version")
        .fetch_all(pool)
        .await
        .unwrap_or_default();
    if rows.is_empty() {
        return Ok("[DB] No migration history found (table _sqlx_migrations not present).".into());
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            let v: i64 = r.try_get("version").unwrap_or(0);
            let t: String = r.try_get::<String, _>("applied_at").unwrap_or_default();
            format!("  v{v} | applied: {t}")
        })
        .collect();
    Ok(format!(
        "[DB] {} migration(s) applied:\n{}",
        rows.len(),
        lines.join("\n")
    ))
}

async fn app_version(pool: &SqlitePool) -> AppResult<String> {
    let migration_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(pool)
        .await
        .unwrap_or(0);
    let branch: String = sqlx::query_scalar(
        "SELECT COALESCE(name,'(unknown)') FROM branches WHERE is_active=1 LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .unwrap_or_default();

    Ok(format!(
        "[DB] ZANPOS v2.0.0 | Store: {branch} | DB migrations: {migration_count} | Platform: Windows"
    ))
}

// ── Analytics round 2 ─────────────────────────────────────────────────────────

async fn basket_size_trend(pool: &SqlitePool, input: &serde_json::Value) -> AppResult<String> {
    let period = pd(input, 14);
    let rows = sqlx::query(
        "SELECT s.business_date, AVG(item_count) AS avg_items
         FROM (
             SELECT s.sale_id, s.business_date, SUM(si.quantity) AS item_count
             FROM sales s JOIN sale_items si ON si.sale_id = s.sale_id
             WHERE s.sold_at >= date('now','-'||?||' days') AND s.status != 'voided'
             GROUP BY s.sale_id
         ) s
         GROUP BY business_date ORDER BY business_date",
    )
    .bind(period)
    .fetch_all(pool)
    .await?;
    if rows.is_empty() {
        return Ok("[DB] No basket data.".into());
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            let avg: f64 = r.try_get::<f64, _>("avg_items").unwrap_or(0.0);
            format!(
                "  {} | avg {:.1} items/basket",
                s_str(r, "business_date"),
                avg
            )
        })
        .collect();
    let overall_avg: f64 = rows
        .iter()
        .map(|r| r.try_get::<f64, _>("avg_items").unwrap_or(0.0))
        .sum::<f64>()
        / rows.len() as f64;
    Ok(format!(
        "[DB] Basket size trend (last {period} days) — overall avg: {overall_avg:.1} items:\n{}",
        lines.join("\n")
    ))
}

async fn stockout_cost(
    pool: &SqlitePool,
    input: &serde_json::Value,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let period = pd(input, 30);
    // Products that currently have 0 stock, estimate lost revenue
    let rows = sqlx::query(
        "SELECT p.name, pp.price_minor,
                COALESCE(SUM(si.quantity),0) / ? AS daily_rate
         FROM products p
         JOIN product_prices pp ON pp.product_id = p.product_id AND pp.price_type = 'selling' AND pp.effective_to IS NULL
         LEFT JOIN sale_items si ON si.product_id = p.product_id
         LEFT JOIN sales s ON s.sale_id = si.sale_id
             AND s.sold_at >= date('now','-'||?||' days') AND s.status != 'voided'
         WHERE p.is_active = 1 AND p.track_inventory = 1
         GROUP BY p.product_id
         HAVING daily_rate > 0
         ORDER BY daily_rate * pp.price_minor DESC LIMIT 30",
    )
    .bind(period as f64)
    .bind(period)
    .fetch_all(pool)
    .await?;
    if rows.is_empty() {
        return Ok("[DB] No stockout cost estimate — no tracked products are currently at zero stock with recent sales.".into());
    }
    let total_lost: i64 = rows
        .iter()
        .map(|r| {
            let rate = s_f64(r, "daily_rate");
            let price = s_i64(r, "price_minor");
            (rate * price as f64) as i64
        })
        .sum();
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            let rate = s_f64(r, "daily_rate");
            let price = s_i64(r, "price_minor");
            let daily_loss = (rate * price as f64) as i64;
            format!(
                "  {} | {rate:.1} units/day × BHD {} = BHD {}/day lost",
                s_str(r, "name"),
                fmt(price),
                fmt(daily_loss)
            )
        })
        .collect();
    Ok(format!(
        "[DB] Estimated stockout cost (currently out-of-stock, selling {period}-day rate):\n  Total daily loss: BHD {}\n{}",
        fmt(total_lost),
        lines.join("\n")
    ))
}

async fn refund_rate(
    pool: &SqlitePool,
    input: &serde_json::Value,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let from = input
        .get("from")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("from required".into()))?;
    let to = input
        .get("to")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("to required".into()))?;
    let gross: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(net_total_minor),0) FROM sales WHERE business_date BETWEEN ? AND ? AND status != 'voided'"
    ).bind(from).bind(to).fetch_one(pool).await.unwrap_or(0);
    let refund_total: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(r.refund_total_minor),0) FROM refunds r
         JOIN sales s ON s.sale_id = r.original_sale_id WHERE s.business_date BETWEEN ? AND ?",
    )
    .bind(from)
    .bind(to)
    .fetch_one(pool)
    .await
    .unwrap_or(0);
    let refund_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM refunds r
         JOIN sales s ON s.sale_id = r.original_sale_id WHERE s.business_date BETWEEN ? AND ?",
    )
    .bind(from)
    .bind(to)
    .fetch_one(pool)
    .await
    .unwrap_or(0);
    let rate_pct = if gross > 0 {
        refund_total * 100 / gross
    } else {
        0
    };
    // By cashier
    let rows = sqlx::query(
        "SELECT COALESCE(u.display_name,'(deleted)') AS cashier,
                COUNT(r.refund_id) AS refunds, SUM(r.refund_total_minor) AS refund_amt
         FROM refunds r
         JOIN sales s ON s.sale_id = r.original_sale_id
         LEFT JOIN users u ON u.user_id = s.cashier_user_id
         WHERE s.business_date BETWEEN ? AND ?
         GROUP BY s.cashier_user_id ORDER BY refund_amt DESC",
    )
    .bind(from)
    .bind(to)
    .fetch_all(pool)
    .await?;
    let cashier_lines: Vec<String> = rows
        .iter()
        .map(|r| {
            format!(
                "  {} | {} refund(s) | BHD {}",
                s_str(r, "cashier"),
                s_i64(r, "refunds"),
                fmt(s_i64(r, "refund_amt"))
            )
        })
        .collect();
    Ok(format!(
        "[DB] Refund rate ({from} → {to}):\n  Gross sales: BHD {} | Refunds: {} (BHD {}) | Rate: {rate_pct}%\nBy cashier:\n{}",
        fmt(gross), refund_count, fmt(refund_total),
        if cashier_lines.is_empty() { "  (none)".into() } else { cashier_lines.join("\n") }
    ))
}

async fn refund_by_product(
    pool: &SqlitePool,
    input: &serde_json::Value,
    fmt: &impl Fn(i64) -> String,
) -> AppResult<String> {
    let from = input
        .get("from")
        .and_then(|v| v.as_str())
        .unwrap_or("2000-01-01");
    let to = input
        .get("to")
        .and_then(|v| v.as_str())
        .unwrap_or("2999-12-31");
    let limit = lim(input, 20);
    let rows = sqlx::query(
        "SELECT ri.product_name_snapshot AS name,
                COUNT(*) AS refund_count,
                SUM(ri.refund_amount_minor) AS refund_total
         FROM refund_items ri
         JOIN refunds r ON r.refund_id = ri.refund_id
         JOIN sales s ON s.sale_id = r.original_sale_id
         WHERE s.business_date BETWEEN ? AND ?
         GROUP BY ri.product_name_snapshot ORDER BY refund_count DESC LIMIT ?",
    )
    .bind(from)
    .bind(to)
    .bind(limit)
    .fetch_all(pool)
    .await?;
    if rows.is_empty() {
        return Ok(format!("[DB] No refund data for {from} → {to}."));
    }
    let lines: Vec<String> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            format!(
                "  {}. {} | {} refund(s) | BHD {} refunded",
                i + 1,
                s_str(r, "name"),
                s_i64(r, "refund_count"),
                fmt(s_i64(r, "refund_total"))
            )
        })
        .collect();
    Ok(format!(
        "[DB] Most-refunded products ({from} → {to}):\n{}",
        lines.join("\n")
    ))
}

// ── Compliance ─────────────────────────────────────────────────────────────────

async fn verify_receipt_sequence(
    pool: &SqlitePool,
    input: &serde_json::Value,
) -> AppResult<String> {
    let from = input
        .get("from")
        .and_then(|v| v.as_str())
        .unwrap_or("2000-01-01");
    let to = input
        .get("to")
        .and_then(|v| v.as_str())
        .unwrap_or("2999-12-31");
    // Fetch receipt numbers that are purely numeric and check for gaps
    let rows = sqlx::query(
        "SELECT receipt_number FROM sales WHERE business_date BETWEEN ? AND ?
         ORDER BY CAST(receipt_number AS INTEGER) LIMIT 5000",
    )
    .bind(from)
    .bind(to)
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        return Ok(format!("[DB] No receipts in period {from} → {to}."));
    }

    let nums: Vec<i64> = rows
        .iter()
        .filter_map(|r| s_str(r, "receipt_number").parse::<i64>().ok())
        .collect();

    if nums.is_empty() {
        return Ok("[DB] Receipt numbers are not purely numeric — sequence check skipped.".into());
    }

    let mut gaps: Vec<String> = Vec::new();
    for w in nums.windows(2) {
        if w[1] - w[0] > 1 {
            gaps.push(format!("gap between #{} and #{}", w[0], w[1]));
        }
    }

    let dupes: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM (
             SELECT receipt_number FROM sales WHERE business_date BETWEEN ? AND ?
             GROUP BY receipt_number HAVING COUNT(*) > 1
         )",
    )
    .bind(from)
    .bind(to)
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    if gaps.is_empty() && dupes == 0 {
        Ok(format!(
            "[DB] Receipt sequence OK — {} receipts checked ({from} → {to}), no gaps or duplicates.",
            nums.len()
        ))
    } else {
        Ok(format!(
            "[DB] Receipt sequence issues ({from} → {to}):\n  {} gap(s) found:\n    {}\n  {} duplicate receipt number(s)",
            gaps.len(),
            if gaps.is_empty() { "none".to_string() } else { gaps.join("\n    ") },
            dupes
        ))
    }
}

async fn audit_trail_full(pool: &SqlitePool, input: &serde_json::Value) -> AppResult<String> {
    let entity_type = input
        .get("entity_type")
        .and_then(|v| v.as_str())
        .unwrap_or("%");
    let user_id = input.get("user_id").and_then(|v| v.as_str()).unwrap_or("%");
    let from = input
        .get("from")
        .and_then(|v| v.as_str())
        .unwrap_or("2000-01-01");
    let to = input
        .get("to")
        .and_then(|v| v.as_str())
        .unwrap_or("2999-12-31");
    let limit = lim(input, 30);

    let rows = sqlx::query(
        "SELECT event_type, entity_type, entity_id, actor_user_id, created_at, after_json
         FROM audit_logs
         WHERE entity_type LIKE ? AND actor_user_id LIKE ?
           AND date(created_at) BETWEEN ? AND ?
         ORDER BY created_at DESC LIMIT ?",
    )
    .bind(entity_type)
    .bind(user_id)
    .bind(from)
    .bind(to)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    if rows.is_empty() {
        return Ok("[DB] No audit events found for that filter.".into());
    }
    let lines: Vec<String> = rows
        .iter()
        .map(|r| {
            format!(
                "  {} | {}/{} | actor: {} | {} | {}",
                s_str(r, "created_at"),
                s_str(r, "entity_type"),
                s_str(r, "entity_id"),
                s_str(r, "actor_user_id"),
                s_str(r, "event_type"),
                s_str(r, "after_json")
            )
        })
        .collect();
    Ok(format!(
        "[DB] Audit trail ({} event(s)):\n{}",
        rows.len(),
        lines.join("\n")
    ))
}

// ── Duplicate product scanner ─────────────────────────────────────────────────

async fn find_duplicate_products(
    pool: &SqlitePool,
    input: &serde_json::Value,
) -> AppResult<String> {
    let include_inactive = input
        .get("include_inactive")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let active_filter = if include_inactive {
        "p.deleted_at IS NULL"
    } else {
        "p.is_active = 1 AND p.deleted_at IS NULL"
    };

    // Name duplicates (case-insensitive)
    let name_sql = format!(
        "SELECT 'Exact name' AS match_type,
                LOWER(TRIM(p.name)) AS match_key,
                GROUP_CONCAT(p.product_id, '||') AS ids,
                GROUP_CONCAT(p.name, ' / ') AS names,
                COUNT(*) AS cnt
         FROM products p
         WHERE {active_filter}
         GROUP BY LOWER(TRIM(p.name))
         HAVING cnt > 1
         ORDER BY cnt DESC"
    );
    let name_rows = sqlx::query(&name_sql).fetch_all(pool).await?;

    // Barcode duplicates
    let bc_sql = format!(
        "SELECT 'Same barcode' AS match_type,
                p.barcode AS match_key,
                GROUP_CONCAT(p.product_id, '||') AS ids,
                GROUP_CONCAT(p.name, ' / ') AS names,
                COUNT(*) AS cnt
         FROM products p
         WHERE {active_filter} AND p.barcode IS NOT NULL AND p.barcode != ''
         GROUP BY p.barcode
         HAVING cnt > 1
         ORDER BY cnt DESC"
    );
    let bc_rows = sqlx::query(&bc_sql).fetch_all(pool).await?;

    // SKU duplicates
    let sku_sql = format!(
        "SELECT 'Same SKU' AS match_type,
                p.sku AS match_key,
                GROUP_CONCAT(p.product_id, '||') AS ids,
                GROUP_CONCAT(p.name, ' / ') AS names,
                COUNT(*) AS cnt
         FROM products p
         WHERE {active_filter} AND p.sku IS NOT NULL AND p.sku != ''
         GROUP BY p.sku
         HAVING cnt > 1
         ORDER BY cnt DESC"
    );
    let sku_rows = sqlx::query(&sku_sql).fetch_all(pool).await?;

    let total = name_rows.len() + bc_rows.len() + sku_rows.len();
    if total == 0 {
        return Ok("[DB] No duplicate products found. Your catalog looks clean!".into());
    }

    let mut out = format!("[DB] Found {total} duplicate group(s):\n");

    for (group, r) in (1usize..).zip(
        name_rows
            .iter()
            .chain(bc_rows.iter())
            .chain(sku_rows.iter()),
    ) {
        let match_type: String = r.try_get("match_type").unwrap_or_default();
        let match_key: String = r.try_get("match_key").unwrap_or_default();
        let ids: String = r.try_get("ids").unwrap_or_default();
        let names: String = r.try_get("names").unwrap_or_default();
        let cnt: i64 = r.try_get("cnt").unwrap_or(0);

        out.push_str(&format!(
            "\nGroup {group} — {match_type}: \"{match_key}\" ({cnt} products)\n"
        ));
        for (id, name) in ids.split("||").zip(names.split(" / ")) {
            out.push_str(&format!("  • {name} (ID: {id})\n"));
        }
        out.push_str(
            "  → To merge: merge_products(source_product_id=\"<duplicate_id>\", target_product_id=\"<keep_id>\")\n",
        );
    }

    Ok(out)
}

async fn load_workflow(input: &serde_json::Value) -> AppResult<String> {
    let name = input
        .get("workflow_name")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let text = crate::ai::workflows::get_workflow(name).unwrap_or(
        "Unknown workflow. Available: whatsapp_message, ghost_barcode, low_stock_restock, delivery_lifecycle, cash_discrepancy, sync_recovery, eod_reconciliation, db_maintenance, proactive_alerts, daily_briefing, supplier_invoice, customer_message, bulk_operations",
    );
    Ok(format!("[Workflow: {name}]\n\n{text}"))
}
