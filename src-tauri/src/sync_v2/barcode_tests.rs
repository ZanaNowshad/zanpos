//! Barcodes: the deletion that came back, and the row the hub could never send.
//!
//! Split out of `tombstone_tests` when the two together passed the 500-line
//! rule. They belong apart anyway — the tombstone tests are about one property
//! shared by several tables, while these are about `product_barcodes`
//! specifically and the three separate defects it carried.
//!
//! The first is the same shape as the customer and shift tombstones: three code
//! paths hard-`DELETE`d from a table with no `is_active` to fall back on, so a
//! removed barcode had no marker at all and the hub handed its copy back.
//!
//! The second is not a tombstone problem and is the reason a terminal reported
//! 28,054 products, 28,119 prices and **0 barcodes** with Resync All changing
//! nothing. `updated_at` arrived in 0028 as `NOT NULL DEFAULT ''` and no insert
//! path set it, and the hub selects what to send with
//! `strftime(updated_at) > strftime(:watermark)` — `strftime('')` is NULL, so
//! the comparison is never true and the row is served to nobody, ever.
//! Resetting a watermark cannot help a predicate that cannot match.
//!
//! The third only exists because of the fix to the first: making uniqueness
//! apply to live rows only turned "a barcode moved product" into two rows the
//! hub serves in no particular order relative to each other.

use crate::sync_v2::apply::apply_row;
use serde_json::json;
use sqlx::SqlitePool;

async fn pool() -> SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    pool
}

