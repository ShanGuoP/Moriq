//! Freeze immutable file links with the catalog, then copy bytes off the writer.
use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

#[derive(Clone, Debug, Default)]
pub struct Control {
    pub cancel: Arc<AtomicBool>,
    pub finished: Arc<AtomicUsize>,
    pub total: Arc<AtomicUsize>,
}

pub struct Plan {
    frozen: tempfile::TempDir,
    parent: PathBuf,
    name: String,
    _lease: Arc<File>,
}

pub struct Prepared {
    directory: tempfile::TempDir,
    destination: PathBuf,
    control: Control,
}

impl Library {
    /// Only the catalog snapshot and file names are frozen on the writer.
    /// Managed originals are immutable and the library requires hard-link support.
    /// Private links keep their bytes alive even if the source is purged later.
    pub fn backup_plan(&self, parent: &Path, independent: bool) -> Result<Plan> {
        self.backup_plan_controlled(parent, independent, &Control::default())
    }

    pub fn backup_plan_controlled(
        &self,
        parent: &Path,
        independent: bool,
        control: &Control,
    ) -> Result<Plan> {
        let _scope = cancel::Scope::enter(control.cancel.clone());
        cancel::check()?;
        let parent = parent.canonicalize()?;
        if parent.starts_with(&self.root) {
            return Err(Error::Invalid(
                "请选择资源库外的备份位置，建议另一块磁盘".into(),
            ));
        }
        let frozen = tempfile::Builder::new()
            .prefix(".snapshot-")
            .tempdir_in(safe_path(&self.root, "backups")?)?;
        let database = frozen.path().join("catalog.db");
        self.db.backup(rusqlite::MAIN_DB, &database, None)?;
        let mut db = Connection::open(&database)?;
        // Never copy a database whose committed changes are only in its WAL.
        db.execute_batch("PRAGMA journal_mode=DELETE;")?;
        let tx = db.transaction()?;
        // A running import has no committed asset yet and its stage is mutable.
        // Interrupted imports have stopped; retain those journals and their files.
        tx.execute("DELETE FROM import_context WHERE operation_id IN(SELECT id FROM operation_journal WHERE phase<>'interrupted')", [])?;
        tx.execute(
            "DELETE FROM operation_journal WHERE phase<>'interrupted'",
            [],
        )?;
        let mut manifest = self.manifest.clone();
        if independent {
            manifest.library_id = LibraryId::default();
            manifest.name = format!("{}-克隆", self.info()?.name);
            tx.execute(
                "UPDATE library_identity SET id=?1",
                [manifest.library_id.to_string()],
            )?;
            tx.execute("DELETE FROM synco_tasks", [])?;
            tx.execute("DELETE FROM import_context", [])?;
            tx.execute("DELETE FROM operation_journal", [])?;
            tx.execute("INSERT INTO library_preferences VALUES('display_name',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [&manifest.name])?;
            tx.execute("UPDATE notifications SET retry='' WHERE retry<>''", [])?;
            tx.execute("UPDATE background_jobs SET state='paused',inflight=0,generation=generation+1 WHERE state IN('running','queued')", [])?;
        }
        tx.commit()?;
        let mut output = File::create(frozen.path().join("library.toml"))?;
        output.write_all(
            toml::to_string_pretty(&manifest)
                .map_err(|e| Error::Invalid(e.to_string()))?
                .as_bytes(),
        )?;
        output.sync_all()?;
        for directory in DIRECTORIES {
            cancel::check()?;
            fs::create_dir_all(frozen.path().join(directory))?;
            if directory.starts_with("thumbnails")
                || matches!(
                    *directory,
                    "previews" | "backups" | "indexes/search" | "indexes/layout" | "staging"
                )
                || (independent && directory.starts_with("interop/"))
            {
                continue;
            }
            link_tree(&self.root, Path::new(directory), frozen.path())?;
        }
        if !independent {
            let mut statement =
                db.prepare("SELECT id FROM operation_journal WHERE phase='interrupted'")?;
            for row in statement.query_map([], |r| r.get::<_, String>(0))? {
                let operation = row?;
                if Uuid::parse_str(&operation).is_err() {
                    return Err(Error::Invalid("导入恢复记录身份无效".into()));
                }
                for suffix in ["part", "thumb", "ready"] {
                    let relative = format!("staging/{operation}.{suffix}");
                    let source = safe_path(&self.root, &relative)?;
                    if source.is_file() {
                        fs::hard_link(source, frozen.path().join(relative))?;
                    }
                }
            }
        }
        drop(db);
        OpenOptions::new()
            .read(true)
            .write(true)
            .open(database)?
            .sync_all()?;
        Ok(Plan {
            frozen,
            parent,
            name: self.info()?.name,
            _lease: self._lock.clone(),
        })
    }
}

impl Plan {
    pub fn copy(self, control: &Control) -> Result<Prepared> {
        self.copy_with_progress(control, |_| {})
    }

