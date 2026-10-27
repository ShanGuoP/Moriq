use super::*;

impl Library {
    /// 校验并保存资源库显示名称，不移动资源库目录。
    pub fn rename(&mut self, name: &str) -> Result<()> {
        let name = name.trim().trim_end_matches(".moriq").trim();
        import::validate_component(name)?;
        if name.is_empty()
            || name.len() > 120
            || name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|'])
        {
            return Err(Error::Invalid("请输入有效的资源库名称".into()));
        }
        self.db.execute("INSERT INTO library_preferences(key,value) VALUES('display_name',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [name])?;
        Ok(())
    }

    /// 在临时目录建立初始数据库和目录结构，再发布为指定名称的资源库并打开。
    pub fn create(parent: &Path, name: &str) -> Result<Self> {
        let name = name.trim().trim_end_matches(".moriq").trim();
        import::validate_component(name)?;
        if name.len() > 120 {
            return Err(Error::Invalid(
                "请输入有效库名，避免斜线和系统保留字符".into(),
            ));
        }
        let destination = parent.join(format!("{name}.moriq"));
        if destination.exists() {
            return Err(Error::Invalid(
                "同名资源库已存在，请改名或打开现有库".into(),
            ));
        }
        let staging = tempfile::Builder::new()
            .prefix(".moriq-create-")
            .tempdir_in(parent)?;
        require_hardlinks(staging.path())?;
        for dir in DIRECTORIES {
            fs::create_dir_all(staging.path().join(dir))?;
        }
        let manifest = Manifest {
            format: "moriq".into(),
            library_id: LibraryId::default(),
            name: name.into(),
            schema_version: SCHEMA,
            min_reader_version: SCHEMA,
            created_at_utc: now_ms()?,
        };
        let serialized =
            toml::to_string_pretty(&manifest).map_err(|e| Error::Invalid(e.to_string()))?;
        let mut file = File::create(staging.path().join("library.toml"))?;
        file.write_all(serialized.as_bytes())?;
        file.sync_all()?;
        drop(file);
        {
            let db = Connection::open(staging.path().join("catalog.db"))?;
            db.execute_batch(include_str!("../database/schema.sql"))?;
            db.execute(
                "INSERT INTO library_identity(id) VALUES(?1)",
                [manifest.library_id.to_string()],
            )?;
        }
        fs::rename(staging.path(), &destination)?;
        Self::open_inner(&destination, true)
    }

    /// 打开已有资源库，执行身份校验、必要的结构升级和恢复流程。
    pub fn open(root: &Path) -> Result<Self> {
        Self::open_inner(root, false)
    }

    /// 取得资源库锁并校验清单与数据库身份；升级前备份已有数据库，再恢复未完成操作。
    fn open_inner(root: &Path, fresh: bool) -> Result<Self> {
        let root = root.canonicalize()?;
        let manifest_file = safe_path(&root, "library.toml")?;
        if fs::metadata(&manifest_file)?.len() > 64 * 1024 {
            return Err(Error::Invalid("资源库清单过大，无法安全读取".into()));
        }
        let mut manifest: Manifest = toml::from_str(&fs::read_to_string(manifest_file)?)
            .map_err(|_| Error::Invalid("资源库清单损坏，请从备份恢复".into()))?;
        import::validate_component(&manifest.name)?;
        if manifest.format != "moriq"
            || !(1..=SCHEMA).contains(&manifest.schema_version)
            || manifest.min_reader_version > SCHEMA
        {
            return Err(Error::Invalid(
                "资源库格式不兼容，请使用对应版本的 Moriq Space".into(),
            ));
        }
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(safe_path(&root, "library.lock")?)?;
        lock.try_lock_exclusive().map_err(|_| Error::Locked)?;
        if safe_path(&root, "metadata/library-deletion.pending")?.exists() {
            return Err(Error::Invalid(
                "此资源库正在删除或删除曾中断，请从资源库管理中检查并重试删除".into(),
            ));
        }
        require_hardlinks(&root)?;
        let db_path = safe_path(&root, "catalog.db")?;
        safe_path(&root, "catalog.db-wal")?;
        safe_path(&root, "catalog.db-shm")?;
        let mut db =
            Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)?;
        db.busy_timeout(std::time::Duration::from_millis(500))?;
        db.execute_batch("PRAGMA foreign_keys=ON;")?;
        let version: u32 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
        let identity: String = db.query_row("SELECT id FROM library_identity", [], |r| r.get(0))?;
        if !(1..=SCHEMA).contains(&version) || identity != manifest.library_id.to_string() {
            return Err(Error::Invalid(
                "资源库清单与数据库身份不一致，已停止打开".into(),
            ));
        }
        let migration_backup = if version < SCHEMA && !fresh {
            // Retain a consistent pre-migration database before changing the schema.
            let backup = safe_path(&root, "backups")?;
            fs::create_dir_all(&backup)?;
            let backup = backup.join(format!("schema-{version}-{}.db", Uuid::new_v4()));
            db.backup(rusqlite::MAIN_DB, &backup, None)?;
            tracing::info!(path = %backup.display(), "pre-migration database snapshot saved");
            Some(backup)
        } else {
            None
        };
        let migrations = (|| -> Result<()> {
            if version == 1 {
                let tx = db.transaction()?;
                tx.execute_batch(include_str!("../database/migrations/migration_2.sql"))?;
                tx.commit()?;
            }
            if version <= 2 {
                let tx = db.transaction()?;
                tx.execute_batch(include_str!("../database/migrations/migration_3.sql"))?;
                tx.commit()?;
            }
            if version <= 3 {
                let tx = db.transaction()?;
                tx.execute_batch(include_str!("../database/migrations/migration_4.sql"))?;
                tx.commit()?;
            }
            if version <= 4 {
                let tx = db.transaction()?;
                tx.execute_batch(include_str!("../database/migrations/migration_5.sql"))?;
                tx.commit()?;
            }
            if version <= 5 {
                let tx = db.transaction()?;
                tx.execute_batch(include_str!("../database/migrations/migration_6.sql"))?;
                tx.commit()?;
            }
            if version <= 6 {
                let tx = db.transaction()?;
                tx.execute_batch(include_str!("../database/migrations/migration_7.sql"))?;
                tx.commit()?;
            }
            if version <= 7 {
                let tx = db.transaction()?;
                tx.execute_batch(include_str!("../database/migrations/migration_8.sql"))?;
                tx.commit()?;
            }
            if version <= 8 {
                let tx = db.transaction()?;
                tx.execute_batch(include_str!("../database/migrations/migration_9.sql"))?;
                tx.commit()?;
            }
            if version <= 9 {
                let tx = db.transaction()?;
                tx.execute_batch(include_str!("../database/migrations/migration_10.sql"))?;
                tx.commit()?;
            }
            if version <= 10 {
                let tx = db.transaction()?;
                tx.execute_batch(include_str!("../database/migrations/migration_11.sql"))?;
                tx.commit()?;
            }
            // Older readers ignore unknown Query fields and would broaden saved rules.
            if version <= 11 {
                db.execute_batch("PRAGMA user_version=12;")?;
            }
            if version <= 12 {
                let tx = db.transaction()?;
                tx.execute_batch(include_str!("../database/migrations/migration_13.sql"))?;
                tx.commit()?;
            }
            if version <= 13 {
                let tx = db.transaction()?;
                tx.execute_batch(include_str!("../database/migrations/migration_14.sql"))?;
                tx.commit()?;
            }
            if version <= 14 {
                let tx = db.transaction()?;
                tx.execute_batch(include_str!("../database/migrations/migration_15.sql"))?;
                tx.commit()?;
            }
            if version <= 15 {
                let tx = db.transaction()?;
                tx.execute_batch(include_str!("../database/migrations/migration_16.sql"))?;
                tx.commit()?;
            }
            if version <= 16 {
                let tx = db.transaction()?;
                tx.execute_batch(include_str!("../database/migrations/migration_17.sql"))?;
                tx.commit()?;
            }
            if version <= 17 {
                let tx = db.transaction()?;
                tx.execute_batch(include_str!("../database/migrations/migration_18.sql"))?;
                tx.commit()?;
            }
            if version <= 18 {
                let tx = db.transaction()?;
                tx.execute_batch(include_str!("../database/migrations/migration_19.sql"))?;
                tx.commit()?;
            }
            if version <= 19 {
                let tx = db.transaction()?;
                tx.execute_batch(include_str!("../database/migrations/migration_20.sql"))?;
                tx.commit()?;
            }
            if version <= 20 {
                let tx = db.transaction()?;
                tx.execute_batch(include_str!("../database/migrations/migration_21.sql"))?;
                tx.commit()?;
            }
            if version <= 21 {
                let tx = db.transaction()?;
                tx.execute_batch(include_str!("../database/migrations/migration_22.sql"))?;
                tx.commit()?;
            }
            if version <= 22 {
                let tx = db.transaction()?;
                tx.execute_batch(include_str!("../database/migrations/migration_23.sql"))?;
                tx.commit()?;
            }
            if version <= 23 {
                let tx = db.transaction()?;
                tx.execute_batch(include_str!("../database/migrations/migration_24.sql"))?;
                for category in AiProfile::default().categories {
                    tx.execute(
                        "UPDATE ai_vocabulary SET definition=?2 WHERE id=?1",
                        params![category.id, category.definition],
                    )?;
                }
                tx.commit()?;
            }
            if version <= 24 {
                project_isolation::migrate(&mut db, &root)?;
            }
            if version <= 25 {
                workspace::migrate(&mut db)?;
            }
            Ok(())
        })();
        migrations.map_err(|error| {
            Error::Invalid(match &migration_backup {
                Some(path) => format!(
                    "数据库升级未完成：{error}。升级前备份保留在 {}",
                    path.display()
                ),
                None => error.to_string(),
            })
        })?;
        db.execute("UPDATE background_jobs SET inflight=0,state=CASE WHEN state='running' THEN 'queued' ELSE state END WHERE inflight=1 OR state='running'",[])?;
        db.execute("UPDATE background_jobs SET state='paused',error='库中含外部引用，后台读取已暂停；请核对来源后在任务中继续' WHERE state='queued' \
                AND kind<>'batch' AND EXISTS(SELECT 1 FROM file_locations)",
[])?;

        if manifest.schema_version < SCHEMA {
            manifest.schema_version = SCHEMA;
            manifest.min_reader_version = SCHEMA;
            let bytes =
                toml::to_string_pretty(&manifest).map_err(|e| Error::Invalid(e.to_string()))?;
            let mut temporary = tempfile::NamedTempFile::new_in(&root)?;
            temporary.write_all(bytes.as_bytes())?;
            temporary.as_file().sync_all()?;
            temporary
                .persist(safe_path(&root, "library.toml")?)
                .map_err(|e| Error::Io(e.error))?;
        }
        db.pragma_update(None, "journal_mode", "WAL")?;
        db.pragma_update(None, "synchronous", "FULL")?;
        // Interrupted operations are retained for diagnosis. Never infer success or delete originals.
        let interrupted = db.execute(
            "UPDATE operation_journal SET phase='interrupted' WHERE phase='prepared'",
            [],
        )?;
        if interrupted > 0 {
            tracing::warn!(interrupted, "import recovery records retained");
        }
        let lock = std::sync::Arc::new(lock);
        let readers = read_pool::ReadPool::new(root.clone(), lock.clone());
        let mut library = Self {
            workspace: None,
            organizer_cache: Default::default(),
            root,
            manifest,
            db,
            _lock: lock,
            readers,
            snapshot: None,
            last_import: Default::default(),
        };
        if let Some(path) = migration_backup {
            library.notice(&Notice {
                title: "升级前数据库已备份".into(),
                detail: format!("数据库升级前的完整快照：{}", path.display()),
                level: 0,
                asset: None,
                retry: String::new(),
            })?;
        }
        if let Err(error) = library.recover_purges() {
            library.notice(&Notice {
                title: "删除恢复需要检查".into(),
                detail: format!(
                    "{error}。恢复记录及现存文件均保留；请检查空间维护或从备份恢复后再永久删除。"
                ),
                level: 2,
                asset: None,
                retry: "maintenance".into(),
            })?;
        }
        library.recover_orphans()?;
        library.clean_import_staging()?;
        Ok(library)
    }

