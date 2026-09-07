use crate::commands::rbac;
use crate::errors::{AppError, AppResult};
use crate::{secure_store, AppState};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sqlx::SqlitePool;
use tauri::State;

const API_ROOT: &str = "https://api.cloudflare.com/client/v4";
const TOKEN_KEY: &str = "storefront_cloudflare_api_token";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudflareAccount {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CloudflareConnection {
    pub state: String,
    pub account_id: Option<String>,
    pub account_name: Option<String>,
    pub accounts: Vec<CloudflareAccount>,
    pub credential_stored: bool,
    pub last_verified_at: Option<String>,
    pub issue: Option<String>,
}

#[derive(Deserialize)]
struct ApiError {
    message: String,
}

#[derive(Deserialize)]
struct ApiEnvelope<T> {
    success: bool,
    result: Option<T>,
    #[serde(default)]
    errors: Vec<ApiError>,
}

#[derive(Deserialize)]
struct TokenVerification {
    status: String,
}

async fn cfg(pool: &SqlitePool, key: &str) -> AppResult<String> {
    Ok(
        sqlx::query_scalar("SELECT value FROM app_config WHERE key=?")
            .bind(key)
            .fetch_optional(pool)
            .await?
            .unwrap_or_default(),
    )
}

async fn set_cfg(pool: &SqlitePool, key: &str, value: &str) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO app_config(key,value,updated_at) VALUES (?,?,?)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value,updated_at=excluded.updated_at",
    )
    .bind(key)
    .bind(value)
    .bind(chrono::Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    Ok(())
}

fn validate_token(token: &str) -> AppResult<&str> {
    let token = token.trim();
    if token.len() < 20 || token.len() > 512 || token.bytes().any(|byte| byte.is_ascii_whitespace())
    {
        return Err(AppError::Validation(
            "Enter the API token exactly as Cloudflare provided it".into(),
        ));
    }
    Ok(token)
}

async fn cloudflare_get<T: DeserializeOwned>(
    client: &reqwest::Client,
    token: &str,
    path: &str,
) -> AppResult<T> {
    let response = client
        .get(format!("{API_ROOT}{path}"))
        .bearer_auth(token)
        .send()
        .await
        .map_err(|_| {
            AppError::Validation(
                "Cloudflare could not be reached. Check your connection and try again.".into(),
            )
        })?;
    let status = response.status();
    let body: ApiEnvelope<T> = response.json().await.map_err(|_| {
        AppError::Validation("Cloudflare returned an unreadable response. Try again.".into())
    })?;
    if !status.is_success() || !body.success {
        let detail = body
            .errors
            .first()
            .map(|error| error.message.as_str())
            .unwrap_or("The token was rejected");
        return Err(AppError::Validation(format!(
            "Cloudflare connection failed: {detail}"
        )));
    }
    body.result
        .ok_or_else(|| AppError::Validation("Cloudflare returned no account information.".into()))
}

async fn verify_and_accounts(token: &str) -> AppResult<Vec<CloudflareAccount>> {
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(8))
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|_| AppError::Internal("Could not initialize Cloudflare connection".into()))?;
    let verification: TokenVerification =
        cloudflare_get(&client, token, "/user/tokens/verify").await?;
    if verification.status != "active" {
        return Err(AppError::Validation(
            "This Cloudflare token is not active. Create a new connection key.".into(),
        ));
    }
    let accounts: Vec<CloudflareAccount> =
        cloudflare_get(&client, token, "/accounts?per_page=50").await?;
    if accounts.is_empty() {
        return Err(AppError::Validation(
            "No Cloudflare accounts are available to this token.".into(),
        ));
    }
    Ok(accounts)
}

