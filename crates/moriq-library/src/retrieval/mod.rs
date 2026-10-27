//! Owns read leases only; the retrieval crate cannot access Library or its writer.
use crate::{Result, read_pool};
use moriq_domain::{AssetId, AssetVersionId, SimilarAsset};
use moriq_retrieval::{Catalog, QueryScope};

pub struct SimilarityPlan {
    connection: read_pool::ReadLease,
    scope: QueryScope,
}
impl SimilarityPlan {
    pub(crate) fn new(connection: read_pool::ReadLease, scope: QueryScope) -> Self {
        Self { connection, scope }
    }
    pub fn search(&self, asset: AssetId) -> Result<Vec<SimilarAsset>> {
        Ok(Catalog::new(self.connection.connection()?, self.scope).similar(asset)?)
    }
}

pub struct SemanticPlan {
    connection: read_pool::ReadLease,
    scope: QueryScope,
    revision: String,
}
impl SemanticPlan {
    pub(crate) fn new(
        connection: read_pool::ReadLease,
        scope: QueryScope,
        revision: String,
    ) -> Self {
        Self {
            connection,
            scope,
            revision,
        }
    }
    pub fn search(&self, query: &[f32], exclude: Option<AssetId>) -> Result<Vec<AssetId>> {
        Ok(
            Catalog::new(self.connection.connection()?, self.scope).semantic(
                &self.revision,
                query,
                exclude,
            )?,
        )
    }
    pub fn recommendations(
        &self,
        asset: AssetId,
        version: AssetVersionId,
    ) -> Result<Vec<(AssetId, String)>> {
        Ok(
            Catalog::new(self.connection.connection()?, self.scope).recommendations(
                &self.revision,
                asset,
                version,
            )?,
        )
    }
}
