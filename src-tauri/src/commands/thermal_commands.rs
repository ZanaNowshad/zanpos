use crate::commands::rbac;
use crate::errors::{AppError, AppResult};
use crate::AppState;
use serde::{Deserialize, Serialize};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
/// Thermal/ESC-POS printer configuration and printing.
/// Uses the `serialport` crate directly (synchronous I/O on a blocking thread).
use tauri::State;

// ─── M7: Printer list cache (30s TTL) ────────────────────────────────────────
// list_windows_printers() spawns PowerShell (500ms-2s per call). Cache with
// a 30-second TTL so repeated calls from the settings UI are instant.

struct PrinterCache {
    entries: Vec<PortEntry>,
    refreshed_at: Instant,
}

static PRINTER_CACHE: OnceLock<Mutex<Option<PrinterCache>>> = OnceLock::new();
const PRINTER_CACHE_TTL: Duration = Duration::from_secs(30);

fn get_printer_cache() -> &'static Mutex<Option<PrinterCache>> {
    PRINTER_CACHE.get_or_init(|| Mutex::new(None))
}

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ThermalConfig {
    pub enabled: bool,
    pub port: String,
    pub baud: String,
}

#[derive(Deserialize)]
pub struct ThermalConfigInput {
    pub enabled: bool,
    pub port: String,
    pub baud: String,
}

// ─── ESC/POS byte constants ───────────────────────────────────────────────────

const ESC: u8 = 0x1B;
const GS: u8 = 0x1D;
const LF: u8 = 0x0A;

/// Initialize printer (ESC @)
fn esc_init() -> Vec<u8> {
    vec![ESC, b'@']
}

/// Select alignment: 0=left, 1=center, 2=right (ESC a n)
fn esc_align(n: u8) -> Vec<u8> {
    vec![ESC, b'a', n]
}

/// Bold on/off (ESC E n)
fn esc_bold(on: bool) -> Vec<u8> {
    vec![ESC, b'E', if on { 1 } else { 0 }]
}

/// Double-size text on/off (GS ! n, 0x11 = double height+width)
fn esc_double(on: bool) -> Vec<u8> {
    vec![GS, b'!', if on { 0x11 } else { 0x00 }]
}

/// Feed n lines and cut (GS V 0 = full cut)
fn esc_feed_and_cut(lines: u8) -> Vec<u8> {
    vec![ESC, b'd', lines, GS, b'V', 0x00]
}

/// Maximum number of receipt lines that will be printed before truncation.
/// Each line is ~40 bytes, so 500 lines ≈ 20 KB — well within a thermal printer's
/// typical 64 KB receive buffer.  Caps prevent buffer-overflow garbling for
/// pathological carts (e.g. 28k items).
const MAX_RECEIPT_LINES: usize = 500;

/// Build ESC/POS receipt byte payload.
/// `store_name`, `header_lines`, `item_lines`, `footer_lines` are all pre-formatted strings.
///
/// Lines beyond `MAX_RECEIPT_LINES` are dropped and a truncation notice is appended
/// so the operator sees a warning rather than silent data loss.
pub fn build_receipt_bytes(store_name: &str, receipt_lines: &[String]) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::with_capacity(512);

    // Initialize
    buf.extend_from_slice(&esc_init());

    // Store name — centered, double-size bold
    buf.extend_from_slice(&esc_align(1)); // center
    buf.extend_from_slice(&esc_double(true));
    buf.extend_from_slice(&esc_bold(true));
    buf.extend_from_slice(store_name.as_bytes());
    buf.push(LF);
    buf.extend_from_slice(&esc_double(false));
    buf.extend_from_slice(&esc_bold(false));
    buf.push(LF);

    // Body lines — left aligned
    let truncated = receipt_lines.len() > MAX_RECEIPT_LINES;
    let to_print = if truncated {
        &receipt_lines[..MAX_RECEIPT_LINES]
    } else {
        receipt_lines
    };
    buf.extend_from_slice(&esc_align(0)); // left
    for line in to_print {
        buf.extend_from_slice(line.as_bytes());
        buf.push(LF);
    }
    if truncated {
        let notice = format!(
            "... ({} lines omitted — receipt too long)",
            receipt_lines.len() - MAX_RECEIPT_LINES
        );
        buf.extend_from_slice(notice.as_bytes());
        buf.push(LF);
    }

    // Feed and cut
    buf.extend_from_slice(&esc_feed_and_cut(4));
    buf
}

