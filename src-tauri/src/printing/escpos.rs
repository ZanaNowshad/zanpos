use crate::printing::document::{BlockFont, BlockStyle, PrinterProfile, ReceiptDocument};
use crate::printing::renderer::ReceiptRenderer;

pub struct EscposRenderer;

impl EscposRenderer {
    pub fn new(_profile: &PrinterProfile) -> Self {
        Self
    }
}

impl ReceiptRenderer for EscposRenderer {
    fn render(&self, doc: &ReceiptDocument, profile: &PrinterProfile) -> Vec<u8> {
        let width = profile.width_chars as usize;
        let mut output: Vec<u8> = Vec::new();

        // Initialise printer
        output.extend_from_slice(&[0x1b, 0x40]);

        let blocks: Vec<_> = doc.all_blocks().collect();
        for (i, block) in blocks.iter().enumerate() {
            let style = block.style;

            for line in &block.lines {
                let text = pad_to(&line.text, width);

                match style {
                    BlockStyle::Centered => output.extend_from_slice(&[0x1b, 0x61, 0x01]),
                    BlockStyle::RightAligned => output.extend_from_slice(&[0x1b, 0x61, 0x02]),
                    _ => output.extend_from_slice(&[0x1b, 0x61, 0x00]),
                }

                if matches!(style, BlockStyle::Bold) {
                    output.extend_from_slice(&[0x1b, 0x45, 0x01]);
                }
                if matches!(style, BlockStyle::DoubleWidth | BlockStyle::DoubleHeight) {
                    output.extend_from_slice(&[0x1d, 0x21, 0x11]);
                }

                match line.font {
                    BlockFont::FontA => output.extend_from_slice(&[0x1b, 0x4d, 0x00]),
                    BlockFont::FontB => output.extend_from_slice(&[0x1b, 0x4d, 0x01]),
                }

                output.extend_from_slice(text.as_bytes());
                output.extend_from_slice(b"\n");

                if matches!(style, BlockStyle::Bold) {
                    output.extend_from_slice(&[0x1b, 0x45, 0x00]);
                }
                if matches!(style, BlockStyle::DoubleWidth | BlockStyle::DoubleHeight) {
                    output.extend_from_slice(&[0x1b, 0x21, 0x00]);
                }
            }

            if i < blocks.len() - 1 {
                output.extend_from_slice(b"\n");
            }
        }

        // Feed and cut
        output.extend_from_slice(b"\n\n\n");
        if doc.cut_after {
            output.extend_from_slice(&[0x1d, 0x56, 0x00]);
        }

        output
    }
}

fn pad_to(s: &str, max: usize) -> String {
    let clean: String = s.chars().take(max).collect();
    format!("{:width$}", clean, width = max)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::printing::document::*;

    fn doc() -> ReceiptDocument {
        ReceiptDocument {
            header: vec![ReceiptBlock {
                style: BlockStyle::Centered,
                lines: vec![BlockLine {
                    text: "ZANPOS".into(),
                    font: BlockFont::FontA,
                }],
            }],
            body: vec![ReceiptBlock {
                style: BlockStyle::Normal,
                lines: vec![BlockLine {
                    text: "Item".into(),
                    font: BlockFont::FontA,
                }],
            }],
            footer: vec![ReceiptBlock {
                style: BlockStyle::Bold,
                lines: vec![BlockLine {
                    text: "TOTAL".into(),
                    font: BlockFont::FontA,
                }],
            }],
            cut_after: true,
        }
    }

    #[test]
    fn output_starts_with_esc_init() {
        let renderer = EscposRenderer::new(&PROFILE_80MM);
        let bytes = renderer.render(&doc(), &PROFILE_80MM);
        assert!(bytes[..2] == [0x1b, 0x40], "ESC/POS init command missing");
    }

    #[test]
    fn output_ends_with_cut() {
        let renderer = EscposRenderer::new(&PROFILE_80MM);
        let bytes = renderer.render(&doc(), &PROFILE_80MM);
        let tail = &bytes[bytes.len() - 3..];
        assert!(
            tail == [0x1d, 0x56, 0x00],
            "ESC/POS cut command missing at end"
        );
    }

    #[test]
    fn output_is_non_empty() {
        let renderer = EscposRenderer::new(&PROFILE_58MM);
        let bytes = renderer.render(&doc(), &PROFILE_58MM);
        assert!(bytes.len() > 5);
    }
}