/// Deleting a barcode does not stick.
///
/// `product_barcodes` has no `deleted_at` and is registered `Deletion::Never`,
/// but three code paths hard-`DELETE` from it: the admin screen's remove
/// button, and two ZanAI tools. A hard delete leaves nothing to push and no
/// tombstone to carry, so the hub keeps its copy and hands it straight back on
/// the next pull. The barcode returns, and parity reports the table divergent
/// until it does — which is what `product_barcodes` was doing in the mismatched
/// list on the stuck terminal.
#[tokio::test]
async fn a_deleted_barcode_does_not_come_back_from_the_hub() {
    let pool = pool().await;
    sqlx::query(
        "INSERT INTO categories (category_id, name, created_at, updated_at)
         VALUES ('cat_1','Grocery','2026-08-01T00:00:00Z','2026-08-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO products (product_id, category_id, name, is_active, created_at, updated_at)
         VALUES ('prd_1','cat_1','Rice 5kg',1,'2026-08-01T00:00:00Z','2026-08-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO product_barcodes (barcode_id, product_id, barcode, created_at, updated_at)
         VALUES ('bc_1','prd_1','6291001234567','2026-08-01T00:00:00Z','2026-08-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();

    // The operator removes it — the path admin_commands and the AI tools take.
    crate::db::repositories::product_repo::soft_delete_barcode(&pool, "bc_1")
        .await
        .expect("remove barcode");

    // The hub, which has not heard about the removal yet, re-sends its copy.
    let from_hub = json!({
        "barcode_id": "bc_1",
        "product_id": "prd_1",
        "barcode": "6291001234567",
        "created_at": "2026-08-01T00:00:00Z",
        "updated_at": "2026-08-01T00:00:00Z",
    });
    apply_row(&pool, "product_barcodes", &from_hub)
        .await
        .unwrap();

    let live: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM product_barcodes
          WHERE barcode = '6291001234567' AND deleted_at IS NULL",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(live, 0, "the deleted barcode came back from the hub");
}

/// The reported symptom, at the exact layer it happens.
///
/// A terminal showed 28,054 products, 28,119 prices and **0 barcodes**, and
/// pressing Resync All never changed the number. `product_barcodes.updated_at`
/// was added as `NOT NULL DEFAULT ''` and no insert path set it, so every
/// barcode created since carried an empty string — and the hub serves a pull
/// with `strftime(updated_at) > strftime(:watermark)`. `strftime('')` is NULL,
/// `NULL > x` is not true, and the row is never served. Not late: never, to
/// anybody, because resetting a watermark does not change a predicate that
/// cannot match.
#[tokio::test]
async fn a_barcode_with_no_timestamp_can_never_be_served_by_the_hub() {
    let pool = pool().await;
    sqlx::query(
        "INSERT INTO categories (category_id, name, created_at, updated_at)
         VALUES ('cat_1','Grocery','2026-08-01T00:00:00Z','2026-08-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO products (product_id, category_id, name, is_active, created_at, updated_at)
         VALUES ('prd_1','cat_1','Rice 5kg',1,'2026-08-01T00:00:00Z','2026-08-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();

    // A row exactly as the old insert paths left it. Written blank on purpose:
    // omitting the column no longer produces one, because 0058 rebuilt the table
    // with a default that is an actual timestamp. The rows already sitting on
    // every terminal predate that, which is what 0059 is for and what this test
    // still has to be able to reproduce.
    sqlx::query(
        "INSERT INTO product_barcodes (barcode_id, product_id, barcode, created_at, updated_at)
         VALUES ('bc_broken','prd_1','6291001234567','2026-08-01T00:00:00Z','')",
    )
    .execute(&pool)
    .await
    .unwrap();

    // The predicate the hub actually uses, copied from hub::rest::pull_table.
    let served: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM product_barcodes
          WHERE strftime('%Y-%m-%dT%H:%M:%f', updated_at)
              > strftime('%Y-%m-%dT%H:%M:%f', '1970-01-01T00:00:00Z')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        served, 0,
        "the empty timestamp was expected to be unservable"
    );

    // What migration 0059 does to every such row.
    sqlx::query(
        "UPDATE product_barcodes
            SET updated_at = COALESCE(NULLIF(TRIM(updated_at), ''),
                                      NULLIF(TRIM(created_at), ''),
                                      datetime('now')),
                sync_status = 'pending'
          WHERE TRIM(COALESCE(updated_at, '')) = ''",
    )
    .execute(&pool)
    .await
    .unwrap();

    let served_now: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM product_barcodes
          WHERE strftime('%Y-%m-%dT%H:%M:%f', updated_at)
              > strftime('%Y-%m-%dT%H:%M:%f', '1970-01-01T00:00:00Z')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(served_now, 1, "the backfill did not make the row servable");
}

/// Every insert path has to set it, or the backfill only buys time.
#[tokio::test]
async fn no_barcode_insert_path_leaves_the_timestamp_empty() {
    let pool = pool().await;
    let empty: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM product_barcodes WHERE TRIM(COALESCE(updated_at,'')) = ''",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(empty, 0, "the migrations left a barcode with no timestamp");
}

/// A barcode that moved to another product, arriving in the unhelpful order.
///
/// Before 0058 the column-level `UNIQUE` made a barcode exactly one row, so a
/// move arrived as an upsert that rewrote `product_id`. Under the live-rows-only
/// index it arrives as two rows, and the hub does not order them relative to
/// each other. This is the order that used to fail: the new claim first, while
/// the local row still holds the code.
#[tokio::test]
async fn a_barcode_that_moved_products_applies_before_its_tombstone_arrives() {
    let pool = pool().await;
    sqlx::query(
        "INSERT INTO categories (category_id, name, created_at, updated_at)
         VALUES ('cat_1','Grocery','2026-08-01T00:00:00Z','2026-08-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    for id in ["prd_wrong", "prd_right"] {
        sqlx::query(
            "INSERT INTO products (product_id, category_id, name, is_active, created_at, updated_at)
             VALUES (?,'cat_1','Tuna 185g',1,'2026-08-01T00:00:00Z','2026-08-01T00:00:00Z')",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO product_barcodes (barcode_id, product_id, barcode, created_at, updated_at)
         VALUES ('bc_1','prd_wrong','6291001234567',
                 '2026-08-01T00:00:00Z','2026-08-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let moved = serde_json::json!({
        "barcode_id": "bc_2",
        "product_id": "prd_right",
        "barcode":    "6291001234567",
        "created_at": "2026-08-02T09:00:00Z",
        "updated_at": "2026-08-02T09:00:00Z",
    });
    apply_row(&pool, "product_barcodes", &moved)
        .await
        .expect("the moved barcode was refused");

    let live: Vec<String> = sqlx::query_scalar(
        "SELECT product_id FROM product_barcodes
          WHERE barcode = '6291001234567' AND deleted_at IS NULL",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        live,
        vec!["prd_right"],
        "the code resolves to one product only"
    );

    // The retired claim is left pending on purpose: if the hub is the side still
    // holding it, this terminal's tombstone is what repairs it.
    let retired: String = sqlx::query_scalar(
        "SELECT sync_status FROM product_barcodes
          WHERE product_id = 'prd_wrong' AND deleted_at IS NOT NULL",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(retired, "pending");
}

/// An older claim must not unseat a newer one just by arriving late.
#[tokio::test]
async fn a_stale_barcode_move_does_not_displace_the_current_owner() {
    let pool = pool().await;
    sqlx::query(
        "INSERT INTO categories (category_id, name, created_at, updated_at)
         VALUES ('cat_1','Grocery','2026-08-01T00:00:00Z','2026-08-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    for id in ["prd_old", "prd_current"] {
        sqlx::query(
            "INSERT INTO products (product_id, category_id, name, is_active, created_at, updated_at)
             VALUES (?,'cat_1','Tuna 185g',1,'2026-08-01T00:00:00Z','2026-08-01T00:00:00Z')",
        )
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    }
    sqlx::query(
        "INSERT INTO product_barcodes (barcode_id, product_id, barcode, created_at, updated_at)
         VALUES ('bc_current','prd_current','6291001234567',
                 '2026-08-05T00:00:00Z','2026-08-05T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let stale = serde_json::json!({
        "barcode_id": "bc_old",
        "product_id": "prd_old",
        "barcode":    "6291001234567",
        "created_at": "2026-08-01T00:00:00Z",
        "updated_at": "2026-08-01T00:00:00Z",
    });
    let _ = apply_row(&pool, "product_barcodes", &stale).await;

    let live: Vec<String> = sqlx::query_scalar(
        "SELECT product_id FROM product_barcodes
          WHERE barcode = '6291001234567' AND deleted_at IS NULL",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        live,
        vec!["prd_current"],
        "a stale row took the barcode back"
    );
}

/// No path may go back to hard-deleting a barcode.
///
/// This is the check that would have caught the original bug. `product_barcodes`
/// was `Deletion::Never` in the registry while three separate paths removed rows
/// outright — the admin screen's remove button and two ZanAI tools. A hard
/// delete leaves nothing to push, so the hub kept its copy and handed it back on
/// the next pull, and the table sat in the mismatched list with no explanation
/// for why.
///
/// Nothing in the schema prevents writing that statement again; the tombstone is
/// a convention, and conventions decay. Scanning the source is crude, but it
/// fails at the moment the fourth path is written rather than after a shop
/// spends a week wondering why a deleted barcode still scans.
#[test]
fn nothing_hard_deletes_a_barcode() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    // Assembled rather than written out, so this file is not its own first
    // offender. The check found itself on the first run, which is at least
    // evidence that it looks.
    let needle = format!("{} {} {}", "DELETE", "FROM", "product_barcodes");
    let mut offenders = Vec::new();

    fn walk(dir: &std::path::Path, needle: &str, offenders: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, needle, offenders);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                for (n, line) in text.lines().enumerate() {
                    let squashed = line.split_whitespace().collect::<Vec<_>>().join(" ");
                    if squashed.contains(needle) {
                        offenders.push(format!("{}:{}", path.display(), n + 1));
                    }
                }
            }
        }
    }
    walk(&src, &needle, &mut offenders);

    assert!(
        offenders.is_empty(),
        "these hard-delete a barcode; use product_repo::soft_delete_barcode so the \
         removal can reach other terminals: {offenders:?}"
    );
}
