#[cfg(test)]
mod tests {
    use crate::printing::document::{
        BlockFont, BlockLine, BlockStyle, ReceiptBlock, ReceiptDocument, PrinterProfile, PROFILE_80MM,
    };
    use crate::printing::renderer::{LegacyRenderer, ReceiptRenderer};

    fn sample_doc() -> ReceiptDocument {
        ReceiptDocument {
            header: vec![
                ReceiptBlock {
                    style: BlockStyle::Centered,
                    lines: vec![BlockLine { text: "ZANPOS".into(), font: BlockFont::FontA }],
                },
                ReceiptBlock {
                    style: BlockStyle::Normal,
                    lines: vec![BlockLine { text: "Manama Branch".into(), font: BlockFont::FontA }],
                },
            ],
            body: vec![ReceiptBlock {
                style: BlockStyle::Normal,
                lines: vec![
                    BlockLine { text: "1x Milk 2L    1.500 BHD".into(), font: BlockFont::FontA },
                    BlockLine { text: "2x Bread      0.600 BHD".into(), font: BlockFont::FontA },
                ],
            }],
            footer: vec![
                ReceiptBlock {
                    style: BlockStyle::Bold,
                    lines: vec![BlockLine { text: "TOTAL: 2.100 BHD".into(), font: BlockFont::FontA }],
                },
                ReceiptBlock {
                    style: BlockStyle::Normal,
                    lines: vec![BlockLine { text: "Thank you!".into(), font: BlockFont::FontA }],
                },
            ],
            cut_after: true,
        }
    }

    #[test]
    fn english_receipt_bytes_are_non_empty() {
        let doc = sample_doc();
        let renderer = LegacyRenderer;
        let bytes = renderer.render(&doc, &PROFILE_80MM);
        assert!(!bytes.is_empty(), "Renderer must produce ESC/POS bytes");
    }

    #[test]
    fn receipt_has_expected_block_count() {
        let doc = sample_doc();
        let blocks: Vec<_> = doc.all_blocks().collect();
        assert_eq!(blocks.len(), 5, "Header(2) + Body(1) + Footer(2) = 5 blocks");
    }

    #[cfg(feature = "escpos-driver")]
    #[test]
    fn escpos_renderer_produces_cut_command() {
        use crate::printing::escpos::EscposRenderer;
        let doc = sample_doc();
        let renderer = EscposRenderer::new(&PROFILE_80MM);
        let bytes = renderer.render(&doc, &PROFILE_80MM);
        assert!(!bytes.is_empty());
        assert!(
            bytes.windows(3).any(|w| w == [0x1d, 0x56, 0x00]),
            "ESC/POS cut command must be present when cut_after is true"
        );
    }
}
