use tauri::State;
use crate::domain::report::TodaySummary;
use crate::db::repositories::report_repo;
use crate::errors::AppError;
use crate::AppState;

#[tauri::command]
pub async fn report_today(
    branch_id: String,
    business_date: String,
    state: State<'_, AppState>,
) -> Result<TodaySummary, AppError> {
    report_repo::today_summary(&state.db, &branch_id, &business_date).await
}

#[tauri::command]
pub async fn db_integrity_check(state: State<'_, AppState>) -> Result<String, AppError> {
    let result: String = sqlx::query_scalar("PRAGMA integrity_check")
        .fetch_one(&state.db)
        .await?;
    Ok(result)
}
