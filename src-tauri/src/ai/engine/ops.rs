#![allow(dead_code)]
use std::future::Future;
use std::pin::Pin;

use serde_json::Value;
use sqlx::SqlitePool;

use super::selector::Selector;
use crate::errors::{AppError, AppResult};

fn selector_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "all_records": {"type": "boolean"},
            "category_subtree": {"type": "string"},
            "active": {"type": "boolean"},
            "track_inventory": {"type": "boolean"},
            "text": {"type": "string"},
            "supplier_id": {"type": "string"},
            "below_reorder": {"type": "boolean"},
            "variance_threshold": {"type": "integer"}
        }
    })
}

pub struct BulkStockSet;

impl Operation for BulkStockSet {
    fn id(&self) -> &'static str {
        "bulk_stock_set"
    }

    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "selector": selector_schema(),
                "new_quantity": {
                    "type": "number",
                    "minimum": 0,
                    "description": "Exact quantity on hand to set for every matched inventory-tracked product"
                }
            },
            "required": ["selector", "new_quantity"]
        })
    }

    fn validate<'a>(
        &'a self,
        _db: &'a SqlitePool,
        input: &'a Value,
    ) -> Pin<Box<dyn Future<Output = Result<(), Vec<String>>> + Send + 'a>> {
        let mut errors = Vec::new();
        match input.get("new_quantity").and_then(Value::as_f64) {
            Some(quantity)
                if quantity.is_finite() && (0.0..=1_000_000_000.0).contains(&quantity) => {}
            _ => errors.push(
                "new_quantity must be a finite non-negative number no greater than 1,000,000,000"
                    .into(),
            ),
        }
        match serde_json::from_value::<Selector>(input.get("selector").cloned().unwrap_or_default())
        {
            Ok(selector) => {
                if let Err(error) = selector.validate_for_mutation() {
                    errors.push(error.to_string());
                }
                if selector.track_inventory != Some(true) {
                    errors.push("selector.track_inventory must be true for a stock update".into());
                }
            }
            Err(error) => errors.push(format!("Invalid selector: {error}")),
        }
        Box::pin(async move {
            if errors.is_empty() {
                Ok(())
            } else {
                Err(errors)
            }
        })
    }

    fn preview<'a>(
        &'a self,
        db: &'a SqlitePool,
        input: &'a Value,
    ) -> Pin<Box<dyn Future<Output = AppResult<Preview>> + Send + 'a>> {
        Box::pin(async move {
            let selector: Selector =
                serde_json::from_value(input.get("selector").cloned().unwrap_or_default())
                    .map_err(|error| AppError::Validation(format!("Invalid selector: {error}")))?;
            let count = selector.count(db).await?;
            let new_quantity = input
                .get("new_quantity")
                .and_then(Value::as_f64)
                .ok_or_else(|| AppError::Validation("new_quantity is required".into()))?;
            Ok(Preview {
                description: format!(
                    "Set stock quantity to {new_quantity} for {count} inventory-tracked products"
                ),
                count: Some(count),
                samples: Vec::new(),
            })
        })
    }

    fn commit_batch<'a>(
        &'a self,
        tx: &'a mut sqlx::Transaction<'_, sqlx::Sqlite>,
        input: &'a Value,
        batch: &'a [BatchRow],
    ) -> Pin<Box<dyn Future<Output = AppResult<CommitResult>> + Send + 'a>> {
        Box::pin(async move {
            let new_quantity = input
                .get("new_quantity")
                .and_then(Value::as_f64)
                .ok_or_else(|| AppError::Validation("new_quantity is required".into()))?;
            let branch_id = crate::ai::tool_policy::current_branch_id().ok_or_else(|| {
                AppError::Permission("Missing authenticated AI branch context".into())
            })?;
            let actor_id = crate::ai::tool_policy::current_actor_id().ok_or_else(|| {
                AppError::Permission("Missing authenticated AI actor context".into())
            })?;
            let device_id: String = sqlx::query_scalar(
                "SELECT device_id FROM devices
                 WHERE branch_id=? AND is_active=1
                 ORDER BY created_at LIMIT 1",
            )
            .bind(&branch_id)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or_else(|| {
                AppError::Validation("No active device exists for the authenticated branch".into())
            })?;
            let now = chrono::Utc::now().to_rfc3339();
            let new_quantity_text = new_quantity.to_string();

            for row in batch {
                let tracks_inventory: bool = sqlx::query_scalar(
                    "SELECT EXISTS(
                         SELECT 1 FROM products
                         WHERE product_id=? AND track_inventory=1 AND deleted_at IS NULL
                     )",
                )
                .bind(&row.entity_id)
                .fetch_one(&mut **tx)
                .await?;
                if !tracks_inventory {
                    return Err(AppError::Validation(format!(
                        "Product {} is not inventory-tracked",
                        row.entity_id
                    )));
                }
                let old_quantity: Option<String> = sqlx::query_scalar(
                    "SELECT quantity_on_hand FROM stock_levels
                     WHERE product_id=? AND branch_id=?",
                )
                .bind(&row.entity_id)
                .bind(&branch_id)
                .fetch_optional(&mut **tx)
                .await?
                .flatten();
                let old_quantity_number = old_quantity
                    .as_deref()
                    .unwrap_or("0")
                    .parse::<f64>()
                    .unwrap_or(0.0);
                let delta = new_quantity - old_quantity_number;
                let stock_level_id = format!("SL-{}-{}", row.entity_id, branch_id);
                sqlx::query(
                    "INSERT INTO stock_levels
                     (stock_level_id,product_id,branch_id,quantity_on_hand,created_at,updated_at,last_movement_at)
                     VALUES (?,?,?,?,?,?,?)
                     ON CONFLICT(product_id,branch_id)
                     DO UPDATE SET quantity_on_hand=excluded.quantity_on_hand,
                                   updated_at=excluded.updated_at,
                                   last_movement_at=excluded.last_movement_at,
                                   sync_status='pending'",
                )
                .bind(&stock_level_id)
                .bind(&row.entity_id)
                .bind(&branch_id)
                .bind(&new_quantity_text)
                .bind(&now)
                .bind(&now)
                .bind(&now)
                .execute(&mut **tx)
                .await?;
                sqlx::query(
                    "INSERT INTO stock_movements
                     (movement_id,product_id,branch_id,device_id,origin_device_id,
                      movement_type,quantity_delta,quantity_after,reference_type,
                      notes,created_by_user_id,created_at,sync_status)
                     VALUES (?,?,?,?,?,'stock_take',?,?,'ai_run',
                             'ZanAI confirmed bulk stock set',?,?,'pending')",
                )
                .bind(ulid::Ulid::new().to_string())
                .bind(&row.entity_id)
                .bind(&branch_id)
                .bind(&device_id)
                .bind(&device_id)
                .bind(delta.to_string())
                .bind(&new_quantity_text)
                .bind(&actor_id)
                .bind(&now)
                .execute(&mut **tx)
                .await?;
            }
            Ok(CommitResult {
                rows_changed: batch.len() as i64,
            })
        })
    }
}

