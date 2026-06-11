mod ai;
mod commands;
mod db;
mod domain;
mod errors;
pub mod hub;
mod inventory;
mod secure_store;
mod sync;
pub mod sync_v2;

use crate::sync::SyncWorker;
use sqlx::SqlitePool;
use std::sync::Arc;
use tauri::Manager;

pub struct AppState {
    pub db: SqlitePool,
    pub sync_worker: Arc<SyncWorker>,
    pub whatsapp_child: Arc<std::sync::Mutex<Option<std::process::Child>>>,
    /// Path to the sidecar's shared-secret token file (.sidecar_token).
    /// Written by the Node sidecar on startup; read by every HTTP command so
    /// requests pass the required X-Sidecar-Token auth header.
    pub wa_token_file: std::path::PathBuf,
    /// Embedded LAN hub server runtime (None = not running).
    pub hub: Arc<tokio::sync::Mutex<HubRuntime>>,
}

#[derive(Default)]
pub struct HubRuntime {
    pub handle: Option<crate::hub::HubHandle>,
    pub last_error: Option<String>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let app_data = app
                .path()
                .app_data_dir()
                .expect("Could not resolve app data directory");
            std::fs::create_dir_all(&app_data).ok();

            // OBS-01: log to BOTH stdout (dev) and a rolling daily file in the app
            // data dir (logs/zanpos.log.YYYY-MM-DD), so field issues — sync failures
            // in particular — can be diagnosed from an installed build with no console.
            // The guard must live for the whole process, so we leak it intentionally.
            {
                use tracing_subscriber::prelude::*;
                let log_dir = app_data.join("logs");
                std::fs::create_dir_all(&log_dir).ok();
                let file_appender =
                    tracing_appender::rolling::daily(&log_dir, "zanpos.log");
                let (file_writer, guard) = tracing_appender::non_blocking(file_appender);
                // Keep the worker guard alive for the entire program lifetime.
                Box::leak(Box::new(guard));

                let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

                let _ = tracing_subscriber::registry()
                    .with(env_filter)
                    .with(tracing_subscriber::fmt::layer()) // stdout
                    .with(
                        tracing_subscriber::fmt::layer()
                            .with_ansi(false)
                            .with_writer(file_writer),
                    )
                    .try_init();
            }

            let db_path = app_data.join("zanpos.db");
            let db_path_str = db_path.to_string_lossy().to_string();

            tracing::info!("Database path: {}", db_path_str);
            tracing::info!("Logs directory: {}", app_data.join("logs").to_string_lossy());

            let db = tauri::async_runtime::block_on(async {
                // If the database file exists and isn't ours, nuke it.
                // The old architecture (pre-sync_v2) had a sync_queue table;
                // the new one has sync_watermark. If neither is detected, or
                // if the old marker is found, start fresh.
                if db_path.exists() {
                    let is_ours = {
                        let tmp_url = format!("sqlite:{}?mode=ro", db_path_str);
                        if let Ok(conn) = sqlx::SqlitePool::connect(&tmp_url).await {
                            let ok: Result<String, _> = sqlx::query_scalar(
                                "SELECT name FROM sqlite_master WHERE type='table' AND name='sync_watermark'"
                            ).fetch_one(&conn).await;
                            conn.close().await;
                            ok.is_ok()
                        } else {
                            false // can't even open — assume corrupt
                        }
                    };
                    if !is_ours {
                        tracing::info!("Removing incompatible database at {}", db_path_str);
                        std::fs::remove_file(&db_path).ok();
                        std::fs::remove_file(format!("{db_path_str}-wal")).ok();
                        std::fs::remove_file(format!("{db_path_str}-shm")).ok();
                    }
                }

                // Try init. If it fails, nuke whatever is there and retry once.
                match db::init_db(&db_path_str).await {
                    Ok(pool) => {
                        if let Err(e) = db::repositories::auth_repo::rehash_plain_pins(&pool).await {
                            tracing::warn!("PIN rehash step failed: {:?}", e);
                        }
                        pool
                    }
                    Err(e) => {
                        tracing::error!("DB init failed: {}. Nuking and retrying.", e);
                        // Delete all SQLite sidecar files. If deletion fails (file
                        // locked by another process), fall back to an alternate name.
                        let files = [
                            db_path.clone(),
                            std::path::PathBuf::from(format!("{db_path_str}-wal")),
                            std::path::PathBuf::from(format!("{db_path_str}-shm")),
                            std::path::PathBuf::from(format!("{db_path_str}-journal")),
                        ];
                        for f in &files {
                            if let Err(rm) = std::fs::remove_file(f) {
                                tracing::warn!("Could not delete old DB file {}: {}", f.display(), rm);
                            }
                        }
                        // If the main DB file survived deletion (locked), use a
                        // fallback path so the app can still start.
                        let effective_path = if db_path.exists() {
                            let alt = app_data.join("zanpos_v2.db");
                            tracing::warn!("Original DB still locked — using fallback: {}", alt.display());
                            alt.to_string_lossy().to_string()
                        } else {
                            db_path_str.clone()
                        };
                        db::init_db(&effective_path)
                            .await
                            .expect("Failed to initialize database after nuke")
                    }
                }
            });

