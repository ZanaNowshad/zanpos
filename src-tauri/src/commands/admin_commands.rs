use crate::commands::rbac;
use crate::db::repositories::{audit_hash, auth_repo};
use crate::errors::{AppError, AppResult};
use crate::AppState;
use serde::{Deserialize, Serialize};
use sqlx::Row;
/// Back-office administration commands.
/// Products, categories, tax rules, users, and roles.
use tauri::State;
use ulid::Ulid;

// ─── Product barcode row ──────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ProductBarcodeRow {
    pub barcode_id: String,
    pub product_id: String,
    pub barcode: String,
    pub created_at: String,
}

/// Resolve the active branch_id from the database at runtime.
async fn active_branch_id(state: &AppState) -> AppResult<String> {
    let row = sqlx::query(
        "SELECT branch_id FROM branches WHERE is_active = 1 ORDER BY created_at LIMIT 1",
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::NotFound("No active branch configured".into()))?;
    Ok(row.get("branch_id"))
}

/// Resolve the active device_id from the database at runtime.
async fn active_device_id(state: &AppState) -> String {
    sqlx::query_scalar("SELECT device_id FROM devices WHERE is_active=1 ORDER BY device_code LIMIT 1")
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "unknown".to_string())
}

// ─── Response types ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct AdminProduct {
    pub product_id: String,
    pub category_id: String,
    pub category_name: String,
    pub name: String,
    pub sku: Option<String>,
    pub barcode: Option<String>,
    pub track_inventory: bool,
    pub allow_decimal_quantity: bool,
    pub is_active: bool,
    pub tax_rule_id: Option<String>,
    pub tax_rule_name: Option<String>,
    pub price_minor: i64,
    pub reorder_point: i64,
    pub image_path: Option<String>,
    pub cost_minor: Option<i64>,
    pub description: Option<String>,
    pub default_supplier_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CategoryRow {
    pub category_id: String,
    pub name: String,
    pub sort_order: i64,
    pub is_active: bool,
    pub parent_category_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TaxRuleRow {
    pub tax_rule_id: String,
    pub name: String,
    pub rate_basis_points: i64,
    pub inclusive: bool,
    pub is_active: bool,
}

#[derive(Debug, Serialize)]
pub struct AdminUserRow {
    pub user_id: String,
    pub display_name: String,
    pub username: String,
    pub role_id: String,
    pub role_name: String,
    pub is_active: bool,
    pub last_login_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RoleRow {
    pub role_id: String,
    pub name: String,
}

// ─── Shared price join ────────────────────────────────────────────────────────

const ADMIN_PRODUCT_QUERY: &str = r#"
    SELECT p.product_id, p.category_id, c.name AS category_name,
           p.name, p.sku, p.barcode, p.track_inventory, p.allow_decimal_quantity,
           p.is_active, p.tax_rule_id, p.reorder_point, p.image_path,
           p.cost_minor, p.description, p.default_supplier_id,
           t.name AS tax_rule_name,
           COALESCE(pp.price_minor, 0) AS price_minor
    FROM products p
    JOIN categories c ON c.category_id = p.category_id
    LEFT JOIN tax_rules t ON t.tax_rule_id = p.tax_rule_id AND t.is_active = 1
    LEFT JOIN product_prices pp ON pp.product_id = p.product_id
        AND pp.branch_id IS NULL
        AND pp.price_type = 'selling'
        AND datetime(pp.effective_from) <= datetime('now')
        AND (pp.effective_to IS NULL OR datetime(pp.effective_to) > datetime('now'))
"#;

fn row_to_admin_product(r: &sqlx::sqlite::SqliteRow) -> AdminProduct {
    let track: i64 = r.get("track_inventory");
    let decimal: i64 = r.get("allow_decimal_quantity");
    let active: i64 = r.get("is_active");
    AdminProduct {
        product_id: r.get("product_id"),
        category_id: r.get("category_id"),
        category_name: r.get("category_name"),
        name: r.get("name"),
        sku: r.get("sku"),
        barcode: r.get("barcode"),
        track_inventory: track != 0,
        allow_decimal_quantity: decimal != 0,
        is_active: active != 0,
        tax_rule_id: r.get("tax_rule_id"),
        tax_rule_name: r.get("tax_rule_name"),
        price_minor: r.get("price_minor"),
        reorder_point: r.get("reorder_point"),
        image_path: r.get("image_path"),
        cost_minor: r.get("cost_minor"),
        description: r.get("description"),
        default_supplier_id: r.get("default_supplier_id"),
    }
}

// ─── Product commands ─────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct AdminProductPage {
    pub items: Vec<AdminProduct>,
    pub total: i64,
    pub offset: i64,
    pub limit: i64,
}

#[tauri::command]
pub async fn admin_list_products(
    search: Option<String>,
    category_id: Option<String>,
    offset: Option<i64>,
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<AdminProductPage, AppError> {
    let limit = limit.unwrap_or(100).min(500);
    let offset = offset.unwrap_or(0).max(0);

    // Build WHERE clause with parameterized binds — NO string interpolation of user input.
    // Each bind value is pushed in order; the WHERE placeholders match positionally.
    let mut where_clauses: Vec<&'static str> = Vec::new();
    let mut bind_search: Option<String> = None;
    let mut bind_category: Option<String> = None;

    if let Some(ref q) = search {
        let trimmed = q.trim();
        if !trimmed.is_empty() {
            // Use LIKE '%' || ? || '%' — fully parameterized, injection-safe
            where_clauses.push(
                "(p.name LIKE '%' || ? || '%' \
                  OR p.sku LIKE '%' || ? || '%' \
                  OR p.barcode LIKE '%' || ? || '%')",
            );
            bind_search = Some(trimmed.to_string());
        }
    }
    if let Some(ref cid) = category_id {
        let trimmed = cid.trim();
        if !trimmed.is_empty() {
            where_clauses.push("p.category_id = ?");
            bind_category = Some(trimmed.to_string());
        }
    }

    let where_sql = if where_clauses.is_empty() {
        "1=1".to_string()
    } else {
        where_clauses.join(" AND ")
    };

    // Helper: bind search (×3) then category (×1) onto any query
    macro_rules! bind_all {
        ($q:expr) => {{
            let mut q = $q;
            if let Some(ref s) = bind_search {
                q = q.bind(s).bind(s).bind(s);
            }
            if let Some(ref c) = bind_category {
                q = q.bind(c);
            }
            q
        }};
    }

    // Total count
    let count_sql = format!(
        "SELECT COUNT(*) FROM products p \
         JOIN categories c ON c.category_id = p.category_id \
         WHERE {where_sql}",
    );
    let total: i64 = bind_all!(sqlx::query_scalar::<_, i64>(&count_sql))
        .fetch_one(&state.db)
        .await?;

    // Paginated items — limit/offset are i64 values under our control, not user strings
    let sql = format!(
        "{} WHERE {where_sql} ORDER BY p.is_active DESC, p.name LIMIT {limit} OFFSET {offset}",
        ADMIN_PRODUCT_QUERY,
    );
    let rows = bind_all!(sqlx::query(&sql)).fetch_all(&state.db).await?;

    Ok(AdminProductPage {
        items: rows.iter().map(row_to_admin_product).collect(),
        total,
        offset,
        limit,
    })
}

#[derive(Deserialize)]
pub struct CreateProductInput {
    pub category_id: String,
    pub name: String,
    pub sku: Option<String>,
    pub barcode: Option<String>,
    pub tax_rule_id: Option<String>,
    pub price_minor: i64,
    pub track_inventory: bool,
    pub allow_decimal_quantity: bool,
    pub reorder_point: i64,
    pub created_by_user_id: String,
    pub image_path: Option<String>,
    pub cost_minor: Option<i64>,
    pub description: Option<String>,
    pub default_supplier_id: Option<String>,
}

#[tauri::command]
pub async fn admin_create_product(
    input: CreateProductInput,
    state: State<'_, AppState>,
) -> Result<AdminProduct, AppError> {
    rbac::manager_or_owner(&state.db, &input.created_by_user_id).await?;
    let product_id = Ulid::new().to_string();
    let price_id = Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO products
           (product_id, category_id, name, sku, barcode,
            track_inventory, allow_decimal_quantity, is_active,
            tax_rule_id, reorder_point, image_path, cost_minor, description,
            default_supplier_id, currency, created_at, updated_at, version)
         VALUES (?,?,?,?,?,?,?,1,?,?,?,?,?,?,'BHD',?,?,1)",
    )
    .bind(&product_id)
    .bind(&input.category_id)
    .bind(&input.name)
    .bind(input.sku.as_deref().filter(|s| !s.is_empty()))
    .bind(input.barcode.as_deref().filter(|s| !s.is_empty()))
    .bind(input.track_inventory as i64)
    .bind(input.allow_decimal_quantity as i64)
    .bind(&input.tax_rule_id)
    .bind(input.reorder_point)
    .bind(input.image_path.as_deref().filter(|s| !s.is_empty()))
    .bind(input.cost_minor)
    .bind(input.description.as_deref().filter(|s| !s.is_empty()))
    .bind(input.default_supplier_id.as_deref().filter(|s| !s.is_empty()))
    .bind(&now)
    .bind(&now)
    .execute(&state.db)
    .await
    .map_err(|e| {
        if e.to_string().contains("UNIQUE") {
            AppError::Validation("SKU or barcode already in use".into())
        } else {
            e.into()
        }
    })?;

    sqlx::query(
        "INSERT INTO product_prices
           (price_id, product_id, price_type, price_minor, currency,
            effective_from, created_by_user_id, created_at)
         VALUES (?,?,'selling',?,'BHD',?,?,?)",
    )
    .bind(&price_id)
    .bind(&product_id)
    .bind(input.price_minor)
    .bind(&now)
    .bind(&input.created_by_user_id)
    .bind(&now)
    .execute(&state.db)
    .await?;

    if input.track_inventory {
        let branch_id = active_branch_id(&state).await?;
        let sl_id = format!("SL-{}", product_id);
        sqlx::query(
            "INSERT OR IGNORE INTO stock_levels
               (stock_level_id, product_id, branch_id, quantity_on_hand, created_at, updated_at)
             VALUES (?,?,?,'0',?,?)",
        )
        .bind(&sl_id)
        .bind(&product_id)
        .bind(&branch_id)
        .bind(&now)
        .bind(&now)
        .execute(&state.db)
        .await?;
    }

    // sync_status='pending' is set by column DEFAULT — sync worker picks it up

    let device_id = active_device_id(&state).await;
    let branch_id = active_branch_id(&state).await?;

    // H8: Audit log — product created
    let after = serde_json::json!({
        "product_id": product_id, "name": input.name,
        "category_id": input.category_id, "sku": input.sku,
        "price_minor": input.price_minor, "is_active": true,
    }).to_string();
    let _ = audit_hash::insert_audit_entry(
        &state.db, "PRODUCT_CREATED", "product", &product_id,
        &input.created_by_user_id, "user", &device_id, &branch_id,
        None, Some(&after), None,
    ).await;

    // Return the newly created product
    let sql = format!("{} WHERE p.product_id = ?", ADMIN_PRODUCT_QUERY);
    let row = sqlx::query(&sql)
        .bind(&product_id)
        .fetch_one(&state.db)
        .await?;
    Ok(row_to_admin_product(&row))
}

#[derive(Deserialize)]
pub struct UpdateProductInput {
    pub product_id: String,
    pub category_id: String,
    pub name: String,
    pub sku: Option<String>,
    pub barcode: Option<String>,
    pub tax_rule_id: Option<String>,
    pub price_minor: i64,
    pub track_inventory: bool,
    pub allow_decimal_quantity: bool,
    pub reorder_point: i64,
    pub is_active: bool,
    pub updated_by_user_id: String,
    pub image_path: Option<String>,
    pub cost_minor: Option<i64>,
    pub description: Option<String>,
    pub default_supplier_id: Option<String>,
}

#[tauri::command]
pub async fn admin_update_product(
    input: UpdateProductInput,
    state: State<'_, AppState>,
) -> Result<AdminProduct, AppError> {
    rbac::manager_or_owner(&state.db, &input.updated_by_user_id).await?;
    let now = chrono::Utc::now().to_rfc3339();

    // Check if price changed — fetch full old price row for H-8 re-enqueue
    let old_price_row = sqlx::query(
        "SELECT price_id, price_minor, effective_from FROM product_prices
         WHERE product_id = ? AND branch_id IS NULL
           AND price_type = 'selling' AND effective_to IS NULL
         LIMIT 1",
    )
    .bind(&input.product_id)
    .fetch_optional(&state.db)
    .await?;

    let current_price: Option<i64> = old_price_row.as_ref().map(|r| r.get("price_minor"));

    // H-4: version = version + 1, plus M-3/M-4/M-5 new fields
    sqlx::query(
        "UPDATE products SET
           category_id=?, name=?, sku=?, barcode=?, tax_rule_id=?,
           track_inventory=?, allow_decimal_quantity=?,
           reorder_point=?, is_active=?, image_path=?, cost_minor=?, description=?,
           default_supplier_id=?, updated_at=?, version = version + 1,
           sync_status = 'pending'
         WHERE product_id=?",
    )
    .bind(&input.category_id)
    .bind(&input.name)
    .bind(input.sku.as_deref().filter(|s| !s.is_empty()))
    .bind(input.barcode.as_deref().filter(|s| !s.is_empty()))
    .bind(&input.tax_rule_id)
    .bind(input.track_inventory as i64)
    .bind(input.allow_decimal_quantity as i64)
    .bind(input.reorder_point)
    .bind(input.is_active as i64)
    .bind(input.image_path.as_deref().filter(|s| !s.is_empty()))
    .bind(input.cost_minor)
    .bind(input.description.as_deref().filter(|s| !s.is_empty()))
    .bind(input.default_supplier_id.as_deref().filter(|s| !s.is_empty()))
    .bind(&now)
    .bind(&input.product_id)
    .execute(&state.db)
    .await
    .map_err(|e| {
        if e.to_string().contains("UNIQUE") {
            AppError::Validation("SKU or barcode already in use".into())
        } else {
            e.into()
        }
    })?;

    if current_price != Some(input.price_minor) {
        // Close old price
        sqlx::query(
            "UPDATE product_prices SET effective_to = ?, sync_status = 'pending'
             WHERE product_id = ? AND branch_id IS NULL
               AND price_type = 'selling' AND effective_to IS NULL",
        )
        .bind(&now)
        .bind(&input.product_id)
        .execute(&state.db)
        .await?;

        // sync_status='pending' is set by column DEFAULT — sync worker picks it up

        // Insert new price
        let price_id = Ulid::new().to_string();
        sqlx::query(
            "INSERT INTO product_prices
               (price_id, product_id, price_type, price_minor, currency,
                effective_from, created_by_user_id, created_at)
             VALUES (?,?,'selling',?,'BHD',?,?,?)",
        )
        .bind(&price_id)
        .bind(&input.product_id)
        .bind(input.price_minor)
        .bind(&now)
        .bind(&input.updated_by_user_id)
        .bind(&now)
        .execute(&state.db)
        .await?;

        // sync_status='pending' is set by column DEFAULT — sync worker picks it up
    }

    if input.track_inventory {
        let branch_id = active_branch_id(&state).await?;
        let sl_id = format!("SL-{}", input.product_id);
        sqlx::query(
            "INSERT OR IGNORE INTO stock_levels
               (stock_level_id, product_id, branch_id, quantity_on_hand, created_at, updated_at)
             VALUES (?,?,?,'0',?,?)",
        )
        .bind(&sl_id)
        .bind(&input.product_id)
        .bind(&branch_id)
        .bind(&now)
        .bind(&now)
        .execute(&state.db)
        .await?;
    }

    // sync_status='pending' is set by column DEFAULT — sync worker picks it up

    // H8: Audit log — product updated
    let device_id = active_device_id(&state).await;
    let branch_id_str = active_branch_id(&state).await?;
    let after = serde_json::json!({
        "product_id": input.product_id, "name": input.name,
        "category_id": input.category_id, "sku": input.sku,
        "price_minor": input.price_minor, "is_active": input.is_active,
    }).to_string();
    let _ = audit_hash::insert_audit_entry(
        &state.db, "PRODUCT_UPDATED", "product", &input.product_id,
        &input.updated_by_user_id, "user", &device_id, &branch_id_str,
        None, Some(&after), None,
    ).await;

    let sql = format!("{} WHERE p.product_id = ?", ADMIN_PRODUCT_QUERY);
    let row = sqlx::query(&sql)
        .bind(&input.product_id)
        .fetch_one(&state.db)
        .await?;
    Ok(row_to_admin_product(&row))
}

// ─── Category commands ────────────────────────────────────────────────────────

#[tauri::command]
pub async fn admin_list_categories(
    state: State<'_, AppState>,
) -> Result<Vec<CategoryRow>, AppError> {
    // Returns ALL categories (active + inactive) for the back-office admin view
    // so managers can re-activate archived categories. Product-grid and POS
    // code paths use a separate query filtered to is_active=1.
    let rows = sqlx::query(
        "SELECT category_id, name, sort_order, is_active, parent_category_id
         FROM categories ORDER BY is_active DESC, sort_order, name",
    )
    .fetch_all(&state.db)
    .await?;

    Ok(rows
        .iter()
        .map(|r| {
            let active: i64 = r.get("is_active");
            CategoryRow {
                category_id: r.get("category_id"),
                name: r.get("name"),
                sort_order: r.get("sort_order"),
                is_active: active != 0,
                parent_category_id: r.get("parent_category_id"),
            }
        })
        .collect())
}

#[tauri::command]
pub async fn admin_list_tax_rules(state: State<'_, AppState>) -> Result<Vec<TaxRuleRow>, AppError> {
    let rows = sqlx::query(
        "SELECT tax_rule_id, name, rate_basis_points, inclusive, is_active
         FROM tax_rules ORDER BY name",
    )
    .fetch_all(&state.db)
    .await?;

    Ok(rows
        .iter()
        .map(|r| {
            let inc: i64 = r.get("inclusive");
            let active: i64 = r.get("is_active");
            TaxRuleRow {
                tax_rule_id: r.get("tax_rule_id"),
                name: r.get("name"),
                rate_basis_points: r.get("rate_basis_points"),
                inclusive: inc != 0,
                is_active: active != 0,
            }
        })
        .collect())
}

// ─── Tax Rule commands ────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct SaveTaxRuleInput {
    pub tax_rule_id: Option<String>, // None = create new
    pub name: String,
    /// Rate in basis points (e.g. 1000 = 10.00%). Integer-only, no float.
    pub rate_basis_points: i64,
    pub inclusive: bool,
    pub is_active: bool,
    pub actor_user_id: String,
}

