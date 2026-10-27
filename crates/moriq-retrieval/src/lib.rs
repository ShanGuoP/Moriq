//! Scoped catalog retrieval. No Library, UI, media decoder or model runtime dependency.
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
use moriq_domain::{AssetId, AssetVersionId, ProjectId, SimilarAsset};
use rusqlite::{Connection, OptionalExtension, params};

/// Every retrieval explicitly chooses its candidate and input-asset scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryScope {
    Library,
    Unassigned,
    Project(ProjectId),
}
impl QueryScope {
    fn key(self) -> Option<String> {
        match self {
            Self::Library => None,
            Self::Unassigned => Some(String::new()),
            Self::Project(id) => Some(id.to_string()),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Db(#[from] rusqlite::Error),
    #[error("{0}")]
    Invalid(String),
}
type Result<T> = std::result::Result<T, Error>;

/// The host supplies a read-only catalog lease. Scope has no implicit default.
pub struct Catalog<'a> {
    db: &'a Connection,
    scope: QueryScope,
}
impl<'a> Catalog<'a> {
    pub fn new(db: &'a Connection, scope: QueryScope) -> Self {
        Self { db, scope }
    }

    pub fn require_asset(&self, asset: AssetId) -> Result<()> {
        let exists: bool = self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM assets a WHERE a.id=?1 AND a.deleted_at IS NULL
             AND (?2 IS NULL OR coalesce((SELECT workspace FROM asset_workspaces WHERE asset_id=a.id),'')=?2))",
            params![asset.to_string(), self.scope.key()],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(Error::Invalid(
                "所选素材不属于当前查询范围或已移入回收站".into(),
            ));
        }
        Ok(())
    }

    pub fn similar(&self, asset: AssetId) -> Result<Vec<SimilarAsset>> {
        self.require_asset(asset)?;
        let reference: Option<(i64, u8, u8, u8, f64)> = self
            .db
            .query_row(
                "SELECT f.hash,f.red,f.green,f.blue,f.aspect FROM image_fingerprints f
             JOIN assets a ON a.display_version_id=f.version_id WHERE a.id=?1 AND f.algorithm=1 AND a.deleted_at IS NULL
             AND (?2 IS NULL OR coalesce((SELECT workspace FROM asset_workspaces WHERE asset_id=a.id),'')=?2)",
                params![asset.to_string(), self.scope.key()],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .optional()?;
        let Some((hash, r, g, b, aspect)) = reference else {
            return Err(Error::Invalid(
                "所选图片还没有近似索引，请先在后台任务中建立索引".into(),
            ));
        };
        let mut stmt = self.db.prepare(
            "SELECT a.id,a.name,f.hash,f.red,f.green,f.blue FROM image_fingerprints f
             JOIN assets a ON a.display_version_id=f.version_id
             WHERE a.deleted_at IS NULL AND a.id<>?1 AND f.algorithm=1 AND f.aspect BETWEEN ?2 AND ?3
             AND (?4 IS NULL OR coalesce((SELECT workspace FROM asset_workspaces WHERE asset_id=a.id),'')=?4)",
        )?;
        let mut rows = stmt.query(params![
            asset.to_string(),
            aspect * 0.95,
            aspect * 1.05,
            self.scope.key()
        ])?;
        let mut matches = Vec::new();
        while let Some(row) = rows.next()? {
            let hash_value: i64 = row.get(2)?;
            let distance = ((hash as u64) ^ (hash_value as u64)).count_ones();
            let color: [u8; 3] = [row.get(3)?, row.get(4)?, row.get(5)?];
            if distance <= 8
                && color
                    .into_iter()
                    .zip([r, g, b])
                    .map(|(a, b)| u32::from(a.abs_diff(b)))
                    .sum::<u32>()
                    <= 100
            {
                matches.push(SimilarAsset {
                    id: asset_id(&row.get::<_, String>(0)?)?,
                    name: row.get(1)?,
                    distance,
                });
                if matches.len() > 200 {
                    matches.sort_by_key(|a| a.distance);
                    matches.truncate(100);
                }
            }
        }
        matches.sort_by(|a, b| a.distance.cmp(&b.distance).then(a.name.cmp(&b.name)));
        matches.truncate(100);
        Ok(matches)
    }

    pub fn semantic(
        &self,
        revision: &str,
        query: &[f32],
        exclude: Option<AssetId>,
    ) -> Result<Vec<AssetId>> {
        if let Some(asset) = exclude {
            self.require_asset(asset)?;
        }
        let mut stmt = self.db.prepare(
            "SELECT a.id,e.vector FROM ai_embeddings e JOIN assets a ON a.display_version_id=e.version_id
             WHERE a.deleted_at IS NULL AND e.model_id='chinese-clip' AND e.revision=?1
             AND (?2 IS NULL OR coalesce((SELECT workspace FROM asset_workspaces WHERE asset_id=a.id),'')=?2)",
        )?;
        let mut rows = stmt.query(params![revision, self.scope.key()])?;
        let mut top: Vec<(AssetId, f32)> = Vec::new();
        while let Some(row) = rows.next()? {
            let id = asset_id(&row.get::<_, String>(0)?)?;
            if exclude == Some(id) {
                continue;
            }
            let vector = decode_vector(&row.get::<_, Vec<u8>>(1)?)?;
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

    pub fn recommendations(
        &self,
        revision: &str,
        asset: AssetId,
        version: AssetVersionId,
    ) -> Result<Vec<(AssetId, String)>> {
        self.require_asset(asset)?;
        let bytes: Option<Vec<u8>> = self.db.query_row(
            "SELECT e.vector FROM ai_embeddings e JOIN assets a ON a.display_version_id=e.version_id
             WHERE a.id=?1 AND e.version_id=?2 AND e.model_id='chinese-clip' AND e.revision=?3 AND a.deleted_at IS NULL
             AND (?4 IS NULL OR coalesce((SELECT workspace FROM asset_workspaces WHERE asset_id=a.id),'')=?4)",
            params![asset.to_string(), version.to_string(), revision, self.scope.key()], |row| row.get(0),
        ).optional()?;
        let Some(bytes) = bytes else {
            return Ok(Vec::new());
        };
        let ids = self.semantic(revision, &decode_vector(&bytes)?, Some(asset))?;
        let mut items = Vec::new();
        for id in ids.into_iter().take(8) {
            let name: Option<String> = self.db.query_row(
                "SELECT name FROM assets a WHERE a.id=?1 AND a.deleted_at IS NULL
                 AND (?2 IS NULL OR coalesce((SELECT workspace FROM asset_workspaces WHERE asset_id=a.id),'')=?2)",
                params![id.to_string(), self.scope.key()], |row| row.get(0),
            ).optional()?;
            if let Some(name) = name {
                items.push((id, name));
            }
        }
        Ok(items)
    }
}

fn asset_id(value: &str) -> Result<AssetId> {
    uuid::Uuid::parse_str(value)
        .map(AssetId)
        .map_err(|_| Error::Invalid("资源库包含无效标识，请检查备份".into()))
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
