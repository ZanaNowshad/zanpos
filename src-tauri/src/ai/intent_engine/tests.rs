use super::*;

/// The advertised list and the routable list have to be the same list.
///
/// They drifted once: `adjust_prices_batch` and `bulk_price_adjust` sat in
/// `all_intents()` — schemas, descriptions and all — while `INTENT_NAMES`
/// never carried them, so `streaming.rs` could not route either one. Two
/// intents that looked complete from the outside and were unreachable from
/// the inside. A name in one and not the other is always a bug, in either
/// direction: advertised-but-unroutable strands the capability,
/// routable-but-unadvertised hides it.
#[test]
fn every_defined_intent_is_routable_and_vice_versa() {
    let defined: std::collections::BTreeSet<&str> =
        all_intents().into_iter().map(|i| i.name).collect();
    let routable: std::collections::BTreeSet<&str> = INTENT_NAMES.iter().copied().collect();

    let advertised_but_unroutable: Vec<_> = defined.difference(&routable).collect();
    assert!(
        advertised_but_unroutable.is_empty(),
        "defined in all_intents() but missing from INTENT_NAMES: {advertised_but_unroutable:?}"
    );

    let routable_but_unadvertised: Vec<_> = routable.difference(&defined).collect();
    assert!(
        routable_but_unadvertised.is_empty(),
        "listed in INTENT_NAMES but never defined: {routable_but_unadvertised:?}"
    );
}

/// Every routable name must reach a real arm of `execute_intent`, not the
/// catch-all. Checked by name so a renamed handler cannot slip through.
#[test]
fn mutation_intents_are_a_subset_of_routable_intents() {
    for name in INTENT_NAMES {
        if is_mutation_intent(name) {
            assert!(
                INTENT_NAMES.contains(name),
                "{name} is flagged as a mutation but is not routable"
            );
        }
    }
    // A mutation intent that nobody can call is a confirmation prompt that
    // never fires; one that is routable but unflagged writes without asking.
    // Every intent that writes must be flagged, and the list is derived rather
    // than typed out — it used to name `update_product`, `create_user` and
    // `backup_database`, which were removed for having no dispatch arm, and a
    // hardcoded list would have kept asserting a confirmation policy for
    // capabilities that no longer exist.
    for intent in all_intents() {
        let writes = intent.name.starts_with("create_")
            || intent.name.starts_with("update_")
            || intent.name.starts_with("receive_");
        if writes {
            assert!(
                is_mutation_intent(intent.name),
                "{} writes but is not flagged as a mutation, so it would never ask",
                intent.name
            );
        }
    }
}

// ── Executing every intent, not just listing them ────────────────────────────
//
// The test above compares two lists and they agree — which is why it never
// caught that `backup_database`, `create_user` and `update_product` were
// advertised with full schemas while `execute_intent` had no arm for any of
// them. A name check cannot see a missing match arm, and it cannot see
// `WHERE is_active=1` against a table that has no such column, which is what
// made `list_customers` fail with an internal error for a year.
//
// So this runs them. An intent may legitimately refuse bad input; what it may
// not do is fail to exist, query a column that is not there, or omit a NOT NULL
// column on insert.

async fn migrated_pool() -> SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    pool
}

/// A plausible value for each parameter an intent declares required.
///
/// The values need not match real rows — the point is to get past argument
/// parsing and execute the statement, and "no such column" fires whether or not
/// the id exists. A missing entry is a hard failure rather than a skip, so a new
/// required parameter forces a decision here instead of quietly dropping
/// coverage.
fn sample_argument(field: &str) -> Option<Value> {
    Some(match field {
        f if f.ends_with("_id") => json!("smoke-test-id"),
        "quantity_delta" => json!("1"),
        "from_date" | "to_date" => json!("2026-08-01"),
        "name" | "display_name" => json!("Smoke Test"),
        "username" => json!("smoketest"),
        "pin" => json!("0000"),
        "query" | "search" => json!("milk"),
        "tab" => json!("products"),
        _ => return None,
    })
}

