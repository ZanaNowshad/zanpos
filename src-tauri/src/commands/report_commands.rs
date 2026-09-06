use crate::commands::{rbac, sync_commands};
use crate::db::repositories::audit_hash;
use crate::db::repositories::report_repo;
use crate::domain::report::TodaySummary;
use crate::errors::{AppError, AppResult};
use crate::sync::scope::report_scope;
use crate::AppState;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use tauri::State;
use tokio::io::AsyncWriteExt;

// ─── Range report types ───────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct RangeSummary {
    pub from_date: String,
    pub to_date: String,
    pub transaction_count: i64,
    pub gross_total_minor: i64,
    pub discount_total_minor: i64,
    pub tax_total_minor: i64,
    pub net_total_minor: i64,
    pub cash_total_minor: i64,
    pub card_total_minor: i64,
    pub refund_count: i64,
    pub refund_total_minor: i64,
    pub pending_delivery_count: i64,
    pub pending_delivery_minor: i64,
}

#[derive(Debug, Serialize)]
pub struct TopProduct {
    pub product_name: String,
    pub total_quantity: String,
    pub revenue_minor: i64,
    pub transaction_count: i64,
}

#[derive(Debug, Serialize)]
pub struct MarginSummary {
    pub from_date: String,
    pub to_date: String,
    pub transaction_count: i64,
    pub revenue_minor: i64,
    pub cogs_minor: i64,
    pub gross_margin_minor: i64,
    pub margin_basis_points: Option<i64>,
    pub unknown_cost_line_count: i64,
}

#[derive(Debug, Serialize)]
pub struct ProductMarginRow {
    pub product_id: Option<String>,
    pub product_name: String,
    pub quantity: String,
    pub revenue_minor: i64,
    pub cogs_minor: i64,
    pub gross_margin_minor: i64,
    pub margin_basis_points: Option<i64>,
    pub unknown_cost_line_count: i64,
}

#[derive(Debug, Serialize)]
pub struct SaleListRow {
    pub sale_id: String,
    pub receipt_number: String,
    pub sold_at: String,
    pub cashier_name: String,
    pub net_total_minor: i64,
    pub discount_total_minor: i64,
    pub status: String,
    pub payment_methods: String,
}

/// Paginated wrapper for the sales list.
/// `total` is the count of ALL matching rows (ignoring limit/offset) so the
/// caller can display page counts and detect truncation — fixes F-BIZ-002 / F-INT-001.
#[derive(Debug, Serialize)]
pub struct SaleListPage {
    pub items: Vec<SaleListRow>,
    pub total: i64,
    pub offset: i64,
    pub limit: i64,
}

#[derive(Debug, Serialize)]
pub struct SaleCursorPage {
    pub items: Vec<SaleListRow>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct SaleCursor {
    sold_at: String,
    sale_id: String,
}

fn encode_sale_cursor(sold_at: &str, sale_id: &str) -> AppResult<String> {
    let bytes = serde_json::to_vec(&SaleCursor {
        sold_at: sold_at.to_owned(),
        sale_id: sale_id.to_owned(),
    })
    .map_err(|error| AppError::Internal(format!("Failed to encode sales cursor: {error}")))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}

fn decode_sale_cursor(value: &str) -> AppResult<SaleCursor> {
    let bytes = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| AppError::Validation("Invalid sales cursor".into()))?;
    serde_json::from_slice(&bytes).map_err(|_| AppError::Validation("Invalid sales cursor".into()))
}

fn csv_cell(value: &str) -> String {
    let trimmed = value.trim_start();
    let safe = if matches!(trimmed.chars().next(), Some('=' | '+' | '-' | '@')) {
        format!("'{value}")
    } else {
        value.to_owned()
    };
    format!("\"{}\"", safe.replace('"', "\"\""))
}

#[tauri::command]
pub async fn report_today(
    session_token: String,
    branch_id: String,
    business_date: String,
    state: State<'_, AppState>,
) -> Result<TodaySummary, AppError> {
    let actor = rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::ANY_ROLE)
        .await?;
    // The `branch_id` argument is caller-supplied and therefore not trusted:
    // reports expose financial data, so the scope is resolved from the actor's
    // own record and the incoming value is discarded. The parameter stays in
    // the signature only to keep the existing invoke contract.
    let _ = branch_id;
    let branch_id = actor.branch_id.clone();
    report_repo::today_summary(&state.db, &branch_id, &business_date).await
}

// ─── Date-range commands ──────────────────────────────────────────────────────

#[tauri::command]
pub async fn report_date_range(
    session_token: String,
    branch_id: String,
    from_date: String,
    to_date: String,
    state: State<'_, AppState>,
) -> Result<RangeSummary, AppError> {
    let actor = rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::ANY_ROLE)
        .await?;
    // The `branch_id` argument is caller-supplied and therefore not trusted:
    // reports expose financial data, so the scope is resolved from the actor's
    // own record and the incoming value is discarded. The parameter stays in
    // the signature only to keep the existing invoke contract.
    let _ = branch_id;
    let branch_id = actor.branch_id.clone();
    // M6: Run the 5 independent range aggregation queries concurrently.
    // E: apply the device-scope filter uniformly: scope='all' short-circuits
    // the OR; scope='origin' requires origin_device_id to match this device.
    let pool = &state.db;
    let (scope, origin_device_id) = report_scope(pool).await;
    let scope_str = scope.as_str();
    let (sales_row, cash, card, pending_row, refund_row) = tokio::try_join!(
        sqlx::query(
            "SELECT COUNT(*) AS cnt,
                    COALESCE(SUM(s.gross_total_minor),    0) AS gross,
                    COALESCE(SUM(s.discount_total_minor), 0) AS discount,
                    COALESCE(SUM(s.tax_total_minor),      0) AS tax,
                    COALESCE(SUM(s.net_total_minor),      0) AS net
             FROM sales s
             WHERE s.branch_id = ? AND s.business_date BETWEEN ? AND ?
               AND s.status != 'voided'
               AND (s.is_delivery = 0 OR EXISTS (
                   SELECT 1 FROM delivery_orders d WHERE d.sale_id = s.sale_id AND d.payment_status = 'paid'
               ))
               AND (? = 'all' OR s.origin_device_id = ?)",
        )
        .bind(&branch_id).bind(&from_date).bind(&to_date)
        .bind(scope_str).bind(&origin_device_id)
        .fetch_one(pool),
        sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(SUM(p.amount_minor), 0)
             FROM payments p JOIN sales s ON s.sale_id = p.sale_id
             WHERE s.branch_id = ? AND s.business_date BETWEEN ? AND ?
               AND s.status != 'voided' AND p.payment_method = 'cash'
               AND (s.is_delivery = 0 OR EXISTS (
                   SELECT 1 FROM delivery_orders d WHERE d.sale_id = s.sale_id AND d.payment_status = 'paid'
               ))
               AND (? = 'all' OR s.origin_device_id = ?)",
        )
        .bind(&branch_id).bind(&from_date).bind(&to_date)
        .bind(scope_str).bind(&origin_device_id)
        .fetch_one(pool),
        sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(SUM(p.amount_minor), 0)
             FROM payments p JOIN sales s ON s.sale_id = p.sale_id
             WHERE s.branch_id = ? AND s.business_date BETWEEN ? AND ?
               AND s.status != 'voided' AND p.payment_method = 'card'
               AND (s.is_delivery = 0 OR EXISTS (
                   SELECT 1 FROM delivery_orders d WHERE d.sale_id = s.sale_id AND d.payment_status = 'paid'
               ))
               AND (? = 'all' OR s.origin_device_id = ?)",
        )
        .bind(&branch_id).bind(&from_date).bind(&to_date)
        .bind(scope_str).bind(&origin_device_id)
        .fetch_one(pool),
        sqlx::query(
            "SELECT COUNT(*) AS cnt, COALESCE(SUM(s.net_total_minor), 0) AS total
             FROM sales s
             WHERE s.branch_id = ? AND s.business_date BETWEEN ? AND ?
               AND s.status != 'voided' AND s.is_delivery = 1
               AND NOT EXISTS (
                   SELECT 1 FROM delivery_orders d
                   WHERE d.sale_id = s.sale_id AND d.payment_status = 'paid'
               )
               AND (? = 'all' OR s.origin_device_id = ?)",
        )
        .bind(&branch_id).bind(&from_date).bind(&to_date)
        .bind(scope_str).bind(&origin_device_id)
        .fetch_one(pool),
        sqlx::query(
            "SELECT COUNT(*) AS cnt, COALESCE(SUM(r.refund_total_minor), 0) AS total
             FROM refunds r JOIN sales s ON s.sale_id = r.original_sale_id
             WHERE s.branch_id = ? AND s.business_date BETWEEN ? AND ?
               AND (? = 'all' OR r.origin_device_id = ?)",
        )
        .bind(&branch_id).bind(&from_date).bind(&to_date)
        .bind(scope_str).bind(&origin_device_id)
        .fetch_one(pool),
    )?;

    let (pending_count, pending_minor): (i64, i64) =
        (pending_row.get("cnt"), pending_row.get("total"));

    Ok(RangeSummary {
        from_date,
        to_date,
        transaction_count: sales_row.get("cnt"),
        gross_total_minor: sales_row.get("gross"),
        discount_total_minor: sales_row.get("discount"),
        tax_total_minor: sales_row.get("tax"),
        net_total_minor: sales_row.get("net"),
        cash_total_minor: cash,
        card_total_minor: card,
        refund_count: refund_row.get("cnt"),
        refund_total_minor: refund_row.get("total"),
        pending_delivery_count: pending_count,
        pending_delivery_minor: pending_minor,
    })
}

