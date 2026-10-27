use super::*;

/// 准备当前用例的独立素材和测试资源，由临时目录句柄负责清理。
fn fixture() -> (tempfile::TempDir, Library, PathBuf, ProjectId, ProjectId) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source.png");
    image::RgbImage::from_pixel(8, 6, image::Rgb([20, 80, 140]))
        .save(&source)
        .unwrap();
    let lib = Library::create(dir.path(), "隔离测试").unwrap();
    let a = lib.create_project("项目一").unwrap();
    let b = lib.create_project("项目二").unwrap();
    (dir, lib, source, a, b)
}
/// 从独立测试资源库读取指定样本素材。
fn asset(lib: &mut Library, project: ProjectId) -> Asset {
    lib.page(
        &Query {
            project: Some(project),
            ..Default::default()
        },
        0,
        10,
    )
    .unwrap()
    .assets
    .remove(0)
}

#[test]
/// 验证相同内容在不同项目导入后身份、元数据、回收站与文件独立。
fn same_content_imports_have_independent_identity_metadata_trash_and_bytes() {
    for referenced in [false, true] {
        let (_dir, mut lib, source, a, b) = fixture();
        let original = fs::read(&source).unwrap();
        let file = ImportFile {
            path: source.clone(),
            relative: "source.png".into(),
            project_name: None,
        };
        assert!(lib.import_file(&file, Some(a), referenced).unwrap());
        assert!(lib.import_file(&file, Some(b), referenced).unwrap());
        assert!(!lib.import_file(&file, Some(b), referenced).unwrap());
        let first = asset(&mut lib, a);
        let second = asset(&mut lib, b);
        assert_ne!(first.id, second.id);
        assert_ne!(first.version_id, second.version_id);
        lib.rename_asset(first.id, "修改名称").unwrap();
        lib.edit(&[first.id], &Edit::Favorite(true)).unwrap();
        lib.edit(&[first.id], &Edit::Rating(5)).unwrap();
        lib.edit(&[first.id], &Edit::Notes("只属于项目一".into()))
            .unwrap();
        lib.set_tags(first.id, &["独立标签".into()]).unwrap();
        lib.edit_ai_content(
            first.id,
            first.version_id,
            AiEdit::Description(Some("独立描述".into())),
        )
        .unwrap();
        lib.edit(&[first.id], &Edit::Trash(true)).unwrap();
        let second = asset(&mut lib, b);
        assert!(!second.favorite);
        assert_eq!(second.rating, 0);
        assert_eq!(second.name, "source.png");
        assert!(second.tags.is_empty());
        assert!(lib.details(second.id).unwrap().notes.is_empty());
        assert!(lib.details(second.id).unwrap().ai_caption.is_empty());
        let plan = lib.project_deletion_plan(a).unwrap();
        assert_eq!(lib.delete_project_contents(&plan).unwrap().deleted, 1);
        assert_eq!(
            fs::read(lib.asset_path(second.id).unwrap()).unwrap(),
            original
        );
        assert_eq!(fs::read(&source).unwrap(), original);
        let root = lib.root.clone();
        drop(lib);
        let mut lib = Library::open(&root).unwrap();
        assert_eq!(asset(&mut lib, b).id, second.id);
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
}

#[test]
/// 验证复制到项目保留有效 AI 与人工覆盖，重复操作保持幂等。
fn adding_to_another_project_copies_effective_ai_and_overrides_and_is_idempotent() {
    let (_dir, mut lib, source, a, b) = fixture();
    lib.import(&source, Some(a)).unwrap();
    let first = asset(&mut lib, a);
    lib.set_tags(first.id, &["原标签".into()]).unwrap();
    lib.edit(&[first.id], &Edit::Notes("已有备注".into()))
        .unwrap();
    lib.db.execute("INSERT INTO ai_analysis_runs(version_id,task,revision,preprocessing,data,created_at) VALUES(?1,'qwen_description','r1','test','{\"description\":\"分析描述\",\"tags\":[\"AI标签\"]}',1)", [first.version_id.to_string()]).unwrap();
    let run = lib.db.last_insert_rowid();
    lib.db
        .execute(
            "INSERT INTO ai_effective_results VALUES(?1,'qwen_description',?2)",
            params![first.version_id.to_string(), run],
        )
        .unwrap();
    lib.edit_ai_content(
        first.id,
        first.version_id,
        AiEdit::Description(Some("手动描述".into())),
    )
    .unwrap();
    lib.edit(&[first.id], &Edit::Project(b, true)).unwrap();
    lib.edit(&[first.id], &Edit::Project(b, true)).unwrap();
    assert_eq!(
        lib.page(
            &Query {
                project: Some(b),
                ..Default::default()
            },
            0,
            10
        )
        .unwrap()
        .total,
        1
    );
    let second = asset(&mut lib, b);
    assert_ne!(first.id, second.id);
    let details = lib.details(second.id).unwrap();
    assert_eq!(details.notes, "已有备注");
    assert_eq!(details.ai_caption, "手动描述");
    assert!(second.tags.contains(&"AI标签".into()));
    lib.edit_ai_content(first.id, first.version_id, AiEdit::Description(None))
        .unwrap();
    assert_eq!(lib.details(second.id).unwrap().ai_caption, "手动描述");
    let path = lib.asset_path(second.id).unwrap();
    fs::write(&path, b"independent bytes").unwrap();
    assert_eq!(
        fs::read(lib.asset_path(first.id).unwrap()).unwrap(),
        fs::read(source).unwrap()
    );
}

#[test]
/// 验证版本 24 共享素材升级持久拆分并备份，失败时回滚。
fn schema_24_shared_assets_split_durably_with_backup_and_failure_rolls_back() {
    let (_dir, mut lib, source, a, b) = fixture();
    lib.import(&source, Some(a)).unwrap();
    let first = asset(&mut lib, a);
    lib.set_tags(first.id, &["旧标签".into()]).unwrap();
    lib.db
        .execute(
            "INSERT INTO project_assets VALUES(?1,?2)",
            params![b.to_string(), first.id.to_string()],
        )
        .unwrap();
    lib.db
        .execute_batch("DROP TABLE project_copies; PRAGMA user_version=24;")
        .unwrap();
    lib.db.execute_batch("CREATE TRIGGER fail_copy BEFORE INSERT ON assets BEGIN SELECT RAISE(ABORT,'injected copy failure'); END;").unwrap();
    let root = lib.root.clone();
    drop(lib);
    assert!(Library::open(&root).is_err());
    let db = Connection::open(root.join("catalog.db")).unwrap();
    assert_eq!(
        db.pragma_query_value::<u32, _>(None, "user_version", |r| r.get(0))
            .unwrap(),
        24
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM assets", [], |r| r.get::<_, u64>(0))
            .unwrap(),
        1
    );
    db.execute_batch("DROP TRIGGER fail_copy;").unwrap();
    drop(db);
    let mut lib = Library::open(&root).unwrap();
    let first = asset(&mut lib, a);
    let second = asset(&mut lib, b);
    assert_ne!(first.id, second.id);
    assert_eq!(first.tags, vec!["旧标签"]);
    assert_eq!(second.tags, vec!["旧标签"]);
    lib.edit(&[first.id], &Edit::Trash(true)).unwrap();
    assert_eq!(asset(&mut lib, b).id, second.id);
    assert!(fs::read_dir(root.join("backups")).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("schema-24-")
    }));
    let count = lib.info().unwrap().count;
    drop(lib);
    let lib = Library::open(&root).unwrap();
    assert_eq!(lib.info().unwrap().count, count);
    assert_eq!(fs::read(source).unwrap().len(), first.bytes as usize);
}

