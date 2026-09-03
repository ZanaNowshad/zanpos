#![cfg(test)]
//! A backup is only a backup if it restores.
//!
//! The shop's entire history is one SQLite file. Copying it while the till is
//! trading is the part that has to be right: `PRAGMA wal_checkpoint(TRUNCATE)`
//! followed by a file read is two steps, and a transaction committing between
//! them can leave the copy describing a database that never existed. SQLite has
//! a primitive for exactly this — `VACUUM INTO` takes a read transaction and
//! writes a coherent database out the other side.
//!
//! So these do not check that a file appeared. They restore it, migrate it, and
//! read the shop's takings back out.

use super::{migrated_pool, one_real_sale, CASHIER, DEVICE};
use sqlx::SqlitePool;

/// Where this test's databases live, cleaned up by the OS temp policy.
fn scratch(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("zanpos_backup_{tag}_{}.db", ulid::Ulid::new()))
}

/// Open a database file the way the application does at startup.
async fn open(path: &std::path::Path) -> SqlitePool {
    SqlitePool::connect(&format!("sqlite:{}?mode=rwc", path.display()))
        .await
        .expect("the restored database must open")
}

/// A till with a day's trading on it, written through the real checkout.
async fn a_trading_day(pool: &SqlitePool) -> (String, f64, i64) {
    let (sale, _shift) = one_real_sale(pool, "4", "bk-sale").await;

    sqlx::query(
        "INSERT INTO app_config (key, value, updated_at)
         VALUES ('receipt_footer_test', 'Thank you', datetime('now'))",
    )
    .execute(pool)
    .await
    .expect("a setting");

    let stock: f64 = sqlx::query_scalar::<_, String>(
        "SELECT quantity_on_hand FROM stock_levels WHERE product_id = 'prd_inv'",
    )
    .fetch_one(pool)
    .await
    .unwrap()
    .parse()
    .unwrap();

    let takings: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(net_total_minor), 0) FROM sales WHERE status != 'voided'",
    )
    .fetch_one(pool)
    .await
    .unwrap();

    (sale, stock, takings)
}

/// The whole point: back up a trading store, restore it, and read it back.
///
/// Not "a file exists" — the restored database is opened, the migrator is run
/// against it as it would be on a replacement machine, and the sale, the stock,
/// the settings and the reconciled takings are all read out of it.
#[tokio::test]
async fn a_backup_restores_into_a_working_store() {
    let live = migrated_pool().await;
    let (sale, stock_before, takings_before) = a_trading_day(&live).await;

    // Take the snapshot the way the application should: one statement, one read
    // transaction, a coherent database out the other side.
    let backup_path = scratch("snapshot");
    sqlx::query(&format!("VACUUM INTO '{}'", backup_path.display()))
        .execute(&live)
        .await
        .expect("VACUUM INTO must produce a snapshot");
    assert!(backup_path.exists(), "the snapshot file was not written");

    // A replacement machine: copy the backup in and open it cold.
    let restored_path = scratch("restored");
    std::fs::copy(&backup_path, &restored_path).expect("copy the backup to the new machine");
    let restored = open(&restored_path).await;

    // Migrations run against it, as they would on first launch after a restore.
    sqlx::migrate!("./migrations")
        .run(&restored)
        .await
        .expect("migrations must complete on a restored database");

    // The shop is still there.
    let (receipt, net): (String, i64) =
        sqlx::query_as("SELECT receipt_number, net_total_minor FROM sales WHERE sale_id = ?")
            .bind(&sale)
            .fetch_one(&restored)
            .await
            .expect("the sale must survive the restore");
    assert!(!receipt.is_empty());

    let products: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM products")
        .fetch_one(&restored)
        .await
        .unwrap();
    assert!(products > 0, "the catalogue must survive");

    let stock_after: f64 = sqlx::query_scalar::<_, String>(
        "SELECT quantity_on_hand FROM stock_levels WHERE product_id = 'prd_inv'",
    )
    .fetch_one(&restored)
    .await
    .unwrap()
    .parse()
    .unwrap();
    assert_eq!(stock_after, stock_before, "the shelf count must survive");

    let setting: String =
        sqlx::query_scalar("SELECT value FROM app_config WHERE key = 'receipt_footer_test'")
            .fetch_one(&restored)
            .await
            .expect("settings must survive");
    assert_eq!(setting, "Thank you");

    let users: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&restored)
        .await
        .unwrap();
    assert!(
        users > 0,
        "users must survive — nobody could log in otherwise"
    );

    // And the books still reconcile: the takings equal what was collected.
    let takings_after: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(net_total_minor), 0) FROM sales WHERE status != 'voided'",
    )
    .fetch_one(&restored)
    .await
    .unwrap();
    assert_eq!(takings_after, takings_before);
    assert_eq!(
        takings_after, net,
        "the restored takings must be the sale's own total"
    );

    let collected: i64 = sqlx::query_scalar("SELECT COALESCE(SUM(amount_minor), 0) FROM payments")
        .fetch_one(&restored)
        .await
        .unwrap();
    assert_eq!(
        collected, takings_after,
        "payments and sales must still agree after a restore"
    );

    // The stock ledger still explains the shelf count.
    let drifted = crate::inventory::reconcile::discrepancies(&restored, None, 0.0001)
        .await
        .expect("reconciliation must run on the restored store");
    assert!(
        drifted.is_empty(),
        "a restored store cannot explain its own stock: {drifted:?}"
    );
}

