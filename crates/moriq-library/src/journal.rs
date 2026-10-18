use super::*;
impl Library {
    pub fn update_profile(&mut self, name: &str, description: &str) -> Result<()> {
        let name = name.trim().trim_end_matches(".moriq").trim();
        if name.is_empty()
            || name.len() > 120
            || name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|'])
            || description.len() > 8192
        {
            return Err(Error::Invalid("库名无效或描述超过 8 KiB，未保存".into()));
        }
        let tx = self.db.transaction()?;
        for (key, value) in [("display_name", name), ("description", description)] {
            tx.execute("INSERT INTO library_preferences(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,value])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub(super) fn touch_metadata(&self) -> Result<()> {
        self.db.execute(
            "UPDATE library_state SET updated_at=max(updated_at,?1) WHERE id=1",
            [now_ms()?],
        )?;
        Ok(())
    }
    pub fn visit_project(&self, id: ProjectId) -> Result<()> {
        if self.db.execute(
            "UPDATE projects SET visited_at=?1 WHERE id=?2",
            params![now_ms()?, id.to_string()],
        )? != 1
        {
            return Err(Error::Invalid("项目已不存在".into()));
        }
        Ok(())
    }
    pub fn asset_trash_state(&self, id: AssetId) -> Result<Option<bool>> {
        Ok(self
            .db
            .query_row(
                "SELECT deleted_at IS NOT NULL FROM assets WHERE id=?1",
                [id.to_string()],
                |r| r.get(0),
            )
            .optional()?)
    }
    pub fn notice(&self, value: &Notice) -> Result<()> {
        if value.title.is_empty()
            || value.title.len() > 512
            || value.detail.len() > 65536
            || value.level > 2
            || !["", "recover", "maintenance", "synco"].contains(&value.retry.as_str())
        {
            return Err(Error::Invalid("通知内容无效或超出长度限制".into()));
        }
        self.db.execute("INSERT INTO notifications(at,title,detail,level,asset_id,retry) VALUES(?1,?2,?3,?4,?5,?6)",params![now_ms()?,value.title,value.detail,value.level,value.asset.map(|id|id.to_string()),value.retry])?;
        Ok(())
    }
    pub fn read_notifications(&self, through: i64) -> Result<()> {
        self.db.execute(
            "UPDATE notifications SET read_at=?1 WHERE id<=?2 AND read_at IS NULL",
            params![now_ms()?, through],
        )?;
        Ok(())
    }
    pub fn events(&self, notifications: bool, before: Option<i64>) -> Result<EventPage> {
        let sql = if notifications {
            "SELECT id,at,title,detail,asset_id,NULL,read_at IS NULL,retry FROM notifications WHERE id<?1 ORDER BY id DESC LIMIT 51"
        } else {
            "SELECT id,at,kind,detail,asset_id,project_id,0,'' FROM activity WHERE id<?1 ORDER BY id DESC LIMIT 51"
        };
        let mut items = Vec::new();
        let mut stmt = self.db.prepare(sql)?;
        let rows = stmt.query_map([before.unwrap_or(i64::MAX)], |r| {
            Ok((
                r.get(0)?,
                r.get(1)?,
                r.get(2)?,
                r.get(3)?,
                r.get::<_, Option<String>>(4)?,
                r.get::<_, Option<String>>(5)?,
                r.get(6)?,
                r.get(7)?,
            ))
        })?;
        for row in rows {
            let (id, at, kind, detail, asset, project, unread, retry) = row?;
            items.push(LibraryEvent {
                id,
                at,
                kind,
                detail,
                asset: asset.map(|s| parse_id(&s).map(AssetId)).transpose()?,
                project: project.map(|s| parse_id(&s).map(ProjectId)).transpose()?,
                unread,
                retry,
            });
        }
        let more = items.len() > 50;
        items.truncate(50);
        let unread = if notifications {
            self.db.query_row(
                "SELECT count(*) FROM notifications WHERE read_at IS NULL",
                [],
                |r| r.get(0),
            )?
        } else {
            0
        };
        Ok(EventPage {
            items,
            unread,
            more,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn version_nine_upgrade_keeps_identity_preferences_projects_and_existing_activity() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::create(dir.path(), "旧版本").unwrap();
        let mut manifest = lib.manifest.clone();
        let root = lib.root.clone();
        drop(lib);
        let old = tempfile::NamedTempFile::new_in(&root).unwrap();
        let db = Connection::open(old.path()).unwrap();
        db.execute_batch(include_str!("schema.sql")).unwrap();
        db.execute(
            "INSERT INTO library_identity VALUES(?1)",
            [manifest.library_id.to_string()],
        )
        .unwrap();
        for sql in [
            include_str!("migration_2.sql"),
            include_str!("migration_3.sql"),
            include_str!("migration_4.sql"),
            include_str!("migration_5.sql"),
            include_str!("migration_6.sql"),
            include_str!("migration_7.sql"),
            include_str!("migration_8.sql"),
            include_str!("migration_9.sql"),
        ] {
            db.execute_batch(sql).unwrap();
        }
        let project = ProjectId::default();
        db.execute(
            "INSERT INTO projects(id,name) VALUES(?1,'旧项目')",
            [project.to_string()],
        )
        .unwrap();
        db.execute(
            "INSERT INTO library_preferences VALUES('description','v9 描述')",
            [],
        )
        .unwrap();
        db.execute(
            "INSERT INTO activity(at,kind,detail) VALUES(17,'edit','旧活动')",
            [],
        )
        .unwrap();
        drop(db);
        old.persist(root.join("catalog.db")).unwrap();
        manifest.schema_version = 9;
        manifest.min_reader_version = 9;
        fs::write(
            root.join("library.toml"),
            toml::to_string(&manifest).unwrap(),
        )
        .unwrap();
        let lib = Library::open(&root).unwrap();
        assert_eq!(lib.id(), manifest.library_id);
        assert_eq!(lib.info().unwrap().projects[0].id, project);
        assert_eq!(lib.info().unwrap().projects[0].visited_at, 0);
        assert_eq!(lib.organizer().unwrap().description, "v9 描述");
        assert_eq!(lib.events(false, None).unwrap().items[0].detail, "旧活动");
        assert_eq!(lib.events(true, None).unwrap().unread, 0);
        assert_eq!(
            lib.db
                .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
                .unwrap(),
            SCHEMA
        );
    }
    #[test]
    fn profile_is_atomic_visits_do_not_modify_content_time_and_state_survives_backup() {
        let dir = tempfile::tempdir().unwrap();
        let mut lib = Library::create(dir.path(), "库").unwrap();
        lib.update_profile("显示名称", "真实描述").unwrap();
        assert!(lib.update_profile("另一个名称", &"x".repeat(8193)).is_err());
        assert_eq!(lib.info().unwrap().name, "显示名称");
        assert_eq!(lib.organizer().unwrap().description, "真实描述");
        let p = lib.create_project("项目").unwrap();
        let before = lib.info().unwrap().updated_at;
        lib.visit_project(p).unwrap();
        let info = lib.info().unwrap();
        assert_eq!(info.updated_at, before);
        assert!(info.projects[0].visited_at > 0);
        let backup_dir = dir.path().join("backups");
        fs::create_dir(&backup_dir).unwrap();
        let backup = lib.backup(&backup_dir).unwrap();
        let restored = Library::open(&backup).unwrap();
        assert_eq!(restored.info().unwrap().name, "显示名称");
        assert_eq!(restored.organizer().unwrap().description, "真实描述");
        assert_eq!(
            restored.info().unwrap().projects[0].visited_at,
            info.projects[0].visited_at
        );
        let root = lib.root.clone();
        drop(lib);
        let lib = Library::open(&root).unwrap();
        assert_eq!(lib.info().unwrap().updated_at, before);
        assert_eq!(lib.info().unwrap().name, "显示名称");
    }
    #[test]
    fn notifications_use_bounded_keyset_pages_and_read_cutoff_never_consumes_future_items() {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::create(dir.path(), "通知").unwrap();
        let value = Notice {
            title: "测试".into(),
            detail: "结果".into(),
            level: 2,
            asset: None,
            retry: "maintenance".into(),
        };
        for _ in 0..63 {
            lib.notice(&value).unwrap();
        }
        let head = lib.events(true, None).unwrap();
        assert_eq!(head.items.len(), 50);
        assert!(head.more);
        assert_eq!(head.unread, 63);
        let cutoff = head.items[0].id;
        lib.notice(&value).unwrap();
        lib.read_notifications(cutoff).unwrap();
        let tail = lib.events(true, Some(head.items[49].id)).unwrap();
        assert_eq!(tail.items.len(), 13);
        assert!(!tail.more);
        assert!(tail.items.iter().all(|e| !e.unread));
        assert_eq!(tail.unread, 1);
        let root = lib.root.clone();
        drop(lib);
        let lib = Library::open(&root).unwrap();
        assert_eq!(lib.events(true, None).unwrap().unread, 1);
        assert!(
            lib.notice(&Notice {
                retry: "arbitrary-command".into(),
                ..value
            })
            .is_err()
        );
    }
    #[test]
    fn activity_is_transactional_and_links_survive_deletion_as_history() {
        let dir = tempfile::tempdir().unwrap();
        let mut lib = Library::create(dir.path(), "活动").unwrap();
        let source = dir.path().join("图片.png");
        image::RgbImage::from_pixel(3, 2, image::Rgb([1, 2, 3]))
            .save(&source)
            .unwrap();
        lib.import(&source, None).unwrap();
        let asset = lib.page(&Query::default(), 0, 1).unwrap().assets[0].id;
        let before = lib.events(false, None).unwrap().items.len();
        assert!(
            lib.edit(&[asset, AssetId::default()], &Edit::Favorite(true))
                .is_err()
        );
        assert_eq!(lib.events(false, None).unwrap().items.len(), before);
        lib.edit(&[asset], &Edit::Favorite(true)).unwrap();
        assert!(
            lib.events(false, None)
                .unwrap()
                .items
                .iter()
                .any(|e| e.kind == "favorite" && e.asset == Some(asset))
        );
        lib.edit(&[asset], &Edit::Trash(true)).unwrap();
        lib.purge(&[asset]).unwrap();
        assert!(
            lib.events(false, None)
                .unwrap()
                .items
                .iter()
                .any(|e| e.kind == "purge" && e.asset == Some(asset))
        );
        assert_eq!(lib.asset_trash_state(asset).unwrap(), None);
    }
}
