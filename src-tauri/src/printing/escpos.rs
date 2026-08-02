use crate::printing::document::{PrinterProfile, ReceiptDocument};
use crate::printing::renderer::{render_escpos_document, ReceiptRenderer};

pub struct EscposRenderer;

impl EscposRenderer {
    pub fn new(_profile: &PrinterProfile) -> Self {
        Self
    }
}

impl ReceiptRenderer for EscposRenderer {
    fn render(&self, doc: &ReceiptDocument, profile: &PrinterProfile) -> Vec<u8> {
        render_escpos_document(doc, profile)
    }
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
