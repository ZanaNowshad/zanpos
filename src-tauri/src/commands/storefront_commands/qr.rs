use super::cfg;
use crate::commands::rbac;
use crate::errors::{AppError, AppResult};
use crate::AppState;
use tauri::State;

/// QR PNG (data URL) for the public storefront link — shown in Settings and
/// printable for the shop counter. Generated locally by the sidecar's qrcode
/// package; nothing leaves the machine.
#[tauri::command]
pub async fn storefront_qr(session_token: String, state: State<'_, AppState>) -> AppResult<String> {
    rbac::session_actor(
        &state.sessions,
        &state.db,
        &session_token,
        rbac::MANAGER_OR_OWNER,
    )
    .await?;
    let url = cfg(&state.db, "storefront_public_url").await?;
    if url.trim().is_empty() {
        return Err(AppError::Validation(
            "Deploy the storefront first — there is no public link yet.".into(),
        ));
    }
    let token = crate::commands::whatsapp_commands::read_sidecar_token(&state);
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(5))
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_default();
    let response = client
        .get(format!(
            "{}/qr?text={}",
            crate::commands::whatsapp_commands::SIDECAR_URL,
            urlencode(url.trim())
        ))
        .header("X-Sidecar-Token", &token)
        .send()
        .await
        .map_err(|error| AppError::Internal(format!("QR service unreachable: {error}")))?;
    let body: serde_json::Value = response.json().await.unwrap_or_default();
    body.get("data_url")
        .and_then(|value| value.as_str())
        .map(String::from)
        .ok_or_else(|| {
            AppError::Internal(
                body.get("error")
                    .and_then(|error| error.as_str())
                    .unwrap_or("QR generation failed")
                    .to_string(),
            )
        })
}

fn urlencode(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                output.push(byte as char)
            }
            _ => output.push_str(&format!("%{byte:02X}")),
        }
    }
    output
}
