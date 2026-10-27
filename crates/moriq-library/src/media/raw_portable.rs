use super::*;
use std::{
    os::unix::fs::PermissionsExt,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
#[cfg(bundled_libraw)]
pub(crate) fn worker() -> Result<PathBuf> {
    let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/raw-runtime/raw-worker"));
    let hash = format!("{:x}", Sha256::digest(bytes));
    let root = std::env::temp_dir().join("MORIQ-codecs").join(&hash);
    fs::create_dir_all(&root)?;
    let path = root.join("raw-worker");
    if !path.exists() {
        let mut temporary = tempfile::NamedTempFile::new_in(&root)?;
        temporary.write_all(bytes)?;
        temporary.as_file().sync_all()?;
        temporary
            .as_file()
            .set_permissions(fs::Permissions::from_mode(0o700))?;
        if let Err(error) = temporary.persist_noclobber(&path) {
            if error.error.kind() != std::io::ErrorKind::AlreadyExists {
                return Err(error.error.into());
            }
        }
    }
    if organize::hash_file(&path)? != hash {
        return Err(Error::Invalid("内置 RAW 组件校验失败".into()));
    }
    Ok(path)
}
#[cfg(not(bundled_libraw))]
pub(crate) fn worker() -> Result<PathBuf> {
    Err(Error::Invalid(
        "此构建缺少内置 LibRaw 组件，请使用完整发行包".into(),
    ))
}
pub fn decode(path: &Path, edge: u32) -> Result<native::Decoded> {
    if edge > 65535 {
        return Err(Error::Invalid("RAW 输出尺寸超出预算".into()));
    }
    let temporary = tempfile::tempdir()?;
    let output = temporary.path().join("pixels.bin");
    let mut child = Command::new(worker()?)
        .arg(path)
        .arg(&output)
        .arg(edge.to_string())
        .arg(std::process::id().to_string())
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
                        "内置 LibRaw 解码失败（{}），机型/压缩不支持、文件损坏或超出预算；原片保留",
                        status.code().unwrap_or(-1)
                    )));
                }
                break;
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
            return Err(Error::Invalid("RAW 解码超时，原片保留".into()));
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    let mut file = File::open(output)?;
    let length = file.metadata()?.len();
    if !(24..=512 * 1024 * 1024 + 24).contains(&length) {
        return Err(Error::Invalid("RAW 回执长度无效".into()));
    }
    let mut header = [0u8; 24];
    file.read_exact(&mut header)?;
    if &header[..8] != b"MORIQIMG" {
        return Err(Error::Invalid("RAW 回执标识无效".into()));
    }
    let number =
        |i: usize| u32::from_le_bytes([header[i], header[i + 1], header[i + 2], header[i + 3]]);
    let (width, height, w, h) = (number(8), number(12), number(16), number(20));
    let size = u64::from(w) * u64::from(h) * 4;
    if width == 0
        || height == 0
        || w == 0
        || h == 0
        || size + 24 != length
        || (edge > 0 && (w > edge || h > edge))
    {
        return Err(Error::Invalid("RAW 回执尺寸无效".into()));
    }
    let mut rgba = vec![0; size as usize];
    file.read_exact(&mut rgba)?;
    let image = image::RgbaImage::from_raw(w, h, rgba)
        .ok_or_else(|| Error::Invalid("RAW 像素无效".into()))?;
    Ok(native::Decoded {
        width,
        height,
        image: Some(image::DynamicImage::ImageRgba8(image)),
    })
}
