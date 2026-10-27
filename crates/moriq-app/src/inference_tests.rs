use super::*;
use inference::InferenceRunner;
use moriq_library::analysis::InferenceResult;
use std::time::{Duration, Instant};

#[test]
fn application_retrieval_services_share_the_frozen_project_scope() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.png");
        let near_source = dir.path().join("near.png");
        image::RgbImage::from_pixel(8, 6, image::Rgb([30, 60, 90]))
            .save(&source)
            .unwrap();
        image::RgbImage::from_pixel(8, 6, image::Rgb([31, 60, 90]))
            .save(&near_source)
            .unwrap();
        let mut library = Library::create(dir.path(), "scoped retrieval").unwrap();
        let a = library.create_project("a").unwrap();
        let b = library.create_project("b").unwrap();
        library.import(&source, Some(a)).unwrap();
        library.import(&near_source, Some(a)).unwrap();
        library.import(&source, Some(b)).unwrap();
        library.set_workspace(Some(a));
        let assets = library.page(&Query::default(), 0, 10).unwrap().assets;
        let first = assets
            .iter()
            .find(|asset| asset.name == "source.png")
            .unwrap()
            .clone();
        let near = assets
            .iter()
            .find(|asset| asset.name == "near.png")
            .unwrap()
            .clone();
        library.set_workspace(Some(b));
        let other = library
            .page(&Query::default(), 0, 10)
            .unwrap()
            .assets
            .remove(0);
        let owner = library.id();
        let root = library.root().to_path_buf();
        drop(library);
        let db = rusqlite::Connection::open(root.join("catalog.db")).unwrap();
        let vector: Vec<u8> = [1.0f32, 0.0]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect();
        for asset in [&first, &near, &other] {
            db.execute(
                "INSERT INTO ai_embeddings VALUES(?1,'chinese-clip','test-revision',?2)",
                rusqlite::params![asset.version_id.to_string(), vector],
            )
            .unwrap();
        }
        drop(db);
        let runner = FakeRunner::new(0, true);
        let service = Service::start_with_inference(runner.clone()).unwrap();
        service.open(root).await.unwrap();
        let scoped = service.for_project(Some(a));
        let semantic = scoped.semantic_search(owner, "image".into()).await.unwrap();
        assert_eq!(semantic.len(), 2);
        assert!(!semantic.contains(&other.id));
        assert_eq!(
            scoped.search_by_image(owner, first.id).await.unwrap(),
            vec![near.id]
        );
        assert_eq!(
            scoped
                .recommendations(owner, first.id, first.version_id)
                .await
                .unwrap()[0]
                .0,
            near.id
        );
        assert_eq!(
            scoped.similar_assets(owner, first.id).await.unwrap()[0].id,
            near.id
        );
        assert!(scoped.similar_assets(owner, other.id).await.is_err());
        runner.entered.store(false, Ordering::Release);
        assert!(scoped.search_by_image(owner, other.id).await.is_err());
        assert!(!runner.entered.load(Ordering::Acquire));
        assert_eq!(
            service.similar_assets(owner, first.id).await.unwrap().len(),
            2
        );
        service.close().await.unwrap();
    });
}

