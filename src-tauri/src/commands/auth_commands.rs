use tauri::State;
use crate::domain::auth::{UserSummary, SessionUser};
use crate::db::repositories::auth_repo;
use crate::errors::AppError;
use crate::AppState;

#[tauri::command]
pub async fn auth_list_users(state: State<'_, AppState>) -> Result<Vec<UserSummary>, AppError> {
    auth_repo::list_active_users(&state.db).await
}

#[derive(serde::Deserialize)]
pub struct LoginInput {
    pub username: String,
    pub pin: String,
}

#[tauri::command]
pub async fn auth_login_pin(input: LoginInput, state: State<'_, AppState>) -> Result<SessionUser, AppError> {
    auth_repo::login_pin(&state.db, &input.username, &input.pin).await
}
