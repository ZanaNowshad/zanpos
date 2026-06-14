use crate::ai::engine::selector::Selector;
use crate::ai::engine::{apply_price, runs, PriceOp};
use crate::errors::AppResult;
use sqlx::SqlitePool;

/// One product's pre-change price, for undo.
#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Reverse {
    product_id: String,
    prev_price_minor: i64,
}

/// Execute a price adjustment across every product matching `selector`, in
/// keyset batches. Each batch is one transaction: it rewrites prices and writes
/// a reverse-snapshot, then advances the run checkpoint. Returns rows changed.
pub async fn execute_price_adjust(
    pool: &SqlitePool,
    run_id: &str,
    selector: &Selector,
    op: &PriceOp,
    batch_size: i64,
) -> AppResult<i64> {
    runs::set_status(pool, run_id, "executing").await?;
    let c = selector.compile();
    let mut cursor = String::new();
    let mut batch_seq: i64 = 0;
    let mut total_changed: i64 = 0;

    loop {
        let sql = format!(
            "{} SELECT p.product_id, pp.price_minor FROM products p \
             JOIN product_prices pp ON pp.product_id = p.product_id \
               AND pp.price_type='selling' AND pp.effective_to IS NULL \
             WHERE {} AND p.product_id > ? ORDER BY p.product_id LIMIT ?",
            c.cte, c.where_sql
        );
        let mut q = sqlx::query_as::<_, (String, i64)>(&sql);
        for b in &c.binds {
            q = q.bind(b);
        }
        q = q.bind(&cursor).bind(batch_size);
        let rows: Vec<(String, i64)> = q.fetch_all(pool).await?;
        if rows.is_empty() {
            break;
        }

        let now = chrono::Utc::now().to_rfc3339();
        let mut tx = pool.begin().await?;
        let mut reverses: Vec<Reverse> = Vec::with_capacity(rows.len());

        for (pid, current) in &rows {
            let new_price = apply_price(*current, op);
            sqlx::query(
                "UPDATE product_prices SET effective_to=?, sync_status='pending' \
                 WHERE product_id=? AND price_type='selling' AND effective_to IS NULL",
            )
            .bind(&now)
            .bind(pid)
            .execute(&mut *tx)
            .await?;
            let price_id = ulid::Ulid::new().to_string();
            sqlx::query(
                "INSERT INTO product_prices \
                 (price_id,product_id,branch_id,price_type,price_minor,currency,effective_from,created_by_user_id,created_by_ai_action_id,created_at) \
                 VALUES (?,?,NULL,'selling',?,'BHD',?,?,?,?)",
            )
            .bind(&price_id)
            .bind(pid)
            .bind(new_price)
            .bind(&now)
            .bind("AI_ADMIN")
            .bind(run_id)
            .bind(&now)
            .execute(&mut *tx)
            .await?;
            reverses.push(Reverse {
                product_id: pid.clone(),
                prev_price_minor: *current,
            });
        }

        let entry_id = ulid::Ulid::new().to_string();
        let reverse_json = serde_json::to_string(&reverses).unwrap_or_else(|_| "[]".into());
        sqlx::query(
            "INSERT INTO ai_run_undo_log (entry_id,run_id,batch_seq,reverse_json,applied,created_at) VALUES (?,?,?,?,0,?)",
        )
        .bind(&entry_id)
        .bind(run_id)
        .bind(batch_seq)
        .bind(&reverse_json)
        .bind(&now)
        .execute(&mut *tx)
        .await?;

        let last_pid = rows.last().map(|r| r.0.clone()).unwrap();
        total_changed += rows.len() as i64;
        sqlx::query(
            "UPDATE ai_runs SET done_count=?, checkpoint_cursor=?, updated_at=? WHERE run_id=?",
        )
        .bind(total_changed)
        .bind(&last_pid)
        .bind(&now)
        .bind(run_id)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        cursor = last_pid;
        batch_seq += 1;
    }

    runs::set_status(pool, run_id, "done").await?;
    Ok(total_changed)
}

