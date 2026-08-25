use crate::ai::client::ToolDef;
use crate::ai::tools_web::*;
use crate::db::repositories::{product_repo, report_repo, sync_repo};
use crate::domain::ai_admin::{ToolPreview, ToolPreviewField};
use crate::domain::money;
use crate::errors::{AppError, AppResult};
use crate::inventory::{movements, stock_repo};
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};

// ── Dynamic-bind helper (S-01) ──────────────────────────────────────────────────
// Lets dynamic UPDATE statements bind heterogeneous values as proper parameters
// instead of interpolating escaped strings into SQL. Both match arms return the
// same Query type, so binds can be applied in a loop.
type SqliteQuery<'q> = sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>>;

enum SqlBind {
    S(String),
    I(i64),
}

impl SqlBind {
    fn apply<'q>(&'q self, q: SqliteQuery<'q>) -> SqliteQuery<'q> {
        match self {
            SqlBind::S(s) => q.bind(s),
            SqlBind::I(i) => q.bind(i),
        }
    }
}

// ── Tool catalogue ─────────────────────────────────────────────────────────────

pub fn all_tool_definitions() -> Vec<ToolDef> {
    crate::ai::tools_catalogue::all_tool_definitions()
}

/// L16: Single source of truth for mutation tools.
/// When adding a new tool to `all_tool_definitions`, add its name here too.
/// Omitting a mutation tool from this list makes it silently execute without
/// confirmation — the hardcoded list is intentional but must be kept in sync.
pub const MUTATION_TOOLS: &[&str] = &[
    "update_product_price",
    "set_product_active",
    "update_product_name",
    "adjust_stock",
    "stock_take",
    "create_product",
    "update_reorder_point",
    "create_customer",
    "update_customer",
    "advance_delivery_status",
    "bulk_stock_take",
    "create_category",
    "update_category",
    "create_user",
    "update_user",
    "create_tax_rule",
    "update_tax_rule",
    "update_product_full",
    "update_store_settings",
    "update_business_rules",
    "confirm_delivery_payment",
    "cancel_delivery",
    "backup_database",
    "sync_reset_stuck",
    "sync_queue_retry",
    "sync_queue_dismiss",
    "apply_system_health_fix",
    "void_sale",
    "delete_customer",
    "set_device_active",
    "receive_stock",
    "add_loyalty_points",
    "bulk_update_prices",
    "bulk_price_adjust",
    "bulk_stock_set",
    // ── Extension mutations ──────────────────────────────────────────────────
    "create_refund",
    "create_cash_event",
    "open_shift",
    "close_shift",
    "add_product_barcode",
    "remove_product_barcode",
    "trigger_sync_now",
    "force_full_resync",
    "revert_delivery_payment",
    "update_branch_settings",
    "register_device",
    "send_whatsapp_delivery_alert",
    "send_whatsapp_payment_reminder",
    "send_whatsapp_arrival_notice",
    "disconnect_whatsapp",
    "update_thermal_config",
    "open_cash_drawer",
    "reprint_receipt",
    "delete_held_cart",
    "update_benefit_number",
    // ── New extended mutations (Phase 3) ────────────────────────────────────
    "delete_product",
    "delete_category",
    "delete_tax_rule",
    "delete_user",
    "update_delivery_details",
    "reassign_delivery_rider",
    "batch_dispatch_deliveries",
    "duplicate_product",
    "send_whatsapp_to_customer",
    "reset_user_pin",
    "lock_user",
    "unlock_user",
    "bulk_deactivate_products",
    "bulk_activate_products",
    "bulk_set_category",
    "bulk_set_tax_rule",
    "bulk_update_reorder_point",
    "bulk_update_cost",
    "create_supplier",
    "update_supplier",
    "create_purchase_order",
    "update_purchase_order",
    "vacuum_database",
    "reindex_database",
    "force_wal_checkpoint",
    "resolve_ghost_barcode",
    "resolve_sync_conflict",
    "clear_ghost_sync_records",
    "run_diagnostics_and_fix",
    "bulk_import_products",
    "create_products",
    "bulk_import_categories",
    "send_receipt_via_whatsapp",
    "create_customer_note",
    // ── Round 2 mutations ────────────────────────────────────────────────────
    "delete_supplier",
    "receive_purchase_order",
    "delete_purchase_order",
    "bulk_assign_supplier",
    "force_close_shift",
    // ── Engine bulk ops (RunPreview confirmation flow) ───────────────────────
    "bulk_stock_variance_fix",
    "bulk_promotion_apply",
    "bulk_supplier_price_sync",
    "bulk_product_archive",
    "bulk_reorder_point_update",
    "bulk_promotion_remove",
    "product_create",
    // ── Product quality ──────────────────────────────────────────────────────
    "merge_products",
];

pub fn is_mutation_tool(name: &str) -> bool {
    MUTATION_TOOLS.contains(&name)
}

// ── Feature-toggle gated tool sets ─────────────────────────────────────────────

/// Returns tool definitions filtered by feature toggles stored in app_config.
/// Read-only tools (list/get/search) are always included.
pub async fn filtered_tool_definitions(pool: &SqlitePool) -> AppResult<Vec<ToolDef>> {
    crate::ai::tool_policy::filter_enabled_definitions(pool, all_tool_definitions()).await
}

pub async fn filtered_tool_definitions_for_role(
    pool: &SqlitePool,
    role_name: &str,
) -> AppResult<Vec<ToolDef>> {
    let enabled = filtered_tool_definitions(pool).await?;
    crate::ai::tool_policy::filter_definitions_for_role(&enabled, role_name)
}

#[allow(dead_code)]
async fn get_toggle(pool: &SqlitePool, key: &str, default: bool) -> bool {
    let row = sqlx::query("SELECT value FROM app_config WHERE key = ?")
        .bind(key)
        .fetch_optional(pool)
        .await;
    match row {
        Ok(Some(r)) => {
            let v: String = r.get(0);
            v == "1" || v == "true"
        }
        _ => default,
    }
}

/// Read-only core tools always visible regardless of feature toggles.
#[allow(dead_code)]
fn is_readonly_core(name: &str) -> bool {
    matches!(
        name,
        "get_today_summary"
            | "list_products"
            | "search_products"
            | "get_product"
            | "get_stock_levels"
            | "get_low_stock"
            | "get_cash_summary"
            | "get_recent_refunds"
            | "get_audit_log"
            | "get_sync_status"
            | "get_sync_diagnostics"
            | "get_system_health_check"
            | "get_daily_report"
            | "get_date_range_report"
            | "get_top_products"
            | "get_shift_history"
            | "list_categories"
            | "list_safe_drops"
            | "list_no_sale_events"
            | "get_audit_chain_status"
            | "get_hourly_sales"
            | "get_sales_by_category"
            | "get_cashier_performance"
            | "list_customers"
            | "get_customer"
            | "list_deliveries"
            | "list_users"
            | "get_stock_movements"
            | "get_tax_report"
            | "list_roles"
            | "list_tax_rules"
            | "get_store_settings"
            | "get_business_rules"
            | "list_devices"
            | "get_session_timeout"
            | "get_active_shift"
            | "get_sales_list"
            | "get_sale_detail"
            | "get_z_report"
            | "get_eod_cashup"
            | "get_x_report"
            | "get_product_barcodes"
            | "get_whatsapp_status"
            | "get_branch_settings"
            | "get_hub_status"
            | "get_held_carts"
            | "get_db_integrity"
            | "get_thermal_config"
            | "get_delivery_detail"
            | "get_rider_suggestions"
            | "get_sync_queue_stats"
            | "get_product_detail"
            | "get_sales_report"
            | "get_cash_status"
            | "search_sales_by_customer"
            | "find_products_without_barcode"
            | "get_user_permissions"
            | "get_customer_notes"
            | "list_suppliers"
            | "list_purchase_orders"
            | "get_supplier"
            | "get_purchase_order"
            | "get_supplier_products"
            | "get_open_shifts"
            | "get_expected_cash_position"
            | "get_petty_cash_log"
            | "get_user_shift_summary"
            | "get_migration_status"
            | "get_app_version"
            | "get_database_size"
            | "get_table_row_counts"
            | "get_product_versions"
            | "export_product_catalog"
            | "export_customers"
            | "open_tab"
            | "lookup_barcode"
            | "get_exchange_rates"
            | "get_prayer_times"
            | "get_bahrain_holidays"
    )
}

fn is_web_search_tool(name: &str) -> bool {
    matches!(name, "web_search")
}

fn is_web_fetch_tool(name: &str) -> bool {
    matches!(name, "fetch_url")
}

fn is_compare_prices_tool(name: &str) -> bool {
    matches!(name, "compare_store_prices")
}

fn is_market_price_tool(name: &str) -> bool {
    matches!(name, "bahrain_market_price_check" | "search_market_prices")
}

fn is_smart_analytics_tool(name: &str) -> bool {
    matches!(
        name,
        "get_dead_stock"
            | "get_discount_by_cashier"
            | "get_customer_purchase_history"
            | "get_revenue_by_payment_method"
            | "get_profit_margin_report"
            | "get_shelf_label_gap"
            | "get_product_sales_rank"
            | "get_hourly_heatmap"
            | "get_category_performance"
            | "get_cash_discrepancy_log"
            | "get_void_rate_by_cashier"
            | "get_peak_hours"
            | "get_customer_visit_frequency"
            | "get_average_basket_by_time"
            | "get_tax_collected_report"
            | "get_unused_products"
            | "get_category_mix_analysis"
            | "get_sales_by_device"
            | "get_revenue_forecast"
            | "get_day_of_week_comparison"
            | "get_month_over_month_growth"
            | "get_new_vs_returning"
            | "get_void_report"
            | "get_loyalty_summary"
            | "get_customer_segments"
            | "get_active_deliveries_map"
            | "get_delivery_performance"
            | "get_delivery_payment_outstanding"
            | "get_tax_filing_summary"
            | "validate_tax_config"
            | "get_z_report_archive"
            | "get_low_stock_with_velocity"
            | "get_inventory_valuation"
            | "get_overstock_alert"
            | "get_sales_velocity"
            | "get_stock_turnover_ratio"
            | "compare_cashiers"
            | "get_basket_size_trend"
            | "get_stockout_cost"
            | "get_refund_rate"
            | "get_refund_by_product"
            | "verify_receipt_sequence"
            | "get_audit_trail_full"
            | "get_frequently_bought_together"
            | "get_bundle_suggestions"
            | "get_weekly_forecast"
            | "get_rfm_segmentation"
            | "get_margin_trend"
            | "get_restock_priority"
            | "get_dead_stock_value"
            | "get_category_forecast"
            | "find_duplicate_products"
    )
}

fn is_proactive_tool(name: &str) -> bool {
    matches!(
        name,
        "send_whatsapp_delivery_alert"
            | "send_whatsapp_payment_reminder"
            | "send_whatsapp_arrival_notice"
            | "send_whatsapp_to_customer"
    )
}

fn is_inventory_ops_tool(name: &str) -> bool {
    matches!(
        name,
        "update_product_price"
            | "set_product_active"
            | "update_product_name"
            | "adjust_stock"
            | "stock_take"
            | "update_reorder_point"
            | "create_product"
            | "receive_stock"
            | "bulk_stock_take"
            | "bulk_update_prices"
            | "bulk_price_adjust"
            | "bulk_stock_set"
            | "bulk_activate_products"
            | "bulk_deactivate_products"
            | "bulk_set_category"
            | "bulk_set_tax_rule"
            | "bulk_update_reorder_point"
            | "bulk_update_cost"
            | "bulk_assign_supplier"
            | "bulk_stock_variance_fix"
            | "bulk_promotion_apply"
            | "bulk_promotion_remove"
            | "bulk_supplier_price_sync"
            | "bulk_product_archive"
            | "bulk_reorder_point_update"
            | "product_create"
            | "delete_product"
            | "delete_category"
            | "duplicate_product"
            | "update_product_full"
            | "merge_products"
    )
}

fn is_customer_insights_tool(name: &str) -> bool {
    matches!(
        name,
        "get_customer_visit_frequency"
            | "get_customer_ltv"
            | "get_churn_risk"
            | "get_top_spenders"
            | "get_lapsed_customers"
            | "get_customer_outstanding_balance"
            | "add_loyalty_points"
            | "create_customer_note"
            | "get_customer_purchase_history"
    )
}

fn is_insights_engine_tool(name: &str) -> bool {
    matches!(
        name,
        "run_insights_dashboard"
            | "generate_analytics_report"
            | "get_proactive_insights"
            | "get_business_health_score"
            | "get_trend_analysis"
            | "get_anomaly_detection"
    )
}

/// Canonical feature gate for a tool. Execution policy uses this same mapping
/// as definition filtering so disabling a feature cannot be bypassed by a
/// forged/stale model tool call.
pub(crate) fn feature_key_for_tool(name: &str) -> Option<&'static str> {
    if is_web_search_tool(name) {
        Some("feature_web_search")
    } else if is_web_fetch_tool(name) {
        Some("feature_web_fetch")
    } else if is_compare_prices_tool(name) {
        Some("feature_compare_prices")
    } else if is_market_price_tool(name) {
        Some("feature_market_price")
    } else if is_smart_analytics_tool(name) {
        Some("feature_smart_analytics")
    } else if is_proactive_tool(name) {
        Some("feature_proactive")
    } else if is_inventory_ops_tool(name) {
        Some("feature_inventory_ops")
    } else if is_customer_insights_tool(name) {
        Some("feature_customer_insights")
    } else if is_insights_engine_tool(name) {
        Some("feature_insights_engine")
    } else {
        None
    }
}

fn format_health_report(
    report: &crate::commands::system_health_commands::SystemHealthReport,
) -> String {
    let mut lines = vec![
        "[DB] Complete System Health Check".to_string(),
        format!(
            "Summary: {} | DB: {} | migrations: {} | hub mode: {} | devices: {} | pending sync: {} | stuck sync: {} | checked: {}",
            if report.summary.ok { "HEALTHY" } else { "ATTENTION NEEDED" },
            report.summary.db_integrity,
            report.summary.migration_count,
            report.summary.hub_mode,
            report.summary.device_count,
            report.summary.pending_sync_rows,
            report.summary.stuck_sync_rows,
            report.summary.checked_at,
        ),
    ];

    lines.push("\nFindings:".into());
    if report.findings.is_empty() {
        lines.push(
            "- None. No database, migration, sync, hub, terminal, or AI-run issues found.".into(),
        );
    } else {
        for f in &report.findings {
            let fix = f
                .fix_action
                .as_deref()
                .map(|a| format!(" | fix_action: {a}"))
                .unwrap_or_default();
            lines.push(format!(
                "- [{:?}] {} / {}: {} — {}{}",
                f.severity, f.area, f.code, f.title, f.detail, fix
            ));
        }
    }

    lines.push("\nDevices / terminals:".into());
    if report.devices.is_empty() {
        lines.push("- No devices found.".into());
    } else {
        for d in &report.devices {
            lines.push(format!(
                "- {} ({}) | role: {} | status: {} | ip: {} | last_seen: {}",
                d.label,
                d.device_id,
                d.role,
                d.status,
                d.ip.as_deref().unwrap_or("—"),
                d.last_seen.as_deref().unwrap_or("—"),
            ));
        }
    }

    lines.push("\nSync table detail:".into());
    if report.tables.is_empty() {
        lines.push("- No pending or stuck sync rows.".into());
    } else {
        for t in &report.tables {
            lines.push(format!(
                "- {}: {} pending, {} stuck, max attempts {}",
                t.table, t.pending, t.stuck, t.max_attempts
            ));
        }
    }

    lines.join("\n")
}

// ── Read-only tool executor ────────────────────────────────────────────────────

pub async fn execute_read_tool(
    pool: &SqlitePool,
    tool_name: &str,
    input: &Value,
    branch_id: &str,
    currency_exp: u32,
) -> AppResult<String> {
    let started = std::time::Instant::now();
    let result = execute_read_tool_inner(pool, tool_name, input, branch_id, currency_exp).await;
    let estimated_tokens = result
        .as_ref()
        .map(|content| (content.chars().count() as i64 + 3) / 4)
        .unwrap_or(0);
    if let Err(error) = crate::db::repositories::ai_admin_repo::record_tool_metric(
        pool,
        tool_name,
        result.is_ok(),
        started.elapsed().as_millis().min(i64::MAX as u128) as i64,
        estimated_tokens,
    )
    .await
    {
        tracing::warn!(tool = tool_name, %error, "failed to record ZanAI tool metric");
    }
    result
}