pub trait Operation: Send + Sync {
    fn id(&self) -> &'static str;
    fn schema(&self) -> Value;
    fn is_mutation(&self) -> bool {
        true
    }
    fn validate<'a>(
        &'a self,
        _db: &'a SqlitePool,
        _input: &'a Value,
    ) -> Pin<Box<dyn Future<Output = Result<(), Vec<String>>> + Send + 'a>> {
        Box::pin(async { Ok(()) })
    }
    fn preview<'a>(
        &'a self,
        db: &'a SqlitePool,
        input: &'a Value,
    ) -> Pin<Box<dyn Future<Output = AppResult<Preview>> + Send + 'a>>;
    fn commit_batch<'a>(
        &'a self,
        tx: &'a mut sqlx::Transaction<'_, sqlx::Sqlite>,
        input: &'a Value,
        batch: &'a [BatchRow],
    ) -> Pin<Box<dyn Future<Output = AppResult<CommitResult>> + Send + 'a>>;
}

pub struct Preview {
    pub description: String,
    pub count: Option<i64>,
    pub samples: Vec<Value>,
}

pub struct CommitResult {
    pub rows_changed: i64,
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct BatchRow {
    pub entity_id: String,
    pub current_value: Option<i64>,
    #[serde(default)]
    pub current_text: Option<String>,
}

pub struct Registry(Vec<Box<dyn Operation>>);

impl Registry {
    pub fn new() -> Self {
        Self(Vec::new())
    }
    pub fn register(&mut self, op: Box<dyn Operation>) {
        self.0.push(op);
    }
    pub fn find(&self, id: &str) -> Option<&dyn Operation> {
        self.0.iter().find(|o| o.id() == id).map(|o| o.as_ref())
    }
    pub fn all_ids(&self) -> Vec<&'static str> {
        self.0.iter().map(|o| o.id()).collect()
    }
}

static OPERATION_REGISTRY: std::sync::OnceLock<Registry> = std::sync::OnceLock::new();

/// Authoritative execution registry for long-running/run-based AI operations.
/// Provider routing, policy metadata, and tests all derive from this factory.
pub fn operation_registry() -> &'static Registry {
    OPERATION_REGISTRY.get_or_init(|| {
        let mut registry = Registry::new();
        registry.register(Box::new(BulkPriceAdjust));
        registry.register(Box::new(BulkStockSet));
        registry.register(Box::new(BulkStockVarianceFix));
        registry.register(Box::new(BulkPromotionApply));
        registry.register(Box::new(BulkPromotionRemove));
        registry.register(Box::new(BulkSupplierPriceSync));
        registry.register(Box::new(BulkProductArchive));
        registry.register(Box::new(BulkReorderPointUpdate));
        registry.register(Box::new(ProductCreate));
        registry
    })
}

pub fn is_registered_operation(tool_name: &str) -> bool {
    operation_registry().find(tool_name).is_some()
}

// ── Concrete operation: bulk.price_adjust ──

use super::{apply_price, PriceOp};

pub struct BulkPriceAdjust;

