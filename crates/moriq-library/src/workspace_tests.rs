use super::*;

#[test]
fn real_schema_24_shared_assets_upgrade_before_workspace_tables_exist() {
    let (_dir, mut lib, a, b, first, second) = fixture();
    lib.set_workspace(None);
    lib.db
        .execute(
            "DELETE FROM project_assets WHERE asset_id=?1",
            [second.id.to_string()],
        )
        .unwrap();
    lib.db
        .execute(
            "INSERT INTO project_assets VALUES(?1,?2)",
            params![b.to_string(), first.id.to_string()],
        )
        .unwrap();
    let folder = lib.create_virtual_folder("旧共享分组", false).unwrap();
    lib.db
        .execute(
            "INSERT INTO virtual_folder_assets VALUES(?1,?2)",
            params![folder, first.id.to_string()],
        )
        .unwrap();
    let root = lib.root.clone();
    drop(lib);
    let db = Connection::open(root.join("catalog.db")).unwrap();
    let sql = include_str!("migration_24.sql");
    db.execute_batch(&sql[sql.find("DROP VIEW").unwrap()..])
        .unwrap();
    db.execute_batch("DROP VIEW workspace_vocabulary; DROP TABLE project_ai_profiles; DROP TABLE workspace_people; DROP TABLE workspace_folders; DROP TABLE workspace_jobs; DROP TABLE workspace_collections; DROP TABLE project_copies; PRAGMA user_version=24;").unwrap();
    drop(db);
    let mut lib = Library::open(&root).unwrap();
    lib.set_workspace(Some(a));
    let first = lib.page(&Query::default(), 0, 10).unwrap().assets.remove(0);
    assert_eq!(lib.virtual_folders().unwrap()[0].count, 1);
    lib.set_workspace(Some(b));
    let copy = lib.page(&Query::default(), 0, 10).unwrap().assets.remove(0);
    assert_ne!(first.id, copy.id);
    assert_eq!(lib.virtual_folders().unwrap()[0].count, 1);
    assert_eq!(
        fs::read(lib.asset_path(first.id).unwrap()).unwrap(),
        fs::read(lib.asset_path(copy.id).unwrap()).unwrap()
    );
}

fn fixture() -> (
    tempfile::TempDir,
    Library,
    ProjectId,
    ProjectId,
    Asset,
    Asset,
) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("photo.png");
    image::RgbImage::from_pixel(8, 6, image::Rgb([20, 80, 140]))
        .save(&source)
        .unwrap();
    let mut lib = Library::create(dir.path(), "项目空间").unwrap();
    let a = lib.create_project("客户甲").unwrap();
    let b = lib.create_project("客户乙").unwrap();
    lib.import(&source, Some(a)).unwrap();
    lib.import(&source, Some(b)).unwrap();
    lib.set_workspace(Some(a));
    let first = lib.page(&Query::default(), 0, 10).unwrap().assets.remove(0);
    lib.set_workspace(Some(b));
    let second = lib.page(&Query::default(), 0, 10).unwrap().assets.remove(0);
    (dir, lib, a, b, first, second)
}

