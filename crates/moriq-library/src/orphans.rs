//! Only old, unregistered managed files are candidates. Quarantine is recoverable.
use super::*;
use std::sync::Arc;
pub struct Scan {
    root: PathBuf,
    id: LibraryId,
    readers: Arc<read_pool::ReadPool>,
    lease: Arc<File>,
}
#[derive(Debug, Clone)]
pub struct Plan {
    root: PathBuf,
    id: LibraryId,
    snapshot: Arc<tempfile::TempPath>,
    _lease: Arc<File>,
    pub count: u64,
    pub bytes: u64,
    pub examples: Vec<String>,
}
#[derive(Debug)]
pub struct Candidate {
    path: String,
    bytes: u64,
    modified: u64,
    identity: String,
}
fn modified(metadata: &fs::Metadata) -> u64 {
    metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|t| t.as_nanos().min(u64::MAX as u128) as u64)
        .unwrap_or(0)
}
fn linked(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    false
}
impl Library {
    pub fn orphan_scan(&self) -> Scan {
        Scan {
            root: self.root.clone(),
            id: self.id(),
            readers: self.readers.clone(),
            lease: self._lock.clone(),
        }
    }
}
impl Scan {
    pub fn run(self) -> Result<Plan> {
        let reader = self.readers.checkout()?;
        let source = reader.connection()?;
        source.execute_batch("CREATE TEMP TABLE orphan_registered AS SELECT path FROM asset_versions UNION SELECT path FROM project_files UNION SELECT path FROM operation_journal UNION SELECT source FROM purge_files;CREATE UNIQUE INDEX orphan_registered_path ON orphan_registered(path);")?;
        let temporary = tempfile::NamedTempFile::new()?;
        let mut db = Connection::open(temporary.path())?;
        db.execute_batch("PRAGMA journal_mode=OFF;PRAGMA synchronous=OFF;PRAGMA cache_size=-2048;CREATE TABLE candidates(path TEXT PRIMARY KEY,bytes INTEGER,modified INTEGER,identity TEXT);")?;
        let tx = db.transaction()?;
        let cutoff = SystemTime::now()
            .checked_sub(std::time::Duration::from_secs(86400))
            .ok_or_else(|| Error::Invalid("系统时间无效".into()))?;
        let mut stack = vec![PathBuf::from("assets"), PathBuf::from("versions")];
        let mut count = 0;
        let mut bytes = 0;
        let mut examples = Vec::new();
        while let Some(relative) = stack.pop() {
            let text = relative
                .to_str()
                .ok_or_else(|| Error::Invalid("库内路径不能表示".into()))?
                .replace('\\', "/");
            let directory = safe_path(&self.root, &text)?;
            for entry in fs::read_dir(directory)? {
                let entry = entry?;
                let metadata = fs::symlink_metadata(entry.path())?;
                if linked(&metadata) {
                    continue;
                }
                let relative = relative.join(entry.file_name());
                if metadata.is_dir() {
                    stack.push(relative);
                    continue;
                }
                if !metadata.is_file() || metadata.modified().map(|t| t > cutoff).unwrap_or(true) {
                    continue;
                }
                let Some(text) = relative.to_str() else {
                    continue;
                };
                let text = text.replace('\\', "/");
                let exists: bool = source.query_row(
                    "SELECT EXISTS(SELECT 1 FROM orphan_registered WHERE path=?1)",
                    [&text],
                    |r| r.get(0),
                )?;
                if exists {
                    continue;
                }
                let path = safe_path(&self.root, &text)?;
                let identity = format!("{:?}", file_id::get_file_id(&path)?);
                tx.execute(
                    "INSERT INTO candidates VALUES(?1,?2,?3,?4)",
                    params![text, metadata.len(), modified(&metadata), identity],
                )?;
                count += 1;
                bytes += metadata.len();
                if examples.len() < 30 {
                    examples.push(text);
                }
            }
        }
        tx.commit()?;
        drop(db);
        Ok(Plan {
            root: self.root,
            id: self.id,
            snapshot: Arc::new(temporary.into_temp_path()),
            _lease: self.lease,
            count,
            bytes,
            examples,
        })
    }
}
impl Plan {
    pub fn page(&self, after: &str) -> Result<Vec<Candidate>> {
        let db = Connection::open_with_flags(
            self.snapshot.as_ref(),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        db.prepare("SELECT path,bytes,modified,identity FROM candidates WHERE path>?1 ORDER BY path LIMIT 128")?.query_map([after],|r|Ok(Candidate{path:r.get(0)?,bytes:r.get(1)?,modified:r.get(2)?,identity:r.get(3)?}))?.collect::<std::result::Result<Vec<_>,_>>().map_err(Error::from)
    }
}
impl Candidate {
    pub fn cursor(&self) -> &str {
        &self.path
    }
}
impl Library {
    pub fn quarantine_orphans(
        &mut self,
        plan: &Plan,
        candidates: Vec<Candidate>,
    ) -> Result<OrphanReport> {
        if self.id() != plan.id || self.root != plan.root {
            return Err(Error::Invalid("孤儿文件检查所属资源库已改变".into()));
        }
        fs::create_dir_all(safe_path(&self.root, "recovery/orphans")?)?;
        let mut report = OrphanReport::default();
        for candidate in candidates {
            let exists:bool=self.db.query_row("SELECT EXISTS(SELECT 1 FROM asset_versions WHERE path=?1 UNION ALL SELECT 1 FROM project_files WHERE path=?1 UNION ALL SELECT 1 FROM operation_journal WHERE path=?1 UNION ALL SELECT 1 FROM purge_files WHERE source=?1)",[&candidate.path],|r|r.get(0))?;
            if exists {
                report.skipped += 1;
                continue;
            }
            let path = safe_path(&self.root, &candidate.path)?;
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                report.skipped += 1;
                continue;
            };
            if linked(&metadata)
                || !metadata.is_file()
                || metadata.len() != candidate.bytes
                || modified(&metadata) != candidate.modified
                || format!("{:?}", file_id::get_file_id(&path)?) != candidate.identity
            {
                report.skipped += 1;
                continue;
            }
            let id = Uuid::new_v4().to_string();
            let relative = format!("recovery/orphans/{id}.bin");
            let target = safe_path(&self.root, &relative)?;
            self.db.execute(
                "INSERT INTO orphan_quarantine VALUES(?1,?2,?3,?4,'prepared',?5)",
                params![id, candidate.path, relative, candidate.bytes, now_ms()?],
            )?;
            match fs::rename(path, target) {
                Ok(()) => {
                    self.db.execute(
                        "UPDATE orphan_quarantine SET state='quarantined' WHERE id=?1",
                        [&id],
                    )?;
                    report.moved += 1;
                    report.bytes += candidate.bytes;
                }
                Err(_) => {
                    self.db
                        .execute("DELETE FROM orphan_quarantine WHERE id=?1", [id])?;
                    report.skipped += 1;
                }
            }
        }
        Ok(report)
    }
    pub fn recover_orphans(&mut self) -> Result<()> {
        let mut cursor = String::new();
        loop {
            let rows = self.db.prepare("SELECT id,original,target,state FROM orphan_quarantine WHERE state<>'quarantined' AND id>?1 ORDER BY id LIMIT 128")?.query_map([&cursor],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?)))?.collect::<std::result::Result<Vec<_>,_>>()?;
            let Some(last) = rows.last() else {
                break;
            };
            cursor = last.0.clone();
            for (id, original, target, state) in rows {
                if Uuid::parse_str(&id).is_err()
                    || target != format!("recovery/orphans/{id}.bin")
                    || !(original.starts_with("assets/") || original.starts_with("versions/"))
                {
                    return Err(Error::Invalid("隔离记录路径无效".into()));
                }
                let source = safe_path(&self.root, &original)?;
                let target = safe_path(&self.root, &target)?;
                if state == "purging" {
                    match fs::remove_file(target) {
                        Ok(()) => {}
                        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                        Err(e) => {
                            tracing::warn!(%e,"orphan purge deferred");
                            continue;
                        }
                    }
                    self.db
                        .execute("DELETE FROM orphan_quarantine WHERE id=?1", [id])?;
                } else if target.exists() {
                    self.db.execute(
                        "UPDATE orphan_quarantine SET state='quarantined' WHERE id=?1",
                        [id],
                    )?;
                } else if source.exists() {
                    self.db
                        .execute("DELETE FROM orphan_quarantine WHERE id=?1", [id])?;
                }
            }
        }
        Ok(())
    }
    pub fn restore_orphans_page(
        &mut self,
        purge: bool,
        after: &str,
    ) -> Result<(String, OrphanReport)> {
        let rows=self.db.prepare("SELECT id,original,target,bytes FROM orphan_quarantine WHERE state='quarantined' AND id>?1 ORDER BY id LIMIT 128")?.query_map([after],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,u64>(3)?)))?.collect::<std::result::Result<Vec<_>,_>>()?;
        let cursor = rows
            .last()
            .map(|r| r.0.clone())
            .unwrap_or_else(|| after.to_string());
        let mut report = OrphanReport::default();
        for (id, original, target, bytes) in rows {
            if target != format!("recovery/orphans/{id}.bin")
                || Uuid::parse_str(&id).is_err()
                || !(original.starts_with("assets/") || original.starts_with("versions/"))
            {
                return Err(Error::Invalid("隔离记录路径无效".into()));
            }
            let target = safe_path(&self.root, &target)?;
            if purge {
                self.db.execute(
                    "UPDATE orphan_quarantine SET state='purging' WHERE id=?1",
                    [&id],
                )?;
                match fs::remove_file(&target) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(_) => {
                        report.skipped += 1;
                        continue;
                    }
                }
            } else {
                let source = safe_path(&self.root, &original)?;
                let registered:bool=self.db.query_row("SELECT EXISTS(SELECT 1 FROM asset_versions WHERE path=?1 UNION ALL SELECT 1 FROM project_files WHERE path=?1 UNION ALL SELECT 1 FROM operation_journal WHERE path=?1 UNION ALL SELECT 1 FROM purge_files WHERE source=?1)",[&original],|r|r.get(0))?;
                if source.exists() || registered || !target.is_file() {
                    report.skipped += 1;
                    continue;
                }
                self.db.execute(
                    "UPDATE orphan_quarantine SET state='restoring' WHERE id=?1",
                    [&id],
                )?;
                if fs::rename(&target, source).is_err() {
                    self.db.execute(
                        "UPDATE orphan_quarantine SET state='quarantined' WHERE id=?1",
                        [&id],
                    )?;
                    report.skipped += 1;
                    continue;
                }
            }
            self.db
                .execute("DELETE FROM orphan_quarantine WHERE id=?1", [id])?;
            report.moved += 1;
            report.bytes += bytes;
        }
        Ok((cursor, report))
    }
}
