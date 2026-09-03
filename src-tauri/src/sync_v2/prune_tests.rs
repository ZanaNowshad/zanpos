//! What retention pruning is allowed to delete.
//!
//! Pruning is the one place in this codebase that removes a row the sync
//! protocol has no way to describe. `sales`, `sale_items`, `payments`,
//! `stock_movements` and `audit_logs` are all `Deletion::Never` in the registry
//! — there is no tombstone for them, only "this row exists" — so a node that
//! quietly drops one is holding a state it cannot communicate and the other
//! nodes cannot reconstruct.
//!
//! Three subsystems each assumed that could not happen, and each broke:
//!
//! * **Parity** compares these tables in full, so a pruned row reads as a lost
//!   delivery. Reconciliation hands it straight back and the next prune deletes
//!   it again.
//! * **The audit chain** links each row to its predecessor's hash, so deleting
//!   the oldest rows made every subsequent verification report tampering.
//! * **Stock** is derived from the movement ledger, which was being truncated
//!   under a setting labelled "logs".
//!
//! These tests pin the contract that came out of that: what may be deleted, what
//! may not, and that every node computes the same answer.

use super::worker::SyncWorker;
use sqlx::SqlitePool;

async fn migrated_pool() -> SqlitePool {
    let path = std::env::temp_dir().join(format!("zanpos_prune_{}.db", ulid::Ulid::new()));
    let pool = SqlitePool::connect(&format!("sqlite:{}?mode=rwc", path.display()))
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    // `last_prune_at` is absent on a fresh database, so the 24-hour cadence gate
    // lets the first call through and each test exercises the deletion rules
    // rather than the scheduling around them.
    pool
}

const DEV: &str = "01JTESTPRUNE000000000001";
const USER: &str = "01JUSER000000000000ADMIN1";

async fn branch(pool: &SqlitePool) -> String {
    sqlx::query_scalar("SELECT branch_id FROM branches LIMIT 1")
        .fetch_one(pool)
        .await
        .unwrap()
}

/// A sale old enough to be past any retention window, with one line and one
/// payment. `sync_status` is set per row so the guards can be exercised.
async fn add_old_sale(pool: &SqlitePool, id: &str, sale: &str, item: &str, payment: &str) {
    let branch = branch(pool).await;
    let old = "2020-01-01T09:00:00Z";

    sqlx::query(
        "INSERT OR IGNORE INTO shifts (shift_id, branch_id, device_id, cashier_user_id,
             status, opened_at, created_at, updated_at)
         VALUES ('01JTESTPRUNESHIFT000001', ?, ?, ?, 'open', ?, ?, ?)",
    )
    .bind(&branch)
    .bind(DEV)
    .bind(USER)
    .bind(old)
    .bind(old)
    .bind(old)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO sales (sale_id, branch_id, device_id, origin_device_id, receipt_number,
             shift_id, cashier_user_id, status, gross_total_minor, discount_total_minor,
             tax_total_minor, net_total_minor, business_date, idempotency_key,
             sold_at, created_at, updated_at, sync_status)
         VALUES (?, ?, ?, ?, ?, '01JTESTPRUNESHIFT000001', ?, 'completed',
             1000, 0, 0, 1000, '2020-01-01', ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(&branch)
    .bind(DEV)
    .bind(DEV)
    .bind(format!("R-{id}"))
    .bind(USER)
    .bind(format!("{id}-ik"))
    .bind(old)
    .bind(old)
    .bind(old)
    .bind(sale)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO sale_items (sale_item_id, sale_id, product_name_snapshot, quantity,
             unit_price_minor, line_total_minor, created_at, updated_at, sync_status)
         VALUES (?, ?, 'Milk', '1', 1000, 1000, ?, ?, ?)",
    )
    .bind(format!("{id}-line"))
    .bind(id)
    .bind(old)
    .bind(old)
    .bind(item)
    .execute(pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO payments (payment_id, sale_id, payment_method, amount_minor,
             recorded_by_user_id, recorded_at, created_at, updated_at, sync_status)
         VALUES (?, ?, 'cash', 1000, ?, ?, ?, ?, ?)",
    )
    .bind(format!("{id}-pay"))
    .bind(id)
    .bind(USER)
    .bind(old)
    .bind(old)
    .bind(old)
    .bind(payment)
    .execute(pool)
    .await
    .unwrap();
}

