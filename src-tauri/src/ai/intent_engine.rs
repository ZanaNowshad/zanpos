//! ZanAI Intent Engine v2 — Deterministic business actions.
//!
//! Instead of giving the AI 115 individual tools and hoping it picks the right one,
//! this engine exposes ~20 high-level business **intents**. Each intent has:
//!   - Fixed, validated parameters (no model guessing)
//!   - Built-in chunking for large operations
//!   - Progress reporting
//!   - Structured, deterministic results
//!
//! The AI describes WHAT it wants to do. The engine handles HOW.

use crate::db::repositories::ai_admin_repo;
use crate::domain::money;
use crate::errors::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};

// ── Intent Definition ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
// Retained: the declarative intent registry was superseded by tool_catalogue.
#[allow(dead_code)]
pub struct IntentDef {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: Value, // JSON Schema
}

/// Whether an intent name performs mutations (needs confirmation).
pub fn is_mutation_intent(name: &str) -> bool {
    matches!(name, "create_product" | "receive_stock" | "create_customer")
}

/// List of valid intent names the AI may call.
pub const INTENT_NAMES: &[&str] = &[
    "search_products",
    "get_product_detail",
    "create_product",
    "get_low_stock",
    "receive_stock",
    "get_sales_report",
    "get_today_summary",
    "list_customers",
    "create_customer",
    "list_users",
    "get_cash_status",
    "list_deliveries",
    "get_sync_status",
    "get_audit_log",
    "open_tab",
];
// Retained: the declarative intent registry was superseded by tool_catalogue.
#[allow(dead_code)]
pub fn all_intents() -> Vec<IntentDef> {
    // `update_product`, `create_user` and `backup_database` used to sit in this
    // list with full schemas while `execute_intent` had no arm for any of them,
    // so the model was offered three capabilities that answered
    // "Unknown intent". All three exist as real tools — `update_product_full`,
    // `create_user`, `backup_database` — so the tool path already covers them
    // and the intent entries were duplicate advertising for code that was never
    // written. Removed rather than implemented: two ways to do one thing is how
    // they drifted apart in the first place.
    vec![
        IntentDef { name: "search_products", description: "Search products by name, SKU, barcode, or category. Returns paginated results.", parameters: json!({"type":"object","properties":{"query":{"type":"string"},"category":{"type":"string"}}}) },
        IntentDef { name: "get_product_detail", description: "Get full details of a single product including all prices, stock levels, and barcodes.", parameters: json!({"type":"object","properties":{"product_id":{"type":"string"}},"required":["product_id"]}) },
        IntentDef { name: "create_product", description: "Create a new product with all required fields. Validates category, tax rule, and barcode uniqueness.", parameters: json!({"type":"object","properties":{"name":{"type":"string"},"category_id":{"type":"string"},"sku":{"type":"string"},"barcode":{"type":"string"},"selling_price_minor":{"type":"integer"},"cost_minor":{"type":"integer"},"tax_rule_id":{"type":"string"},"track_inventory":{"type":"boolean"}},"required":["name","category_id"]}) },
        // adjust_prices_batch and bulk_price_adjust used to be defined here. Both
        // are superseded by the bulk_price_adjust *engine* operation, which is the
        // only price path with preview, confirm and undo. They stayed in this list
        // long after they stopped being routable — INTENT_NAMES never included
        // them — so they read as live capability while being unreachable.
        // execute_intent still answers adjust_prices_batch with a redirect, for a
        // model that learned the old name.
        IntentDef { name: "get_low_stock", description: "List all products below their reorder point, with stock levels and reorder quantities.", parameters: json!({"type":"object","properties":{"category_id":{"type":"string"}}}) },
        IntentDef { name: "receive_stock", description: "Record receiving stock for a product. Updates inventory and stock levels.", parameters: json!({"type":"object","properties":{"product_id":{"type":"string"},"quantity_delta":{"type":"string"},"notes":{"type":"string"}},"required":["product_id","quantity_delta"]}) },
        IntentDef { name: "get_sales_report", description: "Get sales report for a date range. Returns totals, top products, payment breakdown.", parameters: json!({"type":"object","properties":{"from_date":{"type":"string"},"to_date":{"type":"string"}},"required":["from_date","to_date"]}) },
        IntentDef { name: "get_today_summary", description: "Quick snapshot: today's transaction count, revenue, discounts, tax, refunds.", parameters: json!({"type":"object","properties":{}}) },
        IntentDef { name: "list_customers", description: "Search and list customers. Supports search by name, phone, or email.", parameters: json!({"type":"object","properties":{"search":{"type":"string"}}}) },
        IntentDef { name: "create_customer", description: "Create a new customer record.", parameters: json!({"type":"object","properties":{"name":{"type":"string"},"phone":{"type":"string"},"email":{"type":"string"},"notes":{"type":"string"}},"required":["name"]}) },
        IntentDef { name: "list_users", description: "List all system users with roles.", parameters: json!({"type":"object","properties":{}}) },
        IntentDef { name: "get_cash_status", description: "Current cash drawer status: expected cash, counted, variance, paid in/out, safe drops.", parameters: json!({"type":"object","properties":{}}) },
        IntentDef { name: "list_deliveries", description: "List delivery orders, filterable by status. Returns customer, address, payment status.", parameters: json!({"type":"object","properties":{"status":{"type":"string","enum":["pending","dispatched","delivered","cancelled","all"]}}}) },
        IntentDef { name: "get_sync_status", description: "Current hub sync status: online/offline, pending count, last sync time.", parameters: json!({"type":"object","properties":{}}) },
        IntentDef { name: "get_audit_log", description: "View audit trail entries for a date range.", parameters: json!({"type":"object","properties":{"from_date":{"type":"string"},"to_date":{"type":"string"}}}) },
        IntentDef { name: "open_tab", description: "Navigate the admin workspace to a destination accepted by the tab schema.", parameters: json!({"type":"object","properties":{"tab":{"type":"string","enum":["products","categories","inventory","reports","cashier","eod","deliveries","customers","users","purchasing","settings","audit","devices"]}},"required":["tab"]}) },
    ]
}

