//! Immutable export plans are built on the writer, media work runs off the writer.
use super::*;
pub struct Item {
    source: PathBuf,
    hash: String,
    name: String,
}
impl Library {
    pub fn export_plan(&self, ids: &[AssetId]) -> Result<Vec<Item>> {
        if ids.is_empty() || ids.len() > 10000 {
            return Err(Error::Invalid("请选择 1～10000 张素材导出".into()));
        }
        ids.iter().map(|id| {
            let (version,hash,name): (String,String,String) = self.db.query_row("SELECT v.id,v.sha256,a.name FROM assets a JOIN asset_versions v ON v.id=a.display_version_id WHERE a.id=?1",[id.to_string()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
            Ok(Item { source: self.version_path(&version,false)?, hash, name })
        }).collect()
    }
}
pub fn run(items: Vec<Item>, parent: &Path, options: ExportOptions) -> Result<PathBuf> {
    if options.max_edge > 30000
        || options.quality == 0
        || options.quality > 100
        || ![0, 90, 180, 270].contains(&options.rotate)
    {
        return Err(Error::Invalid("导出尺寸、质量或旋转角度无效".into()));
    }
    let parent = parent.canonicalize()?;
    // Reject writing inside a library even when the plan comes from another source library.
    if parent.ancestors().any(|p| {
        p.extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("moriq"))
    }) {
        return Err(Error::Invalid("请选择资源库之外的导出位置".into()));
    }
    let stage = tempfile::Builder::new()
        .prefix(".moriq-export-")
        .tempdir_in(&parent)?;
    for item in items {
        let frozen = stage.path().join(".source-part");
        organize::copy_owned(&item.source, &frozen)?;
        if organize::hash_file(&frozen)? != item.hash {
            return Err(Error::Invalid(
                "导出期间源文件发生变化，已停止发布导出目录".into(),
            ));
        }
        let original = options.format == ExportFormat::Original
            && options.max_edge == 0
            && options.rotate == 0;
        let format = match options.format {
            ExportFormat::Original if original => None,
            ExportFormat::Original => Some(ImageFormat::Png),
            ExportFormat::Jpeg => Some(ImageFormat::Jpeg),
            ExportFormat::Png => Some(ImageFormat::Png),
            ExportFormat::Webp => Some(ImageFormat::WebP),
        };
        let extension = match format {
            Some(ImageFormat::Jpeg) => "jpg",
            Some(ImageFormat::WebP) => "webp",
            Some(_) => "png",
            None => item
                .source
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("bin"),
        };
        let stem = Path::new(&item.name)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("图片");
        let stem: String = stem
            .chars()
            .map(|c| {
                if c.is_control() || "/\\:*?\"<>|".contains(c) {
                    '_'
                } else {
                    c
                }
            })
            .collect();
        let stem = stem.trim_end_matches([' ', '.']);
        let stem = if stem.is_empty() { "图片" } else { stem };
        let mut sequence = 1usize;
        let mut destination = stage.path().join(format!("{stem}.{extension}"));
        while destination.exists() {
            sequence += 1;
            destination = stage
                .path()
                .join(format!("{stem} ({sequence}).{extension}"));
        }
        if original {
            fs::rename(&frozen, &destination)?;
            continue;
        }
        let mut decoded = media::raster(File::open(&frozen)?, &item.source, options.max_edge)?
            .image
            .ok_or_else(|| Error::Invalid("导出解码无像素".into()))?;
        decoded = match options.rotate {
            90 => decoded.rotate90(),
            180 => decoded.rotate180(),
            270 => decoded.rotate270(),
            _ => decoded,
        };
        if options.max_edge > 0 {
            decoded = resize::fit(&decoded, options.max_edge)?;
        }
        if options.format == ExportFormat::Jpeg {
            let mut rgb = decoded.to_rgb8();
            if decoded.color().has_alpha() {
                let rgba = decoded.to_rgba8();
                for (pixel, source) in rgb.pixels_mut().zip(rgba.pixels()) {
                    let alpha = u16::from(source[3]);
                    for c in 0..3 {
                        pixel[c] = ((u16::from(source[c]) * alpha + 255 * (255 - alpha) + 127)
                            / 255) as u8;
                    }
                }
            }
            let mut output = File::create(&destination)?;
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, options.quality)
                .encode_image(&rgb)?;
            output.sync_all()?;
        } else {
            decoded.save_with_format(&destination, format.unwrap_or(ImageFormat::Png))?;
            OpenOptions::new()
                .write(true)
                .open(&destination)?
                .sync_all()?;
        }
        fs::remove_file(frozen)?;
    }
    let destination = parent.join(format!("MORIQ-导出-{}", Uuid::new_v4()));
    fs::rename(stage.path(), &destination)?;
    Ok(destination)
}
