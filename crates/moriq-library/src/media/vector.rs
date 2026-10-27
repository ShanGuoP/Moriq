//! Bounded local SVG rendering. No file resolver or network resources.
use super::*;
/// 从本地 SVG 建立渲染树，并按资源读取策略限制外部内容。
fn tree(file: File) -> Result<resvg::usvg::Tree> {
    let mut bytes = Vec::new();
    file.take(16 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(Error::Invalid("SVG 超出16 MiB预算".into()));
    }
    static FONTS: std::sync::OnceLock<std::sync::Arc<resvg::usvg::fontdb::Database>> =
        std::sync::OnceLock::new();
    let fonts = FONTS.get_or_init(|| {
        let mut db = resvg::usvg::fontdb::Database::new();
        db.load_system_fonts();
        db.set_sans_serif_family(if cfg!(windows) {
            "Microsoft YaHei"
        } else {
            "Arial"
        });
        std::sync::Arc::new(db)
    });
    let mut options = resvg::usvg::Options {
        fontdb: fonts.clone(),
        ..Default::default()
    };
    options.image_href_resolver.resolve_string = Box::new(|_, _| None);
    resvg::usvg::Tree::from_data(&bytes, &options)
        .map_err(|e| Error::Invalid(format!("SVG 无法解析：{e}")))
}
/// 将渲染器的预乘像素转换为统一 RGBA 输出。
fn rgba(mut pixmap: resvg::tiny_skia::Pixmap) -> Result<image::RgbaImage> {
    let (w, h) = (pixmap.width(), pixmap.height());
    for p in pixmap.data_mut().as_chunks_mut::<4>().0 {
        let a = u32::from(p[3]);
        for c in &mut p[..3] {
            *c = (u32::from(*c) * 255 + a / 2)
                .checked_div(a)
                .unwrap_or(0)
                .min(255) as u8;
        }
    }
    image::RgbaImage::from_raw(w, h, pixmap.take())
        .ok_or_else(|| Error::Invalid("SVG 像素无效".into()))
}
/// 按指定原图区域栅格化 SVG，返回用于查看器的像素块。
pub(super) fn svg_region(file: File, region: ImageRegion) -> Result<Pixels> {
    let tree = tree(file)?;
    let scale = 1. / region.level as f32;
    let (w, h) = (
        region.width.div_ceil(region.level),
        region.height.div_ceil(region.level),
    );
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h)
        .ok_or_else(|| Error::Invalid("SVG 区域分配失败".into()))?;
    let transform = resvg::tiny_skia::Transform::from_row(
        scale,
        0.,
        0.,
        scale,
        -(region.x as f32) * scale,
        -(region.y as f32) * scale,
    );
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    Ok(pixels(image::DynamicImage::ImageRgba8(rgba(pixmap)?)))
}
/// 按目标最长边栅格化 SVG，输出与其他媒体一致的解码结果。
pub(super) fn svg(file: File, edge: u32) -> Result<native::Decoded> {
    let tree = tree(file)?;
    let size = tree.size();
    let width = size.width().ceil() as u32;
    let height = size.height().ceil() as u32;
    if width == 0 || height == 0 {
        return Err(Error::Invalid("SVG 尺寸无效".into()));
    }
    let scale = if edge > 0 {
        (edge as f32 / width.max(height) as f32).min(1.0)
    } else {
        1.0
    };
    let w = (width as f32 * scale).round().max(1.0) as u32;
    let h = (height as f32 * scale).round().max(1.0) as u32;
    if u64::from(w) * u64::from(h) > 64 * 1024 * 1024 {
        return Err(Error::Invalid("SVG 渲染超出内存预算".into()));
    }
    let mut pixmap =
        resvg::tiny_skia::Pixmap::new(w, h).ok_or_else(|| Error::Invalid("SVG 分配失败".into()))?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let image = rgba(pixmap)?;
    Ok(native::Decoded {
        width,
        height,
        image: Some(image::DynamicImage::ImageRgba8(image)),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    /// 验证 SVG 导入、预览、导出和维护复用同一本地渲染流程。
    fn svg_import_preview_export_and_maintenance_use_the_same_local_renderer() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shape.svg");
        let source=br#"<svg xmlns="http://www.w3.org/2000/svg" width="2000" height="1000"><rect width="2000" height="1000" fill="red" fill-opacity="0.5"/></svg>"#;
        fs::write(&path, source).unwrap();
        let mut lib = Library::create(dir.path(), "SVG").unwrap();
        lib.import(&path, None).unwrap();
        let asset = lib.page(&Query::default(), 0, 1).unwrap().assets.remove(0);
        assert_eq!(asset.format, "svg");
        assert_eq!((asset.width, asset.height), (2000, 1000));
        let preview = lib.preview(asset.id).unwrap();
        assert_eq!((preview.width, preview.height), (1600, 800));
        assert!(preview.rgba[0] > 250);
        assert!((126..=129).contains(&preview.rgba[3]));
        let out = export::run(
            lib.export_plan(&[asset.id]).unwrap(),
            dir.path(),
            ExportOptions {
                format: ExportFormat::Tiff,
                max_edge: 500,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            image::open(out.join("shape.tiff")).unwrap().dimensions(),
            (500, 250)
        );
        assert_eq!(fs::read(path).unwrap(), source);
    }
}
