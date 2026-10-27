//! Projects own asset/version identities. Copies retain content but never mutable state.
use super::*;
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct Copies {
    paths: Vec<PathBuf>,
    committed: bool,
}
impl Copies {
    /// 创建并记录项目专属文件副本，使未提交的复制能够在退出时清理。
    fn copy(&mut self, source: &Path, destination: &Path) -> Result<()> {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(destination)?;
        self.paths.push(destination.to_owned());
        std::io::copy(&mut File::open(source)?, &mut output)?;
        output.sync_all()?;
        Ok(())
    }
    /// 标记文件复制已提交，阻止清理守卫删除有效副本。
    pub fn commit(mut self) {
        self.committed = true;
    }
}
impl Drop for Copies {
    /// 清理未提交的项目副本；清理失败时留下诊断以供后续恢复。
    fn drop(&mut self) {
        if !self.committed {
            for path in &self.paths {
                if let Err(error) = fs::remove_file(path) {
                    tracing::warn!(%error, path = %path.display(), "project copy cleanup deferred");
                }
            }
        }
    }
}

// All identifiers come from the fixed internal table list, never user input.
/// 从固定内部表复制记录，并用参数覆盖新身份字段，避免拼接用户输入。
fn copy_rows(
    db: &Connection,
    table: &str,
    key: &str,
    source: &str,
    overrides: &[(&str, rusqlite::types::Value)],
) -> Result<()> {
    let columns = db
        .prepare(&format!("PRAGMA table_info({table})"))?
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut values = vec![rusqlite::types::Value::Text(source.into())];
    let expressions: Vec<_> = columns
        .iter()
        .map(|column| {
            if let Some((_, value)) = overrides.iter().find(|(name, _)| *name == column) {
                values.push(value.clone());
                format!("?{}", values.len())
            } else {
                format!("\"{column}\"")
            }
        })
        .collect();
    let names = columns
        .iter()
        .map(|c| format!("\"{c}\""))
        .collect::<Vec<_>>()
        .join(",");
    db.execute(
        &format!(
            "INSERT INTO {table}({names}) SELECT {} FROM {table} WHERE {key}=?1",
            expressions.join(",")
        ),
        rusqlite::params_from_iter(values),
    )?;
    Ok(())
}

