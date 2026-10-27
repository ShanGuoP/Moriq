//! Qwen uses actual portraits for matching; no synthetic face embeddings are stored.
use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

pub struct References {
    _directory: tempfile::TempDir,
    pub items: Vec<(String, PathBuf)>,
}
pub(crate) fn references(
    pool: &Arc<read_pool::ReadPool>,
    scope: &str,
    cancel: &AtomicBool,
) -> Result<References> {
    let directory = tempfile::tempdir()?;
    let lease = pool.checkout()?;
    let rows = lease.connection()?.prepare("SELECT person_id,asset_id,version_id,box FROM (SELECT f.person_id,v.asset_id,f.version_id,f.box,row_number() OVER(PARTITION BY f.person_id ORDER BY f.manual DESC,json_extract(f.box,'$[2]')*json_extract(f.box,'$[3]') DESC,f.id) rank FROM detected_faces f JOIN asset_versions v ON v.id=f.version_id JOIN assets a ON a.display_version_id=v.id WHERE a.deleted_at IS NULL AND f.person_id IS NOT NULL AND coalesce((SELECT workspace FROM workspace_people WHERE person_id=f.person_id),'')=?1) WHERE rank=1 ORDER BY person_id")?
        .query_map([scope], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?)))?
        .collect::<std::result::Result<Vec<_>,_>>()?;
    drop(lease);
    let mut items = Vec::new();
    for (person, asset, version, bounds) in rows {
        if cancel.load(Ordering::Acquire) {
            return Err(Error::Invalid("AI 任务已停止".into()));
        }
        let source = pool.media_source(
            AssetId(parse_id(&asset)?),
            AssetVersionId(parse_id(&version)?),
        )?;
        let preview = source.preview_edge(1024)?.image;
        let bounds = serde_json::from_str(&bounds).map_err(|e| Error::Invalid(e.to_string()))?;
        let crop = portraits::crop_at(&preview, bounds, 256)?;
        let path = directory.path().join(format!("{}.png", items.len()));
        image::save_buffer_with_format(
            &path,
            &crop.rgba,
            crop.width,
            crop.height,
            image::ColorType::Rgba8,
            ImageFormat::Png,
        )?;
        items.push((person, path));
    }
    Ok(References {
        _directory: directory,
        items,
    })
}
fn overlap(a: [f32; 4], b: [f32; 4]) -> f32 {
    let width = ((a[0] + a[2]).min(b[0] + b[2]) - a[0].max(b[0])).max(0.0);
    let height = ((a[1] + a[3]).min(b[1] + b[3]) - a[1].max(b[1])).max(0.0);
    let area = width * height;
    area / (a[2] * a[3] + b[2] * b[3] - area).max(f32::EPSILON)
}
pub(crate) fn store(
    db: &Connection,
    version: &str,
    revision: &str,
    result: &serde_json::Value,
) -> Result<()> {
    let scope = workspace::version_scope(db, version)?;
    let faces = result["faces"]
        .as_array()
        .ok_or_else(|| Error::Invalid("Qwen 人脸回执缺少 faces".into()))?;
    if revision.is_empty() || faces.len() > 32 {
        return Err(Error::Invalid("Qwen 人脸回执超出协议限制".into()));
    }
    let live: bool = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM assets WHERE display_version_id=?1 AND deleted_at IS NULL)",
        [version],
        |r| r.get(0),
    )?;
    if !live {
        return Ok(());
    }
    let old = db
        .prepare("SELECT id,box,person_id,manual FROM detected_faces WHERE version_id=?1")?
        .query_map([version], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, bool>(3)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut valid = Vec::new();
    for face in faces {
        let bounds: [f32; 4] = serde_json::from_value(face["box"].clone())
            .map_err(|e| Error::Invalid(e.to_string()))?;
        if bounds
            .iter()
            .any(|n| !n.is_finite() || !(0.0..=1.0).contains(n))
            || bounds[2] <= 0.0
            || bounds[3] <= 0.0
            || bounds[0] + bounds[2] > 1.001
            || bounds[1] + bounds[3] > 1.001
        {
            return Err(Error::Invalid("Qwen 人脸位置无效".into()));
        }
        valid.push((face, bounds));
    }
    db.execute(
        "DELETE FROM detected_faces WHERE version_id=?1 AND manual=0",
        [version],
    )?;
    let mut used = std::collections::HashSet::new();
    let mut used_people = old
        .iter()
        .filter(|f| f.3)
        .filter_map(|f| f.2.clone())
        .collect::<std::collections::HashSet<_>>();
    let mut ordinal: i64 = db.query_row(
        "SELECT coalesce(max(ordinal),-1)+1 FROM detected_faces WHERE version_id=?1",
        [version],
        |r| r.get(0),
    )?;
    for (face, bounds) in valid {
        let previous = old
            .iter()
            .filter(|f| !used.contains(&f.0))
            .filter_map(|f| {
                let previous: [f32; 4] = serde_json::from_str(&f.1).ok()?;
                let score = overlap(bounds, previous);
                (score >= 0.4).then_some((f, score))
            })
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|v| v.0);
        if let Some(previous) = previous {
            used.insert(previous.0.clone());
            if previous.3 {
                continue;
            }
        }
        let candidate = face["person"].as_str();
        let person = if let Some(candidate) = candidate.filter(|id| !used_people.contains(*id)) {
            db.query_row("SELECT id FROM people WHERE id=?1 AND coalesce((SELECT workspace FROM workspace_people WHERE person_id=people.id),'')=?2", params![candidate,scope], |r| {
                r.get::<_, String>(0)
            })
            .optional()?
        } else {
            None
        };
        let person = if let Some(person) = person {
            person
        } else {
            let person = Uuid::new_v4().to_string();
            db.execute(
                "INSERT INTO people VALUES(?1,'未命名人物',?2,?3)",
                params![person, revision, now_ms()?],
            )?;
            workspace::own_person(db, &person, &scope)?;
            person
        };
        used_people.insert(person.clone());
        let id = previous
            .map(|f| f.0.clone())
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        db.execute("INSERT INTO detected_faces(id,version_id,revision,ordinal,box,vector,person_id) VALUES(?1,?2,?3,?4,?5,x'',?6)",params![id,version,format!("qwen-faces-v1:{revision}"),ordinal,serde_json::to_string(&bounds).map_err(|e|Error::Invalid(e.to_string()))?,person])?;
        ordinal += 1;
    }
    db.execute("INSERT INTO ai_results VALUES(?1,'qwen35-4b/faces',?2,'portrait-compare-v1',?3,?4) ON CONFLICT(version_id,model_id,revision,preprocessing) DO UPDATE SET data=excluded.data,created_at=excluded.created_at",params![version,revision,result.to_string(),now_ms()?])?;
    db.execute("DELETE FROM person_vectors WHERE person_id IN(SELECT person_id FROM detected_faces WHERE version_id=?1 AND length(vector)=0)",[version])?;
    db.execute("DELETE FROM people WHERE name='未命名人物' AND NOT EXISTS(SELECT 1 FROM detected_faces WHERE person_id=people.id)",[])?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn qwen_groups_are_durable_manual_corrections_survive_and_references_use_live_faces() {
        let dir = tempfile::tempdir().unwrap();
        let mut lib = Library::create(dir.path(), "Qwen人物").unwrap();
        for color in [30, 200] {
            let path = dir.path().join(format!("{color}.png"));
            image::RgbImage::from_pixel(48, 48, image::Rgb([color, 80, 140]))
                .save(&path)
                .unwrap();
            lib.import(&path, None).unwrap();
        }
        let assets = lib.page(&Query::default(), 0, 10).unwrap().assets;
        let payload = |person: Option<&str>| serde_json::json!({"faces":[{"box":[0.1,0.1,0.4,0.5],"person":person}]});
        store(
            &lib.db,
            &assets[0].version_id.to_string(),
            "qwen-revision",
            &payload(None),
        )
        .unwrap();
        let first = lib.faces(assets[0].id).unwrap().remove(0);
        let person = first.person.clone().unwrap();
        store(
            &lib.db,
            &assets[1].version_id.to_string(),
            "qwen-revision",
            &payload(Some(&person)),
        )
        .unwrap();
        assert_eq!(lib.people().unwrap().len(), 1);
        assert_eq!(lib.people().unwrap()[0].count, 2);
        lib.rename_person(&person, "人物甲").unwrap();
        lib.assign_face(&first.id, Some(&person)).unwrap();
        store(
            &lib.db,
            &assets[0].version_id.to_string(),
            "qwen-revision",
            &serde_json::json!({"faces":[]}),
        )
        .unwrap();
        assert_eq!(lib.faces(assets[0].id).unwrap()[0].name, "人物甲");
        let refs = references(&lib.readers, "", &AtomicBool::new(false)).unwrap();
        assert_eq!(refs.items.len(), 1);
        assert_eq!(
            image::image_dimensions(&refs.items[0].1).unwrap(),
            (256, 256)
        );
        lib.edit(&[assets[0].id, assets[1].id], &Edit::Trash(true))
            .unwrap();
        assert!(
            references(&lib.readers, "", &AtomicBool::new(false))
                .unwrap()
                .items
                .is_empty()
        );
        let root = lib.root.clone();
        drop(lib);
        let lib = Library::open(&root).unwrap();
        assert_eq!(lib.faces(assets[0].id).unwrap()[0].name, "人物甲");
        assert!(
            lib.db
                .query_row(
                    "SELECT NOT EXISTS(SELECT 1 FROM pragma_foreign_key_check)",
                    [],
                    |r| r.get::<_, bool>(0)
                )
                .unwrap()
        );
    }
    #[test]
    fn invalid_results_and_old_or_trashed_versions_do_not_replace_groups() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("image.png");
        image::RgbImage::from_pixel(16, 16, image::Rgb([50, 80, 120]))
            .save(&input)
            .unwrap();
        let mut lib = Library::create(dir.path(), "人脸边界").unwrap();
        lib.import(&input, None).unwrap();
        let a = lib.page(&Query::default(), 0, 1).unwrap().assets.remove(0);
        let value = serde_json::json!({"faces":[{"box":[0.1,0.1,0.4,0.4],"person":"outside-library"},{"box":[0.6,0.1,0.3,0.4],"person":"outside-library"}]});
        store(&lib.db, &a.version_id.to_string(), "qwen", &value).unwrap();
        let faces = lib.faces(a.id).unwrap();
        assert_eq!(faces.len(), 2);
        assert_ne!(faces[0].person, faces[1].person);
        assert!(
            store(
                &lib.db,
                &a.version_id.to_string(),
                "qwen",
                &serde_json::json!({"faces":[{"box":[0.9,0.1,0.4,0.4]}]})
            )
            .is_err()
        );
        assert_eq!(lib.faces(a.id).unwrap().len(), 2);
        lib.edit(&[a.id], &Edit::Trash(true)).unwrap();
        store(
            &lib.db,
            &a.version_id.to_string(),
            "qwen",
            &serde_json::json!({"faces":[]}),
        )
        .unwrap();
        assert_eq!(lib.faces(a.id).unwrap().len(), 2);
    }
}
