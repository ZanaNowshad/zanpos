//! Money is stored as integer minor units to avoid floating-point errors.
//! BHD uses 3 decimal places: 1.000 BHD = 1000 minor units.

/// Multiply an amount in minor units by a decimal quantity string (e.g. "2.5").
/// Uses integer arithmetic only — no floating point.  Rounds half-up.
/// Returns 0 for unparseable or non-positive quantities.
pub fn mul_minor_by_qty(minor: i64, qty: &str) -> i64 {
    let qty = qty.trim();
    if qty.is_empty() {
        return 0;
    }
    // Split on decimal point; handle sign if present
    let (int_part, frac_part) = match qty.split_once('.') {
        Some((i, f)) => (i, f),
        None => (qty, ""),
    };
    // Parse integer part (handle leading sign)
    let int_val: i64 = match int_part.parse() {
        Ok(v) => v,
        Err(_) => return 0,
    };
    if frac_part.is_empty() {
        // Simple integer quantity
        return minor.saturating_mul(int_val.max(0));
    }
    // Fractional quantity — compute as rational
    let frac_len = frac_part.len().min(9); // up to 9 decimal places (fits i64 * 10^9)
    let frac_str = if frac_part.len() > 9 { &frac_part[..9] } else { frac_part };
    let denom = 10_i64.pow(frac_len as u32);
    let frac_val: i64 = match frac_str.parse() {
        Ok(v) => v,
        Err(_) => return 0,
    };
    // qty_numerator = whole_part * denom + fractional_part
    let qty_num = int_val.max(0).saturating_mul(denom).saturating_add(frac_val);
    if qty_num <= 0 {
        return 0;
    }
    // (minor * qty_num + denom/2) / denom  — half-up rounding
    let half = denom / 2;
    // Use i128 for intermediate to avoid overflow
    let product = minor as i128 * qty_num as i128 + half as i128;
    let result = product / denom as i128;
    // Clamp to i64 range
    result.min(i64::MAX as i128).max(0) as i64
}

/// Check whether a decimal quantity string is strictly positive and ≤ max.
pub fn qty_in_range(qty: &str, max: i64) -> bool {
    let qty = qty.trim();
    if qty.is_empty() {
        return false;
    }
    let (int_part, frac_part) = match qty.split_once('.') {
        Some((i, f)) => (i, f),
        None => (qty, ""),
    };
    let int_val: i64 = match int_part.parse() {
        Ok(v) => v,
        Err(_) => return false,
    };
    if int_val < 0 {
        return false;
    }
    if int_val > max {
        return false;
    }
    if int_val == max && !frac_part.is_empty() {
        // At boundary: any fractional part makes it exceed max
        let frac_trimmed = frac_part.trim_end_matches('0');
        if !frac_trimmed.is_empty() {
            return false;
        }
    }
    // Must be > 0
    if int_val == 0 {
        let frac_trimmed = frac_part.trim_end_matches('0');
        if frac_trimmed.is_empty() {
            return false;
        }
    }
    true
}

