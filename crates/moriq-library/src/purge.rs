//! Durable two-phase removal: uncommitted file moves roll back; committed files are unlinked.
use super::*;
use std::collections::BTreeSet;

fn owned_path(root: &Path, relative: &str) -> Result<PathBuf> {
    let import_stage = relative.strip_prefix("staging/").is_some_and(|name| {
        name.strip_suffix(".part")
            .or_else(|| name.strip_suffix(".thumb"))
            .is_some_and(|id| Uuid::parse_str(id).is_ok())
    });
    if !import_stage
        && ![
            "assets/",
            "versions/",
            "thumbnails/256/",
            "previews/",
            "interop/outgoing/",
        ]
        .iter()
        .any(|prefix| relative.starts_with(prefix))
    {
        return Err(Error::Invalid(
            "删除路径不属于素材文件区域，已停止操作".into(),
        ));
    }
    no_links_path(root, relative)
}

fn no_links_path(root: &Path, relative: &str) -> Result<PathBuf> {
    let resolved = safe_path(root, relative)?;
    // Never follow an injected link even if it points to a different file inside this library.
    let mut path = root.to_path_buf();
    for component in Path::new(relative).components() {
        path.push(component);
        if fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return Err(Error::Invalid("删除路径包含链接，已停止操作".into()));
        }
    }
    Ok(resolved)
}

impl Library {
    pub fn trash_snapshot(&self) -> Result<Vec<AssetId>> {
        self.db
            .prepare("SELECT id FROM assets WHERE deleted_at IS NOT NULL ORDER BY id")?
            .query_map([], |r| r.get::<_, String>(0))?
            .map(|r| Ok(AssetId(parse_id(&r?)?)))
            .collect()
    }
    pub fn purge_trash_chunk(&mut self, ids: &[AssetId]) -> Result<PurgeReport> {
        let mut current = Vec::new();
        for id in ids {
            let trashed: bool = self.db.query_row(
                "SELECT EXISTS(SELECT 1 FROM assets WHERE id=?1 AND deleted_at IS NOT NULL)",
                [id.to_string()],
                |r| r.get(0),
            )?;
            if trashed {
                current.push(*id)
            }
        }
        if current.is_empty() {
            return Ok(PurgeReport::default());
        }
        self.purge(&current)
    }
    /// Only assets already in the trash can be removed. The caller freezes the confirmed IDs.
    pub fn purge(&mut self, ids: &[AssetId]) -> Result<PurgeReport> {
        if ids.is_empty() || ids.len() > 10000 {
            return Err(Error::Invalid("每批请选择 1～10000 项永久删除".into()));
        }
        self.recover_purges()?;
        self.run_purge(ids, None)
    }
    pub(super) fn run_purge(
        &mut self,
        ids: &[AssetId],
        project: Option<&ProjectDeletionPlan>,
    ) -> Result<PurgeReport> {
        let pending: bool =
            self.db
                .query_row("SELECT EXISTS(SELECT 1 FROM purge_jobs)", [], |r| r.get(0))?;
        if pending {
            return Err(Error::Invalid(
                "存在尚未完成的删除恢复，请先检查通知和备份".into(),
            ));
        }
        let (job, files) = self.prepare_purge_for(ids, project)?;
        let commit = (|| {
            for (source, target) in &files {
                let source = owned_path(&self.root, source)?;
                let target = no_links_path(&self.root, target)?;
                if target.exists() {
                    return Err(Error::Invalid("删除暂存位置已存在，已停止操作".into()));
                }
                fs::rename(source, target)?;
            }
            self.commit_purge_for(&job, ids, project)?;
            Ok::<(), Error>(())
        })();
        if let Err(error) = commit {
            if let Err(recovery) = self.recover_purges() {
                return Err(Error::Invalid(format!(
                    "删除未提交：{error}；文件恢复尚未完成：{recovery}。请保留资源库并重新打开以继续恢复。"
                )));
            }
            return Err(error);
        }
        self.snapshot = None;
        let mut report = PurgeReport {
            deleted: ids
                .iter()
                .map(ToString::to_string)
                .collect::<BTreeSet<_>>()
                .len(),
            cleanup_pending: false,
        };
        if let Err(error) = self.recover_purge(&job, true) {
            tracing::warn!(%error, "committed purge cleanup deferred");
            report.cleanup_pending = true;
        }
        Ok(report)
    }

