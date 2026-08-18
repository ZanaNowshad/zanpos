use crate::commands::{rbac, sync_commands};
use crate::db::repositories::{audit_hash, auth_repo, product_dedup_repo};
use crate::errors::{AppError, AppResult};
use crate::product_image_search::{
    search_product_image, ProductImageSearchRequest, ProductImageSearchResult,
};
use crate::AppState;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
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
    crate::device_identity::current_or_unknown(&state.db).await
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

/// A product image is either a local file path from the picker or a URL a user
/// typed. This field is also written by the AI catalogue tools and by CSV
/// import, so the check lives here rather than in the form: a `javascript:` or
/// `data:` value reaching the catalogue would be rendered by every screen that
/// shows the product.
pub(crate) fn validate_image_path(value: Option<&str>) -> AppResult<Option<String>> {
    let trimmed = match value.map(str::trim) {
        None | Some("") => return Ok(None),
        Some(v) => v,
    };
    // Anything with a scheme is treated as a URL and must be http(s). A bare
    // path (including a Windows drive letter) is a local file and passes.
    let looks_like_url =
        trimmed.contains("://") || (trimmed.contains(':') && !is_windows_drive_path(trimmed));
    if looks_like_url {
        let lower = trimmed.to_ascii_lowercase();
        if !(lower.starts_with("http://") || lower.starts_with("https://")) {
            return Err(AppError::Validation(
                "Product image links must start with http:// or https://".into(),
            ));
        }
        if trimmed.len() > 2_048 {
            return Err(AppError::Validation(
                "Product image link is too long".into(),
            ));
        }
    }
    Ok(Some(trimmed.to_string()))
}

/// `C:\images\milk.png` has a colon but is a path, not a URL.
fn is_windows_drive_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() > 2
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
}

#[tauri::command]
pub async fn admin_list_products(
    actor_user_id: String,
    search: Option<String>,
    category_id: Option<String>,
    offset: Option<i64>,
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> Result<AdminProductPage, AppError> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;
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
                  OR p.barcode LIKE '%' || ? || '%' \
                  OR EXISTS (SELECT 1 FROM product_barcodes pb WHERE pb.product_id = p.product_id AND pb.barcode LIKE '%' || ? || '%'))",
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
                q = q.bind(s).bind(s).bind(s).bind(s);
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

/// Search is a manager catalogue action. It is intentionally server-side:
/// ZANPOS keeps a strict frontend CSP and never exposes arbitrary HTTP fetching
/// to the WebView.
#[tauri::command]
pub async fn admin_search_product_image(
    actor_user_id: String,
    request: ProductImageSearchRequest,
    state: State<'_, AppState>,
) -> Result<ProductImageSearchResult, AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    search_product_image(request).await
}

/// Update only the image column. This must stay separate from
/// `admin_update_product`: a catalogue-row image fetch does not carry the full
/// form and therefore must never null cost, description, supplier or tax data.
pub(crate) async fn persist_product_image(
    pool: &SqlitePool,
    product_id: &str,
    image_url: &str,
) -> AppResult<Option<String>> {
    let image_url = validate_image_path(Some(image_url))?
        .ok_or_else(|| AppError::Validation("Product image URL is required".into()))?;
    if !(image_url.starts_with("https://") || image_url.starts_with("http://")) {
        return Err(AppError::Validation(
            "Fetched product images must use an http:// or https:// URL".into(),
        ));
    }
    let previous = sqlx::query_scalar::<_, Option<String>>(
        "SELECT image_path FROM products WHERE product_id = ? AND deleted_at IS NULL",
    )
    .bind(product_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound("Product not found".into()))?;

    sqlx::query(
        "UPDATE products
         SET image_path = ?, updated_at = ?, version = version + 1,
             sync_status = 'pending', sync_attempts = 0
         WHERE product_id = ? AND deleted_at IS NULL",
    )
    .bind(image_url)
    .bind(chrono::Utc::now().to_rfc3339())
    .bind(product_id)
    .execute(pool)
    .await?;
    Ok(previous)
}

