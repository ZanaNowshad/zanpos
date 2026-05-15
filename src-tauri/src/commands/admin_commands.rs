/// Back-office administration commands.
/// Products, categories, tax rules, users, and roles.
use tauri::State;
use sqlx::Row;
use ulid::Ulid;
use serde::{Deserialize, Serialize};
use crate::errors::{AppError, AppResult};
use crate::db::repositories::auth_repo;
use crate::AppState;

const BRANCH_ID: &str = "01JBRANCH0000000000000001";

// ─── Response types ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct AdminProduct {
    pub product_id:             String,
    pub category_id:            String,
    pub category_name:          String,
    pub name:                   String,
    pub sku:                    Option<String>,
    pub barcode:                Option<String>,
    pub track_inventory:        bool,
    pub allow_decimal_quantity: bool,
    pub is_active:              bool,
    pub tax_rule_id:            Option<String>,
    pub tax_rule_name:          Option<String>,
    pub price_minor:            i64,
    pub reorder_point:          i64,
}

#[derive(Debug, Serialize)]
pub struct CategoryRow {
    pub category_id: String,
    pub name:        String,
    pub sort_order:  i64,
    pub is_active:   bool,
}

#[derive(Debug, Serialize)]
pub struct TaxRuleRow {
    pub tax_rule_id:        String,
    pub name:               String,
    pub rate_basis_points:  i64,
    pub inclusive:          bool,
}

