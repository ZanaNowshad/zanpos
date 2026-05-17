/// Auto-updater check command.
/// Uses tauri-plugin-updater when available; gracefully returns None if not configured.
use crate::errors::AppError;

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
