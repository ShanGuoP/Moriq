//! The application owns job execution; UI polling only refreshes projections.
use super::*;

#[derive(Default)]
pub(super) struct Cancellation(
    std::sync::Mutex<Vec<(LibraryId, String, std::sync::Weak<AtomicBool>)>>,
);
impl Cancellation {
    /// 为指定库与任务登记取消标志，清除已释放令牌的记录。
    pub fn register(&self, owner: LibraryId, job: String) -> Arc<AtomicBool> {
        let flag = Arc::new(AtomicBool::new(false));
        let mut tasks = self.0.lock().unwrap_or_else(|e| e.into_inner());
        tasks.retain(|(_, _, flag)| flag.strong_count() > 0);
        tasks.push((owner, job, Arc::downgrade(&flag)));
        flag
    }
    /// 向指定任务或全部在途任务发送取消，保留仍被工作线程持有的令牌。
    pub fn stop(&self, target: Option<(LibraryId, &str)>) {
        let mut tasks = self.0.lock().unwrap_or_else(|e| e.into_inner());
        tasks.retain(|(owner, job, flag)| {
            if let Some(flag) = flag.upgrade() {
                if target.is_none_or(|(id, name)| id == *owner && name == job) {
                    flag.store(true, Ordering::Release);
                }
                true
            } else {
                false
            }
        });
    }
}

