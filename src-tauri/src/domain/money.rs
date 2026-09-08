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
    let frac_str = if frac_part.len() > 9 {
        &frac_part[..9]
    } else {
        frac_part
    };
    let denom = 10_i64.pow(frac_len as u32);
    let frac_val: i64 = match frac_str.parse() {
        Ok(v) => v,
        Err(_) => return 0,
    };
    // qty_numerator = whole_part * denom + fractional_part
    let qty_num = int_val
        .max(0)
        .saturating_mul(denom)
        .saturating_add(frac_val);
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

/// Order two decimal quantity strings without going through floating point.
///
/// `"1.10"` and `"1.1"` are the same quantity written two ways, and `"2"` and
/// `"10"` must not order as text — both happen, because quantities are stored as
/// whatever string the writer produced. Returns `None` when either side does not
/// parse, so a caller has to decide what an unreadable quantity means rather
/// than being handed a silent `Equal`.
pub fn cmp_decimal_qty(a: &str, b: &str) -> Option<std::cmp::Ordering> {
    fn scaled(q: &str, frac_len: usize) -> Option<i64> {
        let (int_str, frac_str) = q.trim().split_once('.').unwrap_or((q.trim(), ""));
        if int_str.is_empty() || !int_str.bytes().all(|c| c.is_ascii_digit()) {
            return None;
        }
        if !frac_str.bytes().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let int_val: i64 = int_str.parse().ok()?;
        let padded = format!("{:0<width$}", frac_str, width = frac_len);
        let frac_val: i64 = if frac_len == 0 {
            0
        } else {
            padded[..frac_len].parse().ok()?
        };
        int_val
            .checked_mul(10_i64.checked_pow(frac_len as u32)?)?
            .checked_add(frac_val)
    }

    let frac_len = a
        .trim()
        .split_once('.')
        .map_or(0, |(_, f)| f.len())
        .max(b.trim().split_once('.').map_or(0, |(_, f)| f.len()))
        .min(9);
    Some(scaled(a, frac_len)?.cmp(&scaled(b, frac_len)?))
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
    let abs = minor.unsigned_abs(); // u64 — avoids i64::MIN overflow
    let abs_divisor = divisor as u64;
    let whole = abs / abs_divisor;
    let frac = abs % abs_divisor;
    // A zero-exponent currency has no minor unit, so it has no decimal point.
    // The unconditional separator rendered ¥1000 as "1000.0" — a decimal place
    // that does not exist in the currency, printed on the receipt.
    if exponent == 0 {
        return format!("{sign}{whole}");
    }
    format!(
        "{}{}.{:0>width$}",
        sign,
        whole,
        frac,
        width = exponent as usize
    )
}

