//! Content fingerprints detect compression/size variants; never delete automatically.
use super::*;
#[derive(Debug, Clone)]
pub struct Signature {
    pub hash: u64,
    pub color: [u8; 3],
    pub aspect: f64,
}

/// 从有界图像计算感知指纹、颜色和宽高比特征。
pub fn signature(image: &image::DynamicImage) -> Signature {
    let scaled = image
        .resize_exact(32, 32, image::imageops::FilterType::Triangle)
        .to_rgba8();
    let mut colors = [0u64; 3];
    let mut luminance = [[0.0; 32]; 32];
    for (x, y, p) in scaled.enumerate_pixels() {
        let a = u64::from(p[3]);
        let mut rgb = [0.0; 3];
        for c in 0..3 {
            let n = (u64::from(p[c]) * a + 255 * (255 - a) + 127) / 255;
            colors[c] += n;
            rgb[c] = n as f64;
        }
        luminance[y as usize][x as usize] = rgb[0] * 0.299 + rgb[1] * 0.587 + rgb[2] * 0.114;
    }
    static COS: std::sync::OnceLock<[[f64; 32]; 8]> = std::sync::OnceLock::new();
    let cos = COS.get_or_init(|| {
        std::array::from_fn(|u| {
            std::array::from_fn(|x| {
                ((2 * x + 1) as f64 * u as f64 * std::f64::consts::PI / 64.0).cos()
            })
        })
    });
    let mut horizontal = [[0.0; 8]; 32];
    for y in 0..32 {
        for u in 0..8 {
            horizontal[y][u] = (0..32).map(|x| luminance[y][x] * cos[u][x]).sum();
        }
    }
    let mut coefficients = [0.0; 64];
    for v in 0..8 {
        for u in 0..8 {
            coefficients[v * 8 + u] = (0..32).map(|y| horizontal[y][u] * cos[v][y]).sum();
        }
    }
    let mut values = coefficients[1..].to_vec();
    values.sort_by(f64::total_cmp);
    let median = values[31].max(coefficients[0].abs() * 0.001);
    let mut hash = 0;
    for (i, value) in coefficients.iter().enumerate().skip(1) {
        if *value > median + 0.001 {
            hash |= 1u64 << i;
        }
    }
    Signature {
        hash,
        color: colors.map(|n| (n / 1024) as u8),
        aspect: image.width() as f64 / image.height().max(1) as f64,
    }
}
/// 将指纹写入指定素材版本，供后续相似性检索使用。
pub(crate) fn store(tx: &Connection, version: &str, s: &Signature) -> Result<()> {
    tx.execute("INSERT INTO image_fingerprints(version_id,hash,red,green,blue,aspect) VALUES(?1,?2,?3,?4,?5,?6) ON \
                CONFLICT(version_id) DO UPDATE SET \
                hash=excluded.hash,red=excluded.red,green=excluded.green,blue=excluded.blue,aspect=excluded.aspect,algorithm=1",
params![version,
s.hash as i64,
s.color[0],
s.color[1],
s.color[2],
s.aspect])?;

    Ok(())
}
pub use crate::retrieval::SimilarityPlan;
impl Library {
    /// 取得相似性搜索使用的独立读取租约，避免持有写入端连接。
    pub fn similarity_plan(&self) -> Result<SimilarityPlan> {
        Ok(SimilarityPlan::new(
            self.readers.checkout()?,
            self.query_scope(),
        ))
    }
    /// 为指定素材准备并执行相似性查询。
    pub fn similar_assets(&self, asset: AssetId) -> Result<Vec<SimilarAsset>> {
        self.similarity_plan()?.search(asset)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    /// 验证相似性识别缩放与重压缩图片，同时不合并素材或修改来源。
    fn detects_resized_and_recompressed_images_without_merging_or_modifying_sources() {
        let dir = tempfile::tempdir().unwrap();
        let mut image = image::RgbImage::new(512, 256);
        for (x, y, p) in image.enumerate_pixels_mut() {
            *p = image::Rgb([
                (x / 2) as u8,
                (y / 2) as u8,
                ((x / 32 + y / 16) % 2 * 160) as u8,
            ]);
        }
        let first = dir.path().join("original.png");
        image.save(&first).unwrap();
        let second = dir.path().join("compressed.jpg");
        image::DynamicImage::ImageRgb8(image)
            .resize_exact(256, 128, image::imageops::FilterType::Triangle)
            .save(&second)
            .unwrap();
        let unrelated = dir.path().join("other.png");
        image::RgbImage::from_pixel(512, 256, image::Rgb([0, 0, 255]))
            .save(&unrelated)
            .unwrap();
        let before = organize::hash_file(&first).unwrap();
        let mut lib = Library::create(dir.path(), "近似").unwrap();
        for file in [&first, &second, &unrelated] {
            lib.import(file, None).unwrap();
        }
        let assets = lib.page(&Query::default(), 0, 5).unwrap().assets;
        let reference = assets.iter().find(|a| a.name == "original.png").unwrap();
        let matches = lib.similar_assets(reference.id).unwrap();
        assert!(matches.iter().any(|a| a.name == "compressed.jpg"));
        assert!(!matches.iter().any(|a| a.name == "other.png"));
        assert_eq!(lib.info().unwrap().count, 3);
        assert_eq!(organize::hash_file(&first).unwrap(), before);
    }
}
