use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};
use crate::ai::client::ToolDef;
use crate::domain::ai_admin::{ToolPreview, ToolPreviewField};
use crate::errors::{AppError, AppResult};
use crate::db::repositories::{product_repo, report_repo, sync_repo};
use crate::domain::money;
use crate::sync::outbox;
use crate::inventory::{stock_repo, movements};

// ── Tool catalogue ─────────────────────────────────────────────────────────────

pub fn all_tool_definitions() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "get_today_summary".into(),
            description: "Get today's sales summary including totals, transaction count, and payment breakdown.".into(),
            input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
        },
        ToolDef {
            name: "list_products".into(),
            description: "List all active products with their current prices.".into(),
            input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
        },
        ToolDef {
            name: "search_products".into(),
            description: "Search products by name, SKU, or barcode.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search term" }
                },
                "required": ["query"]
            }),
        },
        ToolDef {
            name: "get_product".into(),
            description: "Get full details of a specific product by ID.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "product_id": { "type": "string" }
                },
                "required": ["product_id"]
            }),
        },
        ToolDef {
            name: "update_product_price".into(),
            description: "Update the selling price of a product. Requires admin confirmation.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "product_id": { "type": "string" },
                    "new_price_minor": { "type": "integer", "description": "New price in minor units (e.g. 1500 = BHD 1.500)" },
                    "reason": { "type": "string", "description": "Reason for price change" }
                },
                "required": ["product_id", "new_price_minor"]
            }),
        },
        ToolDef {
            name: "set_product_active".into(),
            description: "Enable or disable a product. Disabled products don't appear in POS. Requires admin confirmation.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "product_id": { "type": "string" },
                    "is_active": { "type": "boolean" }
                },
                "required": ["product_id", "is_active"]
            }),
        },
        ToolDef {
            name: "update_product_name".into(),
            description: "Rename a product. Requires admin confirmation.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "product_id": { "type": "string" },
                    "new_name": { "type": "string" }
                },
                "required": ["product_id", "new_name"]
            }),
        },
        // ── Inventory tools ───────────────────────────────────────────────────
        ToolDef {
            name: "get_stock_levels".into(),
            description: "List all inventory-tracked products with their current stock quantity, reorder point, and low-stock status.".into(),
            input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
        },
        ToolDef {
            name: "get_low_stock".into(),
            description: "List only the products that are at or below their reorder point (low stock or out of stock).".into(),
            input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
        },
        ToolDef {
            name: "adjust_stock".into(),
            description: "Apply a positive or negative quantity adjustment to a product's stock. Use for corrections, write-offs, or manual receives. Requires admin confirmation.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "product_id": { "type": "string" },
                    "quantity_delta": { "type": "number", "description": "Amount to add (positive) or remove (negative)" },
                    "notes": { "type": "string", "description": "Reason for adjustment" }
                },
                "required": ["product_id", "quantity_delta"]
            }),
        },
        ToolDef {
            name: "stock_take".into(),
            description: "Set a product's stock to an exact counted quantity (full stock take). Requires admin confirmation.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "product_id": { "type": "string" },
                    "new_quantity": { "type": "number", "description": "The counted quantity on hand" },
                    "notes": { "type": "string", "description": "Optional notes" }
                },
                "required": ["product_id", "new_quantity"]
            }),
        },
        ToolDef {
            name: "get_cash_summary".into(),
            description: "Get the current cash drawer reconciliation for the active shift: opening float, cash sales, refunds, paid-in/out, safe drops, expected total, and counted total if entered.".into(),
            input_schema: json!({ "type": "object", "properties": { "shift_id": { "type": "string" } }, "required": ["shift_id"] }),
        },
        ToolDef {
            name: "get_recent_refunds".into(),
            description: "List the most recent refunds (up to 20). Shows refund ID, original sale, amount, reason, and date.".into(),
            input_schema: json!({ "type": "object", "properties": { "limit": { "type": "integer", "description": "Max refunds to return (default 10, max 20)" } }, "required": [] }),
        },
        ToolDef {
            name: "get_audit_log".into(),
            description: "Retrieve recent audit log entries for today. Useful for reviewing cashier actions, voids, and refunds.".into(),
            input_schema: json!({ "type": "object", "properties": { "event_type": { "type": "string", "description": "Optional filter by event type, e.g. sale.created, sale.voided, CART_VOID, NO_SALE, X_REPORT, refund.created" } }, "required": [] }),
        },
        ToolDef {
            name: "get_sync_status".into(),
            description: "Check the cloud sync status: whether Supabase is configured, last sync time, pending queue count, and any failed or conflicted events.".into(),
            input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
        },
        // ── Extended analytics & audit tools ──────────────────────────────────
        ToolDef {
            name: "get_daily_report".into(),
            description: "Get sales summary for a specific date (YYYY-MM-DD). Use for historical reports or comparing days.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "date": { "type": "string", "description": "Date in YYYY-MM-DD format" }
                },
                "required": ["date"]
            }),
        },
        ToolDef {
            name: "get_date_range_report".into(),
            description: "Get aggregated sales summary for a date range. Both dates inclusive. Max 90 days.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "from": { "type": "string", "description": "Start date YYYY-MM-DD (inclusive)" },
                    "to":   { "type": "string", "description": "End date YYYY-MM-DD (inclusive)" }
                },
                "required": ["from", "to"]
            }),
        },
        ToolDef {
            name: "get_top_products".into(),
            description: "List top-selling products by revenue for the last N days. Use to see bestsellers.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "limit":       { "type": "integer", "description": "How many products to return (default 10, max 25)" },
                    "period_days": { "type": "integer", "description": "Lookback window in days (default 30)" }
                },
                "required": []
            }),
        },
        ToolDef {
            name: "get_shift_history".into(),
            description: "List recent cashier shifts with open/close times, opening float, cashier name, and total sales.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "description": "Number of shifts to return (default 10, max 30)" }
                },
                "required": []
            }),
        },
        ToolDef {
            name: "list_categories".into(),
            description: "List all product categories with their IDs, names, and colors.".into(),
            input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
        },
        ToolDef {
            name: "list_safe_drops".into(),
            description: "List safe drop events for a shift (cash physically removed from drawer for security).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "shift_id": { "type": "string", "description": "The shift ID to query" }
                },
                "required": ["shift_id"]
            }),
        },
        ToolDef {
            name: "list_no_sale_events".into(),
            description: "List no-sale (drawer opened without a transaction) events for a shift. Key audit signal.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "shift_id": { "type": "string", "description": "The shift ID to query" }
                },
                "required": ["shift_id"]
            }),
        },
        ToolDef {
            name: "get_audit_chain_status".into(),
            description: "Verify the SHA-256 hash chain integrity for audit logs on this device. Detects tampering or data loss.".into(),
            input_schema: json!({ "type": "object", "properties": {}, "required": [] }),
        },
        // ── Mutation: create product ──────────────────────────────────────────
        ToolDef {
            name: "create_product".into(),
            description: "Create a new product in the catalog with a name, price, and category. Requires admin confirmation.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name":        { "type": "string",  "description": "Product display name" },
                    "price_minor": { "type": "integer", "description": "Selling price in minor currency units (e.g. 1500 = BHD 1.500)" },
                    "category_id": { "type": "string",  "description": "Category ID — use list_categories to get valid IDs" },
                    "sku":         { "type": "string",  "description": "Optional SKU / product code" },
                    "barcode":     { "type": "string",  "description": "Optional barcode (EAN/UPC)" }
                },
                "required": ["name", "price_minor", "category_id"]
            }),
        },
    ]
}

