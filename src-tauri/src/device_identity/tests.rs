use super::*;
use sqlx::sqlite::SqlitePoolOptions;

async fn make_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("in-memory pool");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("migrations");
    sqlx::query("UPDATE branches SET is_active = 1")
        .execute(&pool)
        .await
        .expect("activate branch");
    pool
}

async fn config_device_id(pool: &SqlitePool) -> Option<String> {
    read_config(pool).await
}

// The core guarantee: two installs of the same seeded database must not end up
// claiming the same identity, because receipt_number is derived from it and is
// UNIQUE across the fleet.
#[tokio::test]
async fn two_installs_get_distinct_identities() {
    let a = make_pool().await;
    let b = make_pool().await;

    let id_a = ensure_in_db(&a, None).await.expect("a");
    let id_b = ensure_in_db(&b, None).await.expect("b");

    assert!(matches!(id_a, Identity::Rekeyed(_)));
    assert!(matches!(id_b, Identity::Rekeyed(_)));
    assert_ne!(id_a.id(), id_b.id(), "identities must differ");
    assert_ne!(id_a.id(), SEED_DEVICE_ID);
    assert_ne!(id_b.id(), SEED_DEVICE_ID);
}

// device_code is the half that actually collides receipts: the counter is per
// device_id, but the printed number embeds device_code.
#[tokio::test]
async fn rekey_replaces_the_seeded_device_code() {
    let a = make_pool().await;
    let b = make_pool().await;
    ensure_in_db(&a, None).await.expect("a");
    ensure_in_db(&b, None).await.expect("b");

    let code = |pool: SqlitePool| async move {
        sqlx::query_scalar::<_, String>("SELECT device_code FROM devices WHERE is_active = 1")
            .fetch_one(&pool)
            .await
            .expect("device_code")
    };
    let code_a = code(a).await;
    let code_b = code(b).await;

    assert_ne!(code_a, SEED_DEVICE_CODE);
    assert_ne!(code_b, SEED_DEVICE_CODE);
    assert_ne!(code_a, code_b, "receipt namespaces must not overlap");
}

#[tokio::test]
async fn ensure_is_idempotent() {
    let pool = make_pool().await;
    let first = ensure_in_db(&pool, None).await.expect("first");
    let second = ensure_in_db(&pool, None).await.expect("second");

    assert!(matches!(second, Identity::Existing(_)));
    assert_eq!(first.id(), second.id());
}

// A reinstall recovers its identity rather than orphaning its hub pairing.
#[tokio::test]
async fn recovered_identity_is_adopted_over_minting() {
    let pool = make_pool().await;
    let recovered = "01JRECOVERED00000000000001".to_string();

    let outcome = ensure_in_db(&pool, Some(recovered.clone()))
        .await
        .expect("ensure");

    assert_eq!(outcome.id(), recovered);
    assert_eq!(config_device_id(&pool).await.as_deref(), Some(&*recovered));
}

// A recovered value that is itself the shared seed is not an identity.
#[tokio::test]
async fn seeded_value_is_never_adopted_as_recovery() {
    let pool = make_pool().await;
    let outcome = ensure_in_db(&pool, Some(SEED_DEVICE_ID.to_string()))
        .await
        .expect("ensure");
    assert_ne!(outcome.id(), SEED_DEVICE_ID);
}

// Existing rows must follow the terminal to its new identity, otherwise its own
// history becomes unattributable and drops out of origin-scoped pulls.
#[tokio::test]
async fn rekey_rewrites_existing_rows() {
    let pool = make_pool().await;
    let branch: String = sqlx::query_scalar("SELECT branch_id FROM branches LIMIT 1")
        .fetch_one(&pool)
        .await
        .expect("branch");

    sqlx::query(
        "INSERT INTO shifts (shift_id, branch_id, device_id, origin_device_id, cashier_user_id,
                             opened_at, status, created_at, updated_at, version)
         VALUES ('SH-1', ?, ?, ?, '01JUSER000000000000CASH01',
                 datetime('now'), 'open', datetime('now'), datetime('now'), 1)",
    )
    .bind(&branch)
    .bind(SEED_DEVICE_ID)
    .bind(SEED_DEVICE_ID)
    .execute(&pool)
    .await
    .expect("seed shift");

    let outcome = ensure_in_db(&pool, None).await.expect("ensure");

    let (device_id, origin): (String, String) =
        sqlx::query_as("SELECT device_id, origin_device_id FROM shifts WHERE shift_id = 'SH-1'")
            .fetch_one(&pool)
            .await
            .expect("read back shift");

    assert_eq!(device_id, outcome.id());
    assert_eq!(origin, outcome.id());
}

