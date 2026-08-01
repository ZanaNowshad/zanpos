#[allow(unused)]
pub struct Rasterizer;

#[cfg(feature = "arabic-rendering")]
impl Rasterizer {
    pub fn new(_width_px: u32, _height_px: u32) -> Self {
        Self
    }

    pub fn rasterize(&self, _glyphs: &[u8]) -> Vec<u8> {
        vec![]
    }
}
