//! Invoice / price-list photo → catalog update (review-first).
//!
//! The owner forwards a supplier invoice or price list over WhatsApp. The POS runs
//! the Phase-1 local OCR over the image, the AI structures the extracted TEXT into
//! line items, each line is matched to the catalogue (barcode then name), and a
//! proposal is returned for the owner to review. NOTHING is written during extract.
//!
//! On approval, changes are applied by dispatching the SAME mutations the AI tools
//! use — `update_product_price`, `create_product`, `receive_stock` via
//! `ai::tools::execute_mutation` — so all invariants (price history, barcodes,
//! stock movements, audit, sync) are preserved. Cost and supplier link are
//! single-table updates. New products are created as inactive drafts under a
//! default "Uncategorized" category for the owner to complete before selling.

use crate::commands::sync_commands;
use crate::commands::whatsapp_commands::{read_sidecar_token, SIDECAR_URL};
use crate::errors::{AppError, AppResult};
use crate::AppState;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::SqlitePool;
use std::time::Duration;
use tauri::State;
use ulid::Ulid;

// ── Types ─────────────────────────────────────────────────────────────────────

/// One reviewable line returned by extraction.
#[derive(Debug, Serialize)]
pub struct ProposedLine {
    pub line_no: usize,
    pub name: String,
    pub barcode: Option<String>,
    pub quantity: Option<f64>,
    pub matched_product_id: Option<String>,
    pub matched_name: Option<String>,
    pub current_price_minor: Option<i64>,
    pub new_price_minor: Option<i64>,
    pub current_cost_minor: Option<i64>,
    pub new_cost_minor: Option<i64>,
    pub action: String, // "update" | "create"
}

#[derive(Debug, Serialize)]
pub struct CatalogImportProposal {
    pub supplier_name: Option<String>,
    pub supplier_id: Option<String>,
    pub currency_exponent: i64,
    pub lines: Vec<ProposedLine>,
    pub ocr_chars: usize,
}

/// One approved (possibly owner-edited) line sent back to apply.
#[derive(Debug, Deserialize)]
pub struct ApplyLine {
    pub action: String, // "update" | "create"
    pub product_id: Option<String>,
    pub name: String,
    pub barcode: Option<String>,
    pub new_price_minor: Option<i64>,
    pub new_cost_minor: Option<i64>,
    /// Stock to receive, in whole units as a string (matches receive_stock).
    pub receive_qty: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ApplyInput {
    pub currency_exponent: u32,
    pub supplier_name: Option<String>,
    pub supplier_id: Option<String>,
    pub lines: Vec<ApplyLine>,
}

#[derive(Debug, Serialize, Default)]
pub struct ApplyResult {
    pub updated: usize,
    pub created: usize,
    pub received: usize,
    pub supplier_id: Option<String>,
    pub errors: Vec<String>,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(60))
        .build()
        .unwrap_or_default()
}

/// Decimal (e.g. 1.5) → minor units for the given exponent (1.5, exp 3 → 1500).
fn to_minor(value: f64, exp: u32) -> i64 {
    (value * 10f64.powi(exp as i32)).round() as i64
}

#[derive(serde::Deserialize)]
struct OcrResp {
    ok: bool,
    text: Option<String>,
    error: Option<String>,
}

/// AI-structured line item (amounts as decimals in major units).
#[derive(Debug, Deserialize)]
struct ExtractedLine {
    #[serde(default)]
    name: String,
    #[serde(default)]
    barcode: Option<String>,
    #[serde(default)]
    cost: Option<f64>,
    #[serde(default)]
    price: Option<f64>,
    #[serde(default)]
    quantity: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct Extraction {
    #[serde(default)]
    supplier_name: Option<String>,
    #[serde(default)]
    lines: Vec<ExtractedLine>,
}

fn extract_json(text: &str) -> Option<String> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    (end > start).then(|| text[start..=end].to_string())
}

// ── Extract (read-only) ───────────────────────────────────────────────────────

