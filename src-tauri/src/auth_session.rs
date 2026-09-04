use crate::errors::{AppError, AppResult};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Utc};
use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};
use std::{collections::HashMap, time::Duration};
use tokio::sync::RwLock;

pub const SESSION_TTL: Duration = Duration::from_secs(12 * 60 * 60);
const TOKEN_BYTES: usize = 32;
const MAX_TOKEN_CHARS: usize = 64;

#[derive(Debug, Clone)]
struct SessionRecord {
    user_id: String,
    expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct IssuedSession {
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct AuthenticatedActor {
    pub user_id: String,
    pub branch_id: String,
    pub role_name: String,
}

pub struct SessionStore {
    sessions: RwLock<HashMap<[u8; 32], SessionRecord>>,
    ttl: Duration,
}

impl Default for SessionStore {
    fn default() -> Self {
        Self::new(SESSION_TTL)
    }
}

impl SessionStore {
    pub fn new(ttl: Duration) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            ttl,
        }
    }

    pub async fn issue(&self, user_id: &str) -> IssuedSession {
        let now = Utc::now();
        self.purge_expired(now).await;
        let mut raw = [0_u8; TOKEN_BYTES];
        OsRng.fill_bytes(&mut raw);
        let token = URL_SAFE_NO_PAD.encode(raw);
        let key = token_hash(&raw);
        let expires_at = now
            + chrono::Duration::from_std(self.ttl).unwrap_or_else(|_| chrono::Duration::hours(12));
        self.sessions.write().await.insert(
            key,
            SessionRecord {
                user_id: user_id.to_owned(),
                expires_at,
            },
        );
        IssuedSession { token, expires_at }
    }

    pub async fn revoke(&self, token: &str) -> AppResult<bool> {
        let key = parse_token(token)?;
        self.purge_expired(Utc::now()).await;
        Ok(self.sessions.write().await.remove(&key).is_some())
    }

    pub async fn resolve_office(
        &self,
        pool: &SqlitePool,
        token: &str,
    ) -> AppResult<AuthenticatedActor> {
        let actor = self.resolve_active_actor(pool, token).await?;
        if matches!(actor.role_name.as_str(), "manager" | "owner") {
            Ok(actor)
        } else {
            Err(AppError::Permission(
                "Office AI requires a manager or owner".into(),
            ))
        }
    }

    pub async fn resolve_ai(
        &self,
        pool: &SqlitePool,
        token: &str,
    ) -> AppResult<AuthenticatedActor> {
        let actor = self.resolve_active_actor(pool, token).await?;
        if matches!(actor.role_name.as_str(), "cashier" | "manager" | "owner") {
            Ok(actor)
        } else {
            Err(AppError::Permission(
                "ZanAI requires an active POS role".into(),
            ))
        }
    }

    /// Resolve the authenticated caller from a session token, with no role
    /// policy applied — the caller decides which roles it accepts.
    ///
    /// Every field of the returned actor is server-derived: the token indexes
    /// an in-memory session this process issued at login, and the branch and
    /// role are read from `users`/`roles` at resolve time. Nothing in a request
    /// payload can influence any of them. This is the distinction that
    /// `commands::rbac::require_role` cannot make — it is handed a user id and
    /// looks up whatever it is given, so it answers "does this id hold the
    /// role", never "is the caller that user".
    pub async fn resolve(
        &self,
        pool: &SqlitePool,
        token: &str,
    ) -> AppResult<AuthenticatedActor> {
        self.resolve_active_actor(pool, token).await
    }

    async fn resolve_active_actor(
        &self,
        pool: &SqlitePool,
        token: &str,
    ) -> AppResult<AuthenticatedActor> {
        let key = parse_token(token)?;
        let now = Utc::now();
        self.purge_expired(now).await;
        let user_id = self
            .sessions
            .read()
            .await
            .get(&key)
            .filter(|record| record.expires_at > now)
            .map(|record| record.user_id.clone())
            .ok_or_else(|| AppError::Permission("Session is invalid or expired".into()))?;

        let row = sqlx::query(
            "SELECT u.user_id, u.branch_id, r.name AS role_name
             FROM users u JOIN roles r ON r.role_id = u.role_id
             WHERE u.user_id = ? AND u.is_active = 1
               ",
        )
        .bind(&user_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::Permission("User is inactive or unavailable".into()))?;

        Ok(AuthenticatedActor {
            user_id: row.get("user_id"),
            branch_id: row.get("branch_id"),
            role_name: row.get("role_name"),
        })
    }

    async fn purge_expired(&self, now: DateTime<Utc>) {
        self.sessions
            .write()
            .await
            .retain(|_, record| record.expires_at > now);
    }
}