#[tauri::command]
pub async fn admin_save_tax_rule(
    input: SaveTaxRuleInput,
    state: State<'_, AppState>,
) -> Result<TaxRuleRow, AppError> {
    rbac::manager_or_owner(&state.db, &input.actor_user_id).await?;

    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::Validation("Tax rule name is required".into()));
    }
    if input.rate_basis_points < 0 || input.rate_basis_points > 10000 {
        return Err(AppError::Validation("Rate must be between 0% and 100%".into()));
    }
    let rate_basis_points = input.rate_basis_points;
    let now = chrono::Utc::now().to_rfc3339();
    let is_tax_update = input.tax_rule_id.is_some();

    let tax_rule_id = if let Some(ref id) = input.tax_rule_id {
        // H-6: Check if rate or inclusive changed — tax rules are append-only
        let existing = sqlx::query(
            "SELECT rate_basis_points, inclusive FROM tax_rules WHERE tax_rule_id = ?",
        )
        .bind(id)
        .fetch_optional(&state.db)
        .await?;

        let rate_changed = existing.as_ref().map_or(true, |r| {
            let old_rate: i64 = r.get("rate_basis_points");
            let old_inclusive: i64 = r.get("inclusive");
            old_rate != rate_basis_points || old_inclusive != (input.inclusive as i64)
        });

        if rate_changed {
            // Close old rule by setting effective_to
            sqlx::query(
                "UPDATE tax_rules SET effective_to = ?, updated_at = ?, sync_status = 'pending' WHERE tax_rule_id = ?",
            )
            .bind(&now)
            .bind(&now)
            .bind(id)
            .execute(&state.db)
            .await?;

            // sync_status='pending' is set by column DEFAULT — sync worker picks it up

            // Insert new rule with new rate/inclusive (append-only pattern)
            let new_id = Ulid::new().to_string();
            sqlx::query(
                "INSERT INTO tax_rules
                   (tax_rule_id, name, rate_basis_points, inclusive, is_active,
                    effective_from, created_at, updated_at, version)
                 VALUES (?,?,?,?,?,?,?,?,1)",
            )
            .bind(&new_id)
            .bind(&name)
            .bind(rate_basis_points)
            .bind(input.inclusive as i64)
            .bind(input.is_active as i64)
            .bind(&now)
            .bind(&now)
            .bind(&now)
            .execute(&state.db)
            .await?;
            new_id
        } else {
            // Non-rate change: normal UPDATE (name, is_active only)
            sqlx::query(
                "UPDATE tax_rules SET name=?, is_active=?, updated_at=?, sync_status='pending'
                 WHERE tax_rule_id=?",
            )
            .bind(&name)
            .bind(input.is_active as i64)
            .bind(&now)
            .bind(id)
            .execute(&state.db)
            .await?;
            id.clone()
        }
    } else {
        // Create new
        let id = Ulid::new().to_string();
        sqlx::query(
            "INSERT INTO tax_rules
               (tax_rule_id, name, rate_basis_points, inclusive, is_active, effective_from,
                created_at, updated_at)
             VALUES (?,?,?,?,?,?,?,?)",
        )
        .bind(&id)
        .bind(&name)
        .bind(rate_basis_points)
        .bind(input.inclusive as i64)
        .bind(input.is_active as i64)
        .bind(&now)
        .bind(&now)
        .bind(&now)
        .execute(&state.db)
        .await?;
        id
    };

    let row = sqlx::query(
        "SELECT tax_rule_id, name, rate_basis_points, inclusive, is_active
         FROM tax_rules WHERE tax_rule_id=?",
    )
    .bind(&tax_rule_id)
    .fetch_one(&state.db)
    .await?;

    let inc: i64 = row.get("inclusive");
    let active: i64 = row.get("is_active");
    let result = TaxRuleRow {
        tax_rule_id: row.get("tax_rule_id"),
        name: row.get("name"),
        rate_basis_points: row.get("rate_basis_points"),
        inclusive: inc != 0,
        is_active: active != 0,
    };

    // sync_status='pending' is set by column DEFAULT — sync worker picks it up

    // H8: Audit log — tax rule created/updated
    let device_id = active_device_id(&state).await;
    let branch_id = active_branch_id(&state).await?;
    let tax_event = if is_tax_update { "TAX_RULE_UPDATED" } else { "TAX_RULE_CREATED" };
    let after = serde_json::json!({
        "tax_rule_id": result.tax_rule_id, "name": result.name,
        "rate_basis_points": result.rate_basis_points, "inclusive": result.inclusive,
        "is_active": result.is_active,
    }).to_string();
    let _ = audit_hash::insert_audit_entry(
        &state.db, tax_event, "tax_rule", &result.tax_rule_id,
        &input.actor_user_id, "user", &device_id, &branch_id,
        None, Some(&after), None,
    ).await;

    Ok(result)
}

