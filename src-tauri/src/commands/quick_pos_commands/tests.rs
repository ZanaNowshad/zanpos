use super::*;
use sqlx::sqlite::SqlitePoolOptions;

const COLA: &str = "01JPROD00000000000COLA001";

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
        "INSERT OR IGNORE INTO categories (category_id, name, sort_order, is_active, created_at, updated_at, version)
         VALUES ('01JCAT000000000000DRINK01', 'Drinks', 1, 1, datetime('now'), datetime('now'), 1)",
    ).execute(&pool).await.expect("category");
    sqlx::query(
        "INSERT OR IGNORE INTO products
           (product_id, category_id, name, sku, barcode, track_inventory, allow_decimal_quantity,
            is_active, currency, created_at, updated_at, version)
         VALUES (?, '01JCAT000000000000DRINK01', 'Coca-Cola 330ml', 'COLA-330', '5449000000996',
                 1, 0, 1, 'BHD', datetime('now'), datetime('now'), 1)",
    )
    .bind(COLA)
    .execute(&pool)
    .await
    .expect("product");
    sqlx::query(
        "INSERT INTO product_prices
           (price_id, product_id, price_type, price_minor, currency, effective_from, created_by_user_id, created_at)
         VALUES ('PR-COLA', ?, 'selling', 400, 'BHD', datetime('now','-1 hour'), 'seed', datetime('now'))",
    ).bind(COLA).execute(&pool).await.expect("price");
    pool
}

async fn set_slots(pool: &SqlitePool, json: &str) {
    sqlx::query(
        "INSERT INTO app_config(key, value, updated_at) VALUES ('quick_pos_products', ?, datetime('now'))
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    ).bind(json).execute(pool).await.expect("write slots");
}

#[tokio::test]
async fn unconfigured_reads_as_ten_empty_slots() {
    let pool = make_pool().await;
    let slots = load_inner(&pool).await.expect("load");
    assert_eq!(slots.len(), SLOT_COUNT);
    assert!(slots.iter().all(|s| s.product_id.is_none()));
    // Positions are stated, not inferred, so the caller can render the gaps.
    assert_eq!(
        slots.iter().map(|s| s.slot).collect::<Vec<_>>(),
        (0..SLOT_COUNT).collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn resolves_name_and_live_price_rather_than_storing_them() {
    let pool = make_pool().await;
    set_slots(&pool, &format!("[\"{COLA}\"]")).await;

    let slots = load_inner(&pool).await.expect("load");
    assert_eq!(slots[0].product_id.as_deref(), Some(COLA));
    assert_eq!(slots[0].name.as_deref(), Some("Coca-Cola 330ml"));
    assert_eq!(slots[0].price_minor, Some(400));

    // A repricing reaches the till without anyone re-picking the item.
    sqlx::query("UPDATE product_prices SET price_minor = 550 WHERE price_id = 'PR-COLA'")
        .execute(&pool)
        .await
        .unwrap();
    let slots = load_inner(&pool).await.expect("reload");
    assert_eq!(slots[0].price_minor, Some(550));
}

// A tile for something the till cannot sell is worse than a gap: the cashier
// taps it mid-queue and nothing happens.
#[tokio::test]
async fn a_deactivated_product_reads_back_as_an_empty_slot() {
    let pool = make_pool().await;
    set_slots(&pool, &format!("[\"{COLA}\"]")).await;
    sqlx::query("UPDATE products SET is_active = 0 WHERE product_id = ?")
        .bind(COLA)
        .execute(&pool)
        .await
        .unwrap();

    let slots = load_inner(&pool).await.expect("load");
    assert!(
        slots[0].product_id.is_none(),
        "retired product must leave a gap"
    );
    assert_eq!(slots.len(), SLOT_COUNT);
}

#[tokio::test]
async fn malformed_config_does_not_break_the_till() {
    let pool = make_pool().await;
    set_slots(&pool, "not json at all").await;
    let slots = load_inner(&pool).await.expect("load must not fail");
    assert_eq!(slots.len(), SLOT_COUNT);
    assert!(slots.iter().all(|s| s.product_id.is_none()));
}

#[tokio::test]
async fn an_over_long_list_is_truncated_to_the_row_the_till_renders() {
    let pool = make_pool().await;
    let ids = (0..25)
        .map(|_| format!("\"{COLA}\""))
        .collect::<Vec<_>>()
        .join(",");
    set_slots(&pool, &format!("[{ids}]")).await;
    let slots = load_inner(&pool).await.expect("load");
    assert_eq!(slots.len(), SLOT_COUNT);
}