#[tauri::command]
pub async fn report_top_products(
    session_token: String,
    branch_id: String,
    from_date: String,
    to_date: String,
    state: State<'_, AppState>,
) -> Result<Vec<TopProduct>, AppError> {
    let actor = rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::ANY_ROLE)
        .await?;
    // The `branch_id` argument is caller-supplied and therefore not trusted:
    // reports expose financial data, so the scope is resolved from the actor's
    // own record and the incoming value is discarded. The parameter stays in
    // the signature only to keep the existing invoke contract.
    let _ = branch_id;
    let branch_id = actor.branch_id.clone();
    let pool = &state.db;
    let (scope, origin_device_id) = report_scope(pool).await;
    let rows = sqlx::query(
        "SELECT si.product_name_snapshot AS product_name,
                CAST(SUM(CAST(si.quantity AS REAL)) AS TEXT) AS total_quantity,
                SUM(si.line_total_minor)    AS revenue_minor,
                COUNT(DISTINCT si.sale_id)  AS transaction_count
         FROM sale_items si
         JOIN sales s ON s.sale_id = si.sale_id
         WHERE s.branch_id = ? AND s.business_date BETWEEN ? AND ?
           AND s.status != 'voided' AND si.voided = 0
           AND (? = 'all' OR s.origin_device_id = ?)
         GROUP BY si.product_name_snapshot
         ORDER BY revenue_minor DESC
         LIMIT 15",
    )
    .bind(&branch_id)
    .bind(&from_date)
    .bind(&to_date)
    .bind(scope.as_str())
    .bind(&origin_device_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| TopProduct {
            product_name: r.get("product_name"),
            total_quantity: r.get("total_quantity"),
            revenue_minor: r.get("revenue_minor"),
            transaction_count: r.get("transaction_count"),
        })
        .collect())
}

#[tauri::command]
pub async fn report_margin(
    session_token: String,
    branch_id: String,
    from_date: String,
    to_date: String,
    state: State<'_, AppState>,
) -> Result<MarginSummary, AppError> {
    let actor = rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::MANAGER_OR_OWNER)
        .await?;
    // The `branch_id` argument is caller-supplied and therefore not trusted:
    // reports expose financial data, so the scope is resolved from the actor's
    // own record and the incoming value is discarded. The parameter stays in
    // the signature only to keep the existing invoke contract.
    let _ = branch_id;
    let branch_id = actor.branch_id.clone();
    report_margin_inner(&state.db, &branch_id, &from_date, &to_date).await
}

#[tauri::command]
pub async fn report_product_margin(
    session_token: String,
    branch_id: String,
    from_date: String,
    to_date: String,
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<Vec<ProductMarginRow>, AppError> {
    let actor = rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::MANAGER_OR_OWNER)
        .await?;
    // The `branch_id` argument is caller-supplied and therefore not trusted:
    // reports expose financial data, so the scope is resolved from the actor's
    // own record and the incoming value is discarded. The parameter stays in
    // the signature only to keep the existing invoke contract.
    let _ = branch_id;
    let branch_id = actor.branch_id.clone();
    report_product_margin_inner(
        &state.db,
        &branch_id,
        &from_date,
        &to_date,
        limit.unwrap_or(50).clamp(1, 200),
    )
    .await
}

async fn report_margin_inner(
    pool: &SqlitePool,
    branch_id: &str,
    from_date: &str,
    to_date: &str,
) -> Result<MarginSummary, AppError> {
    let (scope, origin_device_id) = report_scope(pool).await;
    let row = sqlx::query(
        "SELECT COUNT(DISTINCT s.sale_id) AS transaction_count,
                COALESCE(SUM(si.line_total_minor - si.tax_amount_minor), 0) AS revenue_minor,
                COALESCE(SUM(
                    CASE
                      WHEN si.cost_minor_snapshot IS NULL THEN 0
                      ELSE CAST(ROUND(CAST(si.quantity AS REAL) * si.cost_minor_snapshot) AS INTEGER)
                    END
                ), 0) AS cogs_minor,
                COALESCE(SUM(CASE WHEN si.cost_minor_snapshot IS NULL THEN 1 ELSE 0 END), 0) AS unknown_cost_line_count
         FROM sales s
         JOIN sale_items si ON si.sale_id = s.sale_id
         WHERE s.branch_id = ? AND s.business_date BETWEEN ? AND ?
           AND s.status != 'voided' AND si.voided = 0
           AND (s.is_delivery = 0 OR EXISTS (
               SELECT 1 FROM delivery_orders d WHERE d.sale_id = s.sale_id AND d.payment_status = 'paid'
           ))
           AND (? = 'all' OR s.origin_device_id = ?)",
    )
    .bind(branch_id)
    .bind(from_date)
    .bind(to_date)
    .bind(scope.as_str())
    .bind(&origin_device_id)
    .fetch_one(pool)
    .await?;

    let revenue_minor: i64 = row.get("revenue_minor");
    let cogs_minor: i64 = row.get("cogs_minor");
    Ok(MarginSummary {
        from_date: from_date.to_string(),
        to_date: to_date.to_string(),
        transaction_count: row.get("transaction_count"),
        revenue_minor,
        cogs_minor,
        gross_margin_minor: revenue_minor - cogs_minor,
        margin_basis_points: margin_basis_points(revenue_minor, cogs_minor),
        unknown_cost_line_count: row.get("unknown_cost_line_count"),
    })
}