// ─── Category commands ────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct SaveCategoryInput {
    pub category_id: Option<String>, // None = create
    pub name: String,
    pub sort_order: i64,
    pub is_active: bool,
    pub parent_category_id: Option<String>,
    pub actor_user_id: String,
}

#[tauri::command]
pub async fn admin_save_category(
    input: SaveCategoryInput,
    state: State<'_, AppState>,
) -> Result<CategoryRow, AppError> {
    rbac::manager_or_owner(&state.db, &input.actor_user_id).await?;
    let now = chrono::Utc::now().to_rfc3339();
    // Determine audit event type before consuming input.category_id
    let is_update = input.category_id.is_some();

    let category_id = if let Some(id) = input.category_id {
        // H-5: version = version + 1 on UPDATE
        sqlx::query(
            "UPDATE categories SET name=?, sort_order=?, is_active=?, parent_category_id=?, updated_at=?,
             version = version + 1, sync_status = 'pending'
             WHERE category_id=?",
        )
        .bind(&input.name)
        .bind(input.sort_order)
        .bind(input.is_active as i64)
        .bind(&input.parent_category_id)
        .bind(&now)
        .bind(&id)
        .execute(&state.db)
        .await?;
        id
    } else {
        let id = Ulid::new().to_string();
        sqlx::query(
            "INSERT INTO categories
               (category_id, name, sort_order, is_active, parent_category_id, created_at, updated_at, version)
             VALUES (?,?,?,1,?,?,?,1)",
        )
        .bind(&id)
        .bind(&input.name)
        .bind(input.sort_order)
        .bind(&input.parent_category_id)
        .bind(&now)
        .bind(&now)
        .execute(&state.db)
        .await?;
        id
    };

    let row = sqlx::query(
        "SELECT category_id, name, sort_order, is_active, parent_category_id FROM categories WHERE category_id = ?",
    )
    .bind(&category_id)
    .fetch_one(&state.db)
    .await?;

    let active: i64 = row.get("is_active");
    let result = CategoryRow {
        category_id: row.get("category_id"),
        name: row.get("name"),
        sort_order: row.get("sort_order"),
        is_active: active != 0,
        parent_category_id: row.get("parent_category_id"),
    };

    // sync_status='pending' is set by column DEFAULT — sync worker picks it up

    // H8: Audit log — category created/updated
    let device_id = active_device_id(&state).await;
    let branch_id = active_branch_id(&state).await?;
    let event = if is_update { "CATEGORY_UPDATED" } else { "CATEGORY_CREATED" };
    let after = serde_json::json!({
        "category_id": result.category_id, "name": result.name,
        "sort_order": result.sort_order, "is_active": result.is_active,
        "parent_category_id": result.parent_category_id,
    }).to_string();
    let _ = audit_hash::insert_audit_entry(
        &state.db, event, "category", &result.category_id,
        &input.actor_user_id, "user", &device_id, &branch_id,
        None, Some(&after), None,
    ).await;

    Ok(result)
}

