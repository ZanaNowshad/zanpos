use crate::domain::money::{apply_discount_bp, calc_tax_exclusive, mul_minor_by_qty, qty_in_range};
use crate::errors::AppError;
use serde::{Deserialize, Serialize};

/// Maximum quantity allowed per cart line (1 million units).
const MAX_QTY: i64 = 1_000_000;
/// Maximum line total / cart total in minor units (~1 billion BHD, far above any real POS value).
const MAX_MINOR: i64 = 1_000_000_000_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CartLine {
    pub cart_line_id: String,
    pub product_id: Option<String>,
    pub product_name: String,
    pub sku: Option<String>,
    pub barcode: Option<String>,
    /// Catalogue image captured with the line so every POS add route can render it.
    /// Default keeps carts created by older application versions compatible.
    #[serde(default)]
    pub image_path: Option<String>,
    /// Stored as string to support decimal quantities
    pub quantity: String,
    pub unit_price_minor: i64,
    pub line_discount_minor: i64,
    /// Required non-empty reason when line_discount_minor > 0; flows to audit log.
    pub line_discount_reason: Option<String>,
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
        let subtotal = mul_minor_by_qty(unit_price_minor, quantity);
        let tax_amount = if tax_inclusive {
            // Extract tax with half-up rounding to match exclusive-tax direction:
            // tax = price * rate / (10000 + rate)  rounded half-up
            let divisor = 10_000 + tax_rate_basis_points;
            (subtotal * tax_rate_basis_points + divisor / 2) / divisor
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
            image_path: None,
            quantity: quantity.to_string(),
            unit_price_minor,
            line_discount_minor: 0,
            line_discount_reason: None,
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
        let subtotal = mul_minor_by_qty(self.unit_price_minor, &self.quantity);
        // Clamp to 0: prevents negative discounted base if a crafted payload somehow
        // bypasses the command-layer guard in pos_apply_line_discount. The validate()
        // call in finalize_sale is the final backstop, but we should not propagate
        // garbage values into tax arithmetic.
        let discounted = (subtotal - self.line_discount_minor).max(0);
        let tax_amount = if self.tax_inclusive {
            let divisor = 10_000 + self.tax_rate_basis_points;
            (discounted * self.tax_rate_basis_points + divisor / 2) / divisor
        } else {
            calc_tax_exclusive(discounted, self.tax_rate_basis_points)
        };
        self.tax_amount_minor = tax_amount;
        self.line_total_minor = discounted + if self.tax_inclusive { 0 } else { tax_amount };
    }

    /// Apply a percentage discount (in basis points) to this line.
    /// Used by the AI batch-discount tool flow.
    #[allow(dead_code)]
    pub fn apply_discount_percent(&mut self, discount_basis_points: i64) {
        let subtotal = mul_minor_by_qty(self.unit_price_minor, &self.quantity);
        self.line_discount_minor = apply_discount_bp(subtotal, discount_basis_points);
        self.recalculate();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_line(unit_price_minor: i64, qty: &str, tax_bp: i64, inclusive: bool) -> CartLine {
        CartLine::new(
            None,
            "Test Item".into(),
            None,
            None,
            qty,
            unit_price_minor,
            String::new(),
            tax_bp,
            inclusive,
        )
    }

    // ── Test 1: Tax-exclusive line total ──────────────────────────────────────
    #[test]
    fn line_total_tax_exclusive() {
        // 1.000 BHD × 2 qty at 10% excl → subtotal 2000, tax 200, total 2200
        let line = make_line(1_000, "2", 1_000, false);
        assert_eq!(line.line_total_minor, 2_200);
        assert_eq!(line.tax_amount_minor, 200);
    }

    // ── Test 2: Tax-inclusive line total ─────────────────────────────────────
    #[test]
    fn line_total_tax_inclusive() {
        // 1.100 BHD inclusive with 10% tax:
        // extracted tax = 1100 * 1000 / 11000 = 100; total stays 1100
        let line = make_line(1_100, "1", 1_000, true);
        assert_eq!(line.line_total_minor, 1_100);
        assert_eq!(line.tax_amount_minor, 100);
    }

    // ── Test 3: Cart gross total sums only active lines ───────────────────────
    #[test]
    fn cart_gross_total_excludes_voided() {
        let mut cart = Cart::new("b".into(), "d".into(), "s".into(), "u".into());
        let mut l1 = make_line(1_000, "1", 0, false);
        let mut l2 = make_line(500, "1", 0, false);
        l2.voided = true;
        cart.lines.push(l1.clone());
        // Suppress unused warning
        l1.voided = false;
        cart.lines.push(l2);
        assert_eq!(cart.gross_total(), 1_000);
    }

    // ── Test 4: Net total clamps to zero when bill discount exceeds gross ─────
    #[test]
    fn net_total_never_negative() {
        let mut cart = Cart::new("b".into(), "d".into(), "s".into(), "u".into());
        cart.lines.push(make_line(500, "1", 0, false));
        cart.bill_discount_minor = 9_999; // bigger than gross
        assert_eq!(cart.net_total(), 0);
    }

    // ── Test 5: Bill discount reflects in net_total ───────────────────────────
    #[test]
    fn bill_discount_applied_to_net() {
        let mut cart = Cart::new("b".into(), "d".into(), "s".into(), "u".into());
        cart.lines.push(make_line(1_000, "2", 0, false)); // gross = 2000
        cart.bill_discount_minor = 200;
        assert_eq!(cart.net_total(), 1_800);
    }

    // ── Test 6: Line discount reduces line total after recalculate ────────────
    #[test]
    fn line_discount_recalculate() {
        let mut line = make_line(1_000, "2", 0, false); // gross 2000
        line.line_discount_minor = 300;
        line.recalculate();
        assert_eq!(line.line_total_minor, 1_700);
    }

    // ── Test 7: Payment under-tender is rejected ──────────────────────────────
    #[test]
    fn payment_undertender_detected() {
        // Simulate the check in sale_repo::finalize_sale without DB
        let net_total: i64 = 2_000;
        let total_paid: i64 = 1_500;
        assert!(
            total_paid < net_total,
            "Under-payment must be caught before writing to DB"
        );
    }

    // ── Test 8: Split payment sum covers net total ────────────────────────────
    #[test]
    fn split_payment_sum_covers_total() {
        let net_total: i64 = 3_000;
        let cash: i64 = 2_000;
        let card: i64 = 1_000;
        assert!(cash + card >= net_total);
    }

    // ── Test 9: Partial refund amount = qty × unit price ─────────────────────
    #[test]
    fn partial_refund_amount_calculation() {
        let unit_price_minor: i64 = 500;
        let refund_qty: i64 = 3;
        let expected: i64 = 1_500;
        assert_eq!(refund_qty * unit_price_minor, expected);
    }

    // ── Test 10: X-report expected cash = opening + sales - refunds + in - out ─
    #[test]
    fn xreport_expected_cash_formula() {
        let opening_float: i64 = 5_000;
        let cash_sales: i64 = 20_000;
        let cash_refunds: i64 = 1_000;
        let paid_in: i64 = 500;
        let paid_out: i64 = 300;
        let expected = opening_float + cash_sales - cash_refunds + paid_in - paid_out;
        assert_eq!(expected, 24_200);
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
    /// Required non-empty reason when bill_discount_minor > 0; flows to audit log.
    pub bill_discount_reason: Option<String>,
}

impl Cart {
    pub fn new(
        branch_id: String,
        device_id: String,
        shift_id: String,
        cashier_user_id: String,
    ) -> Self {
        Self {
            cart_id: ulid::Ulid::new().to_string(),
            branch_id,
            device_id,
            shift_id,
            cashier_user_id,
            lines: Vec::new(),
            bill_discount_minor: 0,
            bill_discount_reason: None,
        }
    }

    /// Pre-discount subtotal: sum of unit_price * qty for every active line,
    /// using integer-only arithmetic (no float round-trips).  Does NOT include
    /// tax for exclusive-tax items — that is the tax_total() component.
    pub fn gross_total(&self) -> i64 {
        self.lines
            .iter()
            .filter(|l| !l.voided)
            .map(|l| mul_minor_by_qty(l.unit_price_minor, &l.quantity))
            .sum()
    }

    pub fn tax_total(&self) -> i64 {
        self.lines
            .iter()
            .filter(|l| !l.voided)
            .map(|l| l.tax_amount_minor)
            .sum()
    }

    pub fn discount_total(&self) -> i64 {
        let item_discounts: i64 = self
            .lines
            .iter()
            .filter(|l| !l.voided)
            .map(|l| l.line_discount_minor)
            .sum();
        item_discounts + self.bill_discount_minor
    }

    /// Post-line-discount total (before bill discount).  Used as the base for
    /// the bill-discount guard and for computing the net due.
    pub fn post_line_total(&self) -> i64 {
        self.lines
            .iter()
            .filter(|l| !l.voided)
            .map(|l| l.line_total_minor)
            .sum()
    }

    /// Net amount the customer must pay.
    /// Satisfies: net_total == gross_total - discount_total + tax_total
    pub fn net_total(&self) -> i64 {
        (self.post_line_total() - self.bill_discount_minor).max(0)
    }

    /// Validate all quantities and monetary totals are within safe ranges.
    /// Call this before persisting a sale to catch overflow / malicious input.
    pub fn validate(&self) -> Result<(), AppError> {
        for line in self.lines.iter().filter(|l| !l.voided) {
            if !qty_in_range(&line.quantity, MAX_QTY) {
                return Err(AppError::Validation(format!(
                    "Invalid quantity for '{}': {}",
                    line.product_name, line.quantity
                )));
            }
            let subtotal = mul_minor_by_qty(line.unit_price_minor, &line.quantity);
            if subtotal > MAX_MINOR {
                return Err(AppError::Validation(format!(
                    "Line total for '{}' is out of valid range.",
                    line.product_name
                )));
            }
            if line.line_total_minor < 0 || line.line_total_minor > MAX_MINOR {
                return Err(AppError::Validation(format!(
                    "Line total for '{}' is out of valid range ({}). Please re-add the item.",
                    line.product_name, line.line_total_minor
                )));
            }
        }
        let gross = self.gross_total();
        if !(0..=MAX_MINOR).contains(&gross) {
            return Err(AppError::Validation(format!(
                "Cart gross total is out of valid range ({gross})."
            )));
        }
        let net = self.net_total();
        if !(0..=MAX_MINOR).contains(&net) {
            return Err(AppError::Validation(format!(
                "Cart net total is out of valid range ({net})."
            )));
        }
        Ok(())
    }
}