async fn report_product_margin_inner(
    pool: &SqlitePool,
    branch_id: &str,
    from_date: &str,
    to_date: &str,
    limit: i64,
) -> Result<Vec<ProductMarginRow>, AppError> {
    let (scope, origin_device_id) = report_scope(pool).await;
    let rows = sqlx::query(
        "SELECT si.product_id AS product_id,
                si.product_name_snapshot AS product_name,
                CAST(SUM(CAST(si.quantity AS REAL)) AS TEXT) AS quantity,
                COALESCE(SUM(si.line_total_minor - si.tax_amount_minor), 0) AS revenue_minor,
                COALESCE(SUM(
                    CASE
                      WHEN si.cost_minor_snapshot IS NULL THEN 0
                      ELSE CAST(ROUND(CAST(si.quantity AS REAL) * si.cost_minor_snapshot) AS INTEGER)
                    END
                ), 0) AS cogs_minor,
                COALESCE(SUM(CASE WHEN si.cost_minor_snapshot IS NULL THEN 1 ELSE 0 END), 0) AS unknown_cost_line_count
         FROM sales s
         JOIN sale_items si ON si.sale_id = s.sale_id
         WHERE s.branch_id = ? AND s.business_date BETWEEN ? AND ?
           AND s.status != 'voided' AND si.voided = 0
           AND (s.is_delivery = 0 OR EXISTS (
               SELECT 1 FROM delivery_orders d WHERE d.sale_id = s.sale_id AND d.payment_status = 'paid'
           ))
           AND (? = 'all' OR s.origin_device_id = ?)
         GROUP BY si.product_id, si.product_name_snapshot
         ORDER BY (revenue_minor - cogs_minor) DESC, revenue_minor DESC
         LIMIT ?",
    )
    .bind(branch_id)
    .bind(from_date)
    .bind(to_date)
    .bind(scope.as_str())
    .bind(&origin_device_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| {
            let revenue_minor: i64 = r.get("revenue_minor");
            let cogs_minor: i64 = r.get("cogs_minor");
            ProductMarginRow {
                product_id: r.get("product_id"),
                product_name: r.get("product_name"),
                quantity: r.get("quantity"),
                revenue_minor,
                cogs_minor,
                gross_margin_minor: revenue_minor - cogs_minor,
                margin_basis_points: margin_basis_points(revenue_minor, cogs_minor),
                unknown_cost_line_count: r.get("unknown_cost_line_count"),
            }
        })
        .collect())
}

fn margin_basis_points(revenue_minor: i64, cogs_minor: i64) -> Option<i64> {
    if revenue_minor <= 0 {
        None
    } else {
        Some(((revenue_minor - cogs_minor) * 10_000) / revenue_minor)
    }
}

#[tauri::command]
pub async fn report_sales_list(
    session_token: String,
    branch_id: String,
    from_date: String,
    to_date: String,
    offset: Option<i64>,
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<SaleListPage, AppError> {
    let actor = rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::ANY_ROLE)
        .await?;
    // The `branch_id` argument is caller-supplied and therefore not trusted:
    // reports expose financial data, so the scope is resolved from the actor's
    // own record and the incoming value is discarded. The parameter stays in
    // the signature only to keep the existing invoke contract.
    let _ = branch_id;
    let branch_id = actor.branch_id.clone();
    let limit = limit.unwrap_or(200).clamp(1, 500);
    let offset = offset.unwrap_or(0).max(0);

    let pool = &state.db;
    let (scope, origin_device_id) = report_scope(pool).await;

    // M13: COUNT must use identical JOINs/WHERE as the data query to avoid
    // pagination totals diverging from actual row counts. Use LEFT JOIN users
    // in both to count even if the cashier account was later deleted.
    // FIX: exclude voided sales from count — data query also excludes them implicitly
    // (voided show in list but are filtered by status). Count must match displayed rows.
    let total: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT s.sale_id)
         FROM sales s
         LEFT JOIN users u ON u.user_id = s.cashier_user_id
         WHERE s.branch_id = ? AND s.business_date BETWEEN ? AND ?
           AND (? = 'all' OR s.origin_device_id = ?)
           AND s.status != 'voided'",
    )
    .bind(&branch_id)
    .bind(&from_date)
    .bind(&to_date)
    .bind(scope.as_str())
    .bind(&origin_device_id)
    .fetch_one(pool)
    .await?;

    let rows = sqlx::query(
        "SELECT s.sale_id, s.receipt_number, s.sold_at,
                s.net_total_minor, s.discount_total_minor, s.status,
                COALESCE(u.display_name, '(deleted)') AS cashier_name,
                GROUP_CONCAT(DISTINCT p.payment_method) AS payment_methods
         FROM sales s
         LEFT JOIN users u ON u.user_id = s.cashier_user_id
         LEFT JOIN payments p ON p.sale_id = s.sale_id
         WHERE s.branch_id = ? AND s.business_date BETWEEN ? AND ?
           AND (? = 'all' OR s.origin_device_id = ?)
           AND s.status != 'voided'
         GROUP BY s.sale_id
         ORDER BY s.sold_at DESC
         LIMIT ? OFFSET ?",
    )
    .bind(&branch_id)
    .bind(&from_date)
    .bind(&to_date)
    .bind(scope.as_str())
    .bind(&origin_device_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    let items = rows
        .iter()
        .map(|r| {
            let methods: Option<String> = r.get("payment_methods");
            SaleListRow {
                sale_id: r.get("sale_id"),
                receipt_number: r.get("receipt_number"),
                sold_at: r.get("sold_at"),
                cashier_name: r.get("cashier_name"),
                net_total_minor: r.get("net_total_minor"),
                discount_total_minor: r.get("discount_total_minor"),
                status: r.get("status"),
                payment_methods: methods.unwrap_or_default(),
            }
        })
        .collect();

    Ok(SaleListPage {
        items,
        total,
        offset,
        limit,
    })
}

async fn fetch_sales_cursor_page(
    pool: &SqlitePool,
    branch_id: &str,
    from_date: &str,
    to_date: &str,
    scope: &str,
    origin_device_id: &str,
    cursor: Option<&SaleCursor>,
    limit: i64,
) -> AppResult<SaleCursorPage> {
    let fetch_limit = limit.clamp(1, 1_000) + 1;
    let rows = if let Some(cursor) = cursor {
        sqlx::query(
            "SELECT s.sale_id, s.receipt_number, s.sold_at,
                    s.net_total_minor, s.discount_total_minor, s.status,
                    COALESCE(u.display_name, '(deleted)') AS cashier_name,
                    GROUP_CONCAT(DISTINCT p.payment_method) AS payment_methods
             FROM sales s
             LEFT JOIN users u ON u.user_id = s.cashier_user_id
             LEFT JOIN payments p ON p.sale_id = s.sale_id
             WHERE s.branch_id = ? AND s.business_date BETWEEN ? AND ?
               AND (? = 'all' OR s.origin_device_id = ?)
               AND s.status != 'voided'
               AND (s.sold_at < ? OR (s.sold_at = ? AND s.sale_id < ?))
             GROUP BY s.sale_id
             ORDER BY s.sold_at DESC, s.sale_id DESC
             LIMIT ?",
        )
        .bind(branch_id)
        .bind(from_date)
        .bind(to_date)
        .bind(scope)
        .bind(origin_device_id)
        .bind(&cursor.sold_at)
        .bind(&cursor.sold_at)
        .bind(&cursor.sale_id)
        .bind(fetch_limit)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            "SELECT s.sale_id, s.receipt_number, s.sold_at,
                    s.net_total_minor, s.discount_total_minor, s.status,
                    COALESCE(u.display_name, '(deleted)') AS cashier_name,
                    GROUP_CONCAT(DISTINCT p.payment_method) AS payment_methods
             FROM sales s
             LEFT JOIN users u ON u.user_id = s.cashier_user_id
             LEFT JOIN payments p ON p.sale_id = s.sale_id
             WHERE s.branch_id = ? AND s.business_date BETWEEN ? AND ?
               AND (? = 'all' OR s.origin_device_id = ?)
               AND s.status != 'voided'
             GROUP BY s.sale_id
             ORDER BY s.sold_at DESC, s.sale_id DESC
             LIMIT ?",
        )
        .bind(branch_id)
        .bind(from_date)
        .bind(to_date)
        .bind(scope)
        .bind(origin_device_id)
        .bind(fetch_limit)
        .fetch_all(pool)
        .await?
    };

    let has_more = rows.len() as i64 > limit.clamp(1, 1_000);
    let mut items: Vec<SaleListRow> = rows
        .into_iter()
        .take(limit.clamp(1, 1_000) as usize)
        .map(|row| SaleListRow {
            sale_id: row.get("sale_id"),
            receipt_number: row.get("receipt_number"),
            sold_at: row.get("sold_at"),
            cashier_name: row.get("cashier_name"),
            net_total_minor: row.get("net_total_minor"),
            discount_total_minor: row.get("discount_total_minor"),
            status: row.get("status"),
            payment_methods: row
                .get::<Option<String>, _>("payment_methods")
                .unwrap_or_default(),
        })
        .collect();
    let next_cursor = if has_more {
        items
            .last()
            .map(|last| encode_sale_cursor(&last.sold_at, &last.sale_id))
            .transpose()?
    } else {
        None
    };

    Ok(SaleCursorPage {
        items: std::mem::take(&mut items),
        next_cursor,
        has_more,
    })
}