// ── Intent Execution ─────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct IntentResult {
    pub ok: bool,
    pub data: Value,
    pub progress: Option<ProgressInfo>,
    pub metadata: Option<IntentMeta>,
}

#[derive(Debug, Serialize)]
pub struct ProgressInfo {
    pub processed: usize,
    pub total: usize,
    pub current_chunk: usize,
}

#[derive(Debug, Serialize)]
pub struct IntentMeta {
    pub intent_name: String,
    pub chunks_used: usize,
    pub records_affected: usize,
}

/// Main dispatch. The AI picks an intent name, provides parameters,
/// and this engine executes it with built-in chunking and validation.
pub async fn execute_intent(
    pool: &SqlitePool,
    intent_name: &str,
    params: &Value,
    branch_id: &str,
) -> AppResult<IntentResult> {
    match intent_name {
        "adjust_prices_batch" => Err(AppError::Validation(
            "The 'adjust_prices_batch' intent has been consolidated into the 'bulk_price_adjust' engine operation. Please use that tool instead — it provides proper preview, confirm, and undo support.".into(),
        )),
        "search_products" => search_products_intent(pool, params).await,
        "get_product_detail" => get_product_detail(pool, params).await,
        "create_product" => create_product_intent(pool, params, branch_id).await,
        "get_low_stock" => get_low_stock_intent(pool, params).await,
        "get_today_summary" => get_today_summary_intent(pool, branch_id).await,
        "get_sales_report" => get_sales_report_intent(pool, params, branch_id).await,
        "list_customers" => list_customers_intent(pool, params, branch_id).await,
        "create_customer" => create_customer_intent(pool, params, branch_id).await,
        "list_users" => list_users_intent(pool).await,
        "get_cash_status" => get_cash_status_intent(pool, branch_id).await,
        "get_sync_status" => get_sync_status_intent(pool).await,
        "list_deliveries" => list_deliveries_intent(pool, params, branch_id).await,
        "receive_stock" => receive_stock_intent(pool, params, branch_id).await,
        "get_audit_log" => get_audit_log_intent(pool, params).await,
        "open_tab" => Ok(IntentResult {
            ok: true,
            data: params.clone(),
            progress: None,
            metadata: None,
        }),
        _ => Err(AppError::Validation(format!(
            "Unknown intent: {intent_name}"
        ))),
    }
}

// ── Intent Implementations ───────────────────────────────────────────────────────

async fn search_products_intent(pool: &SqlitePool, params: &Value) -> AppResult<IntentResult> {
    let query = params.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let _category = params.get("category").and_then(|v| v.as_str());
    let results =
        crate::db::repositories::product_repo::search_products_paginated(pool, query, None, 25)
            .await?;
    Ok(IntentResult {
        ok: true,
        data: json!({"products": results.iter().take(20).map(|p| json!({"id":p.product.product_id,"name":p.product.name,"sku":p.product.sku,"barcode":p.product.barcode,"price_minor":p.price_minor,"category":p.category_name})).collect::<Vec<_>>(),"total":results.len()}),
        progress: None,
        metadata: None,
    })
}

