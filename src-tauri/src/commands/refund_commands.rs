use tauri::State;
use crate::domain::refund::{SaleForRefund, RefundItemInput, RefundResult};
use crate::domain::sale::SaleResult;
use crate::db::repositories::refund_repo;
use crate::errors::AppError;
use crate::AppState;

/// Reason code validation mirroring refund_repo::create_refund guard.
/// `pub` so unit tests can reach it; only called at test-time.
#[cfg_attr(not(test), allow(dead_code))]
pub fn sanitise_reason_code(code: &str) -> &str {
    match code {
        "customer_return" | "defective" | "wrong_item" | "exchange" | "other" => code,
        _ => "other",
    }
}

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
    /// Structured reason code: customer_return | defective | wrong_item | exchange | other
    pub return_reason_code: Option<String>,
    pub created_by_user_id: String,
}

#[tauri::command]
pub async fn refund_create(input: CreateRefundInput, state: State<'_, AppState>) -> Result<RefundResult, AppError> {
    let reason_code = input.return_reason_code.as_deref().unwrap_or("other");
    refund_repo::create_refund(
        &state.db,
        &input.original_sale_id,
        input.items,
        &input.reason,
        reason_code,
        &input.created_by_user_id,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_reason_codes_pass_through() {
        assert_eq!(sanitise_reason_code("customer_return"), "customer_return");
        assert_eq!(sanitise_reason_code("defective"),       "defective");
        assert_eq!(sanitise_reason_code("wrong_item"),      "wrong_item");
        assert_eq!(sanitise_reason_code("exchange"),        "exchange");
        assert_eq!(sanitise_reason_code("other"),           "other");
    }

    #[test]
    fn unknown_reason_code_falls_back_to_other() {
        assert_eq!(sanitise_reason_code("SCAM"),      "other");
        assert_eq!(sanitise_reason_code(""),          "other");
        assert_eq!(sanitise_reason_code("undefined"), "other");
    }
}
