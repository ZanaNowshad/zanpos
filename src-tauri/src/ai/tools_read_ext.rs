//! Extension read tools — 15 additional read-only tools that dispatch from
//! `execute_read_tool` in tools.rs via its catch-all arm.

use crate::ai::tools::mask_phone;
use crate::db::repositories::{delivery_repo, held_cart_repo, refund_repo};
use crate::domain::money;
use crate::errors::{AppError, AppResult};
use sqlx::{Row, SqlitePool};

// ── Helpers ────────────────────────────────────────────────────────────────────

async fn active_branch(pool: &SqlitePool) -> AppResult<String> {
    sqlx::query_scalar(
        "SELECT branch_id FROM branches WHERE is_active = 1 ORDER BY created_at LIMIT 1",
    )
    .fetch_optional(pool)
    .await?
    .flatten()
    .ok_or_else(|| AppError::NotFound("No active branch configured".into()))
}

async fn active_device(pool: &SqlitePool) -> String {
    sqlx::query_scalar(
        "SELECT value FROM app_config WHERE key = 'device_id'",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .flatten()
    .unwrap_or_default()
}

fn sidecar_token() -> String {
    let app_data = std::env::var("APPDATA").unwrap_or_default();
    let path = std::path::Path::new(&app_data)
        .join("com.super.zanpos")
        .join("wa-session")
        .join(".sidecar_token");
    std::fs::read_to_string(path).unwrap_or_default().trim().to_string()
}

fn cfg_val<'a>(pool: &'a SqlitePool, key: &'a str) -> impl std::future::Future<Output = Option<String>> + 'a {
    let key = key.to_string();
    async move {
        sqlx::query_scalar("SELECT value FROM app_config WHERE key = ?")
            .bind(&key)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten()
            .flatten()
    }
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
        "get_sales_list"        => sales_list(pool, input, &fmt).await,
        "get_sale_detail"       => sale_detail(pool, input, &fmt).await,
        "get_z_report"          => z_report(pool, input, &fmt).await,
        "get_eod_cashup"        => eod_cashup(pool, input, &fmt).await,
        "get_x_report"          => x_report(pool, input, &fmt).await,
        "get_product_barcodes"  => product_barcodes(pool, input).await,
        "get_whatsapp_status"   => whatsapp_status().await,
        "get_branch_settings"   => branch_settings(pool).await,
        "get_supabase_status"   => supabase_status(pool).await,
        "get_held_carts"        => held_carts(pool, &fmt).await,
        "get_db_integrity"      => db_integrity(pool).await,
        "get_thermal_config"    => thermal_config(pool).await,
        "get_delivery_detail"   => delivery_detail(pool, input, &fmt).await,
        "get_rider_suggestions" => rider_suggestions(pool).await,
        "get_sync_queue_stats"  => sync_queue_stats(pool).await,
        other => Err(AppError::Validation(format!("Unknown read tool: {other}"))),
    }
}

// ── Implementations ────────────────────────────────────────────────────────────

async fn sales_list(pool: &SqlitePool, input: &serde_json::Value, fmt: &impl Fn(i64) -> String) -> AppResult<String> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let from = input.get("date_from").and_then(|v| v.as_str()).unwrap_or(&today).to_string();
    let to   = input.get("date_to").and_then(|v| v.as_str()).unwrap_or(&today).to_string();
    let limit = input.get("limit").and_then(|v| v.as_i64()).unwrap_or(50).clamp(1, 200);
    let branch = active_branch(pool).await?;

    let rows = sqlx::query(
        "SELECT s.receipt_number, s.sold_at, s.net_total_minor, s.status,
                COALESCE(u.display_name,'(deleted)') AS cashier_name,
                GROUP_CONCAT(DISTINCT p.payment_method) AS payment_methods
         FROM sales s
         LEFT JOIN users u ON u.user_id = s.cashier_user_id
         LEFT JOIN payments p ON p.sale_id = s.sale_id
         WHERE s.branch_id = ? AND s.business_date BETWEEN ? AND ?
         GROUP BY s.sale_id ORDER BY s.sold_at DESC LIMIT ?",
    )
    .bind(&branch).bind(&from).bind(&to).bind(limit)
    .fetch_all(pool).await?;

    if rows.is_empty() {
        return Ok(format!("[DB] No sales found for {} → {}.", from, to));
    }
    let lines: Vec<String> = rows.iter().map(|r| {
        let rcpt: String = r.get("receipt_number");
        let total: i64   = r.get("net_total_minor");
        let cashier: String = r.get("cashier_name");
        let status: String  = r.get("status");
        let methods: Option<String> = r.get("payment_methods");
        format!("  #{rcpt} | BHD {} | {} | {} | {}", fmt(total), cashier, methods.unwrap_or_default(), status)
    }).collect();
    Ok(format!("[DB] {} sale(s) {} → {}:\n{}", rows.len(), from, to, lines.join("\n")))
}

