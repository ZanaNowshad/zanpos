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

/// Clamp a line to exactly the paper width: truncate what overflows, pad what
/// falls short.
///
/// Shared with the native ESC/POS renderer rather than reimplemented there.
/// The two paths having their own idea of line width is what let them drift
/// apart in the first place (ledger PRINT1) — one truncated at the roll width
/// and the other sent the whole string and let the printer wrap it, which
/// breaks the column layout on a 58 mm roll.
pub(crate) fn pad_to(value: &str, width: usize) -> String {
    let truncated: String = value.chars().take(width).collect();
    format!("{truncated:width$}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::printing::document::*;

    fn doc_with(text: &str) -> ReceiptDocument {
        ReceiptDocument {
            header: vec![],
            body: vec![ReceiptBlock {
                style: BlockStyle::Normal,
                lines: vec![BlockLine {
                    text: text.into(),
                    font: BlockFont::FontA,
                }],
            }],
            footer: vec![],
            cut_after: false,
        }
    }

    /// Ledger PRINT1, asserted against the path that actually ships.
    ///
    /// `escpos.rs` holds the same contract for the native driver, but that file
    /// only exists under the `escpos-driver` feature and `default = []`, so
    /// this is the copy that runs in an ordinary build.
    #[test]
    fn a_line_wider_than_the_roll_is_clamped_to_it() {
        let long_name = "EXTRA LONG PRODUCT NAME THAT OVERFLOWS THE ROLL";
        assert!(long_name.len() > PROFILE_58MM.width_chars as usize);

        let bytes = render_escpos_document(&doc_with(long_name), &PROFILE_58MM);
        let rendered = String::from_utf8_lossy(&bytes);

        assert!(
            !rendered.contains(long_name),
            "the full over-long line must not reach the printer"
        );
        let clamped: String = long_name
            .chars()
            .take(PROFILE_58MM.width_chars as usize)
            .collect();
        assert!(
            rendered.contains(&clamped),
            "expected it truncated to width"
        );
    }

    #[test]
    fn pad_to_fills_short_lines_and_truncates_long_ones() {
        assert_eq!(pad_to("ab", 5), "ab   ", "short lines are padded to width");
        assert_eq!(pad_to("abcdef", 3), "abc", "long lines are truncated");
        assert_eq!(pad_to("abc", 3), "abc", "an exact fit is unchanged");
        assert_eq!(pad_to("", 3), "   ", "an empty line still occupies width");
    }

    #[test]
    fn pad_to_counts_characters_not_bytes() {
        // A receipt carrying an Arabic customer name must not be cut mid-code-point.
        // `chars().take(n)` is what makes this safe; `&s[..n]` would panic.
        let arabic = "مرحبا بالعالم";
        let clamped = pad_to(arabic, 5);
        assert_eq!(
            clamped.chars().count(),
            5,
            "five characters, whatever they cost in bytes"
        );
        assert!(arabic.starts_with(clamped.trim_end()));
    }
}
