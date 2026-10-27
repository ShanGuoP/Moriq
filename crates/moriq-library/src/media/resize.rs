use super::*;
/// SIMD area filtering, preserving aspect, pixel type and premultiplied alpha.
/// Only the small output is allocated; source pixels are borrowed without a full-size conversion.
pub(super) fn fit(source: &image::DynamicImage, longest: u32) -> Result<image::DynamicImage> {
    fit_with(source, longest, Resampling::Area)
}
pub(super) fn fit_with(
    source: &image::DynamicImage,
    longest: u32,
    filter: Resampling,
) -> Result<image::DynamicImage> {
    let (width, height) = source.dimensions();
    let edge = width.max(height);
    if edge == 0 || longest == 0 {
        return Err(Error::Invalid("图片缩放尺寸无效".into()));
    }
    if edge <= longest {
        return Ok(source.clone());
    }
    let width = (u64::from(width) * u64::from(longest) / u64::from(edge)).max(1) as u32;
    let height = (u64::from(height) * u64::from(longest) / u64::from(edge)).max(1) as u32;
    let mut output = image::DynamicImage::new(width, height, source.color());
    use fast_image_resize::{FilterType, ResizeAlg};
    let algorithm = match filter {
        Resampling::Area => ResizeAlg::Convolution(FilterType::Box),
        Resampling::Lanczos3 => ResizeAlg::Convolution(FilterType::Lanczos3),
        Resampling::Bicubic => ResizeAlg::Convolution(FilterType::CatmullRom),
        Resampling::Nearest => ResizeAlg::Nearest,
    };
    let options = fast_image_resize::ResizeOptions::new().resize_alg(algorithm);
    fast_image_resize::Resizer::new()
        .resize(source, &mut output, Some(&options))
        .map_err(|e| Error::Invalid(format!("缩略图生成失败：{e}")))?;
    Ok(output)
}
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn bounded_resizing_preserves_aspect_small_images_and_transparency() {
        let source = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            4032,
            3024,
            image::Rgb([40, 80, 120]),
        ));
        let result = fit(&source, 256).unwrap();
        assert_eq!(result.dimensions(), (256, 192));
        assert_eq!(result.to_rgb8().get_pixel(100, 100).0, [40, 80, 120]);
        let small = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            12,
            8,
            image::Rgba([1, 2, 3, 40]),
        ));
        assert_eq!(fit(&small, 256).unwrap().to_rgba8(), small.to_rgba8());
        let mut alpha = image::RgbaImage::from_pixel(20, 10, image::Rgba([0, 255, 0, 0]));
        for x in 0..10 {
            for y in 0..10 {
                alpha.put_pixel(x, y, image::Rgba([255, 0, 0, 255]));
            }
        }
        let result = fit(&image::DynamicImage::ImageRgba8(alpha), 1)
            .unwrap()
            .to_rgba8();
        let pixel = result.get_pixel(0, 0);
        assert!(pixel[0] > 240 && pixel[1] < 10 && pixel[3] > 110 && pixel[3] < 140);
    }
}