fn parse_token(token: &str) -> AppResult<[u8; 32]> {
    if token.is_empty() || token.len() > MAX_TOKEN_CHARS {
        return Err(AppError::Validation("Invalid session token".into()));
    }
    let raw = URL_SAFE_NO_PAD
        .decode(token)
        .map_err(|_| AppError::Validation("Invalid session token".into()))?;
    if raw.len() != TOKEN_BYTES {
        return Err(AppError::Validation("Invalid session token".into()));
    }
    Ok(token_hash(&raw))
}

fn token_hash(raw: &[u8]) -> [u8; 32] {
    Sha256::digest(raw).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;
    use std::time::Duration;

    async fn pool() -> sqlx::SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query("UPDATE users SET is_active = 1 WHERE user_id = '01JUSER000000000000ADMIN1'")
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    #[tokio::test]
    async fn issued_tokens_are_opaque_unique_and_revocable() {
        let store = SessionStore::new(Duration::from_secs(60));
        let first = store.issue("01JUSER000000000000ADMIN1").await;
        let second = store.issue("01JUSER000000000000ADMIN1").await;

        assert_ne!(first.token, second.token);
        assert!(!first.token.contains("ADMIN1"));
        assert!(store.revoke(&first.token).await.unwrap());
        assert!(!store.revoke(&first.token).await.unwrap());
        let pool = pool().await;
        assert!(matches!(
            store.resolve_office(&pool, &first.token).await,
            Err(crate::errors::AppError::Permission(_))
        ));
    }

    #[tokio::test]
    async fn office_resolution_accepts_manager_owner_and_rejects_cashier() {
        let pool = pool().await;
        sqlx::query(
            "INSERT INTO users
             (user_id, branch_id, display_name, username, pin_hash, role_id, is_active,
              created_at, updated_at, version)
             VALUES ('MANAGER1', '01JBRANCH0000000000000001', 'Manager', 'manager1',
                     'unused', '01JROLES000000000000000002', 1,
                     '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z', 1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        let store = SessionStore::new(Duration::from_secs(60));
        let owner = store.issue("01JUSER000000000000ADMIN1").await;
        let manager = store.issue("MANAGER1").await;
        let cashier = store.issue("01JUSER000000000000CASH01").await;

        let actor = store.resolve_office(&pool, &owner.token).await.unwrap();
        assert_eq!(actor.user_id, "01JUSER000000000000ADMIN1");
        assert_eq!(actor.role_name, "owner");
        assert_eq!(
            store
                .resolve_office(&pool, &manager.token)
                .await
                .unwrap()
                .role_name,
            "manager"
        );
        assert!(matches!(
            store.resolve_office(&pool, &cashier.token).await,
            Err(crate::errors::AppError::Permission(_))
        ));

        sqlx::query(
            "UPDATE users SET role_id = '01JROLES000000000000000003' WHERE user_id = 'MANAGER1'",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(matches!(
            store.resolve_office(&pool, &manager.token).await,
            Err(crate::errors::AppError::Permission(_))
        ));

        sqlx::query(
            "UPDATE users SET role_id = '01JROLES000000000000000002', is_active = 0,
                              branch_id = 'NEW_BRANCH' WHERE user_id = 'MANAGER1'",
        )
        .execute(&pool)
        .await
        .unwrap();
        assert!(matches!(
            store.resolve_office(&pool, &manager.token).await,
            Err(crate::errors::AppError::Permission(_))
        ));

        sqlx::query("UPDATE users SET is_active = 1 WHERE user_id = 'MANAGER1'")
            .execute(&pool)
            .await
            .unwrap();
        let moved = store.resolve_office(&pool, &manager.token).await.unwrap();
        assert_eq!(moved.branch_id, "NEW_BRANCH");
    }

    #[tokio::test]
    async fn ai_resolution_accepts_active_cashier_without_weakening_office_resolution() {
        let pool = pool().await;
        let store = SessionStore::new(Duration::from_secs(60));
        let cashier = store.issue("01JUSER000000000000CASH01").await;

        assert_eq!(
            store
                .resolve_ai(&pool, &cashier.token)
                .await
                .unwrap()
                .role_name,
            "cashier"
        );
        assert!(matches!(
            store.resolve_office(&pool, &cashier.token).await,
            Err(crate::errors::AppError::Permission(_))
        ));
    }

    #[tokio::test]
    async fn expired_and_malformed_tokens_are_denied() {
        let pool = pool().await;
        let store = SessionStore::new(Duration::ZERO);
        let expired = store.issue("01JUSER000000000000ADMIN1").await;

        assert!(matches!(
            store.resolve_office(&pool, &expired.token).await,
            Err(crate::errors::AppError::Permission(_))
        ));
        assert!(matches!(
            store.resolve_office(&pool, "not base64 !!!").await,
            Err(crate::errors::AppError::Validation(_))
        ));
    }
}
