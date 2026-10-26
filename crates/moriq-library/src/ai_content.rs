//! Effective version-scoped content and explicit human overrides.
use super::*;

pub(crate) fn save_run(
    db: &Connection,
    version: &str,
    revision: &str,
    data: &serde_json::Value,
) -> Result<()> {
    let task = data["task"].as_str().unwrap_or_default();
    if !matches!(task, "qwen_classification" | "qwen_description") {
        return Err(Error::Invalid("Qwen 分析任务身份无效".into()));
    }
    db.execute("INSERT INTO ai_analysis_runs(version_id,task,revision,preprocessing,data,created_at) VALUES(?1,?2,?3,?4,?5,?6)",
        params![version, task, revision, analysis::PREPROCESSING, data.to_string(), now_ms()?])?;
    db.execute("INSERT INTO ai_effective_results VALUES(?1,?2,?3) ON CONFLICT(version_id,task) DO UPDATE SET run_id=excluded.run_id",
        params![version, task, db.last_insert_rowid()])?;
    Ok(())
}

pub(crate) fn reject_tag(db: &Connection, version: &str, tag: &str) -> Result<()> {
    db.execute(
        "INSERT OR IGNORE INTO ai_rejected_tags VALUES(?1,?2)",
        params![version, tag.trim()],
    )?;
    Ok(())
}

impl Library {
    pub(crate) fn content_details(&self, version: &str, details: &mut AssetDetails) -> Result<()> {
        self.db.query_row("SELECT medium,themes,description,manual_medium,manual_themes,manual_description FROM ai_effective_content WHERE version_id=?1", [version], |r| {
            details.medium = r.get(0)?;
            let themes: String = r.get(1)?;
            details.themes = serde_json::from_str(&themes).unwrap_or_default();
            details.ai_caption = r.get(2)?;
            details.manual_medium = r.get(3)?;
            details.manual_themes = r.get(4)?;
            details.manual_description = r.get(5)?;
            Ok(())
        })?;
        details.ai_tags = self
            .db
            .prepare("SELECT name FROM ai_visible_tags WHERE version_id=?1 ORDER BY name")?
            .query_map([version], |r| r.get(0))?
            .collect::<std::result::Result<_, _>>()?;
        details.manual_tags = self.db.prepare("SELECT t.name FROM tags t JOIN asset_tags x ON x.tag_id=t.id JOIN asset_versions v ON v.asset_id=x.asset_id WHERE v.id=?1 ORDER BY t.name")?
            .query_map([version], |r| r.get(0))?.collect::<std::result::Result<_, _>>()?;
        Ok(())
    }

    pub fn edit_ai_content(
        &mut self,
        asset: AssetId,
        version: AssetVersionId,
        edit: AiEdit,
    ) -> Result<()> {
        self.require_workspace_assets(&[asset])?;
        let profile = self.ai_profile_for(&self.asset_workspace(asset)?)?;
        let version = version.to_string();
        let asset = asset.to_string();
        let tx = self.db.transaction()?;
        let current: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM assets WHERE id=?1 AND display_version_id=?2 AND deleted_at IS NULL)", params![asset, version], |r| r.get(0))?;
        if !current {
            return Err(Error::Invalid(
                "当前素材版本已变化，请重新选择后再保存".into(),
            ));
        }
        tx.execute(
            "INSERT OR IGNORE INTO ai_overrides(version_id) VALUES(?1)",
            [&version],
        )?;
        let promote = matches!(edit, AiEdit::PromoteTag(_));
        match edit {
            AiEdit::Classification(medium, mut themes) => {
                if !AI_MEDIA.iter().any(|(id, _)| *id == medium)
                    || themes.len() > 64
                    || themes.iter().any(|s| {
                        !profile
                            .categories
                            .iter()
                            .any(|c| c.id == *s && c.kind == AiCategoryKind::Theme && c.enabled)
                    })
                {
                    return Err(Error::Invalid("媒介或主题不在支持的词表中".into()));
                }
                themes.sort_unstable();
                themes.dedup();
                tx.execute(
                    "UPDATE ai_overrides SET medium=?2,themes=?3 WHERE version_id=?1",
                    params![version, medium, serde_json::json!(themes).to_string()],
                )?;
            }
            AiEdit::Medium(value) => {
                if value
                    .as_ref()
                    .is_some_and(|s| !AI_MEDIA.iter().any(|(id, _)| id == s))
                {
                    return Err(Error::Invalid("媒介不在支持的词表中".into()));
                }
                tx.execute(
                    "UPDATE ai_overrides SET medium=?2 WHERE version_id=?1",
                    params![version, value],
                )?;
            }
            AiEdit::Themes(value) => {
                if value.as_ref().is_some_and(|v| {
                    v.len() > 64
                        || v.iter().any(|s| {
                            !profile
                                .categories
                                .iter()
                                .any(|c| c.id == *s && c.kind == AiCategoryKind::Theme && c.enabled)
                        })
                }) {
                    return Err(Error::Invalid("主题无效或超出数量限制".into()));
                }
                let value = value.map(|mut v| {
                    v.sort_unstable();
                    v.dedup();
                    serde_json::json!(v).to_string()
                });
                tx.execute(
                    "UPDATE ai_overrides SET themes=?2 WHERE version_id=?1",
                    params![version, value],
                )?;
            }
            AiEdit::Description(value) => {
                if value.as_ref().is_some_and(|v| v.chars().count() > 2000) {
                    return Err(Error::Invalid("人工描述最多 2000 字".into()));
                }
                tx.execute(
                    "UPDATE ai_overrides SET description=?2 WHERE version_id=?1",
                    params![version, value],
                )?;
            }
            AiEdit::RejectTag(tag) | AiEdit::PromoteTag(tag) => {
                let tag = tag.trim();
                if tag.is_empty() || tag.len() > 128 {
                    return Err(Error::Invalid("标签内容或长度无效".into()));
                }
                reject_tag(&tx, &version, tag)?;
                if promote {
                    tx.execute("INSERT OR IGNORE INTO tags(name) VALUES(?1)", [tag])?;
                    tx.execute(
                        "INSERT OR IGNORE INTO asset_tags SELECT ?1,id FROM tags WHERE name=?2",
                        params![asset, tag],
                    )?;
                }
            }
            AiEdit::ResetRejectedTags => {
                tx.execute(
                    "DELETE FROM ai_rejected_tags WHERE version_id=?1",
                    [&version],
                )?;
            }
        }
        tx.execute(
            "UPDATE assets SET edited_at=?2 WHERE id=?1",
            params![asset, now_ms()?],
        )?;
        tx.commit()?;
        self.snapshot = None;
        Ok(())
    }
}
