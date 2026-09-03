use crate::ai::engine::ops::{BatchRow, Operation};
use crate::ai::engine::selector::Selector;
use crate::ai::engine::{apply_price, runs, PriceOp};
use crate::errors::AppResult;
use sqlx::SqlitePool;

const MAX_AI_BULK_AFFECTED: i64 = 10_000;
const MAX_AI_STOCK_SET_AFFECTED: i64 = 50_000;

pub(crate) fn max_bulk_affected(tool_name: &str) -> i64 {
    if tool_name == "bulk_stock_set" {
        MAX_AI_STOCK_SET_AFFECTED
    } else {
        MAX_AI_BULK_AFFECTED
    }
}

async fn validate_bulk_start(
    pool: &SqlitePool,
    selector: &Selector,
    tool_name: &str,
) -> AppResult<i64> {
    selector.validate_for_mutation()?;
    crate::ai::tool_policy::require_tool_enabled(pool, tool_name).await?;
    let count = selector.count(pool).await?;
    if count == 0 {
        return Err(crate::errors::AppError::Validation(
            "Bulk mutation selector matched no records".into(),
        ));
    }
    let maximum = max_bulk_affected(tool_name);
    if count > maximum {
        return Err(crate::errors::AppError::Validation(format!(
            "Bulk mutation matches {count} records; maximum is {maximum}"
        )));
    }
    Ok(count)
}

/// One product's pre-change price, for undo.
#[derive(serde::Serialize, serde::Deserialize, Default)]
struct Reverse {
    product_id: String,
    prev_price_minor: i64,
}