#[test]
fn project_profiles_labels_search_tags_and_statistics_stay_independent_after_reopen() {
    let (_dir, mut lib, a, b, first, second) = fixture();
    for (project, asset) in [(a, &first), (b, &second)] {
        lib.set_workspace(Some(project));
        lib.edit_ai_content(
            asset.id,
            asset.version_id,
            AiEdit::Classification("photography".into(), vec!["portrait".into()]),
        )
        .unwrap();
        lib.set_tags(asset.id, &["共享字面标签".into()]).unwrap();
    }
    lib.set_workspace(Some(a));
    let mut profile = lib.ai_profile().unwrap();
    profile
        .categories
        .iter_mut()
        .find(|c| c.id == "portrait")
        .unwrap()
        .label = "客户甲肖像".into();
    lib.save_ai_profile(profile.clone()).unwrap();
    assert!(lib.save_ai_profile(profile).is_err());
    lib.rename_tag("共享字面标签", "甲方独立标签").unwrap();
    assert_eq!(
        lib.page(
            &Query {
                text: "客户甲肖像".into(),
                ..Default::default()
            },
            0,
            10
        )
        .unwrap()
        .total,
        1
    );
    assert_eq!(
        lib.ai_groups()
            .unwrap()
            .iter()
            .find(|g| g.id == "portrait")
            .unwrap()
            .count,
        1
    );
    assert_eq!(
        lib.organizer().unwrap().tags,
        vec![("甲方独立标签".into(), 1)]
    );
    assert!(
        lib.page(
            &Query {
                project: Some(b),
                ..Default::default()
            },
            0,
            10
        )
        .is_err()
    );
    lib.edit(&[first.id], &Edit::Trash(true)).unwrap();
    assert_eq!(lib.trash_snapshot().unwrap(), vec![first.id]);
    lib.set_workspace(Some(b));
    assert_eq!(lib.page(&Query::default(), 0, 10).unwrap().total, 1);
    assert!(lib.trash_snapshot().unwrap().is_empty());
    assert_eq!(
        lib.ai_profile()
            .unwrap()
            .categories
            .iter()
            .find(|c| c.id == "portrait")
            .unwrap()
            .label,
        "人像"
    );
    assert_eq!(
        lib.details(second.id).unwrap().manual_tags,
        vec!["共享字面标签"]
    );
    assert_eq!(
        lib.page(
            &Query {
                text: "客户甲肖像".into(),
                ..Default::default()
            },
            0,
            10
        )
        .unwrap()
        .total,
        0
    );
    let root = lib.root.clone();
    drop(lib);
    let mut lib = Library::open(&root).unwrap();
    lib.set_workspace(Some(a));
    assert_eq!(
        lib.ai_profile()
            .unwrap()
            .categories
            .iter()
            .find(|c| c.id == "portrait")
            .unwrap()
            .label,
        "客户甲肖像"
    );
    assert_eq!(lib.organizer().unwrap().trash_count, 1);
    lib.set_workspace(None);
    assert_eq!(lib.page(&Query::default(), 0, 10).unwrap().total, 1);
}

#[test]
fn jobs_freeze_project_profile_and_other_project_edits_do_not_cancel_or_retarget_work() {
    let (_dir, mut lib, a, b, first, _second) = fixture();
    lib.set_workspace(Some(a));
    let job_a = lib
        .enqueue_media_job(JobKind::QwenClassification, 100)
        .unwrap();
    let profile_a = lib.ai_profile().unwrap();
    lib.set_workspace(Some(b));
    let job_b = lib
        .enqueue_media_job(JobKind::QwenClassification, 0)
        .unwrap();
    assert_ne!(job_a, job_b);
    assert_eq!(lib.background_jobs().unwrap().len(), 1);
    assert!(lib.control_job(&job_a, JobCommand::Cancel).is_err());
    let request = match lib
        .claim_job_step()
        .unwrap()
        .unwrap()
        .execute(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
            false,
        ))) {
        jobs::Execution::QwenClassification(request) => request,
        _ => panic!("expected classification"),
    };
    assert_eq!(request.profile().unwrap(), &profile_a);
    let mut profile_b = lib.ai_profile().unwrap();
    profile_b.max_themes = 4;
    assert_eq!(lib.save_ai_profile(profile_b).unwrap().1, vec![job_b]);
    let outcome = request.complete_qwen(
        "qwen_classification",
        Ok(analysis::InferenceResult {
            model: ModelId::Qwen35_4b,
            revision: "test".into(),
            data: serde_json::json!({"medium":"photography","themes":["portrait"]}),
        }),
    );
    // Commit while B is selected. The receipt still belongs to A.
    lib.finish_job_step(outcome).unwrap();
    assert_eq!(lib.details(first.id).unwrap().themes, vec!["portrait"]);
    lib.set_workspace(Some(a));
    assert_eq!(lib.background_jobs().unwrap()[0].completed, 1);
}

