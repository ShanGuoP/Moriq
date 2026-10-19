//! Windows system codecs run in a short-lived process with OS resource limits.
use super::*;
pub struct Decoded {
    pub width: u32,
    pub height: u32,
    pub image: Option<image::DynamicImage>,
}
pub const RAW_EXTENSIONS: &[&str] = &[
    "raw", "dng", "crw", "cr2", "cr3", "nef", "nrw", "arw", "sr2", "srf", "rw2", "orf", "raf",
    "pef", "srw", "rwl", "3fr", "fff", "iiq", "kdc", "dcr", "mos", "mrw", "erf", "x3f",
];
pub fn raw_extension(path: &Path) -> Option<&'static str> {
    let value = path.extension()?.to_str()?;
    RAW_EXTENSIONS
        .iter()
        .copied()
        .find(|e| value.eq_ignore_ascii_case(e))
}
pub(super) fn raw_sql() -> String {
    RAW_EXTENSIONS
        .iter()
        .map(|s| format!("'{s}'"))
        .collect::<Vec<_>>()
        .join(",")
}
#[cfg(windows)]
pub(crate) fn worker() -> Result<PathBuf> {
    let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/wic-worker.exe"));
    let hash = format!("{:x}", Sha256::digest(bytes));
    let root = std::env::temp_dir().join("MORIQ-codecs").join(&hash);
    fs::create_dir_all(&root)?;
    let path = root.join("wic-worker.exe");
    if !path.exists() {
        let mut staged = tempfile::NamedTempFile::new_in(&root)?;
        staged.write_all(bytes)?;
        staged.as_file().sync_all()?;
        if let Err(e) = staged.persist_noclobber(&path)
            && e.error.kind() != std::io::ErrorKind::AlreadyExists
        {
            return Err(Error::Io(e.error));
        }
    }
    if organize::hash_file(&path)? != hash {
        return Err(Error::Invalid("媒体解码组件校验失败".into()));
    }
    Ok(path)
}
pub fn decode(path: &Path, edge: u32) -> Result<Decoded> {
    decode_worker(path, edge, false)
}
pub fn decode_raw(path: &Path, edge: u32) -> Result<Decoded> {
    decode_worker(path, edge, true)
}
#[cfg(all(windows, bundled_libraw))]
pub(crate) fn raw_worker() -> Result<PathBuf> {
    let exe = include_bytes!(concat!(env!("OUT_DIR"), "/raw-runtime/raw-worker.exe"));
    let dll = include_bytes!(concat!(env!("OUT_DIR"), "/raw-runtime/libraw.dll"));
    let mut digest = Sha256::new();
    digest.update(exe);
    digest.update(dll);
    let root = std::env::temp_dir()
        .join("MORIQ-codecs")
        .join(format!("{:x}", digest.finalize()));
    fs::create_dir_all(&root)?;
    for (name, bytes) in [
        ("raw-worker.exe", exe.as_slice()),
        ("libraw.dll", dll.as_slice()),
    ] {
        let path = root.join(name);
        if !path.exists() {
            let mut staged = tempfile::NamedTempFile::new_in(&root)?;
            staged.write_all(bytes)?;
            staged.as_file().sync_all()?;
            if let Err(e) = staged.persist_noclobber(&path)
                && e.error.kind() != std::io::ErrorKind::AlreadyExists
            {
                return Err(Error::Io(e.error));
            }
        }
        if organize::hash_file(&path)? != format!("{:x}", Sha256::digest(bytes)) {
            return Err(Error::Invalid("内置 RAW 解码组件校验失败".into()));
        }
    }
    Ok(root.join("raw-worker.exe"))
}
#[cfg(all(windows, not(bundled_libraw)))]
pub(crate) fn raw_worker() -> Result<PathBuf> {
    Err(Error::Invalid(
        "此构建未包含 LibRaw，请使用包含内置 RAW 解码器的发行包".into(),
    ))
}
fn decode_worker(path: &Path, edge: u32, raw: bool) -> Result<Decoded> {
    #[cfg(windows)]
    {
        use std::{
            os::windows::process::CommandExt,
            process::{Command, Stdio},
            time::{Duration, Instant},
        };
        if edge > if raw { 65535 } else { 8192 } {
            return Err(Error::Invalid("系统解码输出尺寸超出预算".into()));
        }
        let output = tempfile::NamedTempFile::new()?;
        // The child opens this path independently; no write handle may remain locked.
        let output = output.into_temp_path();
        let mut child = Command::new(if raw { raw_worker()? } else { worker()? })
            .arg(path)
            .arg(&output)
            .arg(edge.to_string())
            .arg(std::process::id().to_string())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .env("OMP_NUM_THREADS", "1")
            .creation_flags(0x08000000)
            .spawn()?;
        let start = Instant::now();
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {}
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(Error::Io(error));
                }
            }
            if start.elapsed() > Duration::from_secs(if raw { 120 } else { 45 }) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error::Invalid(
                    "系统媒体解码超时，已停止解码进程；原片保留".into(),
                ));
            }
            std::thread::sleep(Duration::from_millis(15));
        };
        if !status.success() {
            if raw {
                return Err(Error::Invalid(format!(
                    "内置 LibRaw 解码失败（{}）：文件损坏、机型/压缩方式不支持或超出资源预算；原片保留",
                    status.code().unwrap_or(-1)
                )));
            }
            return Err(Error::Invalid(match status.code() {
                Some(4) => {
                    "系统没有可用的此格式/相机解码器；RAW 需要兼容的系统 RAW 编解码器，原片未修改"
                        .into()
                }
                Some(3) => "无法建立媒体解码资源隔离，已停止解码".into(),
                _ => format!(
                    "系统媒体解码失败或超出内存/时间预算（{}），原片保留",
                    status.code().unwrap_or(-1)
                ),
            }));
        }
        let length = fs::metadata(&output)?.len();
        if !(24..=512 * 1024 * 1024 + 24).contains(&length) {
            return Err(Error::Invalid("系统解码回执长度无效".into()));
        }
        let mut file = File::open(&output)?;
        let mut header = [0u8; 24];
        file.read_exact(&mut header)?;
        if &header[..8] != b"MORIQIMG" {
            return Err(Error::Invalid("系统解码回执无效".into()));
        }
        let number = |at: usize| {
            u32::from_le_bytes([header[at], header[at + 1], header[at + 2], header[at + 3]])
        };
        let (width, height, w, h) = (number(8), number(12), number(16), number(20));
        let bytes = u64::from(w) * u64::from(h) * 4;
        if width == 0
            || height == 0
            || (edge > 0 && (w > edge || h > edge))
            || bytes + 24 != length
            || (edge > 0 && (w == 0 || h == 0))
        {
            return Err(Error::Invalid("系统解码尺寸声明无效".into()));
        }
        let image = if edge == 0 && !raw {
            None
        } else {
            let mut rgba = vec![0; bytes as usize];
            file.read_exact(&mut rgba)?;
            Some(image::DynamicImage::ImageRgba8(
                image::RgbaImage::from_raw(w, h, rgba)
                    .ok_or_else(|| Error::Invalid("系统解码像素无效".into()))?,
            ))
        };
        Ok(Decoded {
            width,
            height,
            image,
        })
    }
    #[cfg(not(windows))]
    {
        #[cfg(target_os = "macos")]
        if raw {
            return crate::raw_portable::decode(path, edge);
        }
        let _ = (path, edge, raw);
        Err(Error::Invalid(
            "此平台的 RAW/系统媒体解码适配尚不可用".into(),
        ))
    }
}
pub fn decode_file(file: File, extension: &str, edge: u32) -> Result<Decoded> {
    let suffix = if extension.chars().all(|c| c.is_ascii_alphanumeric()) {
        format!(".{extension}")
    } else {
        ".image".into()
    };
    let mut input = tempfile::Builder::new().suffix(&suffix).tempfile()?;
    let limit = file_limit(Path::new(&format!("image.{extension}")));
    if std::io::copy(&mut file.take(limit + 1), input.as_file_mut())? > limit {
        return Err(Error::Invalid("媒体文件超出大小预算".into()));
    }
    input.as_file().sync_all()?;
    let input = input.into_temp_path();
    if RAW_EXTENSIONS
        .iter()
        .any(|e| extension.eq_ignore_ascii_case(e))
    {
        decode_raw(&input, edge)
    } else {
        decode(&input, edge)
    }
}

