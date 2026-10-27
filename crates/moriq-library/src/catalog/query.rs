use super::*;

impl Library {
    pub fn page(&mut self, query: &Query, offset: usize, limit: usize) -> Result<Page> {
        let scoped = self.workspace_query(query)?;
        let query = &scoped;
        let project = query.project.map(|id| id.to_string());
        // instr makes %, _ and quotes literal. All user input stays parameterized.
        const WHERE_HEAD: &str = "WHERE (?1='' OR instr(lower(a.name),lower(?1))>0 OR instr(lower(a.notes),lower(?1))>0 OR \
                EXISTS(SELECT 1 FROM ai_search_source ar WHERE ar.version_id=v.id AND instr(lower(ar.body),lower(?1))>0) OR \
                EXISTS(SELECT 1 FROM effective_asset_tags t WHERE t.asset_id=a.id AND instr(lower(t.name),lower(?1))>0))";

        const WHERE_TAIL: &str = "
          AND (?2=0 OR a.favorite=1) AND (?3 IS NULL OR EXISTS(SELECT 1 FROM project_assets pa WHERE pa.asset_id=a.id AND pa.project_id IN(WITH RECURSIVE descendants(id) AS(SELECT ?3 UNION ALL SELECT p.id FROM projects p JOIN descendants d ON p.parent_id=d.id) SELECT id FROM descendants)))
          AND (a.deleted_at IS NOT NULL)=?4 AND (?5='' OR v.format=?5) AND a.rating>=?6
          AND v.bytes>=?7 AND (?8=0 OR v.bytes<=?8) AND a.imported_at>=?9 AND v.width>=?10
          AND (?18=0 OR a.imported_at<?18)
          AND (?11=0 OR (?11=1 AND v.width>v.height) OR (?11=2 AND v.height>v.width) OR (?11=3 AND v.width=v.height))
          AND (?12=0 OR NOT EXISTS(SELECT 1 FROM effective_asset_tags WHERE asset_id=a.id))
          AND (?13=0 OR NOT EXISTS(SELECT 1 FROM project_assets WHERE asset_id=a.id))
          AND (?14='' OR EXISTS(SELECT 1 FROM effective_asset_tags t WHERE t.asset_id=a.id AND t.name=?14)) AND (?15='' OR v.dominant_color=?15) AND a.edited_at>=?16 AND (?17=0 OR (?17=1 AND NOT EXISTS(SELECT 1 FROM file_locations f WHERE f.version_id=v.id)) OR (?17=2 AND EXISTS(SELECT 1 FROM file_locations f WHERE f.version_id=v.id)))";

        let indexed_text = if query.text.chars().count() >= 3 {
            Some(format!("\"{}\"", query.text.replace('"', "\"\"")))
        } else {
            None
        };
        let search_text = indexed_text.as_deref().unwrap_or(&query.text);
        let where_head = if indexed_text.is_some() {
            "WHERE (a.id IN(SELECT asset_id FROM search_fts WHERE search_fts MATCH ?1) OR a.display_version_id IN(SELECT version_id FROM ai_text_fts WHERE ai_text_fts MATCH ?1))"
        } else {
            WHERE_HEAD
        };
        let media_condition = format!(
            "{where_head}{WHERE_TAIL} AND (?19=0 OR (?19=1 AND v.format IN({})) OR (?19=2 AND v.format IN({}))) \
                AND (?20='' OR EXISTS(SELECT 1 FROM asset_ai_categories ac WHERE ac.asset_id=a.id AND \
                ac.category=?20) OR (?20='未分类' AND NOT EXISTS(SELECT 1 FROM asset_ai_categories ac WHERE \
                ac.asset_id=a.id))) AND (?21='' OR EXISTS(SELECT 1 FROM detected_faces df WHERE df.version_id=v.id \
                AND df.person_id=?21)) AND (?23=0 OR a.id IN(SELECT value FROM json_each(?22))) AND (?24='' OR \
                EXISTS(SELECT 1 FROM virtual_folder_assets fa WHERE fa.folder_id=?24 AND fa.asset_id=a.id)) AND (?25='' OR EXISTS(SELECT 1 FROM ai_effective_content e WHERE e.version_id=v.id AND e.medium=?25)) AND NOT EXISTS(SELECT 1 FROM json_each(?26) q WHERE NOT EXISTS(SELECT 1 FROM ai_effective_content e,json_each(e.themes) t WHERE e.version_id=v.id AND t.value=q.value)) AND (?27='' OR EXISTS(SELECT 1 FROM asset_classification lc WHERE lc.asset_id=a.id AND lc.category=?27 AND (lc.manual=1 OR NOT EXISTS(SELECT 1 FROM ai_effective_content e WHERE e.version_id=v.id AND e.medium<>''))))",
            native::raw_sql(),
            video::sql_formats()
        );
        let media_condition = if query.project_exact {
            media_condition.replace("pa.project_id IN(WITH RECURSIVE descendants(id) AS(SELECT ?3 UNION ALL SELECT p.id FROM projects p JOIN descendants d ON p.parent_id=d.id) SELECT id FROM descendants)", "pa.project_id=?3")
        } else {
            media_condition
        };
        let mut condition = media_condition.as_str();
        let mut values: Vec<rusqlite::types::Value> = params![
            search_text,
            query.favorites_only,
            project,
            query.trash,
            query.format,
            query.min_rating,
            query.min_bytes,
            query.max_bytes,
            query.since,
            query.min_width,
            query.orientation,
            query.untagged,
            query.unassigned,
            query.tag,
            query.color,
            query.edited_since,
            query.storage,
            query.before,
            match query.media_kind {
                MediaKind::All => 0,
                MediaKind::Raw => 1,
                MediaKind::Video => 2,
            },
            query.category,
            query.person.as_deref().unwrap_or(""),
            serde_json::to_string(&query.semantic_ids)
                .map_err(|e| Error::Invalid(e.to_string()))?,
            query.semantic_active,
            query.folder.as_deref().unwrap_or(""),
            query.medium,
            serde_json::to_string(&query.themes).map_err(|e| Error::Invalid(e.to_string()))?,
            query.legacy_category
        ]
        .iter()
        .map(|p| {
            p.to_sql().and_then(|v| match v {
                rusqlite::types::ToSqlOutput::Borrowed(value) => value
                    .try_into()
                    .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error))),
                rusqlite::types::ToSqlOutput::Owned(value) => Ok(value),
                _ => Err(rusqlite::Error::InvalidParameterName(
                    "unsupported rule parameter".into(),
                )),
            })
        })
        .collect::<std::result::Result<_, _>>()?;
        let metadata_condition;
        if !query.metadata.is_empty() {
            metadata_condition = format!(
                "{condition} AND ({})",
                metadata::compile(&query.metadata, &mut values)?
            );
            condition = &metadata_condition;
        }
        let rule_condition;
        if let Some(rule) = &query.rule {
            rule_condition = format!("{condition} AND ({})", rules::compile(rule, &mut values)?);
            condition = &rule_condition;
        }
        let order_by = match query.sort {
            SortOrder::Newest => "a.imported_at DESC,a.id DESC",
            SortOrder::Oldest => "a.imported_at,a.id",
            SortOrder::Name => "a.name COLLATE NOCASE,a.id",
            SortOrder::Largest => "v.bytes DESC,a.id",
            SortOrder::Rating => "a.rating DESC,a.id",
        };
        let order_by = if query.semantic_active {
            "(SELECT key FROM json_each(?22) WHERE value=a.id),a.id"
        } else {
            order_by
        };
        let unfiltered = *query == Query::default();
        let total: u64 = if unfiltered {
            self.db
                .query_row("SELECT count(*) FROM asset_order", [], |r| r.get(0))?
        } else if let Some((cached, count)) = &self.snapshot
            && cached == query
        {
            *count
        } else {
            self.snapshot = None;
            self.db.execute_batch("CREATE TEMP TABLE IF NOT EXISTS query_snapshot(position INTEGER PRIMARY KEY,asset_id TEXT NOT \
                NULL); CREATE UNIQUE INDEX IF NOT EXISTS query_snapshot_asset ON query_snapshot(asset_id); DELETE \
                FROM query_snapshot;")?;

            self.db.execute(&format!("INSERT INTO query_snapshot SELECT row_number() OVER(ORDER BY {order_by})-1,a.id FROM assets a LEFT \
                JOIN asset_order o ON o.asset_id=a.id JOIN asset_versions v ON v.id=a.display_version_id {condition}"),
rusqlite::params_from_iter(values))?;

            let count = self
                .db
                .query_row("SELECT count(*) FROM query_snapshot", [], |r| r.get(0))?;
            self.snapshot = Some((query.clone(), count));
            count
        };
        let end = (offset as u64)
            .saturating_add(limit.min(128) as u64)
            .min(total);
        let (source, start, end, order) = if unfiltered {
            (
                "asset_order",
                total.saturating_sub(end),
                total.saturating_sub(offset as u64),
                "DESC",
            )
        } else {
            ("query_snapshot", offset as u64, end, "ASC")
        };
        let sql = format!(
            "SELECT a.id,v.id,a.name,v.format,v.width,v.height,v.bytes,a.favorite,a.rating,a.imported_at FROM \
                {source} o JOIN assets a ON a.id=o.asset_id JOIN asset_versions v ON v.id=a.display_version_id \
                WHERE o.position>=?1 AND o.position<?2 ORDER BY o.position {order}"
        );
        let assets = self.read_assets(&sql, params![start, end])?;
        Ok(Page {
            total,
            offset,
            assets,
        })
    }

    pub(super) fn read_assets(
        &self,
        sql: &str,
        parameters: impl rusqlite::Params,
    ) -> Result<Vec<Asset>> {
        let mut stmt = self.db.prepare_cached(sql)?;
        let rows = stmt.query_map(parameters, |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, u32>(4)?,
                r.get::<_, u32>(5)?,
                r.get::<_, u64>(6)?,
                r.get::<_, bool>(7)?,
                r.get::<_, u8>(8)?,
                r.get::<_, u64>(9)?,
            ))
        })?;
        let mut assets = Vec::new();
        for row in rows {
            let (id, version, name, format, width, height, bytes, favorite, rating, imported_at) =
                row?;
            assets.push(Asset {
                id: AssetId(parse_id(&id)?),
                version_id: AssetVersionId(parse_id(&version)?),
                name,
                format,
                width,
                height,
                bytes,
                favorite,
                rating,
                tags: Vec::new(),
                imported_at,
            });
        }
        if !assets.is_empty() {
            self.load_page_tags(&mut assets)?;
        }
        Ok(assets)
    }

    /// Read tags for the bounded page together, rather than expanding AI views per tile.
    fn load_page_tags(&self, assets: &mut [Asset]) -> Result<()> {
        let ids: Vec<_> = assets.iter().map(|asset| asset.id.to_string()).collect();
        let ids = serde_json::to_string(&ids).map_err(|e| Error::Invalid(e.to_string()))?;
        let mut statement = self.db.prepare_cached(
            "WITH selected AS MATERIALIZED (
                SELECT id,display_version_id FROM assets WHERE id IN (SELECT value FROM json_each(?1))
             )
             SELECT s.id,t.name FROM selected s JOIN asset_tags a ON a.asset_id=s.id
                JOIN tags t ON t.id=a.tag_id
             UNION SELECT s.id,t.name FROM selected s JOIN ai_visible_tags t
                ON t.version_id=s.display_version_id
             ORDER BY 1,2",
        )?;
        let rows = statement.query_map([ids], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut tags = std::collections::HashMap::<AssetId, Vec<String>>::new();
        for row in rows {
            let (id, tag) = row?;
            tags.entry(AssetId(parse_id(&id)?)).or_default().push(tag);
        }
        for asset in assets {
            asset.tags = tags.remove(&asset.id).unwrap_or_default();
        }
        Ok(())
    }

    pub fn layout_sizes(&mut self, query: &Query) -> Result<Vec<(u32, u32)>> {
        let query = self.workspace_query(query)?;
        self.page(&query, 0, 0)?;
        let (table, order) = if query == Query::default() {
            ("asset_order", "DESC")
        } else {
            ("query_snapshot", "ASC")
        };
        let mut statement=self.db.prepare(&format!("SELECT v.width,v.height FROM {table} o JOIN assets a ON a.id=o.asset_id JOIN asset_versions v ON \
                v.id=a.display_version_id ORDER BY o.position {order}"))?;

        statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Error::from)
    }
}
