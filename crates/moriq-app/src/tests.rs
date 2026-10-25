use super::*;
#[test]
fn asset_cover_persists_to_the_card_and_switching_library_during_budget_wait_rejects_commit() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("cover.png");
        image::RgbImage::from_pixel(200, 100, image::Rgb([40, 80, 120]))
            .save(&input)
            .unwrap();
        let mut lib = Library::create(dir.path(), "cover").unwrap();
        lib.import(&input, None).unwrap();
        let asset = lib.page(&Query::default(), 0, 1).unwrap().assets.remove(0);
        let root = lib.root().to_owned();
        drop(lib);
        let service = Service::start().unwrap();
        let owner = service.open(root.clone()).await.unwrap().info.id;
        service
            .set_asset_cover(owner, asset.id, asset.version_id)
            .await
            .unwrap();
        let previous = std::fs::read(root.join("metadata/cover.png")).unwrap();
        assert!(
            service.library_cards(vec![root.clone()]).await.unwrap()[0]
                .1
                .is_some()
        );
        assert!(
            service
                .set_asset_cover(owner, asset.id, AssetVersionId::default())
                .await
                .is_err()
        );
        let occupied = service
            .import_memory
            .clone()
            .acquire_many_owned(4096)
            .await
            .unwrap();
        let clone = service.clone();
        let mut saving = tokio::spawn(async move {
            clone
                .set_asset_cover(owner, asset.id, asset.version_id)
                .await
        });
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), &mut saving)
                .await
                .is_err()
        );
        let other = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            service.create(dir.path().into(), "other".into()),
        )
        .await
        .unwrap()
        .unwrap();
        drop(occupied);
        assert!(saving.await.unwrap().is_err());
        assert!(service.cover(other.info.id).await.unwrap().is_none());
        assert_eq!(
            std::fs::read(root.join("metadata/cover.png")).unwrap(),
            previous
        );
        service.open(root).await.unwrap();
        assert_eq!(service.cover(owner).await.unwrap().unwrap().width, 200);
        service.close().await.unwrap();
    });
}
#[test]
fn cached_original_tiles_fit_the_small_interactive_budget() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("original.png");
        image::RgbaImage::from_pixel(1200, 800, image::Rgba([30, 70, 120, 160]))
            .save(&input)
            .unwrap();
        let service = Service::start().unwrap();
        let library = service
            .create(dir.path().into(), "tiles".into())
            .await
            .unwrap();
        let owner = library.info.id;
        assert_eq!(
            service
                .import_sources(owner, vec![input], None, ImportControl::default())
                .await
                .imported,
            1
        );
        let asset = service
            .page(owner, Query::default(), 0, 1)
            .await
            .unwrap()
            .tiles
            .remove(0)
            .asset;
        let session = service
            .region_session(owner, asset.id, asset.version_id)
            .await
            .unwrap();
        let occupied = service
            .import_memory
            .clone()
            .acquire_many_owned(4080)
            .await
            .unwrap();
        let tile = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            service.region_tile(
                owner,
                session,
                ImageRegion {
                    x: 0,
                    y: 0,
                    width: 512,
                    height: 512,
                    level: 1,
                },
            ),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(&tile.rgba[..4], &[30, 70, 120, 160]);
        drop(occupied);
        service.close().await.unwrap();
    });
}
#[test]
fn cached_preview_uses_small_budget_and_corrupt_cache_escalates_before_decoding() {
    use std::time::Duration;
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("preview.png");
        image::RgbaImage::from_pixel(1200, 800, image::Rgba([30, 70, 120, 160]))
            .save(&input)
            .unwrap();
        let service = Service::start().unwrap();
        let library = service
            .create(dir.path().into(), "cache".into())
            .await
            .unwrap();
        let owner = library.info.id;
        let report = service
            .import_sources(owner, vec![input], None, ImportControl::default())
            .await;
        assert_eq!(report.imported, 1);
        let asset = service
            .page(owner, Query::default(), 0, 1)
            .await
            .unwrap()
            .tiles
            .remove(0)
            .asset;
        let first = service
            .preview_version(owner, asset.id, asset.version_id)
            .await
            .unwrap();
        assert_eq!(&first.image.rgba[..4], &[30, 70, 120, 160]);
        service.media_cache.clear();
        let occupied = service
            .import_memory
            .clone()
            .acquire_many_owned(4000)
            .await
            .unwrap();
        let disk = tokio::time::timeout(
            Duration::from_secs(3),
            service.preview_version(owner, asset.id, asset.version_id),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(disk.image.rgba, first.image.rgba);
        service.media_cache.clear();
        std::fs::write(
            library
                .root
                .join(format!("previews/{}-srgb1-1600.png", asset.version_id)),
            b"corrupt",
        )
        .unwrap();
        assert!(
            tokio::time::timeout(
                Duration::from_millis(100),
                service.preview_version(owner, asset.id, asset.version_id)
            )
            .await
            .is_err()
        );
        drop(occupied);
        let repaired = service
            .preview_version(owner, asset.id, asset.version_id)
            .await
            .unwrap();
        assert_eq!(repaired.image.rgba, first.image.rgba);
        service.close().await.unwrap();
    });
}
#[test]
fn category_covers_use_real_matching_assets_and_clear_for_empty_or_other_library() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.png");
        let b = dir.path().join("b.png");
        image::RgbImage::from_pixel(24, 16, image::Rgb([20, 80, 140]))
            .save(&a)
            .unwrap();
        image::RgbImage::from_pixel(24, 16, image::Rgb([190, 70, 60]))
            .save(&b)
            .unwrap();
        let service = Service::start().unwrap();
        let library = service
            .create(dir.path().into(), "covers".into())
            .await
            .unwrap();
        let owner = library.info.id;
        let project = service.project(owner, "project".into()).await.unwrap();
        let report = service
            .import_sources(owner, vec![a, b], Some(project), ImportControl::default())
            .await;
        assert_eq!(report.imported, 2);
        let asset = service
            .call(move |active| {
                Ok(check(active, owner)?
                    .page(&Query::default(), 0, 2)
                    .map_err(|e| e.to_string())?
                    .assets
                    .into_iter()
                    .find(|a| a.name == "a.png")
                    .ok_or("missing asset")?
                    .id)
            })
            .await
            .unwrap();
        service
            .set_category(owner, vec![asset], "风景".into())
            .await
            .unwrap();
        service.favorite(owner, asset, true).await.unwrap();
        let overview = service.overview(owner).await.unwrap();
        assert_eq!(overview.favorites.len(), 1);
        assert!(overview.projects.iter().all(|(_, p)| p.is_some()));
        for (label, pixels) in overview.categories {
            match label.as_str() {
                "legacy:风景" => assert_eq!(&pixels.unwrap().rgba[..4], &[20, 80, 140, 255]),
                "_unclassified" => assert_eq!(&pixels.unwrap().rgba[..4], &[190, 70, 60, 255]),
                _ => assert!(pixels.is_none()),
            }
        }
        service
            .edit(owner, vec![asset], Edit::Trash(true))
            .await
            .unwrap();
        assert!(
            service
                .overview(owner)
                .await
                .unwrap()
                .categories
                .iter()
                .all(|(label, p)| label != "legacy:风景" || p.is_none())
        );
        let next = service
            .create(dir.path().into(), "empty".into())
            .await
            .unwrap();
        assert!(
            service
                .overview(next.info.id)
                .await
                .unwrap()
                .categories
                .iter()
                .all(|(_, p)| p.is_none())
        );
        assert!(service.overview(owner).await.is_err());
        service.close().await.unwrap();
    });
}
#[test]
fn ten_worker_jpeg_import_runs_concurrently_with_one_gib_budget() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let source_dir = dir.path().join("photos");
        std::fs::create_dir(&source_dir).unwrap();
        let source = source_dir.join("0.jpg");
        image::RgbImage::from_pixel(4016, 6016, image::Rgb([20, 80, 140]))
            .save(&source)
            .unwrap();
        for index in 1..10 {
            std::fs::copy(&source, source_dir.join(format!("{index}.jpg"))).unwrap();
        }
        let service = Service::start().unwrap();
        let library = service
            .create(dir.path().into(), "concurrent".into())
            .await
            .unwrap();
        let control = ImportControl {
            workers: 10,
            memory_mib: 1024,
            ..Default::default()
        };
        let report = service
            .import_sources(library.info.id, vec![source_dir], None, control.clone())
            .await;
        let peak = control.peak.load(Ordering::Relaxed);
        eprintln!(
            "24 MP JPEG: workers=10 budget=1024 MiB peak={peak} largest={} MiB",
            control.largest_mib.load(Ordering::Relaxed)
        );
        assert_eq!(report.failed, 0, "{:?}", report.failures);
        assert_eq!(report.imported + report.duplicates, 10);
        assert!(peak > 1, "JPEG pipeline unexpectedly serialized");
        assert!(peak <= 10);
        assert_eq!(control.active.load(Ordering::Relaxed), 0);
        assert_eq!(control.waiting_memory.load(Ordering::Relaxed), 0);
        service.close().await.unwrap();
    });
}
#[test]
fn blocked_media_reads_leave_writer_responsive_and_cancellation_releases_library_lease() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("原片.png");
        image::RgbImage::from_pixel(3, 2, image::Rgb([1, 2, 3]))
            .save(&source)
            .unwrap();
        let service = Service::start().unwrap();
        let opened = service
            .create(dir.path().into(), "读取隔离".into())
            .await
            .unwrap();
        let id = opened.info.id;
        service
            .import(
                id,
                vec![ImportFile {
                    path: source,
                    relative: "原片.png".into(),
                    project_name: None,
                }],
                None,
                ImportControl::default(),
            )
            .await;
        let asset = service
            .page(id, Query::default(), 0, 1)
            .await
            .unwrap()
            .tiles[0]
            .asset
            .id;
        let budget = service
            .import_memory
            .clone()
            .acquire_many_owned(4096)
            .await
            .unwrap();
        let reader = service.clone();
        let task = tokio::spawn(async move { reader.preview_full(id, asset).await });
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while service.read_slots.available_permits() == 4 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        let info = tokio::time::timeout(std::time::Duration::from_secs(2), service.info(id))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(info.id, id);
        service.close().await.unwrap();
        assert!(Library::open(&opened.root).is_err());
        task.abort();
        let _ = task.await;
        drop(budget);
        assert_eq!(service.read_slots.available_permits(), 4);
        assert_eq!(service.import_memory.available_permits(), 4096);
        let lib = Library::open(&opened.root).unwrap();
        assert_eq!(lib.info().unwrap().count, 1);
    });
}
fn import_fixtures(dir: &std::path::Path) -> Vec<ImportFile> {
    std::fs::create_dir_all(dir).unwrap();
    (0..8)
        .map(|n| {
            let path = dir.join(format!("照片-{n}.jpg"));
            image::RgbImage::from_pixel(4032, 3024, image::Rgb([n * 20, 90, 150]))
                .save(&path)
                .unwrap();
            ImportFile {
                path,
                relative: format!("项目/子目录/照片-{n}.jpg").into(),
                project_name: Some("项目".into()),
            }
        })
        .collect()
}
#[test]
fn maintenance_waiting_for_media_budget_can_cancel_without_blocking_writer() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let service = Service::start().unwrap();
        let library = service
            .create(dir.path().into(), "维护库".into())
            .await
            .unwrap();
        let id = library.info.id;
        let budget = service
            .import_memory
            .clone()
            .acquire_many_owned(4096)
            .await
            .unwrap();
        let control = MaintenanceControl::default();
        let worker_service = service.clone();
        let worker_control = control.clone();
        let task = tokio::spawn(async move {
            worker_service
                .maintenance_controlled(id, true, worker_control)
                .await
        });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while service.maintenance_slot.available_permits() != 0 {
            assert!(std::time::Instant::now() < deadline);
            tokio::task::yield_now().await;
        }
        assert_eq!(service.info(id).await.unwrap().id, id);
        assert!(
            service
                .maintenance_controlled(id, false, MaintenanceControl::default())
                .await
                .is_err()
        );
        control.cancel.store(true, Ordering::Relaxed);
        assert!(task.await.unwrap().unwrap().cancelled);
        drop(budget);
        assert_eq!(service.import_memory.available_permits(), 4096);
        assert_eq!(service.maintenance_slot.available_permits(), 1);
        service.close().await.unwrap();
        service.open(library.root).await.unwrap();
        service.close().await.unwrap();
    });
}
#[test]
fn background_repair_and_purge_keep_external_source_and_reject_stale_library() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("外部.png");
        image::RgbImage::from_pixel(32, 24, image::Rgb([1, 2, 3]))
            .save(&source)
            .unwrap();
        let service = Service::start().unwrap();
        let library = service
            .create(dir.path().into(), "维护库".into())
            .await
            .unwrap();
        let id = library.info.id;
        let control = ImportControl {
            referenced: true,
            ..Default::default()
        };
        assert_eq!(
            service
                .import_sources(id, vec![source.clone()], None, control)
                .await
                .imported,
            1
        );
        let page = service.page(id, Query::default(), 0, 10).await.unwrap();
        let asset = page.tiles[0].asset.clone();
        let cache = library
            .root
            .join(format!("thumbnails/256/{}.png", asset.version_id));
        std::fs::write(&cache, b"broken").unwrap();
        let control = MaintenanceControl::default();
        let report = service
            .maintenance_controlled(id, true, control.clone())
            .await
            .unwrap();
        assert_eq!((report.checked, report.repaired), (1, 1));
        assert_eq!(control.finished.load(Ordering::Relaxed), 1);
        assert!(image::open(cache).is_ok());
        service
            .edit(id, vec![asset.id], Edit::Trash(true))
            .await
            .unwrap();
        assert_eq!(service.purge(id, vec![asset.id]).await.unwrap().deleted, 1);
        assert!(source.is_file());
        service.close().await.unwrap();
        let other = service
            .create(dir.path().into(), "另一个库".into())
            .await
            .unwrap();
        assert!(service.purge(id, vec![asset.id]).await.is_err());
        assert_eq!(service.info(other.info.id).await.unwrap().count, 0);
        service.close().await.unwrap();
    });
}
#[test]
fn concurrent_pipeline_deduplicates_preserves_paths_and_drains_workers() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let files = import_fixtures(&dir.path().join("源"));
        let service = Service::start().unwrap();
        let library = service
            .create(dir.path().into(), "库".into())
            .await
            .unwrap();
        let control = ImportControl {
            workers: 4,
            ..Default::default()
        };
        let mut duplicates = files.clone();
        duplicates.extend(files.clone());
        let result = service
            .import(library.info.id, duplicates, None, control.clone())
            .await;
        assert_eq!(
            (result.imported, result.duplicates, result.failed),
            (8, 8, 0),
            "{:?}",
            result.failures
        );
        assert!(control.peak.load(Ordering::Relaxed) > 1);
        assert!(control.peak.load(Ordering::Relaxed) <= 4);
        assert_eq!(control.active.load(Ordering::Relaxed), 0);
        assert_eq!(service.import_memory.available_permits(), 4096);
        assert_eq!(service.import_slots.available_permits(), 32);
        assert_eq!(
            service
                .page(library.info.id, Query::default(), 0, 32)
                .await
                .unwrap()
                .total,
            8
        );
        for file in files {
            let target = library
                .root
                .join("assets/项目/子目录")
                .join(file.path.file_name().unwrap());
            assert_eq!(
                std::fs::read(file.path).unwrap(),
                std::fs::read(target).unwrap()
            );
        }
        assert_eq!(
            std::fs::read_dir(library.root.join("staging"))
                .unwrap()
                .count(),
            0
        );
        service.close().await.unwrap();
    });
}
#[test]
fn stopping_pipeline_leaves_no_late_writes_or_workers() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let files = import_fixtures(&dir.path().join("源"));
        let service = Service::start().unwrap();
        let library = service
            .create(dir.path().into(), "库".into())
            .await
            .unwrap();
        let control = ImportControl {
            workers: 4,
            ..Default::default()
        };
        let task_service = service.clone();
        let task_control = control.clone();
        let id = library.info.id;
        let task =
            tokio::spawn(async move { task_service.import(id, files, None, task_control).await });
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while control.active.load(Ordering::Relaxed) == 0 {
            assert!(std::time::Instant::now() < deadline, "worker never started");
            tokio::task::yield_now().await;
        }
        // The writer remains available while media workers run.
        assert_eq!(service.info(id).await.unwrap().id, id);
        control.cancel.store(true, Ordering::Relaxed);
        assert!(task.await.unwrap().cancelled);
        assert_eq!(control.active.load(Ordering::Relaxed), 0);
        assert_eq!(service.import_memory.available_permits(), 4096);
        assert_eq!(service.import_slots.available_permits(), 32);
        service.close().await.unwrap();
        // Successful reopen also proves all filesystem ticket leases were released.
        let reopened = service.open(library.root).await.unwrap();
        assert_eq!(reopened.info.id, id);
        service.close().await.unwrap();
    });
}
#[test]
fn streaming_import_continues_after_bad_media_and_finishes_scanner() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("来源/子目录");
        std::fs::create_dir_all(&folder).unwrap();
        for n in 0..3u8 {
            image::RgbImage::from_pixel(24, 16, image::Rgb([n * 60, 80, 150]))
                .save(folder.join(format!("{n}.png")))
                .unwrap();
        }
        std::fs::write(folder.join("损坏.jpg"), b"invalid").unwrap();
        let service = Service::start().unwrap();
        let library = service
            .create(dir.path().into(), "库".into())
            .await
            .unwrap();
        let control = ImportControl::with_preferences(ImportPreferences {
            workers: 8,
            memory_mib: 512,
        });
        let report = service
            .import_sources(
                library.info.id,
                vec![dir.path().join("来源")],
                None,
                control.clone(),
            )
            .await;
        assert_eq!((report.imported, report.failed), (3, 1));
        assert_eq!(control.discovered.load(Ordering::Relaxed), 4);
        assert!(!control.scanning.load(Ordering::Relaxed));
        assert_eq!(control.active.load(Ordering::Relaxed), 0);
        assert!(library.root.join("assets/来源/子目录/0.png").is_file());
        service.close().await.unwrap();
    });
}
#[test]
fn folder_scan_keeps_root_name_and_nested_structure() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("导入目录/子目录");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("照片.JPG"), [1, 2, 3]).unwrap();
        std::fs::write(folder.join("说明.txt"), [0]).unwrap();
        let service = Service::start().unwrap();
        let library = service
            .create(dir.path().into(), "库".into())
            .await
            .unwrap();
        let files = service
            .scan(library.info.id, vec![dir.path().join("导入目录")])
            .await
            .unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].project_name.as_deref(), Some("导入目录"));
        assert_eq!(files[0].relative, PathBuf::from("导入目录/子目录/照片.JPG"));
        assert!(
            service
                .scan(library.info.id, vec![library.root])
                .await
                .is_err()
        );
        service.close().await.unwrap();
    });
}
#[test]
fn stale_library_commands_are_rejected() {
    let mut active = None;
    assert!(check(&mut active, LibraryId::default()).is_err());
}
#[test]
fn switching_libraries_rejects_old_jobs_and_preserves_active_on_open_failure() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let service = Service::start().unwrap();
        let a = service
            .create(directory.path().into(), "A".into())
            .await
            .unwrap();
        let b = service
            .create(directory.path().into(), "B".into())
            .await
            .unwrap();
        assert!(service.info(a.info.id).await.is_err());
        assert!(
            service
                .favorite(a.info.id, AssetId::default(), true)
                .await
                .is_err()
        );
        assert!(
            service
                .open(directory.path().join("missing.moriq"))
                .await
                .is_err()
        );
        assert_eq!(service.info(b.info.id).await.unwrap().id, b.info.id);
        let reopened = service.open(a.root).await.unwrap();
        assert_eq!(reopened.info.id, a.info.id);
        service
            .call(|active| {
                *active = None;
                Ok(())
            })
            .await
            .unwrap();
    });
}
