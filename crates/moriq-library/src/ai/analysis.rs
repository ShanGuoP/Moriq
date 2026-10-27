//! Version-bound AI results; human labels and face assignments survive reanalysis.
use super::*;
pub const PREPROCESSING: &str = "srgb-1024-v1";
pub struct SemanticPlan {
    db: read_pool::ReadLease,
    _lease: std::sync::Arc<File>,
    revision: String,
}
impl SemanticPlan {
    /// 根据指定展示版本的语义向量返回少量相近素材，排除素材自身。
    pub fn recommendations(
        &self,
        asset: AssetId,
        version: AssetVersionId,
    ) -> Result<Vec<(AssetId, String)>> {
        let bytes:Option<Vec<u8>>=self.db.connection()?.query_row("SELECT e.vector FROM ai_embeddings e JOIN assets a ON a.display_version_id=e.version_id WHERE \
                a.id=?1 AND e.version_id=?2 AND e.model_id='chinese-clip' AND e.revision=?3",
params![asset.to_string(),
version.to_string(),
self.revision],
|r|r.get(0)).optional()?;

        let Some(bytes) = bytes else {
            return Ok(Vec::new());
        };
        let ids = self.search(&decode_vector(&bytes)?, Some(asset))?;
        let mut items = Vec::new();
        for id in ids.into_iter().take(8) {
            let name: Option<String> = self
                .db
                .connection()?
                .query_row(
                    "SELECT name FROM assets WHERE id=?1 AND deleted_at IS NULL",
                    [id.to_string()],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(name) = name {
                items.push((id, name));
            }
        }
        Ok(items)
    }
    /// 按同模型修订的向量相似度筛选素材，维护有界候选集合。
    pub fn search(&self, query: &[f32], exclude: Option<AssetId>) -> Result<Vec<AssetId>> {
        let mut stmt=self.db.connection()?.prepare("SELECT a.id,e.vector FROM ai_embeddings e JOIN assets a ON a.display_version_id=e.version_id WHERE \
                a.deleted_at IS NULL AND e.model_id='chinese-clip' AND e.revision=?1")?;

        let mut rows = stmt.query([&self.revision])?;
        let mut top: Vec<(AssetId, f32)> = Vec::new();
        while let Some(row) = rows.next()? {
            let id = AssetId(parse_id(&row.get::<_, String>(0)?)?);
            if exclude == Some(id) {
                continue;
            }
            let bytes: Vec<u8> = row.get(1)?;
            let vector = decode_vector(&bytes)?;
            if vector.len() != query.len() {
                continue;
            }
            let score = query.iter().zip(vector).map(|(a, b)| a * b).sum::<f32>();
            top.push((id, score));
            if top.len() > 400 {
                top.sort_by(|a, b| b.1.total_cmp(&a.1));
                top.truncate(200);
            }
        }
        top.sort_by(|a, b| b.1.total_cmp(&a.1));
        top.truncate(200);
        Ok(top.into_iter().map(|(id, _)| id).collect())
    }
}
/// 将浮点向量编码为小端字节，统一数据库中的向量存储格式。
fn vector_bytes(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}
/// 校验 JSON 向量的维度与有限值，并归一化为单位向量。
pub fn query_vector(value: &serde_json::Value) -> Result<Vec<f32>> {
    let raw = value
        .as_array()
        .ok_or_else(|| Error::Invalid("AI 向量缺失".into()))?;
    if raw.is_empty() || raw.len() > 4096 {
        return Err(Error::Invalid("AI 向量维度无效".into()));
    }
    let mut values = raw
        .iter()
        .map(|v| {
            v.as_f64()
                .map(|v| v as f32)
                .filter(|v| v.is_finite())
                .ok_or_else(|| Error::Invalid("AI 向量包含非有限值".into()))
        })
        .collect::<Result<Vec<_>>>()?;
    let norm = values.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm <= 0.0 || !norm.is_finite() {
        return Err(Error::Invalid("AI 向量长度无效".into()));
    }
    for v in &mut values {
        *v /= norm;
    }
    Ok(values)
}
/// 从小端字节还原浮点向量，拒绝非法长度和非有限数值。
pub fn decode_vector(bytes: &[u8]) -> Result<Vec<f32>> {
    if bytes.is_empty() || !bytes.len().is_multiple_of(4) || bytes.len() > 16384 {
        return Err(Error::Invalid("已存向量数据无效".into()));
    }
    let values = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect::<Vec<_>>();
    if values.iter().any(|v| !v.is_finite()) {
        return Err(Error::Invalid("已存向量包含非有限值".into()));
    }
    Ok(values)
}
pub struct PreparedImage(tempfile::NamedTempFile);
impl PreparedImage {
    /// 借用推理用临时图片路径，文件寿命由准备结果持有者管理。
    pub fn path(&self) -> &Path {
        self.0.path()
    }
}
#[derive(Debug, Clone)]
pub struct InferenceResult {
    pub model: ModelId,
    pub revision: String,
    pub data: serde_json::Value,
}
impl InferenceResult {
    /// 核对推理回执的模型身份与修订字段，拒绝不匹配的结果。
    pub fn checked(self, expected: ModelId) -> TaskResult<Self> {
        if self.model != expected || self.revision.trim().is_empty() || self.revision.len() > 256 {
            return Err(TaskError::InvalidInput("分析回执模型身份或修订无效".into()));
        }
        Ok(self)
    }
}
/// 将冻结素材来源转为有界的推理 PNG，返回持有临时文件的准备结果。
pub fn prepare_image(source: media::Source) -> Result<PreparedImage> {
    let preview = source.preview_edge(1024)?.image;
    let temp = tempfile::Builder::new().suffix(".png").tempfile()?;
    image::save_buffer(
        temp.path(),
        &preview.rgba,
        preview.width,
        preview.height,
        image::ColorType::Rgba8,
    )?;
    Ok(PreparedImage(temp))
}
/// 按默认分类配置校验并规范化 Qwen 输出。
pub fn qwen_payload(
    task: &'static str,
    output: serde_json::Value,
) -> TaskResult<serde_json::Value> {
    qwen_payload_for_profile(task, output, &AiProfile::default())
}
/// 按任务类型、启用词表和长度限制校验 Qwen 字段，并附加配置摘要。
pub fn qwen_payload_for_profile(
    task: &'static str,
    output: serde_json::Value,
    profile: &AiProfile,
) -> TaskResult<serde_json::Value> {
    let valid = match task {
        "qwen_classification" => {
            output["medium"].as_str().is_some_and(|v| {
                v.is_empty()
                    || profile
                        .categories
                        .iter()
                        .any(|c| c.kind == AiCategoryKind::Medium && c.enabled && c.id == v)
            }) && output["themes"].as_array().is_some_and(|v| {
                v.len() <= usize::from(profile.max_themes)
                    && v.iter().all(|t| {
                        t.as_str().is_some_and(|t| {
                            profile
                                .categories
                                .iter()
                                .any(|c| c.kind == AiCategoryKind::Theme && c.enabled && c.id == t)
                        })
                    })
            })
        }
        "qwen_description" => {
            output["description"]
                .as_str()
                .is_some_and(|v| v.chars().count() <= 300)
                && output["tags"].as_array().is_some_and(|v| {
                    v.len() <= 12
                        && v.iter()
                            .all(|t| t.as_str().is_some_and(|t| t.chars().count() <= 24))
                })
        }
        _ => false,
    };
    if !valid {
        return Err(TaskError::InvalidInput(
            "Qwen 返回的字段、词表或长度不符合分析协议".into(),
        ));
    }
    let mut result = serde_json::json!({"task": task});
    if task == "qwen_classification" {
        let medium = output["medium"].as_str().unwrap_or_default();
        let mut themes = output["themes"]
            .as_array()
            .map(|values| {
                values
                    .iter()
                    .filter_map(|value| value.as_str())
                    .filter_map(|id| {
                        profile
                            .categories
                            .iter()
                            .find(|c| c.id == id && c.kind == AiCategoryKind::Theme && c.enabled)
                    })
                    .map(|c| c.id.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        themes.sort_by_key(|id| profile.categories.iter().position(|c| c.id == *id));
        themes.dedup();
        result["medium"] = serde_json::Value::String(medium.into());
        result["themes"] = serde_json::json!(themes);
        result["profile_hash"] = serde_json::json!(
            crate::ai_profile::classification_hash(profile)
                .map_err(|e| TaskError::Storage(e.to_string()))?
        );
    } else {
        if let Some(description) = output["description"].as_str() {
            result["description"] = serde_json::Value::String(description.into());
        }
        if let Some(tags) = output["tags"].as_array() {
            let mut normalized = Vec::<String>::new();
            for tag in tags.iter().filter_map(|s| s.as_str()) {
                let tag = tag.split_whitespace().collect::<Vec<_>>().join(" ");
                let tag = if tag.eq_ignore_ascii_case("cosplay") {
                    "角色扮演".into()
                } else {
                    tag
                };
                if !tag.is_empty() && !normalized.contains(&tag) {
                    normalized.push(tag);
                }
            }
            result["tags"] = serde_json::json!(normalized);
        }
    }
    if let Some(uncertainties) = output["uncertainties"].as_array() {
        result["uncertainties"] = serde_json::json!(uncertainties);
    }
    Ok(result)
}
/// 提交版本绑定的分析结果，保留人工分类与人物归属等人工状态。
pub(crate) fn store(
    tx: &Connection,
    version: &str,
    model: &str,
    revision: &str,
    result: &serde_json::Value,
) -> Result<()> {
    if revision.is_empty() || revision.len() > 256 {
        return Err(Error::Invalid("AI 结果缺少有效模型修订".into()));
    }
    // Each Qwen task replaces only its own fields; the other successful task survives.
    let incoming_task = result["task"].as_str().unwrap_or_default().to_string();
    let mut merged = result.clone();
    if model == "qwen35-4b" {
        let previous: Option<String> = tx.query_row(
            "SELECT data FROM ai_results WHERE version_id=?1 AND model_id=?2 AND revision=?3 AND preprocessing=?4",
            params![version, model, revision, PREPROCESSING], |row| row.get(0),
        ).optional()?;
        if let Some(previous) = previous {
            let mut previous: serde_json::Value = serde_json::from_str(&previous)
                .map_err(|error| Error::Invalid(error.to_string()))?;
            if let (Some(previous), Some(incoming)) = (previous.as_object_mut(), result.as_object())
            {
                previous.extend(
                    incoming
                        .iter()
                        .map(|(key, value)| (key.clone(), value.clone())),
                );
            }
            merged = previous;
        }
    }
    let result = &merged;
    let data = serde_json::to_string(result).map_err(|e| Error::Invalid(e.to_string()))?;
    if data.len() > 1024 * 1024 {
        return Err(Error::Invalid("AI 分析回执过大".into()));
    }
    let asset: Option<String> = tx
        .query_row(
            "SELECT asset_id FROM asset_versions WHERE id=?1",
            [version],
            |r| r.get(0),
        )
        .optional()?;
    let Some(_) = asset else { return Ok(()) };
    tx.execute("INSERT INTO ai_results(version_id,model_id,revision,preprocessing,data,created_at) \
                VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(version_id,model_id,revision,preprocessing) DO UPDATE SET \
                data=excluded.data,created_at=excluded.created_at",
params![version,
model,
revision,
PREPROCESSING,
data,
now_ms()?])?;

    if model == "chinese-clip" {
        let embedding = query_vector(&result["image_vector"])?;
        tx.execute("INSERT INTO ai_embeddings VALUES(?1,?2,?3,?4) ON CONFLICT(version_id,model_id,revision) DO UPDATE \
                SET vector=excluded.vector",
params![version,
model,
revision,
vector_bytes(&embedding)])?;

        // Qwen owns effective categories now; CLIP only refreshes retrieval vectors.
    }
    if model == "faces" {
        store_faces(tx, version, revision, result)?;
    }
    if model == "qwen35-4b" {
        let mut task_result = merged.clone();
        task_result["task"] = serde_json::json!(incoming_task);
        crate::ai_content::save_run(tx, version, revision, &task_result)?;
    }
    Ok(())
}

/// 计算两个人脸向量的相似度，供人物匹配使用。
fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return -1.0;
    }
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}
/// 根据人脸向量给出候选人物分组，供后续归属判断。
pub(crate) fn suggest_people(
    pool: &std::sync::Arc<read_pool::ReadPool>,
    version: &str,
    revision: &str,
    result: &mut serde_json::Value,
) -> Result<()> {
    let Some(faces) = result["faces"].as_array_mut() else {
        return Ok(());
    };
    let vectors = faces
        .iter()
        .take(64)
        .map(|f| query_vector(&f["vector"]))
        .collect::<Result<Vec<_>>>()?;
    let mut best: Vec<Option<(String, f32)>> = vec![None; vectors.len()];
    let lease = pool.checkout()?;
    let db = lease.connection()?;
    let mut stmt = db.prepare("SELECT p.person_id,p.vector FROM person_vectors p WHERE p.revision=?1 AND NOT EXISTS(SELECT 1 FROM \
                detected_faces f WHERE f.version_id=?2 AND f.person_id=p.person_id)")?;

    let mut rows = stmt.query(params![revision, version])?;
    while let Some(row) = rows.next()? {
        let id: String = row.get(0)?;
        let bytes: Vec<u8> = row.get(1)?;
        let other = decode_vector(&bytes)?;
        for (embedding, best) in vectors.iter().zip(&mut best) {
            let score = cosine(embedding, &other);
            if score >= 0.6 && best.as_ref().is_none_or(|(_, old)| score > *old) {
                *best = Some((id.clone(), score));
            }
        }
    }
    // One photograph cannot automatically put two detections in the same person.
    let mut order = (0..best.len()).collect::<Vec<_>>();
    order.sort_by(|a, b| {
        best[*b]
            .as_ref()
            .map(|v| v.1)
            .unwrap_or(-1.)
            .total_cmp(&best[*a].as_ref().map(|v| v.1).unwrap_or(-1.))
    });
    let mut used = std::collections::HashSet::new();
    for index in order {
        if let Some((id, _)) = &best[index]
            && used.insert(id.clone())
        {
            faces[index]["suggested_person"] = serde_json::Value::String(id.clone());
        }
    }
    Ok(())
}
/// 根据人物当前有效人脸重建代表向量，剔除失效来源。
fn refresh_person_vectors(tx: &Connection, person: &str) -> Result<()> {
    tx.execute("DELETE FROM person_vectors WHERE person_id=?1", [person])?;
    let revisions = tx
        .prepare("SELECT DISTINCT revision FROM detected_faces WHERE person_id=?1")?
        .query_map([person], |r| r.get::<_, String>(0))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for revision in revisions {
        let mut sum: Vec<f32> = Vec::new();
        let mut count = 0;
        let mut stmt=tx.prepare("SELECT vector FROM detected_faces WHERE person_id=?1 AND revision=?2 ORDER BY manual DESC,rowid DESC LIMIT 64")?;
        let mut rows = stmt.query(params![person, revision])?;
        while let Some(row) = rows.next()? {
            let bytes: Vec<u8> = row.get(0)?;
            if bytes.is_empty() {
                continue;
            } // Qwen compares portraits directly; it does not invent embeddings.
            let values = decode_vector(&bytes)?;
            if sum.is_empty() {
                sum.resize(values.len(), 0.);
            }
            if sum.len() != values.len() {
                continue;
            }
            for (s, v) in sum.iter_mut().zip(values) {
                *s += v;
            }
            count += 1;
        }
        let norm = sum.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 0. && norm.is_finite() {
            for v in &mut sum {
                *v /= norm;
            }
            tx.execute(
                "INSERT INTO person_vectors VALUES(?1,?2,?3,?4)",
                params![person, revision, vector_bytes(&sum), count],
            )?;
        }
    }
    Ok(())
}
struct PreviousFace {
    id: String,
    person: Option<String>,
    manual: bool,
    bounds: [f32; 4],
    vector: Vec<f32>,
}
/// 计算两个人脸框的重叠程度，用于检测结果去重或匹配。
fn overlap(a: &[f32; 4], b: &[f32; 4]) -> f32 {
    let w = ((a[0] + a[2]).min(b[0] + b[2]) - a[0].max(b[0])).max(0.);
    let h = ((a[1] + a[3]).min(b[1] + b[3]) - a[1].max(b[1])).max(0.);
    let intersection = w * h;
    intersection / (a[2] * a[3] + b[2] * b[3] - intersection).max(f32::EPSILON)
}
/// 保存当前版本的人脸检测及特征数据，并维护人物候选关联。
fn store_faces(
    tx: &Connection,
    version: &str,
    revision: &str,
    result: &serde_json::Value,
) -> Result<()> {
    let mut stmt=tx.prepare("SELECT id,person_id,manual,box,vector FROM detected_faces WHERE version_id=?1 AND revision=?2")?;
    let rows = stmt
        .query_map(params![version, revision], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, bool>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Vec<u8>>(4)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let old = rows
        .into_iter()
        .map(|(id, person, manual, bounds, bytes)| {
            Ok(PreviousFace {
                id,
                person,
                manual,
                bounds: serde_json::from_str(&bounds).map_err(|e| Error::Invalid(e.to_string()))?,
                vector: decode_vector(&bytes)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let scope = workspace::version_scope(tx, version)?;
    let mut affected = old
        .iter()
        .filter_map(|f| f.person.clone())
        .collect::<std::collections::HashSet<_>>();
    tx.execute(
        "DELETE FROM detected_faces WHERE version_id=?1 AND revision=?2 AND manual=0",
        params![version, revision],
    )?;
    let mut ordinal:i64=tx.query_row("SELECT coalesce(max(ordinal),-1)+1 FROM detected_faces WHERE version_id=?1 AND revision=?2",params![version,revision],|r|r.get(0))?;
    let mut matched = std::collections::HashSet::new();
    let mut assigned = old
        .iter()
        .filter(|f| f.manual)
        .filter_map(|f| f.person.clone())
        .collect::<std::collections::HashSet<_>>();
    for face in result["faces"].as_array().into_iter().flatten().take(64) {
        let embedding = query_vector(&face["vector"])?;
        let bounds: [f32; 4] = serde_json::from_value(face["box"].clone())
            .map_err(|e| Error::Invalid(e.to_string()))?;
        if bounds
            .iter()
            .any(|n| !n.is_finite() || !(0.0..=1.0).contains(n))
            || bounds[2] <= 0.
            || bounds[3] <= 0.
        {
            return Err(Error::Invalid("人脸位置无效".into()));
        }
        let previous = old
            .iter()
            .filter(|f| !matched.contains(&f.id))
            .filter_map(|f| {
                let iou = overlap(&bounds, &f.bounds);
                let score = cosine(&embedding, &f.vector);
                (iou >= 0.4 && score >= 0.65).then_some((f, iou + score))
            })
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(f, _)| f);
        if let Some(previous) = previous {
            matched.insert(previous.id.clone());
            if previous.manual {
                continue;
            }
        }
        let candidate = previous
            .and_then(|f| f.person.as_deref())
            .or_else(|| face["suggested_person"].as_str());
        let person = if let Some(id) = candidate.filter(|id| !assigned.contains(*id)) {
            let exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM people WHERE id=?1 AND coalesce((SELECT workspace FROM workspace_people WHERE person_id=people.id),'')=?2)",
                params![id,scope],
                |r| r.get(0),
            )?;
            exists.then(|| id.to_string())
        } else {
            None
        };
        let person = match person {
            Some(id) => id,
            None => {
                let id = Uuid::new_v4().to_string();
                tx.execute(
                    "INSERT INTO people VALUES(?1,'未命名人物',?2,?3)",
                    params![id, revision, now_ms()?],
                )?;
                workspace::own_person(tx, &id, &scope)?;
                id
            }
        };
        assigned.insert(person.clone());
        affected.insert(person.clone());
        let id = previous
            .map(|f| f.id.clone())
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        tx.execute("INSERT INTO detected_faces(id,version_id,revision,ordinal,box,vector,person_id) VALUES(?1,?2,?3,?4,?5,?6,?7)",
params![id,
version,
revision,
ordinal,
serde_json::to_string(&bounds).map_err(|e|Error::Invalid(e.to_string()))?,
vector_bytes(&embedding),
person])?;

        ordinal += 1;
    }
    for person in affected {
        refresh_person_vectors(tx, &person)?;
    }
    tx.execute("DELETE FROM people WHERE name='未命名人物' AND NOT EXISTS(SELECT 1 FROM detected_faces f WHERE f.person_id=people.id)",[])?;
    Ok(())
}
impl Library {
    /// 冻结语义检索所需的读取租约与模型修订。
    pub fn semantic_plan(&self, revision: String) -> Result<SemanticPlan> {
        let db = self.readers.checkout()?;
        Ok(SemanticPlan {
            db,
            _lease: self._lock.clone(),
            revision,
        })
    }
    /// 保存素材的人工分类，保留人工优先于自动分析的语义。
    pub fn set_category(&mut self, ids: &[AssetId], category: &str) -> Result<()> {
        self.require_workspace_assets(ids)?;
        if ids.is_empty() || ids.len() > 10000 || category.trim().is_empty() || category.len() > 128
        {
            return Err(Error::Invalid("分类名称或选择范围无效".into()));
        }
        let tx = self.db.transaction()?;
        for id in ids {
            tx.execute("INSERT INTO asset_classification(asset_id,category,manual) VALUES(?1,?2,1) ON CONFLICT(asset_id) DO \
                UPDATE SET category=excluded.category,manual=1",
params![id.to_string(),
category.trim()])?;
        }
        tx.commit()?;
        self.snapshot = None;
        Ok(())
    }
    /// 查询当前项目的人物分组及摘要信息。
    pub fn people(&self) -> Result<Vec<Person>> {
        self.db.prepare("SELECT p.id,p.name,count(DISTINCT a.id) FROM people p LEFT JOIN detected_faces f ON \
                f.person_id=p.id LEFT JOIN assets a ON a.display_version_id=f.version_id AND a.deleted_at IS NULL \
                WHERE coalesce((SELECT workspace FROM workspace_people WHERE person_id=p.id),'')=?1 GROUP BY p.id ORDER BY count(DISTINCT a.id) DESC,p.name")?.query_map([self.workspace_key()],
|r|Ok(Person{
id:r.get(0)?,
name:r.get(1)?,
count:r.get(2)?}
))?.collect::<std::result::Result<Vec<_>,
_>>().map_err(Error::from)
    }
    /// 校验人物归属和名称后更新显示名称。
    pub fn rename_person(&mut self, id: &str, name: &str) -> Result<()> {
        self.require_workspace_object("people", id)?;
        let name = name.trim();
        if name.is_empty() || name.len() > 128 {
            return Err(Error::Invalid("人物名称无效".into()));
        }
        if self
            .db
            .execute("UPDATE people SET name=?2 WHERE id=?1", params![id, name])?
            == 0
        {
            return Err(Error::Invalid("人物记录已不存在".into()));
        }
        Ok(())
    }
    /// 合并人物分组及人脸关联，并更新人物代表向量。
    pub fn merge_people(&mut self, source: &str, target: &str) -> Result<()> {
        self.require_workspace_object("people", source)?;
        self.require_workspace_object("people", target)?;
        if source == target {
            return Err(Error::Invalid("请选择两个不同人物".into()));
        }
        let tx = self.db.transaction()?;
        let count: u64 = tx.query_row(
            "SELECT count(*) FROM people WHERE id IN(?1,?2)",
            params![source, target],
            |r| r.get(0),
        )?;
        if count != 2 {
            return Err(Error::Invalid("人物记录已改变".into()));
        }
        tx.execute(
            "UPDATE detected_faces SET person_id=?2,manual=1 WHERE person_id=?1",
            params![source, target],
        )?;
        tx.execute("DELETE FROM people WHERE id=?1", [source])?;
        refresh_person_vectors(&tx, target)?;
        tx.commit()?;
        self.snapshot = None;
        Ok(())
    }
    /// 读取素材当前版本的人脸检测和人物归属。
    pub fn faces(&self, asset: AssetId) -> Result<Vec<Face>> {
        let mut stmt=self.db.prepare("SELECT f.id,f.version_id,f.person_id,coalesce(p.name,'未分组'),f.box FROM detected_faces f JOIN assets \
                a ON a.display_version_id=f.version_id LEFT JOIN people p ON p.id=f.person_id WHERE a.id=?1 ORDER \
                BY f.ordinal")?;

        let rows = stmt
            .query_map([asset.to_string()], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(id, version, person, name, bounds)| {
                Ok(Face {
                    id,
                    version: AssetVersionId(parse_id(&version)?),
                    person,
                    name,
                    bounds: serde_json::from_str(&bounds)
                        .map_err(|e| Error::Invalid(e.to_string()))?,
                })
            })
            .collect()
    }
    /// 保存人工人脸归属，并更新相关人物代表数据。
    pub fn assign_face(&mut self, face: &str, person: Option<&str>) -> Result<()> {
        let version: String = self.db.query_row(
            "SELECT version_id FROM detected_faces WHERE id=?1",
            [face],
            |r| r.get(0),
        )?;
        let scope = workspace::version_scope(&self.db, &version)?;
        if scope != self.workspace_key() {
            return Err(Error::Invalid("请进入人脸所属项目后修改人物分组".into()));
        }
        if let Some(person) = person {
            self.require_workspace_object("people", person)?;
        }
        let tx = self.db.transaction()?;
        let (revision, old): (String, Option<String>) = tx.query_row(
            "SELECT revision,person_id FROM detected_faces WHERE id=?1",
            [face],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let person = if let Some(person) = person {
            let exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM people WHERE id=?1)",
                [person],
                |r| r.get(0),
            )?;
            if !exists {
                return Err(Error::Invalid("人物记录已不存在".into()));
            }
            person.to_string()
        } else {
            let id = Uuid::new_v4().to_string();
            tx.execute(
                "INSERT INTO people VALUES(?1,'未命名人物',?2,?3)",
                params![id, revision, now_ms()?],
            )?;
            workspace::own_person(&tx, &id, &scope)?;
            id
        };
        tx.execute(
            "UPDATE detected_faces SET person_id=?2,manual=1 WHERE id=?1",
            params![face, person],
        )?;
        if let Some(old) = old {
            refresh_person_vectors(&tx, &old)?;
        }
        refresh_person_vectors(&tx, &person)?;
        tx.execute("DELETE FROM people WHERE name='未命名人物' AND NOT EXISTS(SELECT 1 FROM detected_faces f WHERE f.person_id=people.id)",[])?;
        tx.commit()?;
        self.snapshot = None;
        Ok(())
    }
    /// 清理指定 AI 索引数据，按既有规则保留人工内容及其他模型结果。
    pub fn clear_ai_indexes(&mut self, model: &str) -> Result<()> {
        let scope = self.workspace_key();
        let tx = self.db.transaction()?;
        if model == "qwen35-4b" {
            tx.execute("DELETE FROM ai_analysis_runs WHERE version_id IN(SELECT v.id FROM asset_versions v JOIN asset_workspaces w ON w.asset_id=v.asset_id WHERE w.workspace=?1)",[&scope])?;
        }
        tx.execute("DELETE FROM ai_results WHERE model_id=?1 AND version_id IN(SELECT v.id FROM asset_versions v JOIN asset_workspaces w ON w.asset_id=v.asset_id WHERE w.workspace=?2)", params![model,scope])?;
        tx.execute("DELETE FROM ai_embeddings WHERE model_id=?1 AND version_id IN(SELECT v.id FROM asset_versions v JOIN asset_workspaces w ON w.asset_id=v.asset_id WHERE w.workspace=?2)", params![model,scope])?;
        if model == "faces" {
            tx.execute("DELETE FROM detected_faces WHERE manual=0 AND version_id IN(SELECT v.id FROM asset_versions v JOIN asset_workspaces w ON w.asset_id=v.asset_id WHERE w.workspace=?1)", [&scope])?;
            tx.execute("DELETE FROM person_vectors WHERE person_id IN(SELECT id FROM people WHERE coalesce((SELECT workspace FROM workspace_people WHERE person_id=people.id),'')=?1)", [&scope])?;
            let people = tx
                .prepare(
                    "SELECT DISTINCT person_id FROM detected_faces WHERE person_id IS NOT NULL",
                )?
                .query_map([], |r| r.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            for person in people {
                refresh_person_vectors(&tx, &person)?;
            }
            tx.execute("DELETE FROM people WHERE name='未命名人物' AND NOT EXISTS(SELECT 1 FROM detected_faces f WHERE f.person_id=people.id)",[])?;
        }
        tx.commit()?;
        self.snapshot = None;
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    /// 验证旧展示版本迟到的分析不会修改新版本分类。
    fn late_old_version_analysis_cannot_classify_the_new_display_version() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("source.png");
        image::RgbImage::from_pixel(2, 2, image::Rgb([5, 10, 15]))
            .save(&input)
            .unwrap();
        let mut library = Library::create(dir.path(), "版本边界").unwrap();
        library.import(&input, None).unwrap();
        let asset = library
            .page(&Query::default(), 0, 1)
            .unwrap()
            .assets
            .remove(0);
        let new_version = AssetVersionId::default();
        library.db.execute("INSERT INTO asset_versions(id,asset_id,path,sha256,bytes,width,height,format,origin) SELECT ?1,asset_id,'originals/new-version.png',sha256,bytes,width,height,format,origin FROM asset_versions WHERE id=?2",params![new_version.to_string(),asset.version_id.to_string()]).unwrap();
        library.display_version(asset.id, new_version).unwrap();
        let result = serde_json::json!({"image_vector":[1.0,0.0],"labels":[{"label":"旧分类"}]});
        store(
            &library.db,
            &asset.version_id.to_string(),
            "chinese-clip",
            "injected-revision",
            &result,
        )
        .unwrap();
        assert!(library.details(asset.id).unwrap().category.is_empty());
        assert!(
            library
                .semantic_plan("injected-revision".into())
                .unwrap()
                .search(&[1.0, 0.0], None)
                .unwrap()
                .is_empty()
        );
        library.display_version(asset.id, asset.version_id).unwrap();
        assert_eq!(
            library
                .semantic_plan("injected-revision".into())
                .unwrap()
                .search(&[1.0, 0.0], None)
                .unwrap(),
            vec![asset.id]
        );
        assert!(
            library
                .semantic_plan("different-revision".into())
                .unwrap()
                .search(&[1.0, 0.0], None)
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    /// 验证 Qwen 结果持久化，CLIP 结果不会覆盖分类。
    fn qwen_results_are_persisted_and_clip_no_longer_overwrites_categories() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("cosplay.png");
        image::RgbImage::from_pixel(4, 4, image::Rgb([220, 90, 140]))
            .save(&input)
            .unwrap();
        let mut library = Library::create(dir.path(), "Qwen边界").unwrap();
        library.import(&input, None).unwrap();
        let asset = library
            .page(&Query::default(), 0, 1)
            .unwrap()
            .assets
            .remove(0);
        let classification = qwen_payload(
            "qwen_classification",
            serde_json::json!({
                "medium": "photography",
                "themes": ["portrait", "cosplay"],
                "uncertainties": []
            }),
        )
        .unwrap();
        store(
            &library.db,
            &asset.version_id.to_string(),
            "qwen35-4b",
            "qwen-revision",
            &classification,
        )
        .unwrap();
        assert_eq!(library.details(asset.id).unwrap().category, "人像");
        let description = qwen_payload(
            "qwen_description",
            serde_json::json!({
                "description": "人物照片，手持道具。",
                "tags": ["角色扮演", "灯笼"],
                "uncertainties": []
            }),
        )
        .unwrap();
        store(
            &library.db,
            &asset.version_id.to_string(),
            "qwen35-4b",
            "qwen-revision",
            &description,
        )
        .unwrap();
        let clip = serde_json::json!({"image_vector":[1.0,0.0],"labels":[{"label":"插画"}]});
        store(
            &library.db,
            &asset.version_id.to_string(),
            "chinese-clip",
            "clip-revision",
            &clip,
        )
        .unwrap();
        assert_eq!(library.details(asset.id).unwrap().category, "人像");
        assert_eq!(
            library.details(asset.id).unwrap().ai_caption,
            "人物照片，手持道具。"
        );
        let data: String = library
            .db
            .query_row(
                "SELECT data FROM ai_results WHERE model_id='qwen35-4b' AND version_id=?1",
                [asset.version_id.to_string()],
                |row| row.get(0),
            )
            .unwrap();
        let data: serde_json::Value = serde_json::from_str(&data).unwrap();
        assert_eq!(data["medium"], "photography");
        assert!(data["themes"].is_array());
        assert_eq!(data["tags"], serde_json::json!(["角色扮演", "灯笼"]));
        // Reclassification must preserve the successful description and human category.
        library.set_category(&[asset.id], "人工分类").unwrap();
        store(
            &library.db,
            &asset.version_id.to_string(),
            "qwen35-4b",
            "qwen-revision",
            &classification,
        )
        .unwrap();
        assert_eq!(
            library.details(asset.id).unwrap().ai_caption,
            "人物照片，手持道具。"
        );
        assert_eq!(library.details(asset.id).unwrap().category, "人工分类");
        assert_eq!(
            library
                .page(
                    &Query {
                        text: "手持道具".into(),
                        ..Query::default()
                    },
                    0,
                    10
                )
                .unwrap()
                .total,
            1
        );
    }
}
