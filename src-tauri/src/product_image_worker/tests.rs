//! Tests for the automatic product-image backfill.
//!
//! What is worth pinning here is *which* product gets picked and when it stops
//! being picked — the selection and the backoff. The lookup itself goes out to
//! Open Food Facts and Bing, and a test that hit either would be measuring
//! someone else's uptime rather than this code.

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
    sqlx::query(
        "INSERT INTO categories (category_id, name, created_at, updated_at)
         VALUES ('cat_dairy', 'Dairy', datetime('now'), datetime('now'))",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool
}

/// `created_at` is explicit because selection order depends on it.
async fn seed(pool: &SqlitePool, id: &str, barcode: Option<&str>, image: Option<&str>, created: &str) {
    sqlx::query(
        "INSERT INTO products
           (product_id, category_id, name, sku, barcode, image_path,
            is_active, currency, reorder_point, created_at, updated_at)
         VALUES (?, 'cat_dairy', ?, ?, ?, ?, 1, 'BHD', 0, ?, ?)",
    )
    .bind(id)
    .bind(format!("Product {id}"))
    .bind(format!("SKU-{id}"))
    .bind(barcode)
    .bind(image)
    .bind(created)
    .bind(created)
    .execute(pool)
    .await
    .unwrap();
}

fn worker(pool: &SqlitePool) -> ProductImageWorker {
    ProductImageWorker { db: pool.clone() }
}

#[tokio::test]
async fn picks_a_product_that_has_no_image() {
    let pool = make_pool().await;
    seed(&pool, "p1", Some("6280123456781"), None, "2026-01-01T00:00:00Z").await;

    let found = worker(&pool).next_candidate().await.unwrap();
    let found = found.expect("a product with no image is work");
    assert_eq!(found.product_id, "p1");
    assert_eq!(found.barcode.as_deref(), Some("6280123456781"));
    assert_eq!(found.category_name.as_deref(), Some("Dairy"));
}

#[tokio::test]
async fn leaves_a_product_that_already_has_one() {
    let pool = make_pool().await;
    seed(&pool, "p1", Some("6280123456781"), Some("https://img.test/p1.jpg"), "2026-01-01T00:00:00Z").await;

    assert!(worker(&pool).next_candidate().await.unwrap().is_none());
}

#[tokio::test]
async fn treats_a_blank_image_path_as_missing() {
    // An empty string is what a cleared field leaves behind, and it renders the
    // same grey box as NULL does.
    let pool = make_pool().await;
    seed(&pool, "p1", Some("6280123456781"), Some("   "), "2026-01-01T00:00:00Z").await;

    assert!(worker(&pool).next_candidate().await.unwrap().is_some());
}

#[tokio::test]
async fn skips_a_product_with_nothing_to_search_on() {
    /* The lookup is grounded on the barcode — `validate_search_identity`
       rejects a request without one. Selecting such a product would burn an
       attempt on a request that cannot succeed. */
    let pool = make_pool().await;
    seed(&pool, "p1", None, None, "2026-01-01T00:00:00Z").await;
    seed(&pool, "p2", Some("  "), None, "2026-01-01T00:00:00Z").await;

    assert!(worker(&pool).next_candidate().await.unwrap().is_none());
}

#[tokio::test]
async fn takes_the_newest_product_first() {
    /* This is what makes "a new product gets its picture by itself" true
       without a separate path for creation: the item added at the counter is
       the newest row, so it jumps ahead of the historical backfill. */
    let pool = make_pool().await;
    seed(&pool, "old", Some("6280000000001"), None, "2020-01-01T00:00:00Z").await;
    seed(&pool, "new", Some("6280000000002"), None, "2026-08-22T10:00:00Z").await;
    seed(&pool, "mid", Some("6280000000003"), None, "2024-01-01T00:00:00Z").await;

    let found = worker(&pool).next_candidate().await.unwrap().unwrap();
    assert_eq!(found.product_id, "new");
}