/// Keyset pagination stays O(page size) even deep into million-row reports.
#[tauri::command]
pub async fn report_sales_cursor(
    session_token: String,
    branch_id: String,
    from_date: String,
    to_date: String,
    cursor: Option<String>,
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<SaleCursorPage, AppError> {
    let actor = rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::ANY_ROLE)
        .await?;
    let _ = branch_id;
    let branch_id = actor.branch_id.clone();
    let decoded = cursor.as_deref().map(decode_sale_cursor).transpose()?;
    let (scope, origin_device_id) = report_scope(&state.db).await;
    fetch_sales_cursor_page(
        &state.db,
        &branch_id,
        &from_date,
        &to_date,
        scope.as_str(),
        &origin_device_id,
        decoded.as_ref(),
        limit.unwrap_or(200),
    )
    .await
}

/// Writes rows incrementally in bounded keyset pages. The complete report is
/// never materialized in the webview or Rust heap.
#[tauri::command]
pub async fn report_sales_export_csv(
    session_token: String,
    branch_id: String,
    from_date: String,
    to_date: String,
    dest_path: String,
    state: State<'_, AppState>,
) -> Result<i64, AppError> {
    let actor = rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::ANY_ROLE)
        .await?;
    let _ = branch_id;
    let branch_id = actor.branch_id.clone();
    let path = std::path::Path::new(&dest_path);
    if !path.is_absolute()
        || path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_ascii_lowercase)
            != Some("csv".into())
    {
        return Err(AppError::Validation(
            "Choose an absolute destination ending in .csv".into(),
        ));
    }
    let mut file = tokio::fs::File::create(path)
        .await
        .map_err(|error| AppError::Internal(format!("Could not create CSV export: {error}")))?;
    file.write_all(
        b"\xEF\xBB\xBF\"receipt\",\"sold_at\",\"cashier\",\"payment_methods\",\"discount_minor\",\"net_total_minor\",\"status\"\r\n",
    )
    .await
    .map_err(|error| AppError::Internal(format!("Could not write CSV export: {error}")))?;

    let (scope, origin_device_id) = report_scope(&state.db).await;
    let mut cursor: Option<SaleCursor> = None;
    let mut exported = 0_i64;
    loop {
        let page = fetch_sales_cursor_page(
            &state.db,
            &branch_id,
            &from_date,
            &to_date,
            scope.as_str(),
            &origin_device_id,
            cursor.as_ref(),
            500,
        )
        .await?;
        let mut chunk = String::with_capacity(page.items.len() * 160);
        for sale in &page.items {
            let fields = [
                csv_cell(&sale.receipt_number),
                csv_cell(&sale.sold_at),
                csv_cell(&sale.cashier_name),
                csv_cell(&sale.payment_methods),
                sale.discount_total_minor.to_string(),
                sale.net_total_minor.to_string(),
                csv_cell(&sale.status),
            ];
            chunk.push_str(&fields.join(","));
            chunk.push_str("\r\n");
        }
        file.write_all(chunk.as_bytes())
            .await
            .map_err(|error| AppError::Internal(format!("Could not write CSV export: {error}")))?;
        exported += page.items.len() as i64;
        match page.next_cursor {
            Some(next) => cursor = Some(decode_sale_cursor(&next)?),
            None => break,
        }
    }
    file.flush()
        .await
        .map_err(|error| AppError::Internal(format!("Could not finish CSV export: {error}")))?;
    Ok(exported)
}

// ─── Sales by cashier ─────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct CashierSummaryRow {
    pub cashier_user_id: String,
    pub cashier_name: String,
    pub transaction_count: i64,
    pub net_total_minor: i64,
    pub cash_total_minor: i64,
    pub card_total_minor: i64,
    pub discount_total_minor: i64,
    pub refund_count: i64,
    pub refund_total_minor: i64,
}

/// Sales totals grouped by cashier for a date range.
/// Single CTE query — avoids the prior N+1 (4 queries per cashier).
#[tauri::command]
pub async fn report_by_cashier(
    session_token: String,
    branch_id: String,
    from_date: String,
    to_date: String,
    state: State<'_, AppState>,
) -> Result<Vec<CashierSummaryRow>, AppError> {
    let actor = rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::ANY_ROLE)
        .await?;
    // The `branch_id` argument is caller-supplied and therefore not trusted:
    // reports expose financial data, so the scope is resolved from the actor's
    // own record and the incoming value is discarded. The parameter stays in
    // the signature only to keep the existing invoke contract.
    let _ = branch_id;
    let branch_id = actor.branch_id.clone();
    let pool = &state.db;
    let (scope, origin_device_id) = report_scope(pool).await;
    let scope_str = scope.as_str();
    // payment_totals: cash and card per cashier, pre-aggregated before joining
    // to avoid inflating sale-level sums when a sale has multiple payment rows.
    // refund_totals: similarly pre-aggregated to avoid double-counting on sales
    // with multiple refunds.
    let rows = sqlx::query(
        "WITH payment_totals AS (
             SELECT s.cashier_user_id,
                    COALESCE(SUM(CASE WHEN p.payment_method = 'cash' THEN p.amount_minor ELSE 0 END), 0) AS cash_total,
                    COALESCE(SUM(CASE WHEN p.payment_method = 'card' THEN p.amount_minor ELSE 0 END), 0) AS card_total
             FROM sales s
             JOIN payments p ON p.sale_id = s.sale_id
             WHERE s.branch_id = ? AND s.business_date BETWEEN ? AND ?
               AND s.status != 'voided'
               AND (s.is_delivery = 0 OR EXISTS (SELECT 1 FROM delivery_orders d WHERE d.sale_id = s.sale_id AND d.payment_status = 'paid'))
               AND (? = 'all' OR s.origin_device_id = ?)
             GROUP BY s.cashier_user_id
         ),
         refund_totals AS (
             SELECT s.cashier_user_id,
                    COUNT(r.refund_id)                         AS refund_count,
                    COALESCE(SUM(r.refund_total_minor), 0)     AS refund_total
             FROM sales s
             JOIN refunds r ON r.original_sale_id = s.sale_id
             WHERE s.branch_id = ? AND s.business_date BETWEEN ? AND ?
               AND (? = 'all' OR r.origin_device_id = ?)
             GROUP BY s.cashier_user_id
         )
         SELECT s.cashier_user_id,
                COALESCE(u.display_name, s.cashier_user_id)   AS cashier_name,
                COUNT(s.sale_id)                               AS transaction_count,
                COALESCE(SUM(s.net_total_minor),      0)       AS net_total_minor,
                COALESCE(SUM(s.discount_total_minor), 0)       AS discount_total_minor,
                COALESCE(pt.cash_total,  0)                    AS cash_total_minor,
                COALESCE(pt.card_total,  0)                    AS card_total_minor,
                COALESCE(rt.refund_count, 0)                   AS refund_count,
                COALESCE(rt.refund_total, 0)                   AS refund_total_minor
         FROM sales s
         LEFT JOIN users u         ON u.user_id         = s.cashier_user_id
         LEFT JOIN payment_totals pt ON pt.cashier_user_id = s.cashier_user_id
         LEFT JOIN refund_totals  rt ON rt.cashier_user_id = s.cashier_user_id
         WHERE s.branch_id = ? AND s.business_date BETWEEN ? AND ?
           AND s.status != 'voided'
           AND (s.is_delivery = 0 OR EXISTS (SELECT 1 FROM delivery_orders d WHERE d.sale_id = s.sale_id AND d.payment_status = 'paid'))
           AND (? = 'all' OR s.origin_device_id = ?)
         GROUP BY s.cashier_user_id
         ORDER BY net_total_minor DESC",
    )
    .bind(&branch_id) // payment_totals WHERE
    .bind(&from_date)
    .bind(&to_date)
    .bind(scope_str)
    .bind(&origin_device_id)
    .bind(&branch_id) // refund_totals WHERE
    .bind(&from_date)
    .bind(&to_date)
    .bind(scope_str)
    .bind(&origin_device_id)
    .bind(&branch_id) // main WHERE
    .bind(&from_date)
    .bind(&to_date)
    .bind(scope_str)
    .bind(&origin_device_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|r| CashierSummaryRow {
            cashier_user_id: r.get("cashier_user_id"),
            cashier_name: r.get("cashier_name"),
            transaction_count: r.get("transaction_count"),
            net_total_minor: r.get("net_total_minor"),
            cash_total_minor: r.get("cash_total_minor"),
            card_total_minor: r.get("card_total_minor"),
            discount_total_minor: r.get("discount_total_minor"),
            refund_count: r.get("refund_count"),
            refund_total_minor: r.get("refund_total_minor"),
        })
        .collect())
}