    /// 借用最近一次导入各阶段的耗时统计，不重新执行导入。
    pub fn import_metrics(&self) -> &ImportMetrics {
        &self.last_import
    }

    /// 返回当前资源库的根目录路径。
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 返回清单中稳定的资源库身份，用于请求和结果的归属校验。
    pub fn id(&self) -> LibraryId {
        self.manifest.library_id
    }

    /// 汇总素材数量、空间、项目及显示名称，生成资源库信息快照。
    pub fn info(&self) -> Result<LibraryInfo> {
        let (count, favorites): (u64, u64) = self.db.query_row(
            "SELECT count(*),COALESCE(sum(favorite),0) FROM assets WHERE deleted_at IS NULL",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let bytes = self.db.query_row(
            "SELECT COALESCE(sum(bytes),0) FROM asset_versions",
            [],
            |r| r.get(0),
        )?;
        let mut stmt = self.db.prepare("WITH RECURSIVE tree(root,id) AS(SELECT id,id FROM projects UNION ALL SELECT tree.root,p.id FROM \
                projects p JOIN tree ON p.parent_id=tree.id) SELECT p.id,p.name,count(DISTINCT \
                pa.asset_id),p.parent_id,p.visited_at FROM projects p LEFT JOIN tree ON tree.root=p.id LEFT JOIN \
                project_assets pa ON pa.project_id=tree.id AND EXISTS(SELECT 1 FROM assets a WHERE a.id=pa.asset_id \
                AND a.deleted_at IS NULL) GROUP BY p.id ORDER BY p.name")?;

        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, u64>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, u64>(4)?,
            ))
        })?;
        let mut projects = Vec::new();
        for row in rows {
            let (id, name, count, parent, visited_at) = row?;
            projects.push(Project {
                id: ProjectId(parse_id(&id)?),
                parent: parent.map(|v| parse_id(&v).map(ProjectId)).transpose()?,
                name,
                count,
                visited_at,
            });
        }
        Ok(LibraryInfo {
            id: self.id(),
            name: self
                .db
                .query_row(
                    "SELECT value FROM library_preferences WHERE key='display_name'",
                    [],
                    |r| r.get(0),
                )
                .optional()?
                .unwrap_or_else(|| self.manifest.name.clone()),
            count,
            favorites,
            bytes,
            created_at: self.manifest.created_at_utc,
            updated_at: self
                .db
                .query_row("SELECT updated_at FROM library_state WHERE id=1", [], |r| {
                    r.get::<_, u64>(0)
                })?
                .max(self.manifest.created_at_utc),
            projects,
        })
    }
}