/// Execute a price adjustment across every product matching `selector`, in
/// keyset batches. Each batch is one transaction: it rewrites prices and writes
/// a reverse-snapshot, then advances the run checkpoint. Returns rows changed.
/// `on_progress(done, total)` is called after each committed batch.
pub async fn execute_price_adjust(
    pool: &SqlitePool,
    run_id: &str,
    selector: &Selector,
    op: &PriceOp,
    batch_size: i64,
    on_progress: impl Fn(i64, i64) + Send,
) -> AppResult<i64> {
    runs::set_status_guarded(pool, run_id, "executing", Some("previewing")).await?;
    let total_expected = validate_bulk_start(pool, selector, "bulk_price_adjust").await?;
    let c = selector.compile();
    let mut cursor = String::new();
    let mut batch_seq: i64 = 0;
    let mut total_changed: i64 = 0;

    loop {
        crate::ai::tool_policy::require_tool_enabled(pool, "bulk_price_adjust").await?;
        let sql = format!(
            "{} SELECT p.product_id, pp.price_minor FROM products p \
             JOIN v_current_selling_price pp ON pp.product_id = p.product_id \
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
            .bind(crate::ai::tool_policy::current_actor_id().ok_or_else(|| crate::errors::AppError::Permission("Missing authenticated AI actor context".into()))?)
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
        on_progress(total_changed, total_expected);
        cursor = last_pid;
        batch_seq += 1;

        // Poll for cancellation between batches so long-running runs can be
        // interrupted without leaving partial state.
        if runs::status_only(pool, run_id).await == "cancelling" {
            let _ = runs::set_status_guarded(pool, run_id, "cancelled", Some("cancelling")).await;
            return Ok(total_changed);
        }
    }

    runs::set_status_guarded(pool, run_id, "done", Some("executing"))
        .await
        .inspect_err(|e| tracing::error!("FATAL: failed to set run {run_id} status to done: {e}"))
        .ok();
    Ok(total_changed)
}

/// Execute an arbitrary engine Operation across every entity matching `selector`,
/// in keyset batches. Writes undo-log entries so the run can be reversed later.
/// `on_progress(done, total)` is called after each committed batch.
pub async fn execute_op(
    pool: &SqlitePool,
    run_id: &str,
    op: &dyn Operation,
    selector: &Selector,
    input: &serde_json::Value,
    batch_size: i64,
    on_progress: impl Fn(i64, i64) + Send,
) -> AppResult<i64> {
    runs::set_status_guarded(pool, run_id, "executing", Some("previewing")).await?;
    let total_expected = validate_bulk_start(pool, selector, op.id()).await?;
    let c = selector.compile();
    let mut cursor = String::new();
    let mut batch_seq: i64 = 0;
    let mut total_changed: i64 = 0;

    loop {
        crate::ai::tool_policy::require_tool_enabled(pool, op.id()).await?;
        let sql = format!(
            "{} SELECT p.product_id FROM products p WHERE {} AND p.product_id > ? ORDER BY p.product_id LIMIT ?",
            c.cte, c.where_sql
        );
        let mut q = sqlx::query_as::<_, (String,)>(&sql);
        for b in &c.binds {
            q = q.bind(b);
        }
        q = q.bind(&cursor).bind(batch_size);
        let rows: Vec<(String,)> = q.fetch_all(pool).await?;
        if rows.is_empty() {
            break;
        }

        let stock_values = if op.id() == "bulk_stock_set" {
            let branch_id = crate::ai::tool_policy::current_branch_id().ok_or_else(|| {
                crate::errors::AppError::Permission(
                    "Missing authenticated AI branch context".into(),
                )
            })?;
            let mut builder = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
                "SELECT product_id, quantity_on_hand FROM stock_levels WHERE branch_id = ",
            );
            builder.push_bind(branch_id);
            builder.push(" AND product_id IN (");
            let mut separated = builder.separated(", ");
            for (product_id,) in &rows {
                separated.push_bind(product_id);
            }
            separated.push_unseparated(")");
            builder
                .build_query_as::<(String, String)>()
                .fetch_all(pool)
                .await?
                .into_iter()
                .collect::<std::collections::HashMap<_, _>>()
        } else {
            std::collections::HashMap::new()
        };
        let batch: Vec<BatchRow> = rows
            .iter()
            .map(|(id,)| BatchRow {
                entity_id: id.clone(),
                current_value: None,
                current_text: stock_values.get(id).cloned(),
            })
            .collect();

        let now = chrono::Utc::now().to_rfc3339();
        let mut tx = pool.begin().await?;
        let result = op.commit_batch(&mut tx, input, &batch).await?;

        let entry_id = ulid::Ulid::new().to_string();
        let reverse_json = serde_json::to_string(&batch).unwrap_or_else(|_| "[]".into());
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
        total_changed += result.rows_changed;
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
        on_progress(total_changed, total_expected);
        cursor = last_pid;
        batch_seq += 1;

        if runs::status_only(pool, run_id).await == "cancelling" {
            let _ = runs::set_status_guarded(pool, run_id, "cancelled", Some("cancelling")).await;
            return Ok(total_changed);
        }
    }

    runs::set_status_guarded(pool, run_id, "done", Some("executing"))
        .await
        .inspect_err(|e| tracing::error!("FATAL: failed to set run {run_id} status to done: {e}"))
        .ok();
    Ok(total_changed)
}

/// Reverse a completed run using the operation's verified run-level undo path.
pub async fn undo_run(pool: &SqlitePool, run_id: &str) -> AppResult<i64> {
    let op_id: String = sqlx::query_scalar("SELECT op_id FROM ai_runs WHERE run_id=?")
        .bind(run_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| crate::errors::AppError::NotFound("AI run not found".into()))?;
    match op_id.as_str() {
        "bulk_price_adjust" => undo_price_run(pool, run_id).await,
        "bulk_stock_set" => undo_stock_set_run(pool, run_id).await,
        _ => Err(crate::errors::AppError::Validation(format!(
            "Run operation '{op_id}' does not have a verified undo implementation"
        ))),
    }
}

async fn pending_undo_entries(pool: &SqlitePool, run_id: &str) -> AppResult<Vec<(String, String)>> {
    Ok(sqlx::query_as(
        "SELECT entry_id, reverse_json FROM ai_run_undo_log
         WHERE run_id=? AND applied=0 ORDER BY batch_seq DESC",
    )
    .bind(run_id)
    .fetch_all(pool)
    .await?)
}

