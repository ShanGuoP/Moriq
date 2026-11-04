use super::*;

impl Service {
    /// Probe an SMB directory away from the SQLite writer, then approve its canonical root.
    pub async fn connect_nas(&self, library: LibraryId, path: PathBuf) -> Result<NasSource> {
        let root = tokio::task::spawn_blocking(move || {
            if !moriq_library::nas::is_unc(&path) {
                return Err("请输入 Windows SMB 共享路径，例如 \\\\服务器\\照片".to_string());
            }
            let root = path
                .canonicalize()
                .map_err(|e| format!("无法连接 NAS：{e}"))?;
            if !moriq_library::nas::is_unc(&root) || !root.is_dir() {
                return Err("NAS 地址不是可读取的 SMB 目录".into());
            }
            std::fs::read_dir(&root)
                .and_then(|mut entries| entries.next().transpose().map(|_| ()))
                .map_err(|e| format!("无法列出 NAS 目录：{e}"))?;
            Ok(root)
        })
        .await
        .map_err(|e| e.to_string())??;
        self.call(move |active| {
            check(active, library)?
                .register_nas_source(&root)
                .map_err(|e| e.to_string())
        })
        .await
    }

    pub async fn nas_sources(&self, library: LibraryId) -> Result<Vec<NasSource>> {
        self.call(move |active| {
            check(active, library)?
                .nas_sources()
                .map_err(|e| e.to_string())
        })
        .await
    }

    pub async fn import_nas_source(
        &self,
        library: LibraryId,
        source_id: String,
        path: PathBuf,
        project: Option<ProjectId>,
        mut control: ImportControl,
    ) -> ImportReport {
        control.workers = control.workers.min(2);
        let sources = self.nas_sources(library).await;
        let result = match sources {
            Ok(sources) => match sources.into_iter().find(|source| source.id == source_id) {
                Some(source) => tokio::task::spawn_blocking(move || {
                    let selected = path.canonicalize().map_err(|e| e.to_string())?;
                    if !selected.starts_with(&source.path) {
                        return Err("所选目录不在已连接的 NAS 来源内".to_string());
                    }
                    Ok(selected)
                })
                .await
                .map_err(|e| e.to_string())
                .and_then(|result| result),
                None => Err("NAS 来源已不存在，请重新连接".into()),
            },
            Err(error) => Err(error),
        };
        match result {
            Ok(selected) => {
                self.import_sources(library, vec![selected], project, control)
                    .await
            }
            Err(error) => ImportReport {
                failed: 1,
                failures: vec![error],
                ..Default::default()
            },
        }
    }

    /// Convert selected remote references in place, preserving asset and version identities.
    pub async fn download_nas(
        &self,
        library: LibraryId,
        assets: Vec<AssetId>,
        control: ImportControl,
    ) -> ImportReport {
        let mut report = ImportReport::default();
        for asset in assets {
            if control.cancel.load(Ordering::Relaxed) {
                break;
            }
            let plans = self
                .call(move |active| {
                    check(active, library)?
                        .nas_download_plans(asset)
                        .map_err(|e| e.to_string())
                })
                .await;
            let result = match plans {
                Err(error) => Err(error),
                Ok(plans) if plans.is_empty() => Err("所选素材没有 NAS 原片".into()),
                Ok(plans) => {
                    let mut result = Ok(());
                    for plan in plans {
                        result = self.download_nas_version(library, plan, &control).await;
                        if result.is_err() {
                            break;
                        }
                    }
                    result
                }
            };
            match result {
                Ok(()) => report.imported += 1,
                Err(error) if !control.cancel.load(Ordering::Relaxed) => {
                    report.failed += 1;
                    if report.failures.len() < 100 {
                        report.failures.push(format!("{asset}：{error}"));
                    }
                }
                Err(_) => {}
            }
            control.finished.fetch_add(1, Ordering::Relaxed);
        }
        report.cancelled = control.cancel.load(Ordering::Relaxed);
        report
    }

    async fn download_nas_version(
        &self,
        library: LibraryId,
        plan: moriq_library::nas::NasDownload,
        control: &ImportControl,
    ) -> Result<()> {
        if control.cancel.load(Ordering::Relaxed) {
            return Err("下载已取消".into());
        }
        let cancel = control.cancel.clone();
        let (plan, copied) = tokio::task::spawn_blocking(move || {
            let result = plan.copy(&cancel).map_err(|e| e.to_string());
            (plan, result)
        })
        .await
        .map_err(|e| e.to_string())?;
        copied?;
        let cancel = control.cancel.clone();
        self.call(move |active| {
            let result = if cancel.load(Ordering::Relaxed) {
                Err("下载已取消".into())
            } else {
                check(active, library)
                    .and_then(|lib| lib.finish_nas_download(&plan).map_err(|e| e.to_string()))
            };
            if result.is_err() {
                plan.discard();
            }
            result
        })
        .await
    }
}
