//! Compatibility boundary for historical model IDs; execution is Qwen-only.
use crate::Result;
use serde_json::Value;
use std::process::Command;

pub fn set_keep_loaded(value: bool) -> Result<()> {
    crate::llama::set_keep_loaded(value)
}
pub fn unload() -> Result<()> {
    crate::llama::unload()
}
pub fn infer(id: &str, request: Value) -> Result<Value> {
    infer_controlled(id, request, &std::sync::atomic::AtomicBool::new(false))
}
pub fn infer_controlled(
    id: &str,
    request: Value,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<Value> {
    if id != "qwen35-4b" {
        return Err("此模型已移除，请使用 Qwen3.5-4B 重新分析".into());
    }
    let image = request["image"].as_str().ok_or("Qwen 任务缺少图片输入")?;
    crate::llama::infer(
        request["task"].as_str().unwrap_or("classify"),
        std::path::Path::new(image),
        cancel,
    )
}

pub(crate) fn guarded_command(executable: &std::path::Path) -> Result<Command> {
    #[cfg(windows)]
    {
        use sha2::{Digest, Sha256};
        use std::io::Write;
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
            return Err("AI 保护进程校验失败".into());
        }
        let mut command = Command::new(path);
        command
            .arg(std::process::id().to_string())
            .arg("--ai")
            .arg(executable);
        Ok(command)
    }
    #[cfg(not(windows))]
    {
        Ok(Command::new(executable))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn retired_models_never_start_a_runtime() {
        for model in ["chinese-clip", "florence", "faces"] {
            assert!(super::infer(model, serde_json::json!({})).is_err());
        }
    }
}