/// A snapshot taken while the till is trading is still a valid database.
///
/// This is the difference between `VACUUM INTO` and copying the file. The copy
/// path is checkpoint-then-read: two steps, with a window between them in which
/// a sale can commit. `VACUUM INTO` holds a read transaction for the duration,
/// so what lands on disk is a database that existed at one instant.
#[tokio::test]
async fn a_snapshot_taken_mid_trade_is_still_a_valid_database() {
    let live = migrated_pool().await;
    let (_sale, _stock, _takings) = a_trading_day(&live).await;

    // Snapshot while writes are landing on the same pool.
    let backup_path = scratch("concurrent");
    let writer = {
        let pool = live.clone();
        tokio::spawn(async move {
            for n in 0..40 {
                let _ = sqlx::query(
                    "INSERT INTO app_config (key, value, updated_at)
                     VALUES (?, ?, datetime('now'))
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                )
                .bind(format!("churn_{n}"))
                .bind(n.to_string())
                .execute(&pool)
                .await;
            }
        })
    };

    sqlx::query(&format!("VACUUM INTO '{}'", backup_path.display()))
        .execute(&live)
        .await
        .expect("a snapshot must be takeable while the till is trading");
    let _ = writer.await;

    // The snapshot opens and passes SQLite's own integrity check — which is what
    // "a valid database" means, rather than "a file of about the right size".
    let restored = open(&backup_path).await;
    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&restored)
        .await
        .expect("integrity check must run");
    assert_eq!(
        integrity, "ok",
        "a snapshot taken during trading did not restore to a sound database"
    );

    // And it is a real store, not an empty file that happens to be well-formed.
    let sales: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sales")
        .fetch_one(&restored)
        .await
        .unwrap();
    assert!(sales > 0, "the snapshot contains no trading history");
}

/// A corrupted backup is refused, not restored over a working store.
///
/// The dangerous failure is not a backup that cannot be read — it is one that
/// reads far enough to overwrite the live database and then stops. Anything
/// restoring a file must satisfy itself the file is a database first.
#[tokio::test]
async fn a_corrupt_backup_is_detectable_before_it_is_trusted() {
    let live = migrated_pool().await;
    a_trading_day(&live).await;

    let good = scratch("good");
    sqlx::query(&format!("VACUUM INTO '{}'", good.display()))
        .execute(&live)
        .await
        .expect("snapshot");

    // Truncate it, as a half-finished copy or a failing disk would.
    let bytes = std::fs::read(&good).expect("read the snapshot");
    let torn = scratch("torn");
    std::fs::write(&torn, &bytes[..bytes.len() / 3]).expect("write a truncated copy");

    let opened = SqlitePool::connect(&format!("sqlite:{}?mode=rw", torn.display())).await;
    let verdict = match opened {
        Err(_) => "refused to open".to_string(),
        Ok(pool) => sqlx::query_scalar::<_, String>("PRAGMA integrity_check")
            .fetch_one(&pool)
            .await
            .unwrap_or_else(|e| format!("integrity check failed: {e}")),
    };
    assert_ne!(
        verdict, "ok",
        "a truncated backup passed verification — restoring it would replace a \
         working store with a broken one"
    );

    // The live store is untouched by any of this.
    let sales: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sales")
        .fetch_one(&live)
        .await
        .unwrap();
    assert!(sales > 0, "the live store must be unaffected");
    let _ = (CASHIER, DEVICE);
}
