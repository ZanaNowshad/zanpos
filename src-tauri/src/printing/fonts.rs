#[allow(unused)]
pub struct BundledFont;

#[cfg(feature = "arabic-rendering")]
impl BundledFont {
    pub fn load_arabic() -> Result<Self, String> {
        Ok(Self)
    }
}