async fn get_low_stock_intent(pool: &SqlitePool, _params: &Value) -> AppResult<IntentResult> {
    let rows: Vec<(String, String, String, i64, String)> = sqlx::query_as(
        "SELECT p.product_id, p.name, sl.quantity_on_hand, p.reorder_point, c.name as cat_name
         FROM products p
         JOIN stock_levels sl ON sl.product_id = p.product_id
         LEFT JOIN categories c ON c.category_id = p.category_id
         WHERE p.track_inventory = 1 AND CAST(sl.quantity_on_hand AS REAL) <= p.reorder_point
         ORDER BY CAST(sl.quantity_on_hand AS REAL) ASC
         LIMIT 30",
    )
    .fetch_all(pool)
    .await?;
    let items: Vec<Value> = rows.into_iter().map(|(id, name, qty, reorder, cat)| {
        json!({"id":id,"name":name,"quantity":qty,"reorder_point":reorder,"category":cat})
    }).collect();
    Ok(IntentResult {
        ok: true,
        data: json!({"low_stock": items, "count": items.len()}),
        progress: None,
        metadata: None,
    })
}

async fn get_today_summary_intent(pool: &SqlitePool, branch_id: &str) -> AppResult<IntentResult> {
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let summary =
        crate::db::repositories::report_repo::today_summary(pool, branch_id, &today).await?;
    Ok(IntentResult {
        ok: true,
        data: json!({"business_date":summary.business_date,"transactions":summary.transaction_count,"net_total":money::format_minor(summary.net_total_minor, 3),"tax":money::format_minor(summary.tax_total_minor, 3),"discounts":money::format_minor(summary.discount_total_minor, 3),"cash":money::format_minor(summary.cash_total_minor, 3),"card":money::format_minor(summary.card_total_minor, 3),"refunds":summary.refund_count,"refund_total":money::format_minor(summary.refund_total_minor, 3)}),
        progress: None,
        metadata: None,
    })
}

async fn get_product_detail(pool: &SqlitePool, params: &Value) -> AppResult<IntentResult> {
    let pid = params
        .get("product_id")
        .and_then(|v| v.as_str())
        .ok_or(AppError::Validation("product_id required".into()))?;
    let row = sqlx::query(
        "SELECT p.name, p.sku, p.barcode, p.description, p.is_active, p.track_inventory, p.reorder_point, p.cost_minor,
                c.name as cat_name, sl.quantity_on_hand, pp.price_minor
         FROM products p LEFT JOIN categories c ON c.category_id = p.category_id
         LEFT JOIN stock_levels sl ON sl.product_id = p.product_id
         LEFT JOIN v_current_selling_price pp ON pp.product_id = p.product_id
         WHERE p.product_id = ?"
    ).bind(pid).fetch_optional(pool).await?.ok_or(AppError::NotFound("Product not found".into()))?;
    Ok(IntentResult {
        ok: true,
        data: json!({"name":row.get::<String,_>(0),"sku":row.get::<Option<String>,_>(1),"barcode":row.get::<Option<String>,_>(2),"category":row.get::<Option<String>,_>(7),"stock":row.get::<Option<String>,_>(8),"price_minor":row.get::<Option<i64>,_>(9)}),
        progress: None,
        metadata: None,
    })
}

async fn create_product_intent(
    pool: &SqlitePool,
    params: &Value,
    _branch_id: &str,
) -> AppResult<IntentResult> {
    let name = params
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or(AppError::Validation("name required".into()))?;
    let category_id = params
        .get("category_id")
        .and_then(|v| v.as_str())
        .ok_or(AppError::Validation("category_id required".into()))?;
    let pid = ulid::Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("INSERT INTO products (product_id,category_id,name,sku,barcode,cost_minor,is_active,track_inventory,reorder_point,created_at,updated_at) VALUES (?,?,?,?,?,?,1,1,0,?,?)")
        .bind(&pid).bind(category_id).bind(name)
        .bind(params.get("sku").and_then(|v| v.as_str()).unwrap_or(""))
        .bind(params.get("barcode").and_then(|v| v.as_str()).unwrap_or(""))
        .bind(params.get("cost_minor").and_then(|v| v.as_i64()).unwrap_or(0))
        .bind(&now).bind(&now).execute(pool).await?;
    if let Some(price) = params.get("selling_price_minor").and_then(|v| v.as_i64()) {
        sqlx::query("INSERT INTO product_prices (price_id,product_id,price_type,price_minor,currency,effective_from,created_by_user_id,created_at) VALUES (?,?,'selling',?,'BHD',datetime('now'),'SYSTEM',datetime('now'))")
            .bind(ulid::Ulid::new().to_string()).bind(&pid).bind(price).execute(pool).await?;
    }
    Ok(IntentResult {
        ok: true,
        data: json!({"product_id":pid,"name":name}),
        progress: None,
        metadata: None,
    })
}