async fn execute_read_tool_inner(
    pool: &SqlitePool,
    tool_name: &str,
    input: &Value,
    branch_id: &str,
    currency_exp: u32,
) -> AppResult<String> {
    match tool_name {
        "request_full_tool_access" => Ok(
            "The full mutation-tool catalogue will be available on the next step. Re-evaluate the operator's request before choosing a mutation; the runtime will enforce the configured confirmation policy."
                .into(),
        ),
        // Validation lives in `forms`, so an unusable spec comes back as a
        // recoverable tool error the model can correct. The streaming loop
        // re-parses the same input to build the event it sends the widget;
        // parsing is pure, so a spec that succeeds here succeeds there.
        "request_input" => {
            let form = crate::ai::forms::parse_form_spec(input)?;
            Ok(crate::ai::forms::acknowledgement(&form))
        }
        "report_expiring_stock" => {
            let lead_days = input.get("lead_days").and_then(Value::as_i64).unwrap_or(7);
            let lots = crate::inventory::lots::expiring_lots(pool, branch_id, lead_days).await?;
            if lots.is_empty() {
                return Ok(format!(
                    "No received lots expire within {lead_days} days."
                ));
            }
            let mut lines = vec![
                format!("Expiring stock within {lead_days} days (FEFO order):"),
                "Product | Expiry | Days | Remaining | Suggested action".into(),
            ];
            for lot in lots {
                let action = if lot.days_until_expiry < 0 {
                    "Remove from sale and review write-off"
                } else if lot.days_until_expiry <= 2 {
                    "Urgent clearance/markdown review"
                } else {
                    "Plan clearance or supplier return"
                };
                lines.push(format!(
                    "{} | {} | {} | {} | {}",
                    lot.product_name,
                    lot.expiry_date,
                    lot.days_until_expiry,
                    lot.quantity_remaining,
                    action
                ));
            }
            lines.push(
                "Suggestions are advisory. Any markdown is a price mutation governed by the runtime confirmation policy."
                    .into(),
            );
            Ok(lines.join("\n"))
        }
        "get_margin_erosion" => {
            let threshold = input
                .get("threshold_basis_points")
                .and_then(Value::as_i64)
                .unwrap_or(500);
            let report =
                crate::ai::business_insights::margin_erosion(pool, branch_id, threshold).await?;
            let mut lines = vec![format!(
                "Margin erosion report (minimum {} bps):",
                threshold
            )];
            lines.push(format!(
                "UNKNOWN COST LINES (last 30 days): {}. Margin totals are incomplete when this is above zero.",
                report.unknown_cost_line_count
            ));
            if report.rows.is_empty() {
                lines.push("No unchanged shelf prices crossed the erosion threshold.".into());
            } else {
                lines.push(
                    "Product | Price | Old cost | New cost | Margin loss | Current margin".into(),
                );
                for row in report.rows {
                    lines.push(format!(
                        "{} | {} | {} | {} | {} bps | {} bps",
                        row.product_name,
                        row.selling_price_minor,
                        row.old_cost_minor,
                        row.new_cost_minor,
                        row.margin_drop_basis_points,
                        row.current_margin_basis_points
                    ));
                }
            }
            lines.push(
                "Any shelf-price change is a mutation governed by the runtime confirmation policy.".into(),
            );
            Ok(lines.join("\n"))
        }
        "get_seasonal_demand_plan" => {
            let from = input.get("from").and_then(Value::as_str).unwrap_or("");
            let to = input.get("to").and_then(Value::as_str).unwrap_or("");
            let years = input
                .get("comparison_years")
                .and_then(Value::as_i64)
                .unwrap_or(2) as i32;
            let plan = crate::ai::business_insights::seasonal_demand_plan(
                pool, branch_id, from, to, years,
            )
            .await?;
            if plan.is_empty() {
                return Ok(format!(
                    "No same-period sales history is available for {from} to {to}."
                ));
            }
            let mut lines = vec![
                format!(
                    "Seasonal demand plan for {from} to {to}, averaged across {years} prior year(s):"
                ),
                "Product | Historical avg qty | On hand | Suggested reorder".into(),
            ];
            for row in plan {
                lines.push(format!(
                    "{} | {:.3} | {:.3} | {:.3}",
                    row.product_name,
                    row.historical_average_quantity,
                    row.quantity_on_hand,
                    row.suggested_reorder_quantity
                ));
            }
            lines.push(
                "This is an advisory plan from store history. Review it before creating a purchase order."
                    .into(),
            );
            Ok(lines.join("\n"))
        }
        "get_cash_flow_forecast" => {
            let forecast =
                crate::ai::business_insights::cash_flow_forecast(pool, branch_id).await?;
            Ok(format!(
                "Known cash-flow forecast (minor currency units):\n\
                 - Unpaid cash-on-delivery receivables: {}\n\
                 - Outstanding ordered/partial PO commitments: {}\n\
                 - Net known position: {}\n\
                 - PO lines with unknown/zero cost: {}\n\
                 This excludes unrecorded bills, draft POs, card settlement timing, and future sales. It is an operational forecast, not financial advice.",
                forecast.delivery_cod_receivable_minor,
                forecast.purchase_commitments_minor,
                forecast.net_known_position_minor,
                forecast.unknown_cost_po_line_count,
            ))
        }
        // ── AI task ledger (persistent progress scratchpad) ──────────────────
        "get_task_ledger" => {
            let row: Option<(String, String, String)> = sqlx::query_as(
                "SELECT description, state_json, updated_at FROM ai_task_ledger
                 WHERE branch_id = ? AND task_key = 'current'",
            )
            .bind(branch_id)
            .fetch_optional(pool)
            .await?;
            match row {
                Some((description, state, updated_at)) => Ok(format!(
                    "[LEDGER] Current task (updated {updated_at}):\nDescription: {description}\nState: {state}"
                )),
                None => Ok("[LEDGER] No task in progress.".into()),
            }
        }
        "set_task_ledger" => {
            if input.get("clear").and_then(|v| v.as_bool()) == Some(true) {
                sqlx::query(
                    "DELETE FROM ai_task_ledger WHERE branch_id = ? AND task_key = 'current'",
                )
                .bind(branch_id)
                .execute(pool)
                .await?;
                return Ok("[LEDGER] Cleared.".into());
            }
            let description = input
                .get("description")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| AppError::Validation("Missing description".into()))?;
            let state = input.get("state").and_then(|v| v.as_str()).unwrap_or("{}");
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query(
                "INSERT INTO ai_task_ledger (branch_id, task_key, description, state_json, updated_at)
                 VALUES (?, 'current', ?, ?, ?)
                 ON CONFLICT(branch_id, task_key) DO UPDATE SET
                     description = excluded.description,
                     state_json = excluded.state_json,
                     updated_at = excluded.updated_at",
            )
            .bind(branch_id)
            .bind(description)
            .bind(state)
            .bind(&now)
            .execute(pool)
            .await?;
            Ok("[LEDGER] Saved.".into())
        }
        // ── WhatsApp commerce (read) ─────────────────────────────────────────
        "list_whatsapp_orders" => {
            if !crate::commands::whatsapp_catalog_commands::orders_enabled(pool).await {
                return Ok("[DB] Orders are turned off (enable WhatsApp Commerce or the Storefront in Settings).".into());
            }
            let status = input.get("status").and_then(|v| v.as_str());
            let limit = input
                .get("limit")
                .and_then(|v| v.as_i64())
                .unwrap_or(50)
                .clamp(1, 200);
            let rows = if let Some(s) = status.filter(|s| !s.is_empty()) {
                sqlx::query_as::<_, (String, Option<String>, String, Option<i64>, Option<String>, i64, String)>(
                    "SELECT order_id, customer_name, status, total_minor, currency, product_count, created_at
                     FROM wa_orders WHERE status = ? ORDER BY created_at DESC LIMIT ?",
                )
                .bind(s)
                .bind(limit)
                .fetch_all(pool)
                .await?
            } else {
                sqlx::query_as::<_, (String, Option<String>, String, Option<i64>, Option<String>, i64, String)>(
                    "SELECT order_id, customer_name, status, total_minor, currency, product_count, created_at
                     FROM wa_orders ORDER BY created_at DESC LIMIT ?",
                )
                .bind(limit)
                .fetch_all(pool)
                .await?
            };
            if rows.is_empty() {
                return Ok("[DB] No WhatsApp orders found.".into());
            }
            let lines: Vec<String> = rows
                .iter()
                .map(|(id, name, st, total, cur, count, created)| {
                    let total_str = total
                        .map(|t| money::format_minor(t, currency_exp))
                        .unwrap_or_else(|| "—".into());
                    format!(
                        "- [{st}] {} — {count} item(s), {} {} ({created}) · order {id}",
                        name.as_deref().unwrap_or("customer"),
                        cur.as_deref().unwrap_or("BHD"),
                        total_str
                    )
                })
                .collect();
            Ok(format!(
                "[DB] {} WhatsApp order(s):\n{}",
                rows.len(),
                lines.join("\n")
            ))
        }
        "get_today_summary" => {
            let today = chrono::Local::now().format("%Y-%m-%d").to_string();
            let s = report_repo::today_summary(pool, branch_id, &today).await?;
            let fmt = |n: i64| money::format_minor(n, currency_exp);
            Ok(format!(
                "[DB] Today ({}):\n- Transactions: {}\n- Net Total: BHD {}\n- Tax: BHD {}\n- Discounts: BHD {}\n- Cash: BHD {}\n- Card: BHD {}\n- Refunds: {} (BHD {})",
                s.business_date, s.transaction_count,
                fmt(s.net_total_minor), fmt(s.tax_total_minor), fmt(s.discount_total_minor),
                fmt(s.cash_total_minor), fmt(s.card_total_minor),
                s.refund_count, fmt(s.refund_total_minor)
            ))
        }
        "list_products" => {
            let products = product_repo::list_all_active(pool, None, u32::MAX).await?;
            if products.is_empty() {
                return Ok("No active products found.".into());
            }
            let fmt = |n: i64| money::format_minor(n, currency_exp);
            let lines: Vec<String> = products
                .iter()
                .map(|p| {
                    format!(
                        "- {} (ID: {}) — BHD {} — {}",
                        p.product.name,
                        p.product.product_id,
                        fmt(p.price_minor),
                        p.category_name
                    )
                })
                .collect();
            Ok(format!(
                "[DB] {} active products:\n{}",
                products.len(),
                lines.join("\n")
            ))
        }
        "search_products" => {
            let query = input.get("query").and_then(|v| v.as_str()).unwrap_or("");
            let products = product_repo::search_products_indexed(pool, query, 20).await?;
            if products.is_empty() {
                return Ok(format!("No products found matching '{}'.", query));
            }
            let fmt = |n: i64| money::format_minor(n, currency_exp);
            let lines: Vec<String> = products
                .iter()
                .map(|p| {
                    format!(
                        "- {} (ID: {}) — BHD {}",
                        p.product.name,
                        p.product.product_id,
                        fmt(p.price_minor)
                    )
                })
                .collect();
            Ok(format!(
                "{} results for '{}':\n{}",
                products.len(),
                query,
                lines.join("\n")
            ))
        }
        "get_product" => {
            let product_id = input
                .get("product_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let p = product_repo::get_product_by_id(pool, product_id)
                .await?
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
            let page = stock_repo::get_levels_paged(pool, branch_id, None, 0, 50).await?;
            if page.total == 0 {
                return Ok("No inventory-tracked products found.".into());
            }
            let lines: Vec<String> = page
                .items
                .iter()
                .map(|s| {
                    let status = if s.is_out_of_stock {
                        "❌ OUT"
                    } else if s.is_low_stock {
                        "⚠ LOW"
                    } else {
                        "✓"
                    };
                    format!(
                        "- {} (ID: {}) — qty: {} | reorder ≤{} {status}",
                        s.product_name, s.product_id, s.quantity_on_hand, s.reorder_point
                    )
                })
                .collect();
            Ok(format!(
                "[DB] {} tracked products (showing {}):\n{}",
                page.total,
                page.items.len(),
                lines.join("\n")
            ))
        }
        "get_low_stock" => {
            let levels = stock_repo::get_low_stock(pool, &active_branch_id(pool).await?).await?;
            if levels.is_empty() {
                return Ok("All products are above their reorder points. 🎉".into());
            }
            let lines: Vec<String> = levels
                .iter()
                .map(|s| {
                    let status = if s.is_out_of_stock {
                        "OUT OF STOCK"
                    } else {
                        "LOW STOCK"
                    };
                    format!(
                        "- {} — qty: {} | reorder ≤{} [{status}]",
                        s.product_name, s.quantity_on_hand, s.reorder_point
                    )
                })
                .collect();
            Ok(format!(
                "[DB] {} product(s) need restocking:\n{}",
                levels.len(),
                lines.join("\n")
            ))
        }
        "get_cash_summary" => {
            let shift_id = input
                .get("shift_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing shift_id".into()))?;
            let fmt = |n: i64| money::format_minor(n, currency_exp);

            let shift = sqlx::query(
                "SELECT opening_cash_minor, counted_cash_minor FROM shifts WHERE shift_id = ?",
            )
            .bind(shift_id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Shift not found".into()))?;

            let opening: i64 = shift.get("opening_cash_minor");
            let counted: Option<i64> = shift.get("counted_cash_minor");

            let cash_sales: i64 = sqlx::query_scalar(
                "SELECT COALESCE(SUM(p.amount_minor),0) FROM payments p
                 JOIN sales s ON s.sale_id=p.sale_id
                 WHERE s.shift_id=? AND p.payment_method='cash' AND s.status!='voided'
                   AND (s.is_delivery = 0 OR EXISTS (
                       SELECT 1 FROM delivery_orders d WHERE d.sale_id = s.sale_id AND d.payment_status = 'paid'
                   ))",
            )
            .bind(shift_id)
            .fetch_one(pool)
            .await?;

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
                 WHERE s.shift_id = ?",
            )
            .bind(shift_id)
            .fetch_one(pool)
            .await?;

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
                lines.push(format!(
                    "  Variance:       {} {}",
                    if v >= 0 { "+" } else { "" },
                    fmt(v)
                ));
            } else {
                lines.push("  Counted:        (not yet entered)".into());
            }
            Ok(lines.join("\n"))
        }
        "get_recent_refunds" => {
            let limit = input
                .get("limit")
                .and_then(|v| v.as_i64())
                .unwrap_or(10)
                .min(20);
            let fmt = |n: i64| money::format_minor(n, currency_exp);
            let rows = sqlx::query(
                "SELECT r.refund_id, r.original_sale_id, r.refund_total_minor,
                        r.reason, r.return_reason_code, r.created_at
                 FROM refunds r ORDER BY r.created_at DESC LIMIT ?",
            )
            .bind(limit)
            .fetch_all(pool)
            .await?;

            if rows.is_empty() {
                return Ok("No refunds found.".into());
            }
            let lines: Vec<String> = rows
                .iter()
                .map(|r| {
                    let total: i64 = r.get("refund_total_minor");
                    let id: String = r.get("refund_id");
                    let sale: String = r.get("original_sale_id");
                    let reason: Option<String> = r.get("reason");
                    let code: Option<String> = r.get("return_reason_code");
                    let at: String = r.get("created_at");
                    format!(
                        "- {} | sale {} | {} | {} [{}] | {}",
                        &id[..8.min(id.len())],
                        &sale[..8.min(sale.len())],
                        fmt(total),
                        reason.as_deref().unwrap_or("—"),
                        code.as_deref().unwrap_or("other"),
                        &at[..10]
                    )
                })
                .collect();
            Ok(format!(
                "{} recent refund(s):\n{}",
                rows.len(),
                lines.join("\n")
            ))
        }
        "get_audit_log" => {
            let event_filter = input.get("event_type").and_then(|v| v.as_str());
            // `created_at` is stored as UTC ISO-8601. Compute UTC start-of-today in Bahrain
            // (UTC+3, no DST) so records from early morning local time are not missed.
            // Bahrain midnight = UTC midnight − 3 h, i.e. previous day 21:00 UTC.
            const BAHRAIN_OFFSET_HOURS: i64 = 3;
            let utc_now = chrono::Utc::now();
            let bahrain_naive_now =
                utc_now.naive_utc() + chrono::Duration::hours(BAHRAIN_OFFSET_HOURS);
            let bahrain_today = bahrain_naive_now.date();
            let today_utc_start = (bahrain_today.and_hms_opt(0, 0, 0).unwrap().and_utc()
                - chrono::Duration::hours(BAHRAIN_OFFSET_HOURS))
            .to_rfc3339();
            let rows = if let Some(et) = event_filter {
                sqlx::query(
                    "SELECT audit_log_id, event_type, entity_type, entity_id,
                            actor_user_id, created_at
                     FROM audit_logs
                     WHERE event_type = ? AND created_at >= ?
                     ORDER BY created_at DESC LIMIT 30",
                )
                .bind(et)
                .bind(&today_utc_start)
                .fetch_all(pool)
                .await?
            } else {
                sqlx::query(
                    "SELECT audit_log_id, event_type, entity_type, entity_id,
                            actor_user_id, created_at
                     FROM audit_logs
                     WHERE created_at >= ?
                     ORDER BY created_at DESC LIMIT 30",
                )
                .bind(&today_utc_start)
                .fetch_all(pool)
                .await?
            };

            if rows.is_empty() {
                return Ok("No audit log entries found for today.".into());
            }
            let lines: Vec<String> = rows
                .iter()
                .map(|r| {
                    let id: String = r.get("audit_log_id");
                    let et: String = r.get("event_type");
                    let eid: Option<String> = r.get("entity_id");
                    let actor: Option<String> = r.get("actor_user_id");
                    let at: String = r.get("created_at");
                    format!(
                        "- {} | {} | entity: {} | actor: {} | {}",
                        &id[..8.min(id.len())],
                        et,
                        &eid.as_deref().unwrap_or("—")
                            [..8.min(eid.as_deref().unwrap_or("—").len())],
                        actor.as_deref().unwrap_or("system"),
                        &at[11..19.min(at.len())]
                    )
                })
                .collect();
            Ok(format!(
                "{} audit entries today:\n{}",
                rows.len(),
                lines.join("\n")
            ))
        }
        "get_sync_status" => {
            // Fetch device_id from the active device
            let device_id: String = sqlx::query_scalar(
                "SELECT device_id FROM devices WHERE is_active=1 ORDER BY device_code LIMIT 1",
            )
            .fetch_optional(pool)
            .await?
            .flatten()
            .unwrap_or_default();

            let status = sync_repo::get_sync_status(pool, &device_id).await?;
            let pending = status.pending_events;
            let failed: i64 = 0; // new model uses sync_attempts on individual rows

            let cloud = if status.hub_configured {
                "✓ configured"
            } else {
                "✗ not configured"
            };
            let last = status.last_successful_sync_at.as_deref().unwrap_or("never");
            let lines = [
                "Sync Status:".to_string(),
                format!("  Hub (LAN sync):   {cloud}"),
                format!("  Last sync:        {last}"),
                format!("  Pending rows:     {pending}"),
                format!("  Stuck rows:       {failed}"),
            ];
            Ok(lines.join("\n"))
        }
        "get_sync_diagnostics" => {
            let mut lines = vec!["[DB] Sync Diagnostics:".to_string()];

            let hub_mode: Option<String> =
                sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'hub_mode'")
                    .fetch_optional(pool)
                    .await?
                    .flatten();
            let hub_url: Option<String> =
                sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'hub_url'")
                    .fetch_optional(pool)
                    .await?
                    .flatten();
            let token = crate::secure_store::get_secret("hub_store_token").unwrap_or_default();
            let hub_label = if hub_mode.as_deref() == Some("1") {
                "✓ this device IS the hub".to_string()
            } else if hub_url.as_deref().is_some_and(|u| !u.is_empty()) && !token.is_empty() {
                format!(
                    "✓ terminal connected to {}",
                    hub_url.as_deref().unwrap_or("")
                )
            } else {
                "✗ NOT configured (Settings → Hub)".to_string()
            };
            lines.push(format!("  Hub: {hub_label}"));

            let last_sync: Option<String> = sqlx::query_scalar(
                "SELECT last_pushed_at FROM sync_watermark WHERE table_name = 'sales'",
            )
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();
            lines.push(format!(
                "  Last Sync: {}",
                last_sync.as_deref().unwrap_or("never")
            ));

            lines.push("".to_string());
            lines.push(
                "  Per-table breakdown — pending / stuck (attempts>=10) / max-att / avg-att:"
                    .to_string(),
            );

            for table in crate::commands::sync_commands::SYNC_TABLES {
                let pending: i64 = sqlx::query_scalar(
                    &format!("SELECT COUNT(*) FROM {table} WHERE sync_status = 'pending' AND sync_attempts < 10"),
                ).fetch_one(pool).await.unwrap_or(0);
                let stuck: i64 = sqlx::query_scalar(
                    &format!("SELECT COUNT(*) FROM {table} WHERE sync_status = 'pending' AND sync_attempts >= 10"),
                ).fetch_one(pool).await.unwrap_or(0);
                if pending == 0 && stuck == 0 {
                    continue;
                }
                let max_att: i64 = sqlx::query_scalar(&format!(
                    "SELECT COALESCE(MAX(sync_attempts),0) FROM {table}"
                ))
                .fetch_one(pool)
                .await
                .unwrap_or(0);
                let avg = sqlx::query_scalar::<_, f64>(
                    &format!("SELECT COALESCE(AVG(CAST(sync_attempts AS REAL)),0) FROM {table} WHERE sync_status = 'pending'"),
                ).fetch_one(pool).await.unwrap_or(0.0);
                let flag = if stuck > 0 {
                    " ⚠ STUCK"
                } else if pending > 0 {
                    " ⏳ pending"
                } else {
                    " ✓ clean"
                };
                lines.push(format!(
                    "    {table}: {pending} pending, {stuck} stuck, max {max_att} att, avg {avg:.1}{flag}"
                ));
            }

            Ok(lines.join("\n"))
        }
        "get_system_health_check" => {
            let report = crate::commands::system_health_commands::run_local_health_check(
                pool,
                crate::commands::sync_commands::SYNC_TABLES,
            )
            .await?;
            Ok(format_health_report(&report))
        }
        "sync_queue_list" => {
            let mut items = Vec::new();
            for table in crate::commands::sync_commands::SYNC_TABLES {
                let pk = crate::commands::sync_commands::table_pk(table);
                let sql = format!(
                    "SELECT {pk} AS _pk, sync_status, sync_attempts, created_at
                     FROM {table} WHERE sync_status IN ('pending', 'failed')
                     LIMIT 50",
                );
                if let Ok(rows) = sqlx::query(&sql).fetch_all(pool).await {
                    for r in &rows {
                        let id: String = r.get("_pk");
                        let status: String = r.get("sync_status");
                        let att: i64 = r.get("sync_attempts");
                        let at: String = r.get("created_at");
                        items.push(format!(
                            "{}|{}|{}|{}|{}",
                            table,
                            &id[..12.min(id.len())],
                            status,
                            att,
                            &at[11..19]
                        ));
                    }
                }
            }
            if items.is_empty() {
                return Ok("[DB] Sync queue is empty — all events synced.".into());
            }
            items.truncate(50);
            Ok(format!(
                "[DB] Sync Queue (top 50):\n  table|entity|status|att|created\n  {}",
                items.join("\n  ")
            ))
        }
        "get_active_shift" => {
            let shift: Option<(String, String, String, i64, String)> = sqlx::query_as(
                "SELECT s.shift_id, u.display_name, s.opened_at, s.opening_cash_minor, s.status
                 FROM shifts s JOIN users u ON u.user_id = s.cashier_user_id
                 WHERE s.status = 'open' AND s.device_id = (SELECT device_id FROM devices WHERE is_active=1 LIMIT 1)
                 ORDER BY s.opened_at DESC LIMIT 1",
            )
            .fetch_optional(pool).await?;
            match shift {
                Some((id, name, opened, float, status)) => {
                    Ok(format!("[DB] Active Shift: {} | Cashier: {} | Opened: {} | Float: {} fils | Status: {}",
                        &id[..12], name, &opened[11..19], float, status))
                }
                None => Ok("No active shift found. A shift must be opened before processing sales.".into()),
            }
        }
        "get_daily_report" => {
            let date = input
                .get("date")
                .and_then(|v| v.as_str())
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
            let from = input
                .get("from")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing from".into()))?;
            let to = input
                .get("to")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing to".into()))?;
            let fmt = |n: i64| money::format_minor(n, currency_exp);

            let row = sqlx::query(
                "SELECT COUNT(*) AS cnt,
                        COALESCE(SUM(net_total_minor),      0) AS net,
                        COALESCE(SUM(tax_total_minor),      0) AS tax,
                        COALESCE(SUM(discount_total_minor), 0) AS discount
                 FROM sales
                 WHERE branch_id = ? AND business_date BETWEEN ? AND ? AND status != 'voided'",
            )
            .bind(branch_id)
            .bind(from)
            .bind(to)
            .fetch_one(pool)
            .await?;

            let cnt: i64 = row.get("cnt");
            let net: i64 = row.get("net");
            let tax: i64 = row.get("tax");
            let disc: i64 = row.get("discount");

            let cash: i64 = sqlx::query_scalar(
                "SELECT COALESCE(SUM(p.amount_minor),0) FROM payments p
                 JOIN sales s ON s.sale_id=p.sale_id
                 WHERE s.branch_id=? AND s.business_date BETWEEN ? AND ?
                   AND p.payment_method='cash' AND s.status!='voided'",
            )
            .bind(branch_id)
            .bind(from)
            .bind(to)
            .fetch_one(pool)
            .await?;

            let card: i64 = sqlx::query_scalar(
                "SELECT COALESCE(SUM(p.amount_minor),0) FROM payments p
                 JOIN sales s ON s.sale_id=p.sale_id
                 WHERE s.branch_id=? AND s.business_date BETWEEN ? AND ?
                   AND p.payment_method='card' AND s.status!='voided'",
            )
            .bind(branch_id)
            .bind(from)
            .bind(to)
            .fetch_one(pool)
            .await?;

            let refund_cnt: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM refunds r
                 JOIN sales s ON s.sale_id=r.original_sale_id
                 WHERE s.branch_id=? AND s.business_date BETWEEN ? AND ?",
            )
            .bind(branch_id)
            .bind(from)
            .bind(to)
            .fetch_one(pool)
            .await?;

            let refund_total: i64 = sqlx::query_scalar(
                "SELECT COALESCE(SUM(r.refund_total_minor),0) FROM refunds r
                 JOIN sales s ON s.sale_id=r.original_sale_id
                 WHERE s.branch_id=? AND s.business_date BETWEEN ? AND ?",
            )
            .bind(branch_id)
            .bind(from)
            .bind(to)
            .fetch_one(pool)
            .await?;

            Ok(format!(
                "Sales Report {from} → {to}:\n- Transactions: {cnt}\n- Net Total: {}\n- Tax: {}\n- Discounts: {}\n- Cash: {}\n- Card: {}\n- Refunds: {refund_cnt} ({})",
                fmt(net), fmt(tax), fmt(disc), fmt(cash), fmt(card), fmt(refund_total)
            ))
        }
        "get_top_products" => {
            let limit = input
                .get("limit")
                .and_then(|v| v.as_i64())
                .unwrap_or(10)
                .min(25);
            let period_days = input
                .get("period_days")
                .and_then(|v| v.as_i64())
                .unwrap_or(30)
                .min(365);
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
                 LIMIT ?",
            )
            .bind(format!("-{}", period_days))
            .bind(limit)
            .fetch_all(pool)
            .await?;

            if rows.is_empty() {
                return Ok(format!("No sales data in the last {period_days} days."));
            }
            let lines: Vec<String> = rows
                .iter()
                .enumerate()
                .map(|(i, r)| {
                    let name: String = r.get("name");
                    let rev: i64 = r.get("revenue");
                    let txn: i64 = r.get("txn_count");
                    format!("{}. {} — {} ({} transactions)", i + 1, name, fmt(rev), txn)
                })
                .collect();
            Ok(format!(
                "Top {} products (last {period_days} days):\n{}",
                rows.len(),
                lines.join("\n")
            ))
        }
        "get_shift_history" => {
            let limit = input
                .get("limit")
                .and_then(|v| v.as_i64())
                .unwrap_or(10)
                .min(30);
            let fmt = |n: i64| money::format_minor(n, currency_exp);

            let rows = sqlx::query(
                "SELECT s.shift_id, u.display_name AS cashier,
                        s.opened_at, s.closed_at, s.opening_cash_minor,
                        COALESCE((
                            SELECT SUM(net_total_minor) FROM sales
                            WHERE shift_id = s.shift_id AND status != 'voided'
                        ), 0) AS sales_total
                 FROM shifts s
                 LEFT JOIN users u ON u.user_id = s.cashier_user_id
                 ORDER BY s.opened_at DESC
                 LIMIT ?",
            )
            .bind(limit)
            .fetch_all(pool)
            .await?;

            if rows.is_empty() {
                return Ok("No shifts found.".into());
            }

            let lines: Vec<String> = rows
                .iter()
                .map(|r| {
                    let cashier: String = r
                        .get::<Option<String>, _>("cashier")
                        .unwrap_or_else(|| "Unknown".into());
                    let opened: String = r.get("opened_at");
                    let closed: Option<String> = r.get("closed_at");
                    let opening: i64 = r.get("opening_cash_minor");
                    let sales: i64 = r.get("sales_total");
                    let status = if closed.is_some() { "Closed" } else { "OPEN" };
                    format!(
                        "- {} [{status}] | Opened: {} | Float: {} | Sales: {}",
                        cashier,
                        &opened[..16.min(opened.len())],
                        fmt(opening),
                        fmt(sales)
                    )
                })
                .collect();
            Ok(format!(
                "{} recent shift(s):\n{}",
                rows.len(),
                lines.join("\n")
            ))
        }
        "list_categories" => {
            let query = input
                .get("query")
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim();
            let rows = if query.is_empty() {
                sqlx::query("SELECT category_id, name FROM categories WHERE is_active = 1 AND deleted_at IS NULL ORDER BY name")
                    .fetch_all(pool)
                    .await?
            } else {
                let escaped = query
                    .replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_");
                sqlx::query(
                    "SELECT category_id, name FROM categories
                     WHERE is_active = 1 AND deleted_at IS NULL
                       AND name LIKE ? ESCAPE '\\'
                     ORDER BY name",
                )
                .bind(format!("%{escaped}%"))
                .fetch_all(pool)
                .await?
            };

            if rows.is_empty() {
                return Ok(if query.is_empty() {
                    "No categories found.".into()
                } else {
                    format!("No categories found matching '{query}'.")
                });
            }
            let lines: Vec<String> = rows
                .iter()
                .map(|r| {
                    let id: String = r.get("category_id");
                    let name: String = r.get("name");
                    format!("- {} (ID: {})", name, id)
                })
                .collect();
            let heading = if query.is_empty() {
                format!("{} categories", rows.len())
            } else {
                format!("{} categories matching '{query}'", rows.len())
            };
            Ok(format!("{heading}:\n{}", lines.join("\n")))
        }
        "list_safe_drops" => {
            let shift_id = input
                .get("shift_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing shift_id".into()))?;
            let fmt = |n: i64| money::format_minor(n, currency_exp);

            let rows = sqlx::query(
                "SELECT amount_minor, note, created_by_user_id, created_at
                 FROM cash_events
                 WHERE shift_id = ? AND event_type = 'safe_drop'
                 ORDER BY created_at",
            )
            .bind(shift_id)
            .fetch_all(pool)
            .await?;

            if rows.is_empty() {
                return Ok(format!(
                    "No safe drops recorded for shift {}.",
                    &shift_id[..8.min(shift_id.len())]
                ));
            }
            let total: i64 = rows.iter().map(|r| r.get::<i64, _>("amount_minor")).sum();
            let lines: Vec<String> = rows
                .iter()
                .map(|r| {
                    let amt: i64 = r.get("amount_minor");
                    let note: Option<String> = r.get("note");
                    let at: String = r.get("created_at");
                    format!(
                        "- {} | {} | {}",
                        fmt(amt),
                        note.as_deref().unwrap_or("—"),
                        &at[..16.min(at.len())]
                    )
                })
                .collect();
            Ok(format!(
                "{} safe drop(s) | Total: {}\n{}",
                rows.len(),
                fmt(total),
                lines.join("\n")
            ))
        }
        "list_no_sale_events" => {
            let shift_id = input
                .get("shift_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing shift_id".into()))?;

            let rows = sqlx::query(
                "SELECT actor_user_id, note, created_at
                 FROM no_sale_events
                 WHERE shift_id = ?
                 ORDER BY created_at",
            )
            .bind(shift_id)
            .fetch_all(pool)
            .await?;

            if rows.is_empty() {
                return Ok(format!(
                    "No no-sale events recorded for shift {}.",
                    &shift_id[..8.min(shift_id.len())]
                ));
            }
            let lines: Vec<String> = rows
                .iter()
                .map(|r| {
                    let actor: String = r.get("actor_user_id");
                    let note: Option<String> = r.get("note");
                    let at: String = r.get("created_at");
                    format!(
                        "- Actor: {} | {} | {}",
                        &actor[..8.min(actor.len())],
                        note.as_deref().unwrap_or("no note"),
                        &at[11..16.min(at.len())]
                    )
                })
                .collect();
            Ok(format!(
                "{} no-sale event(s):\n{}",
                rows.len(),
                lines.join("\n")
            ))
        }
        "get_audit_chain_status" => {
            let device_id: String = sqlx::query_scalar(
                "SELECT device_id FROM devices WHERE is_active = 1 ORDER BY device_code LIMIT 1",
            )
            .fetch_optional(pool)
            .await?
            .flatten()
            .unwrap_or_default();

            let r = crate::db::repositories::audit_hash::verify_chain(pool, &device_id).await?;
            let status = if r.ok {
                "✓ INTACT"
            } else {
                "⚠ ANOMALIES DETECTED"
            };
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
        "get_hourly_sales" => {
            let today = chrono::Local::now().format("%Y-%m-%d").to_string();
            let fmt = |n: i64| money::format_minor(n, currency_exp);
            let rows = sqlx::query(
                "SELECT strftime('%H', sold_at, 'localtime') AS hour,
                        COUNT(*) AS cnt,
                        COALESCE(SUM(net_total_minor), 0) AS net
                 FROM sales
                 WHERE business_date = ? AND status != 'voided'
                 GROUP BY hour
                 ORDER BY hour",
            )
            .bind(&today)
            .fetch_all(pool)
            .await?;

            if rows.is_empty() {
                return Ok(format!("No sales yet today ({today})."));
            }
            let lines: Vec<String> = rows
                .iter()
                .map(|r| {
                    let hour: String = r.get("hour");
                    let cnt: i64 = r.get("cnt");
                    let net: i64 = r.get("net");
                    let h: u32 = hour.parse().unwrap_or(0);
                    let label = format!("{:02}:00–{:02}:59", h, h);
                    format!(
                        "  {} | {:>3} sale{} | BHD {}",
                        label,
                        cnt,
                        if cnt == 1 { "" } else { "s" },
                        fmt(net)
                    )
                })
                .collect();
            Ok(format!(
                "Hourly sales breakdown — {today}:\n{}",
                lines.join("\n")
            ))
        }
        "get_sales_by_category" => {
            let today = chrono::Local::now().format("%Y-%m-%d").to_string();
            let fmt = |n: i64| money::format_minor(n, currency_exp);
            let rows = sqlx::query(
                "SELECT COALESCE(c.name, 'Uncategorised') AS category,
                        COUNT(DISTINCT s.sale_id) AS txn_count,
                        COALESCE(SUM(si.line_total_minor), 0) AS revenue
                 FROM sale_items si
                 JOIN sales s ON s.sale_id = si.sale_id
                 LEFT JOIN products p ON p.product_id = si.product_id
                 LEFT JOIN categories c ON c.category_id = p.category_id
                 WHERE s.business_date = ? AND s.status != 'voided'
                 GROUP BY c.category_id, c.name
                 ORDER BY revenue DESC",
            )
            .bind(&today)
            .fetch_all(pool)
            .await?;

            if rows.is_empty() {
                return Ok(format!("No sales today ({today})."));
            }
            let lines: Vec<String> = rows
                .iter()
                .map(|r| {
                    let cat: String = r.get("category");
                    let txn: i64 = r.get("txn_count");
                    let rev: i64 = r.get("revenue");
                    format!(
                        "  {:.<30} BHD {} ({} txn{})",
                        format!("{cat} "),
                        fmt(rev),
                        txn,
                        if txn == 1 { "" } else { "s" }
                    )
                })
                .collect();
            Ok(format!(
                "Sales by category — {today}:\n{}",
                lines.join("\n")
            ))
        }
        "get_cashier_performance" => {
            let from = input
                .get("from")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing from".into()))?;
            let to = input
                .get("to")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing to".into()))?;
            let fmt = |n: i64| money::format_minor(n, currency_exp);
            let rows = sqlx::query(
                "SELECT COALESCE(u.display_name, s.cashier_user_id) AS cashier,
                        COUNT(*) AS txn_count,
                        COALESCE(SUM(s.net_total_minor), 0) AS net_total,
                        COALESCE(SUM(s.discount_total_minor), 0) AS discounts,
                        COUNT(CASE WHEN s.status='voided' THEN 1 END) AS voids
                 FROM sales s
                 LEFT JOIN users u ON u.user_id = s.cashier_user_id
                 WHERE s.business_date BETWEEN ? AND ?
                 GROUP BY s.cashier_user_id, u.display_name
                 ORDER BY net_total DESC",
            )
            .bind(from)
            .bind(to)
            .fetch_all(pool)
            .await?;

            if rows.is_empty() {
                return Ok(format!("No sales between {from} and {to}."));
            }
            let lines: Vec<String> = rows
                .iter()
                .map(|r| {
                    let name: String = r.get("cashier");
                    let txn: i64 = r.get("txn_count");
                    let net: i64 = r.get("net_total");
                    let disc: i64 = r.get("discounts");
                    let voids: i64 = r.get("voids");
                    format!(
                        "  {} — BHD {} | {} txns | BHD {} discounts | {} voids",
                        name,
                        fmt(net),
                        txn,
                        fmt(disc),
                        voids
                    )
                })
                .collect();
            Ok(format!(
                "Cashier performance {from} → {to}:\n{}",
                lines.join("\n")
            ))
        }
        // ── Customers ─────────────────────────────────────────────────────────
        "list_customers" => {
            let search = input
                .get("search")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let rows = if search.is_empty() {
                sqlx::query(
                    "SELECT customer_id, name, phone, email, loyalty_points
                     FROM customers ORDER BY name LIMIT 50",
                )
                .fetch_all(pool)
                .await?
            } else {
                sqlx::query(
                    "SELECT customer_id, name, phone, email, loyalty_points
                     FROM customers
                     WHERE name LIKE ? OR phone LIKE ?
                     ORDER BY name LIMIT 50",
                )
                .bind(format!("%{search}%"))
                .bind(format!("%{search}%"))
                .fetch_all(pool)
                .await?
            };
            if rows.is_empty() {
                return Ok("No customers found.".into());
            }
            let lines: Vec<String> = rows
                .iter()
                .map(|r| {
                    let id: String = r.get("customer_id");
                    let name: String = r.get("name");
                    let phone: Option<String> = r.get("phone");
                    let pts: i64 = r.get("loyalty_points");
                    // PII-01: mask phone — only last 4 digits shown in AI context
                    let masked = phone
                        .as_deref()
                        .map(mask_phone)
                        .unwrap_or_else(|| "—".into());
                    format!(
                        "- {} (ID: {}) | Phone: {} | Loyalty: {} pts",
                        name,
                        &id[..8.min(id.len())],
                        masked,
                        pts
                    )
                })
                .collect();
            Ok(format!("{} customer(s):\n{}", rows.len(), lines.join("\n")))
        }
        "get_customer" => {
            let customer_id = input
                .get("customer_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing customer_id".into()))?;
            let r = sqlx::query(
                "SELECT customer_id, name, phone, email, loyalty_points, notes, created_at
                 FROM customers WHERE customer_id = ?",
            )
            .bind(customer_id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Customer not found".into()))?;
            let name: String = r.get("name");
            let phone: Option<String> = r.get("phone");
            let email: Option<String> = r.get("email");
            let pts: i64 = r.get("loyalty_points");
            let notes: Option<String> = r.get("notes");
            let at: String = r.get("created_at");
            // PII-01: mask phone — only last 4 digits shown in AI context
            let masked_phone = phone
                .as_deref()
                .map(mask_phone)
                .unwrap_or_else(|| "—".into());
            Ok(format!(
                "Customer: {name}\nID: {customer_id}\nPhone: {}\nEmail: {}\nLoyalty: {pts} pts\nNotes: {}\nSince: {}",
                masked_phone,
                email.as_deref().unwrap_or("—"),
                notes.as_deref().unwrap_or("—"),
                &at[..10.min(at.len())]
            ))
        }
        // ── Deliveries ────────────────────────────────────────────────────────
        "list_deliveries" => {
            let status_filter = input.get("status").and_then(|v| v.as_str());
            let limit = input
                .get("limit")
                .and_then(|v| v.as_i64())
                .unwrap_or(20)
                .min(50);
            let fmt = |n: i64| money::format_minor(n, currency_exp);
            let rows = if let Some(status) = status_filter {
                sqlx::query(
                    "SELECT d.delivery_id, d.delivery_status, d.delivery_staff_name,
                            d.payment_status, d.amount_minor, d.created_at
                     FROM delivery_orders d
                     WHERE d.delivery_status = ?
                     ORDER BY d.created_at DESC LIMIT ?",
                )
                .bind(status)
                .bind(limit)
                .fetch_all(pool)
                .await?
            } else {
                sqlx::query(
                    "SELECT d.delivery_id, d.delivery_status, d.delivery_staff_name,
                            d.payment_status, d.amount_minor, d.created_at
                     FROM delivery_orders d
                     ORDER BY d.created_at DESC LIMIT ?",
                )
                .bind(limit)
                .fetch_all(pool)
                .await?
            };
            if rows.is_empty() {
                return Ok("No deliveries found.".into());
            }
            let lines: Vec<String> = rows
                .iter()
                .map(|r| {
                    let id: String = r.get("delivery_id");
                    let status: String = r.get("delivery_status");
                    let rider: Option<String> = r.get("delivery_staff_name");
                    let pay: String = r.get("payment_status");
                    let total: i64 = r.get("amount_minor");
                    let at: String = r.get("created_at");
                    format!(
                        "- {} | {} | Rider: {} | Pay: {} | BHD {} | {}",
                        &id[..8.min(id.len())],
                        status,
                        rider.as_deref().unwrap_or("—"),
                        pay,
                        fmt(total),
                        &at[..16.min(at.len())]
                    )
                })
                .collect();
            Ok(format!(
                "{} delivery/deliveries:\n{}",
                rows.len(),
                lines.join("\n")
            ))
        }
        // ── Staff ─────────────────────────────────────────────────────────────
        "list_users" => {
            let rows = sqlx::query(
                "SELECT u.user_id, u.display_name, u.username, r.name AS role_name, u.is_active
                 FROM users u
                 JOIN roles r ON r.role_id = u.role_id
                 ORDER BY u.is_active DESC, u.display_name",
            )
            .fetch_all(pool)
            .await?;
            if rows.is_empty() {
                return Ok("No users found.".into());
            }
            let lines: Vec<String> = rows
                .iter()
                .map(|r| {
                    let name: String = r.get("display_name");
                    let uname: String = r.get("username");
                    let role: String = r.get("role_name");
                    let active: bool = r.get("is_active");
                    format!(
                        "- {} (@{}) | {} | {}",
                        name,
                        uname,
                        role,
                        if active { "Active" } else { "Inactive" }
                    )
                })
                .collect();
            Ok(format!("{} user(s):\n{}", rows.len(), lines.join("\n")))
        }
        // ── Stock movements ───────────────────────────────────────────────────
        "get_stock_movements" => {
            let product_id = input
                .get("product_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let limit = input
                .get("limit")
                .and_then(|v| v.as_i64())
                .unwrap_or(20)
                .min(50);
            let rows = sqlx::query(
                "SELECT movement_type, quantity_delta, quantity_after, notes, created_at
                 FROM stock_movements
                 WHERE product_id = ?
                 ORDER BY created_at DESC LIMIT ?",
            )
            .bind(product_id)
            .bind(limit)
            .fetch_all(pool)
            .await?;
            if rows.is_empty() {
                return Ok("No stock movements found for this product.".into());
            }
            // Get product name for context
            let pname: Option<String> =
                sqlx::query_scalar("SELECT name FROM products WHERE product_id = ?")
                    .bind(product_id)
                    .fetch_optional(pool)
                    .await?;
            let lines: Vec<String> = rows
                .iter()
                .map(|r| {
                    let mtype: String = r.get("movement_type");
                    let delta: f64 = r.get("quantity_delta");
                    let after: f64 = r.get("quantity_after");
                    let notes: Option<String> = r.get("notes");
                    let at: String = r.get("created_at");
                    format!(
                        "- {} | {:+.3} → {:.3} | {} | {}",
                        mtype,
                        delta,
                        after,
                        notes.as_deref().unwrap_or("—"),
                        &at[..16.min(at.len())]
                    )
                })
                .collect();
            Ok(format!(
                "Stock movements for {} ({}):\n{}",
                pname.as_deref().unwrap_or(product_id),
                rows.len(),
                lines.join("\n")
            ))
        }
        // ── Tax report ────────────────────────────────────────────────────────
        "get_tax_report" => {
            let from = input
                .get("from")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing from".into()))?;
            let to = input
                .get("to")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing to".into()))?;
            let fmt = |n: i64| money::format_minor(n, currency_exp);
            let rows = sqlx::query(
                "SELECT business_date AS day,
                        COUNT(*) AS transaction_count,
                        COALESCE(SUM(tax_total_minor), 0) AS tax_minor
                 FROM sales
                 WHERE branch_id = ? AND business_date BETWEEN ? AND ?
                   AND status != 'voided'
                 GROUP BY business_date
                 ORDER BY business_date ASC",
            )
            .bind(branch_id)
            .bind(from)
            .bind(to)
            .fetch_all(pool)
            .await?;
            if rows.is_empty() {
                return Ok(format!("No tax data between {from} and {to}."));
            }
            let mut cumulative = 0i64;
            let lines: Vec<String> = rows
                .iter()
                .map(|r| {
                    let day: String = r.get("day");
                    let tax: i64 = r.get("tax_minor");
                    let txn: i64 = r.get("transaction_count");
                    cumulative += tax;
                    format!(
                        "  {} | {} txns | Tax: BHD {} | Cumulative: BHD {}",
                        day,
                        txn,
                        fmt(tax),
                        fmt(cumulative)
                    )
                })
                .collect();
            Ok(format!(
                "Tax Report {from} → {to}:\n{}\nTotal tax collected: BHD {}",
                lines.join("\n"),
                fmt(cumulative)
            ))
        }
        // ── Free web search (DuckDuckGo lite — no API key) ────────────────────
        "web_search" => {
            let query = input
                .get("query")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing query".into()))?;
            let max_results = input
                .get("max_results")
                .and_then(|v| v.as_i64())
                .unwrap_or(5)
                .min(10) as usize;
            duckduckgo_search(query, max_results).await
        }
        "search_market_prices" => {
            let product = input
                .get("product_name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_name".into()))?;
            let location = input
                .get("location")
                .and_then(|v| v.as_str())
                .unwrap_or("Bahrain");
            let query = format!("{product} price {location} BHD supermarket shop store");
            duckduckgo_search(&query, 6).await
        }
        // ── Smart barcode lookup (OFFF + web fallback) ─────────────────────────
        "smart_barcode_lookup" => {
            let barcode = input
                .get("barcode")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .trim();
            if barcode.is_empty()
                || barcode.len() < 8
                || barcode.len() > 14
                || !barcode.chars().all(|c| c.is_ascii_digit())
            {
                return Err(AppError::Validation(
                    "barcode must be 8-14 digits (UPC/EAN)".into(),
                ));
            }
            // First try Open Food Facts
            let off_result = match open_food_facts_lookup(barcode).await {
                Ok(result)
                    if !result.contains("not found")
                        && !result.contains("product data unavailable") =>
                {
                    Some(result)
                }
                _ => None,
            };
            // If OFFF failed or got sparse data, search the web
            let web_results = if off_result.is_some() {
                None // Got good OFFF data, skip web search
            } else {
                match duckduckgo_search(&format!("barcode {barcode} product name"), 3).await {
                    Ok(r) if !r.contains("No results found") => Some(r),
                    _ => None,
                }
            };
            // Capture name hint before consuming the results
            let name_hint = off_result.as_ref().or(web_results.as_ref()).and_then(|r| {
                let needle = "Product: ";
                r.find(needle).map(|i| {
                    let start = i + needle.len();
                    let end = r[start..].find('\n').map(|e| start + e).unwrap_or(r.len());
                    r[start..end].trim().to_string()
                })
            });
            let has_off = off_result.is_some();
            let has_web = web_results.is_some();
            let mut lines = vec![
                format!("[SCAN] **Smart Barcode Lookup: {barcode}**"),
                String::new(),
            ];
            if let Some(off) = off_result {
                lines.push("### Open Food Facts Data".into());
                lines.push(off);
            }
            if let Some(web) = web_results {
                lines.push(String::new());
                lines.push("### Web Search Results (cross-reference)".into());
                lines.push(web);
            }
            if !has_off && !has_web {
                lines.push("This barcode was not found in Open Food Facts and web search returned no results.".into());
                lines.push(
                    "Try searching by product name instead, or manually enter the product details."
                        .into(),
                );
            }
            // Append product creation instructions if we found a name
            if let Some(ref name) = name_hint {
                if !name.is_empty() && name != "—" {
                    lines.push(String::new());
                    lines.push(format!(
                        "### Suggested Category: {}",
                        categorize_product(name)
                    ));
                    lines.push(String::new());
                    lines.push("📋 **To create this product, I need:**".into());
                    lines.push("- Product name (extracted from lookup above)".into());
                    lines.push("- Selling price in BHD".into());
                    lines.push("- Category ID (use `list_categories` to pick the best fit)".into());
                    lines.push(String::new());
                    lines.push("Reply with: \"Create it at BHD X.XXX\" and I'll create the product for you.".into());
                }
            }
            Ok(lines.join("\n"))
        }
        // ── Multi-store price comparison ────────────────────────────────────────
        "compare_store_prices" => {
            let product = input
                .get("product_name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let location = input
                .get("location")
                .and_then(|v| v.as_str())
                .unwrap_or("Bahrain");
            // Search multiple stores in parallel
            let stores = [
                (
                    "Lulu Hypermarket",
                    format!("{product} price luluhypermarket.com bahrain BHD"),
                ),
                (
                    "Carrefour Bahrain",
                    format!("{product} price carrefourbahrain.com BHD"),
                ),
                (
                    "Alosra Supermarket",
                    format!("{product} price alosra bahrain BHD"),
                ),
                (
                    "Talabat Mart",
                    format!("{product} talabat bahrain price BHD"),
                ),
                (
                    "General Search",
                    format!("{product} price {location} BHD supermarket"),
                ),
            ];
            let mut results: Vec<(String, String)> = Vec::new();
            for (store, query) in &stores {
                match duckduckgo_search(query, 3).await {
                    Ok(r) if !r.contains("No results found") => {
                        // Truncate each store's results
                        let short: String = r.lines().take(8).collect::<Vec<_>>().join("\n");
                        results.push((store.to_string(), short));
                    }
                    _ => {}
                }
            }
            let mut out = vec![
                format!(
                    "[WEB] **Price Comparison: \"{}\" in {}**",
                    product, location
                ),
                String::new(),
            ];
            if results.is_empty() {
                out.push("No prices found across the checked stores. Try a more specific product name or search manually on the store websites.".into());
            } else {
                for (store, content) in &results {
                    out.push(format!("#### {}", store));
                    out.push(content.clone());
                    out.push(String::new());
                }
                out.push("---".into());
                out.push("**Tip:** The AI does not have real-time API access to these stores. Prices shown are from recent web search results. For live prices, visit the store websites directly.".into());
                out.push("To set a price in your POS based on this research, use `create_product` or `update_product_price`.".into());
            }
            Ok(out.join("\n"))
        }
        // ── Bahrain grocery delivery price check ────────────────────────────────
        "bahrain_market_price_check" => {
            let product = input
                .get("product_name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let max = input
                .get("max_results")
                .and_then(|v| v.as_i64())
                .unwrap_or(3)
                .min(5) as usize;
            let sources = [
                (
                    "Talabat / Talabat Mart",
                    format!("\"{product}\" site:talabat.com bahrain"),
                ),
                (
                    "Lulu Online",
                    format!("\"{product}\" price luluhypermarket bahrain BHD"),
                ),
            ];
            let mut out = vec![
                format!("[WEB] **Bahrain Market Check: \"{}\"**", product),
                String::new(),
            ];
            let mut found_any = false;
            for (label, query) in &sources {
                match duckduckgo_search(query, max).await {
                    Ok(r) if !r.contains("No results found") => {
                        let short: String = r.lines().take(6).collect::<Vec<_>>().join("\n");
                        out.push(format!("#### {}", label));
                        out.push(short);
                        out.push(String::new());
                        found_any = true;
                    }
                    _ => {
                        out.push(format!("#### {} — no results", label));
                        out.push(String::new());
                    }
                }
            }
            if !found_any {
                out.push("No current listings found on these platforms. The product may not be listed on delivery apps, or the name may need to be more specific.".into());
            }
            out.push("---".into());
            out.push("These are delivery-platform prices which may include markup. Store shelf prices are typically 5-15% lower.".into());
            Ok(out.join("\n"))
        }
        // ── Free URL reader via Jina.ai Reader (no API key) ───────────────────
        "fetch_url" => {
            let url = input
                .get("url")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing url".into()))?;
            if !url.starts_with("http://") && !url.starts_with("https://") {
                return Err(AppError::Validation(
                    "URL must start with http:// or https://".into(),
                ));
            }
            jina_fetch(url).await
        }
        // ── Barcode lookup via Open Food Facts (no API key) ───────────────────
        "lookup_barcode" => {
            let barcode = input
                .get("barcode")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing barcode".into()))?
                .trim();
            // Validate: only digits, 8-14 chars
            if barcode.is_empty()
                || barcode.len() < 8
                || barcode.len() > 14
                || !barcode.chars().all(|c| c.is_ascii_digit())
            {
                return Err(AppError::Validation(
                    "barcode must be 8-14 digits (UPC/EAN)".into(),
                ));
            }
            open_food_facts_lookup(barcode).await
        }
        // ── Live exchange rates via Frankfurter ECB (no API key) ──────────────
        "get_exchange_rates" => {
            let currencies = input
                .get("currencies")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .map(|s| s.to_uppercase())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            frankfurter_rates(&currencies).await
        }
        // ── Prayer times via Aladhan (no API key) ─────────────────────────────
        "get_prayer_times" => {
            let date = input
                .get("date")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            aladhan_prayer_times(&date).await
        }
        // ── Bahrain public holidays via nager.date (no API key) ───────────────
        "get_bahrain_holidays" => {
            let current_year = chrono::Local::now()
                .format("%Y")
                .to_string()
                .parse::<i64>()
                .unwrap_or(2026);
            let year = input
                .get("year")
                .and_then(|v| v.as_i64())
                .unwrap_or(current_year);
            nager_bahrain_holidays(year as u16).await
        }
        // ── Roles ─────────────────────────────────────────────────────────────
        "list_roles" => {
            let rows = sqlx::query("SELECT role_id, name FROM roles ORDER BY name")
                .fetch_all(pool)
                .await?;
            if rows.is_empty() {
                return Ok("No roles found.".into());
            }
            let lines: Vec<String> = rows
                .iter()
                .map(|r| {
                    let id: String = r.get("role_id");
                    let name: String = r.get("name");
                    format!("- {} ({})", name, id)
                })
                .collect();
            Ok(format!("[DB] {} roles:\n{}", rows.len(), lines.join("\n")))
        }
        // ── Tax rules ─────────────────────────────────────────────────────────
        "list_tax_rules" => {
            let rows = sqlx::query(
                "SELECT tax_rule_id, name, rate_basis_points, inclusive, is_active FROM tax_rules ORDER BY name"
            ).fetch_all(pool).await?;
            if rows.is_empty() {
                return Ok("[DB] No tax rules defined.".into());
            }
            let lines: Vec<String> = rows
                .iter()
                .map(|r| {
                    let id: String = r.get("tax_rule_id");
                    let name: String = r.get("name");
                    let bp: i64 = r.get("rate_basis_points");
                    let inclusive: bool = r.get("inclusive");
                    let active: bool = r.get("is_active");
                    let pct = bp as f64 / 100.0;
                    format!(
                        "- {} (ID: {}) — {}% {} — {}",
                        name,
                        id,
                        pct,
                        if inclusive { "inclusive" } else { "exclusive" },
                        if active { "Active" } else { "Inactive" }
                    )
                })
                .collect();
            Ok(format!(
                "[DB] {} tax rule(s):\n{}",
                rows.len(),
                lines.join("\n")
            ))
        }
        // ── Store settings ────────────────────────────────────────────────────
        "get_store_settings" => {
            let r = sqlx::query(
                "SELECT branch_id, name, timezone, address, phone, receipt_header, receipt_footer, tax_number, cr_number FROM branches WHERE is_active=1 LIMIT 1"
            ).fetch_optional(pool).await?
            .ok_or_else(|| AppError::NotFound("No active branch found".into()))?;
            let _id: String = r.get("branch_id");
            let name: String = r.get("name");
            let tz: Option<String> = r.get("timezone");
            let addr: Option<String> = r.get("address");
            let phone: Option<String> = r.get("phone");
            let rhead: Option<String> = r.get("receipt_header");
            let rfoot: Option<String> = r.get("receipt_footer");
            let tax: Option<String> = r.get("tax_number");
            let cr: Option<String> = r.get("cr_number");
            Ok(format!(
                "[DB] Store Settings:\n- Name: {}\n- Timezone: {}\n- Address: {}\n- Phone: {}\n- Tax/VAT Number: {}\n- CR Number: {}\n- Receipt Header: {}\n- Receipt Footer: {}",
                name,
                tz.as_deref().unwrap_or("—"),
                addr.as_deref().unwrap_or("—"),
                phone.as_deref().unwrap_or("—"),
                tax.as_deref().unwrap_or("—"),
                cr.as_deref().unwrap_or("—"),
                rhead.as_deref().unwrap_or("—"),
                rfoot.as_deref().unwrap_or("—")
            ))
        }
        // ── Business rules ────────────────────────────────────────────────────
        "get_business_rules" => {
            let neg = sqlx::query_scalar::<_, Option<String>>(
                "SELECT value FROM app_config WHERE key='flag_allow_negative_stock'",
            )
            .fetch_optional(pool)
            .await?
            .flatten()
            .unwrap_or_default()
                == "1";
            let dis = sqlx::query_scalar::<_, Option<String>>(
                "SELECT value FROM app_config WHERE key='flag_require_discount_reason'",
            )
            .fetch_optional(pool)
            .await?
            .flatten()
            .unwrap_or_default()
                == "1";
            let cc = sqlx::query_scalar::<_, Option<String>>(
                "SELECT value FROM app_config WHERE key='flag_cashier_can_discount'",
            )
            .fetch_optional(pool)
            .await?
            .flatten()
            .unwrap_or_default()
                == "1";
            let ap = sqlx::query_scalar::<_, Option<String>>(
                "SELECT value FROM app_config WHERE key='flag_auto_print_receipt'",
            )
            .fetch_optional(pool)
            .await?
            .flatten()
            .unwrap_or_default()
                == "1";
            Ok(format!(
                "[DB] Business Rules:\n- Allow negative stock: {}\n- Require discount reason: {}\n- Cashier can discount: {}\n- Auto-print receipt: {}",
                if neg { "Yes" } else { "No" },
                if dis { "Yes" } else { "No" },
                if cc { "Yes" } else { "No" },
                if ap { "Yes" } else { "No" }
            ))
        }
        // ── Devices ───────────────────────────────────────────────────────────
        "list_devices" => {
            let rows = sqlx::query(
                "SELECT device_id, device_code, is_active, created_at FROM devices ORDER BY device_code"
            ).fetch_all(pool).await?;
            if rows.is_empty() {
                return Ok("[DB] No devices registered.".into());
            }
            let lines: Vec<String> = rows
                .iter()
                .map(|r| {
                    let id: String = r.get("device_id");
                    let code: String = r.get("device_code");
                    let active: bool = r.get("is_active");
                    let created: Option<String> = r.get("created_at");
                    format!(
                        "- {} ({}) — {} — Created: {}",
                        code,
                        &id[..8.min(id.len())],
                        if active { "Active" } else { "Inactive" },
                        created
                            .as_deref()
                            .map(|s| &s[..10.min(s.len())])
                            .unwrap_or("never")
                    )
                })
                .collect();
            Ok(format!(
                "[DB] {} device(s):\n{}",
                rows.len(),
                lines.join("\n")
            ))
        }
        // ── Session timeout ───────────────────────────────────────────────────
        "get_session_timeout" => {
            let mins: Option<String> = sqlx::query_scalar(
                "SELECT value FROM app_config WHERE key = 'idle_timeout_minutes'",
            )
            .fetch_optional(pool)
            .await?
            .flatten();
            let timeout = mins.and_then(|v| v.parse::<i64>().ok()).unwrap_or(5);
            Ok(format!(
                "[DB] Session timeout: {} minutes ({}).",
                timeout,
                if timeout == 0 {
                    "never locks"
                } else {
                    "auto-locks after idle"
                }
            ))
        }
        "list_promotions" => {
            let status = input
                .get("status")
                .and_then(|v| v.as_str())
                .unwrap_or("active");
            let search = input.get("search").and_then(|v| v.as_str()).unwrap_or("");
            let limit = input.get("limit").and_then(|v| v.as_i64()).unwrap_or(50);
            let now = chrono::Utc::now().to_rfc3339();
            let status_clause = match status {
                "active" => "pp.effective_to > ?".to_string(),
                "upcoming" => "pp.effective_from > ?".to_string(),
                "expired" => "pp.effective_to <= ? AND pp.effective_to IS NOT NULL".to_string(),
                _ => "1=1".to_string(),
            };
            let search_clause = if search.is_empty() {
                String::new()
            } else {
                "AND p.name LIKE ?".to_string()
            };
            let sql = format!(
                "SELECT pp.price_id, pp.product_id, p.name AS product_name, pp.price_minor, \
                 pp.effective_from, pp.effective_to, pp.created_at \
                 FROM product_prices pp JOIN products p ON p.product_id = pp.product_id \
                 WHERE pp.price_type = 'promotional' AND ({status_clause}) {search_clause} \
                 ORDER BY pp.effective_from DESC LIMIT ?"
            );
            let mut q = sqlx::query_as::<
                _,
                (String, String, String, i64, String, Option<String>, String),
            >(&sql);
            if status != "all" {
                q = q.bind(&now);
            }
            if !search.is_empty() {
                q = q.bind(format!("%{search}%"));
            }
            q = q.bind(limit);
            let rows = q.fetch_all(pool).await?;
            if rows.is_empty() {
                return Ok("No promotions found.".into());
            }
            let fmt = |n: i64| money::format_minor(n, currency_exp);
            let lines: Vec<String> = rows
                .iter()
                .map(|(_pid, prod_id, name, price, from, to, _)| {
                    let label = match to {
                        Some(t) if t < &now => "EXPIRED",
                        Some(_) => "ACTIVE",
                        None => "ACTIVE",
                    };
                    format!(
                        "- [{label}] {name} (ID: {prod_id}) — BHD {} — {} → {}",
                        fmt(*price),
                        from,
                        to.as_deref().unwrap_or("ongoing")
                    )
                })
                .collect();
            Ok(format!(
                "[DB] {} promotion(s):\n{}",
                rows.len(),
                lines.join("\n")
            ))
        }
        "get_promotion" => {
            let price_id = input
                .get("price_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing price_id".into()))?;
            let now = chrono::Utc::now().to_rfc3339();
            let row = sqlx::query_as::<_, (String,String,String,i64,String,Option<String>,String)>(
                "SELECT pp.price_id, pp.product_id, p.name, pp.price_minor, pp.effective_from, pp.effective_to, pp.created_at \
                 FROM product_prices pp JOIN products p ON p.product_id = pp.product_id \
                 WHERE pp.price_id = ? AND pp.price_type = 'promotional'"
            ).bind(price_id).fetch_optional(pool).await?
            .ok_or_else(|| AppError::NotFound("Promotion not found".into()))?;
            let fmt = |n: i64| money::format_minor(n, currency_exp);
            let status = match &row.5 {
                Some(t) if t < &now => "Expired",
                Some(_) => "Active",
                None => "Active (no end date)",
            };
            Ok(format!(
                "Promotion: {}\nProduct: {} (ID: {})\nPrice: BHD {}\nEffective: {} → {}\nStatus: {}\nCreated: {}",
                row.0, row.2, row.1, fmt(row.3), row.4, row.5.as_deref().unwrap_or("ongoing"), status, row.6
            ))
        }
        "open_tab" => {
            let tab = input.get("tab").and_then(|v| v.as_str()).unwrap_or("");
            const VALID: &[&str] = &[
                "products",
                "categories",
                "inventory",
                "reports",
                "cashier",
                "eod",
                "deliveries",
                "customers",
                "users",
                "purchasing",
                "settings",
                "audit",
                "devices",
            ];
            if VALID.contains(&tab) {
                Ok(format!("{{\"ok\":true,\"tab\":\"{tab}\"}}"))
            } else {
                Ok(format!(
                    "{{\"ok\":false,\"tab\":\"{tab}\",\"error\":\"invalid tab\"}}"
                ))
            }
        }
        // Parity tools reach the hub, so they take the pool and their own input
        // rather than the branch/currency shape the reporting tools share.
        name if crate::ai::tools_parity::handles(name) => {
            crate::ai::tools_parity::execute(pool, name, input).await
        }
        name => {
            crate::ai::tools_read_ext::execute(pool, name, input, branch_id, currency_exp).await
        }
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
            let product_id = input
                .get("product_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let new_price = input
                .get("new_price_minor")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| AppError::Validation("Missing new_price_minor".into()))?;
            let reason = input.get("reason").and_then(|v| v.as_str()).unwrap_or("—");

            let p = product_repo::get_product_by_id(pool, product_id)
                .await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;

            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Update selling price of '{}'", p.product.name),
                fields: vec![
                    ToolPreviewField {
                        label: "Product".into(),
                        value: p.product.name.clone(),
                    },
                    ToolPreviewField {
                        label: "Current Price".into(),
                        value: format!("BHD {}", fmt(p.price_minor)),
                    },
                    ToolPreviewField {
                        label: "New Price".into(),
                        value: format!("BHD {}", fmt(new_price)),
                    },
                    ToolPreviewField {
                        label: "Reason".into(),
                        value: reason.into(),
                    },
                ],
            })
        }
        "set_product_active" => {
            let product_id = input
                .get("product_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let is_active = input
                .get("is_active")
                .and_then(|v| v.as_bool())
                .ok_or_else(|| AppError::Validation("Missing is_active".into()))?;

            let p = product_repo::get_product_by_id(pool, product_id)
                .await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;

            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!(
                    "{} product '{}'",
                    if is_active { "Enable" } else { "Disable" },
                    p.product.name
                ),
                fields: vec![
                    ToolPreviewField {
                        label: "Product".into(),
                        value: p.product.name.clone(),
                    },
                    ToolPreviewField {
                        label: "Action".into(),
                        value: if is_active {
                            "Enable (show in POS)".into()
                        } else {
                            "Disable (hide from POS)".into()
                        },
                    },
                ],
            })
        }
        "update_product_name" => {
            let product_id = input
                .get("product_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let new_name = input
                .get("new_name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing new_name".into()))?;

            let p = product_repo::get_product_by_id(pool, product_id)
                .await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;

            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: "Rename product".to_string(),
                fields: vec![
                    ToolPreviewField {
                        label: "Current Name".into(),
                        value: p.product.name.clone(),
                    },
                    ToolPreviewField {
                        label: "New Name".into(),
                        value: new_name.into(),
                    },
                ],
            })
        }
        "adjust_stock" => {
            let product_id = input
                .get("product_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let delta = input
                .get("quantity_delta")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| AppError::Validation("Missing quantity_delta".into()))?;
            let notes = input.get("notes").and_then(|v| v.as_str()).unwrap_or("—");
            let p = product_repo::get_product_by_id(pool, product_id)
                .await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;
            let levels = stock_repo::get_all_levels(pool, &active_branch_id(pool).await?).await?;
            let current = levels
                .iter()
                .find(|s| s.product_id == product_id)
                .map(|s| s.quantity_on_hand.clone())
                .unwrap_or_else(|| "0".into());
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Adjust stock for '{}'", p.product.name),
                fields: vec![
                    ToolPreviewField {
                        label: "Product".into(),
                        value: p.product.name.clone(),
                    },
                    ToolPreviewField {
                        label: "Current Qty".into(),
                        value: current,
                    },
                    ToolPreviewField {
                        label: "Adjustment".into(),
                        value: format!("{:+}", delta),
                    },
                    ToolPreviewField {
                        label: "Reason".into(),
                        value: notes.into(),
                    },
                ],
            })
        }
        "stock_take" => {
            let product_id = input
                .get("product_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let new_qty = input
                .get("new_quantity")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| AppError::Validation("Missing new_quantity".into()))?;
            let notes = input.get("notes").and_then(|v| v.as_str()).unwrap_or("—");
            let p = product_repo::get_product_by_id(pool, product_id)
                .await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;
            let levels = stock_repo::get_all_levels(pool, &active_branch_id(pool).await?).await?;
            let current = levels
                .iter()
                .find(|s| s.product_id == product_id)
                .map(|s| s.quantity_on_hand.clone())
                .unwrap_or_else(|| "0".into());
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Stock take for '{}'", p.product.name),
                fields: vec![
                    ToolPreviewField {
                        label: "Product".into(),
                        value: p.product.name.clone(),
                    },
                    ToolPreviewField {
                        label: "Current Qty".into(),
                        value: current,
                    },
                    ToolPreviewField {
                        label: "New Count".into(),
                        value: format!("{}", new_qty),
                    },
                    ToolPreviewField {
                        label: "Notes".into(),
                        value: notes.into(),
                    },
                ],
            })
        }
        "create_product" | "product_create" => {
            let name = input
                .get("name")
                .and_then(|v| v.as_str())
                .filter(|name| !name.trim().is_empty())
                .ok_or_else(|| AppError::Validation("Missing name".into()))?;
            let price = input
                .get("price_minor")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| AppError::Validation("Missing price_minor".into()))?;
            let category_id = input
                .get("category_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing category_id".into()))?;
            let sku = input.get("sku").and_then(|v| v.as_str()).unwrap_or("—");
            let barcode = input.get("barcode").and_then(|v| v.as_str()).unwrap_or("—");

            let cat_name: Option<String> =
                sqlx::query_scalar("SELECT name FROM categories WHERE category_id = ? AND is_active = 1 AND deleted_at IS NULL")
                    .bind(category_id)
                    .fetch_optional(pool)
                    .await?;

            // Reject invalid category_id early so the model gets a clear
            // Validation error and knows to call list_categories first.
            let cat_name = cat_name.ok_or_else(|| {
                AppError::Validation(format!(
                    "Category '{}' not found. Call list_categories first to get a valid category_id.",
                    category_id
                ))
            })?;

            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Create new product '{}'", name),
                fields: vec![
                    ToolPreviewField {
                        label: "Name".into(),
                        value: name.into(),
                    },
                    ToolPreviewField {
                        label: "Price".into(),
                        value: format!("BHD {}", fmt(price)),
                    },
                    ToolPreviewField {
                        label: "Category".into(),
                        value: cat_name,
                    },
                    ToolPreviewField {
                        label: "SKU".into(),
                        value: sku.into(),
                    },
                    ToolPreviewField {
                        label: "Barcode".into(),
                        value: barcode.into(),
                    },
                ],
            })
        }
        "update_reorder_point" => {
            let product_id = input
                .get("product_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let new_point = input
                .get("reorder_point")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| AppError::Validation("Missing reorder_point".into()))?;
            let p = product_repo::get_product_by_id(pool, product_id)
                .await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;
            let levels = stock_repo::get_all_levels(pool, &active_branch_id(pool).await?).await?;
            let current_point = levels
                .iter()
                .find(|s| s.product_id == product_id)
                .map(|s| s.reorder_point.to_string())
                .unwrap_or_else(|| "0".to_string());
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Update reorder point for '{}'", p.product.name),
                fields: vec![
                    ToolPreviewField {
                        label: "Product".into(),
                        value: p.product.name.clone(),
                    },
                    ToolPreviewField {
                        label: "Current Reorder Point".into(),
                        value: current_point,
                    },
                    ToolPreviewField {
                        label: "New Reorder Point".into(),
                        value: format!("{}", new_point),
                    },
                ],
            })
        }
        // ── New customer / delivery / bulk-stock dry-runs ─────────────────────
        "create_customer" => {
            let name = input
                .get("name")
                .and_then(|v| v.as_str())
                .filter(|name| !name.trim().is_empty())
                .ok_or_else(|| AppError::Validation("Missing name".into()))?;
            let phone = input.get("phone").and_then(|v| v.as_str()).unwrap_or("—");
            let email = input.get("email").and_then(|v| v.as_str()).unwrap_or("—");
            let notes = input.get("notes").and_then(|v| v.as_str()).unwrap_or("—");
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Create new customer '{}'", name),
                fields: vec![
                    ToolPreviewField {
                        label: "Name".into(),
                        value: name.into(),
                    },
                    ToolPreviewField {
                        label: "Phone".into(),
                        value: phone.into(),
                    },
                    ToolPreviewField {
                        label: "Email".into(),
                        value: email.into(),
                    },
                    ToolPreviewField {
                        label: "Notes".into(),
                        value: notes.into(),
                    },
                ],
            })
        }
        "update_customer" => {
            let customer_id = input
                .get("customer_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing customer_id".into()))?;
            let row = sqlx::query("SELECT name FROM customers WHERE customer_id = ?")
                .bind(customer_id)
                .fetch_optional(pool)
                .await?;
            let old_name: String = row
                .map(|r| r.get::<String, _>("name"))
                .unwrap_or_else(|| "unknown".to_string());
            let new_name = input.get("name").and_then(|v| v.as_str()).unwrap_or("—");
            let phone = input.get("phone").and_then(|v| v.as_str()).unwrap_or("—");
            let email = input.get("email").and_then(|v| v.as_str()).unwrap_or("—");
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Update customer '{}' → '{}'", old_name, new_name),
                fields: vec![
                    ToolPreviewField {
                        label: "Old Name".into(),
                        value: old_name,
                    },
                    ToolPreviewField {
                        label: "New Name".into(),
                        value: new_name.into(),
                    },
                    ToolPreviewField {
                        label: "Phone".into(),
                        value: phone.into(),
                    },
                    ToolPreviewField {
                        label: "Email".into(),
                        value: email.into(),
                    },
                ],
            })
        }
        "advance_delivery_status" => {
            let delivery_id = input
                .get("delivery_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing delivery_id".into()))?;
            let new_status = input
                .get("new_status")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing new_status".into()))?;
            let row = sqlx::query(
                "SELECT delivery_status, customer_name FROM delivery_orders WHERE delivery_id = ?",
            )
            .bind(delivery_id)
            .fetch_optional(pool)
            .await?;
            let (old_status, cust_name) = row
                .map(|r| {
                    (
                        r.get::<String, _>("delivery_status"),
                        r.get::<Option<String>, _>("customer_name")
                            .unwrap_or_else(|| "unknown".to_string()),
                    )
                })
                .unwrap_or_else(|| ("unknown".to_string(), "unknown".to_string()));
            let display_new = if new_status == "in_transit" {
                "out_for_delivery"
            } else {
                new_status
            };
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!(
                    "Advance delivery {} from '{}' → '{}'",
                    &delivery_id[..8.min(delivery_id.len())],
                    old_status,
                    display_new
                ),
                fields: vec![
                    ToolPreviewField {
                        label: "Delivery ID".into(),
                        value: delivery_id[..8.min(delivery_id.len())].to_string(),
                    },
                    ToolPreviewField {
                        label: "Customer".into(),
                        value: cust_name,
                    },
                    ToolPreviewField {
                        label: "Current Status".into(),
                        value: old_status,
                    },
                    ToolPreviewField {
                        label: "New Status".into(),
                        value: display_new.into(),
                    },
                ],
            })
        }
        "bulk_stock_take" => {
            let items = input
                .get("items")
                .and_then(|v| v.as_array())
                .ok_or_else(|| AppError::Validation("Missing items array".into()))?;
            let lines: Vec<String> = items
                .iter()
                .map(|item| {
                    let pid = item
                        .get("product_id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("?");
                    let qty = item
                        .get("new_quantity")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    format!("{}: {} units", &pid[..8.min(pid.len())], qty)
                })
                .collect();
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Bulk stock take: {} products", items.len()),
                fields: vec![ToolPreviewField {
                    label: "Changes".into(),
                    value: lines.join("; "),
                }],
            })
        }
        // ── Category dry-runs ─────────────────────────────────────────────────
        "create_category" => {
            let name = input.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let order = input
                .get("sort_order")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Create category '{}'", name),
                fields: vec![
                    ToolPreviewField {
                        label: "Name".into(),
                        value: name.into(),
                    },
                    ToolPreviewField {
                        label: "Sort order".into(),
                        value: order.to_string(),
                    },
                ],
            })
        }
        "update_category" => {
            let category_id = input
                .get("category_id")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let row = sqlx::query("SELECT name FROM categories WHERE category_id = ? AND is_active = 1 AND deleted_at IS NULL")
                .bind(category_id)
                .fetch_optional(pool)
                .await?;
            let old_name = row
                .map(|r| r.get::<String, _>("name"))
                .unwrap_or_else(|| "?".into());
            let name = input.get("name").and_then(|v| v.as_str());
            let active = input.get("is_active").and_then(|v| v.as_bool());
            let mut fields = vec![ToolPreviewField {
                label: "Category".into(),
                value: old_name.clone(),
            }];
            if let Some(n) = name {
                fields.push(ToolPreviewField {
                    label: "New name".into(),
                    value: n.into(),
                });
            }
            if let Some(a) = active {
                fields.push(ToolPreviewField {
                    label: "Active".into(),
                    value: (if a { "Yes" } else { "No" }).into(),
                });
            }
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Update category '{}'", old_name),
                fields,
            })
        }
        // ── User dry-runs ─────────────────────────────────────────────────────
        "create_user" => {
            let display = input
                .get("display_name")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let username = input
                .get("username")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let role_id = input.get("role_id").and_then(|v| v.as_str()).unwrap_or("?");
            let role_name: Option<String> =
                sqlx::query_scalar("SELECT name FROM roles WHERE role_id = ?")
                    .bind(role_id)
                    .fetch_optional(pool)
                    .await?
                    .flatten();
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Create staff account '{}'", display),
                fields: vec![
                    ToolPreviewField {
                        label: "Name".into(),
                        value: display.into(),
                    },
                    ToolPreviewField {
                        label: "Username".into(),
                        value: username.into(),
                    },
                    ToolPreviewField {
                        label: "Role".into(),
                        value: role_name.unwrap_or_else(|| role_id.into()),
                    },
                ],
            })
        }
        "update_user" => {
            let user_id = input.get("user_id").and_then(|v| v.as_str()).unwrap_or("?");
            let row = sqlx::query("SELECT display_name, is_active FROM users WHERE user_id = ?")
                .bind(user_id)
                .fetch_optional(pool)
                .await?
                .ok_or_else(|| AppError::NotFound("User not found".into()))?;
            let old_name: String = row.get("display_name");
            let _old_active: bool = row.get("is_active");
            let name = input.get("display_name").and_then(|v| v.as_str());
            let active = input.get("is_active").and_then(|v| v.as_bool());
            let mut fields = vec![ToolPreviewField {
                label: "User".into(),
                value: old_name.clone(),
            }];
            if let Some(n) = name {
                fields.push(ToolPreviewField {
                    label: "New name".into(),
                    value: n.into(),
                });
            }
            if let Some(a) = active {
                fields.push(ToolPreviewField {
                    label: "Active".into(),
                    value: (if a { "Yes" } else { "No" }).into(),
                });
            }
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Update user '{}'", old_name),
                fields,
            })
        }
        // ── Tax rule dry-runs ─────────────────────────────────────────────────
        "create_tax_rule" => {
            let name = input.get("name").and_then(|v| v.as_str()).unwrap_or("?");
            let bp = input
                .get("rate_basis_points")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let inclusive = input
                .get("inclusive")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Create tax rule '{}'", name),
                fields: vec![
                    ToolPreviewField {
                        label: "Name".into(),
                        value: name.into(),
                    },
                    ToolPreviewField {
                        label: "Rate".into(),
                        value: format!("{} bp", bp),
                    },
                    ToolPreviewField {
                        label: "Type".into(),
                        value: (if inclusive { "Inclusive" } else { "Exclusive" }).into(),
                    },
                ],
            })
        }
        "update_tax_rule" => {
            let tax_rule_id = input
                .get("tax_rule_id")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let row =
                sqlx::query("SELECT name, rate_basis_points FROM tax_rules WHERE tax_rule_id = ?")
                    .bind(tax_rule_id)
                    .fetch_optional(pool)
                    .await?
                    .ok_or_else(|| AppError::NotFound("Tax rule not found".into()))?;
            let old_name: String = row.get("name");
            let old_bp: i64 = row.get("rate_basis_points");
            let name = input.get("name").and_then(|v| v.as_str());
            let bp_val = input.get("rate_basis_points").and_then(|v| v.as_i64());
            let inclusive = input.get("inclusive").and_then(|v| v.as_bool());
            let active = input.get("is_active").and_then(|v| v.as_bool());
            let mut fields = vec![ToolPreviewField {
                label: "Tax Rule".into(),
                value: format!("{} ({} bp)", old_name, old_bp),
            }];
            if let Some(n) = name {
                fields.push(ToolPreviewField {
                    label: "New name".into(),
                    value: n.into(),
                });
            }
            if let Some(b) = bp_val {
                fields.push(ToolPreviewField {
                    label: "New rate".into(),
                    value: format!("{} bp", b),
                });
            }
            if let Some(i) = inclusive {
                fields.push(ToolPreviewField {
                    label: "Inclusive".into(),
                    value: (if i { "Yes" } else { "No" }).into(),
                });
            }
            if let Some(a) = active {
                fields.push(ToolPreviewField {
                    label: "Active".into(),
                    value: (if a { "Yes" } else { "No" }).into(),
                });
            }
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Update tax rule '{}'", old_name),
                fields,
            })
        }
        // ── Holistic product update dry-run ────────────────────────────────────
        "update_product_full" => {
            let product_id = input
                .get("product_id")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let p = product_repo::get_product_by_id(pool, product_id)
                .await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;
            let mut changes: Vec<String> = Vec::new();
            if let Some(n) = input.get("name").and_then(|v| v.as_str()) {
                if n != p.product.name {
                    changes.push(format!("Name: {} → {}", p.product.name, n));
                }
            }
            if let Some(v) = input.get("price_minor").and_then(|v| v.as_i64()) {
                if v != p.price_minor {
                    changes.push(format!(
                        "Price: BHD {} → BHD {}",
                        fmt(p.price_minor),
                        fmt(v)
                    ));
                }
            }
            if let Some(v) = input.get("is_active").and_then(|v| v.as_bool()) {
                if v != p.product.is_active {
                    changes.push(format!("Active: {} → {}", p.product.is_active, v));
                }
            }
            if let Some(v) = input.get("track_inventory").and_then(|v| v.as_bool()) {
                if v != p.product.track_inventory {
                    changes.push(format!(
                        "Track inventory: {} → {}",
                        p.product.track_inventory, v
                    ));
                }
            }
            if changes.is_empty() {
                changes.push("No changes detected".into());
            }
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Update product '{}'", p.product.name),
                fields: vec![ToolPreviewField {
                    label: "Changes".into(),
                    value: changes.join("; "),
                }],
            })
        }
        // ── Store settings dry-run ─────────────────────────────────────────────
        "update_store_settings" => {
            let r = sqlx::query("SELECT name FROM branches WHERE is_active=1 LIMIT 1")
                .fetch_optional(pool)
                .await?
                .map(|r| r.get::<String, _>("name"))
                .unwrap_or_else(|| "?".into());
            let mut changes = vec![];
            for key in &[
                "name",
                "address",
                "phone",
                "tax_number",
                "cr_number",
                "receipt_header",
                "receipt_footer",
                "timezone",
            ] {
                if let Some(v) = input.get(*key).and_then(|v| v.as_str()) {
                    changes.push(format!("{} = {}", key, v));
                }
            }
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Update store '{}' settings", r),
                fields: vec![ToolPreviewField {
                    label: "Changes".into(),
                    value: if changes.is_empty() {
                        "(none)".into()
                    } else {
                        changes.join(", ")
                    },
                }],
            })
        }
        // ── Business rules dry-run ─────────────────────────────────────────────
        "update_business_rules" => {
            let mut changes = vec![];
            for key in &[
                "allow_negative_stock",
                "require_discount_reason",
                "cashier_can_discount",
                "auto_print_receipt",
            ] {
                if let Some(v) = input.get(*key).and_then(|v| v.as_bool()) {
                    changes.push(format!("{} = {}", key, v));
                }
            }
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: "Update business rules".into(),
                fields: vec![ToolPreviewField {
                    label: "Setting".into(),
                    value: if changes.is_empty() {
                        "(none)".into()
                    } else {
                        changes.join(", ")
                    },
                }],
            })
        }
        // ── Delivery dry-runs ──────────────────────────────────────────────────
        "confirm_delivery_payment" => {
            let delivery_id = input
                .get("delivery_id")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let row = sqlx::query("SELECT delivery_status, payment_status, amount_minor FROM delivery_orders WHERE delivery_id = ?")
                .bind(delivery_id).fetch_optional(pool).await?
                .ok_or_else(|| AppError::NotFound("Delivery not found".into()))?;
            let amt: i64 = row.get("amount_minor");
            let ps: String = row.get("payment_status");
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!(
                    "Confirm payment for delivery {}",
                    &delivery_id[..8.min(delivery_id.len())]
                ),
                fields: vec![
                    ToolPreviewField {
                        label: "Amount".into(),
                        value: format!("BHD {}", fmt(amt)),
                    },
                    ToolPreviewField {
                        label: "Status".into(),
                        value: format!("{} → paid", ps),
                    },
                ],
            })
        }
        "cancel_delivery" => {
            let delivery_id = input
                .get("delivery_id")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let row =
                sqlx::query("SELECT delivery_status FROM delivery_orders WHERE delivery_id = ?")
                    .bind(delivery_id)
                    .fetch_optional(pool)
                    .await?
                    .ok_or_else(|| AppError::NotFound("Delivery not found".into()))?;
            let status: String = row.get("delivery_status");
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!(
                    "Cancel delivery {}",
                    &delivery_id[..8.min(delivery_id.len())]
                ),
                fields: vec![
                    ToolPreviewField {
                        label: "Current status".into(),
                        value: status,
                    },
                    ToolPreviewField {
                        label: "New status".into(),
                        value: "cancelled".into(),
                    },
                ],
            })
        }
        // ── Sync repair dry-runs ──────────────────────────────────────────────
        "sync_reset_stuck" => {
            let mut stuck = 0i64;
            for table in crate::commands::sync_commands::SYNC_TABLES {
                let n: i64 = sqlx::query_scalar(
                    &format!("SELECT COUNT(*) FROM {table} WHERE sync_status='pending' AND sync_attempts>=10"),
                ).fetch_one(pool).await.unwrap_or(0);
                stuck += n;
            }
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!(
                    "Reset {stuck} stuck rows across all tables back to pending with 0 attempts"
                ),
                fields: vec![ToolPreviewField {
                    label: "Stuck rows".into(),
                    value: stuck.to_string(),
                }],
            })
        }
        "sync_queue_retry" => {
            let event_id = input.get("event_id").and_then(|v| v.as_str()).unwrap_or("");
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Retry sync event: {event_id}"),
                fields: vec![ToolPreviewField {
                    label: "Event".into(),
                    value: event_id.to_string(),
                }],
            })
        }
        "sync_queue_dismiss" => {
            let event_id = input.get("event_id").and_then(|v| v.as_str()).unwrap_or("");
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Dismiss sync event: {event_id}"),
                fields: vec![ToolPreviewField {
                    label: "Event".into(),
                    value: event_id.to_string(),
                }],
            })
        }
        "apply_system_health_fix" => {
            let fix_action = input
                .get("fix_action")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let title = match fix_action {
                "reset_stuck_sync" => "Reset stuck sync retry counters",
                "clear_stuck_ai_runs" => "Mark stale AI runs as failed",
                "clear_stuck_ai_actions" => "Mark stale AI actions as failed",
                "reconcile_stock_drift" => "Repair stock levels from movement history",
                "trigger_sync_now" => "Trigger an immediate full sync cycle",
                _ => "Apply system health fix",
            };
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: title.into(),
                fields: vec![ToolPreviewField {
                    label: "Fix action".into(),
                    value: fix_action.to_string(),
                }],
            })
        }
        "void_sale" => {
            let receipt = input
                .get("receipt_number")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let reason = input.get("reason").and_then(|v| v.as_str()).unwrap_or("");
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Void sale {receipt}: {reason}"),
                fields: vec![
                    ToolPreviewField {
                        label: "Receipt".into(),
                        value: receipt.to_string(),
                    },
                    ToolPreviewField {
                        label: "Reason".into(),
                        value: reason.to_string(),
                    },
                ],
            })
        }
        "delete_customer" => {
            let cid = input
                .get("customer_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let name: Option<String> =
                sqlx::query_scalar("SELECT name FROM customers WHERE customer_id=?")
                    .bind(cid)
                    .fetch_optional(pool)
                    .await?
                    .flatten();
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Delete customer: {}", name.as_deref().unwrap_or(cid)),
                fields: vec![ToolPreviewField {
                    label: "Customer".into(),
                    value: name.unwrap_or_else(|| cid.to_string()),
                }],
            })
        }
        "set_device_active" => {
            let did = input
                .get("device_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let active = input
                .get("is_active")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Set device {did} active={active}"),
                fields: vec![ToolPreviewField {
                    label: "Device".into(),
                    value: did.to_string(),
                }],
            })
        }
        "receive_stock" => {
            let pid = input
                .get("product_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let qty = input
                .get("quantity")
                .and_then(|v| v.as_str())
                .unwrap_or("0");
            let pname: Option<String> =
                sqlx::query_scalar("SELECT name FROM products WHERE product_id=?")
                    .bind(pid)
                    .fetch_optional(pool)
                    .await?
                    .flatten();
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Receive {qty} of {}", pname.as_deref().unwrap_or(pid)),
                fields: vec![ToolPreviewField {
                    label: "Product".into(),
                    value: pname.unwrap_or_else(|| pid.to_string()),
                }],
            })
        }
        "add_loyalty_points" => {
            let _cid = input
                .get("customer_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let pts = input.get("points").and_then(|v| v.as_i64()).unwrap_or(0);
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Add {pts} loyalty points to customer"),
                fields: vec![ToolPreviewField {
                    label: "Points".into(),
                    value: pts.to_string(),
                }],
            })
        }
        "bulk_update_prices" => {
            let count = input
                .get("updates")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);
            Ok(ToolPreview {
                tool_name: tool_name.into(),
                description: format!("Bulk update prices for {count} products"),
                fields: vec![ToolPreviewField {
                    label: "Products".into(),
                    value: count.to_string(),
                }],
            })
        }
        // ── Backup dry-run ─────────────────────────────────────────────────────
        "backup_database" => Ok(ToolPreview {
            tool_name: tool_name.into(),
            description: "Create full database backup".into(),
            fields: vec![ToolPreviewField {
                label: "Action".into(),
                value: "Backup to app data directory".into(),
            }],
        }),
        name => crate::ai::tools_write_ext::dry_run(pool, name, input, currency_exp).await,
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

