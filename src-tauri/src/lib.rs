mod ai;
mod commands;
mod db;
mod domain;
mod errors;
mod inventory;
mod sync;

use std::sync::Arc;
use sqlx::SqlitePool;
use tauri::Manager;
use crate::sync::SyncWorker;

pub struct AppState {
    pub db:          SqlitePool,
    pub sync_worker: Arc<SyncWorker>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
                )
                .init();

            let app_data = app.path().app_data_dir()
                .expect("Could not resolve app data directory");
            std::fs::create_dir_all(&app_data).ok();
            let db_path = app_data.join("zanpos.db");
            let db_path_str = db_path.to_string_lossy().to_string();

            tracing::info!("Database path: {}", db_path_str);

            let db = tauri::async_runtime::block_on(async {
                db::init_db(&db_path_str).await.expect("Failed to initialize database")
            });

            // Spawn background sync worker
            let sync_worker = SyncWorker::new(db.clone());
            SyncWorker::spawn(sync_worker.clone());

            app.manage(AppState { db, sync_worker });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Auth
            commands::auth_commands::auth_list_users,
            commands::auth_commands::auth_login_pin,
            // Shift
            commands::shift_commands::shift_get_active,
            commands::shift_commands::shift_open,
            commands::shift_commands::shift_close,
            // POS
            commands::pos_commands::pos_start_cart,
            commands::pos_commands::pos_add_item,
            commands::pos_commands::pos_add_item_by_barcode,
            commands::pos_commands::pos_update_quantity,
            commands::pos_commands::pos_remove_line,
            commands::pos_commands::pos_finalize_sale,
            commands::pos_commands::pos_cart_summary,
            commands::pos_commands::pos_apply_bill_discount,
            commands::pos_commands::pos_apply_line_discount,
            commands::pos_commands::pos_set_line_note,
            commands::pos_commands::pos_add_custom_item,
            commands::pos_commands::pos_void_sale,
            // Back-office admin
            commands::admin_commands::admin_list_products,
            commands::admin_commands::admin_create_product,
            commands::admin_commands::admin_update_product,
            commands::admin_commands::admin_list_categories,
            commands::admin_commands::admin_list_tax_rules,
            commands::admin_commands::admin_save_category,
            commands::admin_commands::admin_list_users_all,
            commands::admin_commands::admin_list_roles,
            commands::admin_commands::admin_create_user,
            commands::admin_commands::admin_update_user,
            // Products
            commands::product_commands::product_search,
            commands::product_commands::product_get_by_barcode,
            commands::product_commands::product_list_all,
            // Held carts
            commands::held_cart_commands::held_cart_save,
            commands::held_cart_commands::held_cart_list,
            commands::held_cart_commands::held_cart_resume,
            commands::held_cart_commands::held_cart_delete,
            // Refunds
            commands::refund_commands::refund_get_sale,
            commands::refund_commands::refund_create,
            commands::refund_commands::receipt_reprint,
            // Reports
            commands::report_commands::report_today,
            commands::report_commands::report_date_range,
            commands::report_commands::report_top_products,
            commands::report_commands::report_sales_list,
            commands::report_commands::db_integrity_check,
            // Inventory
            commands::inventory_commands::inventory_get_levels,
            commands::inventory_commands::inventory_get_low_stock,
            commands::inventory_commands::inventory_get_movements,
            commands::inventory_commands::inventory_receive_stock,
            commands::inventory_commands::inventory_adjust_stock,
            // Setup & Settings
            commands::setup_commands::app_config_load,
            commands::setup_commands::setup_wizard_complete,
            commands::setup_commands::settings_get_branch,
            commands::setup_commands::settings_update_branch,
            // Phase 10a — timeout, backup, tax report, audit log
            commands::phase10a_commands::app_config_get_timeout,
            commands::phase10a_commands::app_config_set_timeout,
            commands::phase10a_commands::db_backup,
            commands::phase10a_commands::report_tax_by_day,
            commands::phase10a_commands::audit_log_list,
            // Sync
            commands::sync_commands::sync_status,
            commands::sync_commands::sync_trigger_now,
            commands::sync_commands::admin_setup_supabase,
            commands::sync_commands::admin_get_supabase_status,
            // AI Admin — provider management
            commands::ai_admin_commands::admin_get_provider_config,
            commands::ai_admin_commands::admin_set_anthropic,
            commands::ai_admin_commands::admin_validate_openai,
            commands::ai_admin_commands::admin_set_openai,
            // AI Admin — legacy (kept for compat)
            commands::ai_admin_commands::admin_get_api_key_set,
            commands::ai_admin_commands::admin_set_api_key,
            // AI Admin — chat
            commands::ai_admin_commands::ai_chat,
            commands::ai_admin_commands::ai_execute_action,
            commands::ai_admin_commands::ai_cancel_action,
            commands::ai_admin_commands::ai_undo_action,
            // Customers
            commands::customer_commands::customer_list,
            commands::customer_commands::customer_create,
            commands::customer_commands::customer_update,
            commands::customer_commands::customer_get,
            commands::customer_commands::customer_add_loyalty,
            // Devices
            commands::device_commands::device_list,
            commands::device_commands::device_create,
            commands::device_commands::device_toggle_active,
            // Product image picker
            commands::updater_commands::product_pick_image,
            // Auto-updater
            commands::updater_commands::check_for_updates,
            // Thermal printer
            commands::thermal_commands::thermal_get_config,
            commands::thermal_commands::thermal_set_config,
            commands::thermal_commands::thermal_print_test,
            commands::thermal_commands::print_receipt_raw,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
