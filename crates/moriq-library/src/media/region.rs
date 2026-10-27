//! Immutable viewer session. GPU uploads and tile buffers stay small at any zoom.
use super::*;
use std::{
    process::{Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};
pub struct Session {
    pub width: u32,
    pub height: u32,
    path: tempfile::TempPath,
    extension: String,
    flat: Option<File>,
    _temporary_flat: Option<tempfile::TempPath>,
    _lease: Arc<File>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegionSession")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish()
    }
}
pub(crate) fn run(
    program: PathBuf,
    input: &Path,
    output: &Path,
    edge: u32,
    region: Option<ImageRegion>,
) -> Result<()> {
    let mut command = Command::new(program);
    command
        .arg(input)
        .arg(output)
        .arg(edge.to_string())
        .arg(std::process::id().to_string());
    if let Some(region) = region {
        command.args([
            region.x.to_string(),
            region.y.to_string(),
            region.width.to_string(),
            region.height.to_string(),
        ]);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .env("OMP_NUM_THREADS", "1")
        .spawn()?;
    let start = Instant::now();
    loop {
        if crate::cancel::stopped() {
            let _ = child.kill();
            let _ = child.wait();
            crate::cancel::check()?;
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return Err(Error::Invalid(format!(
                        "区域解码失败（{}）；原片保留",
                        status.code().unwrap_or(-1)
                    )));
                }
                return Ok(());
            }
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error.into());
            }
        }
        if start.elapsed() > Duration::from_secs(120) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::Invalid("区域解码超时；原片保留".into()));
        }
        std::thread::sleep(Duration::from_millis(15));
    }
}
fn header(file: &mut File) -> Result<(u32, u32, u32, u32)> {
    let mut data = [0u8; 24];
    file.read_exact(&mut data)?;
    if &data[..8] != b"MORIQIMG" {
        return Err(Error::Invalid("区域像素缓存标识无效".into()));
    }
    let number = |at| u32::from_le_bytes([data[at], data[at + 1], data[at + 2], data[at + 3]]);
    let (width, height, w, h) = (number(8), number(12), number(16), number(20));
    let bytes = u64::from(w) * u64::from(h) * 4;
    if width == 0
        || height == 0
        || w == 0
        || h == 0
        || bytes > 512 * 1024 * 1024
        || file.metadata()?.len() != 24 + bytes
    {
        return Err(Error::Invalid("区域像素缓存尺寸无效".into()));
    }
    Ok((width, height, w, h))
}
impl Session {
    pub(crate) fn open(
        root: PathBuf,
        version: AssetVersionId,
        dimensions: (u32, u32),
        extension: String,
        cacheable: bool,
        path: tempfile::TempPath,
        lease: Arc<File>,
    ) -> Result<Self> {
        crate::cancel::check()?;
        let mut session = Self {
            width: dimensions.0,
            height: dimensions.1,
            path,
            extension,
            flat: None,
            _temporary_flat: None,
            _lease: lease,
        };
        let raw = native::RAW_EXTENSIONS.contains(&session.extension.as_str());
        let raster = ["jpg", "jpeg", "png", "webp", "bmp"].contains(&session.extension.as_str())
            && u64::from(dimensions.0) * u64::from(dimensions.1) <= 64 * 1024 * 1024;
        if raw || raster {
            let kind = if raw { "raw" } else { "raster" };
            let cached = cacheable
                .then(|| safe_path(&root, &format!("previews/{version}-{kind}-flat1.rgba")))
                .transpose()?;
            let valid = cached
                .as_ref()
                .and_then(|path| File::open(path).ok())
                .and_then(|mut file| header(&mut file).ok().map(|size| (file, size)))
                .filter(|(_, (width, height, w, h))| {
                    (*width, *height) == dimensions && (*w, *h) == dimensions
                });
            let (file, (width, height, w, h)) = if let Some(value) = valid {
                value
            } else {
                let temporary = if let Some(path) = &cached {
                    tempfile::NamedTempFile::new_in(
                        path.parent()
                            .ok_or_else(|| Error::Invalid("区域缓存目录无效".into()))?,
                    )?
                } else {
                    tempfile::NamedTempFile::new()?
                };
                if raw {
                    #[cfg(windows)]
                    let program = native::raw_worker()?;
                    #[cfg(target_os = "macos")]
                    let program = crate::raw_portable::worker()?;
                    #[cfg(not(any(windows, target_os = "macos")))]
                    return Err(Error::Invalid("此构建缺少内置 RAW 解码器".into()));
                    #[cfg(any(windows, target_os = "macos"))]
                    run(program, &session.path, temporary.path(), 0, None)?;
                } else {
                    // Decode once in Rust, including ICC and EXIF orientation. All
                    // viewport tiles then read this bounded, rebuildable pixel file.
                    let image = media::raster(
                        File::open(&session.path)?,
                        Path::new(&format!("image.{}", session.extension)),
                        0,
                    )?
                    .image
                    .ok_or_else(|| Error::Invalid("原尺寸解码无像素".into()))?
                    .into_rgba8();
                    crate::cancel::check()?;
                    if image.dimensions() != dimensions {
                        return Err(Error::Invalid("原尺寸与登记信息不一致".into()));
                    }
                    let mut output = temporary.reopen()?;
                    output.write_all(b"MORIQIMG")?;
                    for n in [image.width(), image.height(), image.width(), image.height()] {
                        output.write_all(&n.to_le_bytes())?;
                    }
                    output.write_all(image.as_raw())?;
                }
                let mut file = temporary.reopen()?;
                let size = header(&mut file)?;
                if let Some(path) = cached {
                    temporary.as_file().sync_all()?;
                    temporary.persist(path).map_err(|e| Error::Io(e.error))?;
                } else {
                    session._temporary_flat = Some(temporary.into_temp_path());
                }
                (file, size)
            };
            if (width, height) != (w, h) {
                return Err(Error::Invalid("区域缓存不是原尺寸".into()));
            }
            session.width = width;
            session.height = height;
            session.flat = Some(file);
        }
        Ok(session)
    }
    pub fn tile_memory_mib(&self) -> u32 {
        if self.flat.is_some() { 16 } else { 1024 }
    }
    pub fn tile_cancellable(
        &self,
        region: ImageRegion,
        cancelled: Arc<std::sync::atomic::AtomicBool>,
    ) -> Result<Pixels> {
        let _scope = crate::cancel::Scope::enter(cancelled);
        self.tile(region)
    }
    pub fn tile(&self, region: ImageRegion) -> Result<Pixels> {
        crate::cancel::check()?;
        if region.width == 0
            || region.height == 0
            || !region.level.is_power_of_two()
            || region.level > 64
            || region.width > 512 * region.level
            || region.height > 512 * region.level
            || u64::from(region.x) + u64::from(region.width) > u64::from(self.width)
            || u64::from(region.y) + u64::from(region.height) > u64::from(self.height)
        {
            return Err(Error::Invalid("区域范围无效".into()));
        }
        if let Some(file) = &self.flat {
            let w = region.width.div_ceil(region.level);
            let h = region.height.div_ceil(region.level);
            let mut pixels = vec![0u8; (w * h * 4) as usize];
            let mut row = vec![0u8; region.width as usize * 4];
            for y in 0..h {
                crate::cancel::check()?;
                let sy = region.y + (y * region.level + region.level / 2).min(region.height - 1);
                let mut offset =
                    24 + (u64::from(sy) * u64::from(self.width) + u64::from(region.x)) * 4;
                let mut remaining = row.as_mut_slice();
                while !remaining.is_empty() {
                    #[cfg(windows)]
                    let n = {
                        use std::os::windows::fs::FileExt;
                        file.seek_read(remaining, offset)?
                    };
                    #[cfg(unix)]
                    let n = {
                        use std::os::unix::fs::FileExt;
                        file.read_at(remaining, offset)?
                    };
                    if n == 0 {
                        return Err(Error::Invalid("区域缓存像素被截断".into()));
                    }
                    offset += n as u64;
                    remaining = &mut remaining[n..];
                }
                for x in 0..w {
                    let sx = (x * region.level + region.level / 2).min(region.width - 1) as usize;
                    let destination = ((y * w + x) * 4) as usize;
                    pixels[destination..destination + 4].copy_from_slice(&row[sx * 4..sx * 4 + 4]);
                }
            }
            return Ok(Pixels {
                width: w,
                height: h,
                rgba: pixels.into(),
            });
        }
        if self.extension == "svg" {
            return crate::vector::svg_region(File::open(&self.path)?, region);
        }
        #[cfg(windows)]
        if self.extension != "svg" {
            let output = tempfile::NamedTempFile::new()?;
            run(
                native::worker()?,
                &self.path,
                output.path(),
                512,
                Some(region),
            )?;
            let mut file = output.reopen()?;
            let (_, _, w, h) = header(&mut file)?;
            if w > 512 || h > 512 {
                return Err(Error::Invalid("区域回执超出预算".into()));
            }
            let mut rgba = vec![0u8; (w * h * 4) as usize];
            file.read_exact(&mut rgba)?;
            return Ok(Pixels {
                width: w,
                height: h,
                rgba: rgba.into(),
            });
        }
        let image = media::raster(
            File::open(&self.path)?,
            Path::new(&format!("image.{}", self.extension)),
            0,
        )?
        .image
        .ok_or_else(|| Error::Invalid("区域解码无像素".into()))?;
        if region.x + region.width > image.width() || region.y + region.height > image.height() {
            return Err(Error::Invalid("原片尺寸已变化".into()));
        }
        let cropped = image.crop_imm(region.x, region.y, region.width, region.height);
        Ok(pixels(resize::fit(&cropped, 512)?))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn raster_tiles_reuse_decoded_pixels_rebuild_corruption_and_keep_live_handles_after_trim() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("original.png");
        let expected =
            image::RgbaImage::from_fn(1100, 800, |x, y| image::Rgba([x as u8, y as u8, 42, 177]));
        expected.save(&source).unwrap();
        let original = fs::read(&source).unwrap();
        let mut lib = Library::create(dir.path(), "viewer").unwrap();
        lib.import(&source, None).unwrap();
        let asset = lib.page(&Query::default(), 0, 1).unwrap().assets.remove(0);
        let cache = lib
            .root
            .join(format!("previews/{}-raster-flat1.rgba", asset.version_id));
        fs::write(&cache, b"corrupt").unwrap();
        let session = lib.media_source(asset.id).unwrap().regions().unwrap();
        assert_eq!(session.tile_memory_mib(), 16);
        assert_eq!(fs::metadata(&cache).unwrap().len(), 24 + 1100 * 800 * 4);
        let region = ImageRegion {
            x: 100,
            y: 140,
            width: 512,
            height: 400,
            level: 2,
        };
        let tile = session.tile(region).unwrap();
        assert_eq!((tile.width, tile.height), (256, 200));
        assert_eq!(&tile.rgba[..4], &expected.get_pixel(101, 141).0);
        assert_eq!(
            &tile.rgba[tile.rgba.len() - 4..],
            &expected.get_pixel(611, 539).0
        );
        let reopened = lib.media_source(asset.id).unwrap().regions().unwrap();
        assert_eq!(reopened.tile(region).unwrap().rgba, tile.rgba);
        assert!(
            reopened
                .tile_cancellable(region, Arc::new(std::sync::atomic::AtomicBool::new(true)))
                .is_err()
        );
        assert_eq!(reopened.tile(region).unwrap().rgba, tile.rgba);
        lib.cache_plan().trim(0).unwrap();
        assert!(!cache.exists());
        assert_eq!(session.tile(region).unwrap().rgba, tile.rgba);
        fs::remove_file(lib.asset_path(asset.id).unwrap()).unwrap();
        assert_eq!(session.tile(region).unwrap().rgba, tile.rgba);
        assert_eq!(fs::read(source).unwrap(), original);
    }
}
