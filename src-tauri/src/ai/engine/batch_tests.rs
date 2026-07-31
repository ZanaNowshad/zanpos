
use super::*;
use sqlx::sqlite::SqlitePoolOptions;

async fn setup() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let now = "2026-01-01T00:00:00Z";
    for (id, parent) in [
        ("toys", None),
        ("girls", Some("toys")),
        ("boys", Some("toys")),
    ] {
        sqlx::query("INSERT INTO categories (category_id,parent_category_id,name,sort_order,is_active,created_at,updated_at) VALUES (?,?,?,0,1,?,?)")
                .bind(id).bind(parent).bind(id).bind(now).bind(now).execute(&pool).await.unwrap();
    }
    for i in 0..250 {
        let pid = format!("g{:04}", i);
        seed_product(&pool, &pid, "girls", 12500, now).await;
    }
    seed_product(&pool, "b0001", "boys", 12500, now).await;
    pool
}

async fn seed_product(pool: &SqlitePool, pid: &str, cat: &str, price: i64, now: &str) {
    sqlx::query("INSERT INTO products (product_id,category_id,name,is_active,currency,reorder_point,created_at,updated_at) VALUES (?,?,?,1,'BHD',0,?,?)")
            .bind(pid).bind(cat).bind(pid).bind(now).bind(now).execute(pool).await.unwrap();
    let price_id = format!("pr_{}", pid);
    sqlx::query("INSERT INTO product_prices (price_id,product_id,branch_id,price_type,price_minor,currency,effective_from,created_by_user_id,created_at) VALUES (?,?,NULL,'selling',?,'BHD',?,'seed',?)")
            .bind(&price_id).bind(pid).bind(price).bind(now).bind(now).execute(pool).await.unwrap();
}

