use super::*;

async fn pool() -> SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO categories (category_id,name,sort_order,is_active,created_at,updated_at)
         VALUES ('cat-1','Dairy',0,1,'2026-01-01','2026-01-01')",
    )
    .execute(&pool)
    .await
    .unwrap();
    pool
}

async fn add_product(pool: &SqlitePool, id: &str, name: &str, cost: i64) {
    sqlx::query(
        "INSERT INTO products
           (product_id, category_id, name, track_inventory, allow_decimal_quantity,
            is_active, cost_minor, currency, reorder_point, created_at, updated_at, version)
         VALUES (?, 'cat-1', ?, 1, 0, 1, ?, 'BHD', 0, '2026-01-01', '2026-01-01', 1)",
    )
    .bind(id)
    .bind(name)
    .bind(cost)
    .execute(pool)
    .await
    .unwrap();
}

/// The bucket a key lands in has to be identical on both machines, across
/// builds and platforms. `DefaultHasher` is documented as free to change
/// between Rust releases; two terminals on different toolchains would then
/// bucket the same key differently and *every* bucket would read as mismatched.
#[test]
fn bucketing_is_pinned_so_two_machines_agree() {
    // Values, not just properties: a change to the hash shows up here rather
    // than as unexplained whole-catalogue drift in a shop.
    assert_eq!(bucket_of("prd_milk", 64), bucket_of("prd_milk", 64));
    assert_eq!(bucket_of("", 64), 37);
    assert_eq!(bucket_of("prd_milk", 64), 17);
    assert_eq!(bucket_of("prd_bread", 64), 4);
    assert_eq!(bucket_of("6291001234567", 64), 41);

    // Spread, so one bucket does not hold the catalogue.
    let spread: std::collections::BTreeSet<u32> =
        (0..500).map(|n| bucket_of(&format!("prd_{n}"), 64)).collect();
    assert!(spread.len() > 50, "only {} buckets used", spread.len());

    // Never out of range, including the degenerate setting.
    assert!((0..64).contains(&bucket_of("anything", 64)));
    assert_eq!(bucket_of("anything", 0), 0);
}

/// Every row is counted exactly once, and the buckets together describe the
/// whole table — otherwise a row could differ in a bucket nobody compares.
#[tokio::test]
async fn every_row_lands_in_exactly_one_bucket() {
    let pool = pool().await;
    for n in 0..40 {
        add_product(&pool, &format!("prd_{n}"), &format!("Product {n}"), 100).await;
    }

    let digests = bucket_digests(&pool, "products", DEFAULT_BUCKETS).await.unwrap();
    assert_eq!(digests.len(), DEFAULT_BUCKETS as usize);
    assert_eq!(digests.iter().map(|d| d.count).sum::<i64>(), 40);
}

/// The point of the feature: one changed row out of many, found by id.
#[tokio::test]
async fn a_single_changed_row_is_narrowed_to_its_id() {
    let local = pool().await;
    let hub = pool().await;
    for n in 0..40 {
        add_product(&local, &format!("prd_{n}"), &format!("Product {n}"), 100).await;
        add_product(&hub, &format!("prd_{n}"), &format!("Product {n}"), 100).await;
    }
    // One cost edited on the hub and never pulled.
    sqlx::query("UPDATE products SET cost_minor = 250 WHERE product_id = 'prd_17'")
        .execute(&hub)
        .await
        .unwrap();

    let local_buckets = bucket_digests(&local, "products", DEFAULT_BUCKETS).await.unwrap();
    let hub_buckets = bucket_digests(&hub, "products", DEFAULT_BUCKETS).await.unwrap();
    let mismatched = mismatched_buckets(&local_buckets, &hub_buckets);

    // One row differs, so exactly one bucket should — this is what keeps the
    // drill-down to a single follow-up request instead of the whole table.
    assert_eq!(mismatched.len(), 1, "{mismatched:?}");

    let bucket = mismatched[0];
    let diff = diff_rows(
        &row_digests(&local, "products", bucket, DEFAULT_BUCKETS).await.unwrap(),
        &row_digests(&hub, "products", bucket, DEFAULT_BUCKETS).await.unwrap(),
    );
    assert_eq!(diff.len(), 1);
    assert_eq!(diff[0].pk, "prd_17");
    assert_eq!(diff[0].divergence, Divergence::Different);
}

