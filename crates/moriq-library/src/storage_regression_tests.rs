use super::*;

fn fixture() -> (tempfile::TempDir, Library, PathBuf, Asset) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("original.png");
    image::RgbImage::from_pixel(32, 24, image::Rgb([20, 40, 60]))
        .save(&source)
        .unwrap();
    let mut lib = Library::create(dir.path(), "audit").unwrap();
    lib.import(&source, None).unwrap();
    let asset = lib.page(&Query::default(), 0, 1).unwrap().assets.remove(0);
    (dir, lib, source, asset)
}

#[test]
fn orphan_scan_and_recheck_preserve_registered_project_aliases() {
    let (_dir, mut lib, source, asset) = fixture();
    let project = lib.create_project("second").unwrap();
    assert!(!lib.import(&source, Some(project)).unwrap());
    let alias: String = lib
        .db
        .query_row("SELECT path FROM project_files LIMIT 1", [], |r| r.get(0))
        .unwrap();
    let age = SystemTime::now() - std::time::Duration::from_secs(172800);
    File::options()
        .write(true)
        .open(lib.root.join(&alias))
        .unwrap()
        .set_modified(age)
        .unwrap();
    let rogue = "assets/candidate.png";
    fs::copy(&source, lib.root.join(rogue)).unwrap();
    File::options()
        .write(true)
        .open(lib.root.join(rogue))
        .unwrap()
        .set_modified(age)
        .unwrap();
    let plan = lib.orphan_scan().run().unwrap();
    assert_eq!(plan.count, 1);
    let page = plan.page("").unwrap();
    lib.db
        .execute(
            "INSERT INTO project_files VALUES(?1,?2,?3)",
            params![
                project.to_string(),
                asset.version_id.to_string(),
                rogue.to_uppercase()
            ],
        )
        .unwrap();
    let report = lib.quarantine_orphans(&plan, page).unwrap();
    assert_eq!((report.moved, report.skipped), (0, 1));
    assert!(lib.root.join(alias).is_file());
    assert!(lib.root.join(rogue).is_file());
}

#[test]
fn recovery_rejects_partial_copy_and_cleans_successful_staging() {
    for partial in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.png");
        image::RgbImage::from_pixel(16, 16, image::Rgb([30, 60, 90]))
            .save(&source)
            .unwrap();
        let mut lib = Library::create(dir.path(), "recovery").unwrap();
        assert!(lib.import_inner(&source, None, true, false, None).is_err());
        if partial {
            let part = fs::read_dir(lib.root.join("staging"))
                .unwrap()
                .map(|e| e.unwrap().path())
                .find(|p| p.extension().is_some_and(|s| s == "part"))
                .unwrap();
            // Still-decodable bytes must not pass an exact-length recovery receipt.
            File::options()
                .append(true)
                .open(part)
                .unwrap()
                .write_all(b"extra")
                .unwrap();
        }
        let report = lib.recover_imports().unwrap();
        assert_eq!(
            (report.imported, report.failed),
            if partial { (0, 1) } else { (1, 0) }
        );
        if !partial {
            assert_eq!(fs::read_dir(lib.root.join("staging")).unwrap().count(), 0);
        }
        assert!(source.is_file());
    }
}

#[test]
fn every_old_schema_gets_a_consistent_snapshot_before_migration() {
    let (_dir, lib, _source, _asset) = fixture();
    let root = lib.root.clone();
    lib.db
        .execute_batch("DROP TABLE background_job_errors; PRAGMA user_version=21;")
        .unwrap();
    drop(lib);
    let lib = Library::open(&root).unwrap();
    let backup = fs::read_dir(root.join("backups"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("schema-21-")
        })
        .unwrap();
    let db = Connection::open(backup).unwrap();
    assert_eq!(
        db.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))
            .unwrap(),
        21
    );
    assert_eq!(
        lib.db
            .pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))
            .unwrap(),
        SCHEMA
    );
}

#[test]
fn unsafe_manifest_and_project_names_are_rejected_before_filesystem_use() {
    let (_dir, lib, _source, _asset) = fixture();
    for name in ["CON", "nul.jpg", "COM1", "客户.", "a/b", "../escape"] {
        assert!(lib.create_project(name).is_err(), "{name}");
    }
    let root = lib.root.clone();
    let mut manifest = lib.manifest.clone();
    manifest.name = "../escape".into();
    drop(lib);
    fs::write(
        root.join("library.toml"),
        toml::to_string(&manifest).unwrap(),
    )
    .unwrap();
    assert!(Library::open(&root).is_err());
}

#[test]
fn purge_removes_version_previews_and_preserves_the_source() {
    let (_dir, mut lib, source, asset) = fixture();
    let path = lib
        .root
        .join(format!("previews/{}-srgb1-1600.png", asset.version_id));
    fs::write(&path, b"derivative").unwrap();
    lib.edit(&[asset.id], &Edit::Trash(true)).unwrap();
    lib.purge(&[asset.id]).unwrap();
    assert!(!path.exists());
    assert!(source.is_file());
}

