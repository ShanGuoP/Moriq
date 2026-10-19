use super::*;
#[derive(Debug, Clone)]
pub struct ReferenceCandidate {
    pub version: String,
    pub asset: AssetId,
    pub hash: String,
    pub original: Vec<u8>,
    pub replacement: PathBuf,
    allowed_root: PathBuf,
    verified: Option<(String, u64, SystemTime)>,
}
impl ReferenceCandidate {
    pub fn verify(self) -> Result<Self> {
        let actual = self.replacement.canonicalize()?;
        if !actual.starts_with(&self.allowed_root) || !actual.is_file() {
            return Err(Error::Invalid("新路径已离开所选磁盘目录".into()));
        }
        let before = fs::metadata(&actual)?;
        let identity = format!("{:?}", file_id::get_file_id(&actual)?);
        if organize::hash_file(&actual)? != self.hash {
            return Err(Error::Invalid("新路径文件内容与原片不一致".into()));
        }
        let after = fs::metadata(&actual)?;
        if before.len() != after.len()
            || before.modified()? != after.modified()?
            || identity != format!("{:?}", file_id::get_file_id(&actual)?)
        {
            return Err(Error::Invalid("校验期间文件已改变，请重新连接".into()));
        }
        Ok(Self {
            verified: Some((identity, after.len(), after.modified()?)),
            replacement: actual,
            ..self
        })
    }
}
impl Library {
    pub fn saved_navigation(&self) -> Result<Option<SavedNavigation>> {
        let value: Option<String> = self
            .db
            .query_row(
                "SELECT value FROM library_preferences WHERE key='navigation_v1'",
                [],
                |r| r.get(0),
            )
            .optional()?;
        value
            .map(|value| {
                if value.len() > 512 * 1024 {
                    return Err(Error::Invalid("浏览历史超出预算".into()));
                }
                serde_json::from_str(&value).map_err(|e| Error::Invalid(e.to_string()))
            })
            .transpose()
    }
    pub fn save_navigation(&self, value: &SavedNavigation) -> Result<()> {
        if value.back.len() > 32
            || value.forward.len() > 32
            || std::iter::once(&value.current)
                .chain(&value.back)
                .chain(&value.forward)
                .any(|v| {
                    v.screen > 2
                        || !v.scroll_y.is_finite()
                        || v.scroll_y < 0.
                        || v.scroll_y > 1e9
                        || !v.card_width.is_finite()
                        || v.query.semantic_ids.len() > 200
                })
        {
            return Err(Error::Invalid("浏览历史范围无效".into()));
        }
        let value = serde_json::to_string(value).map_err(|e| Error::Invalid(e.to_string()))?;
        if value.len() > 512 * 1024 {
            return Err(Error::Invalid("浏览历史超出预算".into()));
        }
        self.db.execute("INSERT INTO library_preferences VALUES('navigation_v1',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value WHERE value<>excluded.value",[value])?;
        Ok(())
    }
    pub fn reconnect_page(
        &self,
        old_root: &Path,
        new_root: &Path,
        after: &str,
    ) -> Result<(String, Vec<ReferenceCandidate>)> {
        if !old_root.is_absolute()
            || old_root
                .components()
                .any(|c| matches!(c, Component::ParentDir))
        {
            return Err(Error::Invalid("原磁盘目录必须是没有 .. 的绝对路径".into()));
        }
        let new_root = new_root.canonicalize()?;
        let mut statement=self.db.prepare("SELECT v.id,v.asset_id,v.sha256,f.platform,f.native_path FROM file_locations f JOIN asset_versions v ON v.id=f.version_id WHERE v.id>?1 ORDER BY v.id LIMIT 256")?;
        let mut rows = statement.query([after])?;
        let mut cursor = after.to_string();
        let mut candidates = Vec::new();
        while let Some(row) = rows.next()? {
            let version: String = row.get(0)?;
            cursor = version.clone();
            let original: Vec<u8> = row.get(4)?;
            let path = organize::decode_path(&row.get::<_, String>(3)?, &original)?;
            let Ok(relative) = path.strip_prefix(old_root) else {
                continue;
            };
            if relative
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
            {
                continue;
            }
            let replacement = new_root.join(relative);
            if replacement
                .canonicalize()
                .is_ok_and(|p| !p.starts_with(&new_root))
            {
                continue;
            }
            candidates.push(ReferenceCandidate {
                allowed_root: new_root.clone(),
                verified: None,
                version,
                asset: AssetId(parse_id(&row.get::<_, String>(1)?)?),
                hash: row.get(2)?,
                original,
                replacement,
            });
        }
        Ok((cursor, candidates))
    }
    pub fn apply_reconnect(&mut self, candidate: ReferenceCandidate) -> Result<bool> {
        let Some((identity, bytes, modified)) = &candidate.verified else {
            return Err(Error::Invalid("源文件尚未通过内容校验".into()));
        };
        let Ok(actual) = candidate.replacement.canonicalize() else {
            return Ok(false);
        };
        if actual != candidate.replacement || !actual.starts_with(&candidate.allowed_root) {
            return Ok(false);
        }
        let metadata = fs::metadata(&actual)?;
        if metadata.len() != *bytes
            || metadata.modified()? != *modified
            || format!("{:?}", file_id::get_file_id(&actual)?) != *identity
        {
            return Ok(false);
        }
        let current:Option<(String,Vec<u8>)>=self.db.query_row("SELECT v.sha256,f.native_path FROM file_locations f JOIN asset_versions v ON v.id=f.version_id WHERE v.id=?1 AND v.asset_id=?2",params![candidate.version,candidate.asset.to_string()],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        if current
            .as_ref()
            .is_none_or(|(hash, path)| *hash != candidate.hash || *path != candidate.original)
        {
            return Ok(false);
        }
        let (platform, native) = organize::encode_path(&candidate.replacement)?;
        self.db.execute(
            "UPDATE file_locations SET platform=?2,native_path=?3 WHERE version_id=?1",
            params![candidate.version, platform, native],
        )?;
        self.snapshot = None;
        Ok(true)
    }
}
