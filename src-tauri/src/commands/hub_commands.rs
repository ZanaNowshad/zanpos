use crate::commands::rbac;
use crate::db::repositories::ai_admin_repo;
use crate::errors::AppError;
use crate::AppState;
use rand::RngCore;
use serde::Serialize;
use sqlx::Row;
use std::collections::HashMap;
use tauri::State;

#[derive(Serialize)]
pub struct HubStatus {
    pub mode: String,
    pub running: bool,
    pub port: u16,
    pub lan_ips: Vec<String>,
    pub token: Option<String>,
    pub hub_url: Option<String>,
    pub last_error: Option<String>,
    pub terminals: Vec<TerminalSeen>,
}

#[derive(Serialize)]
pub struct TerminalSeen {
    pub device_id: String,
    pub ip: String,
    pub last_seen: String,
}

pub(crate) async fn read_cfg(pool: &sqlx::SqlitePool, key: &str) -> Option<String> {
    ai_admin_repo::get_config(pool, key)
        .await
        .ok()
        .flatten()
        .filter(|v| !v.is_empty())
}

#[tauri::command]
pub async fn hub_status(
    session_token: String,
    state: State<'_, AppState>,
) -> Result<HubStatus, AppError> {
    rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::ANY_ROLE).await?;
    let hub_mode = read_cfg(&state.db, "hub_mode").await.as_deref() == Some("1");
    let hub_url = read_cfg(&state.db, "hub_url").await;
    let port = read_cfg(&state.db, "hub_port")
        .await
        .and_then(|v| v.parse().ok())
        .unwrap_or(8923);
    let rt = state.hub.lock().await;
    let running = rt.handle.is_some();
    let last_error = rt.last_error.clone();
    drop(rt);
    // Derived from the devices table rather than the in-memory seen map alone:
    // the map resets when the hub restarts, which made every terminal look
    // like it had never contacted this hub until the next beat arrived. The
    // in-memory entries still overlay it so a beat recorded in the last few
    // seconds is not hidden by a stale read.
    let terminals = if hub_mode {
        let mut seen: HashMap<String, TerminalSeen> = sqlx::query(
            "SELECT device_id, observed_ip, last_seen_at FROM devices
              WHERE last_seen_at IS NOT NULL AND observed_ip IS NOT NULL
                AND deleted_at IS NULL",
        )
        .fetch_all(&state.db)
        .await
        .map(|rows| {
            rows.iter()
                .map(|row| {
                    let device_id: String = row.get("device_id");
                    (
                        device_id.clone(),
                        TerminalSeen {
                            device_id,
                            ip: row.get("observed_ip"),
                            last_seen: row.get("last_seen_at"),
                        },
                    )
                })
                .collect()
        })
        .unwrap_or_default();
        let rt = state.hub.lock().await;
        if let Some(h) = rt.handle.as_ref() {
            if let Ok(m) = h.seen.lock() {
                for (device_id, (ip, ts)) in m.iter() {
                    seen.entry(device_id.clone())
                        .or_insert_with(|| TerminalSeen {
                            device_id: device_id.clone(),
                            ip: ip.clone(),
                            last_seen: ts.clone(),
                        });
                }
            }
        }
        drop(rt);
        seen.into_values().collect()
    } else {
        Vec::new()
    };
    Ok(HubStatus {
        mode: if hub_mode {
            "hub"
        } else if hub_url.is_some() {
            "terminal"
        } else {
            "standalone"
        }
        .into(),
        running,
        port,
        lan_ips: crate::hub::lan_ips(),
        token: if hub_mode {
            crate::secure_store::get_secret("hub_store_token")
        } else {
            None
        },
        hub_url,
        last_error,
        terminals,
    })
}

