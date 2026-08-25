//! Competitor prices, read off a page and turned into fils.
//!
//! Everything here refuses rather than rounds. A price that is not exactly
//! representable in three decimals is not a Bahraini price we misread slightly
//! — it is a page we parsed wrong, a different currency, or markup that moved.
//! Rounding it would put a plausible-looking wrong number into the price
//! history, where nothing downstream could ever tell it from a real one.
//!
//! No floating point at any step. `"1.375"` becomes `1375` by moving digits,
//! which is exact by construction and needs no crate to be trusted.

use crate::errors::{AppError, AppResult};

/// BHD has three decimals. Not a parameter: every source here is a Bahrain
/// retailer and a source quoting anything else is a source we cannot use.
const EXPONENT: usize = 3;

/// 100,000 BHD. Far above any shelf price, low enough that a mis-parse which
/// swallowed a page's worth of digits is refused instead of stored.
const MAX_MINOR: i64 = 100_000_000;

/// Currency tokens a Bahrain retailer writes. `BD` is what Akelny renders,
/// `BHD` is what its structured data and Bahrain Pharmacy use.
const BAHRAINI: &[&str] = &["BHD", "BD", "﷼", "د.ب"];

/// Parse a price into fils.
///
/// Accepts an optional currency token before or after the number, an optional
/// thousands comma, and nought to three decimals — `"1"`, `"1.5"`, `"1.30"` and
/// `"1.300"` are all 1.500-style shorthand for the same shelf label and all
/// resolve exactly.
pub fn parse_bhd(raw: &str) -> AppResult<i64> {
    let cleaned = raw.trim();
    if cleaned.is_empty() {
        return Err(AppError::Validation("price is empty".into()));
    }

    let mut digits = String::new();
    let mut currency: Option<String> = None;
    let mut word = String::new();
    for ch in cleaned.chars() {
        match ch {
            '0'..='9' | '.' => digits.push(ch),
            ',' => {} // thousands separator; perfumes reach four figures
            c if c.is_whitespace() => {}
            c => word.push(c),
        }
    }
    if !word.is_empty() {
        currency = Some(word);
    }

    // A currency we do not recognise is not a formatting quirk. Quoting a Saudi
    // riyal price as Bahraini would understate a competitor by a factor of ten.
    if let Some(token) = &currency {
        let upper = token.to_uppercase();
        if !BAHRAINI.iter().any(|c| c.eq_ignore_ascii_case(&upper)) {
            return Err(AppError::Validation(format!(
                "price is not in Bahraini dinar: {raw}"
            )));
        }
    }

    parse_decimal_digits(&digits, raw)
}

fn parse_decimal_digits(digits: &str, raw: &str) -> AppResult<i64> {
    let (whole, fraction) = match digits.split_once('.') {
        None => (digits, ""),
        Some((whole, fraction)) => {
            if fraction.contains('.') {
                return Err(AppError::Validation(format!("price is malformed: {raw}")));
            }
            (whole, fraction)
        }
    };
    if whole.is_empty() && fraction.is_empty() {
        return Err(AppError::Validation(format!("price has no digits: {raw}")));
    }
    if !whole.chars().all(|c| c.is_ascii_digit()) || !fraction.chars().all(|c| c.is_ascii_digit()) {
        return Err(AppError::Validation(format!("price is malformed: {raw}")));
    }
    // Refused, not truncated: four decimals means we are reading a unit price,
    // a different currency, or the wrong element.
    if fraction.len() > EXPONENT {
        return Err(AppError::Validation(format!(
            "price has more than {EXPONENT} decimals: {raw}"
        )));
    }

    let mut minor = String::with_capacity(whole.len() + EXPONENT);
    minor.push_str(if whole.is_empty() { "0" } else { whole });
    minor.push_str(fraction);
    for _ in fraction.len()..EXPONENT {
        minor.push('0');
    }

    let value: i64 = minor
        .parse()
        .map_err(|_| AppError::Validation(format!("price is out of range: {raw}")))?;
    if value <= 0 {
        return Err(AppError::Validation(format!(
            "price must be greater than zero: {raw}"
        )));
    }
    if value > MAX_MINOR {
        return Err(AppError::Validation(format!(
            "price is implausibly large: {raw}"
        )));
    }
    Ok(value)
}

/// Render fils the way a Bahraini shelf label does.
pub fn format_bhd(minor: i64) -> String {
    format!("{}.{:03}", minor / 1000, (minor % 1000).abs())
}

/// Middle value of a set of prices.
///
/// The mean is the wrong summary here. One clearance price on a discontinued
/// line drags a mean below every shelf in the country, and a manager pricing
/// against it undercuts a sale that has already ended.
pub fn median_minor(prices: &[i64]) -> Option<i64> {
    if prices.is_empty() {
        return None;
    }
    let mut sorted = prices.to_vec();
    sorted.sort_unstable();
    let middle = sorted.len() / 2;
    Some(if sorted.len() % 2 == 1 {
        sorted[middle]
        // Even counts land between two observations. Rounding down keeps the
        // answer conservative: a suggested price is never higher than something
        // actually seen on a shelf.
    } else {
        (sorted[middle - 1] + sorted[middle]) / 2
    })
}

#[cfg(test)]
mod tests;
