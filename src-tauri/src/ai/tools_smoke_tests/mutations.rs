//! Mutations, previewed and executed against the real schema.
//!
//! Split from the read sweep for the 500-line rule; the shared helpers stay
//! next door and are imported here.

use super::{goes_outside, migrated_pool};
use crate::ai::tool_registry::{ToolKind, ToolRegistry};
use crate::errors::AppError;

// ── The other 102 ────────────────────────────────────────────────────────────
//
// The sweep above covers read tools. Mutations were never executed by any test,
// so the whole class of fault it exists to catch — a column that is not there, a
// NOT NULL that is not bound, a match arm nobody wrote — was invisible on the
// write side. That is exactly how six of eighteen intents stayed broken, and
// mutations are the half that changes a shop's data.
//
// Both halves of a mutation are exercised. `dry_run_mutation` is what the
// operator sees before confirming, and it reads current state; the executor is
// what writes. Either can carry the bug independently.
//
// Everything runs against a throwaway in-memory database that is discarded at
// the end of the test, so "execute every mutation" writes nothing that outlives
// the process.

/// A plausible value for a mutation's required parameters.
///
/// Deliberately separate from the read table: mutations want quantities,
/// prices and reasons, and a value that parses is what gets the statement to
/// the database, which is where the faults are.
fn sample_mutation_argument(field: &str) -> Option<serde_json::Value> {
    use serde_json::json;
    Some(match field {
        f if f.ends_with("_ids") => json!(["smoke-test-id"]),
        // Batch payloads: one plausible line, carrying the keys every bulk
        // tool's schema names. A wrong key here reads as an empty batch, which
        // would let the tool pass without touching the database.
        "items" | "lines" | "products" | "updates" | "adjustments" | "corrections"
        | "entries" | "rows" | "changes" => json!([{
            "product_id": "smoke-test-id",
            "barcode": "6291001234567",
            "quantity": "1",
            "counted_quantity": "1",
            "price_minor": 100,
            "cost_minor": 100,
            "reorder_point": 1,
            "name": "Smoke Test",
        }]),
        f if f.ends_with("_id") || f == "id" => json!("smoke-test-id"),
        "barcode" | "new_barcode" => json!("6291001234567"),
        f if f.ends_with("_name") || f == "name" => json!("Smoke Test"),
        "username" => json!("smoketest"),
        "pin" | "new_pin" => json!("0000"),
        "quantity" | "quantity_delta" | "new_quantity" | "counted_quantity" => json!("1"),
        "price_minor" | "new_price_minor" | "cost_minor" | "amount_minor"
        | "selling_price_minor" | "points" => json!(100),
        f if f.ends_with("reorder_point") || f.ends_with("_count") || f.ends_with("_days") => json!(1),
        "percent" | "adjustment_percent" => json!(10),
        f if f.ends_with("_basis_points") => json!(1000),
        f if f.ends_with("_minor") => json!(100),
        "reason" | "note" | "notes" | "reason_code" => json!("smoke test"),
        "status" | "new_status" => json!("pending"),
        "date" | "from" | "to" | "from_date" | "to_date" | "business_date"
        | "effective_from" | "effective_to" | "starts_at" | "ends_at" => json!("2026-08-01"),
        "is_active" | "active" | "tracked" => json!(true),
        "method" | "payment_method" => json!("cash"),
        "phone" => json!("+97333050666"),
        "email" => json!("smoke@example.test"),
        "url" => json!("https://example.test/"),
        "receipt_number" => json!("R-1"),
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
        _ => return None,
    })
}

/// Faults that mean the tool is broken rather than merely refusing input.
fn structural_fault(error: &AppError) -> Option<String> {
    // `internal_database_detail`, not `to_string`: AppError::Database renders as
    // the bare phrase "A database error occurred", so classifying on the plain
    // string hides every SQLite message. That masking is what let a query
    // against a non-existent column read as an ordinary refusal.
    let detail = error.internal_database_detail().to_lowercase();
    for (needle, reason) in [
        ("no such column", "queries a column that does not exist"),
        ("no such table", "queries a table that does not exist"),
        ("not null constraint", "omits a NOT NULL column"),
        ("no such function", "malformed SQL"),
        ("syntax error", "malformed SQL"),
        ("unknown or unavailable ai tool", "has no executor arm"),
        ("unknown tool", "has no executor arm"),
        ("unsupported mutation", "has no executor arm"),
    ] {
        if detail.contains(needle) {
            return Some(format!("{reason} — {detail}"));
        }
    }
    None
}

