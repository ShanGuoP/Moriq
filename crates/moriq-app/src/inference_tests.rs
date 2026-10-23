use super::*;
use inference::InferenceRunner;
use moriq_library::analysis::InferenceResult;
use std::time::{Duration, Instant};

struct FakeRunner {
    entered: AtomicBool,
    release: AtomicBool,
    cancelled: AtomicBool,
    mode: u8,
}
impl FakeRunner {
    fn new(mode: u8, release: bool) -> Arc<Self> {
        Arc::new(Self {
            entered: AtomicBool::new(false),
            release: AtomicBool::new(release),
            cancelled: AtomicBool::new(false),
            mode,
        })
    }
}
impl InferenceRunner for FakeRunner {
    fn revision(&self, _: ModelId) -> TaskResult<String> {
        Ok("test-revision".into())
    }
    fn ready(&self, _: ModelId) -> TaskResult<()> {
        Ok(())
    }
    fn infer(
        &self,
        model: ModelId,
        input: serde_json::Value,
        cancel: &AtomicBool,
    ) -> TaskResult<InferenceResult> {
        if let Some(path) = input["image"].as_str() {
            assert!(std::path::Path::new(path).is_file());
        }
        self.entered.store(true, Ordering::Release);
        let started = Instant::now();
        while !self.release.load(Ordering::Acquire) {
            if cancel.load(Ordering::Acquire) {
                self.cancelled.store(true, Ordering::Release);
            }
            assert!(started.elapsed() < Duration::from_secs(15));
            std::thread::sleep(Duration::from_millis(5));
        }
        self.cancelled
            .fetch_or(cancel.load(Ordering::Acquire), Ordering::Release);
        // Intentionally return late results even after cancellation: the coordinator owns acceptance.
        if self.mode == 1 {
            return Err(TaskError::Runtime("injected failure".into()));
        }
        Ok(InferenceResult {
            model: if self.mode == 2 {
                ModelId::Faces
            } else {
                model
            },
            revision: if self.mode == 3 {
                String::new()
            } else {
                "test-revision".into()
            },
            data: serde_json::json!({"image_vector":[1.0,0.0],"text_vector":[1.0,0.0],"labels":[{"label":"测试分类"}]}),
        })
    }
}
fn seed() -> (tempfile::TempDir, PathBuf, AssetId) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("input.png");
    image::RgbImage::from_pixel(8, 8, image::Rgb([20, 40, 60]))
        .save(&path)
        .unwrap();
    let mut library = Library::create(dir.path(), "推理边界").unwrap();
    library.import(&path, None).unwrap();
    let asset = library.page(&Query::default(), 0, 1).unwrap().assets[0].id;
    (dir, library.root().into(), asset)
}
async fn wait_entered(runner: &FakeRunner) {
    let started = Instant::now();
    while !runner.entered.load(Ordering::Acquire) {
        assert!(started.elapsed() < Duration::from_secs(10));
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
async fn wait_completed(service: &Service, owner: LibraryId) -> BackgroundJob {
    let started = Instant::now();
    loop {
        let job = service.background_jobs(owner).await.unwrap().remove(0);
        if job.state == JobState::Completed {
            return job;
        }
        assert!(started.elapsed() < Duration::from_secs(10), "{job:?}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
#[test]
fn injected_inference_binds_revision_and_supports_both_search_inputs_without_runtime() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let (_dir, root, asset) = seed();
        let runner = FakeRunner::new(0, true);
        let service = Service::start_with_inference(runner).unwrap();
        let owner = service.open(root).await.unwrap().info.id;
        service
            .queue_media_job(owner, JobKind::Analysis(ModelId::ChineseClip))
            .await
            .unwrap();
        assert_eq!(wait_completed(&service, owner).await.failed, 0);
        assert_eq!(
            service.semantic_search(owner, "测试".into()).await.unwrap(),
            vec![asset]
        );
        assert!(
            service
                .search_by_image(owner, asset)
                .await
                .unwrap()
                .is_empty()
        ); // selected image is excluded
        assert_eq!(
            service.details(owner, asset).await.unwrap().category,
            "测试分类"
        );
        assert!(service.semantic_search(owner, String::new()).await.is_err());
    });
}
#[test]
fn injected_failure_and_invalid_receipts_are_recorded_without_results() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        for mode in [1, 2, 3] {
            let (_dir, root, asset) = seed();
            let service = Service::start_with_inference(FakeRunner::new(mode, true)).unwrap();
            let owner = service.open(root).await.unwrap().info.id;
            service
                .queue_media_job(owner, JobKind::Analysis(ModelId::ChineseClip))
                .await
                .unwrap();
            assert_eq!(wait_completed(&service, owner).await.failed, 1);
            assert!(
                service
                    .details(owner, asset)
                    .await
                    .unwrap()
                    .category
                    .is_empty()
            );
            assert!(service.semantic_search(owner, "测试".into()).await.is_err());
            service.close().await.unwrap();
        }
    });
}
#[test]
fn cancelled_and_old_library_inference_never_publishes_late_results() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        for switch in [false, true] {
            let (dir, root, asset) = seed();
            let runner = FakeRunner::new(0, false);
            let service = Service::start_with_inference(runner.clone()).unwrap();
            let owner = service.open(root.clone()).await.unwrap().info.id;
            service
                .queue_media_job(owner, JobKind::Analysis(ModelId::ChineseClip))
                .await
                .unwrap();
            wait_entered(&runner).await;
            tokio::time::timeout(Duration::from_secs(2), service.rating(owner, asset, 4))
                .await
                .unwrap()
                .unwrap();
            if switch {
                service
                    .create(dir.path().into(), "另一个库".into())
                    .await
                    .unwrap();
            } else {
                let job = service.background_jobs(owner).await.unwrap().remove(0);
                service
                    .control_job(owner, job.id, JobCommand::Cancel)
                    .await
                    .unwrap();
            }
            let started = Instant::now();
            while !runner.cancelled.load(Ordering::Acquire) {
                assert!(started.elapsed() < Duration::from_secs(3));
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            runner.release.store(true, Ordering::Release);
            // Taking the job slot waits until the late worker has been rejected.
            let slot = tokio::time::timeout(
                Duration::from_secs(3),
                service.job_slot.clone().acquire_owned(),
            )
            .await
            .unwrap()
            .unwrap();
            drop(slot);
            if switch {
                service.open(root).await.unwrap();
            } // recovered queued checkpoint may resume only after this check
            assert!(
                service
                    .details(owner, asset)
                    .await
                    .unwrap()
                    .category
                    .is_empty()
            );
            assert_eq!(
                service
                    .page(owner, Query::default(), 0, 1)
                    .await
                    .unwrap()
                    .tiles[0]
                    .asset
                    .rating,
                4
            );
            service.close().await.unwrap();
            assert_eq!(
                service.queue_media_job(owner, JobKind::Thumbnails).await,
                Err(TaskError::StaleLibrary)
            );
        }
    });
}