/// Build a short test-page payload.
fn build_test_bytes(port: &str, baud: &str) -> Vec<u8> {
    let lines = vec![
        "--------------------------------".to_string(),
        "   ESC/POS TEST PAGE".to_string(),
        "--------------------------------".to_string(),
        format!("Port: {}  Baud: {}", port, baud),
        "".to_string(),
        "Left-aligned text".to_string(),
    ];

    let mut buf = esc_init();
    buf.extend_from_slice(&esc_align(1));
    buf.extend_from_slice(&esc_bold(true));
    buf.extend_from_slice(b"TEST PRINT");
    buf.push(LF);
    buf.extend_from_slice(&esc_bold(false));
    buf.push(LF);
    buf.extend_from_slice(&esc_align(0));
    for line in &lines {
        buf.extend_from_slice(line.as_bytes());
        buf.push(LF);
    }
    buf.extend_from_slice(&esc_feed_and_cut(3));
    buf
}

/// Route raw bytes to either a serial COM port or a Windows-named printer.
///
/// Routing rule (Windows):
///   • Port name starts with "COM" (case-insensitive) → serial
///   • Anything else → Windows print spooler RAW job
///
/// On non-Windows platforms only serial is supported.
pub fn write_to_port(port_name: &str, baud: u32, payload: Vec<u8>) -> AppResult<()> {
    #[cfg(windows)]
    {
        if !port_name.trim().to_uppercase().starts_with("COM") {
            // Named Windows printer — bypass serial, use Win32 spooler
            return write_to_windows_printer(port_name, &payload);
        }
    }

    // Serial / COM port path
    // M25: 5s was too short for busy USB printers; 15s accommodates thermal printers
    // that need extra time to respond when their buffer is almost full.
    //
    // FlowControl::Hardware enables RTS/CTS handshaking so the printer can signal
    // when its receive buffer is full. Without this, large receipts (>~4 KB) may
    // overflow the printer's hardware buffer, causing garbled output or truncation.
    let mut port = serialport::new(port_name, baud)
        .flow_control(serialport::FlowControl::Hardware)
        .timeout(Duration::from_secs(15))
        .open()
        .map_err(|e| AppError::Internal(format!(
            "Could not open printer port '{}': {}. Check that the printer is connected and not in use by another application.",
            port_name, e
        )))?;

    use std::io::Write;
    port.write_all(&payload)
        .map_err(|e| AppError::Internal(format!("Serial write failed: {}", e)))?;
    port.flush()
        .map_err(|e| AppError::Internal(format!("Serial flush failed: {}", e)))?;
    Ok(())
}

/// Send raw ESC/POS bytes directly to a Windows-named printer via the Win32
/// print spooler with data type "RAW" (bypasses GDI rendering entirely).
///
/// This is the standard method for USB thermal printers whose Windows driver
/// does not create a virtual COM port.
#[cfg(windows)]
fn write_to_windows_printer(printer_name: &str, payload: &[u8]) -> AppResult<()> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use winapi::shared::minwindef::DWORD;
    use winapi::um::winspool::{
        ClosePrinter, EndDocPrinter, EndPagePrinter, OpenPrinterW, StartDocPrinterW,
        StartPagePrinter, WritePrinter, DOC_INFO_1W,
    };

    fn to_wide(s: &str) -> Vec<u16> {
        OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0u16))
            .collect()
    }

    let wide_name = to_wide(printer_name);
    let wide_raw = to_wide("RAW");
    let wide_doc = to_wide("ZANPOS Receipt");

    unsafe {
        // Open printer handle
        let mut h_printer = std::ptr::null_mut();
        if OpenPrinterW(
            wide_name.as_ptr() as *mut _,
            &mut h_printer,
            std::ptr::null_mut(),
        ) == 0
            || h_printer.is_null()
        {
            let err = winapi::um::errhandlingapi::GetLastError();
            return Err(AppError::Internal(format!(
                "Cannot open printer '{}' (Win32 error {})",
                printer_name, err
            )));
        }

        // Start a raw document
        let mut doc_info = DOC_INFO_1W {
            pDocName: wide_doc.as_ptr() as *mut _,
            pOutputFile: std::ptr::null_mut(),
            pDatatype: wide_raw.as_ptr() as *mut _,
        };
        let job_id = StartDocPrinterW(h_printer, 1, &mut doc_info as *mut DOC_INFO_1W as *mut u8);
        if job_id == 0 {
            ClosePrinter(h_printer);
            return Err(AppError::Internal(format!(
                "StartDocPrinter failed for '{}'",
                printer_name
            )));
        }

        // Start page + write bytes + end page
        if StartPagePrinter(h_printer) == 0 {
            EndDocPrinter(h_printer);
            ClosePrinter(h_printer);
            return Err(AppError::Internal("StartPagePrinter failed".into()));
        }

        let mut written: DWORD = 0;
        let ret = WritePrinter(
            h_printer,
            payload.as_ptr() as *mut winapi::ctypes::c_void,
            payload.len() as DWORD,
            &mut written,
        );
        if ret == 0 || written != payload.len() as DWORD {
            let err_msg = format!(
                "WritePrinter: returned={} written={} expected={}",
                ret,
                written,
                payload.len()
            );
            tracing::error!("{}", err_msg);
            EndPagePrinter(h_printer);
            EndDocPrinter(h_printer);
            ClosePrinter(h_printer);
            return Err(AppError::Internal(err_msg));
        }

        EndPagePrinter(h_printer);
        EndDocPrinter(h_printer);
        ClosePrinter(h_printer);
    }

    Ok(())
}

