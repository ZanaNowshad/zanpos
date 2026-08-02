use crate::printing::document::{BlockFont, BlockStyle, PrinterProfile, ReceiptDocument};

/// Renders a ReceiptDocument into printer-ready bytes.
///
/// Implementations produce ESC/POS byte sequences from the ZANPOS document model.
pub trait ReceiptRenderer: Send + Sync {
    fn render(&self, doc: &ReceiptDocument, profile: &PrinterProfile) -> Vec<u8>;
}

/// The legacy hand-rolled ESC/POS encoder (current production path).
pub struct LegacyRenderer;

impl ReceiptRenderer for LegacyRenderer {
    fn render(&self, doc: &ReceiptDocument, profile: &PrinterProfile) -> Vec<u8> {
        render_escpos_document(doc, profile)
    }
}

pub(crate) fn render_escpos_document(doc: &ReceiptDocument, profile: &PrinterProfile) -> Vec<u8> {
    let width = profile.width_chars as usize;
    let mut output = vec![0x1b, 0x40];
    let blocks: Vec<_> = doc.all_blocks().collect();

    for (index, block) in blocks.iter().enumerate() {
        for line in &block.lines {
            match block.style {
                BlockStyle::Centered => output.extend_from_slice(&[0x1b, 0x61, 0x01]),
                BlockStyle::RightAligned => output.extend_from_slice(&[0x1b, 0x61, 0x02]),
                _ => output.extend_from_slice(&[0x1b, 0x61, 0x00]),
            }

            if matches!(block.style, BlockStyle::Bold) {
                output.extend_from_slice(&[0x1b, 0x45, 0x01]);
            }
            if matches!(
                block.style,
                BlockStyle::DoubleWidth | BlockStyle::DoubleHeight
            ) {
                output.extend_from_slice(&[0x1d, 0x21, 0x11]);
            }

            match line.font {
                BlockFont::FontA => output.extend_from_slice(&[0x1b, 0x4d, 0x00]),
                BlockFont::FontB => output.extend_from_slice(&[0x1b, 0x4d, 0x01]),
            }

            output.extend_from_slice(pad_to(&line.text, width).as_bytes());
            output.push(b'\n');

            if matches!(block.style, BlockStyle::Bold) {
                output.extend_from_slice(&[0x1b, 0x45, 0x00]);
            }
            if matches!(
                block.style,
                BlockStyle::DoubleWidth | BlockStyle::DoubleHeight
            ) {
                output.extend_from_slice(&[0x1d, 0x21, 0x00]);
            }
        }

        if index + 1 < blocks.len() {
            output.push(b'\n');
        }
    }

    output.extend_from_slice(b"\n\n\n");
    if doc.cut_after {
        output.extend_from_slice(&[0x1d, 0x56, 0x00]);
    }

    output
}

fn pad_to(value: &str, width: usize) -> String {
    let truncated: String = value.chars().take(width).collect();
    format!("{truncated:width$}")
}
