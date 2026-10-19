//! Content fingerprints detect compression/size variants; never delete automatically.
use super::*;
#[derive(Debug, Clone)]
pub struct Signature {
    pub hash: u64,
    pub color: [u8; 3],
    pub aspect: f64,
}

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
pub(crate) fn store(tx: &Connection, version: &str, s: &Signature) -> Result<()> {
    tx.execute("INSERT INTO image_fingerprints(version_id,hash,red,green,blue,aspect) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(version_id) DO UPDATE SET hash=excluded.hash,red=excluded.red,green=excluded.green,blue=excluded.blue,aspect=excluded.aspect,algorithm=1",params![version,s.hash as i64,s.color[0],s.color[1],s.color[2],s.aspect])?;
    Ok(())
}
pub struct SimilarityPlan {
    connection: read_pool::ReadLease,
}
impl Library {
    pub fn similarity_plan(&self) -> Result<SimilarityPlan> {
        Ok(SimilarityPlan {
            connection: self.readers.checkout()?,
        })
    }
    pub fn similar_assets(&self, asset: AssetId) -> Result<Vec<SimilarAsset>> {
        self.similarity_plan()?.search(asset)
    }
}
impl SimilarityPlan {
    pub fn search(&self, asset: AssetId) -> Result<Vec<SimilarAsset>> {
        let db = self.connection.connection()?;
        let reference:Option<(i64,u8,u8,u8,f64)>=db.query_row("SELECT f.hash,f.red,f.green,f.blue,f.aspect FROM image_fingerprints f JOIN assets a ON a.display_version_id=f.version_id WHERE a.id=?1 AND a.deleted_at IS NULL",[asset.to_string()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?;
        let Some((hash, r, g, b, aspect)) = reference else {
            return Err(Error::Invalid(
                "所选图片还没有近似索引，请先在后台任务中建立索引".into(),
            ));
        };
        let mut stmt=db.prepare("SELECT a.id,a.name,f.hash,f.red,f.green,f.blue FROM image_fingerprints f JOIN assets a ON a.display_version_id=f.version_id WHERE a.deleted_at IS NULL AND a.id<>?1 AND f.algorithm=1 AND f.aspect BETWEEN ?2 AND ?3")?;
        let mut rows = stmt.query(params![asset.to_string(), aspect * 0.95, aspect * 1.05])?;
        let mut matches = Vec::new();
        while let Some(row) = rows.next()? {
            let value: i64 = row.get(2)?;
            let distance = ((hash as u64) ^ (value as u64)).count_ones();
            let color: [u8; 3] = [row.get(3)?, row.get(4)?, row.get(5)?];
            if distance <= 8
                && color
                    .into_iter()
                    .zip([r, g, b])
                    .map(|(a, b)| u32::from(a.abs_diff(b)))
                    .sum::<u32>()
                    <= 100
            {
                matches.push(SimilarAsset {
                    id: AssetId(parse_id(&row.get::<_, String>(0)?)?),
                    name: row.get(1)?,
                    distance,
                });
                if matches.len() > 200 {
                    matches.sort_by_key(|a| a.distance);
                    matches.truncate(100);
                }
            }
        }
        matches.sort_by(|a, b| a.distance.cmp(&b.distance).then(a.name.cmp(&b.name)));
        matches.truncate(100);
        Ok(matches)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
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
