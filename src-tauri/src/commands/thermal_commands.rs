/// Thermal/ESC-POS printer configuration and printing.
/// Uses the `serialport` crate directly (synchronous I/O on a blocking thread).
use tauri::State;
use serde::{Deserialize, Serialize};
use crate::errors::{AppError, AppResult};
use crate::AppState;

// ─── Types ────────────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ThermalConfig {
    pub enabled: bool,
    pub port:    String,
    pub baud:    String,
}

#[derive(Deserialize)]
pub struct ThermalConfigInput {
    pub enabled: bool,
    pub port:    String,
    pub baud:    String,
}

// ─── ESC/POS byte constants ───────────────────────────────────────────────────

const ESC: u8  = 0x1B;
const GS:  u8  = 0x1D;
const LF:  u8  = 0x0A;

/// Initialize printer (ESC @)
fn esc_init() -> Vec<u8> { vec![ESC, b'@'] }

/// Select alignment: 0=left, 1=center, 2=right (ESC a n)
fn esc_align(n: u8) -> Vec<u8> { vec![ESC, b'a', n] }

/// Bold on/off (ESC E n)
fn esc_bold(on: bool) -> Vec<u8> { vec![ESC, b'E', if on { 1 } else { 0 }] }

/// Double-size text on/off (GS ! n, 0x11 = double height+width)
fn esc_double(on: bool) -> Vec<u8> { vec![GS, b'!', if on { 0x11 } else { 0x00 }] }

/// Feed n lines and cut (GS V 0 = full cut)
fn esc_feed_and_cut(lines: u8) -> Vec<u8> { vec![ESC, b'd', lines, GS, b'V', 0x00] }

/// Build ESC/POS receipt byte payload.
/// `store_name`, `header_lines`, `item_lines`, `footer_lines` are all pre-formatted strings.
pub fn build_receipt_bytes(
    store_name: &str,
    receipt_lines: &[String],
) -> Vec<u8> {
    let mut buf: Vec<u8> = Vec::with_capacity(512);

    // Initialize
    buf.extend_from_slice(&esc_init());

    // Store name — centered, double-size bold
    buf.extend_from_slice(&esc_align(1));  // center
    buf.extend_from_slice(&esc_double(true));
    buf.extend_from_slice(&esc_bold(true));
    buf.extend_from_slice(store_name.as_bytes());
    buf.push(LF);
    buf.extend_from_slice(&esc_double(false));
    buf.extend_from_slice(&esc_bold(false));
    buf.push(LF);

    // Body lines — left aligned
    buf.extend_from_slice(&esc_align(0));  // left
    for line in receipt_lines {
        buf.extend_from_slice(line.as_bytes());
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
    buf.extend_from_slice(b"ZANPOS");
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

/// Write raw bytes to a serial port on a dedicated blocking thread.
fn write_to_port(port_name: &str, baud: u32, payload: Vec<u8>) -> AppResult<()> {
    use std::time::Duration;
    let mut port = serialport::new(port_name, baud)
        .timeout(Duration::from_secs(5))
        .open()
        .map_err(|e| AppError::Internal(format!("Cannot open port '{}': {}", port_name, e)))?;

    use std::io::Write;
    port.write_all(&payload)
        .map_err(|e| AppError::Internal(format!("Serial write failed: {}", e)))?;
    port.flush()
        .map_err(|e| AppError::Internal(format!("Serial flush failed: {}", e)))?;
    Ok(())
}

// ─── Config helpers ───────────────────────────────────────────────────────────

async fn config_get(state: &AppState, key: &str, default: &str) -> String {
    sqlx::query_scalar::<_, Option<String>>(
        "SELECT value FROM app_config WHERE key = ?"
    )
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
         ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at"
    )
    .bind(key)
    .bind(value)
    .bind(&now)
    .execute(&state.db)
    .await?;
    Ok(())
}

// ─── Commands ─────────────────────────────────────────────────────────────────

/// Get current thermal printer configuration.
#[tauri::command]
pub async fn thermal_get_config(state: State<'_, AppState>) -> Result<ThermalConfig, AppError> {
    let enabled_str = config_get(&state, "thermal_printer_enabled", "0").await;
    let port        = config_get(&state, "thermal_printer_port", "").await;
    let baud        = config_get(&state, "thermal_printer_baud", "9600").await;

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
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    config_set(&state, "thermal_printer_enabled", if input.enabled { "1" } else { "0" }).await?;
    config_set(&state, "thermal_printer_port", &input.port).await?;
    config_set(&state, "thermal_printer_baud", &input.baud).await?;
    Ok(())
}

/// Send a test page to the configured thermal printer.
#[tauri::command]
pub async fn thermal_print_test(state: State<'_, AppState>) -> Result<String, AppError> {
    let config = thermal_get_config(state).await?;

    if !config.enabled {
        return Ok("Thermal printing is disabled. Enable it in Settings first.".into());
    }
    if config.port.trim().is_empty() {
        return Ok("No port configured. Enter a COM port (e.g. COM3) in Settings.".into());
    }

    let port_name = config.port.clone();
    let baud_str  = config.baud.clone();
    let baud: u32 = baud_str.parse().unwrap_or(9600);
    let payload   = build_test_bytes(&port_name, &baud_str);

    tokio::task::spawn_blocking(move || write_to_port(&port_name, baud, payload))
        .await
        .map_err(|e| AppError::Internal(format!("Thread error: {e}")))?
        .map(|_| format!("Test page sent to {} at {} baud.", config.port, config.baud))
}

/// Print receipt lines via ESC/POS to the configured thermal printer.
/// `store_name` is printed as a centered header; `lines` are the receipt body.
#[tauri::command]
pub async fn print_receipt_raw(
    store_name: String,
    lines:      Vec<String>,
    state:      State<'_, AppState>,
) -> Result<String, AppError> {
    let config = thermal_get_config(state).await?;

    if !config.enabled {
        return Ok("Thermal printing disabled".into());
    }
    if config.port.trim().is_empty() {
        return Err(AppError::Validation("No thermal printer port configured".into()));
    }

    let port_name = config.port.clone();
    let baud: u32 = config.baud.parse().unwrap_or(9600);
    let payload   = build_receipt_bytes(&store_name, &lines);

    tokio::task::spawn_blocking(move || write_to_port(&port_name, baud, payload))
        .await
        .map_err(|e| AppError::Internal(format!("Thread error: {e}")))?
        .map(|_| "Printed".into())
}
