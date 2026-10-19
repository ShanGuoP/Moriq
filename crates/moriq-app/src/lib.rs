//! Bounded single-writer coordinator. UI never owns a DB connection or performs media I/O.
use futures::{Stream, StreamExt, stream};
mod cache;
mod scan;
use moriq_domain::*;
use moriq_library::Library;
pub use moriq_library::Pixels;
pub use moriq_library::maintenance::Control as MaintenanceControl;
use moriq_library::maintenance::Outcome as MaintenanceOutcome;
pub use moriq_library::native::IMPORT_EXTENSIONS;
pub use moriq_library::storage::Control as StorageControl;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};
use tokio::sync::{Semaphore, mpsc, oneshot};

pub type Result<T> = std::result::Result<T, String>;
pub const SOURCE_REPOSITORY_URL: &str = "https://github.com/ShanGuoP/Moriq";
pub fn is_video_format(format: &str) -> bool {
    moriq_library::video::EXTENSIONS.contains(&format)
}
pub use moriq_ai::{DownloadControl, Hardware, ModelInfo};
pub use moriq_library::orphans::Plan as OrphanPlan;
pub use moriq_library::region::Session as RegionSession;
pub use moriq_library::video::Playback;

pub async fn open_source_repository() -> Result<()> {
    tokio::task::spawn_blocking(|| {
        #[cfg(windows)]
        let mut command = std::process::Command::new("explorer.exe");
        #[cfg(target_os = "macos")]
        let mut command = std::process::Command::new("open");
        #[cfg(not(any(windows, target_os = "macos")))]
        let mut command = std::process::Command::new("xdg-open");
        command
            .arg(SOURCE_REPOSITORY_URL)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("无法打开项目开源地址：{e}"))
    })
    .await
    .map_err(|e| e.to_string())?
}
type Job = Box<dyn FnOnce(&mut Option<Library>) + Send>;

#[derive(Clone)]
pub struct Service {
    media_cache: Arc<cache::MediaCache>,
    queue: mpsc::Sender<Job>,
    import_slots: Arc<Semaphore>,
    import_memory: Arc<Semaphore>,
    maintenance_slot: Arc<Semaphore>,
    read_slots: Arc<Semaphore>,
}
#[derive(Debug, Clone)]
pub struct Opened {
    pub info: LibraryInfo,
    pub root: PathBuf,
}
#[derive(Debug, Clone)]
pub struct Tile {
    pub asset: Asset,
    pub thumbnail: Option<Pixels>,
}
#[derive(Debug, Clone)]
pub struct LoadedPage {
    pub total: u64,
    pub offset: usize,
    pub tiles: Vec<Tile>,
    pub warnings: usize,
}

