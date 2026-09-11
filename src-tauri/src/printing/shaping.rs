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
            let font_data = Self::load_bundled_font().or_else(Self::load_resource_font);
            Self { font_data }
        }

        /// A shaper with no font, for exercising the no-font path.
        ///
        /// `new()` cannot serve that purpose: it falls back to
        /// `load_resource_font`, which reads `src-tauri/resources/arabic.ttf` —
        /// a file committed to the repository — so `new()` always finds a font
        /// on any checkout.
        #[cfg(test)]
        pub(crate) fn without_font() -> Self {
            Self { font_data: None }
        }

        /// Try loading from the compiled-in resource.
        fn load_bundled_font() -> Option<Vec<u8>> {
            // IBM Plex Sans Arabic or bundled Arabic font.
            // When embedded via include_bytes!, add:
            // Some(include_bytes!("../resources/arabic.ttf").to_vec())
            None
        }

        /// Try loading from the filesystem resource directory.
        fn load_resource_font() -> Option<Vec<u8>> {
            let path =
                std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/resources/arabic.ttf"));
            std::fs::read(path).ok()
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

            let font_bytes = self.font_data.as_ref().unwrap();
            let face = match rustybuzz::Face::from_slice(font_bytes, 0) {
                Some(f) => f,
                None => return vec![],
            };

            let mut buffer = rustybuzz::UnicodeBuffer::new();
            buffer.push_str(text);
            buffer.set_direction(rustybuzz::Direction::RightToLeft);

            let features: [rustybuzz::Feature; 0] = [];
            let glyph_infos = rustybuzz::shape(&face, &features, buffer);

            glyph_infos
                .glyph_infos()
                .iter()
                .zip(glyph_infos.glyph_positions().iter())
                .map(|(info, pos)| ShapedGlyph {
                    glyph_id: info.glyph_id as u16,
                    x_offset: pos.x_offset,
                    y_offset: pos.y_offset,
                    x_advance: pos.x_advance,
                })
                .collect()
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
    fn shaper_handles_font_loading() {
        let shaper = ArabicShaper::new();
        let glyphs = shaper.shape("مرحبا", 12.0);
        // If a font was loaded (bundled or resource), shaping produces glyphs.
        // If not, returns empty gracefully — no panic.
        if shaper.has_font() {
            assert!(!glyphs.is_empty(), "Arabic text must shape with font");
        }
    }

    #[test]
    fn shaper_with_explicit_font_produces_glyphs() {
        let font_data = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/resources/arabic.ttf"))
            .expect("Arabic font should exist in resources");
        let shaper = ArabicShaper::new().with_font(font_data);
        let glyphs = shaper.shape("مرحبا", 12.0);
        assert!(!glyphs.is_empty(), "Shaped Arabic text must produce glyphs");
        // Arabic joining: the glyph count differs from the character count due to ligatures
        assert!(
            glyphs.len() >= 2,
            "Arabic text should produce multiple positioned glyphs"
        );
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
        // Built without a font explicitly rather than via `new()`, which falls
        // back to a font committed at `src-tauri/resources/arabic.ttf` and so
        // always finds one — this assertion could never hold on any checkout.
        // It went unnoticed because the whole module sits behind the
        // `arabic-rendering` feature, which `default = []` leaves off.
        let shaper = ArabicShaper::without_font();
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