/// How many decimal places a currency's minor units carry (ISO 4217).
///
/// This lived in `commands::setup_commands`, which made it the one rule the
/// repository layer had to reach *upward* into the command layer to obtain — the
/// only such dependency in the crate. It is a property of the currency, not of a
/// Tauri command, and it belongs beside [`format_minor`], which is the function
/// that consumes it.
pub fn currency_exponent(currency: &str) -> i32 {
    match currency {
        "BHD" | "KWD" | "OMR" => 3,
        "JPY" | "KRW" | "IDR" => 0,
        _ => 2, // USD, EUR, GBP, SAR, AED, QAR, EGP, MAD, etc.
    }
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

/// The tax already contained inside a tax-inclusive amount.
/// tax = amount * rate / (10000 + rate), rounded to nearest minor unit.
pub fn calc_tax_inclusive(amount_minor: i64, rate_basis_points: i64) -> i64 {
    let divisor = 10_000 + rate_basis_points;
    (amount_minor * rate_basis_points + divisor / 2) / divisor
}

/// Spread a whole-bill discount across lines, and re-extract the VAT inside
/// each one from what is left.
///
/// A discount on the whole bill reduces what the customer hands over, so it
/// reduces the consideration VAT is due on. Computing each line's tax first and
/// subtracting the discount afterwards leaves tax charged on money nobody paid:
/// on a 2.000 basket at 10% with 0.300 off, the receipt read 1.700 + 0.200 =
/// 1.900, an implied 11.76% against a 10% rate, and the store over-declared
/// output VAT by 0.027.
///
/// `line_totals` are tax-inclusive, which is what both pricing modes produce —
/// an inclusive line's total already contains its tax, and an exclusive line's
/// has had it added. The discount is apportioned pro rata by line total, with
/// the rounding remainder going to the lines that lost the most to flooring, so
/// the parts sum to the discount exactly and the net is unchanged.
///
/// Apportioning pro rata across *every* line, rather than only across
/// standard-rated ones, is the conventional basis and the one a mixed basket of
/// 10% and zero-rated goods is assumed to take here. It is the single choice in
/// this function that a tax adviser could reasonably direct otherwise; see
/// `docs/vat-receipt-review.md`.
///
/// Returns the reduced line totals and the tax inside each.
pub fn apportion_bill_discount(
    line_totals: &[i64],
    rate_basis_points: &[i64],
    bill_discount_minor: i64,
) -> (Vec<i64>, Vec<i64>) {
    let extract = |totals: &[i64]| -> Vec<i64> {
        totals
            .iter()
            .zip(rate_basis_points)
            .map(|(&total, &rate)| calc_tax_inclusive(total, rate))
            .collect()
    };

    let gross: i64 = line_totals.iter().sum();
    let discount = bill_discount_minor.clamp(0, gross);
    if discount == 0 || gross == 0 {
        return (line_totals.to_vec(), extract(line_totals));
    }

    // Floor each share, then hand the remainder to the largest fractional parts
    // first. Flooring alone would leave the shares summing to less than the
    // discount, and the customer would be charged the difference.
    let mut shares: Vec<i64> = line_totals
        .iter()
        .map(|&total| total * discount / gross)
        .collect();
    let mut remainder = discount - shares.iter().sum::<i64>();
    let mut order: Vec<usize> = (0..line_totals.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse((line_totals[i] * discount) % gross));
    for &i in &order {
        if remainder == 0 {
            break;
        }
        shares[i] += 1;
        remainder -= 1;
    }

    let reduced: Vec<i64> = line_totals
        .iter()
        .zip(&shares)
        .map(|(&total, &share)| (total - share).max(0))
        .collect();
    let taxes = extract(&reduced);
    (reduced, taxes)
}

#[cfg(test)]
mod tests {
    use super::{apportion_bill_discount, calc_tax_inclusive};

    const VAT10: i64 = 1_000;
    const ZERO_RATED: i64 = 0;

    /// The defect this function exists to fix, in the numbers the existing
    /// bill-discount test already uses: 2 x 1.000 at 10% exclusive, 0.300 off
    /// the bill. The customer pays 1.900, so 1.900 is the consideration and the
    /// VAT inside it is 0.173 — not the 0.200 computed before the discount,
    /// which made the receipt read 1.700 + 0.200 and imply 11.76%.
    #[test]
    fn a_bill_discount_reduces_the_amount_vat_is_charged_on() {
        let (totals, taxes) = apportion_bill_discount(&[2_200], &[VAT10], 300);
        assert_eq!(totals, vec![1_900], "the customer still pays 1.900");
        assert_eq!(
            taxes,
            vec![173],
            "VAT is the tax inside 1.900, not inside 2.000"
        );
        assert_eq!(totals[0] - taxes[0], 1_727, "taxable consideration");
    }

    /// The net must not move. Whatever the split, the sum of the line totals is
    /// still what the till asked for — otherwise the payment guard in
    /// `finalize_sale` would reject a correctly tendered sale.
    #[test]
    fn apportioning_never_changes_what_the_customer_pays() {
        for discount in [0, 1, 7, 300, 999, 2_199, 2_200] {
            let (totals, _) = apportion_bill_discount(&[1_100, 1_100], &[VAT10, VAT10], discount);
            assert_eq!(
                totals.iter().sum::<i64>(),
                2_200 - discount,
                "discount of {discount} moved the net",
            );
        }
    }

    /// Flooring every share would leave the parts summing to less than the
    /// discount, quietly charging the customer the shortfall. Three lines and a
    /// discount that does not divide by three is the case that catches it.
    #[test]
    fn rounding_remainder_is_handed_out_not_dropped() {
        let (totals, _) = apportion_bill_discount(&[1_000, 1_000, 1_000], &[VAT10; 3], 100);
        assert_eq!(totals.iter().sum::<i64>(), 2_900);
        // 100 across three equal lines: 34/33/33 in some order, never 33/33/33.
        let mut shares: Vec<i64> = totals.iter().map(|&t| 1_000 - t).collect();
        shares.sort();
        assert_eq!(shares, vec![33, 33, 34]);
    }

    /// A zero-rated line takes its share of the discount but never acquires
    /// tax — the basket's VAT comes only from the standard-rated part.
    #[test]
    fn a_zero_rated_line_carries_no_tax_after_apportioning() {
        let (totals, taxes) = apportion_bill_discount(&[1_100, 1_000], &[VAT10, ZERO_RATED], 210);
        assert_eq!(totals.iter().sum::<i64>(), 1_890);
        assert_eq!(taxes[1], 0, "zero-rated stays zero-rated");
        assert_eq!(taxes[0], calc_tax_inclusive(totals[0], VAT10));
    }

    /// A discount larger than the basket cannot invert a line into a negative
    /// total, and cannot refund more than was owed.
    #[test]
    fn an_oversized_discount_clamps_at_free() {
        let (totals, taxes) = apportion_bill_discount(&[1_100, 1_100], &[VAT10, VAT10], 9_999);
        assert_eq!(totals, vec![0, 0]);
        assert_eq!(taxes, vec![0, 0]);
    }

    #[test]
    fn quantities_compare_by_value_not_by_spelling() {
        use std::cmp::Ordering::*;
        // Same number, two spellings — the trailing zero must not make it larger.
        assert_eq!(cmp_decimal_qty("1.10", "1.1"), Some(Equal));
        assert_eq!(cmp_decimal_qty("2", "2.000"), Some(Equal));
        // Text ordering would put "10" below "2"; value ordering must not.
        assert_eq!(cmp_decimal_qty("10", "2"), Some(Greater));
        assert_eq!(cmp_decimal_qty("2", "10"), Some(Less));
        // A weighed line against a whole-number approval.
        assert_eq!(cmp_decimal_qty("0.750", "1"), Some(Less));
        assert_eq!(cmp_decimal_qty("1.001", "1"), Some(Greater));
        // Unreadable input is not silently equal — the caller has to decide.
        assert_eq!(cmp_decimal_qty("", "1"), None);
        assert_eq!(cmp_decimal_qty("-1", "1"), None);
        assert_eq!(cmp_decimal_qty("two", "1"), None);
        assert_eq!(cmp_decimal_qty("1", "1e3"), None);
    }

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

    /// The exponent decides where the decimal point goes on every receipt and
    /// every reported total, so it is pinned rather than assumed. ZANPOS is a
    /// Bahrain product and BHD is the case that matters, but two decimals is
    /// what every other currency falls back to.
    #[test]
    fn currency_exponents_match_iso_4217_minor_units() {
        for three in ["BHD", "KWD", "OMR"] {
            assert_eq!(currency_exponent(three), 3, "{three}");
        }
        for zero in ["JPY", "KRW", "IDR"] {
            assert_eq!(currency_exponent(zero), 0, "{zero}");
        }
        for two in ["USD", "EUR", "GBP", "SAR", "AED"] {
            assert_eq!(currency_exponent(two), 2, "{two}");
        }
        // An unknown code takes the common case rather than panicking: the
        // alternative is a receipt that cannot be printed at all.
        assert_eq!(currency_exponent(""), 2);
        assert_eq!(currency_exponent("ZZZ"), 2);
    }

    /// The exponent and the formatter are the two halves of rendering an amount,
    /// which is why they now live beside each other. A currency with no minor
    /// unit must not be given a decimal point.
    #[test]
    fn the_exponent_and_the_formatter_agree() {
        let bhd = currency_exponent("BHD").max(0) as u32;
        assert_eq!(format_minor(1000, bhd), "1.000");

        let usd = currency_exponent("USD").max(0) as u32;
        assert_eq!(format_minor(1000, usd), "10.00");

        let jpy = currency_exponent("JPY").max(0) as u32;
        assert_eq!(format_minor(1000, jpy), "1000");
        assert_eq!(format_minor(-1000, jpy), "-1000");
    }
}