// ─── Bulk import ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct BulkCategoryRow {
    pub name: String,
    pub sort_order: Option<i64>,
    pub parent_category_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BulkProductRow {
    pub name: String,
    pub category_name: String,
    /// Selling price in full currency units (e.g. "1.500" = BHD 1.500 = 1500 minor)
    pub price: String,
    pub sku: Option<String>,
    /// Single barcode — kept for backward compat. Ignored if `barcodes` is present.
    pub barcode: Option<String>,
    /// Pipe-separated list of barcodes e.g. "12345|67890|11111".
    /// First value → products.barcode (legacy POS scanner compat).
    /// All values → product_barcodes table (multi-barcode).
    /// Takes priority over `barcode` when both are present.
    pub barcodes: Option<String>,
    pub track_inventory: Option<bool>,
    pub tax_rule_name: Option<String>,
    pub reorder_point: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct BulkRowError {
    pub row: usize,
    pub name: String,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct BulkImportResult {
    pub inserted: usize,
    pub skipped: usize,
    pub errors: Vec<BulkRowError>,
}

/// Bulk-import categories from CSV rows.
/// Rows whose name already exists are skipped (not an error).
#[tauri::command]
pub async fn admin_bulk_import_categories(
    rows: Vec<BulkCategoryRow>,
    actor_user_id: String,
    state: State<'_, AppState>,
) -> AppResult<BulkImportResult> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;

    // Determine the current max sort_order so auto-assigned ones don't collide.
    let max_order: i64 = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(MAX(sort_order), 0) FROM categories",
    )
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let now = chrono::Utc::now().to_rfc3339();
    let mut inserted = 0usize;
    let mut skipped = 0usize;
    let mut errors: Vec<BulkRowError> = Vec::new();

    for (idx, row) in rows.iter().enumerate() {
        let name = row.name.trim();
        if name.is_empty() {
            errors.push(BulkRowError { row: idx + 1, name: row.name.clone(), reason: "Name is empty".into() });
            continue;
        }

        // Check duplicate by name (case-insensitive)
        let exists: bool = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM categories WHERE lower(name) = lower(?)",
        )
        .bind(name)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0) > 0;

        if exists {
            skipped += 1;
            continue;
        }

        let sort_order = row.sort_order.unwrap_or(max_order + inserted as i64 + 1);
        let category_id = Ulid::new().to_string();

        match sqlx::query(
            "INSERT INTO categories (category_id, name, sort_order, is_active, parent_category_id, created_at, updated_at, version)
             VALUES (?,?,?,1,?,?,?,1)",
        )
        .bind(&category_id)
        .bind(name)
        .bind(sort_order)
        .bind(row.parent_category_id.as_deref().filter(|s| !s.is_empty()))
        .bind(&now)
        .bind(&now)
        .execute(&state.db)
        .await
        {
            Ok(_) => {
                inserted += 1;
            }
            Err(e) if e.to_string().contains("UNIQUE") => skipped += 1,
            Err(e) => errors.push(BulkRowError {
                row: idx + 1,
                name: row.name.clone(),
                reason: format!("DB error: {}", e),
            }),
        }
    }

    // sync_status='pending' is set by column DEFAULT — sync worker picks it up

    Ok(BulkImportResult { inserted, skipped, errors })
}

