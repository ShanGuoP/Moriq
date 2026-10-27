//! Frozen file handles and library leases cross to bounded media workers.
use super::*;
use std::{
    io::{BufReader, Seek, SeekFrom},
    sync::Arc,
};

pub struct Source {
    root: PathBuf,
    version: AssetVersionId,
    dimensions: (u32, u32),
    file: File,
    path: PathBuf,
    identity: file_id::FileId,
    expected: Option<String>,
    registered_hash: String,
    extension: String,
    _lease: Arc<File>,
}
impl Source {
    /// 从读取连接冻结指定素材版本的文件来源、身份及必要元数据。
    pub(crate) fn from_read_connection(
        db: &Connection,
        root: &Path,
        lease: Arc<File>,
        asset: AssetId,
        version: AssetVersionId,
    ) -> Result<Self> {
        let (hash, extension, width, height, relative): (String,String,u32,u32,String) = db.query_row(
            "SELECT sha256,format,width,height,path FROM asset_versions WHERE asset_id=?1 AND id=?2",
            params![asset.to_string(),version.to_string()], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))?;
        let reference: Option<(String, Vec<u8>)> = db
            .query_row(
                "SELECT platform,native_path FROM file_locations WHERE version_id=?1",
                [version.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let path = if let Some((platform, bytes)) = &reference {
            let path = organize::decode_path(platform, bytes)?;
            if !path.is_absolute() {
                return Err(Error::Invalid("引用路径无效".into()));
            }
            organize::reject_network_reference(&path)?;
            path
        } else {
            safe_path(root, &relative)?
        };
        let file = File::open(&path)?;
        let identity = file_id::get_file_id(&path)?;
        Ok(Self {
            root: root.to_owned(),
            version,
            dimensions: (width, height),
            file,
            path,
            identity,
            expected: reference.is_some().then_some(hash.clone()),
            registered_hash: hash,
            extension,
            _lease: lease,
        })
    }
}
enum Thumbnail {
    Cached(File),
    Rebuild(Source),
}
pub struct Thumbnails {
    files: Vec<std::result::Result<Thumbnail, String>>,
    _lease: Arc<File>,
}
#[derive(Clone)]
pub struct Preview {
    pub image: Pixels,
    pub background: Pixels,
}
pub struct Details {
    metadata: AssetDetails,
    source: std::result::Result<Source, String>,
}
impl Library {
    /// Retrieval inputs use the same explicit scope as their candidates before decoding.
    pub fn retrieval_source(&self, asset: AssetId) -> Result<Source> {
        moriq_retrieval::Catalog::new(&self.db, self.query_scope()).require_asset(asset)?;
        self.media_source(asset)
    }
    /// 为素材当前展示版本建立媒体读取来源。
    pub fn media_source(&self, asset: AssetId) -> Result<Source> {
        let version: String = self.db.query_row(
            "SELECT display_version_id FROM assets WHERE id=?1",
            [asset.to_string()],
            |r| r.get(0),
        )?;
        self.version_source(asset, AssetVersionId(parse_id(&version)?))
    }
    /// 校验版本属于指定素材后建立来源，不修改当前展示版本。
    pub fn version_source(&self, asset: AssetId, version: AssetVersionId) -> Result<Source> {
        let (version, hash, extension,width,height): (String, String, String,u32,u32) = self.db.query_row(
            "SELECT id,sha256,format,width,height FROM asset_versions WHERE asset_id=?1 AND id=?2",
            params![asset.to_string(), version.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?,r.get(3)?,r.get(4)?)),
        )?;
        let referenced = self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM file_locations WHERE version_id=?1)",
            [&version],
            |r| r.get::<_, bool>(0),
        )?;
        let path = self.version_path(&version, false)?;
        let file = File::open(&path)?;
        let identity = file_id::get_file_id(&path)?;
        Ok(Source {
            root: self.root.clone(),
            version: AssetVersionId(parse_id(&version)?),
            dimensions: (width, height),
            file,
            path,
            identity,
            expected: referenced.then_some(hash.clone()),
            registered_hash: hash,
            extension,
            _lease: self._lock.clone(),
        })
    }
    /// 冻结有限数量版本的缩略图来源，供媒体线程批量读取。
    pub fn thumbnail_plan(&self, versions: &[AssetVersionId]) -> Result<Thumbnails> {
        if versions.len() > 128 {
            return Err(Error::Invalid("一次最多读取 128 张缩略图".into()));
        }
        let files = versions
            .iter()
            .map(|id| {
                let path = safe_path(&self.root, &format!("thumbnails/256/{id}.png"))
                    .map_err(|e| e.to_string())?;
                match File::open(path) {
                    Ok(file) => Ok(Thumbnail::Cached(file)),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        let asset: String = self
                            .db
                            .query_row(
                                "SELECT asset_id FROM asset_versions WHERE id=?1",
                                [id.to_string()],
                                |r| r.get(0),
                            )
                            .map_err(|e| e.to_string())?;
                        self.version_source(
                            AssetId(parse_id(&asset).map_err(|e| e.to_string())?),
                            *id,
                        )
                        .map(Thumbnail::Rebuild)
                        .map_err(|e| e.to_string())
                    }
                    Err(e) => Err(e.to_string()),
                }
            })
            .collect();
        Ok(Thumbnails {
            files,
            _lease: self._lock.clone(),
        })
    }
    /// 冻结详情所需的数据库字段与媒体来源，避免界面直接访问文件。
    pub fn details_plan(&self, id: AssetId) -> Result<Details> {
        let mut metadata = self.details_metadata(id)?;
        metadata.faces = self.faces(id)?;
        metadata.category = self
            .db
            .query_row(
                "SELECT category FROM asset_classification WHERE asset_id=?1",
                [id.to_string()],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or_default();
        let version: String = self.db.query_row(
            "SELECT display_version_id FROM assets WHERE id=?1",
            [id.to_string()],
            |r| r.get(0),
        )?;
        self.content_details(&version, &mut metadata)?;
        let manual_category: bool = self.db.query_row(
            "SELECT coalesce((SELECT manual FROM asset_classification WHERE asset_id=?1),0)",
            [id.to_string()],
            |r| r.get(0),
        )?;
        if !manual_category && !metadata.medium.is_empty() {
            let id = metadata.themes.first().unwrap_or(&metadata.medium);
            metadata.category = self
                .db
                .query_row("SELECT label FROM workspace_vocabulary WHERE id=?1 AND workspace=(SELECT workspace FROM ai_effective_content WHERE version_id=?2)", params![id,version], |r| {
                    r.get(0)
                })
                .optional()?
                .unwrap_or_default();
        }
        // Historical OCR stays readable independently of Qwen's current description.
        metadata.ocr = self.db.query_row(
            "SELECT coalesce(json_extract(ar.data,'$.ocr'),'') FROM ai_results ar JOIN assets a ON a.display_version_id=ar.version_id WHERE a.id=?1 AND ar.model_id='florence' ORDER BY ar.created_at DESC LIMIT 1",
            [id.to_string()], |row| row.get(0),
        ).optional()?.unwrap_or_default();
        let video:Option<String>=self.db.query_row("SELECT m.json FROM video_metadata m JOIN assets a ON a.display_version_id=m.version_id WHERE a.id=?1",
[id.to_string()],
|r|r.get(0)).optional()?;

        if let Some(json) = video {
            let video: crate::video::Metadata =
                serde_json::from_str(&json).map_err(|e| Error::Invalid(e.to_string()))?;
            metadata.exif = vec![
                ("媒体类型".into(), "视频".into()),
                (
                    "时长".into(),
                    format!("{:.2} 秒", video.duration_ms as f64 / 1000.0),
                ),
                ("编码".into(), video.codec),
                ("帧率".into(), video.frame_rate),
                ("像素格式".into(), video.pixel_format),
                (
                    "音轨".into(),
                    if video.has_audio { "有" } else { "无" }.into(),
                ),
                ("色彩空间".into(), video.color_space),
                ("传递函数".into(), video.color_transfer),
            ];
        }
        Ok(Details {
            metadata,
            source: self.media_source(id).map_err(|e| e.to_string()),
        })
    }
}
impl Source {
    /// 在取消作用域中建立原图分块会话，及时释放未完成读取。
    pub fn regions_cancellable(
        self,
        cancelled: Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<crate::region::Session> {
        let _scope = crate::cancel::Scope::enter(cancelled);
        self.regions()
    }
    /// 核对已冻结来源的文件内容与登记校验和，拒绝被替换的原片。
    pub(crate) fn verify_integrity(mut self) -> Result<()> {
        let mut hash = Sha256::new();
        let mut bytes = 0u64;
        let mut buffer = [0u8; 65536];
        loop {
            crate::cancel::check()?;
            let read = self.file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            bytes += read as u64;
            if bytes > 64 * 1024 * 1024 * 1024 {
                return Err(Error::Invalid("源文件超出检查预算".into()));
            }
            hash.update(&buffer[..read]);
        }
        if format!("{:x}", hash.finalize()) != self.registered_hash {
            return Err(Error::Invalid("原文件内容与登记哈希不一致".into()));
        }
        Ok(())
    }
    /// 建立持有来源快照和资源库租约的分块解码会话。
    pub fn regions(self) -> Result<crate::region::Session> {
        crate::cancel::check()?;
        let root = self.root.clone();
        let version = self.version;
        let dimensions = self.dimensions;
        let extension = self.extension.clone();
        let cacheable = self.cacheable();
        let lease = self._lease.clone();
        if cacheable
            && let Ok(path) = crate::video::linked_snapshot(&self.path, &self.root, self.identity)
        {
            return crate::region::Session::open(
                root, version, dimensions, extension, cacheable, path, lease,
            );
        }
        let (mut file, frozen) = self.frozen_file()?;
        let path = if let Some(file) = frozen {
            file.into_temp_path()
        } else {
            let mut temporary = tempfile::Builder::new()
                .suffix(&format!(".{extension}"))
                .tempfile()?;
            crate::cancel::copy(&mut file, temporary.as_file_mut())?;
            temporary.as_file().sync_all()?;
            temporary.into_temp_path()
        };
        crate::region::Session::open(root, version, dimensions, extension, cacheable, path, lease)
    }
    /// 返回此媒体来源绑定的素材版本身份。
    pub fn version(&self) -> AssetVersionId {
        self.version
    }
    /// 返回该来源是否允许使用受管预览缓存。
    pub fn cacheable(&self) -> bool {
        self.expected.is_none()
    }
    /// 从冻结文件读取规范化媒体元数据。
    pub fn metadata_record(self) -> Result<serde_json::Value> {
        let (file, _frozen) = self.frozen_file()?;
        crate::metadata::read(file)
    }
    /// 为视频来源创建受保护的播放快照并启动播放会话。
    pub fn playback(self) -> Result<Arc<crate::video::Playback>> {
        let requested = std::time::Instant::now();
        if crate::video::extension(Path::new(&format!("video.{}", self.extension))).is_none() {
            return Err(Error::Invalid("所选素材不是视频".into()));
        }
        let lease = self._lease.clone();
        if self.cacheable()
            && let Ok(snapshot) =
                crate::video::linked_snapshot(&self.path, &self.root, self.identity)
        {
            return crate::video::Playback::start(snapshot, lease, requested);
        }
        let (file, frozen) = self.frozen_file()?;
        let snapshot = match frozen {
            Some(frozen) => frozen.into_temp_path(),
            None => crate::video::copied_snapshot(file)?,
        };
        crate::video::Playback::start(snapshot, lease, requested)
    }
    /// 取得与冻结身份一致的文件；必要时生成临时副本以隔离后续路径变化。
    fn frozen_file(mut self) -> Result<(File, Option<tempfile::NamedTempFile>)> {
        if let Some(expected) = self.expected.take() {
            if fs2::available_space(std::env::temp_dir())
                .is_ok_and(|available| self.file.metadata().is_ok_and(|m| m.len() > available))
            {
                return Err(Error::Invalid(
                    "临时磁盘空间不足以冻结外部引用，请释放空间后重试".into(),
                ));
            }
            let mut frozen = tempfile::NamedTempFile::new()?;
            let limit = crate::native::file_limit(Path::new(&format!("image.{}", self.extension)));
            let length = crate::cancel::copy(&mut self.file.take(limit + 1), frozen.as_file_mut())?;
            if length > limit || organize::hash_file(frozen.path())? != expected {
                return Err(Error::Invalid(
                    "外部引用原片已变化，保留登记信息，请重新定位原片".into(),
                ));
            }
            frozen.as_file_mut().seek(SeekFrom::Start(0))?;
            Ok((frozen.reopen()?, Some(frozen)))
        } else {
            Ok((self.file, None))
        }
    }
    /// 使用默认预览边长解码当前冻结来源。
    pub fn preview(self) -> Result<Preview> {
        self.preview_edge(1600)
    }
    /// Read only the bounded, managed 1600px derivative. Never fall back to source decoding.
    ///
    /// 仅读取已存在的有效预览缓存，缺失或损坏时返回空值。
    pub fn cached_preview(&self) -> Option<Preview> {
        if !self.cacheable() {
            return None;
        }
        let path = safe_path(
            &self.root,
            &format!("previews/{}-srgb1-1600.png", self.version),
        )
        .ok()?;
        let file = File::open(path).ok()?;
        if file.metadata().ok()?.len() > 32 * 1024 * 1024 {
            return None;
        }
        let mut reader = ImageReader::new(BufReader::new(file))
            .with_guessed_format()
            .ok()?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(1600);
        limits.max_image_height = Some(1600);
        limits.max_alloc = Some(32 * 1024 * 1024);
        reader.limits(limits);
        preview_pixels(reader.decode().ok()?).ok()
    }
    /// 按目标边长读取缓存或有界解码来源，并按来源策略写回缓存。
    pub fn preview_edge(self, edge: u32) -> Result<Preview> {
        if edge == 1600
            && let Some(preview) = self.cached_preview()
        {
            return Ok(preview);
        }
        // Hold the lease until all decoding and derivative generation is finished.
        let lease = self._lease.clone();
        let extension = self.extension.clone();
        let cache = if self.cacheable() && edge == 1600 {
            Some(safe_path(
                &self.root,
                &format!("previews/{}-srgb1-{edge}.png", self.version),
            )?)
        } else {
            None
        };
        let (file, _frozen) = self.frozen_file()?;
        let preview = {
            let image = raster(file, &PathBuf::from(format!("image.{extension}")), edge)?
                .image
                .ok_or_else(|| Error::Invalid("预览解码无像素".into()))?;
            if let Some(path) = cache {
                let result = (|| -> Result<()> {
                    let mut temporary = tempfile::NamedTempFile::new_in(
                        path.parent()
                            .ok_or_else(|| Error::Invalid("缓存目录无效".into()))?,
                    )?;
                    image.write_with_encoder(image::codecs::png::PngEncoder::new_with_quality(
                        &mut temporary,
                        image::codecs::png::CompressionType::Fast,
                        image::codecs::png::FilterType::Adaptive,
                    ))?;
                    temporary.as_file().sync_all()?;
                    temporary.persist(path).map_err(|e| Error::Io(e.error))?;
                    Ok(())
                })();
                if let Err(error) = result {
                    tracing::warn!(%error,"disk preview cache skipped");
                }
            }
            image
        };
        let result = preview_pixels(preview)?;
        drop(lease);
        Ok(result)
    }
}
/// 将解码图像转换为统一预览像素及实际尺寸。
fn preview_pixels(preview: image::DynamicImage) -> Result<Preview> {
    let small = resize::fit(&preview, 128)?.to_rgba8();
    let background = image::DynamicImage::ImageRgba8(image::imageops::blur(&small, 8.0));
    Ok(Preview {
        image: pixels(preview),
        background: pixels(background),
    })
}
impl Source {
    /// 从冻结来源重建缺失缩略图，校验来源后再发布缓存。
    fn rebuild_thumbnail(self) -> Result<Pixels> {
        let destination = safe_path(&self.root, &format!("thumbnails/256/{}.png", self.version))?;
        let path = self.path.clone();
        let lease = self._lease.clone();
        let (file, _frozen) = self.frozen_file()?;
        let image = raster(file, &path, 256)?
            .image
            .ok_or_else(|| Error::Invalid("缩略图解码无像素".into()))?;
        let result = (|| -> Result<()> {
            let mut temp = tempfile::NamedTempFile::new_in(
                destination
                    .parent()
                    .ok_or_else(|| Error::Invalid("缩略图目录无效".into()))?,
            )?;
            image.write_to(&mut temp, ImageFormat::Png)?;
            temp.as_file().sync_all()?;
            if let Err(error) = temp.persist_noclobber(&destination)
                && error.error.kind() != std::io::ErrorKind::AlreadyExists
            {
                return Err(error.error.into());
            }
            Ok(())
        })();
        if let Err(error) = result {
            tracing::warn!(%error, "rebuilt thumbnail cache write deferred");
        }
        drop(lease);
        Ok(pixels(image))
    }
}
impl Thumbnails {
    /// 计算批量缩略图读取所需的内存预算。
    pub fn memory_mib(&self) -> u32 {
        if self
            .files
            .iter()
            .any(|file| matches!(file, Ok(Thumbnail::Rebuild(_))))
        {
            1024
        } else {
            128
        }
    }

