use super::*;

fn database() -> Connection {
    let db = Connection::open_in_memory().unwrap();
    db.execute_batch("CREATE TABLE assets(id TEXT PRIMARY KEY,name TEXT,display_version_id TEXT,deleted_at INTEGER);
        CREATE TABLE asset_workspaces(asset_id TEXT PRIMARY KEY,workspace TEXT);
        CREATE TABLE image_fingerprints(version_id TEXT,hash INTEGER,red INTEGER,green INTEGER,blue INTEGER,aspect REAL,algorithm INTEGER);
        CREATE TABLE ai_embeddings(version_id TEXT,model_id TEXT,revision TEXT,vector BLOB);").unwrap();
    db
}
fn add(db: &Connection, project: Option<ProjectId>, score: f32) -> (AssetId, AssetVersionId) {
    let id = AssetId::default();
    let version = AssetVersionId::default();
    db.execute(
        "INSERT INTO assets VALUES(?1,'image',?2,NULL)",
        params![id.to_string(), version.to_string()],
    )
    .unwrap();
    db.execute(
        "INSERT INTO asset_workspaces VALUES(?1,?2)",
        params![
            id.to_string(),
            project.map(|id| id.to_string()).unwrap_or_default()
        ],
    )
    .unwrap();
    db.execute(
        "INSERT INTO image_fingerprints VALUES(?1,0,20,80,140,1.0,1)",
        [version.to_string()],
    )
    .unwrap();
    let vector: Vec<u8> = [score, (1.0 - score * score).sqrt()]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect();
    db.execute(
        "INSERT INTO ai_embeddings VALUES(?1,'chinese-clip','revision',?2)",
        params![version.to_string(), vector],
    )
    .unwrap();
    (id, version)
}

#[test]
fn project_candidates_and_input_assets_obey_the_same_scope() {
    let db = database();
    let a = ProjectId::default();
    let b = ProjectId::default();
    let (source, version) = add(&db, Some(a), 1.0);
    let (same, _) = add(&db, Some(a), 0.5);
    let (other, other_version) = add(&db, Some(b), 0.9);
    let (unassigned, _) = add(&db, None, 0.8);
    let catalog = Catalog::new(&db, QueryScope::Project(a));
    assert_eq!(
        catalog
            .similar(source)
            .unwrap()
            .iter()
            .map(|a| a.id)
            .collect::<Vec<_>>(),
        vec![same]
    );
    assert_eq!(
        catalog
            .semantic("revision", &[1.0, 0.0], Some(source))
            .unwrap(),
        vec![same]
    );
    assert_eq!(
        catalog
            .recommendations("revision", source, version)
            .unwrap()[0]
            .0,
        same
    );
    assert!(catalog.similar(other).is_err());
    assert!(
        catalog
            .recommendations("revision", other, other_version)
            .is_err()
    );
    let unowned = Catalog::new(&db, QueryScope::Unassigned);
    assert_eq!(
        unowned.semantic("revision", &[1.0, 0.0], None).unwrap(),
        vec![unassigned]
    );
    assert!(unowned.similar(source).is_err());
    let all = Catalog::new(&db, QueryScope::Library);
    assert_eq!(all.similar(source).unwrap().len(), 3);
    assert_eq!(
        all.semantic("revision", &[1.0, 0.0], None).unwrap().len(),
        4
    );
    db.execute(
        "UPDATE assets SET deleted_at=1 WHERE id=?1",
        [same.to_string()],
    )
    .unwrap();
    assert!(catalog.similar(source).unwrap().is_empty());
    assert!(
        catalog
            .recommendations("revision", source, version)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn scope_filters_before_top_n_and_old_versions_do_not_supply_recommendations() {
    let db = database();
    let a = ProjectId::default();
    let b = ProjectId::default();
    let (source, version) = add(&db, Some(a), 1.0);
    let (same, _) = add(&db, Some(a), 0.1);
    for _ in 0..220 {
        add(&db, Some(b), 0.9);
    }
    let catalog = Catalog::new(&db, QueryScope::Project(a));
    assert_eq!(
        catalog
            .semantic("revision", &[1.0, 0.0], Some(source))
            .unwrap(),
        vec![same]
    );
    assert_eq!(
        catalog
            .recommendations("revision", source, version)
            .unwrap()[0]
            .0,
        same
    );
    assert!(
        catalog
            .recommendations("other-revision", source, version)
            .unwrap()
            .is_empty()
    );
    assert!(
        catalog
            .recommendations("revision", source, AssetVersionId::default())
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        Catalog::new(&db, QueryScope::Library)
            .semantic("revision", &[1.0, 0.0], None)
            .unwrap()
            .len(),
        200
    );
}
