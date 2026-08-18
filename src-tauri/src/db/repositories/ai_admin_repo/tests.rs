//! Tests for `ai_admin_repo`.
//!
//! Split out for size only — the parent was 956 lines against a 500-line ship-gate rule.
use super::*;
use crate::db::repositories::ai_admin_actions_repo::ACTION_LIST_MAX_LIMIT;

use sqlx::sqlite::SqlitePoolOptions;

async fn seeded_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    pool
}

/// Prepare one action per status so filtering can be asserted.
async fn seed_actions(pool: &SqlitePool, branch: &str) -> Vec<String> {
    let mut ids = Vec::new();
    for tool in ["a_prepared", "b_executed", "c_cancelled", "d_expired"] {
        let a = create_action(
            pool, "U1", branch, tool, "{}", "hash", "preview", "tok", 600,
        )
        .await
        .unwrap();
        ids.push(a.action_id);
    }
    mark_executed(pool, &ids[1], "{}").await.unwrap();
    mark_cancelled(pool, &ids[2]).await.unwrap();
    sqlx::query("UPDATE ai_actions SET status='expired' WHERE action_id=?")
        .bind(&ids[3])
        .execute(pool)
        .await
        .unwrap();
    ids
}

async fn executed_action_with_undo(pool: &SqlitePool, branch: &str) -> (String, String) {
    let a = create_action(
        pool,
        "U1",
        branch,
        "product_update",
        "{}",
        "h",
        "p",
        "tok",
        600,
    )
    .await
    .unwrap();
    mark_executed(pool, &a.action_id, "{}").await.unwrap();
    let undo = create_undo_record(
        pool,
        &a.action_id,
        "product",
        "P1",
        "{}",
        "product_update",
        "{}",
    )
    .await
    .unwrap();
    (a.action_id, undo.undo_id)
}

#[tokio::test]
async fn available_undo_is_reported() {
    let pool = seeded_pool().await;
    let (action_id, undo_id) = executed_action_with_undo(&pool, "BRANCH1").await;
    let got = get_undo_availability(&pool, "BRANCH1", &action_id)
        .await
        .unwrap()
        .expect("undo record should be visible");
    assert_eq!(got.undo_id, undo_id);
    assert!(got.available);
    assert_eq!(got.status, "available");
    assert!(got.undone_at.is_none());
}

#[tokio::test]
async fn already_used_undo_is_not_available() {
    let pool = seeded_pool().await;
    let (action_id, undo_id) = executed_action_with_undo(&pool, "BRANCH1").await;
    mark_undone(&pool, &undo_id, "U1").await.unwrap();

    let got = get_undo_availability(&pool, "BRANCH1", &action_id)
        .await
        .unwrap()
        .expect("record still exists after being used");
    assert!(
        !got.available,
        "a used undo must never present as available"
    );
    assert_eq!(got.status, "undone");
    assert!(got.undone_at.is_some());
}

