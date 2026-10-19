//! Immutable export plans are built on the writer, media work runs off the writer.
use super::*;
pub struct Item {
    source: PathBuf,
    hash: String,
    name: String,
    imported_at: u64,
    metadata: serde_json::Value,
}
impl Library {
    pub fn export_plan(&self, ids: &[AssetId]) -> Result<Vec<Item>> {
        if ids.is_empty() || ids.len() > 10000 {
            return Err(Error::Invalid("请选择 1～10000 张素材导出".into()));
        }
        ids.iter().map(|id| {
            let (version,hash,name): (String,String,String) = self.db.query_row("SELECT v.id,v.sha256,a.name FROM assets a JOIN asset_versions v ON v.id=a.display_version_id WHERE a.id=?1",[id.to_string()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
            let imported_at=self.db.query_row("SELECT imported_at FROM assets WHERE id=?1",[id.to_string()],|r|r.get(0))?;
            let notes:String=self.db.query_row("SELECT notes FROM assets WHERE id=?1",[id.to_string()],|r|r.get(0))?;
            let tags=self.db.prepare("SELECT t.name FROM tags t JOIN asset_tags at ON at.tag_id=t.id WHERE at.asset_id=?1 ORDER BY t.name")?.query_map([id.to_string()],|r|r.get::<_,String>(0))?.collect::<std::result::Result<Vec<_>,_>>()?;
            let exif:Option<String>=self.db.query_row("SELECT data FROM version_exif WHERE version_id=?1",[&version],|r|r.get(0)).optional()?;
            let exif=exif.map(|s|serde_json::from_str::<serde_json::Value>(&s)).transpose().map_err(|e|Error::Invalid(e.to_string()))?;
            let project:Option<String>=self.db.query_row("SELECT p.name FROM projects p JOIN project_assets pa ON pa.project_id=p.id WHERE pa.asset_id=?1 ORDER BY p.name LIMIT 1",[id.to_string()],|r|r.get(0)).optional()?;
            let metadata=serde_json::json!({"asset_id":id.to_string(),"version_id":version,"source_sha256":hash,"source_name":name,"notes":notes,"tags":tags,"exif":exif,"project":project});
            Ok(Item { source: self.version_path(&version,false)?, hash, name, imported_at, metadata })
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
        if crate::video::extension(&item.source).is_some() && !original {
            return Err(Error::Invalid(
                "视频当前支持原文件导出；请勿使用图片转换选项".into(),
            ));
        }
        let format = match options.format {
            ExportFormat::Original if original => None,
            ExportFormat::Original => Some(ImageFormat::Png),
            ExportFormat::Jpeg => Some(ImageFormat::Jpeg),
            ExportFormat::Png => Some(ImageFormat::Png),
            ExportFormat::Webp => Some(ImageFormat::WebP),
            ExportFormat::Tiff => Some(ImageFormat::Tiff),
            ExportFormat::Bmp => Some(ImageFormat::Bmp),
        };
        let extension = match format {
            Some(ImageFormat::Jpeg) => "jpg",
            Some(ImageFormat::WebP) => "webp",
            Some(ImageFormat::Tiff) => "tiff",
            Some(ImageFormat::Bmp) => "bmp",
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
        let folder = if !options.directory_template.trim().is_empty() {
            export_metadata::directory(
                &options.directory_template,
                stage.path(),
                extension,
                item.imported_at,
                &item.metadata,
            )?
        } else {
            match options.layout {
                ExportLayout::Flat => stage.path().to_path_buf(),
                ExportLayout::Format => stage.path().join(extension),
                ExportLayout::ImportDate => {
                    let date = chrono::DateTime::from_timestamp_millis(item.imported_at as i64)
                        .ok_or_else(|| Error::Invalid("导入日期无效".into()))?;
                    stage.path().join(date.format("%Y-%m-%d").to_string())
                }
            }
        };
        fs::create_dir_all(&folder)?;
        let mut sequence = 1usize;
        let mut destination = folder.join(format!("{stem}.{extension}"));
        while destination.exists() {
            sequence += 1;
            destination = folder.join(format!("{stem} ({sequence}).{extension}"));
        }
        if options.metadata_sidecar {
            let sidecar = destination.with_file_name(format!(
                "{}.metadata.json",
                destination
                    .file_name()
                    .ok_or_else(|| Error::Invalid("导出文件名无效".into()))?
                    .to_string_lossy()
            ));
            let mut output = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(sidecar)?;
            output.write_all(
                &serde_json::to_vec_pretty(&item.metadata)
                    .map_err(|e| Error::Invalid(e.to_string()))?,
            )?;
            output.sync_all()?;
        }
        if original {
            fs::rename(&frozen, &destination)?;
            continue;
        }
        let mut decoded = media::raster(File::open(&frozen)?, &item.source, 0)?
            .image
            .ok_or_else(|| Error::Invalid("导出解码无像素".into()))?;
        decoded = match options.rotate {
            90 => decoded.rotate90(),
            180 => decoded.rotate180(),
            270 => decoded.rotate270(),
            _ => decoded,
        };
        if options.max_edge > 0 {
            decoded = resize::fit_with(&decoded, options.max_edge, options.resampling)?;
        }
        let exif = if options.preserve_exif {
            export_metadata::normalized_exif(&frozen, decoded.width(), decoded.height())?
        } else {
            None
        };
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
            let mut encoder =
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, options.quality);
            export_metadata::attach(&mut encoder, exif.as_deref())?;
            encoder.encode_image(&rgb)?;
            output.sync_all()?;
        } else if matches!(
            format,
            Some(ImageFormat::Png | ImageFormat::WebP | ImageFormat::Tiff)
        ) {
            use image::ImageEncoder;
            let mut output = File::create(&destination)?;
            match format {
                Some(ImageFormat::WebP) => {
                    let pixels = decoded.to_rgba8();
                    let mut encoder = image::codecs::webp::WebPEncoder::new_lossless(&mut output);
                    export_metadata::attach(&mut encoder, exif.as_deref())?;
                    encoder.write_image(
                        &pixels,
                        pixels.width(),
                        pixels.height(),
                        image::ExtendedColorType::Rgba8,
                    )?;
                }
                Some(ImageFormat::Tiff) => {
                    let mut encoder = image::codecs::tiff::TiffEncoder::new(&mut output);
                    export_metadata::attach(&mut encoder, None)?;
                    encoder.write_image(
                        decoded.as_bytes(),
                        decoded.width(),
                        decoded.height(),
                        decoded.color().into(),
                    )?;
                }
                _ => {
                    let mut encoder = image::codecs::png::PngEncoder::new(&mut output);
                    export_metadata::attach(&mut encoder, exif.as_deref())?;
                    encoder.write_image(
                        decoded.as_bytes(),
                        decoded.width(),
                        decoded.height(),
                        decoded.color().into(),
                    )?;
                }
            }
            output.sync_all()?;
        } else {
            decoded.save_with_format(&destination, format.unwrap_or(ImageFormat::Png))?;
            OpenOptions::new()
                .write(true)
                .open(&destination)?
                .sync_all()?;
        }
        if !matches!(
            format,
            Some(ImageFormat::Jpeg | ImageFormat::Png | ImageFormat::WebP)
        ) && let Some(exif) = exif
        {
            let sidecar = destination.with_file_name(format!(
                "{}.exif",
                destination
                    .file_name()
                    .ok_or_else(|| Error::Invalid("导出文件名无效".into()))?
                    .to_string_lossy()
            ));
            let mut file = File::create(sidecar)?;
            file.write_all(&exif)?;
            file.sync_all()?;
        }
        fs::remove_file(frozen)?;
    }
    let destination = parent.join(format!("MORIQ-导出-{}", Uuid::new_v4()));
    fs::rename(stage.path(), &destination)?;
    Ok(destination)
}
