//! One per-library vocabulary drives inference, cards, filters, and sidebar groups.
use super::*;

/// 计算影响分类推理的配置摘要，供任务回执判断配置是否过期。
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
    /// 读取当前项目实际使用的 AI 分类配置。
    pub fn ai_profile(&self) -> Result<AiProfile> {
        self.ai_profile_for(&self.workspace_key())
    }
    /// 按指定项目范围读取分类词表、启用状态与主题数量限制。
    pub(crate) fn ai_profile_for(&self, scope: &str) -> Result<AiProfile> {
        if !scope.is_empty() {
            let (revision,max_themes,json):(u64,u8,String)=self.db.query_row("SELECT revision,max_themes,categories FROM project_ai_profiles WHERE project_id=?1",[scope],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
            let categories =
                serde_json::from_str(&json).map_err(|e| Error::Invalid(e.to_string()))?;
            return Ok(AiProfile {
                revision,
                max_themes,
                categories,
            });
        }
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
    /// 校验并保存分类配置，按语义变化处理旧分类任务与失效结果。
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
        if let Some(project) = self.workspace {
            profile.revision = profile.revision.saturating_add(1);
            let tx = self.db.transaction()?;
            let categories = serde_json::to_string(&profile.categories)
                .map_err(|e| Error::Invalid(e.to_string()))?;
            tx.execute("UPDATE project_ai_profiles SET revision=?2,max_themes=?3,categories=?4 WHERE project_id=?1", params![project.to_string(),profile.revision,profile.max_themes,categories])?;
            let mut cancelled = Vec::new();
            if changed {
                cancelled = tx.prepare("UPDATE background_jobs SET state='cancelled',inflight=0,generation=generation+1,error='本项目分类定义已更新，请重新整理' WHERE kind='analysis:qwen35-4b/classify' AND state IN('queued','running','paused') AND id IN(SELECT job_id FROM workspace_jobs WHERE workspace=?1) RETURNING id")?
                    .query_map([project.to_string()],|r|r.get(0))?.collect::<std::result::Result<Vec<_>,_>>()?;
            }
            if reindex {
                tx.execute("DELETE FROM ai_text_fts WHERE version_id IN(SELECT v.id FROM asset_versions v JOIN asset_workspaces w ON w.asset_id=v.asset_id WHERE w.workspace=?1)",[project.to_string()])?;
                tx.execute("INSERT INTO ai_text_fts SELECT s.version_id,s.body FROM ai_search_source s JOIN asset_versions v ON v.id=s.version_id JOIN asset_workspaces w ON w.asset_id=v.asset_id WHERE w.workspace=?1 AND trim(s.body)<>''",[project.to_string()])?;
            }
            tx.commit()?;
            self.snapshot = None;
            return Ok((profile, cancelled));
        }
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
            cancelled=tx.prepare("SELECT id FROM background_jobs WHERE kind='analysis:qwen35-4b/classify' AND state IN('queued','running','paused') AND NOT EXISTS(SELECT 1 FROM workspace_jobs w WHERE w.job_id=background_jobs.id AND w.workspace<>'')")?
                .query_map([],|r| r.get(0))?.collect::<std::result::Result<_,_>>()?;
            tx.execute("UPDATE background_jobs SET state='cancelled',inflight=0,generation=generation+1,error='分类定义已更新，请重新整理以使用新配置' WHERE kind='analysis:qwen35-4b/classify' AND state IN('queued','running','paused') AND NOT EXISTS(SELECT 1 FROM workspace_jobs w WHERE w.job_id=background_jobs.id AND w.workspace<>'')",[])?;
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
    /// 汇总当前项目可见的 AI 分类分组和素材数量。
    pub fn ai_groups(&self) -> Result<Vec<AiGroup>> {
        let profile = self.ai_profile()?;
        // Materialize effective content once. The old implementation expanded the
        // full content views independently for every sidebar category and cover.
        let rows = self.db.prepare("WITH live AS MATERIALIZED (SELECT a.id,a.display_version_id,a.imported_at,e.medium,e.themes FROM assets a JOIN ai_effective_content e ON e.version_id=a.display_version_id WHERE a.deleted_at IS NULL AND e.workspace=?1), entries AS (SELECT 'medium:'||medium key,id,display_version_id,imported_at FROM live WHERE medium<>'' UNION ALL SELECT 'theme:'||j.value,l.id,l.display_version_id,l.imported_at FROM live l,json_each(l.themes) j UNION ALL SELECT 'legacy:'||c.category,l.id,l.display_version_id,l.imported_at FROM live l JOIN asset_classification c ON c.asset_id=l.id WHERE c.manual=1 OR l.medium='' UNION ALL SELECT '_unclassified',l.id,l.display_version_id,l.imported_at FROM live l LEFT JOIN asset_classification c ON c.asset_id=l.id WHERE (l.medium='' AND c.category IS NULL) OR c.category='未分类'), ranked AS (SELECT key,display_version_id,count(*) OVER(PARTITION BY key) n,row_number() OVER(PARTITION BY key ORDER BY imported_at DESC,id DESC) rank FROM entries) SELECT key,n,display_version_id FROM ranked WHERE rank=1")?
            .query_map([self.workspace_key()],|r|Ok((r.get::<_,String>(0)?,r.get::<_,u64>(1)?,r.get::<_,String>(2)?)))?
            .collect::<std::result::Result<Vec<_>,_>>()?;
        let mut counts = std::collections::BTreeMap::new();
        for (key, n, version) in rows {
            counts.insert(key, (n, AssetVersionId(parse_id(&version)?)));
        }
        let mut groups = profile
            .categories
            .into_iter()
            .filter(|c| c.enabled)
            .map(|c| {
                let query = if c.kind == AiCategoryKind::Medium {
                    Query {
                        medium: c.id.clone(),
                        ..Default::default()
                    }
                } else {
                    Query {
                        themes: vec![c.id.clone()],
                        ..Default::default()
                    }
                };
                let key = format!(
                    "{}:{}",
                    if c.kind == AiCategoryKind::Medium {
                        "medium"
                    } else {
                        "theme"
                    },
                    c.id
                );
                (
                    key,
                    AiGroup {
                        id: c.id,
                        label: c.label,
                        query,
                        count: 0,
                        cover: None,
                    },
                )
            })
            .collect::<Vec<_>>();
        for key in counts.keys().filter(|key| key.starts_with("legacy:")) {
            let label = key.trim_start_matches("legacy:").to_string();
            groups.push((
                key.clone(),
                AiGroup {
                    id: key.clone(),
                    label: format!("历史 · {label}"),
                    query: Query {
                        legacy_category: label,
                        ..Default::default()
                    },
                    count: 0,
                    cover: None,
                },
            ));
        }
        groups.push((
            "_unclassified".into(),
            AiGroup {
                id: "_unclassified".into(),
                label: "未分类".into(),
                query: Query {
                    category: "未分类".into(),
                    ..Default::default()
                },
                count: 0,
                cover: None,
            },
        ));
        Ok(groups
            .into_iter()
            .map(|(key, mut g)| {
                if let Some((n, v)) = counts.get(&key) {
                    g.count = *n;
                    g.cover = Some(*v);
                }
                g.query.project = self.workspace;
                g.query.project_exact = self.workspace.is_some();
                g
            })
            .collect())
    }
    /// 统计需要按当前分类配置重新分析的素材数量。
    pub fn outdated_ai_count(&self) -> Result<u64> {
        let hash = classification_hash(&self.ai_profile()?)?;
        Ok(self.db.query_row("SELECT count(*) FROM assets a JOIN ai_effective_results e ON e.version_id=a.display_version_id AND e.task='qwen_classification' JOIN ai_analysis_runs r ON r.id=e.run_id WHERE a.deleted_at IS NULL AND coalesce(json_extract(r.data,'$.profile_hash'),'')<>?1 AND (SELECT workspace FROM asset_workspaces WHERE asset_id=a.id)=?2",params![hash,self.workspace_key()],|r| r.get(0))?)
    }
}
