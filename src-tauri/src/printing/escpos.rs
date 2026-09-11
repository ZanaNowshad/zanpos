use crate::printing::document::{PrinterProfile, ReceiptDocument};
use crate::printing::renderer::ReceiptRenderer;
use std::cell::RefCell;
use std::io::Write as IoWrite;
use std::rc::Rc;

pub struct EscposRenderer;

impl EscposRenderer {
    pub fn new(_profile: &PrinterProfile) -> Self {
        Self
    }
}

impl ReceiptRenderer for EscposRenderer {
    fn render(&self, doc: &ReceiptDocument, profile: &PrinterProfile) -> Vec<u8> {
        #[cfg(feature = "escpos-driver")]
        {
            use escpos::driver::Driver;
            use escpos::errors::PrinterError;
            use escpos::errors::Result as EscposResult;
            use escpos::printer::Printer;
            use escpos::utils::Protocol;

            struct BytesDriver(Rc<RefCell<Vec<u8>>>);

            impl Driver for BytesDriver {
                fn name(&self) -> String {
                    "bytes".to_owned()
                }
                fn write(&self, data: &[u8]) -> EscposResult<()> {
                    self.0
                        .borrow_mut()
                        .write_all(data)
                        .map_err(|e| PrinterError::Io(e.to_string()))
                }
                fn flush(&self) -> EscposResult<()> {
                    Ok(())
                }
            }

            let buf = Rc::new(RefCell::new(Vec::new()));
            let driver = BytesDriver(Rc::clone(&buf));
            let mut printer = Printer::new(driver, Protocol::default());
            let _ = printer.init();

            for block in &doc.header {
                render_block_native(&mut printer, block, profile);
            }
            for block in &doc.body {
                render_block_native(&mut printer, block, profile);
            }
            for block in &doc.footer {
                render_block_native(&mut printer, block, profile);
            }
            let _ = printer.print();

            // The cut is written straight to the buffer rather than through
            // `printer.cut()`, because the two renderers must emit the same
            // bytes and the crate's does not: escpos 0.6 sends
            // GS_PAPER_CUT_FULL = [GS, 'V', 'A', 0] (the four-byte feed-and-cut,
            // function B), while the fallback renderer this path stands in for
            // sends [0x1d, 0x56, 0x00] (the three-byte full cut, function A) at
            // `renderer.rs:68`. Both are valid ESC/POS and a printer accepts
            // either, but a receipt should not change shape depending on which
            // Cargo feature was enabled, and both tests here assert the
            // three-byte form. Emitting it directly keeps the two paths
            // byte-identical at the tail.
            if doc.cut_after {
                buf.borrow_mut().extend_from_slice(&[0x1d, 0x56, 0x00]);
            }

            let result = buf.borrow().clone();
            drop(printer);
            result
        }
        #[cfg(not(feature = "escpos-driver"))]
        {
            crate::printing::renderer::render_escpos_document(doc, profile)
        }
    }
}

#[cfg(feature = "escpos-driver")]
fn render_block_native(
    printer: &mut escpos::printer::Printer<impl escpos::driver::Driver>,
    block: &crate::printing::document::ReceiptBlock,
    profile: &PrinterProfile,
) {
    use crate::printing::document::BlockStyle;
    use escpos::utils::JustifyMode;

    let justify = match block.style {
        BlockStyle::Centered => JustifyMode::CENTER,
        _ => JustifyMode::LEFT,
    };
    let _ = printer.justify(justify);

    if matches!(block.style, BlockStyle::Bold) {
        let _ = printer.bold(true);
    }
    if matches!(block.style, BlockStyle::DoubleHeight) {
        let _ = printer.size(1, 2);
    }

    // Clamped to the roll width with the same helper the fallback renderer
    // uses. Sending the raw string instead let the printer wrap it, so a
    // product name longer than the paper broke the column layout on one path
    // and not the other.
    let width = profile.width_chars as usize;
    for line in &block.lines {
        let _ = printer.writeln(&crate::printing::renderer::pad_to(&line.text, width));
    }

    let _ = printer.bold(false);
    let _ = printer.size(1, 1);
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

    /// A line wider than the roll must be clamped to the roll, whichever
    /// renderer produced it.
    ///
    /// This is ledger PRINT1. The native driver path used to send `line.text`
    /// straight to `printer.writeln`, leaving the printer to wrap it, while the
    /// fallback truncated at `profile.width_chars`. A product name longer than
    /// the paper therefore broke the column layout on a 58 mm roll under one
    /// feature flag and not the other.
    ///
    /// This file only exists under `escpos-driver`, so this covers the native
    /// path alone. The clamp is a property of the receipt rather than of the
    /// driver, so the same contract is asserted against the fallback in
    /// `renderer.rs` — which is the path that actually ships, since
    /// `default = []`.
    #[test]
    fn a_line_wider_than_the_roll_is_clamped_to_it() {
        let long_name = "EXTRA LONG PRODUCT NAME THAT OVERFLOWS THE ROLL";
        assert!(
            long_name.len() > PROFILE_58MM.width_chars as usize,
            "the fixture has to actually overflow to test anything"
        );

        let doc = ReceiptDocument {
            header: vec![],
            body: vec![ReceiptBlock {
                style: BlockStyle::Normal,
                lines: vec![BlockLine {
                    text: long_name.into(),
                    font: BlockFont::FontA,
                }],
            }],
            footer: vec![],
            cut_after: false,
        };

        let bytes = EscposRenderer::new(&PROFILE_58MM).render(&doc, &PROFILE_58MM);
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
            "expected the line truncated to {} columns",
            PROFILE_58MM.width_chars
        );
    }

    /// The clamp follows the profile rather than being a fixed number: the same
    /// line keeps more characters on 80 mm than on 58 mm.
    #[test]
    fn a_wider_roll_keeps_more_of_the_line() {
        let long_name = "EXTRA LONG PRODUCT NAME THAT OVERFLOWS THE ROLL";
        let doc = ReceiptDocument {
            header: vec![],
            body: vec![ReceiptBlock {
                style: BlockStyle::Normal,
                lines: vec![BlockLine {
                    text: long_name.into(),
                    font: BlockFont::FontA,
                }],
            }],
            footer: vec![],
            cut_after: false,
        };

        let narrow = EscposRenderer::new(&PROFILE_58MM).render(&doc, &PROFILE_58MM);
        let wide = EscposRenderer::new(&PROFILE_80MM).render(&doc, &PROFILE_80MM);

        let kept = |bytes: &[u8], width: u8| {
            let text = String::from_utf8_lossy(bytes).to_string();
            let expected: String = long_name.chars().take(width as usize).collect();
            text.contains(&expected)
        };

        assert!(kept(&narrow, PROFILE_58MM.width_chars));
        assert!(kept(&wide, PROFILE_80MM.width_chars));
        assert!(
            wide.len() > narrow.len(),
            "the wider roll should carry more of the line, not the same"
        );
    }
}