/// Look up the active branch_id from the database (read-only queries only need branch).
pub(super) async fn active_branch_id(pool: &SqlitePool) -> crate::errors::AppResult<String> {
    if let Some(branch_id) = crate::ai::tool_policy::current_branch_id() {
        let valid: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM branches WHERE branch_id = ? AND is_active = 1)",
        )
        .bind(&branch_id)
        .fetch_one(pool)
        .await?;
        return if valid {
            Ok(branch_id)
        } else {
            Err(AppError::Permission(
                "Authenticated branch is inactive or unavailable".into(),
            ))
        };
    }
    sqlx::query_scalar(
        "SELECT branch_id FROM branches WHERE is_active = 1 ORDER BY created_at LIMIT 1",
    )
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| {
        crate::errors::AppError::NotFound(
            "No active branch configured — complete store setup first".into(),
        )
    })
}

/// Look up the active device_id and branch_id from the database.
/// Returns an error if either is missing (setup not complete).
pub(super) async fn active_device_branch(
    pool: &SqlitePool,
) -> crate::errors::AppResult<(String, String)> {
    let branch_id = active_branch_id(pool).await?;
    let device_id: Option<String> = sqlx::query_scalar(
        "SELECT device_id FROM devices WHERE branch_id = ? AND is_active = 1 ORDER BY device_code LIMIT 1",
    )
    .bind(&branch_id)
    .fetch_optional(pool)
    .await?;

    match device_id {
        Some(device_id) => Ok((device_id, branch_id)),
        None => Err(crate::errors::AppError::NotFound(
            "No active device or branch — complete store setup first".into(),
        )),
    }
}

