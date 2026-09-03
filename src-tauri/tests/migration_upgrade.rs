//! What an existing shop's database becomes after an update.
//!
//! Every test that migrates an empty file proves migrations *apply*. None of
//! them proves the thing that actually matters to a shop that has been trading
//! for a year: that upgrading its database produces the same schema a fresh
//! install gets, and that its takings are still there afterwards.
//!
//! Those two can come apart quietly. A table created in `0001` and altered in
//! `0030` ends up with the columns in a different order — harmless — but a table
//! *rebuilt* in a later migration (`0033` and `0058` both do this: create `_new`,
//! copy, drop, rename) can easily be given a definition that differs from what
//! `0001` plus the intervening ALTERs produced. From then on a fresh install and
//! an upgraded install are running different databases, and only one of them was
//! ever tested.
//!
//! Realistic version boundaries are chosen deliberately: each is a release a
//! shop could actually be sitting on, and each is immediately followed by a
//! migration that rebuilds or restructures a table.

use sqlx::migrate::Migrator;
use sqlx::SqlitePool;
use std::borrow::Cow;
use std::collections::BTreeMap;

fn all_migrations() -> Migrator {
    sqlx::migrate!("./migrations")
}

/// The migrator restricted to everything up to and including `version`.
///
/// Built from the same embedded set the application ships, so an "old" database
/// here is byte-for-byte what that release actually produced — not a hand-copied
/// approximation that could drift from it.
fn migrator_through(version: i64) -> Migrator {
    let all = all_migrations();
    let subset: Vec<_> = all
        .iter()
        .filter(|m| m.version <= version)
        .cloned()
        .collect();
    Migrator {
        migrations: Cow::Owned(subset),
        ..all
    }
}

async fn empty_pool(tag: &str) -> SqlitePool {
    let path = std::env::temp_dir().join(format!("zanpos_upg_{tag}_{}.db", ulid::Ulid::new()));
    let _ = std::fs::remove_file(&path);
    SqlitePool::connect(&format!("sqlite:{}?mode=rwc", path.display()))
        .await
        .expect("open database")
}

/// Table → column name → declared type, plus notnull and default.
///
/// Compared as a map rather than as raw `sqlite_master` SQL: two definitions can
/// differ in whitespace, column order or quoting and still be the same schema,
/// and failing on that would make this test noise rather than signal.
type Schema = BTreeMap<String, BTreeMap<String, String>>;

async fn schema_of(pool: &SqlitePool) -> Schema {
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master
          WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name <> '_sqlx_migrations'
          ORDER BY name",
    )
    .fetch_all(pool)
    .await
    .expect("read tables");

    let mut out = Schema::new();
    for table in tables {
        let cols: Vec<(String, String, i64, Option<String>)> = sqlx::query_as(&format!(
            "SELECT name, type, \"notnull\", dflt_value FROM pragma_table_info('{table}')"
        ))
        .fetch_all(pool)
        .await
        .unwrap_or_default();
        let mut m = BTreeMap::new();
        for (name, ty, notnull, default) in cols {
            m.insert(
                name,
                format!(
                    "{} notnull={} default={}",
                    ty.to_uppercase(),
                    notnull,
                    default.unwrap_or_else(|| "-".into())
                ),
            );
        }
        out.insert(table, m);
    }
    out
}

async fn index_names(pool: &SqlitePool) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT name FROM sqlite_master
          WHERE type='index' AND name NOT LIKE 'sqlite_autoindex%' ORDER BY name",
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default()
}

fn compare(fresh: &Schema, upgraded: &Schema, from: i64) -> Vec<String> {
    let mut diffs = Vec::new();
    for (table, cols) in fresh {
        match upgraded.get(table) {
            None => diffs.push(format!("upgraded-from-{from} is missing table `{table}`")),
            Some(other) => {
                for (col, spec) in cols {
                    match other.get(col) {
                        None => diffs.push(format!("`{table}.{col}` missing after upgrade")),
                        Some(their) if their != spec => diffs.push(format!(
                            "`{table}.{col}` differs — fresh: {spec} / upgraded: {their}"
                        )),
                        _ => {}
                    }
                }
                for col in other.keys() {
                    if !cols.contains_key(col) {
                        diffs.push(format!(
                            "`{table}.{col}` exists only on an upgraded install"
                        ));
                    }
                }
            }
        }
    }
    for table in upgraded.keys() {
        if !fresh.contains_key(table) {
            diffs.push(format!(
                "table `{table}` exists only on an upgraded install"
            ));
        }
    }
    diffs
}