#[tokio::test]
async fn a_row_only_one_side_holds_is_named_and_sided() {
    let local = pool().await;
    let hub = pool().await;
    add_product(&local, "prd_shared", "Shared", 100).await;
    add_product(&hub, "prd_shared", "Shared", 100).await;
    // A push that never landed.
    add_product(&local, "prd_only_local", "Local only", 100).await;
    // A pull that never landed.
    add_product(&hub, "prd_only_hub", "Hub only", 100).await;

    let mut found = Vec::new();
    for bucket in 0..DEFAULT_BUCKETS {
        found.extend(diff_rows(
            &row_digests(&local, "products", bucket, DEFAULT_BUCKETS).await.unwrap(),
            &row_digests(&hub, "products", bucket, DEFAULT_BUCKETS).await.unwrap(),
        ));
    }
    found.sort_by(|a, b| a.pk.cmp(&b.pk));

    assert_eq!(found.len(), 2, "{found:?}");
    assert_eq!(found[0].pk, "prd_only_hub");
    assert_eq!(found[0].divergence, Divergence::MissingLocally);
    assert_eq!(found[1].pk, "prd_only_local");
    assert_eq!(found[1].divergence, Divergence::MissingOnHub);
}

/// Terminals in step must report nothing at all. A parity tool that cries wolf
/// gets ignored, and then it is worse than absent.
#[tokio::test]
async fn identical_terminals_report_no_buckets_and_no_rows() {
    let local = pool().await;
    let hub = pool().await;
    for n in 0..25 {
        add_product(&local, &format!("prd_{n}"), &format!("Product {n}"), 100).await;
        add_product(&hub, &format!("prd_{n}"), &format!("Product {n}"), 100).await;
    }

    let local_buckets = bucket_digests(&local, "products", DEFAULT_BUCKETS).await.unwrap();
    let hub_buckets = bucket_digests(&hub, "products", DEFAULT_BUCKETS).await.unwrap();
    assert!(mismatched_buckets(&local_buckets, &hub_buckets).is_empty());
}

/// Columns the table checksum ignores must be ignored here too. `sync_status`
/// is per-device bookkeeping and is never the same on two terminals; counting
/// it would make every row on every table read as divergent forever.
#[tokio::test]
async fn per_device_bookkeeping_columns_are_not_drift() {
    let local = pool().await;
    let hub = pool().await;
    add_product(&local, "prd_1", "Milk", 100).await;
    add_product(&hub, "prd_1", "Milk", 100).await;
    sqlx::query("UPDATE products SET sync_status='synced', sync_attempts=4 WHERE product_id='prd_1'")
        .execute(&hub)
        .await
        .unwrap();

    let bucket = bucket_of("prd_1", DEFAULT_BUCKETS);
    let diff = diff_rows(
        &row_digests(&local, "products", bucket, DEFAULT_BUCKETS).await.unwrap(),
        &row_digests(&hub, "products", bucket, DEFAULT_BUCKETS).await.unwrap(),
    );
    assert!(diff.is_empty(), "{diff:?}");
}