impl Operation for BulkPriceAdjust {
    fn id(&self) -> &'static str {
        "bulk_price_adjust"
    }
    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "selector": selector_schema(),
                "adjustment": {
                    "type": "object",
                    "properties": {
                        "mode": {"type": "string", "enum": ["Percent", "Absolute", "Set"]},
                        "value": {"type": "number"}
                    },
                    "required": ["mode", "value"]
                }
            },
            "required": ["selector", "adjustment"]
        })
    }
    fn preview<'a>(
        &'a self,
        db: &'a SqlitePool,
        input: &'a Value,
    ) -> Pin<Box<dyn Future<Output = AppResult<Preview>> + Send + 'a>> {
        Box::pin(async move {
            let sel: Selector =
                serde_json::from_value(input.get("selector").cloned().unwrap_or_default())
                    .map_err(|e| AppError::Validation(format!("Invalid selector: {e}")))?;
            let count = sel.count(db).await?;
            Ok(Preview {
                description: format!("Adjust prices for {} products", count),
                count: Some(count),
                samples: vec![],
            })
        })
    }
    fn commit_batch<'a>(
        &'a self,
        tx: &'a mut sqlx::Transaction<'_, sqlx::Sqlite>,
        input: &'a Value,
        batch: &'a [BatchRow],
    ) -> Pin<Box<dyn Future<Output = AppResult<CommitResult>> + Send + 'a>> {
        Box::pin(async move {
            let adj: PriceOp =
                serde_json::from_value(input.get("adjustment").cloned().unwrap_or_default())
                    .map_err(|e| AppError::Validation(format!("Invalid adjustment: {e}")))?;
            let now = chrono::Utc::now().to_rfc3339();
            for row in batch {
                let current = row.current_value.ok_or_else(|| {
                    AppError::Validation(
                        "BulkPriceAdjust requires current_value per row; use the dedicated price_adjust path"
                            .into(),
                    )
                })?;
                let new_price = apply_price(current, &adj);
                sqlx::query(
                    "UPDATE product_prices SET effective_to=?, sync_status='pending' WHERE product_id=? AND price_type='selling' AND effective_to IS NULL",
                )
                .bind(&now)
                .bind(&row.entity_id)
                .execute(&mut **tx)
                .await?;
                let price_id = ulid::Ulid::new().to_string();
                sqlx::query(
                    "INSERT INTO product_prices (price_id,product_id,branch_id,price_type,price_minor,currency,effective_from,created_by_user_id,created_at) VALUES (?,?,NULL,'selling',?,'BHD',?,?,?)",
                )
                .bind(&price_id)
                .bind(&row.entity_id)
                .bind(new_price)
                .bind(&now)
                .bind(crate::ai::tool_policy::current_actor_id().ok_or_else(|| AppError::Permission("Missing authenticated AI actor context".into()))?)
                .bind(&now)
                .execute(&mut **tx)
                .await?;
            }
            Ok(CommitResult {
                rows_changed: batch.len() as i64,
            })
        })
    }
}

// ── Concrete operation: product.create ──

pub struct ProductCreate;