#[tauri::command]
pub async fn admin_set_product_image(
    actor_user_id: String,
    product_id: String,
    image_url: String,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    let previous = persist_product_image(&state.db, &product_id, &image_url).await?;
    sync_commands::schedule_immediate_sync(&state);

    let device_id = active_device_id(&state).await;
    let branch_id = active_branch_id(&state).await?;
    let before = serde_json::json!({ "image_path": previous }).to_string();
    let after = serde_json::json!({ "image_path": image_url }).to_string();
    if let Err(error) = audit_hash::insert_audit_entry(
        &state.db,
        "PRODUCT_IMAGE_UPDATED",
        "product",
        &product_id,
        &actor_user_id,
        "user",
        &device_id,
        &branch_id,
        Some(&before),
        Some(&after),
        Some("Product image fetched from an external catalogue search"),
    )
    .await
    {
        tracing::error!("AUDIT WRITE FAILED [PRODUCT_IMAGE_UPDATED]: {:?}", error);
    }
    Ok(())
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

    // IPC input validation — enforce bounds before any DB write
    let name_trimmed = input.name.trim();
    if name_trimmed.is_empty() {
        return Err(AppError::Validation("Product name is required".into()));
    }
    if name_trimmed.len() > 255 {
        return Err(AppError::Validation(
            "Product name must not exceed 255 characters".into(),
        ));
    }
    if let Some(ref sku) = input.sku {
        if sku.trim().len() > 100 {
            return Err(AppError::Validation(
                "SKU must not exceed 100 characters".into(),
            ));
        }
    }
    if let Some(ref bc) = input.barcode {
        if bc.trim().len() > 100 {
            return Err(AppError::Validation(
                "Barcode must not exceed 100 characters".into(),
            ));
        }
    }

    // BUG-PRODUCTS-4: reject zero or negative prices
    if input.price_minor <= 0 {
        return Err(AppError::Validation(
            "Product price must be greater than zero".into(),
        ));
    }

    let product_id = Ulid::new().to_string();
    let price_id = Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    // BUG-PRODUCTS-13: wrap product + price + stock inserts in one transaction
    // so a price-insert failure doesn't leave a price-less product in the DB.
    let mut tx = state.db.begin().await?;

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
    .bind(validate_image_path(input.image_path.as_deref())?)
    .bind(input.cost_minor)
    .bind(input.description.as_deref().filter(|s| !s.is_empty()))
    .bind(
        input
            .default_supplier_id
            .as_deref()
            .filter(|s| !s.is_empty()),
    )
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        if e.to_string().contains("UNIQUE") || e.to_string().contains("barcode already in use") {
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
    .execute(&mut *tx)
    .await?;

    if input.track_inventory {
        let branch_id = active_branch_id(&state).await?;
        let sl_id = format!("SL-{}-{}", product_id, branch_id);
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
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    // sync_status='pending' is set by column DEFAULT — sync worker picks it up
    sync_commands::schedule_immediate_sync(&state);

    let device_id = active_device_id(&state).await;
    let branch_id = active_branch_id(&state).await?;

    // H8: Audit log — product created
    let after = serde_json::json!({
        "product_id": product_id, "name": input.name,
        "category_id": input.category_id, "sku": input.sku,
        "price_minor": input.price_minor, "is_active": true,
    })
    .to_string();
    if let Err(e) = audit_hash::insert_audit_entry(
        &state.db,
        "PRODUCT_CREATED",
        "product",
        &product_id,
        &input.created_by_user_id,
        "user",
        &device_id,
        &branch_id,
        None,
        Some(&after),
        None,
    )
    .await
    {
        tracing::error!("AUDIT WRITE FAILED [PRODUCT_CREATED]: {:?}", e);
    }

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

    // IPC input validation — enforce bounds before any DB write
    let name_trimmed = input.name.trim();
    if name_trimmed.is_empty() {
        return Err(AppError::Validation("Product name is required".into()));
    }
    if name_trimmed.len() > 255 {
        return Err(AppError::Validation(
            "Product name must not exceed 255 characters".into(),
        ));
    }
    if let Some(ref sku) = input.sku {
        if sku.trim().len() > 100 {
            return Err(AppError::Validation(
                "SKU must not exceed 100 characters".into(),
            ));
        }
    }
    if let Some(ref bc) = input.barcode {
        if bc.trim().len() > 100 {
            return Err(AppError::Validation(
                "Barcode must not exceed 100 characters".into(),
            ));
        }
    }

    // BUG-PRODUCTS-4: reject zero or negative prices on update too
    if input.price_minor <= 0 {
        return Err(AppError::Validation(
            "Product price must be greater than zero".into(),
        ));
    }

    let now = chrono::Utc::now().to_rfc3339();

    // Resolve branch/device IDs before the transaction
    let branch_id = active_branch_id(&state).await?;
    let device_id = active_device_id(&state).await;

    // Check if price changed — fetch full old price row for H-8 re-enqueue
    // (read before the transaction so we know the current state)
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

    // BUG-PRODUCTS-13: wrap product update + price close/insert + stock init
    // in one transaction so a partial write doesn't leave inconsistent state
    // (e.g. product row updated but no valid price row).
    let mut tx = state.db.begin().await?;

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
    .bind(validate_image_path(input.image_path.as_deref())?)
    .bind(input.cost_minor)
    .bind(input.description.as_deref().filter(|s| !s.is_empty()))
    .bind(
        input
            .default_supplier_id
            .as_deref()
            .filter(|s| !s.is_empty()),
    )
    .bind(&now)
    .bind(&input.product_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        if e.to_string().contains("UNIQUE") || e.to_string().contains("barcode already in use") {
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
        .execute(&mut *tx)
        .await?;

        // Insert new price (sync_status defaults to 'pending')
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
        .execute(&mut *tx)
        .await?;
    }

    if input.track_inventory {
        let sl_id = format!("SL-{}-{}", input.product_id, branch_id);
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
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    // sync_status='pending' is set by column DEFAULT — sync worker picks it up
    sync_commands::schedule_immediate_sync(&state);

    // H8: Audit log — product updated
    let after = serde_json::json!({
        "product_id": input.product_id, "name": input.name,
        "category_id": input.category_id, "sku": input.sku,
        "price_minor": input.price_minor, "is_active": input.is_active,
    })
    .to_string();
    if let Err(e) = audit_hash::insert_audit_entry(
        &state.db,
        "PRODUCT_UPDATED",
        "product",
        &input.product_id,
        &input.updated_by_user_id,
        "user",
        &device_id,
        &branch_id,
        None,
        Some(&after),
        None,
    )
    .await
    {
        tracing::error!("AUDIT WRITE FAILED [PRODUCT_UPDATED]: {:?}", e);
    }

    let sql = format!("{} WHERE p.product_id = ?", ADMIN_PRODUCT_QUERY);
    let row = sqlx::query(&sql)
        .bind(&input.product_id)
        .fetch_one(&state.db)
        .await?;
    Ok(row_to_admin_product(&row))
}

// ─── Duplicate-product detection & resolution ─────────────────────────────────

/// Scan the whole catalog and return duplicate groups (by name, barcode, SKU)
/// for the back-office "Duplicate Products" triage screen. Manager/owner only.
#[tauri::command]
pub async fn admin_find_duplicate_products(
    actor_user_id: String,
    include_inactive: Option<bool>,
    state: State<'_, AppState>,
) -> Result<Vec<product_dedup_repo::DuplicateGroup>, AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    product_dedup_repo::find_duplicate_groups(&state.db, include_inactive.unwrap_or(false)).await
}

/// Merge one duplicate product into another: combine stock, transfer movements,
/// optionally reassign sale history, then archive the source. Manager/owner only.
#[tauri::command]
pub async fn admin_merge_products(
    actor_user_id: String,
    source_product_id: String,
    target_product_id: String,
    transfer_history: Option<bool>,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    let transfer = transfer_history.unwrap_or(false);
    let outcome = product_dedup_repo::merge_products(
        &state.db,
        &source_product_id,
        &target_product_id,
        transfer,
    )
    .await?;

    let device_id = active_device_id(&state).await;
    let branch_id = active_branch_id(&state).await?;
    let after = serde_json::json!({
        "source_product_id": source_product_id,
        "target_product_id": target_product_id,
        "source_name": outcome.source_name,
        "target_name": outcome.target_name,
        "transfer_history": transfer,
    })
    .to_string();
    if let Err(e) = audit_hash::insert_audit_entry(
        &state.db,
        "PRODUCT_MERGED",
        "product",
        &source_product_id,
        &actor_user_id,
        "user",
        &device_id,
        &branch_id,
        None,
        Some(&after),
        None,
    )
    .await
    {
        tracing::error!("AUDIT WRITE FAILED [PRODUCT_MERGED]: {:?}", e);
    }
    sync_commands::schedule_immediate_sync(&state);
    Ok(())
}

/// Soft-delete (archive) a single product — used to resolve a duplicate by
/// dropping the redundant entry rather than merging. Manager/owner only.
#[tauri::command]
pub async fn admin_delete_product(
    actor_user_id: String,
    product_id: String,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    rbac::manager_or_owner(&state.db, &actor_user_id).await?;
    let name = product_dedup_repo::soft_delete_product(&state.db, &product_id).await?;

    let device_id = active_device_id(&state).await;
    let branch_id = active_branch_id(&state).await?;
    let after = serde_json::json!({ "product_id": product_id, "name": name }).to_string();
    if let Err(e) = audit_hash::insert_audit_entry(
        &state.db,
        "PRODUCT_DELETED",
        "product",
        &product_id,
        &actor_user_id,
        "user",
        &device_id,
        &branch_id,
        None,
        Some(&after),
        None,
    )
    .await
    {
        tracing::error!("AUDIT WRITE FAILED [PRODUCT_DELETED]: {:?}", e);
    }
    sync_commands::schedule_immediate_sync(&state);
    Ok(())
}

// ─── Category commands ────────────────────────────────────────────────────────

#[tauri::command]
pub async fn admin_list_categories(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<CategoryRow>, AppError> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;
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
pub async fn admin_list_tax_rules(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<TaxRuleRow>, AppError> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;
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
        return Err(AppError::Validation(
            "Rate must be between 0% and 100%".into(),
        ));
    }
    let rate_basis_points = input.rate_basis_points;
    let now = chrono::Utc::now().to_rfc3339();
    let is_tax_update = input.tax_rule_id.is_some();

    let tax_rule_id = if let Some(ref id) = input.tax_rule_id {
        // Update existing tax rule in-place — sale_items.tax_rule_snapshot
        // preserves the rate that was used at sale time, so in-place
        // updates don't corrupt historical data.
        sqlx::query(
            "UPDATE tax_rules SET name=?, rate_basis_points=?, inclusive=?, is_active=?,
             updated_at=?, version = version + 1, sync_status='pending'
             WHERE tax_rule_id=?",
        )
        .bind(&name)
        .bind(rate_basis_points)
        .bind(input.inclusive as i64)
        .bind(input.is_active as i64)
        .bind(&now)
        .bind(id)
        .execute(&state.db)
        .await?;
        id.clone()
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
    let tax_event = if is_tax_update {
        "TAX_RULE_UPDATED"
    } else {
        "TAX_RULE_CREATED"
    };
    let after = serde_json::json!({
        "tax_rule_id": result.tax_rule_id, "name": result.name,
        "rate_basis_points": result.rate_basis_points, "inclusive": result.inclusive,
        "is_active": result.is_active,
    })
    .to_string();
    if let Err(e) = audit_hash::insert_audit_entry(
        &state.db,
        tax_event,
        "tax_rule",
        &result.tax_rule_id,
        &input.actor_user_id,
        "user",
        &device_id,
        &branch_id,
        None,
        Some(&after),
        None,
    )
    .await
    {
        tracing::error!("AUDIT WRITE FAILED [TAX_RULE]: {:?}", e);
    }

    sync_commands::schedule_immediate_sync(&state);
    Ok(result)
}

/// Delete a tax rule (soft-delete by setting is_active=0, sync_status='pending').
/// Sync worker pushes the deactivation to Supabase so other terminals pick it up.
#[derive(Deserialize)]
pub struct DeleteTaxRuleInput {
    pub tax_rule_id: String,
    pub actor_user_id: String,
}

#[tauri::command]
pub async fn admin_delete_tax_rule(
    input: DeleteTaxRuleInput,
    state: State<'_, AppState>,
) -> Result<(), AppError> {
    rbac::manager_or_owner(&state.db, &input.actor_user_id).await?;

    let now = chrono::Utc::now().to_rfc3339();
    let affected = sqlx::query(
        "UPDATE tax_rules SET is_active = 0, updated_at = ?, sync_status = 'pending',
         version = version + 1
         WHERE tax_rule_id = ?",
    )
    .bind(&now)
    .bind(&input.tax_rule_id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound("Tax rule not found".into()));
    }

    // H8: Audit log — tax rule deleted
    let device_id = active_device_id(&state).await;
    let branch_id = active_branch_id(&state).await?;
    let after = serde_json::json!({
        "tax_rule_id": input.tax_rule_id, "is_active": false, "deleted": true,
    })
    .to_string();
    if let Err(e) = audit_hash::insert_audit_entry(
        &state.db,
        "TAX_RULE_DELETED",
        "tax_rule",
        &input.tax_rule_id,
        &input.actor_user_id,
        "user",
        &device_id,
        &branch_id,
        None,
        Some(&after),
        None,
    )
    .await
    {
        tracing::error!("AUDIT WRITE FAILED [TAX_RULE_DELETED]: {:?}", e);
    }

    sync_commands::schedule_immediate_sync(&state);
    Ok(())
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
        // Guard: validate parent exists and check for circular hierarchy.
        if let Some(ref parent_id) = input.parent_category_id {
            // Self-parent is always a cycle.
            if parent_id == &id {
                return Err(AppError::Validation(
                    "A category cannot be its own parent".into(),
                ));
            }
            // Verify the parent category exists
            let parent_exists: bool = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM categories WHERE category_id = ?",
            )
            .bind(parent_id)
            .fetch_one(&state.db)
            .await
            .unwrap_or(0)
                > 0;
            if !parent_exists {
                return Err(AppError::Validation(
                    "Parent category does not exist".into(),
                ));
            }
            // Recursive CTE: walk ancestors of the proposed parent. If the
            // category being edited appears anywhere in that chain it would
            // create a cycle.
            let cycle_found: bool = sqlx::query_scalar::<_, i64>(
                "WITH RECURSIVE ancestors(cat_id) AS (
                     SELECT parent_category_id FROM categories WHERE category_id = ?
                     UNION ALL
                     SELECT c.parent_category_id
                     FROM categories c
                     JOIN ancestors a ON c.category_id = a.cat_id
                     WHERE a.cat_id IS NOT NULL
                 )
                 SELECT EXISTS(SELECT 1 FROM ancestors WHERE cat_id = ?)",
            )
            .bind(parent_id)
            .bind(&id)
            .fetch_one(&state.db)
            .await
            .unwrap_or(0)
                != 0;

            if cycle_found {
                return Err(AppError::Validation(
                    "Setting this parent would create a circular category hierarchy".into(),
                ));
            }
        }

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
        // Validate parent_category_id exists before creating the new category
        if let Some(ref parent_id) = input.parent_category_id {
            let parent_exists: bool = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM categories WHERE category_id = ?",
            )
            .bind(parent_id)
            .fetch_one(&state.db)
            .await
            .unwrap_or(0)
                > 0;
            if !parent_exists {
                return Err(AppError::Validation(
                    "Parent category does not exist".into(),
                ));
            }
        }
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
    let event = if is_update {
        "CATEGORY_UPDATED"
    } else {
        "CATEGORY_CREATED"
    };
    let after = serde_json::json!({
        "category_id": result.category_id, "name": result.name,
        "sort_order": result.sort_order, "is_active": result.is_active,
        "parent_category_id": result.parent_category_id,
    })
    .to_string();
    if let Err(e) = audit_hash::insert_audit_entry(
        &state.db,
        event,
        "category",
        &result.category_id,
        &input.actor_user_id,
        "user",
        &device_id,
        &branch_id,
        None,
        Some(&after),
        None,
    )
    .await
    {
        tracing::error!("AUDIT WRITE FAILED [CATEGORY]: {:?}", e);
    }

    sync_commands::schedule_immediate_sync(&state);
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
    let max_order: i64 =
        sqlx::query_scalar::<_, i64>("SELECT COALESCE(MAX(sort_order), 0) FROM categories")
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
            errors.push(BulkRowError {
                row: idx + 1,
                name: row.name.clone(),
                reason: "Name is empty".into(),
            });
            continue;
        }

        // Check duplicate by name (case-insensitive)
        let exists: bool = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM categories WHERE lower(name) = lower(?)",
        )
        .bind(name)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0)
            > 0;

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
    if inserted > 0 {
        sync_commands::schedule_immediate_sync(&state);
    }

    Ok(BulkImportResult {
        inserted,
        skipped,
        errors,
    })
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
            errors.push(BulkRowError {
                row: idx + 1,
                name: row.name.clone(),
                reason: "Name is empty".into(),
            });
            continue;
        }

        // Parse price — decimal string → minor integer (no float)
        let price_minor = match crate::domain::money::parse_major_to_minor(&row.price, 3) {
            Some(v) if v <= 0 => {
                errors.push(BulkRowError {
                    row: idx + 1,
                    name: row.name.clone(),
                    reason: "Price must be greater than zero".into(),
                });
                continue;
            }
            Some(v) if v <= 999_999_000 => v, // max 999.999 BHD
            Some(_) => {
                errors.push(BulkRowError {
                    row: idx + 1,
                    name: row.name.clone(),
                    reason: format!("Price out of range: {}", row.price),
                });
                continue;
            }
            None => {
                errors.push(BulkRowError {
                    row: idx + 1,
                    name: row.name.clone(),
                    reason: format!("Invalid price: '{}'", row.price),
                });
                continue;
            }
        };

        // Resolve or create category
        let cat_key = row.category_name.trim().to_lowercase();
        if cat_key.is_empty() {
            errors.push(BulkRowError {
                row: idx + 1,
                name: row.name.clone(),
                reason: "category_name is empty".into(),
            });
            continue;
        }
        let category_id = if let Some(id) = cat_map.get(&cat_key) {
            id.clone()
        } else {
            // Auto-create the category
            let new_cat_id = Ulid::new().to_string();
            let max_order: i64 =
                sqlx::query_scalar("SELECT COALESCE(MAX(sort_order), 0) FROM categories")
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
                            errors.push(BulkRowError {
                                row: idx + 1,
                                name: row.name.clone(),
                                reason: format!(
                                    "Could not resolve category '{}'",
                                    row.category_name
                                ),
                            });
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
            bc_str
                .split('|')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect()
        } else if let Some(ref bc) = row.barcode {
            let bc = bc.trim();
            if bc.is_empty() {
                vec![]
            } else {
                vec![bc]
            }
        } else {
            vec![]
        };
        // PRE-VALIDATE all barcodes before any DB insert.
        // Check both product_barcodes AND products.barcode to catch duplicates
        // from all sources (admin_create_product writes to products.barcode only).
        // If any barcode conflicts, skip the entire row — do not create an
        // orphaned product that has no valid barcodes.
        let mut barcode_conflict = false;
        for bc in &all_barcodes {
            let already_exists: bool = sqlx::query_scalar::<_, i64>(
                "SELECT EXISTS(
                    SELECT 1 FROM product_barcodes WHERE barcode = ?
                    UNION ALL
                    SELECT 1 FROM products WHERE barcode = ? AND is_active = 1
                )",
            )
            .bind(bc)
            .bind(bc)
            .fetch_one(&mut *tx)
            .await
            .unwrap_or(0)
                != 0;

            if already_exists {
                errors.push(BulkRowError {
                    row: idx + 1,
                    name: row.name.clone(),
                    reason: format!("Barcode '{}' is already registered to another product", bc),
                });
                barcode_conflict = true;
                break;
            }
        }
        if barcode_conflict {
            skipped += 1;
            continue;
        }

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
                errors.push(BulkRowError {
                    row: idx + 1,
                    name: row.name.clone(),
                    reason: format!("DB error: {}", e),
                });
                continue;
            }
            Ok(_) => {}
        }

        // Insert all barcodes into product_barcodes table.
        // Duplicates were already checked in the pre-validation block above,
        // so we can insert without re-checking here.
        for bc in &all_barcodes {
            let bc_id = format!("BC-{}", Ulid::new());
            if let Err(e) = sqlx::query(
                "INSERT INTO product_barcodes
                   (barcode_id, product_id, barcode, created_at)
                 VALUES (?,?,?,?)",
            )
            .bind(&bc_id)
            .bind(&product_id)
            .bind(bc)
            .bind(&now)
            .execute(&mut *tx)
            .await
            {
                errors.push(BulkRowError {
                    row: idx + 1,
                    name: row.name.clone(),
                    reason: format!("Barcode insert failed for '{}': {}", bc, e),
                });
            }
        }

        // Insert price — critical: product without price cannot be sold; capture failure (T07)
        if let Err(e) = sqlx::query(
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
        .await
        {
            errors.push(BulkRowError {
                row: idx + 1,
                name: row.name.clone(),
                reason: format!("Price write failed: {}", e),
            });
            continue;
        }

        // Init stock level (INSERT OR IGNORE — duplicate safe)
        if track {
            let sl_id = format!("SL-{}-{}", product_id, branch_id);
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
    sync_commands::schedule_immediate_sync(&state);

    Ok(BulkImportResult {
        inserted,
        skipped,
        errors,
    })
}

// ─── User commands ────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn admin_list_users_all(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<AdminUserRow>, AppError> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;
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
pub async fn admin_list_roles(
    actor_user_id: String,
    state: State<'_, AppState>,
) -> Result<Vec<RoleRow>, AppError> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;
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

/// Guard for user-administration writes.
///
/// `manager_or_owner` alone is not enough for anything that touches a role.
/// `admin_update_user` writes `role_id` straight from its input, so a manager
/// could previously grant the owner role — to another user or to themselves —
/// and could deactivate the real owner. Both are privilege escalation performed
/// entirely through a permitted command; hiding the control in the UI is not a
/// defence, because the command is reachable directly.
///
/// The rules, using the roles that already exist:
///   * only an owner may grant or remove the owner role;
///   * only an owner may modify an account that is currently an owner;
///   * the target must be in the actor's branch.
///
/// Everything else a manager could already do is unchanged.
async fn authorize_user_admin(
    pool: &SqlitePool,
    actor_user_id: &str,
    target_user_id: Option<&str>,
    target_role_id: &str,
) -> Result<(), AppError> {
    rbac::manager_or_owner(pool, actor_user_id).await?;

    let actor_is_owner = rbac::owner_only(pool, actor_user_id).await.is_ok();
    let actor_branch = rbac::actor_branch_id(pool, actor_user_id).await?;

    let target_role_is_owner: bool =
        sqlx::query_scalar("SELECT name = 'owner' FROM roles WHERE role_id = ?")
            .bind(target_role_id)
            .fetch_optional(pool)
            .await?
            .unwrap_or(false);
    if target_role_is_owner && !actor_is_owner {
        return Err(AppError::Permission(
            "Only an owner can grant the owner role".into(),
        ));
    }

    if let Some(user_id) = target_user_id {
        let existing: Option<(String, bool)> = sqlx::query_as(
            "SELECT u.branch_id, r.name = 'owner'
             FROM users u JOIN roles r ON r.role_id = u.role_id
             WHERE u.user_id = ?",
        )
        .bind(user_id)
        .fetch_optional(pool)
        .await?;
        let (target_branch, target_is_owner) =
            existing.ok_or_else(|| AppError::NotFound(format!("User {user_id} not found")))?;

        if target_branch != actor_branch {
            // NotFound rather than Permission: confirming the account exists in
            // another branch is itself a disclosure.
            return Err(AppError::NotFound(format!("User {user_id} not found")));
        }
        if target_is_owner && !actor_is_owner {
            return Err(AppError::Permission(
                "Only an owner can modify an owner account".into(),
            ));
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn admin_create_user(
    input: CreateUserInput,
    state: State<'_, AppState>,
) -> Result<AdminUserRow, AppError> {
    authorize_user_admin(&state.db, &input.actor_user_id, None, &input.role_id).await?;
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
    })
    .to_string();
    if let Err(e) = audit_hash::insert_audit_entry(
        &state.db,
        "USER_CREATED",
        "user",
        &result.user_id,
        &input.actor_user_id,
        "user",
        &device_id,
        &branch_id,
        None,
        Some(&after),
        None,
    )
    .await
    {
        tracing::error!("AUDIT WRITE FAILED [USER_CREATED]: {:?}", e);
    }

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
    let barcode = barcode.trim().to_string();
    if barcode.is_empty() {
        return Err(AppError::Validation("Barcode is required".into()));
    }
    if barcode.len() > 100 {
        return Err(AppError::Validation(
            "Barcode must not exceed 100 characters".into(),
        ));
    }
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
        if e.to_string().contains("UNIQUE") || e.to_string().contains("barcode already in use") {
            AppError::Conflict("Barcode already registered to another product".into())
        } else {
            e.into()
        }
    })?;

    // M-11: Audit trail for barcode add
    let device_id = active_device_id(&state).await;
    let branch_id = active_branch_id(&state).await?;
    let after = serde_json::json!({
        "barcode_id": barcode_id, "product_id": product_id, "barcode": barcode,
    })
    .to_string();
    if let Err(e) = audit_hash::insert_audit_entry(
        &state.db,
        "BARCODE_ADDED",
        "product",
        &product_id,
        &actor_user_id,
        "user",
        &device_id,
        &branch_id,
        None,
        Some(&after),
        None,
    )
    .await
    {
        tracing::error!("AUDIT WRITE FAILED [BARCODE_ADDED]: {:?}", e);
    }
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
    let record =
        sqlx::query("SELECT product_id, barcode FROM product_barcodes WHERE barcode_id = ?")
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
        if let Err(e) = audit_hash::insert_audit_entry(
            &state.db,
            "BARCODE_REMOVED",
            "product",
            &product_id,
            &actor_user_id,
            "user",
            &device_id,
            &branch_id,
            None,
            Some(&after),
            None,
        )
        .await
        {
            tracing::error!("AUDIT WRITE FAILED [BARCODE_REMOVED]: {:?}", e);
        }
    }

    Ok(())
}

