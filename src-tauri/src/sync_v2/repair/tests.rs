//! The parts of repair that can be checked without a hub.
//!
//! The round trip over real HTTP — divergence created, reconciled, re-measured
//! — is `tests/reconciliation_e2e.rs`, because a repair that only works against
//! a mock is not a repair.

use super::*;
use crate::sync_v2::reconcile::plan;
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

fn row(pk: &str, divergence: Divergence) -> DivergentRow {
    DivergentRow {
        pk: pk.to_string(),
        divergence,
    }
}

/// Each side is sent only the keys it is supposed to deliver. Getting this
/// backwards would push the terminal's absence over the hub's data.
#[test]
fn each_side_is_asked_only_for_the_rows_it_holds() {
    let plan = plan(
        "products",
        &[
            row("p_hub", Divergence::MissingLocally),
            row("p_local", Divergence::MissingOnHub),
            row("p_both", Divergence::Different),
        ],
    );

    assert_eq!(keys_from(&plan, Side::Hub), vec!["p_hub".to_string()]);
    assert_eq!(
        keys_from(&plan, Side::Terminal),
        vec!["p_local".to_string()]
    );
    // The conflicting row is in neither delivery list.
    assert!(!keys_from(&plan, Side::Hub).contains(&"p_both".to_string()));
    assert!(!keys_from(&plan, Side::Terminal).contains(&"p_both".to_string()));
}

/// A financial conflict must not reach either delivery list, whatever else the
/// plan contains alongside it.
#[test]
fn a_financial_conflict_is_delivered_to_nobody() {
    let plan = plan(
        "payments",
        &[
            row("pay_missing", Divergence::MissingLocally),
            row("pay_conflict", Divergence::Different),
        ],
    );

    let all: Vec<String> = keys_from(&plan, Side::Hub)
        .into_iter()
        .chain(keys_from(&plan, Side::Terminal))
        .collect();
    assert_eq!(all, vec!["pay_missing".to_string()]);
}

#[tokio::test]
async fn local_rows_come_back_shaped_as_the_protocol_sends_them() {
    let pool = pool().await;
    sqlx::query(
        "INSERT INTO categories (category_id, name, created_at, updated_at)
         VALUES ('cat_1','Grocery','2026-08-01T00:00:00Z','2026-08-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let rows = local_rows_by_pk(&pool, "categories", &["cat_1".to_string()])
        .await
        .unwrap();

    assert_eq!(rows.len(), 1);
    let row = rows[0].as_object().unwrap();
    assert_eq!(row.get("category_id"), Some(&json!("cat_1")));
    assert_eq!(row.get("name"), Some(&json!("Grocery")));
    // Per-device bookkeeping must not travel, or the receiving terminal inherits
    // this one's sync state and stops pushing its own changes.
    assert!(
        !row.contains_key("sync_status"),
        "sync_status crossed the wire"
    );
    assert!(!row.contains_key("sync_attempts"));
}

/// Asking for keys that are not there returns nothing rather than erroring —
/// a row deleted between detection and repair is an ordinary race, not a fault.
#[tokio::test]
async fn absent_keys_yield_no_rows_rather_than_an_error() {
    let pool = pool().await;
    let rows = local_rows_by_pk(&pool, "categories", &["nope".to_string()])
        .await
        .unwrap();
    assert!(rows.is_empty());
}

/// Primary keys arrive over the network from the hub. They are bound, never
/// interpolated, so a key shaped like SQL is just a key that matches nothing.
#[tokio::test]
async fn a_primary_key_containing_sql_is_treated_as_data() {
    let pool = pool().await;
    sqlx::query(
        "INSERT INTO categories (category_id, name, created_at, updated_at)
         VALUES ('cat_keep','Keep','2026-08-01T00:00:00Z','2026-08-01T00:00:00Z')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let hostile = "x'); DROP TABLE categories; --".to_string();
    let rows = local_rows_by_pk(&pool, "categories", &[hostile])
        .await
        .unwrap();
    assert!(rows.is_empty());

    let survived: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM categories WHERE category_id='cat_keep'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        survived, 1,
        "the table did not survive a hostile primary key"
    );
}

/// The summary is what an operator reads. It has to distinguish "repaired" from
/// "tried and did not take", because those need opposite responses.
#[test]
fn the_summary_says_whether_the_table_actually_matches_now() {
    let repaired = ReconciliationOutcome {
        table: "products".into(),
        diverged_before: 3,
        delivered_from_hub: 2,
        delivered_to_hub: 1,
        left_for_review: vec![],
        diverged_after: 0,
        audit: vec![],
    };
    let summary = repaired.summary();
    assert!(summary.contains("2 pulled from the hub"), "{summary}");
    assert!(summary.contains("1 pushed to the hub"), "{summary}");
    assert!(summary.contains("table now matches"), "{summary}");

    let stuck = ReconciliationOutcome {
        diverged_after: 3,
        ..repaired.clone()
    };
    assert!(
        stuck.summary().contains("3 still differ"),
        "{}",
        stuck.summary()
    );
    assert!(!stuck.summary().contains("now matches"));
}

/// Rows left for review are named in the summary, because a count alone gives an
/// operator nothing to look at.
#[test]
fn rows_left_for_review_are_named_not_counted() {
    let outcome = ReconciliationOutcome {
        table: "payments".into(),
        diverged_before: 1,
        delivered_from_hub: 0,
        delivered_to_hub: 0,
        left_for_review: vec!["pay_7".into()],
        diverged_after: 1,
        audit: vec![],
    };
    let summary = outcome.summary();
    assert!(summary.contains("pay_7"), "{summary}");
    assert!(summary.contains("1 left for review"), "{summary}");
}

#[test]
fn a_table_that_already_matches_says_so_plainly() {
    let outcome = ReconciliationOutcome {
        table: "products".into(),
        diverged_before: 0,
        delivered_from_hub: 0,
        delivered_to_hub: 0,
        left_for_review: vec![],
        diverged_after: 0,
        audit: vec![],
    };
    assert!(outcome.summary().contains("already identical"));
}

#[test]
fn each_divergence_is_described_in_terms_of_what_went_wrong() {
    let described = describe(&[
        row("a", Divergence::MissingLocally),
        row("b", Divergence::MissingOnHub),
        row("c", Divergence::Different),
    ]);
    assert_eq!(described.len(), 3);
    assert!(described[0].contains("pull that never landed"));
    assert!(described[1].contains("push still queued"));
    assert!(described[2].contains("contents disagree"));
}

/// Reconciling something parity never checks would compare a table against a
/// digest that does not exist and report a clean result.
#[tokio::test]
async fn an_unchecked_table_is_refused_before_any_network_call() {
    let pool = pool().await;
    let client = crate::sync_v2::client::HttpSyncClient::new(
        "http://127.0.0.1:1",
        "unused-test-token",
        Some("dev-test"),
    );

    let err = reconcile_table(&pool, &client, "audit_logs")
        .await
        .unwrap_err();
    assert!(
        format!("{err}").contains("not a parity-checked table"),
        "{err}"
    );
}