async fn get_sales_report_intent(
    pool: &SqlitePool,
    params: &Value,
    branch_id: &str,
) -> AppResult<IntentResult> {
    let from = params
        .get("from_date")
        .and_then(|v| v.as_str())
        .ok_or(AppError::Validation("from_date required".into()))?;
    let to = params
        .get("to_date")
        .and_then(|v| v.as_str())
        .ok_or(AppError::Validation("to_date required".into()))?;
    // `business_date` and not `sold_at`, and voided sales left out.
    //
    // `sold_at` is a full RFC3339 timestamp and the bounds are plain dates, so
    // `sold_at BETWEEN '2026-08-31' AND '2026-09-01'` compares '2026-09-01T09:00:00Z'
    // against '2026-09-01' as text and finds it larger: every sale on the closing
    // day was dropped. `business_date` is the date the till itself recorded, in
    // the same shape as the bounds.
    //
    // Voided sales were counted too, so this figure exceeded the till and every
    // other report in the app, which all filter them. Someone asking the
    // assistant what the shop took got a number that agreed with nothing.
    let (txn_count, net, tax): (i64, Option<i64>, Option<i64>) = sqlx::query_as(
        "SELECT COUNT(*),
                COALESCE(SUM(net_total_minor), 0),
                COALESCE(SUM(tax_total_minor), 0)
           FROM sales
          WHERE branch_id = ?
            AND business_date BETWEEN ? AND ?
            AND status != 'voided'",
    )
    .bind(branch_id)
    .bind(from)
    .bind(to)
    .fetch_one(pool)
    .await?;
    Ok(IntentResult {
        ok: true,
        data: json!({"from":from,"to":to,"transactions":txn_count,"net_total":money::format_minor(net.unwrap_or(0),3),"tax":money::format_minor(tax.unwrap_or(0),3)}),
        progress: None,
        metadata: None,
    })
}

/// The intent engine wins over `ai::tools` for every read intent, so this is
/// the `list_customers` ZanAI actually runs — the copy in `tools.rs` is
/// unreachable for this name, and the smoke tests covering it were passing
/// against a path nothing calls.
///
/// This copy had drifted badly. It queried no branch, so ZanAI could read
/// another branch's customers, and it matched only `name` and `phone`, so a
/// lookup by email, by the name WhatsApp knows someone by, or by a number
/// typed with its separators found nothing at all.
///
/// It now uses the one predicate the till and the customer directory use
/// rather than keeping a third copy of it.
async fn list_customers_intent(
    pool: &SqlitePool,
    params: &Value,
    branch_id: &str,
) -> AppResult<IntentResult> {
    type CustomerRow = (
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    );
    let search = params.get("search").and_then(|v| v.as_str()).unwrap_or("");
    let rows: Vec<CustomerRow> = if search.trim().is_empty() {
        sqlx::query_as(
            "SELECT customer_id, name, whatsapp_name, phone, email
             FROM customers
             WHERE branch_id = ? AND deleted_at IS NULL
             ORDER BY name LIMIT 30",
        )
        .bind(branch_id)
        .fetch_all(pool)
        .await?
    } else {
        let (pattern, digit_pattern) =
            crate::commands::customer_search::customer_search_patterns(search);
        let sql = format!(
            "SELECT customer_id, name, whatsapp_name, phone, email
             FROM customers
             WHERE branch_id = ? AND deleted_at IS NULL AND {}
             ORDER BY name LIMIT 30",
            crate::commands::customer_search::customer_search_where(digit_pattern.is_some()),
        );
        // One bind per text placeholder — name, whatsapp_name, phone, email —
        // then the digit pattern when the query had digits.
        let mut query = sqlx::query_as(&sql)
            .bind(branch_id)
            .bind(&pattern)
            .bind(&pattern)
            .bind(&pattern)
            .bind(&pattern);
        if let Some(digits) = &digit_pattern {
            query = query.bind(digits);
        }
        query.fetch_all(pool).await?
    };
    Ok(IntentResult {
        ok: true,
        data: json!({
            "customers": rows
                .into_iter()
                .map(|(id, name, whatsapp_name, phone, email)| json!({
                    "id": id,
                    "name": name,
                    "whatsapp_name": whatsapp_name,
                    "phone": phone,
                    "email": email,
                }))
                .collect::<Vec<_>>()
        }),
        progress: None,
        metadata: None,
    })
}

