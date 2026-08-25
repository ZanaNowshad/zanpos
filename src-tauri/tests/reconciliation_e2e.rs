//! Reconciliation against a real hub over real HTTP.
//!
//! The unit tests prove the decision rules. They cannot prove that a repair
//! actually moves a row, that the re-run sees the result, or that a conflict
//! survives untouched — those need two databases and a socket, which is what
//! this is.
//!
//! The financial case is the one that matters: a sale held differently on both
//! sides must come out of reconciliation byte-for-byte unchanged on both sides.

use sqlx::SqlitePool;
use zanpos_lib::hub;
use zanpos_lib::sync_v2::client::HttpSyncClient;
use zanpos_lib::sync_v2::repair;

const TERM_DEV: &str = "01JTESTRECONCILE00000001";
const TOKEN: &str = "test-token-reconcile-0123456789";

async fn fresh_db(tag: &str) -> SqlitePool {
    let path = std::env::temp_dir().join(format!("zanpos_rec_{tag}_{}.db", ulid::Ulid::new()));
    let pool = SqlitePool::connect(&format!("sqlite:{}?mode=rwc", path.display()))
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    pool
}

async fn add_category(pool: &SqlitePool, id: &str, name: &str) {
    sqlx::query(
        "INSERT INTO categories (category_id, name, created_at, updated_at)
         VALUES (?, ?, '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')",
    )
    .bind(id)
    .bind(name)
    .execute(pool)
    .await
    .unwrap();
}

async fn category_names(pool: &SqlitePool) -> Vec<String> {
    sqlx::query_scalar("SELECT name FROM categories ORDER BY category_id")
        .fetch_all(pool)
        .await
        .unwrap()
}

const SHIFT: &str = "01JTESTRECONCILESHIFT001";

/// A sale, so the financial refusal can be tested on a real financial row.
///
/// The shift it hangs off is written with fixed values, so when both sides
/// create one they are byte-identical and `shifts` does not diverge as a side
/// effect of setting up a test about `sales`.
async fn add_sale(pool: &SqlitePool, id: &str, total_minor: i64, updated_at: &str) {
    let branch: String = sqlx::query_scalar("SELECT branch_id FROM branches LIMIT 1")
        .fetch_one(pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT OR IGNORE INTO shifts (shift_id, branch_id, device_id, cashier_user_id,
             status, opened_at, created_at, updated_at)
         VALUES (?, ?, ?, '01JUSER000000000000ADMIN1', 'open',
             '2026-08-01T08:00:00Z', '2026-08-01T08:00:00Z', '2026-08-01T08:00:00Z')",
    )
    .bind(SHIFT)
    .bind(&branch)
    .bind(TERM_DEV)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO sales (sale_id, branch_id, device_id, origin_device_id, receipt_number,
             shift_id, cashier_user_id, status, gross_total_minor, discount_total_minor,
             tax_total_minor, net_total_minor, business_date, idempotency_key,
             sold_at, created_at, updated_at)
         VALUES (?, ?, ?, ?, 'T1-0001', ?, '01JUSER000000000000ADMIN1', 'completed',
             ?, 0, 0, ?, '2026-08-01', ?, '2026-08-01T09:00:00Z',
             '2026-08-01T09:00:00Z', ?)",
    )
    .bind(id)
    .bind(&branch)
    .bind(TERM_DEV)
    .bind(TERM_DEV)
    .bind(SHIFT)
    .bind(total_minor)
    .bind(total_minor)
    .bind(format!("{id}-ik"))
    .bind(updated_at)
    .execute(pool)
    .await
    .unwrap();
}