pub fn is_mutation_tool(name: &str) -> bool {
    matches!(name,
        "update_product_price" | "set_product_active" | "update_product_name"
        | "adjust_stock" | "stock_take" | "create_product"
    )
}

// ── Read-only tool executor ────────────────────────────────────────────────────

pub async fn execute_read_tool(
    pool: &SqlitePool,
    tool_name: &str,
    input: &Value,
    branch_id: &str,
    currency_exp: u32,
) -> AppResult<String> {
    match tool_name {
        "get_today_summary" => {
            let today = chrono::Local::now().format("%Y-%m-%d").to_string();
            let s = report_repo::today_summary(pool, branch_id, &today).await?;
            let fmt = |n: i64| money::format_minor(n, currency_exp);
            Ok(format!(
                "Today ({}):\n- Transactions: {}\n- Net Total: BHD {}\n- Tax: BHD {}\n- Discounts: BHD {}\n- Cash: BHD {}\n- Card: BHD {}\n- Refunds: {} (BHD {})",
                s.business_date, s.transaction_count,
                fmt(s.net_total_minor), fmt(s.tax_total_minor), fmt(s.discount_total_minor),
                fmt(s.cash_total_minor), fmt(s.card_total_minor),
                s.refund_count, fmt(s.refund_total_minor)
            ))
        }
        "list_products" => {
            let products = product_repo::list_all_active(pool).await?;
            if products.is_empty() {
                return Ok("No active products found.".into());
            }
            let fmt = |n: i64| money::format_minor(n, currency_exp);
            let lines: Vec<String> = products.iter().map(|p| {
                format!("- {} (ID: {}) — BHD {} — {}",
                    p.product.name, p.product.product_id,
                    fmt(p.price_minor), p.category_name)
            }).collect();
            Ok(format!("{} active products:\n{}", products.len(), lines.join("\n")))
        }
        "search_products" => {
            let query = input.get("query").and_then(|v| v.as_str()).unwrap_or("");
            let products = product_repo::search_products(pool, query, 20).await?;
            if products.is_empty() {
                return Ok(format!("No products found matching '{}'.", query));
            }
            let fmt = |n: i64| money::format_minor(n, currency_exp);
            let lines: Vec<String> = products.iter().map(|p| {
                format!("- {} (ID: {}) — BHD {}", p.product.name, p.product.product_id, fmt(p.price_minor))
            }).collect();
            Ok(format!("{} results for '{}':\n{}", products.len(), query, lines.join("\n")))
        }
        "get_product" => {
            let product_id = input.get("product_id").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let p = product_repo::get_product_by_id(pool, product_id).await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;
            let fmt = |n: i64| money::format_minor(n, currency_exp);
            Ok(format!(
                "Product: {}\nID: {}\nSKU: {}\nBarcode: {}\nCategory: {}\nPrice: BHD {}\nActive: {}\nTrack Inventory: {}",
                p.product.name, p.product.product_id,
                p.product.sku.as_deref().unwrap_or("—"),
                p.product.barcode.as_deref().unwrap_or("—"),
                p.category_name,
                fmt(p.price_minor),
                p.product.is_active,
                p.product.track_inventory
            ))
        }
        "get_stock_levels" => {
            let levels = stock_repo::get_all_levels(pool).await?;
            if levels.is_empty() {
                return Ok("No inventory-tracked products found.".into());
            }
            let lines: Vec<String> = levels.iter().map(|s| {
                let status = if s.is_out_of_stock { "❌ OUT" }
                    else if s.is_low_stock { "⚠ LOW" }
                    else { "✓" };
                format!("- {} (ID: {}) — qty: {} | reorder ≤{} {status}",
                    s.product_name, s.product_id, s.quantity_on_hand, s.reorder_point)
            }).collect();
            Ok(format!("{} tracked products:\n{}", levels.len(), lines.join("\n")))
        }
        "get_low_stock" => {
            let levels = stock_repo::get_low_stock(pool).await?;
            if levels.is_empty() {
                return Ok("All products are above their reorder points. 🎉".into());
            }
            let lines: Vec<String> = levels.iter().map(|s| {
                let status = if s.is_out_of_stock { "OUT OF STOCK" } else { "LOW STOCK" };
                format!("- {} — qty: {} | reorder ≤{} [{status}]",
                    s.product_name, s.quantity_on_hand, s.reorder_point)
            }).collect();
            Ok(format!("{} product(s) need restocking:\n{}", levels.len(), lines.join("\n")))
        }
        "get_cash_summary" => {
            let shift_id = input.get("shift_id").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing shift_id".into()))?;
            let fmt = |n: i64| money::format_minor(n, currency_exp);

            let shift = sqlx::query(
                "SELECT opening_cash_minor, counted_cash_minor FROM shifts WHERE shift_id = ?"
            )
            .bind(shift_id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Shift not found".into()))?;

            let opening: i64          = shift.get("opening_cash_minor");
            let counted: Option<i64>  = shift.get("counted_cash_minor");

            let cash_sales: i64 = sqlx::query_scalar(
                "SELECT COALESCE(SUM(p.amount_minor),0) FROM payments p
                 JOIN sales s ON s.sale_id=p.sale_id
                 WHERE s.shift_id=? AND p.payment_method='cash' AND s.status!='voided'"
            ).bind(shift_id).fetch_one(pool).await?;

            let cash_refunds: i64 = sqlx::query_scalar(
                "SELECT COALESCE(SUM(r.refund_total_minor),0) FROM refunds r
                 JOIN sales s ON s.sale_id=r.original_sale_id WHERE s.shift_id=?"
            ).bind(shift_id).fetch_one(pool).await?;

            let paid_in: i64  = sqlx::query_scalar(
                "SELECT COALESCE(SUM(amount_minor),0) FROM cash_events WHERE shift_id=? AND event_type='paid_in'"
            ).bind(shift_id).fetch_one(pool).await?;
            let paid_out: i64 = sqlx::query_scalar(
                "SELECT COALESCE(SUM(amount_minor),0) FROM cash_events WHERE shift_id=? AND event_type='paid_out'"
            ).bind(shift_id).fetch_one(pool).await?;
            let safe_drop: i64 = sqlx::query_scalar(
                "SELECT COALESCE(SUM(amount_minor),0) FROM cash_events WHERE shift_id=? AND event_type='safe_drop'"
            ).bind(shift_id).fetch_one(pool).await?;

            let expected = opening + cash_sales - cash_refunds + paid_in - paid_out - safe_drop;
            let variance = counted.map(|c| c - expected);

            let mut lines = vec![
                format!("Cash Drawer — Shift {shift_id}"),
                format!("  Opening float:  {}", fmt(opening)),
                format!("  Cash sales:     +{}", fmt(cash_sales)),
                format!("  Cash refunds:   -{}", fmt(cash_refunds)),
                format!("  Paid in:        +{}", fmt(paid_in)),
                format!("  Paid out:       -{}", fmt(paid_out)),
                format!("  Safe drops:     -{}", fmt(safe_drop)),
                format!("  Expected:       {}", fmt(expected)),
            ];
            if let Some(c) = counted {
                let v = variance.unwrap_or(0);
                lines.push(format!("  Counted:        {}", fmt(c)));
                lines.push(format!("  Variance:       {} {}", if v >= 0 { "+" } else { "" }, fmt(v)));
            } else {
                lines.push("  Counted:        (not yet entered)".into());
            }
            Ok(lines.join("\n"))
        }
        "get_recent_refunds" => {
            let limit = input.get("limit").and_then(|v| v.as_i64()).unwrap_or(10).min(20);
            let fmt = |n: i64| money::format_minor(n, currency_exp);
            let rows = sqlx::query(
                "SELECT r.refund_id, r.original_sale_id, r.refund_total_minor,
                        r.reason, r.return_reason_code, r.created_at
                 FROM refunds r ORDER BY r.created_at DESC LIMIT ?"
            )
            .bind(limit)
            .fetch_all(pool)
            .await?;

            if rows.is_empty() { return Ok("No refunds found.".into()); }
            let lines: Vec<String> = rows.iter().map(|r| {
                let total: i64         = r.get("refund_total_minor");
                let id: String         = r.get("refund_id");
                let sale: String       = r.get("original_sale_id");
                let reason: Option<String> = r.get("reason");
                let code: Option<String>   = r.get("return_reason_code");
                let at: String         = r.get("created_at");
                format!("- {} | sale {} | {} | {} [{}] | {}",
                    &id[..8.min(id.len())], &sale[..8.min(sale.len())],
                    fmt(total),
                    reason.as_deref().unwrap_or("—"),
                    code.as_deref().unwrap_or("other"),
                    &at[..10])
            }).collect();
            Ok(format!("{} recent refund(s):\n{}", rows.len(), lines.join("\n")))
        }
        "get_audit_log" => {
            let event_filter = input.get("event_type").and_then(|v| v.as_str());
            let today = chrono::Local::now().format("%Y-%m-%d").to_string();
            let rows = if let Some(et) = event_filter {
                sqlx::query(
                    "SELECT audit_log_id, event_type, entity_type, entity_id,
                            actor_user_id, created_at
                     FROM audit_logs
                     WHERE event_type = ? AND created_at >= ?
                     ORDER BY created_at DESC LIMIT 30"
                )
                .bind(et).bind(&today).fetch_all(pool).await?
            } else {
                sqlx::query(
                    "SELECT audit_log_id, event_type, entity_type, entity_id,
                            actor_user_id, created_at
                     FROM audit_logs
                     WHERE created_at >= ?
                     ORDER BY created_at DESC LIMIT 30"
                )
                .bind(&today).fetch_all(pool).await?
            };

            if rows.is_empty() { return Ok("No audit log entries found for today.".into()); }
            let lines: Vec<String> = rows.iter().map(|r| {
                let id: String          = r.get("audit_log_id");
                let et: String          = r.get("event_type");
                let eid: Option<String> = r.get("entity_id");
                let actor: Option<String> = r.get("actor_user_id");
                let at: String          = r.get("created_at");
                format!("- {} | {} | entity: {} | actor: {} | {}",
                    &id[..8.min(id.len())], et,
                    &eid.as_deref().unwrap_or("—")[..8.min(eid.as_deref().unwrap_or("—").len())],
                    actor.as_deref().unwrap_or("system"),
                    &at[11..19.min(at.len())])
            }).collect();
            Ok(format!("{} audit entries today:\n{}", rows.len(), lines.join("\n")))
        }
        "get_sync_status" => {
            // Fetch device_id from the active device
            let device_id: String = sqlx::query_scalar(
                "SELECT device_id FROM devices WHERE is_active=1 ORDER BY device_code LIMIT 1"
            ).fetch_optional(pool).await?.flatten().unwrap_or_default();

            let status = sync_repo::get_sync_status(pool, &device_id).await?;
            let pending: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM sync_queue WHERE status='pending'"
            ).fetch_one(pool).await?;
            let failed: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM sync_queue WHERE status='failed'"
            ).fetch_one(pool).await?;
            let conflict: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM sync_queue WHERE status='conflict'"
            ).fetch_one(pool).await?;

            let cloud = if status.supabase_configured { "✓ configured" } else { "✗ not configured" };
            let last = status.last_successful_sync_at.as_deref().unwrap_or("never");
            let mut lines = vec![
                format!("Sync Status:"),
                format!("  Cloud (Supabase): {cloud}"),
                format!("  Last sync:        {last}"),
                format!("  Pending events:   {pending}"),
                format!("  Failed events:    {failed}"),
                format!("  Conflicts:        {conflict}"),
            ];
            if conflict > 0 {
                // Show the conflicted events
                let conflicts = sqlx::query(
                    "SELECT sync_event_id, entity_type, entity_id, operation, last_error
                     FROM sync_queue WHERE status='conflict' LIMIT 10"
                ).fetch_all(pool).await?;
                lines.push(String::new());
                lines.push("Conflicted events:".into());
                for r in &conflicts {
                    let eid: String = r.get("sync_event_id");
                    let et: String  = r.get("entity_type");
                    let id: String  = r.get("entity_id");
                    let op: String  = r.get("operation");
                    let err: Option<String> = r.get("last_error");
                    lines.push(format!("  - {} {} {} [{}]: {}",
                        &eid[..8.min(eid.len())], et, &id[..8.min(id.len())], op,
                        err.as_deref().unwrap_or("unknown error")));
                }
            }
            Ok(lines.join("\n"))
        }
        "get_daily_report" => {
            let date = input.get("date").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing date".into()))?;
            let s = report_repo::today_summary(pool, branch_id, date).await?;
            let fmt = |n: i64| money::format_minor(n, currency_exp);
            Ok(format!(
                "Sales Report — {date}:\n- Transactions: {}\n- Net Total: {}\n- Tax: {}\n- Discounts: {}\n- Cash: {}\n- Card: {}\n- Refunds: {} ({})",
                s.transaction_count,
                fmt(s.net_total_minor), fmt(s.tax_total_minor), fmt(s.discount_total_minor),
                fmt(s.cash_total_minor), fmt(s.card_total_minor),
                s.refund_count, fmt(s.refund_total_minor)
            ))
        }
        "get_date_range_report" => {
            let from = input.get("from").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing from".into()))?;
            let to = input.get("to").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing to".into()))?;
            let fmt = |n: i64| money::format_minor(n, currency_exp);

            let row = sqlx::query(
                "SELECT COUNT(*) AS cnt,
                        COALESCE(SUM(net_total_minor),      0) AS net,
                        COALESCE(SUM(tax_total_minor),      0) AS tax,
                        COALESCE(SUM(discount_total_minor), 0) AS discount
                 FROM sales
                 WHERE branch_id = ? AND business_date BETWEEN ? AND ? AND status != 'voided'"
            )
            .bind(branch_id).bind(from).bind(to)
            .fetch_one(pool).await?;

            let cnt: i64  = row.get("cnt");
            let net: i64  = row.get("net");
            let tax: i64  = row.get("tax");
            let disc: i64 = row.get("discount");

            let cash: i64 = sqlx::query_scalar(
                "SELECT COALESCE(SUM(p.amount_minor),0) FROM payments p
                 JOIN sales s ON s.sale_id=p.sale_id
                 WHERE s.branch_id=? AND s.business_date BETWEEN ? AND ?
                   AND p.payment_method='cash' AND s.status!='voided'"
            ).bind(branch_id).bind(from).bind(to).fetch_one(pool).await?;

            let card: i64 = sqlx::query_scalar(
                "SELECT COALESCE(SUM(p.amount_minor),0) FROM payments p
                 JOIN sales s ON s.sale_id=p.sale_id
                 WHERE s.branch_id=? AND s.business_date BETWEEN ? AND ?
                   AND p.payment_method='card' AND s.status!='voided'"
            ).bind(branch_id).bind(from).bind(to).fetch_one(pool).await?;

            let refund_cnt: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM refunds r
                 JOIN sales s ON s.sale_id=r.original_sale_id
                 WHERE s.branch_id=? AND s.business_date BETWEEN ? AND ?"
            ).bind(branch_id).bind(from).bind(to).fetch_one(pool).await?;

            let refund_total: i64 = sqlx::query_scalar(
                "SELECT COALESCE(SUM(r.refund_total_minor),0) FROM refunds r
                 JOIN sales s ON s.sale_id=r.original_sale_id
                 WHERE s.branch_id=? AND s.business_date BETWEEN ? AND ?"
            ).bind(branch_id).bind(from).bind(to).fetch_one(pool).await?;

            Ok(format!(
                "Sales Report {from} → {to}:\n- Transactions: {cnt}\n- Net Total: {}\n- Tax: {}\n- Discounts: {}\n- Cash: {}\n- Card: {}\n- Refunds: {refund_cnt} ({})",
                fmt(net), fmt(tax), fmt(disc), fmt(cash), fmt(card), fmt(refund_total)
            ))
        }
        "get_top_products" => {
            let limit       = input.get("limit").and_then(|v| v.as_i64()).unwrap_or(10).min(25);
            let period_days = input.get("period_days").and_then(|v| v.as_i64()).unwrap_or(30).min(365);
            let fmt = |n: i64| money::format_minor(n, currency_exp);

            let rows = sqlx::query(
                "SELECT p.name,
                        SUM(si.unit_price_minor * CAST(si.quantity AS REAL)) AS revenue,
                        COUNT(DISTINCT s.sale_id) AS txn_count
                 FROM sale_items si
                 JOIN sales s    ON s.sale_id    = si.sale_id
                 JOIN products p ON p.product_id = si.product_id
                 WHERE s.business_date >= date('now', ? || ' days') AND s.status != 'voided'
                 GROUP BY si.product_id, p.name
                 ORDER BY revenue DESC
                 LIMIT ?"
            )
            .bind(format!("-{}", period_days))
            .bind(limit)
            .fetch_all(pool).await?;

            if rows.is_empty() {
                return Ok(format!("No sales data in the last {period_days} days."));
            }
            let lines: Vec<String> = rows.iter().enumerate().map(|(i, r)| {
                let name: String = r.get("name");
                let rev: i64     = r.get("revenue");
                let txn: i64     = r.get("txn_count");
                format!("{}. {} — {} ({} transactions)", i + 1, name, fmt(rev), txn)
            }).collect();
            Ok(format!("Top {} products (last {period_days} days):\n{}", rows.len(), lines.join("\n")))
        }
        "get_shift_history" => {
            let limit = input.get("limit").and_then(|v| v.as_i64()).unwrap_or(10).min(30);
            let fmt = |n: i64| money::format_minor(n, currency_exp);

            let rows = sqlx::query(
                "SELECT s.shift_id, u.display_name AS cashier,
                        s.opened_at, s.closed_at, s.opening_cash_minor,
                        COALESCE((
                            SELECT SUM(net_total_minor) FROM sales
                            WHERE shift_id = s.shift_id AND status != 'voided'
                        ), 0) AS sales_total
                 FROM shifts s
                 LEFT JOIN users u ON u.user_id = s.opened_by_user_id
                 ORDER BY s.opened_at DESC
                 LIMIT ?"
            )
            .bind(limit)
            .fetch_all(pool).await?;

            if rows.is_empty() { return Ok("No shifts found.".into()); }

            let lines: Vec<String> = rows.iter().map(|r| {
                let cashier: String        = r.get::<Option<String>, _>("cashier").unwrap_or_else(|| "Unknown".into());
                let opened: String         = r.get("opened_at");
                let closed: Option<String> = r.get("closed_at");
                let opening: i64           = r.get("opening_cash_minor");
                let sales: i64             = r.get("sales_total");
                let status = if closed.is_some() { "Closed" } else { "OPEN" };
                format!("- {} [{status}] | Opened: {} | Float: {} | Sales: {}",
                    cashier, &opened[..16.min(opened.len())], fmt(opening), fmt(sales))
            }).collect();
            Ok(format!("{} recent shift(s):\n{}", rows.len(), lines.join("\n")))
        }
        "list_categories" => {
            let rows = sqlx::query(
                "SELECT category_id, name, color FROM categories ORDER BY name"
            )
            .fetch_all(pool).await?;

            if rows.is_empty() { return Ok("No categories found.".into()); }
            let lines: Vec<String> = rows.iter().map(|r| {
                let id: String            = r.get("category_id");
                let name: String          = r.get("name");
                let color: Option<String> = r.get("color");
                format!("- {} (ID: {}){}",
                    name, id,
                    color.as_deref().map(|c| format!(" [{}]", c)).unwrap_or_default())
            }).collect();
            Ok(format!("{} categories:\n{}", rows.len(), lines.join("\n")))
        }
        "list_safe_drops" => {
            let shift_id = input.get("shift_id").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing shift_id".into()))?;
            let fmt = |n: i64| money::format_minor(n, currency_exp);

            let rows = sqlx::query(
                "SELECT amount_minor, note, created_by_user_id, created_at
                 FROM cash_events
                 WHERE shift_id = ? AND event_type = 'safe_drop'
                 ORDER BY created_at"
            )
            .bind(shift_id)
            .fetch_all(pool).await?;

            if rows.is_empty() {
                return Ok(format!("No safe drops recorded for shift {}.", &shift_id[..8.min(shift_id.len())]));
            }
            let total: i64 = rows.iter().map(|r| r.get::<i64, _>("amount_minor")).sum();
            let lines: Vec<String> = rows.iter().map(|r| {
                let amt: i64             = r.get("amount_minor");
                let note: Option<String> = r.get("note");
                let at: String           = r.get("created_at");
                format!("- {} | {} | {}",
                    fmt(amt), note.as_deref().unwrap_or("—"),
                    &at[..16.min(at.len())])
            }).collect();
            Ok(format!("{} safe drop(s) | Total: {}\n{}", rows.len(), fmt(total), lines.join("\n")))
        }
        "list_no_sale_events" => {
            let shift_id = input.get("shift_id").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing shift_id".into()))?;

            let rows = sqlx::query(
                "SELECT actor_user_id, note, created_at
                 FROM no_sale_events
                 WHERE shift_id = ?
                 ORDER BY created_at"
            )
            .bind(shift_id)
            .fetch_all(pool).await?;

            if rows.is_empty() {
                return Ok(format!("No no-sale events recorded for shift {}.", &shift_id[..8.min(shift_id.len())]));
            }
            let lines: Vec<String> = rows.iter().map(|r| {
                let actor: String        = r.get("actor_user_id");
                let note: Option<String> = r.get("note");
                let at: String           = r.get("created_at");
                format!("- Actor: {} | {} | {}",
                    &actor[..8.min(actor.len())],
                    note.as_deref().unwrap_or("no note"),
                    &at[11..16.min(at.len())])
            }).collect();
            Ok(format!("{} no-sale event(s):\n{}", rows.len(), lines.join("\n")))
        }
        "get_audit_chain_status" => {
            let device_id: String = sqlx::query_scalar(
                "SELECT device_id FROM devices WHERE is_active = 1 ORDER BY device_code LIMIT 1"
            )
            .fetch_optional(pool).await?.flatten().unwrap_or_default();

            let r = crate::db::repositories::audit_hash::verify_chain(pool, &device_id).await?;
            let status = if r.ok { "✓ INTACT" } else { "⚠ ANOMALIES DETECTED" };
            Ok(format!(
                "Audit Chain [{status}]:\n- Total rows:   {}\n- Legacy rows:  {} (pre-chain, not verified)\n- Verified:     {}\n- Broken hash:  {}\n- Broken links: {}\n\n{}",
                r.total_rows, r.legacy_rows, r.verified, r.broken_hash, r.broken_link,
                if r.ok {
                    "Chain integrity confirmed — no tampering detected."
                } else {
                    "⚠ WARNING: Chain anomalies found. Contact your system administrator immediately."
                }
            ))
        }
        _ => Err(AppError::Validation(format!("Unknown read tool: {}", tool_name))),
    }
}

