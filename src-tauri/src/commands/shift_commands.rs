use tauri::State;
use crate::domain::shift::Shift;
use crate::db::repositories::shift_repo;
use crate::errors::AppError;
use crate::AppState;

#[tauri::command]
pub async fn shift_get_active(device_id: String, state: State<'_, AppState>) -> Result<Option<Shift>, AppError> {
    shift_repo::get_active_shift(&state.db, &device_id).await
}

#[derive(serde::Deserialize)]
pub struct OpenShiftInput {
    pub branch_id: String,
    pub device_id: String,
    pub cashier_user_id: String,
    pub opening_cash_minor: i64,
}

#[tauri::command]
pub async fn shift_open(input: OpenShiftInput, state: State<'_, AppState>) -> Result<Shift, AppError> {
    shift_repo::open_shift(
        &state.db,
        &input.branch_id,
        &input.device_id,
        &input.cashier_user_id,
        input.opening_cash_minor,
    )
    .await
}

#[derive(serde::Deserialize)]
pub struct CloseShiftInput {
    pub shift_id: String,
    pub counted_cash_minor: Option<i64>,
    pub notes: Option<String>,
}

#[tauri::command]
pub async fn shift_close(input: CloseShiftInput, state: State<'_, AppState>) -> Result<Shift, AppError> {
    shift_repo::close_shift(&state.db, &input.shift_id, input.counted_cash_minor, input.notes).await
}
