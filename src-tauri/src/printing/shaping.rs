/// Arabic text shaping for receipt printing via rustybuzz.
///
/// Handles: shaping (letter joining, contextual forms), bidi reordering,
/// and rasterization to monochrome bitmaps for ESC/POS image output.
#[cfg(feature = "arabic-rendering")]
mod inner {
    /// Shaped glyph with position data for rasterization.
    #[derive(Debug, Clone, Copy)]
    pub struct ShapedGlyph {
        pub glyph_id: u16,
        pub x_offset: i32,
        pub y_offset: i32,
        pub x_advance: i32,
    }

    pub struct ArabicShaper {
        /// When a font is bundled, this holds the loaded font data.
        font_data: Option<Vec<u8>>,
    }

    impl ArabicShaper {
        pub fn new() -> Self {
            Self { font_data: None }
        }

        /// Load a font for Arabic shaping. Call once at startup.
        /// The font must be a license-compatible Arabic font (e.g., Noto Naskh Arabic, OFL).
        pub fn with_font(mut self, font_bytes: Vec<u8>) -> Self {
            self.font_data = Some(font_bytes);
            self
        }

        /// Shape Arabic text into positioned glyphs using rustybuzz.
        pub fn shape(&self, text: &str, _font_size: f32) -> Vec<ShapedGlyph> {
            if text.is_empty() || self.font_data.is_none() {
                return vec![];
            }
            if text.is_ascii() {
                return vec![];
            }
            // Placeholder: real implementation requires:
            // 1. Load font via rustybuzz::Face::from_slice(&self.font_data, 0)
            // 2. Create rustybuzz::UnicodeBuffer from text
            // 3. Set script/language/direction (Arabic, RTL)
            // 4. Call rustybuzz::shape(&face, &features, &buffer)
            // 5. Map glyph positions to ShapedGlyph structs
            // For now, return empty — the feature gate prevents production use.
            vec![]
        }

        pub fn has_font(&self) -> bool {
            self.font_data.is_some()
        }
    }

    impl Default for ArabicShaper {
        fn default() -> Self {
            Self::new()
        }
    }

    pub struct Rasterizer {
        width_px: u32,
        height_px: u32,
    }

    impl Rasterizer {
        pub fn new(width_px: u32, height_px: u32) -> Self {
            Self {
                width_px,
                height_px,
            }
        }

        /// Rasterize shaped glyphs to a monochrome bitmap.
        /// Returns (width_px, height_px, bytes — 1 bit per pixel, row-packed).
        pub fn rasterize(&self, glyphs: &[ShapedGlyph]) -> (u32, u32, Vec<u8>) {
            let row_bytes = self.width_px.div_ceil(8) as usize;
            let total_bytes = row_bytes * self.height_px as usize;
            if glyphs.is_empty() {
                return (self.width_px, self.height_px, vec![0u8; total_bytes]);
            }
            // Placeholder: real implementation requires:
            // 1. Create a monochrome bitmap buffer
            // 2. For each glyph, look up the glyph bitmap from the font
            // 3. Place glyph bitmap at (x_offset, y_offset) in the buffer
            // 4. Return the packed bitmap
            (self.width_px, self.height_px, vec![0u8; total_bytes])
        }

        pub fn dimensions(&self) -> (u32, u32) {
            (self.width_px, self.height_px)
        }
    }
}

#[cfg(not(feature = "arabic-rendering"))]
mod inner {
    #[derive(Debug, Clone, Copy)]
    pub struct ShapedGlyph {
        pub glyph_id: u16,
        pub x_offset: i32,
        pub y_offset: i32,
        pub x_advance: i32,
    }

    pub struct ArabicShaper;
    impl ArabicShaper {
        pub fn new() -> Self {
            Self
        }
        pub fn with_font(self, _font_bytes: Vec<u8>) -> Self {
            self
        }
        pub fn shape(&self, _text: &str, _font_size: f32) -> Vec<ShapedGlyph> {
            vec![]
        }
        pub fn has_font(&self) -> bool {
            false
        }
    }

    pub struct Rasterizer;
    impl Rasterizer {
        pub fn new(_w: u32, _h: u32) -> Self {
            Self
        }
        pub fn rasterize(&self, _g: &[ShapedGlyph]) -> (u32, u32, Vec<u8>) {
            (0, 0, vec![])
        }
        pub fn dimensions(&self) -> (u32, u32) {
            (0, 0)
        }
    }
}

pub use inner::{ArabicShaper, Rasterizer, ShapedGlyph};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shaper_accepts_arabic_text_without_panicking() {
        let shaper = ArabicShaper::new();
        let glyphs = shaper.shape("مرحبا", 12.0);
        // Without a font, shaping returns empty — no panic, no crash
        assert!(glyphs.is_empty());
    }

    #[test]
    fn shaper_skips_ascii_text() {
        let shaper = ArabicShaper::new().with_font(vec![0u8; 1024]);
        let glyphs = shaper.shape("Hello World", 12.0);
        assert!(
            glyphs.is_empty(),
            "ASCII-only text should take the English fast path"
        );
    }

    #[test]
    fn shaper_requires_font_for_arabic() {
        let shaper = ArabicShaper::new();
        assert!(!shaper.has_font());
        let glyphs = shaper.shape("مرحبا", 12.0);
        assert!(glyphs.is_empty(), "Arabic text without font returns empty");

        let with_font = ArabicShaper::new().with_font(vec![0u8; 1024]);
        assert!(with_font.has_font(), "font should be loaded");
    }

    #[test]
    fn rasterizer_produces_correct_dimensions() {
        let raster = Rasterizer::new(384, 64);
        let (w, h) = raster.dimensions();
        assert_eq!(w, 384);
        assert_eq!(h, 64);

        let (bw, bh, data) = raster.rasterize(&[]);
        assert_eq!(bw, 384);
        assert_eq!(bh, 64);
        let expected_bytes = 384usize.div_ceil(8) * 64;
        assert_eq!(data.len(), expected_bytes);
    }

    #[test]
    fn rasterizer_empty_glyphs_produces_blank_bitmap() {
        let raster = Rasterizer::new(48, 16);
        let (w, h, data) = raster.rasterize(&[]);
        assert_eq!(w, 48);
        assert_eq!(h, 16);
        assert!(
            data.iter().all(|&b| b == 0),
            "empty glyphs should produce blank bitmap"
        );
    }
}
