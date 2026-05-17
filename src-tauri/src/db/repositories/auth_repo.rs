use sqlx::{SqlitePool, Row};
use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use crate::domain::auth::{UserSummary, SessionUser};
use crate::errors::{AppError, AppResult};

const MAX_ATTEMPTS: i64 = 5;
const LOCKOUT_MINUTES: i64 = 15;

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

/// On startup, re-hash any legacy PLAIN:<pin> entries to argon2id so that
/// no cleartext PINs remain in the database after the first launch.
pub async fn rehash_plain_pins(pool: &SqlitePool) -> AppResult<()> {
    let rows = sqlx::query(
        "SELECT user_id, pin_hash FROM users WHERE pin_hash LIKE 'PLAIN:%'"
    )
    .fetch_all(pool)
    .await?;

    for row in rows {
        let user_id: String = row.get("user_id");
        let stored: String = row.get("pin_hash");
        if let Some(plain) = stored.strip_prefix("PLAIN:") {
            match hash_pin(plain) {
                Ok(hashed) => {
                    sqlx::query("UPDATE users SET pin_hash = ? WHERE user_id = ?")
                        .bind(&hashed)
                        .bind(&user_id)
                        .execute(pool)
                        .await?;
                    tracing::info!("Migrated PLAIN: PIN to argon2id for user {}", user_id);
                }
                Err(e) => {
                    tracing::warn!("Failed to rehash PIN for user {}: {:?}", user_id, e);
                }
            }
        }
    }
    Ok(())
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
        "SELECT u.user_id, u.display_name, u.username, u.pin_hash, u.role_id, r.name as role_name,
                u.failed_pin_attempts, u.locked_until
         FROM users u JOIN roles r ON r.role_id = u.role_id
         WHERE u.username = ? AND u.is_active = 1"
    )
    .bind(username)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::Permission("Invalid credentials".into()))?;

    let user_id: String = row.get("user_id");

    // Check lockout
    let locked_until: Option<String> = row.get("locked_until");
    if let Some(ref locked_str) = locked_until {
        if let Ok(locked_dt) = chrono::DateTime::parse_from_rfc3339(locked_str) {
            let now = chrono::Utc::now();
            if locked_dt > now {
                let remaining = (locked_dt.with_timezone(&chrono::Utc) - now).num_minutes() + 1;
                return Err(AppError::Permission(
                    format!("Account locked. Try again in {} minute(s).", remaining)
                ));
            }
            // Lock expired — reset
            sqlx::query(
                "UPDATE users SET failed_pin_attempts = 0, locked_until = NULL WHERE user_id = ?"
            )
            .bind(&user_id)
            .execute(pool)
            .await?;
        }
    }

    let pin_hash: String = row.get("pin_hash");
    let failed_attempts: i64 = row.get("failed_pin_attempts");

    if !verify_pin(&pin_hash, pin) {
        let new_attempts = failed_attempts + 1;
        if new_attempts >= MAX_ATTEMPTS {
            let locked_until = (chrono::Utc::now() + chrono::Duration::minutes(LOCKOUT_MINUTES))
                .to_rfc3339();
            sqlx::query(
                "UPDATE users SET failed_pin_attempts = ?, locked_until = ? WHERE user_id = ?"
            )
            .bind(new_attempts)
            .bind(&locked_until)
            .bind(&user_id)
            .execute(pool)
            .await?;
            return Err(AppError::Permission(
                format!("Too many failed attempts. Account locked for {} minutes.", LOCKOUT_MINUTES)
            ));
        } else {
            sqlx::query(
                "UPDATE users SET failed_pin_attempts = ? WHERE user_id = ?"
            )
            .bind(new_attempts)
            .bind(&user_id)
            .execute(pool)
            .await?;
            let remaining = MAX_ATTEMPTS - new_attempts;
            return Err(AppError::Permission(
                format!("Invalid PIN. {} attempt(s) remaining.", remaining)
            ));
        }
    }

    // Success — reset failure counter and update last login
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE users SET last_login_at = ?, failed_pin_attempts = 0, locked_until = NULL WHERE user_id = ?"
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    // ── Test: lockout threshold is exactly 5 attempts ──────────────────────────
    #[test]
    fn lockout_threshold_is_five() {
        assert_eq!(MAX_ATTEMPTS, 5, "Lockout must trigger after 5 failed attempts");
    }

    // ── Test: lockout duration is exactly 15 minutes ───────────────────────────
    #[test]
    fn lockout_duration_is_fifteen_minutes() {
        assert_eq!(LOCKOUT_MINUTES, 15, "Lockout must last 15 minutes");
    }

    // ── Test: PLAIN: prefix verification (legacy dev seeds) ───────────────────
    #[test]
    fn verify_pin_plain_prefix_correct() {
        let stored = "PLAIN:1234";
        assert!(verify_pin(stored, "1234"), "PLAIN: prefix should match exact PIN");
    }

    #[test]
    fn verify_pin_plain_prefix_wrong() {
        let stored = "PLAIN:1234";
        assert!(!verify_pin(stored, "9999"), "PLAIN: prefix should reject wrong PIN");
    }

    // ── Test: argon2id hash round-trip ────────────────────────────────────────
    #[test]
    fn hash_and_verify_pin_roundtrip() {
        let hashed = hash_pin("5678").expect("hash must succeed");
        assert!(verify_pin(&hashed, "5678"), "Argon2id hash must verify correctly");
        assert!(!verify_pin(&hashed, "0000"), "Argon2id hash must reject wrong PIN");
    }
}