async fn current_price(pool: &SqlitePool, pid: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(
            "SELECT price_minor FROM product_prices WHERE product_id=? AND price_type='selling' AND effective_to IS NULL",
        )
        .bind(pid)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn bulk_adjust_changes_only_matched_with_exact_math() {
    let pool = setup().await;
    let sel = Selector {
        category_subtree: Some("girls".into()),
        ..Default::default()
    };
    let total = sel.count(&pool).await.unwrap();
    let run_id = runs::create_run(
        &pool,
        "bulk_price_adjust",
        "{}",
        "{}",
        total,
        "U1",
        "BRANCH1",
    )
    .await
    .unwrap();

    let context = crate::ai::tool_policy::MutationExecutionContext {
        actor_user_id: "U1".into(),
        branch_id: "BRANCH1".into(),
    };
    let changed = crate::ai::tool_policy::with_mutation_context(
        &context,
        execute_price_adjust(
            &pool,
            &run_id,
            &sel,
            &PriceOp::Percent(20.0),
            100,
            |_, _| {},
        ),
    )
    .await
    .unwrap();

    assert_eq!(changed, 250);
    assert_eq!(current_price(&pool, "g0000").await, 15000);
    assert_eq!(current_price(&pool, "g0249").await, 15000);
    assert_eq!(current_price(&pool, "b0001").await, 12500);
    assert_eq!(runs::get_run(&pool, &run_id).await.unwrap().status, "done");
    assert_eq!(runs::get_run(&pool, &run_id).await.unwrap().done_count, 250);
}

#[tokio::test]
async fn undo_restores_every_price() {
    let pool = setup().await;
    let sel = Selector {
        category_subtree: Some("girls".into()),
        ..Default::default()
    };
    let total = sel.count(&pool).await.unwrap();
    let run_id = runs::create_run(
        &pool,
        "bulk_price_adjust",
        "{}",
        "{}",
        total,
        "U1",
        "BRANCH1",
    )
    .await
    .unwrap();
    let context = crate::ai::tool_policy::MutationExecutionContext {
        actor_user_id: "U1".into(),
        branch_id: "BRANCH1".into(),
    };
    crate::ai::tool_policy::with_mutation_context(
        &context,
        execute_price_adjust(
            &pool,
            &run_id,
            &sel,
            &PriceOp::Percent(20.0),
            100,
            |_, _| {},
        ),
    )
    .await
    .unwrap();
    assert_eq!(current_price(&pool, "g0000").await, 15000);

    let restored =
        crate::ai::tool_policy::with_mutation_context(&context, undo_run(&pool, &run_id))
            .await
            .unwrap();

    assert_eq!(restored, 250);
    assert_eq!(current_price(&pool, "g0000").await, 12500);
    assert_eq!(current_price(&pool, "g0249").await, 12500);
    assert_eq!(
        runs::get_run(&pool, &run_id).await.unwrap().status,
        "undone"
    );
}

#[tokio::test]
async fn bulk_stock_set_run_restores_previous_quantities() {
    let pool = setup().await;
    let branch_id = "01JBRANCH0000000000000001";
    let now = "2026-01-01T00:00:00Z";
    for (product_id, quantity) in [("g0000", "5"), ("g0001", "7")] {
        sqlx::query(
            "INSERT INTO stock_levels
                 (stock_level_id,product_id,branch_id,quantity_on_hand,created_at,updated_at)
                 VALUES (?,?,?,?,?,?)",
        )
        .bind(format!("SL-{product_id}"))
        .bind(product_id)
        .bind(branch_id)
        .bind(quantity)
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();
    }
    let selector = Selector {
        category_subtree: Some("girls".into()),
        track_inventory: Some(true),
        ..Default::default()
    };
    let input = serde_json::json!({
        "selector": {
            "category_subtree": "girls",
            "track_inventory": true
        },
        "new_quantity": 100
    });
    let total = selector.count(&pool).await.unwrap();
    let run_id = runs::create_run(
        &pool,
        "bulk_stock_set",
        &serde_json::to_string(&selector).unwrap(),
        &input.to_string(),
        total,
        "01JUSERS000000000000000001",
        branch_id,
    )
    .await
    .unwrap();
    let context = crate::ai::tool_policy::MutationExecutionContext {
        actor_user_id: "01JUSERS000000000000000001".into(),
        branch_id: branch_id.into(),
    };

    let changed = crate::ai::tool_policy::with_mutation_context(
        &context,
        execute_op(
            &pool,
            &run_id,
            &crate::ai::engine::ops::BulkStockSet,
            &selector,
            &input,
            100,
            |_, _| {},
        ),
    )
    .await
    .unwrap();
    assert_eq!(changed, 250);
    let set_quantity: String = sqlx::query_scalar(
        "SELECT quantity_on_hand FROM stock_levels
             WHERE product_id='g0000' AND branch_id=?",
    )
    .bind(branch_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(set_quantity, "100");

    let restored =
        crate::ai::tool_policy::with_mutation_context(&context, undo_run(&pool, &run_id))
            .await
            .unwrap();

    assert_eq!(restored, 250);
    let restored_quantities: Vec<String> = sqlx::query_scalar(
        "SELECT quantity_on_hand FROM stock_levels
             WHERE product_id IN ('g0000','g0001') AND branch_id=?
             ORDER BY product_id",
    )
    .bind(branch_id)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(restored_quantities, vec!["5", "7"]);
    let absent_again: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM stock_levels
             WHERE product_id='g0002' AND branch_id=?",
    )
    .bind(branch_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(absent_again, 0);
}

#[tokio::test]
async fn unsupported_run_undo_fails_without_consuming_log() {
    let pool = setup().await;
    let run_id = runs::create_run(
        &pool,
        "bulk_product_archive",
        "{}",
        "{}",
        1,
        "U1",
        "BRANCH1",
    )
    .await
    .unwrap();
    sqlx::query("INSERT INTO ai_run_undo_log(entry_id,run_id,batch_seq,reverse_json,applied,created_at) VALUES('E1',?,0,'[]',0,datetime('now'))")
            .bind(&run_id).execute(&pool).await.unwrap();
    assert!(undo_run(&pool, &run_id).await.is_err());
    let applied: i64 =
        sqlx::query_scalar("SELECT applied FROM ai_run_undo_log WHERE entry_id='E1'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(applied, 0);
}

#[tokio::test]
async fn stock_set_large_run_does_not_widen_other_bulk_operations() {
    let pool = setup().await;
    sqlx::query(
            "WITH RECURSIVE seq(n) AS (
                 SELECT 1
                 UNION ALL
                 SELECT n + 1 FROM seq WHERE n < 10001
             )
             INSERT INTO products
             (product_id,category_id,name,track_inventory,is_active,currency,reorder_point,created_at,updated_at)
             SELECT printf('large-%05d', n),'toys',printf('Large %05d', n),1,1,'BHD',0,
                    '2026-01-01T00:00:00Z','2026-01-01T00:00:00Z'
             FROM seq",
        )
        .execute(&pool)
        .await
        .unwrap();
    let selector = Selector {
        all_records: true,
        track_inventory: Some(true),
        ..Default::default()
    };

    assert!(validate_bulk_start(&pool, &selector, "bulk_stock_set")
        .await
        .is_ok());
    assert!(validate_bulk_start(&pool, &selector, "bulk_price_adjust")
        .await
        .is_err());
}