/// A database that started life on an older release must end up identical to one
/// installed today.
async fn assert_upgrade_matches_fresh(from_version: i64) {
    let fresh = empty_pool("fresh").await;
    all_migrations()
        .run(&fresh)
        .await
        .expect("fresh install migrates");

    let upgraded = empty_pool(&format!("v{from_version}")).await;
    migrator_through(from_version)
        .run(&upgraded)
        .await
        .unwrap_or_else(|e| panic!("could not build a v{from_version} database: {e}"));
    all_migrations()
        .run(&upgraded)
        .await
        .unwrap_or_else(|e| panic!("upgrading from v{from_version} failed: {e}"));

    let diffs = compare(
        &schema_of(&fresh).await,
        &schema_of(&upgraded).await,
        from_version,
    );
    assert!(
        diffs.is_empty(),
        "a shop upgrading from v{from_version} ends up with a different schema \
         from a fresh install ({} difference(s)):\n  {}",
        diffs.len(),
        diffs.join("\n  ")
    );

    let fresh_idx = index_names(&fresh).await;
    let upgraded_idx = index_names(&upgraded).await;
    let missing: Vec<&String> = fresh_idx
        .iter()
        .filter(|i| !upgraded_idx.contains(i))
        .collect();
    assert!(
        missing.is_empty(),
        "upgrading from v{from_version} leaves these indexes absent — the same \
         queries will be slower on an upgraded shop than on a new one: {missing:?}"
    );
}

#[tokio::test]
async fn upgrading_from_the_first_release_matches_a_fresh_install() {
    assert_upgrade_matches_fresh(1).await;
}

/// Just before `0033`, which rebuilds `ai_sessions` and `ai_usage_log`.
#[tokio::test]
async fn upgrading_from_v32_matches_a_fresh_install() {
    assert_upgrade_matches_fresh(32).await;
}

/// Just before `0058`, which rebuilds `product_barcodes` to add the tombstone
/// and swap a column-level UNIQUE for a live-rows-only partial index.
#[tokio::test]
async fn upgrading_from_v57_matches_a_fresh_install() {
    assert_upgrade_matches_fresh(57).await;
}

/// The most recent boundary — one release behind today.
#[tokio::test]
async fn upgrading_from_the_previous_release_matches_a_fresh_install() {
    assert_upgrade_matches_fresh(59).await;
}

/// Data written on the old release survives the upgrade intact.
///
/// A schema comparison alone would pass on a migration that rebuilt a table and
/// forgot to copy the rows across — which is exactly the shape of `0033` and
/// `0058`. This puts a shop's takings in before the upgrade and reads them back
/// after.
#[tokio::test]
async fn a_shops_takings_survive_the_upgrade() {
    let pool = empty_pool("data").await;
    migrator_through(32)
        .run(&pool)
        .await
        .expect("build a v32 database");

    let branch: String = sqlx::query_scalar("SELECT branch_id FROM branches LIMIT 1")
        .fetch_one(&pool)
        .await
        .expect("the seed branch exists on v32");

    sqlx::query(
        "INSERT INTO shifts (shift_id, branch_id, device_id, cashier_user_id, status,
             opened_at, created_at, updated_at)
         VALUES ('SH-UPG', ?, '01JDEVICE0000000000000001', '01JUSER000000000000ADMIN1',
                 'open', '2024-01-01T08:00:00Z','2024-01-01T08:00:00Z','2024-01-01T08:00:00Z')",
    )
    .bind(&branch)
    .execute(&pool)
    .await
    .expect("write a shift on the old schema");

    sqlx::query(
        "INSERT INTO sales (sale_id, receipt_number, branch_id, device_id, origin_device_id,
             shift_id, cashier_user_id, status, gross_total_minor, discount_total_minor,
             tax_total_minor, net_total_minor, business_date, idempotency_key,
             sold_at, created_at, updated_at)
         VALUES ('SALE-UPG','MAIN-POS01-00000001', ?, '01JDEVICE0000000000000001',
                 '01JDEVICE0000000000000001','SH-UPG','01JUSER000000000000ADMIN1','completed',
                 3000, 0, 0, 3000, '2024-01-01','idem-upg',
                 '2024-01-01T09:00:00Z','2024-01-01T09:00:00Z','2024-01-01T09:00:00Z')",
    )
    .bind(&branch)
    .execute(&pool)
    .await
    .expect("write a sale on the old schema");

    sqlx::query(
        "INSERT INTO payments (payment_id, sale_id, payment_method, amount_minor,
             recorded_by_user_id, recorded_at, created_at, updated_at)
         VALUES ('PAY-UPG','SALE-UPG','cash',3000,'01JUSER000000000000ADMIN1',
                 '2024-01-01T09:00:00Z','2024-01-01T09:00:00Z','2024-01-01T09:00:00Z')",
    )
    .execute(&pool)
    .await
    .expect("write a payment on the old schema");

    all_migrations()
        .run(&pool)
        .await
        .expect("upgrade a database that already holds trading data");

    let (receipt, net): (String, i64) = sqlx::query_as(
        "SELECT receipt_number, net_total_minor FROM sales WHERE sale_id='SALE-UPG'",
    )
    .fetch_one(&pool)
    .await
    .expect("the sale must still be there after the upgrade");
    assert_eq!(receipt, "MAIN-POS01-00000001");
    assert_eq!(net, 3000, "the sale total changed during the upgrade");

    let paid: i64 =
        sqlx::query_scalar("SELECT amount_minor FROM payments WHERE sale_id='SALE-UPG'")
            .fetch_one(&pool)
            .await
            .expect("the payment must still be there after the upgrade");
    assert_eq!(paid, 3000);

    let shift_open: String =
        sqlx::query_scalar("SELECT status FROM shifts WHERE shift_id='SH-UPG'")
            .fetch_one(&pool)
            .await
            .expect("the shift must still be there after the upgrade");
    assert_eq!(shift_open, "open");
}

