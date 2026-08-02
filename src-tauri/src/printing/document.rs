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
