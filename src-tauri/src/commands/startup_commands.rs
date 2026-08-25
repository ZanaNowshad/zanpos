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
    let wa_log_path = app_data.join("logs").join("whatsapp-sidecar.log");

    // Startup's own launcher, not a copy of it. Every difference between the
    // two used to be a defect: no log to read when a restart failed, no session
    // directory, no retry when a scanner briefly held node.exe, and no job
    // object — which left a restarted sidecar able to outlive the app.
    let node_exe = crate::sidecar_paths::node(&app);
    let script = crate::sidecar_paths::script(&app).ok_or("sidecar script not found")?;
    let child = crate::sidecar_paths::spawn(&node_exe, &script, &wa_session_dir, &wa_log_path, 3)
        .await
        .ok_or("sidecar failed to start")?;

    state
        .sidecar
        .child
        .lock()
        .map_err(|e| e.to_string())?
        .replace(child);

    // Re-read the token on every attempt, never once up front.
    //
    // The sidecar mints a fresh random token at each start (server.mjs) and
    // writes it to this file. Reading it immediately after spawning therefore
    // picks up the *previous* process's token, every probe with it comes back
    // 401, and after twenty-four seconds the command reports failure for a
    // sidecar that came up perfectly — which is what made Restart look broken.
    for _ in 0..12 {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        let token = std::fs::read_to_string(&state.wa_token_file)
            .unwrap_or_default()
            .trim()
            .to_string();
        if token.is_empty() {
            continue; // not written yet — it is still starting
        }
        if probe_sidecar_health(&token).await {
            return Ok(true);
        }
    }

    Ok(false)
}
