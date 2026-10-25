use super::*;
fn validate(patch: &BatchPatch) -> Result<()> {
    if patch.rating.is_some_and(|n| n > 5)
        || patch.notes.as_ref().is_some_and(|s| s.len() > 65536)
        || patch
            .category
            .as_ref()
            .is_some_and(|s| s.trim().is_empty() || s.len() > 128)
        || patch.add_tags.len() + patch.remove_tags.len() > 64
        || patch
            .add_tags
            .iter()
            .chain(&patch.remove_tags)
            .any(|s| s.len() > 128)
    {
        return Err(Error::Invalid("批量字段内容或数量无效".into()));
    }
    if patch.favorite.is_none()
        && patch.rating.is_none()
        && patch.notes.is_none()
        && patch.category.is_none()
        && patch.add_tags.is_empty()
        && patch.remove_tags.is_empty()
        && patch.project.is_none()
    {
        return Err(Error::Invalid("请至少修改一个字段".into()));
    }
    Ok(())
}
impl Library {
    pub fn enqueue_batch(&mut self, scope: BatchScope, patch: BatchPatch) -> Result<String> {
        validate(&patch)?;
        let table = match &scope {
            BatchScope::Query(query) => {
                self.page(query, 0, 0)?;
                if **query == Query::default() {
                    "asset_order"
                } else {
                    "query_snapshot"
                }
            }
            BatchScope::Assets(ids) => {
                if ids.is_empty() || ids.len() > 10000 {
                    return Err(Error::Invalid("请选择素材或使用完整查询范围".into()));
                }
                self.db.execute_batch("CREATE TEMP TABLE IF NOT EXISTS batch_selection(asset_id TEXT PRIMARY KEY); DELETE FROM batch_selection;")?;
                let tx = self.db.transaction()?;
                for id in ids {
                    tx.execute(
                        "INSERT OR IGNORE INTO batch_selection VALUES(?1)",
                        [id.to_string()],
                    )?;
                }
                tx.commit()?;
                "batch_selection"
            }
        };
        let id = Uuid::new_v4().to_string();
        let tx = self.db.transaction()?;
        if let Some(project) = patch.project {
            let exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM projects WHERE id=?1)",
                [project.to_string()],
                |r| r.get(0),
            )?;
            if !exists {
                return Err(Error::Invalid("目标项目已不存在".into()));
            }
        }
        tx.execute("INSERT INTO background_jobs(id,kind,state,priority,created_at) VALUES(?1,'batch','queued',20,?2)",params![id,now_ms()?])?;
        tx.execute(
            "INSERT INTO background_job_payload VALUES(?1,?2)",
            params![
                id,
                serde_json::to_string(&patch).map_err(|e| Error::Invalid(e.to_string()))?
            ],
        )?;
        tx.execute(&format!("INSERT INTO background_job_items SELECT ?1,row_number() OVER(ORDER BY a.id),a.display_version_id \
                FROM {table} s JOIN assets a ON a.id=s.asset_id"),
[&id])?;

        tx.execute("UPDATE background_jobs SET total=(SELECT count(*) FROM background_job_items WHERE job_id=?1) WHERE id=?1",[&id])?;
        tx.commit()?;
        Ok(id)
    }
}
pub(crate) fn apply(tx: &Connection, version: &str, value: &serde_json::Value) -> Result<()> {
    let patch: BatchPatch =
        serde_json::from_value(value.clone()).map_err(|e| Error::Invalid(e.to_string()))?;
    validate(&patch)?;
    let asset: Option<String> = tx
        .query_row(
            "SELECT asset_id FROM asset_versions WHERE id=?1",
            [version],
            |r| r.get(0),
        )
        .optional()?;
    let Some(asset) = asset else { return Ok(()) };
    tx.execute("UPDATE assets SET \
                favorite=coalesce(?2,favorite),rating=coalesce(?3,rating),notes=coalesce(?4,notes),edited_at=?5 \
                WHERE id=?1",
params![asset,
patch.favorite,
patch.rating,
patch.notes,
now_ms()?])?;

    if let Some(category) = patch.category {
        tx.execute("INSERT INTO asset_classification VALUES(?1,?2,1) ON CONFLICT(asset_id) DO UPDATE SET category=excluded.category,manual=1",params![asset,category.trim()])?;
    }
    for tag in patch.remove_tags {
        tx.execute("DELETE FROM asset_tags WHERE asset_id=?1 AND tag_id IN(SELECT id FROM tags WHERE name=?2)",params![asset,tag.trim()])?;
        tx.execute("INSERT OR IGNORE INTO ai_rejected_tags SELECT display_version_id,?2 FROM assets WHERE id=?1", params![asset, tag.trim()])?;
    }
    for tag in patch.add_tags {
        let tag = tag.trim();
        if tag.is_empty() {
            continue;
        }
        tx.execute("INSERT OR IGNORE INTO tags(name) VALUES(?1)", [tag])?;
        tx.execute(
            "INSERT OR IGNORE INTO asset_tags SELECT ?1,id FROM tags WHERE name=?2",
            params![asset, tag],
        )?;
    }
    if let Some(project) = patch.project {
        tx.execute(
            "INSERT OR IGNORE INTO project_assets VALUES(?1,?2)",
            params![project.to_string(), asset],
        )?;
    }
    Ok(())
}
