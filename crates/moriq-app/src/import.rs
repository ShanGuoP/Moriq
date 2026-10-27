//! Streaming import and source recovery.
use super::*;

impl Service {
    /// 分页查找旧根目录引用，在文件线程验证新位置后提交重连结果。
    pub async fn reconnect_root(
        &self,
        id: LibraryId,
        old: PathBuf,
        new: PathBuf,
    ) -> Result<(u64, u64)> {
        let mut cursor = String::new();
        let mut connected = 0;
        let mut skipped = 0;
        loop {
            let old = old.clone();
            let new = new.clone();
            let after = cursor.clone();
            let (next, candidates) = self
                .call(move |active| {
                    check(active, id)?
                        .reconnect_page(&old, &new, &after)
                        .map_err(|e| e.to_string())
                })
                .await?;
            if next == cursor {
                break;
            }
            cursor = next;
            for candidate in candidates {
                let checked = tokio::task::spawn_blocking(move || candidate.verify())
                    .await
                    .map_err(|e| e.to_string())?;
                match checked {
                    Ok(candidate) => {
                        if self
                            .call(move |active| {
                                check(active, id)?
                                    .apply_reconnect(candidate)
                                    .map_err(|e| e.to_string())
                            })
                            .await?
                        {
                            connected += 1
                        } else {
                            skipped += 1
                        }
                    }
                    Err(_) => skipped += 1,
                }
            }
        }
        Ok((connected, skipped))
    }
    /// 执行资源库恢复入口，并返回更新后的状态。
    pub async fn recover(&self, id: LibraryId) -> Result<ImportReport> {
        self.recover_controlled(id, ImportControl::default()).await
    }

    pub async fn recover_controlled(
        &self,
        id: LibraryId,
        control: ImportControl,
    ) -> Result<ImportReport> {
        let mut stop = RecoveryDrop(Some(control.cancel.clone()));
        self.job_cancel
            .register_flag(id, "recovery".into(), control.cancel.clone());
        let plans = self
            .call(move |active| {
                check(active, id)?
                    .recovery_plans()
                    .map_err(|e| e.to_string())
            })
            .await?;
        control.discovered.store(plans.len(), Ordering::Release);
        let mut report = ImportReport::default();
        for plan in plans {
            if control.cancel.load(Ordering::Acquire) {
                break;
            }
            let label = plan.label();
            let result = async {
                let slot = self
                    .recovery_admit(self.import_slots.clone(), 1, &control.cancel)
                    .await?;
                let probe_memory = self
                    .recovery_admit(self.import_memory.clone(), 256, &control.cancel)
                    .await?;
                let cancel = control.cancel.clone();
                let (work, weight) = tokio::task::spawn_blocking(move || {
                    let _probe = probe_memory;
                    let work = plan.freeze(cancel).map_err(|e| e.to_string())?;
                    let weight = work.memory_mib().map_err(|e| e.to_string())?;
                    Ok::<_, String>((work, weight))
                })
                .await
                .map_err(|e| e.to_string())??;
                if weight > control.memory_mib.clamp(256, 4096) {
                    return Err(format!("恢复媒体预计需要 {weight} MiB，请提高导入内存预算"));
                }
                control
                    .largest_mib
                    .fetch_max(weight as usize, Ordering::Relaxed);
                control.waiting_memory.fetch_add(1, Ordering::Relaxed);
                let waiting = ImportActivity(control.waiting_memory.clone());
                let memory = self
                    .recovery_admit(self.import_memory.clone(), weight, &control.cancel)
                    .await?;
                drop(waiting);
                let cancel = control.cancel.clone();
                let prepared = tokio::task::spawn_blocking(move || {
                    let _admission = (slot, memory);
                    work.prepare(cancel).map_err(|e| e.to_string())
                })
                .await
                .map_err(|e| e.to_string())??;
                let cancel = control.cancel.clone();
                self.call(move |active| {
                    if cancel.load(Ordering::Acquire) {
                        return Err("恢复已停止".into());
                    }
                    check(active, id)?
                        .finish_recovery(prepared)
                        .map_err(|e| e.to_string())
                })
                .await
            }
            .await;
            if control.cancel.load(Ordering::Acquire) {
                break;
            }
            match result {
                Ok(true) => report.imported += 1,
                Ok(false) => report.duplicates += 1,
                Err(error) => {
                    report.failed += 1;
                    report.failures.push(format!("{label}：{error}"));
                }
            }
            control.finished.fetch_add(1, Ordering::Release);
        }
        report.cancelled = control.cancel.load(Ordering::Acquire);
        stop.0 = None;
        Ok(report)
    }