async fn sale_detail(pool: &SqlitePool, input: &serde_json::Value, fmt: &impl Fn(i64) -> String) -> AppResult<String> {
    let receipt = input.get("receipt_number").and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("receipt_number required".into()))?;
    let sale = refund_repo::get_sale_by_receipt(pool, receipt).await?;
    let items: Vec<String> = sale.items.iter().map(|i| {
        format!("    {} × {} @ BHD {} = BHD {}", i.quantity, i.product_name_snapshot,
            fmt(i.unit_price_minor), fmt(i.line_total_minor))
    }).collect();
    Ok(format!(
        "[DB] Sale #{} — BHD {} — {} — {}\nCashier: {}\nItems:\n{}",
        sale.receipt_number, fmt(sale.net_total_minor), sale.sold_at, sale.status,
        sale.cashier_name, items.join("\n")
    ))
}

async fn z_report(pool: &SqlitePool, input: &serde_json::Value, fmt: &impl Fn(i64) -> String) -> AppResult<String> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let date = input.get("date").and_then(|v| v.as_str()).unwrap_or(&today).to_string();
    let branch = active_branch(pool).await?;

    let (tx_count, net_total, discount_total, tax_total): (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT COUNT(DISTINCT s.sale_id), COALESCE(SUM(s.net_total_minor),0),
                COALESCE(SUM(s.discount_total_minor),0), COALESCE(SUM(s.tax_total_minor),0)
         FROM sales s WHERE s.branch_id = ? AND s.business_date = ? AND s.status != 'voided'"
    ).bind(&branch).bind(&date).fetch_one(pool).await?;

    let pay_rows = sqlx::query(
        "SELECT p.payment_method, COALESCE(SUM(p.amount_minor),0) AS total
         FROM payments p JOIN sales s ON s.sale_id = p.sale_id
         WHERE s.branch_id = ? AND s.business_date = ? AND s.status != 'voided'
         GROUP BY p.payment_method"
    ).bind(&branch).bind(&date).fetch_all(pool).await?;

    let (refund_count, refund_total): (i64, i64) = sqlx::query_as(
        "SELECT COUNT(r.refund_id), COALESCE(SUM(r.refund_total_minor),0)
         FROM refunds r JOIN sales s ON s.sale_id = r.original_sale_id
         WHERE s.branch_id = ? AND s.business_date = ?"
    ).bind(&branch).bind(&date).fetch_one(pool).await?;

    let pay_lines: Vec<String> = pay_rows.iter().map(|r| {
        let method: String = r.get("payment_method");
        let total: i64 = r.get("total");
        format!("  {}: BHD {}", method, fmt(total))
    }).collect();

    Ok(format!(
        "[DB] Z-Report — {date}\nTransactions: {tx_count}\nNet Revenue: BHD {}\nDiscounts: BHD {}\nTax: BHD {}\nRefunds: {refund_count} (BHD {})\nPayments:\n{}",
        fmt(net_total), fmt(discount_total), fmt(tax_total), fmt(refund_total),
        if pay_lines.is_empty() { "  (no payments)".into() } else { pay_lines.join("\n") }
    ))
}

async fn eod_cashup(pool: &SqlitePool, input: &serde_json::Value, fmt: &impl Fn(i64) -> String) -> AppResult<String> {
    // Alias to z_report — same data, different label
    z_report(pool, input, fmt).await.map(|s| s.replacen("Z-Report", "EOD Cashup", 1))
}

