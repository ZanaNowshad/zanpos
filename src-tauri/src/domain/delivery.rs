use serde::{Deserialize, Serialize};

/// Input provided by the cashier when marking a sale as delivery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryInput {
    pub customer_id: Option<String>,
    pub customer_name: Option<String>,
    pub contact_number: String,      // E.164: +97333050666, validated by frontend
    pub house_number: Option<String>,
    pub area: Option<String>,
    pub address_text: String,        // required
    pub delivery_note: Option<String>,
    pub delivery_staff_name: Option<String>,
    pub expected_payment_method: String, // cash | card | wallet
}

/// Full delivery order row returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DeliveryRow {
    pub delivery_id: String,
    pub sale_id: String,
    pub receipt_number: String,
    pub customer_id: Option<String>,
    pub customer_name: Option<String>,
    pub contact_number: String,
    pub house_number: Option<String>,
    pub area: Option<String>,
    pub address_text: String,
    pub delivery_note: Option<String>,
    pub delivery_staff_name: Option<String>,
    pub expected_payment_method: String,
    pub payment_status: String,
    pub delivery_status: String,
    pub amount_minor: i64,
    pub currency: String,
    pub paid_confirmed_by_user_id: Option<String>,
    pub paid_confirmed_at: Option<String>,
    pub payment_reference: Option<String>,
    pub payment_note: Option<String>,
    pub created_by_user_id: String,
    pub branch_id: String,
    pub device_id: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Filter for listing delivery orders.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliveryListFilter {
    pub payment_status: Option<String>,
    pub delivery_status: Option<String>,
    pub date_from: Option<String>,
    pub date_to: Option<String>,
    pub staff_name: Option<String>,
    pub contact_search: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// Input for admin payment confirmation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfirmPaymentInput {
    pub delivery_id: String,
    pub confirmed_by_user_id: String,
    pub payment_reference: Option<String>,
    pub payment_note: Option<String>,
}

/// Input for updating delivery status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateDeliveryStatusInput {
    pub delivery_id: String,
    pub delivery_status: String,  // pending | dispatched | delivered | cancelled
    pub actor_user_id: String,
}

/// Input for cancelling a delivery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelDeliveryInput {
    pub delivery_id: String,
    pub actor_user_id: String,
}

/// Input for admin reverting a paid delivery back to unpaid.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RevertPaymentInput {
    pub delivery_id: String,
    pub actor_user_id: String,
    pub reason: Option<String>,
}
