#![allow(dead_code)]
use crate::errors::{AppError, AppResult};
use sqlx::SqlitePool;

/// Declarative product filter. A1 supports the fields the first demo needs;
/// price_range / stock / not_sold_since are added in later phases.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selector {
    /// Required for bulk mutations that intentionally target every record.
    #[serde(default)]
    pub all_records: bool,
    /// Resolve this category AND all descendants (adjacency list → recursive CTE).
    pub category_subtree: Option<String>,
    /// Restrict to active (true) or inactive (false) products.
    pub active: Option<bool>,
    /// Restrict to products that do (true) or do not (false) track inventory.
    pub track_inventory: Option<bool>,
    /// Case-insensitive LIKE over name / sku / barcode.
    pub text: Option<String>,
    /// Filter products by default_supplier_id.
    pub supplier_id: Option<String>,
    /// Only products currently at or below their reorder_point (needs stock_levels JOIN).
    /// Callers must add the JOIN themselves; compile() only produces WHERE conditions
    /// for the products table. Use needs_stock_join() to detect this requirement.
    pub below_reorder: Option<bool>,
    /// Stock variance threshold: |expected - actual| >= this value (needs stock_levels JOIN).
    pub variance_threshold: Option<i64>,
}

/// A compiled WHERE fragment with ordered bind values (all TEXT in this phase).
pub struct Compiled {
    /// Optional recursive-CTE prelude (empty string when no subtree filter).
    pub cte: String,
    /// SQL boolean conditions joined with AND, referencing `p` (products alias).
    pub where_sql: String,
    /// Bind values, in the order the placeholders appear across cte + where_sql.
    pub binds: Vec<String>,
}

impl Selector {
    pub fn validate(&self) -> AppResult<()> {
        for (label, value) in [
            ("category_subtree", self.category_subtree.as_deref()),
            ("supplier_id", self.supplier_id.as_deref()),
        ] {
            if value.is_some_and(|value| value.is_empty() || value.chars().count() > 128) {
                return Err(AppError::Validation(format!(
                    "Selector {label} must contain 1..=128 characters"
                )));
            }
        }
        if self
            .text
            .as_deref()
            .is_some_and(|value| value.chars().count() > 200)
        {
            return Err(AppError::Validation(
                "Selector text exceeds 200 characters".into(),
            ));
        }
        if self
            .variance_threshold
            .is_some_and(|value| !(0..=10_000_000).contains(&value))
        {
            return Err(AppError::Validation(
                "Selector variance_threshold must be between 0 and 10,000,000".into(),
            ));
        }
        Ok(())
    }

    pub fn validate_for_mutation(&self) -> AppResult<()> {
        self.validate()?;
        let has_filter = self.category_subtree.is_some()
            || self.active.is_some()
            || self.track_inventory.is_some()
            || self.text.is_some()
            || self.supplier_id.is_some()
            || self.below_reorder.is_some()
            || self.variance_threshold.is_some();
        if !has_filter && !self.all_records {
            return Err(AppError::Validation(
                "Bulk mutations require a selector filter or explicit all_records=true".into(),
            ));
        }
        Ok(())
    }

    /// Build the CTE + WHERE for this selector. Always excludes soft-deleted rows.
    pub fn compile(&self) -> Compiled {
        let mut cte = String::new();
        let mut conds: Vec<String> = vec!["p.deleted_at IS NULL".into()];
        let mut binds: Vec<String> = Vec::new();

        if let Some(cat) = &self.category_subtree {
            cte = "WITH RECURSIVE subtree(category_id) AS (\
                     SELECT category_id FROM categories WHERE category_id = ? \
                     UNION ALL \
                     SELECT c.category_id FROM categories c \
                       JOIN subtree s ON c.parent_category_id = s.category_id) "
                .into();
            binds.push(cat.clone());
            conds.push("p.category_id IN (SELECT category_id FROM subtree)".into());
        }
        if let Some(active) = self.active {
            conds.push(format!("p.is_active = {}", if active { 1 } else { 0 }));
        }
        if let Some(track_inventory) = self.track_inventory {
            conds.push(format!(
                "p.track_inventory = {}",
                if track_inventory { 1 } else { 0 }
            ));
        }
        if let Some(text) = &self.text {
            conds.push("(p.name LIKE ? OR p.sku LIKE ? OR p.barcode LIKE ?)".into());
            let like = format!("%{}%", text);
            binds.push(like.clone());
            binds.push(like.clone());
            binds.push(like);
        }
        if let Some(sid) = &self.supplier_id {
            conds.push("p.default_supplier_id = ?".into());
            binds.push(sid.clone());
        }
        Compiled {
            cte,
            where_sql: conds.join(" AND "),
            binds,
        }
    }