#[derive(Debug, Clone)]
pub struct LoadedNeighborhood {
    pub selected: AssetId,
    pub position: usize,
    pub page: LoadedPage,
}
#[derive(Debug, Clone, Default)]
pub struct LoadedOverview {
    pub favorites: Vec<Tile>,
    pub projects: Vec<(ProjectId, Option<Pixels>)>,
}
#[derive(Debug, Clone)]
pub struct ImportControl {
    pub cancel: Arc<AtomicBool>,
    pub finished: Arc<AtomicUsize>,
    pub total: usize,
    pub referenced: bool,
    pub workers: usize,
    pub memory_mib: u32,
    pub discovered: Arc<AtomicUsize>,
    pub scanning: Arc<AtomicBool>,
    pub active: Arc<AtomicUsize>,
    pub peak: Arc<AtomicUsize>,
}
impl Default for ImportControl {
    fn default() -> Self {
        Self {
            cancel: Arc::default(),
            finished: Arc::default(),
            total: 0,
            referenced: false,
            workers: ImportPreferences::default().effective_workers(
                std::thread::available_parallelism()
                    .map(usize::from)
                    .unwrap_or(4),
            ),
            memory_mib: 1024,
            discovered: Arc::default(),
            scanning: Arc::default(),
            active: Arc::default(),
            peak: Arc::default(),
        }
    }
}
struct ImportActivity(Arc<AtomicUsize>);
impl Drop for ImportActivity {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}
impl ImportControl {
    pub fn with_preferences(prefs: ImportPreferences) -> Self {
        Self {
            workers: prefs.effective_workers(
                std::thread::available_parallelism()
                    .map(usize::from)
                    .unwrap_or(4),
            ),
            memory_mib: prefs.memory_mib.clamp(256, 4096),
            ..Default::default()
        }
    }
}
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
    pub async fn region_session(
        &self,
        id: LibraryId,
        asset: AssetId,
        version: AssetVersionId,
    ) -> Result<Arc<RegionSession>> {
        self.read_plan(
            2048,
            move |active| {
                check(active, id)?
                    .version_source(asset, version)
                    .map_err(|e| e.to_string())
            },
            |source| source.regions().map(Arc::new).map_err(|e| e.to_string()),
        )
        .await
    }
    pub async fn region_tile(
        &self,
        id: LibraryId,
        session: Arc<RegionSession>,
        region: ImageRegion,
    ) -> Result<Pixels> {
        self.read_plan(
            1024,
            move |active| {
                check(active, id)?;
                Ok(session)
            },
            move |session| session.tile(region).map_err(|e| e.to_string()),
        )
        .await
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
    pub async fn synco_preferences(&self, value: SyncoPreferences) -> Result<UserSettings> {
        self.call(move |_| {
            moriq_library::settings::synco_preferences(value).map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn retry_synco(&self, id: LibraryId, task: String) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .retry_synco(&task)
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn saved_navigation(&self, id: LibraryId) -> Result<Option<SavedNavigation>> {
        self.call(move |active| {
            check(active, id)?
                .saved_navigation()
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn save_navigation(&self, id: LibraryId, value: SavedNavigation) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .save_navigation(&value)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn ai_preferences(&self, value: AiPreferences) -> Result<UserSettings> {
        let preferences = self
            .call(move |_| {
                moriq_library::settings::ai_preferences(value).map_err(|e| e.to_string())
            })
            .await?;
        self.ai_policy(value).await?;
        Ok(preferences)
    }
    pub async fn ai_policy(&self, value: AiPreferences) -> Result<()> {
        tokio::task::spawn_blocking(move || moriq_ai::set_keep_loaded(!value.on_demand))
            .await
            .map_err(|e| e.to_string())?
    }
    pub async fn clone_library(&self, id: LibraryId, parent: PathBuf) -> Result<PathBuf> {
        self.call(move |active| {
            check(active, id)?
                .clone_library(&parent)
                .map_err(|e| e.to_string())
        })
        .await
    }
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
    pub async fn batch_edit(
        &self,
        id: LibraryId,
        scope: BatchScope,
        patch: BatchPatch,
    ) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .enqueue_batch(scope, patch)
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn folder_edit(&self, id: LibraryId, edit: FolderEdit) -> Result<()> {
        self.call(move |active| {
            let library = check(active, id)?;
            match edit {
                FolderEdit::Create(name, favorite) => {
                    library.create_virtual_folder(&name, favorite).map(|_| ())
                }
                FolderEdit::Rename(folder, name) => library.rename_virtual_folder(&folder, &name),
                FolderEdit::Delete(folder) => library.delete_virtual_folder(&folder),
                FolderEdit::Members(folder, assets, add) => {
                    library.virtual_folder_members(&folder, &assets, add)
                }
                FolderEdit::Cover(folder, asset) => library.virtual_folder_cover(&folder, asset),
            }
            .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn semantic_search(&self, id: LibraryId, text: String) -> Result<Vec<AssetId>> {
        if text.trim().is_empty() || text.len() > 4096 {
            return Err("请输入1～4096字节的自然语言描述".into());
        }
        let result = tokio::task::spawn_blocking(move || {
            moriq_ai::infer("chinese-clip", serde_json::json!({"text":text}))
        })
        .await
        .map_err(|e| e.to_string())??;
        let query: Vec<f32> =
            serde_json::from_value(result["text_vector"].clone()).map_err(|e| e.to_string())?;
        self.read_plan(
            128,
            move |active| {
                check(active, id)?
                    .semantic_plan()
                    .map_err(|e| e.to_string())
            },
            move |plan| plan.search(&query, None).map_err(|e| e.to_string()),
        )
        .await
    }
    pub async fn search_by_image(&self, id: LibraryId, asset: AssetId) -> Result<Vec<AssetId>> {
        let result = self
            .read_plan(
                2048,
                move |active| {
                    check(active, id)?
                        .media_source(asset)
                        .map_err(|e| e.to_string())
                },
                |source| moriq_library::analysis::run(source, "chinese-clip"),
            )
            .await?;
        let query: Vec<f32> =
            serde_json::from_value(result["image_vector"].clone()).map_err(|e| e.to_string())?;
        self.read_plan(
            128,
            move |active| {
                check(active, id)?
                    .semantic_plan()
                    .map_err(|e| e.to_string())
            },
            move |plan| plan.search(&query, Some(asset)).map_err(|e| e.to_string()),
        )
        .await
    }
    pub async fn recommendations(
        &self,
        id: LibraryId,
        asset: AssetId,
        version: AssetVersionId,
    ) -> Result<Vec<(AssetId, String)>> {
        self.read_plan(
            64,
            move |active| {
                check(active, id)?
                    .semantic_plan()
                    .map_err(|e| e.to_string())
            },
            move |plan| {
                plan.recommendations(asset, version)
                    .map_err(|e| e.to_string())
            },
        )
        .await
    }
    pub async fn models(&self) -> Result<(Vec<ModelInfo>, Hardware)> {
        tokio::task::spawn_blocking(|| Ok((moriq_ai::inventory()?, moriq_ai::hardware())))
            .await
            .map_err(|e| e.to_string())?
    }
    pub async fn install_model(&self, model: String, control: DownloadControl) -> Result<()> {
        tokio::task::spawn_blocking(move || moriq_ai::install(&model, &control))
            .await
            .map_err(|e| e.to_string())?
    }
    pub async fn uninstall_model(&self, model: String) -> Result<()> {
        tokio::task::spawn_blocking(move || moriq_ai::uninstall(&model))
            .await
            .map_err(|e| e.to_string())?
    }
    pub async fn set_category(
        &self,
        id: LibraryId,
        assets: Vec<AssetId>,
        category: String,
    ) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .set_category(&assets, &category)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn rename_person(&self, id: LibraryId, person: String, name: String) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .rename_person(&person, &name)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn merge_people(&self, id: LibraryId, source: String, target: String) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .merge_people(&source, &target)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn faces(&self, id: LibraryId, asset: AssetId) -> Result<Vec<Face>> {
        self.call(move |active| check(active, id)?.faces(asset).map_err(|e| e.to_string()))
            .await
    }
    pub async fn assign_face(
        &self,
        id: LibraryId,
        face: String,
        person: Option<String>,
    ) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .assign_face(&face, person.as_deref())
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn clear_ai_indexes(&self, id: LibraryId, model: String) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .clear_ai_indexes(&model)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn queue_media_job(&self, id: LibraryId, kind: String) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .enqueue_media_job(&kind, 0)
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn control_job(&self, id: LibraryId, job: String, command: JobCommand) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .control_job(&job, command)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn job_failures(&self, id: LibraryId, job: String) -> Result<Vec<JobFailure>> {
        self.call(move |active| {
            check(active, id)?
                .job_failures(&job)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn similar_assets(&self, id: LibraryId, asset: AssetId) -> Result<Vec<SimilarAsset>> {
        self.read_plan(
            64,
            move |active| {
                check(active, id)?
                    .similarity_plan()
                    .map_err(|e| e.to_string())
            },
            move |plan| plan.search(asset).map_err(|e| e.to_string()),
        )
        .await
    }
    pub async fn drive_job_step(&self, id: LibraryId) -> Result<()> {
        let service = self.clone();
        // A dropped UI future must not strand an in-flight checkpoint in an open library.
        tokio::spawn(async move {
            let started = std::time::Instant::now();
            for _ in 0..64 {
                let work = service
                    .call(move |active| {
                        check(active, id)?
                            .claim_job_step()
                            .map_err(|e| e.to_string())
                    })
                    .await?;
                let Some(work) = work else { return Ok(()) };
                let budget = work.memory_mib();
                let (job, generation) = work.abort_token();
                let memory = service
                    .import_memory
                    .clone()
                    .acquire_many_owned(budget)
                    .await
                    .map_err(|e| e.to_string())?;
                let outcome = tokio::task::spawn_blocking(move || {
                    let _permit = memory;
                    work.run()
                })
                .await;
                let outcome = match outcome {
                    Ok(outcome) => outcome,
                    Err(error) => {
                        let message = error.to_string();
                        let detail = message.clone();
                        let _ = service
                            .call(move |active| {
                                check(active, id)?
                                    .abort_job(&job, generation, &detail)
                                    .map_err(|e| e.to_string())
                            })
                            .await;
                        return Err(message);
                    }
                };
                service
                    .call(move |active| {
                        check(active, id)?
                            .finish_job_step(outcome)
                            .map_err(|e| e.to_string())
                    })
                    .await?;
                if started.elapsed() >= std::time::Duration::from_millis(250) {
                    break;
                }
            }
            Ok(())
        })
        .await
        .map_err(|e| e.to_string())?
    }
    pub async fn import_preferences(&self, value: ImportPreferences) -> Result<UserSettings> {
        self.call(move |_| {
            moriq_library::settings::import_preferences(value).map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn overview(&self, id: LibraryId) -> Result<LoadedOverview> {
        self.read_plan(
            64,
            move |active| {
                let lib = check(active, id)?;
                let favorites = lib
                    .page(
                        &Query {
                            favorites_only: true,
                            ..Default::default()
                        },
                        0,
                        4,
                    )
                    .map_err(|e| e.to_string())?
                    .assets;
                let projects = lib.project_covers().map_err(|e| e.to_string())?;
                let versions = favorites
                    .iter()
                    .map(|a| a.version_id)
                    .chain(projects.iter().filter_map(|(_, v)| *v))
                    .collect::<Vec<_>>();
                let thumbnails = lib.thumbnail_plan(&versions).map_err(|e| e.to_string())?;
                Ok((favorites, projects, thumbnails))
            },
            move |(favorites, projects, thumbnails)| {
                let (images, _warnings) = thumbnails.read();
                let mut images = images.into_iter();
                let favorites = favorites
                    .into_iter()
                    .map(|asset| Tile {
                        asset,
                        thumbnail: images.next().flatten(),
                    })
                    .collect();
                let projects = projects
                    .into_iter()
                    .map(|(id, version)| (id, version.and_then(|_| images.next().flatten())))
                    .collect();
                Ok(LoadedOverview {
                    favorites,
                    projects,
                })
            },
        )
        .await
    }
    pub async fn prepare_synco(
        &self,
        id: LibraryId,
        assets: Vec<AssetId>,
        project: Option<ProjectId>,
    ) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .prepare_synco(&assets, project)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn display_version(
        &self,
        id: LibraryId,
        asset: AssetId,
        version: AssetVersionId,
    ) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .display_version(asset, version)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn synco_poll(&self, id: LibraryId) -> Result<usize> {
        self.call(move |active| {
            check(active, id)?
                .cleanup_synco_copies()
                .map_err(|e| e.to_string())
        })
        .await?;
        let tasks = self
            .call(move |active| {
                check(active, id)?
                    .synco_pending()
                    .map_err(|e| e.to_string())
            })
            .await?;
        if tasks.is_empty() {
            return Ok(0);
        }
        let settings = self.settings().await?;
        let client = if settings.synco.auto_start {
            moriq_synco::Client::discover_or_start(settings.synco.executable).await?
        } else {
            moriq_synco::Client::discover().await?
        };
        let mut finished = 0;
        let mut failure = None;
        for task in tasks {
            let task_id = task.id.clone();
            let task_client = client.clone();
            let service = self.clone();
            let result = async {
                let request: serde_json::Value =
                    serde_json::from_str(&task.request).map_err(|e| e.to_string())?;
                let client_id = request["client_id"]
                    .as_str()
                    .ok_or("交接请求缺少客户端身份")?;
                let ack = if let Some(ack) = task.ack {
                    Some(
                        serde_json::from_str::<serde_json::Value>(&ack)
                            .map_err(|e| e.to_string())?,
                    )
                } else {
                    let response = match task_client.get(&task_id, client_id).await? {
                        Some(value) => value,
                        None if task.state == "queued" => {
                            task_client.send(&request).await?;
                            task_client
                                .get(&task_id, client_id)
                                .await?
                                .ok_or("Synco 接受后尚未提供任务状态，请重试")?
                        }
                        None => {
                            return Err(
                                "Synco 中找不到已送达的任务，请检查是否连接到原来的实例".into()
                            );
                        }
                    };
                    let task = task_id.clone();
                    service
                        .call(move |active| {
                            check(active, id)?
                                .receive_synco(&task, &response)
                                .map_err(|e| e.to_string())
                        })
                        .await?
                };
                if let Some(ack) = ack {
                    task_client.ack(&task_id, &ack).await?;
                    let task = task_id.clone();
                    service
                        .call(move |active| {
                            check(active, id)?
                                .synco_acked(&task)
                                .map_err(|e| e.to_string())
                        })
                        .await?;
                    finished += 1;
                }
                Ok::<(), String>(())
            }
            .await;
            if let Err(error) = result {
                let task = task.id;
                let detail = error.clone();
                let retained = self
                    .call(move |active| {
                        check(active, id)?
                            .synco_error(&task, &detail)
                            .map_err(|e| e.to_string())
                    })
                    .await?;
                if !retained {
                    continue;
                }
                failure = Some(error);
            }
        }
        if let Some(error) = failure {
            return Err(error);
        }
        Ok(finished)
    }
    pub async fn recover(&self, id: LibraryId) -> Result<ImportReport> {
        self.call(move |active| {
            check(active, id)?
                .recover_imports()
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn settings(&self) -> Result<UserSettings> {
        self.call(|_| moriq_library::settings::load().map_err(|e| e.to_string()))
            .await
    }
    pub async fn remember(&self, name: String, path: PathBuf) -> Result<UserSettings> {
        self.call(move |_| moriq_library::settings::remember(name, path).map_err(|e| e.to_string()))
            .await
    }
    pub async fn language(&self, english: bool) -> Result<UserSettings> {
        self.call(move |_| moriq_library::settings::language(english).map_err(|e| e.to_string()))
            .await
    }
    pub async fn description(&self, id: LibraryId, value: String) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .description(&value)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn cover(&self, id: LibraryId) -> Result<Option<Pixels>> {
        self.dispatch(true, move |active| {
            check(active, id)?.cover().map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn set_cover(&self, id: LibraryId, path: PathBuf) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .set_cover(&path)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn organizer(&self, id: LibraryId) -> Result<Organizer> {
        self.call(move |active| check(active, id)?.organizer().map_err(|e| e.to_string()))
            .await
    }
    pub async fn details(&self, id: LibraryId, asset: AssetId) -> Result<AssetDetails> {
        self.read_plan(
            32,
            move |active| {
                check(active, id)?
                    .details_plan(asset)
                    .map_err(|e| e.to_string())
            },
            move |plan| Ok(plan.read()),
        )
        .await
    }
    pub async fn edit(&self, id: LibraryId, assets: Vec<AssetId>, edit: Edit) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .edit(&assets, &edit)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn rename_project(
        &self,
        id: LibraryId,
        project: ProjectId,
        name: String,
    ) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .rename_project(project, &name)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn delete_project(&self, id: LibraryId, project: ProjectId) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .delete_project(project)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn project_deletion_plan(
        &self,
        id: LibraryId,
        project: ProjectId,
    ) -> Result<ProjectDeletionPlan> {
        self.call(move |active| {
            check(active, id)?
                .project_deletion_plan(project)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn delete_project_contents(
        &self,
        id: LibraryId,
        plan: ProjectDeletionPlan,
    ) -> Result<PurgeReport> {
        self.call(move |active| {
            check(active, id)?
                .delete_project_contents(&plan)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn collection(&self, id: LibraryId, name: String, query: Query) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .save_collection(&name, &query)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn delete_collection(&self, id: LibraryId, collection: String) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .delete_collection(&collection)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn export(
        &self,
        id: LibraryId,
        assets: Vec<AssetId>,
        parent: PathBuf,
    ) -> Result<PathBuf> {
        self.export_options(id, assets, parent, ExportOptions::default())
            .await
    }
    pub async fn export_options(
        &self,
        id: LibraryId,
        assets: Vec<AssetId>,
        parent: PathBuf,
        options: ExportOptions,
    ) -> Result<PathBuf> {
        let plan = self
            .call(move |active| {
                check(active, id)?
                    .export_plan(&assets)
                    .map_err(|e| e.to_string())
            })
            .await?;
        tokio::task::spawn_blocking(move || {
            moriq_library::export::run(plan, &parent, options).map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())?
    }
    pub async fn selection_range(
        &self,
        id: LibraryId,
        query: Query,
        first: AssetId,
        last: AssetId,
    ) -> Result<Vec<AssetId>> {
        self.call(move |active| {
            check(active, id)?
                .selection_range(&query, first, last)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn create_child_project(
        &self,
        id: LibraryId,
        name: String,
        parent: Option<ProjectId>,
    ) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .create_child_project(&name, parent)
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn move_project(
        &self,
        id: LibraryId,
        project: ProjectId,
        parent: Option<ProjectId>,
    ) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .move_project(project, parent)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn rename_tag(&self, id: LibraryId, source: String, target: String) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .rename_tag(&source, &target)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn delete_tag(&self, id: LibraryId, name: String) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .delete_tag(&name)
                .map_err(|e| e.to_string())
        })
        .await
    }
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
    pub async fn backup(&self, id: LibraryId, parent: PathBuf) -> Result<PathBuf> {
        self.call(move |active| {
            check(active, id)?
                .backup(&parent)
                .map_err(|e| e.to_string())
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
            report.checked += 1;
            match outcome {
                MaintenanceOutcome::Missing(version) => {
                    report.missing += 1;
                    if report.warnings.len() < 50 {
                        report.warnings.push(format!("版本 {version} 文件缺失"));
                    }
                }
                MaintenanceOutcome::Damaged(version) => {
                    report.damaged += 1;
                    if report.warnings.len() < 50 {
                        report.warnings.push(format!("版本 {version} 文件校验不符"));
                    }
                }
                MaintenanceOutcome::Warning(warning) => {
                    if report.warnings.len() < 50 {
                        report.warnings.push(warning);
                    }
                }
                MaintenanceOutcome::Valid(Some(result)) => {
                    if control.cancel.load(Ordering::Relaxed) {
                        continue;
                    }
                    match self
                        .call(move |active| {
                            check(active, id)?
                                .apply_cache_repair(result)
                                .map_err(|e| e.to_string())
                        })
                        .await
                    {
                        Ok(true) => report.repaired += 1,
                        Ok(false) => {}
                        Err(error) => {
                            failure = Some(error);
                            control.cancel.store(true, Ordering::Relaxed);
                            break;
                        }
                    }
                }
                MaintenanceOutcome::Valid(None) => {}
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
    pub async fn rename_library(&self, id: LibraryId, name: String) -> Result<LibraryInfo> {
        self.call(move |active| {
            let library = check(active, id)?;
            library.rename(&name).map_err(|e| e.to_string())?;
            library.info().map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn rename_asset(
        &self,
        library: LibraryId,
        asset: AssetId,
        name: String,
    ) -> Result<()> {
        self.call(move |active| {
            check(active, library)?
                .rename_asset(asset, &name)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn close(&self) -> Result<()> {
        self.media_cache.clear();
        self.call(|active| {
            *active = None;
            Ok(())
        })
        .await
    }
    pub async fn reveal(&self, library: LibraryId) -> Result<()> {
        self.call(move |active| {
            let root = check(active, library)?.root().to_path_buf();
            #[cfg(target_os = "windows")]
            let mut command = std::process::Command::new("explorer.exe");
            #[cfg(target_os = "macos")]
            let mut command = std::process::Command::new("open");
            #[cfg(not(any(target_os = "windows", target_os = "macos")))]
            let mut command = std::process::Command::new("xdg-open");
            command
                .arg(root)
                .spawn()
                .map_err(|error| format!("无法打开文件夹：{error}"))?;
            Ok(())
        })
        .await
    }
    pub async fn reveal_asset(&self, library: LibraryId, asset: AssetId) -> Result<()> {
        self.call(move |active| {
            let path = check(active, library)?
                .asset_path(asset)
                .map_err(|e| e.to_string())?;
            #[cfg(target_os = "windows")]
            std::process::Command::new("explorer.exe")
                .arg(format!("/select,{}", path.display()))
                .spawn()
                .map_err(|e| format!("无法定位文件：{e}"))?;
            #[cfg(target_os = "macos")]
            std::process::Command::new("open")
                .arg("-R")
                .arg(&path)
                .spawn()
                .map_err(|e| format!("无法定位文件：{e}"))?;
            #[cfg(not(any(target_os = "windows", target_os = "macos")))]
            std::process::Command::new("xdg-open")
                .arg(path.parent().ok_or("文件没有所属目录")?)
                .spawn()
                .map_err(|e| format!("无法定位文件：{e}"))?;
            Ok(())
        })
        .await
    }
    pub async fn resources(
        &self,
        resources: Vec<(&'static str, &'static [u8])>,
    ) -> Result<Vec<(String, Pixels)>> {
        self.dispatch(true, move |_| {
            resources
                .into_iter()
                .map(|(name, bytes)| {
                    moriq_library::decode_resource(bytes)
                        .map(|pixels| (name.to_string(), pixels))
                        .map_err(|e| e.to_string())
                })
                .collect()
        })
        .await
    }
    pub async fn masonry(
        &self,
        library: LibraryId,
        query: Query,
        columns: usize,
        width: f32,
    ) -> Result<Arc<moriq_render::MasonryIndex>> {
        let sizes = self
            .dispatch(true, move |active| {
                check(active, library)?
                    .layout_sizes(&query)
                    .map_err(|e| e.to_string())
            })
            .await?;
        tokio::task::spawn_blocking(move || {
            Arc::new(moriq_render::MasonryIndex::new(
                &sizes, columns, width, 10.0,
            ))
        })
        .await
        .map_err(|e| e.to_string())
    }
    pub fn start() -> std::io::Result<Self> {
        let (queue, mut receiver) = mpsc::channel::<Job>(16);
        std::thread::Builder::new()
            .name("moriq-library-writer".into())
            .spawn(move || {
                let mut active = None;
                while let Some(job) = receiver.blocking_recv() {
                    job(&mut active);
                }
            })?;
        Ok(Self {
            queue,
            import_slots: Arc::new(Semaphore::new(32)),
            import_memory: Arc::new(Semaphore::new(4096)),
            maintenance_slot: Arc::new(Semaphore::new(1)),
            read_slots: Arc::new(Semaphore::new(4)),
            media_cache: Arc::new(Default::default()),
        })
    }
    async fn call<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Option<Library>) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        self.dispatch(false, f).await
    }
    async fn dispatch<T: Send + 'static>(
        &self,
        cancellable: bool,
        f: impl FnOnce(&mut Option<Library>) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let (sender, receiver) = oneshot::channel();
        self.queue
            .send(Box::new(move |active| {
                if cancellable && sender.is_closed() {
                    return;
                }
                if sender.send(f(active)).is_err() {
                    tracing::debug!("caller no longer listening to worker response");
                }
            }))
            .await
            .map_err(|_| "后台服务已关闭，请重新打开 MORIQ".to_string())?;
        receiver
            .await
            .map_err(|_| "后台服务中断，请重新打开 MORIQ".to_string())?
    }
    pub async fn open(&self, path: PathBuf) -> Result<Opened> {
        self.call(move |active| {
            if let Some(lib) = active.as_ref()
                && lib.root() == path.canonicalize().map_err(|e| e.to_string())?
            {
                return Ok(Opened {
                    info: lib.info().map_err(|e| e.to_string())?,
                    root: lib.root().into(),
                });
            }
            let library = Library::open(&path).map_err(|e| e.to_string())?;
            if active.as_ref().is_some_and(|current|current.id()==library.id()) {
                return Err("检测到与当前库身份相同的副本。请关闭应用后打开迁移后的库；需要独立使用时请选择创建独立克隆。".into());
            }
            let opened = Opened {
                info: library.info().map_err(|e| e.to_string())?,
                root: library.root().into(),
            };
            *active = Some(library);
            Ok(opened)
        })
        .await
    }
    pub async fn create(&self, parent: PathBuf, name: String) -> Result<Opened> {
        self.call(move |active| {
            let library = Library::create(&parent, &name).map_err(|e| e.to_string())?;
            let opened = Opened {
                info: library.info().map_err(|e| e.to_string())?,
                root: library.root().into(),
            };
            *active = Some(library);
            Ok(opened)
        })
        .await
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
    pub async fn set_location(
        &self,
        id: LibraryId,
        asset: AssetId,
        version: AssetVersionId,
        value: Option<AssetLocation>,
    ) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .set_location(asset, version, value.as_ref())
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn open_map(&self, id: LibraryId, location: AssetLocation) -> Result<()> {
        self.call(move |active| {
            check(active, id)?;
            let url = location.map_url().ok_or("坐标无效")?;
            #[cfg(windows)]
            let mut command = std::process::Command::new("explorer.exe");
            #[cfg(target_os = "macos")]
            let mut command = std::process::Command::new("open");
            #[cfg(not(any(windows, target_os = "macos")))]
            let mut command = std::process::Command::new("xdg-open");
            command
                .arg(url)
                .spawn()
                .map_err(|e| format!("无法打开地图：{e}"))?;
            Ok(())
        })
        .await
    }
    pub async fn events(
        &self,
        id: LibraryId,
        notices: bool,
        before: Option<i64>,
    ) -> Result<EventPage> {
        self.call(move |active| {
            check(active, id)?
                .events(notices, before)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn notice(&self, id: LibraryId, notice: Notice) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .notice(&notice)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn read_notifications(&self, id: LibraryId, through: i64) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .read_notifications(through)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn visit_project(&self, id: LibraryId, project: ProjectId) -> Result<LibraryInfo> {
        self.call(move |active| {
            let lib = check(active, id)?;
            lib.visit_project(project).map_err(|e| e.to_string())?;
            lib.info().map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn asset_trash_state(&self, id: LibraryId, asset: AssetId) -> Result<Option<bool>> {
        self.call(move |active| {
            check(active, id)?
                .asset_trash_state(asset)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn update_profile(
        &self,
        id: LibraryId,
        name: String,
        description: String,
    ) -> Result<LibraryInfo> {
        self.call(move |active| {
            let lib = check(active, id)?;
            lib.update_profile(&name, &description)
                .map_err(|e| e.to_string())?;
            lib.info().map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn info(&self, id: LibraryId) -> Result<LibraryInfo> {
        self.call(move |active| check(active, id)?.info().map_err(|e| e.to_string()))
            .await
    }
    pub async fn page(
        &self,
        id: LibraryId,
        query: Query,
        offset: usize,
        limit: usize,
    ) -> Result<LoadedPage> {
        self.read_plan(
            128,
            move |active| {
                let lib = check(active, id)?;
                let page = lib.page(&query, offset, limit).map_err(|e| e.to_string())?;
                let versions = page.assets.iter().map(|a| a.version_id).collect::<Vec<_>>();
                let thumbnails = lib.thumbnail_plan(&versions).map_err(|e| e.to_string())?;
                Ok((page, thumbnails))
            },
            move |(page, thumbnails)| {
                let (images, warnings) = thumbnails.read();
                Ok(LoadedPage {
                    total: page.total,
                    offset: page.offset,
                    warnings,
                    tiles: page
                        .assets
                        .into_iter()
                        .zip(images)
                        .map(|(asset, thumbnail)| Tile { asset, thumbnail })
                        .collect(),
                })
            },
        )
        .await
    }
    pub async fn neighborhood(
        &self,
        id: LibraryId,
        query: Query,
        anchor: AssetId,
        direction: i8,
    ) -> Result<Option<LoadedNeighborhood>> {
        self.read_plan(
            16,
            move |active| {
                let lib = check(active, id)?;
                let Some(result) = lib
                    .neighborhood(&query, anchor, direction)
                    .map_err(|e| e.to_string())?
                else {
                    return Ok(None);
                };
                let thumbnails = lib
                    .thumbnail_plan(
                        &result
                            .page
                            .assets
                            .iter()
                            .map(|a| a.version_id)
                            .collect::<Vec<_>>(),
                    )
                    .map_err(|e| e.to_string())?;
                Ok(Some((result, thumbnails)))
            },
            move |plan| {
                let Some((result, thumbnails)) = plan else {
                    return Ok(None);
                };
                let (images, warnings) = thumbnails.read();
                Ok(Some(LoadedNeighborhood {
                    selected: result.selected,
                    position: result.position,
                    page: LoadedPage {
                        total: result.page.total,
                        offset: result.page.offset,
                        warnings,
                        tiles: result
                            .page
                            .assets
                            .into_iter()
                            .zip(images)
                            .map(|(asset, thumbnail)| Tile { asset, thumbnail })
                            .collect(),
                    },
                }))
            },
        )
        .await
    }
    async fn read_plan<P: Send + 'static, T: Send + 'static>(
        &self,
        memory_mib: u32,
        prepare: impl FnOnce(&mut Option<Library>) -> Result<P> + Send + 'static,
        work: impl FnOnce(P) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        self.read_plan_sized(move |_| memory_mib, prepare, work)
            .await
    }
    async fn read_plan_sized<P: Send + 'static, T: Send + 'static>(
        &self,
        budget: impl FnOnce(&P) -> u32 + Send + 'static,
        prepare: impl FnOnce(&mut Option<Library>) -> Result<P> + Send + 'static,
        work: impl FnOnce(P) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        // Admit before freezing any handles: active and waiting media plans share four slots.
        let slots = self
            .read_slots
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| e.to_string())?;
        let plan = self.dispatch(true, prepare).await?;
        let memory_mib = budget(&plan);
        let memory = self
            .import_memory
            .clone()
            .acquire_many_owned(memory_mib)
            .await
            .map_err(|e| e.to_string())?;
        tokio::task::spawn_blocking(move || {
            let _permits = (slots, memory);
            work(plan)
        })
        .await
        .map_err(|e| e.to_string())?
    }
    pub async fn preview_full(
        &self,
        library: LibraryId,
        asset: AssetId,
    ) -> Result<moriq_library::media::Preview> {
        let cache = self.media_cache.clone();
        let prepare_cache = cache.clone();
        self.read_plan_sized(
            |(_, _, cached): &(_, _, Option<moriq_library::media::Preview>)| {
                if cached.is_some() { 0 } else { 1024 }
            },
            move |active| {
                let source = check(active, library)?
                    .media_source(asset)
                    .map_err(|e| e.to_string())?;
                let cached = source
                    .cacheable()
                    .then(|| prepare_cache.get((library, source.version(), 1600)))
                    .flatten();
                Ok((source.version(), source, cached))
            },
            move |(version, source, cached)| {
                if let Some(value) = cached {
                    return Ok(value);
                }
                let key = (library, version, 1600);
                let cacheable = source.cacheable();
                if cacheable && let Some(value) = cache.get(key) {
                    return Ok(value);
                }
                let value = source.preview().map_err(|e| e.to_string())?;
                if cacheable {
                    cache.put(key, value.clone());
                }
                Ok(value)
            },
        )
        .await
    }
    pub async fn preview(&self, library: LibraryId, asset: AssetId) -> Result<Pixels> {
        self.preview_full(library, asset).await.map(|p| p.image)
    }
    pub async fn preview_original(
        &self,
        library: LibraryId,
        asset: AssetId,
        version: AssetVersionId,
    ) -> Result<Pixels> {
        self.read_plan(
            2048,
            move |active| {
                check(active, library)?
                    .version_source(asset, version)
                    .map_err(|e| e.to_string())
            },
            |source| {
                source
                    .preview_edge(0)
                    .map(|p| p.image)
                    .map_err(|e| e.to_string())
            },
        )
        .await
    }
    pub async fn playback(
        &self,
        library: LibraryId,
        asset: AssetId,
        version: AssetVersionId,
    ) -> Result<Arc<Playback>> {
        self.read_plan(
            64,
            move |active| {
                check(active, library)?
                    .version_source(asset, version)
                    .map_err(|e| e.to_string())
            },
            |s| s.playback().map_err(|e| e.to_string()),
        )
        .await
    }
    pub async fn compare_versions(
        &self,
        library: LibraryId,
        asset: AssetId,
        left: AssetVersionId,
        right: AssetVersionId,
    ) -> Result<(Pixels, Pixels)> {
        self.read_plan(
            1024,
            move |active| {
                let library = check(active, library)?;
                Ok((
                    library
                        .version_source(asset, left)
                        .map_err(|e| e.to_string())?,
                    library
                        .version_source(asset, right)
                        .map_err(|e| e.to_string())?,
                ))
            },
            move |(left, right)| {
                // Sequential decode: one decoder's peak allocation plus two bounded previews.
                Ok((
                    left.preview().map_err(|e| e.to_string())?.image,
                    right.preview().map_err(|e| e.to_string())?.image,
                ))
            },
        )
        .await
    }
    pub async fn favorite(&self, library: LibraryId, asset: AssetId, value: bool) -> Result<()> {
        self.call(move |active| {
            check(active, library)?
                .set_favorite(asset, value)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn rating(&self, library: LibraryId, asset: AssetId, value: u8) -> Result<()> {
        self.call(move |active| {
            check(active, library)?
                .set_rating(asset, value)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn tags(&self, library: LibraryId, asset: AssetId, value: Vec<String>) -> Result<()> {
        self.call(move |active| {
            check(active, library)?
                .set_tags(asset, &value)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn project(&self, library: LibraryId, name: String) -> Result<ProjectId> {
        self.call(move |active| {
            check(active, library)?
                .create_project(&name)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn save_capture(
        &self,
        path: PathBuf,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    ) -> Result<()> {
        self.call(move |_| {
            moriq_library::save_capture(&path, width, height, &rgba).map_err(|e| e.to_string())
        })
        .await
    }
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
            let weight = tokio::task::spawn_blocking(move || {
                let _slot = copy_slot;
                work.copy(&cancel)?;
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
fn check(active: &mut Option<Library>, expected: LibraryId) -> Result<&mut Library> {
    let library = active.as_mut().ok_or("请先创建或打开资源库")?;
    if library.id() != expected {
        return Err("资源库已经切换，旧操作已停止".into());
    }
    Ok(library)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
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
                tokio::spawn(
                    async move { task_service.import(id, files, None, task_control).await },
                );
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
}