/// 为目标项目创建独立素材和版本身份，复制必要文件与元数据并复用已有有效副本。
pub(super) fn clone_asset(
    db: &Connection,
    root: &Path,
    asset: &str,
    project: &str,
    files: &mut Copies,
) -> Result<String> {
    let workspace_schema: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='workspace_people')",
        [],
        |r| r.get(0),
    )?;
    if let Some(existing) = db
        .query_row(
            "SELECT c.asset_id FROM project_copies c WHERE source_asset_id=?1 AND project_id=?2 AND NOT EXISTS(SELECT 1 FROM project_assets p WHERE p.asset_id=c.asset_id AND p.project_id<>?2)",
            params![asset, project],
            |r| r.get::<_, String>(0),
        )
        .optional()?
    {
        db.execute(
            "INSERT OR IGNORE INTO project_assets VALUES(?1,?2)",
            params![project, existing],
        )?;
        return Ok(existing);
    }
    let new_asset = AssetId::default().to_string();
    let display: String = db.query_row(
        "SELECT display_version_id FROM assets WHERE id=?1",
        [asset],
        |r| r.get(0),
    )?;
    let versions = db.prepare("SELECT id,path,parent_version_id,EXISTS(SELECT 1 FROM file_locations f WHERE f.version_id=v.id) FROM asset_versions v WHERE asset_id=?1")?
        .query_map([asset], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, bool>(3)?)))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mapping: BTreeMap<_, _> = versions
        .iter()
        .map(|(id, _, _, _)| (id.clone(), AssetVersionId::default().to_string()))
        .collect();
    let text = |v: &str| rusqlite::types::Value::Text(v.into());
    copy_rows(
        db,
        "assets",
        "id",
        asset,
        &[
            ("id", text(&new_asset)),
            ("display_version_id", text(&mapping[&display])),
        ],
    )?;
    for (old, path, parent, referenced) in &versions {
        let new = &mapping[old];
        let extension = Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("bin");
        let relative = format!("assets/project-copies/{project}/{new}.{extension}");
        if !referenced {
            let source = safe_path(root, path)?;
            // Preserve the missing-file state of an offline asset as well as its metadata.
            if source.exists() {
                files.copy(&source, &safe_path(root, &relative)?)?;
            }
        }
        copy_rows(
            db,
            "asset_versions",
            "id",
            old,
            &[
                ("id", text(new)),
                ("asset_id", text(&new_asset)),
                ("path", text(&relative)),
                (
                    "parent_version_id",
                    parent
                        .as_ref()
                        .and_then(|id| mapping.get(id))
                        .map(|id| text(id))
                        .unwrap_or(rusqlite::types::Value::Null),
                ),
            ],
        )?;
        db.execute(
            "UPDATE project_files SET version_id=?1 WHERE project_id=?2 AND version_id=?3",
            params![new, project, old],
        )?;
        let thumbnail = safe_path(root, &format!("thumbnails/256/{old}.png"))?;
        if thumbnail.exists() {
            files.copy(
                &thumbnail,
                &safe_path(root, &format!("thumbnails/256/{new}.png"))?,
            )?;
        }
        for table in [
            "file_locations",
            "version_locations",
            "video_metadata",
            "version_exif",
            "image_fingerprints",
            "ai_results",
            "ai_embeddings",
            "ai_overrides",
            "ai_rejected_tags",
        ] {
            copy_rows(db, table, "version_id", old, &[("version_id", text(new))])?;
        }
        let runs = db
            .prepare("SELECT id FROM ai_analysis_runs WHERE version_id=?1 ORDER BY id")?
            .query_map([old], |r| r.get::<_, i64>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for run in runs {
            db.execute("INSERT INTO ai_analysis_runs(version_id,task,revision,preprocessing,data,created_at) SELECT ?1,task,revision,preprocessing,data,created_at FROM ai_analysis_runs WHERE id=?2", params![new, run])?;
            let cloned = db.last_insert_rowid();
            db.execute("INSERT INTO ai_effective_results SELECT ?1,task,?3 FROM ai_effective_results WHERE version_id=?2 AND run_id=?4", params![new, old, cloned, run])?;
        }
        let faces = db
            .prepare("SELECT id FROM detected_faces WHERE version_id=?1")?
            .query_map([old], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for face in faces {
            copy_rows(
                db,
                "detected_faces",
                "id",
                &face,
                &[
                    ("id", text(&Uuid::new_v4().to_string())),
                    ("version_id", text(new)),
                ],
            )?;
        }
    }
    for table in [
        "asset_tags",
        "asset_classification",
        "virtual_folder_assets",
    ] {
        if table == "virtual_folder_assets" && workspace_schema {
            continue;
        }
        copy_rows(
            db,
            table,
            "asset_id",
            asset,
            &[("asset_id", text(&new_asset))],
        )?;
    }
    db.execute(
        "INSERT INTO project_assets VALUES(?1,?2)",
        params![project, new_asset],
    )?;
    if workspace_schema {
        workspace::map_classification(db, asset, &mapping, project)?;
        db.execute("DELETE FROM asset_classification WHERE asset_id=?1 AND NOT EXISTS(SELECT 1 FROM workspace_vocabulary WHERE workspace=?2 AND label=asset_classification.category AND enabled=1)",params![new_asset,project])?;
        workspace::rehome_asset(db, &new_asset, project)?;
    }
    db.execute("DELETE FROM asset_order WHERE asset_id=?1 AND EXISTS(SELECT 1 FROM assets WHERE id=?1 AND deleted_at IS NOT NULL)",[&new_asset])?;
    db.execute(
        "INSERT INTO project_copies VALUES(?1,?2,?3) ON CONFLICT(source_asset_id,project_id) DO UPDATE SET asset_id=excluded.asset_id",
        params![asset, project, new_asset],
    )?;
    Ok(new_asset)
}

/// 升级旧版共享素材模型，为各项目拆分可变状态并保留原始内容。
pub(super) fn migrate(db: &mut Connection, root: &Path) -> Result<()> {
    let shared = db.prepare("SELECT project_id,asset_id FROM project_assets pa WHERE project_id<>(SELECT min(project_id) FROM project_assets WHERE asset_id=pa.asset_id) ORDER BY asset_id,project_id")?
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut files = Copies::default();
    let tx = db.transaction()?;
    tx.execute_batch("PRAGMA defer_foreign_keys=ON;")?;
    tx.execute_batch("CREATE TABLE IF NOT EXISTS project_copies(source_asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,PRIMARY KEY(source_asset_id,project_id));")?;
    for (project, asset) in shared {
        clone_asset(&tx, root, &asset, &project, &mut files)?;
        tx.execute(
            "DELETE FROM project_assets WHERE project_id=?1 AND asset_id=?2",
            params![project, asset],
        )?;
    }
    tx.execute_batch("DELETE FROM asset_order; INSERT INTO asset_order SELECT row_number() OVER(ORDER BY imported_at,id)-1,id FROM assets WHERE deleted_at IS NULL; PRAGMA user_version=25;")?;
    tx.commit()?;
    files.commit();
    Ok(())
}
