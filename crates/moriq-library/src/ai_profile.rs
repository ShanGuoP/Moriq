//! One per-library vocabulary drives inference, cards, filters, and sidebar groups.
use super::*;

pub fn classification_hash(profile: &AiProfile) -> Result<String> {
    let mut definitions = profile
        .categories
        .iter()
        .filter(|c| c.enabled)
        .map(|c| (&c.id, c.kind, &c.definition))
        .collect::<Vec<_>>();
    definitions.sort_by_key(|(id, _, _)| *id);
    let bytes = serde_json::to_vec(&(profile.max_themes, definitions))
        .map_err(|e| Error::Invalid(e.to_string()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
impl Library {
    pub fn ai_profile(&self) -> Result<AiProfile> {
        let (revision, max_themes) = self.db.query_row(
            "SELECT revision,max_themes FROM ai_profile_state WHERE id=1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let categories = self
            .db
            .prepare(
                "SELECT id,kind,label,definition,enabled FROM ai_vocabulary ORDER BY position,id",
            )?
            .query_map([], |r| {
                Ok(AiCategory {
                    id: r.get(0)?,
                    kind: if r.get::<_, String>(1)? == "medium" {
                        AiCategoryKind::Medium
                    } else {
                        AiCategoryKind::Theme
                    },
                    label: r.get(2)?,
                    definition: r.get(3)?,
                    enabled: r.get(4)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(AiProfile {
            revision,
            max_themes,
            categories,
        })
    }
    pub fn save_ai_profile(&mut self, mut profile: AiProfile) -> Result<(AiProfile, Vec<String>)> {
        for c in &mut profile.categories {
            c.label = c.label.trim().into();
            c.definition = c.definition.trim().into();
        }
        profile.validate().map_err(|e| Error::Invalid(e.into()))?;
        let old = self.ai_profile()?;
        if old.revision != profile.revision {
            return Err(Error::Invalid(
                "分类配置已更新，请重新打开编辑器后再保存".into(),
            ));
        }
        if old.categories.iter().any(|old| {
            !profile
                .categories
                .iter()
                .any(|c| c.id == old.id && c.kind == old.kind)
        }) {
            return Err(Error::Invalid(
                "已有分类 ID 和类型不能更换；不参与识别的分类请禁用".into(),
            ));
        }
        let changed = classification_hash(&old)? != classification_hash(&profile)?;
        let reindex = old.categories.iter().any(|c| {
            profile
                .categories
                .iter()
                .find(|n| n.id == c.id)
                .is_some_and(|n| n.label != c.label || n.enabled != c.enabled)
        });
        let tx = self.db.transaction()?;
        tx.execute(
            "UPDATE ai_vocabulary SET label='__moriq_profile_transaction_renaming__'||id",
            [],
        )?;
        for (position, c) in profile.categories.iter().enumerate() {
            tx.execute("INSERT INTO ai_vocabulary(id,kind,label,definition,position,enabled) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(id) DO UPDATE SET label=excluded.label,definition=excluded.definition,position=excluded.position,enabled=excluded.enabled",
                params![c.id,if c.kind==AiCategoryKind::Medium {"medium"} else {"theme"},c.label,c.definition,position as u64,c.enabled])?;
        }
        profile.revision = profile.revision.saturating_add(1);
        tx.execute(
            "UPDATE ai_profile_state SET revision=?1,max_themes=?2 WHERE id=1",
            params![profile.revision, profile.max_themes],
        )?;
        let mut cancelled = Vec::new();
        if changed {
            cancelled=tx.prepare("SELECT id FROM background_jobs WHERE kind='analysis:qwen35-4b/classify' AND state IN('queued','running','paused')")?
                .query_map([],|r| r.get(0))?.collect::<std::result::Result<_,_>>()?;
            tx.execute("UPDATE background_jobs SET state='cancelled',inflight=0,generation=generation+1,error='分类定义已更新，请重新整理以使用新配置' WHERE kind='analysis:qwen35-4b/classify' AND state IN('queued','running','paused')",[])?;
        }
        if reindex {
            // Replace display labels, not the immutable model receipts or human tags.
            tx.execute("DELETE FROM ai_text_fts", [])?;
            tx.execute("INSERT INTO ai_text_fts SELECT version_id,body FROM ai_search_source WHERE trim(body)<>''",[])?;
        }
        tx.commit()?;
        self.snapshot = None;
        Ok((profile, cancelled))
    }
    pub fn ai_groups(&self) -> Result<Vec<AiGroup>> {
        let profile = self.ai_profile()?;
        let mut groups = profile
            .categories
            .into_iter()
            .filter(|c| c.enabled)
            .map(|c| {
                let query = if c.kind == AiCategoryKind::Medium {
                    Query {
                        medium: c.id.clone(),
                        ..Query::default()
                    }
                } else {
                    Query {
                        themes: vec![c.id.clone()],
                        ..Query::default()
                    }
                };
                AiGroup {
                    id: c.id,
                    label: c.label,
                    query,
                    count: 0,
                    cover: None,
                }
            })
            .collect::<Vec<_>>();
        let legacy=self.db.prepare("SELECT DISTINCT c.category FROM asset_classification c JOIN assets a ON a.id=c.asset_id WHERE a.deleted_at IS NULL AND (c.manual=1 OR NOT EXISTS(SELECT 1 FROM ai_effective_content e WHERE e.version_id=a.display_version_id AND e.medium<>'')) ORDER BY c.category")?
            .query_map([],|r| r.get::<_,String>(0))?.collect::<std::result::Result<Vec<_>,_>>()?;
        for label in legacy {
            groups.push(AiGroup {
                id: format!("legacy:{label}"),
                label: format!("历史 · {label}"),
                query: Query {
                    legacy_category: label,
                    ..Query::default()
                },
                count: 0,
                cover: None,
            });
        }
        groups.push(AiGroup {
            id: "_unclassified".into(),
            label: "未分类".into(),
            query: Query {
                category: "未分类".into(),
                ..Query::default()
            },
            count: 0,
            cover: None,
        });
        for g in &mut groups {
            let (condition, key) = if !g.query.medium.is_empty() {
                (
                    "EXISTS(SELECT 1 FROM ai_effective_content e WHERE e.version_id=a.display_version_id AND e.medium=?1)",
                    &g.query.medium,
                )
            } else if let Some(theme) = g.query.themes.first() {
                (
                    "EXISTS(SELECT 1 FROM ai_effective_content e,json_each(e.themes) j WHERE e.version_id=a.display_version_id AND j.value=?1)",
                    theme,
                )
            } else if !g.query.legacy_category.is_empty() {
                (
                    "EXISTS(SELECT 1 FROM asset_classification c WHERE c.asset_id=a.id AND c.category=?1 AND (c.manual=1 OR NOT EXISTS(SELECT 1 FROM ai_effective_content e WHERE e.version_id=a.display_version_id AND e.medium<>'')))",
                    &g.query.legacy_category,
                )
            } else {
                (
                    "(?1='未分类') AND (NOT EXISTS(SELECT 1 FROM asset_ai_categories c WHERE c.asset_id=a.id) OR EXISTS(SELECT 1 FROM asset_classification c WHERE c.asset_id=a.id AND c.category='未分类'))",
                    &g.query.category,
                )
            };
            let row=self.db.query_row(&format!("SELECT count(*) OVER(),a.display_version_id FROM assets a WHERE a.deleted_at IS NULL AND {condition} ORDER BY a.imported_at DESC,a.id DESC LIMIT 1"),[key],|r| Ok((r.get::<_,u64>(0)?,r.get::<_,String>(1)?))).optional()?;
            if let Some((count, version)) = row {
                g.count = count;
                g.cover = Some(AssetVersionId(parse_id(&version)?));
            }
        }
        Ok(groups)
    }
    pub fn outdated_ai_count(&self) -> Result<u64> {
        let hash = classification_hash(&self.ai_profile()?)?;
        Ok(self.db.query_row("SELECT count(*) FROM assets a JOIN ai_effective_results e ON e.version_id=a.display_version_id AND e.task='qwen_classification' JOIN ai_analysis_runs r ON r.id=e.run_id WHERE a.deleted_at IS NULL AND coalesce(json_extract(r.data,'$.profile_hash'),'')<>?1",[hash],|r| r.get(0))?)
    }
}