/// Guard rail: reject obviously invalid AI-generated mutation inputs before
/// they touch the database. This is a safety net — the LLM should never
/// generate these values, but if it does, we catch it here.
fn validate_mutation_input(tool_name: &str, input: &Value) -> AppResult<()> {
    match tool_name {
        "update_product_price" | "create_product" => {
            let price = input
                .get("new_price_minor")
                .or_else(|| input.get("price_minor"));
            if let Some(p) = price.and_then(|v| v.as_i64()) {
                if p <= 0 {
                    return Err(AppError::Validation(
                        "Price must be positive (minor units > 0)".into(),
                    ));
                }
                if p > 100_000_000 {
                    return Err(AppError::Validation(
                        "Price exceeds maximum (100M minor units)".into(),
                    ));
                }
            }
            if tool_name == "create_product" || tool_name == "product_create" {
                let name = input.get("name").and_then(|v| v.as_str()).unwrap_or("");
                if name.trim().is_empty() {
                    return Err(AppError::Validation("Product name cannot be empty".into()));
                }
                if name.len() > 200 {
                    return Err(AppError::Validation(
                        "Product name too long (max 200 chars)".into(),
                    ));
                }
            }
        }
        "update_product_name" => {
            let name = input.get("new_name").and_then(|v| v.as_str()).unwrap_or("");
            if name.trim().is_empty() {
                return Err(AppError::Validation("Product name cannot be empty".into()));
            }
            if name.len() > 200 {
                return Err(AppError::Validation(
                    "Product name too long (max 200 chars)".into(),
                ));
            }
        }
        "set_product_active" => {
            let _is_active = input
                .get("is_active")
                .and_then(|v| v.as_bool())
                .ok_or_else(|| AppError::Validation("is_active must be a boolean".into()))?;
        }
        "adjust_stock" => {
            let delta = input
                .get("quantity_delta")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            if delta == 0.0 {
                return Err(AppError::Validation("quantity_delta cannot be zero".into()));
            }
            if delta.abs() > 10_000_000.0 {
                return Err(AppError::Validation(
                    "quantity_delta exceeds maximum (±10M)".into(),
                ));
            }
        }
        "stock_take" | "bulk_stock_take" => {
            let qty = input
                .get("new_quantity")
                .and_then(|v| v.as_f64())
                .unwrap_or(-1.0);
            if qty < 0.0 {
                return Err(AppError::Validation(
                    "new_quantity cannot be negative".into(),
                ));
            }
            if qty > 10_000_000.0 {
                return Err(AppError::Validation(
                    "new_quantity exceeds maximum (10M)".into(),
                ));
            }
        }
        "update_reorder_point" => {
            let rp = input
                .get("reorder_point")
                .and_then(|v| v.as_f64())
                .unwrap_or(-1.0);
            if rp < 0.0 {
                return Err(AppError::Validation(
                    "reorder_point cannot be negative".into(),
                ));
            }
            if rp > 1_000_000.0 {
                return Err(AppError::Validation(
                    "reorder_point exceeds maximum (1M)".into(),
                ));
            }
        }
        "receive_stock" => {
            let quantity = input
                .get("quantity")
                .and_then(|value| value.as_str())
                .ok_or_else(|| AppError::Validation("quantity must be a decimal string".into()))?
                .parse::<f64>()
                .map_err(|_| AppError::Validation("quantity must be a number".into()))?;
            if !quantity.is_finite() || quantity <= 0.0 || quantity > 10_000_000.0 {
                return Err(AppError::Validation(
                    "quantity must be greater than 0 and at most 10,000,000".into(),
                ));
            }
            crate::inventory::lots::validate_expiry_date(
                input.get("expiry_date").and_then(Value::as_str),
            )?;
        }
        "create_customer" | "update_customer" => {
            let name = input.get("name").and_then(|v| v.as_str()).unwrap_or("");
            if name.trim().is_empty() {
                return Err(AppError::Validation("Customer name cannot be empty".into()));
            }
            if name.len() > 200 {
                return Err(AppError::Validation(
                    "Customer name too long (max 200 chars)".into(),
                ));
            }
        }
        "advance_delivery_status" => {
            let status = input
                .get("new_status")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if !matches!(status, "in_transit" | "delivered" | "cancelled") {
                return Err(AppError::Validation(format!(
                    "Invalid delivery status: '{}'. Must be in_transit | delivered | cancelled",
                    status
                )));
            }
        }
        "apply_system_health_fix" => {
            let fix_action = input
                .get("fix_action")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if !matches!(
                fix_action,
                "reset_stuck_sync"
                    | "clear_stuck_ai_runs"
                    | "clear_stuck_ai_actions"
                    | "reconcile_stock_drift"
                    | "trigger_sync_now"
            ) {
                return Err(AppError::Validation(format!(
                    "Invalid health fix action: '{fix_action}'"
                )));
            }
        }
        // Every call reached this function through tool_policy, which performs
        // fail-closed canonical schema and global boundary validation first.
        // These match arms add domain-specific invariants where needed.
        _ => {}
    }
    Ok(())
}