// ── Mutation dry-run: build a human-readable preview ──────────────────────────

pub async fn dry_run_mutation(
    pool: &SqlitePool,
    tool_name: &str,
    input: &Value,
    currency_exp: u32,
) -> AppResult<ToolPreview> {
    let fmt = |n: i64| money::format_minor(n, currency_exp);

    match tool_name {
        "update_product_price" => {
            let product_id = input.get("product_id").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let new_price = input.get("new_price_minor").and_then(|v| v.as_i64())
                .ok_or_else(|| AppError::Validation("Missing new_price_minor".into()))?;
            let reason = input.get("reason").and_then(|v| v.as_str()).unwrap_or("—");

            let p = product_repo::get_product_by_id(pool, product_id).await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;

            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Update selling price of '{}'", p.product.name),
                fields: vec![
                    ToolPreviewField { label: "Product".into(), value: p.product.name.clone() },
                    ToolPreviewField { label: "Current Price".into(), value: format!("BHD {}", fmt(p.price_minor)) },
                    ToolPreviewField { label: "New Price".into(), value: format!("BHD {}", fmt(new_price)) },
                    ToolPreviewField { label: "Reason".into(), value: reason.into() },
                ],
            })
        }
        "set_product_active" => {
            let product_id = input.get("product_id").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let is_active = input.get("is_active").and_then(|v| v.as_bool())
                .ok_or_else(|| AppError::Validation("Missing is_active".into()))?;

            let p = product_repo::get_product_by_id(pool, product_id).await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;

            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("{} product '{}'", if is_active { "Enable" } else { "Disable" }, p.product.name),
                fields: vec![
                    ToolPreviewField { label: "Product".into(), value: p.product.name.clone() },
                    ToolPreviewField {
                        label: "Action".into(),
                        value: if is_active { "Enable (show in POS)".into() } else { "Disable (hide from POS)".into() },
                    },
                ],
            })
        }
        "update_product_name" => {
            let product_id = input.get("product_id").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let new_name = input.get("new_name").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing new_name".into()))?;

            let p = product_repo::get_product_by_id(pool, product_id).await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;

            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: "Rename product".to_string(),
                fields: vec![
                    ToolPreviewField { label: "Current Name".into(), value: p.product.name.clone() },
                    ToolPreviewField { label: "New Name".into(), value: new_name.into() },
                ],
            })
        }
        "adjust_stock" => {
            let product_id = input.get("product_id").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let delta = input.get("quantity_delta").and_then(|v| v.as_f64())
                .ok_or_else(|| AppError::Validation("Missing quantity_delta".into()))?;
            let notes = input.get("notes").and_then(|v| v.as_str()).unwrap_or("—");
            let p = product_repo::get_product_by_id(pool, product_id).await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;
            let levels = stock_repo::get_all_levels(pool).await?;
            let current = levels.iter()
                .find(|s| s.product_id == product_id)
                .map(|s| s.quantity_on_hand.clone())
                .unwrap_or_else(|| "0".into());
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Adjust stock for '{}'", p.product.name),
                fields: vec![
                    ToolPreviewField { label: "Product".into(), value: p.product.name.clone() },
                    ToolPreviewField { label: "Current Qty".into(), value: current },
                    ToolPreviewField { label: "Adjustment".into(), value: format!("{:+}", delta) },
                    ToolPreviewField { label: "Reason".into(), value: notes.into() },
                ],
            })
        }
        "stock_take" => {
            let product_id = input.get("product_id").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let new_qty = input.get("new_quantity").and_then(|v| v.as_f64())
                .ok_or_else(|| AppError::Validation("Missing new_quantity".into()))?;
            let notes = input.get("notes").and_then(|v| v.as_str()).unwrap_or("—");
            let p = product_repo::get_product_by_id(pool, product_id).await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;
            let levels = stock_repo::get_all_levels(pool).await?;
            let current = levels.iter()
                .find(|s| s.product_id == product_id)
                .map(|s| s.quantity_on_hand.clone())
                .unwrap_or_else(|| "0".into());
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Stock take for '{}'", p.product.name),
                fields: vec![
                    ToolPreviewField { label: "Product".into(), value: p.product.name.clone() },
                    ToolPreviewField { label: "Current Qty".into(), value: current },
                    ToolPreviewField { label: "New Count".into(), value: format!("{}", new_qty) },
                    ToolPreviewField { label: "Notes".into(), value: notes.into() },
                ],
            })
        }
        "create_product" => {
            let name = input.get("name").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing name".into()))?;
            let price = input.get("price_minor").and_then(|v| v.as_i64())
                .ok_or_else(|| AppError::Validation("Missing price_minor".into()))?;
            let category_id = input.get("category_id").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing category_id".into()))?;
            let sku     = input.get("sku").and_then(|v| v.as_str()).unwrap_or("—");
            let barcode = input.get("barcode").and_then(|v| v.as_str()).unwrap_or("—");

            let cat_name: Option<String> = sqlx::query_scalar(
                "SELECT name FROM categories WHERE category_id = ?"
            )
            .bind(category_id)
            .fetch_optional(pool).await?;

            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Create new product '{}'", name),
                fields: vec![
                    ToolPreviewField { label: "Name".into(),     value: name.into() },
                    ToolPreviewField { label: "Price".into(),    value: format!("BHD {}", fmt(price)) },
                    ToolPreviewField { label: "Category".into(), value: cat_name.unwrap_or_else(|| category_id.into()) },
                    ToolPreviewField { label: "SKU".into(),      value: sku.into() },
                    ToolPreviewField { label: "Barcode".into(),  value: barcode.into() },
                ],
            })
        }
        _ => Err(AppError::Validation(format!("Unknown mutation tool: {}", tool_name))),
    }
}