#[test]
fn folders_collections_people_and_copy_options_have_project_ownership() {
    let (_dir, mut lib, a, b, first, second) = fixture();
    lib.set_workspace(Some(a));
    let folder = lib.create_virtual_folder("交付", true).unwrap();
    lib.virtual_folder_members(&folder, &[first.id], true)
        .unwrap();
    lib.save_collection(
        "精选",
        &Query {
            favorites_only: true,
            ..Default::default()
        },
    )
    .unwrap();
    qwen_faces::store(
        &lib.db,
        &first.version_id.to_string(),
        "test",
        &serde_json::json!({"faces":[{"box":[0.1,0.1,0.3,0.3]}]}),
    )
    .unwrap();
    let person = lib.people().unwrap().remove(0).id;
    lib.rename_person(&person, "甲方模特").unwrap();
    lib.set_workspace(Some(b));
    assert!(lib.virtual_folders().unwrap().is_empty());
    assert!(lib.people().unwrap().is_empty());
    assert!(lib.rename_person(&person, "串项目").is_err());
    assert!(
        lib.virtual_folder_members(&folder, &[second.id], true)
            .is_err()
    );
    lib.save_collection(
        "精选",
        &Query {
            min_rating: 4,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(lib.smart_collections().unwrap()[0].query.min_rating, 4);
    qwen_faces::store(
        &lib.db,
        &second.version_id.to_string(),
        "test",
        &serde_json::json!({"faces":[{"box":[0.1,0.1,0.3,0.3],"person":person}]}),
    )
    .unwrap();
    assert_ne!(lib.people().unwrap()[0].id, person);
    let refs = qwen_faces::references(
        &lib.readers,
        &b.to_string(),
        &std::sync::atomic::AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(refs.items.len(), 1);
    assert_ne!(refs.items[0].0, person);
    lib.set_workspace(Some(a));
    let source = lib.asset_path(first.id).unwrap();
    let bytes = fs::read(&source).unwrap();
    lib.edit(&[first.id], &Edit::Notes("不要带走".into()))
        .unwrap();
    lib.set_tags(first.id, &["保密标签".into()]).unwrap();
    lib.copy_to_project(&[first.id], b, false).unwrap();
    lib.set_workspace(Some(b));
    let copied = lib
        .page(&Query::default(), 0, 10)
        .unwrap()
        .assets
        .into_iter()
        .find(|v| v.id != second.id)
        .unwrap();
    assert!(copied.tags.is_empty());
    assert!(lib.details(copied.id).unwrap().notes.is_empty());
    assert_eq!(fs::read(lib.asset_path(copied.id).unwrap()).unwrap(), bytes);
    assert_eq!(lib.people().unwrap().len(), 1);
    assert!(lib.virtual_folders().unwrap().is_empty());
    lib.set_workspace(Some(a));
    assert_eq!(lib.smart_collections().unwrap()[0].query.min_rating, 0);
    assert_eq!(fs::read(&source).unwrap(), bytes);
}

#[test]
fn templates_and_parent_projects_are_independent_and_copied_taxonomy_maps_by_label() {
    let (_dir, mut lib, a, b, first, _) = fixture();
    let child = lib
        .create_project_configured("产品", Some(a), ProjectTemplate::Product)
        .unwrap();
    lib.set_workspace(Some(child));
    let product = lib.ai_profile().unwrap();
    assert!(product.categories.iter().any(|c| c.id == "product_fashion"));
    let copied = lib
        .create_project_configured("复制配置", None, ProjectTemplate::Existing(child))
        .unwrap();
    let mut changed = product;
    changed.max_themes = 5;
    lib.save_ai_profile(changed).unwrap();
    lib.set_workspace(Some(copied));
    assert_eq!(lib.ai_profile().unwrap().max_themes, 3);
    lib.set_workspace(Some(a));
    assert_eq!(lib.page(&Query::default(), 0, 10).unwrap().total, 1);
    lib.edit_ai_content(
        first.id,
        first.version_id,
        AiEdit::Classification("photography".into(), vec!["portrait".into()]),
    )
    .unwrap();
    lib.set_workspace(Some(b));
    let mut profile = lib.ai_profile().unwrap();
    profile
        .categories
        .iter_mut()
        .find(|c| c.id == "portrait")
        .unwrap()
        .label = "不同含义".into();
    profile.categories.push(AiCategory {
        id: "client_portrait".into(),
        label: "人像".into(),
        definition: "客户肖像".into(),
        kind: AiCategoryKind::Theme,
        enabled: true,
    });
    lib.save_ai_profile(profile).unwrap();
    lib.set_workspace(Some(a));
    lib.copy_to_project(&[first.id], b, true).unwrap();
    lib.set_workspace(Some(b));
    let assets = lib.page(&Query::default(), 0, 10).unwrap().assets;
    assert!(
        assets
            .iter()
            .any(|asset| lib.details(asset.id).unwrap().themes == vec!["client_portrait"])
    );
}

#[test]
fn schema_25_upgrade_splits_shared_people_folders_and_collections_with_atomic_rollback() {
    let (_dir, mut lib, a, b, first, second) = fixture();
    lib.set_workspace(None);
    lib.db
        .execute("INSERT INTO people VALUES('shared','原人物','test',0)", [])
        .unwrap();
    for (n, asset) in [&first, &second].iter().enumerate() {
        lib.db.execute("INSERT INTO detected_faces(id,version_id,revision,ordinal,box,vector,person_id) VALUES(?1,?2,'test',0,'[0.1,0.1,0.3,0.3]',x'','shared')",params![n.to_string(),asset.version_id.to_string()]).unwrap();
    }
    let folder = lib.create_virtual_folder("旧分组", true).unwrap();
    lib.db
        .execute(
            "INSERT INTO virtual_folder_assets VALUES(?1,?2)",
            params![folder, first.id.to_string()],
        )
        .unwrap();
    lib.db
        .execute(
            "INSERT INTO virtual_folder_assets VALUES(?1,?2)",
            params![folder, second.id.to_string()],
        )
        .unwrap();
    lib.save_collection(
        "旧集合",
        &Query {
            folder: Some(folder.clone()),
            ..Default::default()
        },
    )
    .unwrap();
    let root = lib.root.clone();
    drop(lib);
    let db = Connection::open(root.join("catalog.db")).unwrap();
    let sql = include_str!("migration_24.sql");
    db.execute_batch(&sql[sql.find("DROP VIEW").unwrap()..])
        .unwrap();
    db.execute_batch("DROP VIEW workspace_vocabulary; DROP TABLE project_ai_profiles; DROP TABLE workspace_people; DROP TABLE workspace_folders; DROP TABLE workspace_jobs; DROP TABLE workspace_collections; PRAGMA user_version=25; CREATE TRIGGER fail_people BEFORE INSERT ON people BEGIN SELECT RAISE(ABORT,'injected workspace migration failure'); END;").unwrap();
    drop(db);
    assert!(Library::open(&root).is_err());
    let db = Connection::open(root.join("catalog.db")).unwrap();
    assert_eq!(
        db.pragma_query_value::<u32, _>(None, "user_version", |r| r.get(0))
            .unwrap(),
        25
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM people", [], |r| r.get::<_, u64>(0))
            .unwrap(),
        1
    );
    db.execute_batch("DROP TRIGGER fail_people;").unwrap();
    drop(db);
    let mut lib = Library::open(&root).unwrap();
    lib.set_workspace(Some(a));
    let person_a = lib.people().unwrap().remove(0).id;
    let folder_a = lib.virtual_folders().unwrap().remove(0);
    assert_eq!(folder_a.count, 1);
    lib.rename_person(&person_a, "只改甲方").unwrap();
    assert_eq!(
        lib.smart_collections().unwrap()[0].query.folder,
        Some(folder_a.id)
    );
    lib.set_workspace(Some(b));
    assert_eq!(lib.people().unwrap()[0].name, "原人物");
    assert_ne!(lib.people().unwrap()[0].id, person_a);
    assert_eq!(lib.virtual_folders().unwrap()[0].count, 1);
    assert_eq!(lib.smart_collections().unwrap().len(), 1);
    assert!(
        lib.db
            .prepare("PRAGMA foreign_key_check")
            .unwrap()
            .query([])
            .unwrap()
            .next()
            .unwrap()
            .is_none()
    );
    assert!(fs::read_dir(root.join("backups")).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("schema-25-")
    }));
}