/// The drill-down and the table checksum have to share one definition of
/// equality. If they diverged, parity would name rows the checksum considers
/// identical — which reads as the tool lying.
#[tokio::test]
async fn the_bucket_checksums_agree_with_the_whole_table_checksum() {
    let local = pool().await;
    let hub = pool().await;
    for n in 0..30 {
        add_product(&local, &format!("prd_{n}"), &format!("Product {n}"), 100).await;
        add_product(&hub, &format!("prd_{n}"), &format!("Product {n}"), 100).await;
    }

    let same_table = crate::sync_v2::consistency::table_snapshot(&local, "products").await.unwrap();
    let hub_table = crate::sync_v2::consistency::table_snapshot(&hub, "products").await.unwrap();
    assert_eq!(same_table.checksum, hub_table.checksum);
    assert!(mismatched_buckets(
        &bucket_digests(&local, "products", DEFAULT_BUCKETS).await.unwrap(),
        &bucket_digests(&hub, "products", DEFAULT_BUCKETS).await.unwrap(),
    )
    .is_empty());

    // Now make them differ, and require both measures to notice.
    sqlx::query("UPDATE products SET name='Changed' WHERE product_id='prd_5'")
        .execute(&hub)
        .await
        .unwrap();
    let changed = crate::sync_v2::consistency::table_snapshot(&hub, "products").await.unwrap();
    assert_ne!(same_table.checksum, changed.checksum);
    assert_eq!(
        mismatched_buckets(
            &bucket_digests(&local, "products", DEFAULT_BUCKETS).await.unwrap(),
            &bucket_digests(&hub, "products", DEFAULT_BUCKETS).await.unwrap(),
        )
        .len(),
        1
    );
}

/// A hub answering with a different bucket count is on another setting, and
/// comparing across the two would be meaningless rather than merely wrong.
#[test]
fn a_hub_on_a_different_bucket_count_is_flagged_not_silently_compared() {
    let local = vec![
        BucketDigest { bucket: 0, count: 1, checksum: "a".into() },
        BucketDigest { bucket: 1, count: 1, checksum: "b".into() },
    ];
    let hub = vec![BucketDigest { bucket: 0, count: 1, checksum: "a".into() }];

    assert_eq!(mismatched_buckets(&local, &hub), vec![1]);
}


// ── Acceptance: financial divergence must never read as "in step" ───────────
//
// Every case below returned "100% consistent" before the registry landed,
// because `sales`, `payments` and the rest were synced but never compared.

async fn add_sale(pool: &SqlitePool, id: &str, total: i64) {
    sqlx::query(
        "INSERT INTO sales
           (sale_id, branch_id, device_id, shift_id, cashier_user_id, receipt_number,
            idempotency_key, gross_total_minor, discount_total_minor, tax_total_minor,
            net_total_minor, currency, status, sold_at, business_date, created_at, updated_at)
         VALUES (?, 'br-1', 'dev-1', 'shf-1', 'usr-1', ?, ?, ?, 0, 0, ?, 'BHD', 'completed',
                 '2026-08-24T10:00:00Z', '2026-08-24', '2026-08-24', '2026-08-24')",
    )
    .bind(id)
    .bind(format!("R-{id}"))
    .bind(format!("idem-{id}"))
    .bind(total)
    .bind(total)
    .execute(pool)
    .await
    .unwrap();
}

async fn add_payment(pool: &SqlitePool, id: &str, sale: &str, amount: i64) {
    sqlx::query(
        "INSERT INTO payments
           (payment_id, sale_id, payment_method, amount_minor, recorded_by_user_id,
            recorded_at, created_at, updated_at)
         VALUES (?, ?, 'cash', ?, 'usr-1', '2026-08-24', '2026-08-24', '2026-08-24')",
    )
    .bind(id)
    .bind(sale)
    .bind(amount)
    .execute(pool)
    .await
    .unwrap();
}

fn differs(local: &[BucketDigest], hub: &[BucketDigest]) -> bool {
    !mismatched_buckets(local, hub).is_empty()
}

/// A till missing a sale is the failure the whole exercise is about.
#[tokio::test]
async fn a_sale_present_on_one_side_only_is_reported_as_divergence() {
    let local = pool().await;
    let hub = pool().await;
    for n in 0..5 {
        add_sale(&local, &format!("sale_{n}"), 1000).await;
        add_sale(&hub, &format!("sale_{n}"), 1000).await;
    }
    // The hub took a sale this terminal never received.
    add_sale(&hub, "sale_missing", 4500).await;

    let l = bucket_digests(&local, "sales", DEFAULT_BUCKETS).await.unwrap();
    let h = bucket_digests(&hub, "sales", DEFAULT_BUCKETS).await.unwrap();
    assert!(differs(&l, &h), "a missing sale read as in step");

    let bucket = bucket_of("sale_missing", DEFAULT_BUCKETS);
    let diff = diff_rows(
        &row_digests(&local, "sales", bucket, DEFAULT_BUCKETS).await.unwrap(),
        &row_digests(&hub, "sales", bucket, DEFAULT_BUCKETS).await.unwrap(),
    );
    assert_eq!(diff.len(), 1);
    assert_eq!(diff[0].pk, "sale_missing");
    assert_eq!(diff[0].divergence, Divergence::MissingLocally);
}