async fn x_report(pool: &SqlitePool, input: &serde_json::Value, fmt: &impl Fn(i64) -> String) -> AppResult<String> {
    let shift_id = input.get("shift_id").and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("shift_id required".into()))?;

    let shift = sqlx::query(
        "SELECT opening_cash_minor, counted_cash_minor FROM shifts WHERE shift_id = ?"
    ).bind(shift_id).fetch_optional(pool).await?
     .ok_or_else(|| AppError::NotFound("Shift not found".into()))?;
    let opening: i64 = shift.get("opening_cash_minor");
    let counted: Option<i64> = shift.get("counted_cash_minor");

    let cash_sales: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(p.amount_minor),0) FROM payments p
         JOIN sales s ON s.sale_id = p.sale_id
         WHERE s.shift_id = ? AND p.payment_method = 'cash' AND s.status != 'voided'
           AND (s.is_delivery = 0 OR EXISTS (
               SELECT 1 FROM delivery_orders d WHERE d.sale_id = s.sale_id AND d.payment_status = 'paid'
           ))"
    ).bind(shift_id).fetch_one(pool).await?;

    // Proportional cash refunds: only the cash-portion of refunds on split-payment sales.
    let cash_refunds: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(
            CASE WHEN s.net_total_minor <= 0 THEN 0
            ELSE MIN(
                (SELECT COALESCE(SUM(p2.amount_minor), 0)
                 FROM payments p2
                 WHERE p2.sale_id = s.sale_id AND p2.payment_method = 'cash'),
                s.net_total_minor
            ) * r.refund_total_minor / s.net_total_minor
            END
        ), 0)
         FROM refunds r
         JOIN sales s ON s.sale_id = r.original_sale_id
         WHERE s.shift_id = ?"
    ).bind(shift_id).fetch_one(pool).await?;

    let paid_in: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(amount_minor),0) FROM cash_events WHERE shift_id = ? AND event_type = 'paid_in'"
    ).bind(shift_id).fetch_one(pool).await?;
    let paid_out: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(amount_minor),0) FROM cash_events WHERE shift_id = ? AND event_type = 'paid_out'"
    ).bind(shift_id).fetch_one(pool).await?;
    let safe_drop: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(amount_minor),0) FROM cash_events WHERE shift_id = ? AND event_type = 'safe_drop'"
    ).bind(shift_id).fetch_one(pool).await?;

    let expected = opening + cash_sales - cash_refunds + paid_in - paid_out - safe_drop;
    let variance = counted.map(|c| c - expected);

    Ok(format!(
        "[DB] X-Report — Shift {shift_id}\nOpening: BHD {}\nCash Sales: BHD {}\nCash Refunds: BHD {}\nPaid In: BHD {}\nPaid Out: BHD {}\nSafe Drop: BHD {}\nExpected: BHD {}\nCounted: {}\nVariance: {}",
        fmt(opening), fmt(cash_sales), fmt(cash_refunds), fmt(paid_in), fmt(paid_out), fmt(safe_drop), fmt(expected),
        counted.map_or("(not counted)".into(), |c| format!("BHD {}", fmt(c))),
        variance.map_or("(not counted)".into(), |v| format!("BHD {}", fmt(v)))
    ))
}

async fn product_barcodes(pool: &SqlitePool, input: &serde_json::Value) -> AppResult<String> {
    let pid = input.get("product_id").and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("product_id required".into()))?;
    let rows = sqlx::query(
        "SELECT barcode_id, barcode, created_at FROM product_barcodes WHERE product_id = ? ORDER BY created_at"
    ).bind(pid).fetch_all(pool).await?;
    if rows.is_empty() {
        return Ok(format!("[DB] No extra barcodes registered for product {pid}."));
    }
    let lines: Vec<String> = rows.iter().map(|r| {
        let id: String = r.get("barcode_id"); let bc: String = r.get("barcode");
        format!("  {bc} (id: {id})")
    }).collect();
    Ok(format!("[DB] {} barcode(s) for product {pid}:\n{}", rows.len(), lines.join("\n")))
}

async fn whatsapp_status() -> AppResult<String> {
    let token = sidecar_token();
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(10))
        .build().unwrap_or_default();
    match client.get("http://127.0.0.1:3131/status")
        .header("X-Sidecar-Token", &token).send().await
    {
        Ok(resp) => {
            let json: serde_json::Value = resp.json().await.unwrap_or_default();
            let connected = json.get("connected").and_then(|v| v.as_bool()).unwrap_or(false);
            Ok(format!("[WA] Sidecar running. Connected: {}.", if connected { "yes ✓" } else { "no — scan QR code to pair" }))
        }
        Err(_) => Ok("[WA] WhatsApp sidecar is not running. Start it from WhatsApp Settings.".into()),
    }
}

async fn branch_settings(pool: &SqlitePool) -> AppResult<String> {
    let row = sqlx::query(
        "SELECT name, branch_code, currency, timezone, address, phone, tax_number, cr_number,
                receipt_header, receipt_footer FROM branches WHERE is_active = 1 LIMIT 1"
    ).fetch_optional(pool).await?
     .ok_or_else(|| AppError::NotFound("No active branch".into()))?;

    let name: String = row.get("name");
    let code: String = row.get("branch_code");
    let currency: String = row.get("currency");
    let tz: String = row.get("timezone");
    let address: Option<String> = row.get("address");
    let phone: Option<String> = row.get("phone");
    let tax: Option<String> = row.get("tax_number");
    let cr: Option<String> = row.get("cr_number");
    let header: Option<String> = row.get("receipt_header");
    let footer: Option<String> = row.get("receipt_footer");

    Ok(format!(
        "[DB] Branch: {name} ({code}) | {currency} | {tz}\nAddress: {}\nPhone: {}\nVAT/Tax: {}\nCR: {}\nReceipt Header: {}\nReceipt Footer: {}",
        address.as_deref().unwrap_or("—"), phone.as_deref().unwrap_or("—"),
        tax.as_deref().unwrap_or("—"), cr.as_deref().unwrap_or("—"),
        header.as_deref().unwrap_or("—"), footer.as_deref().unwrap_or("—")
    ))
}

