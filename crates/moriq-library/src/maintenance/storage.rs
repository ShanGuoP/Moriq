use super::*;
use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};
#[derive(Debug, Clone, Default)]
pub struct Control {
    pub cancel: Arc<AtomicBool>,
    pub files: Arc<AtomicU64>,
}
pub struct Plan {
    readers: Arc<read_pool::ReadPool>,
    root: PathBuf,
    id: LibraryId,
    _lease: Arc<File>,
}
impl Library {
    pub fn storage_plan(&self) -> Result<Plan> {
        Ok(Plan {
            readers: self.readers.clone(),
            root: self.root.clone(),
            id: self.id(),
            _lease: self._lock.clone(),
        })
    }
}
fn identity_key(id: file_id::FileId) -> Vec<u8> {
    match id {
        file_id::FileId::Inode {
            device_id,
            inode_number,
        } => [
            vec![0],
            device_id.to_le_bytes().to_vec(),
            inode_number.to_le_bytes().to_vec(),
        ]
        .concat(),
        file_id::FileId::LowRes {
            volume_serial_number,
            file_index,
        } => [
            vec![1],
            volume_serial_number.to_le_bytes().to_vec(),
            file_index.to_le_bytes().to_vec(),
        ]
        .concat(),
        file_id::FileId::HighRes {
            volume_serial_number,
            file_id,
        } => [
            vec![2],
            volume_serial_number.to_le_bytes().to_vec(),
            file_id.to_le_bytes().to_vec(),
        ]
        .concat(),
    }
}
fn category(path: &Path) -> &'static str {
    match path
        .components()
        .next()
        .map(|c| c.as_os_str())
        .and_then(|s| s.to_str())
    {
        Some("assets") => "托管原片及项目归档",
        Some("versions") => "追加版本",
        Some("thumbnails" | "previews") => "可重建图片缓存",
        Some("indexes") => "索引",
        Some("interop") => "Synco 交接副本",
        Some("staging" | "recovery" | "trash") => "恢复与暂存数据",
        Some("backups") => "数据库升级备份",
        _ => "数据库与元数据",
    }
}
fn linked(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}
impl Plan {
    pub fn volume(&self) -> Result<(u64, u64)> {
        Ok((
            fs2::total_space(&self.root)?,
            fs2::available_space(&self.root)?,
        ))
    }
    pub fn scan(self, control: &Control) -> Result<StorageStats> {
        let (volume_total, volume_available) = self.volume()?;
        let connection = self.readers.checkout()?;
        let source = connection.connection()?;
        let identity: String =
            source.query_row("SELECT id FROM library_identity", [], |r| r.get(0))?;
        if identity != self.id.to_string() {
            return Err(Error::Invalid("资源库身份已改变".into()));
        }
        let (managed,
referenced)=source.query_row("SELECT coalesce(sum(CASE WHEN f.version_id IS NULL THEN v.bytes ELSE 0 END),0),coalesce(sum(CASE \
                WHEN f.version_id IS NOT NULL THEN v.bytes ELSE 0 END),0) FROM asset_versions v LEFT JOIN \
                file_locations f ON f.version_id=v.id",
[],
|r|Ok((r.get(0)?,
r.get(1)?)))?;

        drop(connection);
        let mut result = StorageStats {
            managed_registered: managed,
            referenced_registered: referenced,
            volume_total,
            volume_available,
            ..Default::default()
        };
        let identity_file = tempfile::NamedTempFile::new()?;
        let identities = Connection::open(identity_file.path())?;
        identities.execute_batch("PRAGMA journal_mode=OFF; PRAGMA synchronous=OFF; PRAGMA cache_size=-8192; CREATE TABLE seen(id BLOB PRIMARY KEY) WITHOUT ROWID; BEGIN;")?;
        let mut categories = BTreeMap::<String, u64>::new();
        let mut stack = vec![fs::read_dir(&self.root)?];
        while let Some(entries) = stack.last_mut() {
            if control.cancel.load(Ordering::Relaxed) {
                result.cancelled = true;
                break;
            }
            let Some(entry) = entries.next() else {
                stack.pop();
                continue;
            };
            let entry = match entry {
                Ok(v) => v,
                Err(_) => {
                    result.warnings += 1;
                    continue;
                }
            };
            let path = entry.path();
            let metadata = match fs::symlink_metadata(&path) {
                Ok(v) => v,
                Err(_) => {
                    result.warnings += 1;
                    continue;
                }
            };
            if linked(&metadata) {
                result.warnings += 1;
                continue;
            }
            if metadata.is_dir() {
                if stack.len() >= 512 {
                    result.warnings += 1;
                    continue;
                }
                match fs::read_dir(&path) {
                    Ok(entries) => stack.push(entries),
                    Err(_) => result.warnings += 1,
                }
            } else if metadata.is_file() {
                result.files += 1;
                control.files.store(result.files, Ordering::Relaxed);
                let id = match file_id::get_file_id(&path) {
                    Ok(v) => v,
                    Err(_) => {
                        result.warnings += 1;
                        continue;
                    }
                };
                if identities
                    .execute("INSERT OR IGNORE INTO seen VALUES(?1)", [identity_key(id)])?
                    == 0
                {
                    result.linked_paths += 1;
                    continue;
                }
                let bytes = match File::open(&path).and_then(|f| f.allocated_size()) {
                    Ok(v) => v,
                    Err(_) => {
                        result.warnings += 1;
                        continue;
                    }
                };
                result.allocated = result.allocated.saturating_add(bytes);
                result.unique_file_bytes = result.unique_file_bytes.saturating_add(metadata.len());
                let relative = path
                    .strip_prefix(&self.root)
                    .map_err(|e| Error::Invalid(e.to_string()))?;
                *categories.entry(category(relative).into()).or_default() += bytes;
            }
        }
        result.measured_at = now_ms()?;
        result.categories = categories.into_iter().collect();
        Ok(result)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn physical_storage_deduplicates_hardlinks_counts_references_separately_and_can_stop() {
        let dir = tempfile::tempdir().unwrap();
        let mut lib = Library::create(dir.path(), "空间").unwrap();
        let external = dir.path().join("外部.png");
        image::RgbImage::from_pixel(3, 2, image::Rgb([1, 2, 3]))
            .save(&external)
            .unwrap();
        let bytes = fs::read(&external).unwrap();
        lib.import_referenced(&external, None).unwrap();
        let a = lib.root.join("assets/one.data");
        let b = lib.root.join("assets/two.data");
        fs::write(&a, vec![42; 512 * 1024]).unwrap();
        let first = lib
            .storage_plan()
            .unwrap()
            .scan(&Control::default())
            .unwrap();
        fs::hard_link(&a, &b).unwrap();
        let second = lib
            .storage_plan()
            .unwrap()
            .scan(&Control::default())
            .unwrap();
        assert_eq!(second.unique_file_bytes, first.unique_file_bytes);
        assert_eq!(second.allocated, first.allocated);
        assert_eq!(second.linked_paths, first.linked_paths + 1);
        assert_eq!(second.managed_registered, 0);
        assert_eq!(second.referenced_registered, bytes.len() as u64);
        assert_eq!(
            second.categories.iter().map(|(_, size)| size).sum::<u64>(),
            second.allocated
        );
        assert!(second.volume_total >= second.volume_available);
        let control = Control::default();
        control.cancel.store(true, Ordering::Relaxed);
        assert!(
            lib.storage_plan()
                .unwrap()
                .scan(&control)
                .unwrap()
                .cancelled
        );
        assert_eq!(fs::read(external).unwrap(), bytes);
        assert!(a.is_file() && b.is_file());
    }
}
