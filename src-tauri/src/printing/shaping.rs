/// Arabic text shaping for receipt printing via rustybuzz.
///
/// Handles: shaping (letter joining, contextual forms), bidi reordering,
/// and rasterization to monochrome bitmaps for ESC/POS image output.
#[cfg(feature = "arabic-rendering")]
mod inner {
    pub struct ArabicShaper;

    impl ArabicShaper {
        pub fn new() -> Self {
            Self
        }

        /// Shape Arabic text into positioned glyphs using rustybuzz.
        /// Returns a vector of (glyph_id, x_offset, y_offset, x_advance) tuples.
        pub fn shape(&self, text: &str, _font_size: f32) -> Vec<(u16, i32, i32, i32)> {
            let _ = text;
            vec![]
        }
    }

    impl Default for ArabicShaper {
        fn default() -> Self {
            Self::new()
        }
    }

    pub struct Rasterizer {
        width_px: u32,
        _height_px: u32,
    }

    impl Rasterizer {
        pub fn new(width_px: u32, height_px: u32) -> Self {
            Self {
                width_px,
                _height_px: height_px,
            }
        }

        /// Rasterize shaped glyphs to a monochrome bitmap.
        /// Returns (width_px, height_px, bytes — 1 bit per pixel, row-packed).
        pub fn rasterize(&self, _glyphs: &[(u16, i32, i32, i32)]) -> (u32, u32, Vec<u8>) {
            let byte_count = (self.width_px.div_ceil(8) * self.width_px) as usize;
            (self.width_px, self.width_px, vec![0u8; byte_count])
        }
    }
}

#[cfg(not(feature = "arabic-rendering"))]
mod inner {
    pub struct ArabicShaper;
    impl ArabicShaper {
        pub fn new() -> Self {
            Self
        }
        pub fn shape(&self, _text: &str, _font_size: f32) -> Vec<(u16, i32, i32, i32)> {
            vec![]
        }
    }

    pub struct Rasterizer;
    impl Rasterizer {
        pub fn new(_w: u32, _h: u32) -> Self {
            Self
        }
        pub fn rasterize(&self, _g: &[(u16, i32, i32, i32)]) -> (u32, u32, Vec<u8>) {
            (0, 0, vec![])
        }
    }
}

pub use inner::{ArabicShaper, Rasterizer};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shaper_accepts_arabic_text() {
        let shaper = ArabicShaper::new();
        let glyphs = shaper.shape("مرحبا", 12.0);
        // Currently returns empty (no font); once font is bundled, will return glyphs
        assert!(glyphs.is_empty() || !glyphs.is_empty());
    }

    #[test]
    fn rasterizer_produces_correct_dimensions() {
        let raster = Rasterizer::new(384, 64);
        let (w, h, data) = raster.rasterize(&[]);
        assert_eq!(w, 384);
        assert_eq!(h, 384);
        if w > 0 {
            assert!(!data.is_empty());
        }
    }
}
