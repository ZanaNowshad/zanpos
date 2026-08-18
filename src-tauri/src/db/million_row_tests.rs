use sqlx::{Row, SqlitePool};

async fn migrated_pool() -> SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    pool
}

async fn query_plan(pool: &SqlitePool, sql: &str) -> String {
    sqlx::query(sql)
        .fetch_all(pool)
        .await
        .unwrap()
        .into_iter()
        .map(|row| row.get::<String, _>("detail"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn million_row_report_paths_have_covering_indexes() {
    let pool = migrated_pool().await;
    let sales = query_plan(
        &pool,
        "EXPLAIN QUERY PLAN
         SELECT SUM(net_total_minor), COUNT(*) FROM sales
         WHERE branch_id='B1' AND business_date BETWEEN '2026-01-01' AND '2026-12-31'
           AND status='completed'",
    )
    .await;
    assert!(sales.contains("idx_sales_branch_date_status"), "{sales}");

    let items = query_plan(
        &pool,
        "EXPLAIN QUERY PLAN
         SELECT SUM(CAST(quantity AS REAL)), SUM(line_total_minor)
         FROM sale_items WHERE product_id='P1' AND voided=0",
    )
    .await;
    assert!(items.contains("idx_sale_items_product_voided"), "{items}");

    let inventory = query_plan(
        &pool,
        "EXPLAIN QUERY PLAN
         SELECT COUNT(*) FROM stock_levels
         WHERE branch_id='B1' AND CAST(quantity_on_hand AS REAL) <= 25",
    )
    .await;
    assert!(
        inventory.contains("idx_stock_levels_branch_quantity_numeric"),
        "{inventory}"
    );
}

/// Explicit performance harness. It is ignored in ordinary CI because it
/// creates two million business rows; release qualification runs it directly.
#[tokio::test]
#[ignore = "million-row sales and inventory load harness"]
async fn million_row_sales_and_inventory_queries_stay_bounded() {
    let pool = migrated_pool().await;
    sqlx::query("PRAGMA foreign_keys=OFF")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("CREATE TEMP TABLE load_digits(n INTEGER PRIMARY KEY)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO load_digits(n) VALUES(0),(1),(2),(3),(4),(5),(6),(7),(8),(9)")
        .execute(&pool)
        .await
        .unwrap();
    let number = "(a.n + 10*b.n + 100*c.n + 1000*d.n + 10000*e.n + 100000*f.n)";
    let sales_insert = format!(
        "INSERT INTO sales
           (sale_id, receipt_number, branch_id, device_id, shift_id, cashier_user_id,
            status, net_total_minor, business_date, sold_at, idempotency_key, created_at, updated_at)
         SELECT printf('LS%06d',{number}), printf('R%06d',{number}), 'B1', 'D1', 'S1', 'U1',
                'completed', 1000, '2026-08-15', '2026-08-15T12:00:00Z',
                printf('K%06d',{number}), '2026-08-15T12:00:00Z', '2026-08-15T12:00:00Z'
         FROM load_digits a CROSS JOIN load_digits b CROSS JOIN load_digits c
         CROSS JOIN load_digits d CROSS JOIN load_digits e CROSS JOIN load_digits f"
    );
    sqlx::query(&sales_insert).execute(&pool).await.unwrap();

    let products_insert = format!(
        "INSERT INTO products
           (product_id, category_id, name, currency, created_at, updated_at)
         SELECT printf('LP%06d',{number}), 'LCAT', printf('Load product %06d',{number}),
                'BHD', '2026-08-15T12:00:00Z', '2026-08-15T12:00:00Z'
         FROM load_digits a CROSS JOIN load_digits b CROSS JOIN load_digits c
         CROSS JOIN load_digits d CROSS JOIN load_digits e CROSS JOIN load_digits f"
    );
    sqlx::query(
        "INSERT INTO categories(category_id,name,sort_order,is_active,created_at,updated_at,version)
         VALUES('LCAT','Load',0,1,datetime('now'),datetime('now'),1)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(&products_insert).execute(&pool).await.unwrap();
    let stock_insert = format!(
        "INSERT INTO stock_levels
           (stock_level_id, product_id, branch_id, quantity_on_hand, created_at, updated_at)
         SELECT printf('LST%06d',{number}), printf('LP%06d',{number}), 'B1', '25',
                '2026-08-15T12:00:00Z', '2026-08-15T12:00:00Z'
         FROM load_digits a CROSS JOIN load_digits b CROSS JOIN load_digits c
         CROSS JOIN load_digits d CROSS JOIN load_digits e CROSS JOIN load_digits f"
    );
    sqlx::query(&stock_insert).execute(&pool).await.unwrap();

    let sales_started = std::time::Instant::now();
    let total: i64 = sqlx::query_scalar(
        "SELECT SUM(net_total_minor) FROM sales
         WHERE branch_id='B1' AND business_date='2026-08-15' AND status='completed'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(total, 1_000_000_000);
    assert!(sales_started.elapsed() < std::time::Duration::from_secs(3));

    let inventory_started = std::time::Instant::now();
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM stock_levels WHERE branch_id='B1' AND CAST(quantity_on_hand AS REAL) <= 25",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1_000_000);
    assert!(inventory_started.elapsed() < std::time::Duration::from_secs(3));
}
