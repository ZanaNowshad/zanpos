use super::*;
use serde_json::json;

async fn pool() -> SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    pool
}

fn sale() -> Value {
    json!({
        "sale_id": "SALE_1",
        "net_total_minor": 1250,
        "updated_at": "2026-08-01T10:00:00Z",
    })
}

/// Quarantine is a pause, not a deletion. The payload has to survive in full or
/// setting a row aside is data loss with extra steps.
#[tokio::test]
async fn the_payload_is_kept_verbatim_so_the_row_can_be_replayed() {
    let pool = pool().await;
    quarantine(&pool, "sales", "SALE_1", &sale(), "fk missing", 5)
        .await
        .unwrap();

    let stored: String =
        sqlx::query_scalar("SELECT payload_json FROM sync_dead_letter WHERE entity_id='SALE_1'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let parsed: Value = serde_json::from_str(&stored).unwrap();
    assert_eq!(parsed, sale(), "the quarantined row cannot be reconstructed");
}

/// Setting a row aside weakens a guarantee that used to be absolute, so it has
/// to reach the surface the operator already looks at — not just the log.
#[tokio::test]
async fn quarantining_raises_an_error_level_conflict() {
    let pool = pool().await;
    quarantine(&pool, "sales", "SALE_1", &sale(), "fk missing", 5)
        .await
        .unwrap();

    let (kind, severity, detail): (String, String, String) = sqlx::query_as(
        "SELECT conflict_type, severity, detail FROM sync_conflicts WHERE entity_id='SALE_1'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(kind, "quarantined");
    assert_eq!(severity, "error", "a set-aside sale is not a warning");
    assert!(detail.contains("fk missing"), "{detail}");
    assert!(detail.contains("replayed"), "{detail}");
}

/// A terminal knowingly missing a record must never be able to report itself
/// healthy — that is the whole reason the old behaviour was strict.
#[tokio::test]
async fn a_quarantined_row_makes_the_pending_count_non_zero() {
    let pool = pool().await;
    assert_eq!(pending_count(&pool).await, 0);

    quarantine(&pool, "sales", "SALE_1", &sale(), "fk missing", 5)
        .await
        .unwrap();

    assert_eq!(pending_count(&pool).await, 1);
    let listed = pending(&pool, 10).await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].0, "sales");
    assert_eq!(listed[0].1, "SALE_1");
}

/// The same row failing again must update the record rather than pile up
/// duplicates, or the pending count stops meaning "rows missing".
#[tokio::test]
async fn re_quarantining_the_same_row_updates_rather_than_duplicates() {
    let pool = pool().await;
    quarantine(&pool, "sales", "SALE_1", &sale(), "fk missing", 5)
        .await
        .unwrap();
    quarantine(&pool, "sales", "SALE_1", &sale(), "still missing", 9)
        .await
        .unwrap();

    assert_eq!(pending_count(&pool).await, 1);
    let (attempts, reason): (i64, String) =
        sqlx::query_as("SELECT attempts, reason FROM sync_dead_letter WHERE entity_id='SALE_1'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(attempts, 9);
    assert_eq!(reason, "still missing");
}

/// The threshold is deliberately above one: a dependency arriving out of order
/// looks identical to one that never arrives, and most of them arrive.
#[test]
fn the_threshold_gives_out_of_order_rows_a_chance() {
    assert!(
        QUARANTINE_AFTER_ATTEMPTS >= 3,
        "quarantining this eagerly would set aside rows that were merely early"
    );
}