/// 启动独立调度线程，以弱队列所有权驱动任务；最后一个公开服务释放后停止。
pub(super) fn start(service: &Service) -> std::io::Result<()> {
    // Only weak ownership of the writer queue: dropping the last public Service
    // stops scheduling and releases the active library after in-flight work settles.
    let queue = service.queue.downgrade();
    let media_cache = service.media_cache.clone();
    let inference = service.inference.clone();
    let import_slots = service.import_slots.clone();
    let import_memory = service.import_memory.clone();
    let maintenance_slot = service.maintenance_slot.clone();
    let read_slots = service.read_slots.clone();
    let job_slot = service.job_slot.clone();
    let job_cancel = service.job_cancel.clone();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_time()
        .build()?;
    std::thread::Builder::new()
        .name("moriq-job-scheduler".into())
        .spawn(move || {
            let mut failures = 0u32;
            loop {
                let Some(queue) = queue.upgrade() else { break };
                let service = Service {
                    workspace: None,
                    queue,
                    media_cache: media_cache.clone(),
                    inference: inference.clone(),
                    import_slots: import_slots.clone(),
                    import_memory: import_memory.clone(),
                    maintenance_slot: maintenance_slot.clone(),
                    read_slots: read_slots.clone(),
                    job_slot: job_slot.clone(),
                    job_cancel: job_cancel.clone(),
                };
                let result = runtime.block_on(async {
                    let owner = service
                        .call(|active| Ok(active.as_ref().map(Library::id)))
                        .await
                        .map_err(|e| (None, TaskError::Storage(e)))?;
                    if let Some(id) = owner {
                        service
                            .drive_job_step(id)
                            .await
                            .map_err(|e| (Some(id), e))?;
                    }
                    Ok::<_, (Option<LibraryId>, TaskError)>(())
                });
                let delay = if matches!(
                    result,
                    Err((_, TaskError::Cancelled | TaskError::StaleLibrary))
                ) {
                    failures = 0;
                    250
                } else if let Err((owner, error)) = result {
                    failures = failures.saturating_add(1);
                    tracing::warn!(%error, "background job scheduling failed");
                    if failures >= 5
                        && let Some(owner) = owner
                    {
                        let detail = format!("后台任务连续失败，已暂停。请检查后重试：{error}");
                        let _ = runtime.block_on(service.call(move |active| {
                            check(active, owner)?
                                .pause_jobs_after_error(&detail)
                                .map_err(|e| e.to_string())
                        }));
                    }
                    (1000u64 << failures.min(5).saturating_sub(1)).min(30000)
                } else {
                    failures = 0;
                    250
                };
                drop(service);
                std::thread::sleep(std::time::Duration::from_millis(delay));
            }
        })?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    /// 验证并行登记的取消令牌不会互相覆盖，单项取消仅影响目标任务。
    fn cancellation_keeps_multiple_tasks_and_targets_only_the_requested_job() {
        let cancellation = Cancellation::default();
        let owner = LibraryId::default();
        let other = LibraryId::default();
        let job = cancellation.register(owner, "job".into());
        let interactive = cancellation.register(owner, "interactive".into());
        let foreign = cancellation.register(other, "job".into());
        cancellation.stop(Some((owner, "job")));
        assert!(job.load(Ordering::Acquire));
        assert!(!interactive.load(Ordering::Acquire));
        assert!(!foreign.load(Ordering::Acquire));
        drop(job);
        cancellation.stop(None);
        assert!(interactive.load(Ordering::Acquire));
        assert!(foreign.load(Ordering::Acquire));
        drop(interactive);
        drop(foreign);
        cancellation.stop(None);
        assert!(cancellation.0.lock().unwrap().is_empty());
    }
    #[test]
    /// 验证等待预算时切库会拒绝旧结果，并保留可恢复任务检查点。
    fn switching_library_during_budget_wait_rejects_old_result_and_recovers_checkpoint() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let dir = tempfile::tempdir().unwrap();
            let source = dir.path().join("original.png");
            image::RgbImage::from_pixel(2, 2, image::Rgb([10, 20, 30]))
                .save(&source)
                .unwrap();
            let mut library = Library::create(dir.path(), "旧库").unwrap();
            library.import(&source, None).unwrap();
            library.enqueue_media_job(JobKind::Thumbnails, 0).unwrap();
            let root = library.root().to_path_buf();
            let id = library.id();
            drop(library);
            let service = Service::start().unwrap();
            let memory = service
                .import_memory
                .clone()
                .acquire_many_owned(4096)
                .await
                .unwrap();
            service.open(root.clone()).await.unwrap();
            let started = Instant::now();
            while service.background_jobs(id).await.unwrap()[0].state != JobState::Running {
                assert!(started.elapsed() < Duration::from_secs(15));
                std::thread::sleep(Duration::from_millis(20));
            }
            let other = service
                .create(dir.path().into(), "新库".into())
                .await
                .unwrap();
            assert!(
                service
                    .background_jobs(other.info.id)
                    .await
                    .unwrap()
                    .is_empty()
            );
            assert!(service.background_jobs(id).await.is_err());
            // Reopening must succeed even while the old memory budget is held.
            service.open(root.clone()).await.unwrap();
            drop(memory);
            loop {
                let jobs = service.background_jobs(id).await.unwrap();
                if jobs[0].state == JobState::Completed {
                    assert_eq!((jobs[0].completed, jobs[0].failed), (1, 0));
                    break;
                }
                assert!(started.elapsed() < Duration::from_secs(15));
                std::thread::sleep(Duration::from_millis(20));
            }
            service.close().await.unwrap();
            let other = service.open(other.root).await.unwrap();
            assert_eq!(other.info.count, 0);
            assert!(
                service
                    .background_jobs(other.info.id)
                    .await
                    .unwrap()
                    .is_empty()
            );
            service.close().await.unwrap();
        });
    }

    #[test]
    /// 验证任务独立于界面投影执行，服务释放后调度器停止。
    fn jobs_run_without_ui_despite_projection_errors_and_stop_with_the_service() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let dir = tempfile::tempdir().unwrap();
            let mut library = Library::create(dir.path(), "自主调度").unwrap();
            for n in 0..3 {
                let source = dir.path().join(format!("{n}.png"));
                image::RgbImage::from_pixel(2, 2, image::Rgb([n, 2, 3]))
                    .save(source.clone())
                    .unwrap();
                library.import(&source, None).unwrap();
            }
            let job = library.enqueue_media_job(JobKind::Integrity, 0).unwrap();
            let root = library.root().to_path_buf();
            drop(library);
            let db = rusqlite::Connection::open(root.join("catalog.db")).unwrap();
            db.execute_batch("ALTER TABLE virtual_folders RENAME TO unavailable_folders")
                .unwrap();
            let service = Service::start().unwrap();
            let memory = service
                .import_memory
                .clone()
                .acquire_many_owned(4096)
                .await
                .unwrap();
            let opened = service.open(root.clone()).await.unwrap();
            let id = opened.info.id;
            let started = Instant::now();
            loop {
                let jobs = service.background_jobs(id).await.unwrap();
                if jobs[0].state == JobState::Running {
                    break;
                }
                assert!(started.elapsed() < Duration::from_secs(10));
                std::thread::sleep(Duration::from_millis(20));
            }
            service
                .control_job(id, job.clone(), JobCommand::Pause)
                .await
                .unwrap();
            drop(memory);
            while service.job_slot.available_permits() == 0 {
                assert!(started.elapsed() < Duration::from_secs(10));
                std::thread::sleep(Duration::from_millis(20));
            }
            let jobs = service.background_jobs(id).await.unwrap();
            assert_eq!((jobs[0].state.as_str(), jobs[0].completed), ("paused", 0));
            let projection = service.organizer(id).await.unwrap();
            assert_eq!(projection.errors[0].0, OrganizerSection::Folders);
            service
                .control_job(id, job, JobCommand::Resume)
                .await
                .unwrap();
            loop {
                let jobs = service.background_jobs(id).await.unwrap();
                if jobs[0].state == JobState::Completed {
                    assert_eq!((jobs[0].completed, jobs[0].failed), (3, 0));
                    break;
                }
                assert!(started.elapsed() < Duration::from_secs(10));
                std::thread::sleep(Duration::from_millis(20));
            }
            drop(service);
            loop {
                if let Ok(reopened) = Library::open(&root) {
                    assert_eq!(reopened.id(), id);
                    break;
                }
                assert!(started.elapsed() < Duration::from_secs(10));
                std::thread::sleep(Duration::from_millis(20));
            }
        });
    }
}
