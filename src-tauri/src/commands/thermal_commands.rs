/// Thermal/ESC-POS printer configuration and stub print command.
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

// ─── Helper ───────────────────────────────────────────────────────────────────

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

/// Stub: attempt a test print (serial port plugin not yet installed).
/// Returns an informational message rather than an error so the UI can show it.
#[tauri::command]
pub async fn thermal_print_test(state: State<'_, AppState>) -> Result<String, AppError> {
    let config = thermal_get_config(state).await?;

    if !config.enabled {
        return Ok(
            "Thermal printing is disabled. Enable it and configure a port first.".into()
        );
    }

    if config.port.trim().is_empty() {
        return Ok("No serial port configured. Please enter a port (e.g. COM3 or /dev/ttyUSB0).".into());
    }

    // Serial port integration requires `tauri-plugin-serialport`.
    // Add to Cargo.toml:
    //   tauri-plugin-serialport = { git = "https://github.com/deid84/tauri-plugin-serialport" }
    // Register it in lib.rs:
    //   .plugin(tauri_plugin_serialport::init())
    // Then replace this stub with actual ESC/POS byte writing:
    //   let mut port = serialport::new(&config.port, config.baud.parse().unwrap_or(9600))
    //       .open()?;
    //   port.write_all(&[0x1B, 0x40])?;  // ESC @ — initialize printer
    //   port.write_all(b"Test Receipt\n")?;
    //   port.write_all(&[0x1B, 0x64, 5])?; // feed 5 lines

    Ok(format!(
        "STUB: Serial port plugin not installed.\n\
         Configured port: {} at {} baud.\n\
         To enable hardware printing, add tauri-plugin-serialport to Cargo.toml \
         and implement the ESC/POS write logic in thermal_commands.rs.",
        config.port, config.baud
    ))
}

/// Stub: print receipt lines via ESC/POS.
/// Full implementation requires `tauri-plugin-serialport`.
#[tauri::command]
pub async fn print_receipt_raw(
    lines: Vec<String>,
    state: State<'_, AppState>,
) -> Result<String, AppError> {
    let config = thermal_get_config(state).await?;

    if !config.enabled {
        return Ok("Thermal printing disabled".into());
    }

    // TODO: Replace with actual serial write when tauri-plugin-serialport is available.
    // ESC/POS implementation:
    //   let mut port = serialport::new(&config.port, baud).open()?;
    //   port.write_all(&[0x1B, 0x40])?;        // initialize
    //   for line in &lines {
    //       port.write_all(line.as_bytes())?;
    //       port.write_all(b"\n")?;
    //   }
    //   port.write_all(&[0x1D, 0x56, 0x00])?;  // cut

    let line_count = lines.len();
    Ok(format!(
        "STUB: Would print {} lines to {} (serial plugin not installed)",
        line_count, config.port
    ))
}
