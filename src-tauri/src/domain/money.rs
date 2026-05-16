//! Money is stored as integer minor units to avoid floating-point errors.
//! BHD uses 3 decimal places: 1.000 BHD = 1000 minor units.

/// Format minor units as a decimal string for the configured currency exponent.
/// For BHD (exponent 3): 1500 → "1.500"
pub fn format_minor(minor: i64, exponent: u32) -> String {
    let divisor = 10i64.pow(exponent);
    let whole = minor / divisor;
    let frac = minor.abs() % divisor;
    format!("{}.{:0>width$}", whole, frac, width = exponent as usize)
}

/// Apply a percentage discount given in basis points (100 bp = 1%).
/// Rounds down (floor) to avoid giving more discount than intended.
/// Used by `CartLine::apply_discount_percent`; not called from command layer directly.
#[allow(dead_code)]
pub fn apply_discount_bp(amount_minor: i64, discount_basis_points: i64) -> i64 {
    amount_minor * discount_basis_points / 10_000
}

/// Calculate exclusive tax amount given rate in basis points.
/// tax = price * rate / 10000, rounded to nearest minor unit.
pub fn calc_tax_exclusive(price_minor: i64, rate_basis_points: i64) -> i64 {
    (price_minor * rate_basis_points + 5_000) / 10_000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_bhd() {
        assert_eq!(format_minor(1000, 3), "1.000");
        assert_eq!(format_minor(1500, 3), "1.500");
        assert_eq!(format_minor(0, 3), "0.000");
        assert_eq!(format_minor(400, 3), "0.400");
    }

    #[test]
    fn tax_exclusive_10pct() {
        // 1.000 BHD at 10% VAT → 0.100 BHD tax
        assert_eq!(calc_tax_exclusive(1000, 1000), 100);
    }

    #[test]
    fn discount_10pct() {
        // 10% off 1.000 BHD → 0.100 BHD discount
        assert_eq!(apply_discount_bp(1000, 1000), 100);
    }
}
