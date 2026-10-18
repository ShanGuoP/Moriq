//! Frozen file handles and library leases cross to bounded media workers.
use super::*;
use std::{
    io::{BufReader, Seek, SeekFrom},
    sync::Arc,
};

pub struct Source {
    file: File,
    expected: Option<String>,
    extension: String,
    _lease: Arc<File>,
}
pub struct Thumbnails {
    files: Vec<std::result::Result<Option<File>, String>>,
    _lease: Arc<File>,
}
pub struct Preview {
    pub image: Pixels,
    pub background: Pixels,
}
pub struct Details {
    metadata: AssetDetails,
    source: std::result::Result<Source, String>,
}
impl Library {
    pub fn media_source(&self, asset: AssetId) -> Result<Source> {
        let version: String = self.db.query_row(
            "SELECT display_version_id FROM assets WHERE id=?1",
            [asset.to_string()],
            |r| r.get(0),
        )?;
        self.version_source(asset, AssetVersionId(parse_id(&version)?))
    }
    pub fn version_source(&self, asset: AssetId, version: AssetVersionId) -> Result<Source> {
        let (version, hash, extension): (String, String, String) = self.db.query_row(
            "SELECT id,sha256,format FROM asset_versions WHERE asset_id=?1 AND id=?2",
            params![asset.to_string(), version.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        let referenced = self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM file_locations WHERE version_id=?1)",
            [&version],
            |r| r.get::<_, bool>(0),
        )?;
        let file = File::open(self.version_path(&version, false)?)?;
        Ok(Source {
            file,
            expected: referenced.then_some(hash),
            extension,
            _lease: self._lock.clone(),
        })
    }
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
                    Ok(file) => Ok(Some(file)),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                    Err(e) => Err(e.to_string()),
                }
            })
            .collect();
        Ok(Thumbnails {
            files,
            _lease: self._lock.clone(),
        })
    }
    pub fn details_plan(&self, id: AssetId) -> Result<Details> {
        Ok(Details {
            metadata: self.details_metadata(id)?,
            source: self.media_source(id).map_err(|e| e.to_string()),
        })
    }
}
impl Source {
    fn frozen_file(mut self) -> Result<(File, Option<tempfile::NamedTempFile>)> {
        if let Some(expected) = self.expected.take() {
            let mut frozen = tempfile::NamedTempFile::new()?;
            let length = std::io::copy(&mut self.file.take(MAX_BYTES + 1), frozen.as_file_mut())?;
            if length > MAX_BYTES || organize::hash_file(frozen.path())? != expected {
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
    pub fn preview(self) -> Result<Preview> {
        // Hold the lease until all decoding and derivative generation is finished.
        let lease = self._lease.clone();
        let extension = self.extension.clone();
        let (file, _frozen) = self.frozen_file()?;
        let preview = raster(file, &PathBuf::from(format!("image.{extension}")), 1600)?
            .image
            .ok_or_else(|| Error::Invalid("预览解码无像素".into()))?;
        let small = resize::fit(&preview, 128)?.to_rgba8();
        let background = image::DynamicImage::ImageRgba8(image::imageops::blur(&small, 8.0));
        let result = Preview {
            image: pixels(preview),
            background: pixels(background),
        };
        drop(lease);
        Ok(result)
    }
}
impl Thumbnails {
    pub fn read(self) -> (Vec<Option<Pixels>>, usize) {
        let mut warnings = 0;
        let images = self
            .files
            .into_iter()
            .map(|file| {
                let result = (|| -> Result<Option<Pixels>> {
                    let Some(file) = file.map_err(Error::Invalid)? else {
                        return Ok(None);
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
    pub fn read(mut self) -> AssetDetails {
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
pub fn raster(mut file: File, hint: &Path, edge: u32) -> Result<crate::native::Decoded> {
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
        Err(Error::Invalid("RAW 需要系统解码".into()))
    } else {
        (|| -> Result<crate::native::Decoded> {
            let mut decoder = reader.into_decoder()?;
            if decoder.total_bytes() > 384 * 1024 * 1024 {
                return Err(Error::Invalid("此图需要系统缩放解码以控制内存".into()));
            }
            let orientation = decoder.orientation()?;
            let mut image = image::DynamicImage::from_decoder(decoder)?;
            image.apply_orientation(orientation);
            let (width, height) = image.dimensions();
            let image = if edge > 0 {
                resize::fit(&image, edge)?
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
        Err(original) => {
            file.seek(SeekFrom::Start(0))?;
            let extension = hint.extension().and_then(|s| s.to_str()).unwrap_or("image");
            let result =
                crate::native::decode_file(file, extension, if edge == 0 { 8192 } else { edge });
            match result {
                Ok(decoded) if edge == 0 && decoded.width.max(decoded.height) > 8192 => {
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
        library.db.execute("INSERT INTO asset_versions(id,asset_id,parent_version_id,origin,path,sha256,format,width,height,bytes) VALUES(?1,?2,?3,'synco','versions/edit.png',?5,'png',20,40,?4)",params![version.to_string(),asset.id.to_string(),asset.version_id.to_string(),fs::metadata(&edited).unwrap().len(),organize::hash_file(&edited).unwrap()]).unwrap();
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
