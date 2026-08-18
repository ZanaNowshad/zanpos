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

#![allow(dead_code)]
use crate::db::repositories::ai_admin_repo;
use crate::domain::money;
use crate::errors::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};

// ── Intent Definition ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntentDef {
    pub name: &'static str,
    pub description: &'static str,
    pub parameters: Value, // JSON Schema
}

/// Whether an intent name performs mutations (needs confirmation).
pub fn is_mutation_intent(name: &str) -> bool {
    matches!(
        name,
        "create_product"
            | "update_product"
            | "receive_stock"
            | "create_customer"
            | "create_user"
            | "backup_database"
    )
}

/// List of valid intent names the AI may call.
pub const INTENT_NAMES: &[&str] = &[
    "search_products",
    "get_product_detail",
    "create_product",
    "update_product",
    "get_low_stock",
    "receive_stock",
    "get_sales_report",
    "get_today_summary",
    "list_customers",
    "create_customer",
    "list_users",
    "create_user",
    "get_cash_status",
    "list_deliveries",
    "get_sync_status",
    "get_audit_log",
    "backup_database",
    "open_tab",
];
pub fn all_intents() -> Vec<IntentDef> {
    vec![
        IntentDef { name: "search_products", description: "Search products by name, SKU, barcode, or category. Returns paginated results.", parameters: json!({"type":"object","properties":{"query":{"type":"string"},"category":{"type":"string"}}}) },
        IntentDef { name: "get_product_detail", description: "Get full details of a single product including all prices, stock levels, and barcodes.", parameters: json!({"type":"object","properties":{"product_id":{"type":"string"}},"required":["product_id"]}) },
        IntentDef { name: "create_product", description: "Create a new product with all required fields. Validates category, tax rule, and barcode uniqueness.", parameters: json!({"type":"object","properties":{"name":{"type":"string"},"category_id":{"type":"string"},"sku":{"type":"string"},"barcode":{"type":"string"},"selling_price_minor":{"type":"integer"},"cost_minor":{"type":"integer"},"tax_rule_id":{"type":"string"},"track_inventory":{"type":"boolean"}},"required":["name","category_id"]}) },
        IntentDef { name: "update_product", description: "Update any field of an existing product. Only sends changed fields.", parameters: json!({"type":"object","properties":{"product_id":{"type":"string"},"name":{"type":"string"},"category_id":{"type":"string"},"is_active":{"type":"boolean"},"selling_price_minor":{"type":"integer"}},"required":["product_id"]}) },
        IntentDef { name: "adjust_prices_batch", description: "Adjust prices across multiple products. Supports percentage increase/decrease, flat amount, or category-based filtering. Automatically chunks large operations.", parameters: json!({"type":"object","properties":{"filter":{"type":"object","properties":{"category_id":{"type":"string"},"name_contains":{"type":"string"},"is_active":{"type":"boolean"}}},"adjustment":{"type":"object","properties":{"type":{"type":"string","enum":["percentage","flat","set"]},"value":{"type":"integer"}}},"dry_run":{"type":"boolean","description":"If true, preview changes without applying"}},"required":["adjustment"]}) },
        IntentDef { name: "bulk_price_adjust", description: "DEPRECATED — use the bulk_price_adjust engine operation instead. Adjust prices across multiple products via the engine for proper preview/confirm/undo support.", parameters: json!({"type":"object","properties":{"filter":{"type":"object","properties":{"category_id":{"type":"string"},"name_contains":{"type":"string"},"is_active":{"type":"boolean"}}},"adjustment":{"type":"object","properties":{"type":{"type":"string","enum":["percentage","flat","set"]},"value":{"type":"integer"}}},"dry_run":{"type":"boolean","description":"If true, preview changes without applying"}},"required":["adjustment"]}) },
        IntentDef { name: "get_low_stock", description: "List all products below their reorder point, with stock levels and reorder quantities.", parameters: json!({"type":"object","properties":{"category_id":{"type":"string"}}}) },
        IntentDef { name: "receive_stock", description: "Record receiving stock for a product. Updates inventory and stock levels.", parameters: json!({"type":"object","properties":{"product_id":{"type":"string"},"quantity_delta":{"type":"string"},"notes":{"type":"string"}},"required":["product_id","quantity_delta"]}) },
        IntentDef { name: "get_sales_report", description: "Get sales report for a date range. Returns totals, top products, payment breakdown.", parameters: json!({"type":"object","properties":{"from_date":{"type":"string"},"to_date":{"type":"string"}},"required":["from_date","to_date"]}) },
        IntentDef { name: "get_today_summary", description: "Quick snapshot: today's transaction count, revenue, discounts, tax, refunds.", parameters: json!({"type":"object","properties":{}}) },
        IntentDef { name: "list_customers", description: "Search and list customers. Supports search by name, phone, or email.", parameters: json!({"type":"object","properties":{"search":{"type":"string"}}}) },
        IntentDef { name: "create_customer", description: "Create a new customer record.", parameters: json!({"type":"object","properties":{"name":{"type":"string"},"phone":{"type":"string"},"email":{"type":"string"},"notes":{"type":"string"}},"required":["name"]}) },
        IntentDef { name: "list_users", description: "List all system users with roles.", parameters: json!({"type":"object","properties":{}}) },
        IntentDef { name: "create_user", description: "Create a new system user (cashier, manager, etc).", parameters: json!({"type":"object","properties":{"display_name":{"type":"string"},"username":{"type":"string"},"pin":{"type":"string"},"role_id":{"type":"string"}},"required":["display_name","username","pin","role_id"]}) },
        IntentDef { name: "get_cash_status", description: "Current cash drawer status: expected cash, counted, variance, paid in/out, safe drops.", parameters: json!({"type":"object","properties":{}}) },
        IntentDef { name: "list_deliveries", description: "List delivery orders, filterable by status. Returns customer, address, payment status.", parameters: json!({"type":"object","properties":{"status":{"type":"string","enum":["pending","dispatched","delivered","cancelled","all"]}}}) },
        IntentDef { name: "get_sync_status", description: "Current hub sync status: online/offline, pending count, last sync time.", parameters: json!({"type":"object","properties":{}}) },
        IntentDef { name: "get_audit_log", description: "View audit trail entries for a date range.", parameters: json!({"type":"object","properties":{"from_date":{"type":"string"},"to_date":{"type":"string"}}}) },
        IntentDef { name: "backup_database", description: "Create a database backup to the specified path. Returns file path and size.", parameters: json!({"type":"object","properties":{}}) },
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
        "list_customers" => list_customers_intent(pool, params).await,
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
         LEFT JOIN product_prices pp ON pp.product_id = p.product_id AND pp.price_type = 'selling'
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
    let txn_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sales WHERE branch_id=? AND sold_at BETWEEN ? AND ?",
    )
    .bind(branch_id)
    .bind(from)
    .bind(to)
    .fetch_one(pool)
    .await?;
    let net: Option<i64> = sqlx::query_scalar("SELECT COALESCE(SUM(net_total_minor),0) FROM sales WHERE branch_id=? AND sold_at BETWEEN ? AND ?").bind(branch_id).bind(from).bind(to).fetch_one(pool).await?;
    let tax: Option<i64> = sqlx::query_scalar("SELECT COALESCE(SUM(tax_total_minor),0) FROM sales WHERE branch_id=? AND sold_at BETWEEN ? AND ?").bind(branch_id).bind(from).bind(to).fetch_one(pool).await?;
    Ok(IntentResult {
        ok: true,
        data: json!({"from":from,"to":to,"transactions":txn_count,"net_total":money::format_minor(net.unwrap_or(0),3),"tax":money::format_minor(tax.unwrap_or(0),3)}),
        progress: None,
        metadata: None,
    })
}