// ─── End-of-day cash-up report ────────────────────────────────────────────────

/// Per-tax-rule breakdown row for Z/EOD reports (BUG-3).
#[derive(Debug, Serialize)]
pub struct TaxBreakdownRow {
    /// e.g. "VAT 10%" or "Zero-rated"
    pub tax_rule_name: String,
    /// Basis points (1000 = 10 %)
    pub rate_basis_points: i64,
    /// Net taxable sales amount for this rule (minor units)
    pub net_sales_minor: i64,
    /// Tax collected for this rule (minor units)
    pub tax_collected_minor: i64,
}

#[derive(Debug, Serialize)]
pub struct EodShiftRow {
    pub shift_id: String,
    pub cashier_name: String,
    pub opened_at: String,
    pub closed_at: Option<String>,
    pub opening_minor: i64,
    // ── Cash drawer ──────────────────────────────────────────────────────────
    pub cash_sales_minor: i64,
    pub safe_drop_minor: i64,
    pub paid_in_minor: i64,
    pub paid_out_minor: i64,
    pub expected_minor: i64,
    pub counted_minor: Option<i64>,
    pub variance_minor: Option<i64>,
    // ── Sales totals (BUG-3: all payment methods + gross/discount/refund) ───
    pub gross_sales_minor: i64,
    pub discount_total_minor: i64,
    pub net_sales_minor: i64,
    /// All non-cash, non-card payment methods (wallet, other) combined.
    pub card_sales_minor: i64,
    pub other_sales_minor: i64,
    pub refund_count: i64,
    pub refund_total_minor: i64,
    // ── Tax breakdown per rule (BUG-3) ────────────────────────────────────
    pub tax_by_rule: Vec<TaxBreakdownRow>,
}

#[derive(Debug, Serialize)]
pub struct EodCashupReport {
    pub date: String,
    pub shifts: Vec<EodShiftRow>,
    pub total_net_minor: i64,
    pub total_cash_minor: i64,
    pub total_card_minor: i64,
    pub total_other_minor: i64,
    pub total_gross_minor: i64,
    pub total_discount_minor: i64,
    pub total_refund_minor: i64,
    pub total_counted_minor: Option<i64>,
    pub total_variance_minor: Option<i64>,
}