#[test]
fn external_references_pause_automatic_reads_and_unc_is_rejected_without_opening_it() {
    let (_dir, mut lib, source, _asset) = fixture();
    // Use distinct content to register an external original rather than deduplicate it.
    image::RgbImage::from_pixel(2, 2, image::Rgb([99, 88, 77]))
        .save(&source)
        .unwrap();
    lib.import_file(
        &ImportFile {
            path: source,
            relative: "external.png".into(),
            project_name: None,
        },
        None,
        true,
    )
    .unwrap();
    lib.enqueue_media_job(JobKind::Integrity, 0).unwrap();
    let root = lib.root.clone();
    drop(lib);
    let lib = Library::open(&root).unwrap();
    assert_eq!(lib.background_jobs().unwrap()[0].state, JobState::Paused);
    #[cfg(windows)]
    {
        let version: String = lib
            .db
            .query_row("SELECT version_id FROM file_locations", [], |r| r.get(0))
            .unwrap();
        let (platform, bytes) =
            organize::encode_path(Path::new(r"\\audit-no-connect.invalid\share\photo.png"))
                .unwrap();
        lib.db
            .execute(
                "UPDATE file_locations SET platform=?1,native_path=?2",
                params![platform, bytes],
            )
            .unwrap();
        assert!(
            lib.version_path(&version, true)
                .unwrap_err()
                .to_string()
                .contains("网络")
        );
    }
}

#[test]
fn video_link_snapshot_keeps_content_after_source_name_is_removed() {
    let (_dir, lib, _source, asset) = fixture();
    let source = lib.asset_path(asset.id).unwrap();
    let expected = fs::read(&source).unwrap();
    let snapshot =
        video::linked_snapshot(&source, &lib.root, file_id::get_file_id(&source).unwrap()).unwrap();
    assert_eq!(
        file_id::get_file_id(&snapshot).unwrap(),
        file_id::get_file_id(&source).unwrap()
    );
    fs::remove_file(source).unwrap();
    assert_eq!(fs::read(&snapshot).unwrap(), expected);
    let path = snapshot.to_path_buf();
    drop(snapshot);
    assert!(!path.exists());
}

#[test]
fn duplicate_import_restores_trashed_content_instead_of_hiding_it() {
    let (_dir, mut lib, source, asset) = fixture();
    lib.edit(&[asset.id], &Edit::Trash(true)).unwrap();
    assert!(!lib.import(&source, None).unwrap());
    assert_eq!(lib.page(&Query::default(), 0, 10).unwrap().total, 1);
    assert_eq!(lib.asset_trash_state(asset.id).unwrap(), Some(false));
}

#[test]
fn location_rule_snapshot_updates_after_setting_and_clearing_location() {
    let (_dir, mut lib, _source, asset) = fixture();
    let query = Query {
        rule: Some(Rule::HasLocation),
        ..Default::default()
    };
    assert_eq!(lib.page(&query, 0, 10).unwrap().total, 0);
    lib.set_location(
        asset.id,
        asset.version_id,
        Some(&AssetLocation {
            latitude: 10.,
            longitude: 20.,
            label: "studio".into(),
        }),
    )
    .unwrap();
    assert_eq!(lib.page(&query, 0, 10).unwrap().total, 1);
    lib.set_location(asset.id, asset.version_id, None).unwrap();
    assert_eq!(lib.page(&query, 0, 10).unwrap().total, 0);
}

#[test]
fn conflicting_purge_recovery_keeps_both_files_and_allows_open_for_inspection() {
    let (_dir, mut lib, _source, asset) = fixture();
    let job = Uuid::new_v4().to_string();
    let source: String = lib
        .db
        .query_row(
            "SELECT path FROM asset_versions WHERE id=?1",
            [asset.version_id.to_string()],
            |r| r.get(0),
        )
        .unwrap();
    let target = format!("trash/purge-{job}/0.part");
    fs::create_dir_all(lib.root.join(format!("trash/purge-{job}"))).unwrap();
    fs::copy(lib.root.join(&source), lib.root.join(&target)).unwrap();
    lib.db
        .execute("INSERT INTO purge_jobs VALUES(?1,0)", [&job])
        .unwrap();
    lib.db
        .execute(
            "INSERT INTO purge_files VALUES(?1,?2,?3)",
            params![job, source, target],
        )
        .unwrap();
    let root = lib.root.clone();
    drop(lib);
    lib = Library::open(&root).unwrap();
    assert!(root.join(&source).is_file());
    assert!(root.join(&target).is_file());
    assert!(
        lib.events(true, None)
            .unwrap()
            .items
            .iter()
            .any(|notice| notice.kind == "删除恢复需要检查")
    );
    lib.edit(&[asset.id], &Edit::Trash(true)).unwrap();
    assert!(lib.purge(&[asset.id]).is_err());
    assert!(root.join(source).is_file());
    assert!(root.join(target).is_file());
}