impl Operation for ProductCreate {
    fn id(&self) -> &'static str {
        "create_product"
    }
    fn schema(&self) -> Value {
        serde_json::json!({"type":"object","properties":{"name":{"type":"string"},"category_id":{"type":"string"},"sku":{"type":"string"},"barcode":{"type":"string"},"price_minor":{"type":"integer"}},"required":["name","category_id","price_minor"]})
    }
    fn validate<'a>(
        &'a self,
        db: &'a SqlitePool,
        input: &'a Value,
    ) -> Pin<Box<dyn Future<Output = Result<(), Vec<String>>> + Send + 'a>> {
        Box::pin(async move {
            let mut errs = vec![];
            if input
                .get("name")
                .and_then(|v| v.as_str())
                .map_or(true, |s| s.trim().is_empty())
            {
                errs.push("name is required".into());
            }
            match input.get("category_id").and_then(|v| v.as_str()) {
                Some(cat) if !cat.trim().is_empty() => {
                    let exists: bool = sqlx::query_scalar(
                        "SELECT EXISTS(SELECT 1 FROM categories WHERE category_id=? AND is_active=1 AND deleted_at IS NULL)",
                    )
                    .bind(cat)
                    .fetch_one(db)
                    .await
                    .unwrap_or(false);
                    if !exists {
                        errs.push(format!("category {} not found", cat));
                    }
                }
                _ => errs.push("category_id is required".into()),
            }
            if input.get("price_minor").and_then(|v| v.as_i64()).is_none() {
                errs.push("price_minor is required".into());
            }
            if let Some(bc) = input
                .get("barcode")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            {
                let dup: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM products WHERE barcode=? UNION SELECT 1 FROM product_barcodes WHERE barcode=?)",
                )
                .bind(bc)
                .bind(bc)
                .fetch_one(db)
                .await
                .unwrap_or(false);
                if dup {
                    errs.push(format!("barcode {} already in use", bc));
                }
            }
            if errs.is_empty() {
                Ok(())
            } else {
                Err(errs)
            }
        })
    }
    fn preview<'a>(
        &'a self,
        _db: &'a SqlitePool,
        input: &'a Value,
    ) -> Pin<Box<dyn Future<Output = AppResult<Preview>> + Send + 'a>> {
        Box::pin(async move {
            let name = input
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("new product");
            Ok(Preview {
                description: format!("Create product: {}", name),
                count: Some(1),
                samples: vec![],
            })
        })
    }
    fn commit_batch<'a>(
        &'a self,
        tx: &'a mut sqlx::Transaction<'_, sqlx::Sqlite>,
        input: &'a Value,
        _batch: &'a [BatchRow],
    ) -> Pin<Box<dyn Future<Output = AppResult<CommitResult>> + Send + 'a>> {
        Box::pin(async move {
            let name = input
                .get("name")
                .and_then(|v| v.as_str())
                .filter(|name| !name.trim().is_empty())
                .ok_or_else(|| AppError::Validation("name is required".into()))?;
            let cat = input
                .get("category_id")
                .and_then(|v| v.as_str())
                .filter(|category_id| !category_id.trim().is_empty())
                .ok_or_else(|| AppError::Validation("category_id is required".into()))?;
            let price = input
                .get("price_minor")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| AppError::Validation("price_minor is required".into()))?;
            let category_exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM categories WHERE category_id=? AND is_active=1 AND deleted_at IS NULL)",
            )
            .bind(cat)
            .fetch_one(&mut **tx)
            .await?;
            if !category_exists {
                return Err(AppError::Validation(format!("category {cat} not found")));
            }
            let pid = ulid::Ulid::new().to_string();
            let now = chrono::Utc::now().to_rfc3339();
            sqlx::query(
                "INSERT INTO products (product_id,category_id,name,sku,barcode,cost_minor,is_active,track_inventory,reorder_point,created_at,updated_at) VALUES (?,?,?,?,?,0,1,1,0,?,?)",
            )
            .bind(&pid)
            .bind(cat)
            .bind(name)
            .bind(input.get("sku").and_then(|v| v.as_str()).unwrap_or(""))
            .bind(input.get("barcode").and_then(|v| v.as_str()).unwrap_or(""))
            .bind(&now)
            .bind(&now)
            .execute(&mut **tx)
            .await?;
            sqlx::query(
                "INSERT INTO product_prices (price_id,product_id,branch_id,price_type,price_minor,currency,effective_from,created_by_user_id,created_at) VALUES (?,?,NULL,'selling',?,'BHD',?,?,?)",
            )
            .bind(ulid::Ulid::new().to_string())
            .bind(&pid)
            .bind(price)
            .bind(&now)
            .bind(crate::ai::tool_policy::current_actor_id().ok_or_else(|| AppError::Permission("Missing authenticated AI actor context".into()))?)
            .bind(&now)
            .execute(&mut **tx)
            .await?;
            Ok(CommitResult { rows_changed: 1 })
        })
    }
}

// ── Concrete operation: bulk.product_archive ──

pub struct BulkProductArchive;

impl Operation for BulkProductArchive {
    fn id(&self) -> &'static str {
        "bulk_product_archive"
    }
    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "selector": selector_schema(),
                "reason": {"type": "string"}
            },
            "required": ["selector"]
        })
    }
    fn preview<'a>(
        &'a self,
        db: &'a SqlitePool,
        input: &'a Value,
    ) -> Pin<Box<dyn Future<Output = AppResult<Preview>> + Send + 'a>> {
        Box::pin(async move {
            let selector: Selector =
                serde_json::from_value(input.get("selector").cloned().unwrap_or_default())
                    .map_err(|e| AppError::Validation(format!("Invalid selector: {e}")))?;
            let count = selector.count(db).await?;
            let c = selector.compile();
            let sql = format!(
                "{} SELECT p.name FROM products p WHERE {} LIMIT 5",
                c.cte, c.where_sql
            );
            let mut q = sqlx::query_as::<_, (String,)>(&sql);
            for b in &c.binds {
                q = q.bind(b);
            }
            let samples: Vec<_> = q
                .fetch_all(db)
                .await?
                .into_iter()
                .map(|(name,)| serde_json::json!({"name": name}))
                .collect();
            Ok(Preview {
                description: format!("Archive {} products (set inactive)", count),
                count: Some(count),
                samples,
            })
        })
    }
    fn commit_batch<'a>(
        &'a self,
        tx: &'a mut sqlx::Transaction<'_, sqlx::Sqlite>,
        _input: &'a Value,
        batch: &'a [BatchRow],
    ) -> Pin<Box<dyn Future<Output = AppResult<CommitResult>> + Send + 'a>> {
        Box::pin(async move {
            let now = chrono::Utc::now().to_rfc3339();
            let mut changed: i64 = 0;
            for row in batch {
                sqlx::query(
                    "UPDATE products SET is_active=0, updated_at=?, sync_status='pending' WHERE product_id=?",
                )
                    .bind(&now)
                    .bind(&row.entity_id)
                    .execute(&mut **tx)
                    .await?;
                changed += 1;
            }
            Ok(CommitResult {
                rows_changed: changed,
            })
        })
    }
}

// ── Concrete operation: bulk.reorder_point_update ──

pub struct BulkReorderPointUpdate;

