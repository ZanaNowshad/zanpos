use sqlx::SqlitePool;

async fn pool() -> SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    pool
}

/// The reported defect, at the level an operator sees it.
///
/// POS-7757Z showed "online" with its last-seen and IP never populated,
/// because status was a stored string nobody updated. A terminal that has never
/// checked in must never read as online on this screen.
#[tokio::test]
async fn a_terminal_that_never_checked_in_is_not_online() {
    let pool = pool().await;
    let branch: String = sqlx::query_scalar("SELECT branch_id FROM branches LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    // Seeded with status 'online' deliberately: the old bug was trusting it.
    sqlx::query(
        "INSERT INTO devices (device_id, branch_id, device_code, name, status, is_active,
             created_at, updated_at)
         VALUES ('dev_never', ?, 'POS-7757Z', 'Till 3', 'online', 1,
             '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .bind(&branch)
    .execute(&pool)
    .await
    .unwrap();
    // Bound: this installation IS that device. That is the reported case — the
    // record was correctly registered and had still never reached the hub, so
    // "unpaired" would send somebody to re-register a device that is fine.
    sqlx::query(
        "INSERT INTO app_config (key, value, updated_at)
         VALUES ('device_id', 'dev_never', '2026-01-01T00:00:00Z')
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    )
    .execute(&pool)
    .await
    .unwrap();

    let roster = crate::commands::device_state::roster(&pool).await.unwrap();
    let row = roster.iter().find(|r| r.device_code == "POS-7757Z").unwrap();

    assert_eq!(row.state, "never_seen", "a stored status was believed again");
    assert_ne!(row.state, "online");
    assert_eq!(row.seconds_since_seen, None);
    // Every state carries what to do about it, so the screen cannot invent its
    // own wording for a meaning defined in device_state.
    assert!(row.advice.contains("never checked in"), "{}", row.advice);
}

#[tokio::test]
async fn a_hub_paired_terminal_without_a_heartbeat_reads_as_never_seen() {
    let pool = pool().await;
    let branch: String = sqlx::query_scalar("SELECT branch_id FROM branches LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO devices
            (device_id, branch_id, device_code, name, status, is_active, created_at, updated_at)
         VALUES ('dev_paired', ?, 'POS05', 'Till 5', 'online', 1,
                 '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .bind(branch)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO hub_paired_devices
            (device_id, device_name, token_digest, paired_at)
         VALUES ('dev_paired', 'Till 5', X'00', '2026-01-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let roster = crate::commands::device_state::roster(&pool).await.unwrap();
    let row = roster.iter().find(|row| row.device_id == "dev_paired").unwrap();
    assert_eq!(row.state, "never_seen");
}

#[tokio::test]
async fn a_terminal_that_beat_recently_reads_as_online() {
    let pool = pool().await;
    let branch: String = sqlx::query_scalar("SELECT branch_id FROM branches LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    let recent = (chrono::Utc::now() - chrono::Duration::seconds(20)).to_rfc3339();
    sqlx::query(
        "INSERT INTO devices (device_id, branch_id, device_code, name, status, is_active,
             last_heartbeat_at, heartbeat_seq, observed_ip, app_version, created_at, updated_at)
         VALUES ('dev_live', ?, 'POS02', 'Till 2', 'offline', 1, ?, 12, '192.168.1.51', '2.0.0',
             '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .bind(&branch)
    .bind(&recent)
    .execute(&pool)
    .await
    .unwrap();

    let roster = crate::commands::device_state::roster(&pool).await.unwrap();
    let row = roster.iter().find(|r| r.device_code == "POS02").unwrap();

    // Stored status says 'offline'; the evidence says otherwise and wins.
    assert_eq!(row.state, "online");
    assert_eq!(row.observed_ip.as_deref(), Some("192.168.1.51"));
}

/// A till mid-reboot and a till that died look identical for a few minutes.
/// Collapsing them sends somebody to the shop floor for a terminal that is
/// already coming back.
#[tokio::test]
async fn a_recent_gap_reads_as_stale_rather_than_offline() {
    let pool = pool().await;
    let branch: String = sqlx::query_scalar("SELECT branch_id FROM branches LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    let while_ago = (chrono::Utc::now() - chrono::Duration::minutes(5)).to_rfc3339();
    sqlx::query(
        "INSERT INTO devices (device_id, branch_id, device_code, name, status, is_active,
             last_heartbeat_at, heartbeat_seq, created_at, updated_at)
         VALUES ('dev_reboot', ?, 'POS04', 'Till 4', 'online', 1, ?, 3,
             '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .bind(&branch)
    .bind(&while_ago)
    .execute(&pool)
    .await
    .unwrap();

    let roster = crate::commands::device_state::roster(&pool).await.unwrap();
    let row = roster.iter().find(|r| r.device_code == "POS04").unwrap();
    assert_eq!(row.state, "stale");
    assert!(row.advice.contains("few minutes"), "{}", row.advice);
}

/// The roster and the ZanAI tool must not be able to disagree about whether a
/// till is online — that would be its own small version of the original bug.
#[tokio::test]
async fn the_screen_and_the_ai_tool_read_the_same_derivation() {
    let pool = pool().await;
    let branch: String = sqlx::query_scalar("SELECT branch_id FROM branches LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO devices (device_id, branch_id, device_code, name, status, is_active,
             created_at, updated_at)
         VALUES ('dev_x', ?, 'POS09', 'Till 9', 'online', 1,
             '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
    )
    .bind(&branch)
    .execute(&pool)
    .await
    .unwrap();

    let roster = crate::commands::device_state::roster(&pool).await.unwrap();
    let from_screen = roster.iter().find(|r| r.device_code == "POS09").unwrap();

    let from_ai = crate::ai::tools_parity::execute(
        &pool,
        "get_terminal_roster",
        &serde_json::json!({}),
    )
    .await
    .unwrap();

    assert!(
        from_ai.contains(&from_screen.state),
        "the AI roster said something the screen does not: {from_ai}"
    );
}
