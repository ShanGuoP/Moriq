//! Project ownership is frozen per writer request, never shared with async work.
use super::*;

impl Library {
    /// 按复制选项把素材加入目标项目，隔离其身份和项目所属配置。
    pub fn copy_to_project(
        &mut self,
        ids: &[AssetId],
        project: ProjectId,
        annotations: bool,
    ) -> Result<()> {
        self.require_workspace_assets(ids)?;
        if ids.is_empty() || ids.len() > 10000 {
            return Err(Error::Invalid("请选择 1～10000 个素材".into()));
        }
        let mut files = project_isolation::Copies::default();
        let tx = self.db.transaction()?;
        let target = project.to_string();
        for id in ids {
            let source = id.to_string();
            let exists: bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM project_assets WHERE project_id=?1 AND asset_id=?2) OR EXISTS(SELECT 1 FROM project_copies c JOIN project_assets p ON p.asset_id=c.asset_id AND p.project_id=c.project_id WHERE c.project_id=?1 AND c.source_asset_id=?2)",params![target,source],|r|r.get(0))?;
            if exists {
                continue;
            }
            let copy =
                project_isolation::clone_asset(&tx, &self.root, &source, &target, &mut files)?;
            if !annotations {
                tx.execute(
                    "UPDATE assets SET favorite=0,rating=0,notes='' WHERE id=?1",
                    [&copy],
                )?;
                for table in ["asset_tags", "asset_classification"] {
                    tx.execute(&format!("DELETE FROM {table} WHERE asset_id=?1"), [&copy])?;
                }
                for table in [
                    "ai_overrides",
                    "ai_rejected_tags",
                    "ai_analysis_runs",
                    "ai_results",
                    "ai_embeddings",
                    "detected_faces",
                ] {
                    tx.execute(&format!("DELETE FROM {table} WHERE version_id IN(SELECT id FROM asset_versions WHERE asset_id=?1)"),[&copy])?;
                }
                tx.execute("DELETE FROM people WHERE id IN(SELECT person_id FROM workspace_people WHERE workspace=?1) AND NOT EXISTS(SELECT 1 FROM detected_faces WHERE person_id=people.id)",[&target])?;
            }
        }
        tx.commit()?;
        files.commit();
        self.snapshot = None;
        Ok(())
    }
    /// 根据配置创建项目，并按选项建立独立的分类、分组等初始数据。
    pub fn create_project_configured(
        &mut self,
        name: &str,
        parent: Option<ProjectId>,
        template: ProjectTemplate,
    ) -> Result<ProjectId> {
        import::validate_component(name.trim())?;
        if name.trim().is_empty() || name.len() > 120 {
            return Err(Error::Invalid("项目名称应为 1～120 字节".into()));
        }
        let mut profile = match template {
            ProjectTemplate::Existing(project) => self.ai_profile_for(&project.to_string())?,
            _ => self.ai_profile_for("")?,
        };
        let themes: &[(&str, &str, &str)] = match template {
            ProjectTemplate::Portrait => &[
                (
                    "portrait_studio",
                    "棚拍写真",
                    "摄影棚内以人物为主体的写真照片",
                ),
                ("portrait_wedding", "婚礼", "婚礼仪式、婚纱照与婚礼活动"),
                ("portrait_street", "街拍人像", "街道或户外环境中的人物摄影"),
            ],
            ProjectTemplate::Product => &[
                (
                    "product_fashion",
                    "鞋包服饰",
                    "以鞋、包或服装商品为主体的产品照片",
                ),
                (
                    "product_digital",
                    "数码产品",
                    "以电子数码设备为主体的产品照片",
                ),
                (
                    "product_home",
                    "家居用品",
                    "以家具、家居器物为主体的产品照片",
                ),
            ],
            ProjectTemplate::Travel => &[
                (
                    "travel_city",
                    "城市旅行",
                    "旅行中的城市、街道、建筑与人文场景",
                ),
                ("travel_nature", "自然风光", "旅行中的山川、湖海及自然景观"),
                ("travel_food", "当地美食", "旅行途中当地食物与餐饮体验"),
            ],
            _ => &[],
        };
        if !themes.is_empty() {
            profile
                .categories
                .retain(|c| c.kind == AiCategoryKind::Medium);
            profile
                .categories
                .extend(themes.iter().map(|(id, label, definition)| AiCategory {
                    id: (*id).into(),
                    label: (*label).into(),
                    definition: (*definition).into(),
                    kind: AiCategoryKind::Theme,
                    enabled: true,
                }));
        }
        profile.validate().map_err(|e| Error::Invalid(e.into()))?;
        let id = ProjectId::default();
        let tx = self.db.transaction()?;
        tx.execute(
            "INSERT INTO projects(id,name,parent_id) VALUES(?1,?2,?3)",
            params![id.to_string(), name.trim(), parent.map(|id| id.to_string())],
        )?;
        insert_profile(&tx, id, &profile)?;
        tx.commit()?;
        Ok(id)
    }
    /// 设置后续资源库操作的项目范围；空值表示资源库聚合视图。
    pub fn set_workspace(&mut self, project: Option<ProjectId>) {
        self.workspace = project;
    }
    /// 返回当前选择的项目范围。
    pub fn workspace(&self) -> Option<ProjectId> {
        self.workspace
    }
    /// Freeze an explicit retrieval scope before a read plan leaves the writer.
    pub fn query_scope(&self) -> QueryScope {
        self.workspace
            .map_or(QueryScope::Library, QueryScope::Project)
    }
    /// 将当前项目范围转换为数据库查询使用的稳定键。
    pub(crate) fn workspace_key(&self) -> String {
        self.workspace.map(|id| id.to_string()).unwrap_or_default()
    }
    /// 为固定的素材表达式生成当前项目范围条件。
    pub(crate) fn workspace_filter(&self, asset: &str) -> String {
        match self.workspace {
            Some(project) => format!(
                "{asset} IN(SELECT asset_id FROM project_assets WHERE project_id='{project}')"
            ),
            None => "1=1".into(),
        }
    }
    /// 将当前项目范围合入查询，拒绝与会话范围冲突的项目请求。
    pub(crate) fn workspace_query(&self, query: &Query) -> Result<Query> {
        let mut query = query.clone();
        if let Some(project) = self.workspace {
            if query.project.is_some_and(|id| id != project) {
                return Err(Error::Invalid("查询所属项目已改变，请重新打开页面".into()));
            }
            query.project = Some(project);
            query.project_exact = true;
        }
        Ok(query)
    }
    /// 查询指定素材所属的项目范围。
    pub(crate) fn asset_workspace(&self, asset: AssetId) -> Result<String> {
        asset_scope(&self.db, &asset.to_string())
    }
    /// 核对固定内部表中的对象是否属于当前项目，拒绝跨项目操作。
    pub(crate) fn require_workspace_object(&self, table: &str, key: &str) -> Result<()> {
        let (column, source) = match table {
            "people" => ("person_id", "workspace_people"),
            "folders" => ("folder_id", "workspace_folders"),
            "jobs" => ("job_id", "workspace_jobs"),
            _ => return Err(Error::Invalid("未知项目对象".into())),
        };
        let scope: String = self.db.query_row(
            &format!("SELECT coalesce((SELECT workspace FROM {source} WHERE {column}=?1),'')"),
            [key],
            |r| r.get(0),
        )?;
        if scope != self.workspace_key() {
            return Err(Error::Invalid(
                "此内容属于其他项目，请先进入所属项目".into(),
            ));
        }
        Ok(())
    }
    /// 验证所选素材均处于允许的当前项目范围。
    pub(crate) fn require_workspace_assets(&self, ids: &[AssetId]) -> Result<()> {
        if let Some(project) = self.workspace {
            for id in ids {
                if self.asset_workspace(*id)? != project.to_string() {
                    return Err(Error::Invalid("选区包含其他项目的素材，请重新选择".into()));
                }
            }
        }
        Ok(())
    }
    /// 对需要精确归属的操作逐项校验素材的项目身份。
    pub(crate) fn require_exact_workspace_assets(&self, ids: &[AssetId]) -> Result<()> {
        let scope = self.workspace_key();
        for id in ids {
            if self.asset_workspace(*id)? != scope {
                return Err(Error::Invalid("请进入素材所属项目后管理分类或集合".into()));
            }
        }
        Ok(())
    }
}