/// Reverse a completed run: replay every un-applied undo-log entry in reverse
/// batch order, restoring each product's prior selling price. Idempotent per entry.
pub async fn undo_run(pool: &SqlitePool, run_id: &str) -> AppResult<i64> {
    let entries: Vec<(String, String)> = sqlx::query_as(
        "SELECT entry_id, reverse_json FROM ai_run_undo_log WHERE run_id=? AND applied=0 ORDER BY batch_seq DESC",
    )
    .bind(run_id)
    .fetch_all(pool)
    .await?;

    let now = chrono::Utc::now().to_rfc3339();
    let mut restored = 0i64;
    let mut tx = pool.begin().await?;
    for (entry_id, reverse_json) in &entries {
        let reverses: Vec<Reverse> = serde_json::from_str(reverse_json).unwrap_or_default();
        for r in &reverses {
            sqlx::query(
                "UPDATE product_prices SET effective_to=?, sync_status='pending' \
                 WHERE product_id=? AND price_type='selling' AND effective_to IS NULL",
            )
            .bind(&now)
            .bind(&r.product_id)
            .execute(&mut *tx)
            .await?;
            let price_id = ulid::Ulid::new().to_string();
            sqlx::query(
                "INSERT INTO product_prices \
                 (price_id,product_id,branch_id,price_type,price_minor,currency,effective_from,created_by_user_id,created_by_ai_action_id,created_at) \
                 VALUES (?,?,NULL,'selling',?,'BHD',?,?,?,?)",
            )
            .bind(&price_id)
            .bind(&r.product_id)
            .bind(r.prev_price_minor)
            .bind(&now)
            .bind("AI_ADMIN")
            .bind(run_id)
            .bind(&now)
            .execute(&mut *tx)
            .await?;
            restored += 1;
        }
        sqlx::query("UPDATE ai_run_undo_log SET applied=1 WHERE entry_id=?")
            .bind(entry_id)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("UPDATE ai_runs SET status='undone', updated_at=? WHERE run_id=?")
        .bind(&now)
        .bind(run_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(restored)
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
            ("boys", Some("toys")),
        ] {
            sqlx::query("INSERT INTO categories (category_id,parent_category_id,name,sort_order,is_active,created_at,updated_at) VALUES (?,?,?,0,1,?,?)")
                .bind(id).bind(parent).bind(id).bind(now).bind(now).execute(&pool).await.unwrap();
        }
        for i in 0..250 {
            let pid = format!("g{:04}", i);
            seed_product(&pool, &pid, "girls", 12500, now).await;
        }
        seed_product(&pool, "b0001", "boys", 12500, now).await;
        pool
    }

    async fn seed_product(pool: &SqlitePool, pid: &str, cat: &str, price: i64, now: &str) {
        sqlx::query("INSERT INTO products (product_id,category_id,name,is_active,currency,reorder_point,created_at,updated_at) VALUES (?,?,?,1,'BHD',0,?,?)")
            .bind(pid).bind(cat).bind(pid).bind(now).bind(now).execute(pool).await.unwrap();
        let price_id = format!("pr_{}", pid);
        sqlx::query("INSERT INTO product_prices (price_id,product_id,branch_id,price_type,price_minor,currency,effective_from,created_by_user_id,created_at) VALUES (?,?,NULL,'selling',?,'BHD',?,'seed',?)")
            .bind(&price_id).bind(pid).bind(price).bind(now).bind(now).execute(pool).await.unwrap();
    }

    async fn current_price(pool: &SqlitePool, pid: &str) -> i64 {
        sqlx::query_scalar::<_, i64>(
            "SELECT price_minor FROM product_prices WHERE product_id=? AND price_type='selling' AND effective_to IS NULL",
        )
        .bind(pid)
        .fetch_one(pool)
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn bulk_adjust_changes_only_matched_with_exact_math() {
        let pool = setup().await;
        let sel = Selector {
            category_subtree: Some("girls".into()),
            ..Default::default()
        };
        let total = sel.count(&pool).await.unwrap();
        let run_id = runs::create_run(&pool, "bulk.price_adjust", "{}", "{}", total, "U1")
            .await
            .unwrap();

        let changed = execute_price_adjust(&pool, &run_id, &sel, &PriceOp::Percent(20.0), 100)
            .await
            .unwrap();

        assert_eq!(changed, 250);
        assert_eq!(current_price(&pool, "g0000").await, 15000);
        assert_eq!(current_price(&pool, "g0249").await, 15000);
        assert_eq!(current_price(&pool, "b0001").await, 12500);
        assert_eq!(runs::get_run(&pool, &run_id).await.unwrap().status, "done");
        assert_eq!(runs::get_run(&pool, &run_id).await.unwrap().done_count, 250);
    }

    #[tokio::test]
    async fn undo_restores_every_price() {
        let pool = setup().await;
        let sel = Selector {
            category_subtree: Some("girls".into()),
            ..Default::default()
        };
        let total = sel.count(&pool).await.unwrap();
        let run_id = runs::create_run(&pool, "bulk.price_adjust", "{}", "{}", total, "U1")
            .await
            .unwrap();
        execute_price_adjust(&pool, &run_id, &sel, &PriceOp::Percent(20.0), 100)
            .await
            .unwrap();
        assert_eq!(current_price(&pool, "g0000").await, 15000);

        let restored = undo_run(&pool, &run_id).await.unwrap();

        assert_eq!(restored, 250);
        assert_eq!(current_price(&pool, "g0000").await, 12500);
        assert_eq!(current_price(&pool, "g0249").await, 12500);
        assert_eq!(
            runs::get_run(&pool, &run_id).await.unwrap().status,
            "undone"
        );
    }
}