pub const IMPORT_EXTENSIONS: &[&str] = &[
    "svg", "mp4", "mov", "m4v", "mkv", "webm", "avi", "mts", "m2ts", "jpg", "jpeg", "png", "webp",
    "gif", "bmp", "tif", "tiff", "heic", "heif", "avif", "raw", "dng", "crw", "cr2", "cr3", "nef",
    "nrw", "arw", "sr2", "srf", "rw2", "orf", "raf", "pef", "srw", "rwl", "3fr", "fff", "iiq",
    "kdc", "dcr", "mos", "mrw", "erf", "x3f",
];
pub fn supported_path(path: &Path) -> bool {
    if crate::video::extension(path).is_some() {
        return true;
    }
    path.extension().is_some_and(|value| {
        IMPORT_EXTENSIONS
            .iter()
            .any(|e| value.eq_ignore_ascii_case(e))
    })
}
pub(crate) fn file_limit(path: &Path) -> u64 {
    if crate::video::extension(path).is_some() {
        64 * 1024 * 1024 * 1024
    } else if raw_extension(path).is_some() {
        2 * 1024 * 1024 * 1024
    } else {
        MAX_BYTES
    }
}

#[cfg(all(test, windows))]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn isolated_decoder_scales_and_validates_output_and_tiff_roundtrips_without_touching_source() {
        let dir = tempfile::tempdir().unwrap();
        let jpeg = dir.path().join("图片.jpg");
        image::RgbImage::from_pixel(23, 17, image::Rgb([60, 90, 120]))
            .save(&jpeg)
            .unwrap();
        let result = decode(&jpeg, 8).unwrap();
        assert_eq!((result.width, result.height), (23, 17));
        assert_eq!(result.image.unwrap().dimensions(), (8, 5));
        assert!(decode(&jpeg, 0).unwrap().image.is_none());
        let bad = dir.path().join("损坏.rw2");
        fs::write(&bad, b"not a RAW file").unwrap();
        assert!(decode(&bad, 8).is_err());
        // A minimal uncompressed RGB TIFF is a generated fixture, not a camera RAW claim.
        let mut bytes = b"II\x2a\0\x08\0\0\0".to_vec();
        bytes.extend_from_slice(&10u16.to_le_bytes());
        for (tag, kind, count, value) in [
            (256u16, 4u16, 1u32, 3u32),
            (257, 4, 1, 2),
            (258, 3, 3, 134),
            (259, 3, 1, 1),
            (262, 3, 1, 2),
            (273, 4, 1, 140),
            (277, 3, 1, 3),
            (278, 4, 1, 2),
            (279, 4, 1, 18),
            (284, 3, 1, 1),
        ] {
            bytes.extend_from_slice(&tag.to_le_bytes());
            bytes.extend_from_slice(&kind.to_le_bytes());
            bytes.extend_from_slice(&count.to_le_bytes());
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&[8, 0, 8, 0, 8, 0]);
        bytes.extend_from_slice(&[40, 80, 120].repeat(6));
        let path = dir.path().join("测试.tiff");
        fs::write(&path, &bytes).unwrap();
        let mut lib = Library::create(dir.path(), "系统格式测试").unwrap();
        lib.import(&path, None).unwrap();
        let asset = lib.page(&Query::default(), 0, 1).unwrap().assets.remove(0);
        assert_eq!(asset.format, "tiff");
        assert_eq!((asset.width, asset.height), (3, 2));
        assert_eq!(lib.preview(asset.id).unwrap().width, 3);
        let output = export::run(
            lib.export_plan(&[asset.id]).unwrap(),
            dir.path(),
            ExportOptions {
                format: ExportFormat::Png,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(output.join("测试.png").is_file());
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
}
