use super::*;

impl Library {
    /// 校验素材名称并更新编辑时间，同时使旧查询快照失效。
    pub fn rename_asset(&mut self, id: AssetId, name: &str) -> Result<()> {
        if name.trim().is_empty() || name.len() > 512 {
            return Err(Error::Invalid("素材名称不能为空且最多 512 字节".into()));
        }
        self.snapshot = None;
        if self.db.execute(
            "UPDATE assets SET name=?1,edited_at=?3 WHERE id=?2",
            params![name.trim(), id.to_string(), now_ms()?],
        )? != 1
        {
            return Err(Error::Invalid("找不到需要重命名的素材".into()));
        }
        Ok(())
    }

    /// 更新素材收藏标记和编辑时间，使后续收藏筛选读取新状态。
    pub fn set_favorite(&mut self, id: AssetId, favorite: bool) -> Result<()> {
        self.snapshot = None;
        self.db.execute(
            "UPDATE assets SET favorite=?1,edited_at=?3 WHERE id=?2",
            params![favorite, id.to_string(), now_ms()?],
        )?;
        Ok(())
    }

    /// 校验评分范围并保存评分，同时使旧查询快照失效。
    pub fn set_rating(&mut self, id: AssetId, rating: u8) -> Result<()> {
        self.snapshot = None;
        if rating > 5 {
            return Err(Error::Invalid("评分只能是 0 至 5".into()));
        }
        self.db.execute(
            "UPDATE assets SET rating=?1,edited_at=?3 WHERE id=?2",
            params![rating, id.to_string(), now_ms()?],
        )?;
        Ok(())
    }

    /// 保存人工标签及对自动标签的取舍，保留未被人工覆盖的 AI 来源信息。
    pub fn set_tags(&mut self, id: AssetId, tags: &[String]) -> Result<()> {
        self.snapshot = None;
        if tags.len() > 64 || tags.iter().any(|s| s.len() > 128) {
            return Err(Error::Invalid("最多 64 个标签，每个最多 128 字节".into()));
        }
        let tx = self.db.transaction()?;
        let version: String = tx.query_row(
            "SELECT display_version_id FROM assets WHERE id=?1",
            [id.to_string()],
            |r| r.get(0),
        )?;
        let automatic = tx
            .prepare("SELECT name FROM ai_visible_tags WHERE version_id=?1")?
            .query_map([&version], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let manual = tx
            .prepare(
                "SELECT t.name FROM tags t JOIN asset_tags x ON x.tag_id=t.id WHERE x.asset_id=?1",
            )?
            .query_map([id.to_string()], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for tag in &automatic {
            if !tags.iter().any(|s| s.trim() == tag) {
                ai_content::reject_tag(&tx, &version, tag)?;
            }
        }
        tx.execute("DELETE FROM asset_tags WHERE asset_id=?1", [id.to_string()])?;
        for tag in tags.iter().map(|s| s.trim()).filter(|s| !s.is_empty()) {
            if automatic.iter().any(|s| s == tag) && !manual.iter().any(|s| s == tag) {
                continue;
            }
            tx.execute("INSERT OR IGNORE INTO tags(name) VALUES(?1)", [tag])?;
            tx.execute(
                "INSERT OR IGNORE INTO asset_tags SELECT ?1,id FROM tags WHERE name=?2",
                params![id.to_string(), tag],
            )?;
        }
        tx.execute(
            "UPDATE assets SET edited_at=?1 WHERE id=?2",
            params![now_ms()?, id.to_string()],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// 校验项目名称并在事务中创建项目及其独立 AI 配置。
    pub fn create_project(&self, name: &str) -> Result<ProjectId> {
        import::validate_component(name.trim())?;
        if name.trim().is_empty() || name.len() > 120 {
            return Err(Error::Invalid("项目名不能为空且最多 120 字节".into()));
        }
        let id = ProjectId::default();
        let profile = self.ai_profile_for("")?;
        let tx = self.db.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO projects(id,name) VALUES(?1,?2)",
            params![id.to_string(), name.trim()],
        )?;
        workspace::insert_profile(&tx, id, &profile)?;
        tx.commit()?;
        Ok(id)
    }
}