/// Inner EOD cashup implementation — testable without AppState.
pub(crate) async fn report_eod_cashup_inner(
    pool: &SqlitePool,
    branch_id: &str,
    date: &str,
    _date_to: &str,
) -> AppResult<EodCashupReport> {
    // Use business_date (local date populated at shift-open time) rather than
    // DATE(opened_at) which is UTC and misattributes Bahrain late-night shifts
    // (opened between midnight and 03:00 local / 21:00–00:00 UTC) to the wrong day.
    // Fallback: DATE(opened_at, '+3 hours') covers rows backfilled from 0021.
    let (scope, origin_device_id) = report_scope(pool).await;
    let scope_str = scope.as_str();
    let shifts = sqlx::query(
        "SELECT s.shift_id, COALESCE(u.display_name,'Unknown') AS cashier_name,
                s.opened_at, s.closed_at,
                s.opening_cash_minor, s.counted_cash_minor
         FROM shifts s
         LEFT JOIN users u ON u.user_id = s.cashier_user_id
         WHERE s.branch_id = ?
           AND COALESCE(s.business_date, DATE(s.opened_at, '+3 hours')) = ?
           AND (? = 'all' OR s.origin_device_id = ?)
         ORDER BY s.opened_at ASC",
    )
    .bind(branch_id)
    .bind(date)
    .bind(scope_str)
    .bind(&origin_device_id)
    .fetch_all(pool)
    .await?;

    let mut rows: Vec<EodShiftRow> = Vec::new();
    let mut total_net: i64 = 0;
    let mut total_cash: i64 = 0;
    let mut total_card: i64 = 0;
    let mut total_other: i64 = 0;
    let mut total_gross: i64 = 0;
    let mut total_discount: i64 = 0;
    let mut total_refund: i64 = 0;
    let mut total_counted: Option<i64> = Some(0);
    let mut all_counted = true;

    for sh in &shifts {
        let shift_id: String = sh.get("shift_id");
        let opening: i64 = sh.get("opening_cash_minor");
        let counted: Option<i64> = sh.get("counted_cash_minor");

        // Sales totals: gross, discount, net (paid deliveries only)
        let sales_totals_row = sqlx::query(
            "SELECT COALESCE(SUM(s.gross_total_minor),0)    AS gross,
                    COALESCE(SUM(s.discount_total_minor),0) AS discount,
                    COALESCE(SUM(s.net_total_minor),0)      AS net
             FROM sales s
             WHERE s.shift_id=? AND s.status!='voided'
               AND (s.is_delivery = 0 OR EXISTS (
                   SELECT 1 FROM delivery_orders d WHERE d.sale_id = s.sale_id AND d.payment_status = 'paid'
               ))",
        )
        .bind(&shift_id)
        .fetch_one(pool)
        .await?;
        let gross_sales: i64 = sales_totals_row.get("gross");
        let discount_total: i64 = sales_totals_row.get("discount");
        let net_sales: i64 = sales_totals_row.get("net");

        // BUG-3: All payment methods — query dynamically, then split into
        // cash / card / other so the struct stays typed and frontend-friendly.
        let payment_rows = sqlx::query(
            "SELECT p.payment_method,
                    COALESCE(SUM(p.amount_minor),0) AS total
             FROM payments p
             JOIN sales s ON s.sale_id = p.sale_id
             WHERE s.shift_id=? AND s.status!='voided'
               AND (s.is_delivery = 0 OR EXISTS (
                   SELECT 1 FROM delivery_orders d WHERE d.sale_id = s.sale_id AND d.payment_status = 'paid'
               ))
             GROUP BY p.payment_method",
        )
        .bind(&shift_id)
        .fetch_all(pool)
        .await?;

        let mut cash_sales: i64 = 0;
        let mut card_sales: i64 = 0;
        let mut other_sales: i64 = 0;
        for pr in &payment_rows {
            let method: String = pr.get("payment_method");
            let total: i64 = pr.get("total");
            match method.as_str() {
                "cash" => cash_sales += total,
                "card" => card_sales += total,
                _ => other_sales += total,
            }
        }

        let safe_drop: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount_minor),0) FROM cash_events WHERE shift_id=? AND event_type='safe_drop'"
        ).bind(&shift_id).fetch_one(pool).await?;

        let paid_in: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount_minor),0) FROM cash_events WHERE shift_id=? AND event_type='paid_in'"
        ).bind(&shift_id).fetch_one(pool).await?;

        let paid_out: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount_minor),0) FROM cash_events WHERE shift_id=? AND event_type='paid_out'"
        ).bind(&shift_id).fetch_one(pool).await?;

        // BUG-REPORTS-3: Only deduct the cash portion of refunds.
        // Using EXISTS (cash payment) overcounted for split-payment sales — the full
        // refund was deducted from the cash drawer even when only part was cash.
        // Fix: proportionally scale refund_total by (cash_paid / sale_total), capped
        // at sale_total to avoid over-counting change given back on pure-cash sales.
        let cash_refunds: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(
                 CASE WHEN s.net_total_minor <= 0 THEN 0
                 ELSE MIN(
                     (SELECT COALESCE(SUM(p2.amount_minor),0)
                      FROM payments p2
                      WHERE p2.sale_id = s.sale_id AND p2.payment_method = 'cash'),
                     s.net_total_minor
                 ) * MAX(
                     r.refund_total_minor - COALESCE((
                         SELECT SUM(ep.amount_minor)
                         FROM payments ep
                         WHERE ep.payment_method = 'exchange_credit'
                           AND ep.external_reference = r.refund_id
                     ), 0),
                     0
                 ) / s.net_total_minor
                 END
             ), 0)
             FROM refunds r
             JOIN sales s ON s.sale_id = r.original_sale_id
             WHERE s.shift_id = ?
               AND EXISTS (
                   SELECT 1 FROM payments p
                   WHERE p.sale_id = s.sale_id AND p.payment_method = 'cash'
               )",
        )
        .bind(&shift_id)
        .fetch_one(pool)
        .await?;

        // BUG-3: Total refunds for the shift (all methods)
        let refund_row = sqlx::query(
            "SELECT COUNT(r.refund_id) AS cnt,
                    COALESCE(SUM(r.refund_total_minor),0) AS total
             FROM refunds r
             JOIN sales s ON s.sale_id = r.original_sale_id
             WHERE s.shift_id = ?",
        )
        .bind(&shift_id)
        .fetch_one(pool)
        .await?;
        let refund_count: i64 = refund_row.get("cnt");
        let refund_total: i64 = refund_row.get("total");

        // BUG-3: Tax breakdown per tax rule for this shift.
        // Groups sale_items by their embedded tax_rule snapshot; joins tax_rules
        // for the canonical name and rate. Falls back to snapshot values so
        // historic data (pre-rule-rename) is still reported correctly.
        let tax_rows = sqlx::query(
            "SELECT COALESCE(t.name, json_extract(si.tax_rule_snapshot,'$.name'), 'Unknown') AS rule_name,
                    COALESCE(t.rate_basis_points,
                             CAST(json_extract(si.tax_rule_snapshot,'$.rate_basis_points') AS INTEGER),
                             0) AS rate_bp,
                    COALESCE(SUM(si.line_total_minor - si.tax_amount_minor), 0) AS taxable_net,
                    COALESCE(SUM(si.tax_amount_minor), 0) AS tax_collected
             FROM sale_items si
             JOIN sales s ON s.sale_id = si.sale_id
             LEFT JOIN tax_rules t ON t.tax_rule_id = json_extract(si.tax_rule_snapshot,'$.tax_rule_id')
             WHERE s.shift_id = ? AND s.status != 'voided' AND si.voided = 0
             GROUP BY rule_name, rate_bp
             ORDER BY rate_bp DESC",
        )
        .bind(&shift_id)
        .fetch_all(pool)
        .await?;

        let tax_by_rule: Vec<TaxBreakdownRow> = tax_rows
            .iter()
            .map(|r| TaxBreakdownRow {
                tax_rule_name: r.get("rule_name"),
                rate_basis_points: r.get("rate_bp"),
                net_sales_minor: r.get("taxable_net"),
                tax_collected_minor: r.get("tax_collected"),
            })
            .collect();

        let expected = opening + cash_sales - cash_refunds + paid_in - paid_out - safe_drop;
        let variance = counted.map(|c| c - expected);

        total_net += net_sales;
        total_cash += cash_sales;
        total_card += card_sales;
        total_other += other_sales;
        total_gross += gross_sales;
        total_discount += discount_total;
        total_refund += refund_total;
        if let Some(c) = counted {
            if let Some(ref mut tc) = total_counted {
                *tc += c;
            }
        } else {
            all_counted = false;
        }

        rows.push(EodShiftRow {
            shift_id,
            cashier_name: sh.get("cashier_name"),
            opened_at: sh.get("opened_at"),
            closed_at: sh.get("closed_at"),
            opening_minor: opening,
            cash_sales_minor: cash_sales,
            card_sales_minor: card_sales,
            other_sales_minor: other_sales,
            gross_sales_minor: gross_sales,
            discount_total_minor: discount_total,
            net_sales_minor: net_sales,
            refund_count,
            refund_total_minor: refund_total,
            safe_drop_minor: safe_drop,
            paid_in_minor: paid_in,
            paid_out_minor: paid_out,
            expected_minor: expected,
            counted_minor: counted,
            variance_minor: variance,
            tax_by_rule,
        });
    }

    if !all_counted {
        total_counted = None;
    }
    // Overall variance: sum of per-shift variances where available
    let total_variance_minor = rows
        .iter()
        .filter_map(|r| r.variance_minor)
        .reduce(|a, b| a + b);

    Ok(EodCashupReport {
        date: date.to_string(),
        shifts: rows,
        total_net_minor: total_net,
        total_cash_minor: total_cash,
        total_card_minor: total_card,
        total_other_minor: total_other,
        total_gross_minor: total_gross,
        total_discount_minor: total_discount,
        total_refund_minor: total_refund,
        total_counted_minor: total_counted,
        total_variance_minor,
    })
}

/// End-of-day cash-up: Tauri command wrapper around `report_eod_cashup_inner`.
#[tauri::command]
pub async fn report_eod_cashup(
    session_token: String,
    branch_id: String,
    date: String,
    state: State<'_, AppState>,
) -> Result<EodCashupReport, AppError> {
    let actor = rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::ANY_ROLE)
        .await?;
    // The `branch_id` argument is caller-supplied and therefore not trusted:
    // reports expose financial data, so the scope is resolved from the actor's
    // own record and the incoming value is discarded. The parameter stays in
    // the signature only to keep the existing invoke contract.
    let _ = branch_id;
    let branch_id = actor.branch_id.clone();
    report_eod_cashup_inner(&state.db, &branch_id, &date, &date).await
}

