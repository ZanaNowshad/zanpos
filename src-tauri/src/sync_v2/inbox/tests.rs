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

fn row(name: &str, updated_at: &str) -> Map<String, Value> {
    json!({
        "category_id": "cat_1",
        "name": name,
        "updated_at": updated_at,
        "origin_device_id": "dev_a",
    })
    .as_object()
    .unwrap()
    .clone()
}

async fn status_of(pool: &SqlitePool, id: &str) -> Option<String> {
    sqlx::query_scalar("SELECT status FROM sync_inbox WHERE event_id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await
        .unwrap()
}

/// The id has to come out of the payload, because rows cross the wire without
/// one. Identical content must produce an identical id or nothing deduplicates.
#[test]
fn the_same_payload_always_produces_the_same_id() {
    let (first, hash_a) = event_id(
        "categories",
        "cat_1",
        &row("Grocery", "2026-08-01T10:00:00Z"),
    );
    let (second, hash_b) = event_id(
        "categories",
        "cat_1",
        &row("Grocery", "2026-08-01T10:00:00Z"),
    );
    assert_eq!(first, second);
    assert_eq!(hash_a, hash_b);
    assert!(first.starts_with("categories:cat_1:"));
}

/// And a genuine edit to the same row must not be mistaken for a redelivery,
/// or the second edit would be silently discarded.
#[test]
fn an_edit_to_the_same_row_produces_a_different_id() {
    let (unchanged, _) = event_id(
        "categories",
        "cat_1",
        &row("Grocery", "2026-08-01T10:00:00Z"),
    );
    let (edited, _) = event_id(
        "categories",
        "cat_1",
        &row("Produce", "2026-08-01T11:00:00Z"),
    );
    assert_ne!(unchanged, edited);
}

#[tokio::test]
async fn a_first_arrival_is_claimed_and_confirmed() {
    let pool = pool().await;
    let payload = row("Grocery", "2026-08-01T10:00:00Z");
    let (id, _) = event_id("categories", "cat_1", &payload);

    assert_eq!(
        claim(&pool, "categories", "cat_1", &payload).await.unwrap(),
        Decision::Apply
    );
    assert_eq!(status_of(&pool, &id).await.as_deref(), Some("received"));

    confirm(&pool, "categories", "cat_1", &payload).await;
    assert_eq!(status_of(&pool, &id).await.as_deref(), Some("applied"));
}

#[tokio::test]
async fn a_redelivery_of_an_applied_row_is_recognised() {
    let pool = pool().await;
    let payload = row("Grocery", "2026-08-01T10:00:00Z");

    claim(&pool, "categories", "cat_1", &payload).await.unwrap();
    confirm(&pool, "categories", "cat_1", &payload).await;

    assert_eq!(
        claim(&pool, "categories", "cat_1", &payload).await.unwrap(),
        Decision::AlreadySeen
    );
}

/// The bug this guards: a row that failed must be retried, not reported as
/// handled. Treating "seen" as "done" would make a permanently failing row look
/// successful, and the retries that eventually quarantine it would never run.
#[tokio::test]
async fn a_failed_row_is_offered_again_rather_than_treated_as_done() {
    let pool = pool().await;
    let payload = row("Grocery", "2026-08-01T10:00:00Z");

    claim(&pool, "categories", "cat_1", &payload).await.unwrap();
    let attempts = record_failure(&pool, "categories", "cat_1", &payload, "fk missing").await;
    assert_eq!(attempts, 1);

    assert_eq!(
        claim(&pool, "categories", "cat_1", &payload).await.unwrap(),
        Decision::Apply,
        "a failed row was never retried"
    );
}

/// A run that died between claiming and confirming must not strand the row.
#[tokio::test]
async fn a_row_claimed_but_never_confirmed_is_retried() {
    let pool = pool().await;
    let payload = row("Grocery", "2026-08-01T10:00:00Z");

    claim(&pool, "categories", "cat_1", &payload).await.unwrap();
    // No confirm — as if the process died here.
    assert_eq!(
        claim(&pool, "categories", "cat_1", &payload).await.unwrap(),
        Decision::Apply
    );
}

/// The count has to survive a restart, or a row would retry forever and never
/// reach the quarantine threshold.
#[tokio::test]
async fn the_failure_count_accumulates_across_attempts() {
    let pool = pool().await;
    let payload = row("Grocery", "2026-08-01T10:00:00Z");
    claim(&pool, "categories", "cat_1", &payload).await.unwrap();

    let mut last = 0;
    for expected in 1..=5 {
        last = record_failure(&pool, "categories", "cat_1", &payload, "fk missing").await;
        assert_eq!(last, expected);
    }
    assert!(last >= crate::sync_v2::dead_letter::QUARANTINE_AFTER_ATTEMPTS);
}

#[tokio::test]
async fn the_source_device_is_recorded_when_the_row_carries_one() {
    let pool = pool().await;
    let payload = row("Grocery", "2026-08-01T10:00:00Z");
    claim(&pool, "categories", "cat_1", &payload).await.unwrap();

    let device: Option<String> = sqlx::query_scalar("SELECT source_device FROM sync_inbox LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(device.as_deref(), Some("dev_a"));
}

/// Pruning keeps the table from growing without bound, but must never discard
/// the failures — those are the ones somebody still has to look at.
#[tokio::test]
async fn pruning_keeps_failures_and_drops_settled_rows() {
    let pool = pool().await;
    let old = (chrono::Utc::now() - chrono::Duration::days(60)).to_rfc3339();

    for (id, status) in [
        ("e_applied", "applied"),
        ("e_duplicate", "duplicate"),
        ("e_failed", "failed"),
    ] {
        sqlx::query(
            "INSERT INTO sync_inbox (event_id, table_name, entity_id, payload_hash,
                 received_at, status) VALUES (?, 'categories', 'cat_1', 'h', ?, ?)",
        )
        .bind(id)
        .bind(&old)
        .bind(status)
        .execute(&pool)
        .await
        .unwrap();
    }

    let removed = prune(&pool, 30).await.unwrap();
    assert_eq!(removed, 2);

    let left: Vec<String> = sqlx::query_scalar("SELECT event_id FROM sync_inbox")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(left, vec!["e_failed".to_string()]);
}

/// Recent rows stay, or a redelivery arriving a minute later would reprocess.
#[tokio::test]
async fn pruning_leaves_recent_rows_alone() {
    let pool = pool().await;
    let payload = row("Grocery", "2026-08-01T10:00:00Z");
    claim(&pool, "categories", "cat_1", &payload).await.unwrap();
    confirm(&pool, "categories", "cat_1", &payload).await;

    assert_eq!(prune(&pool, 30).await.unwrap(), 0);
}
