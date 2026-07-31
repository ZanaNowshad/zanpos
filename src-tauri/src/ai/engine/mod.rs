//! Deterministic bulk-operation engine: declarative selectors, checkpointed
//! batched execution, and whole-run undo. See docs/superpowers/specs.

pub mod batch;
pub mod ops;
pub mod runs;
pub mod selector;

/// A price mutation. Exact integer (fils) math; never floats for the result.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(tag = "mode", content = "value")]
pub enum PriceOp {
    /// Percentage change, e.g. +20.0 means ×1.20.
    Percent(f64),
    /// Add/subtract a fixed number of fils.
    Absolute(i64),
    /// Set every matched product to this fils value.
    Set(i64),
}

/// Apply a price operation to a current fils value. Result is clamped to >= 0.
/// Percent uses f64 internally then rounds to the nearest fils — the ONLY place
/// a float touches money, and the result is immediately an integer.
pub fn apply_price(current_minor: i64, op: &PriceOp) -> i64 {
    let result = match op {
        PriceOp::Percent(p) => (current_minor as f64 * (1.0 + p / 100.0)).round() as i64,
        PriceOp::Absolute(d) => current_minor.saturating_add(*d),
        PriceOp::Set(v) => *v,
    };
    result.max(0)
}

#[cfg(test)]
mod price_tests {
    use super::*;

    #[test]
    fn percent_exact_fils() {
        assert_eq!(apply_price(12500, &PriceOp::Percent(20.0)), 15000);
        assert_eq!(apply_price(4250, &PriceOp::Percent(20.0)), 5100);
    }

    #[test]
    fn percent_rounds_to_nearest_fils() {
        assert_eq!(apply_price(333, &PriceOp::Percent(20.0)), 400);
    }

    #[test]
    fn absolute_and_set() {
        assert_eq!(apply_price(1000, &PriceOp::Absolute(-250)), 750);
        assert_eq!(apply_price(1000, &PriceOp::Set(99)), 99);
    }

    #[test]
    fn never_negative() {
        assert_eq!(apply_price(100, &PriceOp::Absolute(-500)), 0);
    }
}