/// Bulk-import products from CSV rows.
///
/// - `category_name` is matched case-insensitively. If it doesn't exist it is
///   created automatically.
/// - `price` is a decimal string in full currency units (BHD): "1.500" → 1500 minor.
/// - Rows whose name+barcode/sku already conflict are skipped.
#[tauri::command]
pub async fn admin_bulk_import_products(
    rows: Vec<BulkProductRow>,
    actor_user_id: String,
    state: State<'_, AppState>,
) -> AppResult<BulkImportResult> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;

    let now = chrono::Utc::now().to_rfc3339();
    let branch_id = active_branch_id(&state).await?;

    // Build case-insensitive maps once
    let cat_rows = sqlx::query("SELECT category_id, name FROM categories WHERE is_active = 1")
        .fetch_all(&state.db)
        .await?;
    let mut cat_map: std::collections::HashMap<String, String> = cat_rows
        .iter()
        .map(|r| {
            let n: String = r.get("name");
            let id: String = r.get("category_id");
            (n.to_lowercase(), id)
        })
        .collect();

    let tax_rows = sqlx::query("SELECT tax_rule_id, name FROM tax_rules WHERE is_active = 1")
        .fetch_all(&state.db)
        .await?;
    let tax_map: std::collections::HashMap<String, String> = tax_rows
        .iter()
        .map(|r| {
            let n: String = r.get("name");
            let id: String = r.get("tax_rule_id");
            (n.to_lowercase(), id)
        })
        .collect();

    let mut inserted = 0usize;
    let mut skipped = 0usize;
    let mut errors: Vec<BulkRowError> = Vec::new();

    // Wrap all inserts in a single explicit transaction.
    // Without this SQLite auto-commits each statement individually, meaning
    // a 500-product import does ~2000 separate fsyncs.  One transaction
    // reduces that to one fsync and makes the import ~50× faster.
    let mut tx = state.db.begin().await?;

    for (idx, row) in rows.iter().enumerate() {
        let name = row.name.trim();
        if name.is_empty() {
            errors.push(BulkRowError { row: idx + 1, name: row.name.clone(), reason: "Name is empty".into() });
            continue;
        }

        // Parse price — decimal string → minor integer (no float)
        let price_minor = match crate::domain::money::parse_major_to_minor(&row.price, 3) {
            Some(v) if v <= 999_999_000 => v, // max 999.999 BHD
            Some(_) => {
                errors.push(BulkRowError { row: idx + 1, name: row.name.clone(), reason: format!("Price out of range: {}", row.price) });
                continue;
            }
            None => {
                errors.push(BulkRowError { row: idx + 1, name: row.name.clone(), reason: format!("Invalid price: '{}'", row.price) });
                continue;
            }
        };

        // Resolve or create category
        let cat_key = row.category_name.trim().to_lowercase();
        if cat_key.is_empty() {
            errors.push(BulkRowError { row: idx + 1, name: row.name.clone(), reason: "category_name is empty".into() });
            continue;
        }
        let category_id = if let Some(id) = cat_map.get(&cat_key) {
            id.clone()
        } else {
            // Auto-create the category
            let new_cat_id = Ulid::new().to_string();
            let max_order: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(sort_order), 0) FROM categories")
                .fetch_one(&mut *tx)
                .await
                .unwrap_or(0);
            let res = sqlx::query(
                "INSERT OR IGNORE INTO categories (category_id, name, sort_order, is_active, created_at, updated_at, version)
                 VALUES (?,?,?,1,?,?,1)",
            )
            .bind(&new_cat_id)
            .bind(row.category_name.trim())
            .bind(max_order + 1)
            .bind(&now)
            .bind(&now)
            .execute(&mut *tx)
            .await;
            match res {
                Ok(r) if r.rows_affected() > 0 => {
                    cat_map.insert(cat_key, new_cat_id.clone());
                    new_cat_id
                }
                _ => {
                    // Might have been created by a previous row in this batch — re-query
                    match sqlx::query_scalar::<_, String>(
                        "SELECT category_id FROM categories WHERE lower(name) = lower(?) LIMIT 1",
                    )
                    .bind(row.category_name.trim())
                    .fetch_optional(&mut *tx)
                    .await
                    {
                        Ok(Some(id)) => {
                            cat_map.insert(cat_key, id.clone());
                            id
                        }
                        _ => {
                            errors.push(BulkRowError { row: idx + 1, name: row.name.clone(), reason: format!("Could not resolve category '{}'", row.category_name) });
                            continue;
                        }
                    }
                }
            }
        };

        // Resolve optional tax rule
        let tax_rule_id: Option<String> = row
            .tax_rule_name
            .as_deref()
            .and_then(|n| tax_map.get(&n.trim().to_lowercase()))
            .cloned();

        let sku = row.sku.as_deref().map(str::trim).filter(|s| !s.is_empty());

        // Resolve barcodes — pipe-separated `barcodes` field takes priority over single `barcode`
        let all_barcodes: Vec<&str> = if let Some(ref bc_str) = row.barcodes {
            bc_str.split('|').map(str::trim).filter(|s| !s.is_empty()).collect()
        } else if let Some(ref bc) = row.barcode {
            let bc = bc.trim();
            if bc.is_empty() { vec![] } else { vec![bc] }
        } else {
            vec![]
        };
        let primary_barcode = all_barcodes.first().copied(); // stored in products.barcode

        let track = row.track_inventory.unwrap_or(true);
        let reorder = row.reorder_point.unwrap_or(0);
        let product_id = Ulid::new().to_string();
        let price_id = format!("PRC-{}", Ulid::new());

        let res = sqlx::query(
            "INSERT INTO products
               (product_id, category_id, name, sku, barcode,
                track_inventory, allow_decimal_quantity, is_active,
                tax_rule_id, reorder_point, currency, created_at, updated_at, version)
             VALUES (?,?,?,?,?,?,0,1,?,?,'BHD',?,?,1)",
        )
        .bind(&product_id)
        .bind(&category_id)
        .bind(name)
        .bind(sku)
        .bind(primary_barcode)
        .bind(track as i64)
        .bind(&tax_rule_id)
        .bind(reorder)
        .bind(&now)
        .bind(&now)
        .execute(&mut *tx)
        .await;

        match res {
            Err(e) if e.to_string().contains("UNIQUE") => {
                skipped += 1;
                continue;
            }
            Err(e) => {
                errors.push(BulkRowError { row: idx + 1, name: row.name.clone(), reason: format!("DB error: {}", e) });
                continue;
            }
            Ok(_) => {}
        }

        // Insert all barcodes into product_barcodes table
        for bc in &all_barcodes {
            let bc_id = format!("BC-{}", Ulid::new());
            let _ = sqlx::query(
                "INSERT OR IGNORE INTO product_barcodes
                   (barcode_id, product_id, barcode, created_at)
                 VALUES (?,?,?,?)",
            )
            .bind(&bc_id)
            .bind(&product_id)
            .bind(bc)
            .bind(&now)
            .execute(&mut *tx)
            .await;
        }

        // Insert price
        let _ = sqlx::query(
            "INSERT INTO product_prices
               (price_id, product_id, price_type, price_minor, currency,
                effective_from, created_by_user_id, created_at)
             VALUES (?,?,'selling',?,'BHD',?,?,?)",
        )
        .bind(&price_id)
        .bind(&product_id)
        .bind(price_minor)
        .bind(&now)
        .bind(&actor_user_id)
        .bind(&now)
        .execute(&mut *tx)
        .await;

        // Init stock level
        if track {
            let sl_id = format!("SL-{}", product_id);
            let _ = sqlx::query(
                "INSERT OR IGNORE INTO stock_levels
                   (stock_level_id, product_id, branch_id, quantity_on_hand, created_at, updated_at)
                 VALUES (?,?,?,'0',?,?)",
            )
            .bind(&sl_id)
            .bind(&product_id)
            .bind(&branch_id)
            .bind(&now)
            .bind(&now)
            .execute(&mut *tx)
            .await;
        }

        inserted += 1;
    }

    tx.commit().await?;

    // sync_status='pending' is set by column DEFAULT — sync worker picks it up

    Ok(BulkImportResult { inserted, skipped, errors })
}

