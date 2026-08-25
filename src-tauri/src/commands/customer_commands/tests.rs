//! Tests for `customer_commands`.
//!
//! In its own file only for size — `customer_commands.rs` was 1086 lines and
//! the ship gate caps a file at 500. Rust lets `foo.rs` own a `foo/` directory,
//! so this stays a plain `#[cfg(test)] mod tests` of the parent with full
//! access to its private helpers.
use super::*;
use crate::commands::customer_loyalty_commands::{
    customer_loyalty_summary_inner, customer_top_balances_inner,
};
use sqlx::sqlite::SqlitePoolOptions;

const BRANCH_A: &str = "01JBRANCH0000000000000001";
const BRANCH_B: &str = "01JBRANCH0000000000000002";
const USER_A: &str = "01JUSER00000000000BRANCHA";
const USER_B: &str = "01JUSER00000000000BRANCHB";
const ROLE_MANAGER: &str = "01JROLES000000000000000002";
const ROLE_CASHIER: &str = "01JROLES000000000000000003";

#[test]
fn clean_optional_text_trims_and_nulls_blanks() {
    assert_eq!(
        clean_optional_text(Some("  +97333112233  ")).as_deref(),
        Some("+97333112233")
    );
    assert_eq!(clean_optional_text(Some("   ")), None);
    assert_eq!(clean_optional_text(None), None);
}

/// Two branches, each with its own user and a large customer roster, so
/// isolation is tested against real SQL rather than a single-row toy.
async fn make_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("in-memory pool");
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("migrations");

    sqlx::query("UPDATE branches SET is_active = 1 WHERE branch_id = ?")
        .bind(BRANCH_A)
        .execute(&pool)
        .await
        .expect("activate branch A");
    sqlx::query(
        "INSERT OR IGNORE INTO branches
           (branch_id, branch_code, name, is_active, created_at, updated_at, version)
         VALUES (?, 'BRB', 'Branch B', 1, datetime('now'), datetime('now'), 1)",
    )
    .bind(BRANCH_B)
    .execute(&pool)
    .await
    .expect("seed branch B");

    for (user, branch, role, username) in [
        (USER_A, BRANCH_A, ROLE_MANAGER, "mgr_a"),
        (USER_B, BRANCH_B, ROLE_MANAGER, "mgr_b"),
    ] {
        sqlx::query(
            "INSERT OR IGNORE INTO users
               (user_id, branch_id, display_name, username, pin_hash, role_id,
                is_active, created_at, updated_at, version)
             VALUES (?, ?, ?, ?, 'PLAIN:1234', ?, 1, datetime('now'), datetime('now'), 1)",
        )
        .bind(user)
        .bind(branch)
        .bind(username)
        .bind(username)
        .bind(role)
        .execute(&pool)
        .await
        .expect("seed user");
    }
    pool
}

async fn seed_customer(
    pool: &SqlitePool,
    id: &str,
    branch: &str,
    name: &str,
    phone: Option<&str>,
    points: i64,
) {
    sqlx::query(
        "INSERT INTO customers
           (customer_id, branch_id, name, phone, email, loyalty_points, notes,
            origin_device_id, created_at, updated_at, version, sync_status)
         VALUES (?, ?, ?, ?, NULL, ?, NULL, 'dev', datetime('now'), datetime('now'), 1, 'pending')",
    )
    .bind(id)
    .bind(branch)
    .bind(name)
    .bind(phone)
    .bind(points)
    .execute(pool)
    .await
    .expect("seed customer");
}

/// 120 in branch A and 30 in branch B — more than one page either way.
async fn seed_large_roster(pool: &SqlitePool) {
    // `customers(phone)` carries a UNIQUE partial index that is global
    // rather than per-branch, so every fixture phone must differ.
    for i in 0..120 {
        let phone = format!("+973 36{i:06}");
        seed_customer(
            pool,
            &format!("cus_a_{i:04}"),
            BRANCH_A,
            &format!("A Customer {i:04}"),
            if i % 3 == 0 { None } else { Some(&phone) },
            if i % 2 == 0 { 10 } else { 0 },
        )
        .await;
    }
    for i in 0..30 {
        let phone = format!("+973 39{i:06}");
        seed_customer(
            pool,
            &format!("cus_b_{i:04}"),
            BRANCH_B,
            &format!("B Customer {i:04}"),
            Some(&phone),
            1000,
        )
        .await;
    }
}

