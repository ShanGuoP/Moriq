use crate::Result;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelFile {
    pub path: String,
    pub url: String,
    pub sha256: String,
    pub bytes: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelSpec {
    pub id: String,
    pub name: String,
    pub revision: String,
    pub license: String,
    pub source: String,
    pub minimum_memory_mib: u64,
    pub files: Vec<ModelFile>,
}
#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub spec: ModelSpec,
    pub installed: bool,
    pub resumable: bool,
    pub bytes: u64,
}
#[derive(Debug, Clone, Default)]
pub struct DownloadControl {
    pub stop: Arc<AtomicBool>,
    pub downloaded: Arc<AtomicU64>,
    pub total: Arc<AtomicU64>,
}
#[derive(Debug, Clone)]
pub struct Hardware {
    pub memory_mib: u64,
    pub available_mib: u64,
    pub logical_cpus: usize,
    pub provider: String,
}
pub fn hardware() -> Hardware {
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    Hardware {
        memory_mib: system.total_memory() / 1024 / 1024,
        available_mib: system.available_memory() / 1024 / 1024,
        logical_cpus: std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(2),
        provider: crate::runtime::provider_name(),
    }
}
pub fn registry() -> Result<Vec<ModelSpec>> {
    serde_json::from_str(
        include_str!("../../../workers/ai/models.json").trim_start_matches('\u{feff}'),
    )
    .map_err(|e| e.to_string())
}
pub fn model_root() -> Result<PathBuf> {
    #[cfg(windows)]
    let base = std::env::var_os("APPDATA").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let base =
        std::env::var_os("HOME").map(|p| PathBuf::from(p).join("Library/Application Support"));
    #[cfg(not(any(windows, target_os = "macos")))]
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/share")));
    Ok(base.ok_or("无法定位模型目录")?.join("MORIQ/models"))
}
fn spec(id: &str) -> Result<ModelSpec> {
    registry()?
        .into_iter()
        .find(|m| m.id == id)
        .ok_or_else(|| "未知模型".into())
}
fn key(model: &ModelSpec) -> String {
    format!("{:x}", Sha256::digest(model.revision.as_bytes()))[..24].into()
}
fn safe_file(root: &Path, name: &str) -> Result<PathBuf> {
    if name.is_empty()
        || name.contains('\\')
        || Path::new(name)
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err("模型文件路径无效".into());
    }
    Ok(root.join(name))
}
fn digest(path: &Path) -> Result<String> {
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn model_folder(model: &ModelSpec) -> Result<PathBuf> {
    Ok(model_root()?.join(&model.id))
}
pub fn inventory() -> Result<Vec<ModelInfo>> {
    registry()?
        .into_iter()
        .map(|model| {
            let root = model_folder(&model)?;
            let current = root.join("current.json");
            let active = fs::read(&current)
                .ok()
                .and_then(|b| serde_json::from_slice::<ModelSpec>(&b).ok());
            let installed = active
                .as_ref()
                .is_some_and(|m| m.id == model.id && m.revision == model.revision)
                && model.files.iter().all(|f| {
                    safe_file(&root.join(key(&model)), &f.path)
                        .is_ok_and(|p| valid_installed_file(&p, f))
                });
            let resumable = root.join(format!(".download-{}", key(&model))).is_dir();
            let bytes = model.files.iter().map(|f| f.bytes).sum();
            Ok(ModelInfo {
                spec: model,
                installed,
                resumable,
                bytes,
            })
        })
        .collect()
}
pub fn installed_path(id: &str) -> Result<PathBuf> {
    let model = spec(id)?;
    let root = model_folder(&model)?;
    let current: ModelSpec = serde_json::from_slice(
        &fs::read(root.join("current.json")).map_err(|_| format!("请先安装 {}", model.name))?,
    )
    .map_err(|e| e.to_string())?;
    if current.id != model.id || current.revision != model.revision {
        return Err("模型版本需要升级，请在模型页更新".into());
    }
    let path = root.join(key(&model));
    if !path.is_dir() {
        return Err("模型文件缺失，请重新安装".into());
    }
    for artifact in &model.files {
        let file = safe_file(&path, &artifact.path)?;
        if !valid_installed_file(&file, artifact) {
            return Err(format!("模型文件 {} 缺失或损坏，请重新安装", artifact.path));
        }
    }
    Ok(path)
}
fn valid_installed_file(path: &Path, artifact: &ModelFile) -> bool {
    type Stamp = (
        u64,
        Option<std::time::SystemTime>,
        Option<std::time::SystemTime>,
    );
    type Cache = std::collections::HashMap<PathBuf, (Stamp, String)>;
    static CACHE: std::sync::OnceLock<std::sync::Mutex<Cache>> = std::sync::OnceLock::new();
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() || metadata.len() != artifact.bytes {
        return false;
    }
    let stamp = (
        metadata.len(),
        metadata.modified().ok(),
        metadata.created().ok(),
    );
    let cache = CACHE.get_or_init(Default::default);
    if let Ok(entries) = cache.lock()
        && entries
            .get(path)
            .is_some_and(|(known, hash)| known == &stamp && hash == &artifact.sha256)
    {
        return true;
    }
    if digest(path).is_ok_and(|hash| hash == artifact.sha256) {
        if let Ok(mut entries) = cache.lock() {
            if entries.len() >= 128 {
                entries.clear();
            }
            entries.insert(path.into(), (stamp, artifact.sha256.clone()));
        }
        true
    } else {
        false
    }
}
pub(crate) fn verified_lease(id: &str) -> Result<File> {
    let model = spec(id)?;
    let root = model_folder(&model)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join("install.lock"))
        .map_err(|e| e.to_string())?;
    FileExt::try_lock_shared(&lock).map_err(|_| "模型正在安装或卸载，请稍后重试")?;
    let path = installed_path(id)?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    for artifact in &model.files {
        let file = safe_file(&path, &artifact.path)?
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if !file.starts_with(&path) || digest(&file)? != artifact.sha256 {
            return Err(format!(
                "模型文件 {} 完整性校验失败，请重新安装",
                artifact.path
            ));
        }
    }
    Ok(lock)
}
pub fn install(id: &str, control: &DownloadControl) -> Result<()> {
    install_with_mirror(id, control, true)
}
fn download_url(url: &str, use_mirror: bool) -> String {
    if use_mirror && let Some(path) = url.strip_prefix("https://huggingface.co/") {
        format!("https://hf-mirror.com/{path}")
    } else {
        url.to_string()
    }
}
pub fn install_with_mirror(id: &str, control: &DownloadControl, use_mirror: bool) -> Result<()> {
    let model = spec(id)?;
    let root = model_folder(&model)?;
    fs::create_dir_all(&root).map_err(|e| e.to_string())?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join("install.lock"))
        .map_err(|e| e.to_string())?;
    lock.try_lock_exclusive()
        .map_err(|_| "此模型已有安装或卸载任务".to_string())?;
    let stage = root.join(format!(".download-{}", key(&model)));
    fs::create_dir_all(&stage).map_err(|e| e.to_string())?;
    control
        .total
        .store(model.files.iter().map(|f| f.bytes).sum(), Ordering::Relaxed);
    control.downloaded.store(0, Ordering::Relaxed);
    let client = reqwest::blocking::Client::builder()
        .user_agent(concat!("Moriq-Space/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            let url = attempt.url();
            let allowed = url.host_str().is_some_and(|host| {
                host == "huggingface.co"
                    || host == "hf-mirror.com"
                    || host.ends_with(".huggingface.co")
                    || host.ends_with(".hf.co")
                    || host.ends_with(".xethub.hf.co")
            });
            if attempt.previous().len() >= 5 || url.scheme() != "https" || !allowed {
                attempt.error("模型下载重定向目标不受信任")
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(|e| e.to_string())?;
    for artifact in &model.files {
        if control.stop.load(Ordering::Relaxed) {
            return Err("模型下载已暂停，已保留断点".into());
        }
        let destination = safe_file(&stage, &artifact.path)?;
        fs::create_dir_all(destination.parent().ok_or("模型目录无效")?)
            .map_err(|e| e.to_string())?;
        if destination.is_file() && digest(&destination)? == artifact.sha256 {
            control
                .downloaded
                .fetch_add(artifact.bytes, Ordering::Relaxed);
            continue;
        }
        let partial = destination.with_extension(format!(
            "{}.part",
            destination
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("data")
        ));
        let mut offset = fs::metadata(&partial).map(|m| m.len()).unwrap_or(0);
        if offset > artifact.bytes {
            OpenOptions::new()
                .write(true)
                .truncate(true)
                .open(&partial)
                .map_err(|e| e.to_string())?;
            offset = 0;
        }
        control.downloaded.fetch_add(offset, Ordering::Relaxed);
        let mut received = offset;
        while received < artifact.bytes {
            if control.stop.load(Ordering::Relaxed) {
                return Err("模型下载已暂停，已保留断点".into());
            }
            let end = (received + 8 * 1024 * 1024 - 1).min(artifact.bytes - 1);
            let mut response = request_range(
                &client,
                &download_url(&artifact.url, use_mirror),
                received,
                end,
                control,
            )
            .map_err(|e| download_error(&artifact.path, use_mirror, &e))?;
            let ranged = response.status() == reqwest::StatusCode::PARTIAL_CONTENT;
            if ranged {
                let expected = format!("bytes {received}-{end}/{}", artifact.bytes);
                let range = response
                    .headers()
                    .get(reqwest::header::CONTENT_RANGE)
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("");
                if range != expected {
                    return Err("下载服务器返回了错误的续传范围".into());
                }
            } else {
                control.downloaded.fetch_sub(received, Ordering::Relaxed);
                received = 0;
            }
            let before = received;
            let mut output = OpenOptions::new()
                .create(true)
                .write(true)
                .append(ranged && received > 0)
                .truncate(!ranged || received == 0)
                .open(&partial)
                .map_err(|e| e.to_string())?;
            let mut buffer = [0u8; 65536];
            loop {
                if control.stop.load(Ordering::Relaxed) {
                    output.sync_all().map_err(|e| e.to_string())?;
                    return Err("模型下载已暂停，已保留断点".into());
                }
                let n = response
                    .read(&mut buffer)
                    .map_err(|e| format!("下载中断，已保留断点：{e}"))?;
                if n == 0 {
                    break;
                }
                received += n as u64;
                if received > artifact.bytes || (ranged && received > end + 1) {
                    return Err("模型文件超出清单声明大小或续传范围".into());
                }
                output.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
                control.downloaded.fetch_add(n as u64, Ordering::Relaxed);
            }
            output.sync_all().map_err(|e| e.to_string())?;
            if received == before || (ranged && received != end + 1) {
                return Err("下载响应未完成，已保留断点，请继续下载".into());
            }
        }
        if received != artifact.bytes || digest(&partial)? != artifact.sha256 {
            let _ = fs::remove_file(&partial);
            return Err(format!(
                "{} 的大小或 SHA-256 校验失败，请重试",
                artifact.path
            ));
        }
        fs::rename(&partial, &destination).map_err(|e| e.to_string())?;
    }
    let target = root.join(key(&model));
    if !target.exists() {
        fs::rename(&stage, &target).map_err(|e| e.to_string())?;
    } else {
        for artifact in &model.files {
            let file = safe_file(&target, &artifact.path)?;
            if !file.is_file() || digest(&file)? != artifact.sha256 {
                return Err("已有模型目录不完整，请卸载后重新安装".into());
            }
        }
    }
    let mut current = tempfile::NamedTempFile::new_in(&root).map_err(|e| e.to_string())?;
    current
        .write_all(&serde_json::to_vec_pretty(&model).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    current.as_file().sync_all().map_err(|e| e.to_string())?;
    current
        .persist(root.join("current.json"))
        .map_err(|e| e.to_string())?;
    Ok(())
}
fn request_range(
    client: &reqwest::blocking::Client,
    url: &str,
    start: u64,
    end: u64,
    control: &DownloadControl,
) -> Result<reqwest::blocking::Response> {
    for attempt in 0..4 {
        if control.stop.load(Ordering::Relaxed) {
            return Err("模型下载已暂停，已保留断点".into());
        }
        let response = client
            .get(url)
            .header(reqwest::header::RANGE, format!("bytes={start}-{end}"))
            .send()
            .map_err(|e| e.to_string())?;
        if (response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
            || response.status().is_server_error())
            && attempt < 3
        {
            let seconds = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(2 << attempt)
                .min(30);
            drop(response);
            for _ in 0..seconds * 10 {
                if control.stop.load(Ordering::Relaxed) {
                    return Err("模型下载已暂停，已保留断点".into());
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            continue;
        }
        return response.error_for_status().map_err(|e| e.to_string());
    }
    Err("下载源暂时不可用，已保留断点".into())
}
fn download_error(path: &str, use_mirror: bool, error: &impl std::fmt::Display) -> String {
    format!(
        "{} 下载失败（{}）：{}。可在模型设置中切换下载源后继续，断点已保留。",
        path,
        if use_mirror {
            "HF-Mirror"
        } else {
            "Hugging Face"
        },
        error
    )
}
pub fn uninstall(id: &str) -> Result<()> {
    let model = spec(id)?;
    crate::unload()?;
    let root = model_folder(&model)?;
    if !root.exists() {
        return Ok(());
    }
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let base = model_root()?.canonicalize().map_err(|e| e.to_string())?;
    if root.parent() != Some(base.as_path()) {
        return Err("模型卸载路径越界".into());
    }
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join("install.lock"))
        .map_err(|e| e.to_string())?;
    lock.try_lock_exclusive()
        .map_err(|_| "模型安装正在进行".to_string())?;
    if root.join("current.json").exists() {
        fs::remove_file(root.join("current.json")).map_err(|e| e.to_string())?;
    }
    for entry in fs::read_dir(&root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name != key(&model) && name != format!(".download-{}", key(&model)) {
                continue;
            }
            let path = entry.path().canonicalize().map_err(|e| e.to_string())?;
            if path.parent() != Some(root.as_path()) {
                return Err("模型卸载目录不是预期子目录".into());
            }
            fs::remove_dir_all(path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod source_tests {
    use super::*;
    #[test]
    #[allow(clippy::unwrap_used)]
    fn installation_state_detects_same_size_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("weights.onnx");
        fs::write(&path, b"good").unwrap();
        let artifact = ModelFile {
            path: "weights.onnx".into(),
            url: String::new(),
            sha256: format!("{:x}", Sha256::digest(b"good")),
            bytes: 4,
        };
        assert!(valid_installed_file(&path, &artifact));
        fs::write(&path, b"evil").unwrap();
        File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(std::time::SystemTime::now() + Duration::from_secs(1))
            .unwrap();
        assert!(!valid_installed_file(&path, &artifact));
    }
    #[test]
    #[allow(clippy::unwrap_used)]
    fn throttled_source_retries_the_same_range_and_pause_makes_no_request() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            for response in [
                "HTTP/1.1 429 Too Many Requests\r\nRetry-After: 0\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 5-7/8\r\nContent-Length: 3\r\nConnection: close\r\n\r\nabc",
            ] {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut request = Vec::new();
                let mut byte = [0];
                while !request.ends_with(b"\r\n\r\n") && request.len() < 4096 {
                    stream.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                }
                assert!(
                    String::from_utf8_lossy(&request)
                        .to_lowercase()
                        .contains("range: bytes=5-7")
                );
                stream.write_all(response.as_bytes()).unwrap();
            }
        });
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap();
        let control = DownloadControl::default();
        let response =
            request_range(&client, &format!("http://{address}/model"), 5, 7, &control).unwrap();
        assert_eq!(response.text().unwrap(), "abc");
        server.join().unwrap();
        control.stop.store(true, Ordering::Relaxed);
        assert!(
            request_range(&client, "http://127.0.0.1:1", 5, 7, &control)
                .unwrap_err()
                .contains("暂停")
        );
    }
    #[test]
    fn mirror_keeps_every_pinned_revision_and_artifact_path() {
        let models = registry().unwrap_or_default();
        assert_eq!(models.len(), 3);
        for file in models.iter().flat_map(|m| &m.files) {
            let suffix = file
                .url
                .strip_prefix("https://huggingface.co/")
                .unwrap_or_default();
            assert!(!suffix.is_empty());
            assert_eq!(
                download_url(&file.url, true),
                format!("https://hf-mirror.com/{suffix}")
            );
            assert_eq!(download_url(&file.url, false), file.url);
        }
        for url in [
            "https://huggingface.co.evil.test/repo",
            "https://other.test/file",
            "http://127.0.0.1/model",
        ] {
            assert_eq!(download_url(url, true), url);
        }
    }
}
