//! The application owns job execution; UI polling only refreshes projections.
use super::*;

pub(super) fn start(service: &Service) -> std::io::Result<()> {
    // Only weak ownership of the writer queue: dropping the last public Service
    // stops scheduling and releases the active library after in-flight work settles.
    let queue = service.queue.downgrade();
    let media_cache = service.media_cache.clone();
    let import_slots = service.import_slots.clone();
    let import_memory = service.import_memory.clone();
    let maintenance_slot = service.maintenance_slot.clone();
    let read_slots = service.read_slots.clone();
    let job_slot = service.job_slot.clone();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .build()?;
    std::thread::Builder::new()
        .name("moriq-job-scheduler".into())
        .spawn(move || {
            loop {
                let Some(queue) = queue.upgrade() else { break };
                let service = Service {
                    queue,
                    media_cache: media_cache.clone(),
                    import_slots: import_slots.clone(),
                    import_memory: import_memory.clone(),
                    maintenance_slot: maintenance_slot.clone(),
                    read_slots: read_slots.clone(),
                    job_slot: job_slot.clone(),
                };
                let result = runtime.block_on(async {
                    let owner = service
                        .call(|active| Ok(active.as_ref().map(Library::id)))
                        .await?;
                    if let Some(id) = owner {
                        service.drive_job_step(id).await?;
                    }
                    Ok::<_, String>(())
                });
                drop(service);
                let delay = if let Err(error) = result {
                    tracing::warn!(%error, "background job scheduling failed");
                    1000
                } else {
                    250
                };
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
    fn switching_library_during_budget_wait_rejects_old_result_and_recovers_checkpoint() {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let dir = tempfile::tempdir().unwrap();
            let source = dir.path().join("original.png");
            image::RgbImage::from_pixel(2, 2, image::Rgb([10, 20, 30]))
                .save(&source)
                .unwrap();
            let mut library = Library::create(dir.path(), "旧库").unwrap();
            library.import(&source, None).unwrap();
            library.enqueue_media_job("thumbnails", 0).unwrap();
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
            while service.background_jobs(id).await.unwrap()[0].state != "running" {
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
            drop(memory);
            loop {
                if service.open(root.clone()).await.is_ok() {
                    break;
                }
                assert!(started.elapsed() < Duration::from_secs(15));
                std::thread::sleep(Duration::from_millis(20));
            }
            loop {
                let jobs = service.background_jobs(id).await.unwrap();
                if jobs[0].state == "completed" {
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
            let job = library.enqueue_media_job("integrity", 0).unwrap();
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
                if jobs[0].state == "running" {
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
            loop {
                let jobs = service.background_jobs(id).await.unwrap();
                assert_eq!(jobs[0].state, "paused");
                if jobs[0].completed == 1 {
                    break;
                }
                assert!(started.elapsed() < Duration::from_secs(10));
                std::thread::sleep(Duration::from_millis(20));
            }
            let projection = service.organizer(id).await.unwrap();
            assert_eq!(projection.errors[0].0, OrganizerSection::Folders);
            service
                .control_job(id, job, JobCommand::Resume)
                .await
                .unwrap();
            loop {
                let jobs = service.background_jobs(id).await.unwrap();
                if jobs[0].state == "completed" {
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