// ── Branch isolation ─────────────────────────────────────────────────────

#[tokio::test]
async fn list_never_returns_another_branchs_customers() {
    let pool = make_pool().await;
    seed_large_roster(&pool).await;

    let a = customer_list_inner(&pool, USER_A, "", None, Some(200))
        .await
        .expect("branch A list");
    assert_eq!(a.total, 120, "branch A sees exactly its own customers");
    assert!(
        a.items.iter().all(|c| c.branch_id == BRANCH_A),
        "no branch B row may appear in branch A's list"
    );

    let b = customer_list_inner(&pool, USER_B, "", None, Some(200))
        .await
        .expect("branch B list");
    assert_eq!(b.total, 30);
    assert!(b.items.iter().all(|c| c.branch_id == BRANCH_B));
}

#[tokio::test]
async fn search_cannot_reach_across_branches() {
    let pool = make_pool().await;
    seed_large_roster(&pool).await;

    // "B Customer" exists, but not in branch A.
    let leaked = customer_list_inner(&pool, USER_A, "B Customer", None, None)
        .await
        .expect("search");
    assert_eq!(leaked.total, 0, "cross-branch search must find nothing");
    assert!(leaked.items.is_empty());
}

#[tokio::test]
async fn get_hides_a_foreign_customer_as_not_found() {
    let pool = make_pool().await;
    seed_customer(&pool, "cus_b_secret", BRANCH_B, "Secret", None, 0).await;

    let err = customer_in_branch(&pool, "cus_b_secret", BRANCH_A)
        .await
        .expect_err("foreign customer must be rejected");
    // NotFound, not Permission: Permission would confirm the row exists.
    assert!(matches!(err, AppError::NotFound(_)), "got {err:?}");

    customer_in_branch(&pool, "cus_b_secret", BRANCH_B)
        .await
        .expect("own branch may read it");
}

#[tokio::test]
async fn mutations_cannot_cross_branch_by_sending_a_foreign_id() {
    let pool = make_pool().await;
    seed_customer(&pool, "cus_b_target", BRANCH_B, "Target", None, 500).await;

    let actor_branch = actor_branch_id(&pool, USER_A).await.expect("branch");
    assert_eq!(actor_branch, BRANCH_A);
    // This is the guard both customer_update and customer_add_loyalty run
    // before touching anything.
    assert!(customer_in_branch(&pool, "cus_b_target", &actor_branch)
        .await
        .is_err());

    let after: i64 =
        sqlx::query_scalar("SELECT loyalty_points FROM customers WHERE customer_id = ?")
            .bind("cus_b_target")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(after, 500, "the foreign balance must be untouched");
}

#[tokio::test]
async fn actor_branch_comes_from_the_database_not_the_caller() {
    let pool = make_pool().await;
    // A fabricated id resolves to no branch at all.
    assert!(actor_branch_id(&pool, "' OR 1=1 --").await.is_err());
    assert!(actor_branch_id(&pool, "unknown-user").await.is_err());

    sqlx::query("UPDATE users SET is_active = 0 WHERE user_id = ?")
        .bind(USER_A)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        actor_branch_id(&pool, USER_A).await.is_err(),
        "a deactivated user has no branch scope"
    );
}

// ── Paging ───────────────────────────────────────────────────────────────

#[tokio::test]
async fn pages_cover_every_row_exactly_once() {
    let pool = make_pool().await;
    seed_large_roster(&pool).await;

    let mut seen: Vec<String> = Vec::new();
    let mut offset = 0;
    loop {
        let page = customer_list_inner(&pool, USER_A, "", Some(offset), Some(25))
            .await
            .expect("page");
        if page.items.is_empty() {
            break;
        }
        seen.extend(page.items.iter().map(|c| c.customer_id.clone()));
        offset += 25;
        assert!(offset < 500, "paging must terminate");
    }

    assert_eq!(seen.len(), 120, "no gaps");
    let unique: std::collections::HashSet<_> = seen.iter().collect();
    assert_eq!(unique.len(), 120, "no duplicates across pages");
}

