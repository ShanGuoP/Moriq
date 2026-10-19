//! Rebuildable paths only. Originals, AI results, recovery and interop are never cache.
use super::*;
pub struct Plan {
    root: PathBuf,
    _lease: std::sync::Arc<File>,
}
impl Library {
    pub fn cache_plan(&self) -> Plan {
        Plan {
            root: self.root.clone(),
            _lease: self._lock.clone(),
        }
    }
}
impl Plan {
    pub fn trim(self, budget: u64) -> Result<CacheReport> {
        let snapshot = tempfile::NamedTempFile::new()?;
        let mut db = Connection::open(snapshot.path())?;
        db.execute_batch("PRAGMA journal_mode=OFF;PRAGMA synchronous=OFF;PRAGMA cache_size=-2048; CREATE TABLE files(path TEXT PRIMARY KEY,bytes INTEGER,modified INTEGER);")?;
        let tx = db.transaction()?;
        let mut report = CacheReport::default();
        for folder in ["previews"] {
            let directory = safe_path(&self.root, folder)?;
            for entry in fs::read_dir(directory)? {
                let entry = entry?;
                let metadata = fs::symlink_metadata(entry.path())?;
                let name = entry.file_name();
                let Some(name) = name.to_str() else {
                    report.skipped += 1;
                    continue;
                };
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    if metadata.file_attributes() & 0x400 != 0 {
                        report.skipped += 1;
                        continue;
                    }
                }
                if !metadata.is_file()
                    || metadata.file_type().is_symlink()
                    || !(name.ends_with(".png") || name.ends_with("-raw-flat1.rgba"))
                    || name.get(..36).is_none_or(|id| Uuid::parse_str(id).is_err())
                {
                    report.skipped += 1;
                    continue;
                }
                let modified = metadata
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                tx.execute(
                    "INSERT INTO files VALUES(?1,?2,?3)",
                    params![format!("{folder}/{name}"), metadata.len(), modified],
                )?;
                report.remaining_bytes = report.remaining_bytes.saturating_add(metadata.len());
            }
        }
        tx.commit()?;
        let mut statement = db.prepare("SELECT path,bytes FROM files ORDER BY modified,path")?;
        let mut rows = statement.query([])?;
        while report.remaining_bytes > budget {
            let Some(row) = rows.next()? else { break };
            let relative: String = row.get(0)?;
            let bytes: u64 = row.get(1)?;
            let path = safe_path(&self.root, &relative)?;
            match fs::remove_file(path) {
                Ok(()) => {
                    report.removed_files += 1;
                    report.removed_bytes += bytes;
                    report.remaining_bytes = report.remaining_bytes.saturating_sub(bytes)
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    report.remaining_bytes = report.remaining_bytes.saturating_sub(bytes)
                }
                Err(_) => report.skipped += 1,
            }
        }
        Ok(report)
    }
}
