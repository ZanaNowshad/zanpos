use crate::domain::auth::{SessionUser, UserSummary};
use crate::errors::{AppError, AppResult};
use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use sqlx::{Row, SqlitePool};

// Security note: 4-digit numeric PINs have 10,000 combinations.
// At MAX_ATTEMPTS=5 per LOCKOUT_MINUTES=30, the expected brute-force
// time is ~42 days. Consider increasing LOCKOUT_MINUTES to 60 for
// higher-security deployments, or adding a progressive delay between
// attempts (50ms, 200ms, 500ms, 1000ms, lockout).
const MAX_ATTEMPTS: i64 = 5;
// S-02: 60-minute lockout for production. With a 4-digit PIN (10K combinations)
// and a 5-attempt window, this caps brute-force throughput at ~5 guesses/hour.
const LOCKOUT_MINUTES: i64 = 60;

/// Hash a PIN for storage using Argon2id.
pub fn hash_pin(pin: &str) -> AppResult<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(pin.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::Internal(format!("Hashing failed: {e}")))
}

/// Verify a PIN against a stored hash.
/// PLAIN: hashes must have been migrated at startup via rehash_plain_pins.
/// Accepting them at runtime would be a security bypass — they are rejected here.
/// Public so the Maintenance-page owner-PIN gate (auth_commands) can reuse it.
pub fn verify_pin(stored_hash: &str, input_pin: &str) -> bool {
    // PLAIN: hashes should have been migrated at startup.
    // Accepting them at runtime is a security bypass — reject and require re-setup.
    if stored_hash.starts_with("PLAIN:") {
        tracing::error!("Un-migrated PLAIN: PIN hash detected at login. Login denied. Run rehash.");
        return false;
    }
    // Argon2id verification
    PasswordHash::new(stored_hash)
        .ok()
        .map(|parsed| {
            Argon2::default()
                .verify_password(input_pin.as_bytes(), &parsed)
                .is_ok()
        })
        .unwrap_or(false)
}

/// On startup, re-hash any legacy PLAIN:<pin> entries to argon2id so that
/// no cleartext PINs remain in the database after the first launch.
pub async fn rehash_plain_pins(pool: &SqlitePool) -> AppResult<()> {
    let rows = sqlx::query("SELECT user_id, pin_hash FROM users WHERE pin_hash LIKE 'PLAIN:%'")
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
         WHERE u.is_active = 1 ORDER BY u.display_name",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .iter()
        .map(|row| UserSummary {
            user_id: row.get("user_id"),
            display_name: row.get("display_name"),
            username: row.get("username"),
            role_name: row.get("role_name"),
        })
        .collect())
}

pub async fn login_pin(pool: &SqlitePool, username: &str, pin: &str) -> AppResult<SessionUser> {
    let row = sqlx::query(
        "SELECT u.user_id, u.branch_id, u.display_name, u.username, u.pin_hash, u.role_id, r.name as role_name,
                u.failed_pin_attempts, u.locked_until
         FROM users u JOIN roles r ON r.role_id = u.role_id
         WHERE u.username = ? AND u.is_active = 1",
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
                return Err(AppError::Permission(format!(
                    "Account locked. Try again in {} minute(s).",
                    remaining
                )));
            }
            // Lock expired — reset
            sqlx::query(
                "UPDATE users SET failed_pin_attempts = 0, locked_until = NULL WHERE user_id = ?",
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
            let locked_until =
                (chrono::Utc::now() + chrono::Duration::minutes(LOCKOUT_MINUTES)).to_rfc3339();
            sqlx::query(
                "UPDATE users SET failed_pin_attempts = ?, locked_until = ? WHERE user_id = ?",
            )
            .bind(new_attempts)
            .bind(&locked_until)
            .bind(&user_id)
            .execute(pool)
            .await?;
            return Err(AppError::Permission(format!(
                "Too many failed attempts. Account locked for {} minutes.",
                LOCKOUT_MINUTES
            )));
        } else {
            sqlx::query("UPDATE users SET failed_pin_attempts = ? WHERE user_id = ?")
                .bind(new_attempts)
                .bind(&user_id)
                .execute(pool)
                .await?;
            let remaining = MAX_ATTEMPTS - new_attempts;
            return Err(AppError::Permission(format!(
                "Invalid PIN. {} attempt(s) remaining.",
                remaining
            )));
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
        branch_id: row.get("branch_id"),
        display_name: row.get("display_name"),
        username: row.get("username"),
        role_id: row.get("role_id"),
        role_name: row.get("role_name"),
        session_token: String::new(),
        session_expires_at: String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Test: lockout threshold is exactly 5 attempts ──────────────────────────
    #[test]
    fn lockout_threshold_is_five() {
        assert_eq!(
            MAX_ATTEMPTS, 5,
            "Lockout must trigger after 5 failed attempts"
        );
    }

    // ── Test: lockout duration is 60 minutes (S-02 hardening) ──────────────────
    #[test]
    fn lockout_duration_is_sixty_minutes() {
        assert_eq!(LOCKOUT_MINUTES, 60, "Lockout must last 60 minutes (S-02)");
    }

    // ── Test: PLAIN: prefix is now rejected at login (security fix) ──────────
    #[test]
    fn verify_pin_plain_prefix_always_denied() {
        // PLAIN: hashes must have been migrated at startup.
        // They must be rejected at runtime regardless of whether the PIN matches.
        assert!(
            !verify_pin("PLAIN:1234", "1234"),
            "PLAIN: prefix must be rejected even with correct PIN"
        );
        assert!(
            !verify_pin("PLAIN:1234", "9999"),
            "PLAIN: prefix must be rejected with wrong PIN too"
        );
    }

    // ── Test: argon2id hash round-trip ────────────────────────────────────────
    #[test]
    fn hash_and_verify_pin_roundtrip() {
        let hashed = hash_pin("5678").expect("hash must succeed");
        assert!(
            verify_pin(&hashed, "5678"),
            "Argon2id hash must verify correctly"
        );
        assert!(
            !verify_pin(&hashed, "0000"),
            "Argon2id hash must reject wrong PIN"
        );
    }
}