struct FakeRunner {
    entered: AtomicBool,
    release: AtomicBool,
    cancelled: AtomicBool,
    mode: u8,
}
impl FakeRunner {
    /// 构造当前隔离用例使用的可控替身或测试状态。
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
    /// 模拟人脸推理并暴露可控同步点，供调度与取消测试使用。
    fn qwen_faces(
        &self,
        image: &std::path::Path,
        references: &[(String, PathBuf)],
        cancel: &AtomicBool,
    ) -> TaskResult<InferenceResult> {
        assert!(image.is_file());
        self.entered.store(true, Ordering::Release);
        let start = Instant::now();
        while !self.release.load(Ordering::Acquire) {
            if cancel.load(Ordering::Acquire) {
                self.cancelled.store(true, Ordering::Release);
                return Err(TaskError::Cancelled);
            }
            assert!(start.elapsed() < Duration::from_secs(15));
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(InferenceResult {
            model: ModelId::Qwen35_4b,
            revision: "test-qwen".into(),
            data: serde_json::json!({"faces":[{"box":[0.1,0.1,0.5,0.5],"person":references.first().map(|p|p.0.as_str())}]}),
        })
    }
    /// 模拟 Qwen 推理，记录输入配置并按用例安排返回或阻塞。
    fn qwen_infer(
        &self,
        _: &str,
        image: &std::path::Path,
        profile: &AiProfile,
        cancel: &AtomicBool,
    ) -> TaskResult<InferenceResult> {
        assert!(image.is_file());
        if self.mode == 5 {
            self.entered.store(true, Ordering::Release);
            let start = Instant::now();
            while !self.release.load(Ordering::Acquire) {
                if cancel.load(Ordering::Acquire) {
                    self.cancelled.store(true, Ordering::Release);
                }
                assert!(start.elapsed() < Duration::from_secs(15));
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        if self.mode == 4 {
            let theme = profile
                .categories
                .iter()
                .find(|c| c.id == "theme_stage" && c.enabled)
                .unwrap();
            assert!(theme.definition.contains("舞台"));
            return Ok(InferenceResult {
                model: ModelId::Qwen35_4b,
                revision: "test-revision".into(),
                data: serde_json::json!({"medium":"photography","themes":[theme.id]}),
            });
        }
        Ok(InferenceResult {
            model: ModelId::Qwen35_4b,
            revision: "test-revision".into(),
            data: serde_json::json!({
                "medium": "photography",
                "themes": ["portrait", "cosplay"],
                "description": "人物照片",
                "tags": ["人像", "角色扮演"]
            }),
        })
    }
    /// 返回测试模型的固定修订，供回执身份校验使用。
    fn revision(&self, _: ModelId) -> TaskResult<String> {
        Ok("test-revision".into())
    }
    /// 模拟测试模型可用性，不启动真实运行器。
    fn ready(&self, _: ModelId) -> TaskResult<()> {
        Ok(())
    }
    /// 模拟模型推理并记录输入，供语义检索和取消验证使用。
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
/// 在独立资源库登记测试素材及分析输入。
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
/// 等待替身工作进入预期阶段，避免依赖不稳定的固定延时。
async fn wait_entered(runner: &FakeRunner) {
    let started = Instant::now();
    while !runner.entered.load(Ordering::Acquire) {
        assert!(started.elapsed() < Duration::from_secs(10));
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
/// 等待测试任务达到完成状态，并在超时后报告用例失败。
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
/// 验证人脸推理经过实际调度器，取消后不会提交迟到结果。
fn qwen_faces_run_through_the_scheduler_and_cancel_without_late_publication() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        for cancel in [false, true] {
            let (_dir, root, asset) = seed();
            let runner = FakeRunner::new(6, !cancel);
            let service = Service::start_with_inference(runner.clone()).unwrap();
            let owner = service.open(root).await.unwrap().info.id;
            service
                .queue_media_job(owner, JobKind::QwenFaces)
                .await
                .unwrap();
            if cancel {
                wait_entered(&runner).await;
                tokio::time::timeout(Duration::from_secs(2), service.rating(owner, asset, 3))
                    .await
                    .unwrap()
                    .unwrap();
                let job = service.background_jobs(owner).await.unwrap().remove(0);
                service
                    .control_job(owner, job.id, JobCommand::Cancel)
                    .await
                    .unwrap();
                let deadline = Instant::now() + Duration::from_secs(3);
                while !runner.cancelled.load(Ordering::Acquire) {
                    assert!(Instant::now() < deadline);
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                assert!(service.faces(owner, asset).await.unwrap().is_empty());
            } else {
                assert_eq!(wait_completed(&service, owner).await.failed, 0);
                assert_eq!(service.faces(owner, asset).await.unwrap().len(), 1);
                assert_eq!(service.organizer(owner).await.unwrap().people.len(), 1);
            }
            service.close().await.unwrap();
        }
    });
}
#[test]
/// 验证注入推理绑定实际修订，并支持文本及图片两种检索输入。
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
        assert_eq!(service.details(owner, asset).await.unwrap().category, "");
        assert!(service.semantic_search(owner, String::new()).await.is_err());
    });
}

#[test]
/// 验证自定义分类配置传至应用执行器，结果进入当前项目分类查询。
fn custom_profile_reaches_the_application_runner_and_current_group_filter() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let (_dir, root, asset) = seed();
        let service = Service::start_with_inference(FakeRunner::new(4, true)).unwrap();
        let owner = service.open(root).await.unwrap().info.id;
        let mut profile = service.organizer(owner).await.unwrap().ai_profile;
        profile.categories.push(AiCategory {
            id: "theme_stage".into(),
            kind: AiCategoryKind::Theme,
            label: "舞台表演".into(),
            definition: "舞台上的表演者为画面主体。".into(),
            enabled: true,
        });
        service.save_ai_profile(owner, profile).await.unwrap();
        service
            .queue_media_job(owner, JobKind::QwenClassification)
            .await
            .unwrap();
        assert_eq!(wait_completed(&service, owner).await.failed, 0);
        assert_eq!(
            service.details(owner, asset).await.unwrap().themes,
            vec!["theme_stage"]
        );
        let group = service
            .organizer(owner)
            .await
            .unwrap()
            .ai_groups
            .into_iter()
            .find(|g| g.id == "theme_stage")
            .unwrap();
        assert_eq!(group.count, 1);
        assert_eq!(
            service.page(owner, group.query, 0, 10).await.unwrap().total,
            1
        );
        service.close().await.unwrap();
    });
}
#[test]
/// 验证推理失败或身份不合法的回执会被记录，但不会产生有效分析结果。
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
/// 验证取消与切库都会阻止交互推理提交迟到结果。
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