// ─── Config helpers ───────────────────────────────────────────────────────────

async fn config_get(state: &AppState, key: &str, default: &str) -> String {
    sqlx::query_scalar::<_, Option<String>>("SELECT value FROM app_config WHERE key = ?")
        .bind(key)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .flatten()
        .unwrap_or_else(|| default.to_string())
}

async fn config_set(state: &AppState, key: &str, value: &str) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO app_config(key, value, updated_at) VALUES(?,?,?)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
    )
    .bind(key)
    .bind(value)
    .bind(&now)
    .execute(&state.db)
    .await?;
    Ok(())
}

// ─── Commands ─────────────────────────────────────────────────────────────────

/// A printer entry shown in the printer picker dropdown.
#[derive(Debug, Serialize, Clone)]
pub struct PortEntry {
    /// Identifier used for printing — either a COM port ("COM3") or a Windows printer name ("EPSON TM-T88V")
    pub port: String,
    /// Human-readable label shown in the UI
    pub label: String,
    /// True if this is the operating-system default printer
    pub is_default: bool,
}

/// Enumerate all printers available on this machine:
///   1. Windows system printers (real device names from the print spooler)
///   2. Serial/COM ports (USB-to-serial adapters, Bluetooth virtual COM, etc.)
///
/// Windows printer entries always appear first; serial ports are appended below.
/// The OS-default printer is flagged with `is_default = true` and marked in its label.
#[tauri::command]
pub fn thermal_list_ports() -> Vec<PortEntry> {
    let mut entries: Vec<PortEntry> = Vec::new();

    // ── 1. Windows system printers via PowerShell / WMI ──────────────────────
    //   Uses Win32_Printer (WMI class) which exposes the real printer name and
    //   which one is the current default.  Works on Windows 7 – 11.
    //   M7: Results are cached for 30 seconds (TTL) to avoid spawning PowerShell
    //   on every settings-page render (PowerShell takes 500ms-2s each call).
    #[cfg(windows)]
    {
        let cached = {
            let guard = get_printer_cache()
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            guard.as_ref().and_then(|c| {
                if c.refreshed_at.elapsed() < PRINTER_CACHE_TTL {
                    Some(c.entries.clone())
                } else {
                    None
                }
            })
        };

        let printers = cached.unwrap_or_else(|| {
            let fresh = list_windows_printers();
            if let Ok(mut guard) = get_printer_cache().lock() {
                *guard = Some(PrinterCache {
                    entries: fresh.clone(),
                    refreshed_at: Instant::now(),
                });
            }
            fresh
        });
        entries.extend(printers);
    }

    // ── 2. Serial / COM ports via the serialport crate ────────────────────────
    //   Catches USB-to-serial adapters (Prolific, FTDI, Silabs) and Bluetooth
    //   virtual COM ports that bypass the print spooler entirely.
    if let Ok(ports) = serialport::available_ports() {
        // Build a set of port names already covered by the Windows printer list
        // so we don't create duplicate entries (some printers expose both paths).
        let already_listed: std::collections::HashSet<String> =
            entries.iter().map(|e| e.port.to_uppercase()).collect();

        for p in ports {
            // Skip if already represented as a Windows printer
            if already_listed.contains(&p.port_name.to_uppercase()) {
                continue;
            }
            let description = match &p.port_type {
                serialport::SerialPortType::UsbPort(info) => {
                    let parts: Vec<&str> = [info.manufacturer.as_deref(), info.product.as_deref()]
                        .iter()
                        .filter_map(|x| *x)
                        .filter(|s| !s.is_empty())
                        .collect();
                    if parts.is_empty() {
                        None
                    } else {
                        Some(parts.join(" "))
                    }
                }
                _ => None,
            };
            let label = match description {
                Some(desc) => format!("{} — {} [Serial]", p.port_name, desc),
                None => format!("{} [Serial]", p.port_name),
            };
            entries.push(PortEntry {
                port: p.port_name,
                label,
                is_default: false,
            });
        }
    }

    entries
}

