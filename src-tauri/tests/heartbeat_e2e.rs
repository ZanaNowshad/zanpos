//! End-to-end: heartbeat lifecycle and the re-registration path it feeds,
//! terminal <-> embedded hub over real HTTP on 127.0.0.1.
//!
//! The worker's internal recovery (409 catch-up, self-beat) is unit-tested in
//! `sync_v2::worker::tests`; this file covers the wire halves a unit test
//! cannot — what the hub persists, and what the real push path does when a
//! heartbeat answers 404.
use sqlx::SqlitePool;
use zanpos_lib::hub;
use zanpos_lib::sync_v2::client::{HeartbeatOutcome, HttpSyncClient};
use zanpos_lib::sync_v2::worker::SyncWorker;

async fn fresh_db(tag: &str) -> SqlitePool {
    let path = std::env::temp_dir().join(format!("zanpos_hb_{tag}_{}.db", ulid::Ulid::new()));
    let pool = SqlitePool::connect(&format!("sqlite:{}?mode=rwc", path.display()))
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    pool
}

async fn set_cfg(pool: &SqlitePool, k: &str, v: &str) {
    sqlx::query(
        "INSERT INTO app_config(key,value,updated_at) VALUES (?,?,datetime('now'))
                 ON CONFLICT(key) DO UPDATE SET value=excluded.value",
    )
    .bind(k)
    .bind(v)
    .execute(pool)
    .await
    .unwrap();
}

async fn register_device(pool: &SqlitePool, device_id: &str, code: &str) -> String {
    let branch_id: String = sqlx::query_scalar("SELECT branch_id FROM branches LIMIT 1")
        .fetch_one(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO devices (device_id, branch_id, device_code, name, status, is_active, created_at, updated_at)
         VALUES (?, ?, ?, 'Till', 'offline', 1, datetime('now'), datetime('now'))",
    )
    .bind(device_id)
    .bind(&branch_id)
    .bind(code)
    .execute(pool)
    .await
    .unwrap();
    branch_id
}

/// A beat that lost the sequence race is not a silent success: the hub names
/// its current value, and the terminal's next beat catches up and lands.
#[tokio::test]
async fn a_stale_sequence_is_named_and_catch_up_lands() {
    let hub_pool = fresh_db("stale").await;
    let token = "stale-token-0123456789abcdef";
    register_device(&hub_pool, "DEV-STALE", "STL01").await;
    let handle = hub::start_hub(hub_pool.clone(), 0, token).await.unwrap();
    let client = HttpSyncClient::new(
        &format!("http://127.0.0.1:{}", handle.port),
        token,
        Some("DEV-STALE"),
    );

    assert_eq!(
        client
            .heartbeat(10, "2026-01-01T00:00:00Z", "2.0.0")
            .await
            .unwrap(),
        HeartbeatOutcome::Accepted
    );
    // A delayed/replayed beat (or a sibling sharing the identity) gets the
    // diagnosis, not a 204.
    assert_eq!(
        client
            .heartbeat(3, "2026-01-01T00:00:00Z", "2.0.0")
            .await
            .unwrap(),
        HeartbeatOutcome::Stale {
            current_sequence: 10
        }
    );
    // Catch up and the next beat is fresh again.
    assert_eq!(
        client
            .heartbeat(11, "2026-01-01T00:00:01Z", "2.0.0")
            .await
            .unwrap(),
        HeartbeatOutcome::Accepted
    );
    let seq: i64 =
        sqlx::query_scalar("SELECT heartbeat_seq FROM devices WHERE device_id='DEV-STALE'")
            .fetch_one(&hub_pool)
            .await
            .unwrap();
    assert_eq!(seq, 11);
    let skew_reported: Option<String> =
        sqlx::query_scalar("SELECT heartbeat_sent_at FROM devices WHERE device_id='DEV-STALE'")
            .fetch_one(&hub_pool)
            .await
            .unwrap();
    assert_eq!(skew_reported.as_deref(), Some("2026-01-01T00:00:01Z"));
    handle.shutdown();
}