impl Operation for BulkReorderPointUpdate {
    fn id(&self) -> &'static str {
        "bulk_reorder_point_update"
    }
    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "selector": selector_schema(),
                "new_reorder_point": {"type": "number", "description": "New reorder point value"}
            },
            "required": ["selector", "new_reorder_point"]
        })
    }
    fn validate<'a>(
        &'a self,
        _db: &'a SqlitePool,
        input: &'a Value,
    ) -> Pin<Box<dyn Future<Output = Result<(), Vec<String>>> + Send + 'a>> {
        let has_point = input
            .get("new_reorder_point")
            .and_then(|v| v.as_f64())
            .is_some();
        Box::pin(async move {
            if !has_point {
                Err(vec!["new_reorder_point is required".into()])
            } else {
                Ok(())
            }
        })
    }
    fn preview<'a>(
        &'a self,
        db: &'a SqlitePool,
        input: &'a Value,
    ) -> Pin<Box<dyn Future<Output = AppResult<Preview>> + Send + 'a>> {
        Box::pin(async move {
            let selector: Selector =
                serde_json::from_value(input.get("selector").cloned().unwrap_or_default())
                    .map_err(|e| AppError::Validation(format!("Invalid selector: {e}")))?;
            let count = selector.count(db).await?;
            let new_pt = input
                .get("new_reorder_point")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            Ok(Preview {
                description: format!("Update reorder point to {new_pt:.1} for {count} products"),
                count: Some(count),
                samples: vec![],
            })
        })
    }
    fn commit_batch<'a>(
        &'a self,
        tx: &'a mut sqlx::Transaction<'_, sqlx::Sqlite>,
        input: &'a Value,
        batch: &'a [BatchRow],
    ) -> Pin<Box<dyn Future<Output = AppResult<CommitResult>> + Send + 'a>> {
        Box::pin(async move {
            let new_pt = input
                .get("new_reorder_point")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let now = chrono::Utc::now().to_rfc3339();
            let mut changed: i64 = 0;
            for row in batch {
                sqlx::query(
                    "UPDATE products SET reorder_point=?, updated_at=?, sync_status='pending' WHERE product_id=?",
                )
                    .bind(new_pt)
                    .bind(&now)
                    .bind(&row.entity_id)
                    .execute(&mut **tx)
                    .await?;
                changed += 1;
            }
            Ok(CommitResult {
                rows_changed: changed,
            })
        })
    }
}

// ── Concrete operation: bulk.promotion_apply ──

pub struct BulkPromotionApply;

impl Operation for BulkPromotionApply {
    fn id(&self) -> &'static str {
        "bulk_promotion_apply"
    }
    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "selector": selector_schema(),
                "promo_price_minor": {"type": "integer", "description": "Promotional price in fils"},
                "effective_from": {"type": "string", "description": "ISO datetime start"},
                "effective_to": {"type": "string", "description": "ISO datetime end"}
            },
            "required": ["selector", "promo_price_minor", "effective_from", "effective_to"]
        })
    }
    fn validate<'a>(
        &'a self,
        _db: &'a SqlitePool,
        input: &'a Value,
    ) -> Pin<Box<dyn Future<Output = Result<(), Vec<String>>> + Send + 'a>> {
        let mut errs: Vec<String> = vec![];
        if input
            .get("promo_price_minor")
            .and_then(|v| v.as_i64())
            .is_none()
        {
            errs.push("promo_price_minor is required".into());
        }
        if input
            .get("effective_from")
            .and_then(|v| v.as_str())
            .is_none()
        {
            errs.push("effective_from is required".into());
        }
        if input.get("effective_to").and_then(|v| v.as_str()).is_none() {
            errs.push("effective_to is required".into());
        }
        Box::pin(async move {
            if errs.is_empty() {
                Ok(())
            } else {
                Err(errs)
            }
        })
    }
    fn preview<'a>(
        &'a self,
        db: &'a SqlitePool,
        input: &'a Value,
    ) -> Pin<Box<dyn Future<Output = AppResult<Preview>> + Send + 'a>> {
        Box::pin(async move {
            let selector: Selector =
                serde_json::from_value(input.get("selector").cloned().unwrap_or_default())
                    .map_err(|e| AppError::Validation(format!("Invalid selector: {e}")))?;
            let count = selector.count(db).await?;
            let price = input
                .get("promo_price_minor")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let from = input
                .get("effective_from")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let to = input
                .get("effective_to")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            Ok(Preview {
                description: format!(
                    "Apply promotional price of {price} fils to {count} products from {from} to {to}"
                ),
                count: Some(count),
                samples: vec![],
            })
        })
    }
    fn commit_batch<'a>(
        &'a self,
        tx: &'a mut sqlx::Transaction<'_, sqlx::Sqlite>,
        input: &'a Value,
        batch: &'a [BatchRow],
    ) -> Pin<Box<dyn Future<Output = AppResult<CommitResult>> + Send + 'a>> {
        Box::pin(async move {
            let price = input
                .get("promo_price_minor")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let from = input
                .get("effective_from")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let to = input
                .get("effective_to")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let now = chrono::Utc::now().to_rfc3339();
            let mut changed: i64 = 0;
            for row in batch {
                let price_id = ulid::Ulid::new().to_string();
                sqlx::query(
                    "INSERT INTO product_prices \
                     (price_id, product_id, price_type, price_minor, currency, effective_from, effective_to, \
                      created_by_user_id, created_at, sync_status) \
                     VALUES (?, ?, 'promotional', ?, 'BHD', ?, ?, ?, ?, 'pending')",
                )
                .bind(&price_id)
                .bind(&row.entity_id)
                .bind(price)
                .bind(&from)
                .bind(&to)
                .bind(crate::ai::tool_policy::current_actor_id().ok_or_else(|| AppError::Permission("Missing authenticated AI actor context".into()))?)
                .bind(&now)
                .execute(&mut **tx)
                .await?;
                changed += 1;
            }
            Ok(CommitResult {
                rows_changed: changed,
            })
        })
    }
}