/// Add two decimal quantity strings (e.g. "2" + "0.5" = "2.5") using integer
/// arithmetic only — no floating point.  Handles up to 9 decimal places.
/// Returns "0" for invalid inputs.
pub fn add_decimal_qty_str(a: &str, b: &str) -> String {
    let a = a.trim();
    let b = b.trim();
    if a.is_empty() || b.is_empty() {
        return "0".to_string();
    }
    let (a_int_str, a_frac_str) = a.split_once('.').unwrap_or((a, ""));
    let (b_int_str, b_frac_str) = b.split_once('.').unwrap_or((b, ""));

    let a_int: i64 = a_int_str.parse().unwrap_or(0);
    let b_int: i64 = b_int_str.parse().unwrap_or(0);
    if a_int < 0 || b_int < 0 {
        return "0".to_string();
    }

    let max_frac = a_frac_str.len().max(b_frac_str.len()).min(9);
    let denom = 10_i64.pow(max_frac as u32);

    let a_frac: i64 = {
        let pad = format!("{:0<width$}", a_frac_str, width = max_frac);
        pad[..max_frac].parse().unwrap_or(0)
    };
    let b_frac: i64 = {
        let pad = format!("{:0<width$}", b_frac_str, width = max_frac);
        pad[..max_frac].parse().unwrap_or(0)
    };

    let a_total = a_int.saturating_mul(denom).saturating_add(a_frac);
    let b_total = b_int.saturating_mul(denom).saturating_add(b_frac);
    let sum = a_total.saturating_add(b_total);

    let int_part = sum / denom;
    let frac_part = sum % denom;

    if frac_part == 0 {
        format!("{}", int_part)
    } else {
        let frac_str = format!("{:0>width$}", frac_part, width = max_frac);
        let trimmed = frac_str.trim_end_matches('0');
        format!("{}.{}", int_part, trimmed)
    }
}

/// Parse a decimal price string (e.g. "1.500") into minor units (1500 fils).
/// Uses integer arithmetic only — no floating point.
/// Returns None for negative, unparseable, out-of-range, or inputs with more
/// significant decimal digits than the exponent (e.g. "1.5001" with exponent=3
/// would silently truncate — BUG-BACKEND-7 fix: reject instead).
pub fn parse_major_to_minor(s: &str, exponent: u32) -> Option<i64> {
    let parts: Vec<&str> = s.trim().splitn(2, '.').collect();
    let whole: i64 = parts[0].parse().ok()?;
    if whole < 0 {
        return None;
    }
    let frac_str = parts.get(1).copied().unwrap_or("");
    // Reject if the input has more significant decimal digits than the currency
    // exponent allows — truncating silently would corrupt the amount.
    if frac_str.len() > exponent as usize {
        let excess = &frac_str[exponent as usize..];
        if excess.chars().any(|c| c != '0') {
            return None;
        }
    }
    let frac_padded = format!("{:0<width$}", frac_str, width = exponent as usize);
    let frac: i64 = frac_padded[..exponent as usize].parse().unwrap_or(0);
    let divisor = 10_i64.pow(exponent);
    whole.checked_mul(divisor)?.checked_add(frac)
}

/// Format minor units as a decimal string for the configured currency exponent.
/// For BHD (exponent 3): 1500 → "1.500", -1500 → "-1.500", -500 → "-0.500"
///
/// Bug fix: when |minor| < divisor (e.g. -500 BHD minor), `minor / divisor`
/// rounds toward zero to 0, losing the negative sign entirely.  We handle the
/// sign explicitly so "-0.500" is never silently emitted as "0.500".
pub fn format_minor(minor: i64, exponent: u32) -> String {
    let divisor = 10i64.pow(exponent);
    let sign = if minor < 0 { "-" } else { "" };
    let abs = minor.unsigned_abs();           // u64 — avoids i64::MIN overflow
    let abs_divisor = divisor as u64;
    let whole = abs / abs_divisor;
    let frac  = abs % abs_divisor;
    format!("{}{}.{:0>width$}", sign, whole, frac, width = exponent as usize)
}

/// Apply a percentage discount given in basis points (100 bp = 1%).
/// Rounds down (floor) to avoid giving more discount than intended.
/// Retained as part of the discount API surface; exercised by unit tests and
/// available to the AI batch-discount flow.
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
    fn format_minor_negative() {
        // Whole-unit negative
        assert_eq!(format_minor(-1500, 3), "-1.500");
        // Sub-unit negative — previously emitted "0.500" (sign lost)
        assert_eq!(format_minor(-500, 3), "-0.500");
        // Exact negative one
        assert_eq!(format_minor(-1000, 3), "-1.000");
        // Large negative
        assert_eq!(format_minor(-10_250, 3), "-10.250");
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
