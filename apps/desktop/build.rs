use std::{env, fs, path::PathBuf};

/// 生成桌面程序所需的图标和平台资源，并登记资源文件的重建依赖。
fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-env-changed=MORIQ_BUILD_ID");
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
        let version = env::var("CARGO_PKG_VERSION")?;
        let mut parts = version.split('.').take(3).collect::<Vec<_>>();
        while parts.len() < 3 {
            parts.push("0");
        }
        let numeric = format!("{},0", parts.join(","));
        let identity = env::var("MORIQ_BUILD_ID").unwrap_or_else(|_| "development".into());
        let identity = identity.replace(['\"', '\\', '\n', '\r'], "_");
        let resource = output.join("moriq.rc");
        fs::write(
            &resource,
            format!(
                "#pragma code_page(65001)\n1 ICON \"{}\"\n1 VERSIONINFO\nFILEVERSION {numeric}\nPRODUCTVERSION \
                {numeric}\nFILEOS 0x40004\nFILETYPE 1\nBEGIN\n BLOCK \"StringFileInfo\"\n BEGIN\n  BLOCK \
                \"040904b0\"\n  BEGIN\n   VALUE \"CompanyName\", \"杉果派\\0\"\n   VALUE \"ProductName\", \"Moriq \
                Space\\0\"\n   VALUE \"FileDescription\", \"Moriq Space 本地素材管理器\\0\"\n   VALUE \"FileVersion\", \
                \"{version}\\0\"\n   VALUE \"ProductVersion\", \"{version} ({identity})\\0\"\n   VALUE \
                \"OriginalFilename\", \"Moriq Space.exe\\0\"\n  END\n END\n BLOCK \"VarFileInfo\"\n BEGIN\n  VALUE \
                \"Translation\", 0x0409, 1200\n END\nEND\n",
                icon.to_string_lossy().replace('\\', "/")
            ),
        )?;
        embed_resource::compile(resource, embed_resource::NONE)
            .manifest_optional()
            .map_err(|error| format!("Windows icon: {error:?}"))?;
    }
    Ok(())
}
