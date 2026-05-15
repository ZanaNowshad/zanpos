use tauri::State;
use crate::domain::refund::{SaleForRefund, RefundItemInput, RefundResult};
use crate::domain::sale::SaleResult;
use crate::db::repositories::refund_repo;
use crate::errors::AppError;
use crate::AppState;

#[tauri::command]
pub async fn refund_get_sale(receipt_number: String, state: State<'_, AppState>) -> Result<SaleForRefund, AppError> {
    refund_repo::get_sale_by_receipt(&state.db, &receipt_number).await
}

#[tauri::command]
pub async fn receipt_reprint(receipt_number: String, state: State<'_, AppState>) -> Result<SaleResult, AppError> {
    refund_repo::get_sale_result_by_receipt(&state.db, &receipt_number).await
}

#[derive(serde::Deserialize)]
pub struct CreateRefundInput {
    pub original_sale_id: String,
    pub items: Vec<RefundItemInput>,
    pub reason: String,
    pub created_by_user_id: String,
}

#[tauri::command]
pub async fn refund_create(input: CreateRefundInput, state: State<'_, AppState>) -> Result<RefundResult, AppError> {
    refund_repo::create_refund(
        &state.db,
        &input.original_sale_id,
        input.items,
        &input.reason,
        &input.created_by_user_id,
    )
    .await
}