// ── Concrete operation: bulk.supplier_price_sync ──

pub struct BulkSupplierPriceSync;

impl Operation for BulkSupplierPriceSync {
    fn id(&self) -> &'static str {
        "bulk_supplier_price_sync"
    }
    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "selector": selector_schema(),
                "updates": {
                    "type": "array",
                    "minItems": 1,
                    "items": {
                        "type": "object",
                        "properties": {
                            "product_id": {"type": "string"},
                            "new_cost_minor": {"type": "integer"}
                        },
                        "required": ["product_id", "new_cost_minor"]
                    }
                }
            },
            "required": ["selector", "updates"]
        })
    }
    fn preview<'a>(
        &'a self,
        _db: &'a SqlitePool,
        input: &'a Value,
    ) -> Pin<Box<dyn Future<Output = AppResult<Preview>> + Send + 'a>> {
        Box::pin(async move {
            let count = input
                .get("updates")
                .and_then(|v| v.as_array())
                .map(|a| a.len() as i64)
                .unwrap_or(0);
            Ok(Preview {
                description: format!("Sync supplier costs for {count} products"),
                count: Some(count),
                samples: vec![],
            })
        })
    }
    fn commit_batch<'a>(
        &'a self,
        tx: &'a mut sqlx::Transaction<'_, sqlx::Sqlite>,
        input: &'a Value,
        batch: &'a [BatchRow],
    ) -> Pin<Box<dyn Future<Output = AppResult<CommitResult>> + Send + 'a>> {
        Box::pin(async move {
            let updates = input
                .get("updates")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            // Build lookup map upfront — O(n+m) instead of O(n*m)
            let cost_map: std::collections::HashMap<&str, i64> = updates
                .iter()
                .filter_map(|u| {
                    let pid = u.get("product_id").and_then(|v| v.as_str())?;
                    let cost = u.get("new_cost_minor").and_then(|v| v.as_i64())?;
                    Some((pid, cost))
                })
                .collect();
            let now = chrono::Utc::now().to_rfc3339();
            let mut changed: i64 = 0;
            for row in batch {
                if let Some(cost) = cost_map.get(row.entity_id.as_str()).copied() {
                    sqlx::query(
                        "UPDATE products SET cost_minor=?, updated_at=?, sync_status='pending' WHERE product_id=?",
                    )
                    .bind(cost)
                    .bind(&now)
                    .bind(&row.entity_id)
                    .execute(&mut **tx)
                    .await?;
                    changed += 1;
                }
            }
            Ok(CommitResult {
                rows_changed: changed,
            })
        })
    }
}

// ── Concrete operation: bulk.stock_variance_fix ──
// stock_movements schema (from 0001_initial.sql):
//   movement_id, product_id, branch_id, device_id, origin_device_id,
//   movement_type, quantity_delta (TEXT), quantity_after (TEXT),
//   reference_type, reference_id, notes, created_by_user_id, created_at,
//   sync_status, sync_attempts

pub struct BulkStockVarianceFix;