// The devices row is updated in place, so its receipt counter carries over
// instead of restarting at 1 and re-minting numbers already printed.
#[tokio::test]
async fn rekey_preserves_the_receipt_counter() {
    let pool = make_pool().await;
    sqlx::query("UPDATE devices SET next_receipt_seq = 47 WHERE device_id = ?")
        .bind(SEED_DEVICE_ID)
        .execute(&pool)
        .await
        .expect("advance counter");

    let outcome = ensure_in_db(&pool, None).await.expect("ensure");

    let seq: i64 = sqlx::query_scalar("SELECT next_receipt_seq FROM devices WHERE device_id = ?")
        .bind(outcome.id())
        .fetch_one(&pool)
        .await
        .expect("counter survives");
    assert_eq!(seq, 47);
}

// `current` must not be answerable by a sibling's synced device row.
#[tokio::test]
async fn current_prefers_config_over_a_synced_sibling_row() {
    let pool = make_pool().await;
    let mine = ensure_in_db(&pool, None).await.expect("ensure");

    let branch: String = sqlx::query_scalar("SELECT branch_id FROM branches LIMIT 1")
        .fetch_one(&pool)
        .await
        .expect("branch");
    // A sibling terminal pulled from the hub, sorting first by device_code.
    sqlx::query(
        "INSERT INTO devices (device_id, branch_id, device_code, name, status, is_active,
                              created_at, updated_at, version)
         VALUES ('01JOTHERTERMINAL00000001', ?, 'AAA-0001', 'Lane 2', 'online', 1,
                 '2020-01-01T00:00:00Z', '2020-01-01T00:00:00Z', 1)",
    )
    .bind(&branch)
    .execute(&pool)
    .await
    .expect("insert sibling");

    let resolved = current(&pool).await.expect("current");
    assert_eq!(
        resolved,
        mine.id(),
        "a sibling's row must never answer for this terminal"
    );
}

// A unique id behind a seeded code still collides receipt numbers, because the
// printed number embeds the code. Reachable via hub join, which takes the code
// as free text.
#[tokio::test]
async fn seeded_device_code_is_repaired_on_a_unique_identity() {
    let pool = make_pool().await;
    let unique = "01JUNIQUETERMINAL0000001";

    sqlx::query("UPDATE devices SET device_id = ?, device_code = ? WHERE device_id = ?")
        .bind(unique)
        .bind(SEED_DEVICE_CODE)
        .bind(SEED_DEVICE_ID)
        .execute(&pool)
        .await
        .expect("simulate hub join keeping the seeded code");
    sqlx::query(
        "INSERT INTO app_config(key,value,updated_at) VALUES ('device_id',?,datetime('now'))
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
    )
    .bind(unique)
    .execute(&pool)
    .await
    .expect("claim identity");

    let outcome = ensure_in_db(&pool, None).await.expect("ensure");
    assert_eq!(outcome, Identity::Existing(unique.to_string()));

    let code: String = sqlx::query_scalar("SELECT device_code FROM devices WHERE device_id = ?")
        .bind(unique)
        .fetch_one(&pool)
        .await
        .expect("device_code");
    assert_ne!(code, SEED_DEVICE_CODE, "seeded code must be replaced");
}

#[test]
fn device_code_is_derived_from_the_identity() {
    let code = device_code_for("01JABCDEFGHJKMNPQRSTVWXYZ");
    assert!(code.starts_with("POS-"));
    assert_eq!(code.len(), 9);
    assert_ne!(code, SEED_DEVICE_CODE);
}

