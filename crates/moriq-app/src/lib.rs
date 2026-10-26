//! Bounded single-writer coordinator. UI never owns a DB connection or performs media I/O.
use futures::{Stream, StreamExt, stream};
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod backup_tests;
mod cache;
mod catalog;
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod export_budget_tests;
mod import;
pub mod inference;
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod inference_tests;
mod interop;
mod jobs;
mod launch;
mod maintenance;
mod media;
mod scan;
mod scheduler;
mod settings;
use moriq_domain::*;
use moriq_library::Library;
pub use moriq_library::Pixels;
pub use moriq_library::backup::Control as BackupControl;
pub use moriq_library::library_removal::Plan as LibraryRemovalPlan;
pub use moriq_library::maintenance::Control as MaintenanceControl;
pub use moriq_library::native::IMPORT_EXTENSIONS;
pub use moriq_library::storage::Control as StorageControl;
pub use settings::LibraryCard;
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
pub fn model_directory() -> Result<PathBuf> {
    moriq_ai::model_root()
}
pub async fn open_models_directory() -> Result<()> {
    let directory = tokio::task::spawn_blocking(|| {
        let directory = model_directory()?;
        std::fs::create_dir_all(&directory).map_err(|e| format!("无法创建模型文件夹：{e}"))?;
        directory
            .canonicalize()
            .map_err(|e| format!("无法读取模型下载目录：{e}"))
    })
    .await
    .map_err(|e| e.to_string())??;
    launch::open(directory.into_os_string(), false).await
}
pub fn diagnostic_logging_enabled() -> bool {
    moriq_library::settings::load().is_ok_and(|settings| settings.diagnostic_logging)
}
pub fn is_video_format(format: &str) -> bool {
    moriq_library::video::EXTENSIONS
        .iter()
        .any(|extension| format.eq_ignore_ascii_case(extension))
}
pub use moriq_ai::{DownloadControl, Hardware, ModelInfo};
pub use moriq_library::orphans::Plan as OrphanPlan;
pub use moriq_library::region::Session as RegionSession;
pub use moriq_library::video::Playback;

pub async fn open_source_repository() -> Result<()> {
    launch::open(SOURCE_REPOSITORY_URL.into(), false).await
}

type Job = Box<dyn FnOnce(&mut Option<Library>) + Send>;