async fn supabase_status(pool: &SqlitePool) -> AppResult<String> {
    let url: Option<String> = cfg_val(pool, "supabase_url").await;
    let has_key = crate::secure_store::get_secret("supabase_service_key")
        .map_or(false, |k| !k.is_empty());
    let pending: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM products WHERE sync_status = 'pending'"
    ).fetch_one(pool).await.unwrap_or(0);
    let last_sync: Option<String> = cfg_val(pool, "last_sync_at").await;

    Ok(format!(
        "[DB] Supabase URL: {}\nKey configured: {}\nPending rows (products sample): {}\nLast sync: {}",
        url.as_deref().unwrap_or("(not configured)"),
        if has_key { "yes" } else { "no" },
        pending,
        last_sync.as_deref().unwrap_or("(never)")
    ))
}

async fn held_carts(pool: &SqlitePool, fmt: &impl Fn(i64) -> String) -> AppResult<String> {
    let device = active_device(pool).await;
    let carts = held_cart_repo::list_held_carts(pool, &device).await?;
    if carts.is_empty() {
        return Ok("[DB] No held/parked carts on this device.".into());
    }
    let lines: Vec<String> = carts.iter().map(|c| format!(
        "  {} | {} item(s) | BHD {} | {}{}",
        c.held_cart_id, c.line_count, fmt(c.estimated_total_minor),
        c.held_at, c.note.as_deref().map(|n| format!(" | Note: {n}")).unwrap_or_default()
    )).collect();
    Ok(format!("[DB] {} held cart(s):\n{}", carts.len(), lines.join("\n")))
}

async fn db_integrity(pool: &SqlitePool) -> AppResult<String> {
    let result: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(pool).await?;
    Ok(format!("[DB] SQLite integrity_check: {result}"))
}

async fn thermal_config(pool: &SqlitePool) -> AppResult<String> {
    let enabled = cfg_val(pool, "thermal_printer_enabled").await.unwrap_or_default();
    let port    = cfg_val(pool, "thermal_printer_port").await.unwrap_or_default();
    let baud    = cfg_val(pool, "thermal_printer_baud").await.unwrap_or("9600".into());
    Ok(format!(
        "[DB] Thermal printer — enabled: {} | port: {} | baud: {}",
        if enabled == "1" { "yes" } else { "no" },
        if port.is_empty() { "(not set)" } else { &port }, baud
    ))
}

async fn delivery_detail(pool: &SqlitePool, input: &serde_json::Value, fmt: &impl Fn(i64) -> String) -> AppResult<String> {
    let id = input.get("delivery_id").and_then(|v| v.as_str())
        .ok_or_else(|| AppError::Validation("delivery_id required".into()))?;
    let d = delivery_repo::get_delivery(pool, id).await?;
    Ok(format!(
        "[DB] Delivery {}\nReceipt: #{}\nCustomer: {} | Phone: {}\nAddress: {}\nAmount: BHD {} | Payment: {} | Status: {}\nCreated: {}",
        d.delivery_id, d.receipt_number,
        d.customer_name.as_deref().unwrap_or("—"), mask_phone(&d.contact_number),
        d.address_text, fmt(d.amount_minor), d.payment_status, d.delivery_status, d.created_at
    ))
}

async fn rider_suggestions(pool: &SqlitePool) -> AppResult<String> {
    let branch = active_branch(pool).await?;
    let names = delivery_repo::rider_suggestions(pool, &branch).await?;
    if names.is_empty() {
        return Ok("[DB] No rider name suggestions yet (based on past deliveries).".into());
    }
    Ok(format!("[DB] Suggested riders: {}", names.join(", ")))
}

async fn sync_queue_stats(pool: &SqlitePool) -> AppResult<String> {
    let tables = crate::commands::sync_commands::SYNC_TABLES;
    let mut lines = Vec::new();
    for table in tables {
        let pending: i64 = sqlx::query_scalar(
            &format!("SELECT COUNT(*) FROM {table} WHERE sync_status = 'pending'")
        ).fetch_one(pool).await.unwrap_or(0);
        let failed: i64 = sqlx::query_scalar(
            &format!("SELECT COUNT(*) FROM {table} WHERE sync_status = 'pending' AND sync_attempts >= 10")
        ).fetch_one(pool).await.unwrap_or(0);
        if pending > 0 || failed > 0 {
            lines.push(format!("  {table}: {pending} pending, {failed} stuck"));
        }
    }
    if lines.is_empty() {
        Ok("[DB] All sync queues are clear — nothing pending.".into())
    } else {
        Ok(format!("[DB] Sync queue stats:\n{}", lines.join("\n")))
    }
}
