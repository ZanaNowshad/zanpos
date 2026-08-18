//! Open the operating system's on-screen keyboard.
//!
//! A till is a touchscreen with no physical keyboard attached. The dialpad in
//! the payment modal covers digits, but the delivery journey also asks for a
//! road name and a customer name, and those need letters.
//!
//! Windows ships two on-screen keyboards and they are not interchangeable:
//!
//! * `TabTip.exe` — the *touch keyboard*, docked at the bottom of the screen,
//!   with the large keys a cashier expects. This is the one to prefer. It
//!   depends on the Touch Keyboard and Handwriting Panel Service, which is
//!   present on Windows 10/11 but can be disabled on a locked-down machine.
//! * `osk.exe` — the *accessibility* On-Screen Keyboard, a floating resizable
//!   window. Always present in System32, always works, looks less at home on a
//!   till but is a dependable fallback.
//!
//! Microsoft's own documented way to raise TabTip is the `ITipInvocation` COM
//! interface. Reaching it means either taking on the `windows` crate or
//! hand-rolling COM vtables through raw FFI; against one button, neither earns
//! its keep in a binary built with fat LTO for installer size. Launching the
//! executable is the documented-enough path and degrades to `osk.exe` when it
//! does not take.

use crate::errors::{AppError, AppResult};

/// Raise the system on-screen keyboard. Returns the name of whichever keyboard
/// was started so the caller can say something specific when it is not the one
/// the operator expected.
#[tauri::command]
pub async fn system_keyboard_open() -> Result<String, String> {
    tokio::task::spawn_blocking(open_system_keyboard)
        .await
        .map_err(|e| format!("On-screen keyboard task failed: {e}"))?
        .map_err(|e| e.to_string())
}

#[cfg(target_os = "windows")]
fn open_system_keyboard() -> AppResult<String> {
    use std::os::windows::process::CommandExt;
    use std::path::PathBuf;

    // Detaching keeps the keyboard alive past this call and stops a console
    // window flashing over the till, the same treatment the sidecar gets.
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const DETACHED_PROCESS: u32 = 0x0000_0008;

    let common_files = std::env::var("CommonProgramFiles")
        .unwrap_or_else(|_| r"C:\Program Files\Common Files".to_string());
    let tab_tip = PathBuf::from(common_files)
        .join("microsoft shared")
        .join("ink")
        .join("TabTip.exe");

    if tab_tip.is_file()
        && std::process::Command::new(&tab_tip)
            .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
            .spawn()
            .is_ok()
    {
        return Ok("touch".to_string());
    }

    // `osk.exe` is resolved through PATH rather than a hard-coded System32
    // path: on 64-bit Windows a 32-bit process is redirected to SysWOW64,
    // which has no copy of it, and the redirected path fails confusingly.
    std::process::Command::new("osk.exe")
        .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
        .spawn()
        .map(|_| "osk".to_string())
        .map_err(|e| {
            AppError::Internal(format!(
                "Could not open the on-screen keyboard. Windows reported: {e}"
            ))
        })
}

#[cfg(not(target_os = "windows"))]
fn open_system_keyboard() -> AppResult<String> {
    Err(AppError::Validation(
        "The on-screen keyboard is only available on Windows.".to_string(),
    ))
}
