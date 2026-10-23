//! Library snapshots, maintenance and storage operations.
use super::*;

impl Service {
    pub async fn orphan_plan(&self, id: LibraryId) -> Result<Arc<OrphanPlan>> {
        self.read_plan(
            128,
            move |active| Ok(check(active, id)?.orphan_scan()),
            |scan| scan.run().map(Arc::new).map_err(|e| e.to_string()),
        )
        .await
    }
    pub async fn quarantine_orphans(
        &self,
        id: LibraryId,
        plan: Arc<OrphanPlan>,
    ) -> Result<OrphanReport> {
        let mut report = OrphanReport::default();
        let mut cursor = String::new();
        loop {
            let source = plan.clone();
            let after = cursor.clone();
            let candidates =
                tokio::task::spawn_blocking(move || source.page(&after).map_err(|e| e.to_string()))
                    .await
                    .map_err(|e| e.to_string())??;
            let Some(last) = candidates.last() else { break };
            cursor = last.cursor().to_string();
            let source = plan.clone();
            let current = self
                .call(move |active| {
                    check(active, id)?
                        .quarantine_orphans(&source, candidates)
                        .map_err(|e| e.to_string())
                })
                .await?;
            report.moved += current.moved;
            report.bytes += current.bytes;
            report.skipped += current.skipped;
        }
        Ok(report)
    }
    pub async fn restore_orphans(&self, id: LibraryId, purge: bool) -> Result<OrphanReport> {
        let mut report = OrphanReport::default();
        let mut cursor = String::new();
        loop {
            let after = cursor.clone();
            let (next, current) = self
                .call(move |active| {
                    check(active, id)?
                        .restore_orphans_page(purge, &after)
                        .map_err(|e| e.to_string())
                })
                .await?;
            report.moved += current.moved;
            report.bytes += current.bytes;
            report.skipped += current.skipped;
            if next == cursor {
                break;
            }
            cursor = next;
        }
        Ok(report)
    }
    pub async fn trim_cache(&self, id: LibraryId, clear: bool) -> Result<CacheReport> {
        self.media_cache.clear();
        self.read_plan(
            128,
            move |active| Ok(check(active, id)?.cache_plan()),
            move |plan| {
                plan.trim(if clear { 0 } else { 4 * 1024 * 1024 * 1024 })
                    .map_err(|e| e.to_string())
            },
        )
        .await
    }
    pub async fn trash_snapshot(&self, id: LibraryId) -> Result<Vec<AssetId>> {
        self.call(move |active| {
            check(active, id)?
                .trash_snapshot()
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn empty_trash(&self, id: LibraryId, assets: Vec<AssetId>) -> Result<PurgeReport> {
        let mut report = PurgeReport::default();
        for chunk in assets.chunks(1000) {
            let ids = chunk.to_vec();
            let current = self
                .call(move |active| {
                    check(active, id)?
                        .purge_trash_chunk(&ids)
                        .map_err(|e| e.to_string())
                })
                .await?;
            report.deleted += current.deleted;
            report.cleanup_pending |= current.cleanup_pending;
        }
        Ok(report)
    }
    pub async fn clone_library(&self, id: LibraryId, parent: PathBuf) -> Result<PathBuf> {
        self.copy_library(id, parent, true, BackupControl::default())
            .await
    }
    pub async fn backup(&self, id: LibraryId, parent: PathBuf) -> Result<PathBuf> {
        self.copy_library(id, parent, false, BackupControl::default())
            .await
    }
    pub async fn copy_library(
        &self,
        id: LibraryId,
        parent: PathBuf,
        independent: bool,
        control: BackupControl,
    ) -> Result<PathBuf> {
        let freeze_control = control.clone();
        let plan = self
            .call(move |active| {
                check(active, id)?
                    .backup_plan_controlled(&parent, independent, &freeze_control)
                    .map_err(|e| e.to_string())
            })
            .await?;
        let prepared = tokio::task::spawn_blocking(move || plan.copy(&control))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        self.call(move |active| {
            check(active, id)?;
            prepared.publish().map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn maintenance(&self, id: LibraryId, repair: bool) -> Result<MaintenanceReport> {
        self.maintenance_controlled(id, repair, MaintenanceControl::default())
            .await
    }
    pub async fn purge(&self, id: LibraryId, ids: Vec<AssetId>) -> Result<PurgeReport> {
        self.call(move |active| {
            check(active, id)?
                .purge(&ids)
                .map_err(|error| error.to_string())
        })
        .await
    }
    pub async fn maintenance_controlled(
        &self,
        id: LibraryId,
        repair: bool,
        control: MaintenanceControl,
    ) -> Result<MaintenanceReport> {
        let _slot = self
            .maintenance_slot
            .clone()
            .try_acquire_owned()
            .map_err(|_| "已有检查任务正在运行".to_string())?;
        let plan = self
            .call(move |active| Ok(check(active, id)?.maintenance_plan()))
            .await?;
        // A single maintenance decoder shares the global media budget with import workers.
        let memory = self.import_memory.clone();
        let worker_control = control.clone();
        let (sender, mut receiver) = mpsc::channel(2);
        let worker = tokio::task::spawn_blocking(move || {
            let _budget = loop {
                if worker_control.cancel.load(Ordering::Relaxed) {
                    return Ok::<(), String>(());
                }
                if let Ok(permit) = memory.clone().try_acquire_many_owned(1024) {
                    break permit;
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            };
            plan.run(repair, &worker_control, |outcome| {
                sender.blocking_send(outcome).is_ok()
            })
            .map_err(|error| error.to_string())
        });
        let mut report = MaintenanceReport::default();
        let mut failure = None;
        while let Some(outcome) = receiver.recv().await {
            if control.cancel.load(Ordering::Relaxed) {
                continue;
            }
            if let Some(repair) = outcome.record(&mut report) {
                match self
                    .call(move |active| {
                        check(active, id)?
                            .apply_cache_repair(repair)
                            .map_err(|e| e.to_string())
                    })
                    .await
                {
                    Ok(applied) => report.repaired += u64::from(applied),
                    Err(error) => {
                        failure = Some(error);
                        control.cancel.store(true, Ordering::Relaxed);
                        break;
                    }
                }
            }
        }
        drop(receiver);
        worker.await.map_err(|error| error.to_string())??;
        // Check the library identity even when the last item required no DB mutation.
        self.call(move |active| check(active, id).map(|_| ()))
            .await?;
        if let Some(error) = failure {
            return Err(error);
        }
        report.cancelled = control.cancel.load(Ordering::Relaxed);
        Ok(report)
    }
    pub async fn storage(&self, id: LibraryId, control: StorageControl) -> Result<StorageStats> {
        self.read_plan(
            128,
            move |active| check(active, id)?.storage_plan().map_err(|e| e.to_string()),
            move |plan| plan.scan(&control).map_err(|e| e.to_string()),
        )
        .await
    }
    pub async fn volume(&self, id: LibraryId) -> Result<(u64, u64)> {
        self.read_plan(
            1,
            move |active| check(active, id)?.storage_plan().map_err(|e| e.to_string()),
            move |plan| plan.volume().map_err(|e| e.to_string()),
        )
        .await
    }
}