#[tauri::command]
pub async fn catalog_import_extract(
    media_id: String,
    currency_exponent: u32,
    session_token: String,
    state: State<'_, AppState>,
) -> AppResult<CatalogImportProposal> {
    state
        .sessions
        .resolve_office(&state.db, &session_token)
        .await?;
    if media_id.is_empty() || media_id.chars().count() > 128 {
        return Err(AppError::Validation(
            "media_id must contain 1..=128 characters".into(),
        ));
    }
    if currency_exponent > 6 {
        return Err(AppError::Validation(
            "currency_exponent must be between 0 and 6".into(),
        ));
    }
    let db = &state.db;

    // 1) Deterministic local OCR over the forwarded image (reuses Phase 1 /ocr).
    let token = read_sidecar_token(&state);
    let ocr: OcrResp = client()
        .get(format!("{}/ocr", SIDECAR_URL))
        .query(&[("id", media_id.as_str())])
        .header("X-Sidecar-Token", &token)
        .send()
        .await
        .map_err(|e| AppError::Internal(format!("OCR request failed: {e}")))?
        .json()
        .await
        .map_err(|e| AppError::Internal(format!("OCR bad response: {e}")))?;
    if !ocr.ok {
        return Err(AppError::Internal(
            ocr.error.unwrap_or_else(|| "OCR unavailable".into()),
        ));
    }
    let ocr_text = ocr.text.unwrap_or_default();
    if ocr_text.trim().is_empty() {
        return Err(AppError::Validation(
            "No readable text found in the image. Try a clearer photo.".into(),
        ));
    }

    // 2) AI structures the OCR text into line items (no tools, one shot).
    let provider = crate::ai::provider::Provider::from_db_with_fallback(db)
        .await?
        .ok_or_else(|| AppError::Internal("No AI provider configured.".into()))?;
    let params = crate::ai::config::load_ai_params(db).await;
    let system = "You convert the OCR text of a supplier invoice or price list into structured \
        JSON. Respond with ONLY a JSON object and nothing else: \
        {\"supplier_name\":string,\"lines\":[{\"name\":string,\"barcode\":string,\"cost\":number,\"price\":number,\"quantity\":number}]}. \
        'cost' is the supplier/buy price and 'price' the sell/retail price in major currency units \
        (e.g. 1.500). Omit a field if not present; never invent values. Ignore headers, totals, tax \
        lines and page furniture — only real product lines.";
    let user = format!("OCR text:\n\"\"\"\n{ocr_text}\n\"\"\"");
    let resp = provider
        .send_chat(system, &[], &user, &[], params.context_window_chars, None)
        .await?;
    let parsed: Extraction = extract_json(&resp.text)
        .and_then(|j| serde_json::from_str(&j).ok())
        .ok_or_else(|| AppError::Internal("AI could not structure the document.".into()))?;

    // 3) Match each line to the catalogue (barcode first, then name).
    let mut lines = Vec::new();
    for (i, l) in parsed.lines.iter().enumerate() {
        if l.name.trim().is_empty() && l.barcode.is_none() {
            continue;
        }
        let matched = match &l.barcode {
            Some(bc) if !bc.is_empty() => {
                crate::db::repositories::product_repo::get_product_by_barcode(db, bc).await?
            }
            _ => None,
        };
        let matched = match matched {
            Some(p) => Some(p),
            None if !l.name.trim().is_empty() => {
                crate::db::repositories::product_repo::search_products(db, l.name.trim(), 1)
                    .await?
                    .into_iter()
                    .next()
            }
            None => None,
        };

        let new_price_minor = l.price.map(|v| to_minor(v, currency_exponent));
        let new_cost_minor = l.cost.map(|v| to_minor(v, currency_exponent));

        let (matched_product_id, matched_name, current_price_minor, current_cost_minor, action) =
            match matched {
                Some(p) => (
                    Some(p.product.product_id),
                    Some(p.product.name),
                    Some(p.price_minor),
                    p.product.cost_minor,
                    "update".to_string(),
                ),
                None => (None, None, None, None, "create".to_string()),
            };

        lines.push(ProposedLine {
            line_no: i + 1,
            name: l.name.trim().to_string(),
            barcode: l.barcode.clone().filter(|b| !b.is_empty()),
            quantity: l.quantity,
            matched_product_id,
            matched_name,
            current_price_minor,
            new_price_minor,
            current_cost_minor,
            new_cost_minor,
            action,
        });
    }

    let supplier_id = match &parsed.supplier_name {
        Some(name) if !name.trim().is_empty() => find_supplier_by_name(db, name.trim()).await,
        _ => None,
    };

    Ok(CatalogImportProposal {
        supplier_name: parsed.supplier_name.filter(|s| !s.trim().is_empty()),
        supplier_id,
        currency_exponent: currency_exponent as i64,
        lines,
        ocr_chars: ocr_text.len(),
    })
}