    /// Returns true if below_reorder or variance_threshold are set, meaning callers
    /// must add a stock_levels JOIN before executing the compiled WHERE fragment.
    pub fn needs_stock_join(&self) -> bool {
        self.below_reorder.is_some() || self.variance_threshold.is_some()
    }

    /// Count active promotional prices for products matching this selector.
    pub async fn count_promotional(&self, pool: &SqlitePool) -> AppResult<i64> {
        self.validate()?;
        let c = self.compile();
        let now = chrono::Utc::now().to_rfc3339();
        let sql = format!(
            "{} SELECT COUNT(*) FROM product_prices pp \
             JOIN products p ON p.product_id = pp.product_id \
             WHERE pp.price_type = 'promotional' AND pp.effective_to > ? AND {}",
            c.cte, c.where_sql
        );
        let mut q = sqlx::query_scalar::<_, i64>(&sql);
        q = q.bind(&now);
        for b in &c.binds {
            q = q.bind(b);
        }
        Ok(q.fetch_one(pool).await?)
    }

    /// Count products matching this selector.
    pub async fn count(&self, pool: &SqlitePool) -> AppResult<i64> {
        self.validate()?;
        let c = self.compile();
        let sql = format!(
            "{} SELECT COUNT(*) FROM products p WHERE {}",
            c.cte, c.where_sql
        );
        let mut q = sqlx::query_scalar::<_, i64>(&sql);
        for b in &c.binds {
            q = q.bind(b);
        }
        Ok(q.fetch_one(pool).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let now = "2026-01-01T00:00:00Z";
        for (id, parent) in [
            ("toys", None),
            ("girls", Some("toys")),
            ("dolls", Some("girls")),
            ("boys", Some("toys")),
        ] {
            sqlx::query("INSERT INTO categories (category_id,parent_category_id,name,sort_order,is_active,created_at,updated_at) VALUES (?,?,?,0,1,?,?)")
                .bind(id).bind(parent).bind(id).bind(now).bind(now).execute(&pool).await.unwrap();
        }
        for (id, cat) in [
            ("p_g1", "girls"),
            ("p_g2", "girls"),
            ("p_d1", "dolls"),
            ("p_b1", "boys"),
        ] {
            sqlx::query("INSERT INTO products (product_id,category_id,name,is_active,currency,reorder_point,created_at,updated_at) VALUES (?,?,?,1,'BHD',0,?,?)")
                .bind(id).bind(cat).bind(id).bind(now).bind(now).execute(&pool).await.unwrap();
        }
        pool
    }

    #[tokio::test]
    async fn subtree_includes_descendants_excludes_siblings() {
        let pool = setup().await;
        let sel = Selector {
            category_subtree: Some("girls".into()),
            ..Default::default()
        };
        let count = sel.count(&pool).await.unwrap();
        assert_eq!(count, 3); // p_g1, p_g2, p_d1 — NOT p_b1
    }

    #[tokio::test]
    async fn empty_selector_matches_all_active() {
        let pool = setup().await;
        let sel = Selector::default();
        assert_eq!(sel.count(&pool).await.unwrap(), 4);
    }

    #[test]
    fn selector_rejects_unknown_and_oversized_fields() {
        assert!(serde_json::from_value::<Selector>(serde_json::json!({"admin": true})).is_err());
        let selector = Selector {
            text: Some("x".repeat(201)),
            ..Default::default()
        };
        assert!(selector.validate().is_err());
    }

    #[test]
    fn mutation_selector_requires_explicit_all_records() {
        assert!(Selector::default().validate_for_mutation().is_err());
        assert!(Selector {
            all_records: true,
            ..Default::default()
        }
        .validate_for_mutation()
        .is_ok());
    }
}