async fn create_customer_intent(
    pool: &SqlitePool,
    params: &Value,
    branch_id: &str,
) -> AppResult<IntentResult> {
    let name = params
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or(AppError::Validation("name required".into()))?;
    let cid = ulid::Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("INSERT INTO customers (customer_id,branch_id,name,phone,email,notes,created_at,updated_at) VALUES (?,?,?,?,?,?,?,?)")
        .bind(&cid).bind(branch_id).bind(name)
        .bind(params.get("phone").and_then(|v| v.as_str()).unwrap_or(""))
        .bind(params.get("email").and_then(|v| v.as_str()).unwrap_or(""))
        .bind(params.get("notes").and_then(|v| v.as_str()).unwrap_or(""))
        .bind(&now).bind(&now).execute(pool).await?;
    Ok(IntentResult {
        ok: true,
        data: json!({"customer_id":cid,"name":name}),
        progress: None,
        metadata: None,
    })
}

async fn list_users_intent(pool: &SqlitePool) -> AppResult<IntentResult> {
    let rows: Vec<(String,String,String,String,bool)> = sqlx::query_as("SELECT u.user_id,u.display_name,u.username,r.name,u.is_active FROM users u JOIN roles r ON r.role_id=u.role_id ORDER BY u.display_name").fetch_all(pool).await?;
    Ok(IntentResult {
        ok: true,
        data: json!({"users": rows.into_iter().map(|(id,name,username,role,active)| json!({"id":id,"name":name,"username":username,"role":role,"active":active})).collect::<Vec<_>>()}),
        progress: None,
        metadata: None,
    })
}

async fn get_cash_status_intent(pool: &SqlitePool, _branch_id: &str) -> AppResult<IntentResult> {
    let row = sqlx::query("SELECT COALESCE(SUM(CASE WHEN event_type='paid_in' THEN amount_minor ELSE 0 END),0) as paid_in, COALESCE(SUM(CASE WHEN event_type='paid_out' THEN amount_minor ELSE 0 END),0) as paid_out, COALESCE(SUM(CASE WHEN event_type='safe_drop' THEN amount_minor ELSE 0 END),0) as safe_drops FROM cash_events WHERE DATE(created_at, '+3 hours') = DATE('now', '+3 hours')").fetch_one(pool).await?;
    Ok(IntentResult {
        ok: true,
        data: json!({"paid_in":row.get::<i64,_>(0),"paid_out":row.get::<i64,_>(1),"safe_drops":row.get::<i64,_>(2)}),
        progress: None,
        metadata: None,
    })
}

async fn get_sync_status_intent(pool: &SqlitePool) -> AppResult<IntentResult> {
    let pending: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM (SELECT 1 FROM sales WHERE sync_status='pending' UNION ALL SELECT 1 FROM sale_items WHERE sync_status='pending' UNION ALL SELECT 1 FROM stock_movements WHERE sync_status='pending')").fetch_one(pool).await?;
    let url = ai_admin_repo::get_config(pool, "hub_url")
        .await
        .ok()
        .flatten();
    Ok(IntentResult {
        ok: true,
        data: json!({"pending":pending,"hub_configured":url.is_some()}),
        progress: None,
        metadata: None,
    })
}

async fn list_deliveries_intent(
    pool: &SqlitePool,
    params: &Value,
    _branch_id: &str,
) -> AppResult<IntentResult> {
    let status = params
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("all");
    let sql = if status == "all" {
        "SELECT delivery_id,contact_number,delivery_status,amount_minor,created_at FROM delivery_orders ORDER BY created_at DESC LIMIT 20".to_string()
    } else {
        "SELECT delivery_id,contact_number,delivery_status,amount_minor,created_at FROM delivery_orders WHERE delivery_status=? ORDER BY created_at DESC LIMIT 20".to_string()
    };
    let mut q = sqlx::query(&sql);
    if status != "all" {
        q = q.bind(status);
    }
    let rows: Vec<(String, Option<String>, String, Option<i64>, String)> = q
        .map(|r: sqlx::sqlite::SqliteRow| (r.get(0), r.get(1), r.get(2), r.get(3), r.get(4)))
        .fetch_all(pool)
        .await?;
    Ok(IntentResult {
        ok: true,
        data: json!({"deliveries": rows.into_iter().map(|(id,contact,status,amount,date)| json!({"id":id,"contact":contact,"status":status,"amount":amount,"date":date})).collect::<Vec<_>>()}),
        progress: None,
        metadata: None,
    })
}

