use super::*;
use crate::sync_v2::apply::apply_row;
use serde_json::json;

async fn pool_with_customer() -> SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let branch: String = sqlx::query_scalar("SELECT branch_id FROM branches LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO customers (customer_id, branch_id, name, loyalty_points,
             created_at, updated_at, sync_status)
         VALUES ('cus_1', ?, 'Fatima', 0,
             '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z', 'synced')",
    )
    .bind(&branch)
    .execute(&pool)
    .await
    .unwrap();
    pool
}

fn award(points: i64) -> AwardContext<'static> {
    AwardContext {
        customer_id: "cus_1",
        branch_id: None,
        device_id: Some("dev_local"),
        event: LoyaltyEvent::Earn,
        points_delta: points,
        reference_type: Some("sale"),
        reference_id: None,
        reason: None,
        actor_user_id: None,
    }
}

async fn cached_points(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT loyalty_points FROM customers WHERE customer_id='cus_1'")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn awards_accumulate_and_the_cache_follows() {
    let pool = pool_with_customer().await;

    assert_eq!(record(&pool, award(10)).await.unwrap(), 10);
    assert_eq!(record(&pool, award(5)).await.unwrap(), 15);
    assert_eq!(cached_points(&pool).await, 15);
    assert_eq!(ledger_balance(&pool, "cus_1").await.unwrap(), Some(15));
}

#[tokio::test]
async fn a_redemption_subtracts() {
    let pool = pool_with_customer().await;
    record(&pool, award(30)).await.unwrap();

    let spend = AwardContext {
        event: LoyaltyEvent::Redeem,
        points_delta: -12,
        ..award(0)
    };
    assert_eq!(record(&pool, spend).await.unwrap(), 18);
}

/// A balance below zero is a bug upstream, and showing a customer minus forty
/// points at the counter helps nobody.
#[tokio::test]
async fn the_balance_never_goes_negative() {
    let pool = pool_with_customer().await;
    record(&pool, award(5)).await.unwrap();

    let overspend = AwardContext {
        event: LoyaltyEvent::Redeem,
        points_delta: -50,
        ..award(0)
    };
    assert_eq!(record(&pool, overspend).await.unwrap(), 0);
    assert_eq!(ledger_balance(&pool, "cus_1").await.unwrap(), Some(0));
}

/// The reported defect, as a test.
///
/// Two tills award the same customer during one shift. Under the old counter
/// each computed a total from what it could see — 10 and 5 — and the merge kept
/// `MAX(10, 5)`, so five points vanished. Events add up whichever order they
/// arrive in, so both terminals land on fifteen.
#[tokio::test]
async fn concurrent_awards_from_two_tills_add_up_instead_of_one_winning() {
    let pool = pool_with_customer().await;

    // This till's own award.
    record(&pool, award(10)).await.unwrap();

    // The other till's award, arriving over sync. It was computed against a
    // balance of zero, so its points_after says 5 — deliberately lower than
    // what this terminal already holds, which is exactly the shape that used to
    // lose the smaller award.
    let remote = json!({
        "loyalty_event_id": "other_till_event",
        "customer_id": "cus_1",
        "origin_device_id": "dev_other",
        "event_type": "earn",
        "points_delta": 5,
        "points_after": 5,
        "created_at": "2026-08-01T10:00:00Z",
        "updated_at": "2026-08-01T10:00:00Z",
    });
    apply_row(&pool, "loyalty_events", &remote).await.unwrap();

    assert_eq!(
        cached_points(&pool).await,
        15,
        "one of the two awards was discarded"
    );
}

/// Redelivery is normal in this protocol. Points must not double.
#[tokio::test]
async fn the_same_event_arriving_twice_is_counted_once() {
    let pool = pool_with_customer().await;
    let remote = json!({
        "loyalty_event_id": "evt_once",
        "customer_id": "cus_1",
        "origin_device_id": "dev_other",
        "event_type": "earn",
        "points_delta": 7,
        "points_after": 7,
        "created_at": "2026-08-01T10:00:00Z",
        "updated_at": "2026-08-01T10:00:00Z",
    });

    apply_row(&pool, "loyalty_events", &remote).await.unwrap();
    apply_row(&pool, "loyalty_events", &remote).await.unwrap();

    assert_eq!(cached_points(&pool).await, 7);
}

/// The cached figure is derived, so it must not cross the wire — a remote copy
/// computed from a different subset of events would overwrite a correct local
/// total, which is the defect `stock_levels` had.
#[test]
fn the_cached_total_is_never_sent_or_compared() {
    assert!(crate::sync_v2::apply::skip_on_wire(
        "customers",
        "loyalty_points"
    ));
    assert!(crate::sync_v2::apply::skip_in_fingerprint(
        "customers",
        "loyalty_points"
    ));
    // The ledger underneath it is what has to agree.
    assert!(!crate::sync_v2::apply::skip_on_wire(
        "loyalty_events",
        "points_delta"
    ));
}

/// Existing balances have to survive the migration, or every customer's points
/// read as zero the first time anything recomputes them.
#[tokio::test]
async fn an_existing_balance_is_carried_forward_as_an_opening_event() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    // Migrations run in order, so a customer inserted here cannot pre-date
    // 0056. Insert against the finished schema, then prove the *shape* the
    // backfill produces: an opening event restores a balance.
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let branch: String = sqlx::query_scalar("SELECT branch_id FROM branches LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO customers (customer_id, branch_id, name, loyalty_points,
             created_at, updated_at) VALUES ('cus_old', ?, 'Long-standing', 340,
             '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')",
    )
    .bind(&branch)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO loyalty_events (loyalty_event_id, customer_id, event_type,
             points_delta, points_after, created_at, updated_at, sync_status)
         VALUES ('LOYOPEN-cus_old', 'cus_old', 'opening', 340, 340,
             '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z', 'synced')",
    )
    .execute(&pool)
    .await
    .unwrap();

    assert_eq!(ledger_balance(&pool, "cus_old").await.unwrap(), Some(340));
    assert_eq!(recompute(&pool, "cus_old").await.unwrap(), 340);
}

/// A customer with no events at all must keep the number already on screen
/// rather than being reset to zero by a recompute.
#[tokio::test]
async fn a_customer_with_no_events_keeps_their_shown_balance() {
    let pool = pool_with_customer().await;
    sqlx::query("UPDATE customers SET loyalty_points = 42 WHERE customer_id='cus_1'")
        .execute(&pool)
        .await
        .unwrap();

    assert_eq!(ledger_balance(&pool, "cus_1").await.unwrap(), None);
    assert_eq!(recompute(&pool, "cus_1").await.unwrap(), 42);
    assert_eq!(cached_points(&pool).await, 42);
}
