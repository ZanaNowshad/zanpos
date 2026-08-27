//! Every read tool, run once against a real schema.
//!
//! `get_loyalty_summary` and `export_customers` both queried
//! `customers.points_balance`. No migration has ever created that column — it is
//! `loyalty_points` — so both tools failed on every call, for every shop, with
//! "A database error occurred". Nothing caught it: the code compiles, because
//! SQL in a string is not type-checked, and no test ever executed them.
//!
//! That is the gap this closes. It is not a test of what any tool *says*; it is
//! a test that each one can run at all against the schema the migrations
//! actually produce. A column renamed in a migration, a table dropped, a typo in
//! a `SELECT` — all of them look exactly like this, and all of them reach a shop
//! before anyone notices.
//!
//! The first version of this only ran tools needing no arguments, and that
//! blind spot showed up immediately: `get_supplier_products` had the same
//! broken column and the sweep could not see it, because it wants a
//! `supplier_id`. So argument-taking tools are covered too, from a table of
//! minimal values below. The invariant is now the strong one — **every**
//! registered read tool executes at least once against the migrated schema, and
//! a new tool with no entry here fails rather than going unchecked.

use crate::ai::tool_registry::{ToolKind, ToolRegistry};
use crate::errors::AppError;
use sqlx::SqlitePool;

async fn migrated_pool() -> SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    pool
}

/// Tools that reach outside the database. Excluded because a smoke test must
/// not depend on a network, not because they are exempt from being correct.
fn goes_outside(name: &str) -> bool {
    crate::ai::tool_policy::is_external_content_tool(name)
        || matches!(
            name,
            // Draws a form from its own input; no input, nothing to draw.
            "request_input"
                // Widens the catalogue for the next step; no query to make.
                | "request_full_tool_access"
                // Reads a hub over HTTP.
                | "check_terminal_parity"
                | "find_diverged_rows"
                | "get_catalogue_parity_summary"
                | "preview_reconciliation"
        )
}

/// A plausible value for each required parameter the read tools ask for.
///
/// The values do not have to match real rows — the point is to get the SQL
/// executed, and "no such column" fires whether or not the id exists. A missing
/// entry is a hard failure rather than a skip, so adding a tool with a new
/// parameter forces a decision here instead of quietly dropping coverage.
fn sample_argument(field: &str) -> Option<serde_json::Value> {
    use serde_json::json;
    Some(match field {
        f if f.ends_with("_id") || f == "id" => json!("smoke-test-id"),
        "barcode" => json!("6291001234567"),
        "query" | "search" | "term" | "name" | "product_name" => json!("milk"),
        "date" | "from" | "to" | "date_from" | "date_to" | "from_date" | "to_date"
        | "business_date" => json!("2026-08-24"),
        // Must name a real workflow: the tool looks it up, so a made-up name
        // exercises the not-found path instead of the query behind it.
        "workflow_name" => json!("margin_erosion"),
        "receipt_number" => json!("R-1"),
        "limit" | "days" | "lead_days" | "period_days" | "top_n" | "max_results" => json!(5),
        "threshold_basis_points" | "comparison_years" => json!(1),
        "event_type" => json!("cash_in"),
        "amount_bhd" => json!("1.000"),
        "fix_action" => json!("reset_stuck_sync"),
        f if f.ends_with("_code") => json!("SMOKE01"),
        f if f.ends_with("_type") => json!("cash_in"),
        f if f.ends_with("_action") => json!("reset_stuck_sync"),
        f if f.ends_with("_bhd") => json!("1.000"),
        f if f.ends_with("_number") => json!("R-1"),
        "message" | "text" | "body" | "subject" | "title" => json!("smoke test"),
        // Allowlisted action verbs; "retry" is valid for the conflict resolver
        // and harmless for anything else that names its action this way.
        "action" => json!("retry"),
        // Structured operands for the engine operations. Shapes taken from the
        // schemas in tools_catalogue rather than invented, so the operation
        // reaches its own validation instead of failing to parse.
        "selector" => json!({ "text": "smoke", "active": true }),
        "adjustment" => json!({ "mode": "Percent", "value": 10.0 }),
        // A real, decodable one-row CSV: a placeholder string would fail at the
        // base64 step and never reach the import's own SQL, which is the part
        // this sweep exists to exercise.
        "csv_base64" | "csv" => json!("bmFtZSxjYXRlZ29yeV9pZCxzZWxsaW5nX3ByaWNlX21pbm9yClNtb2tlIFRlc3Qsc21va2UtdGVzdC1pZCwxMDAK"),
        "tab" => json!("products"),
        "url" => json!("https://example.test/"),
        "reason" | "note" | "notes" => json!("smoke test"),
        _ => return None,
    })
}