#[tokio::test]
async fn an_altered_payment_amount_is_reported_as_divergence() {
    let local = pool().await;
    let hub = pool().await;
    for p in [&local, &hub] {
        add_sale(p, "sale_1", 2500).await;
        add_payment(p, "pay_1", "sale_1", 2500).await;
    }
    sqlx::query("UPDATE payments SET amount_minor = 9900 WHERE payment_id = 'pay_1'")
        .execute(&hub)
        .await
        .unwrap();

    let l = bucket_digests(&local, "payments", DEFAULT_BUCKETS).await.unwrap();
    let h = bucket_digests(&hub, "payments", DEFAULT_BUCKETS).await.unwrap();
    assert!(differs(&l, &h), "an altered payment read as in step");
}

/// Sale lines are where a wrong basket hides: the sale total can agree while
/// the items behind it do not.
#[tokio::test]
async fn sale_items_are_compared_not_just_the_sale_header() {
    let local = pool().await;
    let hub = pool().await;
    for p in [&local, &hub] {
        add_sale(p, "sale_1", 2500).await;
    }
    sqlx::query(
        "INSERT INTO sale_items
           (sale_item_id, sale_id, product_id, product_name_snapshot, quantity,
            unit_price_minor, line_discount_minor, tax_amount_minor, line_total_minor,
            created_at, updated_at)
         VALUES ('si_1','sale_1','prd_1','Milk','1',2500,0,0,2500,'2026-08-24','2026-08-24')",
    )
    .execute(&hub)
    .await
    .unwrap();

    let l = bucket_digests(&local, "sale_items", DEFAULT_BUCKETS).await.unwrap();
    let h = bucket_digests(&hub, "sale_items", DEFAULT_BUCKETS).await.unwrap();
    assert!(differs(&l, &h), "a missing sale line read as in step");
}

/// Once the two sides agree again, parity has to go quiet. A checker that
/// never reaches 100% is one nobody reads.
#[tokio::test]
async fn parity_returns_to_clean_once_the_missing_row_arrives() {
    let local = pool().await;
    let hub = pool().await;
    add_sale(&hub, "sale_late", 3000).await;

    let before = differs(
        &bucket_digests(&local, "sales", DEFAULT_BUCKETS).await.unwrap(),
        &bucket_digests(&hub, "sales", DEFAULT_BUCKETS).await.unwrap(),
    );
    assert!(before);

    // The pull lands.
    add_sale(&local, "sale_late", 3000).await;

    let after = differs(
        &bucket_digests(&local, "sales", DEFAULT_BUCKETS).await.unwrap(),
        &bucket_digests(&hub, "sales", DEFAULT_BUCKETS).await.unwrap(),
    );
    assert!(!after, "parity stayed dirty after the row arrived");
}

/// Every financial table has to be reachable by the drill-down, not just the
/// ones with a convenient primary key.
#[tokio::test]
async fn every_financial_table_can_be_bucketed() {
    let pool = pool().await;
    for table in [
        "sales", "sale_items", "payments", "refunds", "refund_items",
        "cash_events", "shifts", "delivery_orders", "product_cost_history",
    ] {
        let digests = bucket_digests(&pool, table, DEFAULT_BUCKETS).await;
        assert!(digests.is_ok(), "{table}: {:?}", digests.err());
        assert_eq!(digests.unwrap().len(), DEFAULT_BUCKETS as usize, "{table}");
    }
}
