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
    if !["ffmpeg", "ffprobe"].contains(&name) {
        return Err(Error::Invalid("媒体工具名称无效".into()));
    }
    let name = format!("{name}{}", std::env::consts::EXE_SUFFIX);
    let path = runtime_root()?.join(name);
    if !path.is_file() {
        return Err(Error::Invalid("发行包缺少视频工具".into()));
    }
    Ok(path)
}
pub(crate) fn runtime_root() -> Result<PathBuf> {
    let exe = std::env::current_exe()?;
    let folder = exe
        .parent()
        .ok_or_else(|| Error::Invalid("程序目录不可用".into()))?;
    let root = folder.join("codecs");
    #[cfg(debug_assertions)]
    let root = if root.is_dir() {
        root
    } else {
        PathBuf::from(env!("MORIQ_DEV_CODECS_ROOT"))
    };
    static VERIFIED: std::sync::OnceLock<std::result::Result<PathBuf, String>> =
        std::sync::OnceLock::new();
    let root = VERIFIED
        .get_or_init(|| {
            moriq_ai::verify_runtime(
                &root,
                include_str!(concat!(env!("OUT_DIR"), "/codecs-checksums.json")),
                &["libraw.dll"],
            )
        })
        .as_ref()
        .map_err(|e| Error::Invalid(e.clone()))?;
    Ok(root.clone())
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
    child: Mutex<Option<Child>>,
    input: Mutex<(std::process::ChildStdin, u64)>,
    source: Option<tempfile::TempPath>,
    lease: Option<Arc<File>>,
    diagnostics: Option<tempfile::NamedTempFile>,
    prepare_spawn_ms: f64,
    requested: Instant,
    frames: Arc<Mutex<PlaybackFrames>>,
}
pub struct PlaybackFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}
#[derive(Default)]
struct PlaybackFrames {
    latest: Option<PlaybackFrame>,
    received: u64,
    position: u64,
    duration: u64,
    error: Option<String>,
}
fn receive_frames(mut pipe: impl Read, frames: &Mutex<PlaybackFrames>) -> std::io::Result<()> {
    loop {
        let mut header = [0_u8; 32];
        pipe.read_exact(&mut header)?;
        let width = u32::from_le_bytes([header[8], header[9], header[10], header[11]]);
        let height = u32::from_le_bytes([header[12], header[13], header[14], header[15]]);
        if &header[..8] != b"MORIQFR1" || width == 0 || height == 0 || width > 1920 || height > 1920
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "视频帧尺寸或协议无效",
            ));
        }
        let mut rgba = vec![0; width as usize * height as usize * 4];
        pipe.read_exact(&mut rgba)?;
        let mut state = frames
            .lock()
            .map_err(|_| std::io::Error::other("视频帧状态锁失效"))?;
        state.position = u64::from_le_bytes([
            header[16], header[17], header[18], header[19], header[20], header[21], header[22],
            header[23],
        ]);
        state.duration = u64::from_le_bytes([
            header[24], header[25], header[26], header[27], header[28], header[29], header[30],
            header[31],
        ]);
        state.received += 1;
        // Replace, never queue: a slow renderer cannot retain minutes of decoded video.
        state.latest = Some(PlaybackFrame {
            width,
            height,
            rgba,
        });
    }
}
#[cfg(test)]
#[allow(clippy::expect_used)]
mod frame_tests {
    use super::*;
    fn packet(width: u32, height: u32, position: u64, rgba: &[u8]) -> Vec<u8> {
        let mut bytes = b"MORIQFR1".to_vec();
        bytes.extend(width.to_le_bytes());
        bytes.extend(height.to_le_bytes());
        bytes.extend(position.to_le_bytes());
        bytes.extend(5000_u64.to_le_bytes());
        bytes.extend(rgba);
        bytes
    }
    #[test]
    fn frame_pipe_replaces_old_pixels_and_preserves_rgba_and_clock() {
        let frames = Mutex::new(PlaybackFrames::default());
        let mut bytes = packet(1, 1, 40, &[255, 0, 0, 255]);
        bytes.extend(packet(2, 1, 80, &[0, 255, 0, 255, 0, 0, 255, 255]));
        assert_eq!(
            receive_frames(bytes.as_slice(), &frames)
                .err()
                .map(|e| e.kind()),
            Some(std::io::ErrorKind::UnexpectedEof)
        );
        let state = frames.lock().expect("frame state");
        assert_eq!(
            (state.received, state.position, state.duration),
            (2, 80, 5000)
        );
        let frame = state.latest.as_ref().expect("latest frame");
        assert_eq!((frame.width, frame.height), (2, 1));
        assert_eq!(frame.rgba, [0, 255, 0, 255, 0, 0, 255, 255]);
    }
    #[test]
    fn frame_pipe_rejects_corrupt_and_oversized_packets_before_allocating() {
        for mut bytes in [
            packet(0, 1, 0, &[]),
            packet(1921, 1, 0, &[]),
            packet(u32::MAX, u32::MAX, 0, &[]),
            packet(1, 1, 0, &[]),
        ] {
            if bytes[8..12] == 1_u32.to_le_bytes() {
                bytes[0] = 0;
            }
            let frames = Mutex::new(PlaybackFrames::default());
            assert_eq!(
                receive_frames(bytes.as_slice(), &frames)
                    .err()
                    .map(|e| e.kind()),
                Some(std::io::ErrorKind::InvalidData)
            );
            assert_eq!(frames.lock().expect("frame state").received, 0);
        }
        let frames = Mutex::new(PlaybackFrames::default());
        assert!(receive_frames(packet(2, 2, 0, &[0; 4]).as_slice(), &frames).is_err());
        assert!(frames.lock().expect("frame state").latest.is_none());
    }
}
pub(crate) fn linked_snapshot(
    source: &Path,
    root: &Path,
    identity: file_id::FileId,
) -> Result<tempfile::TempPath> {
    let staging = safe_path(root, "staging")?;
    fs::create_dir_all(&staging)?;
    let snapshot = tempfile::Builder::new()
        .prefix(".moriq-playback-")
        .tempfile_in(staging)?
        .into_temp_path();
    fs::remove_file(&snapshot)?;
    fs::hard_link(source, &snapshot)?;
    if file_id::get_file_id(&snapshot)? != identity {
        return Err(Error::Invalid(
            "播放前原片路径发生变化，已停止链接快照".into(),
        ));
    }
    Ok(snapshot)
}
pub(crate) fn copied_snapshot(file: File) -> Result<tempfile::TempPath> {
    if fs2::available_space(std::env::temp_dir())
        .is_ok_and(|available| file.metadata().is_ok_and(|m| m.len() > available))
    {
        return Err(Error::Invalid(
            "临时磁盘空间不足以播放此视频，请释放空间后重试".into(),
        ));
    }
    let mut snapshot = tempfile::NamedTempFile::new()?;
    if std::io::copy(
        &mut file.take(64 * 1024 * 1024 * 1024 + 1),
        snapshot.as_file_mut(),
    )? > 64 * 1024 * 1024 * 1024
    {
        return Err(Error::Invalid("视频文件超出64 GiB预算".into()));
    }
    snapshot.as_file().sync_all()?;
    Ok(snapshot.into_temp_path())
}
impl std::fmt::Debug for Playback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Playback")
    }
}
impl Drop for Playback {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.get_mut().ok().and_then(Option::take) {
            let _ = child.kill();
            let source = self.source.take();
            let lease = self.lease.take();
            let diagnostics = self.diagnostics.take();
            // Closing or switching media may drop the final Arc on the UI thread.
            // Reap the worker before releasing its snapshot, without blocking that thread.
            std::thread::spawn(move || {
                let _ = child.wait();
                drop((source, lease, diagnostics));
            });
        }
    }
}
impl Playback {
    pub fn elapsed_ms(&self) -> u128 {
        self.requested.elapsed().as_millis()
    }
    pub fn take_frame(&self) -> Result<Option<PlaybackFrame>> {
        let mut frames = self
            .frames
            .lock()
            .map_err(|_| Error::Invalid("视频帧状态锁失效".into()))?;
        if let Some(error) = frames.error.take() {
            return Err(Error::Invalid(error));
        }
        Ok(frames.latest.take())
    }
    pub fn progress(&self) -> (u64, u64) {
        self.frames
            .lock()
            .map(|s| (s.position, s.duration))
            .unwrap_or_default()
    }
    /// Controls are tiny writes; callers execute them outside the UI thread.
    pub fn control(
        &self,
        revision: u64,
        paused: bool,
        muted: bool,
        seek: Option<u64>,
    ) -> Result<()> {
        let mut input = self
            .input
            .lock()
            .map_err(|_| Error::Invalid("播放器控制锁失效".into()))?;
        if revision <= input.1 {
            return Ok(());
        }
        input.1 = revision;
        let input = &mut input.0;
        writeln!(
            input,
            "PAUSE {}\nMUTE {}",
            u8::from(paused),
            u8::from(muted)
        )?;
        if let Some(position) = seek {
            writeln!(input, "SEEK {position}")?;
        }
        input.flush()?;
        Ok(())
    }
    /// Explicit shutdown runs on a blocking worker before the desktop exits.
    pub fn stop(&self) -> Result<()> {
        let child = self
            .child
            .lock()
            .map_err(|_| Error::Invalid("播放器状态锁失效".into()))?
            .take();
        if let Some(mut child) = child {
            let _ = child.kill();
            child.wait()?;
        }
        if let Some(source) = &self.source {
            match fs::remove_file(source) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
    pub fn running(&self) -> Result<bool> {
        let mut child = self
            .child
            .lock()
            .map_err(|_| Error::Invalid("播放器状态锁失效".into()))?;
        let Some(child) = child.as_mut() else {
            return Ok(false);
        };
        child
            .try_wait()
            .map(|status| status.is_none())
            .map_err(Error::from)
    }
    pub fn diagnostics(&self) -> Result<String> {
        let mut text = format!("rust_prepare_spawn_ms={:.3}\n", self.prepare_spawn_ms);
        if let Ok(frames) = self.frames.lock() {
            text.push_str(&format!(
                "rust_received_frames={} position_ms={} frame_error={:?}\n",
                frames.received, frames.position, frames.error
            ));
        }
        self.diagnostics
            .as_ref()
            .ok_or_else(|| Error::Invalid("播放器诊断不可用".into()))?
            .reopen()?
            .take(65536)
            .read_to_string(&mut text)?;
        Ok(text)
    }
    pub(crate) fn start(
        snapshot: tempfile::TempPath,
        lease: Arc<File>,
        requested: Instant,
    ) -> Result<Arc<Self>> {
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
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let input = child
            .stdin
            .take()
            .ok_or_else(|| Error::Invalid("播放器控制通道不可用".into()))?;
        let output = child
            .stdout
            .take()
            .ok_or_else(|| Error::Invalid("播放器画面通道不可用".into()))?;
        let frames = Arc::new(Mutex::new(PlaybackFrames::default()));
        let received = frames.clone();
        std::thread::spawn(move || {
            if let Err(error) = receive_frames(output, &received)
                && let Ok(mut frames) = received.lock()
            {
                frames.error = Some(format!("视频画面通道已关闭：{error}"));
            }
        });
        if let Some(status) = child.try_wait()? {
            return Err(Error::Invalid(format!("内置播放器未能启动（{status}）")));
        }
        Ok(Arc::new(Self {
            input: Mutex::new((input, 0)),
            child: Mutex::new(Some(child)),
            source: Some(snapshot),
            lease: Some(lease),
            diagnostics: Some(diagnostics),
            prepare_spawn_ms: requested.elapsed().as_secs_f64() * 1000.0,
            requested,
            frames,
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
        crate::cancel::check()?;
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
    cmd.args(["-hide_banner",
"-loglevel",
"error",
"-nostdin",
"-y",
"-max_alloc",
"268435456",
"-threads",
"2",
"-protocol_whitelist",
"file,pipe",
"-format_whitelist",
"mov,matroska,webm,avi,mpegts",
"-ss"])
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