/// Tools whose schema declares no required fields, so `{}` is a valid call.
fn zero_argument_read_tools() -> Vec<String> {
    let registry = ToolRegistry::global().unwrap();
    crate::ai::tools_catalogue::all_tool_definitions()
        .into_iter()
        .filter(|definition| {
            registry
                .get(&definition.name)
                .is_some_and(|d| d.kind == ToolKind::Read)
        })
        .filter(|definition| !goes_outside(&definition.name))
        .filter(|definition| {
            definition
                .input_schema
                .get("required")
                .and_then(|r| r.as_array())
                .is_none_or(|r| r.is_empty())
        })
        .map(|definition| definition.name)
        .collect()
}

/// A read tool that cannot execute against an empty, fully migrated database is
/// broken for every shop on its first day — which is exactly the state a new
/// install is in.
#[tokio::test]
async fn every_zero_argument_read_tool_runs_against_the_real_schema() {
    let pool = migrated_pool().await;
    let names = zero_argument_read_tools();

    // If this ever collapses to a handful, the filter above has broken and the
    // test is passing by not testing anything.
    assert!(
        names.len() > 40,
        "only {} tools smoke-tested — the filter is wrong",
        names.len()
    );

    let mut broken = Vec::new();
    for name in &names {
        match crate::ai::tools::execute_read_tool(&pool, name, &serde_json::json!({}), "br-1", 3)
            .await
        {
            Ok(_) => {}
            // A Database error is the signal: the SQL does not match the schema.
            // Validation and NotFound are legitimate answers to an empty shop.
            Err(AppError::Database(e)) => broken.push(format!("{name}: {e}")),
            Err(_) => {}
        }
    }

    assert!(
        broken.is_empty(),
        "{} read tool(s) cannot run against the migrated schema:\n  {}",
        broken.len(),
        broken.join("\n  ")
    );
}

/// The specific pair that was broken, pinned by name.
///
/// The sweep above would catch them again, but only while they stay
/// zero-argument. Naming them keeps the regression covered if either grows a
/// required parameter later.
#[tokio::test]
async fn the_customer_tools_that_queried_a_column_that_never_existed_now_run() {
    let pool = migrated_pool().await;
    for name in ["get_loyalty_summary", "export_customers", "list_customers"] {
        let result =
            crate::ai::tools::execute_read_tool(&pool, name, &serde_json::json!({}), "br-1", 3)
                .await;
        assert!(
            !matches!(result, Err(AppError::Database(_))),
            "{name} still fails against the schema: {result:?}"
        );
    }
}

