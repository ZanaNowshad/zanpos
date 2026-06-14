use crate::errors::AppResult;
use sqlx::SqlitePool;

/// Declarative product filter. A1 supports the fields the first demo needs;
/// price_range / stock / not_sold_since are added in later phases.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Selector {
    /// Resolve this category AND all descendants (adjacency list → recursive CTE).
    pub category_subtree: Option<String>,
    /// Restrict to active (true) or inactive (false) products.
    pub active: Option<bool>,
    /// Case-insensitive LIKE over name / sku / barcode.
    pub text: Option<String>,
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
        if let Some(text) = &self.text {
            conds.push("(p.name LIKE ? OR p.sku LIKE ? OR p.barcode LIKE ?)".into());
            let like = format!("%{}%", text);
            binds.push(like.clone());
            binds.push(like.clone());
            binds.push(like);
        }
        Compiled {
            cte,
            where_sql: conds.join(" AND "),
            binds,
        }
    }

    /// Count products matching this selector.
    pub async fn count(&self, pool: &SqlitePool) -> AppResult<i64> {
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
}