/// Query installed Windows printers via PowerShell's Win32_Printer WMI class.
/// Returns an empty Vec on any failure — the serial port list is still returned.
#[cfg(windows)]
fn list_windows_printers() -> Vec<PortEntry> {
    // PowerShell command: enumerate Win32_Printer objects, extract Name + Default flag,
    // serialise to JSON.  On PS 5.1 ConvertTo-Json may return a single object instead of
    // an array when there is exactly one printer — we handle that on the Rust side.
    let ps_cmd = "Get-CimInstance -Class Win32_Printer \
                  | Select-Object Name, Default \
                  | ConvertTo-Json -Compress";

    let output = match std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", ps_cmd])
        .output()
    {
        Ok(o) => o,
        Err(_) => return vec![],
    };

    if !output.status.success() {
        return vec![];
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    let text = raw.trim();
    if text.is_empty() {
        return vec![];
    }

    // Normalise to a JSON array (PS 5.1 returns a bare object for single-printer systems)
    let json_str = if text.starts_with('[') {
        text.to_string()
    } else if text.starts_with('{') {
        format!("[{}]", text)
    } else {
        return vec![];
    };

    #[derive(serde::Deserialize)]
    struct WinPrinter {
        #[serde(rename = "Name")]
        name: String,
        #[serde(rename = "Default")]
        default: Option<bool>,
    }

    let printers: Vec<WinPrinter> = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(_) => return vec![],
    };

    printers
        .into_iter()
        .filter(|p| !p.name.trim().is_empty())
        .map(|p| {
            let is_default = p.default.unwrap_or(false);
            let label = if is_default {
                format!("{} ★ (Default)", p.name)
            } else {
                p.name.clone()
            };
            PortEntry {
                port: p.name,
                label,
                is_default,
            }
        })
        .collect()
}

/// Get current thermal printer configuration.
#[tauri::command]
pub async fn thermal_get_config(
    session_token: String,
    state: State<'_, AppState>,
) -> Result<ThermalConfig, AppError> {
    rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::ANY_ROLE).await?;
    let enabled_str = config_get(&state, "thermal_printer_enabled", "0").await;
    let port = config_get(&state, "thermal_printer_port", "").await;
    let baud = config_get(&state, "thermal_printer_baud", "9600").await;

    Ok(ThermalConfig {
        enabled: enabled_str == "1",
        port,
        baud,
    })
}

/// Save thermal printer configuration.
#[tauri::command]
pub async fn thermal_set_config(
    input: ThermalConfigInput,
    session_token: String,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    rbac::session_actor(
        &state.sessions,
        &state.db,
        &session_token,
        rbac::MANAGER_OR_OWNER,
    )
    .await?;

    // BUG-PRINTER-VALIDATION: validate port and baud_rate before persisting.
    let port = input.port.trim();
    if input.enabled && port.is_empty() {
        return Err(AppError::Validation(
            "A printer port must be selected when printing is enabled".into(),
        ));
    }
    if input.enabled && port.len() > 50 {
        return Err(AppError::Validation(
            "Printer port name must be 50 characters or fewer".into(),
        ));
    }

    const VALID_BAUDS: &[&str] = &["9600", "19200", "38400", "57600", "115200"];
    let baud = input.baud.trim();
    if !VALID_BAUDS.contains(&baud) {
        return Err(AppError::Validation(format!(
            "Invalid baud rate '{}'. Must be one of: {}",
            baud,
            VALID_BAUDS.join(", ")
        )));
    }

    config_set(
        &state,
        "thermal_printer_enabled",
        if input.enabled { "1" } else { "0" },
    )
    .await?;
    config_set(&state, "thermal_printer_port", port).await?;
    config_set(&state, "thermal_printer_baud", baud).await?;
    Ok(())
}

/// Send a test page to the configured thermal printer.
#[tauri::command]
pub async fn thermal_print_test(
    session_token: String,
    state: State<'_, AppState>,
) -> Result<String, AppError> {
    rbac::session_actor(
        &state.sessions,
        &state.db,
        &session_token,
        rbac::MANAGER_OR_OWNER,
    )
    .await?;
    let config = thermal_get_config(session_token.clone(), state.clone()).await?;

    if !config.enabled {
        return Ok("Thermal printing is disabled. Enable it in Settings first.".into());
    }
    if config.port.trim().is_empty() {
        return Ok("No port configured. Enter a COM port (e.g. COM3) in Settings.".into());
    }

    let port_name = config.port.clone();
    let baud_str = config.baud.clone();
    let baud: u32 = baud_str.parse().unwrap_or(9600);
    let payload = build_test_bytes(&port_name, &baud_str);

    tokio::task::spawn_blocking(move || write_to_port(&port_name, baud, payload))
        .await
        .map_err(|e| AppError::Internal(format!("Thread error: {e}")))?
        .map(|_| format!("Test page sent to {} at {} baud.", config.port, config.baud))
}

