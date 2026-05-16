use serde::{Deserialize, Serialize};
use crate::domain::money::{apply_discount_bp, calc_tax_exclusive};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CartLine {
    pub cart_line_id: String,
    pub product_id: Option<String>,
    pub product_name: String,
    pub sku: Option<String>,
    pub barcode: Option<String>,
    /// Stored as string to support decimal quantities
    pub quantity: String,
    pub unit_price_minor: i64,
    pub line_discount_minor: i64,
    /// Tax rule snapshot
    pub tax_rule_id: String,
    pub tax_rate_basis_points: i64,
    pub tax_inclusive: bool,
    pub tax_amount_minor: i64,
    pub line_total_minor: i64,
    pub note: Option<String>,
    pub voided: bool,
}

impl CartLine {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        product_id: Option<String>,
        product_name: String,
        sku: Option<String>,
        barcode: Option<String>,
        quantity: &str,
        unit_price_minor: i64,
        tax_rule_id: String,
        tax_rate_basis_points: i64,
        tax_inclusive: bool,
    ) -> Self {
        let qty: f64 = quantity.parse().unwrap_or(1.0);
        let subtotal = (unit_price_minor as f64 * qty) as i64;
        let tax_amount = if tax_inclusive {
            // Extract tax from inclusive price: tax = price * rate / (10000 + rate)
            subtotal * tax_rate_basis_points / (10_000 + tax_rate_basis_points)
        } else {
            calc_tax_exclusive(subtotal, tax_rate_basis_points)
        };
        let line_total = subtotal + if tax_inclusive { 0 } else { tax_amount };

        Self {
            cart_line_id: ulid::Ulid::new().to_string(),
            product_id,
            product_name,
            sku,
            barcode,
            quantity: quantity.to_string(),
            unit_price_minor,
            line_discount_minor: 0,
            tax_rule_id,
            tax_rate_basis_points,
            tax_inclusive,
            tax_amount_minor: tax_amount,
            line_total_minor: line_total,
            note: None,
            voided: false,
        }
    }

    pub fn recalculate(&mut self) {
        let qty: f64 = self.quantity.parse().unwrap_or(1.0);
        let subtotal = (self.unit_price_minor as f64 * qty) as i64;
        let discounted = subtotal - self.line_discount_minor;
        let tax_amount = if self.tax_inclusive {
            discounted * self.tax_rate_basis_points / (10_000 + self.tax_rate_basis_points)
        } else {
            calc_tax_exclusive(discounted, self.tax_rate_basis_points)
        };
        self.tax_amount_minor = tax_amount;
        self.line_total_minor = discounted + if self.tax_inclusive { 0 } else { tax_amount };
    }

    /// Apply a percentage discount (in basis points) to this line.
    /// Used by the batch-discount AI tool; not called from the UI command layer directly.
    #[allow(dead_code)]
    pub fn apply_discount_percent(&mut self, discount_basis_points: i64) {
        let qty: f64 = self.quantity.parse().unwrap_or(1.0);
        let subtotal = (self.unit_price_minor as f64 * qty) as i64;
        self.line_discount_minor = apply_discount_bp(subtotal, discount_basis_points);
        self.recalculate();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Cart {
    pub cart_id: String,
    pub branch_id: String,
    pub device_id: String,
    pub shift_id: String,
    pub cashier_user_id: String,
    pub lines: Vec<CartLine>,
    pub bill_discount_minor: i64,
}

impl Cart {
    pub fn new(branch_id: String, device_id: String, shift_id: String, cashier_user_id: String) -> Self {
        Self {
            cart_id: ulid::Ulid::new().to_string(),
            branch_id,
            device_id,
            shift_id,
            cashier_user_id,
            lines: Vec::new(),
            bill_discount_minor: 0,
        }
    }

    pub fn gross_total(&self) -> i64 {
        self.lines.iter().filter(|l| !l.voided).map(|l| l.line_total_minor).sum()
    }

    pub fn tax_total(&self) -> i64 {
        self.lines.iter().filter(|l| !l.voided).map(|l| l.tax_amount_minor).sum()
    }

    pub fn discount_total(&self) -> i64 {
        let item_discounts: i64 = self.lines.iter().filter(|l| !l.voided).map(|l| l.line_discount_minor).sum();
        item_discounts + self.bill_discount_minor
    }

    pub fn net_total(&self) -> i64 {
        (self.gross_total() - self.bill_discount_minor).max(0)
    }
}
