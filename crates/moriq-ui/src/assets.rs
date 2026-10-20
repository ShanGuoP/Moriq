use iced::widget::image;
use std::collections::HashMap;
#[derive(Debug, Clone, Default)]
pub struct Artwork {
    pub images: HashMap<String, image::Handle>,
}
impl Artwork {
    pub fn get(&self, key: &str) -> Option<&image::Handle> {
        self.images.get(key)
    }
}
pub fn bundled() -> Vec<(&'static str, &'static [u8])> {
    // Real libraries supply their own covers. Design-reference photos are not distributed.
    Vec::new()
}
