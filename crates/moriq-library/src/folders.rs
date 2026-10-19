use super::*;
impl Library {
    pub fn virtual_folders(&self) -> Result<Vec<VirtualFolder>> {
        let mut stmt=self.db.prepare("SELECT f.id,f.name,f.kind,coalesce((SELECT a.id FROM assets a JOIN virtual_folder_assets m ON m.asset_id=a.id WHERE m.folder_id=f.id AND a.deleted_at IS NULL ORDER BY a.id=f.cover_id DESC,a.imported_at DESC LIMIT 1),NULL),(SELECT count(*) FROM virtual_folder_assets m JOIN assets a ON a.id=m.asset_id WHERE m.folder_id=f.id AND a.deleted_at IS NULL) FROM virtual_folders f ORDER BY f.kind DESC,f.name")?;
        let values = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, bool>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, u64>(4)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        values
            .into_iter()
            .map(|(id, name, favorite_group, cover, count)| {
                Ok(VirtualFolder {
                    id,
                    name,
                    favorite_group,
                    cover: cover.map(|id| parse_id(&id).map(AssetId)).transpose()?,
                    count,
                })
            })
            .collect()
    }
    pub fn create_virtual_folder(&mut self, name: &str, favorite_group: bool) -> Result<String> {
        let name = name.trim();
        if name.is_empty() || name.len() > 256 {
            return Err(Error::Invalid("分组名称需要 1～256 字节".into()));
        }
        let id = Uuid::new_v4().to_string();
        self.db.execute(
            "INSERT INTO virtual_folders(id,name,kind,created_at) VALUES(?1,?2,?3,?4)",
            params![id, name, favorite_group, now_ms()?],
        )?;
        Ok(id)
    }
    pub fn rename_virtual_folder(&mut self, id: &str, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() || name.len() > 256 {
            return Err(Error::Invalid("分组名称无效".into()));
        }
        if self.db.execute(
            "UPDATE virtual_folders SET name=?2 WHERE id=?1",
            params![id, name],
        )? == 0
        {
            return Err(Error::Invalid("分组已不存在".into()));
        }
        Ok(())
    }
    pub fn delete_virtual_folder(&mut self, id: &str) -> Result<()> {
        self.db
            .execute("DELETE FROM virtual_folders WHERE id=?1", [id])?;
        self.snapshot = None;
        Ok(())
    }
    pub fn virtual_folder_members(
        &mut self,
        folder: &str,
        ids: &[AssetId],
        add: bool,
    ) -> Result<()> {
        if ids.is_empty() || ids.len() > 10000 {
            return Err(Error::Invalid("请选择 1～10000 个素材".into()));
        }
        let tx = self.db.transaction()?;
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM virtual_folders WHERE id=?1)",
            [folder],
            |r| r.get(0),
        )?;
        if !exists {
            return Err(Error::Invalid("分组已不存在".into()));
        }
        for id in ids {
            if add {
                tx.execute(
                    "INSERT OR IGNORE INTO virtual_folder_assets VALUES(?1,?2)",
                    params![folder, id.to_string()],
                )?;
            } else {
                tx.execute(
                    "DELETE FROM virtual_folder_assets WHERE folder_id=?1 AND asset_id=?2",
                    params![folder, id.to_string()],
                )?;
            }
        }
        tx.commit()?;
        self.snapshot = None;
        Ok(())
    }
    pub fn virtual_folder_cover(&mut self, folder: &str, asset: AssetId) -> Result<()> {
        let member: bool = self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM virtual_folder_assets WHERE folder_id=?1 AND asset_id=?2)",
            params![folder, asset.to_string()],
            |r| r.get(0),
        )?;
        if !member {
            return Err(Error::Invalid("封面需要是分组内的素材".into()));
        }
        self.db.execute(
            "UPDATE virtual_folders SET cover_id=?2 WHERE id=?1",
            params![folder, asset.to_string()],
        )?;
        Ok(())
    }
}
