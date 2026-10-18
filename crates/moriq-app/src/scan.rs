//! Depth-first streaming traversal. Retains directory cursors, not the whole file list.
use super::*;
use std::{fs, path::Path};

pub(super) fn walk(
    root: &Path,
    paths: Vec<PathBuf>,
    cancel: &AtomicBool,
    mut emit: impl FnMut(Result<ImportFile>) -> bool,
) {
    for path in paths {
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        let result: std::io::Result<Option<PathBuf>> = (|| {
            if fs::symlink_metadata(&path)?.file_type().is_symlink() {
                return Ok(None);
            }
            Ok(Some(path.canonicalize()?))
        })();
        let path = match result {
            Ok(Some(p)) => p,
            Ok(None) => continue,
            Err(e) => {
                if !emit(Err(format!("无法读取来源：{e}"))) {
                    return;
                }
                continue;
            }
        };
        if path.starts_with(root) {
            if !emit(Err("不能将资源库内部文件再次作为导入来源".into())) {
                return;
            }
            continue;
        }
        let folder = if path.is_dir() {
            path.as_path()
        } else {
            path.parent().unwrap_or(&path)
        };
        let base = folder.parent().unwrap_or(folder).to_path_buf();
        let project_name = folder
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_string);
        let mut next = Some(path);
        let mut cursors: Vec<fs::ReadDir> = Vec::new();
        loop {
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            let path = if let Some(path) = next.take() {
                path
            } else {
                let Some(cursor) = cursors.last_mut() else {
                    break;
                };
                match cursor.next() {
                    Some(Ok(entry)) => entry.path(),
                    Some(Err(e)) => {
                        if !emit(Err(format!("无法读取目录项：{e}"))) {
                            return;
                        }
                        continue;
                    }
                    None => {
                        cursors.pop();
                        continue;
                    }
                }
            };
            let metadata = match fs::symlink_metadata(&path) {
                Ok(m) => m,
                Err(e) => {
                    if !emit(Err(format!(
                        "无法读取 {}：{e}",
                        path.file_name().unwrap_or_default().to_string_lossy()
                    ))) {
                        return;
                    }
                    continue;
                }
            };
            if metadata.file_type().is_symlink() || path.starts_with(root) {
                continue;
            }
            if metadata.is_dir() {
                if path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("moriq"))
                {
                    continue;
                }
                match fs::read_dir(&path) {
                    Ok(cursor) => cursors.push(cursor),
                    Err(e) => {
                        if !emit(Err(format!(
                            "无法读取 {}：{e}",
                            path.file_name().unwrap_or_default().to_string_lossy()
                        ))) {
                            return;
                        }
                    }
                }
            } else if metadata.is_file() && moriq_library::native::supported_path(&path) {
                match path.strip_prefix(&base) {
                    Ok(relative) => {
                        let file = ImportFile {
                            relative: relative.to_path_buf(),
                            path,
                            project_name: project_name.clone(),
                        };
                        if !emit(Ok(file)) {
                            return;
                        }
                    }
                    Err(_) => {
                        if !emit(Err("源目录发生变化".into())) {
                            return;
                        }
                    }
                }
            }
        }
    }
}
