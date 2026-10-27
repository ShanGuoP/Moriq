//! Explicit whole-library removal. Referenced originals outside the root are never traversed.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub root: PathBuf,
    pub id: LibraryId,
    pub name: String,
    pub assets: u64,
}
/// 校验待移除路径的文件类型，拒绝链接等不适合直接删除的对象。
fn regular(path: &Path) -> Result<fs::Metadata> {
    let meta = fs::symlink_metadata(path)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return Err(Error::Invalid(
                "删除范围包含链接或重解析点，已停止操作".into(),
            ));
        }
    }
    if meta.file_type().is_symlink() {
        return Err(Error::Invalid("删除范围包含链接，已停止操作".into()));
    }
    Ok(meta)
}
/// 检查资源库身份、目录内容与删除范围，生成可供确认的移除计划。
pub fn inspect(path: &Path) -> Result<Plan> {
    if !path.is_absolute() {
        return Err(Error::Invalid("资源库路径必须为绝对路径".into()));
    }
    for ancestor in path.ancestors() {
        regular(ancestor)?;
    }
    let root = path.canonicalize()?;
    if root.parent().is_none()
        || !root
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("moriq"))
    {
        return Err(Error::Invalid("只允许删除完整的 .moriq 资源库目录".into()));
    }
    let manifest_path = root.join("library.toml");
    if regular(&manifest_path)?.len() > 65536 {
        return Err(Error::Invalid("资源库清单过大".into()));
    }
    let manifest: Manifest = toml::from_str(&fs::read_to_string(manifest_path)?)
        .map_err(|_| Error::Invalid("资源库清单损坏，拒绝删除".into()))?;
    if manifest.format != "moriq" {
        return Err(Error::Invalid("不是 Moriq 资源库".into()));
    }
    regular(&root.join("catalog.db"))?;
    let db = Connection::open_with_flags(
        root.join("catalog.db"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let identity: String = db.query_row("SELECT id FROM library_identity", [], |r| r.get(0))?;
    if identity != manifest.library_id.to_string() {
        return Err(Error::Invalid("资源库身份不一致，拒绝删除".into()));
    }
    let name = db
        .query_row(
            "SELECT value FROM library_preferences WHERE key='display_name'",
            [],
            |r| r.get::<_, String>(0),
        )
        .optional()?
        .unwrap_or(manifest.name);
    let assets = db.query_row("SELECT count(*) FROM assets", [], |r| r.get(0))?;
    Ok(Plan {
        root,
        id: manifest.library_id,
        name,
        assets,
    })
}
/// 按已确认的计划移除受管资源库内容，校验锁和身份并保留外部引用原片。
pub fn execute(plan: &Plan) -> Result<()> {
    if inspect(&plan.root)? != *plan {
        return Err(Error::Invalid(
            "资源库身份、名称或素材数已改变，请重新确认".into(),
        ));
    }
    let lock_path = plan.root.join("library.lock");
    regular(&lock_path)?;
    let lock = OpenOptions::new().read(true).write(true).open(&lock_path)?;
    lock.try_lock_exclusive().map_err(|_| Error::Locked)?;
    if inspect(&plan.root)? != *plan {
        return Err(Error::Invalid("资源库已改变，请重新确认".into()));
    }
    let mut pending = vec![plan.root.clone()];
    while let Some(path) = pending.pop() {
        if regular(&path)?.is_dir() {
            if path != plan.root && path.join("library.toml").exists() {
                return Err(Error::Invalid(
                    "资源库内包含另一个资源库，请先将其移出".into(),
                ));
            }
            for entry in fs::read_dir(path)? {
                pending.push(entry?.path());
            }
        }
    }
    // Rename inside the same parent before removing anything; a failed cleanup retains a named recovery path.
    let parent = plan
        .root
        .parent()
        .ok_or_else(|| Error::Invalid("资源库父路径无效".into()))?;
    let staged = parent.join(format!(".moriq-deleting-{}", Uuid::new_v4()));
    if staged.exists() || staged.parent() != Some(parent) {
        return Err(Error::Invalid("删除暂存路径无效".into()));
    }
    let marker = plan.root.join("metadata/library-deletion.pending");
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&marker)
    {
        Ok(mut file) => {
            if let Err(error) = file
                .write_all(plan.id.to_string().as_bytes())
                .and_then(|_| file.sync_all())
            {
                drop(file);
                let _ = fs::remove_file(&marker);
                return Err(error.into());
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            if fs::read_to_string(&marker)? != plan.id.to_string() {
                return Err(Error::Invalid("删除标记身份不匹配".into()));
            }
        }
        Err(error) => return Err(error.into()),
    }
    // Windows cannot rename a directory containing a locked file. The durable marker fences reopen before releasing it.
    drop(lock);
    if let Err(error) = fs::rename(&plan.root, &staged) {
        let _ = fs::remove_file(&marker);
        return Err(error.into());
    }
    if let Err(error) = fs::remove_dir_all(&staged) {
        let restored = fs::rename(&staged, &plan.root).is_ok();
        return Err(Error::Invalid(format!(
            "删除未完成：{error}；剩余文件位于 {}",
            if restored { &plan.root } else { &staged }.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    /// 验证资源库删除受锁和身份校验保护，并保留引用原片。
    fn deletion_is_locked_identity_checked_and_preserves_referenced_originals() {
        let dir = tempfile::tempdir().unwrap();
        let outside = dir.path().join("original.txt");
        fs::write(&outside, b"preserve").unwrap();
        let lib = Library::create(dir.path(), "delete-test").unwrap();
        let root = lib.root().to_path_buf();
        let plan = inspect(&root).unwrap();
        assert!(matches!(execute(&plan), Err(Error::Locked)));
        drop(lib);
        let mut wrong = plan.clone();
        wrong.id = LibraryId::default();
        assert!(execute(&wrong).is_err());
        assert!(root.exists());
        execute(&plan).unwrap();
        assert!(!root.exists());
        assert_eq!(fs::read(outside).unwrap(), b"preserve");
    }
    #[test]
    /// 验证移除流程拒绝嵌套资源库和非资源库根目录。
    fn removal_rejects_nested_libraries_and_non_library_roots() {
        let dir = tempfile::tempdir().unwrap();
        assert!(inspect(dir.path()).is_err());
        let lib = Library::create(dir.path(), "outer").unwrap();
        let root = lib.root().to_path_buf();
        drop(lib);
        let child = Library::create(&root, "inner").unwrap();
        drop(child);
        let plan = inspect(&root).unwrap();
        assert!(execute(&plan).is_err());
        assert!(root.exists());
    }
    #[test]
    /// 验证中断删除标记阻止误打开，重新确认后可重试。
    fn interrupted_deletion_marker_blocks_reopen_but_can_be_retried_after_confirmation() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::create(dir.path(), "interrupted").unwrap();
        let plan = inspect(lib.root()).unwrap();
        drop(lib);
        fs::write(
            plan.root.join("metadata/library-deletion.pending"),
            plan.id.to_string(),
        )
        .unwrap();
        assert!(Library::open(&plan.root).is_err());
        execute(&plan).unwrap();
        assert!(!plan.root.exists());
    }
}
