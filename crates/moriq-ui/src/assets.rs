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
    macro_rules! resource {
        ($name:literal) => {
            (
                $name,
                include_bytes!(concat!("../assets/", $name, ".png")).as_slice(),
            )
        };
    }
    vec![
        resource!("hero"),
        resource!("hero-cover"),
        resource!("detail-mountain"),
        resource!("project-mountain"),
        resource!("project-building"),
        resource!("project-spring"),
        resource!("project-sea"),
        resource!("favorite-mountain"),
        resource!("favorite-cat"),
        resource!("favorite-plant"),
        resource!("favorite-coast"),
        resource!("library-mountain"),
        resource!("library-building"),
        resource!("library-ai"),
        resource!("library-sea"),
        resource!("category-portrait"),
        resource!("category-landscape"),
        resource!("category-building"),
        resource!("category-illustration"),
        resource!("category-screen"),
        resource!("category-other"),
        resource!("photo-01"),
        resource!("photo-02"),
        resource!("photo-03"),
        resource!("photo-04"),
        resource!("photo-05"),
        resource!("photo-06"),
        resource!("photo-07"),
        resource!("photo-08"),
        resource!("photo-09"),
        resource!("photo-10"),
        resource!("photo-11"),
        resource!("photo-12"),
        resource!("photo-13"),
        resource!("photo-14"),
        resource!("photo-15"),
        resource!("photo-16"),
    ]
}
