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
            // restart() diverges (-> !); it satisfies the Result return type.
            app.restart();
        }
        Ok(None) => Ok(false),
        Err(e) => Err(AppError::Internal(format!("Update check failed: {e}"))),
    }
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
