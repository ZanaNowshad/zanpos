use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};
use ulid::Ulid;

struct OverrideToken {
    manager_user_id: String,
    expires_at: SystemTime,
}

static OVERRIDE_TOKENS: std::sync::LazyLock<Mutex<HashMap<String, OverrideToken>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

/// Issue a short-lived override token (60s TTL). Persists to `app_config` so the
/// token survives a process restart, and also caches in memory for fast-path lookup.
pub async fn store_override_token(pool: &sqlx::SqlitePool, manager_user_id: String) -> String {
    let token = Ulid::new().to_string();
    let expires_at = SystemTime::now() + Duration::from_secs(60);
    let expires_secs = expires_at
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string();

    // In-memory cache (fast path) — drop lock before DB await
    {
        let mut map = OVERRIDE_TOKENS.lock().unwrap_or_else(|e| e.into_inner());
        map.insert(
            token.clone(),
            OverrideToken {
                manager_user_id: manager_user_id.clone(),
                expires_at,
            },
        );
    }

    // DB persistence (survives process restart)
    let value = serde_json::json!({
        "manager_user_id": manager_user_id,
        "expires_at_secs": expires_secs,
    });
    let now = chrono::Utc::now().to_rfc3339();
    let key = format!("override_token_{token}");
    let _ = sqlx::query(
        "INSERT INTO app_config (key, value, updated_at) VALUES (?, ?, ?)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at",
    )
    .bind(&key)
    .bind(value.to_string())
    .bind(&now)
    .execute(pool)
    .await;

    token
}

/// Consume a previously-issued override token. Returns manager_user_id if valid.
/// Checks in-memory cache first, then falls back to DB (for restart-surviving tokens).
/// Deletes the token from both stores on successful use.
pub async fn consume_override_token(pool: &sqlx::SqlitePool, token: &str) -> Option<String> {
    // 1. In-memory cache (fast path)
    let cache_hit: Option<String> = {
        let mut map = OVERRIDE_TOKENS.lock().unwrap_or_else(|e| e.into_inner());
        let now = SystemTime::now();
        map.retain(|_, t| t.expires_at > now);
        map.remove(token).map(|t| t.manager_user_id)
    };
    if let Some(ref manager_id) = cache_hit {
        // Clean up DB entry (best-effort, outside lock scope)
        let key = format!("override_token_{token}");
        let _ = sqlx::query("DELETE FROM app_config WHERE key = ?")
            .bind(&key)
            .execute(pool)
            .await;
        return Some(manager_id.clone());
    }

    // 2. DB fallback (token may have been issued before a restart)
    let key = format!("override_token_{token}");
    let row: Option<String> = sqlx::query_scalar("SELECT value FROM app_config WHERE key = ?")
        .bind(&key)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten();

    if let Some(raw) = row {
        // Parse the JSON value
        let parsed: Option<serde_json::Value> = serde_json::from_str(&raw).ok();
        if let Some(obj) = parsed {
            let manager_user_id = obj.get("manager_user_id").and_then(|v| v.as_str());
            let expires_secs = obj
                .get("expires_at_secs")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse::<u64>().ok());

            if let (Some(uid), Some(exp)) = (manager_user_id, expires_secs) {
                let exp_dur = Duration::from_secs(exp);
                let now = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default();
                if exp_dur > now {
                    // Token is still valid — delete it (one-shot) and return
                    let _ = sqlx::query("DELETE FROM app_config WHERE key = ?")
                        .bind(&key)
                        .execute(pool)
                        .await;
                    return Some(uid.to_string());
                }
            }
        }
        // Token expired or invalid — clean up stale DB entry
        let _ = sqlx::query("DELETE FROM app_config WHERE key = ?")
            .bind(&key)
            .execute(pool)
            .await;
    }

    None
}