            // Spawn background sync worker
            let sync_worker = SyncWorker::new(db.clone());
            SyncWorker::spawn(sync_worker.clone());

            // ── Start LAN hub server when this device is the hub ─────────────────────
            let hub_runtime: Arc<tokio::sync::Mutex<HubRuntime>> = Arc::new(Default::default());
            {
                let db_hub = db.clone();
                let rt = hub_runtime.clone();
                tauri::async_runtime::spawn(async move {
                    let mode: Option<String> = sqlx::query_scalar(
                        "SELECT value FROM app_config WHERE key='hub_mode'")
                        .fetch_optional(&db_hub).await.ok().flatten();
                    if mode.as_deref() != Some("1") { return; }
                    let port: u16 = sqlx::query_scalar::<_, String>(
                        "SELECT value FROM app_config WHERE key='hub_port'")
                        .fetch_optional(&db_hub).await.ok().flatten()
                        .and_then(|v| v.parse().ok()).unwrap_or(8923);
                    let Some(token) = crate::secure_store::get_secret("hub_store_token") else {
                        let msg = "hub_mode=1 but hub_store_token missing from credential store";
                        tracing::error!("{msg}");
                        rt.lock().await.last_error = Some(msg.into());
                        return;
                    };
                    match crate::hub::start_hub(db_hub, port, &token).await {
                        Ok(h) => { rt.lock().await.handle = Some(h); }
                        Err(e) => {
                            tracing::error!("Hub start failed: {e}");
                            rt.lock().await.last_error = Some(e.to_string());
                        }
                    }
                });
            }

            // ── Start WhatsApp sidecar ────────────────────────────────────────────────
            let wa_session_dir = app_data.join("wa-session");
            std::fs::create_dir_all(&wa_session_dir).ok();

            // Resolve the sidecar executable — try every known location so it works
            // in production (Tauri strips the triple-target suffix), dev (full suffix),
            // and any edge-case install layout.
            let sidecar_exe = {
                let exe_dir = std::env::current_exe()
                    .ok()
                    .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                    .unwrap_or_default();

                let candidates = [
                    // ① Production install: Tauri strips the triple suffix
                    exe_dir.join("whatsapp-sidecar.exe"),
                    // ② Production install, alt name (in case triple is kept)
                    exe_dir.join("whatsapp-sidecar-x86_64-pc-windows-msvc.exe"),
                    // ③ Development: cargo manifest dir / binaries /
                    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("binaries")
                        .join("whatsapp-sidecar-x86_64-pc-windows-msvc.exe"),
                ];

                // Return the first candidate that actually exists on disk.
                // If none exist yet, return the production path so log messages are clear.
                candidates.into_iter()
                    .find(|p| p.exists())
                    .unwrap_or_else(|| exe_dir.join("whatsapp-sidecar.exe"))
            };

            /// Spawn the sidecar with up to `max_attempts` retries.
            /// `log_path` receives the sidecar's stdout + stderr so crashes are
            /// visible in logs/whatsapp-sidecar.log instead of disappearing silently.
            fn spawn_sidecar(
                exe: &std::path::Path,
                session_dir: &std::path::Path,
                log_path: &std::path::Path,
                max_attempts: u32,
            ) -> Option<std::process::Child> {
                for attempt in 1..=max_attempts {
                    // Build the command first so we can attach the Windows-only
                    // CREATE_NO_WINDOW flag before spawning.  Without this flag
                    // every spawn flashes a cmd.exe console window and steals
                    // keyboard focus from the Tauri window.
                    let mut cmd = std::process::Command::new(exe);
                    cmd.arg(format!("--session-dir={}", session_dir.to_string_lossy()));

                    // Redirect stdout + stderr to a dedicated log file.
                    // Two separate file handles are required (one per stream).
                    // Fall back to null if the log file can't be opened so we
                    // never prevent startup due to a filesystem permission issue.
                    let out_file = std::fs::OpenOptions::new()
                        .create(true).append(true).open(log_path);
                    let err_file = std::fs::OpenOptions::new()
                        .create(true).append(true).open(log_path);
                    match (out_file, err_file) {
                        (Ok(out), Ok(err)) => { cmd.stdout(out).stderr(err); }
                        _ => {
                            cmd.stdout(std::process::Stdio::null())
                               .stderr(std::process::Stdio::null());
                        }
                    }

                    #[cfg(target_os = "windows")]
                    {
                        use std::os::windows::process::CommandExt;
                        // F-LOW-14: Named constant instead of magic number.
                        // Prevents a console window from appearing when the
                        // sidecar (a Node.js CLI) is spawned or restarted by
                        // the watchdog, which would otherwise steal focus every
                        // ~8 seconds.
                        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
                        cmd.creation_flags(CREATE_NO_WINDOW);
                    }

                    match cmd.spawn() {
                        Ok(child) => {
                            tracing::info!(
                                "WhatsApp sidecar started (pid {}, attempt {})",
                                child.id(), attempt
                            );
                            return Some(child);
                        }
                        Err(e) if attempt < max_attempts => {
                            tracing::warn!(
                                "WhatsApp sidecar start attempt {}/{} failed: {} — retrying in 1 s",
                                attempt, max_attempts, e
                            );
                            std::thread::sleep(std::time::Duration::from_secs(1));
                        }
                        Err(e) => {
                            tracing::error!(
                                "WhatsApp sidecar failed to start after {} attempts: {}",
                                max_attempts, e
                            );
                        }
                    }
                }
                None
            }

