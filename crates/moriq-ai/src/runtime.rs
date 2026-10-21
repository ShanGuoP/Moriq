use crate::{Result, hardware, installed_path, registry};
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{Mutex, OnceLock, mpsc},
    time::Duration,
};
struct Worker {
    child: Child,
    input: ChildStdin,
    output: mpsc::Receiver<Result<Value>>,
    model_lease: Option<(String, std::fs::File)>,
    diagnostics: tempfile::NamedTempFile,
}
impl Worker {
    fn error_detail(&self) -> String {
        let mut bytes = Vec::new();
        if let Ok(file) = self.diagnostics.reopen() {
            let _ = file.take(8192).read_to_end(&mut bytes);
        }
        String::from_utf8_lossy(&bytes).trim().to_string()
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
static WORKER: OnceLock<Mutex<Option<Worker>>> = OnceLock::new();
static PROVIDER: OnceLock<Mutex<Option<String>>> = OnceLock::new();
pub(crate) fn provider_name() -> String {
    PROVIDER
        .get_or_init(|| Mutex::new(None))
        .lock()
        .ok()
        .and_then(|v| v.clone())
        .unwrap_or_else(|| "自动选择本地设备（首次分析后显示实际设备）".into())
}
static KEEP_LOADED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub fn set_keep_loaded(value: bool) -> Result<()> {
    if KEEP_LOADED.swap(value, std::sync::atomic::Ordering::Relaxed) != value {
        // A preference change takes effect at the request boundary. It must not
        // wait for, or interrupt, an inference already using the worker.
        let mutex = WORKER.get_or_init(|| Mutex::new(None));
        match mutex.try_lock() {
            Ok(mut worker) => *worker = None,
            Err(std::sync::TryLockError::WouldBlock) => {}
            Err(std::sync::TryLockError::Poisoned(error)) => {
                *error.into_inner() = None;
                mutex.clear_poison();
            }
        }
    }
    Ok(())
}
fn runtime_root() -> Result<PathBuf> {
    static VERIFIED: OnceLock<Result<PathBuf>> = OnceLock::new();
    VERIFIED.get_or_init(verify_runtime_root).clone()
}
fn verify_runtime_root() -> Result<PathBuf> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let parent = exe.parent().ok_or("程序目录不可用")?;
    let root = parent.join("ai");
    #[cfg(debug_assertions)]
    let root = if root.is_dir() {
        root
    } else {
        PathBuf::from(env!("MORIQ_DEV_AI_ROOT"))
    };
    crate::verify_runtime(
        &root,
        include_str!(concat!(env!("OUT_DIR"), "/ai-checksums.json")),
        &[],
    )
}
fn start() -> Result<Worker> {
    let root = runtime_root()?;
    #[cfg(windows)]
    let mut command = {
        use sha2::{Digest, Sha256};
        let bytes = include_bytes!(concat!(env!("OUT_DIR"), "/ai-guard.exe"));
        let folder = std::env::temp_dir()
            .join("MORIQ-codecs")
            .join(format!("ai-{:x}", Sha256::digest(bytes)));
        std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
        let path = folder.join("ai-guard.exe");
        if !std::fs::read(&path).is_ok_and(|data| Sha256::digest(data) == Sha256::digest(bytes)) {
            let mut file = tempfile::NamedTempFile::new_in(&folder).map_err(|e| e.to_string())?;
            file.write_all(bytes).map_err(|e| e.to_string())?;
            file.as_file().sync_all().map_err(|e| e.to_string())?;
            if let Err(error) = file.persist_noclobber(&path)
                && error.error.kind() != std::io::ErrorKind::AlreadyExists
            {
                return Err(error.to_string());
            }
        }
        if !std::fs::read(&path).is_ok_and(|data| Sha256::digest(data) == Sha256::digest(bytes)) {
            return Err("AI 保护进程校验失败，请关闭其他实例并清理临时运行组件".into());
        }
        let mut command = Command::new(path);
        command
            .arg(std::process::id().to_string())
            .arg("--ai")
            .arg(root.join("node.exe"));
        command
    };
    #[cfg(not(windows))]
    let mut command = Command::new(root.join("node"));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command.env_clear();
    for name in [
        "SystemRoot",
        "WINDIR",
        "TEMP",
        "TMP",
        "TMPDIR",
        "HOME",
        "USERPROFILE",
        "LOCALAPPDATA",
        "APPDATA",
    ] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    #[cfg(windows)]
    if let Some(system) = std::env::var_os("SystemRoot") {
        command.env("PATH", PathBuf::from(system).join("System32"));
    }
    #[cfg(not(windows))]
    command.env("PATH", "/usr/bin:/bin");
    let diagnostics = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
    let mut child = command
        .arg("--max-old-space-size=4096")
        .arg(worker_model_path(&root.join("worker.mjs")))
        .current_dir(worker_model_path(&root))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(diagnostics.reopen().map_err(|e| e.to_string())?)
        .env("HF_HUB_OFFLINE", "1")
        .env("TRANSFORMERS_OFFLINE", "1")
        .spawn()
        .map_err(|e| e.to_string())?;
    let input = child.stdin.take().ok_or("AI 输入管道不可用")?;
    let output = child.stdout.take().ok_or("AI 输出管道不可用")?;
    let (sender, receiver) = mpsc::sync_channel(2);
    std::thread::spawn(move || {
        let mut output = BufReader::new(output);
        loop {
            let mut line = String::new();
            let read = (&mut output).take(2 * 1024 * 1024 + 1).read_line(&mut line);
            match read {
                Ok(0) => break,
                Ok(_) if line.len() <= 2 * 1024 * 1024 => {
                    if sender
                        .send(serde_json::from_str(&line).map_err(|e| format!("AI 回执无效：{e}")))
                        .is_err()
                    {
                        break;
                    }
                }
                _ => {
                    let _ = sender.send(Err("AI 回执超出预算或无法读取".into()));
                    break;
                }
            }
        }
    });
    Ok(Worker {
        model_lease: None,
        child,
        input,
        output: receiver,
        diagnostics,
    })
}
pub fn unload() -> Result<()> {
    let mutex = WORKER.get_or_init(|| Mutex::new(None));
    match mutex.try_lock() {
        Ok(mut worker) => *worker = None,
        Err(std::sync::TryLockError::WouldBlock) => {
            return Err("模型正在使用，请停止分析后再试".into());
        }
        Err(std::sync::TryLockError::Poisoned(error)) => {
            *error.into_inner() = None;
            mutex.clear_poison();
        }
    }
    Ok(())
}
pub fn infer(id: &str, request: Value) -> Result<Value> {
    infer_controlled(id, request, &std::sync::atomic::AtomicBool::new(false))
}
pub fn infer_controlled(
    id: &str,
    mut request: Value,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<Value> {
    let cancelled = || cancel.load(std::sync::atomic::Ordering::Acquire);
    if cancelled() {
        return Err("AI 任务已停止".into());
    }
    request["keep_loaded"] = Value::Bool(KEEP_LOADED.load(std::sync::atomic::Ordering::Relaxed));
    let spec = registry()?
        .into_iter()
        .find(|m| m.id == id)
        .ok_or("未知模型")?;
    let path = installed_path(id)?
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if hardware().available_mib < spec.minimum_memory_mib / 2 {
        return Err(format!(
            "当前可用内存不足以运行 {}，请释放内存后再试",
            spec.name
        ));
    }
    request["model_root"] = Value::String(worker_model_path(&path));
    request["operation"] = Value::String(
        match id {
            "chinese-clip" => "clip",
            "florence" => "florence",
            "faces" => "faces",
            _ => return Err("模型能力未登记".into()),
        }
        .into(),
    );
    let ticket = uuid::Uuid::new_v4().to_string();
    request["id"] = Value::String(ticket.clone());
    let mutex = WORKER.get_or_init(|| Mutex::new(None));
    let mut worker = loop {
        if cancelled() {
            return Err("AI 任务已停止".into());
        }
        match mutex.try_lock() {
            Ok(guard) => break guard,
            Err(std::sync::TryLockError::WouldBlock) => {
                std::thread::sleep(Duration::from_millis(20))
            }
            Err(std::sync::TryLockError::Poisoned(error)) => {
                let mut guard = error.into_inner();
                *guard = None;
                mutex.clear_poison();
                break guard;
            }
        }
    };
    if worker
        .as_mut()
        .is_some_and(|w| w.child.try_wait().ok().flatten().is_some())
    {
        *worker = None;
    }
    if worker.is_none() {
        *worker = Some(start()?);
    }
    let active = worker.as_mut().ok_or("AI 运行组件未启动")?;
    if active
        .model_lease
        .as_ref()
        .is_none_or(|(model, _)| model != id)
    {
        let lease = crate::models::verified_lease(id)?;
        active.model_lease = Some((id.to_string(), lease));
    }
    let encoded = serde_json::to_vec(&request).map_err(|e| e.to_string())?;
    if let Err(error) = active
        .input
        .write_all(&encoded)
        .and_then(|_| active.input.write_all(b"\n"))
        .and_then(|_| active.input.flush())
    {
        let detail = active.error_detail();
        *worker = None;
        return Err(format!("AI 进程已退出：{error} {detail}"));
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(600);
    let result = loop {
        if cancelled() {
            *worker = None;
            return Err("AI 任务已停止".into());
        }
        match active.output.recv_timeout(Duration::from_millis(50)) {
            Err(mpsc::RecvTimeoutError::Timeout) if std::time::Instant::now() < deadline => {
                continue;
            }
            result => break result,
        }
    };
    let result = match result {
        Ok(result) => result,
        Err(error) => {
            let detail = active.error_detail();
            *worker = None;
            return Err(format!("AI 任务超时或进程退出：{error} {detail}"));
        }
    }?;
    if result["id"].as_str() != Some(ticket.as_str()) {
        *worker = None;
        return Err("AI 回执身份不一致".into());
    }
    if let Some(error) = result["error"].as_str() {
        return Err(error.into());
    }
    let result = result.get("result").cloned().ok_or("AI 结果缺失")?;
    if let Some(provider) = result["provider"].as_str() {
        let label = match provider {
            "cpu" => "CPU",
            "dml" => "GPU · DirectML",
            "cuda" => "GPU · CUDA",
            "coreml" => "Apple · Core ML",
            _ => "本地设备",
        };
        if let Ok(mut last) = PROVIDER.get_or_init(|| Mutex::new(None)).lock() {
            *last = Some(format!("{label} · ONNX Runtime"));
        }
    }
    if serde_json::to_vec(&result)
        .map_err(|e| e.to_string())?
        .len()
        > 1024 * 1024
    {
        return Err("AI 分析结果超出预算".into());
    }
    if request["keep_loaded"].as_bool()
        != Some(KEEP_LOADED.load(std::sync::atomic::Ordering::Relaxed))
    {
        *worker = None;
    }
    Ok(result)
}

// Transformers.js appends POSIX separators before passing model filenames to ORT.
// Windows verbatim paths forbid mixed separators, unlike ordinary drive/UNC paths.
// Keep canonical paths for leases and integrity checks; convert only at the JS boundary.
fn worker_model_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    #[cfg(windows)]
    {
        windows_model_path(&value)
    }
    #[cfg(not(windows))]
    {
        value.into_owned()
    }
}

#[cfg(any(windows, test))]
fn windows_model_path(value: &str) -> String {
    if let Some(unc) = value.strip_prefix(r"\\?\UNC\") {
        return format!("//{}", unc.replace('\\', "/"));
    }
    let ordinary = value
        .strip_prefix(r"\\?\")
        .filter(|v| v.as_bytes().get(1) == Some(&b':'))
        .unwrap_or(value);
    ordinary.replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn model_bridge_uses_appendable_drive_and_unc_paths() {
        assert_eq!(
            windows_model_path(r"\\?\C:\Users\测试 用户\models\clip"),
            "C:/Users/测试 用户/models/clip"
        );
        assert_eq!(
            windows_model_path(r"\\?\UNC\server\共享 目录\clip"),
            "//server/共享 目录/clip"
        );
        assert_eq!(
            windows_model_path(r"\\server\共享\clip"),
            "//server/共享/clip"
        );
        assert_eq!(windows_model_path(r"C:\models\clip"), "C:/models/clip");
        assert_eq!(windows_model_path("C:/models/clip"), "C:/models/clip");
    }
}
