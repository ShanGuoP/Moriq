use super::*;

/// 拒绝不允许的网络引用路径，避免自动访问外部网络位置。
pub(super) fn reject_network_reference(path: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        use std::path::Prefix;
        if let Some(Component::Prefix(prefix)) = path.components().next()
            && !matches!(prefix.kind(), Prefix::Disk(_) | Prefix::VerbatimDisk(_))
        {
            return Err(Error::Invalid(
                "不自动读取网络或设备引用。请将素材复制到本地磁盘后重新关联。".into(),
            ));
        }
    }
    #[cfg(not(windows))]
    if path.to_string_lossy().starts_with("//") {
        return Err(Error::Invalid(
            "不自动读取网络引用，请将素材复制到本地后重新关联。".into(),
        ));
    }
    Ok(())
}

impl Library {
    /// 清理指定导入操作的上下文与恢复登记。
    pub(super) fn forget_import(&mut self, operation: &str) -> Result<()> {
        let tx = self.db.transaction()?;
        tx.execute("DELETE FROM import_context WHERE operation_id IN(SELECT id FROM operation_journal WHERE id=?1 AND phase='committed')", [operation])?;
        tx.execute(
            "DELETE FROM operation_journal WHERE id=?1 AND phase='committed'",
            [operation],
        )?;
        tx.commit()?;
        Ok(())
    }
    /// 清理已完成导入留下的受管暂存内容，保留仍需恢复的操作。
    pub(super) fn clean_import_staging(&mut self) -> Result<()> {
        let tx = self.db.transaction()?;
        tx.execute("DELETE FROM import_context WHERE operation_id IN(SELECT id FROM operation_journal WHERE phase='committed')", [])?;
        tx.execute("DELETE FROM operation_journal WHERE phase='committed'", [])?;
        tx.commit()?;
        let staging = safe_path(&self.root, "staging")?;
        if !staging.is_dir() {
            return Ok(());
        }
        for entry in fs::read_dir(staging)? {
            let entry = entry?;
            let path = entry.path();
            if !["part", "thumb", "ready", "synco"]
                .iter()
                .any(|suffix| path.extension().is_some_and(|s| s == *suffix))
            {
                continue;
            }
            let Some(operation) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if Uuid::parse_str(operation).is_err() {
                continue;
            }
            let registered: bool = self.db.query_row(
                "SELECT EXISTS(SELECT 1 FROM operation_journal WHERE id=?1)",
                [operation],
                |r| r.get(0),
            )?;
            if !registered && let Err(error) = fs::remove_file(path) {
                tracing::warn!(%error,"unused staging cleanup deferred");
            }
        }
        Ok(())
    }
    /// 为项目汇总有效素材封面，避免空项目使用其他项目的图片。
    pub fn project_covers(&self) -> Result<Vec<(ProjectId, Option<AssetVersionId>)>> {
        let mut statement = self.db.prepare_cached(
            "SELECT p.id,(
                SELECT a.display_version_id FROM assets a
                JOIN project_assets pa ON pa.asset_id=a.id
                WHERE pa.project_id=p.id AND a.deleted_at IS NULL
                ORDER BY a.imported_at DESC LIMIT 1
             ) FROM projects p ORDER BY p.name LIMIT 32",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })?;
        rows.map(|row| {
            let (project, version) = row?;
            Ok((
                ProjectId(parse_id(&project)?),
                version
                    .map(|id| parse_id(&id).map(AssetVersionId))
                    .transpose()?,
            ))
        })
        .collect()
    }
    /// 校验说明文本长度并保存到资源库偏好。
    pub fn description(&mut self, value: &str) -> Result<()> {
        if value.len() > 8192 {
            return Err(Error::Invalid("资源库描述不能超过 8 KiB".into()));
        }
        self.db.execute("INSERT INTO library_preferences VALUES('description',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[value])?;
        Ok(())
    }
    /// 有界解码资源库自定义封面，缺少图片时返回空值。
    pub fn cover(&self) -> Result<Option<Pixels>> {
        let path = safe_path(&self.root, "metadata/cover.png")?;
        if !path.exists() {
            return Ok(None);
        }
        let mut reader = ImageReader::open(path)?.with_guessed_format()?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(1920);
        limits.max_image_height = Some(1920);
        limits.max_alloc = Some(32 * 1024 * 1024);
        reader.limits(limits);
        Ok(Some(pixels(reader.decode()?)))
    }
    /// 校验并解码指定封面图片，生成资源库持有的独立封面文件。
    pub fn set_cover(&self, path: &Path) -> Result<()> {
        if fs::metadata(path)?.len() > MAX_BYTES {
            return Err(Error::Invalid("封面图片超过 256 MiB".into()));
        }
        let mut reader = ImageReader::open(path)?.with_guessed_format()?;
        reader.limits(decode_limits());
        let mut decoder = reader.into_decoder()?;
        let orientation = decoder.orientation()?;
        let mut decoded = image::DynamicImage::from_decoder(decoder)?;
        decoded.apply_orientation(orientation);
        fs::create_dir_all(safe_path(&self.root, "metadata")?)?;
        let mut temp = tempfile::NamedTempFile::new_in(safe_path(&self.root, "metadata")?)?;
        resize::fit(&decoded, 1920)?.write_to(&mut temp, ImageFormat::Png)?;
        temp.as_file().sync_all()?;
        temp.persist(safe_path(&self.root, "metadata/cover.png")?)
            .map_err(|e| Error::Io(e.error))?;
        self.touch_metadata()?;
        Ok(())
    }
    /// 登记外部原片引用；保留外部文件位置而不把它视为受管副本。
    pub fn import_referenced(&mut self, source: &Path, project: Option<ProjectId>) -> Result<bool> {
        self.snapshot = None;
        self.import_inner(source, project, false, true, None)
    }
    /// 汇总项目、标签、分类、任务等管理视图，并复用适用的读取缓存。
    pub fn organizer(&self) -> Result<Organizer> {
        let scope = self.workspace_key();
        let changes = self.db.total_changes();
        let schema: u32 = self
            .db
            .pragma_query_value(None, "schema_version", |r| r.get(0))?;
        if let Some((known, version, value)) = self.organizer_cache.borrow().get(&scope)
            && *known == changes
            && *version == schema
        {
            return Ok(value.clone());
        }
        let filter = self.workspace_filter("a.id");
        let mut result = Organizer::default();
        // Each projection owns its error. A broken panel must not suppress the
        // remaining projections or become a prerequisite for background execution.
        let mut errors = Vec::new();
        /// 隔离管理视图某一部分的读取错误，记录诊断后返回该部分缺省值。
        fn section<T: Default>(
            errors: &mut Vec<(OrganizerSection, String)>,
            section: OrganizerSection,
            value: Result<T>,
        ) -> T {
            value.unwrap_or_else(|error| {
                tracing::warn!(?section, %error, "organizer projection could not be loaded");
                errors.push((section, error.to_string()));
                T::default()
            })
        }
        use OrganizerSection::*;
        (result.quarantined_files, result.quarantined_bytes) = section(
            &mut errors,
            Statistics,
            (|| {
                Ok(self.db.query_row(
                    "SELECT count(*),coalesce(sum(bytes),0) FROM orphan_quarantine",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )?)
            })(),
        );
        result.folders = section(&mut errors, Folders, self.virtual_folders());
        result.jobs = section(&mut errors, Jobs, self.background_jobs());
        result.people = section(&mut errors, People, self.people());
        result.ai_profile = section(&mut errors, Categories, self.ai_profile());
        result.ai_groups = section(&mut errors, Categories, self.ai_groups());
        result.ai_outdated = section(&mut errors, Categories, self.outdated_ai_count());
        result.ai_categories = result
            .ai_groups
            .iter()
            .filter(|g| g.count > 0)
            .map(|g| (g.label.trim_start_matches("历史 · ").to_string(), g.count))
            .collect();
        result.video_count = section(
            &mut errors,
            Statistics,
            (|| {
                Ok(self.db.query_row(&format!("SELECT count(*) FROM assets a JOIN asset_versions v ON v.id=a.display_version_id WHERE a.deleted_at \
                IS NULL AND {filter} AND v.format IN({})",
crate::video::sql_formats()),
[],
|r|r.get(0))?)
            })(),
        );
        result.tags = section(
            &mut errors,
            Tags,
            (|| {
                Ok(self.db.prepare(&format!("SELECT t.name,count(*) FROM effective_asset_tags t JOIN assets a ON \
                a.id=t.asset_id WHERE a.deleted_at IS NULL AND {filter} GROUP BY t.name ORDER BY t.name LIMIT 2000"))?
            .query_map([],|r|Ok((r.get(0)?,r.get(1)?)))?.collect::<std::result::Result<_,_>>()?)
            })(),
        );
        result.collections = section(&mut errors, Collections, self.smart_collections());
        let week = now_ms()?.saturating_sub(7 * 86400000);
        result.trash_count = section(
            &mut errors,
            Statistics,
            (|| {
                Ok(self.db.query_row(
                    &format!(
                        "SELECT count(*) FROM assets a WHERE deleted_at IS NOT NULL AND {filter}"
                    ),
                    [],
                    |r| r.get(0),
                )?)
            })(),
        );
        (
            result.recent_count,
            result.untagged_count,
            result.unassigned_count,
            result.high_rating_count,
        ) = section(
            &mut errors,
            Statistics,
            (|| {
                Ok(self.db.query_row(&format!("SELECT coalesce(sum(imported_at>=?1),0),coalesce(sum(NOT EXISTS(SELECT 1 FROM effective_asset_tags WHERE \
                asset_id=a.id)),0),coalesce(sum(NOT EXISTS(SELECT 1 FROM project_assets WHERE \
                asset_id=a.id)),0),coalesce(sum(rating>=4),0) FROM assets a WHERE deleted_at IS NULL AND {filter}"),
[week],
|r|Ok((r.get(0)?,
r.get(1)?,
r.get(2)?,
r.get(3)?)))?)
            })(),
        );
        result.interrupted = section(
            &mut errors,
            Statistics,
            (|| {
                Ok(self.db.query_row(
                    "SELECT count(*) FROM operation_journal j WHERE phase='interrupted' AND (?1='' OR EXISTS(SELECT 1 FROM import_context c WHERE c.operation_id=j.id AND c.project_id=?1))",
                    [scope.clone()],
                    |r| r.get(0),
                )?)
            })(),
        );
        result.edited_count = section(
            &mut errors,
            Statistics,
            (|| {
                Ok(self.db.query_row(
                    &format!("SELECT count(*) FROM assets a WHERE deleted_at IS NULL AND edited_at>=?1 AND {filter}"),
                    [week],
                    |r| r.get(0),
                )?)
            })(),
        );
        result.raw_count = section(
            &mut errors,
            Statistics,
            (|| {
                Ok(self.db.query_row(&format!("SELECT count(*) FROM assets a JOIN asset_versions v ON v.id=a.display_version_id WHERE a.deleted_at \
                IS NULL AND {filter} AND v.format IN({})",
native::raw_sql()),
[],
|r|r.get(0))?)
            })(),
        );
        result.synco = section(&mut errors, Synco, self.synco_tasks());
        result.description = section(
            &mut errors,
            Profile,
            (|| {
                Ok(self
                    .db
                    .query_row(
                        "SELECT value FROM library_preferences WHERE key='description'",
                        [],
                        |r| r.get(0),
                    )
                    .optional()?
                    .unwrap_or_default())
            })(),
        );
        result.errors = errors;
        if result.errors.is_empty() {
            let mut cache = self.organizer_cache.borrow_mut();
            if cache.len() >= 32 {
                cache.clear();
            }
            cache.insert(scope, (changes, schema, result.clone()));
        }
        Ok(result)
    }
    /// 读取当前项目可见的智能集合及其筛选条件。
    pub fn smart_collections(&self) -> Result<Vec<Collection>> {
        let mut collections = Vec::new();
        let sql = if self.workspace.is_some() {
            "SELECT id,name,query FROM workspace_collections WHERE workspace=?1 ORDER BY name"
        } else {
            "SELECT id,name,query FROM smart_collections WHERE ?1='' ORDER BY name"
        };
        let mut stmt = self.db.prepare(sql)?;
        for row in stmt.query_map([self.workspace_key()], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })? {
            let (id, name, query) = row?;
            collections.push(Collection {
                id,
                name,
                query: toml::from_str(&query)
                    .map_err(|e| Error::Invalid(format!("集合规则损坏：{e}")))?,
            });
        }
        Ok(collections)
    }
    /// 组合数据库字段和媒体读取结果，返回素材详情。
    pub fn details(&self, id: AssetId) -> Result<AssetDetails> {
        Ok(self.details_plan(id)?.read())
    }
    /// 只读取素材详情中的数据库字段，避免重复进行媒体解码。
    pub(super) fn details_metadata(&self, id: AssetId) -> Result<AssetDetails> {
        let notes = self.db.query_row(
            "SELECT notes FROM assets WHERE id=?1",
            [id.to_string()],
            |r| r.get(0),
        )?;
        let projects = self
            .db
            .prepare("SELECT project_id FROM project_assets WHERE asset_id=?1")?
            .query_map([id.to_string()], |r| r.get::<_, String>(0))?
            .map(|r| Ok(ProjectId(parse_id(&r?)?)))
            .collect::<Result<Vec<_>>>()?;
        Ok(AssetDetails {
            project_copies: self.db.prepare("SELECT c.project_id FROM project_copies c JOIN project_assets p ON p.project_id=c.project_id AND p.asset_id=c.asset_id WHERE c.source_asset_id=?1")?.query_map([id.to_string()],|r|r.get::<_,String>(0))?.map(|r|Ok(ProjectId(parse_id(&r?)?))).collect::<Result<Vec<_>>>()?,
            manual_location:self.manual_location(id)?,
            exif_location:None,
            notes,
            projects,
            exif: vec![],
            versions:self.versions(id)?,
            referenced:self.db.query_row("SELECT EXISTS(SELECT 1 FROM file_locations f JOIN assets a ON a.display_version_id=f.version_id \
                WHERE a.id=?1)",
[id.to_string()],
|r|r.get(0))?,

            ..Default::default()
        })
    }
    /// 保存素材说明等编辑字段并刷新相关元数据状态。
    pub fn edit(&mut self, ids: &[AssetId], edit: &Edit) -> Result<()> {
        if ids.is_empty() || ids.len() > 10000 {
            return Err(Error::Invalid("请选择 1～10000 张素材分批操作".into()));
        }
        if matches!(edit,Edit::Rating(n) if *n>5) {
            return Err(Error::Invalid("评分应为 0～5".into()));
        }
        if matches!(edit,Edit::Notes(n) if n.len()>65536) {
            return Err(Error::Invalid("备注不能超过 64 KiB".into()));
        }
        if let Edit::AddTags(tags) | Edit::RemoveTags(tags) = edit
            && (tags.len() > 64 || tags.iter().any(|t| t.len() > 128))
        {
            return Err(Error::Invalid("最多 64 个标签，每个最多 128 字节".into()));
        }
        self.require_workspace_assets(ids)?;
        let at = now_ms()?;
        let mut copies = project_isolation::Copies::default();
        let tx = self.db.transaction()?;
        tx.execute_batch("PRAGMA defer_foreign_keys=ON;")?;
        for id in ids {
            let exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM assets WHERE id=?1)",
                [id.to_string()],
                |r| r.get(0),
            )?;
            if !exists {
                return Err(Error::Invalid("素材已不存在，批次未保存".into()));
            }
            match edit {
                Edit::Trash(trash) => {
                    tx.execute(
                        "UPDATE assets SET deleted_at=?1 WHERE id=?2",
                        params![trash.then_some(at), id.to_string()],
                    )?;
                }
                Edit::Favorite(value) => {
                    tx.execute(
                        "UPDATE assets SET favorite=?1 WHERE id=?2",
                        params![value, id.to_string()],
                    )?;
                }
                Edit::Rating(value) => {
                    tx.execute(
                        "UPDATE assets SET rating=?1 WHERE id=?2",
                        params![value, id.to_string()],
                    )?;
                }
                Edit::Notes(value) => {
                    tx.execute(
                        "UPDATE assets SET notes=?1 WHERE id=?2",
                        params![value, id.to_string()],
                    )?;
                }
                Edit::Project(project, true) => {
                    let target = project.to_string();
                    let asset = id.to_string();
                    let member: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM project_assets WHERE project_id=?1 AND asset_id=?2)", params![target, asset], |r| r.get(0))?;
                    let assigned: bool = tx.query_row(
                        "SELECT EXISTS(SELECT 1 FROM project_assets WHERE asset_id=?1)",
                        [&asset],
                        |r| r.get(0),
                    )?;
                    if !member && assigned {
                        project_isolation::clone_asset(
                            &tx,
                            &self.root,
                            &asset,
                            &target,
                            &mut copies,
                        )?;
                    } else {
                        tx.execute(
                            "INSERT OR IGNORE INTO project_assets VALUES(?1,?2)",
                            params![target, asset],
                        )?;
                        workspace::rehome_asset(&tx, &asset, &target)?;
                    }
                }
                Edit::Project(project, false) => {
                    let asset = id.to_string();
                    let versions = tx
                        .prepare("SELECT id FROM asset_versions WHERE asset_id=?1")?
                        .query_map([&asset], |r| r.get::<_, String>(0))?
                        .collect::<std::result::Result<Vec<_>, _>>()?;
                    let mapping = versions.into_iter().map(|id| (id.clone(), id)).collect();
                    workspace::map_classification(&tx, &asset, &mapping, "")?;
                    tx.execute(
                        "DELETE FROM project_assets WHERE project_id=?1 AND asset_id=?2",
                        params![project.to_string(), id.to_string()],
                    )?;
                    let scope = workspace::asset_scope(&tx, &asset)?;
                    workspace::rehome_asset(&tx, &asset, &scope)?;
                }
                Edit::RemoveTags(tags) => {
                    for tag in tags {
                        tx.execute("DELETE FROM asset_tags WHERE asset_id=?1 AND tag_id IN(SELECT id FROM tags WHERE name=?2)",params![id.to_string(),tag.trim()])?;
                        tx.execute("INSERT OR IGNORE INTO ai_rejected_tags SELECT display_version_id,?2 FROM assets WHERE id=?1", params![id.to_string(), tag.trim()])?;
                    }
                }
                Edit::AddTags(tags) => {
                    for tag in tags.iter().map(|t| t.trim()).filter(|t| !t.is_empty()) {
                        tx.execute("INSERT OR IGNORE INTO tags(name) VALUES(?1)", [tag])?;
                        tx.execute(
                            "INSERT OR IGNORE INTO asset_tags SELECT ?1,id FROM tags WHERE name=?2",
                            params![id.to_string(), tag],
                        )?;
                    }
                }
            }
            tx.execute(
                "UPDATE assets SET edited_at=?1 WHERE id=?2",
                params![at, id.to_string()],
            )?;
        }
        if matches!(edit, Edit::Trash(_)) {
            // Keep the default viewport's rank dense after soft deletion or restoration.
            tx.execute_batch("DELETE FROM asset_order; INSERT INTO asset_order SELECT row_number() OVER(ORDER BY imported_at,id)-1,id FROM assets WHERE deleted_at IS NULL;")?;
        }
        tx.commit()?;
        copies.commit();
        self.snapshot = None;
        Ok(())
    }
    /// 校验项目名称后修改显示名称，保留其身份及成员关联。
    pub fn rename_project(&mut self, id: ProjectId, name: &str) -> Result<()> {
        import::validate_component(name.trim())?;
        if name.trim().is_empty() || name.len() > 120 {
            return Err(Error::Invalid("项目名称应为 1～120 字节".into()));
        }
        if self.db.execute(
            "UPDATE projects SET name=?1 WHERE id=?2",
            params![name.trim(), id.to_string()],
        )? != 1
        {
            return Err(Error::Invalid("项目不存在".into()));
        }
        Ok(())
    }
    /// 删除项目结构及相应关联，按既有规则处理子项目和素材归属。
    pub fn delete_project(&mut self, id: ProjectId) -> Result<()> {
        let tx = self.db.transaction()?;
        tx.execute(
            "DELETE FROM project_assets WHERE project_id=?1",
            [id.to_string()],
        )?;
        tx.execute("DELETE FROM projects WHERE id=?1", [id.to_string()])?;
        tx.commit()?;
        self.snapshot = None;
        Ok(())
    }
    /// 校验并保存智能集合名称和查询条件。
    pub fn save_collection(&mut self, name: &str, query: &Query) -> Result<()> {
        if let Some(rule) = &query.rule {
            rule.validate().map_err(|e| Error::Invalid(e.into()))?;
        }
        if name.trim().is_empty() || name.len() > 120 {
            return Err(Error::Invalid("请输入 1～120 字节的集合名称".into()));
        }
        let mut query = query.clone();
        query.project = self.workspace;
        let rule = toml::to_string(&query).map_err(|e| Error::Invalid(e.to_string()))?;
        if let Some(project) = self.workspace {
            self.db.execute("INSERT INTO workspace_collections VALUES(?1,?2,?3,?4) ON CONFLICT(workspace,name) DO UPDATE SET query=excluded.query", params![Uuid::new_v4().to_string(),name.trim(),rule,project.to_string()])?;
        } else {
            self.db.execute("INSERT INTO smart_collections VALUES(?1,?2,?3) ON CONFLICT(name) DO UPDATE SET query=excluded.query",params![Uuid::new_v4().to_string(),name.trim(),rule])?;
        }
        Ok(())
    }
    /// 删除指定智能集合，保留实际素材及其其他关联。
    pub fn delete_collection(&mut self, id: &str) -> Result<()> {
        if let Some(project) = self.workspace {
            self.db.execute(
                "DELETE FROM workspace_collections WHERE id=?1 AND workspace=?2",
                params![id, project.to_string()],
            )?;
        } else {
            self.db
                .execute("DELETE FROM smart_collections WHERE id=?1", [id])?;
        }
        Ok(())
    }
    /// 解析素材当前展示版本的可用文件路径。
    pub fn asset_path(&self, id: AssetId) -> Result<PathBuf> {
        let version: String = self.db.query_row(
            "SELECT display_version_id FROM assets WHERE id=?1",
            [id.to_string()],
            |r| r.get(0),
        )?;
        self.version_path(&version, true)
    }
    /// 解析指定版本的受管或外部路径，并按参数控制外部访问。
    pub(super) fn version_path(&self, version: &str, verify_reference: bool) -> Result<PathBuf> {
        let reference: Option<(String, Vec<u8>)> = self
            .db
            .query_row(
                "SELECT platform,native_path FROM file_locations WHERE version_id=?1",
                [version],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((platform, bytes)) = reference {
            let path = decode_path(&platform, &bytes)?;
            if !path.is_absolute() {
                return Err(Error::Invalid("引用路径无效".into()));
            }
            reject_network_reference(&path)?;
            if verify_reference {
                let expected: String = self.db.query_row(
                    "SELECT sha256 FROM asset_versions WHERE id=?1",
                    [version],
                    |r| r.get(0),
                )?;
                if hash_file(&path)? != expected {
                    return Err(Error::Invalid(
                        "引用文件已被外部修改，请重新导入为新素材；已保留旧记录".into(),
                    ));
                }
            }
            return Ok(path);
        }
        let relative: String = self.db.query_row(
            "SELECT path FROM asset_versions WHERE id=?1",
            [version],
            |r| r.get(0),
        )?;
        safe_path(&self.root, &relative)
    }
    /// Publish a whole new directory only after every selected original was copied and synced.
    ///
    /// 为所选素材准备导出项目，再按给定选项写出结果。
    pub fn export(&self, ids: &[AssetId], parent: &Path) -> Result<PathBuf> {
        export::run(self.export_plan(ids)?, parent, ExportOptions::default())
    }
    /// Synchronous tools share the same frozen snapshot protocol as the app.
    ///
    /// 生成资源库备份，沿用独立快照与发布校验流程。
    pub fn backup(&self, parent: &Path) -> Result<PathBuf> {
        self.backup_plan(parent, false)?
            .copy(&backup::Control::default())?
            .publish()
    }
    /// 复制资源库并分配新的库身份，使副本可独立打开和管理。
    pub fn clone_library(&self, parent: &Path) -> Result<PathBuf> {
        self.backup_plan(parent, true)?
            .copy(&backup::Control::default())?
            .publish()
    }
    /// Synchronous adapter for tools and library tests. All checking and cache
    /// repair uses the same engine as the application's cancellable worker.
    ///
    /// 同步执行检查或修复，复用维护计划及版本校验逻辑。
    pub fn maintenance(&mut self, repair: bool) -> Result<MaintenanceReport> {
        let plan = self.maintenance_plan();
        let mut report = MaintenanceReport::default();
        let mut failure = None;
        plan.run(repair, &maintenance::Control::default(), |outcome| {
            if let Some(repair) = outcome.record(&mut report) {
                match self.apply_cache_repair(repair) {
                    Ok(applied) => report.repaired += u64::from(applied),
                    Err(error) => {
                        failure = Some(error);
                        return false;
                    }
                }
            }
            true
        })?;
        if let Some(error) = failure {
            return Err(error);
        }
        Ok(report)
    }
}
/// 从缩略图像素估算主色，并映射为固定的颜色筛选值。
pub(super) fn dominant_color(image: &image::DynamicImage) -> &'static str {
    let mut counts = [0u32; 11];
    for pixel in image.thumbnail(32, 32).to_rgb8().pixels() {
        let r = f32::from(pixel[0]) / 255.0;
        let g = f32::from(pixel[1]) / 255.0;
        let b = f32::from(pixel[2]) / 255.0;
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let delta = max - min;
        let index = if max < 0.15 {
            0
        } else if min > 0.86 {
            1
        } else if delta < 0.14 {
            2
        } else {
            let hue = if max == r {
                60.0 * ((g - b) / delta).rem_euclid(6.0)
            } else if max == g {
                60.0 * ((b - r) / delta + 2.0)
            } else {
                60.0 * ((r - g) / delta + 4.0)
            };
            if !(15.0..345.0).contains(&hue) {
                3
            } else if hue < 45.0 {
                4
            } else if hue < 75.0 {
                5
            } else if hue < 165.0 {
                6
            } else if hue < 195.0 {
                7
            } else if hue < 255.0 {
                8
            } else if hue < 285.0 {
                9
            } else {
                10
            }
        };
        counts[index] += 1;
    }
    let index = counts
        .iter()
        .enumerate()
        .max_by_key(|(_, n)| *n)
        .map(|(i, _)| i)
        .unwrap_or(2);
    [
        "黑", "白", "灰", "红", "橙", "黄", "绿", "青", "蓝", "紫", "粉",
    ][index]
}
/// 按当前平台编码本机路径，保留不能用普通 UTF-8 表示的路径数据。
pub(super) fn encode_path(path: &Path) -> Result<(&'static str, Vec<u8>)> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        Ok((
            "windows",
            path.as_os_str()
                .encode_wide()
                .flat_map(u16::to_le_bytes)
                .collect(),
        ))
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        Ok(("unix", path.as_os_str().as_bytes().to_vec()))
    }
}
/// 按登记的平台标识还原本机路径，拒绝不兼容的编码。
pub(super) fn decode_path(platform: &str, bytes: &[u8]) -> Result<PathBuf> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        if platform != "windows" || !bytes.len().is_multiple_of(2) {
            return Err(Error::Invalid(
                "引用文件属于其他操作系统，请在原电脑打开或重新关联".into(),
            ));
        }
        let wide = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|p| u16::from_le_bytes([p[0], p[1]]))
            .collect::<Vec<_>>();
        Ok(std::ffi::OsString::from_wide(&wide).into())
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        if platform != "unix" {
            return Err(Error::Invalid(
                "引用文件属于其他操作系统，请在原电脑打开或重新关联".into(),
            ));
        }
        Ok(std::ffi::OsString::from_vec(bytes.to_vec()).into())
    }
}
/// 分块计算文件 SHA-256 校验和，并在读取期间响应取消。
pub(super) fn hash_file(path: &Path) -> Result<String> {
    let mut input = File::open(path)?;
    if input.metadata()?.len() > 64 * 1024 * 1024 * 1024 {
        return Err(Error::Invalid("文件超出 64 GiB 大小预算".into()));
    }
    let mut hasher = Sha256::new();
    let mut bytes = [0u8; 65536];
    loop {
        crate::cancel::check()?;
        let count = input.read(&mut bytes)?;
        if count == 0 {
            break;
        }
        hasher.update(&bytes[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}
/// 以新建方式复制受管文件并同步写入，拒绝覆盖已存在的目标。
pub(super) fn copy_owned(source: &Path, destination: &Path) -> Result<()> {
    let mut input = File::open(source)?;
    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(destination)?;
    crate::cancel::copy(&mut input, &mut output)?;
    output.sync_all()?;
    Ok(())
}
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    /// 验证分类卡片查询真实分类及未分类素材。
    fn ai_category_cards_filter_real_classification_and_unclassified_assets() {
        let (_dir, mut lib, assets) = fixture();
        lib.set_category(&[assets[0].id], "风景").unwrap();
        lib.set_category(&[assets[1].id], "未分类").unwrap();
        let landscape = lib
            .page(
                &Query {
                    category: "风景".into(),
                    ..Default::default()
                },
                0,
                128,
            )
            .unwrap();
        assert_eq!(landscape.assets.len(), 1);
        assert_eq!(landscape.assets[0].id, assets[0].id);
        let query = Query {
            category: "未分类".into(),
            ..Default::default()
        };
        let unclassified = lib.page(&query, 0, 128).unwrap();
        assert_eq!(unclassified.total, assets.len() as u64 - 1);
        assert!(
            !unclassified
                .assets
                .iter()
                .any(|asset| asset.id == assets[0].id)
        );
        lib.edit(&[assets[2].id], &Edit::Trash(true)).unwrap();
        assert_eq!(
            lib.page(&query, 0, 128).unwrap().total,
            assets.len() as u64 - 2
        );
    }
    #[test]
    /// 验证中断导入使用受管副本恢复，并保留项目和名称。
    fn interrupted_import_recovers_from_owned_copy_with_project_and_name() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("需要恢复.png");
        image::RgbImage::from_pixel(24, 16, image::Rgb([10, 20, 30]))
            .save(&source)
            .unwrap();
        let original = fs::read(&source).unwrap();
        let mut library = Library::create(dir.path(), "恢复库").unwrap();
        let project = library.create_project("项目").unwrap();
        assert!(
            library
                .import_inner(&source, Some(project), true, false, None)
                .is_err()
        );
        let root = library.root().to_path_buf();
        drop(library);
        fs::remove_file(&source).unwrap();
        let mut library = Library::open(&root).unwrap();
        let report = library.recover_imports().unwrap();
        assert_eq!(report.imported, 1);
        assert_eq!(report.failed, 0);
        let page = library
            .page(
                &Query {
                    project: Some(project),
                    ..Default::default()
                },
                0,
                10,
            )
            .unwrap();
        assert_eq!(page.assets.len(), 1);
        assert_eq!(page.assets[0].name, "需要恢复.png");
        assert_eq!(
            fs::read(library.asset_path(page.assets[0].id).unwrap()).unwrap(),
            original
        );
        assert_eq!(library.recover_imports().unwrap().imported, 0);
    }
    /// 准备当前用例的独立素材和测试资源，由临时目录句柄负责清理。
    fn fixture() -> (tempfile::TempDir, Library, Vec<Asset>) {
        let dir = tempfile::tempdir().unwrap();
        let mut library = Library::create(dir.path(), "日常库").unwrap();
        for i in 0..6u8 {
            let path = dir.path().join(format!("测试-{i}.png"));
            image::RgbImage::from_pixel(20 + u32::from(i), 12, image::Rgb([i * 30, 80, 130]))
                .save(&path)
                .unwrap();
            library.import(&path, None).unwrap();
        }
        let assets = library.page(&Query::default(), 0, 128).unwrap().assets;
        (dir, library, assets)
    }
    #[test]
    /// 验证回收站恢复后分页、标签、项目和原片内容保持一致。
    fn trash_restoration_keeps_dense_pages_tags_projects_and_original_bytes() {
        let (dir, mut lib, assets) = fixture();
        let first = assets[1].id;
        let second = assets[4].id;
        let project = lib.create_project("归档").unwrap();
        let original = fs::read(lib.asset_path(first).unwrap()).unwrap();
        lib.edit(&[first], &Edit::AddTags(vec!["纪实".into()]))
            .unwrap();
        lib.edit(&[first], &Edit::Project(project, true)).unwrap();
        lib.edit(&[first, second], &Edit::Trash(true)).unwrap();
        assert_eq!(lib.info().unwrap().count, 4);
        assert_eq!(lib.page(&Query::default(), 2, 2).unwrap().assets.len(), 2);
        assert_eq!(
            lib.page(
                &Query {
                    trash: true,
                    ..Default::default()
                },
                0,
                10
            )
            .unwrap()
            .total,
            2
        );
        assert_eq!(lib.layout_sizes(&Query::default()).unwrap().len(), 4);
        assert_eq!(lib.organizer().unwrap().trash_count, 2);
        lib.edit(&[first, second], &Edit::Trash(false)).unwrap();
        let path = lib.root().to_path_buf();
        drop(lib);
        let mut lib = Library::open(&path).unwrap();
        assert_eq!(lib.info().unwrap().count, 6);
        assert_eq!(lib.details(first).unwrap().projects, vec![project]);
        assert_eq!(
            lib.page(
                &Query {
                    tag: "纪实".into(),
                    ..Default::default()
                },
                0,
                10
            )
            .unwrap()
            .total,
            1
        );
        assert_eq!(fs::read(lib.asset_path(first).unwrap()).unwrap(), original);
        assert!(dir.path().join("测试-0.png").exists());
    }
    #[test]
    /// 验证筛选、集合说明及失败批量编辑遵守事务边界。
    fn filters_collection_notes_and_failed_batch_are_transactional() {
        let (_dir, mut lib, assets) = fixture();
        let id = assets[2].id;
        lib.edit(&[id], &Edit::Rating(4)).unwrap();
        lib.edit(&[id], &Edit::Notes("客户甲：春季拍摄".into()))
            .unwrap();
        let query = Query {
            text: "客户甲".into(),
            min_rating: 4,
            orientation: 1,
            format: "png".into(),
            sort: SortOrder::Name,
            ..Default::default()
        };
        assert_eq!(lib.page(&query, 0, 10).unwrap().total, 1);
        lib.save_collection("客户", &query).unwrap();
        let collection = lib.organizer().unwrap().collections.remove(0);
        assert_eq!(collection.query, query);
        assert!(
            lib.edit(&[id, AssetId::default()], &Edit::Favorite(true))
                .is_err()
        );
        assert!(!lib.page(&query, 0, 10).unwrap().assets[0].favorite);
        let project = lib.create_project("临时项目").unwrap();
        lib.edit(&[id], &Edit::Project(project, true)).unwrap();
        lib.delete_project(project).unwrap();
        assert_eq!(lib.info().unwrap().count, 6);
        assert!(lib.details(id).unwrap().projects.is_empty());
        assert_eq!(
            lib.page(
                &Query {
                    untagged: true,
                    ..Default::default()
                },
                0,
                10
            )
            .unwrap()
            .total,
            6
        );
        let color = lib
            .db
            .query_row(
                "SELECT dominant_color FROM asset_versions WHERE asset_id=?1",
                [id.to_string()],
                |r| r.get::<_, String>(0),
            )
            .unwrap();
        assert!(!color.is_empty());
        assert!(
            lib.page(
                &Query {
                    color,
                    ..Default::default()
                },
                0,
                128
            )
            .unwrap()
            .assets
            .iter()
            .any(|a| a.id == id)
        );
        assert_eq!(lib.organizer().unwrap().edited_count, 1);
    }

    #[test]
    /// 验证嵌套规则参数化、持久化，并受深度与数量预算限制。
    fn nested_rules_are_parameterized_persisted_and_bounded() {
        let (_dir, mut lib, assets) = fixture();
        let first = assets[0].id;
        let second = assets[1].id;
        let literal = "x%' OR 1=1 --";
        lib.edit(&[first], &Edit::AddTags(vec![literal.into()]))
            .unwrap();
        lib.edit(&[second], &Edit::Favorite(true)).unwrap();
        lib.edit(&[second], &Edit::Rating(4)).unwrap();
        let mut query = Query {
            rule: Some(Rule::All(vec![
                Rule::Any(vec![Rule::Tag(literal.into()), Rule::Favorite]),
                Rule::Not(Box::new(Rule::Rating(4))),
                Rule::Format("PNG".into()),
            ])),
            ..Default::default()
        };
        assert_eq!(lib.page(&query, 0, 10).unwrap().assets[0].id, first);
        assert_eq!(lib.page(&query, 0, 10).unwrap().total, 1);
        query.text = "测试".into();
        assert_eq!(lib.page(&query, 0, 10).unwrap().total, 1);
        query.text = "不存在".into();
        assert_eq!(lib.page(&query, 0, 10).unwrap().total, 0);
        query.text.clear();
        lib.save_collection("组合", &query).unwrap();
        let root = lib.root().to_path_buf();
        drop(lib);
        let mut lib = Library::open(&root).unwrap();
        let stored = lib.organizer().unwrap().collections.remove(0).query;
        assert_eq!(stored, query);
        assert_eq!(lib.page(&stored, 0, 10).unwrap().total, 1);
        lib.edit(&[first], &Edit::Trash(true)).unwrap();
        assert_eq!(lib.page(&stored, 0, 10).unwrap().total, 0);
        let mut trashed = stored.clone();
        trashed.trash = true;
        assert_eq!(lib.page(&trashed, 0, 10).unwrap().total, 1);
        for invalid in [
            Rule::All(vec![]),
            Rule::Any(vec![Rule::Favorite; 65]),
            Rule::Name("".into()),
            Rule::Rating(6),
        ] {
            let q = Query {
                rule: Some(invalid),
                ..Default::default()
            };
            assert!(lib.page(&q, 0, 10).is_err());
            assert!(lib.save_collection("不应保存", &q).is_err());
        }
        let mut deep = Rule::Favorite;
        for _ in 0..10 {
            deep = Rule::Not(Box::new(deep));
        }
        assert!(deep.validate().is_err());
    }
    #[test]
    /// 验证快照备份、导出和缓存修复保留持久化数据。
    fn snapshot_backup_export_and_cache_repair_keep_durable_data() {
        let (dir, mut lib, assets) = fixture();
        let id = assets[0].id;
        lib.description("真实的资源库描述").unwrap();
        lib.set_cover(&dir.path().join("测试-0.png")).unwrap();
        lib.edit(&[id], &Edit::Notes("持久备注".into())).unwrap();
        lib.edit(&[id], &Edit::Trash(true)).unwrap();
        let original = fs::read(lib.asset_path(id).unwrap()).unwrap();
        let exported = lib.export(&[id], dir.path()).unwrap();
        let file = fs::read_dir(exported)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert_eq!(fs::read(file).unwrap(), original);
        assert!(lib.backup(lib.root()).is_err());
        let backup = lib.backup(dir.path()).unwrap();
        drop(lib);
        let mut restored = Library::open(&backup).unwrap();
        assert_eq!(
            restored.organizer().unwrap().description,
            "真实的资源库描述"
        );
        assert!(restored.cover().unwrap().is_some());
        assert_eq!(restored.info().unwrap().count, 5);
        assert_eq!(restored.details(id).unwrap().notes, "持久备注");
        assert_eq!(restored.organizer().unwrap().trash_count, 1);
        assert_eq!(
            fs::read(restored.asset_path(id).unwrap()).unwrap(),
            original
        );
        let report = restored.maintenance(true).unwrap();
        assert_eq!(report.checked, 6);
        assert_eq!(report.repaired, 6);
        assert_eq!(report.damaged, 0);
        restored.edit(&[id], &Edit::Trash(false)).unwrap();
        assert_eq!(
            restored
                .page(&Query::default(), 0, 128)
                .unwrap()
                .assets
                .len(),
            6
        );
    }
    #[test]
    /// 验证外部引用文件被修改后不会冒充已登记版本。
    fn reference_file_changes_never_replace_registered_version() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("外部.png");
        image::RgbImage::from_pixel(10, 8, image::Rgb([1, 2, 3]))
            .save(&source)
            .unwrap();
        let mut lib = Library::create(dir.path(), "引用库").unwrap();
        lib.import_referenced(&source, None).unwrap();
        let asset = lib.page(&Query::default(), 0, 1).unwrap().assets.remove(0);
        assert!(lib.details(asset.id).unwrap().referenced);
        assert_eq!(
            lib.asset_path(asset.id).unwrap(),
            source.canonicalize().unwrap()
        );
        assert_eq!(lib.preview(asset.id).unwrap().width, 10);
        image::RgbImage::from_pixel(10, 8, image::Rgb([4, 5, 6]))
            .save(&source)
            .unwrap();
        assert!(lib.preview(asset.id).is_err());
        assert_eq!(lib.info().unwrap().count, 1);
        assert_eq!(lib.maintenance(false).unwrap().damaged, 1);
        fs::remove_file(&source).unwrap();
        assert_eq!(lib.maintenance(false).unwrap().missing, 1);
        assert_eq!(lib.info().unwrap().count, 1);
    }
    #[test]
    /// 验证版本 1 资源库升级保留身份并生成迁移前备份。
    fn version_one_migration_preserves_identity_and_keeps_backup() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("旧库.moriq");
        fs::create_dir(&root).unwrap();
        let id = LibraryId::default();
        let manifest = Manifest {
            format: "moriq".into(),
            library_id: id,
            name: "旧库".into(),
            schema_version: 1,
            min_reader_version: 1,
            created_at_utc: 123,
        };
        fs::write(
            root.join("library.toml"),
            toml::to_string(&manifest).unwrap(),
        )
        .unwrap();
        {
            let db = Connection::open(root.join("catalog.db")).unwrap();
            db.execute_batch(include_str!("../database/schema.sql"))
                .unwrap();
            db.execute("INSERT INTO library_identity VALUES(?1)", [id.to_string()])
                .unwrap();
        }
        let lib = Library::open(&root).unwrap();
        assert_eq!(lib.id(), id);
        assert_eq!(lib.manifest.schema_version, SCHEMA);
        assert_eq!(lib.organizer().unwrap().trash_count, 0);
        assert_eq!(fs::read_dir(root.join("backups")).unwrap().count(), 1);
        drop(lib);
        assert_eq!(Library::open(&root).unwrap().id(), id);
    }
    #[test]
    /// 验证 EXIF 从真实 JPEG 读取且不改动来源。
    fn exif_is_read_from_actual_jpeg_without_changing_source() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("相机.jpg");
        image::RgbImage::from_pixel(12, 8, image::Rgb([4, 5, 6]))
            .save(&path)
            .unwrap();
        let original = fs::read(&path).unwrap();
        let tiff=b"II\x2a\x00\x08\x00\x00\x00\x01\x00\x10\x01\x02\x00\x06\x00\x00\x00\x1a\x00\x00\x00\x00\x00\x00\x00Canon\x00";
        let mut tagged = original[..2].to_vec();
        tagged.extend_from_slice(&[0xff, 0xe1, 0, 40]);
        tagged.extend_from_slice(b"Exif\0\0");
        tagged.extend_from_slice(tiff);
        tagged.extend_from_slice(&original[2..]);
        fs::write(&path, &tagged).unwrap();
        let mut lib = Library::create(dir.path(), "EXIF库").unwrap();
        lib.import(&path, None).unwrap();
        let id = lib.page(&Query::default(), 0, 1).unwrap().assets[0].id;
        let details = lib.details(id).unwrap();
        assert!(
            details
                .exif
                .iter()
                .any(|(key, value)| key == "相机型号" && value.contains("Canon"))
        );
        assert_eq!(fs::read(path).unwrap(), tagged);
    }
}