/// Faults that mean the intent is broken, as opposed to merely refusing input.
fn is_structural_failure(message: &str) -> Option<&'static str> {
    let m = message.to_lowercase();
    if m.contains("unknown intent") {
        return Some("advertised to the model but execute_intent has no arm for it");
    }
    if m.contains("no such column") {
        return Some("queries a column that does not exist");
    }
    if m.contains("no such table") {
        return Some("queries a table that does not exist");
    }
    if m.contains("not null constraint") {
        return Some("insert omits a NOT NULL column");
    }
    if m.contains("no such function") || m.contains("syntax error") {
        return Some("malformed SQL");
    }
    None
}

#[tokio::test]
async fn every_advertised_intent_actually_executes() {
    let pool = migrated_pool().await;
    let branch: String = sqlx::query_scalar("SELECT branch_id FROM branches LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();

    let mut broken: Vec<String> = Vec::new();
    for intent in all_intents() {
        let mut params = serde_json::Map::new();
        if let Some(required) = intent.parameters.get("required").and_then(Value::as_array) {
            for field in required.iter().filter_map(Value::as_str) {
                match sample_argument(field) {
                    Some(value) => {
                        params.insert(field.to_string(), value);
                    }
                    None => panic!(
                        "intent '{}' requires '{field}', which sample_argument does not know how \
                         to supply — add it rather than letting this intent go unchecked",
                        intent.name
                    ),
                }
            }
        }

        let outcome = execute_intent(&pool, intent.name, &Value::Object(params), &branch).await;
        if let Err(error) = outcome {
            // `internal_database_detail`, not `to_string`: AppError::Database
            // renders as the bare phrase "A database error occurred", so every
            // SQLite message — the column that does not exist, the constraint
            // that failed — is masked. Classifying on the masked string is how
            // `list_customers` querying a non-existent `is_active` column looked
            // like an ordinary refusal.
            let message = error.internal_database_detail();
            if let Some(reason) = is_structural_failure(&message) {
                broken.push(format!("{}: {reason} — {message}", intent.name));
            }
        }
    }

    assert!(
        broken.is_empty(),
        "intents the model is offered but which cannot work:\n  {}",
        broken.join("\n  ")
    );
}

/// The takings the assistant reports are the takings the till actually took.
///
/// Two separate ways this number was wrong. It filtered on `sold_at`, a full
/// RFC3339 timestamp, against plain date bounds — so `'2026-09-01T09:00:00Z'`
/// compared greater than `'2026-09-01'` as text and the whole closing day fell
/// out of every range. And it counted voided sales, which every other report in
/// the app excludes, so the figure was simultaneously missing a day and
/// inflated by cancelled transactions.
#[tokio::test]
async fn the_sales_report_covers_the_closing_day_and_leaves_out_voids() {
    let pool = migrated_pool().await;
    let branch = "br_report";

    sqlx::query(
        "INSERT INTO branches (branch_id, branch_code, name, is_active, created_at, updated_at)
         VALUES (?, 'RPT', 'Report Branch', 1, datetime('now'), datetime('now'))",
    )
    .bind(branch)
    .execute(&pool)
    .await
    .unwrap();

    // Three sales: one on the opening day, two on the closing day, one of which
    // was voided. Only the first two should count.
    let rows = [
        (
            "s_open",
            "2026-08-31",
            "2026-08-31T09:00:00Z",
            1000,
            100,
            "completed",
        ),
        (
            "s_close",
            "2026-09-01",
            "2026-09-01T09:00:00Z",
            2000,
            200,
            "completed",
        ),
        (
            "s_void",
            "2026-09-01",
            "2026-09-01T10:00:00Z",
            5000,
            500,
            "voided",
        ),
    ];
    for (id, date, at, net, tax, status) in rows {
        sqlx::query(
            "INSERT INTO sales
               (sale_id, receipt_number, branch_id, device_id, shift_id, cashier_user_id,
                business_date, sold_at, net_total_minor, tax_total_minor, gross_total_minor,
                discount_total_minor, status, idempotency_key, created_at, updated_at)
             VALUES (?, ?, ?, 'dev', 'shift', 'user', ?, ?, ?, ?, ?, 0, ?, ?,
                     datetime('now'), datetime('now'))",
        )
        .bind(id)
        .bind(id)
        .bind(branch)
        .bind(date)
        .bind(at)
        .bind(net)
        .bind(tax)
        .bind(net)
        .bind(status)
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    }

    let params = json!({"from_date": "2026-08-31", "to_date": "2026-09-01"});
    let result = execute_intent(&pool, "get_sales_report", &params, branch)
        .await
        .expect("the sales report must run");

    assert_eq!(
        result.data["transactions"], 2,
        "the closing day belongs in the range and the voided sale does not"
    );
    assert_eq!(
        result.data["net_total"], "3.000",
        "1.000 on the 31st plus 2.000 on the 1st, with the 5.000 void left out"
    );
    assert_eq!(result.data["tax"], "0.300");
}

// ── list_customers, on the path that actually runs ───────────────────────────
//
// `streaming.rs` routes every read intent to the intent engine before it ever
// looks at `ai::tools`, so the well-covered `list_customers` in tools.rs is
// unreachable for this name. Its three smoke tests were passing against a copy
// nothing calls, while the copy ZanAI runs queried no branch and matched only
// name and phone. These run the reachable one.

async fn seed_customer(
    pool: &SqlitePool,
    id: &str,
    branch: &str,
    name: &str,
    whatsapp_name: Option<&str>,
    phone: &str,
    email: Option<&str>,
    deleted: bool,
) {
    sqlx::query(
        "INSERT INTO customers
           (customer_id, branch_id, name, whatsapp_name, phone, email, loyalty_points,
            origin_device_id, created_at, updated_at, version, sync_status, deleted_at)
         VALUES (?, ?, ?, ?, ?, ?, 0, 'dev', datetime('now'), datetime('now'), 1, 'pending', ?)",
    )
    .bind(id)
    .bind(branch)
    .bind(name)
    .bind(whatsapp_name)
    .bind(phone)
    .bind(email)
    .bind(if deleted { Some("2026-08-01T00:00:00Z") } else { None })
    .execute(pool)
    .await
    .expect("seed customer");
}

async fn listed_customer_ids(pool: &SqlitePool, search: &str, branch: &str) -> Vec<String> {
    let params = json!({ "search": search });
    let result = execute_intent(pool, "list_customers", &params, branch)
        .await
        .expect("list_customers failed");
    result.data["customers"]
        .as_array()
        .expect("customers array")
        .iter()
        .map(|c| c["id"].as_str().unwrap_or_default().to_string())
        .collect()
}

#[tokio::test]
async fn list_customers_intent_refuses_another_branch_and_the_deleted() {
    let pool = migrated_pool().await;
    let branch: String = sqlx::query_scalar("SELECT branch_id FROM branches LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();

    seed_customer(&pool, "cus_ours", &branch, "Shared Name", None, "+97333050001", None, false).await;
    seed_customer(&pool, "cus_theirs", "other-branch", "Shared Name", None, "+97333050002", None, false).await;
    seed_customer(&pool, "cus_gone", &branch, "Shared Name", None, "+97333050003", None, true).await;

    let found = listed_customer_ids(&pool, "Shared", &branch).await;

    assert!(found.contains(&"cus_ours".to_string()));
    assert!(
        !found.contains(&"cus_theirs".to_string()),
        "ZanAI read another branch's customer"
    );
    assert!(
        !found.contains(&"cus_gone".to_string()),
        "ZanAI offered a deleted customer"
    );
}

#[tokio::test]
async fn list_customers_intent_finds_the_fields_a_cashier_actually_types() {
    let pool = migrated_pool().await;
    let branch: String = sqlx::query_scalar("SELECT branch_id FROM branches LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();

    seed_customer(
        &pool,
        "cus_wa",
        &branch,
        "Fatima Al Sayed",
        Some("Umm Yousef"),
        "+973 3305 0004",
        Some("fatima@example.test"),
        false,
    )
    .await;

    // The name WhatsApp knows them by — often the only one the cashier has seen.
    assert_eq!(listed_customer_ids(&pool, "Umm Yousef", &branch).await, vec!["cus_wa"]);
    // Email was not searched at all before.
    assert_eq!(listed_customer_ids(&pool, "fatima@example.test", &branch).await, vec!["cus_wa"]);
    // The stored number carries spaces; nobody types it that way.
    assert_eq!(listed_customer_ids(&pool, "33050004", &branch).await, vec!["cus_wa"]);
    // And the ordinary case still works.
    assert_eq!(listed_customer_ids(&pool, "Fatima", &branch).await, vec!["cus_wa"]);
}
