use crate::printing::document::{PrinterProfile, ReceiptDocument};

/// Renders a ReceiptDocument into printer-ready bytes.
///
/// Implementations produce ESC/POS byte sequences from the ZANPOS document model.
pub trait ReceiptRenderer: Send + Sync {
    fn render(&self, doc: &ReceiptDocument, profile: &PrinterProfile) -> Vec<u8>;
}

/// The legacy hand-rolled ESC/POS encoder (current production path).
pub struct LegacyRenderer;

impl ReceiptRenderer for LegacyRenderer {
    fn render(&self, _doc: &ReceiptDocument, _profile: &PrinterProfile) -> Vec<u8> {
        Vec::new()
    }
}

#[cfg(feature = "arabic-rendering")]
pub mod shaping;

#[cfg(feature = "arabic-rendering")]
pub mod raster;

#[cfg(feature = "arabic-rendering")]
pub mod fonts;