#[derive(Clone)]
pub struct Service {
    workspace: Option<ProjectId>,
    media_cache: Arc<cache::MediaCache>,
    inference: Arc<dyn inference::InferenceRunner>,
    queue: mpsc::Sender<Job>,
    import_slots: Arc<Semaphore>,
    import_memory: Arc<Semaphore>,
    maintenance_slot: Arc<Semaphore>,
    read_slots: Arc<Semaphore>,
    job_slot: Arc<Semaphore>,
    job_cancel: Arc<scheduler::Cancellation>,
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
    pub categories: Vec<(String, Option<Pixels>)>,
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
    pub waiting_memory: Arc<AtomicUsize>,
    pub largest_mib: Arc<AtomicUsize>,
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
            memory_mib: ImportPreferences::default().memory_mib,
            discovered: Arc::default(),
            scanning: Arc::default(),
            active: Arc::default(),
            peak: Arc::default(),
            waiting_memory: Arc::default(),
            largest_mib: Arc::default(),
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
    /// A request owns its project context; changing the UI cannot retarget queued work.
    pub fn for_project(&self, project: Option<ProjectId>) -> Self {
        Self {
            workspace: project,
            ..self.clone()
        }
    }
    pub fn workspace_project(&self) -> Option<ProjectId> {
        self.workspace
    }
    pub async fn close(&self) -> Result<()> {
        self.job_cancel.stop(None);
        self.media_cache.clear();
        self.call(|active| {
            *active = None;
            Ok(())
        })
        .await?;
        // Wait for the owned inference process before allowing the desktop to exit.
        tokio::task::spawn_blocking(|| {
            for _ in 0..200 {
                if moriq_ai::unload_llama().is_ok() {
                    return Ok(());
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            Err("模型仍在停止，请稍后再次关闭应用".to_string())
        })
        .await
        .map_err(|e| e.to_string())?
    }
    pub fn start() -> std::io::Result<Self> {
        Self::start_with_inference(Arc::new(inference::LocalInference))
    }
    pub fn start_with_inference(
        inference: Arc<dyn inference::InferenceRunner>,
    ) -> std::io::Result<Self> {
        let (queue, mut receiver) = mpsc::channel::<Job>(16);
        std::thread::Builder::new()
            .name("moriq-library-writer".into())
            .spawn(move || {
                let mut active = None;
                while let Some(job) = receiver.blocking_recv() {
                    job(&mut active);
                }
            })?;
        let service = Self {
            workspace: None,
            queue,
            inference,
            import_slots: Arc::new(Semaphore::new(32)),
            import_memory: Arc::new(Semaphore::new(4096)),
            maintenance_slot: Arc::new(Semaphore::new(1)),
            read_slots: Arc::new(Semaphore::new(4)),
            media_cache: Arc::new(Default::default()),
            job_slot: Arc::new(Semaphore::new(1)),
            job_cancel: Arc::new(Default::default()),
        };
        scheduler::start(&service)?;
        Ok(service)
    }
    async fn call<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Option<Library>) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        self.dispatch(false, f).await
    }
    async fn call_task<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Option<Library>) -> TaskResult<T> + Send + 'static,
    ) -> TaskResult<T> {
        self.dispatch(false, f).await
    }
    async fn dispatch<T: Send + 'static, E: From<DispatchError> + Send + 'static>(
        &self,
        cancellable: bool,
        f: impl FnOnce(&mut Option<Library>) -> std::result::Result<T, E> + Send + 'static,
    ) -> std::result::Result<T, E> {
        let (sender, receiver) = oneshot::channel();
        let workspace = self.workspace;
        self.queue
            .send(Box::new(move |active| {
                if cancellable && sender.is_closed() {
                    return;
                }
                if let Some(library) = active.as_mut() {
                    library.set_workspace(workspace);
                }
                let result = f(active);
                if let Some(library) = active.as_mut() {
                    library.set_workspace(None);
                }
                if sender.send(result).is_err() {
                    tracing::debug!("caller no longer listening to worker response");
                }
            }))
            .await
            .map_err(|_| E::from(DispatchError::Closed))?;
        receiver
            .await
            .map_err(|_| E::from(DispatchError::Interrupted))?
    }
    pub async fn open(&self, path: PathBuf) -> Result<Opened> {
        for attempt in 0..=20 {
            let cancellation = self.job_cancel.clone();
            let candidate = path.clone();
            let result = self.call(move |active| {
                if let Some(lib) = active.as_ref()
                    && lib.root() == candidate.canonicalize().map_err(|e| e.to_string())?
                {
                    return Ok(Some(Opened { info: lib.info().map_err(|e|e.to_string())?, root:lib.root().into() }));
                }
                let library = match Library::open(&candidate) {
                    Ok(library) => library,
                    Err(moriq_library::Error::Locked) => return Ok(None),
                    Err(error) => return Err(error.to_string()),
                };
                if active.as_ref().is_some_and(|current|current.id()==library.id()) {
                    return Err("检测到与当前库身份相同的副本。请关闭应用后打开迁移后的库；需要独立使用时请选择创建独立克隆。".into());
                }
                let opened = Opened { info:library.info().map_err(|e|e.to_string())?, root:library.root().into() };
                cancellation.stop(None);
                *active=Some(library);
                Ok(Some(opened))
            }).await?;
            if let Some(opened) = result {
                return Ok(opened);
            }
            if attempt < 20 {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
        Err("资源库仍在使用中；后台任务可能正在收尾，也请检查其他打开此库的窗口。".into())
    }
    pub async fn create(&self, parent: PathBuf, name: String) -> Result<Opened> {
        let cancellation = self.job_cancel.clone();
        self.call(move |active| {
            let template = moriq_library::settings::load()
                .map_err(|e| e.to_string())?
                .category_template;
            let mut library = Library::create(&parent, &name).map_err(|e| e.to_string())?;
            if let Some(template) = template {
                let profile = library
                    .ai_profile()
                    .map_err(|e| e.to_string())?
                    .with_template(&template)
                    .map_err(str::to_owned)?;
                library
                    .save_ai_profile(profile)
                    .map_err(|e| e.to_string())?;
            }
            let opened = Opened {
                info: library.info().map_err(|e| e.to_string())?,
                root: library.root().into(),
            };
            cancellation.stop(None);
            *active = Some(library);
            Ok(opened)
        })
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
}

enum DispatchError {
    Closed,
    Interrupted,
}
impl From<DispatchError> for String {
    fn from(error: DispatchError) -> Self {
        match error {
            DispatchError::Closed => "后台服务已关闭，请重新打开 Moriq Space".into(),
            DispatchError::Interrupted => "后台服务中断，请重新打开 Moriq Space".into(),
        }
    }
}
impl From<DispatchError> for TaskError {
    fn from(_: DispatchError) -> Self {
        Self::Unavailable
    }
}
fn check_task(active: &mut Option<Library>, expected: LibraryId) -> TaskResult<&mut Library> {
    active
        .as_mut()
        .filter(|library| library.id() == expected)
        .ok_or(TaskError::StaleLibrary)
}
fn storage_error(error: moriq_library::Error) -> TaskError {
    match error {
        moriq_library::Error::Invalid(message) => TaskError::InvalidInput(message),
        error => TaskError::Storage(error.to_string()),
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
mod tests;
