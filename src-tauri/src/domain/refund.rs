#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct SaleItemForRefund {
    pub sale_item_id: String,
    pub product_name_snapshot: String,
    pub quantity: String,
    pub unit_price_minor: i64,
    pub line_total_minor: i64,
}

#[derive(Debug, serde::Serialize)]
pub struct SaleForRefund {
    pub sale_id: String,
    pub receipt_number: String,
    pub net_total_minor: i64,
    pub currency: String,
    pub sold_at: String,
    pub cashier_name: String,
    pub status: String,
    pub items: Vec<SaleItemForRefund>,
}

#[derive(Debug, serde::Deserialize)]
pub struct RefundItemInput {
    pub sale_item_id: String,
    pub product_name_snapshot: String,
    pub quantity: String,
    pub unit_price_minor: i64,
    pub refund_amount_minor: i64,
}

#[derive(Debug, serde::Serialize)]
pub struct RefundResult {
    pub refund_id: String,
    pub refund_receipt_number: String,
    pub refund_total_minor: i64,
    pub currency: String,
    pub created_at: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct HeldCartSummary {
    pub held_cart_id: String,
    pub note: Option<String>,
    pub held_at: String,
    pub line_count: i64,
    pub estimated_total_minor: i64,
}