// ─── User commands ────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn admin_list_users_all(
    state: State<'_, AppState>,
) -> Result<Vec<AdminUserRow>, AppError> {
    let rows = sqlx::query(
        "SELECT u.user_id, u.display_name, u.username, u.role_id, u.is_active, u.last_login_at,
                r.name AS role_name
         FROM users u JOIN roles r ON r.role_id = u.role_id
         ORDER BY u.is_active DESC, u.display_name",
    )
    .fetch_all(&state.db)
    .await?;

    Ok(rows
        .iter()
        .map(|r| {
            let active: i64 = r.get("is_active");
            AdminUserRow {
                user_id: r.get("user_id"),
                display_name: r.get("display_name"),
                username: r.get("username"),
                role_id: r.get("role_id"),
                role_name: r.get("role_name"),
                is_active: active != 0,
                last_login_at: r.get("last_login_at"),
            }
        })
        .collect())
}

#[tauri::command]
pub async fn admin_list_roles(state: State<'_, AppState>) -> Result<Vec<RoleRow>, AppError> {
    let rows = sqlx::query("SELECT role_id, name FROM roles ORDER BY name")
        .fetch_all(&state.db)
        .await?;
    Ok(rows
        .iter()
        .map(|r| RoleRow {
            role_id: r.get("role_id"),
            name: r.get("name"),
        })
        .collect())
}

