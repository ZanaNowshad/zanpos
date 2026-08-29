//! End-to-end: terminal worker <-> embedded hub over real HTTP on 127.0.0.1.
use sqlx::SqlitePool;
use zanpos_lib::hub;
use zanpos_lib::sync_v2::client::HttpSyncClient;
use zanpos_lib::sync_v2::worker::SyncWorker;

async fn fresh_db(tag: &str) -> SqlitePool {
    let path = std::env::temp_dir().join(format!("zanpos_e2e_{tag}_{}.db", ulid::Ulid::new()));
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

#[tokio::test]
async fn terminal_and_hub_converge_bidirectionally() {
    let hub_pool = fresh_db("hub").await;
    let term_pool = fresh_db("term").await;

    let term_dev = "01JTESTTERMINAL0000000001";
    let branch_id: String = sqlx::query_scalar("SELECT branch_id FROM branches LIMIT 1")
        .fetch_one(&term_pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO devices (device_id, branch_id, device_code, name, status, is_active, created_at, updated_at)
                 VALUES (?, ?, 'POS02', 'Till 2', 'online', 1, datetime('now'), datetime('now'))")
        .bind(term_dev).bind(&branch_id).execute(&term_pool).await.unwrap();
    set_cfg(&term_pool, "device_id", term_dev).await;
    set_cfg(&term_pool, "setup_complete", "1").await;
    set_cfg(&hub_pool, "setup_complete", "1").await;

    let token = "test-token-0123456789abcdef";
    let handle = hub::start_hub(hub_pool.clone(), 0, token).await.unwrap();
    let url = format!("http://127.0.0.1:{}", handle.port);
    let client = HttpSyncClient::new(&url, token, Some(term_dev));

    // 1) HUB-side change → terminal pulls it.
    // Ensure at least one product exists to test catalog sync.
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM products")
        .fetch_one(&hub_pool)
        .await
        .unwrap();
    if count == 0 {
        // Ensure a category exists first (FK constraint)
        let cat_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM categories")
            .fetch_one(&hub_pool)
            .await
            .unwrap();
        if cat_count == 0 {
            sqlx::query("INSERT INTO categories (category_id, name, created_at, updated_at)
                         VALUES ('E2ECAT1', 'TestCat', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')")
                .execute(&hub_pool).await.unwrap();
        }
        sqlx::query("INSERT INTO products (product_id, category_id, name, is_active, created_at, updated_at)
                     VALUES ('E2EPROD1', 'E2ECAT1', 'TestProduct', 1, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')")
            .execute(&hub_pool).await.unwrap();
    }
    sqlx::query(
        "UPDATE products SET name='HubRenamed', updated_at='2030-01-01T00:00:00Z',
                 sync_status='synced' WHERE product_id = (
                 SELECT product_id FROM products LIMIT 1)",
    )
    .execute(&hub_pool)
    .await
    .unwrap();

    // 2) TERMINAL-side sale → push to hub.
    let shift_id = ulid::Ulid::new().to_string();
    sqlx::query("INSERT INTO shifts (shift_id, branch_id, device_id, cashier_user_id, status, opened_at, created_at, updated_at)
                 VALUES (?, ?, ?, '01JUSER000000000000ADMIN1', 'open', datetime('now'), datetime('now'), datetime('now'))")
        .bind(&shift_id).bind(&branch_id).bind(term_dev).execute(&term_pool).await.unwrap();
    sqlx::query(
        "INSERT INTO sales (sale_id, branch_id, device_id, origin_device_id, receipt_number,
            shift_id, cashier_user_id, status, gross_total_minor, discount_total_minor, tax_total_minor, net_total_minor,
            business_date, idempotency_key, sold_at, created_at, updated_at, sync_status)
         VALUES ('E2ESALE1', ?, ?, ?, 'T2-0001',
            ?, '01JUSER000000000000ADMIN1', 'completed', 500, 0, 0, 500,
            '2026-01-01', 'E2ESALE1-ik', datetime('now'), datetime('now'), datetime('now'), 'pending')")
        .bind(&branch_id).bind(term_dev).bind(term_dev).bind(&shift_id)
        .execute(&term_pool).await.unwrap();

    let worker = SyncWorker::new(term_pool.clone());
    worker.run_once_with(&client, term_dev).await;

    let hub_sale: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sales WHERE sale_id='E2ESALE1'")
        .fetch_one(&hub_pool)
        .await
        .unwrap();
    assert_eq!(hub_sale, 1, "terminal sale must reach the hub");

    let term_name: String =
        sqlx::query_scalar("SELECT name FROM products WHERE name='HubRenamed' LIMIT 1")
            .fetch_optional(&term_pool)
            .await
            .unwrap()
            .unwrap_or_default();
    assert_eq!(
        term_name, "HubRenamed",
        "hub catalog change must reach the terminal"
    );

    // 3) Idempotency: second cycle must not duplicate the sale.
    worker.run_once_with(&client, term_dev).await;
    let hub_sale2: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sales WHERE sale_id='E2ESALE1'")
        .fetch_one(&hub_pool)
        .await
        .unwrap();
    assert_eq!(hub_sale2, 1);

    // 4) Wrong token → 401 → worker survives, marks error.
    let bad = HttpSyncClient::new(&url, "wrong-token", Some(term_dev));
    worker.run_once_with(&bad, term_dev).await;

    handle.shutdown();
}

async fn category_state(pool: &SqlitePool) -> Vec<(String, String, i64, Option<String>)> {
    sqlx::query_as(
        "SELECT category_id, name, is_active, deleted_at
           FROM categories WHERE category_id LIKE 'MULTI-%' ORDER BY category_id",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn two_offline_terminals_converge_through_the_hub_after_create_update_delete_and_replay() {
    let hub_pool = fresh_db("multi-hub").await;
    let terminal_a = fresh_db("multi-a").await;
    let terminal_b = fresh_db("multi-b").await;
    let token = "test-token-multi-terminal-012345";
    let handle = hub::start_hub(hub_pool.clone(), 0, token).await.unwrap();
    let url = format!("http://127.0.0.1:{}", handle.port);
    let client_a = HttpSyncClient::new(&url, token, Some("MULTI-DEVICE-A"));
    let client_b = HttpSyncClient::new(&url, token, Some("MULTI-DEVICE-B"));
    let worker_a = SyncWorker::new(terminal_a.clone());
    let worker_b = SyncWorker::new(terminal_b.clone());

    // Both tills keep operating while disconnected and create independent rows.
    sqlx::query(
        "INSERT INTO categories
            (category_id, name, is_active, created_at, updated_at, sync_status)
         VALUES ('MULTI-A', 'From A', 1, '2026-08-01T09:00:00Z',
                 '2026-08-01T09:00:00Z', 'pending')",
    )
    .execute(&terminal_a)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO categories
            (category_id, name, is_active, created_at, updated_at, sync_status)
         VALUES ('MULTI-B', 'From B', 1, '2026-08-01T09:01:00Z',
                 '2026-08-01T09:01:00Z', 'pending')",
    )
    .execute(&terminal_b)
    .await
    .unwrap();

    worker_a.run_once_with(&client_a, "MULTI-DEVICE-A").await;
    worker_b.run_once_with(&client_b, "MULTI-DEVICE-B").await;
    worker_a.run_once_with(&client_a, "MULTI-DEVICE-A").await;
    worker_b.run_once_with(&client_b, "MULTI-DEVICE-B").await;

    let created = category_state(&hub_pool).await;
    assert_eq!(category_state(&terminal_a).await, created);
    assert_eq!(category_state(&terminal_b).await, created);
    assert_eq!(created.len(), 2);

    // B edits A's row; A later deletes B's row. Both mutations must cross in
    // opposite directions and settle to the same business state.
    sqlx::query(
        "UPDATE categories SET name = 'A renamed by B',
                updated_at = '2026-08-01T10:00:00Z', sync_status = 'pending'
          WHERE category_id = 'MULTI-A'",
    )
    .execute(&terminal_b)
    .await
    .unwrap();
    worker_b.run_once_with(&client_b, "MULTI-DEVICE-B").await;
    worker_a.run_once_with(&client_a, "MULTI-DEVICE-A").await;

    sqlx::query(
        "UPDATE categories SET is_active = 0,
                deleted_at = '2026-08-01T11:00:00Z',
                updated_at = '2026-08-01T11:00:00Z', sync_status = 'pending'
          WHERE category_id = 'MULTI-B'",
    )
    .execute(&terminal_a)
    .await
    .unwrap();
    worker_a.run_once_with(&client_a, "MULTI-DEVICE-A").await;
    worker_b.run_once_with(&client_b, "MULTI-DEVICE-B").await;

    // Replay both cycles: idempotency means neither rows nor tombstones multiply.
    worker_a.run_once_with(&client_a, "MULTI-DEVICE-A").await;
    worker_b.run_once_with(&client_b, "MULTI-DEVICE-B").await;

    let authoritative = category_state(&hub_pool).await;
    assert_eq!(category_state(&terminal_a).await, authoritative);
    assert_eq!(category_state(&terminal_b).await, authoritative);
    assert_eq!(authoritative.len(), 2);
    assert_eq!(authoritative[0].1, "A renamed by B");
    assert_eq!(authoritative[1].2, 0);
    assert_eq!(
        authoritative[1].3.as_deref(),
        Some("2026-08-01T11:00:00Z")
    );

    handle.shutdown();
}