/// The strong invariant: every registered read tool executes, arguments and all.
///
/// This is the test that would have caught `get_supplier_products`, which the
/// zero-argument sweep structurally could not reach.
#[tokio::test]
async fn every_read_tool_including_those_taking_arguments_runs_against_the_schema() {
    let pool = migrated_pool().await;
    let registry = ToolRegistry::global().unwrap();

    let mut broken = Vec::new();
    let mut uncovered = Vec::new();
    let mut exercised = 0usize;

    for definition in crate::ai::tools_catalogue::all_tool_definitions() {
        let is_read = registry
            .get(&definition.name)
            .is_some_and(|d| d.kind == ToolKind::Read);
        if !is_read || goes_outside(&definition.name) {
            continue;
        }

        let required: Vec<String> = definition
            .input_schema
            .get("required")
            .and_then(|r| r.as_array())
            .map(|r| r.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
            .unwrap_or_default();

        let mut input = serde_json::Map::new();
        let mut missing = Vec::new();
        for field in &required {
            match sample_argument(field) {
                Some(value) => {
                    input.insert(field.clone(), value);
                }
                // No sample for this parameter: coverage would silently lapse.
                None => missing.push(field.clone()),
            }
        }
        if !missing.is_empty() {
            uncovered.push(format!("{}: no sample for {:?}", definition.name, missing));
            continue;
        }

        exercised += 1;
        if let Err(AppError::Database(e)) = crate::ai::tools::execute_read_tool(
            &pool,
            &definition.name,
            &serde_json::Value::Object(input),
            "br-1",
            3,
        )
        .await
        {
            broken.push(format!("{}: {e}", definition.name));
        }
    }

    assert!(
        uncovered.is_empty(),
        "{} read tool(s) have parameters with no sample value — add one to          `sample_argument` rather than leaving them unexercised:
  {}",
        uncovered.len(),
        uncovered.join("
  ")
    );
    assert!(
        broken.is_empty(),
        "{} read tool(s) cannot run against the migrated schema:
  {}",
        broken.len(),
        broken.join("
  ")
    );
    // Strictly more than the zero-argument sweep, or the argument path is not
    // actually being taken.
    assert!(
        exercised > zero_argument_read_tools().len(),
        "argument-taking tools are not being reached: {exercised} exercised"
    );
}


mod mutations;

// ── The customer lookup ZanAI actually uses ──────────────────────────────────
//
// "Message customer X on WhatsApp" is two steps: resolve the name to a
// customer, then send. The resolve step is `list_customers`, and it carried its
// own `WHERE name LIKE ? OR phone LIKE ?` rather than the predicate the till and
// the customer directory share. So it could not match a WhatsApp name, could not
// match an email, could not match a number typed without separators — and had
// neither a branch filter nor a soft-delete filter, so it read across branches
// and offered deleted people.

async fn seed_customer_for_lookup(
    pool: &SqlitePool,
    id: &str,
    branch: &str,
    name: &str,
    whatsapp_name: Option<&str>,
    phone: &str,
    deleted: bool,
) {
    sqlx::query(
        "INSERT INTO customers
           (customer_id, branch_id, name, whatsapp_name, phone, loyalty_points,
            origin_device_id, created_at, updated_at, version, sync_status, deleted_at)
         VALUES (?, ?, ?, ?, ?, 0, 'dev', datetime('now'), datetime('now'), 1, 'pending', ?)",
    )
    .bind(id)
    .bind(branch)
    .bind(name)
    .bind(whatsapp_name)
    .bind(phone)
    .bind(if deleted { Some("2026-08-01T00:00:00Z") } else { None })
    .execute(pool)
    .await
    .expect("seed customer");
}

async fn lookup(pool: &SqlitePool, branch: &str, search: &str) -> String {
    crate::ai::tools::execute_read_tool(
        pool,
        "list_customers",
        &serde_json::json!({ "search": search }),
        branch,
        3,
    )
    .await
    .expect("list_customers failed")
}

#[tokio::test]
async fn zanai_finds_a_customer_by_the_name_whatsapp_knows_them_by() {
    let pool = migrated_pool().await;
    let branch: String = sqlx::query_scalar("SELECT branch_id FROM branches LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    seed_customer_for_lookup(
        &pool, "cus_z", &branch, "Zanabal Nowshad", Some("Zanabal"), "+97333050666", false,
    )
    .await;

    // Either name resolves, which is what "message customer zanabal" needs.
    for query in ["Zanabal", "Nowshad"] {
        let out = lookup(&pool, &branch, query).await;
        assert!(out.contains("Zanabal Nowshad"), "search '{query}' returned: {out}");
    }
}

/// A number typed the way a cashier types it, against a number stored the way
/// it was dictated.
#[tokio::test]
async fn zanai_finds_a_customer_by_a_number_typed_without_separators() {
    let pool = migrated_pool().await;
    let branch: String = sqlx::query_scalar("SELECT branch_id FROM branches LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    seed_customer_for_lookup(
        &pool, "cus_p", &branch, "Fatima", None, "+973 3600 1122", false,
    )
    .await;

    let out = lookup(&pool, &branch, "36001122").await;
    assert!(out.contains("Fatima"), "{out}");
}

/// ZanAI must not read another branch's customers, and must not offer someone
/// who was deleted. Both were missing from this tool's query entirely.
#[tokio::test]
async fn zanai_customer_lookup_respects_branch_and_deletion() {
    let pool = migrated_pool().await;
    let branch: String = sqlx::query_scalar("SELECT branch_id FROM branches LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    seed_customer_for_lookup(&pool, "cus_ours", &branch, "Ours Shared", None, "+97333050001", false).await;
    seed_customer_for_lookup(&pool, "cus_theirs", "other-branch", "Theirs Shared", None, "+97333050002", false).await;
    seed_customer_for_lookup(&pool, "cus_gone", &branch, "Gone Shared", None, "+97333050003", true).await;

    let out = lookup(&pool, &branch, "Shared").await;
    assert!(out.contains("Ours Shared"), "{out}");
    assert!(!out.contains("Theirs Shared"), "another branch's customer leaked: {out}");
    assert!(!out.contains("Gone Shared"), "a deleted customer was offered: {out}");
}
