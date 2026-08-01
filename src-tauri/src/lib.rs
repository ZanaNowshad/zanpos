#![allow(
    clippy::items_after_test_module,
    clippy::result_large_err,
    clippy::too_many_arguments,
    clippy::type_complexity,
    clippy::await_holding_lock
)]

mod ai;
mod app_events;
mod auth_session;
mod backup;
mod commands;
mod db;
mod db_recovery;
mod diagnostics;
mod digest;
mod domain;
mod errors;
pub mod hub;
mod inventory;
mod license;
pub mod printing;
mod secure_store;
pub mod storefront;
mod sync;
pub mod sync_v2;
mod telemetry_uploader;

use crate::sync::SyncWorker;
use chrono::Utc;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;
use tauri::{Emitter, Manager};

/// Tracks the WhatsApp sidecar child process and provides a clean shutdown path.
/// Mirrors the hub::HubHandle pattern (oneshot channel + graceful teardown).
pub struct SidecarHandle {
    pub child: Arc<std::sync::Mutex<Option<tokio::process::Child>>>,
    shutdown_tx: Arc<std::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>>>,
}

impl SidecarHandle {
    /// Signal the watchdog loop to stop and shut down the sidecar gracefully.
    /// Posts /shutdown to the sidecar HTTP endpoint first (so Baileys gets a
    /// clean sock.end()), then kills the process as a failsafe.
    pub async fn shutdown(self, token_file: &std::path::Path) {
        // Signal watchdog to stop polling
        if let Some(tx) = self
            .shutdown_tx
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        {
            let _ = tx.send(());
        }

        // Try graceful HTTP shutdown first
        let token = std::fs::read_to_string(token_file)
            .unwrap_or_default()
            .trim()
            .to_string();
        if !token.is_empty() {
            let client = reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(2))
                .timeout(std::time::Duration::from_secs(3))
                .build()
                .unwrap_or_default();
            let _ = client
                .post("http://127.0.0.1:3131/shutdown")
                .header("X-Sidecar-Token", &token)
                .send()
                .await;
        }

        // Kill + reap as failsafe (TerminateProcess on Windows)
        #[allow(clippy::await_holding_lock)]
        let mut guard = self.child.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ref mut child) = *guard {
            let _ = child.kill().await;
            let _ = child.wait().await;
            tracing::info!("WhatsApp sidecar terminated");
        }
        *guard = None;
    }
}

pub struct AppState {
    pub db: SqlitePool,
    pub sessions: Arc<auth_session::SessionStore>,
    pub sync_worker: Arc<SyncWorker>,
    pub sidecar: SidecarHandle,
    /// Path to the sidecar's shared-secret token file (.sidecar_token).
    /// Written by the Node sidecar on startup; read by every HTTP command so
    /// requests pass the required X-Sidecar-Token auth header.
    pub wa_token_file: std::path::PathBuf,
    /// Embedded LAN hub server runtime (None = not running).
    pub hub: Arc<tokio::sync::Mutex<HubRuntime>>,
    /// Authenticated, in-flight Office AI requests keyed by a client-generated
    /// opaque request ID. The watch sender lets `ai_cancel_chat` drop the live
    /// provider/tool future instead of merely hiding its UI output.
    pub active_ai_chats: Arc<std::sync::Mutex<HashMap<String, ActiveAiChat>>>,
}

pub struct ActiveAiChat {
    pub user_id: String,
    pub branch_id: String,
    pub cancel: tokio::sync::watch::Sender<bool>,
}

pub type ActiveAiChats = Arc<std::sync::Mutex<HashMap<String, ActiveAiChat>>>;

#[derive(Default)]
pub struct HubRuntime {
    pub handle: Option<crate::hub::HubHandle>,
    pub last_error: Option<String>,
}

