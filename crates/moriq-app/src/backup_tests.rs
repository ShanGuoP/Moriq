use super::*;
use std::{sync::mpsc, time::Duration};

#[test]
/// 验证慢速备份不占住写入端，切库后不会发布旧库副本。
fn slow_snapshot_copy_leaves_writer_available_and_switching_library_rejects_publication() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("original.png");
        image::RgbImage::from_pixel(16, 16, image::Rgb([12, 34, 56]))
            .save(&input)
            .unwrap();
        let service = Service::start().unwrap();
        let opened = service
            .create(dir.path().into(), "源库".into())
            .await
            .unwrap();
        let owner = opened.info.id;
        assert_eq!(
            service
                .import_sources(owner, vec![input], None, ImportControl::default())
                .await
                .imported,
            1
        );
        let parent = dir.path().to_path_buf();
        let plan = service
            .call(move |active| {
                check(active, owner)?
                    .backup_plan(&parent, false)
                    .map_err(|e| e.to_string())
            })
            .await
            .unwrap();
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let (resume_tx, resume_rx) = mpsc::sync_channel(1);
        let worker = tokio::task::spawn_blocking(move || {
            let mut started = Some(started_tx);
            plan.copy_with_progress(&BackupControl::default(), |_| {
                if let Some(sender) = started.take() {
                    sender.send(()).unwrap();
                    resume_rx.recv_timeout(Duration::from_secs(10)).unwrap();
                }
            })
        });
        started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let info = tokio::time::timeout(Duration::from_secs(2), service.info(owner))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(info.count, 1);
        let asset = service
            .page(owner, Query::default(), 0, 1)
            .await
            .unwrap()
            .tiles[0]
            .asset
            .id;
        tokio::time::timeout(Duration::from_secs(2), service.rating(owner, asset, 4))
            .await
            .unwrap()
            .unwrap();
        service
            .create(dir.path().into(), "新库".into())
            .await
            .unwrap();
        resume_tx.send(()).unwrap();
        let prepared = worker.await.unwrap().unwrap();
        let result = service
            .call(move |active| {
                check(active, owner)?;
                prepared.publish().map_err(|e| e.to_string())
            })
            .await;
        assert!(result.is_err());
        assert!(!std::fs::read_dir(dir.path()).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".moriq-backup-")
        }));
        assert!(!std::fs::read_dir(dir.path()).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains("副本")
        }));
        let original = Library::open(&opened.root).unwrap();
        assert_eq!(original.info().unwrap().count, 1);
    });
}
