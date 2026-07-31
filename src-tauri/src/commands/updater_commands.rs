/// Auto-updater check command.
/// Uses tauri-plugin-updater when available; gracefully returns None if not configured.
use crate::commands::rbac;
use crate::errors::AppError;
use crate::AppState;
use tauri::State;

/// Check for available application updates.
/// Returns the new version string if one is available, or None if already up to date.
/// Network errors and missing configuration are treated as "no update available".
#[tauri::command]
pub async fn check_for_updates(app: tauri::AppHandle) -> Result<Option<String>, AppError> {
    use tauri_plugin_updater::UpdaterExt;

    let updater = app
        .updater()
        .map_err(|e| AppError::Internal(format!("Updater not configured: {e}")))?;

    match updater.check().await {
        Ok(Some(update)) => Ok(Some(update.version.clone())),
        Ok(None) => Ok(None),
        Err(e) => {
            // Network errors or missing endpoint are non-fatal — treat as "no update"
            tracing::warn!("Update check failed (non-fatal): {}", e);
            Ok(None)
        }
    }
}

/// Download and install the available update, then restart the app to apply it.
/// Returns Ok(false) if no update is available; on success the app restarts and
/// this call does not return.
#[tauri::command]
pub async fn download_and_install_update(
    app: tauri::AppHandle,
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<bool, AppError> {
    rbac::owner_only(&state.db, &actor_user_id).await?;
    use tauri_plugin_updater::UpdaterExt;

    let updater = app
        .updater()
        .map_err(|e| AppError::Internal(format!("Updater not configured: {e}")))?;

    match updater.check().await {
        Ok(Some(update)) => {
            tracing::info!("Installing update v{}…", update.version);
            update
                .download_and_install(|_chunk, _total| {}, || {})
                .await
                .map_err(|e| AppError::Internal(format!("Update install failed: {e}")))?;
            tracing::info!("Update installed — restarting to apply.");
            // Recorded BEFORE restart(): it diverges, so anything after it
            // never runs. The row is written locally and uploaded on the next
            // launch, which is also the first launch of the new version.
            crate::diagnostics::record_event(
                &state.db,
                "update_applied",
                Some(serde_json::json!({
                    "to_version": update.version,
                    "from_version": env!("CARGO_PKG_VERSION"),
                })),
            )
            .await;
            // restart() diverges (-> !); it satisfies the Result return type.
            app.restart();
        }
        Ok(None) => Ok(false),
        Err(e) => Err(AppError::Internal(format!("Update check failed: {e}"))),
    }
}

#[derive(serde::Serialize)]
pub struct CriticalUpdateInfo {
    pub version: String,
    pub critical: bool,
    pub notes: Option<String>,
}

/// Defensive, unsigned peek at the same releases.zanpos.app manifest the
/// verified tauri-updater plugin already reads, just to surface an advisory
/// `critical` flag the plugin doesn't expose. This deliberately bypasses
/// signature verification — treat the result as a UI-urgency hint only.
/// Every failure mode (timeout, connection error, non-2xx, malformed JSON,
/// missing/wrong-typed fields) resolves to `Ok(None)`, never `Err`, and this
/// must never be wired into anything that installs code — the actual
/// update stays exclusively on the signed check_for_updates /
/// download_and_install_update path.
#[tauri::command]
pub async fn check_critical_update() -> Result<Option<CriticalUpdateInfo>, String> {
    let target = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "darwin"
    } else {
        "linux"
    };
    let arch = std::env::consts::ARCH;
    let url = format!(
        "https://releases.zanpos.app/{}/{}/{}",
        target,
        arch,
        env!("CARGO_PKG_VERSION")
    );

    let Ok(client) = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
    else {
        return Ok(None);
    };
    let resp = match client.get(&url).send().await {
        Ok(r) if r.status().is_success() => r,
        _ => return Ok(None),
    };
    let json: serde_json::Value = match resp.json().await {
        Ok(j) => j,
        Err(_) => return Ok(None),
    };
    let Some(version) = json.get("version").and_then(|v| v.as_str()) else {
        return Ok(None);
    };
    let Some(critical) = json.get("critical").and_then(|v| v.as_bool()) else {
        return Ok(None);
    };
    Ok(Some(CriticalUpdateInfo {
        version: version.to_string(),
        critical,
        notes: json.get("notes").and_then(|v| v.as_str()).map(String::from),
    }))
}

/// Open the image file picker dialog and return the selected path.
/// Returns None if the user cancels.
#[tauri::command]
pub async fn product_pick_image(app: tauri::AppHandle) -> Result<Option<String>, AppError> {
    use tauri_plugin_dialog::DialogExt;

    let path = app
        .dialog()
        .file()
        .add_filter("Images", &["png", "jpg", "jpeg", "webp", "gif", "bmp"])
        .blocking_pick_file();

    match path {
        Some(file_path) => {
            let path_str = file_path.to_string();
            Ok(Some(path_str))
        }
        None => Ok(None),
    }
}