#[test]
/// 验证项目复制失败清理部分文件，并回滚整批修改。
fn project_copy_failure_removes_partial_files_and_rolls_back_batch() {
    let (_dir, mut lib, source, a, b) = fixture();
    lib.import(&source, Some(a)).unwrap();
    let first = asset(&mut lib, a);
    lib.db.execute_batch("CREATE TRIGGER fail_membership BEFORE INSERT ON project_assets BEGIN SELECT RAISE(ABORT,'injected copy failure'); END;").unwrap();
    assert!(lib.edit(&[first.id], &Edit::Project(b, true)).is_err());
    assert_eq!(lib.info().unwrap().count, 1);
    assert_eq!(
        lib.page(
            &Query {
                project: Some(b),
                ..Default::default()
            },
            0,
            10
        )
        .unwrap()
        .total,
        0
    );
    let path = lib.root.join(format!("assets/project-copies/{b}"));
    assert_eq!(fs::read_dir(path).unwrap().count(), 0);
    assert!(lib.asset_path(first.id).unwrap().is_file());
    assert_eq!(
        lib.db
            .query_row("SELECT count(*) FROM project_copies", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        0
    );
}

#[test]
/// 验证排队执行的项目复制提交后仍保持状态独立。
fn queued_batch_project_copy_remains_independent_after_commit() {
    let (_dir, mut lib, source, a, b) = fixture();
    lib.import(&source, Some(a)).unwrap();
    let first = asset(&mut lib, a);
    lib.enqueue_batch(
        BatchScope::Assets(vec![first.id]),
        BatchPatch {
            project: Some(b),
            favorite: Some(true),
            ..Default::default()
        },
    )
    .unwrap();
    let work = lib.claim_job_step().unwrap().unwrap();
    lib.finish_job_step(work.run()).unwrap();
    let second = asset(&mut lib, b);
    assert_ne!(first.id, second.id);
    assert!(second.favorite);
    assert!(lib.asset_path(second.id).unwrap().is_file());
    lib.edit(&[first.id], &Edit::Trash(true)).unwrap();
    assert_eq!(asset(&mut lib, b).id, second.id);
    assert_eq!(lib.details(first.id).unwrap().project_copies, vec![b]);
}

#[test]
/// 验证共享回收站素材升级后不会重新出现在有效图库。
fn upgrading_shared_trashed_assets_never_adds_them_back_to_the_live_gallery() {
    let (_dir, mut lib, source, a, b) = fixture();
    lib.import(&source, Some(a)).unwrap();
    let first = asset(&mut lib, a);
    lib.db
        .execute(
            "INSERT INTO project_assets VALUES(?1,?2)",
            params![b.to_string(), first.id.to_string()],
        )
        .unwrap();
    lib.edit(&[first.id], &Edit::Trash(true)).unwrap();
    lib.db.execute_batch("PRAGMA user_version=24;").unwrap();
    let root = lib.root.clone();
    drop(lib);
    let mut lib = Library::open(&root).unwrap();
    assert_eq!(lib.page(&Query::default(), 0, 10).unwrap().total, 0);
    assert_eq!(
        lib.page(
            &Query {
                trash: true,
                ..Default::default()
            },
            0,
            10
        )
        .unwrap()
        .total,
        2
    );
}