/// 从素材关联中解析其项目范围键。
pub(crate) fn asset_scope(db: &Connection, asset: &str) -> Result<String> {
    Ok(db.query_row(
        "SELECT coalesce((SELECT min(project_id) FROM project_assets WHERE asset_id=?1),'')",
        [asset],
        |r| r.get(0),
    )?)
}
/// 通过版本所属素材解析项目范围键。
pub(crate) fn version_scope(db: &Connection, version: &str) -> Result<String> {
    let asset: String = db.query_row(
        "SELECT asset_id FROM asset_versions WHERE id=?1",
        [version],
        |r| r.get(0),
    )?;
    asset_scope(db, &asset)
}
/// 读取后台任务登记的项目范围。
pub(crate) fn job_scope(db: &Connection, job: &str) -> Result<String> {
    Ok(db.query_row(
        "SELECT coalesce((SELECT workspace FROM workspace_jobs WHERE job_id=?1),'')",
        [job],
        |r| r.get(0),
    )?)
}
/// 登记人物与项目的归属关系。
pub(crate) fn own_person(db: &Connection, person: &str, scope: &str) -> Result<()> {
    db.execute("INSERT INTO workspace_people VALUES(?1,?2) ON CONFLICT(person_id) DO UPDATE SET workspace=excluded.workspace", params![person,scope])?;
    Ok(())
}
/// 为项目保存独立的 AI 分类配置快照。
pub(crate) fn insert_profile(
    db: &Connection,
    project: ProjectId,
    profile: &AiProfile,
) -> Result<()> {
    db.execute(
        "INSERT INTO project_ai_profiles VALUES(?1,1,?2,?3)",
        params![
            project.to_string(),
            profile.max_themes,
            serde_json::to_string(&profile.categories)
                .map_err(|e| Error::Invalid(e.to_string()))?
        ],
    )?;
    Ok(())
}
/// 在目标项目复制人物身份，避免人物分组在项目之间共享可变状态。
fn copy_person(db: &Connection, person: &str, scope: &str) -> Result<String> {
    let id = Uuid::new_v4().to_string();
    db.execute(
        "INSERT INTO people SELECT ?1,name,revision,created_at FROM people WHERE id=?2",
        params![id, person],
    )?;
    own_person(db, &id, scope)?;
    Ok(id)
}