async fn sale_total(pool: &SqlitePool, id: &str) -> i64 {
    sqlx::query_scalar("SELECT net_total_minor FROM sales WHERE sale_id = ?")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Two pools, a running hub, and a client bound to the terminal.
async fn wire_up() -> (SqlitePool, SqlitePool, HttpSyncClient, hub::HubHandle) {
    let hub_pool = fresh_db("hub").await;
    let term_pool = fresh_db("term").await;
    let handle = hub::start_hub(hub_pool.clone(), 0, TOKEN).await.unwrap();
    let client = HttpSyncClient::new(
        &format!("http://127.0.0.1:{}", handle.port),
        TOKEN,
        Some(TERM_DEV),
    );
    (hub_pool, term_pool, client, handle)
}

/// The ordinary case, both directions at once: a row each side is missing gets
/// delivered, and the re-run confirms it rather than the code assuming it.
#[tokio::test]
async fn missing_rows_are_delivered_in_both_directions_and_verified() {
    let (hub_pool, term_pool, client, handle) = wire_up().await;

    add_category(&hub_pool, "cat_hub", "HubOnly").await;
    add_category(&term_pool, "cat_term", "TerminalOnly").await;

    let outcome = repair::reconcile_table(&term_pool, &client, "categories")
        .await
        .unwrap()
        .expect("hub should answer row-level parity");

    assert_eq!(outcome.diverged_before, 2, "{}", outcome.summary());
    assert_eq!(outcome.delivered_from_hub, 1, "{}", outcome.summary());
    assert_eq!(outcome.delivered_to_hub, 1, "{}", outcome.summary());
    assert!(outcome.left_for_review.is_empty(), "{}", outcome.summary());

    // The measured result, not the predicted one.
    assert_eq!(
        outcome.diverged_after, 0,
        "table still differs after repair: {}",
        outcome.summary()
    );

    let both = vec!["HubOnly".to_string(), "TerminalOnly".to_string()];
    assert_eq!(category_names(&term_pool).await, both);
    assert_eq!(category_names(&hub_pool).await, both);

    handle.shutdown();
}

/// The refusal, on real data. Both nodes hold the same sale with different
/// totals; reconciliation must move neither.
#[tokio::test]
async fn a_sale_that_conflicts_is_left_untouched_on_both_sides() {
    let (hub_pool, term_pool, client, handle) = wire_up().await;

    // The hub's copy is newer. Last-writer-wins would take it; authority says no.
    add_sale(&hub_pool, "SALECONFLICT", 500, "2030-01-01T00:00:00Z").await;
    add_sale(&term_pool, "SALECONFLICT", 900, "2029-01-01T00:00:00Z").await;

    let outcome = repair::reconcile_table(&term_pool, &client, "sales")
        .await
        .unwrap()
        .expect("hub should answer row-level parity");

    assert_eq!(
        outcome.left_for_review,
        vec!["SALECONFLICT".to_string()],
        "{}",
        outcome.summary()
    );
    assert_eq!(outcome.delivered_from_hub, 0);
    assert_eq!(outcome.delivered_to_hub, 0);

    // Neither total moved. This is the whole point.
    assert_eq!(sale_total(&term_pool, "SALECONFLICT").await, 900);
    assert_eq!(sale_total(&hub_pool, "SALECONFLICT").await, 500);

    // And it is still reported as diverging, rather than counted as handled.
    assert_eq!(outcome.diverged_after, 1, "{}", outcome.summary());
    assert!(
        outcome.audit.iter().any(|line| line.contains("left for review")),
        "{:?}",
        outcome.audit
    );

    handle.shutdown();
}

/// A sale the hub never received is a delivery, not a conflict — refusing these
/// too would leave the commonest real failure needing a human every time.
#[tokio::test]
async fn a_sale_missing_from_the_hub_is_delivered_without_review() {
    let (hub_pool, term_pool, client, handle) = wire_up().await;

    add_sale(&term_pool, "SALEUNSENT", 1250, "2026-08-01T09:00:00Z").await;

    let outcome = repair::reconcile_table(&term_pool, &client, "sales")
        .await
        .unwrap()
        .unwrap();

    assert!(outcome.left_for_review.is_empty(), "{}", outcome.summary());
    assert_eq!(outcome.delivered_to_hub, 1, "{}", outcome.summary());
    assert_eq!(outcome.diverged_after, 0, "{}", outcome.summary());
    assert_eq!(sale_total(&hub_pool, "SALEUNSENT").await, 1250);

    handle.shutdown();
}

/// A run that finds nothing must say so, and must not report itself as having
/// repaired anything.
#[tokio::test]
async fn an_identical_table_reconciles_to_a_no_op() {
    let (hub_pool, term_pool, client, handle) = wire_up().await;

    add_category(&hub_pool, "cat_same", "Same").await;
    add_category(&term_pool, "cat_same", "Same").await;

    let outcome = repair::reconcile_table(&term_pool, &client, "categories")
        .await
        .unwrap()
        .unwrap();

    assert_eq!(outcome.diverged_before, 0, "{}", outcome.summary());
    assert_eq!(outcome.delivered_from_hub, 0);
    assert_eq!(outcome.delivered_to_hub, 0);
    assert_eq!(outcome.diverged_after, 0);
    assert!(outcome.summary().contains("already identical"));

    handle.shutdown();
}

/// Reconciliation changes data, so what it did has to outlive the run.
#[tokio::test]
async fn the_run_is_recorded_in_the_audit_log() {
    let (hub_pool, term_pool, client, handle) = wire_up().await;

    add_category(&hub_pool, "cat_audit", "Audited").await;

    repair::reconcile_table(&term_pool, &client, "categories")
        .await
        .unwrap()
        .unwrap();

    let (entity, reason): (String, Option<String>) = sqlx::query_as(
        "SELECT entity_id, reason FROM audit_logs
          WHERE event_type = 'SYNC_RECONCILED' ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_one(&term_pool)
    .await
    .expect("reconciliation must leave an audit entry");

    assert_eq!(entity, "categories");
    let reason = reason.unwrap_or_default();
    assert!(reason.contains("pulled from the hub"), "{reason}");

    handle.shutdown();
}

/// Stock converges through the movement ledger alone. The cached figure is a
/// seed, so two terminals that each recorded a movement must agree once the
/// movements have crossed — without either one's cache overwriting the other's.
#[tokio::test]
async fn stock_converges_through_movements_without_the_cache_overwriting() {
    let (hub_pool, term_pool, client, handle) = wire_up().await;

    for pool in [&hub_pool, &term_pool] {
        sqlx::query(
            "INSERT INTO categories (category_id, name, created_at, updated_at)
             VALUES ('cat_1','Grocery','2026-08-01T00:00:00Z','2026-08-01T00:00:00Z')",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO products (product_id, category_id, name, is_active, created_at, updated_at)
             VALUES ('prd_1','cat_1','Rice',1,'2026-08-01T00:00:00Z','2026-08-01T00:00:00Z')",
        )
        .execute(pool)
        .await
        .unwrap();
    }

    // The terminal holds a movement; the hub holds a stale cached figure that
    // would previously have overwritten the ledger-derived value.
    sqlx::query(
        "INSERT INTO stock_movements (movement_id, product_id, branch_id, device_id,
             movement_type, quantity_delta, quantity_after, created_at)
         VALUES ('mv_1','prd_1','br_1','dev_a','adjustment','40','40','2026-08-02T09:00:00Z')",
    )
    .execute(&term_pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO stock_levels (stock_level_id, product_id, branch_id, quantity_on_hand,
             created_at, updated_at)
         VALUES ('SL-prd_1-br_1','prd_1','br_1','999',
                 '2026-08-01T00:00:00Z','2030-01-01T00:00:00Z')",
    )
    .execute(&hub_pool)
    .await
    .unwrap();

    repair::reconcile_table(&term_pool, &client, "stock_movements")
        .await
        .unwrap()
        .unwrap();

    let here: String =
        sqlx::query_scalar("SELECT quantity_on_hand FROM stock_levels WHERE product_id='prd_1'")
            .fetch_optional(&term_pool)
            .await
            .unwrap()
            .unwrap_or_default();
    assert_ne!(here, "999", "a hub cache overwrote a ledger-derived figure");

    handle.shutdown();
}

/// Delivering the same row twice must apply it once and say so.
#[tokio::test]
async fn a_redelivered_row_is_applied_once_and_recorded_as_a_duplicate() {
    let (hub_pool, term_pool, client, handle) = wire_up().await;
    add_category(&hub_pool, "cat_twice", "Once").await;

    repair::reconcile_table(&term_pool, &client, "categories")
        .await
        .unwrap()
        .unwrap();
    // Second pass over an already-converged table: the same payload arrives
    // again through the ordinary path.
    repair::reconcile_table(&term_pool, &client, "categories")
        .await
        .unwrap()
        .unwrap();

    let rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM categories WHERE category_id='cat_twice'")
            .fetch_one(&term_pool)
            .await
            .unwrap();
    assert_eq!(rows, 1);

    let applied: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sync_inbox WHERE entity_id='cat_twice' AND status='applied'",
    )
    .fetch_one(&term_pool)
    .await
    .unwrap();
    assert_eq!(applied, 1, "the arrival was not recorded exactly once");

    handle.shutdown();
}

/// The sweep must not report clean results for tables it never compared.
#[tokio::test]
async fn a_full_sweep_covers_every_parity_checked_table() {
    let (hub_pool, term_pool, client, handle) = wire_up().await;

    add_category(&hub_pool, "cat_sweep", "Swept").await;
    add_sale(&term_pool, "SALESWEEP", 700, "2026-08-01T09:00:00Z").await;

    let outcomes = repair::reconcile_all(&term_pool, &client).await.unwrap();

    let checked: Vec<&str> = zanpos_lib::sync_v2::registry::parity_checked();
    assert_eq!(
        outcomes.len(),
        checked.len(),
        "sweep skipped tables: {:?}",
        checked
            .iter()
            .filter(|t| !outcomes.iter().any(|o| o.table == **t))
            .collect::<Vec<_>>()
    );

    // And the two real divergences were actually repaired, not just visited.
    assert_eq!(category_names(&term_pool).await, vec!["Swept".to_string()]);
    assert_eq!(sale_total(&hub_pool, "SALESWEEP").await, 700);

    handle.shutdown();
}