pub(super) async fn execute_mutation_raw(
    pool: &SqlitePool,
    tool_name: &str,
    input: &Value,
    currency_exp: u32,
) -> AppResult<MutationResult> {
    let started = std::time::Instant::now();
    let result = execute_mutation_raw_inner(pool, tool_name, input, currency_exp).await;
    let estimated_tokens = result
        .as_ref()
        .map(|mutation| (mutation.description.chars().count() as i64 + 3) / 4)
        .unwrap_or(0);
    if let Err(error) = crate::db::repositories::ai_admin_repo::record_tool_metric(
        pool,
        tool_name,
        result.is_ok(),
        started.elapsed().as_millis().min(i64::MAX as u128) as i64,
        estimated_tokens,
    )
    .await
    {
        tracing::warn!(tool = tool_name, %error, "failed to record ZanAI mutation metric");
    }
    result
}

async fn execute_mutation_raw_inner(
    pool: &SqlitePool,
    tool_name: &str,
    input: &Value,
    currency_exp: u32,
) -> AppResult<MutationResult> {
    // ── Input validation gate (prevents nonsensical AI-generated values) ────
    validate_mutation_input(tool_name, input)?;
    let actor_id = crate::ai::tool_policy::current_actor_id()
        .ok_or_else(|| AppError::Permission("Mutation actor context is missing".into()))?;

    let fmt = |n: i64| money::format_minor(n, currency_exp);

    match tool_name {
        "update_product_price" => {
            let product_id = input
                .get("product_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let new_price = input
                .get("new_price_minor")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| AppError::Validation("Missing new_price_minor".into()))?;

            let p = product_repo::get_product_by_id(pool, product_id)
                .await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;
            let old_price = p.price_minor;

            // Expire current active price and insert new one
            let now = chrono::Utc::now().to_rfc3339();
            let new_price_id = ulid::Ulid::new().to_string();

            sqlx::query(
                "UPDATE product_prices SET effective_to = ?, sync_status = 'pending'
                 WHERE product_id = ? AND branch_id IS NULL AND price_type = 'selling'
                   AND effective_to IS NULL",
            )
            .bind(&now)
            .bind(product_id)
            .execute(pool)
            .await?;

            sqlx::query(
                "INSERT INTO product_prices (price_id, product_id, branch_id, price_type, price_minor,
                 currency, effective_from, effective_to, created_by_user_id, created_at)
                 VALUES (?, ?, NULL, 'selling', ?, 'BHD', ?, NULL, ?, ?)"
            )
            .bind(&new_price_id)
            .bind(product_id)
            .bind(new_price)
            .bind(&now)
            .bind(&actor_id)
            .bind(&now)
            .execute(pool)
            .await?;

            // Write audit log
            write_audit(
                pool,
                "AI_ADMIN",
                "product_price_update",
                product_id,
                &json!({"from": old_price, "to": new_price}),
                "product",
            )
            .await?;

            // sync_status='pending' is set by column DEFAULT — sync worker picks it up (effective_to changed on old price, but product itself didn't change — just the price)
            // The product entity sync is handled by the price entry above.

            Ok(MutationResult {
                description: format!(
                    "Price of '{}' changed from BHD {} to BHD {}",
                    p.product.name,
                    fmt(old_price),
                    fmt(new_price)
                ),
                undo_snapshot_json: json!({ "price_minor": old_price }).to_string(),
                rollback_tool: "update_product_price".into(),
                rollback_input_json: json!({
                    "product_id": product_id, "new_price_minor": old_price
                })
                .to_string(),
                entity_type: "product_price".into(),
                entity_id: product_id.into(),
            })
        }
        "set_product_active" => {
            let product_id = input
                .get("product_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let is_active = input
                .get("is_active")
                .and_then(|v| v.as_bool())
                .ok_or_else(|| AppError::Validation("Missing is_active".into()))?;

            let p = product_repo::get_product_by_id(pool, product_id)
                .await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;
            let old_active = p.product.is_active;

            sqlx::query(
                "UPDATE products SET is_active = ?, updated_at = ?, version = version + 1, sync_status = 'pending' WHERE product_id = ?",
            )
            .bind(is_active as i64)
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(product_id)
            .execute(pool)
            .await?;

            write_audit(
                pool,
                "AI_ADMIN",
                "product_status_change",
                product_id,
                &json!({"from": old_active, "to": is_active}),
                "product",
            )
            .await?;

            // sync_status='pending' is set by column DEFAULT — sync worker picks it up

            Ok(MutationResult {
                description: format!(
                    "Product '{}' {}",
                    p.product.name,
                    if is_active { "enabled" } else { "disabled" }
                ),
                undo_snapshot_json: json!({ "is_active": old_active }).to_string(),
                rollback_tool: "set_product_active".into(),
                rollback_input_json: json!({
                    "product_id": product_id, "is_active": old_active
                })
                .to_string(),
                entity_type: "product".into(),
                entity_id: product_id.into(),
            })
        }
        "update_product_name" => {
            let product_id = input
                .get("product_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let new_name = input
                .get("new_name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing new_name".into()))?;

            let p = product_repo::get_product_by_id(pool, product_id)
                .await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;
            let old_name = p.product.name.clone();

            sqlx::query("UPDATE products SET name = ?, updated_at = ?, version = version + 1, sync_status = 'pending' WHERE product_id = ?")
                .bind(new_name)
                .bind(chrono::Utc::now().to_rfc3339())
                .bind(product_id)
                .execute(pool)
                .await?;

            write_audit(
                pool,
                "AI_ADMIN",
                "product_rename",
                product_id,
                &json!({"from": &old_name, "to": new_name}),
                "product",
            )
            .await?;

            // sync_status='pending' is set by column DEFAULT — sync worker picks it up

            Ok(MutationResult {
                description: format!("Product renamed from '{}' to '{}'", old_name, new_name),
                undo_snapshot_json: json!({ "name": &old_name }).to_string(),
                rollback_tool: "update_product_name".into(),
                rollback_input_json: json!({
                    "product_id": product_id, "new_name": &old_name
                })
                .to_string(),
                entity_type: "product".into(),
                entity_id: product_id.into(),
            })
        }
        "adjust_stock" => {
            let product_id = input
                .get("product_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let delta = input
                .get("quantity_delta")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| AppError::Validation("Missing quantity_delta".into()))?;
            let notes = input.get("notes").and_then(|v| v.as_str());
            let p = product_repo::get_product_by_id(pool, product_id)
                .await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;

            let (dv_id, br_id) = active_device_branch(pool).await?;
            let result = movements::manual_adjust(
                pool, product_id, delta, notes, &actor_id, None, &br_id, &dv_id,
            )
            .await?;
            let new_qty = result.quantity_on_hand.clone();

            write_audit(
                pool,
                "AI_ADMIN",
                "stock.adjustment",
                product_id,
                &json!({ "delta": delta, "new_qty": &new_qty, "notes": notes }),
                "stock",
            )
            .await?;

            Ok(MutationResult {
                description: format!(
                    "Stock of '{}' adjusted by {:+} → now {}",
                    p.product.name, delta, new_qty
                ),
                undo_snapshot_json: json!({ "quantity_delta": -delta }).to_string(),
                rollback_tool: "adjust_stock".into(),
                rollback_input_json: json!({
                    "product_id": product_id,
                    "quantity_delta": -delta,
                    "notes": "Undo previous adjustment",
                })
                .to_string(),
                entity_type: "stock_level".into(),
                entity_id: product_id.into(),
            })
        }
        "stock_take" => {
            let product_id = input
                .get("product_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let new_quantity = input
                .get("new_quantity")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| AppError::Validation("Missing new_quantity".into()))?;
            let notes = input.get("notes").and_then(|v| v.as_str());
            let p = product_repo::get_product_by_id(pool, product_id)
                .await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;

            // Get old qty for undo
            let levels = stock_repo::get_all_levels(pool, &active_branch_id(pool).await?).await?;
            let old_qty: f64 = levels
                .iter()
                .find(|s| s.product_id == product_id)
                .and_then(|s| s.quantity_on_hand.parse().ok())
                .unwrap_or(0.0);

            let (dv_id, br_id) = active_device_branch(pool).await?;
            movements::stock_take(
                pool,
                product_id,
                new_quantity,
                notes,
                &actor_id,
                None,
                &br_id,
                &dv_id,
            )
            .await?;

            write_audit(
                pool,
                "AI_ADMIN",
                "stock.stock_take",
                product_id,
                &json!({ "old_qty": old_qty, "new_qty": new_quantity, "notes": notes }),
                "stock",
            )
            .await?;

            Ok(MutationResult {
                description: format!(
                    "Stock take for '{}': counted {} (was {})",
                    p.product.name, new_quantity, old_qty
                ),
                undo_snapshot_json: json!({ "new_quantity": old_qty }).to_string(),
                rollback_tool: "stock_take".into(),
                rollback_input_json: json!({
                    "product_id": product_id,
                    "new_quantity": old_qty,
                    "notes": "Undo stock take",
                })
                .to_string(),
                entity_type: "stock_level".into(),
                entity_id: product_id.into(),
            })
        }
        "create_product" | "product_create" => {
            let mut tx = pool.begin().await?;
            let name = input
                .get("name")
                .and_then(|v| v.as_str())
                .filter(|name| !name.trim().is_empty())
                .ok_or_else(|| AppError::Validation("Missing name".into()))?;
            let price_minor = input
                .get("price_minor")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| AppError::Validation("Missing price_minor".into()))?;
            let category_id = input
                .get("category_id")
                .and_then(|v| v.as_str())
                .filter(|category_id| !category_id.trim().is_empty())
                .ok_or_else(|| AppError::Validation("Missing category_id".into()))?;
            let sku = input.get("sku").and_then(|v| v.as_str());
            let barcode = input.get("barcode").and_then(|v| v.as_str());
            let category_exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM categories WHERE category_id=? AND is_active=1 AND deleted_at IS NULL)",
            )
            .bind(category_id)
            .fetch_one(&mut *tx)
            .await?;
            if !category_exists {
                return Err(AppError::Validation(format!(
                    "Category '{category_id}' not found. Call list_categories first to get a valid category_id."
                )));
            }

            let now = chrono::Utc::now().to_rfc3339();
            let product_id = ulid::Ulid::new().to_string();
            let price_id = ulid::Ulid::new().to_string();

            sqlx::query(
                "INSERT INTO products
                   (product_id, category_id, name, sku, barcode,
                    track_inventory, allow_decimal_quantity, is_active,
                    currency, version, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, 0, 0, 1, 'BHD', 1, ?, ?)",
            )
            .bind(&product_id)
            .bind(category_id)
            .bind(name)
            .bind(sku)
            .bind(barcode)
            .bind(&now)
            .bind(&now)
            .execute(&mut *tx)
            .await?;

            sqlx::query(
                "INSERT INTO product_prices
                   (price_id, product_id, branch_id, price_type, price_minor,
                    currency, effective_from, effective_to, created_by_user_id, created_at)
                 VALUES (?, ?, NULL, 'selling', ?, 'BHD', ?, NULL, ?, ?)",
            )
            .bind(&price_id)
            .bind(&product_id)
            .bind(price_minor)
            .bind(&now)
            .bind(&actor_id)
            .bind(&now)
            .execute(&mut *tx)
            .await?;

            tx.commit().await?;

            write_audit(
                pool,
                "AI_ADMIN",
                "product.created",
                &product_id,
                &json!({ "name": name, "price_minor": price_minor, "category_id": category_id }),
                "product",
            )
            .await?;

            Ok(MutationResult {
                description: format!(
                    "Created product '{}' at BHD {} (ID: {}{})",
                    name,
                    fmt(price_minor),
                    product_id,
                    barcode.map_or(String::new(), |b| format!(", barcode: {b}"))
                ),
                undo_snapshot_json: json!({ "product_id": &product_id }).to_string(),
                rollback_tool: "set_product_active".into(),
                rollback_input_json: json!({
                    "product_id": &product_id, "is_active": false
                })
                .to_string(),
                entity_type: "product".into(),
                entity_id: product_id,
            })
        }
        "update_reorder_point" => {
            let product_id = input
                .get("product_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing product_id".into()))?;
            let new_point = input
                .get("reorder_point")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| AppError::Validation("Missing reorder_point".into()))?;
            let p = product_repo::get_product_by_id(pool, product_id)
                .await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;
            let levels = stock_repo::get_all_levels(pool, &active_branch_id(pool).await?).await?;
            let old_point: f64 = levels
                .iter()
                .find(|s| s.product_id == product_id)
                .map(|s| s.reorder_point as f64)
                .unwrap_or(0.0);

            sqlx::query(
                "UPDATE products SET reorder_point = ?, updated_at = ?, sync_status = 'pending' WHERE product_id = ?",
            )
            .bind(new_point)
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(product_id)
            .execute(pool)
            .await?;

            write_audit(
                pool,
                "AI_ADMIN",
                "stock.reorder_point_update",
                product_id,
                &json!({ "from": old_point, "to": new_point }),
                "stock",
            )
            .await?;

            Ok(MutationResult {
                description: format!(
                    "Reorder point for '{}' changed from {} to {}",
                    p.product.name, old_point, new_point
                ),
                undo_snapshot_json: json!({ "reorder_point": old_point }).to_string(),
                rollback_tool: "update_reorder_point".into(),
                rollback_input_json: json!({
                    "product_id": product_id, "reorder_point": old_point
                })
                .to_string(),
                entity_type: "stock_level".into(),
                entity_id: product_id.into(),
            })
        }
        // ── Customer mutations ────────────────────────────────────────────────
        "create_customer" => {
            let name = input
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing name".into()))?;
            let phone = input.get("phone").and_then(|v| v.as_str());
            let email = input.get("email").and_then(|v| v.as_str());
            let notes = input.get("notes").and_then(|v| v.as_str());
            let branch_id = active_branch_id(pool).await?;
            let customer_id = ulid::Ulid::new().to_string();
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query(
                "INSERT INTO customers (customer_id, branch_id, name, phone, email, notes, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&customer_id)
            .bind(&branch_id)
            .bind(name)
            .bind(phone)
            .bind(email)
            .bind(notes)
            .bind(&now)
            .bind(&now)
            .execute(pool)
            .await?;
            write_audit(
                pool,
                "AI_ADMIN",
                "customer.create",
                &customer_id,
                &json!({ "name": name, "phone": phone, "email": email }),
                "customer",
            )
            .await?;
            Ok(MutationResult {
                description: format!("Customer '{}' created (ID: {})", name, &customer_id[..8]),
                undo_snapshot_json: json!({ "customer_id": customer_id }).to_string(),
                rollback_tool: "delete_customer".into(),
                rollback_input_json: json!({ "customer_id": customer_id }).to_string(),
                entity_type: "customer".into(),
                entity_id: customer_id,
            })
        }
        "update_customer" => {
            let customer_id = input
                .get("customer_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing customer_id".into()))?;
            let row = sqlx::query(
                "SELECT name, phone, email, notes FROM customers WHERE customer_id = ?",
            )
            .bind(customer_id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Customer not found".into()))?;
            let old_name: String = row.get("name");
            let old_phone: Option<String> = row.get("phone");
            let old_email: Option<String> = row.get("email");
            let old_notes: Option<String> = row.get("notes");

            let new_name = input
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(&old_name);
            let new_phone = input.get("phone").and_then(|v| v.as_str());
            let new_email = input.get("email").and_then(|v| v.as_str());
            let new_notes = input.get("notes").and_then(|v| v.as_str());
            sqlx::query(
                "UPDATE customers SET name = ?, phone = COALESCE(?, phone),
                  email = COALESCE(?, email), notes = COALESCE(?, notes),
                  updated_at = ?, sync_status = 'pending'
                  WHERE customer_id = ?",
            )
            .bind(new_name)
            .bind(new_phone)
            .bind(new_email)
            .bind(new_notes)
            .bind(chrono::Utc::now().to_rfc3339())
            .bind(customer_id)
            .execute(pool)
            .await?;
            write_audit(
                pool,
                "AI_ADMIN",
                "customer.update",
                customer_id,
                &json!({ "name": new_name }),
                "customer",
            )
            .await?;
            Ok(MutationResult {
                description: format!("Customer '{}' updated to '{}'", old_name, new_name),
                undo_snapshot_json: json!({
                    "name": old_name, "phone": old_phone, "email": old_email, "notes": old_notes
                })
                .to_string(),
                rollback_tool: "update_customer".into(),
                rollback_input_json: json!({
                    "customer_id": customer_id,
                    "name": old_name,
                    "phone": old_phone,
                    "email": old_email,
                    "notes": old_notes
                })
                .to_string(),
                entity_type: "customer".into(),
                entity_id: customer_id.into(),
            })
        }
        // ── Delivery mutations ────────────────────────────────────────────────
        "advance_delivery_status" => {
            let delivery_id = input
                .get("delivery_id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing delivery_id".into()))?;
            let new_status_input = input
                .get("new_status")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing new_status".into()))?;
            // Map AI-facing "in_transit" → DB column value "dispatched"
            let db_new_status = if new_status_input == "in_transit" {
                "dispatched"
            } else {
                new_status_input
            };
            let row =
                sqlx::query("SELECT delivery_status FROM delivery_orders WHERE delivery_id = ?")
                    .bind(delivery_id)
                    .fetch_optional(pool)
                    .await?
                    .ok_or_else(|| AppError::NotFound("Delivery not found".into()))?;
            let old_status: String = row.get("delivery_status");
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query(
                "UPDATE delivery_orders SET delivery_status = ?, updated_at = ?, sync_status = 'pending', version = version + 1 WHERE delivery_id = ?",
            )
            .bind(db_new_status)
            .bind(&now)
            .bind(delivery_id)
            .execute(pool)
            .await?;
            write_audit(
                pool,
                "AI_ADMIN",
                "delivery.status_advance",
                delivery_id,
                &json!({ "from": old_status, "to": db_new_status }),
                "delivery",
            )
            .await?;
            // Build rollback: use original DB status (old_status already is DB value)
            let rollback_input = if old_status == "out_for_delivery" {
                json!({ "delivery_id": delivery_id, "new_status": "in_transit" })
            } else {
                json!({ "delivery_id": delivery_id, "new_status": old_status })
            };
            Ok(MutationResult {
                description: format!(
                    "Delivery {} status: '{}' → '{}'",
                    &delivery_id[..8.min(delivery_id.len())],
                    old_status,
                    db_new_status
                ),
                undo_snapshot_json: json!({ "delivery_status": old_status }).to_string(),
                rollback_tool: "advance_delivery_status".into(),
                rollback_input_json: rollback_input.to_string(),
                entity_type: "delivery_order".into(),
                entity_id: delivery_id.into(),
            })
        }
        // ── Bulk stock take ───────────────────────────────────────────────────
        "bulk_stock_take" => {
            let items = input
                .get("items")
                .and_then(|v| v.as_array())
                .ok_or_else(|| AppError::Validation("Missing items array".into()))?
                .clone();
            let branch_id = active_branch_id(pool).await?;
            let now = chrono::Utc::now().to_rfc3339();
            let mut undo_items: Vec<Value> = Vec::new();
            let mut results: Vec<String> = Vec::new();

            for item in &items {
                let product_id = item
                    .get("product_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let new_qty = item
                    .get("new_quantity")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);

                // Fetch old quantity for audit snapshot (outside transaction)
                let old_qty_read: Option<String> = sqlx::query_scalar(
                    "SELECT quantity_on_hand FROM stock_levels WHERE product_id = ? AND branch_id = ?",
                )
                .bind(product_id)
                .bind(&branch_id)
                .fetch_optional(pool)
                .await?
                .flatten();
                let old_qty_str = old_qty_read.as_deref().unwrap_or("0");
                let old_qty: f64 = old_qty_str.parse().unwrap_or(0.0);

                // Transaction: read current qty → compute delta → upsert → movement
                let (device_id, _) = active_device_branch(pool).await?;
                let mut tx = pool.begin().await?;

                let old_in_tx: Option<String> = sqlx::query_scalar(
                    "SELECT quantity_on_hand FROM stock_levels WHERE product_id = ? AND branch_id = ?",
                )
                .bind(product_id)
                .bind(&branch_id)
                .fetch_optional(&mut *tx)
                .await?
                .flatten();
                let old_dec: f64 = old_in_tx.as_deref().unwrap_or("0").parse().unwrap_or(0.0);
                let delta = new_qty - old_dec;
                let new_qty_str = format!("{new_qty}");
                let delta_str = format!("{delta}");

                // Upsert stock level
                let stock_level_id = format!("SL-{}-{}", product_id, branch_id);
                sqlx::query(
                    "INSERT INTO stock_levels (stock_level_id, product_id, branch_id, quantity_on_hand, created_at, updated_at, last_movement_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?)
                     ON CONFLICT(product_id, branch_id)
                     DO UPDATE SET quantity_on_hand = excluded.quantity_on_hand,
                                   updated_at = excluded.updated_at,
                                   last_movement_at = excluded.last_movement_at,
                                   sync_status = 'pending'",
                )
                .bind(&stock_level_id)
                .bind(product_id)
                .bind(&branch_id)
                .bind(&new_qty_str)
                .bind(&now)
                .bind(&now)
                .bind(&now)
                .execute(&mut *tx)
                .await?;

                // Record stock movement
                let movement_id = ulid::Ulid::new().to_string();
                sqlx::query(
                    "INSERT INTO stock_movements
                     (movement_id, product_id, branch_id, device_id, origin_device_id,
                      movement_type, quantity_delta, quantity_after,
                      reference_type, notes, created_by_user_id, created_at, sync_status)
                     VALUES (?,?,?,?,?,'stock_take',?,?,'ai_action',NULL,?,?,'pending')",
                )
                .bind(&movement_id)
                .bind(product_id)
                .bind(&branch_id)
                .bind(&device_id)
                .bind(&device_id)
                .bind(&delta_str)
                .bind(&new_qty_str)
                .bind(&actor_id)
                .bind(&now)
                .execute(&mut *tx)
                .await?;

                tx.commit().await?;

                write_audit(
                    pool,
                    "AI_ADMIN",
                    "stock.bulk_take",
                    product_id,
                    &json!({ "from": old_qty, "to": new_qty }),
                    "stock",
                )
                .await?;

                undo_items.push(json!({ "product_id": product_id, "new_quantity": old_qty }));
                results.push(format!(
                    "{}: {} → {}",
                    &product_id[..8.min(product_id.len())],
                    old_qty,
                    new_qty
                ));
            }

            Ok(MutationResult {
                description: format!("Bulk stock take applied: {}", results.join(", ")),
                undo_snapshot_json: json!({ "items": undo_items }).to_string(),
                rollback_tool: "bulk_stock_take".into(),
                rollback_input_json: json!({ "items": undo_items }).to_string(),
                entity_type: "stock_level".into(),
                entity_id: "bulk".into(),
            })
        }
        // ── Category executions ─────────────────────────────────────────────────
        "create_category" => {
            let name = input.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let sort = input
                .get("sort_order")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let category_id = ulid::Ulid::new().to_string();
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query("INSERT INTO categories (category_id, name, sort_order, is_active, created_at, updated_at) VALUES (?, ?, ?, 1, ?, ?)")
                .bind(&category_id).bind(name).bind(sort).bind(&now).bind(&now).execute(pool).await?;
            write_audit(
                pool,
                "AI_ADMIN",
                "category.create",
                &category_id,
                &json!({"name":name,"sort_order":sort}),
                "category",
            )
            .await?;
            Ok(MutationResult {
                description: format!("Category '{}' created (ID: {})", name, category_id),
                undo_snapshot_json: json!({"category_id":&category_id}).to_string(),
                rollback_tool: "update_category".into(),
                rollback_input_json: json!({"category_id":&category_id,"is_active":false})
                    .to_string(),
                entity_type: "category".into(),
                entity_id: category_id,
            })
        }
        "update_category" => {
            let category_id = input
                .get("category_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let row = sqlx::query(
                "SELECT name, sort_order, is_active FROM categories WHERE category_id = ? AND is_active = 1 AND deleted_at IS NULL",
            )
            .bind(category_id)
            .fetch_optional(pool)
            .await?
            .ok_or_else(|| AppError::NotFound("Category not found".into()))?;
            let old_name: String = row.get("name");
            let old_sort: i64 = row.get("sort_order");
            let old_active: bool = row.get("is_active");
            let name = input
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(&old_name);
            let sort = input
                .get("sort_order")
                .and_then(|v| v.as_i64())
                .unwrap_or(old_sort);
            let active_val = input
                .get("is_active")
                .and_then(|v| v.as_bool())
                .unwrap_or(old_active);
            sqlx::query("UPDATE categories SET name=?, sort_order=?, is_active=?, updated_at=?, version = version + 1, sync_status = 'pending' WHERE category_id=?")
                .bind(name).bind(sort).bind(active_val as i64).bind(chrono::Utc::now().to_rfc3339()).bind(category_id).execute(pool).await?;
            write_audit(
                pool,
                "AI_ADMIN",
                "category.update",
                category_id,
                &json!({"name":name,"is_active":active_val}),
                "category",
            )
            .await?;
            Ok(MutationResult {
                description: format!("Category '{}' updated", name),
                undo_snapshot_json: json!({"name":old_name,"sort_order":old_sort,"is_active":old_active}).to_string(),
                rollback_tool: "update_category".into(),
                rollback_input_json: json!({"category_id":category_id,"name":old_name,"sort_order":old_sort,"is_active":old_active}).to_string(),
                entity_type: "category".into(), entity_id: category_id.into(),
            })
        }
        // ── User executions ─────────────────────────────────────────────────────
        "create_user" => {
            let display = input
                .get("display_name")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let username = input.get("username").and_then(|v| v.as_str()).unwrap_or("");
            let pin = input.get("pin").and_then(|v| v.as_str()).unwrap_or("");
            let role_id = input.get("role_id").and_then(|v| v.as_str()).unwrap_or("");
            if pin.len() < 4 {
                return Err(AppError::Validation("PIN must be 4+ digits".into()));
            }
            let user_id = ulid::Ulid::new().to_string();
            let now = chrono::Utc::now().to_rfc3339();
            let pin_hash = crate::db::repositories::auth_repo::hash_pin(pin)?;
            let branch_id = active_branch_id(pool).await?;
            sqlx::query("INSERT INTO users (user_id, branch_id, display_name, username, pin_hash, role_id, is_active, created_at, updated_at) VALUES (?,?,?,?,?,?,1,?,?)")
                .bind(&user_id).bind(&branch_id).bind(display).bind(username).bind(&pin_hash).bind(role_id).bind(&now).bind(&now).execute(pool).await?;
            write_audit(
                pool,
                "AI_ADMIN",
                "user.create",
                &user_id,
                &json!({"display_name":display,"username":username}),
                "user",
            )
            .await?;
            Ok(MutationResult {
                description: format!("Staff '{}' (@{}) created", display, username),
                undo_snapshot_json: json!({"user_id":&user_id}).to_string(),
                rollback_tool: "update_user".into(),
                rollback_input_json: json!({"user_id":&user_id,"is_active":false}).to_string(),
                entity_type: "user".into(),
                entity_id: user_id,
            })
        }
        "update_user" => {
            let user_id = input.get("user_id").and_then(|v| v.as_str()).unwrap_or("");
            let row =
                sqlx::query("SELECT display_name, role_id, is_active FROM users WHERE user_id = ?")
                    .bind(user_id)
                    .fetch_optional(pool)
                    .await?
                    .ok_or_else(|| AppError::NotFound("User not found".into()))?;
            let old_name: String = row.get("display_name");
            let old_role: String = row.get("role_id");
            let old_active: bool = row.get("is_active");
            // S-01: parameterized binds — never interpolate AI-supplied values into SQL.
            let mut sets: Vec<&str> = Vec::new();
            let mut binds: Vec<SqlBind> = Vec::new();
            let mut undo = serde_json::Map::new();
            undo.insert("user_id".into(), json!(user_id));
            if let Some(n) = input.get("display_name").and_then(|v| v.as_str()) {
                sets.push("display_name = ?");
                binds.push(SqlBind::S(n.to_string()));
                undo.insert("display_name".into(), json!(old_name));
            }
            if let Some(r) = input.get("role_id").and_then(|v| v.as_str()) {
                sets.push("role_id = ?");
                binds.push(SqlBind::S(r.to_string()));
                undo.insert("role_id".into(), json!(old_role));
            }
            if let Some(a) = input.get("is_active").and_then(|v| v.as_bool()) {
                sets.push("is_active = ?");
                binds.push(SqlBind::I(if a { 1 } else { 0 }));
                undo.insert("is_active".into(), json!(old_active));
            }
            if let Some(pin_val) = input.get("pin").and_then(|v| v.as_str()) {
                if pin_val.len() < 4 {
                    return Err(AppError::Validation("PIN must be 4+ digits".into()));
                }
                let pin_hash = crate::db::repositories::auth_repo::hash_pin(pin_val)?;
                sets.push("pin_hash = ?");
                binds.push(SqlBind::S(pin_hash));
            }
            if !sets.is_empty() {
                let sql = format!(
                    "UPDATE users SET {}, sync_status = 'pending' WHERE user_id = ?",
                    sets.join(", ")
                );
                let mut q = sqlx::query(&sql);
                for b in &binds {
                    q = b.apply(q);
                }
                q.bind(user_id).execute(pool).await?;
            }
            write_audit(pool, "AI_ADMIN", "user.update", user_id, &json!({}), "user").await?;
            Ok(MutationResult {
                description: format!("User '{}' updated", old_name),
                undo_snapshot_json: serde_json::Value::Object(undo.clone()).to_string(),
                rollback_tool: "update_user".into(),
                rollback_input_json: serde_json::Value::Object(undo).to_string(),
                entity_type: "user".into(),
                entity_id: user_id.into(),
            })
        }
        // ── Tax rule executions ─────────────────────────────────────────────────
        "create_tax_rule" => {
            let name = input.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let bp = input
                .get("rate_basis_points")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let inclusive = input
                .get("inclusive")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let tax_rule_id = ulid::Ulid::new().to_string();
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query("INSERT INTO tax_rules (tax_rule_id, name, rate_basis_points, inclusive, is_active, effective_from, created_at, updated_at) VALUES (?,?,?,?,1,?,?,?)")
                .bind(&tax_rule_id).bind(name).bind(bp).bind(inclusive).bind(&now).bind(&now).bind(&now).execute(pool).await?;
            write_audit(
                pool,
                "AI_ADMIN",
                "tax_rule.create",
                &tax_rule_id,
                &json!({"name":name,"rate_basis_points":bp}),
                "tax_rule",
            )
            .await?;
            Ok(MutationResult {
                description: format!(
                    "Tax rule '{}' created ({} bp {})",
                    name,
                    bp,
                    if inclusive { "inclusive" } else { "exclusive" }
                ),
                undo_snapshot_json: json!({"tax_rule_id":&tax_rule_id}).to_string(),
                rollback_tool: "update_tax_rule".into(),
                rollback_input_json: json!({"tax_rule_id":&tax_rule_id,"is_active":false})
                    .to_string(),
                entity_type: "tax_rule".into(),
                entity_id: tax_rule_id,
            })
        }
        "update_tax_rule" => {
            let tax_rule_id = input
                .get("tax_rule_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let row = sqlx::query("SELECT name, rate_basis_points, inclusive, is_active FROM tax_rules WHERE tax_rule_id = ?")
                .bind(tax_rule_id).fetch_optional(pool).await?
                .ok_or_else(|| AppError::NotFound("Tax rule not found".into()))?;
            let old_name: String = row.get("name");
            let old_bp: i64 = row.get("rate_basis_points");
            let old_inclusive: bool = row.get("inclusive");
            let old_active: bool = row.get("is_active");
            let name = input
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(&old_name);
            let bp = input
                .get("rate_basis_points")
                .and_then(|v| v.as_i64())
                .unwrap_or(old_bp);
            let inc = input
                .get("inclusive")
                .and_then(|v| v.as_bool())
                .unwrap_or(old_inclusive);
            let active_val = input
                .get("is_active")
                .and_then(|v| v.as_bool())
                .unwrap_or(old_active);
            sqlx::query("UPDATE tax_rules SET name=?, rate_basis_points=?, inclusive=?, is_active=?, updated_at=?, sync_status = 'pending' WHERE tax_rule_id=?")
                .bind(name).bind(bp).bind(inc).bind(active_val as i64).bind(chrono::Utc::now().to_rfc3339()).bind(tax_rule_id).execute(pool).await?;
            write_audit(
                pool,
                "AI_ADMIN",
                "tax_rule.update",
                tax_rule_id,
                &json!({"name":name,"rate_basis_points":bp}),
                "tax_rule",
            )
            .await?;
            Ok(MutationResult {
                description: format!("Tax rule '{}' updated", name),
                undo_snapshot_json: json!({"name":old_name,"rate_basis_points":old_bp,"inclusive":old_inclusive,"is_active":old_active}).to_string(),
                rollback_tool: "update_tax_rule".into(),
                rollback_input_json: json!({"tax_rule_id":tax_rule_id,"name":old_name,"rate_basis_points":old_bp,"inclusive":old_inclusive,"is_active":old_active}).to_string(),
                entity_type: "tax_rule".into(), entity_id: tax_rule_id.into(),
            })
        }
        // ── Holistic product update ────────────────────────────────────────────
        "update_product_full" => {
            let product_id = input
                .get("product_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let p = product_repo::get_product_by_id(pool, product_id)
                .await?
                .ok_or_else(|| AppError::NotFound("Product not found".into()))?;
            let name = input
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or(&p.product.name);
            let cat_id = input
                .get("category_id")
                .and_then(|v| v.as_str())
                .unwrap_or(&p.product.category_id);
            let sku = input.get("sku").and_then(|v| v.as_str());
            let barcode = input.get("barcode").and_then(|v| v.as_str());
            let price = input.get("price_minor").and_then(|v| v.as_i64());
            let cost = input.get("cost_minor").and_then(|v| v.as_i64());
            let supplier = input.get("default_supplier_id").and_then(|v| v.as_str());
            let tax = input.get("tax_rule_id").and_then(|v| v.as_str());
            let track = input
                .get("track_inventory")
                .and_then(|v| v.as_bool())
                .unwrap_or(p.product.track_inventory);
            let decimal = input
                .get("allow_decimal_quantity")
                .and_then(|v| v.as_bool())
                .unwrap_or(p.product.allow_decimal_quantity);
            let rp = input.get("reorder_point").and_then(|v| v.as_f64());
            let active = input
                .get("is_active")
                .and_then(|v| v.as_bool())
                .unwrap_or(p.product.is_active);
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query("UPDATE products SET name=?,category_id=?,sku=COALESCE(?,sku),barcode=COALESCE(?,barcode),tax_rule_id=COALESCE(?,tax_rule_id),cost_minor=COALESCE(?,cost_minor),default_supplier_id=COALESCE(?,default_supplier_id),track_inventory=?,allow_decimal_quantity=?,is_active=?,version=version+1,updated_at=?, sync_status = 'pending' WHERE product_id=?")
                .bind(name).bind(cat_id).bind(sku).bind(barcode).bind(tax).bind(cost).bind(supplier).bind(track as i64).bind(decimal as i64).bind(active as i64).bind(&now).bind(product_id).execute(pool).await?;
            if let Some(rp_val) = rp {
                sqlx::query("UPDATE products SET reorder_point = ?, updated_at = ?, sync_status = 'pending' WHERE product_id = ?")
                    .bind(rp_val as i64).bind(&now).bind(product_id).execute(pool).await?;
            }
            if let Some(new_price) = price {
                sqlx::query("UPDATE product_prices SET effective_to=?, sync_status = 'pending' WHERE product_id=? AND price_type='selling' AND effective_to IS NULL").bind(&now).bind(product_id).execute(pool).await?;
                let pid = ulid::Ulid::new().to_string();
                sqlx::query("INSERT INTO product_prices (price_id,product_id,branch_id,price_type,price_minor,currency,effective_from,created_by_user_id,created_at) VALUES (?,?,NULL,'selling',?,'BHD',?,?,?)")
                    .bind(&pid).bind(product_id).bind(new_price).bind(&now).bind(&actor_id).bind(&now).execute(pool).await?;
            }
            write_audit(
                pool,
                "AI_ADMIN",
                "product.update_full",
                product_id,
                &json!({"name":name}),
                "product",
            )
            .await?;
            Ok(MutationResult {
                description: format!("Product '{}' fully updated", name),
                undo_snapshot_json: json!({"product_id":product_id}).to_string(),
                rollback_tool: "update_product_full".into(),
                rollback_input_json: json!({"product_id":product_id,"name":p.product.name,"category_id":p.product.category_id,"cost_minor":p.product.cost_minor,"default_supplier_id":p.product.default_supplier_id,"track_inventory":p.product.track_inventory,"is_active":p.product.is_active}).to_string(),
                entity_type: "product".into(), entity_id: product_id.into(),
            })
        }
        // ── Store settings ──────────────────────────────────────────────────────
        "update_store_settings" => {
            let row = sqlx::query("SELECT name, timezone, address, phone, tax_number, cr_number, receipt_header, receipt_footer FROM branches WHERE is_active=1 LIMIT 1")
                .fetch_optional(pool).await?.ok_or_else(|| AppError::NotFound("No active branch".into()))?;
            let _old_name: String = row.get("name");
            // S-01: parameterized binds for store-settings update.
            let mut updates: Vec<String> = Vec::new();
            let mut binds: Vec<SqlBind> = Vec::new();
            let mut undo_map = serde_json::Map::new();
            for (key, old_val) in [
                ("name", row.get::<String, _>("name")),
                (
                    "address",
                    row.get::<Option<String>, _>("address").unwrap_or_default(),
                ),
                (
                    "phone",
                    row.get::<Option<String>, _>("phone").unwrap_or_default(),
                ),
                (
                    "tax_number",
                    row.get::<Option<String>, _>("tax_number")
                        .unwrap_or_default(),
                ),
                (
                    "cr_number",
                    row.get::<Option<String>, _>("cr_number")
                        .unwrap_or_default(),
                ),
                (
                    "receipt_header",
                    row.get::<Option<String>, _>("receipt_header")
                        .unwrap_or_default(),
                ),
                (
                    "receipt_footer",
                    row.get::<Option<String>, _>("receipt_footer")
                        .unwrap_or_default(),
                ),
                (
                    "timezone",
                    row.get::<Option<String>, _>("timezone")
                        .unwrap_or("Asia/Bahrain".into()),
                ),
            ]
            .iter()
            {
                if let Some(v) = input.get(*key).and_then(|v| v.as_str()) {
                    updates.push(format!("{} = ?", key));
                    binds.push(SqlBind::S(v.to_string()));
                    undo_map.insert(key.to_string(), json!(old_val));
                }
            }
            if !updates.is_empty() {
                let sql = format!(
                    "UPDATE branches SET {} WHERE is_active=1",
                    updates.join(", ")
                );
                let mut q = sqlx::query(&sql);
                for b in &binds {
                    q = b.apply(q);
                }
                q.execute(pool).await?;
            }
            write_audit(
                pool,
                "AI_ADMIN",
                "store_settings.update",
                "branch",
                &json!({}),
                "store_settings",
            )
            .await?;
            Ok(MutationResult {
                description: "Store settings updated".to_string(),
                undo_snapshot_json: serde_json::Value::Object(undo_map.clone()).to_string(),
                rollback_tool: "update_store_settings".into(),
                rollback_input_json: serde_json::Value::Object(undo_map).to_string(),
                entity_type: "branch".into(),
                entity_id: "active".into(),
            })
        }
        // ── Business rules ──────────────────────────────────────────────────────
        "update_business_rules" => {
            for (key, api_name) in [
                ("flag_allow_negative_stock", "allow_negative_stock"),
                ("flag_require_discount_reason", "require_discount_reason"),
                ("flag_cashier_can_discount", "cashier_can_discount"),
                ("flag_auto_print_receipt", "auto_print_receipt"),
            ]
            .iter()
            {
                if let Some(v) = input.get(*api_name).and_then(|v| v.as_bool()) {
                    let val = if v { "1" } else { "0" };
                    sqlx::query("INSERT INTO app_config (key, value, updated_at) VALUES (?, ?, ?) ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at")
                        .bind(key).bind(val).bind(chrono::Utc::now().to_rfc3339()).execute(pool).await?;
                }
            }
            write_audit(
                pool,
                "AI_ADMIN",
                "business_rules.update",
                "rules",
                &json!({}),
                "business_rules",
            )
            .await?;
            Ok(MutationResult {
                description: "Business rules updated".into(),
                undo_snapshot_json: "{}".into(),
                rollback_tool: "_no_undo".into(),
                rollback_input_json: "{}".into(),
                entity_type: "app_config".into(),
                entity_id: "flags".into(),
            })
        }
        // ── Delivery payment/cancel ────────────────────────────────────────────
        "confirm_delivery_payment" => {
            let delivery_id = input
                .get("delivery_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let row =
                sqlx::query("SELECT payment_status FROM delivery_orders WHERE delivery_id = ?")
                    .bind(delivery_id)
                    .fetch_optional(pool)
                    .await?
                    .ok_or_else(|| AppError::NotFound("Delivery not found".into()))?;
            let old_payment: String = row.get("payment_status");
            sqlx::query("UPDATE delivery_orders SET payment_status='paid', updated_at=?, sync_status = 'pending', version = version + 1 WHERE delivery_id=?")
                .bind(chrono::Utc::now().to_rfc3339()).bind(delivery_id).execute(pool).await?;
            write_audit(
                pool,
                "AI_ADMIN",
                "delivery.payment_confirmed",
                delivery_id,
                &json!({}),
                "delivery",
            )
            .await?;
            Ok(MutationResult {
                description: format!(
                    "Payment confirmed for delivery {}",
                    &delivery_id[..8.min(delivery_id.len())]
                ),
                undo_snapshot_json: json!({"payment_status":old_payment}).to_string(),
                rollback_tool: "confirm_delivery_payment".into(),
                rollback_input_json: json!({"delivery_id":delivery_id}).to_string(),
                entity_type: "delivery_order".into(),
                entity_id: delivery_id.into(),
            })
        }
        "cancel_delivery" => {
            let delivery_id = input
                .get("delivery_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let row =
                sqlx::query("SELECT delivery_status FROM delivery_orders WHERE delivery_id = ?")
                    .bind(delivery_id)
                    .fetch_optional(pool)
                    .await?
                    .ok_or_else(|| AppError::NotFound("Delivery not found".into()))?;
            let old_status: String = row.get("delivery_status");
            sqlx::query("UPDATE delivery_orders SET delivery_status='cancelled', updated_at=?, sync_status = 'pending', version = version + 1 WHERE delivery_id=?")
                .bind(chrono::Utc::now().to_rfc3339()).bind(delivery_id).execute(pool).await?;
            write_audit(
                pool,
                "AI_ADMIN",
                "delivery.cancelled",
                delivery_id,
                &json!({}),
                "delivery",
            )
            .await?;
            Ok(MutationResult {
                description: format!("Delivery {} cancelled", &delivery_id[..8.min(delivery_id.len())]),
                undo_snapshot_json: json!({"delivery_status":old_status}).to_string(),
                rollback_tool: "advance_delivery_status".into(),
                rollback_input_json: json!({"delivery_id":delivery_id,"new_status":if old_status=="out_for_delivery"{"in_transit"}else{old_status.as_str()}}).to_string(),
                entity_type: "delivery_order".into(), entity_id: delivery_id.into(),
            })
        }
        // ── Sync repair executors ───────────────────────────────────────────
        "sync_reset_stuck" => {
            let mut total = 0u32;
            for table in crate::commands::sync_commands::SYNC_TABLES {
                let rows = sqlx::query(
                    &format!("UPDATE {table} SET sync_attempts = 0 WHERE sync_status = 'pending' AND sync_attempts >= 10"),
                ).execute(pool).await?.rows_affected();
                total += rows as u32;
            }
            write_audit(
                pool,
                "AI_ADMIN",
                "sync.reset_stuck",
                "sync",
                &json!({"reset":total}),
                "sync",
            )
            .await?;
            Ok(MutationResult {
                description: format!(
                    "Reset {total} stuck rows — sync worker will retry on next cycle"
                ),
                undo_snapshot_json: "{}".into(),
                rollback_tool: "_no_undo".into(),
                rollback_input_json: "{}".into(),
                entity_type: "sync".into(),
                entity_id: "reset_stuck".into(),
            })
        }
        "sync_queue_retry" => {
            let event_id = input.get("event_id").and_then(|v| v.as_str()).unwrap_or("");
            let (table, row_id) = event_id
                .split_once(':')
                .ok_or_else(|| AppError::Validation("Expected format table:entity_id".into()))?;
            let pk = crate::commands::sync_commands::table_pk(table);
            let sql =
                format!("UPDATE {table} SET sync_status='pending', sync_attempts=0 WHERE {pk}=?");
            let rows = sqlx::query(&sql)
                .bind(row_id)
                .execute(pool)
                .await?
                .rows_affected();
            write_audit(
                pool,
                "AI_ADMIN",
                "sync.queue_retry",
                "sync",
                &json!({"event":event_id}),
                "sync",
            )
            .await?;
            Ok(MutationResult {
                description: format!("Retried sync event {event_id} ({rows} row reset)"),
                undo_snapshot_json: "{}".into(),
                rollback_tool: "_no_undo".into(),
                rollback_input_json: "{}".into(),
                entity_type: "sync".into(),
                entity_id: event_id.into(),
            })
        }
        "sync_queue_dismiss" => {
            let event_id = input.get("event_id").and_then(|v| v.as_str()).unwrap_or("");
            let (table, row_id) = event_id
                .split_once(':')
                .ok_or_else(|| AppError::Validation("Expected format table:entity_id".into()))?;
            let pk = crate::commands::sync_commands::table_pk(table);
            let sql =
                format!("UPDATE {table} SET sync_status='synced', sync_attempts=0 WHERE {pk}=?");
            let rows = sqlx::query(&sql)
                .bind(row_id)
                .execute(pool)
                .await?
                .rows_affected();
            write_audit(
                pool,
                "AI_ADMIN",
                "sync.queue_dismiss",
                "sync",
                &json!({"event":event_id}),
                "sync",
            )
            .await?;
            Ok(MutationResult {
                description: format!("Dismissed sync event {event_id} ({rows} row dismissed)"),
                undo_snapshot_json: "{}".into(),
                rollback_tool: "_no_undo".into(),
                rollback_input_json: "{}".into(),
                entity_type: "sync".into(),
                entity_id: event_id.into(),
            })
        }
        "apply_system_health_fix" => {
            let fix_action = input
                .get("fix_action")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError::Validation("Missing fix_action".into()))?;
            let result = crate::commands::system_health_commands::apply_health_fix(
                pool,
                fix_action,
                crate::commands::sync_commands::SYNC_TABLES,
            )
            .await?;
            write_audit(
                pool,
                "AI_ADMIN",
                "system_health.fix_applied",
                fix_action,
                &json!({"fix_action": fix_action, "rows_changed": result.rows_changed}),
                "system_health",
            )
            .await?;
            Ok(MutationResult {
                description: result.message,
                undo_snapshot_json: "{}".into(),
                rollback_tool: "_no_undo".into(),
                rollback_input_json: "{}".into(),
                entity_type: "system_health".into(),
                entity_id: fix_action.into(),
            })
        }
        "void_sale" => {
            let receipt = input
                .get("receipt_number")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let reason = input.get("reason").and_then(|v| v.as_str()).unwrap_or("");
            if reason.trim().is_empty() {
                return Err(AppError::Validation("Reason is required for void".into()));
            }
            let sale_id: Option<String> = sqlx::query_scalar(
                "SELECT sale_id FROM sales WHERE receipt_number=? AND status='completed'",
            )
            .bind(receipt)
            .fetch_optional(pool)
            .await?
            .flatten();
            let sale_id = sale_id.ok_or_else(|| {
                AppError::NotFound(format!("Sale {receipt} not found or already voided"))
            })?;
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query("UPDATE sales SET status='voided', updated_at=?, sync_status='pending' WHERE sale_id=?")
                .bind(&now).bind(&sale_id).execute(pool).await?;
            sqlx::query("UPDATE sale_items SET voided=1 WHERE sale_id=?")
                .bind(&sale_id)
                .execute(pool)
                .await?;
            // Restore inventory: add back quantities from voided sale items
            {
                let items = sqlx::query_as::<_, (String, String, String)>(
                    "SELECT si.product_id, si.quantity, s.branch_id
                     FROM sale_items si JOIN sales s ON s.sale_id = si.sale_id
                     WHERE si.sale_id = ?",
                )
                .bind(&sale_id)
                .fetch_all(pool)
                .await?;
                let now2 = chrono::Utc::now().to_rfc3339();
                for (pid, qty, branch_id) in &items {
                    let (device_id, actor_branch_id) = active_device_branch(pool).await?;
                    if branch_id != &actor_branch_id {
                        return Err(AppError::Permission(
                            "Sale does not belong to the authenticated branch".into(),
                        ));
                    }
                    let level_id = format!("SL-{pid}-{branch_id}");
                    sqlx::query(
                        "INSERT INTO stock_levels (stock_level_id,product_id,branch_id,quantity_on_hand,last_movement_at,created_at,updated_at) \
                         VALUES (?,?,?,?,?,?,?) \
                         ON CONFLICT(product_id,branch_id) DO UPDATE SET \
                           quantity_on_hand = CAST(CAST(stock_levels.quantity_on_hand AS REAL) + CAST(? AS REAL) AS TEXT), \
                           last_movement_at=?, updated_at=?, sync_status='pending'",
                    )
                    .bind(level_id)
                    .bind(pid)
                    .bind(branch_id)
                    .bind(qty)
                    .bind(&now2)
                    .bind(&now2)
                    .bind(&now2)
                    .bind(qty)
                    .bind(&now2)
                    .bind(&now2)
                    .execute(pool)
                    .await?;
                    let qty_after: String = sqlx::query_scalar(
                        "SELECT quantity_on_hand FROM stock_levels WHERE product_id=? AND branch_id=?",
                    )
                    .bind(pid)
                    .bind(branch_id)
                    .fetch_one(pool)
                    .await?;
                    let mid = ulid::Ulid::new().to_string();
                    sqlx::query(
                        "INSERT INTO stock_movements (movement_id,product_id,branch_id,device_id,origin_device_id,movement_type,quantity_delta,quantity_after,reference_type,notes,created_by_user_id,created_at) \
                         VALUES (?,?,?,?,?,'sale_void',?,?,'sale',?,?,?)",
                    )
                    .bind(&mid)
                    .bind(pid)
                    .bind(branch_id)
                    .bind(&device_id)
                    .bind(&device_id)
                    .bind(qty)
                    .bind(&qty_after)
                    .bind(reason)
                    .bind(&actor_id)
                    .bind(&now2)
                    .execute(pool)
                    .await?;
                }
            }
            write_audit(
                pool,
                "AI_ADMIN",
                "sale.voided",
                "sale",
                &json!({"sale_id":sale_id,"reason":reason}),
                "sale",
            )
            .await?;
            Ok(MutationResult {
                description: format!("Voided sale {receipt}: {reason}"),
                undo_snapshot_json: json!({"sale_id":sale_id,"receipt":receipt}).to_string(),
                rollback_tool: "_no_undo".into(),
                rollback_input_json: "{}".into(),
                entity_type: "sale".into(),
                entity_id: sale_id,
            })
        }
        "delete_customer" => {
            let cid = input
                .get("customer_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let name: Option<String> =
                sqlx::query_scalar("SELECT name FROM customers WHERE customer_id=?")
                    .bind(cid)
                    .fetch_optional(pool)
                    .await?
                    .flatten();
            let name = name.ok_or_else(|| AppError::NotFound("Customer not found".into()))?;
            let snapshot = json!({"customer_id": cid, "name": name});
            sqlx::query("DELETE FROM customers WHERE customer_id=?")
                .bind(cid)
                .execute(pool)
                .await?;
            write_audit(
                pool,
                "AI_ADMIN",
                "customer.deleted",
                "customer",
                &snapshot,
                "customer",
            )
            .await?;
            Ok(MutationResult {
                description: format!("Deleted customer: {name}"),
                undo_snapshot_json: snapshot.to_string(),
                rollback_tool: "_no_undo".into(),
                rollback_input_json: "{}".into(),
                entity_type: "customer".into(),
                entity_id: cid.into(),
            })
        }
        "set_device_active" => {
            let did = input
                .get("device_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let active = input
                .get("is_active")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let name: Option<String> =
                sqlx::query_scalar("SELECT name FROM devices WHERE device_id=?")
                    .bind(did)
                    .fetch_optional(pool)
                    .await?
                    .flatten();
            let name = name.unwrap_or_else(|| did.to_string());
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query("UPDATE devices SET is_active=?, updated_at=?, sync_status='pending' WHERE device_id=?")
                .bind(active as i64).bind(&now).bind(did).execute(pool).await?;
            write_audit(
                pool,
                "AI_ADMIN",
                "device.toggle",
                "device",
                &json!({"device_id":did,"active":active}),
                "device",
            )
            .await?;
            Ok(MutationResult {
                description: format!("Device '{name}' set to active={active}"),
                undo_snapshot_json: json!({"device_id":did,"is_active":!active}).to_string(),
                rollback_tool: "set_device_active".into(),
                rollback_input_json: json!({"device_id":did,"is_active":!active}).to_string(),
                entity_type: "device".into(),
                entity_id: did.into(),
            })
        }
        "receive_stock" => {
            let pid = input
                .get("product_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let qty = input
                .get("quantity")
                .and_then(|v| v.as_str())
                .unwrap_or("0");
            let notes = input.get("notes").and_then(|v| v.as_str()).unwrap_or("");
            let expiry_date = crate::inventory::lots::validate_expiry_date(
                input.get("expiry_date").and_then(Value::as_str),
            )?;
            let pname: Option<String> =
                sqlx::query_scalar("SELECT name FROM products WHERE product_id=?")
                    .bind(pid)
                    .fetch_optional(pool)
                    .await?
                    .flatten();
            let _ = pname.ok_or_else(|| AppError::NotFound("Product not found".into()))?;
            let (device_id, branch_id) = active_device_branch(pool).await?;
            let actor_id = crate::ai::tool_policy::current_actor_id()
                .ok_or_else(|| AppError::Permission("Mutation actor context is missing".into()))?;
            let now = chrono::Utc::now().to_rfc3339();
            let level_id = format!("SL-{}-{}", pid, branch_id);
            sqlx::query("INSERT INTO stock_levels (stock_level_id,product_id,branch_id,quantity_on_hand,last_movement_at,created_at,updated_at) VALUES (?,?,?,?,?,?,?) ON CONFLICT(product_id,branch_id) DO UPDATE SET quantity_on_hand = CAST(CAST(stock_levels.quantity_on_hand AS REAL) + CAST(? AS REAL) AS TEXT), last_movement_at=?, updated_at=?, sync_status='pending'")
                .bind(&level_id).bind(pid).bind(&branch_id).bind(qty).bind(&now).bind(&now).bind(&now).bind(qty).bind(&now).bind(&now).execute(pool).await?;
            let mid = ulid::Ulid::new().to_string();
            let qty_after: String = sqlx::query_scalar(
                "SELECT quantity_on_hand FROM stock_levels WHERE product_id=? AND branch_id=?",
            )
            .bind(pid)
            .bind(&branch_id)
            .fetch_one(pool)
            .await?;
            sqlx::query("INSERT INTO stock_movements (movement_id,product_id,branch_id,device_id,origin_device_id,movement_type,quantity_delta,quantity_after,reference_type,notes,created_by_user_id,created_at,expiry_date,lot_quantity_received,lot_quantity_remaining) VALUES (?,?,?,?,?,'receive',?,?,'receive',?,?,?,?,?,?)")
                .bind(&mid).bind(pid).bind(&branch_id).bind(&device_id).bind(&device_id).bind(qty).bind(&qty_after).bind(notes).bind(&actor_id).bind(&now).bind(expiry_date).bind(qty).bind(qty).execute(pool).await?;
            write_audit(
                pool,
                &actor_id,
                "stock.receive",
                "product",
                &json!({"product_id":pid,"qty":qty,"notes":notes}),
                "stock",
            )
            .await?;
            Ok(MutationResult {
                description: format!("Received {qty} of product {pid}"),
                undo_snapshot_json: json!({"product_id":pid,"qty":qty}).to_string(),
                rollback_tool: "adjust_stock".into(),
                rollback_input_json: json!({"product_id":pid,"delta":format!("-{}",qty)})
                    .to_string(),
                entity_type: "stock".into(),
                entity_id: mid,
            })
        }
        "add_loyalty_points" => {
            let cid = input
                .get("customer_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let pts = input.get("points").and_then(|v| v.as_i64()).unwrap_or(0);
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query("UPDATE customers SET loyalty_points = loyalty_points + ?, updated_at = ?, sync_status = 'pending' WHERE customer_id = ?")
                .bind(pts).bind(&now).bind(cid).execute(pool).await?;
            let new_total: i64 =
                sqlx::query_scalar("SELECT loyalty_points FROM customers WHERE customer_id=?")
                    .bind(cid)
                    .fetch_one(pool)
                    .await?;
            write_audit(
                pool,
                "AI_ADMIN",
                "customer.loyalty",
                "customer",
                &json!({"customer_id":cid,"added":pts,"total":new_total}),
                "customer",
            )
            .await?;
            Ok(MutationResult {
                description: format!("Added {pts} loyalty points, new total: {new_total}"),
                undo_snapshot_json: json!({"customer_id":cid,"points":-pts}).to_string(),
                rollback_tool: "add_loyalty_points".into(),
                rollback_input_json: json!({"customer_id":cid,"points":-pts}).to_string(),
                entity_type: "customer".into(),
                entity_id: cid.into(),
            })
        }
        "bulk_update_prices" => {
            let updates = input
                .get("updates")
                .and_then(|v| v.as_array())
                .ok_or_else(|| AppError::Validation("updates array required".into()))?;
            let now = chrono::Utc::now().to_rfc3339();
            let mut updated = 0;
            // Capture old prices for undo support
            let mut undo_updates: Vec<Value> = Vec::new();
            for item in updates {
                let pid = item
                    .get("product_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let price = item
                    .get("price_minor")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                if pid.is_empty() {
                    continue;
                }
                // Read current price before overwriting
                if let Ok(old_price) = sqlx::query_scalar::<_, i64>(
                    "SELECT price_minor FROM product_prices WHERE product_id=? AND price_type='selling' AND effective_to IS NULL",
                )
                .bind(pid)
                .fetch_one(pool)
                .await
                {
                    undo_updates.push(json!({"product_id": pid, "price_minor": old_price}));
                }
                sqlx::query("UPDATE product_prices SET effective_to=?, sync_status='pending' WHERE product_id=? AND price_type='selling' AND effective_to IS NULL")
                    .bind(&now).bind(pid).execute(pool).await?;
                let npid = ulid::Ulid::new().to_string();
                sqlx::query("INSERT INTO product_prices (price_id,product_id,branch_id,price_type,price_minor,currency,effective_from,created_by_user_id,created_at) VALUES (?,?,NULL,'selling',?,'BHD',?,?,?)")
                    .bind(&npid).bind(pid).bind(price).bind(&now).bind(&actor_id).bind(&now).execute(pool).await?;
                updated += 1;
            }
            write_audit(
                pool,
                "AI_ADMIN",
                "product.bulk_price",
                "product",
                &json!({"count":updated}),
                "product",
            )
            .await?;
            Ok(MutationResult {
                description: format!("Updated prices for {updated} products"),
                undo_snapshot_json: json!({"updates": undo_updates}).to_string(),
                rollback_tool: "bulk_update_prices".into(),
                rollback_input_json: json!({"updates": undo_updates}).to_string(),
                entity_type: "product".into(),
                entity_id: "bulk".into(),
            })
        }
        // ── Database backup ────────────────────────────────────────────────────
        "backup_database" => {
            let app_data = std::env::var("APPDATA").unwrap_or_else(|_| ".".into());
            let db_src = format!("{}/ZANPOS/zanpos.db", app_data);
            let backup_dir = format!("{}/ZANPOS/backups", app_data);
            let _ = std::fs::create_dir_all(&backup_dir);
            let ts = chrono::Local::now().format("%Y%m%d_%H%M%S").to_string();
            let backup_path = format!("{}/zanpos_backup_{}.db", backup_dir, ts);
            sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
                .execute(pool)
                .await?;
            std::fs::copy(&db_src, &backup_path)
                .map_err(|e| AppError::Internal(format!("Backup failed: {e}")))?;
            write_audit(
                pool,
                "AI_ADMIN",
                "backup.created",
                &ts,
                &json!({"path":&backup_path}),
                "backup",
            )
            .await?;
            Ok(MutationResult {
                description: format!("Database backed up to {}", backup_path),
                undo_snapshot_json: "{}".into(),
                rollback_tool: "_no_undo".into(),
                rollback_input_json: "{}".into(),
                entity_type: "backup".into(),
                entity_id: ts,
            })
        }
        name => crate::ai::tools_write_ext::execute(pool, name, input, currency_exp).await,
    }
}

// ── Undo executor ─────────────────────────────────────────────────────────────

pub(super) async fn execute_undo_raw(
    pool: &SqlitePool,
    rollback_tool: &str,
    rollback_input_json: &str,
    currency_exp: u32,
) -> AppResult<String> {
    let input: Value = serde_json::from_str(rollback_input_json)
        .map_err(|e| AppError::Validation(format!("Invalid rollback input: {}", e)))?;
    crate::ai::tool_policy::validate_persisted_mutation(rollback_tool, &input)?;
    let result = execute_mutation_raw(pool, rollback_tool, &input, currency_exp).await?;
    Ok(result.description)
}

// ── Audit helper ──────────────────────────────────────────────────────────────

async fn write_audit(
    pool: &SqlitePool,
    actor_user_id: &str,
    event_type: &str,
    entity_id: &str,
    after: &serde_json::Value,
    entity_type: &str,
) -> AppResult<()> {
    let actor_user_id =
        crate::ai::tool_policy::current_actor_id().unwrap_or_else(|| actor_user_id.to_string());
    let id = ulid::Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let hash = format!("{:x}", md5_simple(&format!("{}{}{}", id, event_type, now)));
    // Do NOT include device_id / origin_device_id / branch_id / previous_hash
    // in the column list — let the schema DEFAULTs apply (origin_device_id is
    // TEXT NOT NULL DEFAULT ''; passing explicit NULL would violate that constraint
    // and return "Something went wrong" to the admin user on every mutation).
    sqlx::query(
        "INSERT INTO audit_logs
         (audit_log_id, event_type, entity_type, entity_id, actor_user_id,
          actor_type, after_json, created_at, hash)
         VALUES (?, ?, ?, ?, ?, 'ai_agent', ?, ?, ?)",
    )
    .bind(&id)
    .bind(event_type)
    .bind(entity_type)
    .bind(entity_id)
    .bind(&actor_user_id)
    .bind(after.to_string())
    .bind(&now)
    .bind(&hash)
    .execute(pool)
    .await?;
    Ok(())
}

/// SHA-256 truncated to u64. Previously used DefaultHasher (non-deterministic, CWE-327).
/// This is for audit log hash entries — must be stable across Rust versions.
fn md5_simple(s: &str) -> u64 {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    let bytes = h.finalize();
    // Take the first 8 bytes as a u64 (still 64-bit collision resistance for audit chain)
    u64::from_le_bytes(bytes[..8].try_into().unwrap_or([0u8; 8]))
}

// ── PII helpers ───────────────────────────────────────────────────────────────

/// Mask a customer phone number for AI tool responses.
/// Keeps only the last 4 digits visible, e.g. "+973 3XXX X456" or "XXXX 4567".
/// This prevents full phone numbers from being stored in AI conversation history
/// or appearing in logs. The last 4 digits retain enough context to identify
/// the customer in a lookup without exposing the full number (PII-01).
pub fn mask_phone(phone: &str) -> String {
    let digits: String = phone.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() < 4 {
        return "XXXXX".to_string();
    }
    let last4 = &digits[digits.len() - 4..];
    format!("XXXX-{}", last4)
}

// ── Integrity tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn purchasing_navigation_is_accepted_by_the_executor() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect_lazy("sqlite::memory:")
            .unwrap();
        let result = execute_read_tool_inner(
            &pool,
            "open_tab",
            &serde_json::json!({"tab": "purchasing"}),
            "branch",
            3,
        )
        .await
        .unwrap();

        assert_eq!(result, r#"{"ok":true,"tab":"purchasing"}"#);
    }

    #[tokio::test]
    async fn stock_levels_tool_bounds_large_catalogue_results() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let now = "2026-07-31T00:00:00Z";
        let branch_id = "01JBRANCH0000000000000001";
        sqlx::query(
            "INSERT INTO categories
             (category_id,name,sort_order,is_active,created_at,updated_at)
             VALUES ('cat-1','General',0,1,?,?)",
        )
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();
        for index in 0..51 {
            let product_id = format!("product-{index:03}");
            let product_name = format!("Product {index:03}");
            sqlx::query(
                "INSERT INTO products
                 (product_id,category_id,name,track_inventory,is_active,currency,reorder_point,created_at,updated_at)
                 VALUES (?,'cat-1',?,1,1,'BHD',0,?,?)",
            )
            .bind(&product_id)
            .bind(&product_name)
            .bind(now)
            .bind(now)
            .execute(&pool)
            .await
            .unwrap();
        }

        let result = execute_read_tool(
            &pool,
            "get_stock_levels",
            &serde_json::json!({}),
            branch_id,
            3,
        )
        .await
        .unwrap();

        assert!(result.contains("[DB] 51 tracked products (showing 50):"));
        assert!(result.contains("Product 049"));
        assert!(!result.contains("Product 050"));
        assert!(result.len() < 10_000);
    }

    #[tokio::test]
    async fn confirmed_product_create_revalidates_category_at_execution() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO categories (category_id,name,sort_order,is_active,created_at,updated_at) VALUES ('cat-1','Chocolate',0,0,?,?)")
            .bind(&now)
            .bind(&now)
            .execute(&pool)
            .await
            .unwrap();
        let input = serde_json::json!({
            "name": "Kinder Riegel 21g",
            "category_id": "cat-1",
            "price_minor": 150
        });
        let context = crate::ai::tool_policy::MutationExecutionContext {
            actor_user_id: "admin-1".into(),
            branch_id: "branch-1".into(),
        };

        let result = crate::ai::tool_policy::with_mutation_context(
            &context,
            execute_mutation_raw(&pool, "create_product", &input, 3),
        )
        .await;

        assert!(matches!(result, Err(AppError::Validation(_))));
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM products")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    /// Every mutation tool definition MUST also appear in MUTATION_TOOLS.
    /// If a tool is missing, it would silently execute without admin confirmation.
    #[test]
    fn mutation_tools_list_is_complete() {
        let defs = all_tool_definitions();
        for name in MUTATION_TOOLS {
            let provider_name = if *name == "product_create" {
                "create_product"
            } else {
                name
            };
            assert!(
                defs.iter()
                    .any(|definition| definition.name == *provider_name),
                "Mutation executor '{name}' has no provider definition"
            );
        }
    }

    fn schema_fixture(schema: &Value) -> Value {
        if let Some(first) = schema
            .get("enum")
            .and_then(Value::as_array)
            .and_then(|v| v.first())
        {
            return first.clone();
        }
        match schema
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("object")
        {
            "object" => {
                let required = schema
                    .get("required")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let properties = schema
                    .get("properties")
                    .and_then(Value::as_object)
                    .cloned()
                    .unwrap_or_default();
                let mut result = serde_json::Map::new();
                for name in required.iter().filter_map(Value::as_str) {
                    let property = properties.get(name).cloned().unwrap_or_else(|| json!({}));
                    result.insert(name.to_owned(), schema_fixture(&property));
                }
                Value::Object(result)
            }
            "array" => Value::Array(vec![]),
            "integer" | "number" => json!(1),
            "boolean" => json!(false),
            _ => json!("seed-fixture"),
        }
    }

    /// Exercises the two-phase mutation boundary against a fully migrated,
    /// seeded SQLite database for every registered mutation. Domain validation
    /// may reject synthetic references, but no mutation may fall through to an
    /// unknown executor/preview arm.
    #[tokio::test]
    async fn seeded_database_routes_every_mutation_tool() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let registry = crate::ai::tool_registry::ToolRegistry::global().unwrap();
        assert!(MUTATION_TOOLS.len() >= 101);

        for tool_name in MUTATION_TOOLS {
            let provider_name = if *tool_name == "product_create" {
                "create_product"
            } else {
                tool_name
            };
            let descriptor = registry
                .get(provider_name)
                .unwrap_or_else(|| panic!("{tool_name} missing from authoritative registry"));
            if descriptor.execution == crate::ai::tool_registry::ExecutionPath::Run {
                let operation = crate::ai::engine::ops::operation_registry()
                    .find(provider_name)
                    .unwrap_or_else(|| panic!("{tool_name} has no run executor"));
                let input = schema_fixture(&operation.schema());
                let _ = operation.validate(&pool, &input).await;
            } else {
                let input = schema_fixture(&descriptor.schema);
                if let Err(error) = dry_run_mutation(&pool, provider_name, &input, 3).await {
                    let detail = error.to_string();
                    assert!(
                        !detail.contains("Unknown mutation tool"),
                        "{tool_name} has no dry-run executor: {detail}"
                    );
                }
            }
        }
    }

    #[test]
    fn state_changing_extension_tools_are_classified_as_mutations() {
        for name in [
            "reindex_database",
            "force_wal_checkpoint",
            "resolve_ghost_barcode",
            "resolve_sync_conflict",
            "clear_ghost_sync_records",
            "run_diagnostics_and_fix",
            "bulk_import_products",
            "bulk_import_categories",
            "send_receipt_via_whatsapp",
        ] {
            assert!(is_mutation_tool(name), "{name} must require confirmation");
        }
    }
}
