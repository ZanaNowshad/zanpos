use sqlx::{SqlitePool, Row};
use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use crate::domain::auth::{UserSummary, SessionUser};
use crate::errors::{AppError, AppResult};

/// Hash a PIN for storage using Argon2id.
pub fn hash_pin(pin: &str) -> AppResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(pin.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::Internal(format!("Hashing failed: {e}")))
}

/// Verify a PIN against a stored hash.
/// Supports legacy PLAIN:<pin> prefix so existing dev seeds still work.
fn verify_pin(stored_hash: &str, input_pin: &str) -> bool {
    // Legacy plain-text PINs (dev seeds only)
    if let Some(plain) = stored_hash.strip_prefix("PLAIN:") {
        return plain == input_pin;
    }
    // Argon2id verification
    PasswordHash::new(stored_hash)
        .ok()
        .map(|parsed| Argon2::default().verify_password(input_pin.as_bytes(), &parsed).is_ok())
        .unwrap_or(false)
}

pub async fn list_active_users(pool: &SqlitePool) -> AppResult<Vec<UserSummary>> {
    let rows = sqlx::query(
        "SELECT u.user_id, u.display_name, u.username, r.name as role_name
         FROM users u JOIN roles r ON r.role_id = u.role_id
         WHERE u.is_active = 1 ORDER BY u.display_name"
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.iter().map(|row| UserSummary {
        user_id: row.get("user_id"),
        display_name: row.get("display_name"),
        username: row.get("username"),
        role_name: row.get("role_name"),
    }).collect())
}

pub async fn login_pin(pool: &SqlitePool, username: &str, pin: &str) -> AppResult<SessionUser> {
    let row = sqlx::query(
        "SELECT u.user_id, u.display_name, u.username, u.pin_hash, u.role_id, r.name as role_name
         FROM users u JOIN roles r ON r.role_id = u.role_id
         WHERE u.username = ? AND u.is_active = 1"
    )
    .bind(username)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::Permission("Invalid credentials".into()))?;

    let pin_hash: String = row.get("pin_hash");
    if !verify_pin(&pin_hash, pin) {
        return Err(AppError::Permission("Invalid PIN".into()));
    }

    let user_id: String = row.get("user_id");
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE users SET last_login_at = ? WHERE user_id = ?")
        .bind(&now)
        .bind(&user_id)
        .execute(pool)
        .await?;

    Ok(SessionUser {
        user_id,
        display_name: row.get("display_name"),
        username: row.get("username"),
        role_id: row.get("role_id"),
        role_name: row.get("role_name"),
    })
}
