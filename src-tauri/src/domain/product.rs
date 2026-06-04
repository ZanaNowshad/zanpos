use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Product {
    pub product_id: String,
    pub category_id: String,
    pub name: String,
    pub sku: Option<String>,
    pub barcode: Option<String>,
    pub description: Option<String>,
    pub track_inventory: bool,
    pub allow_decimal_quantity: bool,
    pub is_active: bool,
    pub tax_rule_id: Option<String>,
    pub cost_minor: Option<i64>,
    pub currency: String,
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
    pub reorder_point: i64,
    pub image_path: Option<String>,
    pub default_supplier_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductWithPrice {
    #[serde(flatten)]
    pub product: Product,
    pub price_minor: i64,
    pub tax_rate_basis_points: i64,
    pub tax_inclusive: bool,
    pub category_name: String,
    pub quantity_on_hand: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LowStockAlert {
    pub product_id: String,
    pub product_name: String,
    pub quantity_on_hand: String,
    pub reorder_point: i64,
}

/// TaxRule domain struct — used for typed deserialization in Phase 3 sync inbox.
/// Not yet constructed at the command layer; the admin commands use `TaxRuleRow` instead.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxRule {
    pub tax_rule_id: String,
    pub name: String,
    pub rate_basis_points: i64,
    pub inclusive: bool,
}