// ── Mutation executor ─────────────────────────────────────────────────────────

pub struct MutationResult {
    pub description: String,
    pub undo_snapshot_json: String,
    pub rollback_tool: String,
    pub rollback_input_json: String,
    pub entity_type: String,
    pub entity_id: String,
}

pub async fn execute_mutation(
    pool: &SqlitePool,
    tool_name: &str,
    input: &Value,
    currency_exp: u32,
) -> AppResult<MutationResult> {
    let fmt = |n: i64| money::format_minor(n, currency_exp);

    match tool_name {
        "update_product_price" => {
            let product_id = input.get("product_id").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let new_price = input.get("new_price_minor").and_then(|v| v.as_i64())
                .ok_or_else(|| AppError::Validation("Missing new_price_minor".into()))?;

            let p = product_repo::get_product_by_id(pool, product_id).await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;
            let old_price = p.price_minor;

            // Expire current active price and insert new one
            let now = chrono::Utc::now().to_rfc3339();
            let new_price_id = ulid::Ulid::new().to_string();

            sqlx::query(
                "UPDATE product_prices SET effective_to = ?
                 WHERE product_id = ? AND branch_id IS NULL AND price_type = 'selling'
                   AND effective_to IS NULL"
            )
            .bind(&now)
            .bind(product_id)
            .execute(pool)
            .await?;

            sqlx::query(
                "INSERT INTO product_prices (price_id, product_id, branch_id, price_type, price_minor,
                 currency, effective_from, effective_to, created_by_user_id)
                 VALUES (?, ?, NULL, 'selling', ?, 'BHD', ?, NULL, 'AI_ADMIN')"
            )
            .bind(&new_price_id)
            .bind(product_id)
            .bind(new_price)
            .bind(&now)
            .execute(pool)
            .await?;

            // Write audit log
            write_audit(pool, "AI_ADMIN", "product_price_update", product_id,
                &json!({"from": old_price, "to": new_price})).await?;

            // Enqueue price for sync
            let _ = outbox::enqueue_product_price(
                pool, "01JDEVICE0000000000000001", "01JBRANCH0000000000000001",
                &new_price_id, product_id, new_price, "BHD", &now,
                "AI_ADMIN", None, &now,
            ).await;
            // Enqueue updated product row (effective_to changed on old price, but product itself didn't change — just the price)
            // The product entity sync is handled by the price entry above.

            Ok(MutationResult {
                description: format!("Price of '{}' changed from BHD {} to BHD {}",
                    p.product.name, fmt(old_price), fmt(new_price)),
                undo_snapshot_json: json!({ "price_minor": old_price }).to_string(),
                rollback_tool: "update_product_price".into(),
                rollback_input_json: json!({
                    "product_id": product_id, "new_price_minor": old_price
                }).to_string(),
                entity_type: "product_price".into(),
                entity_id: product_id.into(),
            })
        }
        "set_product_active" => {
            let product_id = input.get("product_id").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let is_active = input.get("is_active").and_then(|v| v.as_bool())
                .ok_or_else(|| AppError::Validation("Missing is_active".into()))?;

            let p = product_repo::get_product_by_id(pool, product_id).await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;
            let old_active = p.product.is_active;

            sqlx::query("UPDATE products SET is_active = ?, version = version + 1 WHERE product_id = ?")
                .bind(is_active as i64)
                .bind(product_id)
                .execute(pool)
                .await?;

            write_audit(pool, "AI_ADMIN", "product_status_change", product_id,
                &json!({"from": old_active, "to": is_active})).await?;

            // Enqueue updated product for sync
            {
                let now_ts = chrono::Utc::now().to_rfc3339();
                let updated = product_repo::get_product_by_id(pool, product_id).await;
                if let Ok(Some(up)) = updated {
                    let ca = if up.product.created_at.is_empty() { now_ts.clone() } else { up.product.created_at.clone() };
                    let ua = if up.product.updated_at.is_empty() { now_ts.clone() } else { up.product.updated_at.clone() };
                    let _ = outbox::enqueue_product(
                        pool, "01JDEVICE0000000000000001", "01JBRANCH0000000000000001",
                        product_id, &up.product.category_id, &up.product.name,
                        up.product.sku.as_deref(), up.product.barcode.as_deref(),
                        up.product.description.as_deref(),
                        up.product.track_inventory, up.product.allow_decimal_quantity,
                        up.product.is_active, up.product.tax_rule_id.as_deref(),
                        up.product.cost_minor, &up.product.currency,
                        &ca, &ua, up.product.version,
                    ).await;
                }
            }

            Ok(MutationResult {
                description: format!("Product '{}' {}",
                    p.product.name, if is_active { "enabled" } else { "disabled" }),
                undo_snapshot_json: json!({ "is_active": old_active }).to_string(),
                rollback_tool: "set_product_active".into(),
                rollback_input_json: json!({
                    "product_id": product_id, "is_active": old_active
                }).to_string(),
                entity_type: "product".into(),
                entity_id: product_id.into(),
            })
        }
        "update_product_name" => {
            let product_id = input.get("product_id").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let new_name = input.get("new_name").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing new_name".into()))?;

            let p = product_repo::get_product_by_id(pool, product_id).await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;
            let old_name = p.product.name.clone();

            sqlx::query("UPDATE products SET name = ?, version = version + 1 WHERE product_id = ?")
                .bind(new_name)
                .bind(product_id)
                .execute(pool)
                .await?;

            write_audit(pool, "AI_ADMIN", "product_rename", product_id,
                &json!({"from": &old_name, "to": new_name})).await?;

            // Enqueue renamed product for sync
            {
                let now_ts = chrono::Utc::now().to_rfc3339();
                let updated = product_repo::get_product_by_id(pool, product_id).await;
                if let Ok(Some(up)) = updated {
                    let ca = if up.product.created_at.is_empty() { now_ts.clone() } else { up.product.created_at.clone() };
                    let ua = if up.product.updated_at.is_empty() { now_ts.clone() } else { up.product.updated_at.clone() };
                    let _ = outbox::enqueue_product(
                        pool, "01JDEVICE0000000000000001", "01JBRANCH0000000000000001",
                        product_id, &up.product.category_id, &up.product.name,
                        up.product.sku.as_deref(), up.product.barcode.as_deref(),
                        up.product.description.as_deref(),
                        up.product.track_inventory, up.product.allow_decimal_quantity,
                        up.product.is_active, up.product.tax_rule_id.as_deref(),
                        up.product.cost_minor, &up.product.currency,
                        &ca, &ua, up.product.version,
                    ).await;
                }
            }

            Ok(MutationResult {
                description: format!("Product renamed from '{}' to '{}'", old_name, new_name),
                undo_snapshot_json: json!({ "name": &old_name }).to_string(),
                rollback_tool: "update_product_name".into(),
                rollback_input_json: json!({
                    "product_id": product_id, "new_name": &old_name
                }).to_string(),
                entity_type: "product".into(),
                entity_id: product_id.into(),
            })
        }
        "adjust_stock" => {
            let product_id = input.get("product_id").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let delta = input.get("quantity_delta").and_then(|v| v.as_f64())
                .ok_or_else(|| AppError::Validation("Missing quantity_delta".into()))?;
            let notes = input.get("notes").and_then(|v| v.as_str());
            let p = product_repo::get_product_by_id(pool, product_id).await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;

            let result = movements::manual_adjust(pool, product_id, delta, notes, "AI_ADMIN", None).await?;
            let new_qty = result.quantity_on_hand.clone();

            write_audit(pool, "AI_ADMIN", "stock.adjustment", product_id,
                &json!({ "delta": delta, "new_qty": &new_qty, "notes": notes })).await?;

            Ok(MutationResult {
                description: format!("Stock of '{}' adjusted by {:+} → now {}",
                    p.product.name, delta, new_qty),
                undo_snapshot_json: json!({ "quantity_delta": -delta }).to_string(),
                rollback_tool: "adjust_stock".into(),
                rollback_input_json: json!({
                    "product_id": product_id,
                    "quantity_delta": -delta,
                    "notes": "Undo previous adjustment",
                }).to_string(),
                entity_type: "stock_level".into(),
                entity_id: product_id.into(),
            })
        }
        "stock_take" => {
            let product_id = input.get("product_id").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let new_quantity = input.get("new_quantity").and_then(|v| v.as_f64())
                .ok_or_else(|| AppError::Validation("Missing new_quantity".into()))?;
            let notes = input.get("notes").and_then(|v| v.as_str());
            let p = product_repo::get_product_by_id(pool, product_id).await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;

            // Get old qty for undo
            let levels = stock_repo::get_all_levels(pool).await?;
            let old_qty: f64 = levels.iter()
                .find(|s| s.product_id == product_id)
                .and_then(|s| s.quantity_on_hand.parse().ok())
                .unwrap_or(0.0);

            movements::stock_take(pool, product_id, new_quantity, notes, "AI_ADMIN", None).await?;

            write_audit(pool, "AI_ADMIN", "stock.stock_take", product_id,
                &json!({ "old_qty": old_qty, "new_qty": new_quantity, "notes": notes })).await?;

            Ok(MutationResult {
                description: format!("Stock take for '{}': counted {} (was {})",
                    p.product.name, new_quantity, old_qty),
                undo_snapshot_json: json!({ "new_quantity": old_qty }).to_string(),
                rollback_tool: "stock_take".into(),
                rollback_input_json: json!({
                    "product_id": product_id,
                    "new_quantity": old_qty,
                    "notes": "Undo stock take",
                }).to_string(),
                entity_type: "stock_level".into(),
                entity_id: product_id.into(),
            })
        }
        "create_product" => {
            let name = input.get("name").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing name".into()))?;
            let price_minor = input.get("price_minor").and_then(|v| v.as_i64())
                .ok_or_else(|| AppError::Validation("Missing price_minor".into()))?;
            let category_id = input.get("category_id").and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing category_id".into()))?;
            let sku     = input.get("sku").and_then(|v| v.as_str());
            let barcode = input.get("barcode").and_then(|v| v.as_str());

            let now        = chrono::Utc::now().to_rfc3339();
            let product_id = ulid::Ulid::new().to_string();
            let price_id   = ulid::Ulid::new().to_string();

            sqlx::query(
                "INSERT INTO products
                   (product_id, category_id, name, sku, barcode,
                    track_inventory, allow_decimal_quantity, is_active,
                    currency, version, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, 0, 0, 1, 'BHD', 1, ?, ?)"
            )
            .bind(&product_id)
            .bind(category_id)
            .bind(name)
            .bind(sku)
            .bind(barcode)
            .bind(&now)
            .bind(&now)
            .execute(pool).await?;

            sqlx::query(
                "INSERT INTO product_prices
                   (price_id, product_id, branch_id, price_type, price_minor,
                    currency, effective_from, effective_to, created_by_user_id)
                 VALUES (?, ?, NULL, 'selling', ?, 'BHD', ?, NULL, 'AI_ADMIN')"
            )
            .bind(&price_id)
            .bind(&product_id)
            .bind(price_minor)
            .bind(&now)
            .execute(pool).await?;

            write_audit(pool, "AI_ADMIN", "product.created", &product_id,
                &json!({ "name": name, "price_minor": price_minor, "category_id": category_id })).await?;

            Ok(MutationResult {
                description: format!("Created product '{}' at BHD {}", name, fmt(price_minor)),
                undo_snapshot_json: json!({ "product_id": &product_id }).to_string(),
                rollback_tool: "set_product_active".into(),
                rollback_input_json: json!({
                    "product_id": &product_id, "is_active": false
                }).to_string(),
                entity_type: "product".into(),
                entity_id: product_id,
            })
        }
        _ => Err(AppError::Validation(format!("Unknown mutation tool: {}", tool_name))),
    }
}