async fn count(pool: &SqlitePool, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Retention still works. Removing the defects must not quietly turn the
/// feature off — a till that never sheds anything fills its disk.
#[tokio::test]
async fn a_fully_synced_aged_sale_is_still_pruned() {
    let pool = migrated_pool().await;
    add_old_sale(&pool, "S-CLEAN", "synced", "synced", "synced").await;

    SyncWorker::new(pool.clone()).prune_old_data().await;

    assert_eq!(count(&pool, "sales").await, 0);
    assert_eq!(count(&pool, "sale_items").await, 0);
    assert_eq!(count(&pool, "payments").await, 0);
}

/// The data-loss case. `PUSH_ORDER` sends `sales` before `sale_items`, so a push
/// interrupted between the two leaves the sale marked synced and its lines still
/// queued. The old prune deleted lines by their parent's state alone, which
/// destroyed them before they had ever left the till — they existed nowhere
/// else.
#[tokio::test]
async fn a_sale_whose_lines_have_not_reached_the_hub_is_left_alone() {
    let pool = migrated_pool().await;
    add_old_sale(&pool, "S-PARTIAL", "synced", "pending", "synced").await;

    SyncWorker::new(pool.clone()).prune_old_data().await;

    assert_eq!(
        count(&pool, "sale_items").await,
        1,
        "an unsent line was deleted; it existed nowhere else"
    );
    assert_eq!(
        count(&pool, "sales").await,
        1,
        "the parent must stay while a child is still queued, or the line is orphaned"
    );
}

/// Same shape, other child.
#[tokio::test]
async fn a_sale_whose_payment_has_not_reached_the_hub_is_left_alone() {
    let pool = migrated_pool().await;
    add_old_sale(&pool, "S-UNPAIDSYNC", "synced", "synced", "pending").await;

    SyncWorker::new(pool.clone()).prune_old_data().await;

    assert_eq!(count(&pool, "payments").await, 1);
    assert_eq!(count(&pool, "sales").await, 1);
}

/// And the original guard still holds: a sale that has not itself been pushed is
/// the only copy that exists.
#[tokio::test]
async fn an_unsynced_sale_is_never_pruned() {
    let pool = migrated_pool().await;
    add_old_sale(&pool, "S-LOCAL", "pending", "pending", "pending").await;

    SyncWorker::new(pool.clone()).prune_old_data().await;

    assert_eq!(count(&pool, "sales").await, 1);
}

/// The stock ledger is what every terminal derives its stock figure from, and
/// the registry marks it `Parity::Full` for that reason. It was being deleted
/// under `retention_days_logs` — a setting that says nothing about inventory,
/// and documentation that mentions only sales and audit logs.
#[tokio::test]
async fn the_stock_ledger_is_never_pruned() {
    let pool = migrated_pool().await;
    let branch = branch(&pool).await;
    sqlx::query(
        "INSERT INTO categories (category_id, name, created_at, updated_at)
         VALUES ('C-PRUNE','Grocery','2020-01-01T00:00:00Z','2020-01-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO products (product_id, category_id, name, created_at, updated_at)
         VALUES ('P1','C-PRUNE','Milk','2020-01-01T00:00:00Z','2020-01-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO stock_movements (movement_id, product_id, branch_id, device_id,
             movement_type, quantity_delta, quantity_after, created_by_user_id,
             created_at, sync_status)
         VALUES ('M-OLD','P1', ?, ?, 'adjustment', '5', '5', ?, '2020-01-01T00:00:00Z', 'synced')",
    )
    .bind(&branch)
    .bind(DEV)
    .bind(USER)
    .execute(&pool)
    .await
    .unwrap();

    SyncWorker::new(pool.clone()).prune_old_data().await;

    assert_eq!(
        count(&pool, "stock_movements").await,
        1,
        "the ledger stock is derived from was truncated"
    );
}

/// Deleting the oldest links of a hash chain made every later verification
/// report a broken link, so a healthy shop accused itself of tampering. The
/// chain is the shop's evidence; thirty days of it is not evidence.
#[tokio::test]
async fn the_audit_chain_is_never_pruned() {
    use crate::db::repositories::audit_hash::{compute_audit_hash, verify_chain, AuditHashInput};

    let pool = migrated_pool().await;
    let branch = branch(&pool).await;
    let old = "2020-01-01T00:00:00Z";

    // Written the way `insert_audit_entry` would have written it back then —
    // aged `created_at` with the hash that actually covers it. Stamping an old
    // date onto a fresh row instead would change the hashed input and read as
    // tampering, which is the check doing its job rather than a fixture.
    let hash = compute_audit_hash(&AuditHashInput {
        audit_log_id: "A-OLD",
        event_type: "SALE_COMPLETED",
        entity_type: "sale",
        entity_id: "S1",
        actor_user_id: USER,
        actor_type: "user",
        created_at: old,
        before_json: None,
        after_json: None,
        reason: None,
        previous_hash: "",
    });
    sqlx::query(
        "INSERT INTO audit_logs (audit_log_id, event_type, entity_type, entity_id,
             actor_user_id, actor_type, device_id, branch_id, created_at, hash,
             previous_hash, sync_status)
         VALUES ('A-OLD','SALE_COMPLETED','sale','S1', ?, 'user', ?, ?, ?, ?, NULL, 'synced')",
    )
    .bind(USER)
    .bind(DEV)
    .bind(&branch)
    .bind(old)
    .bind(&hash)
    .execute(&pool)
    .await
    .unwrap();

    SyncWorker::new(pool.clone()).prune_old_data().await;

    assert_eq!(
        count(&pool, "audit_logs").await,
        1,
        "the tamper-evidence chain was truncated by housekeeping"
    );
    let chain = verify_chain(&pool, DEV).await.unwrap();
    assert!(chain.ok, "pruning left the chain unverifiable: {chain:?}");
}

// ── One answer to "which terminal is this" ───────────────────────────────────

/// The sync worker had its own copy of identity resolution, and its fallback
/// ordered by `device_code` — the ordering `device_identity`'s module
/// documentation names as the one that must not be used, because `devices` is a
/// synced table and a pulled sibling row can win that ordering.
///
/// The consequence is not cosmetic. This value is the `X-Zanpos-Device` header
/// on every heartbeat and the `origin_device_id` exclusion on every pull, so a
/// wrong answer beats against another till's row and filters that till's work
/// out of what this one is offered.
#[tokio::test]
async fn the_sync_worker_resolves_identity_the_same_way_everything_else_does() {
    let pool = migrated_pool().await;
    let branch = branch(&pool).await;

    // The identity key absent, which is the state the fallback exists for.
    sqlx::query("DELETE FROM app_config WHERE key = 'device_id'")
        .execute(&pool)
        .await
        .unwrap();

    // A sibling's row, pulled in from the hub. Created later than the local
    // seed, but with a device_code that sorts first — which is exactly how the
    // old fallback picked the wrong terminal.
    sqlx::query(
        "INSERT INTO devices (device_id, branch_id, device_code, name, is_active,
             created_at, updated_at)
         VALUES ('SIBLING-FROM-HUB', ?, 'AAA01', 'Other till', 1,
                 '2030-01-01T00:00:00Z','2030-01-01T00:00:00Z')",
    )
    .bind(&branch)
    .execute(&pool)
    .await
    .unwrap();

    let canonical = crate::device_identity::current(&pool).await.unwrap();
    let worker = SyncWorker::new(pool.clone());

    assert_ne!(
        canonical, "SIBLING-FROM-HUB",
        "the canonical resolver itself picked a pulled sibling"
    );
    assert_eq!(
        worker.active_device_id().await.unwrap(),
        canonical,
        "the sync worker disagreed with every other caller about which \
         terminal this is — it would heartbeat as the sibling and have the \
         sibling's rows filtered out of its own pulls"
    );
}

/// The structural guard. Every table this prune touches must be one the registry
/// agrees may lose rows locally — which today means only the sale group, whose
/// deletion is bounded by an identical cutoff on every node. A table added to
/// the prune without that reasoning fails here rather than in a shop.
#[test]
fn pruning_only_touches_tables_whose_deletion_every_node_reproduces() {
    let source = include_str!("worker.rs");
    let prune = source
        .split("async fn prune_old_data")
        .nth(1)
        .expect("prune_old_data must exist");

    let deleted: Vec<&str> = super::registry::TABLES
        .iter()
        .map(|table| table.name)
        .filter(|name| prune.contains(&format!("DELETE FROM {name} ")))
        .collect();

    assert_eq!(
        deleted,
        vec!["sales", "sale_items", "payments"],
        "retention pruning changed which tables it deletes from. Every one of \
         these is Deletion::Never — the protocol cannot express their removal — \
         so a node may only drop rows every other node drops too, at the shared \
         day-boundary cutoff. Adding a table here needs that argument made for it."
    );
}