#[tokio::test]
async fn list_is_bounded_even_when_a_caller_asks_for_everything() {
    let pool = make_pool().await;
    seed_large_roster(&pool).await;

    let page = customer_list_inner(&pool, USER_A, "", None, Some(100_000))
        .await
        .expect("list");
    assert_eq!(page.limit, CUSTOMER_LIST_MAX_LIMIT);
    assert!(page.items.len() as i64 <= CUSTOMER_LIST_MAX_LIMIT);

    // Nonsense paging is clamped rather than erroring or wrapping.
    let neg = customer_list_inner(&pool, USER_A, "", Some(-5), Some(0))
        .await
        .expect("list");
    assert_eq!(neg.offset, 0);
    assert_eq!(neg.limit, 1);
}

#[tokio::test]
async fn default_page_is_bounded() {
    let pool = make_pool().await;
    seed_large_roster(&pool).await;
    let page = customer_list_inner(&pool, USER_A, "", None, None)
        .await
        .expect("list");
    assert_eq!(page.limit, CUSTOMER_LIST_DEFAULT_LIMIT);
    assert_eq!(page.items.len(), CUSTOMER_LIST_DEFAULT_LIMIT as usize);
    assert_eq!(page.total, 120, "total describes the branch, not the page");
}

#[tokio::test]
async fn search_finds_a_customer_far_beyond_the_first_page() {
    let pool = make_pool().await;
    seed_large_roster(&pool).await;

    // "A Customer 0119" is last in name order — page 5 of 25.
    let hit = customer_list_inner(&pool, USER_A, "0119", None, None)
        .await
        .expect("search");
    assert_eq!(hit.total, 1);
    assert_eq!(hit.items[0].name, "A Customer 0119");
}

#[tokio::test]
async fn search_covers_email_as_well_as_name_and_phone() {
    let pool = make_pool().await;
    sqlx::query(
        "INSERT INTO customers
           (customer_id, branch_id, name, phone, email, loyalty_points, notes,
            origin_device_id, created_at, updated_at, version, sync_status)
         VALUES ('cus_mail', ?, 'Someone', NULL, 'findme@example.test', 0, NULL,
                 'dev', datetime('now'), datetime('now'), 1, 'pending')",
    )
    .bind(BRANCH_A)
    .execute(&pool)
    .await
    .unwrap();

    let hit = customer_list_inner(&pool, USER_A, "findme@", None, None)
        .await
        .expect("search");
    assert_eq!(hit.total, 1, "email is searchable");
}

/// A number typed at the till has no spaces in it; a number saved by a person
/// usually does. Matching only the stored spelling made a customer of a year
/// look like a stranger at the counter, and the receipt went out unattached.
#[tokio::test]
async fn search_finds_a_phone_however_it_was_written_down() {
    let pool = make_pool().await;
    for (id, name, phone) in [
        ("cus_spaced", "Spaced Sayed", "+973 3600 1122"),
        ("cus_dashed", "Dashed Dosari", "973-3600-4455"),
        ("cus_plain", "Plain Ansari", "36007788"),
    ] {
        sqlx::query(
            "INSERT INTO customers
               (customer_id, branch_id, name, phone, email, loyalty_points, notes,
                origin_device_id, created_at, updated_at, version, sync_status)
             VALUES (?, ?, ?, ?, NULL, 0, NULL,
                     'dev', datetime('now'), datetime('now'), 1, 'pending')",
        )
        .bind(id)
        .bind(BRANCH_A)
        .bind(name)
        .bind(phone)
        .execute(&pool)
        .await
        .unwrap();
    }

    for (typed, expected) in [
        ("36001122", "Spaced Sayed"),
        ("36004455", "Dashed Dosari"),
        ("36007788", "Plain Ansari"),
        // The local eight digits are what a cashier reads off a phone screen,
        // but they sometimes type the country code too.
        ("97336001122", "Spaced Sayed"),
    ] {
        let hit = customer_list_inner(&pool, USER_A, typed, None, None)
            .await
            .expect("search");
        assert_eq!(hit.total, 1, "\"{typed}\" should find exactly {expected}");
        assert_eq!(hit.items[0].name, expected);
    }

    // A query with no digits must not fall through to the digit branch and
    // return the whole branch — the sentinel pattern exists for this.
    let none = customer_list_inner(&pool, USER_A, "zzz-no-such-person", None, None)
        .await
        .expect("search");
    assert_eq!(none.total, 0, "a non-matching name matches nothing");
}

