//! Runtime manifests are embedded at build time, never trusted from the install folder.
pub type Result<T> = std::result::Result<T, String>;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

pub fn verify_runtime(root: &Path, manifest: &str, replaceable: &[&str]) -> Result<PathBuf> {
    let files: BTreeMap<String, String> =
        serde_json::from_str(manifest.trim_start_matches('\u{feff}'))
            .map_err(|e| format!("运行组件清单无效：{e}"))?;
    if files.is_empty() || files.len() > 20000 {
        return Err("运行组件清单缺失，请重新准备或解压完整运行包".into());
    }
    reject_link(root)?;
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let mut pending = vec![root.clone()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            let relative = path
                .strip_prefix(&root)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            reject_link(&path)?;
            if entry.file_type().map_err(|e| e.to_string())?.is_dir() {
                pending.push(path);
                continue;
            }
            // The manifest covers data as well as executables, including POSIX node.
            // Its on-disk copy is informational; only the embedded copy is trusted.
            if relative != "checksums.json" && !files.contains_key(&relative) {
                return Err(format!("发现未登记的运行组件：{relative}"));
            }
        }
    }
    for (name, expected) in files {
        if name.contains('\\')
            || !Path::new(&name)
                .components()
                .all(|c| matches!(c, Component::Normal(_)))
        {
            return Err("运行组件路径无效".into());
        }
        let mut path = root.clone();
        for component in Path::new(&name).components() {
            path.push(component);
            reject_link(&path)?;
        }
        if !path
            .canonicalize()
            .map_err(|e| e.to_string())?
            .starts_with(&root)
        {
            return Err("运行组件路径越界".into());
        }
        if replaceable.contains(&name.as_str()) {
            continue;
        }
        let mut file = fs::File::open(&path).map_err(|e| format!("运行组件缺失 {name}：{e}"))?;
        if file.metadata().map_err(|e| e.to_string())?.len() > 512 * 1024 * 1024 {
            return Err("运行组件过大".into());
        }
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 65536];
        loop {
            let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            hash.update(&buffer[..n]);
        }
        if format!("{:x}", hash.finalize()) != expected.to_lowercase() {
            return Err(format!("运行组件校验失败：{name}，请重新解压完整运行包"));
        }
    }
    Ok(root)
}
fn reject_link(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path).map_err(|e| {
        format!(
            "运行组件缺失或无法读取：{}（{e}），请重新解压完整运行包",
            path.display()
        )
    })?;
    let linked = metadata.file_type().is_symlink();
    #[cfg(windows)]
    let linked = {
        use std::os::windows::fs::MetadataExt;
        linked || metadata.file_attributes() & 0x400 != 0
    };
    if linked {
        return Err("运行组件不能位于链接或重解析目录中".into());
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn manifest_rejects_tampering_escape_and_links() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("worker.mjs"), b"trusted").unwrap();
        let digest = format!("{:x}", Sha256::digest(b"trusted"));
        let manifest = serde_json::json!({"worker.mjs":digest}).to_string();
        assert!(verify_runtime(dir.path(), &manifest, &[]).is_ok());
        for name in ["node", "node.zip", "injected.json", "shadow.js"] {
            fs::write(dir.path().join(name), b"injected").unwrap();
            assert!(verify_runtime(dir.path(), &manifest, &[]).is_err());
            fs::remove_file(dir.path().join(name)).unwrap();
        }
        fs::write(dir.path().join("shadow.js"), b"injected").unwrap();
        assert!(verify_runtime(dir.path(), &manifest, &[]).is_err());
        fs::remove_file(dir.path().join("shadow.js")).unwrap();
        fs::write(dir.path().join("worker.mjs"), b"changed").unwrap();
        assert!(verify_runtime(dir.path(), &manifest, &[]).is_err());
        fs::remove_file(dir.path().join("worker.mjs")).unwrap();
        let missing = verify_runtime(dir.path(), &manifest, &[]).unwrap_err();
        assert!(missing.contains("worker.mjs") && missing.contains("重新解压"));
        assert!(verify_runtime(dir.path(), r#"{"../escape":"00"}"#, &[]).is_err());
        assert!(verify_runtime(dir.path(), "{}", &[]).is_err());
    }
}
