use super::*;
impl Library {
    /// Resolve an anchor against the same ordering and filters used by the gallery.
    /// Only seven metadata rows are materialized, including across gallery pages.
    pub fn neighborhood(
        &mut self,
        query: &Query,
        anchor: AssetId,
        direction: i8,
    ) -> Result<Option<Neighborhood>> {
        let scoped = self.workspace_query(query)?;
        let query = &scoped;
        if !(-1..=1).contains(&direction) {
            return Err(Error::Invalid("浏览方向无效".into()));
        }
        let total = self.page(query, 0, 0)?.total as usize;
        let unfiltered = *query == Query::default();
        let table = if unfiltered {
            "asset_order"
        } else {
            "query_snapshot"
        };
        let rank: Option<usize> = self
            .db
            .query_row(
                &format!("SELECT position FROM {table} WHERE asset_id=?1"),
                [anchor.to_string()],
                |r| r.get(0),
            )
            .optional()?;
        let Some(rank) = rank else { return Ok(None) };
        let position = if unfiltered { total - 1 - rank } else { rank };
        let position = position
            .saturating_add_signed(direction as isize)
            .min(total - 1);
        let offset = position.saturating_sub(3).min(total.saturating_sub(7));
        let page = self.page(query, offset, 7)?;
        let selected = page
            .assets
            .get(position - offset)
            .ok_or_else(|| Error::Invalid("素材浏览位置已失效".into()))?
            .id;
        Ok(Some(Neighborhood {
            selected,
            position,
            page,
        }))
    }
    pub fn selection_range(
        &mut self,
        query: &Query,
        first: AssetId,
        last: AssetId,
    ) -> Result<Vec<AssetId>> {
        let scoped = self.workspace_query(query)?;
        let query = &scoped;
        self.page(query, 0, 1)?;
        let table = if *query == Query::default() {
            "asset_order"
        } else {
            "query_snapshot"
        };
        let rank = |id: AssetId| {
            self.db
                .query_row(
                    &format!("SELECT position FROM {table} WHERE asset_id=?1"),
                    [id.to_string()],
                    |r| r.get::<_, i64>(0),
                )
                .map_err(Error::from)
        };
        let (a, b) = (rank(first)?, rank(last)?);
        if (a - b).unsigned_abs() >= 10000 {
            return Err(Error::Invalid(
                "一次范围选择最多 10000 张，请缩小范围".into(),
            ));
        }
        self.db
            .prepare(&format!(
                "SELECT asset_id FROM {table} WHERE position BETWEEN ?1 AND ?2 ORDER BY position"
            ))?
            .query_map(params![a.min(b), a.max(b)], |r| r.get::<_, String>(0))?
            .map(|r| Ok(AssetId(parse_id(&r?)?)))
            .collect()
    }
    pub fn create_child_project(
        &mut self,
        name: &str,
        parent: Option<ProjectId>,
    ) -> Result<ProjectId> {
        import::validate_component(name.trim())?;
        let profile = self.ai_profile_for("")?;
        let tx = self.db.transaction()?;
        if name.trim().is_empty() || name.len() > 120 {
            return Err(Error::Invalid("文件夹名称应为 1～120 字节".into()));
        }
        let id = ProjectId::default();
        tx.execute(
            "INSERT INTO projects(id,name,parent_id) VALUES(?1,?2,?3)",
            params![id.to_string(), name.trim(), parent.map(|v| v.to_string())],
        )?;
        workspace::insert_profile(&tx, id, &profile)?;
        tx.commit()?;
        self.snapshot = None;
        Ok(id)
    }
    pub fn move_project(&mut self, id: ProjectId, parent: Option<ProjectId>) -> Result<()> {
        if let Some(parent) = parent {
            let cycle:bool=self.db.query_row("WITH RECURSIVE descendants(id) AS(SELECT ?1 UNION ALL SELECT p.id FROM projects p JOIN descendants \
                d ON p.parent_id=d.id) SELECT EXISTS(SELECT 1 FROM descendants WHERE id=?2)",
params![id.to_string(),
parent.to_string()],
|r|r.get(0))?;

            if cycle {
                return Err(Error::Invalid("不能把文件夹移到自身或子文件夹下".into()));
            }
        }
        if self.db.execute(
            "UPDATE projects SET parent_id=?1 WHERE id=?2",
            params![parent.map(|v| v.to_string()), id.to_string()],
        )? != 1
        {
            return Err(Error::Invalid("文件夹已不存在".into()));
        }
        self.snapshot = None;
        Ok(())
    }
    /// Only this project's edges change; vocabulary storage may deduplicate strings.
    pub fn rename_tag(&mut self, source: &str, target: &str) -> Result<()> {
        let target = target.trim();
        if target.is_empty() || target.len() > 128 {
            return Err(Error::Invalid("标签应为 1～128 字节".into()));
        }
        if source == target {
            return Ok(());
        }
        let scope = self.workspace_key();
        let tx = self.db.transaction()?;
        let assets=tx.prepare("SELECT DISTINCT t.asset_id FROM effective_asset_tags t JOIN asset_workspaces w ON w.asset_id=t.asset_id WHERE t.name=?1 AND w.workspace=?2")?
            .query_map(params![source,scope],|r|r.get::<_,String>(0))?.collect::<std::result::Result<Vec<_>,_>>()?;
        if assets.is_empty() {
            return Err(Error::Invalid("本项目中没有此标签".into()));
        }
        tx.execute("INSERT OR IGNORE INTO tags(name) VALUES(?1)", [target])?;
        for asset in assets {
            tx.execute("INSERT OR IGNORE INTO ai_rejected_tags SELECT display_version_id,?2 FROM assets WHERE id=?1",params![asset,source])?;
            tx.execute("DELETE FROM asset_tags WHERE asset_id=?1 AND tag_id IN(SELECT id FROM tags WHERE name=?2)",params![asset,source])?;
            tx.execute(
                "INSERT OR IGNORE INTO asset_tags SELECT ?1,id FROM tags WHERE name=?2",
                params![asset, target],
            )?;
            tx.execute(
                "UPDATE assets SET edited_at=?2 WHERE id=?1",
                params![asset, now_ms()?],
            )?;
        }
        tx.execute("DELETE FROM tags WHERE name=?1 AND NOT EXISTS(SELECT 1 FROM asset_tags WHERE tag_id=tags.id)",[source])?;
        tx.commit()?;
        self.snapshot = None;
        Ok(())
    }
    pub fn delete_tag(&mut self, name: &str) -> Result<()> {
        let scope = self.workspace_key();
        let tx = self.db.transaction()?;
        let assets=tx.prepare("SELECT DISTINCT t.asset_id FROM effective_asset_tags t JOIN asset_workspaces w ON w.asset_id=t.asset_id WHERE t.name=?1 AND w.workspace=?2")?
            .query_map(params![name,scope],|r|r.get::<_,String>(0))?.collect::<std::result::Result<Vec<_>,_>>()?;
        for asset in assets {
            tx.execute("INSERT OR IGNORE INTO ai_rejected_tags SELECT display_version_id,?2 FROM assets WHERE id=?1",params![asset,name])?;
            tx.execute("DELETE FROM asset_tags WHERE asset_id=?1 AND tag_id IN(SELECT id FROM tags WHERE name=?2)",params![asset,name])?;
            tx.execute(
                "UPDATE assets SET edited_at=?2 WHERE id=?1",
                params![asset, now_ms()?],
            )?;
        }
        tx.execute("DELETE FROM tags WHERE name=?1 AND NOT EXISTS(SELECT 1 FROM asset_tags WHERE tag_id=tags.id)",[name])?;
        tx.commit()?;
        self.snapshot = None;
        Ok(())
    }
    pub fn reference_hash(&self, asset: AssetId, version: AssetVersionId) -> Result<String> {
        self.db.query_row("SELECT v.sha256 FROM asset_versions v JOIN file_locations f ON f.version_id=v.id WHERE v.id=?1 AND \
                v.asset_id=?2",
params![version.to_string(),
asset.to_string()],
|r|r.get(0)).map_err(Error::from)
    }
    pub fn relocate_reference(
        &mut self,
        asset: AssetId,
        version: AssetVersionId,
        path: &Path,
        expected: &str,
    ) -> Result<()> {
        if self.reference_hash(asset, version)? != expected {
            return Err(Error::Invalid("引用版本已经改变，请重新选择".into()));
        }
        let (platform, native) = organize::encode_path(path)?;
        self.db.execute(
            "UPDATE file_locations SET platform=?1,native_path=?2 WHERE version_id=?3",
            params![platform, native, version.to_string()],
        )?;
        self.snapshot = None;
        Ok(())
    }
}
pub fn validate_reference(path: &Path, expected: &str) -> Result<PathBuf> {
    let path = path.canonicalize()?;
    if fs::metadata(&path)?.len() > MAX_BYTES || organize::hash_file(&path)? != expected {
        return Err(Error::Invalid(
            "所选文件与登记原片内容不同，请选择原始文件的完整副本".into(),
        ));
    }
    Ok(path)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn neighborhood_crosses_pages_obeys_query_sort_and_survives_reopen() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("sample.png");
        let mut lib = Library::create(dir.path(), "浏览测试").unwrap();
        for n in 0..135u8 {
            image::RgbImage::from_pixel(2, 2, image::Rgb([n, 30, 50]))
                .save(&source)
                .unwrap();
            lib.import(&source, None).unwrap();
        }
        let root = lib.root().to_path_buf();
        let q = Query::default();
        let page = lib.page(&q, 0, 128).unwrap();
        let anchor = page.assets[127].id;
        let next = lib.neighborhood(&q, anchor, 1).unwrap().unwrap();
        assert_eq!(next.position, 128);
        assert_eq!(next.page.assets.len(), 7);
        assert_eq!(next.selected, lib.page(&q, 128, 1).unwrap().assets[0].id);
        assert_eq!(
            lib.neighborhood(&q, next.selected, -1)
                .unwrap()
                .unwrap()
                .selected,
            anchor
        );
        let first = lib.page(&q, 0, 1).unwrap().assets[0].id;
        assert_eq!(
            lib.neighborhood(&q, first, -1).unwrap().unwrap().position,
            0
        );
        let last = lib.page(&q, 134, 1).unwrap().assets[0].id;
        assert_eq!(
            lib.neighborhood(&q, last, 1).unwrap().unwrap().position,
            134
        );
        lib.set_tags(anchor, &["人像".into()]).unwrap();
        lib.set_tags(first, &["人像写真".into()]).unwrap();
        let tag_query = Query {
            tag: "人像".into(),
            ..Default::default()
        };
        let exact = lib.neighborhood(&tag_query, anchor, 1).unwrap().unwrap();
        assert_eq!(exact.page.total, 1);
        assert_eq!(exact.selected, anchor);
        assert!(lib.neighborhood(&tag_query, first, 0).unwrap().is_none());
        for sort in [
            SortOrder::Oldest,
            SortOrder::Name,
            SortOrder::Largest,
            SortOrder::Rating,
        ] {
            let q = Query {
                sort,
                ..Default::default()
            };
            let sorted = lib.page(&q, 0, 128).unwrap();
            let next = lib
                .neighborhood(&q, sorted.assets[127].id, 1)
                .unwrap()
                .unwrap();
            assert_eq!(next.selected, lib.page(&q, 128, 1).unwrap().assets[0].id);
        }
        lib.edit(&[anchor], &Edit::Trash(true)).unwrap();
        assert!(lib.neighborhood(&q, anchor, 0).unwrap().is_none());
        let trash = Query {
            trash: true,
            ..Default::default()
        };
        assert_eq!(
            lib.neighborhood(&trash, anchor, 0)
                .unwrap()
                .unwrap()
                .selected,
            anchor
        );
        drop(lib);
        let mut lib = Library::open(&root).unwrap();
        assert_eq!(
            lib.neighborhood(&trash, anchor, 0)
                .unwrap()
                .unwrap()
                .page
                .total,
            1
        );
        assert!(lib.neighborhood(&q, anchor, 2).is_err());
    }
    #[test]
    fn nested_projects_include_children_prevent_cycles_and_keep_dense_selection() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("原片.png");
        image::RgbImage::from_pixel(24, 16, image::Rgb([10, 80, 150]))
            .save(&source)
            .unwrap();
        let mut library = Library::create(dir.path(), "库").unwrap();
        let root = library.create_project("客户").unwrap();
        let child = library.create_child_project("春季", Some(root)).unwrap();
        let leaf = library.create_child_project("室内", Some(child)).unwrap();
        library.import(&source, Some(leaf)).unwrap();
        assert_eq!(
            library
                .info()
                .unwrap()
                .projects
                .iter()
                .find(|p| p.id == root)
                .unwrap()
                .count,
            1
        );
        let query = Query {
            project: Some(root),
            ..Default::default()
        };
        let asset = library.page(&query, 0, 5).unwrap().assets.remove(0);
        assert_eq!(
            library.selection_range(&query, asset.id, asset.id).unwrap(),
            vec![asset.id]
        );
        assert!(library.move_project(root, Some(leaf)).is_err());
        library.move_project(leaf, None).unwrap();
        assert_eq!(library.page(&query, 0, 5).unwrap().total, 0);
        library.move_project(leaf, Some(child)).unwrap();
        library.delete_project(child).unwrap();
        assert_eq!(
            library
                .info()
                .unwrap()
                .projects
                .iter()
                .find(|p| p.id == leaf)
                .unwrap()
                .parent,
            None
        );
        assert!(library.asset_path(asset.id).unwrap().is_file());
    }
    #[test]
    fn indexed_search_tracks_rename_merge_removal_notes_and_date_boundaries() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("中文摄影原片.png");
        image::RgbImage::from_pixel(24, 16, image::Rgb([10, 80, 150]))
            .save(&source)
            .unwrap();
        let mut library = Library::create(dir.path(), "库").unwrap();
        library.import(&source, None).unwrap();
        let asset = library
            .page(&Query::default(), 0, 1)
            .unwrap()
            .assets
            .remove(0);
        library
            .set_tags(asset.id, &["春季摄影".into(), "人物肖像".into()])
            .unwrap();
        library.rename_tag("春季摄影", "人物肖像").unwrap();
        assert_eq!(
            library
                .page(
                    &Query {
                        text: "季摄影".into(),
                        ..Default::default()
                    },
                    0,
                    4
                )
                .unwrap()
                .total,
            0
        );
        assert_eq!(
            library
                .page(
                    &Query {
                        text: "物肖像".into(),
                        ..Default::default()
                    },
                    0,
                    4
                )
                .unwrap()
                .total,
            1
        );
        library
            .edit(&[asset.id], &Edit::Notes("100%_原样匹配\"文字".into()))
            .unwrap();
        assert_eq!(
            library
                .page(
                    &Query {
                        text: "100%_".into(),
                        ..Default::default()
                    },
                    0,
                    4
                )
                .unwrap()
                .total,
            1
        );
        assert_eq!(
            library
                .page(
                    &Query {
                        text: "配\"文字".into(),
                        ..Default::default()
                    },
                    0,
                    4
                )
                .unwrap()
                .total,
            1
        );
        library.delete_tag("人物肖像").unwrap();
        assert_eq!(
            library
                .page(
                    &Query {
                        text: "物肖像".into(),
                        ..Default::default()
                    },
                    0,
                    4
                )
                .unwrap()
                .total,
            0
        );
        assert_eq!(
            library
                .page(
                    &Query {
                        since: asset.imported_at,
                        before: asset.imported_at + 1,
                        ..Default::default()
                    },
                    0,
                    4
                )
                .unwrap()
                .total,
            1
        );
        assert_eq!(
            library
                .page(
                    &Query {
                        before: asset.imported_at,
                        ..Default::default()
                    },
                    0,
                    4
                )
                .unwrap()
                .total,
            0
        );
        library
            .db
            .execute("UPDATE assets SET rowid=rowid+1000", [])
            .unwrap();
        library.rename_asset(asset.id, "中文摄影重命名").unwrap();
        let root = library.root().to_path_buf();
        drop(library);
        let mut library = Library::open(&root).unwrap();
        assert_eq!(
            library
                .page(
                    &Query {
                        text: "中文摄影".into(),
                        ..Default::default()
                    },
                    0,
                    4
                )
                .unwrap()
                .total,
            1
        );
    }
    #[test]
    fn converted_export_preserves_original_and_reference_can_be_relocated() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("原片.png");
        image::RgbaImage::from_pixel(60, 40, image::Rgba([200, 20, 60, 128]))
            .save(&source)
            .unwrap();
        let original = fs::read(&source).unwrap();
        let mut library = Library::create(dir.path(), "库").unwrap();
        library.import_referenced(&source, None).unwrap();
        let asset = library
            .page(&Query::default(), 0, 1)
            .unwrap()
            .assets
            .remove(0);
        let replacement = dir.path().join("移动后的原片.png");
        fs::rename(&source, &replacement).unwrap();
        assert!(library.asset_path(asset.id).is_err());
        assert!(library.details(asset.id).unwrap().referenced);
        let expected = library.reference_hash(asset.id, asset.version_id).unwrap();
        let path = validate_reference(&replacement, &expected).unwrap();
        library
            .relocate_reference(asset.id, asset.version_id, &path, &expected)
            .unwrap();
        let exported = export::run(
            library.export_plan(&[asset.id]).unwrap(),
            dir.path(),
            ExportOptions {
                format: ExportFormat::Jpeg,
                max_edge: 30,
                quality: 90,
                rotate: 90,
                ..Default::default()
            },
        )
        .unwrap();
        let image = image::open(exported.join("原片.jpg")).unwrap();
        assert_eq!(image.dimensions(), (20, 30));
        assert_eq!(fs::read(replacement).unwrap(), original);
        assert_eq!(library.page(&Query::default(), 0, 4).unwrap().total, 1);
    }
}
