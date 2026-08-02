/// ZANPOS-owned receipt document types — library-agnostic.
///
/// These types are the canonical representation consumed by commands.
/// Renderers (legacy hand-rolled, escpos-rs) produce bytes from them.

#[derive(Debug, Clone)]
pub struct ReceiptBlock {
    pub style: BlockStyle,
    pub lines: Vec<BlockLine>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BlockStyle {
    Normal,
    Bold,
    DoubleWidth,
    DoubleHeight,
    Centered,
    RightAligned,
}

#[derive(Debug, Clone)]
pub struct BlockLine {
    pub text: String,
    pub font: BlockFont,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BlockFont {
    FontA,
    FontB,
}

#[derive(Debug, Clone)]
pub struct ReceiptDocument {
    pub header: Vec<ReceiptBlock>,
    pub body: Vec<ReceiptBlock>,
    pub footer: Vec<ReceiptBlock>,
    pub cut_after: bool,
}

impl ReceiptDocument {
    pub fn all_blocks(&self) -> impl Iterator<Item = &ReceiptBlock> {
        self.header.iter().chain(&self.body).chain(&self.footer)
    }
}

#[derive(Debug, Clone)]
pub struct PrinterProfile {
    pub width_chars: u8,
    pub encoding: PrinterEncoding,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PrinterEncoding {
    Ascii,
}

pub const PROFILE_58MM: PrinterProfile = PrinterProfile {
    width_chars: 32,
    encoding: PrinterEncoding::Ascii,
};

pub const PROFILE_80MM: PrinterProfile = PrinterProfile {
    width_chars: 48,
    encoding: PrinterEncoding::Ascii,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::printing::renderer::{LegacyRenderer, ReceiptRenderer};

    fn sample_doc() -> ReceiptDocument {
        ReceiptDocument {
            header: vec![
                ReceiptBlock {
                    style: BlockStyle::Centered,
                    lines: vec![BlockLine {
                        text: "ZANPOS".into(),
                        font: BlockFont::FontA,
                    }],
                },
                ReceiptBlock {
                    style: BlockStyle::Normal,
                    lines: vec![BlockLine {
                        text: "Manama Branch".into(),
                        font: BlockFont::FontA,
                    }],
                },
            ],
            body: vec![ReceiptBlock {
                style: BlockStyle::Normal,
                lines: vec![
                    BlockLine {
                        text: "1x Milk 2L    1.500 BHD".into(),
                        font: BlockFont::FontA,
                    },
                    BlockLine {
                        text: "2x Bread      0.600 BHD".into(),
                        font: BlockFont::FontA,
                    },
                ],
            }],
            footer: vec![
                ReceiptBlock {
                    style: BlockStyle::Bold,
                    lines: vec![BlockLine {
                        text: "TOTAL: 2.100 BHD".into(),
                        font: BlockFont::FontA,
                    }],
                },
                ReceiptBlock {
                    style: BlockStyle::Normal,
                    lines: vec![BlockLine {
                        text: "Thank you!".into(),
                        font: BlockFont::FontA,
                    }],
                },
            ],
            cut_after: true,
        }
    }

    #[test]
    fn english_receipt_bytes_are_non_empty() {
        let bytes = LegacyRenderer.render(&sample_doc(), &PROFILE_80MM);

        assert!(
            bytes.starts_with(&[0x1b, 0x40]),
            "receipt must initialize the printer"
        );
        assert!(
            bytes
                .windows(b"ZANPOS".len())
                .any(|window| window == b"ZANPOS"),
            "receipt must contain the document header"
        );
        assert!(
            bytes.ends_with(&[0x1d, 0x56, 0x00]),
            "cut_after must emit the ESC/POS cut command"
        );
    }

    #[test]
    fn receipt_has_expected_block_count() {
        let document = sample_doc();
        let blocks: Vec<_> = document.all_blocks().collect();
        assert_eq!(
            blocks.len(),
            5,
            "Header(2) + Body(1) + Footer(2) = 5 blocks"
        );
    }

    #[cfg(feature = "escpos-driver")]
    #[test]
    fn escpos_renderer_produces_cut_command() {
        use crate::printing::escpos::EscposRenderer;

        let renderer = EscposRenderer::new(&PROFILE_80MM);
        let bytes = renderer.render(&sample_doc(), &PROFILE_80MM);
        assert!(
            bytes.windows(3).any(|window| window == [0x1d, 0x56, 0x00]),
            "ESC/POS cut command must be present when cut_after is true"
        );
    }
}
