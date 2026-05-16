use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentInput {
    pub method: String,   // cash/card/wallet/other
    pub amount_minor: i64,
    pub tendered_minor: Option<i64>,  // cash only
    pub external_reference: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaleResult {
    pub sale_id: String,
    pub receipt_number: String,
    pub net_total_minor: i64,
    pub tax_total_minor: i64,
    pub discount_total_minor: i64,
    pub currency: String,
    pub payments: Vec<PaymentSummary>,
    pub items: Vec<SaleItemSummary>,
    pub cashier_name: String,
    pub branch_name: String,
    pub sold_at: String,
    pub business_date: String,
    pub created_offline: bool,
    pub low_stock_alerts: Vec<crate::domain::product::LowStockAlert>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentSummary {
    pub method: String,
    pub amount_minor: i64,
    pub change_minor: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaleItemSummary {
    pub product_name: String,
    pub quantity: String,
    pub unit_price_minor: i64,
    pub line_total_minor: i64,
    pub tax_amount_minor: i64,
}

/// SaleRow — used for cross-device reporting queries in Phase 3 (Supabase direct query).
/// Not yet fetched at the command layer; kept for forward-compatibility.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SaleRow {
    pub sale_id: String,
    pub receipt_number: String,
    pub net_total_minor: i64,
    pub tax_total_minor: i64,
    pub discount_total_minor: i64,
    pub gross_total_minor: i64,
    pub currency: String,
    pub sold_at: String,
    pub business_date: String,
    pub sync_status: String,
}