/// Foreign keys hold on an upgraded database with data in it.
///
/// `PRAGMA foreign_key_check` is off by default in SQLite, so a migration that
/// rebuilds a parent table can orphan children without anything objecting until
/// a later query quietly returns nothing.
#[tokio::test]
async fn an_upgraded_database_has_no_broken_references() {
    let pool = empty_pool("fk").await;
    migrator_through(32).run(&pool).await.expect("v32");

    let branch: String = sqlx::query_scalar("SELECT branch_id FROM branches LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO categories (category_id, name, created_at, updated_at)
         VALUES ('cat_fk','Grocery','2024-01-01T00:00:00Z','2024-01-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO products (product_id, category_id, name, created_at, updated_at)
         VALUES ('prd_fk','cat_fk','Rice','2024-01-01T00:00:00Z','2024-01-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO product_barcodes (barcode_id, product_id, barcode, created_at, updated_at)
         VALUES ('bc_fk','prd_fk','6291000000001','2024-01-01T00:00:00Z','2024-01-01T00:00:00Z')",
    )
    .bind(&branch)
    .execute(&pool)
    .await
    .ok();

    all_migrations().run(&pool).await.expect("upgrade");

    let violations: Vec<(String, i64, String, i64)> = sqlx::query_as("PRAGMA foreign_key_check")
        .fetch_all(&pool)
        .await
        .unwrap_or_default();
    assert!(
        violations.is_empty(),
        "upgrading left {} orphaned reference(s): {violations:?}",
        violations.len()
    );

    // 0058 rebuilds `product_barcodes`. The barcode written before it must have
    // survived the rebuild, not merely the table.
    let survived: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM product_barcodes WHERE barcode='6291000000001'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        survived, 1,
        "the barcode was lost when 0058 rebuilt its table"
    );
}

/// The comparison above returns nothing. This proves that is a result, not a
/// broken comparator.
///
/// A partly-migrated database is a real schema that genuinely differs from a
/// fresh one, so it is the honest control: `0058` rebuilds `product_barcodes`
/// and `0060` adds `devices.heartbeat_sent_at`, and a v57 database has neither.
/// If `compare` cannot see that, every green result above means nothing.
#[tokio::test]
async fn the_schema_comparison_can_actually_see_a_difference() {
    let fresh = empty_pool("ctrl_fresh").await;
    all_migrations().run(&fresh).await.expect("fresh");

    let stale = empty_pool("ctrl_v57").await;
    migrator_through(57).run(&stale).await.expect("v57");

    let diffs = compare(&schema_of(&fresh).await, &schema_of(&stale).await, 57);
    assert!(
        !diffs.is_empty(),
        "a v57 database compared identical to a fully migrated one — the \
         comparator is blind and every upgrade test above is passing vacuously"
    );
    assert!(
        diffs.iter().any(|d| d.contains("heartbeat_sent_at")),
        "expected the comparison to notice `devices.heartbeat_sent_at`, which \
         0060 adds; it reported: {diffs:?}"
    );
}

/// Re-running the migrator over an already-current database changes nothing.
#[tokio::test]
async fn running_the_migrator_twice_is_a_no_op() {
    let pool = empty_pool("twice").await;
    all_migrations().run(&pool).await.expect("first run");
    let once = schema_of(&pool).await;

    all_migrations()
        .run(&pool)
        .await
        .expect("a second run over a current database must be a no-op");
    let twice = schema_of(&pool).await;

    assert_eq!(once, twice, "re-running migrations altered the schema");
}

/// A till that has already issued a refund does not collide after the upgrade.
///
/// Before `0064`, `sale_repo` read the shared receipt counter as
/// `next_receipt_seq - 1` while `refund_repo` read the post-increment value, so a
/// refund left the counter holding the number it had just printed. Correcting
/// `refund_repo` alone is not enough for a shop already trading: its counter
/// still points at a number that is on a refund receipt, and the first sale after
/// the upgrade would print that number a second time.
///
/// `0064` rebuilds the counter from the numbers actually issued. This sets up a
/// till in exactly the broken state — a refund holding number 2 with the counter
/// still on 2 — upgrades it, and checks the next number is 3.
#[tokio::test]
async fn a_till_that_has_refunded_does_not_reissue_a_number_after_upgrading() {
    let pool = empty_pool("seq").await;
    migrator_through(59)
        .run(&pool)
        .await
        .expect("build a v59 database");

    let branch: String = sqlx::query_scalar("SELECT branch_id FROM branches LIMIT 1")
        .fetch_one(&pool)
        .await
        .expect("the seed branch exists");
    let device = "01JDEVICE0000000000000001";

    sqlx::query(
        "INSERT INTO shifts (shift_id, branch_id, device_id, cashier_user_id, status,
             opened_at, created_at, updated_at)
         VALUES ('SH-SEQ', ?, ?, '01JUSER000000000000ADMIN1', 'open',
                 '2024-01-01T08:00:00Z','2024-01-01T08:00:00Z','2024-01-01T08:00:00Z')",
    )
    .bind(&branch)
    .bind(device)
    .execute(&pool)
    .await
    .expect("shift");

    // Sale number 1, then a refund that took number 2 and left the counter on 2.
    sqlx::query(
        "INSERT INTO sales (sale_id, receipt_number, branch_id, device_id, origin_device_id,
             shift_id, cashier_user_id, status, gross_total_minor, discount_total_minor,
             tax_total_minor, net_total_minor, business_date, idempotency_key,
             sold_at, created_at, updated_at)
         VALUES ('SALE-SEQ','MAIN-POS01-00000001', ?, ?, ?, 'SH-SEQ',
                 '01JUSER000000000000ADMIN1','completed', 3000, 0, 0, 3000, '2024-01-01',
                 'idem-seq','2024-01-01T09:00:00Z','2024-01-01T09:00:00Z','2024-01-01T09:00:00Z')",
    )
    .bind(&branch)
    .bind(device)
    .bind(device)
    .execute(&pool)
    .await
    .expect("sale");

    sqlx::query(
        "INSERT INTO refunds (refund_id, original_sale_id, origin_device_id,
             refund_receipt_number, reason, return_reason_code, refund_total_minor,
             created_by_user_id, idempotency_key, created_at, updated_at)
         VALUES ('REF-SEQ','SALE-SEQ', ?, 'MAIN-POS01-00000002','returned','customer_return',
                 1000,'01JUSER000000000000ADMIN1','refund-REF-SEQ',
                 '2024-01-01T10:00:00Z','2024-01-01T10:00:00Z')",
    )
    .bind(device)
    .execute(&pool)
    .await
    .expect("refund");

    sqlx::query("UPDATE devices SET next_receipt_seq = 2 WHERE device_id = ?")
        .bind(device)
        .execute(&pool)
        .await
        .expect("the counter as the old code left it");

    all_migrations()
        .run(&pool)
        .await
        .expect("upgrade a till that has already refunded");

    let next: i64 = sqlx::query_scalar("SELECT next_receipt_seq FROM devices WHERE device_id = ?")
        .bind(device)
        .fetch_one(&pool)
        .await
        .expect("read the counter back");

    assert_eq!(
        next, 3,
        "the counter still points at a number already printed on a refund receipt; \
         the next sale would carry it too"
    );
}