/// Z-report: end-of-day cash-up summary with audit trail.
/// Wraps the EOD cashup logic and records a Z_REPORT_ISSUED audit entry
/// so every Z-report issuance is tamper-evident and traceable.
/// Requires manager or owner role — the Z-report is a sensitive financial summary.
#[tauri::command]
pub async fn report_z_report(
    date: String,
    session_token: String,
    state: State<'_, AppState>,
) -> Result<EodCashupReport, AppError> {
    let actor = rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::MANAGER_OR_OWNER)
        .await?;
    // Resolve active branch — Z-report always targets the current store.
    let branch_id: String = sqlx::query_scalar(
        "SELECT branch_id FROM branches WHERE is_active = 1 ORDER BY created_at LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await?
    .flatten()
    .unwrap_or_default();

    let report = report_eod_cashup_inner(&state.db, &branch_id, &date, &date).await?;

    // Resolve device for audit trail.
    let device_id: String = sqlx::query_scalar(
        "SELECT device_id FROM devices WHERE is_active = 1 ORDER BY device_code LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await?
    .flatten()
    .unwrap_or_default();

    // Record Z-report issuance in the audit hash-chain.
    record_z_report_issued(&state.db, &date, &actor.user_id, &device_id, &branch_id).await?;

    Ok(report)
}

async fn record_z_report_issued(
    pool: &SqlitePool,
    date: &str,
    actor_user_id: &str,
    device_id: &str,
    branch_id: &str,
) -> AppResult<()> {
    audit_hash::insert_audit_entry(
        pool,
        "Z_REPORT_ISSUED",
        "report",
        date,
        actor_user_id,
        "user",
        device_id,
        branch_id,
        None,
        None,
        None,
    )
    .await
}

// ─── Integrity check ──────────────────────────────────────────────────────────

#[tauri::command]
pub async fn db_integrity_check(
    session_token: String,
    state: State<'_, AppState>,
) -> Result<String, AppError> {
    rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::MANAGER_OR_OWNER)
        .await?;
    let result: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&state.db)
        .await?;
    Ok(result)
}

// ─── Reports device-scope config (Phase E) ───────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ReportsConfig {
    /// 'origin' = this device only; 'all' = every device in the branch.
    pub device_scope: String,
    /// Echoed back so the UI can display "Showing all 3 devices" or
    /// "Showing this device only" without a second roundtrip.
    pub device_count: i64,
    /// Local device id (when scope='origin' this is the filter value).
    pub local_device_id: String,
}

#[tauri::command]
pub async fn reports_config_load(
    session_token: String,
    state: State<'_, AppState>,
) -> Result<ReportsConfig, AppError> {
    rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::ANY_ROLE)
        .await?;
    let (scope, local_device_id) = report_scope(&state.db).await;
    let device_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE is_active = 1")
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);
    Ok(ReportsConfig {
        device_scope: scope.as_str().to_string(),
        device_count,
        local_device_id,
    })
}

#[derive(serde::Deserialize)]
pub struct SaveReportsConfigInput {
    /// 'origin' or 'all' — anything else is coerced to 'origin'.
    pub device_scope: String,
}