async fn find_supplier_by_name(db: &SqlitePool, name: &str) -> Option<String> {
    sqlx::query_scalar("SELECT supplier_id FROM suppliers WHERE name = ? COLLATE NOCASE LIMIT 1")
        .bind(name)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
}

/// Resolve (or create) the default "Uncategorized" category for draft products.
async fn default_category_id(
    db: &SqlitePool,
    context: &crate::ai::tool_policy::MutationExecutionContext,
) -> AppResult<String> {
    if let Some(id) = sqlx::query_scalar::<_, String>(
        "SELECT category_id FROM categories WHERE name = 'Uncategorized' COLLATE NOCASE LIMIT 1",
    )
    .fetch_optional(db)
    .await?
    {
        return Ok(id);
    }
    let result = crate::ai::tool_policy::execute_confirmed_mutation(
        db,
        context,
        "create_category",
        &json!({"name":"Uncategorized"}),
        3,
    )
    .await?;
    Ok(result.entity_id)
}

// ── Apply (writes; review-confirmed) ──────────────────────────────────────────

#[tauri::command]
pub async fn catalog_import_apply(
    input: ApplyInput,
    session_token: String,
    state: State<'_, AppState>,
) -> AppResult<ApplyResult> {
    let actor = state
        .sessions
        .resolve_office(&state.db, &session_token)
        .await?;
    validate_apply_input(&input)?;
    let db = &state.db;
    let context = crate::ai::tool_policy::MutationExecutionContext {
        actor_user_id: actor.user_id.clone(),
        branch_id: actor.branch_id,
    };
    let exp = input.currency_exponent;
    let mut result = ApplyResult::default();

    // Resolve/create the supplier once for the whole batch.
    let supplier_id =
        resolve_supplier(db, &context, &input.supplier_id, &input.supplier_name).await?;
    result.supplier_id = supplier_id.clone();

    for line in &input.lines {
        let outcome = apply_one(db, &context, line, exp, supplier_id.as_deref()).await;
        match outcome {
            Ok(ApplyOutcome {
                updated,
                created,
                received,
            }) => {
                result.updated += updated;
                result.created += created;
                result.received += received;
            }
            Err(e) => result.errors.push(format!("{}: {}", line.name, e)),
        }
    }
    if result.updated > 0 || result.created > 0 || result.received > 0 {
        sync_commands::schedule_immediate_sync(&state);
    }
    Ok(result)
}

struct ApplyOutcome {
    updated: usize,
    created: usize,
    received: usize,
}