// The recovery path for a database cloned onto a second PC: the clone must
// stop sharing its sibling's identity — rows, receipt namespace, heartbeat
// counter and watermarks all move to the new id.
#[tokio::test]
async fn rekey_to_fresh_separates_a_cloned_identity() {
    let pool = make_pool().await;
    let old_id = ensure_in_db(&pool, None)
        .await
        .expect("ensure")
        .id()
        .to_string();

    // This install has history under the old identity, and stale sync state
    // that belongs to that identity alone.
    let branch: String = sqlx::query_scalar("SELECT branch_id FROM branches LIMIT 1")
        .fetch_one(&pool)
        .await
        .expect("branch");
    let shift_id = ulid::Ulid::new().to_string();
    sqlx::query(
        "INSERT INTO shifts (shift_id, branch_id, device_id, origin_device_id, cashier_user_id,
                             opened_at, status, created_at, updated_at, version)
         VALUES (?, ?, ?, ?, '01JUSER000000000000ADMIN1',
                 datetime('now'), 'open', datetime('now'), datetime('now'), 1)",
    )
    .bind(&shift_id)
    .bind(&branch)
    .bind(&old_id)
    .bind(&old_id)
    .execute(&pool)
    .await
    .expect("shift under old identity");
    sqlx::query(
        "INSERT INTO sales (sale_id, branch_id, device_id, origin_device_id, receipt_number,
                            shift_id, cashier_user_id, status, gross_total_minor, discount_total_minor,
                            tax_total_minor, net_total_minor, business_date, idempotency_key,
                            sold_at, created_at, updated_at, sync_status)
         VALUES ('SALE-REKEY', ?, ?, ?, 'CLONE-R1', ?, '01JUSER000000000000ADMIN1',
                 'completed', 100, 0, 0, 100, '2026-01-01', 'SALE-REKEY-ik',
                 datetime('now'), datetime('now'), datetime('now'), 'synced')",
    )
    .bind(&branch)
    .bind(&old_id)
    .bind(&old_id)
    .bind(&shift_id)
    .execute(&pool)
    .await
    .expect("sale under old identity");
    sqlx::query("INSERT INTO app_config(key, value, updated_at) VALUES ('heartbeat_seq','9',datetime('now'))")
        .execute(&pool)
        .await
        .expect("heartbeat seq");
    sqlx::query("INSERT INTO app_config(key, value, updated_at) VALUES ('sync_v2_watermark_sales','2030-01-01T00:00:00Z',datetime('now'))")
        .execute(&pool)
        .await
        .expect("watermark");

    let (reported_old, new_id) = rekey_to_fresh(&pool).await.expect("rekey");
    assert_eq!(reported_old, old_id);

    // Identity moved everywhere it is recorded.
    assert_eq!(
        config_device_id(&pool).await.as_deref(),
        Some(new_id.as_str())
    );
    let (sale_origin, sale_status): (String, String) = sqlx::query_as(
        "SELECT origin_device_id, sync_status FROM sales WHERE sale_id = 'SALE-REKEY'",
    )
    .fetch_one(&pool)
    .await
    .expect("read sale");
    assert_eq!(sale_origin, new_id);
    assert_eq!(
        sale_status, "pending",
        "rewritten rows must be re-offered to the hub"
    );
    let old_row_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM devices WHERE device_id = ?")
        .bind(&old_id)
        .fetch_one(&pool)
        .await
        .expect("old device row");
    assert_eq!(
        old_row_count, 0,
        "the retired identity must not stay locally"
    );
    let code: String = sqlx::query_scalar("SELECT device_code FROM devices WHERE device_id = ?")
        .bind(&new_id)
        .fetch_one(&pool)
        .await
        .expect("new device code");
    assert_ne!(code, SEED_DEVICE_CODE);

    // Heartbeat counter and watermarks reset — they are facts of the old
    // identity and would make the new one look stale or skip history.
    let heartbeat: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'heartbeat_seq'")
            .fetch_optional(&pool)
            .await
            .expect("heartbeat key");
    assert!(heartbeat.is_none());
    let watermark: Option<String> =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'sync_v2_watermark_sales'")
            .fetch_optional(&pool)
            .await
            .expect("watermark key");
    assert!(watermark.is_none());
}