/// Print receipt lines via ESC/POS to the configured thermal printer.
/// `store_name` is printed as a centered header; `lines` are the receipt body.
#[tauri::command]
pub async fn print_receipt_raw(
    session_token: String,
    store_name: String,
    lines: Vec<String>,
    state: State<'_, AppState>,
) -> Result<String, AppError> {
    rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::ANY_ROLE).await?;
    let config = thermal_get_config(session_token.clone(), state.clone()).await?;

    if !config.enabled {
        return Ok("Thermal printing disabled".into());
    }
    if config.port.trim().is_empty() {
        return Err(AppError::Validation(
            "No thermal printer port configured".into(),
        ));
    }

    let port_name = config.port.clone();
    let baud: u32 = config.baud.parse().unwrap_or(9600);
    let payload = build_receipt_bytes(&store_name, &lines);

    let printed = tokio::task::spawn_blocking(move || write_to_port(&port_name, baud, payload))
        .await
        .map_err(|e| AppError::Internal(format!("Thread error: {e}")))?;

    // A silent no-print is the failure this instrumentation exists to surface,
    // so record the outcome before propagating it.
    match &printed {
        Ok(_) => crate::diagnostics::record_event(&state.db, "print_ok", None).await,
        Err(e) => {
            crate::diagnostics::record_event(
                &state.db,
                "print_fail",
                Some(serde_json::json!({ "error": e.to_string() })),
            )
            .await;
            let _ = crate::diagnostics::record(
                &state.db,
                "error",
                "print_fail",
                &e.to_string(),
                None,
                None,
            )
            .await;
            // Queue the rendered lines so the receipt can be reproduced once
            // the printer is back — exactly what should have printed, not a
            // re-render that might pick up changed prices or settings.
            let _ = crate::commands::reprint_queue::enqueue(
                &state.db,
                &store_name,
                &lines,
                &e.to_string(),
            )
            .await;
        }
    }
    printed.map(|_| "Printed".into())
}

/// ESC/POS cash drawer kick pulse.
///
/// Command: ESC p m t1 t2  (0x1B 0x70 m t1 t2)
///   m  = 0x00 → drawer connector pin 2 (default on Epson TM / Star mPOP)
///   m  = 0x01 → drawer connector pin 5 (alternate — set via printer DIP switch)
///   t1 = ON  time = t1 × 2 ms  (Epson spec range: 1–127)
///   t2 = OFF time = t2 × 2 ms  (Epson spec range: 1–127)
///
/// We use (60, 120) → 120 ms ON, 240 ms OFF — well within the Epson specification
/// and backwards-compatible with Star Micronics thermal printers (Star supports the
/// EPSON ESC/POS command set on all modern models; older Star printers may need
/// ESC BEL 0x07 n1 n2 instead, which is not implemented here).
///
/// Previous values (0x19, 0xFA) = (25, 250) had t2 outside the 1–127 range;
/// some older Epson models silently clamp out-of-range values, risking a too-short
/// pulse that fails to trip the solenoid.
pub(crate) fn esc_open_drawer() -> Vec<u8> {
    vec![ESC, b'p', 0x00, 60, 120]
}

/// Open the cash drawer connected to the thermal printer's RJ-11 port.
/// Non-fatal: returns Ok("no_printer") if thermal printing is disabled or no port configured.
#[tauri::command]
pub async fn open_cash_drawer(
    session_token: String,
    state: State<'_, AppState>,
) -> Result<String, AppError> {
    rbac::session_actor(&state.sessions, &state.db, &session_token, rbac::ANY_ROLE).await?;
    let config = thermal_get_config(session_token.clone(), state.clone()).await?;

    if !config.enabled || config.port.trim().is_empty() {
        // Drawer kick silently skipped — no printer configured.
        return Ok("no_printer".into());
    }

    let port_name = config.port.clone();
    let baud: u32 = config.baud.parse().unwrap_or(9600);
    let payload = esc_open_drawer();

    tokio::task::spawn_blocking(move || write_to_port(&port_name, baud, payload))
        .await
        .map_err(|e| AppError::Internal(format!("Thread error: {e}")))?
        .map(|_| "opened".into())
}
