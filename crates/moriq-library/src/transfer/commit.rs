use super::*;

impl Library {
    pub fn import(&mut self, source: &Path, project: Option<ProjectId>) -> Result<bool> {
        self.snapshot = None;
        self.import_inner(source, project, false, false, None)
    }

    pub(super) fn import_inner(
        &mut self,
        source: &Path,
        project: Option<ProjectId>,
        interrupt_after_install: bool,
        referenced: bool,
        relative_source: Option<&Path>,
    ) -> Result<bool> {
        self.last_import = ImportMetrics::default();
        let stage_started = std::time::Instant::now();
        let input = File::open(source)?;
        let length = input.metadata()?.len();
        let limit = native::file_limit(source);
        if length == 0 || length > limit {
            return Err(Error::Invalid("媒体超出文件大小预算或为空文件".into()));
        }
        let asset = AssetId::default();
        let version = AssetVersionId::default();
        let operation = Uuid::new_v4().to_string();
        let staged_relative = format!("staging/{operation}.part");
        self.db.execute(
            "INSERT INTO operation_journal VALUES(?1,?2,'prepared')",
            params![operation, staged_relative],
        )?;
        let (platform, native_source) = organize::encode_path(&source.canonicalize()?)?;
        self.db.execute(
            "INSERT INTO import_context(operation_id,platform,source,project_id,referenced,relative_path) VALUES(?1,?2,?3,?4,?5,?6)",
            params![
                operation,
                platform,
                native_source,
                project.map(|p| p.to_string()),
                referenced,
                relative_source.map(|p|p.to_str().ok_or_else(||Error::Invalid("源目录无法无损表示".into()))).transpose()?
            ],
        )?;
        let staged = safe_path(&self.root, &staged_relative)?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staged)?;
        let copied = std::io::copy(&mut input.take(limit + 1), &mut output)?;
        output.sync_all()?;
        drop(output);
        if copied > limit || copied != length {
            return Err(Error::Invalid("读取期间媒体超出文件大小预算".into()));
        }
        ingest::staged_receipt(&self.root, &operation, copied)?;
        self.last_import.copy_ms = stage_started.elapsed().as_secs_f64() * 1000.0;
        let result = self.install(
            &staged,
            asset,
            version,
            &operation,
            source,
            project,
            interrupt_after_install,
            referenced,
            relative_source,
        );
        match result {
            Ok(imported) => {
                let _ = fs::remove_file(safe_path(
                    &self.root,
                    &format!("staging/{operation}.ready"),
                )?);
                if let Err(error) = fs::remove_file(&staged) {
                    tracing::warn!(%error,operation,"committed import staging cleanup deferred");
                }
                // Duplicate skips also finish their journal; they never install another version.
                if !imported {
                    self.db.execute(
                        "UPDATE operation_journal SET phase='committed' WHERE id=?1",
                        [&operation],
                    )?;
                }
                self.forget_import(&operation)?;
                Ok(imported)
            }
            Err(error) => {
                // Keep the owned staging copy and journal. Recovery never deletes user sources.
                self.db.execute(
                    "UPDATE operation_journal SET phase='interrupted' WHERE id=?1",
                    [&operation],
                )?;
                tracing::warn!(operation, "import interrupted; staging retained");
                Err(error)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn install(
        &mut self,
        staged: &Path,
        asset: AssetId,
        version: AssetVersionId,
        operation: &str,
        source: &Path,
        project: Option<ProjectId>,
        interrupt: bool,
        referenced: bool,
        relative_source: Option<&Path>,
    ) -> Result<bool> {
        let prepared = ingest::prepare(staged, operation, source)?;
        let result = self.install_prepared(
            staged,
            asset,
            version,
            operation,
            source,
            project,
            interrupt,
            referenced,
            relative_source,
            prepared,
        );
        if matches!(result, Ok(false)) {
            let _ = fs::remove_file(staged.with_file_name(format!("{operation}.thumb")));
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn install_prepared(
        &mut self,
        staged: &Path,
        asset: AssetId,
        version: AssetVersionId,
        operation: &str,
        source: &Path,
        project: Option<ProjectId>,
        interrupt: bool,
        referenced: bool,
        relative_source: Option<&Path>,
        prepared: ingest::Prepared,
    ) -> Result<bool> {
        let ingest::Prepared {
            hash,
            width,
            height,
            extension,
            dominant_color,
            metrics,
            reference_verified,
            video,
            signature,
            metadata,
        } = prepared;
        let copy_ms = self.last_import.copy_ms;
        self.last_import = metrics;
        self.last_import.copy_ms = copy_ms;
        let duplicate: Option<String> = self
            .db
            .query_row(
                "SELECT v.asset_id FROM asset_versions v JOIN assets a ON a.id=v.asset_id WHERE v.sha256=?1 AND v.bytes=?2 AND \
                 ((?3 IS NULL AND NOT EXISTS(SELECT 1 FROM project_assets p WHERE p.asset_id=a.id)) OR \
                  (?3 IS NOT NULL AND NOT EXISTS(SELECT 1 FROM project_assets p WHERE p.asset_id=a.id AND p.project_id<>?3))) \
                 ORDER BY a.deleted_at IS NOT NULL LIMIT 1",
                params![hash, fs::metadata(staged)?.len(), project.map(|p| p.to_string())],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(existing) = duplicate {
            let asset = AssetId(parse_id(&existing)?);
            if self.asset_trash_state(asset)? == Some(true) {
                self.edit(&[asset], &Edit::Trash(false))?;
            }
            if !referenced && let Some(project) = project {
                let existing_version: String = self.db.query_row(
                    "SELECT id FROM asset_versions WHERE sha256=?1 AND bytes=?2 AND asset_id=?3 LIMIT 1",
                    params![hash, fs::metadata(staged)?.len(), existing],
                    |r| r.get(0),
                )?;
                self.duplicate_project_file(
                    source,
                    staged,
                    project,
                    relative_source,
                    AssetVersionId(parse_id(&existing_version)?),
                )?;
            }
            if let Some(project) = project {
                self.db.execute(
                    "INSERT OR IGNORE INTO project_assets VALUES(?1,?2)",
                    params![project.to_string(), existing],
                )?;
            }
            return Ok(false);
        }
        let relative = if referenced {
            format!("assets/referenced/{version}.{extension}")
        } else {
            self.managed_location(source, project, relative_source, None)?
        };
        let destination = safe_path(&self.root, &relative)?;
        let thumb_relative = format!("thumbnails/256/{version}.png");
        let thumb_path = safe_path(&self.root, &thumb_relative)?;
        let thumb_stage = safe_path(&self.root, &format!("staging/{operation}.thumb"))?;
        let commit_started = std::time::Instant::now();
        self.db.execute(
            "UPDATE operation_journal SET path=?2 WHERE id=?1",
            params![operation, relative],
        )?;
        // Hard-link installation is create-only and on the same volume as staging.
        if !referenced {
            fs::hard_link(staged, &destination)?;
        }
        fs::hard_link(&thumb_stage, &thumb_path)?;
        if interrupt {
            return Err(Error::Invalid("injected interruption".into()));
        }
        let name = source
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "未命名图片".into());
        let transaction = self.db.transaction()?;
        transaction.execute(
            "INSERT INTO assets(id,display_version_id,name,imported_at) VALUES(?1,?2,?3,?4)",
            params![asset.to_string(), version.to_string(), name, now_ms()?],
        )?;
        transaction.execute(
            "INSERT INTO asset_versions(id,asset_id,origin,path,sha256,format,width,height,bytes) \
                VALUES(?1,?2,'original',?3,?4,?5,?6,?7,?8)",
            params![
                version.to_string(),
                asset.to_string(),
                relative,
                hash,
                extension,
                width,
                height,
                fs::metadata(staged)?.len()
            ],
        )?;

        if let Some(metadata) = video {
            transaction.execute(
                "INSERT INTO video_metadata(version_id,json) VALUES(?1,?2)",
                params![
                    version.to_string(),
                    serde_json::to_string(&metadata).map_err(|e| Error::Invalid(e.to_string()))?
                ],
            )?;
        }
        if let Some(signature) = signature {
            similarity::store(&transaction, &version.to_string(), &signature)?;
        }
        metadata::store(&transaction, &version.to_string(), &metadata)?;
        transaction.execute(
            "UPDATE asset_versions SET dominant_color=?1 WHERE id=?2",
            params![dominant_color, version.to_string()],
        )?;
        if referenced {
            if !reference_verified && organize::hash_file(source)? != hash {
                return Err(Error::Invalid("引用源在导入期间发生变化，请重试".into()));
            }
            let (platform, path) = organize::encode_path(&source.canonicalize()?)?;
            transaction.execute(
                "INSERT INTO file_locations VALUES(?1,?2,?3)",
                params![version.to_string(), platform, path],
            )?;
        }
        if let Some(project) = project {
            transaction.execute(
                "INSERT INTO project_assets VALUES(?1,?2)",
                params![project.to_string(), asset.to_string()],
            )?;
        }
        transaction.execute(
            "UPDATE operation_journal SET phase='committed' WHERE id=?1",
            [operation],
        )?;
        transaction.commit()?;
        self.last_import.commit_ms = commit_started.elapsed().as_secs_f64() * 1000.0;
        if let Err(error) = fs::remove_file(thumb_stage) {
            tracing::warn!(%error,operation,"committed thumbnail staging cleanup deferred");
        }
        Ok(true)
    }
}
