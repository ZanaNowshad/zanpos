//! Endpoint tests for the hub's REST surface.
//!
//! Kept beside `rest.rs` rather than inside it: the module was 739 lines, and
//! 210 of them were this. Declared with `#[path]` from `rest.rs` so `use
//! super::*` still resolves exactly as it did when it was inline.

use crate::sync_v2::client::HeartbeatOutcome;
use sqlx::Row;

#[test]
fn param_prefix_parsing() {
    assert_eq!(
        "gt.2026-01-01T00:00:00Z".strip_prefix("gt."),
        Some("2026-01-01T00:00:00Z")
    );
    assert_eq!("neq.DEV1".strip_prefix("neq."), Some("DEV1"));
}

#[tokio::test]
async fn authenticated_heartbeat_is_persisted_and_replay_cannot_move_it_backwards() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();

    let branch_id: String = sqlx::query_scalar(
        "SELECT branch_id FROM branches WHERE is_active = 1 ORDER BY created_at LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO devices
                (device_id, branch_id, device_code, name, status, is_active, created_at, updated_at)
             VALUES ('dev-heartbeat', ?, 'HB01', 'Heartbeat till', 'offline', 1,
                     '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .bind(&branch_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO hub_paired_devices
                (device_id, device_name, token_digest, paired_at)
             VALUES ('dev-heartbeat', 'Heartbeat till', ?, '2026-01-01T00:00:00Z')",
    )
    .bind(crate::hub::token_digest("store-token").to_vec())
    .execute(&pool)
    .await
    .unwrap();

    let handle = crate::hub::start_hub(pool.clone(), 0, "store-token")
        .await
        .unwrap();
    let client = crate::sync_v2::client::HttpSyncClient::new(
        &format!("http://127.0.0.1:{}", handle.port),
        "store-token",
        Some("dev-heartbeat"),
    );

    assert_eq!(
        client
            .heartbeat(7, "2026-01-01T00:00:00Z", "2.1.0")
            .await
            .unwrap(),
        HeartbeatOutcome::Accepted
    );

    let roster = client.terminal_roster().await.unwrap();
    let live = roster
        .iter()
        .find(|row| row.device_id == "dev-heartbeat")
        .unwrap();
    assert_eq!(live.state, "online");
    assert_eq!(live.observed_ip.as_deref(), Some("127.0.0.1"));

    let stored = sqlx::query(
        "SELECT last_heartbeat_at, heartbeat_sent_at, observed_ip, app_version, heartbeat_seq
               FROM devices WHERE device_id = 'dev-heartbeat'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let first_seen: String = stored.get("last_heartbeat_at");
    assert!(!first_seen.is_empty());
    assert_eq!(
        stored.get::<String, _>("heartbeat_sent_at"),
        "2026-01-01T00:00:00Z"
    );
    assert_eq!(stored.get::<String, _>("observed_ip"), "127.0.0.1");
    assert_eq!(stored.get::<String, _>("app_version"), "2.1.0");
    assert_eq!(stored.get::<i64, _>("heartbeat_seq"), 7);
    let paired_seen: Option<String> = sqlx::query_scalar(
        "SELECT last_seen_at FROM hub_paired_devices WHERE device_id = 'dev-heartbeat'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(paired_seen.as_deref(), Some(first_seen.as_str()));

    // A replayed beat is not silently "fine" any more — it is a stale
    // sequence, answered with the hub's current value so the caller can
    // catch up. Nothing may move.
    let stale = client
        .heartbeat(6, "2026-01-01T00:00:00Z", "old")
        .await
        .unwrap();
    assert_eq!(
        stale,
        HeartbeatOutcome::Stale {
            current_sequence: 7
        }
    );

    let after_replay = sqlx::query(
        "SELECT last_heartbeat_at, app_version, heartbeat_seq
               FROM devices WHERE device_id = 'dev-heartbeat'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        after_replay.get::<String, _>("last_heartbeat_at"),
        first_seen
    );
    assert_eq!(after_replay.get::<String, _>("app_version"), "2.1.0");
    assert_eq!(after_replay.get::<i64, _>("heartbeat_seq"), 7);

    sqlx::query(
        "UPDATE devices SET last_heartbeat_at = '2020-01-01T00:00:00Z'
              WHERE device_id = 'dev-heartbeat'",
    )
    .execute(&pool)
    .await
    .unwrap();
    let timed_out = client.terminal_roster().await.unwrap();
    assert_eq!(
        timed_out
            .iter()
            .find(|row| row.device_id == "dev-heartbeat")
            .unwrap()
            .state,
        "offline"
    );

    client
        .heartbeat(8, "2026-01-01T00:00:01Z", "2.2.0")
        .await
        .unwrap();
    let recovered = client.terminal_roster().await.unwrap();
    assert_eq!(
        recovered
            .iter()
            .find(|row| row.device_id == "dev-heartbeat")
            .unwrap()
            .state,
        "online"
    );
    let recovered_seen: String = sqlx::query_scalar(
        "SELECT last_heartbeat_at FROM devices WHERE device_id = 'dev-heartbeat'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    client
        .upsert_rows(
            "devices",
            &[serde_json::json!({
                "device_id": "dev-heartbeat",
                "branch_id": branch_id,
                "device_code": "HB01",
                "name": "Heartbeat till renamed",
                "status": "online",
                "is_active": 1,
                "next_receipt_seq": 1,
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2030-01-01T00:00:00Z",
                "last_heartbeat_at": null,
                "last_seen_at": null,
                "observed_ip": "10.0.0.99",
                "app_version": "forged",
                "heartbeat_seq": 1,
                "heartbeat_hub_id": "wrong-hub",
                "heartbeat_sent_at": "2000-01-01T00:00:00Z"
            })],
        )
        .await
        .unwrap();
    let after_device_push = sqlx::query(
        "SELECT last_heartbeat_at, heartbeat_sent_at, observed_ip, app_version, heartbeat_seq
               FROM devices WHERE device_id = 'dev-heartbeat'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        after_device_push.get::<String, _>("last_heartbeat_at"),
        recovered_seen
    );
    assert_eq!(
        after_device_push.get::<String, _>("heartbeat_sent_at"),
        "2026-01-01T00:00:01Z"
    );
    assert_eq!(
        after_device_push.get::<String, _>("observed_ip"),
        "127.0.0.1"
    );
    assert_eq!(after_device_push.get::<String, _>("app_version"), "2.2.0");
    assert_eq!(after_device_push.get::<i64, _>("heartbeat_seq"), 8);

    handle.shutdown();

    let restarted = crate::hub::start_hub(pool.clone(), 0, "store-token")
        .await
        .unwrap();
    let after_restart = crate::sync_v2::client::HttpSyncClient::new(
        &format!("http://127.0.0.1:{}", restarted.port),
        "store-token",
        Some("dev-heartbeat"),
    );
    assert_eq!(
        after_restart
            .terminal_roster()
            .await
            .unwrap()
            .iter()
            .find(|row| row.device_id == "dev-heartbeat")
            .unwrap()
            .state,
        "online"
    );
    after_restart
        .heartbeat(9, "2026-01-01T00:00:02Z", "2.2.0")
        .await
        .unwrap();
    let sequence: i64 =
        sqlx::query_scalar("SELECT heartbeat_seq FROM devices WHERE device_id = 'dev-heartbeat'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(sequence, 9);
    restarted.shutdown();
}

/// Every "your beat did not land" case has a distinct, actionable answer:
/// 404 = the hub holds no row at all (re-register), 410 = the row is
/// turned off, 403 = the pairing was revoked, and a stale sequence is a
/// 409 carrying the hub's current value rather than a silent 204.
#[tokio::test]
async fn heartbeat_rejections_name_the_reason() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();

    let branch_id: String = sqlx::query_scalar(
        "SELECT branch_id FROM branches WHERE is_active = 1 ORDER BY created_at LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    // All three share the legacy store token — per-device pairing is not
    // required for the upgrade path, but revocation must still win.
    let handle = crate::hub::start_hub(pool.clone(), 0, "store-token")
        .await
        .unwrap();
    let client = crate::sync_v2::client::HttpSyncClient::new(
        &format!("http://127.0.0.1:{}", handle.port),
        "store-token",
        Some("dev-absent"),
    );

    // 404: the hub has never heard of this device.
    let err = client
        .heartbeat(1, "2026-01-01T00:00:00Z", "2.0.0")
        .await
        .unwrap_err();
    assert!(
        matches!(err, crate::errors::AppError::NotFound(_)),
        "{err:?}"
    );

    // 410: the row exists but is deactivated.
    sqlx::query(
        "INSERT INTO devices
                (device_id, branch_id, device_code, name, status, is_active, created_at, updated_at)
             VALUES ('dev-off', ?, 'OFF01', 'till', 'offline', 0,
                     '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .bind(&branch_id)
    .execute(&pool)
    .await
    .unwrap();
    let off = crate::sync_v2::client::HttpSyncClient::new(
        &format!("http://127.0.0.1:{}", handle.port),
        "store-token",
        Some("dev-off"),
    );
    let err = off
        .heartbeat(1, "2026-01-01T00:00:00Z", "2.0.0")
        .await
        .unwrap_err();
    assert!(
        matches!(err, crate::errors::AppError::Validation(_)),
        "{err:?}"
    );
    assert!(err.to_string().contains("deactivated"), "{err:?}");

    // 409: a stale sequence is answered with the current value.
    sqlx::query(
        "INSERT INTO devices
                (device_id, branch_id, device_code, name, status, is_active, created_at, updated_at)
             VALUES ('dev-stale', ?, 'STL01', 'till', 'offline', 1,
                     '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .bind(&branch_id)
    .execute(&pool)
    .await
    .unwrap();
    let stale = crate::sync_v2::client::HttpSyncClient::new(
        &format!("http://127.0.0.1:{}", handle.port),
        "store-token",
        Some("dev-stale"),
    );
    assert_eq!(
        stale
            .heartbeat(10, "2026-01-01T00:00:00Z", "2.0.0")
            .await
            .unwrap(),
        HeartbeatOutcome::Accepted
    );
    assert_eq!(
        stale
            .heartbeat(3, "2026-01-01T00:00:00Z", "2.0.0")
            .await
            .unwrap(),
        HeartbeatOutcome::Stale {
            current_sequence: 10
        }
    );

    // 403: a revoked pairing beats even the legacy shared token.
    sqlx::query(
        "INSERT INTO devices
                (device_id, branch_id, device_code, name, status, is_active, created_at, updated_at)
             VALUES ('dev-revoked', ?, 'REV01', 'till', 'offline', 1,
                     '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .bind(&branch_id)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO hub_paired_devices
                (device_id, device_name, token_digest, paired_at, revoked_at)
             VALUES ('dev-revoked', 'revoked till', ?, '2026-01-01T00:00:00Z',
                     '2026-01-02T00:00:00Z')",
    )
    .bind(crate::hub::token_digest("other-token").to_vec())
    .execute(&pool)
    .await
    .unwrap();
    let revoked = crate::sync_v2::client::HttpSyncClient::new(
        &format!("http://127.0.0.1:{}", handle.port),
        "store-token",
        Some("dev-revoked"),
    );
    let err = revoked
        .heartbeat(1, "2026-01-01T00:00:00Z", "2.0.0")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("credentials"), "{err:?}");
    // And nothing moved: the revoked device never looked alive.
    let seen: Option<String> =
        sqlx::query_scalar("SELECT last_seen_at FROM devices WHERE device_id = 'dev-revoked'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(seen.is_none());

    handle.shutdown();
}
