//! Shared source ICC -> sRGB conversion for import, preview and converted export.
use super::*;
use moxcms::{ColorProfile, DataColorSpace, Layout, TransformOptions};
pub(super) fn to_srgb(
    image: image::DynamicImage,
    icc: Option<&[u8]>,
) -> Result<image::DynamicImage> {
    let Some(icc) = icc else { return Ok(image) };
    if icc.len() > 4 * 1024 * 1024 {
        return Err(Error::Color("ICC 超出4 MiB预算".into()));
    }
    let profile = ColorProfile::new_from_slice(icc).map_err(|e| Error::Color(e.to_string()))?;
    if profile.color_space != DataColorSpace::Rgb {
        return Err(Error::Color(
            "当前仅支持 RGB ICC，灰度/CMYK 配置待适配；原文件仍可导出".into(),
        ));
    }
    let dest = ColorProfile::new_srgb();
    if matches!(
        image,
        image::DynamicImage::ImageRgb16(_) | image::DynamicImage::ImageRgba16(_)
    ) {
        let mut pixels = image.to_rgba16();
        let transform = profile
            .create_in_place_transform_16bit(Layout::Rgba, &dest, TransformOptions::default())
            .map_err(|e| Error::Color(e.to_string()))?;
        let stride = pixels.width() as usize * 4;
        for row in pixels.as_mut().chunks_exact_mut(stride) {
            transform
                .transform(row)
                .map_err(|e| Error::Color(e.to_string()))?;
        }
        Ok(image::DynamicImage::ImageRgba16(pixels))
    } else {
        let mut pixels = image.to_rgba8();
        let stride = pixels.width() as usize * 4;
        let transform = profile
            .create_in_place_transform_8bit(Layout::Rgba, &dest, TransformOptions::default())
            .map_err(|e| Error::Color(e.to_string()))?;
        for row in pixels.as_mut().chunks_exact_mut(stride) {
            transform
                .transform(row)
                .map_err(|e| Error::Color(e.to_string()))?;
        }
        Ok(image::DynamicImage::ImageRgba8(pixels))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use image::ImageEncoder;
    #[cfg(windows)]
    #[test]
    fn wic_preview_and_region_convert_adobe_rgb_to_srgb() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("AdobeRGB.png");
        let mut output = File::create(&source).unwrap();
        let mut encoder = image::codecs::png::PngEncoder::new(&mut output);
        encoder
            .set_icc_profile(ColorProfile::new_adobe_rgb().encode().unwrap())
            .unwrap();
        encoder
            .write_image(
                &[230, 80, 30, 255].repeat(16),
                4,
                4,
                image::ExtendedColorType::Rgba8,
            )
            .unwrap();
        drop(output);
        let original = fs::read(&source).unwrap();
        let normal = media::raster(File::open(&source).unwrap(), &source, 256)
            .unwrap()
            .image
            .unwrap()
            .to_rgba8();
        let native = native::decode(&source, 256)
            .unwrap()
            .image
            .unwrap()
            .to_rgba8();
        assert!(normal.get_pixel(0, 0)[0] > 245);
        assert!(
            normal
                .get_pixel(0, 0)
                .0
                .iter()
                .zip(native.get_pixel(0, 0).0)
                .all(|(a, b)| a.abs_diff(b) <= 4),
            "Rust {:?}, WIC {:?}",
            normal.get_pixel(0, 0),
            native.get_pixel(0, 0)
        );
        let region_output = dir.path().join("region.bin");
        region::run(
            native::worker().unwrap(),
            &source,
            &region_output,
            256,
            Some(ImageRegion {
                x: 0,
                y: 0,
                width: 4,
                height: 4,
                level: 0,
            }),
        )
        .unwrap();
        assert_eq!(
            &fs::read(region_output).unwrap()[24..28],
            native.get_pixel(0, 0).0
        );
        assert_eq!(fs::read(source).unwrap(), original);
    }
    #[test]
    fn icc_preview_full_resolution_and_converted_export_share_srgb_without_touching_original() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("AdobeRGB.png");
        let rgba = image::RgbaImage::from_pixel(2000, 16, image::Rgba([230, 80, 30, 177]));
        let profile = ColorProfile::new_adobe_rgb().encode().unwrap();
        let mut output = File::create(&source).unwrap();
        let mut encoder = image::codecs::png::PngEncoder::new(&mut output);
        encoder.set_icc_profile(profile).unwrap();
        encoder
            .write_image(rgba.as_raw(), 2000, 16, image::ExtendedColorType::Rgba8)
            .unwrap();
        drop(output);
        let original = fs::read(&source).unwrap();
        let mut lib = Library::create(dir.path(), "色彩").unwrap();
        lib.import(&source, None).unwrap();
        let asset = lib.page(&Query::default(), 0, 1).unwrap().assets.remove(0);
        let preview = lib.preview(asset.id).unwrap();
        assert_eq!(preview.width, 1600);
        let full = lib
            .media_source(asset.id)
            .unwrap()
            .preview_edge(0)
            .unwrap()
            .image;
        assert_eq!((full.width, full.height), (2000, 16));
        assert_eq!(full.rgba[3], 177);
        assert!(full.rgba[0] > 230);
        let folder = export::run(
            lib.export_plan(&[asset.id]).unwrap(),
            dir.path(),
            ExportOptions {
                format: ExportFormat::Png,
                max_edge: 1000,
                resampling: Resampling::Lanczos3,
                layout: ExportLayout::Format,
                metadata_sidecar: true,
                ..Default::default()
            },
        )
        .unwrap();
        let exported = image::open(folder.join("png/AdobeRGB.png"))
            .unwrap()
            .to_rgba8();
        assert_eq!(exported.dimensions(), (1000, 8));
        assert!(
            full.rgba[..4]
                .iter()
                .zip(exported.get_pixel(10, 2).0)
                .all(|(a, b)| a.abs_diff(b) <= 1)
        );
        assert!(folder.join("png/AdobeRGB.png.metadata.json").is_file());
        assert_eq!(fs::read(source).unwrap(), original);
    }
    #[test]
    fn invalid_icc_is_not_silently_redecoded_through_a_system_codec() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("bad.png");
        let mut output = File::create(&source).unwrap();
        let mut encoder = image::codecs::png::PngEncoder::new(&mut output);
        encoder.set_icc_profile(vec![42; 132]).unwrap();
        encoder
            .write_image(&[1, 2, 3], 1, 1, image::ExtendedColorType::Rgb8)
            .unwrap();
        drop(output);
        assert!(matches!(
            media::raster(File::open(source).unwrap(), Path::new("bad.png"), 256),
            Err(Error::Color(_))
        ));
    }
}