#[tokio::test]
async fn action_without_undo_returns_none() {
    let pool = seeded_pool().await;
    let a = create_action(&pool, "U1", "BRANCH1", "t", "{}", "h", "p", "tok", 600)
        .await
        .unwrap();
    assert!(get_undo_availability(&pool, "BRANCH1", &a.action_id)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn undo_lookup_cannot_cross_branches() {
    let pool = seeded_pool().await;
    let (action_id, _) = executed_action_with_undo(&pool, "BRANCH1").await;
    // Same action id, different caller branch: must not resolve.
    assert!(get_undo_availability(&pool, "BRANCH2", &action_id)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn undo_availability_does_not_leak_the_rollback_payload() {
    let pool = seeded_pool().await;
    let a = create_action(&pool, "U1", "BRANCH1", "t", "{}", "h", "p", "tok", 600)
        .await
        .unwrap();
    mark_executed(&pool, &a.action_id, "{}").await.unwrap();
    create_undo_record(
        &pool,
        &a.action_id,
        "product",
        "P1",
        "{\"secret_snapshot\":true}",
        "product_update",
        "{\"secret_rollback\":true}",
    )
    .await
    .unwrap();

    let got = get_undo_availability(&pool, "BRANCH1", &a.action_id)
        .await
        .unwrap()
        .unwrap();
    let json = serde_json::to_string(&got).unwrap();
    assert!(!json.contains("secret_snapshot"), "snapshot leaked");
    assert!(!json.contains("secret_rollback"), "rollback payload leaked");
}

#[tokio::test]
async fn lists_only_the_requested_statuses() {
    let pool = seeded_pool().await;
    seed_actions(&pool, "BRANCH1").await;

    let prepared = list_actions(&pool, "BRANCH1", &["prepared".into()], 50, 0)
        .await
        .unwrap();
    assert_eq!(prepared.len(), 1);
    assert_eq!(prepared[0].status, "prepared");

    for status in ["executed", "cancelled", "expired"] {
        let rows = list_actions(&pool, "BRANCH1", &[status.to_string()], 50, 0)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1, "expected one {status} action");
        assert_eq!(rows[0].status, status);
    }
}

#[tokio::test]
async fn empty_status_filter_returns_every_status() {
    let pool = seeded_pool().await;
    seed_actions(&pool, "BRANCH1").await;
    let rows = list_actions(&pool, "BRANCH1", &[], 50, 0).await.unwrap();
    assert_eq!(rows.len(), 4);
}

#[tokio::test]
async fn unknown_status_matches_nothing_rather_than_erroring() {
    let pool = seeded_pool().await;
    seed_actions(&pool, "BRANCH1").await;
    let rows = list_actions(&pool, "BRANCH1", &["not_a_status".into()], 50, 0)
        .await
        .unwrap();
    assert!(rows.is_empty());
}

#[tokio::test]
async fn one_branch_cannot_see_another_branch_actions() {
    let pool = seeded_pool().await;
    seed_actions(&pool, "BRANCH1").await;
    seed_actions(&pool, "BRANCH2").await;

    let one = list_actions(&pool, "BRANCH1", &[], 50, 0).await.unwrap();
    assert_eq!(one.len(), 4);
    assert!(one.iter().all(|a| a.branch_id == "BRANCH1"));
}

#[tokio::test]
async fn ordering_is_newest_first_and_deterministic() {
    let pool = seeded_pool().await;
    seed_actions(&pool, "BRANCH1").await;
    let rows = list_actions(&pool, "BRANCH1", &[], 50, 0).await.unwrap();
    let mut sorted = rows.clone();
    sorted.sort_by(|a, b| {
        b.prepared_at
            .cmp(&a.prepared_at)
            .then_with(|| b.action_id.cmp(&a.action_id))
    });
    let got: Vec<_> = rows.iter().map(|a| &a.action_id).collect();
    let want: Vec<_> = sorted.iter().map(|a| &a.action_id).collect();
    assert_eq!(got, want);
}

#[tokio::test]
async fn pagination_walks_without_overlap() {
    let pool = seeded_pool().await;
    seed_actions(&pool, "BRANCH1").await;
    let first = list_actions(&pool, "BRANCH1", &[], 2, 0).await.unwrap();
    let second = list_actions(&pool, "BRANCH1", &[], 2, 2).await.unwrap();
    assert_eq!(first.len(), 2);
    assert_eq!(second.len(), 2);
    for a in &first {
        assert!(!second.iter().any(|b| b.action_id == a.action_id));
    }
}

#[tokio::test]
async fn limit_is_clamped_and_negative_offset_is_safe() {
    let pool = seeded_pool().await;
    seed_actions(&pool, "BRANCH1").await;
    // Excessive limit must not bypass the ceiling.
    let rows = list_actions(&pool, "BRANCH1", &[], 10_000, -5)
        .await
        .unwrap();
    assert!(rows.len() as i64 <= ACTION_LIST_MAX_LIMIT);
    assert_eq!(rows.len(), 4);
    // Zero/negative limit falls back to the default rather than returning none.
    let defaulted = list_actions(&pool, "BRANCH1", &[], 0, 0).await.unwrap();
    assert_eq!(defaulted.len(), 4);
}

#[tokio::test]
async fn summary_does_not_carry_the_confirmation_token() {
    let pool = seeded_pool().await;
    create_action(
        &pool,
        "U1",
        "BRANCH1",
        "t",
        "{}",
        "hash",
        "preview",
        "SECRET-TOKEN",
        600,
    )
    .await
    .unwrap();
    let rows = list_actions(&pool, "BRANCH1", &[], 50, 0).await.unwrap();
    let json = serde_json::to_string(&rows).unwrap();
    assert!(
        !json.contains("SECRET-TOKEN"),
        "confirmation token leaked into the queue payload"
    );
    assert!(!json.contains("tool_input_hash"));
}

#[tokio::test]
async fn tool_metrics_aggregate_latency_failures_and_estimated_tokens() {
    let pool = seeded_pool().await;
    record_tool_metric(&pool, "get_stock_levels", true, 40, 12)
        .await
        .unwrap();
    record_tool_metric(&pool, "get_stock_levels", false, 10, 2)
        .await
        .unwrap();

    let rows = list_tool_metrics(&pool, 50).await.unwrap();
    let metric = rows
        .iter()
        .find(|row| row.tool_name == "get_stock_levels")
        .unwrap();
    assert_eq!(metric.invocation_count, 2);
    assert_eq!(metric.failure_count, 1);
    assert_eq!(metric.average_latency_ms, 25);
    assert_eq!(metric.estimated_tokens, 14);
}

#[tokio::test]
async fn prepared_action_persists_authenticated_branch() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();

    let action = create_action(
        &pool,
        "U1",
        "BRANCH1",
        "product_update",
        "{}",
        "hash",
        "preview",
        "confirmation",
        10,
    )
    .await
    .unwrap();

    assert_eq!(action.branch_id, "BRANCH1");
}

#[tokio::test]
async fn cancelled_chat_session_has_one_terminal_status() {
    // Migration 0033 expands the persisted terminal-state constraint.
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    create_session(&pool, "S-CANCEL", "B1", "U1", "test", "test")
        .await
        .unwrap();
    end_session(&pool, "S-CANCEL", "cancelled").await.unwrap();
    let session = get_session(&pool, "S-CANCEL").await.unwrap().unwrap();
    assert_eq!(session.status, "cancelled");
    assert!(session.ended_at.is_some());
    assert!(end_session(&pool, "S-CANCEL", "invented").await.is_err());
}

#[tokio::test]
async fn chat_cleanup_enforces_age_and_per_owner_cap() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    for index in 0..201 {
        sqlx::query("INSERT INTO ai_chat_messages(message_id,session_id,branch_id,user_id,role,content,message_type,created_at) VALUES(?, 'S1','B1','U1','user','x','text',datetime('now'))")
            .bind(format!("M{index}"))
            .execute(&pool).await.unwrap();
    }
    sqlx::query("INSERT INTO ai_chat_messages(message_id,session_id,branch_id,user_id,role,content,message_type,created_at) VALUES('OLD','S2','B2','U2','user','x','text','2000-01-01T00:00:00Z')")
        .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO ai_chat_messages(message_id,session_id,branch_id,user_id,role,content,message_type,created_at) VALUES('BEFORE','S2','B2','U2','user','x','text',datetime('now','-31 days')),('AFTER','S2','B2','U2','user','x','text',datetime('now','-29 days'))")
        .execute(&pool).await.unwrap();
    cleanup_old_messages(&pool, 30).await.unwrap();
    let u1: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM ai_chat_messages WHERE user_id='U1'")
        .fetch_one(&pool)
        .await
        .unwrap();
    let old: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM ai_chat_messages WHERE message_id='OLD'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(u1, 200);
    assert_eq!(old, 0);
    let before: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM ai_chat_messages WHERE message_id='BEFORE'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let after: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM ai_chat_messages WHERE message_id='AFTER'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(before, 0);
    assert_eq!(after, 1);
    assert!(cleanup_old_messages(&pool, 0).await.is_err());
    assert!(cleanup_old_messages(&pool, 3_651).await.is_err());
}