#[test]
/// 验证 Qwen 暂停、恢复和永久停止保留进度并拒绝旧结果。
fn qwen_pause_resume_and_permanent_stop_reject_late_outputs_without_losing_progress() {
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let (_dir, root, asset) = seed();
        let runner = FakeRunner::new(5, false);
        let service = Service::start_with_inference(runner.clone()).unwrap();
        let owner = service.open(root).await.unwrap().info.id;
        service
            .queue_media_job(owner, JobKind::QwenClassification)
            .await
            .unwrap();
        wait_entered(&runner).await;
        service
            .control_model_jobs(owner, JobCommand::Pause)
            .await
            .unwrap();
        let start = Instant::now();
        while !runner.cancelled.load(Ordering::Acquire) {
            assert!(start.elapsed() < Duration::from_secs(2));
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        runner.release.store(true, Ordering::Release);
        tokio::time::sleep(Duration::from_millis(100)).await;
        let job = service.background_jobs(owner).await.unwrap().remove(0);
        assert_eq!(job.state, JobState::Paused);
        assert_eq!(job.completed, 0);
        assert_eq!(job.failed, 0);
        assert!(
            service
                .details(owner, asset)
                .await
                .unwrap()
                .medium
                .is_empty()
        );
        service
            .control_model_jobs(owner, JobCommand::Resume)
            .await
            .unwrap();
        assert_eq!(wait_completed(&service, owner).await.completed, 1);
        runner.release.store(false, Ordering::Release);
        runner.entered.store(false, Ordering::Release);
        runner.cancelled.store(false, Ordering::Release);
        service
            .queue_media_job(owner, JobKind::QwenDescription)
            .await
            .unwrap();
        wait_entered(&runner).await;
        service
            .control_model_jobs(owner, JobCommand::Cancel)
            .await
            .unwrap();
        runner.release.store(true, Ordering::Release);
        tokio::time::sleep(Duration::from_millis(100)).await;
        let job = service
            .background_jobs(owner)
            .await
            .unwrap()
            .into_iter()
            .find(|j| j.kind == JobKind::QwenDescription)
            .unwrap();
        assert_eq!(job.state, JobState::Cancelled);
        assert_eq!(job.completed, 0);
        assert!(
            service
                .details(owner, asset)
                .await
                .unwrap()
                .ai_caption
                .is_empty()
        );
        service
            .control_model_jobs(owner, JobCommand::Resume)
            .await
            .unwrap();
        assert_eq!(
            service
                .background_jobs(owner)
                .await
                .unwrap()
                .into_iter()
                .find(|j| j.kind == JobKind::QwenDescription)
                .unwrap()
                .state,
            JobState::Cancelled
        );
        service.close().await.unwrap();
    });
}
