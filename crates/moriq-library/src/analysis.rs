//! Version-bound AI results; human labels and face assignments survive reanalysis.
use super::*;
pub const PREPROCESSING: &str = "srgb-1024-v1";
pub struct SemanticPlan {
    db: read_pool::ReadLease,
    _lease: std::sync::Arc<File>,
    revision: String,
}
impl SemanticPlan {
    pub fn recommendations(
        &self,
        asset: AssetId,
        version: AssetVersionId,
    ) -> Result<Vec<(AssetId, String)>> {
        let bytes:Option<Vec<u8>>=self.db.connection()?.query_row("SELECT e.vector FROM ai_embeddings e JOIN assets a ON a.display_version_id=e.version_id WHERE a.id=?1 AND e.version_id=?2 AND e.model_id='chinese-clip' AND e.revision=?3",params![asset.to_string(),version.to_string(),self.revision],|r|r.get(0)).optional()?;
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
    pub fn search(&self, query: &[f32], exclude: Option<AssetId>) -> Result<Vec<AssetId>> {
        let mut stmt=self.db.connection()?.prepare("SELECT a.id,e.vector FROM ai_embeddings e JOIN assets a ON a.display_version_id=e.version_id WHERE a.deleted_at IS NULL AND e.model_id='chinese-clip' AND e.revision=?1")?;
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
fn vector_bytes(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}
fn vector(value: &serde_json::Value) -> Result<Vec<f32>> {
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
pub fn model_revision(id: &str) -> Result<String> {
    moriq_ai::registry()
        .map_err(Error::Invalid)?
        .into_iter()
        .find(|m| m.id == id)
        .map(|m| m.revision)
        .ok_or_else(|| Error::Invalid("模型未登记".into()))
}
pub fn run(source: media::Source, model: &str) -> std::result::Result<serde_json::Value, String> {
    let preview = source.preview_edge(1024).map_err(|e| e.to_string())?.image;
    let temp = tempfile::Builder::new()
        .suffix(".png")
        .tempfile()
        .map_err(|e| e.to_string())?;
    image::save_buffer(
        temp.path(),
        &preview.rgba,
        preview.width,
        preview.height,
        image::ColorType::Rgba8,
    )
    .map_err(|e| e.to_string())?;
    moriq_ai::infer(model, serde_json::json!({"image":temp.path()}))
}
pub(crate) fn store(
    tx: &Connection,
    version: &str,
    model: &str,
    result: &serde_json::Value,
) -> Result<()> {
    let revision = model_revision(model)?;
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
    let Some(asset) = asset else { return Ok(()) };
    tx.execute("INSERT INTO ai_results(version_id,model_id,revision,preprocessing,data,created_at) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(version_id,model_id,revision,preprocessing) DO UPDATE SET data=excluded.data,created_at=excluded.created_at",params![version,model,revision,PREPROCESSING,data,now_ms()?])?;
    if model == "chinese-clip" {
        let embedding = vector(&result["image_vector"])?;
        tx.execute("INSERT INTO ai_embeddings VALUES(?1,?2,?3,?4) ON CONFLICT(version_id,model_id,revision) DO UPDATE SET vector=excluded.vector",params![version,model,revision,vector_bytes(&embedding)])?;
        if let Some(category) = result["labels"]
            .as_array()
            .and_then(|l| l.first())
            .and_then(|l| l["label"].as_str())
        {
            tx.execute("INSERT INTO asset_classification(asset_id,category,manual) VALUES(?1,?2,0) ON CONFLICT(asset_id) DO UPDATE SET category=excluded.category WHERE manual=0",params![asset,category])?;
        }
    }
    if model == "faces" {
        store_faces(tx, version, &revision, result)?;
    }
    Ok(())
}
fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return -1.0;
    }
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}
pub(crate) fn suggest_people(
    pool: &std::sync::Arc<read_pool::ReadPool>,
    version: &str,
    result: &mut serde_json::Value,
) -> Result<()> {
    let Some(faces) = result["faces"].as_array_mut() else {
        return Ok(());
    };
    let vectors = faces
        .iter()
        .take(64)
        .map(|f| vector(&f["vector"]))
        .collect::<Result<Vec<_>>>()?;
    let mut best: Vec<Option<(String, f32)>> = vec![None; vectors.len()];
    let lease = pool.checkout()?;
    let db = lease.connection()?;
    let mut stmt = db.prepare("SELECT p.person_id,p.vector FROM person_vectors p WHERE p.revision=?1 AND NOT EXISTS(SELECT 1 FROM detected_faces f WHERE f.version_id=?2 AND f.person_id=p.person_id)")?;
    let mut rows = stmt.query(params![model_revision("faces")?, version])?;
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
fn overlap(a: &[f32; 4], b: &[f32; 4]) -> f32 {
    let w = ((a[0] + a[2]).min(b[0] + b[2]) - a[0].max(b[0])).max(0.);
    let h = ((a[1] + a[3]).min(b[1] + b[3]) - a[1].max(b[1])).max(0.);
    let intersection = w * h;
    intersection / (a[2] * a[3] + b[2] * b[3] - intersection).max(f32::EPSILON)
}
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
        let embedding = vector(&face["vector"])?;
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
                "SELECT EXISTS(SELECT 1 FROM people WHERE id=?1)",
                [id],
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
                id
            }
        };
        assigned.insert(person.clone());
        affected.insert(person.clone());
        let id = previous
            .map(|f| f.id.clone())
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        tx.execute("INSERT INTO detected_faces(id,version_id,revision,ordinal,box,vector,person_id) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![id,version,revision,ordinal,serde_json::to_string(&bounds).map_err(|e|Error::Invalid(e.to_string()))?,vector_bytes(&embedding),person])?;
        ordinal += 1;
    }
    for person in affected {
        refresh_person_vectors(tx, &person)?;
    }
    tx.execute("DELETE FROM people WHERE name='未命名人物' AND NOT EXISTS(SELECT 1 FROM detected_faces f WHERE f.person_id=people.id)",[])?;
    Ok(())
}
impl Library {
    pub fn semantic_plan(&self) -> Result<SemanticPlan> {
        let db = self.readers.checkout()?;
        Ok(SemanticPlan {
            db,
            _lease: self._lock.clone(),
            revision: model_revision("chinese-clip")?,
        })
    }
    pub fn set_category(&mut self, ids: &[AssetId], category: &str) -> Result<()> {
        if ids.is_empty() || ids.len() > 10000 || category.trim().is_empty() || category.len() > 128
        {
            return Err(Error::Invalid("分类名称或选择范围无效".into()));
        }
        let tx = self.db.transaction()?;
        for id in ids {
            tx.execute("INSERT INTO asset_classification(asset_id,category,manual) VALUES(?1,?2,1) ON CONFLICT(asset_id) DO UPDATE SET category=excluded.category,manual=1",params![id.to_string(),category.trim()])?;
        }
        tx.commit()?;
        self.snapshot = None;
        Ok(())
    }
    pub fn people(&self) -> Result<Vec<Person>> {
        self.db.prepare("SELECT p.id,p.name,count(DISTINCT a.id) FROM people p LEFT JOIN detected_faces f ON f.person_id=p.id LEFT JOIN assets a ON a.display_version_id=f.version_id AND a.deleted_at IS NULL GROUP BY p.id ORDER BY count(DISTINCT a.id) DESC,p.name")?.query_map([],|r|Ok(Person{id:r.get(0)?,name:r.get(1)?,count:r.get(2)?}))?.collect::<std::result::Result<Vec<_>,_>>().map_err(Error::from)
    }
    pub fn rename_person(&mut self, id: &str, name: &str) -> Result<()> {
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
    pub fn merge_people(&mut self, source: &str, target: &str) -> Result<()> {
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
    pub fn faces(&self, asset: AssetId) -> Result<Vec<Face>> {
        let mut stmt=self.db.prepare("SELECT f.id,f.version_id,f.person_id,coalesce(p.name,'未分组'),f.box FROM detected_faces f JOIN assets a ON a.display_version_id=f.version_id LEFT JOIN people p ON p.id=f.person_id WHERE a.id=?1 ORDER BY f.ordinal")?;
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
    pub fn assign_face(&mut self, face: &str, person: Option<&str>) -> Result<()> {
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
    pub fn clear_ai_indexes(&mut self, model: &str) -> Result<()> {
        let tx = self.db.transaction()?;
        tx.execute("DELETE FROM ai_results WHERE model_id=?1", [model])?;
        tx.execute("DELETE FROM ai_embeddings WHERE model_id=?1", [model])?;
        if model == "faces" {
            tx.execute("DELETE FROM detected_faces WHERE manual=0", [])?;
            tx.execute("DELETE FROM person_vectors", [])?;
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
