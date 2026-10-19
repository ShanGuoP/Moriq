use super::*;
impl Library {
    pub fn virtual_folders(&self) -> Result<Vec<VirtualFolder>> {
        // SQLite permits outer references in WHERE, but not this subquery's ORDER BY.
        let mut stmt = self.db.prepare(
            "SELECT f.id,f.name,f.kind,
             coalesce(
               (SELECT a.id FROM assets a JOIN virtual_folder_assets m ON m.asset_id=a.id
                WHERE m.folder_id=f.id AND a.id=f.cover_id AND a.deleted_at IS NULL),
               (SELECT a.id FROM assets a JOIN virtual_folder_assets m ON m.asset_id=a.id
                WHERE m.folder_id=f.id AND a.deleted_at IS NULL
                ORDER BY a.imported_at DESC,a.id DESC LIMIT 1)),
             (SELECT count(*) FROM virtual_folder_assets m JOIN assets a ON a.id=m.asset_id
              WHERE m.folder_id=f.id AND a.deleted_at IS NULL)
             FROM virtual_folders f ORDER BY f.kind DESC,f.name",
        )?;
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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn broken_folder_and_collection_reads_do_not_hide_jobs_tags_or_statistics() {
        let dir = tempfile::tempdir().unwrap();
        let mut lib = Library::create(dir.path(), "隔离读取").unwrap();
        let source = dir.path().join("test.png");
        image::RgbImage::from_pixel(2, 2, image::Rgb([1, 2, 3]))
            .save(&source)
            .unwrap();
        lib.import(&source, None).unwrap();
        let asset = lib.page(&Query::default(), 0, 1).unwrap().assets[0].id;
        lib.set_tags(asset, &["keep".into()]).unwrap();
        lib.enqueue_media_job("integrity", 0).unwrap();
        lib.db.execute_batch("ALTER TABLE virtual_folders RENAME TO unavailable_folders; INSERT INTO smart_collections(id,name,query) VALUES('broken','bad','this is invalid TOML');").unwrap();
        let result = lib.organizer().unwrap();
        assert_eq!(result.errors.len(), 2);
        assert!(
            result
                .errors
                .iter()
                .any(|(s, _)| *s == OrganizerSection::Folders)
        );
        assert!(
            result
                .errors
                .iter()
                .any(|(s, _)| *s == OrganizerSection::Collections)
        );
        assert_eq!(result.tags, vec![("keep".into(), 1)]);
        assert_eq!(result.jobs.len(), 1);
        assert_eq!(result.recent_count, 1);
        let work = lib.claim_job_step().unwrap().unwrap();
        lib.finish_job_step(work.run()).unwrap();
        assert!(lib.claim_job_step().unwrap().is_none());
        assert_eq!(lib.background_jobs().unwrap()[0].state, "completed");
        lib.db.execute_batch("ALTER TABLE unavailable_folders RENAME TO virtual_folders; DELETE FROM smart_collections WHERE id='broken';").unwrap();
        assert!(lib.organizer().unwrap().errors.is_empty());
    }
    #[test]
    fn covers_use_live_members_and_fall_back_deterministically() {
        let dir = tempfile::tempdir().unwrap();
        let mut lib = Library::create(dir.path(), "分组封面").unwrap();
        assert!(lib.virtual_folders().unwrap().is_empty());
        let folder = lib.create_virtual_folder("收藏分组", true).unwrap();
        let empty = lib.virtual_folders().unwrap().remove(0);
        assert_eq!((empty.cover, empty.count), (None, 0));
        let mut ids = Vec::new();
        for n in 0..3 {
            let path = dir.path().join(format!("{n}.png"));
            image::RgbImage::from_pixel(2, 2, image::Rgb([n, 20, 30]))
                .save(&path)
                .unwrap();
            lib.import(&path, None).unwrap();
            let id = lib
                .page(
                    &Query {
                        text: format!("{n}.png"),
                        ..Default::default()
                    },
                    0,
                    1,
                )
                .unwrap()
                .assets[0]
                .id;
            lib.db
                .execute(
                    "UPDATE assets SET imported_at=?2 WHERE id=?1",
                    params![id.to_string(), n],
                )
                .unwrap();
            ids.push(id);
        }
        lib.virtual_folder_members(&folder, &ids[..2], true)
            .unwrap();
        assert_eq!(lib.virtual_folders().unwrap()[0].cover, Some(ids[1]));
        lib.virtual_folder_cover(&folder, ids[0]).unwrap();
        assert_eq!(lib.virtual_folders().unwrap()[0].cover, Some(ids[0]));
        lib.edit(&[ids[0]], &Edit::Trash(true)).unwrap();
        let live = lib.virtual_folders().unwrap().remove(0);
        assert_eq!((live.cover, live.count), (Some(ids[1]), 1));
        lib.edit(&[ids[0]], &Edit::Trash(false)).unwrap();
        assert_eq!(lib.virtual_folders().unwrap()[0].cover, Some(ids[0]));
        lib.virtual_folder_members(&folder, &[ids[0]], false)
            .unwrap();
        assert_eq!(lib.virtual_folders().unwrap()[0].cover, Some(ids[1]));
        lib.virtual_folder_members(&folder, &[ids[1]], false)
            .unwrap();
        assert_eq!(lib.virtual_folders().unwrap()[0].cover, None);
    }
}