#[derive(Deserialize)]
pub struct CreateUserInput {
    pub display_name: String,
    pub username: String,
    pub pin: String,
    pub role_id: String,
    pub actor_user_id: String,
}

#[tauri::command]
pub async fn admin_create_user(
    input: CreateUserInput,
    state: State<'_, AppState>,
) -> Result<AdminUserRow, AppError> {
    rbac::manager_or_owner(&state.db, &input.actor_user_id).await?;
    if input.pin.len() < 4 {
        return Err(AppError::Validation("PIN must be at least 4 digits".into()));
    }
    let user_id = Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let pin_hash = auth_repo::hash_pin(&input.pin)?;
    let branch_id = active_branch_id(&state).await?;

    sqlx::query(
        "INSERT INTO users
           (user_id, branch_id, display_name, username, pin_hash, role_id,
            branch_scope, is_active, created_at, updated_at, version)
         VALUES (?,?,?,?,?,?,'[]',1,?,?,1)",
    )
    .bind(&user_id)
    .bind(&branch_id)
    .bind(&input.display_name)
    .bind(&input.username)
    .bind(&pin_hash)
    .bind(&input.role_id)
    .bind(&now)
    .bind(&now)
    .execute(&state.db)
    .await
    .map_err(|e| {
        if e.to_string().contains("UNIQUE") {
            AppError::Validation("Username already exists".into())
        } else {
            e.into()
        }
    })?;

    let row = sqlx::query(
        "SELECT u.user_id, u.display_name, u.username, u.role_id, u.is_active, u.last_login_at,
                r.name AS role_name
         FROM users u JOIN roles r ON r.role_id = u.role_id WHERE u.user_id = ?",
    )
    .bind(&user_id)
    .fetch_one(&state.db)
    .await?;

    let active: i64 = row.get("is_active");
    let result = AdminUserRow {
        user_id: row.get("user_id"),
        display_name: row.get("display_name"),
        username: row.get("username"),
        role_id: row.get("role_id"),
        role_name: row.get("role_name"),
        is_active: active != 0,
        last_login_at: row.get("last_login_at"),
    };

    // sync_status='pending' is set by column DEFAULT — sync worker picks it up

    // H8: Audit log — user created
    let device_id = active_device_id(&state).await;
    let branch_id = active_branch_id(&state).await?;
    let after = serde_json::json!({
        "user_id": result.user_id, "display_name": result.display_name,
        "username": result.username, "role_id": result.role_id,
        "is_active": result.is_active,
    }).to_string();
    let _ = audit_hash::insert_audit_entry(
        &state.db, "USER_CREATED", "user", &result.user_id,
        &input.actor_user_id, "user", &device_id, &branch_id,
        None, Some(&after), None,
    ).await;

    Ok(result)
}

// ─── Product barcode management ───────────────────────────────────────────────

