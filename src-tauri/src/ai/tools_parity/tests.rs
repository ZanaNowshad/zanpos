use super::*;

async fn pool() -> SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO branches (branch_id, branch_code, name, created_at, updated_at)
         VALUES ('br-1','B1','Amwaj','2026-01-01','2026-01-01')",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool
}

/// `status` is deliberately seeded to 'online' for every device, including the
/// ones that have never checked in. That is exactly the stored string the tile
/// used to believe, and the roster must now ignore it.
async fn add_device(
    pool: &SqlitePool,
    code: &str,
    name: &str,
    active: i64,
    heartbeat: Option<&str>,
) {
    sqlx::query(
        "INSERT INTO devices
           (device_id, branch_id, device_code, name, status, is_active, next_receipt_seq,
            last_heartbeat_at, heartbeat_seq, observed_ip, app_version, created_at, updated_at)
         VALUES (?, 'br-1', ?, ?, 'online', ?, 1, ?, ?, ?, '2.0.0', '2026-01-01', '2026-01-01')",
    )
    .bind(format!("dev-{code}"))
    .bind(code)
    .bind(name)
    .bind(active)
    .bind(heartbeat)
    .bind(if heartbeat.is_some() { 7_i64 } else { 0 })
    .bind(heartbeat.map(|_| "192.168.1.42"))
    .execute(pool)
    .await
    .unwrap();
}

#[test]
fn the_module_claims_exactly_the_tools_it_implements() {
    for name in [
        "check_terminal_parity",
        "find_diverged_rows",
        "get_terminal_roster",
        "get_catalogue_parity_summary",
    ] {
        assert!(handles(name), "{name}");
    }
    // Anything else has to fall through to the ordinary read dispatch.
    for name in ["get_sync_status", "list_products", "sync_reset_stuck"] {
        assert!(!handles(name), "{name}");
    }
}

/// State comes from heartbeat evidence, not from the stored `status` string —
/// which the fixture sets to 'online' on every row, including ones that have
/// never made contact.
#[tokio::test]
async fn the_roster_derives_state_from_heartbeats_not_the_stored_status() {
    let pool = pool().await;
    let just_now = (chrono::Utc::now() - chrono::Duration::seconds(20)).to_rfc3339();
    let a_while = (chrono::Utc::now() - chrono::Duration::minutes(6)).to_rfc3339();
    let long_ago = (chrono::Utc::now() - chrono::Duration::days(3)).to_rfc3339();
    add_device(&pool, "T1", "Front till", 1, Some(&just_now)).await;
    add_device(&pool, "T2", "Back office", 1, Some(&a_while)).await;
    add_device(&pool, "T3", "Storeroom", 1, Some(&long_ago)).await;
    add_device(&pool, "T4", "Spare", 0, None).await;

    let out = terminal_roster(&pool).await.unwrap();

    assert!(out.contains("online"), "{out}");
    assert!(out.contains("stale"), "{out}");
    assert!(out.contains("offline"), "{out}");
    // A record nothing has claimed is unpaired — a different problem, and a
    // different fix, from one that is bound but silent.
    assert!(out.contains("unpaired"), "{out}");
    assert!(out.contains("3d ago"), "{out}");
    // Evidence, not claims: the address the hub observed.
    assert!(out.contains("192.168.1.42"), "{out}");
    // Anything not serving says what to do about it.
    assert!(out.contains("No installation is bound"), "{out}");
    assert!(out.contains("powered on"), "{out}");
}

/// A fresh install already has the terminal it was set up on. It has never sent
/// a heartbeat, so it must read `never_seen` — this is precisely the POS-7757Z
/// case, and it used to read "online".
#[tokio::test]
async fn the_terminal_seeded_at_setup_reads_as_never_seen_not_online() {
    let pool = pool().await;
    // This installation is the seeded terminal — bound, but never yet in touch.
    sqlx::query(
        "INSERT INTO app_config (key, value, updated_at) VALUES ('device_id', ?, '2026-01-01')",
    )
        .bind("01JDEVICE0000000000000001")
        .execute(&pool)
        .await
        .unwrap();

    let out = terminal_roster(&pool).await.unwrap();

    assert!(out.contains("Terminals registered: 1"), "{out}");
    assert!(out.contains("POS01"), "{out}");
    assert!(out.contains("never_seen"), "{out}");
    assert!(out.contains("never"), "{out}");
    // Bound, so it must NOT be reported as needing re-registration.
    assert!(!out.contains("unpaired"), "{out}");
}

/// A standalone shop has no hub. That is not a fault, and reporting it as one
/// would send an operator hunting for a sync problem that cannot exist.
#[tokio::test]
async fn a_shop_with_no_hub_is_told_there_is_nothing_to_compare() {
    let pool = pool().await;

    let parity = check_parity(&pool).await.unwrap();
    assert!(parity.contains("no hub configured"), "{parity}");
    assert!(!parity.to_lowercase().contains("error"), "{parity}");

    let rows = diverged_rows(&pool, &serde_json::json!({ "table": "products" }))
        .await
        .unwrap();
    assert!(rows.contains("No hub"), "{rows}");

    let catalogue = catalogue_parity(&pool).await.unwrap();
    assert!(catalogue.contains("no hub configured"), "{catalogue}");
}

/// A table name the sync protocol does not know is a mistake worth naming, and
/// the reply lists the real ones so the model can correct itself in one step
/// rather than guessing again.
#[tokio::test]
async fn an_unsynced_table_is_refused_with_the_list_of_real_ones() {
    let pool = pool().await;
    let error = diverged_rows(&pool, &serde_json::json!({ "table": "ai_chat_messages" }))
        .await
        .unwrap_err()
        .to_string();

    assert!(error.contains("ai_chat_messages"), "{error}");
    assert!(error.contains("products"), "{error}");

    assert!(diverged_rows(&pool, &serde_json::json!({})).await.is_err());
}

#[tokio::test]
async fn the_dispatcher_rejects_a_tool_it_does_not_own() {
    let pool = pool().await;
    assert!(execute(&pool, "list_products", &serde_json::json!({}))
        .await
        .is_err());
}
