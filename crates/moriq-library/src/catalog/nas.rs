//! Explicit SMB roots and bounded conversion of remote originals to managed files.
use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Debug, Clone)]
pub struct NasSource {
    pub id: String,
    pub label: String,
    pub path: PathBuf,
}
impl NasSource {
    /// Windows dialogs and labels prefer ordinary UNC notation over verbatim paths.
    pub fn browse_path(&self) -> PathBuf {
        let text = self.path.to_string_lossy();
        if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
            PathBuf::from(format!(r"\\{rest}"))
        } else {
            self.path.clone()
        }
    }
}

/// Only UNC shares qualify; a drive letter can silently point at a network mount.
pub fn is_unc(path: &Path) -> bool {
    #[cfg(windows)]
    {
        use std::path::Prefix;
        matches!(path.components().next(), Some(Component::Prefix(prefix))
            if matches!(prefix.kind(), Prefix::UNC(..) | Prefix::VerbatimUNC(..)))
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        false
    }
}

fn source_for_path(db: &Connection, path: &Path) -> Result<Option<String>> {
    if !is_unc(path) {
        return Ok(None);
    }
    let mut statement = db.prepare("SELECT id,platform,native_path FROM nas_sources")?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let platform: String = row.get(1)?;
        let native: Vec<u8> = row.get(2)?;
        let root = organize::decode_path(&platform, &native)?;
        if is_unc(&root) && path.starts_with(&root) && path != root {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

pub(super) fn import_source(db: &Connection, path: &Path) -> Result<Option<String>> {
    if is_unc(path) {
        source_for_path(db, path)?
            .map(Some)
            .ok_or_else(|| Error::Invalid("请先在 NAS 面板连接包含该文件的共享目录".into()))
    } else {
        organize::reject_network_reference(path)?;
        Ok(None)
    }
}

pub(super) fn reference_path(db: &Connection, version: &str, path: &Path) -> Result<bool> {
    let source: Option<(String, Vec<u8>)> = db.query_row(
        "SELECT s.platform,s.native_path FROM nas_versions v JOIN nas_sources s ON s.id=v.source_id WHERE v.version_id=?1",
        [version], |row| Ok((row.get(0)?, row.get(1)?)),
    ).optional()?;
    if let Some((platform, native)) = source {
        let root = organize::decode_path(&platform, &native)?;
        if !is_unc(&root) || !is_unc(path) || !path.starts_with(&root) || path == root {
            return Err(Error::Invalid("NAS 来源路径不在已连接的共享目录内".into()));
        }
        return Ok(true);
    }
    organize::reject_network_reference(path)?;
    Ok(false)
}

impl Library {
    /// Resolve only a cache path for an already registered remote version.
    pub fn nas_preview_path(
        &self,
        asset: AssetId,
        version: Option<AssetVersionId>,
    ) -> Result<Option<PathBuf>> {
        let version = match version {
            Some(version) => version.to_string(),
            None => self.db.query_row(
                "SELECT display_version_id FROM assets WHERE id=?1",
                [asset.to_string()],
                |row| row.get::<_, String>(0),
            )?,
        };
        let present: bool = self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM nas_versions n JOIN asset_versions v ON v.id=n.version_id \
                WHERE n.version_id=?1 AND v.asset_id=?2)",
            params![version, asset.to_string()],
            |row| row.get(0),
        )?;
        if !present {
            return Ok(None);
        }
        Ok(Some(safe_path(
            &self.root,
            &format!("previews/{version}-srgb1-1600.png"),
        )?))
    }
    /// Store a share only after Service has checked its reachability off the writer.
    pub fn register_nas_source(&mut self, path: &Path) -> Result<NasSource> {
        if !is_unc(path) || !path.is_absolute() {
            return Err(Error::Invalid(
                "NAS 地址必须是 Windows SMB 共享路径（\\\\服务器\\共享目录）".into(),
            ));
        }
        let (platform, native) = organize::encode_path(path)?;
        let mut statement = self
            .db
            .prepare("SELECT id,label,platform,native_path FROM nas_sources")?;
        let existing = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for (id, label, saved_platform, saved_native) in existing {
            if organize::decode_path(&saved_platform, &saved_native)? == path {
                return Ok(NasSource {
                    id,
                    label,
                    path: path.to_path_buf(),
                });
            }
        }
        let id = Uuid::new_v4().to_string();
        let label = NasSource {
            id: id.clone(),
            label: String::new(),
            path: path.to_path_buf(),
        }
        .browse_path()
        .to_string_lossy()
        .into_owned();
        self.db.execute(
            "INSERT INTO nas_sources(id,platform,native_path,label) VALUES(?1,?2,?3,?4)",
            params![id, platform, native, label],
        )?;
        Ok(NasSource {
            id,
            label,
            path: path.to_path_buf(),
        })
    }

    pub fn nas_sources(&self) -> Result<Vec<NasSource>> {
        let mut statement = self
            .db
            .prepare("SELECT id,label,platform,native_path FROM nas_sources ORDER BY label")?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(id, label, platform, native)| {
                Ok(NasSource {
                    id,
                    label,
                    path: organize::decode_path(&platform, &native)?,
                })
            })
            .collect()
    }

    pub fn nas_download_plans(&self, asset: AssetId) -> Result<Vec<NasDownload>> {
        let mut statement = self.db.prepare(
            "SELECT v.id,v.sha256,v.bytes,v.format,f.platform,f.native_path \
            FROM asset_versions v JOIN assets a ON a.id=v.asset_id \
            JOIN file_locations f ON f.version_id=v.id JOIN nas_versions n ON n.version_id=v.id \
            WHERE v.asset_id=?1 AND a.deleted_at IS NULL ORDER BY (v.origin='original') DESC,v.id",
        )?;
        let rows = statement
            .query_map([asset.to_string()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, u64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Vec<u8>>(5)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(version, hash, bytes, extension, platform, native)| {
                let source = organize::decode_path(&platform, &native)?;
                reference_path(&self.db, &version, &source)?;
                let operation = Uuid::new_v4();
                Ok(NasDownload {
                    root: self.root.clone(),
                    source,
                    asset,
                    version,
                    hash,
                    bytes,
                    extension,
                    staged: safe_path(&self.root, &format!("staging/nas-{operation}.part"))?,
                    _lease: self._lock.clone(),
                })
            })
            .collect()
    }

    pub fn finish_nas_download(&mut self, plan: &NasDownload) -> Result<()> {
        if self.root != plan.root {
            return Err(Error::Invalid("下载所属资源库已切换".into()));
        }
        let (hash, platform, native): (String, String, Vec<u8>) = self.db.query_row(
            "SELECT v.sha256,f.platform,f.native_path FROM asset_versions v \
                JOIN file_locations f ON f.version_id=v.id JOIN nas_versions n ON n.version_id=v.id \
                WHERE v.id=?1 AND v.asset_id=?2",
            params![plan.version, plan.asset.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        if hash != plan.hash
            || organize::decode_path(&platform, &native)? != plan.source
            || fs::metadata(&plan.staged)?.len() != plan.bytes
        {
            return Err(Error::Invalid(
                "NAS 原片或下载结果已变化，请重新下载".into(),
            ));
        }
        let relative = format!("assets/nas/{}.{}", plan.version, plan.extension);
        let target = safe_path(&self.root, &relative)?;
        fs::create_dir_all(safe_path(&self.root, "assets/nas")?)?;
        let created = match fs::hard_link(&plan.staged, &target) {
            Ok(()) => true,
            Err(error)
                if error.kind() == std::io::ErrorKind::AlreadyExists
                    && fs::metadata(&target)?.len() == plan.bytes
                    && organize::hash_file(&target)? == plan.hash =>
            {
                false
            }
            Err(error) => return Err(error.into()),
        };
        let result = (|| {
            let tx = self.db.transaction()?;
            tx.execute(
                "UPDATE asset_versions SET path=?2 WHERE id=?1 AND asset_id=?3",
                params![plan.version, relative, plan.asset.to_string()],
            )?;
            tx.execute(
                "DELETE FROM nas_versions WHERE version_id=?1",
                [&plan.version],
            )?;
            tx.execute(
                "DELETE FROM file_locations WHERE version_id=?1",
                [&plan.version],
            )?;
            tx.commit()?;
            Ok(())
        })();
        if result.is_err() && created {
            let _ = fs::remove_file(target);
        } else if result.is_ok() {
            self.snapshot = None;
            let _ = fs::remove_file(&plan.staged);
        }
        result
    }
}

pub struct NasDownload {
    root: PathBuf,
    source: PathBuf,
    asset: AssetId,
    version: String,
    hash: String,
    bytes: u64,
    extension: String,
    staged: PathBuf,
    _lease: Arc<File>,
}
impl Drop for NasDownload {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_file(&self.staged)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(%error, path = %self.staged.display(), "NAS staging cleanup deferred");
        }
    }
}
impl NasDownload {
    pub fn discard(&self) {
        let _ = fs::remove_file(&self.staged);
    }
    /// Copy once to the library volume, then compare the registered content hash.
    pub fn copy(&self, cancel: &AtomicBool) -> Result<()> {
        if fs2::available_space(&self.root).is_ok_and(|free| free < self.bytes) {
            return Err(Error::Invalid("资源库所在磁盘空间不足".into()));
        }
        let mut input = File::open(&self.source)?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.staged)?;
        let result = (|| {
            let mut hasher = Sha256::new();
            let mut total = 0u64;
            let mut buffer = [0u8; 64 * 1024];
            loop {
                if cancel.load(Ordering::Relaxed) {
                    return Err(Error::Invalid("下载已取消".into()));
                }
                let count = input.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                total = total.saturating_add(count as u64);
                if total > self.bytes {
                    return Err(Error::Invalid("NAS 原片在下载期间发生变化".into()));
                }
                hasher.update(&buffer[..count]);
                output.write_all(&buffer[..count])?;
            }
            output.sync_all()?;
            if total != self.bytes || format!("{:x}", hasher.finalize()) != self.hash {
                return Err(Error::Invalid("NAS 原片与登记时不同，请重新扫描".into()));
            }
            Ok(())
        })();
        drop(output);
        if result.is_err() {
            let _ = fs::remove_file(&self.staged);
        }
        result
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn converting_remote_reference_preserves_asset_and_rejects_changed_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.png");
        image::RgbImage::from_pixel(8, 8, image::Rgb([1, 2, 3]))
            .save(&source)
            .unwrap();
        let original = fs::read(&source).unwrap();
        let mut library = Library::create(dir.path(), "nas-test").unwrap();
        assert_eq!(
            library
                .db
                .pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))
                .unwrap(),
            28
        );
        library.import_referenced(&source, None).unwrap();
        let asset = library
            .page(&Query::default(), 0, 1)
            .unwrap()
            .assets
            .remove(0);
        let id = Uuid::new_v4().to_string();
        let (platform, native) = organize::encode_path(dir.path()).unwrap();
        library
            .db
            .execute(
                "INSERT INTO nas_sources(id,platform,native_path,label) VALUES(?1,?2,?3,'fixture')",
                params![id, platform, native],
            )
            .unwrap();
        library
            .db
            .execute(
                "INSERT INTO nas_versions(version_id,source_id) VALUES(?1,?2)",
                params![asset.version_id.to_string(), id],
            )
            .unwrap();
        let plan = NasDownload {
            root: library.root.clone(),
            source: source.canonicalize().unwrap(),
            asset: asset.id,
            version: asset.version_id.to_string(),
            hash: organize::hash_file(&source).unwrap(),
            bytes: original.len() as u64,
            extension: "png".into(),
            staged: safe_path(&library.root, "staging/nas-fixture.part").unwrap(),
            _lease: library._lock.clone(),
        };
        fs::write(&source, b"changed").unwrap();
        assert!(plan.copy(&AtomicBool::new(false)).is_err());
        assert!(library.details(asset.id).unwrap().referenced);
        fs::write(&source, &original).unwrap();
        plan.copy(&AtomicBool::new(false)).unwrap();
        library.finish_nas_download(&plan).unwrap();
        assert_eq!(
            fs::read(library.asset_path(asset.id).unwrap()).unwrap(),
            original
        );
        assert!(!library.details(asset.id).unwrap().referenced);
        assert!(!library.details(asset.id).unwrap().nas);
        assert_eq!(
            library.page(&Query::default(), 0, 1).unwrap().assets[0].id,
            asset.id
        );
        assert!(source.exists());
    }

    #[cfg(windows)]
    #[test]
    fn only_unc_paths_are_explicit_nas_roots() {
        assert!(is_unc(Path::new(r"\\server\photos")));
        assert!(is_unc(Path::new(r"\\?\UNC\server\photos")));
        assert!(!is_unc(Path::new(r"Z:\photos")));
    }

    #[test]
    fn schema_27_library_migrates_nas_tables_on_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let library = Library::create(dir.path(), "upgrade").unwrap();
        library
            .db
            .execute_batch(
                "DROP TABLE nas_versions; DROP TABLE nas_sources; PRAGMA user_version=27;",
            )
            .unwrap();
        let root = library.root.clone();
        drop(library);
        let reopened = Library::open(&root).unwrap();
        assert_eq!(
            reopened
                .db
                .pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))
                .unwrap(),
            28
        );
        assert!(reopened.nas_sources().unwrap().is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn offline_nas_preview_uses_cached_derivative_without_opening_share() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("photo.png");
        image::RgbImage::from_pixel(8, 8, image::Rgb([4, 5, 6]))
            .save(&source)
            .unwrap();
        let mut library = Library::create(dir.path(), "offline").unwrap();
        library.import_referenced(&source, None).unwrap();
        let asset = library
            .page(&Query::default(), 0, 1)
            .unwrap()
            .assets
            .remove(0);
        let root = Path::new(r"\\offline.invalid\images");
        let remote = root.join("photo.png");
        let (platform, native) = organize::encode_path(root).unwrap();
        let id = Uuid::new_v4().to_string();
        library
            .db
            .execute(
                "INSERT INTO nas_sources(id,platform,native_path,label) VALUES(?1,?2,?3,'offline')",
                params![id, platform, native],
            )
            .unwrap();
        let (platform, native) = organize::encode_path(&remote).unwrap();
        library
            .db
            .execute(
                "UPDATE file_locations SET platform=?1,native_path=?2 WHERE version_id=?3",
                params![platform, native, asset.version_id.to_string()],
            )
            .unwrap();
        library
            .db
            .execute(
                "INSERT INTO nas_versions(version_id,source_id) VALUES(?1,?2)",
                params![asset.version_id.to_string(), id],
            )
            .unwrap();
        let cache = library.nas_preview_path(asset.id, None).unwrap().unwrap();
        image::RgbImage::from_pixel(8, 8, image::Rgb([4, 5, 6]))
            .save(&cache)
            .unwrap();
        let plan = library
            .version_source_plan(asset.id, asset.version_id)
            .unwrap();
        assert!(plan.cached_preview().is_some());
        assert!(crate::media::Source::read_cached_preview(&cache).is_some());
    }
}