#[tauri::command]
pub async fn product_barcodes_list(
    actor_user_id: String,
    product_id: String,
    state: State<'_, AppState>,
) -> AppResult<Vec<ProductBarcodeRow>> {
    rbac::require_any_role(&state.db, &actor_user_id).await?;
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
    authorize_user_admin(
        &state.db,
        &input.actor_user_id,
        Some(&input.user_id),
        &input.role_id,
    )
    .await?;
    // FIX: prevent self-demotion or self-deactivation — an owner who deactivates
    // themselves locks out the system permanently.
    if input.user_id == input.actor_user_id && !input.is_active {
        return Err(AppError::Validation(
            "You cannot deactivate your own account".into(),
        ));
    }
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
    })
    .to_string();
    if let Err(e) = audit_hash::insert_audit_entry(
        &state.db,
        "USER_UPDATED",
        "user",
        &result.user_id,
        &input.actor_user_id,
        "user",
        &device_id,
        &branch_id,
        None,
        Some(&after),
        None,
    )
    .await
    {
        tracing::error!("AUDIT WRITE FAILED [USER_UPDATED]: {:?}", e);
    }

    Ok(result)
}

// ─── Diagnostics & auto-fix ────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct DiagnosticReport {
    pub ok: bool,
    pub db_integrity: String,
    pub issues_found: Vec<String>,
    pub issues_fixed: Vec<String>,
    pub note: String,
}

