use crate::{Result, hardware, installed_path, registry};
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{Mutex, OnceLock, mpsc},
    time::Duration,
};
struct Worker {
    child: Child,
    input: ChildStdin,
    output: mpsc::Receiver<Result<Value>>,
    model_lease: Option<(String, std::fs::File)>,
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
        unload()?;
    }
    Ok(())
}
fn runtime_root() -> Result<PathBuf> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    for parent in exe.ancestors().skip(1).take(4) {
        for folder in [parent.join("ai"), parent.join("ai-runtime")] {
            if folder.join("worker.mjs").is_file()
                && folder
                    .join(format!("node{}", std::env::consts::EXE_SUFFIX))
                    .is_file()
            {
                return Ok(folder);
            }
        }
    }
    Err("发行包缺少本地 AI 运行组件，请重新解压完整发行包".into())
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
            file.persist(&path).map_err(|e| e.to_string())?;
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
    let mut child = command
        .arg("--max-old-space-size=4096")
        .arg(root.join("worker.mjs"))
        .current_dir(&root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
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
    })
}
pub fn unload() -> Result<()> {
    *WORKER
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| "AI 运行锁失效")? = None;
    Ok(())
}
pub fn infer(id: &str, mut request: Value) -> Result<Value> {
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
    request["model_root"] = Value::String(path.to_string_lossy().into_owned());
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
    let mut worker = WORKER
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| "AI 运行锁失效")?;
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
        *worker = None;
        return Err(format!("AI 进程已退出：{error}"));
    }
    let result = active.output.recv_timeout(Duration::from_secs(600));
    let result = match result {
        Ok(result) => result,
        Err(error) => {
            *worker = None;
            return Err(format!("AI 任务超时或进程退出：{error}"));
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
    Ok(result)
}