    async fn recovery_admit(
        &self,
        semaphore: Arc<Semaphore>,
        count: u32,
        cancel: &AtomicBool,
    ) -> Result<tokio::sync::OwnedSemaphorePermit> {
        let mut waiting = Box::pin(semaphore.acquire_many_owned(count));
        loop {
            if cancel.load(Ordering::Acquire) {
                return Err("恢复已停止".into());
            }
            if let Ok(result) =
                tokio::time::timeout(std::time::Duration::from_millis(25), &mut waiting).await
            {
                return result.map_err(|e| e.to_string());
            }
        }
    }
    /// 校验目标文件内容后提交外部原片的新位置。
    pub async fn relocate_reference(
        &self,
        id: LibraryId,
        asset: AssetId,
        version: AssetVersionId,
        path: PathBuf,
    ) -> Result<()> {
        let hash = self
            .call(move |active| {
                check(active, id)?
                    .reference_hash(asset, version)
                    .map_err(|e| e.to_string())
            })
            .await?;
        let expected = hash.clone();
        let path = tokio::task::spawn_blocking(move || {
            moriq_library::validate_reference(&path, &expected).map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())??;
        self.call(move |active| {
            check(active, id)?
                .relocate_reference(asset, version, &path, &hash)
                .map_err(|e| e.to_string())
        })
        .await
    }
    /// 建立有界文件扫描流，输出文件及相对目录信息。
    pub async fn scan(&self, id: LibraryId, paths: Vec<PathBuf>) -> Result<Vec<ImportFile>> {
        let root = self
            .call(move |active| Ok(check(active, id)?.root().to_path_buf()))
            .await?;
        tokio::task::spawn_blocking(move || {
            let mut files = Vec::new();
            let mut failure = None;
            scan::walk(&root, paths, &AtomicBool::new(false), |file| {
                match file {
                    Ok(file) => files.push(file),
                    Err(e) => {
                        failure = Some(e);
                        return false;
                    }
                }
                true
            });
            if let Some(error) = failure {
                return Err(error);
            }
            files.sort_by(|a, b| a.path.cmp(&b.path));
            files.dedup_by(|a, b| a.path == b.path);
            Ok(files)
        })
        .await
        .map_err(|e| e.to_string())?
    }
    /// UI path: starts importing as traversal runs, with a bounded discovery channel.
    ///
    /// 扫描多个来源并发导入，汇总新增、重复、失败和取消结果。
    pub async fn import_sources(
        &self,
        id: LibraryId,
        paths: Vec<PathBuf>,
        project: Option<ProjectId>,
        control: ImportControl,
    ) -> ImportReport {
        let root = match self
            .call(move |active| Ok(check(active, id)?.root().to_path_buf()))
            .await
        {
            Ok(root) => root,
            Err(e) => {
                return ImportReport {
                    failed: 1,
                    failures: vec![e],
                    ..Default::default()
                };
            }
        };
        control.scanning.store(true, Ordering::Relaxed);
        let scan_control = control.clone();
        let (send, receive) = mpsc::channel(128);
        let scanner = tokio::task::spawn_blocking(move || {
            scan::walk(&root, paths, &scan_control.cancel, |file| {
                if scan_control.cancel.load(Ordering::Relaxed) {
                    return false;
                }
                scan_control.discovered.fetch_add(1, Ordering::Relaxed);
                send.blocking_send(file).is_ok()
            });
            scan_control.scanning.store(false, Ordering::Relaxed);
        });
        let files = stream::unfold(receive, |mut receiver| async move {
            receiver.recv().await.map(|value| (value, receiver))
        });
        let mut report = self
            .import_stream(id, Box::pin(files), project, control.clone())
            .await;
        if let Err(error) = scanner.await {
            report.failed += 1;
            if report.failures.len() < 100 {
                report.failures.push(format!("目录扫描失败：{error}"));
            }
        }
        control.scanning.store(false, Ordering::Relaxed);
        report
    }
    /// 通过统一导入流程处理指定来源，沿用并发与内存控制。
    pub async fn import(
        &self,
        library: LibraryId,
        paths: Vec<ImportFile>,
        project: Option<ProjectId>,
        control: ImportControl,
    ) -> ImportReport {
        control.discovered.store(paths.len(), Ordering::Relaxed);
        self.import_stream(
            library,
            stream::iter(paths.into_iter().map(Ok)),
            project,
            control,
        )
        .await
    }
    /// 消费扫描流并调度有界导入工作；停止后等待已启动工作收尾。
    async fn import_stream<S: Stream<Item = Result<ImportFile>> + Unpin>(
        &self,
        library: LibraryId,
        files: S,
        project: Option<ProjectId>,
        control: ImportControl,
    ) -> ImportReport {
        let memory = Arc::new(Semaphore::new(control.memory_mib.clamp(256, 4096) as usize));
        let mut report = ImportReport::default();
        // Stream traversal through bounded media pipelines; drain active work on cancellation.
        let mut pending = files
            .map(|path| {
                let service = self.clone();
                let control = control.clone();
                let memory = memory.clone();
                async move {
                    let path = match path {
                        Ok(path) => path,
                        Err(error) => return ("目录扫描".into(), Some(Err(error))),
                    };
                    let label = path
                        .path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    if control.cancel.load(Ordering::Relaxed) {
                        return (label, None);
                    }
                    let result = service
                        .import_one(library, path, project, &control, memory)
                        .await;
                    (label, Some(result))
                }
            })
            .buffer_unordered(control.workers.clamp(1, 32));
        while let Some((label, result)) = pending.next().await {
            let Some(result) = result else {
                continue;
            };
            match result {
                Ok(true) => report.imported += 1,
                Ok(false) => report.duplicates += 1,
                Err(_) if control.cancel.load(Ordering::Relaxed) => {}
                Err(error) => {
                    report.failed += 1;
                    tracing::warn!(%error,"import failed");
                    if report.failures.len() < 100 {
                        report.failures.push(format!("{label}：{error}"));
                    }
                }
            }
            control.finished.fetch_add(1, Ordering::Relaxed);
        }
        report.cancelled = control.cancel.load(Ordering::Relaxed);
        report
    }
    /// 领取导入票据、等待媒体预算、准备像素，再验证库身份并提交结果。
    async fn import_one(
        &self,
        library: LibraryId,
        path: ImportFile,
        project: Option<ProjectId>,
        control: &ImportControl,
        memory: Arc<Semaphore>,
    ) -> Result<bool> {
        let slot = Arc::new(
            self.import_slots
                .clone()
                .acquire_owned()
                .await
                .map_err(|e| e.to_string())?,
        );
        if control.cancel.load(Ordering::Relaxed) {
            return Err("导入已停止".into());
        }
        let referenced = control.referenced;
        let ticket = self
            .call(move |active| {
                check(active, library)?
                    .begin_import(&path, project, referenced)
                    .map_err(|e| e.to_string())
            })
            .await?;
        let work = ticket.clone();
        let cancel = control.cancel.clone();
        let copy_slot = slot.clone();
        let result = async {
            tokio::task::spawn_blocking(move || {
                let _slot = copy_slot;
                work.copy(&cancel)
            })
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
            // Metadata-only RAW probing is isolated at 192 MiB plus parent workspace.
            // Admit probes too; a batch of headers must not bypass the memory budget.
            let probe_batch = memory
                .clone()
                .acquire_many_owned(256)
                .await
                .map_err(|e| e.to_string())?;
            let probe_global = self
                .import_memory
                .clone()
                .acquire_many_owned(256)
                .await
                .map_err(|e| e.to_string())?;
            let work = ticket.clone();
            let weight = tokio::task::spawn_blocking(move || {
                let _batch = probe_batch;
                let _global = probe_global;
                work.memory_mib()
            })
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
            if weight > control.memory_mib.clamp(256, 4096) {
                return Err(format!(
                    "单张图片解码预计需要 {weight} MiB，请在设置中提高导入内存预算"
                ));
            }
            control
                .largest_mib
                .fetch_max(weight as usize, Ordering::Relaxed);
            control.waiting_memory.fetch_add(1, Ordering::Relaxed);
            let waiting = ImportActivity(control.waiting_memory.clone());
            let _batch_memory = memory
                .acquire_many_owned(weight)
                .await
                .map_err(|e| e.to_string())?;
            let _memory = self
                .import_memory
                .clone()
                .acquire_many_owned(weight)
                .await
                .map_err(|e| e.to_string())?;
            drop(waiting);
            let work = ticket.clone();
            let cancel = control.cancel.clone();
            let active = control.active.clone();
            let peak = control.peak.clone();
            let decode_slot = slot.clone();
            let prepared = tokio::task::spawn_blocking(move || {
                let _slot = decode_slot;
                let _memory = _memory;
                let _batch_memory = _batch_memory;
                let current = active.fetch_add(1, Ordering::Relaxed) + 1;
                let _activity = ImportActivity(active);
                peak.fetch_max(current, Ordering::Relaxed);
                work.prepare(&cancel)
            })
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
            // Decode admission is released before queuing the short writer transaction.
            if control.cancel.load(Ordering::Relaxed) {
                return Err("导入已停止".into());
            }
            let work = ticket.clone();
            self.call(move |active| {
                check(active, library)?
                    .finish_import(work, prepared)
                    .map_err(|e| e.to_string())
            })
            .await
        }
        .await;
        if result.is_err() {
            // A library switch may reject this callback; its durable prepared journal remains recoverable.
            let _ = self
                .call(move |active| {
                    check(active, library)?
                        .interrupt_import(&ticket)
                        .map_err(|e| e.to_string())
                })
                .await;
        }
        result
    }
}

struct RecoveryDrop(Option<Arc<AtomicBool>>);
impl Drop for RecoveryDrop {
    fn drop(&mut self) {
        if let Some(cancel) = &self.0 {
            cancel.store(true, Ordering::Release);
        }
    }
}
