use std::future::Future;
use std::pin::Pin;

use serde_json::Value;
use sqlx::SqlitePool;

use crate::errors::AppResult;

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

pub struct BatchRow {
    pub entity_id: String,
    pub current_value: Option<i64>,
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

// ── Concrete operation: bulk.price_adjust ──

use super::selector::Selector;
use super::{apply_price, PriceOp};

pub struct BulkPriceAdjust;

impl Operation for BulkPriceAdjust {
    fn id(&self) -> &'static str {
        "bulk.price_adjust"
    }
    fn schema(&self) -> Value {
        serde_json::json!({"type":"object","properties":{"selector":{"type":"object"},"adjustment":{"type":"object"}}})
    }
    fn preview<'a>(
        &'a self,
        db: &'a SqlitePool,
        input: &'a Value,
    ) -> Pin<Box<dyn Future<Output = AppResult<Preview>> + Send + 'a>> {
        Box::pin(async move {
            let sel: Selector =
                serde_json::from_value(input.get("selector").cloned().unwrap_or_default())
                    .unwrap_or_default();
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
                    .unwrap_or(PriceOp::Percent(0.0));
            let now = chrono::Utc::now().to_rfc3339();
            for row in batch {
                let current = row.current_value.unwrap_or(0);
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
                .bind("AI_ADMIN")
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
        "product.create"
    }
    fn schema(&self) -> Value {
        serde_json::json!({"type":"object","properties":{"name":{"type":"string"},"category_id":{"type":"string"},"sku":{"type":"string"},"barcode":{"type":"string"},"price_minor":{"type":"integer"}},"required":["name","category_id"]})
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
            if let Some(cat) = input.get("category_id").and_then(|v| v.as_str()) {
                let exists: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM categories WHERE category_id=?)",
                )
                .bind(cat)
                .fetch_one(db)
                .await
                .unwrap_or(false);
                if !exists {
                    errs.push(format!("category {} not found", cat));
                }
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
            let name = input.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let cat = input
                .get("category_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
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
            if let Some(price) = input.get("price_minor").and_then(|v| v.as_i64()) {
                sqlx::query(
                    "INSERT INTO product_prices (price_id,product_id,branch_id,price_type,price_minor,currency,effective_from,created_by_user_id,created_at) VALUES (?,?,NULL,'selling',?,'BHD',?,?,?)",
                )
                .bind(ulid::Ulid::new().to_string())
                .bind(&pid)
                .bind(price)
                .bind(&now)
                .bind("AI_ADMIN")
                .bind(&now)
                .execute(&mut **tx)
                .await?;
            }
            Ok(CommitResult { rows_changed: 1 })
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
        assert!(r.find("bulk.price_adjust").is_some());
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
}