pub(crate) async fn verify_stored_connection(pool: &SqlitePool) -> AppResult<()> {
    let token = secure_store::get_secret(TOKEN_KEY)
        .ok_or_else(|| AppError::Validation("Connect Cloudflare first.".into()))?;
    let accounts = verify_and_accounts(&token).await?;
    let account_id = cfg(pool, "storefront_cloudflare_account_id").await?;
    if account_id.is_empty() {
        return Err(AppError::Validation(
            "Choose the Cloudflare account for this storefront first.".into(),
        ));
    }
    if !accounts.iter().any(|account| account.id == account_id) {
        return Err(AppError::Validation(
            "The selected Cloudflare account is no longer available to this connection key.".into(),
        ));
    }
    Ok(())
}

async fn connection_from(
    pool: &SqlitePool,
    accounts: Vec<CloudflareAccount>,
    issue: Option<String>,
) -> AppResult<CloudflareConnection> {
    let stored_id = cfg(pool, "storefront_cloudflare_account_id").await?;
    let selected = accounts.iter().find(|account| account.id == stored_id);
    Ok(CloudflareConnection {
        state: if issue.is_some() {
            "degraded"
        } else if selected.is_some() {
            "connected"
        } else {
            "account_required"
        }
        .into(),
        account_id: selected.map(|account| account.id.clone()),
        account_name: selected.map(|account| account.name.clone()),
        accounts,
        credential_stored: true,
        last_verified_at: match cfg(pool, "storefront_cloudflare_verified_at").await? {
            value if value.is_empty() => None,
            value => Some(value),
        },
        issue,
    })
}

#[tauri::command]
pub async fn storefront_cloudflare_connection_get(
    session_token: String,
    state: State<'_, AppState>,
) -> AppResult<CloudflareConnection> {
    rbac::session_actor(
        &state.sessions,
        &state.db,
        &session_token,
        rbac::MANAGER_OR_OWNER,
    )
    .await?;
    let Some(token) = secure_store::get_secret(TOKEN_KEY) else {
        return Ok(CloudflareConnection {
            state: "unconfigured".into(),
            account_id: None,
            account_name: None,
            accounts: vec![],
            credential_stored: false,
            last_verified_at: None,
            issue: None,
        });
    };
    match verify_and_accounts(&token).await {
        Ok(accounts) => connection_from(&state.db, accounts, None).await,
        Err(error) => {
            let account_id = cfg(&state.db, "storefront_cloudflare_account_id").await?;
            let account_name = cfg(&state.db, "storefront_cloudflare_account_name").await?;
            Ok(CloudflareConnection {
                state: "degraded".into(),
                account_id: (!account_id.is_empty()).then_some(account_id),
                account_name: (!account_name.is_empty()).then_some(account_name),
                accounts: vec![],
                credential_stored: true,
                last_verified_at: None,
                issue: Some(error.user_message().into()),
            })
        }
    }
}

#[tauri::command]
pub async fn storefront_cloudflare_connect(
    session_token: String,
    api_token: String,
    state: State<'_, AppState>,
) -> AppResult<CloudflareConnection> {
    rbac::session_actor(
        &state.sessions,
        &state.db,
        &session_token,
        rbac::MANAGER_OR_OWNER,
    )
    .await?;
    let token = validate_token(&api_token)?;
    let accounts = verify_and_accounts(token).await?;
    if !secure_store::set_secret(TOKEN_KEY, token) {
        return Err(AppError::Internal(
            "Could not store the Cloudflare credential securely".into(),
        ));
    }
    let now = chrono::Utc::now().to_rfc3339();
    set_cfg(&state.db, "storefront_cloudflare_verified_at", &now).await?;
    if accounts.len() == 1 {
        set_cfg(
            &state.db,
            "storefront_cloudflare_account_id",
            &accounts[0].id,
        )
        .await?;
        set_cfg(
            &state.db,
            "storefront_cloudflare_account_name",
            &accounts[0].name,
        )
        .await?;
    } else {
        set_cfg(&state.db, "storefront_cloudflare_account_id", "").await?;
        set_cfg(&state.db, "storefront_cloudflare_account_name", "").await?;
    }
    connection_from(&state.db, accounts, None).await
}