            let wa_log_path = app_data.join("logs").join("whatsapp-sidecar.log");

            let initial_child = if sidecar_exe.exists() {
                spawn_sidecar(&sidecar_exe, &wa_session_dir, &wa_log_path, 3)
            } else {
                tracing::warn!(
                    "WhatsApp sidecar binary not found at {:?} — watchdog will keep retrying",
                    sidecar_exe
                );
                None
            };

            let wa_child: Arc<std::sync::Mutex<Option<std::process::Child>>> =
                Arc::new(std::sync::Mutex::new(initial_child));

            // ── Watchdog: keeps the sidecar alive for the entire app lifetime ─────────
            // Every 8 seconds, check if the process is still running.
            // If it has exited (crash, OOM, killed by Windows) — restart it immediately.
            {
                let wa_child_watch  = Arc::clone(&wa_child);
                let exe_watch       = sidecar_exe.clone();
                let session_watch   = wa_session_dir.clone();
                let log_watch       = wa_log_path.clone();

                tauri::async_runtime::spawn(async move {
                    loop {
                        tokio::time::sleep(tokio::time::Duration::from_secs(8)).await;

                        let needs_restart = {
                            let mut guard = wa_child_watch
                                .lock()
                                .unwrap_or_else(|e| e.into_inner());

                            match guard.as_mut() {
                                // Process slot is empty — sidecar was never started or already
                                // removed after a previous exit.  Try again if the exe is present.
                                None => exe_watch.exists(),

                                Some(child) => match child.try_wait() {
                                    Ok(None) => false, // still running — do nothing
                                    Ok(Some(status)) => {
                                        tracing::warn!(
                                            "WhatsApp sidecar exited (status {:?}) — watchdog restarting",
                                            status
                                        );
                                        *guard = None;
                                        true
                                    }
                                    Err(e) => {
                                        tracing::warn!(
                                            "WhatsApp sidecar status check error: {} — watchdog restarting",
                                            e
                                        );
                                        *guard = None;
                                        true
                                    }
                                },
                            }
                        };

                        if needs_restart {
                            if exe_watch.exists() {
                                if let Some(child) =
                                    spawn_sidecar(&exe_watch, &session_watch, &log_watch, 3)
                                {
                                    let mut guard = wa_child_watch
                                        .lock()
                                        .unwrap_or_else(|e| e.into_inner());
                                    *guard = Some(child);
                                }
                            } else {
                                tracing::warn!(
                                    "WhatsApp sidecar watchdog: binary still not found at {:?}",
                                    exe_watch
                                );
                            }
                        }
                    }
                });
            }