fn mutation_tools_with_supplied_arguments() -> Vec<(String, serde_json::Value)> {
    let registry = ToolRegistry::global().unwrap();
    let mut out = Vec::new();
    for definition in crate::ai::tools_catalogue::all_tool_definitions() {
        let is_mutation = registry
            .get(&definition.name)
            .is_some_and(|d| d.kind == ToolKind::Mutation);
        if !is_mutation || goes_outside(&definition.name) {
            continue;
        }
        let mut args = serde_json::Map::new();
        let required = definition
            .input_schema
            .get("required")
            .and_then(|r| r.as_array())
            .cloned()
            .unwrap_or_default();
        for field in required.iter().filter_map(|f| f.as_str()) {
            match sample_mutation_argument(field) {
                Some(value) => {
                    args.insert(field.to_string(), value);
                }
                None => panic!(
                    "mutation '{}' requires '{field}', which sample_mutation_argument does not \
                     know how to supply — add it rather than letting this tool go unchecked",
                    definition.name
                ),
            }
        }
        out.push((definition.name, serde_json::Value::Object(args)));
    }
    out
}

#[tokio::test]
async fn every_mutation_previews_against_the_real_schema() {
    let cases = mutation_tools_with_supplied_arguments();
    assert!(
        cases.len() > 60,
        "only {} mutations swept — the filter is wrong",
        cases.len()
    );

    // One database for the sweep, not one per tool. The faults being looked for
    // are schema-level — a column that is not there, a NOT NULL that is not
    // bound, a missing match arm — and none of them depends on what a previous
    // tool wrote. Re-running the migrations 200 times to isolate state that
    // cannot affect the result took the suite from 82 seconds to over ten
    // minutes, which is a cost paid on every ship gate.
    let pool = migrated_pool().await;
    let mut broken = Vec::new();
    for (name, args) in &cases {
        if let Err(error) = crate::ai::tools::dry_run_mutation(&pool, name, args, 3).await {
            if let Some(reason) = structural_fault(&error) {
                broken.push(format!("{name}: {reason}"));
            }
        }
    }

    assert!(
        broken.is_empty(),
        "{} mutation preview(s) cannot run against the migrated schema:\n  {}",
        broken.len(),
        broken.join("\n  ")
    );
}

/// The write half.
///
/// A preview reads; the executor writes, and the faults live in different
/// statements. `create_customer` in the intent engine previewed fine and failed
/// on insert with a NOT NULL it never bound — the same shape can hide in any of
/// these.
///
/// Runs against one in-memory database that is discarded when the test ends, so
/// executing every mutation writes nothing that outlives the process and touches
/// no file.
#[tokio::test]
async fn every_mutation_executes_against_the_real_schema() {
    let cases = mutation_tools_with_supplied_arguments();
    assert!(
        cases.len() > 60,
        "only {} mutations swept — the filter is wrong",
        cases.len()
    );

    let pool = migrated_pool().await;
    let mut broken = Vec::new();
    for (name, args) in &cases {
        if let Err(error) = crate::ai::tools::execute_mutation_raw(&pool, name, args, 3).await {
            if let Some(reason) = structural_fault(&error) {
                broken.push(format!("{name}: {reason}"));
            }
        }
    }

    assert!(
        broken.is_empty(),
        "{} mutation(s) cannot execute against the migrated schema:\n  {}",
        broken.len(),
        broken.join("\n  ")
    );
}

/// The classifier the two sweeps above depend on.
///
/// Both of them passed on the first run, and a sweep that finds nothing looks
/// identical to a sweep that cannot find anything. This proves it fires on a
/// real fault — and specifically that it sees through `AppError::Database`,
/// whose `Display` is the bare phrase "A database error occurred". Reading that
/// masked string instead of the detail is what let a query against a
/// non-existent column pass for an ordinary refusal.
#[tokio::test]
async fn the_sweep_can_actually_detect_a_broken_query() {
    let pool = migrated_pool().await;

    let real_fault: AppError = match sqlx::query("SELECT no_such_column FROM customers")
        .fetch_all(&pool)
        .await
    {
        Ok(_) => panic!("the column exists — pick one that does not"),
        Err(e) => e.into(),
    };

    // The masked string carries nothing, which is the whole trap.
    assert_eq!(real_fault.to_string(), "A database error occurred");
    let reason = structural_fault(&real_fault).expect("classifier missed a missing column");
    assert!(reason.contains("column that does not exist"), "{reason}");

    // And an ordinary refusal is not reported as a structural fault, or every
    // sweep would fail on tools that correctly reject smoke-test arguments.
    let refusal = AppError::Validation("Product not found".into());
    assert!(structural_fault(&refusal).is_none());
}
