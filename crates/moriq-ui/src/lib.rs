//! Native presentation. All disk, catalog and decoding work belongs to Service.
#[macro_use]
mod i18n;
mod ai;
mod batch;
mod categories;
mod export;
mod favorites;
mod filters;
mod folders;
use export::ExportChange;
use filters::{Builtin, FilterChange};
mod libraries;
mod lifecycle;
mod metadata;
mod region_viewer;
mod regions;
mod selection;
mod uploads;
mod viewer;
mod viewer_motion;
pub(crate) use ai::AiMessage;
pub(crate) use batch::BatchMessage;
pub(crate) use folders::FolderMessage;
pub(crate) use lifecycle::LifecycleMessage;
pub(crate) use metadata::MetadataMessage;
pub(crate) use regions::RegionMessage;
mod browsing;
#[cfg(feature = "capture")]
mod capture;
mod compare;
mod deletion;
mod drop_import;
mod polling;
use drop_import::DropMessage;
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod interface_tests;
mod popups;
mod project_form;
mod settings;
mod surfaces;
mod tasks;
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod ui_regression_tests;
pub(crate) use deletion::DeletionMessage;
mod components;
mod gallery;
pub(crate) use gallery::TagChoice;
mod icons;
mod journal;
mod rules;
pub(crate) use compare::CompareMessage;
mod startup;
mod storage;
pub(crate) use rules::RuleMessage;
pub(crate) use storage::StorageMessage;
mod manage;
mod models;
mod navigation;
pub(crate) use journal::JournalMessage;
pub(crate) use manage::ManageMessage;
mod liquid;
#[cfg(feature = "capture")]
mod liquid_proof;
mod motion;
#[cfg(feature = "capture")]
mod motion_proof;
mod overview;
mod preview;
mod responsiveness;
mod theme;
mod transition;
mod view;
mod welcome;
use iced::widget::{image, scrollable};
use iced::{Element, Font, Size, Subscription, Task, Theme};
use moriq_app::{ImportControl, LoadedPage, Opened, Service};
use moriq_domain::*;
use moriq_render::Grid;
use std::{collections::HashMap, path::PathBuf, sync::atomic::Ordering, time::Duration};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Screen {
    Overview,
    Assets,
    Models,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dialog {
    CreateLibrary,
    CreateProject,
    RenameLibrary,
    RenameAsset,
    Settings,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    SelectVisible,
    Trash,
    Viewer,
    Help,
    BatchAll,
    Export,
    Favorite,
    Batch,
    Rating(u8),
}
#[derive(Debug, Clone)]
pub(crate) enum Message {
    DpiChanged(f32),
    Regions(RegionMessage),
    UploadTick,
    PersistNavigation,
    NavigationLoaded(LibraryId, u64, Result<Option<SavedNavigation>, String>),
    NavigationSaved(LibraryId, Result<(), String>),
    AiPolicySaved(AiPreferences, Result<UserSettings, String>),
    Lifecycle(LifecycleMessage),
    Batch(BatchMessage),
    Metadata(MetadataMessage),
    Folder(FolderMessage),
    Ai(AiMessage),
    TagFilter(TagChoice),
    CursorMoved(iced::Point),
    OverlayScrolled(f32, f32),
    PageScrolled(u8, f32, f32),
    GalleryViewport(f32, f32),
    Deletion(DeletionMessage),
    OpenImage(AssetId),
    AssetContext(AssetId),
    Compare(CompareMessage),
    Shortcut(Shortcut),
    Rules(RuleMessage),
    Storage(StorageMessage),
    Journal(JournalMessage),
    Manage(ManageMessage),
    Modifiers(iced::keyboard::Modifiers),
    SettingsLoaded(Result<UserSettings, String>),
    LibraryCardsLoaded(u64, Result<Vec<moriq_app::LibraryCard>, String>),
    OpenRecent(PathBuf),
    Libraries(libraries::LibraryMessage),
    ShowLibraries,
    Name(String),
    Create,
    Open,
    PickedParent(Option<PathBuf>),
    PickedLibrary(Option<PathBuf>),
    Opened(Result<Opened, String>),
    Navigate(u8),
    Back,
    Forward,
    Browse(i8),
    NeighborhoodLoaded(
        LibraryId,
        u64,
        Result<Option<moriq_app::LoadedNeighborhood>, String>,
    ),
    Search(String),
    SearchReady(u64),
    CancelSearch,
    GeometryReady(u64),
    Favorites,
    AllAssets,
    Project(ProjectId),
    Import,
    Drop(DropMessage),
    PickedImages(Option<Vec<PathBuf>>),
    ImportPreference(u8, String),
    SaveImportPreferences,
    ImportPreferencesSaved(ImportPreferences, Result<UserSettings, String>),
    Imported(ImportReport),
    CancelImport,
    Tick,
    TransitionFrame(std::time::Instant),
    PageLoaded(u64, Result<LoadedPage, String>),
    Scrolled(f32),
    Resized(Size),
    Zoom(f32),
    List(bool),
    Select(AssetId),
    CardPressed(AssetId),
    FullPreviewLoaded(
        LibraryId,
        AssetId,
        AssetVersionId,
        Result<(image::Handle, image::Handle), String>,
    ),
    ClosePreview,
    Jobs(tasks::JobMessage),
    PlaybackStarted(
        LibraryId,
        AssetId,
        AssetVersionId,
        u64,
        Result<std::sync::Arc<moriq_app::Playback>, String>,
    ),
    VideoFrame,
    VideoUploaded(u64, Result<iced_runtime::image::Allocation, String>),
    VideoPause,
    VideoMute,
    VideoSeek(f32),
    VideoControlled(u64, Result<(), String>),
    ExpandPreview,
    Favorite(AssetId, bool),
    FavoriteSaved(LibraryId, AssetId, u64, Result<(), String>),
    Rating(u8),
    Tags(String),
    SaveTags,
    Saved(LibraryId, Result<(), String>),
    VisualsSaved(LibraryId, Result<(), String>),
    InfoLoaded(u64, Result<LibraryInfo, String>),
    ProjectName(String),
    CreateProject,
    ProjectCreated(LibraryId, Result<ProjectId, String>),
    Dismiss,
    DiagnosticLogging(bool),
    ReducedMotion(bool),
    LanguageChanged(Language),
    AppearanceChanged(Appearance),
    InterfaceSaved(Language, Appearance, Result<(), String>),
    CardHovered(AssetId, bool),
    #[cfg(feature = "capture")]
    Captured(iced::window::Screenshot),
    #[cfg(feature = "capture")]
    LiquidProof(u8),
    #[cfg(feature = "capture")]
    MotionProof(u8),
    #[cfg(feature = "capture")]
    MotionFrame(u8, iced::window::Screenshot),
    #[cfg(feature = "capture")]
    LiquidFrame(u8, iced::window::Screenshot),
    #[cfg(feature = "capture")]
    VideoProof(iced::window::Screenshot),
    #[cfg(feature = "capture")]
    InteractionSnapshot(iced::window::Screenshot),
    #[cfg(feature = "capture")]
    CaptureSaved(Result<(), String>),
    MasonryLoaded(
        u64,
        Result<std::sync::Arc<moriq_render::MasonryIndex>, String>,
    ),
    WindowDrag,
    WindowMinimize,
    WindowMaximize,
    WindowClose,
    ShowCreate,
    HideCreate,
    ShowProject,
    Notice(String),
    DetailTab(u8),
    SetMasonry(bool),
    CopyLibraryPath,
    CopyModelsPath,
    RevealLibrary,
    ToggleTagEditor,
    ToggleModelPolicy,
    ToggleModelMirror,
    ToggleRetainIndexes,
    PlatformDone(&'static str, Result<(), String>),
    OpenSourceRepository,
    OpenModelsDirectory,
    ShutdownReady(Result<(), String>),
    ShowRenameLibrary,
    ShowRenameAsset,
    SaveRenameLibrary,
    SaveRenameAsset,
    LibraryRenamed(LibraryId, Result<LibraryInfo, String>),
    AssetRenamed(LibraryId, AssetId, Result<(), String>),
    Noop,
    ShowSettings,
    FocusSearch,
    ModelSearch(String),
    ModelFilter(u8),
}
#[derive(Clone, Default)]
pub struct LaunchOptions {
    pub window_icon: Option<iced::window::Icon>,
    pub library: Option<PathBuf>,
    pub import_paths: Vec<PathBuf>,
    pub capture: Option<PathBuf>,
    pub screen: u8,
    pub exit_after_capture: bool,
    pub select_first: bool,
    pub panel: Option<String>,
}
#[derive(Clone)]
struct Tile {
    asset: Asset,
    handle: Option<image::Handle>,
}
pub(crate) struct App {
    responsiveness: responsiveness::State,
    drop_import: drop_import::State,
    viewer: viewer::ViewerState,
    dpi_scale: f32,
    gallery_feedback: motion::HoverFeedback,
    uploads: std::collections::VecDeque<(AssetVersionId, moriq_app::Pixels)>,
    upload_generation: u64,
    upload_owner: Option<LibraryId>,
    lifecycle: lifecycle::LifecycleState,
    batch: batch::BatchState,
    metadata: metadata::MetadataState,
    folders: folders::FolderState,
    ai: ai::AiState,
    favorites: favorites::Favorites,
    surfaces: surfaces::Surfaces,
    deletion: deletion::Deletion,
    storage: storage::Storage,
    journal: journal::Journal,
    history: navigation::History,
    pending_scroll: Option<f32>,
    restored_notes: Option<(AssetId, String)>,
    browse_request: u64,
    browse_task: Option<iced::task::Handle>,
    neighbors: Vec<Tile>,
    neighbor_position: Option<(usize, u64)>,
    transition: Option<transition::PageTransition>,
    surface_motion: Option<transition::Entrance>,
    modifiers: iced::keyboard::Modifiers,
    manager: manage::Manager,
    preferences: UserSettings,
    interface: settings::InterfaceState,
    libraries: libraries::State,
    library_covers: HashMap<PathBuf, image::Handle>,
    library_summaries: HashMap<PathBuf, (u64, u64)>,
    library_picker: bool,
    service: Service,
    library: Option<Opened>,
    screen: Screen,
    name: String,
    query: Query,
    tiles: Vec<Tile>,
    total: u64,
    offset: usize,
    generation: u64,
    loading: bool,
    switching: bool,
    jobs: tasks::TaskState,
    compare: compare::Compare,
    tag_input: String,
    project_name: String,
    status: String,
    error: Option<String>,
    import: Option<ImportControl>,
    scroll_y: f32,
    viewport: Size,
    card_width: f32,
    list: bool,
    launch: LaunchOptions,
    #[cfg(feature = "capture")]
    capture_ticks: u8,
    #[cfg(feature = "capture")]
    capture_stress_steps: u16,
    projection_request: u64,
    overview_request: u64,
    cover_request: u64,
    cover_write_request: u64,
    library_cards_request: u64,
    library_cards_task: Option<iced::task::Handle>,
    navigation_request: u64,
    page_task: Option<iced::task::Handle>,
    masonry: Option<std::sync::Arc<moriq_render::MasonryIndex>>,
    masonry_task: Option<iced::task::Handle>,
    masonry_mode: bool,
    dialog: Option<Dialog>,
    detail_tab: u8,
    model_tab: u8,
    layout_generation: u64,
    editing_tags: bool,
    on_demand_models: bool,
    retain_indexes: bool,
    model_search: String,
    model_filter: u8,
}

/// 根据平台构造原生窗口标题栏及边框配置。
fn native_window_style() -> iced::window::settings::PlatformSpecific {
    #[cfg(target_os = "windows")]
    {
        iced::window::settings::PlatformSpecific {
            corner_preference: iced::window::settings::platform::CornerPreference::Round,
            undecorated_shadow: true,
            ..Default::default()
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        Default::default()
    }
}

/// 取得当前诊断日志配置，供启动和事件记录使用。
pub fn diagnostic_logging_enabled() -> bool {
    moriq_app::diagnostic_logging_enabled()
}
/// 建立 Iced 应用与窗口设置，绑定状态更新、视图和事件订阅。
pub fn run(launch: LaunchOptions) -> iced::Result {
    let initial_size = Size::new(1660.0, 940.0);
    let window_icon = launch.window_icon.clone();
    let service = Service::start().map_err(iced::Error::ExecutorCreationFailed)?;
    iced::application(
        move || App::new(service.clone(), launch.clone()),
        App::update,
        App::view,
    )
    .title("Moriq Space")
    .theme(|app: &App| theme::application_theme(app.preferences.appearance))
    .default_font(Font {
        family: iced::font::Family::Name("Microsoft YaHei UI"),
        ..Font::DEFAULT
    })
    .window(iced::window::Settings {
        icon: window_icon,
        size: initial_size,
        min_size: Some(Size::new(1180.0, 740.0)),
        decorations: false,
        platform_specific: native_window_style(),
        exit_on_close_request: false,
        ..Default::default()
    })
    .antialiasing(true)
    .style(|_: &App, theme: &Theme| iced::theme::Style {
        background_color: theme.palette().background,
        text_color: theme.palette().text,
    })
    .subscription(App::subscription)
    .run()
}
impl App {
    /// 初始化应用状态、服务及启动任务，不在界面线程执行媒体读取。
    fn new(service: Service, launch: LaunchOptions) -> (Self, Task<Message>) {
        let settings_service = service.clone();
        let settings = Task::perform(
            async move { settings_service.settings().await },
            Message::SettingsLoaded,
        );
        let open = launch
            .library
            .clone()
            .map(|path| {
                let service = service.clone();
                Task::perform(async move { service.open(path).await }, Message::Opened)
            })
            .unwrap_or_else(Task::none);
        (
            Self {
                responsiveness: Default::default(),
                drop_import: Default::default(),
                viewer: Default::default(),
                storage: Default::default(),
                journal: Default::default(),
                history: Default::default(),
                favorites: Default::default(),
                pending_scroll: None,
                restored_notes: None,
                browse_request: 0,
                browse_task: None,
                neighbors: vec![],
                neighbor_position: None,
                transition: None,
                surface_motion: None,
                manager: Default::default(),
                preferences: Default::default(),
                interface: Default::default(),
                libraries: Default::default(),
                library_covers: HashMap::new(),
                library_summaries: HashMap::new(),
                library_picker: false,
                service,
                library: None,
                screen: match launch.screen {
                    1 => Screen::Assets,
                    2 => Screen::Models,
                    _ => Screen::Overview,
                },
                name: "我的素材库".into(),
                query: Query::default(),
                tiles: vec![],
                total: 0,
                offset: 0,
                generation: 0,
                loading: false,
                switching: launch.library.is_some(),
                modifiers: iced::keyboard::Modifiers::empty(),
                dpi_scale: 1.0,
                gallery_feedback: Default::default(),
                uploads: Default::default(),
                upload_generation: 0,
                upload_owner: None,
                lifecycle: Default::default(),
                batch: Default::default(),
                metadata: Default::default(),
                folders: Default::default(),
                ai: Default::default(),
                jobs: Default::default(),
                surfaces: surfaces::Surfaces::default(),
                deletion: deletion::Deletion::default(),
                compare: compare::Compare::default(),
                tag_input: String::new(),
                project_name: String::new(),
                status: "本地优先 · 灵感随时可用".into(),
                error: None,
                import: None,
                scroll_y: 0.0,
                viewport: Size::new(1660.0, 940.0),
                card_width: 210.0,
                list: false,
                launch,
                #[cfg(feature = "capture")]
                capture_ticks: 0,
                #[cfg(feature = "capture")]
                capture_stress_steps: 0,
                projection_request: 0,
                overview_request: 0,
                cover_request: 0,
                cover_write_request: 0,
                library_cards_request: 0,
                library_cards_task: None,
                navigation_request: 0,
                page_task: None,
                masonry: None,
                masonry_task: None,
                masonry_mode: true,
                dialog: None,
                detail_tab: 0,
                model_tab: 0,
                layout_generation: 0,
                editing_tags: false,
                on_demand_models: true,
                retain_indexes: true,
                model_search: String::new(),
                model_filter: 2,
            },
            Task::batch([
                open,
                settings,
                iced::window::oldest()
                    .and_then(iced::window::scale_factor)
                    .map(Message::DpiChanged),
            ]),
        )
    }
    /// 根据当前布局参数构造固定网格索引。
    fn grid(&self) -> Grid {
        let available = self.gallery_width();
        let viewport_height = (self.viewport.height - 250.0).max(300.0);
        let mut columns = if self.list {
            1
        } else {
            (available / (self.card_width + 12.0)).floor().max(1.0) as usize
        };
        loop {
            let width = (available - (columns - 1) as f32 * 12.0) / columns as f32;
            let row_height = if self.list { 84.0 } else { width * 0.78 + 52.0 };
            // Preserve the 128-thumbnail page budget on high-resolution displays.
            // Larger cards reduce the visible row count instead of creating a reload loop.
            if columns == 1 || ((viewport_height / row_height).ceil() as usize + 5) * columns <= 128
            {
                return Grid {
                    columns,
                    row_height: row_height.max(viewport_height / 123.0),
                    viewport_height,
                };
            }
            columns -= 1;
        }
    }
    /// 把界面所选项目同步到服务句柄，保持后续请求的项目范围一致。
    fn sync_project_workspace(&mut self) -> bool {
        if self.service.workspace_project() == self.query.project {
            return false;
        }
        self.service = self.service.for_project(self.query.project);
        self.projection_request = self.projection_request.wrapping_add(1);
        self.overview_request = self.overview_request.wrapping_add(1);
        self.manager.organizer = Default::default();
        self.manager.favorites.clear();
        self.manager.category_covers.clear();
        let request = self.jobs.request.wrapping_add(1);
        self.jobs = tasks::TaskState {
            request,
            ..Default::default()
        };
        self.manager.filters.query = self.query.clone();
        self.stop_people_portraits();
        self.ai.portraits.clear();
        self.ai.person = None;
        self.ai.target = None;
        let request = self.ai.categories.request.wrapping_add(1);
        self.ai.categories = categories::State {
            request,
            ..Default::default()
        };
        true
    }
    /// 发起当前可见页读取，并绑定查询及库身份以拒绝旧回执。
    fn refresh(&mut self) -> Task<Message> {
        if self.responsiveness.pending() || self.screen != Screen::Assets || self.library_picker {
            return Task::none();
        }
        if self.responsiveness.replacing() && self.page_task.is_some() {
            return Task::none();
        }
        if let Some(handle) = self.page_task.take() {
            handle.abort();
        }
        let Some(library) = self.library.as_ref() else {
            return Task::none();
        };
        self.generation += 1;
        let generation = self.generation;
        let range = if self.responsiveness.replacing() {
            self.grid().visible(self.total as usize, 0.0)
        } else {
            self.visible_range()
        };

        let service = self.service.clone();
        let id = library.info.id;
        let query = self.query.clone();
        self.loading = true;
        let (task, handle) = Task::perform(
            async move {
                service
                    .page(id, query, range.start, range.len().max(40))
                    .await
            },
            move |page| Message::PageLoaded(generation, page),
        )
        .abortable();
        self.page_task = Some(handle);
        task
    }
    /// 异步刷新资源库基本资料。
    fn refresh_info(&self) -> Task<Message> {
        let Some(library) = self.library.as_ref() else {
            return Task::none();
        };
        let service = self.service.clone();
        let id = library.info.id;
        let generation = self.generation;
        Task::perform(async move { service.info(id).await }, move |result| {
            Message::InfoLoaded(generation, result)
        })
    }
    /// 切换查询时清理相关加载与选择状态，防止旧位置套到新查询。
    fn reset_query(&mut self) -> Task<Message> {
        self.responsiveness.cancel();
        self.invalidate_catalog_loads();
        let workspace = if self.sync_project_workspace() {
            Task::batch([self.refresh_organizer(), self.refresh_visuals()])
        } else {
            Task::none()
        };
        self.gallery_feedback.clear();
        self.clear_browsing();
        self.pending_scroll = None;
        self.surfaces.restoring_gallery = None;
        self.scroll_y = 0.0;
        self.offset = 0;
        self.tiles.clear();
        self.manager.selection.clear();
        self.masonry = None;
        let page = self.refresh();
        let layout = if self.screen == Screen::Assets {
            self.rebuild_masonry()
        } else {
            Task::none()
        };
        Task::batch([
            workspace,
            page,
            layout,
            iced::widget::operation::scroll_to(
                "gallery",
                scrollable::AbsoluteOffset { x: 0.0, y: 0.0 },
            ),
        ])
    }
    /// 封装数据修改任务及完成消息，供各界面操作复用。
    fn mutation(
        &self,
        f: impl std::future::Future<Output = Result<(), String>> + Send + 'static,
    ) -> Task<Message> {
        let Some(library) = self.library.as_ref() else {
            return Task::none();
        };
        let id = library.info.id;
        Task::perform(f, move |result| Message::Saved(id, result))
    }
    /// 按当前活动状态订阅窗口、键盘、轮询和动画事件。
    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            if self.transition.is_some()
                || self.surface_motion.is_some()
                || self.gallery_feedback.active()
            {
                iced::time::every(Duration::from_millis(16)).map(Message::TransitionFrame)
            } else {
                Subscription::none()
            },
            if self.viewer.playback.is_some() {
                iced::time::every(Duration::from_millis(16)).map(|_| Message::VideoFrame)
            } else {
                Subscription::none()
            },
            iced::window::close_requests().map(|_| Message::WindowClose),
            iced::window::resize_events().map(|(_, size)| Message::Resized(size)),
            iced::time::every(Duration::from_millis(250)).map(|_| Message::Tick),
            iced::time::every(Duration::from_secs(5)).map(|_| Message::PersistNavigation),
            iced::time::every(Duration::from_secs(300))
                .map(|_| Message::Storage(StorageMessage::Trim(false))),
            if self.uploads.is_empty() {
                Subscription::none()
            } else {
                iced::time::every(Duration::from_millis(16)).map(|_| Message::UploadTick)
            },
            iced::event::listen_with(|event, status, _| match event {
                iced::Event::Window(iced::window::Event::Rescaled(scale)) => {
                    Some(Message::DpiChanged(scale))
                }
                iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
                    Some(Message::CursorMoved(position))
                }
                iced::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(modifiers)) => {
                    Some(Message::Modifiers(modifiers))
                }
                iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key: iced::keyboard::Key::Character(value),
                    modifiers,
                    ..
                }) if modifiers.command() && value.eq_ignore_ascii_case("k") => {
                    Some(Message::FocusSearch)
                }
                iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key: iced::keyboard::Key::Character(value),
                    modifiers,
                    ..
                }) if matches!(status, iced::event::Status::Ignored)
                    && modifiers.command()
                    && modifiers.shift()
                    && value.eq_ignore_ascii_case("a") =>
                {
                    Some(Message::Shortcut(Shortcut::BatchAll))
                }
                iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key: iced::keyboard::Key::Character(value),
                    modifiers,
                    ..
                }) if matches!(status, iced::event::Status::Ignored)
                    && modifiers.command()
                    && value.eq_ignore_ascii_case("e") =>
                {
                    Some(Message::Shortcut(Shortcut::Export))
                }
                iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key: iced::keyboard::Key::Character(value),
                    modifiers,
                    ..
                }) if matches!(status, iced::event::Status::Ignored) && modifiers.is_empty() => {
                    match value.as_str() {
                        "0" | "1" | "2" | "3" | "4" | "5" => Some(Message::Shortcut(
                            Shortcut::Rating(value.as_bytes()[0] - b'0'),
                        )),
                        "f" | "F" => Some(Message::Shortcut(Shortcut::Favorite)),
                        "b" | "B" => Some(Message::Shortcut(Shortcut::Batch)),
                        _ => None,
                    }
                }
                iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key: iced::keyboard::Key::Character(value),
                    modifiers,
                    ..
                }) if matches!(status, iced::event::Status::Ignored)
                    && modifiers.command()
                    && value.eq_ignore_ascii_case("a") =>
                {
                    Some(Message::Shortcut(Shortcut::SelectVisible))
                }
                iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key: iced::keyboard::Key::Named(key),
                    modifiers,
                    ..
                }) if matches!(status, iced::event::Status::Ignored)
                    && modifiers.is_empty()
                    && matches!(
                        key,
                        iced::keyboard::key::Named::Delete
                            | iced::keyboard::key::Named::Enter
                            | iced::keyboard::key::Named::Space
                            | iced::keyboard::key::Named::F1
                    ) =>
                {
                    Some(Message::Shortcut(match key {
                        iced::keyboard::key::Named::Delete => Shortcut::Trash,
                        iced::keyboard::key::Named::Enter | iced::keyboard::key::Named::Space => {
                            Shortcut::Viewer
                        }
                        _ => Shortcut::Help,
                    }))
                }
                iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key: iced::keyboard::Key::Named(key),
                    modifiers,
                    ..
                }) if modifiers.alt() && key == iced::keyboard::key::Named::ArrowLeft => {
                    Some(Message::Back)
                }
                iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key: iced::keyboard::Key::Named(key),
                    modifiers,
                    ..
                }) if modifiers.alt() && key == iced::keyboard::key::Named::ArrowRight => {
                    Some(Message::Forward)
                }
                iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key: iced::keyboard::Key::Named(key),
                    modifiers,
                    ..
                }) if matches!(status, iced::event::Status::Ignored)
                    && modifiers.is_empty()
                    && matches!(
                        key,
                        iced::keyboard::key::Named::ArrowLeft
                            | iced::keyboard::key::Named::ArrowRight
                    ) =>
                {
                    Some(Message::Browse(
                        if key == iced::keyboard::key::Named::ArrowLeft {
                            -1
                        } else {
                            1
                        },
                    ))
                }
                iced::Event::Window(event) => drop_import::window_event(event).map(Message::Drop),
                iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                    ..
                }) => Some(Message::ClosePreview),
                _ => None,
            }),
        ])
    }
    /// 处理应用业务消息与异步回执，维护页面、资源库和素材请求的状态归属。
    fn update_inner(&mut self, message: Message) -> Task<Message> {
        let language = self.preferences.language;
        match message {
            Message::TagFilter(choice) => {
                let tag = match choice {
                    TagChoice::More => return self.manage_message(ManageMessage::Tags),
                    TagChoice::All => String::new(),
                    TagChoice::Tag(tag) => tag,
                };
                if self.query.tag == tag {
                    return Task::none();
                }
                self.query.tag = tag;
                return self.reset_query();
            }
            Message::CursorMoved(position) => self.surfaces.cursor = Some(position),
            Message::OverlayScrolled(y, max) => {
                if self
                    .surfaces
                    .overlay_restore
                    .is_some_and(|target| target.min(max) > 0.0)
                    && y == 0.0
                {
                    return Task::none();
                }
                self.surfaces.y = y;
                self.surfaces.overlay_restore = None;
            }
            Message::GalleryViewport(y, max) => {
                if max <= 0.0 {
                    self.pending_scroll = None;
                    self.surfaces.restoring_gallery = None;
                }
                return self.update_inner(Message::Scrolled(y));
            }
            Message::PageScrolled(index, y, max) if usize::from(index) < 3 => {
                let current = match self.screen {
                    Screen::Overview => 0,
                    Screen::Assets => 1,
                    Screen::Models => 2,
                };
                if index != current {
                    return Task::none();
                }
                if self
                    .surfaces
                    .page_restore
                    .is_some_and(|(i, target)| i == index && target.min(max) > 0.0 && y == 0.0)
                {
                    return Task::none();
                }
                self.surfaces.page_y[usize::from(index)] = y;
                self.surfaces.page_restore = None;
            }
            Message::PageScrolled(_, _, _) => {}
            Message::Deletion(event) => return self.deletion_message(event),
            event @ (Message::OpenImage(id) | Message::AssetContext(id)) => {
                if self.dialog.is_some()
                    || self.manager.panel.is_some()
                    || self.switching
                    || self.screen != Screen::Assets
                {
                    return Task::none();
                }
                let menu = matches!(event, Message::AssetContext(_));
                if !self
                    .tiles
                    .iter()
                    .chain(self.neighbors.iter())
                    .any(|t| t.asset.id == id)
                {
                    return Task::none();
                }
                if menu && self.manager.selection.enabled {
                    if !self.manager.selection.assets.contains(&id) {
                        self.manager.selection.clear();
                        self.manager.selection.assets.insert(id);
                        self.manager.selection.anchor = Some(id);
                    }
                    self.viewer.preview_expanded = false;
                    self.manager.panel = Some(manage::Panel::AssetMenu);
                    return Task::none();
                }
                self.manager.selection.enabled = false;
                // A context click selects its target without re-entering navigation.
                let task = self.update_inner(Message::Select(id));
                if self.viewer.selected.as_ref().is_some_and(|a| a.id == id) {
                    if menu {
                        self.viewer.preview_expanded = false;
                        self.manager.panel = Some(manage::Panel::AssetMenu);
                    } else {
                        return Task::batch([task, self.set_viewer(true)]);
                    }
                }
                return task;
            }
            Message::Compare(event) => return self.compare_message(event),
            Message::Shortcut(which) => {
                if self.dialog.is_some()
                    || self.manager.panel.is_some()
                    || self.switching
                    || self.loading
                {
                    return Task::none();
                }
                if which == Shortcut::Help {
                    return self.manage_message(ManageMessage::Help);
                }
                if self.screen != Screen::Assets || self.library.is_none() {
                    return Task::none();
                }
                match which {
                    Shortcut::SelectVisible if !self.viewer.preview_expanded => {
                        self.manager.selection.enabled = true;
                        self.manager.selection.additive = false;
                        self.manager.selection.assets =
                            self.tiles.iter().map(|t| t.asset.id).collect();
                    }
                    Shortcut::Trash
                        if !self.query.trash
                            && !self.manager.busy
                            && !self.viewer.preview_expanded =>
                    {
                        return self.manage_message(ManageMessage::Edit(Edit::Trash(true)));
                    }
                    Shortcut::Viewer if self.viewer.selected.is_some() => {
                        return self.set_viewer(!self.viewer.preview_expanded);
                    }
                    Shortcut::BatchAll => {
                        let task = self.batch_message(batch::BatchMessage::Open);
                        let all = self.batch_message(batch::BatchMessage::All(true));
                        return Task::batch([task, all]);
                    }
                    Shortcut::Export => return self.manage_message(ManageMessage::ExportMenu),
                    Shortcut::Favorite if !self.query.trash => {
                        if self.manager.selection.enabled
                            && !self.manager.selection.assets.is_empty()
                        {
                            let favorite = !self.manager.selection.assets.iter().all(|id| {
                                self.tiles
                                    .iter()
                                    .chain(self.neighbors.iter())
                                    .find(|tile| tile.asset.id == *id)
                                    .is_some_and(|tile| tile.asset.favorite)
                            });
                            return self
                                .manage_message(ManageMessage::Edit(Edit::Favorite(favorite)));
                        } else if let Some(asset) = &self.viewer.selected {
                            return self.manage_message(ManageMessage::Edit(Edit::Favorite(
                                !asset.favorite,
                            )));
                        }
                    }
                    Shortcut::Batch => return self.batch_message(batch::BatchMessage::Open),
                    Shortcut::Rating(rating) if !self.query.trash => {
                        return self.manage_message(ManageMessage::Edit(Edit::Rating(rating)));
                    }

                    _ => {}
                }
            }
            Message::Storage(event) => return self.storage_message(event),
            Message::Rules(event) => return self.rule_message(event),
            Message::Journal(message) => return self.journal_message(message),
            Message::Back => {
                if self.surface_open() {
                    return self.close_surface();
                }
                if self.viewer.preview_expanded {
                    return self.set_viewer(false);
                }
                return self.travel(false);
            }
            Message::Forward => {
                if self.surface_open() || self.viewer.preview_expanded {
                    return Task::none();
                }
                return self.travel(true);
            }
            Message::Browse(direction) => {
                if self.dialog.is_none()
                    && self.manager.panel.is_none()
                    && let Some(asset) = &self.viewer.selected
                {
                    return self.load_neighborhood(asset.id, direction);
                }
            }
            Message::NeighborhoodLoaded(id, request, result) => {
                return self.browse_message(id, request, result);
            }
            Message::Modifiers(modifiers) => self.modifiers = modifiers,
            Message::Manage(event) => return self.manage_message(event),
            Message::Libraries(event) => return self.library_message(event),
            Message::SettingsLoaded(result) => match result {
                Ok(prefs) => {
                    self.on_demand_models = prefs.ai.on_demand;
                    self.retain_indexes = prefs.ai.retain_indexes;
                    let value = prefs.ai;
                    let service = self.service.clone();
                    self.apply_settings(prefs);

                    return Task::batch([
                        Task::perform(async move { service.ai_policy(value).await }, |r| match r {
                            Ok(()) => Message::Noop,
                            Err(e) => Message::Notice(e),
                        }),
                        self.load_library_cards(),
                    ]);
                }
                Err(e) => self.error = Some(e),
            },
            Message::LibraryCardsLoaded(request, Ok(cards))
                if request == self.library_cards_request =>
            {
                for moriq_app::LibraryCard {
                    path,
                    cover,
                    summary,
                } in cards
                {
                    if self.preferences.recent.iter().any(|(_, p)| p == &path) {
                        if let Some(summary) = summary {
                            self.library_summaries.insert(path.clone(), summary);
                        }
                        if let Some(pixels) = cover {
                            self.library_covers.insert(
                                path,
                                image::Handle::from_rgba(pixels.width, pixels.height, pixels.rgba),
                            );
                        } else {
                            self.library_covers.remove(&path);
                        }
                    }
                }
            }
            Message::LibraryCardsLoaded(request, Err(error))
                if request == self.library_cards_request =>
            {
                self.error = Some(error)
            }
            Message::ShowLibraries if !self.manager.busy => {
                self.stop_viewer();
                self.library_picker = true;
                self.screen = Screen::Overview;
                self.viewer.preview_expanded = false;
                self.dialog = None;
                self.manager.panel = None;
                let service = self.service.clone();
                return Task::perform(
                    async move { service.settings().await },
                    Message::SettingsLoaded,
                );
            }
            Message::OpenRecent(path)
                if self.import.is_none() && !self.switching && !self.manager.busy =>
            {
                self.switching = true;
                let service = self.service.clone();
                return Task::perform(async move { service.open(path).await }, Message::Opened);
            }
            Message::FocusSearch => {
                if self.manager.panel == Some(manage::Panel::Delete) && self.manager.busy {
                    return Task::none();
                }
                self.manager.panel = None;
                self.dialog = None;
                return Task::batch([
                    self.set_viewer(false),
                    iced::widget::operation::focus("global-search"),
                ]);
            }
            Message::WindowDrag => return iced::window::oldest().and_then(iced::window::drag),
            Message::WindowMinimize => {
                return iced::window::oldest().and_then(|id| iced::window::minimize(id, true));
            }
            Message::WindowMaximize => {
                return iced::window::oldest().and_then(iced::window::toggle_maximize);
            }
            Message::WindowClose if self.libraries.busy => {
                return Task::none();
            }
            Message::WindowClose => {
                if self.interface.saving.is_some()
                    || (self.preferences.language, self.preferences.appearance)
                        != self.interface.saved
                {
                    self.interface.close_after_save = true;
                    return self.save_interface();
                }
                self.drop_import.clear();
                let playback = self.viewer.playback.take();
                self.lifecycle.cancel_copy();
                self.stop_viewer();
                for control in self.ai.downloads.values() {
                    control.stop.store(true, Ordering::Relaxed);
                }
                self.compare.clear();
                if let Some(control) = &self.storage.control {
                    control.cancel.store(true, Ordering::Relaxed);
                }
                if let Some(control) = &self.manager.maintenance {
                    control.cancel.store(true, Ordering::Relaxed);
                }
                if let Some(import) = &self.import {
                    import.cancel.store(true, Ordering::Relaxed);
                }
                if let Some(handle) = self.page_task.take() {
                    handle.abort();
                }
                if let Some(handle) = self.viewer.preview_task.take() {
                    handle.abort();
                }
                if let Some(handle) = self.masonry_task.take() {
                    handle.abort();
                }
                let service = self.service.clone();
                self.status =
                    tr!(language, "正在安全关闭资源库…" => "Closing the library safely…").into();
                let navigation = self
                    .library
                    .as_ref()
                    .map(|l| (l.info.id, self.navigation_snapshot()));
                return Task::perform(
                    async move {
                        if let Some(playback) = playback {
                            tokio::task::spawn_blocking(move || {
                                playback.stop().map_err(|e| e.to_string())
                            })
                            .await
                            .map_err(|e| e.to_string())??;
                        }
                        if let Some((owner, value)) = navigation
                            && let Err(error) = service.save_navigation(owner, value).await
                        {
                            tracing::warn!(%error,"navigation could not be saved on exit");
                        }
                        service.close().await
                    },
                    Message::ShutdownReady,
                );
            }
            Message::ShutdownReady(result) => match result {
                Ok(()) => return iced::exit(),
                Err(error) => self.error = Some(error),
            },
            Message::CopyLibraryPath => {
                if let Some(library) = &self.library {
                    return iced::clipboard::write(components::display_path(&library.root));
                }
            }
            Message::CopyModelsPath => match moriq_app::model_directory() {
                Ok(path) => return iced::clipboard::write(components::display_path(&path)),
                Err(error) => self.error = Some(error),
            },
            Message::RevealLibrary => {
                if let Some(library) = &self.library {
                    let service = self.service.clone();
                    let id = library.info.id;
                    return Task::perform(async move { service.reveal(id).await }, move |r| {
                        Message::PlatformDone(
                            tr!(language, "已请求在文件夹中显示资源库" => "Requested to reveal the library folder"),
                            r,
                        )
                    });
                }
            }
            Message::OpenSourceRepository => {
                return Task::perform(moriq_app::open_source_repository(), move |r| {
                    Message::PlatformDone(
                        tr!(language, "已请求在浏览器中打开项目开源地址" => "Requested to open the source repository in your browser"),
                        r,
                    )
                });
            }
            Message::OpenModelsDirectory => {
                return Task::perform(moriq_app::open_models_directory(), move |r| {
                    Message::PlatformDone(
                        tr!(language, "已请求打开模型文件夹" => "Requested to open the model folder"),
                        r,
                    )
                });
            }
            Message::PlatformDone(success, result) => match result {
                Ok(()) => self.status = success.into(),
                Err(error) => self.error = Some(error),
            },
            Message::ToggleTagEditor => self.editing_tags = !self.editing_tags,
            Message::ToggleModelPolicy
            | Message::ToggleRetainIndexes
            | Message::ToggleModelMirror => {
                if matches!(message, Message::ToggleModelPolicy) {
                    self.on_demand_models = !self.on_demand_models
                } else if matches!(message, Message::ToggleModelMirror) {
                    self.preferences.ai.use_mirror = !self.preferences.ai.use_mirror;
                } else {
                    self.retain_indexes = !self.retain_indexes
                }
                self.preferences.ai = AiPreferences {
                    on_demand: self.on_demand_models,
                    retain_indexes: self.retain_indexes,
                    ..self.preferences.ai
                };

                let value = self.preferences.ai;
                let service = self.service.clone();
                return Task::perform(
                    async move { service.ai_preferences(value).await },
                    move |r| Message::AiPolicySaved(value, r),
                );
            }
            Message::AiPolicySaved(value, result) if self.preferences.ai == value => match result {
                Ok(prefs) => self.apply_settings(prefs),
                Err(e) => self.error = Some(e),
            },
            Message::ShowCreate => {
                self.name = tr!(language, "我的素材库" => "My Library").into();
                self.dialog = Some(Dialog::CreateLibrary);
            }
            Message::HideCreate => self.dialog = None,
            Message::ShowProject => {
                self.manager.panel = None;
                self.dialog = Some(Dialog::CreateProject);
            }
            Message::ShowSettings => self.dialog = Some(Dialog::Settings),
            Message::ShowRenameLibrary => {
                if let Some(library) = &self.library {
                    self.name = library.info.name.clone();
                    self.manager.description_draft =
                        Some(self.manager.organizer.description.clone());
                    self.dialog = Some(Dialog::RenameLibrary);
                }
            }
            Message::ShowRenameAsset => {
                if let Some(asset) = &self.viewer.selected {
                    self.name = asset.name.clone();
                    self.dialog = Some(Dialog::RenameAsset);
                }
            }
            Message::SaveRenameLibrary => {
                if let Some(library) = &mut self.library {
                    let id = library.info.id;
                    let name = self.name.clone();
                    let description = self
                        .manager
                        .description_draft
                        .clone()
                        .unwrap_or_else(|| self.manager.organizer.description.clone());
                    let service = self.service.clone();
                    return Task::perform(
                        async move { service.update_profile(id, name, description).await },
                        move |result| Message::LibraryRenamed(id, result),
                    );
                }
            }
            Message::LibraryRenamed(id, result)
                if self
                    .library
                    .as_ref()
                    .is_some_and(|library| library.info.id == id) =>
            {
                match result {
                    Ok(info) => {
                        if let Some(library) = &mut self.library {
                            library.info = info;
                        }
                        if self.dialog == Some(Dialog::RenameLibrary) {
                            self.dialog = None;
                        }
                        return self.refresh_organizer();
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            Message::SaveRenameAsset => {
                if let (Some(library), Some(asset)) = (&self.library, &mut self.viewer.selected) {
                    let id = library.info.id;
                    let asset = asset.id;
                    let name = self.name.clone();
                    let service = self.service.clone();
                    return Task::perform(
                        async move { service.rename_asset(id, asset, name).await },
                        move |result| Message::AssetRenamed(id, asset, result),
                    );
                }
            }
            Message::Notice(message) => {
                self.status = message.clone();
                return self.persist_notice(tr!(language, "提示" => "Notice"), message, 0, "");
            }
            Message::AssetRenamed(id, asset, result)
                if self
                    .library
                    .as_ref()
                    .is_some_and(|library| library.info.id == id) =>
            {
                if result.is_ok()
                    && self.dialog == Some(Dialog::RenameAsset)
                    && self
                        .viewer
                        .selected
                        .as_ref()
                        .is_some_and(|selected| selected.id == asset)
                {
                    self.dialog = None;
                }
                return self.update(Message::VisualsSaved(id, result));
            }
            Message::DetailTab(tab) => self.detail_tab = tab,
            Message::ModelSearch(query) => self.model_search = query,
            Message::ModelFilter(filter) => self.model_filter = filter,
            Message::SetMasonry(value) => {
                self.manager.panel = None;
                self.masonry_mode = value;
                self.list = false;
                return self.reset_query();
            }
            Message::MasonryLoaded(generation, result) if generation == self.layout_generation => {
                self.masonry_task = None;
                match result {
                    Ok(layout) => {
                        self.masonry = Some(layout);
                        return self.refresh();
                    }
                    Err(error) => {
                        self.error = Some(error);
                        return self.restore_scroll();
                    }
                }
            }
            Message::Name(value) => self.name = value,
            Message::Create if self.import.is_none() && !self.switching && !self.manager.busy => {
                self.switching = true;
                return Task::perform(
                    async move {
                        rfd::AsyncFileDialog::new()
                            .set_title(tr!(language, "选择资源库保存位置" => "Choose a location for the library"))
                            .pick_folder()
                            .await
                            .map(|f| f.path().to_path_buf())
                    },
                    Message::PickedParent,
                );
            }
            Message::Open if self.import.is_none() && !self.switching && !self.manager.busy => {
                self.switching = true;
                return Task::perform(
                    async move {
                        rfd::AsyncFileDialog::new()
                            .set_title(tr!(language, "选择 .moriq 资源库目录" => "Select a .moriq library folder"))
                            .pick_folder()
                            .await
                            .map(|f| f.path().to_path_buf())
                    },
                    Message::PickedLibrary,
                );
            }
            Message::PickedParent(Some(parent)) => {
                let service = self.service.clone();
                let name = self.name.clone();
                return Task::perform(
                    async move { service.create(parent, name).await },
                    Message::Opened,
                );
            }
            Message::PickedLibrary(Some(path)) => {
                let service = self.service.clone();
                let models = self.screen == Screen::Models;
                return Task::perform(async move { service.open(path).await }, move |r| {
                    if models {
                        Message::Ai(AiMessage::LibraryOpened(r))
                    } else {
                        Message::Opened(r)
                    }
                });
            }
            Message::PickedParent(None) | Message::PickedLibrary(None) => self.switching = false,
            Message::Opened(result) => {
                self.switching = false;
                match result {
                    Ok(library) => {
                        self.library_picker = false;
                        self.history = Default::default();
                        self.journal.reset();
                        self.storage.reset();
                        self.deletion.reset();
                        self.surfaces = Default::default();
                        self.dialog = None;
                        self.total = library.info.count;
                        let recent_service = self.service.clone();
                        let recent_name = library.info.name.clone();
                        let recent_path = library.root.clone();
                        let remember = if self.launch.capture.is_some() {
                            Task::none()
                        } else {
                            Task::perform(
                                async move { recent_service.remember(recent_name, recent_path).await },
                                Message::SettingsLoaded,
                            )
                        };
                        self.library = Some(library);
                        self.screen = match self.launch.screen {
                            1 => Screen::Assets,
                            2 => Screen::Models,
                            _ => Screen::Overview,
                        };
                        self.manager = Default::default();
                        self.lifecycle.cancel_copy();
                        self.stop_people_portraits();
                        self.ai.portraits.clear();
                        self.ai.portrait_request = self.ai.portrait_request.wrapping_add(1);
                        self.query = Query::default();
                        self.error = None;
                        self.status = tr!(language, "资源库已就绪" => "Library ready").into();
                        let imports = std::mem::take(&mut self.launch.import_paths);
                        return Task::batch([
                            self.reset_query(),
                            self.refresh_library(),
                            self.load_volume(),
                            remember,
                            if imports.is_empty() {
                                Task::none()
                            } else {
                                Task::done(Message::PickedImages(Some(imports)))
                            },
                        ]);
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            Message::Navigate(screen) => {
                if screen != 1 {
                    self.stop_viewer();
                }
                self.library_picker = false;
                self.manager.panel = None;
                self.dialog = None;
                self.screen = match screen {
                    1 => Screen::Assets,
                    2 => Screen::Models,
                    _ => Screen::Overview,
                };
                if self.screen == Screen::Assets {
                    self.pending_scroll = Some(self.scroll_y);
                    return if self.masonry.is_none() {
                        Task::batch([self.refresh(), self.rebuild_masonry()])
                    } else {
                        self.refresh()
                    };
                }
                if self.screen == Screen::Models {
                    self.viewer.preview_expanded = false;
                    let models = if self.ai.loaded {
                        Task::none()
                    } else {
                        self.ai_message(AiMessage::Refresh)
                    };
                    return Task::batch([self.load_volume(), self.restore_page(), models]);
                }
                self.viewer.preview_expanded = false;
                return self.restore_page();
            }
            Message::Search(value) => {
                return self.search_changed(value);
            }
            Message::SearchReady(request) => return self.search_ready(request),
            Message::CancelSearch => return self.cancel_search(),
            Message::GeometryReady(request) => return self.geometry_ready(request),
            Message::AllAssets => {
                self.query = Query::default();
                self.screen = Screen::Assets;
                return self.reset_query();
            }
            Message::Favorites => {
                self.query = Query {
                    project: self.query.project,
                    favorites_only: true,
                    ..Query::default()
                };
                self.screen = Screen::Assets;
                return self.reset_query();
            }
            Message::Project(id) => {
                let visit = if let Some(library) = &self.library {
                    let library = library.info.id;
                    let service = self.service.clone();
                    let generation = self.generation;
                    Task::perform(
                        async move { service.visit_project(library, id).await },
                        move |r| Message::InfoLoaded(generation, r),
                    )
                } else {
                    Task::none()
                };
                self.manager.panel = None;
                self.query = Query {
                    project: Some(id),
                    ..Query::default()
                };
                self.screen = Screen::Assets;
                return Task::batch([self.reset_query(), visit]);
            }
            Message::Import
                if self.library.is_some()
                    && self.import.is_none()
                    && !self.switching
                    && !self.manager.busy =>
            {
                self.manager.referenced_import = false;
                self.manager.panel = None;

                self.manager.busy = true;
                return Task::perform(
                    async move {
                        rfd::AsyncFileDialog::new()
                            .set_title(tr!(language, "导入图片 · 原文件保持不变" => "Import images · Original files are preserved"))
                            .add_filter(tr!(language, "图片" => "Images"), moriq_app::IMPORT_EXTENSIONS)
                            .pick_files()
                            .await
                            .map(|files| {
                                files.into_iter().map(|f| f.path().to_path_buf()).collect()
                            })
                    },
                    Message::PickedImages,
                );
            }
            Message::PickedImages(None) => {
                self.manager.busy = false;
                self.manager.referenced_import = false;
            }
            Message::PickedImages(Some(paths)) if self.import.is_none() && !self.switching => {
                return self.start_import(
                    paths,
                    self.query.project,
                    self.manager.referenced_import,
                );
            }
            Message::ImportPreference(field, value) => match field {
                0 => self.preferences.import.workers = value.parse().unwrap_or(0),
                1 => {
                    self.preferences.import.memory_mib = value
                        .parse()
                        .unwrap_or(ImportPreferences::default().memory_mib)
                }
                _ => {}
            },
            Message::SaveImportPreferences => {
                let prefs = self.preferences.import;
                let service = self.service.clone();
                return Task::perform(
                    async move { service.import_preferences(prefs).await },
                    move |result| Message::ImportPreferencesSaved(prefs, result),
                );
            }
            Message::ImportPreferencesSaved(value, result) => match result {
                Ok(prefs) if self.preferences.import == value => {
                    self.apply_settings(prefs);
                    self.status = tr!(language, "导入设置已保存，下次导入生效" => "Import settings saved. Changes apply to the next import.").into();
                }
                Err(error) => self.error = Some(error),
                _ => {}
            },
            Message::CancelImport => {
                let pending = self.drop_import.pending_count();
                self.drop_import.clear();
                if self.import.is_none() && pending > 0 {
                    self.status =
                        tr!(language,"已取消待导入文件"=>"Queued import cancelled").into();
                }
                if let Some(import) = &self.import {
                    import.cancel.store(true, Ordering::Relaxed);
                    self.status = tr!(language, "正在完成当前文件，随后停止导入…" => "Finishing the current file, then stopping import…").into();
                }
            }
            Message::Imported(report) => {
                if report.cancelled {
                    self.drop_import.clear();
                }
                let parallelism = self
                    .import
                    .as_ref()
                    .map(|job| {
                        trf!(language, " · 解码峰值 {} 张 / 上限 {} 张" => " · Peak decoding {} / limit {}",
                            job.peak.load(Ordering::Relaxed),
                            job.workers
                        )
                    })
                    .unwrap_or_default();
                self.import = None;
                self.manager.referenced_import = false;
                self.status = trf!(language, "{}导入 {} 张 · 跳过 {} 张重复图片 · 失败 {} 张" => "{}Imported {} · Skipped {} duplicates · Failed {}",
                    if report.cancelled {
                        tr!(language, "已停止 · " => "Stopped · ")
                    } else {
                        ""
                    },
                    report.imported,
                    report.duplicates,
                    report.failed
                );
                self.status.push_str(&parallelism);
                if !report.failures.is_empty() {
                    self.error = Some(report.failures.join("\n"));
                }
                return Task::batch([
                    self.persist_notice(
                        tr!(language, "导入结果" => "Import results"),
                        self.status.clone() + "\n" + &report.failures.join("\n"),
                        if report.failed > 0 { 2 } else { 0 },
                        if report.failed > 0 { "recover" } else { "" },
                    ),
                    self.reset_query(),
                    self.refresh_info(),
                    self.refresh_catalog(),
                    self.start_next_drop(),
                ]);
            }
            Message::Jobs(event) => return self.job_message(event),
            Message::Ai(event) => return self.ai_message(event),
            Message::Regions(event) => return self.region_message(event),
            Message::DpiChanged(scale) if scale.is_finite() && (0.5..=8.0).contains(&scale) => {
                self.dpi_scale = scale;
                if self.viewer.regions.target_scale.is_some() {
                    self.viewer.regions.target_scale = Some(1.0 / scale);
                    self.viewer.regions.zoom_request += 1;
                }
            }
            Message::UploadTick => self.drain_uploads(),
            Message::PersistNavigation => return self.persist_navigation(),
            Message::NavigationLoaded(id, generation, result) => {
                return self.navigation_loaded(id, generation, result);
            }
            Message::NavigationSaved(id, Err(e))
                if self.library.as_ref().is_some_and(|l| l.info.id == id) =>
            {
                if self.error.is_none() {
                    self.error = Some(e)
                }
            }
            Message::Folder(event) => return self.folder_message(event),
            Message::Metadata(event) => return self.metadata_message(event),
            Message::Batch(event) => return self.batch_message(event),
            Message::Lifecycle(event) => return self.lifecycle_message(event),
            Message::Tick => return self.tick(),
            #[cfg(feature = "capture")]
            Message::VideoProof(screenshot) => {
                if let Some(path) = self.launch.capture.as_ref() {
                    let path = path.with_extension("early.png");
                    let service = self.service.clone();
                    let request = self.viewer.original_request;
                    let state = format!(
                        "presented_frames={} progress={:?}\n",
                        self.viewer.video_presented,
                        self.viewer.playback.as_ref().map(|p| p.progress())
                    );
                    return Task::perform(
                        async move {
                            service
                                .save_capture_evidence(path.with_extension("txt"), state)
                                .await?;
                            service
                                .save_capture(
                                    path,
                                    screenshot.size.width,
                                    screenshot.size.height,
                                    screenshot.rgba.to_vec(),
                                )
                                .await
                        },
                        move |r| Message::VideoControlled(request, r),
                    );
                }
            }
            #[cfg(feature = "capture")]
            Message::LiquidProof(step) => return self.liquid_proof_step(step),
            #[cfg(feature = "capture")]
            Message::MotionProof(step) => return self.motion_proof_step(step),
            #[cfg(feature = "capture")]
            Message::MotionFrame(step, frame) => return self.motion_proof_frame(step, frame),
            #[cfg(feature = "capture")]
            Message::LiquidFrame(step, screenshot) => {
                return self.liquid_proof_frame(step, screenshot);
            }
            #[cfg(feature = "capture")]
            Message::InteractionSnapshot(screenshot) => {
                return self.capture_search_snapshot(screenshot);
            }
            #[cfg(feature = "capture")]
            Message::Captured(screenshot) => {
                if let Some(path) = self.launch.capture.take() {
                    let service = self.service.clone();
                    let playback = self.viewer.playback.clone();
                    let playback_state = format!(
                        "expanded={} request={} error={:?} presented_frames={} first_gpu_ready_ms={:?} gallery_scroll_y={} screen={:?} query={:?} total={} pending_input={}\n",
                        self.viewer.preview_expanded,
                        self.viewer.original_request,
                        self.error,
                        self.viewer.video_presented,
                        self.viewer.video_first_ready_ms,
                        self.scroll_y,
                        self.screen,
                        self.query.text,
                        self.total,
                        self.responsiveness.pending()
                    );
                    return Task::perform(
                        async move {
                            let evidence = path.with_extension("playback.txt");
                            let details = if let Some(playback) = playback {
                                tokio::task::spawn_blocking(move || {
                                    playback.diagnostics().map_err(|e| e.to_string())
                                })
                                .await
                                .map_err(|e| e.to_string())??
                            } else {
                                "no playback session\n".into()
                            };
                            service
                                .save_capture_evidence(
                                    evidence,
                                    format!("{playback_state}{details}"),
                                )
                                .await?;
                            service
                                .save_capture(
                                    path,
                                    screenshot.size.width,
                                    screenshot.size.height,
                                    screenshot.rgba.to_vec(),
                                )
                                .await
                        },
                        Message::CaptureSaved,
                    );
                }
            }
            #[cfg(feature = "capture")]
            Message::CaptureSaved(result) => {
                if let Err(error) = result {
                    self.error = Some(error);
                } else if self.launch.exit_after_capture {
                    return self.update(Message::WindowClose);
                }
            }
            Message::PageLoaded(generation, result) if generation == self.generation => {
                self.page_task = None;
                self.loading = false;
                match result {
                    Ok(page) => {
                        let search_committed = self.responsiveness.commit_search();
                        if search_committed {
                            if self.viewer.selected.as_ref().is_some_and(|selected| {
                                !page.tiles.iter().any(|tile| {
                                    tile.asset.id == selected.id
                                        && tile.asset.version_id == selected.version_id
                                })
                            }) {
                                self.clear_browsing();
                            }
                            self.manager.selection.clear();
                            self.masonry = None;
                            self.scroll_y = 0.0;
                            self.pending_scroll = Some(0.0);
                            self.surfaces.restoring_gallery = None;
                        }
                        self.total = page.total;
                        self.offset = page.offset;
                        let previous = std::mem::take(&mut self.tiles)
                            .into_iter()
                            .filter_map(|tile| {
                                tile.handle.map(|handle| (tile.asset.version_id, handle))
                            })
                            .collect::<std::collections::HashMap<_, _>>();
                        self.uploads.clear();
                        self.upload_generation = generation;
                        self.upload_owner = self.library.as_ref().map(|l| l.info.id);
                        let mut tiles = Vec::with_capacity(page.tiles.len());
                        for tile in page.tiles {
                            let version = tile.asset.version_id;
                            let handle = previous.get(&version).cloned();
                            if handle.is_none()
                                && let Some(pixels) = tile.thumbnail
                            {
                                self.uploads.push_back((version, pixels));
                            }
                            tiles.push(Tile {
                                asset: tile.asset,
                                handle,
                            });
                        }
                        self.tiles = tiles;
                        let pending: Vec<_> = self
                            .tiles
                            .iter()
                            .filter_map(|tile| {
                                self.pending_favorite(tile.asset.id)
                                    .map(|value| (tile.asset.id, value))
                            })
                            .collect();
                        for (id, value) in pending {
                            self.paint_favorite(id, value);
                        }
                        self.drain_uploads();
                        if page.warnings > 0 {
                            self.status = trf!(language, "{} 张缩略图不可用，原片仍被保留" => "{} thumbnails unavailable. Original files are preserved.", page.warnings);
                        }
                        if let Some(selected) = self.viewer.selected.as_mut()
                            && let Some(tile) =
                                self.tiles.iter().find(|t| t.asset.id == selected.id)
                        {
                            *selected = tile.asset.clone();
                        }
                        if self.total == 0 || self.offset >= self.total as usize {
                            self.scroll_y = 0.0;
                            if self.pending_scroll.is_some() {
                                self.pending_scroll = Some(0.0);
                            }
                        }
                        let neighbors = if search_committed {
                            self.neighbors.clear();
                            self.neighbor_position = None;
                            self.viewer
                                .selected
                                .as_ref()
                                .map(|selected| selected.id)
                                .map(|id| self.load_neighborhood(id, 0))
                                .unwrap_or_else(Task::none)
                        } else {
                            Task::none()
                        };
                        let needed = self.visible_range();
                        if needed.start < self.offset || needed.end > self.offset + self.tiles.len()
                        {
                            return Task::batch([
                                self.refresh(),
                                neighbors,
                                if search_committed {
                                    self.rebuild_masonry()
                                } else {
                                    Task::none()
                                },
                            ]);
                        }
                        if self.launch.select_first
                            && let Some(tile) = self.tiles.first()
                        {
                            let id = tile.asset.id;
                            self.launch.select_first = false;
                            return self.update(Message::Select(id));
                        }
                        return Task::batch([
                            self.restore_scroll(),
                            neighbors,
                            if search_committed {
                                self.rebuild_masonry()
                            } else {
                                Task::none()
                            },
                        ]);
                    }
                    Err(error) if self.responsiveness.replacing() => {
                        self.responsiveness.search_error = Some(error);
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            Message::Scrolled(y) => {
                self.gallery_feedback.clear();
                if self.viewer.preview_expanded
                    || ((self.pending_scroll.or(self.surfaces.restoring_gallery))
                        .is_some_and(|target| target > 0.0)
                        && y == 0.0)
                {
                    return Task::none();
                }
                self.surfaces.restoring_gallery = None;
                self.scroll_y = y;
                let range = self.visible_range();
                if !self.loading
                    && (range.start < self.offset || range.end > self.offset + self.tiles.len())
                {
                    return self.refresh();
                }
            }
            Message::Resized(size) => {
                if self.viewport == size {
                    return Task::none();
                }
                self.viewport = size;
                return self.geometry_changed();
            }
            Message::Zoom(value) => {
                if (self.card_width - value).abs() < 0.1 {
                    return Task::none();
                }
                self.card_width = value;
                return self.geometry_changed();
            }
            Message::List(value) => {
                self.manager.panel = None;
                self.screen = Screen::Assets;
                self.list = value;
                return self.reset_query();
            }
            Message::CardPressed(id) => {
                if self.surface_open() || self.switching || self.viewer.preview_expanded {
                    return Task::none();
                }
                if !self.tiles.iter().any(|tile| tile.asset.id == id) {
                    return Task::none();
                }
                if self.modifiers.control()
                    || (self.manager.selection.enabled && !self.manager.selection.additive)
                {
                    if !self.manager.selection.enabled {
                        self.manager.selection.clear();
                        if let Some(asset) = &self.viewer.selected {
                            self.manager.selection.assets.insert(asset.id);
                            self.manager.selection.anchor = Some(asset.id);
                        }
                        self.manager.selection.enabled = true;
                        self.manager.selection.additive = true;
                    }
                    return self.manage_message(ManageMessage::Toggle(id));
                }
                self.manager.selection.enabled = false;
                self.manager.selection.clear();
                return self.update_inner(Message::Select(id));
            }
            Message::Select(id) => {
                self.screen = Screen::Assets;
                let tile = self
                    .tiles
                    .iter()
                    .chain(self.neighbors.iter())
                    .find(|t| t.asset.id == id)
                    .cloned();
                self.neighbor_position = None;
                self.neighbors.clear();
                let focus = tile
                    .map(|tile| self.focus_tile(tile))
                    .unwrap_or_else(Task::none);
                return Task::batch([focus, self.load_neighborhood(id, 0)]);
            }
            Message::FullPreviewLoaded(library, id, version, result)
                if self.library.as_ref().is_some_and(|l| l.info.id == library)
                    && self
                        .viewer
                        .selected
                        .as_ref()
                        .is_some_and(|a| a.id == id && a.version_id == version) =>
            {
                self.viewer.preview_task = None;
                match result {
                    Ok((preview, background)) => {
                        if self.viewer.original_size.is_none()
                            || self.viewer.regions.session.is_some()
                        {
                            self.viewer.preview = Some(preview);
                        }
                        self.viewer.preview_background = Some(background);
                    }
                    Err(e) => self.error = Some(e),
                }
            }
            Message::VideoFrame if !self.viewer.video_upload => {
                if let Some(playback) = &self.viewer.playback {
                    match playback.take_frame() {
                        Ok(Some(frame)) => {
                            let request = self.viewer.original_request;
                            self.viewer.video_upload = true;
                            // Iced uploads large images asynchronously. Keep the last
                            // allocation visible until this frame is ready on the GPU.
                            return iced_runtime::image::allocate(image::Handle::from_rgba(
                                frame.width,
                                frame.height,
                                frame.rgba,
                            ))
                            .map(move |r| {
                                Message::VideoUploaded(request, r.map_err(|e| e.to_string()))
                            });
                        }
                        Ok(None) => {}
                        Err(error) => {
                            self.error = Some(error.to_string());
                            self.viewer.stop_media();
                        }
                    }
                }
            }
            Message::VideoUploaded(request, result)
                if request == self.viewer.original_request && self.viewer.playback.is_some() =>
            {
                self.viewer.video_upload = false;
                match result {
                    Ok(frame) => {
                        if self.viewer.video_first_ready_ms.is_none() {
                            self.viewer.video_first_ready_ms =
                                self.viewer.playback.as_ref().map(|p| p.elapsed_ms());
                        }
                        self.viewer.video_frame = Some(frame);
                        self.viewer.video_presented += 1;
                    }
                    Err(error) => {
                        self.error = Some(error);
                        self.viewer.stop_media();
                    }
                }
            }
            Message::VideoPause | Message::VideoMute | Message::VideoSeek(_) => {
                if let Some(playback) = self.viewer.playback.clone() {
                    let seek = match message {
                        Message::VideoPause => {
                            let (position, duration) = playback.progress();
                            if duration > 0 && position + 80 >= duration {
                                self.viewer.video_paused = false;
                                Some(0)
                            } else {
                                self.viewer.video_paused = !self.viewer.video_paused;
                                None
                            }
                        }
                        Message::VideoMute => {
                            self.viewer.video_muted = !self.viewer.video_muted;
                            None
                        }
                        Message::VideoSeek(value) => {
                            Some((value.clamp(0., 1.) * playback.progress().1 as f32) as u64)
                        }
                        _ => None,
                    };
                    self.viewer.video_control += 1;
                    let revision = self.viewer.video_control;
                    let (paused, muted, request) = (
                        self.viewer.video_paused,
                        self.viewer.video_muted,
                        self.viewer.original_request,
                    );
                    return Task::perform(
                        async move {
                            tokio::task::spawn_blocking(move || {
                                playback
                                    .control(revision, paused, muted, seek)
                                    .map_err(|e| e.to_string())
                            })
                            .await
                            .map_err(|e| e.to_string())?
                        },
                        move |r| Message::VideoControlled(request, r),
                    );
                }
            }
            Message::VideoControlled(request, Err(error))
                if request == self.viewer.original_request =>
            {
                self.error = Some(error);
            }
            Message::PlaybackStarted(library, id, version, request, result)
                if self.viewer.preview_expanded
                    && !self.surface_open()
                    && self.viewer.original_request == request
                    && self.library.as_ref().is_some_and(|l| l.info.id == library)
                    && self
                        .viewer
                        .selected
                        .as_ref()
                        .is_some_and(|a| a.id == id && a.version_id == version) =>
            {
                match result {
                    Ok(session) => {
                        self.viewer.start_playback(session);
                    }
                    Err(e) => self.error = Some(e),
                }
            }
            Message::ClosePreview => {
                if self.drop_import.hovering() {
                    self.drop_import.clear_hover();
                    return Task::none();
                }
                if self.surface_open() {
                    return self.close_surface();
                }
                if self.responsiveness.replacing() {
                    return self.cancel_search();
                }
                if self.viewer.preview_expanded {
                    return self.set_viewer(false);
                }
                // Escape dismisses the expanded viewer or overlay; a fitted selection stays put.
                if let Some(task) = self.browse_task.take() {
                    task.abort();
                }
                self.browse_request = self.browse_request.wrapping_add(1);
                return Task::none();
            }
            Message::ExpandPreview => return self.set_viewer(!self.viewer.preview_expanded),
            Message::Favorite(asset, value) => {
                return self.favorite(asset, value);
            }
            Message::FavoriteSaved(owner, asset, request, result) => {
                return self.favorite_saved(owner, asset, request, result);
            }
            Message::Rating(rating) => {
                if let (Some(library), Some(asset)) = (&self.library, &self.viewer.selected) {
                    let service = self.service.clone();
                    let id = library.info.id;
                    let asset = asset.id;
                    return self.mutation(async move { service.rating(id, asset, rating).await });
                }
            }
            Message::Tags(value) => self.tag_input = value,
            Message::SaveTags => {
                if let (Some(library), Some(asset)) = (&self.library, &self.viewer.selected) {
                    let service = self.service.clone();
                    let id = library.info.id;
                    let asset = asset.id;
                    let tags = self
                        .tag_input
                        .split([',', '，'])
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                        .collect();
                    return self.mutation(async move { service.tags(id, asset, tags).await });
                }
            }
            Message::VisualsSaved(id, result)
                if self.library.as_ref().is_some_and(|l| l.info.id == id) =>
            {
                let changed = result.is_ok();
                let task = self.update(Message::Saved(id, result));
                return if changed {
                    Task::batch([task, self.refresh_visuals()])
                } else {
                    task
                };
            }
            Message::Saved(id, result)
                if self.library.as_ref().is_some_and(|l| l.info.id == id) =>
            {
                match result {
                    Ok(()) => {
                        self.status =
                            tr!(language, "更改已保存到资源库" => "Changes saved to the library")
                                .into();
                        let browse = self
                            .viewer
                            .selected
                            .as_ref()
                            .map(|a| a.id)
                            .map(|id| self.load_neighborhood(id, 0))
                            .unwrap_or_else(Task::none);
                        return Task::batch([
                            browse,
                            self.refresh(),
                            self.rebuild_masonry(),
                            self.refresh_info(),
                            self.refresh_organizer(),
                            self.refresh_details(),
                        ]);
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            Message::InfoLoaded(_, Ok(info)) => {
                // Identity guards background results even when a view query has since changed.
                if let Some(library) = self.library.as_mut().filter(|l| l.info.id == info.id) {
                    library.info = info;
                }
            }
            Message::InfoLoaded(generation, Err(error)) if generation == self.generation => {
                self.error = Some(error)
            }
            Message::ProjectName(value) => self.project_name = value,
            Message::CreateProject if !self.project_name.trim().is_empty() => {
                if let Some(library) = &self.library {
                    let service = self.service.clone();
                    let id = library.info.id;
                    let name = self.project_name.clone();
                    return Task::perform(
                        async move { service.project(id, name).await },
                        move |result| Message::ProjectCreated(id, result),
                    );
                }
            }
            Message::ProjectCreated(id, result)
                if self.library.as_ref().is_some_and(|l| l.info.id == id) =>
            {
                match result {
                    Ok(project) => {
                        if self.dialog == Some(Dialog::CreateProject) {
                            self.dialog = None;
                        }
                        self.project_name.clear();
                        self.screen = Screen::Assets;
                        self.query = Query {
                            project: Some(project),
                            ..Query::default()
                        };
                        return Task::batch([
                            self.reset_query(),
                            self.refresh_info(),
                            self.refresh_catalog(),
                        ]);
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            Message::Dismiss => self.error = None,
            Message::CardHovered(id, inside) => {
                if inside && !self.tiles.iter().any(|tile| tile.asset.id == id) {
                    return Task::none();
                }
                self.gallery_feedback.set(
                    id,
                    inside,
                    std::time::Instant::now(),
                    self.preferences.reduced_motion,
                );
            }
            Message::ReducedMotion(enabled) => return self.set_reduced_motion(enabled),
            Message::LanguageChanged(language) => return self.set_language(language),
            Message::AppearanceChanged(appearance) => return self.set_appearance(appearance),
            Message::InterfaceSaved(language, appearance, result) => {
                return self.finish_interface_save(language, appearance, result);
            }
            Message::DiagnosticLogging(enabled) => return self.set_diagnostic_logging(enabled),
            // Disabled actions and receipts from an older request are intentionally ignored.
            Message::ShowLibraries
            | Message::LibraryCardsLoaded(..)
            | Message::OpenRecent(..)
            | Message::AiPolicySaved(..)
            | Message::LibraryRenamed(..)
            | Message::AssetRenamed(..)
            | Message::MasonryLoaded(..)
            | Message::Create
            | Message::Open
            | Message::Import
            | Message::Drop(_)
            | Message::PickedImages(..)
            | Message::DpiChanged(..)
            | Message::NavigationSaved(..)
            | Message::PageLoaded(..)
            | Message::FullPreviewLoaded(..)
            | Message::VideoFrame
            | Message::VideoUploaded(..)
            | Message::VideoControlled(..)
            | Message::PlaybackStarted(..)
            | Message::Saved(..)
            | Message::VisualsSaved(..)
            | Message::InfoLoaded(_, Err(_))
            | Message::CreateProject
            | Message::ProjectCreated(..)
            | Message::Noop
            | Message::TransitionFrame(_) => {}
        }
        Task::none()
    }
    /// 构建应用根视图及当前浮层。
    fn view(&self) -> Element<'_, Message> {
        self.shell()
    }
}
