//! Isolated visual fixtures. Usage: cargo run -p moriq-library --example smoke_library -- target/test-artifacts
use moriq_library::Library;
use std::{error::Error, path::PathBuf};
fn main() -> Result<(), Box<dyn Error>> {
    let parent = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("pass an existing scratch directory")?,
    );
    let source_dir = tempfile::Builder::new()
        .prefix("moriq-fixtures-")
        .tempdir_in(&parent)?;
    let mut library = Library::create(&parent, "视觉验证样本")?;
    let project = library.create_project("旅行色彩 · 验证样本")?;
    library.create_project("空项目 · 封面隔离验证")?;
    let colors = [
        [95, 147, 175],
        [219, 169, 146],
        [127, 159, 146],
        [140, 146, 185],
        [216, 185, 134],
        [101, 154, 170],
    ];
    for i in 0..30 {
        let (w, h) = if i % 3 == 0 { (480, 640) } else { (760, 480) };
        let palette = colors[i % colors.len()];
        let mut image = image::RgbImage::new(w, h);
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            let hill =
                (h as f32 * 0.52 + (x as f32 / 90.0 + i as f32).sin() * h as f32 * 0.12) as u32;
            let value = if y < hill {
                let t = y as f32 / h as f32;
                [
                    palette[0] as f32 * (0.75 + t * 0.25),
                    palette[1] as f32 * (0.8 + t * 0.2),
                    palette[2] as f32 * (0.9 + t * 0.1),
                ]
            } else {
                [
                    palette[0] as f32 * 0.65,
                    palette[1] as f32 * 0.7,
                    palette[2] as f32 * 0.72,
                ]
            };
            let sun = (x as i64 - w as i64 * 3 / 4).pow(2) + (y as i64 - h as i64 / 4).pow(2)
                < (h as i64 / 12).pow(2);
            *pixel = if sun {
                image::Rgb([253, 235, 193])
            } else {
                image::Rgb(value.map(|v| v as u8))
            };
        }
        let path = source_dir.path().join(format!("色彩习作_{:02}.png", i + 1));
        image.save(&path)?;
        library.import(&path, Some(project))?;
    }
    let first = library.page(&Default::default(), 0, 4)?;
    for asset in first.assets {
        library.set_favorite(asset.id, true)?;
        library.set_tags(asset.id, &["风景".into(), "色彩".into()])?;
    }
    println!("{}", library.root().display());
    Ok(())
}