/// Copying a photo never imports another project's people or collection membership.
///
/// 调整素材及相关分组数据的项目归属。
pub(crate) fn rehome_asset(db: &Connection, asset: &str, scope: &str) -> Result<()> {
    let faces = db.prepare("SELECT DISTINCT f.person_id FROM detected_faces f JOIN asset_versions v ON v.id=f.version_id WHERE v.asset_id=?1 AND f.person_id IS NOT NULL")?
        .query_map([asset], |r| r.get::<_,String>(0))?.collect::<std::result::Result<Vec<_>,_>>()?;
    for person in faces {
        let current: String = db.query_row(
            "SELECT coalesce((SELECT workspace FROM workspace_people WHERE person_id=?1),'')",
            [&person],
            |r| r.get(0),
        )?;
        if current != scope {
            let new = copy_person(db, &person, scope)?;
            db.execute("UPDATE detected_faces SET person_id=?1 WHERE person_id=?2 AND version_id IN(SELECT id FROM asset_versions WHERE asset_id=?3)", params![new,person,asset])?;
        }
    }
    db.execute("DELETE FROM virtual_folder_assets WHERE asset_id=?1 AND coalesce((SELECT workspace FROM workspace_folders WHERE folder_id=virtual_folder_assets.folder_id),'')<>?2", params![asset,scope])?;
    db.execute("DELETE FROM ai_text_fts WHERE version_id IN(SELECT id FROM asset_versions WHERE asset_id=?1)",[asset])?;
    db.execute("INSERT INTO ai_text_fts SELECT s.version_id,s.body FROM ai_search_source s JOIN asset_versions v ON v.id=s.version_id WHERE v.asset_id=?1 AND trim(s.body)<>''",[asset])?;
    Ok(())
}