    #[cfg(test)]
    fn commit_purge(&mut self, job: &str, ids: &[AssetId]) -> Result<()> {
        self.commit_purge_for(job, ids, None)
    }
    fn commit_purge_for(
        &mut self,
        job: &str,
        ids: &[AssetId],
        project: Option<&ProjectDeletionPlan>,
    ) -> Result<()> {
        let tx = self.db.transaction()?;
        for id in ids.iter().map(ToString::to_string).collect::<BTreeSet<_>>() {
            tx.execute("DELETE FROM synco_tasks WHERE asset_id=?1", [&id])?;
            tx.execute("DELETE FROM project_files WHERE version_id IN(SELECT id FROM asset_versions WHERE asset_id=?1)", [&id])?;
            tx.execute("DELETE FROM file_locations WHERE version_id IN(SELECT id FROM asset_versions WHERE asset_id=?1)", [&id])?;
            tx.execute("DELETE FROM project_assets WHERE asset_id=?1", [&id])?;
            tx.execute("DELETE FROM asset_tags WHERE asset_id=?1", [&id])?;
            tx.execute("DELETE FROM asset_order WHERE asset_id=?1", [&id])?;
            tx.execute(
                "UPDATE asset_versions SET parent_version_id=NULL WHERE asset_id=?1",
                [&id],
            )?;
            tx.execute("DELETE FROM asset_versions WHERE asset_id=?1", [&id])?;
            tx.execute("DELETE FROM assets WHERE id=?1", [&id])?;
        }
        if let Some(project) = project {
            for id in &project.projects {
                // Drop recovery context before the project FK can detach it and make
                // a later recovery import the deleted content into another project.
                let operations = tx
                    .prepare("SELECT operation_id FROM import_context WHERE project_id=?1")?
                    .query_map([id.to_string()], |r| r.get::<_, String>(0))?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                tx.execute(
                    "DELETE FROM import_context WHERE project_id=?1",
                    [id.to_string()],
                )?;
                for operation in operations {
                    tx.execute("DELETE FROM operation_journal WHERE id=?1", [operation])?;
                }
                tx.execute(
                    "DELETE FROM project_assets WHERE project_id=?1",
                    [id.to_string()],
                )?;
                tx.execute("DELETE FROM projects WHERE id=?1", [id.to_string()])?;
            }
            tx.execute_batch("DELETE FROM asset_order; INSERT INTO asset_order SELECT row_number() OVER(ORDER BY imported_at,id)-1,id FROM assets WHERE deleted_at IS NULL;")?;
        }
        tx.execute("UPDATE purge_jobs SET committed=1 WHERE id=?1", [job])?;
        tx.commit()?;
        Ok(())
    }
    #[cfg(test)]
    fn prepare_purge(&mut self, ids: &[AssetId]) -> Result<(String, Vec<(String, String)>)> {
        self.prepare_purge_for(ids, None)
    }
    fn prepare_purge_for(
        &mut self,
        ids: &[AssetId],
        project: Option<&ProjectDeletionPlan>,
    ) -> Result<(String, Vec<(String, String)>)> {
        let mut paths = BTreeSet::new();
        if let Some(project) = project {
            paths.extend(project.aliases.iter().cloned());
            for id in &project.projects {
                let operations = self.db.prepare("SELECT j.id,j.path,j.phase FROM operation_journal j JOIN import_context c ON c.operation_id=j.id WHERE c.project_id=?1")?
                    .query_map([id.to_string()], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?)))?
                    .collect::<std::result::Result<Vec<_>,_>>()?;
                for (operation, path, phase) in operations {
                    parse_id(&operation)?;
                    paths.insert(format!("staging/{operation}.part"));
                    paths.insert(format!("staging/{operation}.thumb"));
                    if phase != "committed" && !self.db.query_row("SELECT EXISTS(SELECT 1 FROM asset_versions WHERE path=?1 COLLATE NOCASE UNION ALL SELECT 1 FROM \
                project_files WHERE path=?1 COLLATE NOCASE)",
 [&path],
 |r| r.get::<_,
bool>(0))? {

                        paths.insert(path);
                    }
                }
            }
        }
        // Enumerate once rather than once per version in a large purge batch.
        let mut previews_by_version = std::collections::HashMap::<String, Vec<String>>::new();
        let previews = safe_path(&self.root, "previews")?;
        if previews.is_dir() {
            for entry in fs::read_dir(previews)? {
                let entry = entry?;
                let name = entry.file_name();
                let Some(name) = name.to_str() else { continue };
                if let Some(id) = name.get(..36)
                    && Uuid::parse_str(id).is_ok()
                    && name.as_bytes().get(36) == Some(&b'-')
                {
                    previews_by_version
                        .entry(id.to_owned())
                        .or_default()
                        .push(format!("previews/{name}"));
                }
            }
        }
        let selected = ids.iter().map(ToString::to_string).collect::<BTreeSet<_>>();
        for id in ids {
            let id = id.to_string();
            let deleted: Option<bool> = self
                .db
                .query_row(
                    "SELECT deleted_at IS NOT NULL FROM assets WHERE id=?1",
                    [&id],
                    |r| r.get(0),
                )
                .optional()?;
            if deleted.is_none() || (project.is_none() && deleted != Some(true)) {
                return Err(Error::Invalid(
                    "仅能永久删除回收站内的素材，请刷新后重试".into(),
                ));
            }
            let mut versions = self.db.prepare("SELECT v.id,v.path,EXISTS(SELECT 1 FROM file_locations f WHERE f.version_id=v.id) FROM asset_versions v WHERE v.asset_id=?1")?;
            for row in versions.query_map([&id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, bool>(2)?,
                ))
            })? {
                let (version, path, referenced) = row?;
                if !referenced {
                    if !["assets/", "versions/"]
                        .iter()
                        .any(|prefix| path.starts_with(prefix))
                    {
                        return Err(Error::Invalid(
                            "托管原片路径不属于素材区域，已停止删除".into(),
                        ));
                    }
                    paths.insert(path);
                }
                paths.insert(format!("thumbnails/256/{version}.png"));
                parse_id(&version)?;
                if let Some(previews) = previews_by_version.get(&version) {
                    paths.extend(previews.iter().cloned());
                }
            }
            paths.extend(self.db.prepare("SELECT p.path FROM project_files p JOIN asset_versions v ON v.id=p.version_id WHERE v.asset_id=?1")?.query_map([&id],
 |r|r.get::<_,
String>(0))?.collect::<std::result::Result<Vec<_>,
_>>()?);

            let mut outgoing = self.db.prepare("SELECT t.id,v.format FROM synco_tasks t JOIN asset_versions v ON v.id=t.sent_version_id WHERE t.asset_id=?1")?;
            for task in outgoing.query_map([&id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })? {
                let (task, format) = task?;
                parse_id(&task)?;
                if !["jpg", "png", "webp", "bmp"].contains(&format.as_str()) {
                    return Err(Error::Invalid("Synco 文件类型记录无效".into()));
                }
                paths.insert(format!("interop/outgoing/{task}.{format}"));
            }
        }
        let job = Uuid::new_v4().to_string();
        let mut files = Vec::new();
        let project_aliases: BTreeSet<&str> = project
            .into_iter()
            .flat_map(|p| p.aliases.iter().map(String::as_str))
            .collect();
        for source in paths {
            let project_alias = project_aliases.contains(source.as_str());
            let mut owners = self.db.prepare("SELECT asset_id FROM asset_versions WHERE path=?1 COLLATE NOCASE UNION SELECT v.asset_id FROM \
                project_files p JOIN asset_versions v ON v.id=p.version_id WHERE p.path=?1 COLLATE NOCASE")?;

            for owner in owners.query_map([&source], |r| r.get::<_, String>(0))? {
                if !selected.contains(&owner?) && !project_alias {
                    return Err(Error::Invalid(
                        "文件仍被其他素材使用，已停止永久删除".into(),
                    ));
                }
            }
            let path = owned_path(&self.root, &source)?;
            match fs::metadata(path) {
                Ok(metadata) if metadata.is_file() => {
                    files.push((source, format!("trash/purge-{job}/{}.part", files.len())))
                }
                Ok(_) => return Err(Error::Invalid("素材路径不是普通文件，已停止删除".into())),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        let directory = no_links_path(&self.root, &format!("trash/purge-{job}"))?;
        fs::create_dir_all(safe_path(&self.root, "trash")?)?;
        fs::create_dir(&directory)?;
        let tx = self.db.transaction()?;
        tx.execute("INSERT INTO purge_jobs(id) VALUES(?1)", [&job])?;
        for (source, target) in &files {
            tx.execute(
                "INSERT INTO purge_files VALUES(?1,?2,?3)",
                params![job, source, target],
            )?;
        }
        tx.commit()?;
        Ok((job, files))
    }

    pub(super) fn recover_purges(&mut self) -> Result<()> {
        let jobs = self
            .db
            .prepare("SELECT id,committed FROM purge_jobs ORDER BY id")?
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, bool>(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for (job, committed) in jobs {
            self.recover_purge(&job, committed)?;
        }
        Ok(())
    }

    fn recover_purge(&mut self, job: &str, committed: bool) -> Result<()> {
        parse_id(job)?;
        let files = self
            .db
            .prepare("SELECT source,target FROM purge_files WHERE job_id=?1 ORDER BY target")?
            .query_map([job], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for (source, target) in files {
            let prefix = format!("trash/purge-{job}/");
            let number = target
                .strip_prefix(&prefix)
                .and_then(|tail| tail.strip_suffix(".part"))
                .and_then(|part| part.parse::<usize>().ok());
            if number.is_none() {
                return Err(Error::Invalid("删除恢复记录路径无效".into()));
            }
            let source = owned_path(&self.root, &source)?;
            let target = no_links_path(&self.root, &target)?;
            if committed {
                match fs::remove_file(&target) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            } else if target.exists() {
                if source.exists() {
                    return Err(Error::Invalid(
                        "删除恢复时原位置已有文件，已保留双方文件并停止恢复".into(),
                    ));
                }
                if let Some(parent) = source.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::rename(target, source)?;
            } else if !source.exists() {
                return Err(Error::Invalid(
                    "删除恢复记录中的原文件与暂存文件均缺失，请从备份恢复".into(),
                ));
            }
        }
        let directory = no_links_path(&self.root, &format!("trash/purge-{job}"))?;
        match fs::remove_dir(directory) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            // Unknown files are not authorized for deletion. Retain them, but retire
            // the completed journal so they cannot block all later purge operations.
            Err(error) if error.kind() == std::io::ErrorKind::DirectoryNotEmpty => {
                tracing::warn!(%job, "unregistered purge files retained in trash");
            }
            Err(error) => return Err(error.into()),
        }
        self.db
            .execute("DELETE FROM purge_jobs WHERE id=?1", [job])?;
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn stray_files_do_not_block_committed_or_rolled_back_purges() {
        for committed in [false, true] {
            let (_dir, mut lib, assets) = fixture();
            let asset = &assets[0];
            lib.edit(&[asset.id], &Edit::Trash(true)).unwrap();
            let (job, files) = lib.prepare_purge(&[asset.id]).unwrap();
            for (source, target) in &files {
                fs::rename(lib.root.join(source), lib.root.join(target)).unwrap();
            }
            let stray = lib.root.join(format!("trash/purge-{job}/desktop.ini"));
            fs::write(&stray, b"unknown data must survive").unwrap();
            if committed {
                lib.commit_purge(&job, &[asset.id]).unwrap();
            }
            lib.recover_purges().unwrap();
            assert_eq!(fs::read(stray).unwrap(), b"unknown data must survive");
            assert!(
                !lib.db
                    .query_row("SELECT EXISTS(SELECT 1 FROM purge_jobs)", [], |r| r
                        .get::<_, bool>(0))
                    .unwrap()
            );
            let next = if committed { assets[1].id } else { asset.id };
            lib.edit(&[next], &Edit::Trash(true)).unwrap();
            assert_eq!(lib.purge(&[next]).unwrap().deleted, 1);
        }
    }
    #[test]
    fn project_delete_cleans_interrupted_imports_without_recovery_and_rolls_back_on_failure() {
        let (dir, mut library, assets) = fixture();
        let root = library.create_project("删除中断导入").unwrap();
        let child = library.create_child_project("子项目", Some(root)).unwrap();
        let other = library.create_project("保留导入").unwrap();
        let source = dir.path().join("外部原片.png");
        fs::write(&source, b"original source").unwrap();
        let (platform, native) = organize::encode_path(&source).unwrap();
        let shared_path = library.asset_path(assets[0].id).unwrap();
        let shared_bytes = fs::read(&shared_path).unwrap();
        let shared_relative = shared_path
            .strip_prefix(&library.root)
            .unwrap()
            .to_str()
            .unwrap()
            .replace('\\', "/");
        let mut deleted_files = Vec::new();
        let mut operations = Vec::new();
        for (project, phase, shared) in [
            (root, "interrupted", false),
            (child, "prepared", false),
            (child, "interrupted", true),
            (root, "committed", true),
            (other, "interrupted", false),
        ] {
            let op = Uuid::new_v4().to_string();
            let path = if shared {
                shared_relative.clone()
            } else {
                format!("assets/{op}.png")
            };
            if !shared {
                fs::write(library.root.join(&path), b"installed recovery copy").unwrap();
            }
            library
                .db
                .execute(
                    "INSERT INTO operation_journal VALUES(?1,?2,?3)",
                    params![op, path, phase],
                )
                .unwrap();
            library.db.execute("INSERT INTO import_context(operation_id,platform,source,project_id,referenced) VALUES(?1,?2,?3,?4,0)",
 params![op,
platform,
native,
project.to_string()]).unwrap();

            for suffix in ["part", "thumb"] {
                let staged = library.root.join(format!("staging/{op}.{suffix}"));
                fs::write(&staged, b"recovery staging").unwrap();
                if project != other {
                    deleted_files.push(staged);
                }
            }
            if project != other {
                if !shared {
                    deleted_files.push(library.root.join(path));
                }
                operations.push(op);
            }
        }
        let plan = library.project_deletion_plan(root).unwrap();
        library.db.execute_batch("CREATE TRIGGER fail_project_delete BEFORE DELETE ON projects BEGIN SELECT RAISE(ABORT,'injected failure'); END;").unwrap();
        assert!(library.delete_project_contents(&plan).is_err());
        for file in &deleted_files {
            assert!(file.exists());
        }
        for op in &operations {
            assert!(
                library
                    .db
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM import_context WHERE operation_id=?1)",
                        [op],
                        |r| r.get::<_, bool>(0)
                    )
                    .unwrap()
            );
        }
        library
            .db
            .execute_batch("DROP TRIGGER fail_project_delete;")
            .unwrap();
        library.delete_project_contents(&plan).unwrap();
        for file in deleted_files {
            assert!(!file.exists());
        }
        for op in operations {
            assert!(
                !library
                    .db
                    .query_row(
                        "SELECT EXISTS(SELECT 1 FROM operation_journal WHERE id=?1)",
                        [op],
                        |r| r.get::<_, bool>(0)
                    )
                    .unwrap()
            );
        }
        assert_eq!(fs::read(&source).unwrap(), b"original source");
        assert_eq!(fs::read(&shared_path).unwrap(), shared_bytes);
        let library_root = library.root.clone();
        drop(library);
        let mut library = Library::open(&library_root).unwrap();
        assert_eq!(
            library
                .info()
                .unwrap()
                .projects
                .iter()
                .map(|p| p.id)
                .collect::<Vec<_>>(),
            vec![other]
        );
        assert_eq!(
            library
                .db
                .query_row(
                    "SELECT count(*) FROM operation_journal WHERE phase='interrupted'",
                    [],
                    |r| r.get::<_, u64>(0)
                )
                .unwrap(),
            1
        );
        let before = library.info().unwrap().count;
        let _ = library.recover_imports().unwrap();
        assert_eq!(library.info().unwrap().count, before);
    }
    #[test]
    fn project_delete_removes_exclusive_content_and_aliases_but_preserves_shared_and_reference_sources()
     {
        let (dir, mut library, assets) = fixture();
        let root = library.create_project("删除父项目").unwrap();
        let child = library
            .create_child_project("删除子项目", Some(root))
            .unwrap();
        let other = library.create_project("保留项目").unwrap();
        let exclusive = assets[0].id;
        let shared = assets[1].id;
        let exclusive_path = library.asset_path(exclusive).unwrap();
        let shared_path = library.asset_path(shared).unwrap();
        library.import(&exclusive_path, Some(root)).unwrap();
        library.import(&shared_path, Some(child)).unwrap();
        // Retain defensive purge coverage for legacy shared records.
        library
            .db
            .execute(
                "INSERT INTO project_assets VALUES(?1,?2)",
                params![other.to_string(), shared.to_string()],
            )
            .unwrap();
        library.set_tags(exclusive, &["待删除测试".into()]).unwrap();
        let external = dir.path().join("用户原片.png");
        image::RgbImage::from_pixel(3, 3, image::Rgb([9, 17, 26]))
            .save(&external)
            .unwrap();
        let bytes = fs::read(&external).unwrap();
        library.import_referenced(&external, Some(child)).unwrap();
        let plan = library.project_deletion_plan(root).unwrap();
        assert_eq!(plan.projects.len(), 2);
        assert_eq!(plan.assets.len(), 2);
        assert_eq!(plan.shared, vec![shared]);
        assert_eq!(plan.referenced, 1);
        let aliases = plan.aliases.clone();
        assert!(!aliases.is_empty());
        assert_eq!(library.delete_project_contents(&plan).unwrap().deleted, 2);
        assert!(!exclusive_path.exists());
        for alias in aliases {
            assert!(!library.root.join(alias).exists());
        }
        assert!(shared_path.is_file());
        assert_eq!(fs::read(&external).unwrap(), bytes);
        assert_eq!(library.details(shared).unwrap().projects, vec![other]);
        assert!(library.details(exclusive).is_err());
        let info = library.info().unwrap();
        assert_eq!(info.projects.len(), 1);
        assert_eq!(info.count, 2);
        assert_eq!(
            library
                .page(&Query::default(), 0, 128)
                .unwrap()
                .assets
                .len(),
            2
        );
        assert_eq!(
            library
                .page(
                    &Query {
                        text: "待删除测试".into(),
                        ..Default::default()
                    },
                    0,
                    1
                )
                .unwrap()
                .total,
            0
        );
        assert_eq!(
            library
                .db
                .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r
                    .get::<_, u64>(
                    0
                ))
                .unwrap(),
            0
        );
        let root = library.root.clone();
        drop(library);
        let mut library = Library::open(&root).unwrap();
        assert_eq!(
            library
                .page(&Query::default(), 0, 128)
                .unwrap()
                .assets
                .len(),
            2
        );
        assert_eq!(library.info().unwrap().projects.len(), 1);
    }
    #[test]
    fn project_delete_rejects_changed_scope_and_database_failure_restores_live_assets() {
        let (_dir, mut library, assets) = fixture();
        let project = library.create_project("测试项目").unwrap();
        library
            .edit(&[assets[0].id], &Edit::Project(project, true))
            .unwrap();
        let stale = library.project_deletion_plan(project).unwrap();
        library
            .edit(&[assets[1].id], &Edit::Project(project, true))
            .unwrap();
        let plan = library.project_deletion_plan(project).unwrap();
        let path = library.asset_path(assets[0].id).unwrap();
        let bytes = fs::read(&path).unwrap();
        library.db.execute_batch("CREATE TRIGGER fail_project_delete BEFORE DELETE ON projects BEGIN SELECT RAISE(ABORT,'injected failure'); END;").unwrap();
        assert!(library.delete_project_contents(&plan).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(library.info().unwrap().count, 3);
        assert_eq!(library.project_deletion_plan(project).unwrap(), plan);
        library
            .db
            .execute_batch("DROP TRIGGER fail_project_delete;")
            .unwrap();
        assert!(library.delete_project_contents(&stale).is_err());
        assert_eq!(library.info().unwrap().count, 3);
        assert_eq!(library.delete_project_contents(&plan).unwrap().deleted, 2);
        let empty = library.create_project("空项目").unwrap();
        let plan = library.project_deletion_plan(empty).unwrap();
        assert_eq!(library.delete_project_contents(&plan).unwrap().deleted, 0);
    }
    #[test]
    fn project_delete_journal_recovers_both_sides_of_commit() {
        for committed in [false, true] {
            let (_dir, mut library, assets) = fixture();
            let project = library.create_project("重启恢复项目").unwrap();
            let asset = assets[0].id;
            library
                .edit(&[asset], &Edit::Project(project, true))
                .unwrap();
            let plan = library.project_deletion_plan(project).unwrap();
            let path = library.asset_path(asset).unwrap();
            let root = library.root.clone();
            let (job, files) = library
                .prepare_purge_for(&plan.assets, Some(&plan))
                .unwrap();
            for (source, target) in files {
                fs::rename(library.root.join(source), library.root.join(target)).unwrap();
            }
            if committed {
                library
                    .commit_purge_for(&job, &plan.assets, Some(&plan))
                    .unwrap();
            }
            drop(library);
            let library = Library::open(&root).unwrap();
            assert_eq!(path.exists(), !committed);
            assert_eq!(library.info().unwrap().projects.is_empty(), committed);
            assert_eq!(library.details(asset).is_err(), committed);
            assert_eq!(
                library
                    .db
                    .query_row("SELECT count(*) FROM purge_jobs", [], |r| r
                        .get::<_, u64>(0))
                    .unwrap(),
                0
            );
        }
    }
    fn fixture() -> (tempfile::TempDir, Library, Vec<Asset>) {
        let dir = tempfile::tempdir().unwrap();
        let mut library = Library::create(dir.path(), "删除测试").unwrap();
        for n in 0..3u8 {
            let path = dir.path().join(format!("{n}.png"));
            image::RgbImage::from_pixel(32, 24, image::Rgb([n * 70, 20, 90]))
                .save(&path)
                .unwrap();
            library.import(&path, None).unwrap();
        }
        let assets = library.page(&Query::default(), 0, 10).unwrap().assets;
        (dir, library, assets)
    }
    #[test]
    fn permanent_delete_removes_aliases_tags_and_search_but_preserves_external_original() {
        let (dir, mut library, assets) = fixture();
        let asset = assets[0].id;
        let path = library.asset_path(asset).unwrap();
        let project = library.create_project("第二个项目").unwrap();
        library.import(&path, Some(project)).unwrap();
        let aliases = library
            .db
            .prepare("SELECT path FROM project_files WHERE version_id=?1")
            .unwrap()
            .query_map([assets[0].version_id.to_string()], |r| {
                r.get::<_, String>(0)
            })
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert!(!aliases.is_empty());
        library
            .edit(&[asset], &Edit::AddTags(vec!["删除检索测试".into()]))
            .unwrap();
        let external = dir.path().join("外部原片.png");
        image::RgbImage::from_pixel(24, 16, image::Rgb([2, 3, 4]))
            .save(&external)
            .unwrap();
        let original = fs::read(&external).unwrap();
        library.import_referenced(&external, None).unwrap();
        let reference = library
            .page(
                &Query {
                    storage: 2,
                    ..Default::default()
                },
                0,
                10,
            )
            .unwrap()
            .assets
            .remove(0);
        assert!(library.purge(&[asset]).is_err());
        library
            .edit(&[asset, reference.id], &Edit::Trash(true))
            .unwrap();
        assert!(library.purge(&[asset, AssetId::default()]).is_err());
        assert!(path.is_file());
        let report = library.purge(&[asset, reference.id, asset]).unwrap();
        assert_eq!(report.deleted, 2);
        assert!(!report.cleanup_pending);
        assert!(!path.exists());
        for alias in aliases {
            assert!(!library.root.join(alias).exists());
        }
        assert_eq!(fs::read(&external).unwrap(), original);
        assert!(library.details(asset).is_err());
        assert_eq!(library.info().unwrap().count, 2);
        assert_eq!(
            library
                .page(
                    &Query {
                        text: "删除检索测试".into(),
                        trash: true,
                        ..Default::default()
                    },
                    0,
                    10
                )
                .unwrap()
                .total,
            0
        );
        assert_eq!(library.maintenance(false).unwrap().checked, 2);
        assert!(
            !library
                .db
                .prepare("PRAGMA foreign_key_check")
                .unwrap()
                .exists([])
                .unwrap()
        );
    }
    #[test]
    fn reopening_restores_half_staged_uncommitted_delete() {
        let (_dir, mut library, assets) = fixture();
        let id = assets[0].id;
        let original_path = library.asset_path(id).unwrap();
        let original = fs::read(&original_path).unwrap();
        library.edit(&[id], &Edit::Trash(true)).unwrap();
        let (_job, files) = library.prepare_purge(&[id]).unwrap();
        let (source, target) = &files[0];
        fs::rename(library.root.join(source), library.root.join(target)).unwrap();
        let root = library.root.clone();
        drop(library);
        let mut reopened = Library::open(&root).unwrap();
        assert_eq!(fs::read(original_path).unwrap(), original);
        assert!(reopened.details(id).is_ok());
        assert_eq!(reopened.organizer().unwrap().trash_count, 1);
        assert_eq!(
            reopened
                .db
                .query_row("SELECT count(*) FROM purge_jobs", [], |r| r
                    .get::<_, u32>(0))
                .unwrap(),
            0
        );
        assert_eq!(reopened.purge(&[id]).unwrap().deleted, 1);
    }
    #[test]
    fn database_failure_rolls_back_metadata_and_all_moved_files() {
        let (_dir, mut library, assets) = fixture();
        let id = assets[0].id;
        let path = library.asset_path(id).unwrap();
        let original = fs::read(&path).unwrap();
        library.edit(&[id], &Edit::Trash(true)).unwrap();
        library.db.execute_batch("CREATE TRIGGER reject_purge BEFORE DELETE ON assets BEGIN SELECT RAISE(ABORT,'injected failure'); END;").unwrap();
        assert!(library.purge(&[id]).is_err());
        assert_eq!(fs::read(&path).unwrap(), original);
        assert!(library.details(id).is_ok());
        assert_eq!(library.organizer().unwrap().trash_count, 1);
        assert!(library.thumbnail(assets[0].version_id).unwrap().is_some());
        library
            .db
            .execute_batch("DROP TRIGGER reject_purge")
            .unwrap();
        assert_eq!(library.purge(&[id]).unwrap().deleted, 1);
    }
    #[test]
    fn committed_recovery_only_unlinks_quarantine_and_preserves_new_file_at_old_name() {
        let (_dir, mut library, assets) = fixture();
        let id = assets[0].id;
        let path = library.asset_path(id).unwrap();
        library.edit(&[id], &Edit::Trash(true)).unwrap();
        let (job, files) = library.prepare_purge(&[id]).unwrap();
        for (source, target) in &files {
            fs::rename(library.root.join(source), library.root.join(target)).unwrap();
        }
        library.commit_purge(&job, &[id]).unwrap();
        fs::write(&path, b"new unrelated file").unwrap();
        let root = library.root.clone();
        drop(library);
        let reopened = Library::open(&root).unwrap();
        assert_eq!(fs::read(path).unwrap(), b"new unrelated file");
        assert!(reopened.details(id).is_err());
        for (_, target) in files {
            assert!(!root.join(target).exists());
        }
    }
    #[test]
    fn explicit_purge_removes_active_synco_task_and_handoff_copy() {
        let (_dir, mut library, assets) = fixture();
        let id = assets[0].id;
        library.prepare_synco(&[id], None).unwrap();
        library.edit(&[id], &Edit::Trash(true)).unwrap();
        let task = library.synco_pending().unwrap().remove(0).id;
        let copies = fs::read_dir(library.root.join("interop/outgoing"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        assert_eq!(copies.len(), 1);
        assert_eq!(library.purge(&[id]).unwrap().deleted, 1);
        assert!(!copies[0].exists());
        assert!(library.synco_pending().unwrap().is_empty());
        assert!(!library.synco_error(&task, "迟到的网络错误").unwrap());
        assert!(
            library
                .receive_synco(&task, &serde_json::json!({}))
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn synco_states_never_block_explicit_project_deletion_and_shared_tasks_are_retained() {
        for state in [
            "queued",
            "importing",
            "editing",
            "rendering",
            "ready",
            "delivered",
            "failed",
            "cancelled",
        ] {
            let (_dir, mut lib, assets) = fixture();
            let asset = &assets[0];
            let project = lib.create_project("删除目标").unwrap();
            lib.edit(&[asset.id], &Edit::Project(project, true))
                .unwrap();
            lib.prepare_synco(&[asset.id], Some(project)).unwrap();
            let returned = AssetVersionId::default();
            fs::copy(
                lib.asset_path(asset.id).unwrap(),
                lib.root.join("versions/result.png"),
            )
            .unwrap();
            lib.db.execute("INSERT INTO \
                asset_versions(id,asset_id,parent_version_id,origin,path,sha256,format,width,height,bytes) SELECT \
                ?1,asset_id,id,'synco','versions/result.png',sha256,format,width,height,bytes FROM asset_versions \
                WHERE id=?2",
params![returned.to_string(),
asset.version_id.to_string()]).unwrap();

            lib.db
                .execute(
                    "UPDATE synco_tasks SET state=?1,ack='{}',returned_version_id=?2",
                    params![state, returned.to_string()],
                )
                .unwrap();
            let plan = lib.project_deletion_plan(project).unwrap();
            assert_eq!(lib.delete_project_contents(&plan).unwrap().deleted, 1);
            assert!(lib.synco_tasks().unwrap().is_empty());
        }
        let (_dir, mut lib, assets) = fixture();
        let asset = assets[0].id;
        let removed = lib.create_project("删除共享归属").unwrap();
        let retained = lib.create_project("保留归属").unwrap();
        lib.edit(&[asset], &Edit::Project(removed, true)).unwrap();
        // Retain defensive purge coverage for legacy shared records.
        lib.db
            .execute(
                "INSERT INTO project_assets VALUES(?1,?2)",
                params![retained.to_string(), asset.to_string()],
            )
            .unwrap();
        lib.prepare_synco(&[asset], Some(retained)).unwrap();
        let plan = lib.project_deletion_plan(removed).unwrap();
        assert!(plan.assets.is_empty());
        assert_eq!(plan.shared, vec![asset]);
        assert_eq!(lib.delete_project_contents(&plan).unwrap().deleted, 0);
        assert!(lib.asset_path(asset).unwrap().exists());
        assert_eq!(lib.synco_pending().unwrap().len(), 1);
        assert_eq!(lib.details(asset).unwrap().projects, vec![retained]);
    }
    #[test]
    fn case_alias_owned_by_another_asset_prevents_deleting_shared_file() {
        let (_dir, mut library, assets) = fixture();
        let id = assets[0].id;
        let path = library.asset_path(id).unwrap();
        let original = fs::read(&path).unwrap();
        let relative: String = library
            .db
            .query_row(
                "SELECT path FROM asset_versions WHERE id=?1",
                [assets[0].version_id.to_string()],
                |r| r.get(0),
            )
            .unwrap();
        library
            .db
            .execute(
                "UPDATE asset_versions SET path=?1 WHERE id=?2",
                params![relative.to_uppercase(), assets[1].version_id.to_string()],
            )
            .unwrap();
        library.edit(&[id], &Edit::Trash(true)).unwrap();
        assert!(library.purge(&[id]).is_err());
        assert_eq!(fs::read(path).unwrap(), original);
        assert!(library.details(id).is_ok());
    }
    #[test]
    fn library_metadata_path_cannot_be_deleted_as_media() {
        let (_dir, mut library, assets) = fixture();
        let id = assets[0].id;
        library.edit(&[id], &Edit::Trash(true)).unwrap();
        library
            .db
            .execute(
                "UPDATE asset_versions SET path='library.toml' WHERE id=?1",
                [assets[0].version_id.to_string()],
            )
            .unwrap();
        assert!(library.purge(&[id]).is_err());
        assert!(library.root.join("library.toml").is_file());
        assert!(library.details(id).is_ok());
    }
}