#[tauri::command]
pub async fn reports_config_save(
    input: SaveReportsConfigInput,
    session_token: String,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    // Only managers and owners can change report scope — it determines
    // whether cashiers see the whole store's takings or just their own.
    rbac::session_actor(
        &state.sessions,
        &state.db,
        &session_token,
        rbac::MANAGER_OR_OWNER,
    )
    .await?;

    let normalized = match input.device_scope.to_ascii_lowercase().as_str() {
        "all" => "all",
        _ => "origin",
    };
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO app_config (key, value, updated_at) VALUES ('reports_device_scope', ?, ?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(normalized)
    .bind(&now)
    .execute(&state.db)
    .await?;

    // sync_status='pending' is set by column DEFAULT — sync worker picks it up
    sync_commands::schedule_immediate_sync(&state);

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Integration tests
// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repositories::sale_repo;
    use crate::domain::cart::{Cart, CartLine};
    use crate::domain::sale::PaymentInput;
    use sqlx::sqlite::SqlitePoolOptions;

    const BRANCH: &str = "01JBRANCH0000000000000001";
    const DEVICE: &str = "01JDEVICE0000000000000001";
    const CASHIER: &str = "01JUSER000000000000CASH01";
    const TAX_ZER: &str = "01JTAX000000000000ZERO01";

    async fn make_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("migrations");
        // Activate seed device and branch
        sqlx::query(
            "UPDATE devices SET is_active = 1 WHERE device_id = '01JDEVICE0000000000000001'",
        )
        .execute(&pool)
        .await
        .ok();
        sqlx::query(
            "UPDATE branches SET is_active = 1 WHERE branch_id = '01JBRANCH0000000000000001'",
        )
        .execute(&pool)
        .await
        .ok();

        // Seed tax rules needed by the test product
        sqlx::query(
            "INSERT OR IGNORE INTO tax_rules (tax_rule_id, name, rate_basis_points, inclusive, is_active, effective_from, created_at, updated_at, version)
             VALUES ('01JTAX000000000000ZERO01', 'Zero-rated', 0, 0, 1, datetime('now'), datetime('now'), datetime('now'), 1)"
        ).execute(&pool).await.expect("seed test tax rule");

        // Seed cashier user
        sqlx::query(
            "INSERT OR IGNORE INTO users (user_id, branch_id, display_name, username, pin_hash, role_id, is_active, created_at, updated_at, version)
             VALUES ('01JUSER000000000000CASH01', '01JBRANCH0000000000000001', 'Cashier 1', 'cashier1', 'PLAIN:1234', '01JROLES000000000000000003', 1, datetime('now'), datetime('now'), 1)"
        ).execute(&pool).await.expect("seed test cashier");

        // Re-seed product + stock needed by tests.
        sqlx::query(
            "INSERT OR IGNORE INTO categories (category_id, name, sort_order, is_active, created_at, updated_at, version)
             VALUES ('01JCAT000000000000DRINK01', 'Drinks', 1, 1, datetime('now'), datetime('now'), 1)"
        ).execute(&pool).await.expect("seed test category");
        sqlx::query(
            "INSERT OR IGNORE INTO products
             (product_id, category_id, name, sku, barcode, description, track_inventory, reorder_point, is_active, tax_rule_id, currency, created_at, updated_at, version)
             VALUES ('01JPROD00000000000WATR001', '01JCAT000000000000DRINK01', 'Water 500ml', 'WATR-500', '6281001511222', NULL, 1, 10, 1, '01JTAX000000000000ZERO01', 'BHD', datetime('now'), datetime('now'), 1)"
        ).execute(&pool).await.expect("seed test product");
        sqlx::query(
            "INSERT OR IGNORE INTO stock_levels (stock_level_id, product_id, branch_id, quantity_on_hand, created_at, updated_at, sync_status, sync_attempts)
             VALUES ('SL-TEST-WATR', '01JPROD00000000000WATR001', ?, '1000', datetime('now'), datetime('now'), 'synced', 0)"
        ).bind(BRANCH).execute(&pool).await.expect("seed test stock");
        pool
    }

    async fn insert_shift(pool: &SqlitePool) -> String {
        let shift_id = ulid::Ulid::new().to_string();
        sqlx::query(
            "INSERT INTO shifts (shift_id, branch_id, device_id, origin_device_id, cashier_user_id, opened_at, status, created_at, updated_at, version, sync_status, sync_attempts)
             VALUES (?, ?, ?, ?, ?, datetime('now'), 'open', datetime('now'), datetime('now'), 1, 'pending', 0)"
        )
        .bind(&shift_id)
        .bind(BRANCH)
        .bind(DEVICE)
        .bind(DEVICE)
        .bind(CASHIER)
        .execute(pool)
        .await
        .expect("insert shift");
        shift_id
    }

    #[test]
    fn sales_cursor_is_opaque_round_trip_and_rejects_tampering() {
        let encoded = encode_sale_cursor("2026-08-15T12:30:00Z", "SALE-42").unwrap();
        assert!(!encoded.contains("SALE-42"));
        let decoded = decode_sale_cursor(&encoded).unwrap();
        assert_eq!(decoded.sold_at, "2026-08-15T12:30:00Z");
        assert_eq!(decoded.sale_id, "SALE-42");
        assert!(decode_sale_cursor("not-a-valid-cursor").is_err());
    }

    #[test]
    fn csv_export_quotes_values_and_neutralizes_spreadsheet_formulas() {
        assert_eq!(csv_cell("Amwaj \"Main\""), "\"Amwaj \"\"Main\"\"\"");
        assert_eq!(
            csv_cell("=HYPERLINK(\"bad\")"),
            "\"'=HYPERLINK(\"\"bad\"\")\""
        );
        assert_eq!(csv_cell("  +SUM(1,1)"), "\"'  +SUM(1,1)\"");
    }

    // ── T10. EOD cashup report shows correct cashier name (not empty) ─────────
    #[tokio::test]
    async fn test_eod_cashup_cashier_name_populated() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;

        // Create a water sale so there's real data
        let mut cart = Cart::new(
            BRANCH.into(),
            DEVICE.into(),
            shift_id.clone(),
            CASHIER.into(),
        );
        cart.lines.push(CartLine {
            cart_line_id: ulid::Ulid::new().to_string(),
            product_id: Some("01JPROD00000000000WATR001".into()),
            product_name: "Water 500ml".into(),
            sku: Some("WATR500".into()),
            barcode: None,
            image_path: None,
            quantity: "1".to_string(),
            unit_price_minor: 250,
            line_discount_minor: 0,
            line_discount_reason: None,
            tax_rule_id: TAX_ZER.to_string(),
            tax_rate_basis_points: 0,
            tax_inclusive: false,
            tax_amount_minor: 0,
            line_total_minor: 250,
            note: None,
            voided: false,
        });
        let payments = vec![PaymentInput {
            method: "cash".into(),
            amount_minor: 250,
            tendered_minor: Some(250),
            external_reference: None,
        }];
        sale_repo::finalize_sale(
            &pool,
            &cart,
            payments,
            "idem-t10-eod",
            None,
            false,
            None,
            false,
        )
        .await
        .expect("finalize sale");

        // Close the shift
        crate::db::repositories::shift_repo::close_shift(&pool, &shift_id, Some(250), None)
            .await
            .expect("close shift");

        // EOD report for today
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let rows = report_eod_cashup_inner(&pool, BRANCH, &today, &today)
            .await
            .expect("eod cashup");

        assert_eq!(rows.shifts.len(), 1, "should have one shift row");
        let row = &rows.shifts[0];

        // cashier_name must be 'Cashier 1' (from seed data), not empty or 'Unknown'
        assert!(
            !row.cashier_name.is_empty(),
            "cashier_name must not be empty, got: '{}'",
            row.cashier_name
        );
        assert_ne!(
            row.cashier_name, "Unknown",
            "cashier_name must be resolved from users table, not 'Unknown'"
        );
        assert_eq!(
            row.cashier_name, "Cashier 1",
            "cashier_name must match seed data"
        );
    }

    #[tokio::test]
    async fn test_margin_report_uses_sale_cost_snapshot() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;

        sqlx::query("UPDATE products SET cost_minor = 100 WHERE product_id = ?")
            .bind("01JPROD00000000000WATR001")
            .execute(&pool)
            .await
            .expect("set starting product cost");

        let mut cart = Cart::new(
            BRANCH.into(),
            DEVICE.into(),
            shift_id.clone(),
            CASHIER.into(),
        );
        cart.lines.push(CartLine {
            cart_line_id: ulid::Ulid::new().to_string(),
            product_id: Some("01JPROD00000000000WATR001".into()),
            product_name: "Water 500ml".into(),
            sku: Some("WATR500".into()),
            barcode: None,
            image_path: None,
            quantity: "2".to_string(),
            unit_price_minor: 250,
            line_discount_minor: 0,
            line_discount_reason: None,
            tax_rule_id: TAX_ZER.to_string(),
            tax_rate_basis_points: 0,
            tax_inclusive: false,
            tax_amount_minor: 0,
            line_total_minor: 500,
            note: None,
            voided: false,
        });
        let payments = vec![PaymentInput {
            method: "cash".into(),
            amount_minor: 500,
            tendered_minor: Some(500),
            external_reference: None,
        }];
        sale_repo::finalize_sale(
            &pool,
            &cart,
            payments,
            "idem-margin-snapshot",
            None,
            false,
            None,
            false,
        )
        .await
        .expect("finalize sale");

        sqlx::query("UPDATE products SET cost_minor = 999 WHERE product_id = ?")
            .bind("01JPROD00000000000WATR001")
            .execute(&pool)
            .await
            .expect("change current product cost");

        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let summary = report_margin_inner(&pool, BRANCH, &today, &today)
            .await
            .expect("margin report");
        let rows = report_product_margin_inner(&pool, BRANCH, &today, &today, 10)
            .await
            .expect("product margin report");

        assert_eq!(summary.revenue_minor, 500);
        assert_eq!(summary.cogs_minor, 200);
        assert_eq!(summary.gross_margin_minor, 300);
        assert_eq!(summary.margin_basis_points, Some(6000));
        assert_eq!(summary.unknown_cost_line_count, 0);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].cogs_minor, 200);
        assert_eq!(rows[0].gross_margin_minor, 300);
    }

    // ── T11. payment_method CHECK constraint rejects invalid values ───────────
    #[tokio::test]
    async fn test_payment_method_check_constraint() {
        let pool = make_pool().await;
        let shift_id = insert_shift(&pool).await;

        // Insert a minimal sale to reference
        let sale_id = ulid::Ulid::new().to_string();
        sqlx::query(
            "INSERT INTO sales
             (sale_id, receipt_number, branch_id, device_id, origin_device_id, shift_id,
              cashier_user_id, status, gross_total_minor, discount_total_minor,
              tax_total_minor, net_total_minor, currency, business_date,
              sold_at, created_offline, idempotency_key, sync_status, created_at, updated_at)
             VALUES (?,'MAIN-POS01-T11',?,?,?,?,?,'completed',100,0,0,100,'BHD',
                     '2026-01-01',datetime('now'),0,'idem-t11','pending',datetime('now'),datetime('now'))",
        )
        .bind(&sale_id)
        .bind(BRANCH)
        .bind(DEVICE)
        .bind(DEVICE)
        .bind(&shift_id)
        .bind(CASHIER)
        .execute(&pool)
        .await
        .expect("insert sale");

        // Try inserting a payment with an invalid method — must fail CHECK constraint
        let err = sqlx::query(
            "INSERT INTO payments
             (payment_id, sale_id, origin_device_id, payment_method, amount_minor, currency,
              recorded_by_user_id, recorded_at, created_at, updated_at)
             VALUES (?,?,?,'bribe',100,'BHD',?,datetime('now'),datetime('now'),datetime('now'))",
        )
        .bind(ulid::Ulid::new().to_string())
        .bind(&sale_id)
        .bind(DEVICE)
        .bind(CASHIER)
        .execute(&pool)
        .await;

        assert!(
            err.is_err(),
            "invalid payment_method 'bribe' must violate CHECK constraint"
        );
    }

    #[tokio::test]
    async fn test_z_report_audit_failure_is_returned() {
        let pool = make_pool().await;
        sqlx::query(
            "CREATE TRIGGER fail_z_report_audit
             BEFORE INSERT ON audit_logs
             WHEN NEW.event_type = 'Z_REPORT_ISSUED'
             BEGIN
               SELECT RAISE(FAIL, 'audit write blocked');
             END",
        )
        .execute(&pool)
        .await
        .expect("create audit failure trigger");

        let err = record_z_report_issued(&pool, "2026-07-05", CASHIER, DEVICE, BRANCH)
            .await
            .expect_err("Z-report audit write failure must be returned");

        assert!(
            matches!(err, AppError::Database(_)),
            "expected database error, got {err:?}"
        );
    }
}