/// 在项目词表之间映射分类，保留能够按名称对应的分类含义。
pub(crate) fn map_classification(
    db: &Connection,
    source: &str,
    mapping: &std::collections::BTreeMap<String, String>,
    target: &str,
) -> Result<()> {
    let source_scope = asset_scope(db, source)?;
    let map = |id: &str| -> Result<Option<String>> {
        Ok(db.query_row("SELECT t.id FROM workspace_vocabulary s JOIN workspace_vocabulary t ON t.kind=s.kind AND t.label=s.label WHERE s.workspace=?1 AND s.id=?2 AND t.workspace=?3 AND t.enabled=1",params![source_scope,id,target],|r|r.get(0)).optional()?)
    };
    for (old, version) in mapping {
        let (medium, themes): (String, String) = db.query_row(
            "SELECT medium,themes FROM ai_effective_content WHERE version_id=?1",
            [old],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if medium.is_empty() && themes == "[]" {
            continue;
        }
        let medium = if medium.is_empty() {
            medium
        } else {
            map(&medium)?.unwrap_or_else(|| "uncertain".into())
        };
        let themes = serde_json::from_str::<Vec<String>>(&themes)
            .map_err(|e| Error::Invalid(e.to_string()))?
            .iter()
            .map(|id| map(id))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        db.execute("INSERT INTO ai_overrides(version_id,medium,themes) VALUES(?1,?2,?3) ON CONFLICT(version_id) DO UPDATE SET medium=excluded.medium,themes=excluded.themes",params![version,medium,serde_json::to_string(&themes).map_err(|e|Error::Invalid(e.to_string()))?])?;
    }
    Ok(())
}

/// 建立项目范围数据表并拆分共享人物、分组和配置，在事务中完成版本升级。
pub(super) fn migrate(db: &mut Connection) -> Result<()> {
    let tx = db.transaction()?;
    tx.execute_batch(include_str!("../database/migrations/migration_26.sql"))?;
    let projects = tx
        .prepare("SELECT id FROM projects")?
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for project in &projects {
        tx.execute("INSERT OR IGNORE INTO project_ai_profiles SELECT ?1,revision,max_themes,(SELECT json_group_array(json_object('id',id,'kind',kind,'label',label,'definition',definition,'enabled',json(CASE enabled WHEN 1 THEN 'true' ELSE 'false' END))) FROM (SELECT * FROM ai_vocabulary ORDER BY position,id)) FROM ai_profile_state WHERE id=1", [project])?;
    }
    // Keep root originals, and split project-specific mutable grouping identities.
    let assets = tx
        .prepare("SELECT asset_id,workspace FROM asset_workspaces WHERE workspace<>''")?
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut people = std::collections::BTreeMap::new();
    for (asset, scope) in &assets {
        let faces = tx.prepare("SELECT DISTINCT f.person_id FROM detected_faces f JOIN asset_versions v ON v.id=f.version_id WHERE v.asset_id=?1 AND f.person_id IS NOT NULL")?
            .query_map([asset], |r|r.get::<_,String>(0))?.collect::<std::result::Result<Vec<_>,_>>()?;
        for person in faces {
            let current: String = tx.query_row(
                "SELECT coalesce((SELECT workspace FROM workspace_people WHERE person_id=?1),'')",
                [&person],
                |r| r.get(0),
            )?;
            if current == *scope {
                continue;
            }
            let key = (person.clone(), scope.clone());
            let new = if let Some(id) = people.get(&key) {
                id
            } else {
                let id = copy_person(&tx, &person, scope)?;
                people.entry(key).or_insert(id)
            };
            tx.execute("UPDATE detected_faces SET person_id=?1 WHERE person_id=?2 AND version_id IN(SELECT id FROM asset_versions WHERE asset_id=?3)",params![new,person,asset])?;
        }
    }
    let folders = tx.prepare("SELECT id FROM virtual_folders WHERE NOT EXISTS(SELECT 1 FROM workspace_folders WHERE folder_id=virtual_folders.id)")?
        .query_map([], |r|r.get::<_,String>(0))?.collect::<std::result::Result<Vec<_>,_>>()?;
    let mut folder_mapping = std::collections::BTreeMap::new();
    for folder in folders {
        for project in &projects {
            let id = Uuid::new_v4().to_string();
            tx.execute("INSERT INTO virtual_folders SELECT ?1,name,kind,NULL,created_at FROM virtual_folders WHERE id=?2",params![id,folder])?;
            tx.execute(
                "INSERT INTO workspace_folders VALUES(?1,?2)",
                params![id, project],
            )?;
            tx.execute("INSERT INTO virtual_folder_assets SELECT ?1,f.asset_id FROM virtual_folder_assets f JOIN asset_workspaces w ON w.asset_id=f.asset_id WHERE f.folder_id=?2 AND w.workspace=?3",params![id,folder,project])?;
            tx.execute("UPDATE virtual_folders SET cover_id=(SELECT cover_id FROM virtual_folders WHERE id=?2) WHERE id=?1 AND EXISTS(SELECT 1 FROM virtual_folder_assets WHERE folder_id=?1 AND asset_id=(SELECT cover_id FROM virtual_folders WHERE id=?2))",params![id,folder])?;
            folder_mapping.insert((folder.clone(), project.clone()), id);
            tx.execute("DELETE FROM virtual_folder_assets WHERE folder_id=?1 AND asset_id IN(SELECT asset_id FROM asset_workspaces WHERE workspace=?2)",params![folder,project])?;
        }
    }
    let collections = tx
        .prepare("SELECT id,name,query FROM smart_collections")?
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for (_, name, value) in collections {
        let Ok(original) = toml::from_str::<Query>(&value) else {
            continue;
        };
        for project in &projects {
            if original.project.is_some_and(|p| p.to_string() != *project) {
                continue;
            }
            let mut query = original.clone();
            query.project = Some(ProjectId(parse_id(project)?));
            query.project_exact = true;
            if let Some(folder) = &query.folder {
                query.folder = folder_mapping
                    .get(&(folder.clone(), project.clone()))
                    .cloned()
                    .or_else(|| Some(folder.clone()));
            }
            if let Some(person) = &query.person
                && let Some(mapped) = people.get(&(person.clone(), project.clone()))
            {
                query.person = Some(mapped.clone());
            }
            tx.execute(
                "INSERT OR IGNORE INTO workspace_collections VALUES(?1,?2,?3,?4)",
                params![
                    Uuid::new_v4().to_string(),
                    name,
                    toml::to_string(&query).map_err(|e| Error::Invalid(e.to_string()))?,
                    project
                ],
            )?;
        }
    }
    // Pre-upgrade AI jobs froze a library-wide selection and cannot safely resume.
    tx.execute("UPDATE background_jobs SET state='cancelled',inflight=0,generation=generation+1,error='项目整理空间已升级，请在所属项目重新启动整理' WHERE kind LIKE 'analysis:%' AND state IN('queued','running','paused') AND NOT EXISTS(SELECT 1 FROM workspace_jobs WHERE job_id=background_jobs.id)",[])?;
    tx.execute_batch("DELETE FROM ai_text_fts; INSERT INTO ai_text_fts SELECT version_id,body FROM ai_search_source WHERE trim(body)<>''; PRAGMA user_version=26;")?;
    tx.commit()?;
    Ok(())
}