async fn undo_price_run(pool: &SqlitePool, run_id: &str) -> AppResult<i64> {
    let entries = pending_undo_entries(pool, run_id).await?;
    let now = chrono::Utc::now().to_rfc3339();
    let mut restored = 0i64;
    let mut tx = pool.begin().await?;
    for (entry_id, reverse_json) in &entries {
        let reverses: Vec<Reverse> = serde_json::from_str(reverse_json).map_err(|e| {
            crate::errors::AppError::Validation(format!(
                "Undo data for entry {entry_id} is malformed; nothing was marked applied: {e}"
            ))
        })?;
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
            .bind(crate::ai::tool_policy::current_actor_id().ok_or_else(|| crate::errors::AppError::Permission("Missing authenticated AI actor context".into()))?)
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

async fn undo_stock_set_run(pool: &SqlitePool, run_id: &str) -> AppResult<i64> {
    let entries = pending_undo_entries(pool, run_id).await?;
    let branch_id = crate::ai::tool_policy::current_branch_id().ok_or_else(|| {
        crate::errors::AppError::Permission("Missing authenticated AI branch context".into())
    })?;
    let actor_id = crate::ai::tool_policy::current_actor_id().ok_or_else(|| {
        crate::errors::AppError::Permission("Missing authenticated AI actor context".into())
    })?;
    let mut restored = 0i64;

    for (entry_id, reverse_json) in entries {
        let reverses: Vec<BatchRow> = serde_json::from_str(&reverse_json).map_err(|error| {
            crate::errors::AppError::Validation(format!(
                "Undo data for entry {entry_id} is malformed; nothing was marked applied: {error}"
            ))
        })?;
        let mut tx = pool.begin().await?;
        let device_id: String = sqlx::query_scalar(
            "SELECT device_id FROM devices
             WHERE branch_id=? AND is_active=1 ORDER BY created_at LIMIT 1",
        )
        .bind(&branch_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| {
            crate::errors::AppError::Validation(
                "No active device exists for the authenticated branch".into(),
            )
        })?;
        let now = chrono::Utc::now().to_rfc3339();
        for reverse in reverses {
            let current: Option<String> = sqlx::query_scalar(
                "SELECT quantity_on_hand FROM stock_levels
                 WHERE product_id=? AND branch_id=?",
            )
            .bind(&reverse.entity_id)
            .bind(&branch_id)
            .fetch_optional(&mut *tx)
            .await?
            .flatten();
            let restored_text = reverse.current_text.clone().unwrap_or_else(|| "0".into());
            if reverse.current_text.is_some() {
                sqlx::query(
                    "UPDATE stock_levels
                     SET quantity_on_hand=?,updated_at=?,last_movement_at=?,sync_status='pending'
                     WHERE product_id=? AND branch_id=?",
                )
                .bind(&restored_text)
                .bind(&now)
                .bind(&now)
                .bind(&reverse.entity_id)
                .bind(&branch_id)
                .execute(&mut *tx)
                .await?;
            } else {
                sqlx::query("DELETE FROM stock_levels WHERE product_id=? AND branch_id=?")
                    .bind(&reverse.entity_id)
                    .bind(&branch_id)
                    .execute(&mut *tx)
                    .await?;
            }
            let current_number = current
                .as_deref()
                .unwrap_or("0")
                .parse::<f64>()
                .unwrap_or(0.0);
            let restored_number = restored_text.parse::<f64>().unwrap_or(0.0);
            sqlx::query(
                "INSERT INTO stock_movements
                 (movement_id,product_id,branch_id,device_id,origin_device_id,
                  movement_type,quantity_delta,quantity_after,reference_type,
                  reference_id,notes,created_by_user_id,created_at,sync_status)
                 VALUES (?,?,?,?,?,'stock_take',?,?,'ai_run_undo',?,
                         'Undo ZanAI bulk stock set',?,?,'pending')",
            )
            .bind(ulid::Ulid::new().to_string())
            .bind(&reverse.entity_id)
            .bind(&branch_id)
            .bind(&device_id)
            .bind(&device_id)
            .bind((restored_number - current_number).to_string())
            .bind(&restored_text)
            .bind(run_id)
            .bind(&actor_id)
            .bind(&now)
            .execute(&mut *tx)
            .await?;
            restored += 1;
        }
        sqlx::query("UPDATE ai_run_undo_log SET applied=1 WHERE entry_id=?")
            .bind(&entry_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
    }
    runs::set_status(pool, run_id, "undone").await?;
    Ok(restored)
}

#[cfg(test)]
#[path = "batch_tests.rs"]
mod tests;
