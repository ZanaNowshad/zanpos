//! Global app-event emitter for backend → frontend push notifications that
//! originate deep inside tool execution (where no `AppHandle` is threaded).
//!
//! Set once during Tauri `.setup()`; safe no-op before that (e.g. unit tests).

use std::sync::OnceLock;

static APP_HANDLE: OnceLock<tauri::AppHandle> = OnceLock::new();

pub fn set_app_handle(handle: tauri::AppHandle) {
    let _ = APP_HANDLE.set(handle);
}

/// Emit per-row progress for long-running bulk operations. The frontend
/// listens on "bulk-progress" and renders a progress bar in the chat panel.
pub fn emit_bulk_progress(tool: &str, done: u64, total: u64) {
    if let Some(handle) = APP_HANDLE.get() {
        use tauri::Emitter;
        let _ = handle.emit(
            "bulk-progress",
            serde_json::json!({ "tool": tool, "done": done, "total": total }),
        );
    }
}
