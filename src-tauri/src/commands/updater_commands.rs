/// Auto-updater check command.
/// Uses tauri-plugin-updater when available; gracefully returns None if not configured.
use crate::commands::rbac;
use crate::db::repositories::ai_admin_repo;
use crate::errors::AppError;
use crate::AppState;
use tauri::State;

/// Which manifest this terminal asks, and why it is not simply the compiled one.
///
/// The endpoint in `tauri.conf.json` is baked into the binary, so changing it
/// means reinstalling on every till by hand. That is not hypothetical: the
/// endpoint shipped in the field pointed at a domain that was never registered,
/// which is why no installed terminal could update itself at all and why the
/// only fix was to visit each machine.
///
/// `update_endpoint` in `app_config` overrides it, and that key syncs, so the
/// owner sets it once on the hub and every till follows. The difference is
/// between a bad URL costing one edit and costing a walk round the shop.
///
/// A blank or whitespace override falls through to the compiled value rather
/// than producing an empty URL — the failure mode of a half-filled settings box
/// should be "unchanged", not "no updates ever again".
async fn manifest_url(app: &tauri::AppHandle, state: &AppState) -> String {
    if let Ok(Some(configured)) = ai_admin_repo::get_config(&state.db, "update_endpoint").await {
        let trimmed = configured.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    compiled_endpoint(app)
}

/// The endpoint the build was compiled with, read from the Tauri config rather
/// than repeated as a constant here. Two copies of a URL is one copy that can
/// drift, and this one would drift silently.
fn compiled_endpoint(app: &tauri::AppHandle) -> String {
    app.config()
        .plugins
        .0
        .get("updater")
        .and_then(|updater| updater.get("endpoints"))
        .and_then(|endpoints| endpoints.as_array())
        .and_then(|list| list.first())
        .and_then(|first| first.as_str())
        .unwrap_or_default()
        .to_string()
}

/// An updater pointed at whichever manifest this terminal should be reading.
///
/// Both the check and the install go through here, so they can never disagree
/// about where the truth is — a check that finds an update at one URL and an
/// install that fetches from another is the kind of split that only shows up in
/// a shop.
async fn build_updater(
    app: &tauri::AppHandle,
    state: &AppState,
) -> Result<tauri_plugin_updater::Updater, AppError> {
    use tauri_plugin_updater::UpdaterExt;

    let url = manifest_url(app, state).await;
    let mut builder = app.updater_builder();
    if let Ok(parsed) = url.parse() {
        // A malformed override must not disable updating. Falling back to a
        // fresh builder leaves the compiled endpoint in play, so a mistyped
        // settings box stays recoverable from the hub — whereas failing here
        // would need another visit to the machine, which is the entire problem
        // this override exists to avoid.
        //
        // `endpoints()` takes `self`, so the builder is consumed even when it
        // rejects the list; there is nothing to put back and a new one is built.
        builder = match builder.endpoints(vec![parsed]) {
            Ok(next) => next,
            Err(e) => {
                tracing::warn!("Ignoring update endpoint {url}: {e}");
                app.updater_builder()
            }
        };
    } else {
        tracing::warn!("Ignoring unparseable update endpoint {url}");
    }
    builder
        .build()
        .map_err(|e| AppError::Internal(format!("Updater not configured: {e}")))
}

/// Check for available application updates.
/// Returns the new version string if one is available, or None if already up to date.
/// Network errors and missing configuration are treated as "no update available".
#[tauri::command]
pub async fn check_for_updates(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<String>, AppError> {
    let updater = build_updater(&app, &state).await?;

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
    session_token: String,
    state: State<'_, AppState>,
) -> Result<bool, AppError> {
    rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::OWNER_ONLY).await?;

    let updater = build_updater(&app, &state).await?;

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

/// Defensive, unsigned peek at the very same manifest the verified
/// tauri-updater plugin reads, just to surface an advisory `critical` flag the
/// plugin doesn't expose. This deliberately bypasses signature verification —
/// treat the result as a UI-urgency hint only.
/// Every failure mode (timeout, connection error, non-2xx, malformed JSON,
/// missing/wrong-typed fields) resolves to `Ok(None)`, never `Err`, and this
/// must never be wired into anything that installs code — the actual
/// update stays exclusively on the signed check_for_updates /
/// download_and_install_update path.
///
/// The URL now comes from [`manifest_url`] rather than being built here. It used
/// to compose `releases.zanpos.app/{target}/{arch}/{version}` by hand, which was
/// wrong twice over once the manifest became a static file: a dead domain, and a
/// path shape that no longer exists. Two places deciding where the manifest
/// lives is one place that goes stale, and this is the one that did.
#[tauri::command]
pub async fn check_critical_update(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<CriticalUpdateInfo>, String> {
    let url = manifest_url(&app, &state).await;
    if url.is_empty() {
        return Ok(None);
    }

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
