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

            // ── One-time bootstrap: enqueue all existing customers + config flags ──────
            // Runs once per install (guarded by app_config key 'sync_bootstrap_v1').
            // Ensures that existing data on device 1 reaches Supabase so newly-joining
            // terminals pull everything on their first sync.
            {
                let db_boot = db.clone();
                tauri::async_runtime::spawn(async move {
                    let done: Option<String> = sqlx::query_scalar(
                        "SELECT value FROM app_config WHERE key='sync_bootstrap_v1'",
                    )
                    .fetch_optional(&db_boot)
                    .await
                    .ok()
                    .flatten()
                    .flatten();

                    if done.as_deref() == Some("1") {
                        return; // already ran
                    }

                    // Resolve device + branch ids
                    let device_id: String = sqlx::query_scalar(
                        "SELECT device_id FROM devices WHERE is_active=1 ORDER BY device_code LIMIT 1",
                    )
                    .fetch_optional(&db_boot)
                    .await
                    .ok()
                    .flatten()
                    .flatten()
                    .unwrap_or_default();

                    let branch_id: String = sqlx::query_scalar(
                        "SELECT branch_id FROM branches WHERE is_active=1 ORDER BY created_at LIMIT 1",
                    )
                    .fetch_optional(&db_boot)
                    .await
                    .ok()
                    .flatten()
                    .flatten()
                    .unwrap_or_default();

                    if device_id.is_empty() || branch_id.is_empty() {
                        return; // setup not done yet — skip
                    }

                    // Enqueue all existing customers
                    if let Ok(rows) = sqlx::query(
                        "SELECT customer_id, name, phone, email, loyalty_points, created_at, notes
                         FROM customers WHERE branch_id = ?",
                    )
                    .bind(&branch_id)
                    .fetch_all(&db_boot)
                    .await
                    {
                        use sqlx::Row;
                        for r in &rows {
                            let _ = crate::sync::outbox::enqueue_customer(
                                &db_boot,
                                &device_id,
                                &branch_id,
                                r.get("customer_id"),
                                r.get("name"),
                                r.get("phone"),
                                r.get("email"),
                                r.get::<i64, _>("loyalty_points"),
                                r.get("notes"),
                                r.get("created_at"),
                            )
                            .await;
                        }
                        tracing::info!(
                            "sync bootstrap: enqueued {} customers",
                            rows.len()
                        );
                    }

                    // Enqueue all store-wide app_config flags
                    let syncable_keys = [
                        "flag_allow_negative_stock",
                        "flag_require_discount_reason",
                        "flag_cashier_can_discount",
                        "flag_auto_print_receipt",
                        "whatsapp_benefit_number",
                    ];
                    for key in &syncable_keys {
                        if let Ok(Some(value)) = sqlx::query_scalar::<_, String>(
                            "SELECT value FROM app_config WHERE key = ?",
                        )
                        .bind(key)
                        .fetch_optional(&db_boot)
                        .await
                        {
                            let _ = crate::sync::outbox::enqueue_app_config(
                                &db_boot,
                                &device_id,
                                &branch_id,
                                key,
                                &value,
                            )
                            .await;
                        }
                    }

                    // Mark bootstrap done
                    let _ = sqlx::query(
                        "INSERT INTO app_config(key, value) VALUES('sync_bootstrap_v1','1')
                         ON CONFLICT(key) DO UPDATE SET value='1'",
                    )
                    .execute(&db_boot)
                    .await;

                    tracing::info!("sync bootstrap v1 complete");
                });
            }

            // ── Push branch record to Supabase so second terminals can join ──────────
            // Runs on every startup (not guarded). Harmless no-op when no Supabase
            // credentials are configured. Ensures Device 1's branch always exists in
            // the central `branches` table so Device 2's join flow can find it.
            {
                let db_br = db.clone();
                tauri::async_runtime::spawn(async move {
                    // Only proceed when setup is complete
                    let setup_done: Option<String> = sqlx::query_scalar(
                        "SELECT value FROM app_config WHERE key='setup_complete'",
                    )
                    .fetch_optional(&db_br).await.ok().flatten().flatten();
                    if setup_done.as_deref() != Some("1") { return; }

                    let sb_url: Option<String> = sqlx::query_scalar(
                        "SELECT value FROM app_config WHERE key='supabase_url'",
                    ).fetch_optional(&db_br).await.ok().flatten().flatten();
                    let sb_key = secure_store::get_secret("supabase_service_key");

                    let (Some(url), Some(key)) = (sb_url, sb_key) else { return; };
                    if url.is_empty() || key.is_empty() { return; }

                    use sqlx::Row;
                    let branch_row = sqlx::query(
                        "SELECT branch_id, branch_code, name, currency, timezone,
                                address, phone, receipt_header, receipt_footer, tax_number, cr_number,
                                created_at
                         FROM branches WHERE is_active=1 ORDER BY created_at LIMIT 1",
                    ).fetch_optional(&db_br).await.ok().flatten();

                    if let Some(br) = branch_row {
                        use crate::sync::supabase_client::SupabaseClient;
                        let client = SupabaseClient::new(url, key);
                        let now = chrono::Utc::now().to_rfc3339();
                        let branch_json = serde_json::json!({
                            "branch_id":      br.get::<String, _>("branch_id"),
                            "branch_code":    br.get::<String, _>("branch_code"),
                            "name":           br.get::<String, _>("name"),
                            "currency":       br.get::<String, _>("currency"),
                            "timezone":       br.get::<String, _>("timezone"),
                            "address":        br.get::<Option<String>, _>("address"),
                            "phone":          br.get::<Option<String>, _>("phone"),
                            "receipt_header": br.get::<Option<String>, _>("receipt_header"),
                            "receipt_footer": br.get::<Option<String>, _>("receipt_footer"),
                            "tax_number":     br.get::<Option<String>, _>("tax_number"),
                            "cr_number":      br.get::<Option<String>, _>("cr_number"),
                            "is_active":      true,
                            "created_at":     br.get::<String, _>("created_at"),
                            "updated_at":     now,
                        });
                        if let Err(e) = client.upsert_branch(&branch_json).await {
                            tracing::warn!("startup branch upsert failed: {e:?}");
                        } else {
                            tracing::info!("startup: branch record pushed to Supabase");
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
            fn spawn_sidecar(
                exe: &std::path::Path,
                session_dir: &std::path::Path,
                max_attempts: u32,
            ) -> Option<std::process::Child> {
                for attempt in 1..=max_attempts {
                    // Build the command first so we can attach the Windows-only
                    // CREATE_NO_WINDOW flag before spawning.  Without this flag
                    // every spawn flashes a cmd.exe console window and steals
                    // keyboard focus from the Tauri window.
                    let mut cmd = std::process::Command::new(exe);
                    cmd.arg(format!("--session-dir={}", session_dir.to_string_lossy()))
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null());

                    #[cfg(target_os = "windows")]
                    {
                        use std::os::windows::process::CommandExt;
                        // 0x0800_0000 = CREATE_NO_WINDOW
                        // Prevents a console window from appearing when the
                        // sidecar (a Node.js CLI) is spawned or restarted by
                        // the watchdog, which would otherwise steal focus every
                        // ~8 seconds.
                        cmd.creation_flags(0x0800_0000);
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

            let initial_child = if sidecar_exe.exists() {
                spawn_sidecar(&sidecar_exe, &wa_session_dir, 3)
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
                                    spawn_sidecar(&exe_watch, &session_watch, 3)
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
            commands::admin_commands::admin_save_tax_rule,
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
            commands::inventory_commands::inventory_get_levels_paged,
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
            commands::whatsapp_commands::whatsapp_import_contacts,
            // Product image picker
            commands::updater_commands::product_pick_image,
            // Auto-updater
            commands::updater_commands::check_for_updates,
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