impl Operation for BulkStockVarianceFix {
    fn id(&self) -> &'static str {
        "bulk_stock_variance_fix"
    }
    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "selector": selector_schema(),
                "corrections": {
                    "type": "array",
                    "minItems": 1,
                    "description": "List of {product_id, branch_id, adjustment_qty} corrections",
                    "items": {
                        "type": "object",
                        "properties": {
                            "product_id": {"type": "string"},
                            "branch_id": {"type": "string"},
                            "adjustment_qty": {"type": "number"}
                        },
                        "required": ["product_id", "branch_id", "adjustment_qty"]
                    }
                }
            },
            "required": ["selector", "corrections"]
        })
    }
    fn preview<'a>(
        &'a self,
        _db: &'a SqlitePool,
        input: &'a Value,
    ) -> Pin<Box<dyn Future<Output = AppResult<Preview>> + Send + 'a>> {
        Box::pin(async move {
            let count = input
                .get("corrections")
                .and_then(|v| v.as_array())
                .map(|a| a.len() as i64)
                .unwrap_or(0);
            Ok(Preview {
                description: format!("Apply stock variance corrections for {count} products"),
                count: Some(count),
                samples: vec![],
            })
        })
    }
    fn commit_batch<'a>(
        &'a self,
        tx: &'a mut sqlx::Transaction<'_, sqlx::Sqlite>,
        input: &'a Value,
        batch: &'a [BatchRow],
    ) -> Pin<Box<dyn Future<Output = AppResult<CommitResult>> + Send + 'a>> {
        Box::pin(async move {
            let corrections = input
                .get("corrections")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let now = chrono::Utc::now().to_rfc3339();
            let mut changed: i64 = 0;
            for row in batch {
                if let Some(c) = corrections.iter().find(|c| {
                    c.get("product_id").and_then(|v| v.as_str()) == Some(row.entity_id.as_str())
                }) {
                    let adj = c
                        .get("adjustment_qty")
                        .and_then(|v| v.as_f64())
                        .unwrap_or(0.0);
                    let branch = crate::ai::tool_policy::current_branch_id().ok_or_else(|| {
                        AppError::Permission("Missing authenticated AI branch context".into())
                    })?;
                    if c.get("branch_id")
                        .and_then(|v| v.as_str())
                        .is_some_and(|requested| requested != branch)
                    {
                        return Err(AppError::Permission(
                            "Cross-branch stock mutation is not allowed".into(),
                        ));
                    }
                    let device: String = sqlx::query_scalar(
                        "SELECT device_id FROM devices WHERE branch_id=? AND is_active=1 ORDER BY created_at LIMIT 1"
                    )
                    .bind(&branch)
                    .fetch_optional(&mut **tx)
                    .await?
                    .ok_or_else(|| AppError::Validation("No active device exists for the authenticated branch".into()))?;
                    let mvt_id = ulid::Ulid::new().to_string();
                    sqlx::query(
                        "INSERT INTO stock_movements \
                         (movement_id, product_id, branch_id, device_id, origin_device_id, \
                          movement_type, quantity_delta, quantity_after, \
                          notes, created_by_user_id, created_at, sync_status) \
                         VALUES (?, ?, ?, ?, ?, 'adjustment', ?, '0', \
                                 'AI stock variance correction', ?, ?, 'pending')",
                    )
                    .bind(&mvt_id)
                    .bind(&row.entity_id)
                    .bind(&branch)
                    .bind(&device)
                    .bind(&device)
                    .bind(format!("{:.3}", adj))
                    .bind(crate::ai::tool_policy::current_actor_id().ok_or_else(|| {
                        AppError::Permission("Missing authenticated AI actor context".into())
                    })?)
                    .bind(&now)
                    .execute(&mut **tx)
                    .await?;
                    changed += 1;
                }
            }
            Ok(CommitResult {
                rows_changed: changed,
            })
        })
    }
}

// ── Concrete operation: bulk.promotion_remove ──

pub struct BulkPromotionRemove;

impl Operation for BulkPromotionRemove {
    fn id(&self) -> &'static str {
        "bulk_promotion_remove"
    }
    fn schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "selector": selector_schema()
            },
            "required": ["selector"]
        })
    }
    fn preview<'a>(
        &'a self,
        db: &'a SqlitePool,
        input: &'a Value,
    ) -> Pin<Box<dyn Future<Output = AppResult<Preview>> + Send + 'a>> {
        Box::pin(async move {
            let selector: Selector =
                serde_json::from_value(input.get("selector").cloned().unwrap_or_default())
                    .map_err(|e| AppError::Validation(format!("Invalid selector: {e}")))?;
            let count = selector.count_promotional(db).await?;
            Ok(Preview {
                description: format!("End {count} active promotion(s) — sets effective_to to now"),
                count: Some(count),
                samples: vec![],
            })
        })
    }
    fn commit_batch<'a>(
        &'a self,
        tx: &'a mut sqlx::Transaction<'_, sqlx::Sqlite>,
        _input: &'a Value,
        batch: &'a [BatchRow],
    ) -> Pin<Box<dyn Future<Output = AppResult<CommitResult>> + Send + 'a>> {
        Box::pin(async move {
            let now = chrono::Utc::now().to_rfc3339();
            let mut changed: i64 = 0;
            for row in batch {
                sqlx::query(
                    "UPDATE product_prices SET effective_to = ?, sync_status = 'pending' \
                     WHERE product_id = ? AND price_type = 'promotional' AND effective_to > ?",
                )
                .bind(&now)
                .bind(&row.entity_id)
                .bind(&now)
                .execute(&mut **tx)
                .await?;
                changed += 1;
            }
            Ok(CommitResult {
                rows_changed: changed,
            })
        })
    }
}