async fn apply_one(
    db: &SqlitePool,
    context: &crate::ai::tool_policy::MutationExecutionContext,
    line: &ApplyLine,
    exp: u32,
    supplier_id: Option<&str>,
) -> AppResult<ApplyOutcome> {
    let mut out = ApplyOutcome {
        updated: 0,
        created: 0,
        received: 0,
    };

    let product_id = if line.action == "create" {
        // New product as an inactive draft under the default category.
        let category_id = default_category_id(db, context).await?;
        // create_product requires a positive sell price. Invoices often carry only a
        // cost, so fall back to cost as a placeholder price (owner adjusts when they
        // complete the draft). Cost itself is set by the single-table update below.
        let price = line.new_price_minor.or(line.new_cost_minor).unwrap_or(0);
        if price <= 0 {
            return Err(AppError::Validation(
                "New product needs a price or cost".into(),
            ));
        }
        let create = json!({
            "name": line.name,
            "category_id": category_id,
            "price_minor": price,
            "barcode": line.barcode,
        });
        let res = crate::ai::tool_policy::execute_confirmed_mutation(
            db,
            context,
            "create_product",
            &create,
            exp,
        )
        .await?;
        out.created += 1;
        let pid = res.entity_id; // MutationResult.entity_id is the new product_id
                                 // Draft until the owner sets category/tax and activates it.
        crate::ai::tool_policy::execute_confirmed_mutation(
            db,
            context,
            "update_product_full",
            &json!({"product_id":pid,"is_active":false}),
            exp,
        )
        .await?;
        pid
    } else {
        let pid = line
            .product_id
            .clone()
            .ok_or_else(|| AppError::Validation("Update line missing product_id".into()))?;
        // Sell price via the same path the AI uses (keeps price history).
        if let Some(p) = line.new_price_minor {
            let inp = json!({ "product_id": pid, "new_price_minor": p });
            crate::ai::tool_policy::execute_confirmed_mutation(
                db,
                context,
                "update_product_price",
                &inp,
                exp,
            )
            .await?;
            out.updated += 1;
        }
        pid
    };

    // Cost + supplier link use the same confirmed product mutation policy.
    if line.new_cost_minor.is_some() || supplier_id.is_some() {
        let now = chrono::Utc::now().to_rfc3339();
        let previous_cost: Option<i64> = if line.new_cost_minor.is_some() {
            sqlx::query_scalar("SELECT cost_minor FROM products WHERE product_id = ?")
                .bind(&product_id)
                .fetch_optional(db)
                .await?
                .flatten()
        } else {
            None
        };
        let mut update = json!({"product_id":product_id});
        if let Some(c) = line.new_cost_minor {
            update["cost_minor"] = json!(c);
        }
        if let Some(s) = supplier_id {
            update["default_supplier_id"] = json!(s);
        }
        crate::ai::tool_policy::execute_confirmed_mutation(
            db,
            context,
            "update_product_full",
            &update,
            exp,
        )
        .await?;
        if let Some(new_cost) = line.new_cost_minor {
            sqlx::query(
                "INSERT INTO product_cost_history
                 (cost_history_id, product_id, old_cost_minor, new_cost_minor, supplier_id, source, actor_user_id, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, 'catalog_import', ?, ?, ?)",
            )
            .bind(Ulid::new().to_string())
            .bind(&product_id)
            .bind(previous_cost)
            .bind(new_cost)
            .bind(supplier_id)
            .bind(&context.actor_user_id)
            .bind(&now)
            .bind(&now)
            .execute(db)
            .await?;
        }
        if line.action != "create" {
            out.updated = out.updated.max(1);
        }
    }

    // Receive stock for the line quantity, if any (reuses receive_stock invariants).
    if let Some(qty) = line
        .receive_qty
        .as_deref()
        .filter(|q| !q.trim().is_empty() && *q != "0")
    {
        let parsed_qty = qty
            .trim()
            .parse::<f64>()
            .map_err(|_| AppError::Validation("Receive quantity must be a number".into()))?;
        if parsed_qty <= 0.0 {
            return Err(AppError::Validation(
                "Receive quantity must be positive".into(),
            ));
        }
        let inp = json!({ "product_id": product_id, "quantity": qty, "notes": format!("Invoice receive by {}", context.actor_user_id) });
        crate::ai::tool_policy::execute_confirmed_mutation(db, context, "receive_stock", &inp, exp)
            .await?;
        out.received += 1;
    }

    Ok(out)
}

async fn resolve_supplier(
    db: &SqlitePool,
    context: &crate::ai::tool_policy::MutationExecutionContext,
    supplier_id: &Option<String>,
    supplier_name: &Option<String>,
) -> AppResult<Option<String>> {
    if let Some(id) = supplier_id.as_ref().filter(|s| !s.is_empty()) {
        return Ok(Some(id.clone()));
    }
    let name = supplier_name
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty());
    let Some(name) = name else { return Ok(None) };
    if let Some(id) = find_supplier_by_name(db, name).await {
        return Ok(Some(id));
    }
    let result = crate::ai::tool_policy::execute_confirmed_mutation(
        db,
        context,
        "create_supplier",
        &json!({"name":name}),
        3,
    )
    .await?;
    Ok(Some(result.entity_id))
}

