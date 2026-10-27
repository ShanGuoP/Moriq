use super::*;
/// 准备当前用例的独立素材和测试资源，由临时目录句柄负责清理。
fn fixture() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("原片.png");
    image::RgbImage::from_pixel(32, 24, image::Rgb([30, 90, 150]))
        .save(&image)
        .unwrap();
    (dir, image)
}
#[test]
/// 验证创建、导入、编辑、重开及目录迁移后的数据持久性。
fn create_import_edit_reopen_and_move_are_durable() {
    let (dir, source) = fixture();
    let original = fs::read(&source).unwrap();
    let mut lib = Library::create(dir.path(), "摄影库").unwrap();
    assert!(Library::open(lib.root()).is_err());
    let id = lib.id();
    let project = lib.create_project("旅行").unwrap();
    assert!(lib.import(&source, Some(project)).unwrap());
    assert!(!lib.import(&source, Some(project)).unwrap());
    let page = lib.page(&Query::default(), 0, 40).unwrap();
    let asset = page.assets[0].id;
    lib.set_favorite(asset, true).unwrap();
    lib.set_rating(asset, 5).unwrap();
    lib.set_tags(asset, &["雪山".into(), "旅行".into()])
        .unwrap();
    lib.rename("新摄影库").unwrap();
    lib.rename_asset(asset, "旅途.png").unwrap();
    assert_eq!(lib.layout_sizes(&Query::default()).unwrap(), vec![(32, 24)]);
    let old = lib.root().to_path_buf();
    drop(lib);
    let moved = dir.path().join("迁移.moriq");
    fs::rename(old, &moved).unwrap();
    let mut lib = Library::open(&moved).unwrap();
    assert_eq!(lib.id(), id);
    assert_eq!(lib.info().unwrap().name, "新摄影库");
    assert_eq!(lib.info().unwrap().projects[0].count, 1);
    let page = lib
        .page(
            &Query {
                text: "雪山".into(),
                favorites_only: true,
                project: Some(project),
                ..Default::default()
            },
            0,
            40,
        )
        .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.assets[0].name, "旅途.png");
    assert_eq!(page.assets[0].rating, 5);
    assert_eq!(lib.preview(asset).unwrap().width, 32);
    assert_eq!(fs::read(source).unwrap(), original);
}
#[test]
/// 验证导入中断不登记半成品，也不修改原始来源。
fn interruption_never_registers_a_half_import_or_touches_source() {
    let (dir, source) = fixture();
    let original = fs::read(&source).unwrap();
    let mut lib = Library::create(dir.path(), "恢复").unwrap();
    assert!(lib.import_inner(&source, None, true, false, None).is_err());
    let root = lib.root().to_path_buf();
    drop(lib);
    let mut lib = Library::open(&root).unwrap();
    assert_eq!(lib.info().unwrap().count, 0);
    let phases: u64 = lib
        .db
        .query_row(
            "SELECT count(*) FROM operation_journal WHERE phase='interrupted'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(phases, 1);
    assert!(lib.import(&source, None).unwrap());
    assert_eq!(lib.info().unwrap().count, 1);
    assert_eq!(fs::read(source).unwrap(), original);
}
#[test]
/// 验证路径穿越、无效媒体与清单身份不一致均被拒绝。
fn rejects_traversal_bad_media_and_manifest_mismatch() {
    let (dir, source) = fixture();
    let mut lib = Library::create(dir.path(), "安全").unwrap();
    for path in [
        "../outside",
        "assets/../../x",
        "C:/outside",
        "assets\\x",
        "/outside",
    ] {
        assert!(safe_path(lib.root(), path).is_err());
    }
    let invalid = dir.path().join("假的.jpg");
    fs::write(&invalid, b"not an image").unwrap();
    assert!(lib.import(&invalid, None).is_err());
    assert_eq!(lib.info().unwrap().count, 0);
    assert_eq!(
        lib.page(
            &Query {
                text: "%' OR 1=1 --".into(),
                ..Query::default()
            },
            0,
            100
        )
        .unwrap()
        .total,
        0
    );
    assert!(lib.import(&source, None).unwrap());
    let root = lib.root().to_path_buf();
    drop(lib);
    let path = root.join("library.toml");
    let mut manifest: Manifest = toml::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    manifest.library_id = LibraryId::default();
    fs::write(path, toml::to_string(&manifest).unwrap()).unwrap();
    assert!(Library::open(&root).is_err());
}

#[test]
/// 验证编辑和导入会使相关筛选快照失效。
fn filtered_snapshot_is_invalidated_by_edits_and_imports() {
    let (dir, source) = fixture();
    let mut lib = Library::create(dir.path(), "查询").unwrap();
    lib.import(&source, None).unwrap();
    let id = lib.page(&Query::default(), 0, 1).unwrap().assets[0].id;
    let favorites = Query {
        favorites_only: true,
        ..Query::default()
    };
    assert_eq!(lib.page(&favorites, 0, 40).unwrap().total, 0);
    lib.set_favorite(id, true).unwrap();
    assert_eq!(lib.page(&favorites, 0, 40).unwrap().total, 1);
    lib.set_favorite(id, false).unwrap();
    assert_eq!(lib.page(&favorites, 0, 40).unwrap().total, 0);
    assert!(
        lib.page(&Query::default(), 100000, 40)
            .unwrap()
            .assets
            .is_empty()
    );
    let query = Query {
        text: "新标签".into(),
        ..Query::default()
    };
    assert_eq!(lib.page(&query, 0, 40).unwrap().total, 0);
    lib.set_tags(id, &["新标签".into()]).unwrap();
    assert_eq!(lib.page(&query, 0, 40).unwrap().total, 1);
}

#[test]
/// 验证外键约束阻止素材引用不属于自己的展示版本。
fn foreign_key_contract_prevents_wrong_cover_version() {
    let (dir, source) = fixture();
    let mut lib = Library::create(dir.path(), "外键").unwrap();
    lib.import(&source, None).unwrap();
    let asset = lib.page(&Query::default(), 0, 1).unwrap().assets[0].id;
    assert!(
        lib.db
            .execute(
                "UPDATE assets SET display_version_id=?1 WHERE id=?2",
                params![AssetVersionId::default().to_string(), asset.to_string()]
            )
            .is_err()
    );
    assert_eq!(lib.info().unwrap().count, 1);
}

#[test]
#[ignore = "synthetic metadata benchmark; no media or frame-time acceptance"]
/// 测量合成数据库的分页、搜索与重开耗时，不代表真实媒体吞吐或帧率验收。
fn synthetic_metadata_benchmark() {
    let dir = tempfile::tempdir().unwrap();
    let mut lib = Library::create(dir.path(), "benchmark").unwrap();
    for (previous, total) in [(0, 100_000), (100_000, 500_000)] {
        let started = std::time::Instant::now();
        let tx = lib.db.transaction().unwrap();
        tx.execute_batch(&format!("WITH RECURSIVE n(i) AS(SELECT {} UNION ALL SELECT i+1 FROM n WHERE i<{}) INSERT INTO \
                assets(id,display_version_id,name,imported_at) SELECT \
                printf('00000000-0000-4000-8000-%012x',i),printf('10000000-0000-4000-8000-%012x',i),printf('样本_%06d.png',i),i FROM n; INSERT INTO asset_versions(id,asset_id,origin,path,sha256,format,width,height,bytes) SELECT display_version_id,id,'original','assets/00/'||display_version_id||'.png','{}','png',1,1,3 FROM assets WHERE imported_at>{};",
previous+1,
total,
"a".repeat(64),
previous)).unwrap();

        tx.commit().unwrap();
        lib.snapshot = None;
        println!(
            "synthetic_rows={total} seed_ms={:.2}",
            started.elapsed().as_secs_f64() * 1000.0
        );
        let mut samples = Vec::new();
        for i in 0..60 {
            let offset = if i % 3 == 0 { 0 } else { total - (i * 17 + 40) };
            let started = std::time::Instant::now();
            let page = lib.page(&Query::default(), offset, 40).unwrap();
            assert_eq!(page.total, total as u64);
            assert_eq!(page.assets.len(), 40);
            samples.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(f64::total_cmp);
        println!(
            "random_page_p50_ms={:.3} random_page_p95_ms={:.3}",
            samples[30], samples[57]
        );
        let query = Query {
            text: "样本_".into(),
            ..Query::default()
        };
        let started = std::time::Instant::now();
        assert_eq!(lib.page(&query, 0, 40).unwrap().total, total as u64);
        println!(
            "text_snapshot_build_ms={:.3}",
            started.elapsed().as_secs_f64() * 1000.0
        );
        let started = std::time::Instant::now();
        assert_eq!(lib.page(&query, total - 40, 40).unwrap().assets.len(), 40);
        println!(
            "text_snapshot_deep_page_ms={:.3}",
            started.elapsed().as_secs_f64() * 1000.0
        );
        let root = lib.root().to_path_buf();
        drop(lib);
        let started = std::time::Instant::now();
        lib = Library::open(&root).unwrap();
        assert_eq!(lib.page(&Query::default(), 0, 40).unwrap().assets.len(), 40);
        println!(
            "reopen_and_first_projection_ms={:.3}",
            started.elapsed().as_secs_f64() * 1000.0
        );
    }
}