#[tauri::command]
pub async fn admin_run_diagnostics(state: State<'_, AppState>) -> AppResult<DiagnosticReport> {
    let mut issues_found: Vec<String> = Vec::new();
    let mut issues_fixed: Vec<String> = Vec::new();

    // 1. Database integrity check
    let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&state.db)
        .await
        .unwrap_or_else(|_| "check failed".into());

    let db_ok = integrity == "ok";
    if !db_ok {
        issues_found.push(format!("Database integrity issue: {}", integrity));
    }

    // 2. Check for stuck AI runs (status='running' but no progress in >5 min)
    let stuck_rows = sqlx::query(
        "SELECT run_id, op_id FROM ai_runs WHERE status='running' AND updated_at < datetime('now','-5 minutes')"
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    let stuck_runs: Vec<(String, String)> = stuck_rows
        .iter()
        .map(|row| (row.get(0), row.get(1)))
        .collect();

    if !stuck_runs.is_empty() {
        issues_found.push(format!("{} stuck AI run(s) detected", stuck_runs.len()));
        for (run_id, _op_id) in &stuck_runs {
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query("UPDATE ai_runs SET status='failed', error='Auto-cleared by diagnostics (stuck)', updated_at=? WHERE run_id=?")
                .bind(&now)
                .bind(run_id)
                .execute(&state.db)
                .await?;
            issues_fixed.push(format!("Cleared stuck AI run {}", run_id));
        }
    }

    // 3. Check for stuck actions (status='executing' with no resolution)
    let stuck_action_rows = sqlx::query(
        "SELECT action_id, action_type FROM ai_actions WHERE status='executing' AND created_at < datetime('now','-10 minutes')"
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    let stuck_actions: Vec<(String, String)> = stuck_action_rows
        .iter()
        .map(|row| (row.get(0), row.get(1)))
        .collect();

    if !stuck_actions.is_empty() {
        issues_found.push(format!(
            "{} stuck AI action(s) detected",
            stuck_actions.len()
        ));
        for (action_id, _action_type) in &stuck_actions {
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query("UPDATE ai_actions SET status='failed', error_message='Auto-cleared by diagnostics (stuck)', completed_at=? WHERE action_id=?")
                .bind(&now)
                .bind(action_id)
                .execute(&state.db)
                .await?;
            issues_fixed.push(format!("Cleared stuck AI action {}", action_id));
        }
    }

    let ok = db_ok && issues_found.is_empty();
    let note = if ok {
        "All systems healthy — no issues found.".into()
    } else if issues_fixed.len() >= issues_found.len() {
        format!("Found and fixed {} issue(s).", issues_fixed.len())
    } else {
        format!("Found {} issue(s), {} could be auto-fixed. Database integrity check may require a restart if corruption is detected.", issues_found.len(), issues_fixed.len())
    };

    Ok(DiagnosticReport {
        ok,
        db_integrity: integrity,
        issues_found,
        issues_fixed,
        note,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- Team user administration (D1) ----------------------------------------

    const ROLE_OWNER: &str = "01JROLES000000000000000001";
    const ROLE_MANAGER: &str = "01JROLES000000000000000002";
    const ROLE_CASHIER: &str = "01JROLES000000000000000003";
    const BRANCH_MAIN: &str = "01JBRANCH0000000000000001";
    const BRANCH_OTHER: &str = "01JBRANCH0000000000000009";

    async fn team_pool() -> SqlitePool {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("pool");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("migrations");
        sqlx::query("UPDATE branches SET is_active = 1 WHERE branch_id = ?")
            .bind(BRANCH_MAIN)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT OR IGNORE INTO branches
               (branch_id, branch_code, name, is_active, created_at, updated_at, version)
             VALUES (?, 'OTH', 'Other', 1, datetime('now'), datetime('now'), 1)",
        )
        .bind(BRANCH_OTHER)
        .execute(&pool)
        .await
        .unwrap();

        for (id, branch, role, uname) in [
            ("u_owner", BRANCH_MAIN, ROLE_OWNER, "owner1"),
            ("u_manager", BRANCH_MAIN, ROLE_MANAGER, "mgr1"),
            ("u_cashier", BRANCH_MAIN, ROLE_CASHIER, "cash1"),
            ("u_other_branch", BRANCH_OTHER, ROLE_CASHIER, "cash2"),
        ] {
            sqlx::query(
                "INSERT INTO users (user_id, branch_id, display_name, username, pin_hash,
                                    role_id, is_active, created_at, updated_at, version)
                 VALUES (?, ?, ?, ?, 'PLAIN:1234', ?, 1, datetime('now'), datetime('now'), 1)",
            )
            .bind(id)
            .bind(branch)
            .bind(uname)
            .bind(uname)
            .bind(role)
            .execute(&pool)
            .await
            .unwrap();
        }
        pool
    }

    #[tokio::test]
    async fn a_manager_cannot_grant_the_owner_role() {
        let pool = team_pool().await;
        // Promoting a cashier to owner -- straightforward escalation.
        let err = authorize_user_admin(&pool, "u_manager", Some("u_cashier"), ROLE_OWNER)
            .await
            .expect_err("a manager must not mint owners");
        assert!(matches!(err, AppError::Permission(_)), "got {err:?}");

        // And promoting themselves, which is the same hole from the inside.
        let err = authorize_user_admin(&pool, "u_manager", Some("u_manager"), ROLE_OWNER)
            .await
            .expect_err("a manager must not promote themselves");
        assert!(matches!(err, AppError::Permission(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn a_manager_cannot_touch_an_owner_account() {
        let pool = team_pool().await;
        // Even demoting an owner to cashier -- the target's current role is
        // what matters, not the role being written.
        let err = authorize_user_admin(&pool, "u_manager", Some("u_owner"), ROLE_CASHIER)
            .await
            .expect_err("a manager must not modify an owner");
        assert!(matches!(err, AppError::Permission(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn an_owner_may_still_administer_everyone() {
        let pool = team_pool().await;
        authorize_user_admin(&pool, "u_owner", Some("u_cashier"), ROLE_OWNER)
            .await
            .expect("an owner may grant the owner role");
        authorize_user_admin(&pool, "u_owner", Some("u_manager"), ROLE_CASHIER)
            .await
            .expect("an owner may change any role");
    }

    #[tokio::test]
    async fn a_manager_may_still_do_ordinary_team_work() {
        let pool = team_pool().await;
        // The fix must not break what managers legitimately did before.
        authorize_user_admin(&pool, "u_manager", Some("u_cashier"), ROLE_CASHIER)
            .await
            .expect("a manager may edit a cashier");
        authorize_user_admin(&pool, "u_manager", None, ROLE_MANAGER)
            .await
            .expect("a manager may create a manager");
    }

    #[tokio::test]
    async fn user_administration_cannot_cross_a_branch() {
        let pool = team_pool().await;
        let err = authorize_user_admin(&pool, "u_manager", Some("u_other_branch"), ROLE_CASHIER)
            .await
            .expect_err("cross-branch mutation must be refused");
        // NotFound, not Permission -- confirming the account exists elsewhere
        // is itself a disclosure.
        assert!(matches!(err, AppError::NotFound(_)), "got {err:?}");

        let err = authorize_user_admin(&pool, "u_owner", Some("u_other_branch"), ROLE_CASHIER)
            .await
            .expect_err("even an owner is scoped to their own branch here");
        assert!(matches!(err, AppError::NotFound(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn a_cashier_or_forged_actor_is_refused_outright() {
        let pool = team_pool().await;
        for actor in ["u_cashier", "nobody", "' OR 1=1 --", ""] {
            assert!(
                authorize_user_admin(&pool, actor, Some("u_cashier"), ROLE_CASHIER)
                    .await
                    .is_err(),
                "{actor:?} must not administer users"
            );
        }
    }

    #[tokio::test]
    async fn a_deactivated_manager_loses_user_administration() {
        let pool = team_pool().await;
        sqlx::query("UPDATE users SET is_active = 0 WHERE user_id = 'u_manager'")
            .execute(&pool)
            .await
            .unwrap();
        assert!(
            authorize_user_admin(&pool, "u_manager", Some("u_cashier"), ROLE_CASHIER)
                .await
                .is_err(),
            "a deactivated account keeps no authority"
        );
    }
}

#[cfg(test)]
mod image_path_tests {
    use super::{persist_product_image, validate_image_path};
    use sqlx::Row;

    #[test]
    fn accepts_https_and_http_links() {
        assert_eq!(
            validate_image_path(Some("https://cdn.example.com/milk.jpg")).unwrap(),
            Some("https://cdn.example.com/milk.jpg".to_string())
        );
        assert!(validate_image_path(Some("http://example.com/a.png")).is_ok());
    }

    #[test]
    fn rejects_schemes_that_are_not_web_links() {
        // These are the values that would turn a catalogue field into a way of
        // smuggling bytes or reading local files on every screen that renders it.
        for bad in [
            "javascript:alert(1)",
            "data:image/svg+xml;base64,AAAA",
            "file:///etc/passwd",
            "ftp://example.com/a.png",
        ] {
            assert!(
                validate_image_path(Some(bad)).is_err(),
                "should reject {bad}"
            );
        }
    }

    #[test]
    fn keeps_local_paths_including_windows_drives() {
        for good in [
            "C:\\images\\milk.png",
            "/home/super/images/milk.png",
            "images/milk.png",
        ] {
            assert!(
                validate_image_path(Some(good)).is_ok(),
                "should accept {good}"
            );
        }
    }

    #[test]
    fn blank_and_missing_values_clear_the_image() {
        assert_eq!(validate_image_path(None).unwrap(), None);
        assert_eq!(validate_image_path(Some("   ")).unwrap(), None);
    }

    #[test]
    fn rejects_absurdly_long_links() {
        let long = format!("https://example.com/{}.jpg", "a".repeat(2_100));
        assert!(validate_image_path(Some(&long)).is_err());
    }

    #[tokio::test]
    async fn image_only_update_preserves_all_other_product_fields() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO categories
               (category_id, name, sort_order, is_active, created_at, updated_at, version)
             VALUES ('cat-image', 'Dairy', 0, 1, datetime('now'), datetime('now'), 1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO products
               (product_id, category_id, name, sku, barcode, description, cost_minor,
                default_supplier_id, currency, created_at, updated_at, version, sync_status)
             VALUES ('prod-image', 'cat-image', 'Milk', 'MILK-1', '6281007023028',
                     'One litre', 450, 'supplier-1', 'BHD', datetime('now'),
                     datetime('now'), 7, 'synced')",
        )
        .execute(&pool)
        .await
        .unwrap();

        persist_product_image(&pool, "prod-image", "https://cdn.example.com/milk.jpg")
            .await
            .unwrap();

        let row = sqlx::query(
            "SELECT name, sku, barcode, description, cost_minor, default_supplier_id,
                    image_path, version, sync_status
             FROM products WHERE product_id = 'prod-image'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.get::<String, _>("name"), "Milk");
        assert_eq!(row.get::<String, _>("sku"), "MILK-1");
        assert_eq!(row.get::<String, _>("barcode"), "6281007023028");
        assert_eq!(row.get::<String, _>("description"), "One litre");
        assert_eq!(row.get::<i64, _>("cost_minor"), 450);
        assert_eq!(row.get::<String, _>("default_supplier_id"), "supplier-1");
        assert_eq!(
            row.get::<String, _>("image_path"),
            "https://cdn.example.com/milk.jpg"
        );
        assert_eq!(row.get::<i64, _>("version"), 8);
        assert_eq!(row.get::<String, _>("sync_status"), "pending");
    }
}

#[cfg(test)]
mod product_image_search_contract_tests {
    use crate::product_image_search::{
        build_search_query, extract_bing_image_urls, validate_search_identity,
        ProductImageSearchMode,
    };

    #[test]
    fn extracts_unique_public_image_urls_from_bing_async_markup() {
        let html = r#"
          murl&amp;quot;:&amp;quot;https://cdn.example.com/milk-front.jpg&amp;quot;
          murl&quot;:&quot;https://cdn.example.com/milk-front.jpg&quot;
          murl&quot;:&quot;https://cdn.example.com/milk-side.png?size=large&amp;v=2&quot;
          murl&quot;:&quot;http://127.0.0.1/private.jpg&quot;
        "#;

        assert_eq!(
            extract_bing_image_urls(html, None),
            vec![
                "https://cdn.example.com/milk-front.jpg".to_string(),
                "https://cdn.example.com/milk-side.png?size=large&v=2".to_string(),
            ]
        );
    }

    #[test]
    fn change_search_is_more_specific_and_excludes_the_current_image() {
        let fetch = build_search_query(
            "Almarai Full Fat Milk",
            Some("6281007023028"),
            Some("MILK-1L"),
            Some("Dairy"),
            ProductImageSearchMode::Fetch,
        );
        let change = build_search_query(
            "Almarai Full Fat Milk",
            Some("6281007023028"),
            Some("MILK-1L"),
            Some("Dairy"),
            ProductImageSearchMode::Change,
        );

        assert_eq!(fetch, "6281007023028 Almarai Full Fat Milk");
        assert!(change.contains("Dairy"));
        assert!(change.contains("MILK-1L"));
        assert!(change.contains("product packaging front"));

        let html = r#"
          murl&quot;:&quot;https://cdn.example.com/current.jpg&quot;
          murl&quot;:&quot;https://cdn.example.com/replacement.jpg&quot;
        "#;
        assert_eq!(
            extract_bing_image_urls(html, Some("https://cdn.example.com/current.jpg")),
            vec!["https://cdn.example.com/replacement.jpg".to_string()]
        );
    }

    #[test]
    fn image_search_requires_both_product_name_and_barcode() {
        assert!(validate_search_identity("Almarai Full Fat Milk", Some("6281007023028")).is_ok());
        assert!(validate_search_identity("Almarai Full Fat Milk", None).is_err());
        assert!(validate_search_identity("Almarai Full Fat Milk", Some("   ")).is_err());
        assert!(validate_search_identity("", Some("6281007023028")).is_err());
    }
}