fn validate_apply_input(input: &ApplyInput) -> AppResult<()> {
    if input.currency_exponent > 6 {
        return Err(AppError::Validation(
            "currency_exponent must be between 0 and 6".into(),
        ));
    }
    if input.lines.is_empty() || input.lines.len() > 200 {
        return Err(AppError::Validation(
            "Catalog import must contain 1..=200 lines".into(),
        ));
    }
    for line in &input.lines {
        if !matches!(line.action.as_str(), "create" | "update") {
            return Err(AppError::Validation("Invalid catalog line action".into()));
        }
        if line.name.trim().is_empty() || line.name.chars().count() > 200 {
            return Err(AppError::Validation(
                "Catalog line name must contain 1..=200 characters".into(),
            ));
        }
        if line.new_price_minor.is_some_and(|value| value <= 0)
            || line.new_cost_minor.is_some_and(|value| value < 0)
        {
            return Err(AppError::Validation(
                "Catalog prices must be positive and costs non-negative".into(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO branches
             (branch_id, branch_code, name, currency, timezone, is_active, created_at, updated_at)
             VALUES ('B1','TEST-BILL','Main','BHD','Asia/Bahrain',1,?,?)",
        )
        .bind(&now)
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO devices
             (device_id, branch_id, device_code, name, status, is_active, created_at, updated_at)
             VALUES ('D1','B1','TILL1','Till 1','online',1,?,?)",
        )
        .bind(&now)
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO categories
             (category_id, name, is_active, created_at, updated_at)
             VALUES ('C1','Goods',1,?,?)",
        )
        .bind(&now)
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO products
             (product_id, category_id, name, track_inventory, allow_decimal_quantity, is_active, cost_minor, currency, created_at, updated_at)
             VALUES ('P1','C1','Test Item',1,0,1,1000,'BHD',?,?)",
        )
        .bind(&now)
        .bind(&now)
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    fn context() -> crate::ai::tool_policy::MutationExecutionContext {
        crate::ai::tool_policy::MutationExecutionContext {
            actor_user_id: "U1".into(),
            branch_id: "B1".into(),
        }
    }

    #[tokio::test]
    async fn apply_line_receive_qty_updates_stock_and_movement() {
        let pool = setup_pool().await;
        let line = ApplyLine {
            action: "update".into(),
            product_id: Some("P1".into()),
            name: "Test Item".into(),
            barcode: None,
            new_price_minor: None,
            new_cost_minor: None,
            receive_qty: Some("5".into()),
        };

        let outcome = apply_one(&pool, &context(), &line, 3, None).await.unwrap();

        assert_eq!(outcome.received, 1);
        let qty: String =
            sqlx::query_scalar("SELECT quantity_on_hand FROM stock_levels WHERE product_id='P1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(qty, "5");
        let delta: String =
            sqlx::query_scalar("SELECT quantity_delta FROM stock_movements WHERE product_id='P1'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(delta, "5");
    }

    #[tokio::test]
    async fn apply_line_cost_update_records_cost_history() {
        let pool = setup_pool().await;
        let supplier_id = resolve_supplier(&pool, &context(), &None, &Some("Acme Supplies".into()))
            .await
            .unwrap()
            .unwrap();
        let line = ApplyLine {
            action: "update".into(),
            product_id: Some("P1".into()),
            name: "Test Item".into(),
            barcode: None,
            new_price_minor: None,
            new_cost_minor: Some(1500),
            receive_qty: None,
        };

        apply_one(&pool, &context(), &line, 3, Some(supplier_id.as_str()))
            .await
            .unwrap();

        let row: (i64, i64, String, String) = sqlx::query_as(
            "SELECT old_cost_minor, new_cost_minor, supplier_id, source
             FROM product_cost_history WHERE product_id='P1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row, (1000, 1500, supplier_id, "catalog_import".into()));
    }

    #[tokio::test]
    async fn apply_line_rejects_negative_receive_qty() {
        let pool = setup_pool().await;
        let line = ApplyLine {
            action: "update".into(),
            product_id: Some("P1".into()),
            name: "Test Item".into(),
            barcode: None,
            new_price_minor: None,
            new_cost_minor: None,
            receive_qty: Some("-3".into()),
        };

        let err = match apply_one(&pool, &context(), &line, 3, None).await {
            Ok(_) => panic!("negative receive quantity was accepted"),
            Err(e) => e,
        };

        assert!(err
            .to_string()
            .contains("Receive quantity must be positive"));
    }

    #[test]
    fn cost_history_participates_in_sync() {
        assert!(crate::commands::sync_commands::SYNC_TABLES.contains(&"product_cost_history"));
        assert!(crate::sync_v2::apply::SYNC_TABLES.contains(&"product_cost_history"));
        assert_eq!(
            crate::sync_v2::apply::pk_for_table("product_cost_history"),
            "cost_history_id"
        );
    }
}