#[tokio::test]
async fn waits_out_the_backoff_after_a_failure() {
    let pool = make_pool().await;
    seed(&pool, "p1", Some("6280123456781"), None, "2026-01-01T00:00:00Z").await;
    let w = worker(&pool);

    let candidate = w.next_candidate().await.unwrap().unwrap();
    w.record_failure(&candidate, "no image found").await.unwrap();

    assert!(
        w.next_candidate().await.unwrap().is_none(),
        "a product just tried must not be picked again on the next tick"
    );

    let attempts: i64 =
        sqlx::query_scalar("SELECT attempts FROM product_image_attempts WHERE product_id = 'p1'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(attempts, 1);
}

#[tokio::test]
async fn comes_back_round_once_the_backoff_expires() {
    let pool = make_pool().await;
    seed(&pool, "p1", Some("6280123456781"), None, "2026-01-01T00:00:00Z").await;
    let w = worker(&pool);
    let candidate = w.next_candidate().await.unwrap().unwrap();
    w.record_failure(&candidate, "timeout").await.unwrap();

    sqlx::query("UPDATE product_image_attempts SET next_attempt_at = '2000-01-01T00:00:00Z'")
        .execute(&pool)
        .await
        .unwrap();

    assert!(w.next_candidate().await.unwrap().is_some());
}

#[tokio::test]
async fn gives_up_after_the_attempt_ceiling() {
    /* Most failures are "this product is not in any public database", which is
       true of unbranded and local goods and never stops being true. Retrying
       forever would mean a permanent trickle of pointless requests. */
    let pool = make_pool().await;
    seed(&pool, "p1", Some("6280123456781"), None, "2026-01-01T00:00:00Z").await;
    let w = worker(&pool);

    for _ in 0..MAX_ATTEMPTS {
        let candidate = w
            .next_candidate()
            .await
            .unwrap()
            .expect("still eligible below the ceiling");
        w.record_failure(&candidate, "no image found").await.unwrap();
        sqlx::query("UPDATE product_image_attempts SET next_attempt_at = '2000-01-01T00:00:00Z'")
            .execute(&pool)
            .await
            .unwrap();
    }

    assert!(
        w.next_candidate().await.unwrap().is_none(),
        "a product tried {MAX_ATTEMPTS} times is left alone"
    );
}

#[tokio::test]
async fn a_resolved_product_records_its_outcome() {
    let pool = make_pool().await;
    seed(&pool, "p1", Some("6280123456781"), None, "2026-01-01T00:00:00Z").await;
    let w = worker(&pool);
    w.record_resolved("p1").await.unwrap();

    let resolved: Option<String> = sqlx::query_scalar(
        "SELECT resolved_at FROM product_image_attempts WHERE product_id = 'p1'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(resolved.is_some());
}

#[tokio::test]
async fn an_image_cleared_by_hand_becomes_work_again() {
    /* The catalogue is the work list, not a queue table. A manager who deletes
       a wrong picture should get a new one without having to know that some
       other table needed poking. */
    let pool = make_pool().await;
    seed(&pool, "p1", Some("6280123456781"), Some("https://img.test/wrong.jpg"), "2026-01-01T00:00:00Z").await;
    let w = worker(&pool);
    w.record_resolved("p1").await.unwrap();
    assert!(w.next_candidate().await.unwrap().is_none());

    sqlx::query("UPDATE products SET image_path = NULL WHERE product_id = 'p1'")
        .execute(&pool)
        .await
        .unwrap();

    assert!(w.next_candidate().await.unwrap().is_some());
}

#[tokio::test]
async fn runs_unless_the_shop_turned_it_off() {
    // On by default: the point of the feature is that nobody has to ask.
    let pool = make_pool().await;
    let w = worker(&pool);
    assert!(w.enabled().await, "no setting means on");

    for off in ["0", "false", "off"] {
        sqlx::query(
            "INSERT INTO app_config (key, value, updated_at)
             VALUES ('product_image_autofetch', ?, datetime('now'))
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        )
        .bind(off)
        .execute(&pool)
        .await
        .unwrap();
        assert!(!w.enabled().await, "\"{off}\" should stop the worker");
        assert_eq!(
            w.run_once().await.unwrap(),
            Outcome::Idle,
            "a disabled worker reports no work rather than doing some"
        );
    }

    sqlx::query("UPDATE app_config SET value = '1' WHERE key = 'product_image_autofetch'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(w.enabled().await);
}

#[test]
fn backoff_lengthens_with_each_attempt() {
    assert_eq!(backoff_minutes(0), 10);
    assert_eq!(backoff_minutes(1), 10);
    assert_eq!(backoff_minutes(2), 60);
    assert_eq!(backoff_minutes(3), 360);
    assert_eq!(backoff_minutes(99), 360);
}
