//! Private bundled FFmpeg tools; never resolve executables through PATH.
use super::*;
use std::sync::{Arc, Mutex};
use std::{
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

pub const EXTENSIONS: &[&str] = &["mp4", "mov", "m4v", "mkv", "webm", "avi", "mts", "m2ts"];
pub fn extension(path: &Path) -> Option<&'static str> {
    let value = path.extension()?.to_str()?;
    EXTENSIONS
        .iter()
        .copied()
        .find(|e| value.eq_ignore_ascii_case(e))
}
pub(crate) fn sql_formats() -> String {
    EXTENSIONS
        .iter()
        .map(|s| format!("'{s}'"))
        .collect::<Vec<_>>()
        .join(",")
}
pub fn tool(name: &str) -> Result<PathBuf> {
    if !["ffmpeg", "ffprobe", "ffplay"].contains(&name) {
        return Err(Error::Invalid("媒体工具名称无效".into()));
    }
    let name = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    let exe = std::env::current_exe()?;
    let folder = exe
        .parent()
        .ok_or_else(|| Error::Invalid("程序目录不可用".into()))?;
    let mut candidates = folder.ancestors().take(3).flat_map(|p| {
        [
            p.join("codecs").join(&name),
            p.join("codecs/runtime").join(&name),
        ]
    });
    candidates
        .find(|p| p.is_file())
        .ok_or_else(|| Error::Invalid("发行包缺少内置视频组件，请重新解压完整发行包".into()))
}
#[cfg(all(windows, bundled_video))]
fn playback_worker() -> Result<PathBuf> {
    let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/video-worker.exe"));
    let root = std::env::temp_dir()
        .join("MORIQ-codecs")
        .join(format!("{:x}", Sha256::digest(bytes)));
    fs::create_dir_all(&root)?;
    let path = root.join("video-worker.exe");
    if !path.exists() {
        let mut temp = tempfile::NamedTempFile::new_in(&root)?;
        temp.write_all(bytes)?;
        temp.as_file().sync_all()?;
        if let Err(e) = temp.persist_noclobber(&path)
            && e.error.kind() != std::io::ErrorKind::AlreadyExists
        {
            return Err(Error::Io(e.error));
        }
    }
    if organize::hash_file(&path)? != format!("{:x}", Sha256::digest(bytes)) {
        return Err(Error::Invalid("视频播放组件校验失败".into()));
    }
    Ok(path)
}
#[cfg(not(all(windows, bundled_video)))]
fn playback_worker() -> Result<PathBuf> {
    Err(Error::Invalid("此构建尚未包含本平台的视频播放组件".into()))
}
pub struct Playback {
    child: Mutex<Child>,
    input: Mutex<std::process::ChildStdin>,
    _source: tempfile::TempPath,
    _lease: Arc<File>,
    diagnostics: tempfile::NamedTempFile,
}
impl std::fmt::Debug for Playback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Playback")
    }
}
impl Drop for Playback {
    fn drop(&mut self) {
        if let Ok(child) = self.child.get_mut() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
impl Playback {
    pub fn running(&self) -> Result<bool> {
        self.child
            .lock()
            .map_err(|_| Error::Invalid("播放器状态锁失效".into()))?
            .try_wait()
            .map(|status| status.is_none())
            .map_err(Error::from)
    }
    pub fn viewport(&self, x: f32, y: f32, width: f32, height: f32) -> Result<()> {
        if [x, y, width, height]
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=100000.0).contains(v))
        {
            return Err(Error::Invalid("视频显示区域无效".into()));
        }
        let mut input = self
            .input
            .lock()
            .map_err(|_| Error::Invalid("播放器控制锁失效".into()))?;
        writeln!(input, "RECT {x:.3} {y:.3} {width:.3} {height:.3}")?;
        input.flush()?;
        Ok(())
    }
    pub fn diagnostics(&self) -> Result<String> {
        let mut text = String::new();
        self.diagnostics
            .reopen()?
            .take(65536)
            .read_to_string(&mut text)?;
        Ok(text)
    }
    pub(crate) fn start(file: File, lease: Arc<File>) -> Result<Arc<Self>> {
        let mut snapshot = tempfile::NamedTempFile::new()?;
        if std::io::copy(
            &mut file.take(64 * 1024 * 1024 * 1024 + 1),
            snapshot.as_file_mut(),
        )? > 64 * 1024 * 1024 * 1024
        {
            return Err(Error::Invalid("视频文件超出64 GiB预算".into()));
        }
        snapshot.as_file().sync_all()?;
        let snapshot = snapshot.into_temp_path();
        let diagnostics = tempfile::NamedTempFile::new()?;
        let mut cmd = Command::new(playback_worker()?);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let codec = tool("ffmpeg")?;
        let directory = codec
            .parent()
            .ok_or_else(|| Error::Invalid("视频组件目录无效".into()))?;
        let windows = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("C:/Windows"));
        cmd.env(
            "PATH",
            std::env::join_paths([directory.to_path_buf(), windows.join("System32"), windows])
                .map_err(|e| Error::Invalid(e.to_string()))?,
        );
        let mut child = cmd
            .arg(&snapshot)
            .arg(std::process::id().to_string())
            .arg(diagnostics.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| Error::Invalid("播放器控制通道不可用".into()))?;
        std::thread::sleep(Duration::from_millis(350));
        if let Some(status) = child.try_wait()? {
            return Err(Error::Invalid(format!("内置播放器未能启动（{status}）")));
        }
        Ok(Arc::new(Self {
            input: Mutex::new(input),
            child: Mutex::new(child),
            _source: snapshot,
            _lease: lease,
            diagnostics,
        }))
    }
}
fn command(name: &str) -> Result<Command> {
    #[cfg(not(windows))]
    let mut cmd = Command::new(tool(name)?);
    #[cfg(windows)]
    let mut cmd = {
        let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/codec-runner.exe"));
        let root = std::env::temp_dir()
            .join("MORIQ-codecs")
            .join(format!("{:x}", Sha256::digest(bytes)));
        fs::create_dir_all(&root)?;
        let path = root.join("codec-runner.exe");
        if !path.exists() {
            let mut temp = tempfile::NamedTempFile::new_in(&root)?;
            temp.write_all(bytes)?;
            temp.as_file().sync_all()?;
            if let Err(e) = temp.persist_noclobber(&path)
                && e.error.kind() != std::io::ErrorKind::AlreadyExists
            {
                return Err(Error::Io(e.error));
            }
        }
        if organize::hash_file(&path)? != format!("{:x}", Sha256::digest(bytes)) {
            return Err(Error::Invalid("媒体资源限制组件校验失败".into()));
        }
        let mut cmd = Command::new(path);
        cmd.arg(std::process::id().to_string()).arg(tool(name)?);
        cmd
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    cmd.stdin(Stdio::null());
    Ok(cmd)
}
struct Running(Child);
impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn run(mut cmd: Command, timeout: Duration, output: Option<(&Path, u64)>) -> Result<()> {
    let log = tempfile::NamedTempFile::new()?;
    cmd.stderr(Stdio::from(log.reopen()?));
    let mut child = Running(cmd.spawn()?);
    let started = Instant::now();
    loop {
        if let Some(status) = child.0.try_wait()? {
            if status.success() {
                return Ok(());
            }
            let mut message = String::new();
            log.reopen()?.take(4096).read_to_string(&mut message)?;
            return Err(Error::Invalid(format!(
                "内置视频解码失败：{}",
                message.trim()
            )));
        }
        if started.elapsed() > timeout
            || log.as_file().metadata()?.len() > 1024 * 1024
            || output.is_some_and(|(p, max)| fs::metadata(p).is_ok_and(|m| m.len() > max))
        {
            return Err(Error::Invalid("视频探测/解码超时或输出超出预算".into()));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {
    pub width: u32,
    pub height: u32,
    pub duration_ms: u64,
    pub codec: String,
    pub pixel_format: String,
    pub frame_rate: String,
    pub color_space: String,
    pub color_transfer: String,
    pub has_audio: bool,
}
pub fn probe(path: &Path) -> Result<Metadata> {
    let output = tempfile::NamedTempFile::new()?;
    let mut cmd = command("ffprobe")?;
    cmd.args([
        "-v",
        "error",
        "-max_alloc",
        "268435456",
        "-protocol_whitelist",
        "file,pipe",
        "-format_whitelist",
        "mov,matroska,webm,avi,mpegts",
        "-show_streams",
        "-show_format",
        "-of",
        "json",
    ])
    .arg(path)
    .stdout(Stdio::from(output.reopen()?));
    run(
        cmd,
        Duration::from_secs(30),
        Some((output.path(), 2 * 1024 * 1024)),
    )?;
    if output.as_file().metadata()?.len() > 2 * 1024 * 1024 {
        return Err(Error::Invalid("视频元数据超出预算".into()));
    }
    let data: serde_json::Value =
        serde_json::from_reader(output.reopen()?).map_err(|e| Error::Invalid(e.to_string()))?;
    let streams = data["streams"]
        .as_array()
        .ok_or_else(|| Error::Invalid("视频文件没有媒体流".into()))?;
    let v = streams
        .iter()
        .find(|s| s["codec_type"] == "video" && s["disposition"]["attached_pic"] != 1)
        .ok_or_else(|| Error::Invalid("文件没有可用视频流".into()))?;
    let number = |key: &str| {
        v[key]
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(0)
    };
    let (mut width, mut height) = (number("width"), number("height"));
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > 536_870_912 {
        return Err(Error::Invalid("视频尺寸无效或超出解码预算".into()));
    }
    let rotation = v["side_data_list"]
        .as_array()
        .and_then(|a| a.iter().find_map(|s| s["rotation"].as_i64()))
        .unwrap_or(0);
    if rotation.rem_euclid(180) == 90 {
        std::mem::swap(&mut width, &mut height);
    }
    let seconds = data["format"]["duration"]
        .as_str()
        .or_else(|| v["duration"].as_str())
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|n| n.is_finite() && *n >= 0.0)
        .unwrap_or(0.0);
    let string = |key: &str| v[key].as_str().unwrap_or("").to_string();
    Ok(Metadata {
        width,
        height,
        duration_ms: (seconds * 1000.0) as u64,
        codec: string("codec_name"),
        pixel_format: string("pix_fmt"),
        frame_rate: string("avg_frame_rate"),
        color_space: string("color_space"),
        color_transfer: string("color_transfer"),
        has_audio: streams.iter().any(|s| s["codec_type"] == "audio"),
    })
}
pub fn decode(path: &Path, edge: u32) -> Result<(crate::native::Decoded, Metadata)> {
    let metadata = probe(path)?;
    let output = tempfile::Builder::new()
        .suffix(".png")
        .tempfile()?
        .into_temp_path();
    let max_edge = if edge == 0 {
        metadata.width.max(metadata.height)
    } else {
        edge
    };
    if max_edge == 0 || max_edge > 16384 {
        return Err(Error::Invalid("视频封面输出尺寸超出预算".into()));
    }
    let at = if metadata.duration_ms > 200 {
        (metadata.duration_ms as f64 / 10000.0).min(3.0)
    } else {
        0.0
    };
    let mut cmd = command("ffmpeg")?;
    cmd.args(["-hide_banner","-loglevel","error","-nostdin","-y","-max_alloc","268435456","-threads","2","-protocol_whitelist","file,pipe","-format_whitelist","mov,matroska,webm,avi,mpegts","-ss"])
       .arg(format!("{at:.3}")).arg("-i").arg(path)
       .args(["-map","0:v:0","-an","-sn","-dn","-frames:v","1","-vf"])
       .arg(format!("scale={max_edge}:{max_edge}:force_original_aspect_ratio=decrease:force_divisible_by=1,setsar=1"))
       .args(["-threads","1","-update","1"]).arg(&output).stdout(Stdio::null());
    run(
        cmd,
        Duration::from_secs(90),
        Some((&output, 512 * 1024 * 1024)),
    )?;
    let mut reader = ImageReader::open(&output)?.with_guessed_format()?;
    reader.limits(decode_limits());
    let image = reader.decode()?;
    Ok((
        crate::native::Decoded {
            width: metadata.width,
            height: metadata.height,
            image: Some(image),
        },
        metadata,
    ))
}
pub fn decode_file(file: File, edge: u32) -> Result<crate::native::Decoded> {
    let mut input = tempfile::NamedTempFile::new()?;
    if std::io::copy(
        &mut file.take(64 * 1024 * 1024 * 1024 + 1),
        input.as_file_mut(),
    )? > 64 * 1024 * 1024 * 1024
    {
        return Err(Error::Invalid("视频文件超出64 GiB预算".into()));
    }
    input.as_file_mut().sync_all()?;
    decode(input.path(), edge).map(|(d, _)| d)
}
