//! Extract only photographic artwork from the user-supplied design references.
//! This is a development asset pipeline, never a screenshot used as an application UI.
use std::{error::Error, path::Path};
fn main() -> Result<(), Box<dyn Error>> {
    let target = Path::new("crates/moriq-ui/assets");
    std::fs::create_dir_all(target)?;
    let overview = image::open("docs/UI/dffe2094-19dc-4faf-a05b-b38c0dc159bd.png")?;
    let gallery = image::open("docs/UI/caa00ec5-b8f8-48d3-b23c-a04530c84985.png")?;
    let models = image::open("docs/UI/bd061e12-8004-4abd-bfc0-e2c0ca643b14.png")?;
    for (name, x, y, w, h) in [
        ("project-mountain", 285, 532, 227, 88),
        ("project-building", 535, 532, 226, 88),
        ("project-spring", 784, 532, 224, 88),
        ("project-sea", 1034, 532, 220, 88),
        ("favorite-mountain", 286, 729, 112, 70),
        ("favorite-cat", 414, 729, 112, 70),
        ("favorite-plant", 549, 729, 112, 70),
        ("favorite-coast", 682, 729, 112, 70),
        ("library-mountain", 36, 292, 48, 47),
        ("library-building", 36, 358, 48, 47),
        ("library-ai", 36, 428, 48, 47),
        ("library-sea", 36, 497, 48, 47),
        ("hero-cover", 306, 117, 106, 108),
    ] {
        overview
            .crop_imm(x, y, w, h)
            .save(target.join(format!("{name}.png")))?;
    }
    for (name, x, y, w, h) in [
        ("photo-01", 304, 202, 245, 173),
        ("photo-02", 304, 390, 245, 172),
        ("photo-03", 304, 581, 245, 157),
        ("photo-04", 304, 757, 245, 167),
        ("photo-05", 570, 203, 194, 125),
        ("photo-06", 570, 349, 194, 225),
        ("photo-07", 570, 592, 194, 179),
        ("photo-08", 570, 792, 194, 132),
        ("photo-09", 784, 202, 213, 181),
        ("photo-10", 784, 402, 213, 133),
        ("photo-11", 784, 555, 213, 159),
        ("photo-12", 784, 737, 213, 172),
        ("photo-13", 1017, 202, 223, 161),
        ("photo-14", 1017, 383, 223, 165),
        ("photo-15", 1017, 568, 223, 181),
        ("photo-16", 1017, 770, 223, 99),
        ("hero", 1016, 565, 225, 186),
        ("detail-mountain", 1325, 123, 166, 125),
        ("avatar", 1456, 35, 18, 18),
    ] {
        gallery
            .crop_imm(x, y, w, h)
            .save(target.join(format!("{name}.png")))?;
    }
    for (name, x, y, w, h) in [
        ("category-portrait", 299, 251, 391, 97),
        ("category-landscape", 714, 251, 410, 97),
        ("category-building", 1148, 251, 386, 97),
        ("category-illustration", 300, 431, 389, 93),
        ("category-screen", 715, 431, 409, 93),
        ("category-other", 1149, 431, 384, 93),
    ] {
        models
            .crop_imm(x, y, w, h)
            .save(target.join(format!("{name}.png")))?;
    }
    Ok(())
}