// ── Aggregates ───────────────────────────────────────────────────────────

#[tokio::test]
async fn loyalty_totals_cover_the_branch_not_the_page() {
    let pool = make_pool().await;
    seed_large_roster(&pool).await;

    let s = customer_loyalty_summary_inner(&pool, USER_A)
        .await
        .expect("summary");
    // 60 of the 120 branch-A customers hold 10 points each.
    assert_eq!(s.total_customers, 120);
    assert_eq!(s.holders, 60);
    assert_eq!(s.outstanding_points, 600);
    assert!(
        s.total_customers > CUSTOMER_LIST_DEFAULT_LIMIT,
        "the aggregate must exceed one page, or it proves nothing"
    );
}

#[tokio::test]
async fn another_branchs_balances_never_enter_the_total() {
    let pool = make_pool().await;
    seed_large_roster(&pool).await;

    let a = customer_loyalty_summary_inner(&pool, USER_A).await.unwrap();
    let b = customer_loyalty_summary_inner(&pool, USER_B).await.unwrap();
    assert_eq!(a.outstanding_points, 600);
    assert_eq!(b.outstanding_points, 30_000, "30 × 1000, branch B only");
    assert_eq!(b.total_customers, 30);
}

#[tokio::test]
async fn contactable_counts_only_holders_with_a_phone() {
    let pool = make_pool().await;
    seed_customer(&pool, "c1", BRANCH_A, "Has phone", Some("+973 111"), 5).await;
    seed_customer(&pool, "c2", BRANCH_A, "No phone", None, 5).await;
    seed_customer(&pool, "c3", BRANCH_A, "Blank phone", Some("   "), 5).await;
    seed_customer(
        &pool,
        "c4",
        BRANCH_A,
        "Phone no points",
        Some("+973 222"),
        0,
    )
    .await;

    let s = customer_loyalty_summary_inner(&pool, USER_A).await.unwrap();
    assert_eq!(s.holders, 3);
    assert_eq!(s.contactable_holders, 1, "blank phone is not contactable");
    assert_eq!(s.total_customers, 4);
}

#[tokio::test]
async fn empty_branch_aggregates_to_zero_rather_than_null() {
    let pool = make_pool().await;
    let s = customer_loyalty_summary_inner(&pool, USER_A).await.unwrap();
    assert_eq!(s.outstanding_points, 0);
    assert_eq!(s.holders, 0);
    assert_eq!(s.total_customers, 0);
    assert_eq!(s.contactable_holders, 0);
}

#[tokio::test]
async fn top_balances_are_branch_scoped_and_ranked() {
    let pool = make_pool().await;
    seed_customer(&pool, "c_low", BRANCH_A, "Low", None, 5).await;
    seed_customer(&pool, "c_high", BRANCH_A, "High", None, 900).await;
    seed_customer(&pool, "c_zero", BRANCH_A, "Zero", None, 0).await;
    seed_customer(&pool, "c_other", BRANCH_B, "Other branch", None, 99_999).await;

    let top = customer_top_balances_inner(&pool, USER_A, Some(10))
        .await
        .expect("top");
    assert_eq!(
        top.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(),
        vec!["High", "Low"],
        "ranked, zero excluded, other branch absent"
    );
}

#[tokio::test]
async fn a_cashier_may_read_customers_but_an_unknown_actor_may_not() {
    let pool = make_pool().await;
    seed_customer(&pool, "c1", BRANCH_A, "Someone", None, 0).await;
    sqlx::query(
        "INSERT INTO users (user_id, branch_id, display_name, username, pin_hash, role_id,
                            is_active, created_at, updated_at, version)
         VALUES ('cash_a', ?, 'Cashier A', 'cash_a', 'PLAIN:1', ?, 1,
                 datetime('now'), datetime('now'), 1)",
    )
    .bind(BRANCH_A)
    .bind(ROLE_CASHIER)
    .execute(&pool)
    .await
    .unwrap();

    assert!(customer_list_inner(&pool, "cash_a", "", None, None)
        .await
        .is_ok());
    assert!(customer_list_inner(&pool, "nobody", "", None, None)
        .await
        .is_err());
}