// ── Test ──

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn registry_finds_registered_ops() {
        let mut r = Registry::new();
        r.register(Box::new(BulkPriceAdjust));
        assert!(r.find("bulk_price_adjust").is_some());
        assert!(r.find("nonexistent").is_none());
    }

    #[tokio::test]
    async fn product_create_validates_empty_name() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let op = ProductCreate;
        let r = op
            .validate(&pool, &serde_json::json!({"name":"","category_id":"x"}))
            .await;
        assert!(r.is_err());
    }

    #[test]
    fn product_create_schema_requires_name_category_and_price() {
        let schema = ProductCreate.schema();

        assert_eq!(
            schema["required"],
            serde_json::json!(["name", "category_id", "price_minor"])
        );
    }

    #[tokio::test]
    async fn product_create_validation_requires_category_and_price() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();

        let errors = ProductCreate
            .validate(&pool, &serde_json::json!({"name":"Kinder Riegel 21g"}))
            .await
            .unwrap_err();

        assert!(errors
            .iter()
            .any(|error| error == "category_id is required"));
        assert!(errors
            .iter()
            .any(|error| error == "price_minor is required"));
    }

    #[tokio::test]
    async fn product_create_commit_revalidates_category_inside_transaction() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO categories (category_id,name,sort_order,is_active,created_at,updated_at) VALUES ('cat-1','Chocolate',0,1,?,?)")
            .bind(&now)
            .bind(&now)
            .execute(&pool)
            .await
            .unwrap();
        let input = serde_json::json!({
            "name": "Kinder Riegel 21g",
            "category_id": "cat-1",
            "price_minor": 150
        });
        ProductCreate.validate(&pool, &input).await.unwrap();
        sqlx::query("UPDATE categories SET is_active=0 WHERE category_id='cat-1'")
            .execute(&pool)
            .await
            .unwrap();

        let mut tx = pool.begin().await.unwrap();
        let result = ProductCreate.commit_batch(&mut tx, &input, &[]).await;

        assert!(matches!(result, Err(AppError::Validation(_))));
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM products")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn bulk_stock_set_validates_and_sets_exact_quantity() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let now = "2026-07-31T00:00:00Z";
        sqlx::query(
            "INSERT INTO categories
             (category_id,name,sort_order,is_active,created_at,updated_at)
             VALUES ('stock-cat','Stock',0,1,?,?)",
        )
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();
        for product_id in ["stock-1", "stock-2"] {
            sqlx::query(
                "INSERT INTO products
                 (product_id,category_id,name,track_inventory,is_active,currency,reorder_point,created_at,updated_at)
                 VALUES (?,'stock-cat',?,1,1,'BHD',0,?,?)",
            )
            .bind(product_id)
            .bind(product_id)
            .bind(now)
            .bind(now)
            .execute(&pool)
            .await
            .unwrap();
        }
        let input = serde_json::json!({
            "selector": {"all_records": true, "active": true, "track_inventory": true},
            "new_quantity": 100
        });
        BulkStockSet.validate(&pool, &input).await.unwrap();
        let preview = BulkStockSet.preview(&pool, &input).await.unwrap();
        assert_eq!(preview.count, Some(2));
        assert!(preview.description.contains("100"));

        let context = crate::ai::tool_policy::MutationExecutionContext {
            actor_user_id: "01JUSERS000000000000000001".into(),
            branch_id: "01JBRANCH0000000000000001".into(),
        };
        let changed = crate::ai::tool_policy::with_mutation_context(&context, async {
            let mut tx = pool.begin().await?;
            let result = BulkStockSet
                .commit_batch(
                    &mut tx,
                    &input,
                    &[
                        BatchRow {
                            entity_id: "stock-1".into(),
                            current_value: None,
                            current_text: None,
                        },
                        BatchRow {
                            entity_id: "stock-2".into(),
                            current_value: None,
                            current_text: None,
                        },
                    ],
                )
                .await?;
            tx.commit().await?;
            Ok(result.rows_changed)
        })
        .await
        .unwrap();

        assert_eq!(changed, 2);
        let quantities: Vec<String> = sqlx::query_scalar(
            "SELECT quantity_on_hand FROM stock_levels
             WHERE branch_id='01JBRANCH0000000000000001'
               AND product_id IN ('stock-1','stock-2')
             ORDER BY product_id",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(quantities, vec!["100", "100"]);
        let movements: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM stock_movements
             WHERE product_id IN ('stock-1','stock-2') AND movement_type='stock_take'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(movements, 2);
    }

    #[tokio::test]
    async fn bulk_stock_set_rejects_negative_quantity() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();

        let errors = BulkStockSet
            .validate(
                &pool,
                &serde_json::json!({
                    "selector": {"all_records": true},
                    "new_quantity": -1
                }),
            )
            .await
            .unwrap_err();

        assert!(errors.iter().any(|error| error.contains("non-negative")));
    }

    #[test]
    fn bulk_operation_schemas_require_explicit_selectors_and_inputs() {
        let operations: Vec<(Box<dyn Operation>, &[&str])> = vec![
            (Box::new(BulkPriceAdjust), &["selector", "adjustment"]),
            (Box::new(BulkStockSet), &["selector", "new_quantity"]),
            (Box::new(BulkProductArchive), &["selector"]),
            (
                Box::new(BulkReorderPointUpdate),
                &["selector", "new_reorder_point"],
            ),
            (
                Box::new(BulkPromotionApply),
                &[
                    "selector",
                    "promo_price_minor",
                    "effective_from",
                    "effective_to",
                ],
            ),
            (Box::new(BulkSupplierPriceSync), &["selector", "updates"]),
            (Box::new(BulkStockVarianceFix), &["selector", "corrections"]),
            (Box::new(BulkPromotionRemove), &["selector"]),
        ];

        for (operation, expected_required) in operations {
            let schema = operation.schema();
            let required = schema["required"].as_array().unwrap();
            for field in expected_required {
                assert!(
                    required.iter().any(|value| value == field),
                    "{} does not require {field}",
                    operation.id()
                );
            }
            let selector = &schema["properties"]["selector"];
            for field in [
                "all_records",
                "category_subtree",
                "active",
                "track_inventory",
                "text",
                "supplier_id",
                "below_reorder",
                "variance_threshold",
            ] {
                assert!(
                    selector["properties"].get(field).is_some(),
                    "{} selector does not advertise {field}",
                    operation.id()
                );
            }
        }
    }
}
