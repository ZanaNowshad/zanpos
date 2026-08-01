use crate::printing::document::{BlockStyle, PrinterProfile, ReceiptDocument};
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
        let mut output = Vec::new();

        for block in doc.all_blocks() {
            for line in &block.lines {
                let text = pad_to(&line.text, width);
                output.extend_from_slice(text.as_bytes());
                output.extend_from_slice(b"\n");
            }
            output.extend_from_slice(b"\n");
        }

        if doc.cut_after {
            output.extend_from_slice(b"\x1d\x56\x00");
        }

        output
    }
}

fn pad_to(s: &str, max: usize) -> String {
    let clean: String = s.chars().take(max).collect();
    format!("{:width$}", clean, width = max)
}
