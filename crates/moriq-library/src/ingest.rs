//! Owned staging tickets cross worker threads; SQLite and final names stay on the writer.
use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Clone)]
pub struct Ticket {
    root: PathBuf,
    source: PathBuf,
    relative: PathBuf,
    project: Option<ProjectId>,
    referenced: bool,
    operation: String,
    asset: AssetId,
    version: AssetVersionId,
    // Keep the library's process lock alive while filesystem workers are in flight.
    _lease: Arc<File>,
}
pub struct Prepared {
    pub(super) hash: String,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) extension: &'static str,
    pub(super) dominant_color: &'static str,
    pub(super) metrics: ImportMetrics,
    pub(super) reference_verified: bool,
    pub(super) video: Option<crate::video::Metadata>,
    pub(super) signature: Option<crate::similarity::Signature>,
    pub(super) metadata: serde_json::Value,
}
impl Ticket {
    fn staged(&self) -> Result<PathBuf> {
        safe_path(&self.root, &format!("staging/{}.part", self.operation))
    }
    pub fn copy(&self, cancel: &AtomicBool) -> Result<()> {
        let staged = self.staged()?;
        let mut input = File::open(&self.source)?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staged)?;
        let result = (|| {
            let mut total = 0u64;
            let mut buffer = [0u8; 64 * 1024];
            loop {
                cancelled(cancel)?;
                let count = input.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                total += count as u64;
                if total > native::file_limit(&self.source) {
                    return Err(Error::Invalid("读取期间媒体文件超出大小预算".into()));
                }
                output.write_all(&buffer[..count])?;
            }
            output.sync_all()?;
            Ok(())
        })();
        drop(output);
        if result.is_err() {
            // An incomplete JPEG can still decode. Never offer a partial copy as recovery media.
            if let Err(error) = fs::remove_file(staged) {
                tracing::warn!(%error,"partial staging cleanup failed");
            }
        }
        result
    }
    /// Admission estimate includes two decoded buffers, compressed input and workspace.
    /// This bounds admitted image work, not the entire process's RSS.
    pub fn memory_mib(&self) -> Result<u32> {
        let staged = self.staged()?;
        let mut reader = ImageReader::open(&staged)?.with_guessed_format()?;
        reader.limits(decode_limits());
        if native::raw_extension(&self.source).is_some()
            && !matches!(
                reader.format(),
                Some(
                    ImageFormat::Jpeg
                        | ImageFormat::Png
                        | ImageFormat::WebP
                        | ImageFormat::Gif
                        | ImageFormat::Bmp
                )
            )
        {
            return native::raw_memory_mib(&staged);
        }
        if crate::video::extension(&self.source).is_some() {
            return Ok(1024);
        }
        let decoder = match reader.into_decoder() {
            Ok(decoder) => decoder,
            Err(_) => return Ok(1024),
        };
        let (width, height) = decoder.dimensions();
        validate_dimensions(width, height)?;
        if u64::from(width) * u64::from(height) * u64::from(decoder.color_type().bytes_per_pixel())
            > 384 * 1024 * 1024
        {
            return Ok(1024);
        }
        let decoded = u64::from(width)
            * u64::from(height)
            * u64::from(decoder.color_type().bytes_per_pixel());
        let bytes = decoded.saturating_mul(2) + fs::metadata(staged)?.len() * 2 + 32 * 1024 * 1024;
        Ok(bytes.div_ceil(1024 * 1024) as u32)
    }
    pub fn prepare(&self, cancel: &AtomicBool) -> Result<Prepared> {
        cancelled(cancel)?;
        let mut prepared = prepare(&self.staged()?, &self.operation, &self.source)?;
        if self.referenced {
            if organize::hash_file(&self.source)? != prepared.hash {
                return Err(Error::Invalid("引用源在导入期间发生变化，请重试".into()));
            }
            prepared.reference_verified = true;
        }
        cancelled(cancel)?;
        Ok(prepared)
    }
}
fn cancelled(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        Err(Error::Invalid("导入已停止".into()))
    } else {
        Ok(())
    }
}
fn validate_dimensions(width: u32, height: u32) -> Result<()> {
    if width == 0 || height == 0 {
        return Err(Error::Invalid("图片尺寸无效".into()));
    }
    Ok(())
}
pub(super) fn prepare(staged: &Path, operation: &str, source: &Path) -> Result<Prepared> {
    let clock = std::time::Instant::now();
    let hash = organize::hash_file(staged)?;
    let hash_ms = clock.elapsed().as_secs_f64() * 1000.0;
    let clock = std::time::Instant::now();
    let reader = ImageReader::open(staged)?.with_guessed_format()?;
    let format = reader.format();
    let extension = match format {
        Some(ImageFormat::Jpeg) => "jpg",
        Some(ImageFormat::Png) => "png",
        Some(ImageFormat::WebP) => "webp",
        Some(ImageFormat::Gif) => "gif",
        Some(ImageFormat::Bmp) => "bmp",
        _ if source
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("svg")) =>
        {
            "svg"
        }
        _ => crate::video::extension(source)
            .or_else(|| native::raw_extension(source))
            .or(match format {
                Some(ImageFormat::Tiff) => Some("tiff"),
                Some(ImageFormat::Avif) => Some("avif"),
                _ => None,
            })
            .or_else(|| {
                let ext = source.extension()?.to_str()?;
                ["heic", "heif", "tif", "tiff", "avif"]
                    .into_iter()
                    .find(|e| ext.eq_ignore_ascii_case(e))
            })
            .ok_or_else(|| Error::Invalid("不支持的媒体格式".into()))?,
    };
    let (decoded, video) = if crate::video::extension(source).is_some() {
        let (decoded, metadata) = crate::video::decode(staged, 256)?;
        (decoded, Some(metadata))
    } else {
        (
            crate::media::raster(File::open(staged)?, source, 256)?,
            None,
        )
    };
    let (width, height) = (decoded.width, decoded.height);
    let thumbnail = decoded
        .image
        .ok_or_else(|| Error::Invalid("缩略图解码无像素".into()))?;
    let decode_ms = clock.elapsed().as_secs_f64() * 1000.0;
    let clock = std::time::Instant::now();
    let dominant_color = organize::dominant_color(&thumbnail);
    let signature = video
        .is_none()
        .then(|| crate::similarity::signature(&thumbnail));
    let thumb_stage = staged.with_file_name(format!("{operation}.thumb"));
    thumbnail.save_with_format(&thumb_stage, ImageFormat::Png)?;
    OpenOptions::new()
        .write(true)
        .open(thumb_stage)?
        .sync_all()?;
    let metadata = if video.is_none() {
        crate::metadata::read(File::open(staged)?)?
    } else {
        serde_json::json!({})
    };
    Ok(Prepared {
        hash,
        width,
        height,
        extension,
        dominant_color,
        reference_verified: false,
        video,
        signature,
        metadata,
        metrics: ImportMetrics {
            hash_ms,
            decode_ms,
            thumbnail_ms: clock.elapsed().as_secs_f64() * 1000.0,
            ..Default::default()
        },
    })
}
impl Library {
    pub fn begin_import(
        &mut self,
        file: &ImportFile,
        project: Option<ProjectId>,
        referenced: bool,
    ) -> Result<Ticket> {
        let length = fs::metadata(&file.path)?.len();
        if length == 0 || length > native::file_limit(&file.path) {
            return Err(Error::Invalid("媒体超出文件大小预算或为空文件".into()));
        }
        let source = file.path.canonicalize()?;
        let project = self.resolve_import_project(file, project)?;
        let operation = Uuid::new_v4().to_string();
        let (platform, native) = organize::encode_path(&source)?;
        let relative = file
            .relative
            .to_str()
            .ok_or_else(|| Error::Invalid("源目录无法无损表示".into()))?;
        let tx = self.db.transaction()?;
        tx.execute(
            "INSERT INTO operation_journal VALUES(?1,?2,'prepared')",
            params![operation, format!("staging/{operation}.part")],
        )?;
        tx.execute("INSERT INTO import_context(operation_id,platform,source,project_id,referenced,relative_path) VALUES(?1,?2,?3,?4,?5,?6)", params![operation, platform, native, project.map(|p|p.to_string()), referenced, relative])?;
        tx.commit()?;
        Ok(Ticket {
            root: self.root.clone(),
            source,
            relative: file.relative.clone(),
            project,
            referenced,
            operation,
            asset: AssetId::default(),
            version: AssetVersionId::default(),
            _lease: self._lock.clone(),
        })
    }
    pub fn finish_import(&mut self, ticket: Ticket, prepared: Prepared) -> Result<bool> {
        if ticket.root != self.root {
            return Err(Error::Invalid("导入所属资源库已切换".into()));
        }
        self.snapshot = None;
        let staged = ticket.staged()?;
        let result = self.install_prepared(
            &staged,
            ticket.asset,
            ticket.version,
            &ticket.operation,
            &ticket.source,
            ticket.project,
            false,
            ticket.referenced,
            Some(&ticket.relative),
            prepared,
        );
        if result.is_ok() {
            self.db.execute(
                "UPDATE operation_journal SET phase='committed' WHERE id=?1",
                [&ticket.operation],
            )?;
            for path in [
                staged,
                self.root
                    .join(format!("staging/{}.thumb", ticket.operation)),
            ] {
                if let Err(error) = fs::remove_file(path) {
                    tracing::warn!(%error,"import staging cleanup deferred");
                }
            }
        } else {
            self.interrupt_import(&ticket)?;
        }
        result
    }
    pub fn interrupt_import(&self, ticket: &Ticket) -> Result<()> {
        if ticket.root != self.root {
            return Err(Error::Invalid("导入所属资源库已切换".into()));
        }
        self.db.execute(
            "UPDATE operation_journal SET phase='interrupted' WHERE id=?1",
            [&ticket.operation],
        )?;
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn jpeg_admission_uses_dimensions_instead_of_fallback_budget() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("photo.jpg");
        image::RgbImage::from_pixel(4016, 6016, image::Rgb([10, 80, 150]))
            .save(&source)
            .unwrap();
        let mut library = Library::create(dir.path(), "test").unwrap();
        let ticket = library
            .begin_import(
                &ImportFile {
                    path: source,
                    relative: "photo.jpg".into(),
                    project_name: None,
                },
                None,
                false,
            )
            .unwrap();
        ticket.copy(&AtomicBool::new(false)).unwrap();
        let weight = ticket.memory_mib().unwrap();
        assert!(weight < 256, "24 MP JPEG was assigned {weight} MiB");
    }
    #[test]
    fn ticket_keeps_library_locked_and_cancelled_copy_is_not_recoverable_media() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("原片.png");
        image::RgbImage::from_pixel(24, 16, image::Rgb([10, 80, 150]))
            .save(&source)
            .unwrap();
        let mut library = Library::create(dir.path(), "库").unwrap();
        let root = library.root().to_path_buf();
        let ticket = library
            .begin_import(
                &ImportFile {
                    path: source.clone(),
                    relative: "原片.png".into(),
                    project_name: None,
                },
                None,
                false,
            )
            .unwrap();
        let original = fs::read(&source).unwrap();
        assert!(ticket.copy(&AtomicBool::new(true)).is_err());
        assert!(!ticket.staged().unwrap().exists());
        assert_eq!(fs::read(source).unwrap(), original);
        drop(library);
        assert!(matches!(Library::open(&root), Err(Error::Locked)));
        drop(ticket);
        assert!(Library::open(&root).is_ok());
    }
    #[test]
    fn reference_worker_rejects_a_source_changed_after_staging() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("原片.png");
        image::RgbImage::from_pixel(24, 16, image::Rgb([10, 80, 150]))
            .save(&source)
            .unwrap();
        let mut library = Library::create(dir.path(), "库").unwrap();
        let ticket = library
            .begin_import(
                &ImportFile {
                    path: source.clone(),
                    relative: "原片.png".into(),
                    project_name: None,
                },
                None,
                true,
            )
            .unwrap();
        let cancel = AtomicBool::new(false);
        ticket.copy(&cancel).unwrap();
        fs::write(source, b"changed source").unwrap();
        assert!(ticket.prepare(&cancel).is_err());
        assert_eq!(library.info().unwrap().count, 0);
    }
}