/// The full recovery the heartbeat loop drives on a 404: the terminal's device
/// row is re-offered through the normal push path, and the next beat lands on
/// the freshly registered row.
#[tokio::test]
async fn a_terminal_that_gets_404_re_registers_its_device_row_and_beats_again() {
    let hub_pool = fresh_db("rereg-hub").await;
    let term_pool = fresh_db("rereg-term").await;
    let token = "rereg-token-0123456789abcdef";
    set_cfg(&hub_pool, "setup_complete", "1").await;
    set_cfg(&term_pool, "setup_complete", "1").await;
    set_cfg(&term_pool, "device_id", "DEV-REREG").await;
    register_device(&term_pool, "DEV-REREG", "REG01").await;

    let handle = hub::start_hub(hub_pool.clone(), 0, token).await.unwrap();
    let url = format!("http://127.0.0.1:{}", handle.port);
    let client = HttpSyncClient::new(&url, token, Some("DEV-REREG"));

    // The hub holds no row for this device.
    let err = client
        .heartbeat(1, "2026-01-01T00:00:00Z", "2.0.0")
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("not registered"),
        "a missing device row must answer as unregistered: {err:?}"
    );

    // What the heartbeat loop does with that answer: mark the local row
    // pending so the next data cycle re-registers it.
    sqlx::query(
        "UPDATE devices SET sync_status='pending', sync_attempts=0 WHERE device_id='DEV-REREG'",
    )
    .execute(&term_pool)
    .await
    .unwrap();
    let worker = SyncWorker::new(term_pool.clone());
    worker.run_once_with(&client, "DEV-REREG").await;

    let hub_row: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE device_id='DEV-REREG'")
            .fetch_one(&hub_pool)
            .await
            .unwrap();
    assert_eq!(
        hub_row, 1,
        "the device row must reach the hub through the push path"
    );

    // And the heartbeat that follows now lands — liveness is restored without
    // any manual database repair.
    assert_eq!(
        client
            .heartbeat(2, "2026-01-01T00:00:00Z", "2.0.0")
            .await
            .unwrap(),
        HeartbeatOutcome::Accepted
    );
    let seen: Option<String> =
        sqlx::query_scalar("SELECT last_seen_at FROM devices WHERE device_id='DEV-REREG'")
            .fetch_one(&hub_pool)
            .await
            .unwrap();
    assert!(
        seen.is_some(),
        "the re-registered device must now look alive"
    );
    handle.shutdown();
}

/// `loyalty_events` fell out of the hand-kept queue tools and the join wipe;
/// this pins the business half — a loyalty event minted on one terminal
/// converges onto the other through the hub.
#[tokio::test]
async fn loyalty_events_replicate_between_terminals() {
    let hub_pool = fresh_db("loyalty-hub").await;
    let terminal_a = fresh_db("loyalty-a").await;
    let terminal_b = fresh_db("loyalty-b").await;
    let token = "loyalty-token-0123456789ab";
    set_cfg(&hub_pool, "setup_complete", "1").await;
    set_cfg(&terminal_a, "setup_complete", "1").await;
    set_cfg(&terminal_b, "setup_complete", "1").await;

    let handle = hub::start_hub(hub_pool.clone(), 0, token).await.unwrap();
    let url = format!("http://127.0.0.1:{}", handle.port);
    let client_a = HttpSyncClient::new(&url, token, Some("LOYAL-A"));
    let client_b = HttpSyncClient::new(&url, token, Some("LOYAL-B"));

    // Terminal A mints a customer and a loyalty event for it.
    let branch_id: String = sqlx::query_scalar("SELECT branch_id FROM branches LIMIT 1")
        .fetch_one(&terminal_a)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO customers (customer_id, branch_id, name, phone, created_at, updated_at, sync_status)
         VALUES ('CUST-LOYAL', ?, 'Loyal Customer', '33112244', datetime('now'), datetime('now'), 'pending')",
    )
    .bind(&branch_id)
    .execute(&terminal_a)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO loyalty_events (loyalty_event_id, customer_id, branch_id, device_id, origin_device_id,
                                     event_type, points_delta, points_after, created_at, updated_at, sync_status)
         VALUES ('LOYAL-EVT-1', 'CUST-LOYAL', ?, 'LOYAL-A', 'LOYAL-A',
                 'earn', 50, 50, datetime('now'), datetime('now'), 'pending')",
    )
    .bind(&branch_id)
    .execute(&terminal_a)
    .await
    .unwrap();

    let worker_a = SyncWorker::new(terminal_a.clone());
    worker_a.run_once_with(&client_a, "LOYAL-A").await;

    let on_hub: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM loyalty_events WHERE loyalty_event_id='LOYAL-EVT-1'",
    )
    .fetch_one(&hub_pool)
    .await
    .unwrap();
    assert_eq!(on_hub, 1, "the loyalty event must reach the hub");

    // Terminal B pulls it and holds the actual row, not just an Ok return.
    let worker_b = SyncWorker::new(terminal_b.clone());
    worker_b.run_once_with(&client_b, "LOYAL-B").await;
    let (delta, customer): (i64, String) = sqlx::query_as(
        "SELECT points_delta, customer_id FROM loyalty_events WHERE loyalty_event_id='LOYAL-EVT-1'",
    )
    .fetch_one(&terminal_b)
    .await
    .unwrap();
    assert_eq!(delta, 50);
    assert_eq!(customer, "CUST-LOYAL");
    handle.shutdown();
}