#[test]
fn orphan_scan_preserves_registered_paths_with_different_case() {
    let (_dir, lib, _source, asset) = fixture();
    let path = lib.asset_path(asset.id).unwrap();
    File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(SystemTime::now() - std::time::Duration::from_secs(172800))
        .unwrap();
    lib.db
        .execute(
            "UPDATE asset_versions SET path=upper(path) WHERE id=?1",
            [asset.version_id.to_string()],
        )
        .unwrap();
    assert_eq!(lib.orphan_scan().run().unwrap().count, 0);
}

#[test]
fn referenced_import_uses_production_ticket_and_survives_reopen_and_purge() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("external.png");
    image::RgbImage::from_pixel(16, 12, image::Rgb([10, 20, 30]))
        .save(&source)
        .unwrap();
    let original = fs::read(&source).unwrap();
    let mut lib = Library::create(dir.path(), "references").unwrap();
    let file = ImportFile {
        path: source.clone(),
        relative: PathBuf::from("external.png"),
        project_name: None,
    };
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let ticket = lib.begin_import(&file, None, true).unwrap();
    ticket.copy(&cancel).unwrap();
    let prepared = ticket.prepare(&cancel).unwrap();
    assert!(lib.finish_import(ticket, prepared).unwrap());
    let root = lib.root.clone();
    drop(lib);
    let mut lib = Library::open(&root).unwrap();
    let page = lib
        .page(
            &Query {
                storage: 2,
                ..Default::default()
            },
            0,
            10,
        )
        .unwrap();
    assert_eq!(page.total, 1);
    let asset = &page.assets[0];
    assert_eq!(
        lib.asset_path(asset.id).unwrap(),
        source.canonicalize().unwrap()
    );
    lib.edit(&[asset.id], &Edit::Trash(true)).unwrap();
    lib.purge(&[asset.id]).unwrap();
    assert_eq!(fs::read(&source).unwrap(), original);
}

#[test]
fn invalid_names_never_publish_unopenable_libraries_or_projects() {
    let dir = tempfile::tempdir().unwrap();
    for name in ["NUL", "COM¹", "a/b", "report\u{1}A"] {
        assert!(Library::create(dir.path(), name).is_err(), "{name}");
        assert!(!dir.path().join(format!("{name}.moriq")).exists());
    }
    let mut lib = Library::create(dir.path(), "valid").unwrap();
    for name in ["CON", "a/b", "child\u{1}"] {
        assert!(lib.create_project(name).is_err());
        assert!(lib.create_child_project(name, None).is_err());
        assert!(lib.update_profile(name, "").is_err());
    }
}

#[test]
fn cache_trim_rebuilds_thumbnails_on_demand_without_changing_originals() {
    let (_dir, lib, _source, asset) = fixture();
    let original = fs::read(lib.asset_path(asset.id).unwrap()).unwrap();
    let cache = lib
        .root
        .join(format!("thumbnails/256/{}.png", asset.version_id));
    assert!(cache.is_file());
    let report = lib.cache_plan().trim(0).unwrap();
    assert!(report.removed_files >= 1);
    assert!(!cache.exists());
    let plan = lib.thumbnail_plan(&[asset.version_id]).unwrap();
    assert_eq!(plan.memory_mib(), 1024);
    let (images, warnings) = plan.read();
    assert_eq!(warnings, 0);
    assert!(images[0].is_some());
    assert!(cache.is_file());
    assert_eq!(
        lib.thumbnail_plan(&[asset.version_id])
            .unwrap()
            .memory_mib(),
        128
    );
    assert_eq!(
        fs::read(lib.asset_path(asset.id).unwrap()).unwrap(),
        original
    );
}

#[test]
fn completed_job_and_item_cleanup_commit_together() {
    let (_dir, mut lib, _source, _asset) = fixture();
    let id = lib.enqueue_media_job(JobKind::Integrity, 0).unwrap();
    lib.db
        .execute(
            "UPDATE background_jobs SET checkpoint=total WHERE id=?1",
            [&id],
        )
        .unwrap();
    lib.db.execute_batch("CREATE TRIGGER fail_item_cleanup BEFORE DELETE ON background_job_items BEGIN SELECT RAISE(ABORT,'injected cleanup failure'); END;").unwrap();
    assert!(lib.claim_job_step().is_err());
    let state: String = lib
        .db
        .query_row(
            "SELECT state FROM background_jobs WHERE id=?1",
            [&id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(state, "queued");
    lib.db
        .execute_batch("DROP TRIGGER fail_item_cleanup")
        .unwrap();
    assert!(lib.claim_job_step().unwrap().is_none());
    assert_eq!(
        lib.db
            .query_row(
                "SELECT count(*) FROM background_job_items WHERE job_id=?1",
                [&id],
                |r| r.get::<_, u64>(0)
            )
            .unwrap(),
        0
    );
}
