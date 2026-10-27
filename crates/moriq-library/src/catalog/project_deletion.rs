//! Project deletion reuses the durable purge journal and one metadata commit.
use super::*;
const TREE: &str = "WITH RECURSIVE tree(id) AS(SELECT ?1 UNION ALL SELECT p.id FROM projects p JOIN tree t ON p.parent_id=t.id)";
impl Library {
    pub fn project_deletion_plan(&self, root: ProjectId) -> Result<ProjectDeletionPlan> {
        let name = self.db.query_row(
            "SELECT name FROM projects WHERE id=?1",
            [root.to_string()],
            |r| r.get(0),
        )?;
        let strings = |sql: &str| -> Result<Vec<String>> {
            Ok(self
                .db
                .prepare(&format!("{TREE} {sql}"))?
                .query_map([root.to_string()], |r| r.get(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?)
        };
        let projects = strings("SELECT id FROM tree ORDER BY id")?;
        let members = strings(
            "SELECT DISTINCT asset_id FROM project_assets WHERE project_id IN(SELECT id FROM tree) ORDER BY asset_id",
        )?;
        let assets = strings(
            "SELECT DISTINCT pa.asset_id FROM project_assets pa WHERE pa.project_id IN(SELECT id FROM tree) AND \
                NOT EXISTS(SELECT 1 FROM project_assets outside WHERE outside.asset_id=pa.asset_id AND \
                outside.project_id NOT IN(SELECT id FROM tree)) ORDER BY pa.asset_id",
        )?;
        let exclusive: std::collections::HashSet<_> = assets.iter().cloned().collect();
        let shared = members
            .into_iter()
            .filter(|id| !exclusive.contains(id))
            .map(|s| parse_id(&s).map(AssetId))
            .collect::<Result<Vec<_>>>()?;
        let versions = strings(
            "SELECT v.id FROM asset_versions v WHERE v.asset_id IN(SELECT pa.asset_id FROM project_assets pa \
                WHERE pa.project_id IN(SELECT id FROM tree) AND NOT EXISTS(SELECT 1 FROM project_assets outside \
                WHERE outside.asset_id=pa.asset_id AND outside.project_id NOT IN(SELECT id FROM tree))) ORDER BY \
                v.id",
        )?;
        // A shared asset's canonical path can live in this project's folder. Keep it;
        // only unlink aliases that no remaining version or project needs.
        let aliases = strings(
            "SELECT pf.path FROM project_files pf WHERE pf.project_id IN(SELECT id FROM tree) AND NOT \
                EXISTS(SELECT 1 FROM asset_versions v WHERE v.path=pf.path COLLATE NOCASE) AND NOT EXISTS(SELECT 1 \
                FROM project_files other WHERE other.path=pf.path COLLATE NOCASE AND other.project_id NOT IN(SELECT \
                id FROM tree)) ORDER BY pf.path",
        )?;
        let referenced = self.db.query_row(&format!("{TREE} SELECT count(*) FROM file_locations f JOIN asset_versions v ON v.id=f.version_id WHERE \
                v.asset_id IN(SELECT pa.asset_id FROM project_assets pa WHERE pa.project_id IN(SELECT id FROM tree) \
                AND NOT EXISTS(SELECT 1 FROM project_assets outside WHERE outside.asset_id=pa.asset_id AND \
                outside.project_id NOT IN(SELECT id FROM tree)))"),
 [root.to_string()],
 |r| r.get(0))?;

        Ok(ProjectDeletionPlan {
            root,
            name,
            projects: projects
                .into_iter()
                .map(|s| parse_id(&s).map(ProjectId))
                .collect::<Result<_>>()?,
            assets: assets
                .into_iter()
                .map(|s| parse_id(&s).map(AssetId))
                .collect::<Result<_>>()?,
            shared,
            versions: versions
                .into_iter()
                .map(|s| parse_id(&s).map(AssetVersionId))
                .collect::<Result<_>>()?,
            aliases,
            referenced,
        })
    }
    pub fn delete_project_contents(&mut self, plan: &ProjectDeletionPlan) -> Result<PurgeReport> {
        self.recover_purges()?;
        let current = self.project_deletion_plan(plan.root)?;
        if &current != plan {
            return Err(Error::Invalid("删除范围已改变，请刷新后重新确认".into()));
        }
        self.run_purge(&current.assets, Some(&current))
    }
}
