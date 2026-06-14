use crate::errors::AppResult;
use sqlx::SqlitePool;

#[derive(Debug, Clone)]
pub struct Run {
    pub run_id: String,
    pub op_id: String,
    pub selector_json: String,
    pub params_json: String,
    pub status: String,
    pub total_count: i64,
    pub done_count: i64,
    pub checkpoint_cursor: Option<String>,
}

/// Create a run row in 'previewing' status. Returns the new run_id (ULID).
pub async fn create_run(
    pool: &SqlitePool,
    op_id: &str,
    selector_json: &str,
    params_json: &str,
    total_count: i64,
    created_by: &str,
) -> AppResult<String> {
    let run_id = ulid::Ulid::new().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO ai_runs (run_id,op_id,selector_json,params_json,status,total_count,done_count,created_by,created_at,updated_at) \
         VALUES (?,?,?,?,'previewing',?,0,?,?,?)",
    )
    .bind(&run_id)
    .bind(op_id)
    .bind(selector_json)
    .bind(params_json)
    .bind(total_count)
    .bind(created_by)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(run_id)
}

pub async fn set_status(pool: &SqlitePool, run_id: &str, status: &str) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE ai_runs SET status=?, updated_at=? WHERE run_id=?")
        .bind(status)
        .bind(&now)
        .bind(run_id)
        .execute(pool)
        .await?;
    Ok(())
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
    let row = sqlx::query_as::<_, (String, String, String, String, String, i64, i64, Option<String>)>(
        "SELECT run_id,op_id,selector_json,params_json,status,total_count,done_count,checkpoint_cursor FROM ai_runs WHERE run_id=?",
    )
    .bind(run_id)
    .fetch_one(pool)
    .await?;
    Ok(Run {
        run_id: row.0,
        op_id: row.1,
        selector_json: row.2,
        params_json: row.3,
        status: row.4,
        total_count: row.5,
        done_count: row.6,
        checkpoint_cursor: row.7,
    })
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
        let id = create_run(&pool, "bulk.price_adjust", "{}", "{}", 312, "U1")
            .await
            .unwrap();
        let r = get_run(&pool, &id).await.unwrap();
        assert_eq!(r.status, "previewing");
        assert_eq!(r.total_count, 312);
        set_status(&pool, &id, "done").await.unwrap();
        assert_eq!(get_run(&pool, &id).await.unwrap().status, "done");
    }
}
