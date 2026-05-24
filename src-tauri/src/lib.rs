mod ai;
mod commands;
mod db;
mod domain;
mod errors;
mod inventory;
mod secure_store;
mod sync;

use crate::sync::SyncWorker;
use sqlx::SqlitePool;
use std::sync::Arc;
use tauri::Manager;

pub struct AppState {
    pub db: SqlitePool,
    pub sync_worker: Arc<SyncWorker>,
    pub whatsapp_child: Arc<std::sync::Mutex<Option<std::process::Child>>>,
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

            let app_data = app
                .path()
                .app_data_dir()
                .expect("Could not resolve app data directory");
            std::fs::create_dir_all(&app_data).ok();
            let db_path = app_data.join("zanpos.db");
            let db_path_str = db_path.to_string_lossy().to_string();

            tracing::info!("Database path: {}", db_path_str);

            let db = tauri::async_runtime::block_on(async {
                let pool = db::init_db(&db_path_str)
                    .await
                    .expect("Failed to initialize database");
                // Migrate any legacy PLAIN: PINs to argon2id on first launch
                if let Err(e) = db::repositories::auth_repo::rehash_plain_pins(&pool).await {
                    tracing::warn!("PIN rehash step failed: {:?}", e);
                }
                pool
            });

            // Spawn background sync worker
            let sync_worker = SyncWorker::new(db.clone());
            SyncWorker::spawn(sync_worker.clone());

            // ── Start WhatsApp sidecar ────────────────────────────────────────────────
            let wa_session_dir = app_data.join("wa-session");
            std::fs::create_dir_all(&wa_session_dir).ok();

            let sidecar_exe = {
                let prod_path = app
                    .path()
                    .resource_dir()
                    .map(|p| p.join("whatsapp-sidecar-x86_64-pc-windows-msvc.exe"))
                    .unwrap_or_default();
                let dev_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("binaries")
                    .join("whatsapp-sidecar-x86_64-pc-windows-msvc.exe");
                if prod_path.exists() { prod_path } else { dev_path }
            };

            let wa_child: Arc<std::sync::Mutex<Option<std::process::Child>>> = if sidecar_exe.exists() {
                match std::process::Command::new(&sidecar_exe)
                    .arg(format!("--session-dir={}", wa_session_dir.to_string_lossy()))
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                {
                    Ok(child) => {
                        tracing::info!("WhatsApp sidecar started (pid {})", child.id());
                        Arc::new(std::sync::Mutex::new(Some(child)))
                    }
                    Err(e) => {
                        tracing::warn!("WhatsApp sidecar failed to start: {}", e);
                        Arc::new(std::sync::Mutex::new(None))
                    }
                }
            } else {
                tracing::info!("WhatsApp sidecar binary not found — WA features disabled");
                Arc::new(std::sync::Mutex::new(None))
            };

            app.manage(AppState { db, sync_worker, whatsapp_child: wa_child });
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
            commands::pos_commands::pos_record_void,
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
            commands::report_commands::report_by_cashier,
            commands::report_commands::report_eod_cashup,
            commands::report_commands::db_integrity_check,
            // Inventory
            commands::inventory_commands::inventory_get_levels,
            commands::inventory_commands::inventory_get_low_stock,
            commands::inventory_commands::inventory_get_movements,
            commands::inventory_commands::inventory_receive_stock,
            commands::inventory_commands::inventory_adjust_stock,
            commands::inventory_commands::inventory_bulk_stock_take,
            // Setup & Settings
            commands::setup_commands::app_config_load,
            commands::setup_commands::setup_wizard_complete,
            commands::setup_commands::setup_join_store,
            commands::setup_commands::settings_get_branch,
            commands::setup_commands::settings_update_branch,
            commands::setup_commands::setup_save_benefit_number,
            commands::setup_commands::business_flags_load,
            commands::setup_commands::business_flags_save,
            // Phase 10a — timeout, backup, tax report, audit log
            commands::phase10a_commands::app_config_get_timeout,
            commands::phase10a_commands::app_config_set_timeout,
            commands::phase10a_commands::db_backup,
            commands::phase10a_commands::report_tax_by_day,
            commands::phase10a_commands::audit_log_list,
            commands::phase10a_commands::audit_verify_chain,
            // Sync
            commands::sync_commands::sync_status,
            commands::sync_commands::sync_trigger_now,
            commands::sync_commands::sync_queue_list,
            commands::sync_commands::sync_queue_retry,
            commands::sync_commands::sync_queue_dismiss,
            commands::sync_commands::admin_setup_supabase,
            commands::sync_commands::admin_setup_supabase_creds_only,
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
            // AI Admin — streaming + history
            commands::ai_admin_commands::ai_chat_stream,
            commands::ai_admin_commands::ai_save_message,
            commands::ai_admin_commands::ai_load_history,
            commands::ai_admin_commands::ai_clear_history,
            // Delivery
            commands::delivery_commands::delivery_list,
            commands::delivery_commands::delivery_get,
            commands::delivery_commands::delivery_update_status,
            commands::delivery_commands::delivery_confirm_payment,
            commands::delivery_commands::delivery_cancel,
            commands::delivery_commands::delivery_rider_suggestions,
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
            // WhatsApp
            commands::whatsapp_commands::whatsapp_status,
            commands::whatsapp_commands::whatsapp_send_delivery,
            commands::whatsapp_commands::whatsapp_notify_arrival,
            commands::whatsapp_commands::whatsapp_payment_reminder,
            commands::whatsapp_commands::whatsapp_disconnect,
            commands::whatsapp_commands::whatsapp_save_config,
            // Product image picker
            commands::updater_commands::product_pick_image,
            // Auto-updater
            commands::updater_commands::check_for_updates,
            // Thermal printer
            commands::thermal_commands::thermal_get_config,
            commands::thermal_commands::thermal_set_config,
            commands::thermal_commands::thermal_print_test,
            commands::thermal_commands::print_receipt_raw,
            // Cash events
            commands::cash_commands::cash_event_create,
            commands::cash_commands::cash_events_list,
            commands::cash_commands::cash_drawer_summary,
            commands::cash_commands::cash_x_report,
            commands::cash_commands::cash_no_sale,
            // Product barcodes
            commands::admin_commands::product_barcode_add,
            commands::admin_commands::product_barcode_remove,
            commands::admin_commands::product_barcodes_list,
            // Migration agent
            commands::migration_commands::migration_agent_chat,
            commands::migration_commands::migration_confirm_execute,
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                let wa_child = {
                    let state: tauri::State<'_, AppState> = window.state();
                    Arc::clone(&state.whatsapp_child)
                };
                let mut guard = wa_child.lock().unwrap_or_else(|e| e.into_inner());
                if let Some(ref mut child) = *guard {
                    let _ = child.kill();
                    let _ = child.wait();
                    tracing::info!("WhatsApp sidecar terminated");
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