    /// 批量读取缩略图，保留与输入对应的位置并统计读取失败项。
    pub fn read(self) -> (Vec<Option<Pixels>>, usize) {
        let mut warnings = 0;
        let images = self
            .files
            .into_iter()
            .map(|file| {
                let result = (|| -> Result<Option<Pixels>> {
                    let file = match file.map_err(Error::Invalid)? {
                        Thumbnail::Cached(file) => file,
                        Thumbnail::Rebuild(source) => return source.rebuild_thumbnail().map(Some),
                    };
                    let mut reader =
                        ImageReader::new(BufReader::new(file)).with_guessed_format()?;
                    let mut limits = image::Limits::default();
                    limits.max_image_width = Some(512);
                    limits.max_image_height = Some(512);
                    limits.max_alloc = Some(4 * 1024 * 1024);
                    reader.limits(limits);
                    Ok(Some(pixels(reader.decode()?)))
                })();
                result.unwrap_or_else(|e| {
                    warnings += 1;
                    tracing::warn!(%e,"thumbnail read failed");
                    None
                })
            })
            .collect();
        (images, warnings)
    }
}
impl Details {
    /// 读取详情中的 EXIF 和地点等媒体信息，合并已冻结的数据库字段。
    pub fn read(mut self) -> AssetDetails {
        if !self.metadata.exif.is_empty() {
            return self.metadata;
        }
        let (fields, location) = match self.source {
            Ok(source) => {
                let lease = source._lease.clone();
                let result = source.frozen_file().map(|(file, _frozen)| read_exif(file));
                drop(lease);
                result.unwrap_or_else(|e| (vec![("原片状态".into(), e.to_string())], None))
            }
            Err(e) => (vec![("原片状态".into(), e)], None),
        };
        self.metadata.exif = fields;
        self.metadata.exif_location = location;
        self.metadata
    }
}
/// 从图像容器提取展示用 EXIF 字段和 GPS，缺少元数据时返回空结果。
fn read_exif(file: File) -> (Vec<(String, String)>, Option<AssetLocation>) {
    let mut bytes = Vec::new();
    if let Err(e) = file.take(16 * 1024 * 1024).read_to_end(&mut bytes) {
        return (vec![("元数据状态".into(), e.to_string())], None);
    }
    match exif::Reader::new().read_from_container(&mut std::io::Cursor::new(bytes)) {
        Ok(data) => {
            let location = crate::location::exif_location(&data);
            let fields = [
                (exif::Tag::Model, "相机型号"),
                (exif::Tag::LensModel, "镜头"),
                (exif::Tag::DateTimeOriginal, "拍摄时间"),
                (exif::Tag::FNumber, "光圈"),
                (exif::Tag::ExposureTime, "快门"),
                (exif::Tag::PhotographicSensitivity, "ISO"),
                (exif::Tag::FocalLength, "焦距"),
                (exif::Tag::GPSLatitude, "纬度"),
                (exif::Tag::GPSLatitudeRef, "纬度方向"),
                (exif::Tag::GPSLongitude, "经度"),
                (exif::Tag::GPSLongitudeRef, "经度方向"),
            ]
            .into_iter()
            .filter_map(|(tag, label)| {
                data.get_field(tag, exif::In::PRIMARY)
                    .map(|f| (label.into(), f.display_value().with_unit(&data).to_string()))
            })
            .collect();
            (fields, location)
        }
        Err(exif::Error::NotFound(_)) => (vec![], None),
        Err(e) => (
            vec![("元数据状态".into(), format!("EXIF 无法读取：{e}"))],
            None,
        ),
    }
}

/// Decode to a requested edge; large/RAW system codecs scale before pixel transfer.
///
/// 按文件类型选择统一的有界栅格解码流程。
pub fn raster(file: File, hint: &Path, edge: u32) -> Result<crate::native::Decoded> {
    raster_with_filter(file, hint, edge, Resampling::Area)
}
/// 按格式、目标边长和滤镜解码媒体，统一处理方向与色彩转换。
pub(crate) fn raster_with_filter(
    mut file: File,
    hint: &Path,
    edge: u32,
    filter: Resampling,
) -> Result<crate::native::Decoded> {
    if hint
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("svg"))
    {
        return crate::vector::svg(file, edge);
    }
    if crate::video::extension(hint).is_some() {
        return crate::video::decode_file(file, edge);
    }
    let mut reader = ImageReader::new(BufReader::new(file.try_clone()?)).with_guessed_format()?;
    let format = reader.format();
    let raw = crate::native::raw_extension(hint).is_some()
        && !matches!(
            format,
            Some(
                ImageFormat::Jpeg
                    | ImageFormat::Png
                    | ImageFormat::WebP
                    | ImageFormat::Gif
                    | ImageFormat::Bmp
            )
        );
    reader.limits(decode_limits());
    let decoded = if raw {
        Err(Error::Invalid("RAW 使用内置 LibRaw 解码".into()))
    } else {
        (|| -> Result<crate::native::Decoded> {
            let mut decoder = reader.into_decoder()?;
            if decoder.total_bytes() > 384 * 1024 * 1024 {
                return Err(Error::Invalid("此图需要系统缩放解码以控制内存".into()));
            }
            let orientation = decoder.orientation()?;
            let icc = decoder.icc_profile()?;
            let mut image = image::DynamicImage::from_decoder(decoder)?;
            image = crate::color::to_srgb(image, icc.as_deref())?;
            image.apply_orientation(orientation);
            let (width, height) = image.dimensions();
            let image = if edge > 0 {
                resize::fit_with(&image, edge, filter)?
            } else {
                image
            };
            Ok(crate::native::Decoded {
                width,
                height,
                image: Some(image),
            })
        })()
    };
    match decoded {
        Ok(image) => Ok(image),
        Err(e @ Error::Color(_)) => Err(e),
        Err(original) => {
            file.seek(SeekFrom::Start(0))?;
            let extension = hint.extension().and_then(|s| s.to_str()).unwrap_or("image");
            let system_edge = if raw {
                edge
            } else if edge == 0 {
                8192
            } else {
                edge
            };
            let result = crate::native::decode_file(file, extension, system_edge);
            match result {
                Ok(decoded) if !raw && edge == 0 && decoded.width.max(decoded.height) > 8192 => {
                    Err(Error::Invalid(
                        "此图超出系统原尺寸转换预算，请指定最长边不超过 8192；原文件导出不受影响"
                            .into(),
                    ))
                }
                Ok(decoded) => Ok(decoded),
                Err(native) => Err(Error::Invalid(format!("{original}；{native}"))),
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    /// 验证版本来源绑定正确素材、保持冻结且不切换展示版本。
    fn version_sources_are_asset_scoped_frozen_and_do_not_change_display() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("original.png");
        image::RgbImage::from_pixel(40, 20, image::Rgb([10, 20, 30]))
            .save(&source)
            .unwrap();
        let mut library = Library::create(dir.path(), "比较").unwrap();
        library.import(&source, None).unwrap();
        let asset = library
            .page(&Query::default(), 0, 1)
            .unwrap()
            .assets
            .remove(0);
        let version = AssetVersionId::default();
        let edited = library.root.join("versions/edit.png");
        image::RgbImage::from_pixel(20, 40, image::Rgb([90, 80, 70]))
            .save(&edited)
            .unwrap();
        library.db.execute("INSERT INTO \
                asset_versions(id,asset_id,parent_version_id,origin,path,sha256,format,width,height,bytes) \
                VALUES(?1,?2,?3,'synco','versions/edit.png',?5,'png',20,40,?4)",
params![version.to_string(),
asset.id.to_string(),
asset.version_id.to_string(),
fs::metadata(&edited).unwrap().len(),
organize::hash_file(&edited).unwrap()]).unwrap();

        assert!(library.version_source(AssetId::default(), version).is_err());
        let left = library.version_source(asset.id, asset.version_id).unwrap();
        let right = library.version_source(asset.id, version).unwrap();
        library.display_version(asset.id, version).unwrap();
        let left = left.preview().unwrap().image;
        let right = right.preview().unwrap().image;
        assert_eq!((left.width, left.height), (40, 20));
        assert_eq!((right.width, right.height), (20, 40));
        assert_eq!(
            library.page(&Query::default(), 0, 1).unwrap().assets[0].version_id,
            version
        );
        assert_eq!(
            fs::read(&source).unwrap(),
            fs::read(
                library
                    .version_path(&asset.version_id.to_string(), false)
                    .unwrap()
            )
            .unwrap()
        );
    }
    #[test]
    /// 验证大像素与长边图片可正常导入，原片内容保持不变。
    fn imports_more_than_sixty_megapixels_and_long_sides_while_preserving_original() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("大图.jpg");
        image::RgbImage::from_pixel(8000, 8000, image::Rgb([35, 70, 105]))
            .save(&source)
            .unwrap();
        let expected = organize::hash_file(&source).unwrap();
        let mut lib = Library::create(dir.path(), "大图验证").unwrap();
        lib.import(&source, None).unwrap();
        let asset = lib.page(&Query::default(), 0, 1).unwrap().assets.remove(0);
        assert_eq!((asset.width, asset.height), (8000, 8000));
        let preview = lib.media_source(asset.id).unwrap().preview().unwrap();
        assert_eq!((preview.image.width, preview.image.height), (1600, 1600));
        assert_eq!(preview.background.width, 128);
        assert_eq!(organize::hash_file(&source).unwrap(), expected);
        let long = dir.path().join("长图.png");
        image::RgbImage::from_pixel(30001, 2, image::Rgb([45, 80, 125]))
            .save(&long)
            .unwrap();
        lib.import(&long, None).unwrap();
        assert_eq!(
            lib.page(
                &Query {
                    format: "png".into(),
                    ..Default::default()
                },
                0,
                1
            )
            .unwrap()
            .assets[0]
                .width,
            30001
        );
    }
}