#[tauri::command]
pub async fn storefront_cloudflare_select_account(
    session_token: String,
    account_id: String,
    state: State<'_, AppState>,
) -> AppResult<CloudflareConnection> {
    rbac::session_actor(
        &state.sessions,
        &state.db,
        &session_token,
        rbac::MANAGER_OR_OWNER,
    )
    .await?;
    let token = secure_store::get_secret(TOKEN_KEY)
        .ok_or_else(|| AppError::Validation("Connect Cloudflare first.".into()))?;
    let accounts = verify_and_accounts(&token).await?;
    let selected = accounts
        .iter()
        .find(|account| account.id == account_id)
        .ok_or_else(|| AppError::Validation("Choose an available Cloudflare account.".into()))?;
    set_cfg(&state.db, "storefront_cloudflare_account_id", &selected.id).await?;
    set_cfg(
        &state.db,
        "storefront_cloudflare_account_name",
        &selected.name,
    )
    .await?;
    connection_from(&state.db, accounts, None).await
}

#[tauri::command]
pub async fn storefront_cloudflare_disconnect(
    session_token: String,
    state: State<'_, AppState>,
) -> AppResult<()> {
    rbac::session_actor(
        &state.sessions,
        &state.db,
        &session_token,
        rbac::MANAGER_OR_OWNER,
    )
    .await?;
    secure_store::delete_secret(TOKEN_KEY);
    for key in [
        "storefront_cloudflare_account_id",
        "storefront_cloudflare_account_name",
        "storefront_cloudflare_verified_at",
    ] {
        set_cfg(&state.db, key, "").await?;
    }
    Ok(())
}

/// One-click GO LIVE. Provisions the R2 bucket, uploads the embedded worker,
/// installs the HMAC publish secret, enables the workers.dev URL, and uploads
/// the bundled storefront SPA — then saves the resulting public URL so
/// publishing and the QR code work immediately. Idempotent; re-running updates
/// the worker and site in place.
#[tauri::command]
pub async fn storefront_cloudflare_deploy(
    session_token: String,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> AppResult<crate::storefront::deploy::DeployReport> {
    rbac::session_actor(
        &state.sessions,
        &state.db,
        &session_token,
        rbac::MANAGER_OR_OWNER,
    )
    .await?;

    let token = secure_store::get_secret(TOKEN_KEY).ok_or_else(|| {
        AppError::Validation(
            "Connect your Cloudflare account first (Settings → Storefront).".into(),
        )
    })?;
    let account_id = cfg(&state.db, "storefront_cloudflare_account_id").await?;
    if account_id.is_empty() {
        return Err(AppError::Validation(
            "Select a Cloudflare account first (Settings → Storefront).".into(),
        ));
    }

    // Ensure the HMAC publish secret exists so the worker and the publisher
    // agree on it from the very first deploy.
    const SECRET_KEY: &str = "storefront_publish_secret";
    let secret = match secure_store::get_secret(SECRET_KEY) {
        Some(s) if !s.is_empty() => s,
        _ => {
            let fresh = format!("{}{}", ulid::Ulid::new(), ulid::Ulid::new());
            if !secure_store::set_secret(SECRET_KEY, &fresh) {
                return Err(AppError::Internal(
                    "Could not save the publish secret in the OS credential store".into(),
                ));
            }
            fresh
        }
    };

    use tauri::Manager as _;
    let resource_dir = app.path().resource_dir().ok();
    let report =
        crate::storefront::deploy::deploy(&token, &account_id, &secret, resource_dir).await?;

    // Wire the URLs into the storefront settings so publish + QR work at once.
    set_cfg(&state.db, "storefront_publish_url", &report.public_url).await?;
    set_cfg(&state.db, "storefront_public_url", &report.public_url).await?;
    set_cfg(&state.db, "storefront_connected", "1").await?;
    crate::commands::sync_commands::schedule_immediate_sync(&state);
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_tokens_are_bounded_and_cannot_contain_whitespace() {
        assert!(validate_token("short").is_err());
        assert!(validate_token("abcdefghijklmnopqrst").is_ok());
        assert!(validate_token("abcdefghijklmnop qrst").is_err());
    }
}
