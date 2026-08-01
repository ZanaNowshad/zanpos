#[allow(unused)]
pub struct ArabicShaper;

#[cfg(feature = "arabic-rendering")]
impl ArabicShaper {
    pub fn new() -> Self {
        Self
    }

    pub fn shape(&self, _text: &str) -> Vec<u8> {
        vec![]
    }
}