/// Bind the sidecar child process to a Windows Job Object with
/// JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE so that when ZanPOS exits — cleanly or
/// via crash / Task Manager kill — the OS closes the job handle and immediately
/// terminates every process in the job, including the sidecar.
///
/// Without this, Windows child processes are NOT automatically killed when
/// the parent exits (unlike Unix where the parent's exit sends SIGHUP to the
/// process group).  This makes the sidecar a strict subprocess: it cannot
/// exist outside ZanPOS.
#[cfg(target_os = "windows")]
fn bind_to_job_object(pid: u32) {
    use winapi::um::handleapi::CloseHandle;
    use winapi::um::jobapi2::{
        AssignProcessToJobObject, CreateJobObjectW, SetInformationJobObject,
    };
    use winapi::um::processthreadsapi::OpenProcess;
    use winapi::um::winnt::{
        JobObjectExtendedLimitInformation, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    unsafe {
        let job = CreateJobObjectW(std::ptr::null_mut(), std::ptr::null());
        if job.is_null() {
            tracing::warn!("WA sidecar job object: CreateJobObjectW failed");
            return;
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &mut info as *mut _ as *mut _,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        ) == 0
        {
            tracing::warn!("WA sidecar job object: SetInformationJobObject failed");
            CloseHandle(job);
            return;
        }
        // PROCESS_SET_QUOTA | PROCESS_TERMINATE — minimum for AssignProcessToJobObject
        let proc = OpenProcess(0x0001 | 0x0100, 0, pid);
        if proc.is_null() {
            tracing::warn!("WA sidecar job object: OpenProcess({}) failed", pid);
            CloseHandle(job);
            return;
        }
        if AssignProcessToJobObject(job, proc) == 0 {
            tracing::warn!("WA sidecar job object: AssignProcessToJobObject({}) failed — sidecar will not be auto-killed on exit", pid);
            CloseHandle(proc);
            CloseHandle(job);
            return;
        }
        CloseHandle(proc);
        // Intentionally keep `job` open — Windows HANDLE has no Rust Drop.
        // The handle stays open until ZanPOS exits, at which point the OS closes
        // it and kills every process in the job (including the sidecar).
        tracing::info!(
            "WA sidecar PID {} bound to job object (strict subprocess)",
            pid
        );
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Must install before Builder::default() so it also covers panics during
    // setup() itself — historically where the worst release crashes lived.
    diagnostics::install_panic_hook();
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
                let prepared = match db_recovery::prepare_existing_database(&app_data, &db_path).await {
                    Ok(prepared) => {
                        if let Some(dir) = &prepared.recovery_dir {
                            tracing::warn!(
                                "Database required recovery before startup. Original files copied to {}",
                                dir.display()
                            );
                        }
                        prepared
                    }
                    Err(e) => {
                        tracing::error!("Database recovery preparation failed: {e}");
                        db_recovery::PreparedDatabase {
                            effective_path: app_data.join("zanpos_v2.db"),
                            recovery_dir: None,
                        }
                    }
                };
                let effective_db_path = prepared.effective_path.to_string_lossy().to_string();

                // Try init. If it fails, copy every SQLite file to recovery before
                // clearing or falling back. A local-first POS must never silently
                // discard store data.
                match db::init_db(&effective_db_path).await {
                    Ok(pool) => {
                        if let Err(e) = db::repositories::auth_repo::rehash_plain_pins(&pool).await {
                            tracing::warn!("PIN rehash step failed: {:?}", e);
                        }
                        pool
                    }
                    Err(e) => {
                        tracing::error!("DB init failed: {}. Recovering files before retry.", e);
                        let recovered = db_recovery::recover_after_init_failure(
                            &app_data,
                            &prepared.effective_path,
                        )
                        .unwrap_or_else(|recovery_err| {
                            tracing::error!("DB recovery after init failure failed: {recovery_err}");
                            db_recovery::PreparedDatabase {
                                effective_path: app_data.join("zanpos_v2.db"),
                                recovery_dir: None,
                            }
                        });
                        if let Some(dir) = &recovered.recovery_dir {
                            tracing::warn!(
                                "Database files copied to recovery before retry: {}",
                                dir.display()
                            );
                        }
                        let retry_path = recovered.effective_path.to_string_lossy().to_string();
                        if recovered.effective_path != db_path {
                            tracing::warn!(
                                "Using fallback database path after recovery: {}",
                                recovered.effective_path.display()
                            );
                        }
                        db::init_db(&retry_path)
                            .await
                            .expect("Failed to initialize database after recovery")
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

            // One-time migration: Baileys v7 sessions are incompatible with the v6
            // creds the previous build wrote. On the first v7 run, wipe the old
            // session so the sidecar starts clean and the user re-pairs (surfaced by
            // the "upgrade required" banner in WhatsAppSection.tsx) instead of
            // looping on a bad session.
            {
                let v7_marker = wa_session_dir.join(".v7");
                if !v7_marker.exists() {
                    for entry in std::fs::read_dir(&wa_session_dir).into_iter().flatten().flatten() {
                        let p = entry.path();
                        let _ = std::fs::remove_file(&p).or_else(|_| std::fs::remove_dir_all(&p));
                    }
                    let _ = std::fs::write(&v7_marker, b"1");
                    tracing::info!("WhatsApp: cleared pre-v7 session for one-time re-pair");
                }
            }

            // Resolve the sidecar executable — try every known location so it works
            // in production (Tauri strips the triple-target suffix), dev (full suffix),
            // and any edge-case install layout.
            // The sidecar is now a Baileys v7 ESM app run by Node — pkg can't bundle
            // v7's ESM + native crypto module. Resolve the Node runtime and the
            // server.mjs entry point separately:
            //   • Prod: bundled node.exe + server.mjs under the app resource dir.
            //   • Dev:  system `node` on PATH + the repo's server.mjs.
            let exe_dir = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|p| p.to_path_buf()))
                .unwrap_or_default();
            let resource_dir = app.path().resource_dir().ok();

            let sidecar_script = {
                let mut candidates: Vec<std::path::PathBuf> = Vec::new();
                if let Some(rd) = &resource_dir {
                    // Production: bundled under resources/sidecar/whatsapp-sidecar/
                    candidates.push(rd.join("sidecar").join("whatsapp-sidecar").join("server.mjs"));
                    // Legacy flat layout fallback
                    candidates.push(rd.join("sidecar").join("server.mjs"));
                }
                candidates.push(exe_dir.join("sidecar").join("server.mjs"));
                // Dev: source tree path (CARGO_MANIFEST_DIR is compile-time only)
                candidates.push(
                    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                        .join("sidecar").join("whatsapp-sidecar").join("server.mjs"),
                );
                candidates.iter().find(|p| p.exists()).cloned()
                    .unwrap_or_else(|| exe_dir.join("sidecar").join("server.mjs"))
            };

            // Node executable: bundled node.exe (prod) else `node` on PATH (dev).
            let node_exe: std::ffi::OsString = {
                let mut candidates: Vec<std::path::PathBuf> = Vec::new();
                if let Some(rd) = &resource_dir {
                    // Production: bundled alongside server.mjs
                    candidates.push(rd.join("sidecar").join("whatsapp-sidecar").join("node.exe"));
                    // Legacy flat layout fallback
                    candidates.push(rd.join("sidecar").join("node.exe"));
                }
                candidates.push(exe_dir.join("node.exe"));
                candidates.into_iter().find(|p| p.exists())
                    .map(std::path::PathBuf::into_os_string)
                    .unwrap_or_else(|| std::ffi::OsString::from("node"))
            };

            /// Spawn the sidecar with up to `max_attempts` retries.
            /// `log_path` receives the sidecar's stdout + stderr so crashes are
            /// visible in logs/whatsapp-sidecar.log instead of disappearing silently.
            async fn spawn_sidecar(
                node: &std::ffi::OsStr,
                script: &std::path::Path,
                session_dir: &std::path::Path,
                log_path: &std::path::Path,
                max_attempts: u32,
            ) -> Option<tokio::process::Child> {
                // Log rotation: if log > 5 MB, rename to .1 before appending
                if let Ok(meta) = std::fs::metadata(log_path) {
                    if meta.len() > 5 * 1024 * 1024 {
                        let rotated = log_path.with_extension("log.1");
                        let _ = std::fs::remove_file(&rotated);
                        let _ = std::fs::rename(log_path, &rotated);
                    }
                }

                tracing::info!(
                    "[wa-sidecar] spawn: node={:?} script={:?} session_dir={:?}",
                    node, script, session_dir
                );

                for attempt in 1..=max_attempts {
                    let mut cmd = tokio::process::Command::new(node);

                    // On Windows, paths under "Program Files" contain spaces.
                    // Passing a path-with-spaces as a command-line argument is
                    // prone to Win32 quoting edge cases that cause Node to
                    // receive a bare drive letter (e.g. "C:") instead of the
                    // full path, crashing immediately with EISDIR.
                    //
                    // Fix: set CWD to the script's parent directory and pass
                    // only the filename — "server.mjs" has no spaces and
                    // requires no quoting.  ESM import.meta.url is resolved
                    // from the file's real location, not from CWD, so all
                    // relative imports inside the sidecar still work correctly.
                    if let Some(dir) = script.parent() {
                        cmd.current_dir(dir);
                        cmd.arg(script.file_name().unwrap_or(script.as_os_str()));
                    } else {
                        cmd.arg(script);
                    }
                    cmd.arg(format!("--session-dir={}", session_dir.to_string_lossy()));

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
                        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
                        cmd.creation_flags(CREATE_NO_WINDOW);
                    }

                    match cmd.spawn() {
                        Ok(child) => {
                            tracing::info!(
                                "WhatsApp sidecar started (pid {:?}, attempt {})",
                                child.id(), attempt
                            );
                            // Bind to a Job Object so the sidecar is killed if
                            // ZanPOS exits for any reason (clean close OR crash).
                            #[cfg(target_os = "windows")]
                            if let Some(pid) = child.id() {
                                bind_to_job_object(pid);
                            }
                            return Some(child);
                        }
                        Err(e) if attempt < max_attempts => {
                            tracing::warn!(
                                "WhatsApp sidecar start attempt {}/{} failed: {} — retrying in 1 s",
                                attempt, max_attempts, e
                            );
                            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
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

            /// Kill any process holding `port` on 127.0.0.1 so the sidecar always
            /// gets a clean start when the app opens or reopens.
            async fn kill_port(port: u16) {
                // Fast path: try to bind — if it works the port is already free.
                if tokio::net::TcpListener::bind(("127.0.0.1", port)).await.is_ok() {
                    return;
                }
                tracing::info!("Port {} occupied — clearing before sidecar start", port);

                #[cfg(target_os = "windows")]
                {
                    if let Ok(out) = tokio::process::Command::new("netstat")
                        .args(["-ano"])
                        .output()
                        .await
                    {
                        let text = String::from_utf8_lossy(&out.stdout);
                        let port_suffix = format!(":{}", port);
                        for line in text.lines() {
                            let fields: Vec<&str> = line.split_whitespace().collect();
                            // [Proto, LocalAddr, ForeignAddr, State, PID]
                            if fields.len() >= 5
                                && fields[3] == "LISTENING"
                                && fields[1].ends_with(&port_suffix)
                            {
                                if let Ok(pid) = fields[4].parse::<u32>() {
                                    tracing::info!("Killing PID {} holding port {}", pid, port);
                                    let _ = tokio::process::Command::new("taskkill")
                                        .args(["/F", "/PID", &pid.to_string()])
                                        .output()
                                        .await;
                                }
                            }
                        }
                    }
                }

                #[cfg(not(target_os = "windows"))]
                {
                    // Try fuser first (kills in one shot), then lsof as fallback.
                    let fuser_ok = tokio::process::Command::new("fuser")
                        .args(["-k", &format!("{}/tcp", port)])
                        .output()
                        .await
                        .map(|o| o.status.success())
                        .unwrap_or(false);
                    if !fuser_ok {
                        if let Ok(out) = tokio::process::Command::new("lsof")
                            .args(["-ti", &format!(":{}", port)])
                            .output()
                            .await
                        {
                            for pid_str in String::from_utf8_lossy(&out.stdout).split_whitespace() {
                                if let Ok(pid) = pid_str.parse::<u32>() {
                                    tracing::info!("Killing PID {} holding port {}", pid, port);
                                    let _ = tokio::process::Command::new("kill")
                                        .args(["-9", &pid.to_string()])
                                        .output()
                                        .await;
                                }
                            }
                        }
                    }
                }

                // Brief pause so the OS releases the port before we bind.
                tokio::time::sleep(tokio::time::Duration::from_millis(400)).await;
            }

            let wa_log_path = app_data.join("logs").join("whatsapp-sidecar.log");

            // Kill any leftover sidecar by PID file first (catches zombies that
            // crashed before binding port 3131, which kill_port() cannot see).
            {
                let pid_file = wa_session_dir.join(".sidecar.pid");
                if let Ok(raw) = std::fs::read_to_string(&pid_file) {
                    if let Ok(pid) = raw.trim().parse::<u32>() {
                        tracing::info!("Found stale sidecar PID {} — killing before restart", pid);
                        #[cfg(target_os = "windows")]
                        let _ = std::process::Command::new("taskkill")
                            .args(["/F", "/PID", &pid.to_string()])
                            .output();
                        #[cfg(not(target_os = "windows"))]
                        let _ = std::process::Command::new("kill")
                            .args(["-9", &pid.to_string()])
                            .output();
                    }
                    let _ = std::fs::remove_file(&pid_file);
                }
            }

            // Also clear port 3131 in case the old sidecar is still running
            // (e.g. job object not set up yet, or crash happened after binding).
            tauri::async_runtime::block_on(kill_port(3131));

            let initial_child = if sidecar_script.exists() {
                tauri::async_runtime::block_on(spawn_sidecar(&node_exe, &sidecar_script, &wa_session_dir, &wa_log_path, 3))
            } else {
                tracing::warn!(
                    "WhatsApp sidecar script not found at {:?} — watchdog will keep retrying",
                    sidecar_script
                );
                None
            };

            let wa_child: Arc<std::sync::Mutex<Option<tokio::process::Child>>> =
                Arc::new(std::sync::Mutex::new(initial_child));

            // ── Watchdog: keeps the sidecar alive for the entire app lifetime ─────────
            // Every 8 seconds, check if the process is still running.
            // If it has exited (crash, OOM, killed by Windows) — restart it immediately.
            let (shutdown_tx, mut shutdown_rx) = tokio::sync::oneshot::channel::<()>();
            {
                let wa_child_watch  = Arc::clone(&wa_child);
                let node_watch      = node_exe.clone();
                let script_watch    = sidecar_script.clone();
                let session_watch   = wa_session_dir.clone();
                let log_watch       = wa_log_path.clone();

                tauri::async_runtime::spawn(async move {
                    loop {
                        tokio::select! {
                            _ = tokio::time::sleep(tokio::time::Duration::from_secs(8)) => {},
                            _ = &mut shutdown_rx => {
                                tracing::info!("WhatsApp sidecar watchdog shutting down");
                                break;
                            }
                        }

                        let needs_restart = {
                            let mut guard = wa_child_watch
                                .lock()
                                .unwrap_or_else(|e| e.into_inner());

                            match guard.as_mut() {
                                // Process slot is empty — sidecar was never started or already
                                // removed after a previous exit.  Try again if the exe is present.
                                None => script_watch.exists(),

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
                            if script_watch.exists() {
                                if let Some(child) =
                                    spawn_sidecar(&node_watch, &script_watch, &session_watch, &log_watch, 3).await
                                {
                                    let mut guard = wa_child_watch
                                        .lock()
                                        .unwrap_or_else(|e| e.into_inner());
                                    *guard = Some(child);
                                }
                            } else {
                                tracing::warn!(
                                    "WhatsApp sidecar watchdog: script still not found at {:?}",
                                    script_watch
                                );
                            }
                        }
                    }
                });
            }

            let sidecar_handle = SidecarHandle {
                child: wa_child,
                shutdown_tx: Arc::new(std::sync::Mutex::new(Some(shutdown_tx))),
            };

            // ORDERING CONSTRAINT: app.manage() MUST be called before
            // .invoke_handler() is registered.  tauri::generate_handler![] builds
            // a static dispatch table at compile time, but the AppState value is
            // resolved at runtime from the managed state map.  If manage() were
            // called after invoke_handler(), any command that fires before manage()
            // completes would panic with "state not managed".  Keep this order.
            let wa_token_file = wa_session_dir.join(".sidecar_token");
            let pool_alerts = db.clone();
            let pool_ai_maintenance = db.clone();
            let pool_maint = db.clone();
            app.manage(AppState {
                db,
                sessions: Arc::new(auth_session::SessionStore::default()),
                sync_worker,
                sidecar: sidecar_handle,
                wa_token_file,
                hub: hub_runtime,
                active_ai_chats: Arc::new(std::sync::Mutex::new(HashMap::new())),
            });

            // ── Trust spine: panic/error diagnostics + telemetry uploader ─────────
            // active_device_id() needs the full AppState (not just the pool), so
            // this has to run after app.manage() above, not right after db::init_db.
            {
                let app_state = app.state::<AppState>();
                tauri::async_runtime::block_on(async {
                    let device_id = commands::sync_commands::active_device_id(&app_state)
                        .await
                        .unwrap_or_else(|_| "unknown".into());
                    diagnostics::set_device_id(device_id);
                    diagnostics::reconcile_pending(&app_state.db).await;
                    diagnostics::record_event(
                        &app_state.db,
                        "app_open",
                        Some(serde_json::json!({ "app_version": env!("CARGO_PKG_VERSION") })),
                    )
                    .await;
                });
                telemetry_uploader::spawn(app_state.db.clone());
                backup::spawn(app_state.db.clone(), app_data.join("zanpos.db"));
            }

            // ── Global emitter for deep-backend progress events ───────────────────
            crate::app_events::set_app_handle(app.handle().clone());

            // ── Proactive alert detection loop ────────────────────────────────────
            let app_handle_alerts = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                crate::ai::proactive::run_detection_loop(app_handle_alerts, pool_alerts).await;
            });
            tauri::async_runtime::spawn(async move {
                crate::ai::proactive::run_chat_maintenance_loop(pool_ai_maintenance).await;
            });

            // ── Scheduled auto-maintenance (24h cycle) ──────────────────────────────
            let app_handle_maint = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let db = pool_maint;
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(86_400)).await;
                    let now = Utc::now().to_rfc3339();
                    let mut alerts: Vec<serde_json::Value> = Vec::new();

                    match sqlx::query_scalar::<_, String>("PRAGMA quick_check")
                        .fetch_one(&db)
                        .await
                    {
                        Ok(result) if result != "ok" => {
                            tracing::warn!("Auto-maintenance: quick_check: {result}");
                            alerts.push(serde_json::json!({
                                "alert_id": format!("maint_quick_{}", now),
                                "severity": "warning",
                                "title": "Database integrity warning",
                                "detail": result,
                                "created_at": now,
                            }));
                        }
                        Err(e) => {
                            tracing::error!("Auto-maintenance: quick_check failed: {e}");
                        }
                        _ => {}
                    }

                    for table in &[
                        "products", "categories", "sales", "sale_items", "customers",
                        "users", "tax_rules", "suppliers", "purchase_orders",
                        "audit_logs", "refunds", "stock_levels",
                    ] {
                        if let Err(e) = sqlx::query(&format!("REINDEX \"{table}\""))
                            .execute(&db)
                            .await
                        {
                            tracing::error!("Auto-maintenance: REINDEX {table} failed: {e}");
                        }
                    }

                    if let Err(e) = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
                        .execute(&db)
                        .await
                    {
                        tracing::warn!("Auto-maintenance: wal_checkpoint failed: {e}");
                    }

                    if let Ok(rows) = sqlx::query("PRAGMA foreign_key_check")
                        .fetch_all(&db)
                        .await
                    {
                        if !rows.is_empty() {
                            tracing::warn!(
                                "Auto-maintenance: {} foreign key violation(s) found",
                                rows.len()
                            );
                            alerts.push(serde_json::json!({
                                "alert_id": format!("maint_fk_{}", now),
                                "severity": "warning",
                                "title": "Foreign key integrity",
                                "detail": format!("{} orphaned reference(s) found", rows.len()),
                                "created_at": now,
                            }));
                        }
                    }

                    for alert in &alerts {
                        if let Err(e) = app_handle_maint.emit("proactive-alert", alert) {
                            tracing::warn!("Auto-maintenance: failed to emit alert: {e}");
                        }
                    }

                    tracing::info!(
                        "Auto-maintenance cycle complete ({} alerts)",
                        alerts.len()
                    );
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Auth
            commands::auth_commands::auth_list_users,
            commands::auth_commands::auth_login_pin,
            commands::auth_commands::auth_logout,
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
            commands::admin_commands::admin_find_duplicate_products,
            commands::admin_commands::admin_merge_products,
            commands::admin_commands::admin_delete_product,
            commands::admin_commands::admin_list_categories,
            commands::admin_commands::admin_list_tax_rules,
            commands::admin_commands::admin_save_tax_rule,
            commands::admin_commands::admin_delete_tax_rule,
            commands::admin_commands::admin_save_category,
            commands::admin_commands::admin_list_users_all,
            commands::admin_commands::admin_list_roles,
            commands::admin_commands::admin_create_user,
            commands::admin_commands::admin_update_user,
            commands::admin_commands::admin_run_diagnostics,
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
            commands::report_commands::report_margin,
            commands::report_commands::report_product_margin,
            commands::report_commands::report_sales_list,
            commands::report_commands::report_by_cashier,
            commands::report_commands::report_eod_cashup,
            commands::report_commands::report_z_report,
            commands::report_commands::reports_config_load,
            commands::report_commands::reports_config_save,
            commands::report_commands::db_integrity_check,
            // Purchasing
            commands::purchasing_commands::supplier_list,
            commands::purchasing_commands::supplier_upsert,
            commands::purchasing_commands::supplier_delete,
            commands::purchasing_commands::po_list,
            commands::purchasing_commands::po_get,
            commands::purchasing_commands::po_create,
            commands::purchasing_commands::po_receive,
            commands::purchasing_commands::po_cancel,
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
            commands::setup_commands::operational_settings_load,
            commands::setup_commands::operational_settings_save,
            commands::setup_commands::onboarding_get_state,
            commands::setup_commands::onboarding_mark_step,
            // Licensing — advisory only; never gates the sale path
            license::license_get_entitlement,
            license::license_import_file,
            // Reprint queue — failed receipts, surfaced at the till and at EOD
            commands::reprint_queue::reprint_queue_pending,
            commands::reprint_queue::reprint_queue_mark_printed,
            // Hub per-device pairing
            commands::hub_pairing_commands::hub_pair_device,
            commands::hub_pairing_commands::hub_revoke_device,
            commands::hub_pairing_commands::hub_list_devices,
            // Off-site encrypted backup
            backup::backup_status,
            backup::backup_run_now,
            backup::backup_restore_file,
            // Public storefront
            commands::storefront_commands::storefront_status,
            commands::storefront_commands::storefront_settings_get,
            commands::storefront_commands::storefront_settings_save,
            commands::storefront_commands::storefront_products_list,
            commands::storefront_commands::storefront_product_update,
            commands::storefront_commands::storefront_publish,
            commands::storefront_commands::storefront_connection_test,
            commands::storefront_commands::qr::storefront_qr,
            commands::cloudflare_commands::storefront_cloudflare_connection_get,
            commands::cloudflare_commands::storefront_cloudflare_connect,
            commands::cloudflare_commands::storefront_cloudflare_select_account,
            commands::cloudflare_commands::storefront_cloudflare_disconnect,
            commands::cloudflare_commands::storefront_cloudflare_deploy,
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
            commands::sync_commands::hub_truth_compare,
            commands::sync_commands::hub_truth_pull,
            commands::sync_commands::sync_conflicts_list,
            commands::sync_commands::sync_conflict_resolve,
            commands::sync_commands::sync_stock_drift_report,
            commands::sync_commands::sync_stock_drift_reconcile,
            // Startup health
            commands::startup_commands::startup_health_check,
            commands::startup_commands::startup_restart_sidecar,
            // System health checkup
            commands::system_health_commands::system_health_check,
            commands::system_health_commands::system_health_apply_fix,
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
            // AI Admin — config
            commands::ai_admin_commands::admin_delete_provider,
            commands::ai_admin_commands::admin_set_anthropic_model,
            commands::ai_admin_commands::admin_get_ai_config,
            commands::ai_admin_commands::admin_save_ai_config,
            commands::ai_admin_commands::admin_get_feature_toggles,
            commands::ai_admin_commands::admin_save_feature_toggles,
            // AI Admin — chat
            commands::ai_admin_commands::ai_execute_action,
            commands::ai_admin_commands::ai_execute_batch_actions,
            commands::ai_admin_commands::ai_cancel_action,
            commands::ai_admin_commands::ai_undo_action,
            // AI Admin — bulk run engine
            commands::ai_admin_commands::ai_run_execute,
            commands::ai_admin_commands::ai_run_undo,
            commands::ai_admin_commands::ai_run_cancel,
            // AI Admin — streaming + history
            commands::ai_admin_commands::ai_chat_stream,
            commands::ai_admin_commands::ai_cancel_chat,
            commands::ai_admin_commands::ai_save_message,
            commands::ai_admin_commands::ai_load_history,
            commands::ai_admin_commands::ai_get_task_ledger_resume,
            commands::ai_admin_commands::ai_clear_history,
            commands::ai_admin_commands::ai_submit_feedback,
            // AI Admin — kill-switch (P0-04)
            commands::ai_admin_commands::admin_set_ai_enabled,
            commands::ai_admin_commands::admin_get_ai_enabled,
            // AI Admin — proactive alerts
            commands::ai_admin_commands::admin_get_alerts,
            commands::ai_admin_commands::admin_dismiss_alert,
            // AI Admin — usage summary
            commands::ai_admin_commands::ai_get_usage_summary,
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
            commands::whatsapp_inbox_commands::whatsapp_list_contacts,
            commands::whatsapp_inbox_commands::whatsapp_list_groups,
            commands::whatsapp_inbox_commands::whatsapp_set_targets,
            commands::whatsapp_inbox_commands::whatsapp_get_targets,
            commands::whatsapp_inbox_commands::whatsapp_poll_messages,
            commands::whatsapp_inbox_commands::whatsapp_list_messages,
            commands::whatsapp_inbox_commands::whatsapp_get_media,
            commands::whatsapp_inbox_commands::whatsapp_mark_read,
            commands::whatsapp_inbox_commands::whatsapp_mark_all_read,
            commands::whatsapp_inbox_commands::whatsapp_clear_messages,
            commands::whatsapp_catalog_commands::whatsapp_commerce_get_enabled,
            commands::whatsapp_catalog_commands::whatsapp_orders_get_enabled,
            commands::whatsapp_catalog_commands::whatsapp_commerce_set_enabled,
            commands::whatsapp_catalog_commands::whatsapp_order_list,
            commands::whatsapp_catalog_commands::whatsapp_order_update_status,
            commands::whatsapp_catalog_commands::whatsapp_order_match,
            commands::whatsapp_catalog_commands::whatsapp_order_message,
            commands::whatsapp_catalog_commands::whatsapp_send_product,
            // AI payment verification (WhatsApp screenshot → OCR → AI confirm)
            commands::payment_confirm_commands::payment_confirmations_list,
            commands::payment_confirm_commands::payment_confirmations_unseen_count,
            commands::payment_confirm_commands::payment_confirmations_mark_all_seen,
            commands::payment_confirm_commands::payment_confirmation_override,
            // Invoice / price-list photo → catalog update (review-first)
            commands::catalog_import_commands::catalog_import_extract,
            commands::catalog_import_commands::catalog_import_apply,
            // Product image picker
            commands::updater_commands::product_pick_image,
            // Auto-updater
            commands::updater_commands::check_for_updates,
            commands::updater_commands::download_and_install_update,
            commands::updater_commands::check_critical_update,
            // Trust spine — diagnostics & telemetry
            diagnostics::log_diagnostic,
            telemetry_uploader::flush_diagnostics_now,
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
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    // Prevent immediate close — we need to shut down the sidecar
                    // gracefully before the process exits.
                    api.prevent_close();
                    let w = window.clone();
                    let token_file = {
                        let state: tauri::State<'_, AppState> = window.state();
                        state.wa_token_file.clone()
                    };
                    let sidecar_child = {
                        let state: tauri::State<'_, AppState> = window.state();
                        state.sidecar.child.clone()
                    };
                    tauri::async_runtime::spawn(async move {
                        // Graceful HTTP shutdown — POST /shutdown to sidecar
                        let token = std::fs::read_to_string(&token_file)
                            .unwrap_or_default().trim().to_string();
                        if !token.is_empty() {
                            let client = reqwest::Client::builder()
                                .connect_timeout(std::time::Duration::from_secs(2))
                                .timeout(std::time::Duration::from_secs(3))
                                .build()
                                .unwrap_or_default();
                            let _ = client
                                .post("http://127.0.0.1:3131/shutdown")
                                .header("X-Sidecar-Token", &token)
                                .send()
                                .await;
                        }
                        // Failsafe: kill + reap
                        let mut child = {
                            let mut guard = sidecar_child
                                .lock()
                                .unwrap_or_else(|e| e.into_inner());
                            guard.take()
                        };
                        if let Some(ref mut c) = child {
                            let _ = c.kill().await;
                            let _ = c.wait().await;
                            tracing::info!("WhatsApp sidecar terminated");
                        }
                        // Now actually close the window
                        let _ = w.destroy();
                    });
                }
                tauri::WindowEvent::Destroyed => {
                    // Last-resort cleanup — only needed if CloseRequested didn't fire
                    // (e.g. process killed externally).
                    let sidecar = {
                        let state: tauri::State<'_, AppState> = window.state();
                        let mut guard = state.sidecar.child
                            .lock()
                            .unwrap_or_else(|e| e.into_inner());
                        guard.take()
                    };
                    if let Some(mut child) = sidecar {
                        // Can't run async in this sync context — best effort
                        let _ = child.start_kill();
                        tracing::info!("WhatsApp sidecar emergency-killed");
                    }
                }
                _ => {}
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