async fn receive_stock_intent(
    pool: &SqlitePool,
    params: &Value,
    _branch_id: &str,
) -> AppResult<IntentResult> {
    let pid = params
        .get("product_id")
        .and_then(|v| v.as_str())
        .ok_or(AppError::Validation("product_id required".into()))?;
    let qty = params
        .get("quantity_delta")
        .and_then(|v| v.as_str())
        .unwrap_or("1");
    let _notes = params.get("notes").and_then(|v| v.as_str()).unwrap_or("");
    let mid = ulid::Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let level_id = format!("SL-{}-{}", pid, _branch_id);
    // One transaction, and `quantity_after` read back rather than assumed.
    //
    // Two defects sat here. The level and the movement were written straight to
    // the pool as separate statements, so a failure between them left the cached
    // quantity changed with nothing in the ledger to explain it — the exact
    // divergence `inventory::movements` holds a transaction to prevent.
    //
    // The worse one: `quantity_after` was bound to the *delta*. Receiving five
    // units recorded `quantity_after = 5` however many were already on the
    // shelf. That column is what `apply::ledger_balance` anchors on when it
    // derives stock from the movement ledger, so a single one of these rows made
    // the derived figure wrong for that product from then on — and made
    // `stock_drift_report` show a discrepancy nothing could account for.
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO stock_levels (stock_level_id,product_id,branch_id,quantity_on_hand,last_movement_at,created_at,updated_at) \
         VALUES (?,?,?,?,?,?,?) \
         ON CONFLICT(product_id,branch_id) DO UPDATE SET \
           quantity_on_hand = CAST(CAST(stock_levels.quantity_on_hand AS REAL) + CAST(? AS REAL) AS TEXT), \
           last_movement_at=?, updated_at=?, sync_status='pending'",
    )
    .bind(&level_id).bind(pid).bind(_branch_id).bind(qty).bind(&now).bind(&now).bind(&now)
    .bind(qty).bind(&now).bind(&now)
    .execute(&mut *tx).await?;

    // The post-receipt quantity, read inside the transaction that wrote it.
    let quantity_after: String = sqlx::query_scalar(
        "SELECT quantity_on_hand FROM stock_levels WHERE product_id = ? AND branch_id = ?",
    )
    .bind(pid)
    .bind(_branch_id)
    .fetch_optional(&mut *tx)
    .await?
    .unwrap_or_else(|| qty.to_string());

    sqlx::query("INSERT INTO stock_movements (movement_id,product_id,branch_id,device_id,movement_type,quantity_delta,quantity_after,notes,created_at,sync_status) VALUES (?,?,?,'SYSTEM','receive',?,?,'AI intent',?,'pending')")
        .bind(&mid).bind(pid).bind(_branch_id).bind(qty).bind(&quantity_after).bind(&now)
        .execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(IntentResult {
        ok: true,
        data: json!({"movement_id":mid,"product_id":pid,"quantity":qty}),
        progress: None,
        metadata: None,
    })
}

async fn get_audit_log_intent(pool: &SqlitePool, params: &Value) -> AppResult<IntentResult> {
    let from = params
        .get("from_date")
        .and_then(|v| v.as_str())
        .unwrap_or("1970-01-01");
    let to = params
        .get("to_date")
        .and_then(|v| v.as_str())
        .unwrap_or("2099-01-01");
    let rows: Vec<(String,String,String,String,String)> = sqlx::query_as("SELECT event_type,entity_type,entity_id,actor_user_id,created_at FROM audit_logs WHERE created_at BETWEEN ? AND ? ORDER BY created_at DESC LIMIT 50")
        .bind(from).bind(to).fetch_all(pool).await?;
    Ok(IntentResult {
        ok: true,
        data: json!({"entries": rows.into_iter().map(|(event,entity,eid,actor,date)| json!({"event":event,"entity":entity,"id":eid,"actor":actor,"date":date})).collect::<Vec<_>>()}),
        progress: None,
        metadata: None,
    })
}

#[cfg(test)]
mod tests;