    pub fn copy_with_progress(
        self,
        control: &Control,
        mut progress: impl FnMut(usize),
    ) -> Result<Prepared> {
        let _scope = cancel::Scope::enter(control.cancel.clone());
        cancel::check()?;
        let directory = tempfile::Builder::new()
            .prefix(".moriq-backup-")
            .tempdir_in(&self.parent)?;
        // Count and copy with depth-bounded traversal instead of collecting an
        // entire large library's file list in Rust memory.
        let mut total = 0usize;
        walk_files(self.frozen.path(), Path::new(""), &mut |_, directory| {
            total += usize::from(!directory);
            Ok(())
        })?;
        control.total.store(total, Ordering::Release);
        let mut finished = 0usize;
        walk_files(
            self.frozen.path(),
            Path::new(""),
            &mut |relative, is_directory| {
                if is_directory {
                    fs::create_dir_all(directory.path().join(relative))?;
                } else {
                    organize::copy_owned(
                        &self.frozen.path().join(relative),
                        &directory.path().join(relative),
                    )?;
                    finished += 1;
                    control.finished.store(finished, Ordering::Release);
                    progress(finished);
                }
                Ok(())
            },
        )?;
        cancel::check()?;
        // Check owned originals against the frozen catalog before publication.
        let db = Connection::open(directory.path().join("catalog.db"))?;
        let mut statement = db.prepare("SELECT v.path,v.sha256 FROM asset_versions v WHERE NOT EXISTS(SELECT 1 FROM file_locations f WHERE f.version_id=v.id)")?;
        for row in
            statement.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        {
            cancel::check()?;
            let (path, expected) = row?;
            let original = safe_path(directory.path(), &path)?;
            // Keep already-missing catalog entries recoverable, as in the source.
            if original.is_file() && organize::hash_file(&original)? != expected {
                return Err(Error::Invalid("备份中的原片校验不符，未发布副本".into()));
            }
        }
        drop(statement);
        drop(db);
        let destination = self
            .parent
            .join(format!("{}-副本-{}.moriq", self.name, Uuid::new_v4()));
        Ok(Prepared {
            directory,
            destination,
            control: control.clone(),
        })
    }
}

impl Prepared {
    /// The app checks the library identity on its writer before calling this.
    pub fn publish(self) -> Result<PathBuf> {
        if self.control.cancel.load(Ordering::Acquire) {
            return Err(Error::Invalid("备份已停止，未发布副本".into()));
        }
        fs::rename(self.directory.path(), &self.destination)?;
        Ok(self.destination)
    }
}

fn walk_files(
    root: &Path,
    relative: &Path,
    visit: &mut impl FnMut(&Path, bool) -> Result<()>,
) -> Result<()> {
    if relative.components().count() > 128 {
        return Err(Error::Invalid("备份目录层级过深".into()));
    }
    cancel::check()?;
    for entry in fs::read_dir(root.join(relative))? {
        cancel::check()?;
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_symlink() || !(kind.is_file() || kind.is_dir()) {
            return Err(Error::Invalid("备份目录包含不支持的文件类型".into()));
        }
        let next = relative.join(entry.file_name());
        visit(&next, kind.is_dir())?;
        if kind.is_dir() {
            walk_files(root, &next, visit)?;
        }
    }
    Ok(())
}

fn link_tree(root: &Path, relative: &Path, destination: &Path) -> Result<()> {
    cancel::check()?;
    if relative.components().count() > 128 {
        return Err(Error::Invalid("备份目录层级过深".into()));
    }
    let text = relative
        .to_str()
        .ok_or_else(|| Error::Invalid("库内路径无效".into()))?
        .replace('\\', "/");
    let source = safe_path(root, &text)?;
    if !source.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(source)? {
        cancel::check()?;
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            return Err(Error::Invalid(
                "备份发现符号链接，已停止。请先检查资源库完整性".into(),
            ));
        }
        let next = relative.join(entry.file_name());
        if kind.is_dir() {
            fs::create_dir_all(destination.join(&next))?;
            link_tree(root, &next, destination)?;
        } else if kind.is_file() {
            let text = next
                .to_str()
                .ok_or_else(|| Error::Invalid("库内路径无效".into()))?
                .replace('\\', "/");
            fs::hard_link(safe_path(root, &text)?, destination.join(next))?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn fixture() -> (tempfile::TempDir, Library, AssetId) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("original.png");
        image::RgbImage::from_pixel(24, 16, image::Rgb([30, 60, 90]))
            .save(&path)
            .unwrap();
        let mut library = Library::create(dir.path(), "备份快照").unwrap();
        library.import(&path, None).unwrap();
        let asset = library.page(&Query::default(), 0, 1).unwrap().assets[0].id;
        (dir, library, asset)
    }

    #[test]
    fn frozen_links_survive_source_purge_and_later_edits_are_not_in_the_backup() {
        let (dir, mut library, asset) = fixture();
        library
            .edit(&[asset], &Edit::Notes("快照前".into()))
            .unwrap();
        let plan = library.backup_plan(dir.path(), false).unwrap();
        library
            .edit(&[asset], &Edit::Notes("快照后".into()))
            .unwrap();
        library.edit(&[asset], &Edit::Trash(true)).unwrap();
        assert_eq!(library.purge(&[asset]).unwrap().deleted, 1);
        let new_file = dir.path().join("new.png");
        image::RgbImage::from_pixel(2, 2, image::Rgb([8, 7, 6]))
            .save(&new_file)
            .unwrap();
        library.import(&new_file, None).unwrap();
        let copied = plan
            .copy_with_progress(&Control::default(), |_| {
                // Catalog work remains available even while the media copier is busy.
                assert_eq!(library.info().unwrap().count, 1);
            })
            .unwrap()
            .publish()
            .unwrap();
        let mut restored = Library::open(&copied).unwrap();
        assert_eq!(restored.id(), library.id());
        assert_eq!(restored.info().unwrap().count, 1);
        assert_eq!(
            restored.page(&Query::default(), 0, 2).unwrap().assets[0].id,
            asset
        );
        assert_eq!(restored.details(asset).unwrap().notes, "快照前");
        assert_eq!(restored.maintenance(false).unwrap().damaged, 0);
        assert_eq!(
            fs::read_dir(library.root.join("backups")).unwrap().count(),
            0
        );
    }

    #[test]
    fn cancellation_and_corruption_never_publish_partial_copies_and_clone_changes_identity() {
        let (dir, library, asset) = fixture();
        let plan = library.backup_plan(dir.path(), false).unwrap();
        let control = Control::default();
        assert!(
            plan.copy_with_progress(&control, |_| control.cancel.store(true, Ordering::Release))
                .is_err()
        );
        assert_eq!(
            fs::read_dir(library.root.join("backups")).unwrap().count(),
            0
        );
        assert!(!fs::read_dir(dir.path()).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".moriq-backup-")
        }));
        let plan = library.backup_plan(dir.path(), false).unwrap();
        fs::write(library.asset_path(asset).unwrap(), b"changed in place").unwrap();
        assert!(plan.copy(&Control::default()).is_err());
        assert_eq!(
            fs::read_dir(library.root.join("backups")).unwrap().count(),
            0
        );
        // A separate intact source is used for the independent identity contract.
        let (other, library, _) = fixture();
        let clone = library.clone_library(other.path()).unwrap();
        let cloned = Library::open(&clone).unwrap();
        assert_ne!(cloned.id(), library.id());
        assert_eq!(cloned.info().unwrap().count, 1);
    }

    #[test]
    fn active_import_stages_are_excluded_but_interrupted_imports_remain_recoverable() {
        let (dir, mut library, _) = fixture();
        let source = dir.path().join("pending.png");
        image::RgbImage::from_pixel(4, 3, image::Rgb([90, 80, 70]))
            .save(&source)
            .unwrap();
        let file = ImportFile {
            project_name: None,
            path: source,
            relative: PathBuf::from("pending.png"),
        };
        let ticket = library.begin_import(&file, None, false).unwrap();
        ticket.copy(&AtomicBool::new(false)).unwrap();
        let active_copy = library.backup(dir.path()).unwrap();
        let active = Library::open(&active_copy).unwrap();
        assert_eq!(active.info().unwrap().count, 1);
        assert_eq!(active.organizer().unwrap().interrupted, 0);
        assert_eq!(
            fs::read_dir(active.root.join("staging")).unwrap().count(),
            0
        );
        // Snapshot cleanup must never mutate the source journal or stage.
        assert_eq!(
            library
                .db
                .query_row(
                    "SELECT count(*) FROM operation_journal WHERE phase='prepared'",
                    [],
                    |r| r.get::<_, u64>(0)
                )
                .unwrap(),
            1
        );
        library.interrupt_import(&ticket).unwrap();
        let interrupted_copy = library.backup(dir.path()).unwrap();
        let mut interrupted = Library::open(&interrupted_copy).unwrap();
        assert_eq!(interrupted.organizer().unwrap().interrupted, 1);
        assert_eq!(interrupted.recover_imports().unwrap().imported, 1);
        assert_eq!(interrupted.info().unwrap().count, 2);
        assert_eq!(library.info().unwrap().count, 1);
        assert_eq!(library.organizer().unwrap().interrupted, 1);
    }
}
