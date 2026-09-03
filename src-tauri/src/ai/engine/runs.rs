use crate::errors::AppResult;
use sqlx::SqlitePool;

// Retained: `sqlx::FromRow` populates every column so the row can be mapped
// in one place; several fields are only ever serialised outward.
#[allow(dead_code)]
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Run {
    pub run_id: String,
    pub op_id: String,
    pub selector_json: String,
    pub params_json: String,
    pub status: String,
    pub total_count: i64,
    pub done_count: i64,
    pub checkpoint_cursor: Option<String>,
    pub created_by: String,
    pub branch_id: String,
}

/// Create a run row in 'previewing' status. Returns the new run_id (ULID).
pub async fn create_run(
    pool: &SqlitePool,
    op_id: &str,
    selector_json: &str,
    params_json: &str,
    total_count: i64,
    created_by: &str,
    branch_id: &str,
) -> AppResult<String> {
    let run_id = ulid::Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO ai_runs (run_id,op_id,selector_json,params_json,status,total_count,done_count,created_by,branch_id,created_at,updated_at) \
         VALUES (?,?,?,?,'previewing',?,0,?,?,?,?)",
    )
    .bind(&run_id)
    .bind(op_id)
    .bind(selector_json)
    .bind(params_json)
    .bind(total_count)
    .bind(created_by)
    .bind(branch_id)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(run_id)
}

pub async fn set_status(pool: &SqlitePool, run_id: &str, status: &str) -> AppResult<()> {
    set_status_guarded(pool, run_id, status, None).await
}

pub async fn set_status_guarded(
    pool: &SqlitePool,
    run_id: &str,
    status: &str,
    expected_from: Option<&str>,
) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let sql = if let Some(from) = expected_from {
        sqlx::query("UPDATE ai_runs SET status=?, updated_at=? WHERE run_id=? AND status=?")
            .bind(status)
            .bind(&now)
            .bind(run_id)
            .bind(from)
    } else {
        sqlx::query("UPDATE ai_runs SET status=?, updated_at=? WHERE run_id=?")
            .bind(status)
            .bind(&now)
            .bind(run_id)
    };
    sql.execute(pool).await?;
    Ok(())
}

pub async fn set_cancelled(pool: &SqlitePool, run_id: &str) -> AppResult<()> {
    // Allow cancelling from "previewing" → "cancelled" directly, or from
    // "executing" → "cancelling" (the batch loop polls for this transition).
    let run = get_run(pool, run_id).await?;
    let target = match run.status.as_str() {
        "previewing" => "cancelled",
        "executing" => "cancelling",
        other => {
            return Err(crate::errors::AppError::Conflict(format!(
                "Run {} is in state '{}', cannot cancel",
                run_id, other
            )));
        }
    };
    set_status(pool, run_id, target).await
}

/// Returns the current status of a run without full deserialization.
/// Used by batch loops to poll for cancellation. Returns "" if not found.
pub async fn status_only(pool: &SqlitePool, run_id: &str) -> String {
    sqlx::query_scalar("SELECT status FROM ai_runs WHERE run_id=?")
        .bind(run_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .unwrap_or_default()
}

pub async fn set_failed(pool: &SqlitePool, run_id: &str, error: &str) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE ai_runs SET status='failed', error=?, updated_at=? WHERE run_id=?")
        .bind(error)
        .bind(&now)
        .bind(run_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_run(pool: &SqlitePool, run_id: &str) -> AppResult<Run> {
    sqlx::query_as::<_, Run>(
        "SELECT run_id,op_id,selector_json,params_json,status,total_count,done_count,
                checkpoint_cursor,created_by,branch_id FROM ai_runs WHERE run_id=?",
    )
    .bind(run_id)
    .fetch_one(pool)
    .await
    .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn pool() -> SqlitePool {
        let p = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&p).await.unwrap();
        p
    }

    #[tokio::test]
    async fn create_then_status_roundtrip() {
        let pool = pool().await;
        let id = create_run(&pool, "bulk_price_adjust", "{}", "{}", 312, "U1", "BRANCH1")
            .await
            .unwrap();
        let r = get_run(&pool, &id).await.unwrap();
        assert_eq!(r.status, "previewing");
        assert_eq!(r.total_count, 312);
        assert_eq!(r.branch_id, "BRANCH1");
        set_status(&pool, &id, "done").await.unwrap();
        assert_eq!(get_run(&pool, &id).await.unwrap().status, "done");
    }
}
