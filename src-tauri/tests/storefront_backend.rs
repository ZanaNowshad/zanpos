use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use zanpos_lib::storefront::{
    catalog::build_catalog_snapshot, orders::ingest_storefront_message, publisher::sign_request,
};

async fn db() -> sqlx::SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO categories
         (category_id,name,is_active,created_at,updated_at)
         VALUES ('01JCAT0000000000000000001','Public Category',1,datetime('now'),datetime('now'))",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO products
         (product_id,category_id,name,description,is_active,currency,cost_minor,created_at,updated_at)
         VALUES ('01JPROD000000000000000001','01JCAT0000000000000000001','Public Product',
                 'Safe description',1,'BHD',999,datetime('now'),datetime('now'))",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool
}

#[tokio::test]
async fn migration_adds_storefront_tables_and_idempotency_index() {
    let pool = db().await;
    let tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master
         WHERE type='table' AND name IN ('storefront_products','storefront_releases')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(tables, 2);

    let columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('wa_orders')")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert!(columns.contains(&"source".to_string()));
    assert!(columns.contains(&"external_ref".to_string()));
}

#[tokio::test]
async fn catalog_uses_effective_branch_price_and_never_serializes_private_fields() {
    let pool = db().await;
    sqlx::query(
        "UPDATE branches SET name='Public Shop', currency='BHD', address='Road 1',
         phone='+97300000000', is_active=1 WHERE branch_id='01JBRANCH0000000000000001'",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO storefront_products(product_id, is_visible, public_image_url, updated_at)
         VALUES ('01JPROD000000000000000001',1,'https://cdn.example/item.jpg',datetime('now'))",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE products SET is_active=1, deleted_at=NULL, barcode='SECRET-BARCODE',
         cost_minor=999, image_path='C:\\private\\photo.jpg'
         WHERE product_id='01JPROD000000000000000001'",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO product_prices
         (price_id,product_id,branch_id,price_type,price_minor,currency,effective_from,
          effective_to,created_by_user_id,created_at)
         VALUES ('future-price','01JPROD000000000000000001',NULL,'selling',9999,'BHD',
                 '2999-01-01T00:00:00Z',NULL,'test',datetime('now')),
                ('branch-price','01JPROD000000000000000001','01JBRANCH0000000000000001',
                 'selling',1250,'BHD','2000-01-01T00:00:00Z',NULL,'test',datetime('now'))",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT OR REPLACE INTO stock_levels
         (stock_level_id,product_id,branch_id,quantity_on_hand,created_at,updated_at)
         VALUES ('sf-stock','01JPROD000000000000000001','01JBRANCH0000000000000001',
                 '42',datetime('now'),datetime('now'))",
    )
    .execute(&pool)
    .await
    .unwrap();

    let snapshot = build_catalog_snapshot(&pool).await.unwrap();
    assert_eq!(snapshot.currency_decimals, 3);
    assert_eq!(snapshot.products.len(), 1);
    assert_eq!(snapshot.products[0].price_minor, 1250);
    assert_eq!(snapshot.products[0].availability, "available");
    assert_eq!(snapshot.products[0].quantity_decimals, 0);
    let json = serde_json::to_value(snapshot).unwrap();
    for forbidden in ["barcode", "cost_minor", "image_path", "quantity_on_hand"] {
        assert!(json.pointer(&format!("/products/0/{forbidden}")).is_none());
    }
}

#[test]
fn hmac_signature_matches_rfc_4231_vector() {
    let signature = sign_request(
        &[0x0b; 20],
        "POST",
        "/v1/storefront/releases",
        "1700000000",
        b"Hi There",
    );
    assert_eq!(
        signature,
        "3337f3856bea0f5bdea17a7683ac8f6025df52ff34d40f87a976502a2e279a0e"
    );
}

#[tokio::test]
async fn storefront_message_reprices_and_is_idempotent() {
    let pool = db().await;
    sqlx::query(
        "INSERT INTO storefront_products(product_id,is_visible,updated_at)
         VALUES ('01JPROD000000000000000001',1,datetime('now'))",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO product_prices
         (price_id,product_id,branch_id,price_type,price_minor,currency,effective_from,
          created_by_user_id,created_at)
         VALUES ('web-price','01JPROD000000000000000001','01JBRANCH0000000000000001',
                 'selling',1750,'BHD','2000-01-01T00:00:00Z','test',datetime('now'))",
    )
    .execute(&pool)
    .await
    .unwrap();
    let body = concat!(
        "Order summary with untrusted human-visible prices\n\n",
        "[ZANPOS:v1]\n",
        "order_id=550e8400-e29b-41d4-a716-446655440000\n",
        "currency=BHD\n",
        "items=01JPROD000000000000000001:2\n",
        "[/ZANPOS]"
    );

    let first = ingest_storefront_message(&pool, body, "message-1", "customer@s.whatsapp.net", "A")
        .await
        .unwrap()
        .unwrap();
    let second =
        ingest_storefront_message(&pool, body, "message-2", "customer@s.whatsapp.net", "A")
            .await
            .unwrap()
            .unwrap();
    assert_eq!(first.order_id, second.order_id);
    assert_eq!(first.total_minor, 3500);
    let raw: Value = serde_json::from_str(&first.raw_json).unwrap();
    assert_eq!(raw["items"][0]["unit_price_minor"], 1750);
    assert!(raw["items"][0].get("price_minor").is_none());
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM wa_orders WHERE source='web_storefront'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn malformed_or_unbounded_storefront_blocks_are_rejected() {
    let pool = db().await;
    assert!(ingest_storefront_message(
        &pool,
        "hello [ZANPOS:v1]\norder_id=nope\ncurrency=BHD\nitems=x:1\n[/ZANPOS] trailing",
        "m",
        "c",
        "n"
    )
    .await
    .is_err());
    let huge = format!(
        "[ZANPOS:v1]\norder_id=550e8400-e29b-41d4-a716-446655440000\ncurrency=BHD\nitems={}:1\n[/ZANPOS]",
        "x".repeat(33_000)
    );
    assert!(ingest_storefront_message(&pool, &huge, "m", "c", "n")
        .await
        .is_err());
}