// ── Undo executor ─────────────────────────────────────────────────────────────

pub async fn execute_undo(
    pool: &SqlitePool,
    rollback_tool: &str,
    rollback_input_json: &str,
    currency_exp: u32,
) -> AppResult<String> {
    let input: Value = serde_json::from_str(rollback_input_json)
        .map_err(|e| AppError::Validation(format!("Invalid rollback input: {}", e)))?;
    let result = execute_mutation(pool, rollback_tool, &input, currency_exp).await?;
    Ok(result.description)
}

// ── Audit helper ──────────────────────────────────────────────────────────────

async fn write_audit(
    pool: &SqlitePool,
    actor_user_id: &str,
    event_type: &str,
    entity_id: &str,
    after: &serde_json::Value,
) -> AppResult<()> {
    let id = ulid::Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let hash = format!("{:x}", md5_simple(&format!("{}{}{}", id, event_type, now)));
    sqlx::query(
        "INSERT INTO audit_logs
         (audit_log_id, event_type, entity_type, entity_id, actor_user_id,
          actor_type, after_json, created_at, hash)
         VALUES (?, ?, 'product', ?, ?, 'ai_agent', ?, ?, ?)"
    )
    .bind(&id)
    .bind(event_type)
    .bind(entity_id)
    .bind(actor_user_id)
    .bind(after.to_string())
    .bind(&now)
    .bind(&hash)
    .execute(pool)
    .await?;
    Ok(())
}

fn md5_simple(s: &str) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}
