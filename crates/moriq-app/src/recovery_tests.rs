use super::*;
use std::time::{Duration, Instant};

fn interrupted() -> (tempfile::TempDir, PathBuf, LibraryId, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("原片.png");
    image::RgbImage::from_pixel(16, 12, image::Rgb([12, 34, 56]))
        .save(&source)
        .unwrap();
    let mut library = Library::create(dir.path(), "恢复库").unwrap();
    let file = ImportFile {
        path: source.clone(),
        relative: PathBuf::from("原片.png"),
        project_name: None,
    };
    let ticket = library.begin_import(&file, None, false).unwrap();
    ticket.copy(&AtomicBool::new(false)).unwrap();
    library.interrupt_import(&ticket).unwrap();
    let root = library.root().to_path_buf();
    let owner = library.id();
    drop(ticket);
    drop(library);
    (dir, root, owner, source)
}
async fn wait_for_plan(control: &ImportControl) {
    let start = Instant::now();
    while control.discovered.load(Ordering::Acquire) == 0 {
        assert!(start.elapsed() < Duration::from_secs(5));
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

#[test]
fn recovery_waiting_for_budget_keeps_writer_responsive_cancels_and_resumes_without_late_writes() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let (dir, root, owner, source) = interrupted();
        let original = std::fs::read(&source).unwrap();
        let service = Service::start().unwrap();
        service.open(root.clone()).await.unwrap();
        let memory = service
            .import_memory
            .clone()
            .acquire_many_owned(4096)
            .await
            .unwrap();
        let control = ImportControl::default();
        let task_service = service.clone();
        let task_control = control.clone();
        let task =
            tokio::spawn(async move { task_service.recover_controlled(owner, task_control).await });
        wait_for_plan(&control).await;
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), service.info(owner))
                .await
                .unwrap()
                .unwrap()
                .count,
            0
        );
        control.cancel.store(true, Ordering::Release);
        let report = tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(report.cancelled);
        assert_eq!(report.imported, 0);
        assert_eq!(service.info(owner).await.unwrap().count, 0);
        let control = ImportControl::default();
        let task_control = control.clone();
        let task_service = service.clone();
        let task =
            tokio::spawn(async move { task_service.recover_controlled(owner, task_control).await });
        wait_for_plan(&control).await;
        let other = service
            .create(dir.path().into(), "新库".into())
            .await
            .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_secs(2), task)
                .await
                .unwrap()
                .unwrap()
                .unwrap()
                .cancelled
        );
        assert_eq!(service.info(other.info.id).await.unwrap().count, 0);
        drop(memory);
        service.open(root.clone()).await.unwrap();
        let report = service.recover(owner).await.unwrap();
        assert_eq!((report.imported, report.failed), (1, 0));
        assert_eq!(std::fs::read(&source).unwrap(), original);
        assert_eq!(std::fs::read_dir(root.join("staging")).unwrap().count(), 0);
        service.close().await.unwrap();
    });
}

#[test]
fn dropping_recovery_future_cancels_the_registered_worker_and_preserves_journal() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let (_dir, root, owner, _source) = interrupted();
        let service = Service::start().unwrap();
        service.open(root).await.unwrap();
        let memory = service
            .import_memory
            .clone()
            .acquire_many_owned(4096)
            .await
            .unwrap();
        let control = ImportControl::default();
        let task_control = control.clone();
        let task_service = service.clone();
        let task =
            tokio::spawn(async move { task_service.recover_controlled(owner, task_control).await });
        wait_for_plan(&control).await;
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert!(control.cancel.load(Ordering::Acquire));
        drop(memory);
        assert_eq!(service.info(owner).await.unwrap().count, 0);
        assert_eq!(service.recover(owner).await.unwrap().imported, 1);
        service.close().await.unwrap();
    });
}

#[test]
fn library_alias_resolution_returns_the_closed_identity_and_respects_unsaved_work() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let (dir, root, owner, _source) = interrupted();
        let service = Service::start().unwrap();
        service.open(root.clone()).await.unwrap();
        let alias = root.join(".");
        assert!(
            service
                .close_library_at(alias.clone(), false)
                .await
                .is_err()
        );
        assert_eq!(service.info(owner).await.unwrap().id, owner);
        assert_eq!(
            service
                .close_library_at(dir.path().join("offline.moriq"), true)
                .await
                .unwrap(),
            None
        );
        assert_eq!(
            service.close_library_at(alias, true).await.unwrap(),
            Some(owner)
        );
        assert!(service.info(owner).await.is_err());
        service.close().await.unwrap();
    });
}
