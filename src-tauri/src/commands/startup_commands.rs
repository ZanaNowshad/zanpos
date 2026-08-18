use crate::AppState;
use serde::Serialize;
use tauri::{Manager, State};

#[derive(Debug, Clone, Serialize)]
pub struct StartupComponentStatus {
    pub component: String,
    pub status: String,
    pub message: String,
}

async fn probe_sidecar_health(token: &str) -> bool {
    let Ok(client) = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(2))
        .timeout(std::time::Duration::from_secs(3))
        .build()
    else {
        return false;
    };
    client
        .get("http://127.0.0.1:3131/health")
        .header("X-Sidecar-Token", token)
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false)
}

#[tauri::command]
pub async fn startup_health_check(
    state: State<'_, AppState>,
) -> Result<Vec<StartupComponentStatus>, String> {
    let mut items = Vec::new();

    items.push(StartupComponentStatus {
        component: "database".into(),
        status: "ok".into(),
        message: "SQLite loaded".into(),
    });

    let wa_token = std::fs::read_to_string(&state.wa_token_file)
        .unwrap_or_default()
        .trim()
        .to_string();
    if wa_token.is_empty() {
        items.push(StartupComponentStatus {
            component: "whatsapp_sidecar".into(),
            status: "starting".into(),
            message: "Waiting for bridge…".into(),
        });
    } else if probe_sidecar_health(&wa_token).await {
        items.push(StartupComponentStatus {
            component: "whatsapp_sidecar".into(),
            status: "ok".into(),
            message: "WhatsApp bridge ready".into(),
        });
    } else {
        items.push(StartupComponentStatus {
            component: "whatsapp_sidecar".into(),
            status: "starting".into(),
            message: "Starting WhatsApp bridge…".into(),
        });
    }

    let hub = state.hub.lock().await;
    let hub_mode: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key='hub_mode'")
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();

    if hub_mode.as_deref() == Some("1") {
        if hub.handle.is_some() {
            items.push(StartupComponentStatus {
                component: "hub_server".into(),
                status: "ok".into(),
                message: "Hub server running".into(),
            });
        } else {
            items.push(StartupComponentStatus {
                component: "hub_server".into(),
                status: "error".into(),
                message: hub
                    .last_error
                    .clone()
                    .unwrap_or_else(|| "Hub failed to start".into()),
            });
        }
    }

    Ok(items)
}

#[tauri::command]
pub async fn startup_restart_sidecar(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let old_child = {
        state
            .sidecar
            .child
            .lock()
            .map_err(|e| e.to_string())?
            .take()
    };
    if let Some(mut old_child) = old_child {
        let _ = old_child.kill().await;
        let _ = old_child.wait().await;
    }

    let app_data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let wa_session_dir = app_data.join("wa-session");

    // Same resolver startup uses. Resolving separately here meant an installed
    // build searched only next to the executable, never the resource directory
    // the installer actually writes to — so this command killed a working
    // sidecar and then could not respawn it.
    let node_exe = crate::sidecar_paths::node(&app);
    let script = crate::sidecar_paths::script(&app).ok_or("sidecar script not found")?;

    let mut cmd = tokio::process::Command::new(&node_exe);
    if let Some(dir) = script.parent() {
        cmd.current_dir(dir);
        cmd.arg(script.file_name().unwrap_or(script.as_os_str()));
    } else {
        cmd.arg(&script);
    }
    cmd.arg(format!(
        "--session-dir={}",
        wa_session_dir.to_string_lossy()
    ));
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::null());

    #[cfg(target_os = "windows")]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let child = cmd.spawn().map_err(|e| format!("spawn failed: {}", e))?;

    state
        .sidecar
        .child
        .lock()
        .map_err(|e| e.to_string())?
        .replace(child);

    let wa_token = std::fs::read_to_string(&state.wa_token_file)
        .unwrap_or_default()
        .trim()
        .to_string();
    if wa_token.is_empty() {
        return Ok(false);
    }

    for _ in 0..12 {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        if probe_sidecar_health(&wa_token).await {
            return Ok(true);
        }
    }

    Ok(false)
}
