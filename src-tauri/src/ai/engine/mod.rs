//! Deterministic bulk-operation engine: declarative selectors, checkpointed
//! batched execution, and whole-run undo. See docs/superpowers/specs.

pub mod batch;
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
