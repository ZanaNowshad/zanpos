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
            use escpos::utils::{JustifyMode, Protocol};

            struct BytesDriver(Rc<RefCell<Vec<u8>>>);

            impl Driver for BytesDriver {
                fn name(&self) -> String { "bytes".to_owned() }
                fn write(&self, data: &[u8]) -> EscposResult<()> {
                    self.0.borrow_mut().write_all(data)
                        .map_err(|e| PrinterError::Io(e.to_string()))
                }
                fn flush(&self) -> EscposResult<()> { Ok(()) }
            }

            let buf = Rc::new(RefCell::new(Vec::new()));
            let driver = BytesDriver(Rc::clone(&buf));
            let mut printer = Printer::new(driver, Protocol::default());
            let _ = printer.init();

            for block in &doc.header {
                render_block_native(&mut printer, block);
            }
            for block in &doc.body {
                render_block_native(&mut printer, block);
            }
            for block in &doc.footer {
                render_block_native(&mut printer, block);
            }
            if doc.cut_after {
                let _ = printer.cut();
            }
            let _ = printer.print();
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

    for line in &block.lines {
        let _ = printer.writeln(&line.text);
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
}
