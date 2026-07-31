//! Per-device pairing for the LAN hub.
//!
//! The hub binds `0.0.0.0:8923`, so anything on the shop WiFi can reach it.
//! Authentication was a single shared store token: good enough to stop a
//! stranger browsing, but it could not distinguish one till from another and
//! could not be withdrawn from a single lost device without re-keying every
//! terminal in the shop.
//!
//! This adds a token per paired device. Only the SHA-256 digest is stored —
//! the raw token is returned once, at pairing, and never written down.
//!
//! Deliberately kept alongside the legacy shared token rather than replacing
//! it: a store that upgrades mid-shift must not lose its second till. The
//! legacy path logs when used so it can be retired once every device is
//! paired.

use crate::errors::{AppError, AppResult};
use serde::Serialize;
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;

/// device_id -> token digest, for devices that are paired and not revoked.
/// Held in memory because `check_auth` is synchronous and runs on every
/// request; a DB round-trip per request on a LAN till is not acceptable.
pub type PairedDevices = HashMap<String, [u8; 32]>;

#[derive(Debug, Serialize)]
pub struct PairedDeviceRow {
    pub device_id: String,
    pub device_name: String,
    pub paired_at: String,
    pub last_seen_at: Option<String>,
    pub revoked_at: Option<String>,
}

/// Loads every live pairing. Called on hub start and after any pair/revoke.
pub async fn load_live(pool: &SqlitePool) -> PairedDevices {
    let rows = sqlx::query(
        "SELECT device_id, token_digest FROM hub_paired_devices WHERE revoked_at IS NULL",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let mut map = PairedDevices::new();
    for row in rows {
        let id: String = row.get("device_id");
        let digest: Vec<u8> = row.get("token_digest");
        // A digest that is not 32 bytes is corrupt; skipping it fails closed
        // (that device simply cannot authenticate) rather than panicking on a
        // slice conversion inside the hub's startup path.
        if let Ok(fixed) = <[u8; 32]>::try_from(digest.as_slice()) {
            map.insert(id, fixed);
        } else {
            tracing::warn!("hub pairing: device {id} has a malformed digest, ignoring");
        }
    }
    map
}

/// Generates a token, stores only its digest, and returns the raw token ONCE.
///
/// Re-pairing an existing device replaces its token, which is also how a
/// device that lost its token recovers without a new identity.
pub async fn pair_device(
    pool: &SqlitePool,
    device_id: &str,
    device_name: &str,
) -> AppResult<String> {
    if device_id.trim().is_empty() {
        return Err(AppError::Validation("A device id is required".into()));
    }
    if device_name.trim().is_empty() {
        return Err(AppError::Validation("A device name is required".into()));
    }

    use rand::Rng;
    let token: String = rand::thread_rng()
        .sample_iter(rand::distributions::Alphanumeric)
        .take(48)
        .map(char::from)
        .collect();
    let digest = super::token_digest(&token);
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO hub_paired_devices (device_id, device_name, token_digest, paired_at, revoked_at)
         VALUES (?, ?, ?, ?, NULL)
         ON CONFLICT(device_id) DO UPDATE SET
             device_name = excluded.device_name,
             token_digest = excluded.token_digest,
             paired_at = excluded.paired_at,
             revoked_at = NULL",
    )
    .bind(device_id)
    .bind(device_name)
    .bind(digest.to_vec())
    .bind(&now)
    .execute(pool)
    .await?;

    Ok(token)
}

/// Revokes a device. The row is kept so the pairing stays auditable.
pub async fn revoke_device(pool: &SqlitePool, device_id: &str) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE hub_paired_devices SET revoked_at = ? WHERE device_id = ? AND revoked_at IS NULL",
    )
    .bind(&now)
    .bind(device_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_devices(pool: &SqlitePool) -> AppResult<Vec<PairedDeviceRow>> {
    let rows = sqlx::query(
        "SELECT device_id, device_name, paired_at, last_seen_at, revoked_at
         FROM hub_paired_devices ORDER BY paired_at DESC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| PairedDeviceRow {
            device_id: row.get("device_id"),
            device_name: row.get("device_name"),
            paired_at: row.get("paired_at"),
            last_seen_at: row.get("last_seen_at"),
            revoked_at: row.get("revoked_at"),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("memory db");
        sqlx::query(
            "CREATE TABLE hub_paired_devices (
                device_id TEXT PRIMARY KEY NOT NULL, device_name TEXT NOT NULL,
                token_digest BLOB NOT NULL, paired_at TEXT NOT NULL,
                last_seen_at TEXT, revoked_at TEXT)",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    #[tokio::test]
    async fn pairing_returns_a_token_whose_digest_is_what_gets_stored() {
        let pool = pool().await;
        let token = pair_device(&pool, "D1", "Till 2").await.unwrap();
        let live = load_live(&pool).await;
        assert_eq!(live.get("D1"), Some(&super::super::token_digest(&token)));
    }

    #[tokio::test]
    async fn the_raw_token_is_never_stored() {
        let pool = pool().await;
        let token = pair_device(&pool, "D1", "Till 2").await.unwrap();
        let stored: Vec<u8> = sqlx::query("SELECT token_digest FROM hub_paired_devices")
            .fetch_one(&pool)
            .await
            .unwrap()
            .get("token_digest");
        assert_ne!(
            stored,
            token.as_bytes(),
            "raw token must never be persisted"
        );
        assert_eq!(stored.len(), 32);
    }

    #[tokio::test]
    async fn a_revoked_device_disappears_from_the_live_set() {
        let pool = pool().await;
        pair_device(&pool, "D1", "Till 2").await.unwrap();
        revoke_device(&pool, "D1").await.unwrap();
        assert!(load_live(&pool).await.is_empty());
        // The row survives for audit.
        assert_eq!(list_devices(&pool).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn re_pairing_replaces_the_token_and_clears_revocation() {
        let pool = pool().await;
        let first = pair_device(&pool, "D1", "Till 2").await.unwrap();
        revoke_device(&pool, "D1").await.unwrap();
        let second = pair_device(&pool, "D1", "Till 2").await.unwrap();
        assert_ne!(first, second);
        let live = load_live(&pool).await;
        assert_eq!(live.get("D1"), Some(&super::super::token_digest(&second)));
    }

    #[tokio::test]
    async fn empty_identifiers_are_rejected_at_the_boundary() {
        let pool = pool().await;
        assert!(pair_device(&pool, "  ", "Till").await.is_err());
        assert!(pair_device(&pool, "D1", " ").await.is_err());
    }
}