#[derive(Debug, Serialize)]
pub struct AdminUserRow {
    pub user_id:       String,
    pub display_name:  String,
    pub username:      String,
    pub role_id:       String,
    pub role_name:     String,
    pub is_active:     bool,
    pub last_login_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RoleRow {
    pub role_id: String,
    pub name:    String,
}

// ─── Shared price join ────────────────────────────────────────────────────────

const ADMIN_PRODUCT_QUERY: &str = r#"
    SELECT p.product_id, p.category_id, c.name AS category_name,
           p.name, p.sku, p.barcode, p.track_inventory, p.allow_decimal_quantity,
           p.is_active, p.tax_rule_id, p.reorder_point,
           t.name AS tax_rule_name,
           COALESCE(pp.price_minor, 0) AS price_minor
    FROM products p
    JOIN categories c ON c.category_id = p.category_id
    LEFT JOIN tax_rules t ON t.tax_rule_id = p.tax_rule_id AND t.is_active = 1
    LEFT JOIN product_prices pp ON pp.product_id = p.product_id
        AND pp.branch_id IS NULL
        AND pp.price_type = 'selling'
        AND pp.effective_from <= datetime('now')
        AND (pp.effective_to IS NULL OR pp.effective_to > datetime('now'))
"#;

fn row_to_admin_product(r: &sqlx::sqlite::SqliteRow) -> AdminProduct {
    let track: i64  = r.get("track_inventory");
    let decimal: i64 = r.get("allow_decimal_quantity");
    let active: i64  = r.get("is_active");
    AdminProduct {
        product_id:             r.get("product_id"),
        category_id:            r.get("category_id"),
        category_name:          r.get("category_name"),
        name:                   r.get("name"),
        sku:                    r.get("sku"),
        barcode:                r.get("barcode"),
        track_inventory:        track != 0,
        allow_decimal_quantity: decimal != 0,
        is_active:              active != 0,
        tax_rule_id:            r.get("tax_rule_id"),
        tax_rule_name:          r.get("tax_rule_name"),
        price_minor:            r.get("price_minor"),
        reorder_point:          r.get("reorder_point"),
    }
}

// ─── Product commands ─────────────────────────────────────────────────────────

#[tauri::command]
pub async fn admin_list_products(state: State<'_, AppState>) -> Result<Vec<AdminProduct>, AppError> {
    let sql = format!("{} ORDER BY p.is_active DESC, p.name", ADMIN_PRODUCT_QUERY);
    let rows = sqlx::query(&sql).fetch_all(&state.db).await?;
    Ok(rows.iter().map(row_to_admin_product).collect())
}

#[derive(Deserialize)]
pub struct CreateProductInput {
    pub category_id:            String,
    pub name:                   String,
    pub sku:                    Option<String>,
    pub barcode:                Option<String>,
    pub tax_rule_id:            Option<String>,
    pub price_minor:            i64,
    pub track_inventory:        bool,
    pub allow_decimal_quantity: bool,
    pub reorder_point:          i64,
    pub created_by_user_id:     String,
}

#[tauri::command]
pub async fn admin_create_product(
    input: CreateProductInput,
    state: State<'_, AppState>,
) -> Result<AdminProduct, AppError> {
    let product_id = Ulid::new().to_string();
    let price_id   = Ulid::new().to_string();
    let now        = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO products
           (product_id, category_id, name, sku, barcode,
            track_inventory, allow_decimal_quantity, is_active,
            tax_rule_id, reorder_point, currency, created_at, updated_at, version)
         VALUES (?,?,?,?,?,?,?,1,?,?,'BHD',?,?,1)"
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
    .bind(&now)
    .bind(&now)
    .execute(&state.db)
    .await
    .map_err(|e| if e.to_string().contains("UNIQUE") {
        AppError::Validation("SKU or barcode already in use".into())
    } else { e.into() })?;

    sqlx::query(
        "INSERT INTO product_prices
           (price_id, product_id, price_type, price_minor, currency,
            effective_from, created_by_user_id, created_at)
         VALUES (?,?,'selling',?,'BHD',?,?,?)"
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
        let sl_id = format!("SL-{}", product_id);
        sqlx::query(
            "INSERT OR IGNORE INTO stock_levels
               (stock_level_id, product_id, branch_id, quantity_on_hand, updated_at)
             VALUES (?,?,?,'0',?)"
        )
        .bind(&sl_id).bind(&product_id).bind(BRANCH_ID).bind(&now)
        .execute(&state.db).await?;
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
    pub product_id:             String,
    pub category_id:            String,
    pub name:                   String,
    pub sku:                    Option<String>,
    pub barcode:                Option<String>,
    pub tax_rule_id:            Option<String>,
    pub price_minor:            i64,
    pub track_inventory:        bool,
    pub allow_decimal_quantity: bool,
    pub reorder_point:          i64,
    pub is_active:              bool,
    pub updated_by_user_id:     String,
}

#[tauri::command]
pub async fn admin_update_product(
    input: UpdateProductInput,
    state: State<'_, AppState>,
) -> Result<AdminProduct, AppError> {
    let now = chrono::Utc::now().to_rfc3339();

    // Check if price changed
    let current_price: Option<i64> = sqlx::query_scalar(
        "SELECT price_minor FROM product_prices
         WHERE product_id = ? AND branch_id IS NULL
           AND price_type = 'selling' AND effective_to IS NULL
         LIMIT 1"
    )
    .bind(&input.product_id)
    .fetch_optional(&state.db)
    .await?;

    sqlx::query(
        "UPDATE products SET
           category_id=?, name=?, sku=?, barcode=?, tax_rule_id=?,
           track_inventory=?, allow_decimal_quantity=?,
           reorder_point=?, is_active=?, updated_at=?
         WHERE product_id=?"
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
    .bind(&now)
    .bind(&input.product_id)
    .execute(&state.db)
    .await
    .map_err(|e| if e.to_string().contains("UNIQUE") {
        AppError::Validation("SKU or barcode already in use".into())
    } else { e.into() })?;

    if current_price != Some(input.price_minor) {
        // Close old price
        sqlx::query(
            "UPDATE product_prices SET effective_to = ?
             WHERE product_id = ? AND branch_id IS NULL
               AND price_type = 'selling' AND effective_to IS NULL"
        )
        .bind(&now).bind(&input.product_id)
        .execute(&state.db).await?;

        // Insert new price
        let price_id = Ulid::new().to_string();
        sqlx::query(
            "INSERT INTO product_prices
               (price_id, product_id, price_type, price_minor, currency,
                effective_from, created_by_user_id, created_at)
             VALUES (?,?,'selling',?,'BHD',?,?,?)"
        )
        .bind(&price_id)
        .bind(&input.product_id)
        .bind(input.price_minor)
        .bind(&now)
        .bind(&input.updated_by_user_id)
        .bind(&now)
        .execute(&state.db).await?;
    }

    if input.track_inventory {
        let sl_id = format!("SL-{}", input.product_id);
        sqlx::query(
            "INSERT OR IGNORE INTO stock_levels
               (stock_level_id, product_id, branch_id, quantity_on_hand, updated_at)
             VALUES (?,?,?,'0',?)"
        )
        .bind(&sl_id).bind(&input.product_id).bind(BRANCH_ID).bind(&now)
        .execute(&state.db).await?;
    }

    let sql = format!("{} WHERE p.product_id = ?", ADMIN_PRODUCT_QUERY);
    let row = sqlx::query(&sql)
        .bind(&input.product_id)
        .fetch_one(&state.db)
        .await?;
    Ok(row_to_admin_product(&row))
}

// ─── Category commands ────────────────────────────────────────────────────────

#[tauri::command]
pub async fn admin_list_categories(state: State<'_, AppState>) -> Result<Vec<CategoryRow>, AppError> {
    let rows = sqlx::query(
        "SELECT category_id, name, sort_order, is_active
         FROM categories ORDER BY sort_order, name"
    )
    .fetch_all(&state.db).await?;

    Ok(rows.iter().map(|r| {
        let active: i64 = r.get("is_active");
        CategoryRow {
            category_id: r.get("category_id"),
            name:        r.get("name"),
            sort_order:  r.get("sort_order"),
            is_active:   active != 0,
        }
    }).collect())
}

#[tauri::command]
pub async fn admin_list_tax_rules(state: State<'_, AppState>) -> Result<Vec<TaxRuleRow>, AppError> {
    let rows = sqlx::query(
        "SELECT tax_rule_id, name, rate_basis_points, inclusive
         FROM tax_rules WHERE is_active = 1 ORDER BY name"
    )
    .fetch_all(&state.db).await?;

    Ok(rows.iter().map(|r| {
        let inc: i64 = r.get("inclusive");
        TaxRuleRow {
            tax_rule_id:       r.get("tax_rule_id"),
            name:              r.get("name"),
            rate_basis_points: r.get("rate_basis_points"),
            inclusive:         inc != 0,
        }
    }).collect())
}

#[derive(Deserialize)]
pub struct SaveCategoryInput {
    pub category_id: Option<String>,  // None = create
    pub name:        String,
    pub sort_order:  i64,
    pub is_active:   bool,
}

#[tauri::command]
pub async fn admin_save_category(
    input: SaveCategoryInput,
    state: State<'_, AppState>,
) -> Result<CategoryRow, AppError> {
    let now = chrono::Utc::now().to_rfc3339();

    let category_id = if let Some(id) = input.category_id {
        sqlx::query(
            "UPDATE categories SET name=?, sort_order=?, is_active=?, updated_at=?
             WHERE category_id=?"
        )
        .bind(&input.name)
        .bind(input.sort_order)
        .bind(input.is_active as i64)
        .bind(&now)
        .bind(&id)
        .execute(&state.db).await?;
        id
    } else {
        let id = Ulid::new().to_string();
        sqlx::query(
            "INSERT INTO categories
               (category_id, name, sort_order, is_active, created_at, updated_at, version)
             VALUES (?,?,?,1,?,?,1)"
        )
        .bind(&id).bind(&input.name).bind(input.sort_order).bind(&now).bind(&now)
        .execute(&state.db).await?;
        id
    };

    let row = sqlx::query(
        "SELECT category_id, name, sort_order, is_active FROM categories WHERE category_id = ?"
    )
    .bind(&category_id)
    .fetch_one(&state.db)
    .await?;

    let active: i64 = row.get("is_active");
    Ok(CategoryRow {
        category_id: row.get("category_id"),
        name:        row.get("name"),
        sort_order:  row.get("sort_order"),
        is_active:   active != 0,
    })
}

// ─── User commands ────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn admin_list_users_all(state: State<'_, AppState>) -> Result<Vec<AdminUserRow>, AppError> {
    let rows = sqlx::query(
        "SELECT u.user_id, u.display_name, u.username, u.role_id, u.is_active, u.last_login_at,
                r.name AS role_name
         FROM users u JOIN roles r ON r.role_id = u.role_id
         ORDER BY u.is_active DESC, u.display_name"
    )
    .fetch_all(&state.db).await?;

    Ok(rows.iter().map(|r| {
        let active: i64 = r.get("is_active");
        AdminUserRow {
            user_id:       r.get("user_id"),
            display_name:  r.get("display_name"),
            username:      r.get("username"),
            role_id:       r.get("role_id"),
            role_name:     r.get("role_name"),
            is_active:     active != 0,
            last_login_at: r.get("last_login_at"),
        }
    }).collect())
}

#[tauri::command]
pub async fn admin_list_roles(state: State<'_, AppState>) -> Result<Vec<RoleRow>, AppError> {
    let rows = sqlx::query("SELECT role_id, name FROM roles ORDER BY name")
        .fetch_all(&state.db).await?;
    Ok(rows.iter().map(|r| RoleRow {
        role_id: r.get("role_id"),
        name:    r.get("name"),
    }).collect())
}

#[derive(Deserialize)]
pub struct CreateUserInput {
    pub display_name: String,
    pub username:     String,
    pub pin:          String,
    pub role_id:      String,
}

#[tauri::command]
pub async fn admin_create_user(
    input: CreateUserInput,
    state: State<'_, AppState>,
) -> Result<AdminUserRow, AppError> {
    if input.pin.len() < 4 {
        return Err(AppError::Validation("PIN must be at least 4 digits".into()));
    }
    let user_id  = Ulid::new().to_string();
    let now      = chrono::Utc::now().to_rfc3339();
    let pin_hash = auth_repo::hash_pin(&input.pin)?;

    sqlx::query(
        "INSERT INTO users
           (user_id, display_name, username, pin_hash, role_id,
            branch_scope, is_active, created_at, updated_at, version)
         VALUES (?,?,?,?,?,'[]',1,?,?,1)"
    )
    .bind(&user_id)
    .bind(&input.display_name)
    .bind(&input.username)
    .bind(&pin_hash)
    .bind(&input.role_id)
    .bind(&now)
    .bind(&now)
    .execute(&state.db)
    .await
    .map_err(|e| if e.to_string().contains("UNIQUE") {
        AppError::Validation("Username already exists".into())
    } else { e.into() })?;

    let row = sqlx::query(
        "SELECT u.user_id, u.display_name, u.username, u.role_id, u.is_active, u.last_login_at,
                r.name AS role_name
         FROM users u JOIN roles r ON r.role_id = u.role_id WHERE u.user_id = ?"
    )
    .bind(&user_id)
    .fetch_one(&state.db).await?;

    let active: i64 = row.get("is_active");
    Ok(AdminUserRow {
        user_id:       row.get("user_id"),
        display_name:  row.get("display_name"),
        username:      row.get("username"),
        role_id:       row.get("role_id"),
        role_name:     row.get("role_name"),
        is_active:     active != 0,
        last_login_at: row.get("last_login_at"),
    })
}

#[derive(Deserialize)]
pub struct UpdateUserInput {
    pub user_id:      String,
    pub display_name: String,
    pub pin:          Option<String>,  // None = unchanged
    pub role_id:      String,
    pub is_active:    bool,
}

#[tauri::command]
pub async fn admin_update_user(
    input: UpdateUserInput,
    state: State<'_, AppState>,
) -> Result<AdminUserRow, AppError> {
    if let Some(pin) = &input.pin {
        if pin.len() < 4 {
            return Err(AppError::Validation("PIN must be at least 4 digits".into()));
        }
    }
    let now = chrono::Utc::now().to_rfc3339();

    if let Some(pin) = &input.pin {
        let pin_hash = auth_repo::hash_pin(pin)?;
        sqlx::query(
            "UPDATE users SET display_name=?, pin_hash=?, role_id=?, is_active=?, updated_at=?
             WHERE user_id=?"
        )
        .bind(&input.display_name)
        .bind(&pin_hash)
        .bind(&input.role_id)
        .bind(input.is_active as i64)
        .bind(&now)
        .bind(&input.user_id)
        .execute(&state.db).await?;
    } else {
        sqlx::query(
            "UPDATE users SET display_name=?, role_id=?, is_active=?, updated_at=?
             WHERE user_id=?"
        )
        .bind(&input.display_name)
        .bind(&input.role_id)
        .bind(input.is_active as i64)
        .bind(&now)
        .bind(&input.user_id)
        .execute(&state.db).await?;
    }

    let row = sqlx::query(
        "SELECT u.user_id, u.display_name, u.username, u.role_id, u.is_active, u.last_login_at,
                r.name AS role_name
         FROM users u JOIN roles r ON r.role_id = u.role_id WHERE u.user_id = ?"
    )
    .bind(&input.user_id)
    .fetch_one(&state.db).await?;

    let active: i64 = row.get("is_active");
    Ok(AdminUserRow {
        user_id:       row.get("user_id"),
        display_name:  row.get("display_name"),
        username:      row.get("username"),
        role_id:       row.get("role_id"),
        role_name:     row.get("role_name"),
        is_active:     active != 0,
        last_login_at: row.get("last_login_at"),
    })
}