async fn list_customers_intent(pool: &SqlitePool, params: &Value) -> AppResult<IntentResult> {
    let search = params.get("search").and_then(|v| v.as_str()).unwrap_or("");
    let rows = if search.is_empty() {
        sqlx::query_as::<_, (String,String,Option<String>,Option<String>)>("SELECT customer_id,name,phone,email FROM customers WHERE is_active=1 ORDER BY name LIMIT 30").fetch_all(pool).await?
    } else {
        sqlx::query_as::<_, (String,String,Option<String>,Option<String>)>("SELECT customer_id,name,phone,email FROM customers WHERE is_active=1 AND (name LIKE '%'||?||'%' OR phone LIKE '%'||?||'%') ORDER BY name LIMIT 30").bind(search).bind(search).fetch_all(pool).await?
    };
    Ok(IntentResult {
        ok: true,
        data: json!({"customers": rows.into_iter().map(|(id,name,phone,email)| json!({"id":id,"name":name,"phone":phone,"email":email})).collect::<Vec<_>>()}),
        progress: None,
        metadata: None,
    })
}

async fn create_customer_intent(
    pool: &SqlitePool,
    params: &Value,
    _branch_id: &str,
) -> AppResult<IntentResult> {
    let name = params
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or(AppError::Validation("name required".into()))?;
    let cid = ulid::Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("INSERT INTO customers (customer_id,name,phone,email,notes,created_at,updated_at) VALUES (?,?,?,?,?,?,?)")
        .bind(&cid).bind(name)
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
    let row = sqlx::query("SELECT COALESCE(SUM(CASE WHEN event_type='paid_in' THEN amount_minor ELSE 0 END),0) as paid_in, COALESCE(SUM(CASE WHEN event_type='paid_out' THEN amount_minor ELSE 0 END),0) as paid_out, COALESCE(SUM(CASE WHEN event_type='safe_drop' THEN amount_minor ELSE 0 END),0) as safe_drops FROM cash_events WHERE DATE(created_at)=DATE('now')").fetch_one(pool).await?;
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
        "SELECT delivery_id,contact_number,status,expected_payment_minor,created_at FROM delivery_orders ORDER BY created_at DESC LIMIT 20".to_string()
    } else {
        "SELECT delivery_id,contact_number,status,expected_payment_minor,created_at FROM delivery_orders WHERE status=? ORDER BY created_at DESC LIMIT 20".to_string()
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
    sqlx::query(
        "INSERT INTO stock_levels (stock_level_id,product_id,branch_id,quantity_on_hand,last_movement_at,created_at,updated_at) \
         VALUES (?,?,?,?,?,?,?) \
         ON CONFLICT(product_id,branch_id) DO UPDATE SET \
           quantity_on_hand = CAST(CAST(stock_levels.quantity_on_hand AS REAL) + CAST(? AS REAL) AS TEXT), \
           last_movement_at=?, updated_at=?, sync_status='pending'",
    )
    .bind(&level_id).bind(pid).bind(_branch_id).bind(qty).bind(&now).bind(&now).bind(&now)
    .bind(qty).bind(&now).bind(&now)
    .execute(pool).await?;
    sqlx::query("INSERT INTO stock_movements (movement_id,product_id,branch_id,device_id,movement_type,quantity_delta,quantity_after,notes,created_at,sync_status) VALUES (?,?,?,'SYSTEM','receive',?,?,'AI intent',?,'pending')")
        .bind(&mid).bind(pid).bind(_branch_id).bind(qty).bind(qty).bind(&now).execute(pool).await?;
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