#[tauri::command]
pub async fn hub_enable(
    session_token: String,
    port: Option<u16>,
    state: State<'_, AppState>,
) -> Result<HubStatus, AppError> {
    rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::OWNER_ONLY).await?;
    if read_cfg(&state.db, "hub_url").await.is_some() {
        return Err(AppError::Validation(
            "This device is joined to another hub. A terminal cannot become a hub.".into(),
        ));
    }
    let port = port.unwrap_or(8923);

    let token = match crate::secure_store::get_secret("hub_store_token") {
        Some(t) if !t.is_empty() => t,
        _ => {
            let mut bytes = [0u8; 32];
            rand::rngs::OsRng.fill_bytes(&mut bytes);
            let t = hex::encode(bytes);
            if !crate::secure_store::set_secret("hub_store_token", &t) {
                return Err(AppError::Internal(
                    "Windows Credential Manager unavailable — cannot store the hub token securely."
                        .into(),
                ));
            }
            t
        }
    };

    let now = chrono::Utc::now().to_rfc3339();
    for (k, v) in [("hub_mode", "1"), ("hub_port", &port.to_string())] {
        sqlx::query("INSERT INTO app_config(key,value,updated_at) VALUES (?,?,?)
                     ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at")
            .bind(k).bind(v).bind(&now).execute(&state.db).await?;
    }

    let mut rt = state.hub.lock().await;
    if let Some(h) = rt.handle.take() {
        h.shutdown();
    }
    match crate::hub::start_hub(state.db.clone(), port, &token).await {
        Ok(h) => {
            rt.handle = Some(h);
            rt.last_error = None;
        }
        Err(e) => {
            rt.last_error = Some(e.to_string());
            return Err(AppError::Internal(format!(
                "Hub could not start on port {port}: {e}. Is the port in use?"
            )));
        }
    }
    drop(rt);
    hub_status(session_token, state).await
}

#[tauri::command]
pub async fn hub_regenerate_token(
    session_token: String,
    state: State<'_, AppState>,
) -> Result<HubStatus, AppError> {
    rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::OWNER_ONLY).await?;
    crate::secure_store::delete_secret("hub_store_token");
    hub_enable(session_token, None, state).await
}

// ── mDNS LAN hub discovery ─────────────────────────────────────────

#[derive(Serialize, Clone)]
pub struct DiscoveredHubDto {
    pub instance_id: String,
    pub host: String,
    pub port: u16,
    pub protocol_version: u16,
    pub branch: String,
    pub pairing_enabled: bool,
    pub tls_fingerprint: Option<String>,
    pub last_seen_secs: u64,
}

#[tauri::command]
pub async fn list_discovered_hubs(
    state: State<'_, AppState>,
) -> Result<Vec<DiscoveredHubDto>, String> {
    let hubs = state.hub_discovery.discover();
    Ok(hubs
        .iter()
        .map(|hub| DiscoveredHubDto {
            instance_id: hub.instance_id.clone(),
            host: hub.host.to_string(),
            port: hub.port,
            protocol_version: hub.protocol_version,
            branch: hub.branch.clone(),
            pairing_enabled: hub.pairing_enabled,
            tls_fingerprint: hub.tls_fingerprint.clone(),
            last_seen_secs: hub.last_seen_secs,
        })
        .collect())
}

#[tauri::command]
pub async fn start_lan_discovery(state: State<'_, AppState>) -> Result<(), String> {
    state.hub_discovery.start()
}

#[tauri::command]
pub async fn stop_lan_discovery(state: State<'_, AppState>) -> Result<(), String> {
    state.hub_discovery.stop();
    Ok(())
}

mod tests {
    #[test]
    fn terminal_join_requires_the_same_schema_as_the_hub() {
        assert!(crate::commands::hub_join_commands::join_schema_compatible(
            30, 30
        ));
        assert!(!crate::commands::hub_join_commands::join_schema_compatible(
            30, 29
        ));
        assert!(!crate::commands::hub_join_commands::join_schema_compatible(
            29, 30
        ));
    }
}
