use std::{env, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source = "../../assets/branding/moriq.svg";
    println!("cargo:rerun-if-changed={source}");
    let output = PathBuf::from(env::var_os("OUT_DIR").ok_or("Missing OUT_DIR")?);
    let tree = resvg::usvg::Tree::from_data(&fs::read(source)?, &Default::default())?;
    let mut frames = Vec::new();
    for size in [16u32, 24, 32, 48, 64, 128, 256] {
        let mut pixels = resvg::tiny_skia::Pixmap::new(size, size).ok_or("Invalid icon size")?;
        let scale = size as f32 / 28.0;
        let transform =
            resvg::tiny_skia::Transform::from_row(scale, 0.0, 0.0, scale, scale, 2.0 * scale);
        resvg::render(&tree, transform, &mut pixels.as_mut());
        let png = pixels.encode_png()?;
        if size == 256 {
            fs::write(output.join("moriq.png"), &png)?;
        }
        frames.push(image::codecs::ico::IcoFrame::with_encoded(
            png,
            size,
            size,
            image::ExtendedColorType::Rgba8,
        )?);
    }
    let icon = output.join("moriq.ico");
    image::codecs::ico::IcoEncoder::new(fs::File::create(&icon)?).encode_images(&frames)?;
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let resource = output.join("moriq.rc");
        fs::write(
            &resource,
            format!(
                "#pragma code_page(65001)\n1 ICON \"{}\"\n",
                icon.to_string_lossy().replace('\\', "/")
            ),
        )?;
        embed_resource::compile(resource, embed_resource::NONE)
            .manifest_optional()
            .map_err(|error| format!("Windows icon: {error:?}"))?;
    }
    Ok(())
}