#[tauri::command]
pub async fn product_barcode_add(
    actor_user_id: String,
    product_id: String,
    barcode: String,
    state: State<'_, AppState>,
) -> AppResult<ProductBarcodeRow> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    let barcode_id = format!("PBC-{}", Ulid::new());
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO product_barcodes (barcode_id, product_id, barcode, created_at)
         VALUES (?, ?, ?, ?)",
    )
    .bind(&barcode_id)
    .bind(&product_id)
    .bind(&barcode)
    .bind(&now)
    .execute(&state.db)
    .await
    .map_err(|e| {
        if e.to_string().contains("UNIQUE") {
            AppError::Validation("Barcode already exists on another product".into())
        } else {
            e.into()
        }
    })?;

    // M-11: Audit trail for barcode add
    let device_id = active_device_id(&state).await;
    let branch_id = active_branch_id(&state).await?;
    let after = serde_json::json!({
        "barcode_id": barcode_id, "product_id": product_id, "barcode": barcode,
    }).to_string();
    let _ = audit_hash::insert_audit_entry(
        &state.db, "BARCODE_ADDED", "product", &product_id,
        &actor_user_id, "user", &device_id, &branch_id,
        None, Some(&after), None,
    ).await;
    // Also re-enqueue the product so other terminals pick up the new barcode field
    // (product_barcodes is a separate table but barcode on products is the primary one)
    drop((device_id, branch_id));

    Ok(ProductBarcodeRow {
        barcode_id,
        product_id,
        barcode,
        created_at: now,
    })
}

#[tauri::command]
pub async fn product_barcode_remove(
    actor_user_id: String,
    barcode_id: String,
    state: State<'_, AppState>,
) -> AppResult<()> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;

    // Fetch the barcode record before deleting so we can audit it
    let record = sqlx::query(
        "SELECT product_id, barcode FROM product_barcodes WHERE barcode_id = ?",
    )
    .bind(&barcode_id)
    .fetch_optional(&state.db)
    .await?;

    let affected = sqlx::query("DELETE FROM product_barcodes WHERE barcode_id = ?")
        .bind(&barcode_id)
        .execute(&state.db)
        .await?
        .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound(format!(
            "Barcode {} not found",
            barcode_id
        )));
    }

    // M-11: Audit trail for barcode removal
    if let Some(row) = record {
        let product_id: String = row.get("product_id");
        let barcode_val: String = row.get("barcode");
        let device_id = active_device_id(&state).await;
        let branch_id = active_branch_id(&state).await?;
        let after = serde_json::json!({
            "barcode_id": barcode_id, "product_id": product_id, "barcode": barcode_val, "removed": true,
        }).to_string();
        let _ = audit_hash::insert_audit_entry(
            &state.db, "BARCODE_REMOVED", "product", &product_id,
            &actor_user_id, "user", &device_id, &branch_id,
            None, Some(&after), None,
        ).await;
    }

    Ok(())
}

#[tauri::command]
pub async fn product_barcodes_list(
    product_id: String,
    state: State<'_, AppState>,
) -> AppResult<Vec<ProductBarcodeRow>> {
    let rows = sqlx::query(
        "SELECT barcode_id, product_id, barcode, created_at
         FROM product_barcodes WHERE product_id = ? ORDER BY created_at",
    )
    .bind(&product_id)
    .fetch_all(&state.db)
    .await?;

    Ok(rows
        .iter()
        .map(|r| ProductBarcodeRow {
            barcode_id: r.get("barcode_id"),
            product_id: r.get("product_id"),
            barcode: r.get("barcode"),
            created_at: r.get("created_at"),
        })
        .collect())
}

#[derive(Deserialize)]
pub struct UpdateUserInput {
    pub user_id: String,
    pub display_name: String,
    pub pin: Option<String>, // None = unchanged
    pub role_id: String,
    pub is_active: bool,
    pub actor_user_id: String,
}

#[tauri::command]
pub async fn admin_update_user(
    input: UpdateUserInput,
    state: State<'_, AppState>,
) -> Result<AdminUserRow, AppError> {
    rbac::manager_or_owner(&state.db, &input.actor_user_id).await?;
    if let Some(pin) = &input.pin {
        if pin.len() < 4 {
            return Err(AppError::Validation("PIN must be at least 4 digits".into()));
        }
    }
    let now = chrono::Utc::now().to_rfc3339();

    if let Some(pin) = &input.pin {
        let pin_hash = auth_repo::hash_pin(pin)?;
        sqlx::query(
            "UPDATE users SET display_name=?, pin_hash=?, role_id=?, is_active=?, updated_at=?,
             sync_status='pending'
             WHERE user_id=?",
        )
        .bind(&input.display_name)
        .bind(&pin_hash)
        .bind(&input.role_id)
        .bind(input.is_active as i64)
        .bind(&now)
        .bind(&input.user_id)
        .execute(&state.db)
        .await?;
    } else {
        sqlx::query(
            "UPDATE users SET display_name=?, role_id=?, is_active=?, updated_at=?,
             sync_status='pending'
             WHERE user_id=?",
        )
        .bind(&input.display_name)
        .bind(&input.role_id)
        .bind(input.is_active as i64)
        .bind(&now)
        .bind(&input.user_id)
        .execute(&state.db)
        .await?;
    }

    let row = sqlx::query(
        "SELECT u.user_id, u.display_name, u.username, u.role_id, u.is_active, u.last_login_at,
                r.name AS role_name
         FROM users u JOIN roles r ON r.role_id = u.role_id WHERE u.user_id = ?",
    )
    .bind(&input.user_id)
    .fetch_one(&state.db)
    .await?;

    let active: i64 = row.get("is_active");
    let result = AdminUserRow {
        user_id: row.get("user_id"),
        display_name: row.get("display_name"),
        username: row.get("username"),
        role_id: row.get("role_id"),
        role_name: row.get("role_name"),
        is_active: active != 0,
        last_login_at: row.get("last_login_at"),
    };

    // sync_status='pending' is set by column DEFAULT — sync worker picks it up

    // H8: Audit log — user updated (pin_changed flag, but never the hash)
    let device_id = active_device_id(&state).await;
    let branch_id = active_branch_id(&state).await?;
    let after = serde_json::json!({
        "user_id": result.user_id, "display_name": result.display_name,
        "role_id": result.role_id, "is_active": result.is_active,
        "pin_changed": input.pin.is_some(),
    }).to_string();
    let _ = audit_hash::insert_audit_entry(
        &state.db, "USER_UPDATED", "user", &result.user_id,
        &input.actor_user_id, "user", &device_id, &branch_id,
        None, Some(&after), None,
    ).await;

    Ok(result)
}