            // ORDERING CONSTRAINT: app.manage() MUST be called before
            // .invoke_handler() is registered.  tauri::generate_handler![] builds
            // a static dispatch table at compile time, but the AppState value is
            // resolved at runtime from the managed state map.  If manage() were
            // called after invoke_handler(), any command that fires before manage()
            // completes would panic with "state not managed".  Keep this order.
            let wa_token_file = wa_session_dir.join(".sidecar_token");
            app.manage(AppState { db, sync_worker, whatsapp_child: wa_child, wa_token_file, hub: hub_runtime });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Auth
            commands::auth_commands::auth_list_users,
            commands::auth_commands::auth_login_pin,
            commands::auth_commands::auth_verify_owner_pin,
            commands::auth_commands::auth_validate_manager_pin,
            // Shift
            commands::shift_commands::shift_get_active,
            commands::shift_commands::shift_open,
            commands::shift_commands::shift_close,
            // POS
            commands::pos_commands::pos_start_cart,
            commands::pos_commands::pos_add_item,
            commands::pos_commands::pos_add_item_by_barcode,
            commands::pos_commands::pos_update_quantity,
            commands::pos_commands::pos_set_line_price,
            commands::pos_commands::pos_remove_line,
            commands::pos_commands::pos_finalize_sale,
            commands::pos_commands::pos_cart_summary,
            commands::pos_commands::pos_record_void,
            commands::pos_commands::pos_apply_bill_discount,
            commands::pos_commands::pos_apply_line_discount,
            commands::pos_commands::pos_set_line_note,
            commands::pos_commands::pos_add_custom_item,
            commands::pos_commands::pos_load_sale_for_edit,
            commands::pos_commands::pos_void_sale,
            // Back-office admin
            commands::admin_commands::admin_list_products,
            commands::admin_commands::admin_create_product,
            commands::admin_commands::admin_update_product,
            commands::admin_commands::admin_list_categories,
            commands::admin_commands::admin_list_tax_rules,
            commands::admin_commands::admin_save_tax_rule,
            commands::admin_commands::admin_delete_tax_rule,
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
            commands::report_commands::report_z_report,
            commands::report_commands::reports_config_load,
            commands::report_commands::reports_config_save,
            commands::report_commands::db_integrity_check,
            // Inventory
            commands::inventory_commands::inventory_get_levels,
            commands::inventory_commands::inventory_get_levels_paged,
            commands::inventory_commands::inventory_get_low_stock,
            commands::inventory_commands::inventory_get_movements,
            commands::inventory_commands::inventory_receive_stock,
            commands::inventory_commands::inventory_adjust_stock,
            commands::inventory_commands::inventory_bulk_stock_take,
            // Setup & Settings
            commands::setup_commands::app_config_load,
            commands::setup_commands::setup_wizard_complete,
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
            commands::sync_commands::sync_bulk_initial,
            commands::sync_commands::setup_pull_catalog,
            commands::sync_commands::sync_force_full_resync,
            commands::sync_commands::sync_reset_stuck,
            commands::sync_commands::sync_queue_list,
            commands::sync_commands::sync_queue_retry,
            commands::sync_commands::sync_queue_dismiss,
            commands::sync_commands::sync_queue_stats,
            commands::sync_commands::sync_diagnostics,
            // Hub (LAN sync server)
            commands::hub_commands::hub_status,
            commands::hub_commands::hub_enable,
            commands::hub_commands::hub_regenerate_token,
            commands::hub_commands::hub_test_connection,
            commands::hub_commands::hub_join,
            commands::hub_commands::hub_connect_existing,
            commands::hub_commands::hub_set_url,
            // AI Admin — provider management
            commands::ai_admin_commands::admin_get_provider_config,
            commands::ai_admin_commands::admin_set_anthropic,
            commands::ai_admin_commands::admin_validate_anthropic,
            commands::ai_admin_commands::admin_validate_openai,
            commands::ai_admin_commands::admin_set_openai,
            commands::ai_admin_commands::admin_validate_gemini,
            commands::ai_admin_commands::admin_set_gemini,
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
            commands::delivery_commands::delivery_revert_payment,
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
            commands::whatsapp_commands::whatsapp_import_contacts,
            commands::whatsapp_commands::whatsapp_send_receipt_pdf,
            // Product image picker
            commands::updater_commands::product_pick_image,
            // Auto-updater
            commands::updater_commands::check_for_updates,
            commands::updater_commands::download_and_install_update,
            // Thermal printer
            commands::thermal_commands::thermal_list_ports,
            commands::thermal_commands::thermal_get_config,
            commands::thermal_commands::thermal_set_config,
            commands::thermal_commands::thermal_print_test,
            commands::thermal_commands::print_receipt_raw,
            commands::thermal_commands::open_cash_drawer,
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
            // Bulk CSV import
            commands::admin_commands::admin_bulk_import_categories,
            commands::admin_commands::admin_bulk_import_products,
            // Migration agent — core
            commands::migration_commands::migration_inspect_file,
            commands::migration_commands::migration_ai_map,
            commands::migration_commands::migration_execute,
            // Migration agent — extended tools
            commands::migration_commands::migration_connect_test,
            commands::migration_commands::migration_list_tables,
            commands::migration_commands::migration_query_remote,
            commands::migration_commands::migration_list_processes,
            commands::migration_commands::migration_find_db_files,
            commands::migration_commands::migration_read_file,
            commands::migration_commands::migration_decompress,
            commands::migration_commands::migration_zanpos_stats,
            commands::migration_commands::migration_rollback,
            commands::migration_commands::migration_agent_chat,
            // Ghost barcode lookup
            commands::ghost_barcode_commands::ghost_record,
            commands::ghost_barcode_commands::ghost_summary,
            commands::ghost_barcode_commands::ghost_list,
            commands::ghost_barcode_commands::ghost_resolve,
            commands::ghost_barcode_commands::ghost_dismiss,
            commands::ghost_barcode_commands::ghost_prefill,
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
