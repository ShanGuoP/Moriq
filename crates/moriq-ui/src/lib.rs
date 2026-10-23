//! Native presentation. All disk, catalog and decoding work belongs to Service.
mod ai;
mod batch;
mod folders;
mod lifecycle;
mod metadata;
mod region_viewer;
mod regions;
mod uploads;
pub(crate) use ai::AiMessage;
pub(crate) use batch::BatchMessage;
pub(crate) use folders::FolderMessage;
pub(crate) use lifecycle::LifecycleMessage;
pub(crate) use metadata::MetadataMessage;
pub(crate) use regions::RegionMessage;
mod browsing;
mod compare;
mod deletion;
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
mod storage;
pub(crate) use rules::RuleMessage;
pub(crate) use storage::StorageMessage;
mod manage;
mod models;
mod navigation;
pub(crate) use journal::JournalMessage;
pub(crate) use manage::ManageMessage;
mod motion;
mod overview;
mod preview;
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
    Shortcut(u8),
    Rules(RuleMessage),
    Storage(StorageMessage),
    Journal(JournalMessage),
    Manage(ManageMessage),
    Modifiers(iced::keyboard::Modifiers),
    SettingsLoaded(Result<UserSettings, String>),
    LibraryCardsLoaded(Result<Vec<(PathBuf, Option<moriq_app::Pixels>)>, String>),
    OpenRecent(PathBuf),
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
    Favorites,
    AllAssets,
    Project(ProjectId),
    Import,
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
    Rating(u8),
    Tags(String),
    SaveTags,
    Saved(LibraryId, Result<(), String>),
    InfoLoaded(u64, Result<LibraryInfo, String>),
    ProjectName(String),
    CreateProject,
    ProjectCreated(LibraryId, Result<ProjectId, String>),
    Dismiss,
    DiagnosticLogging(bool),
    ReducedMotion(bool),
    CardHovered(AssetId, bool),
    #[cfg(feature = "capture")]
    Captured(iced::window::Screenshot),
    #[cfg(feature = "capture")]
    VideoProof(iced::window::Screenshot),
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
    dpi_scale: f32,
    gallery_feedback: motion::HoverFeedback,
    regions: regions::Regions,
    uploads: std::collections::VecDeque<(AssetVersionId, moriq_app::Pixels)>,
    upload_generation: u64,
    upload_owner: Option<LibraryId>,
    lifecycle: lifecycle::LifecycleState,
    batch: batch::BatchState,
    metadata: metadata::MetadataState,
    folders: folders::FolderState,
    ai: ai::AiState,
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
    library_covers: HashMap<PathBuf, image::Handle>,
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
    selected: Option<Asset>,
    preview: Option<image::Handle>,
    preview_background: Option<image::Handle>,
    preview_expanded: bool,
    original_request: u64,
    original_size: Option<(u32, u32)>,
    playback: Option<std::sync::Arc<moriq_app::Playback>>,
    video_paused: bool,
    video_muted: bool,
    video_presented: u64,
    video_first_ready_ms: Option<u128>,
    video_frame: Option<iced_runtime::image::Allocation>,
    video_upload: bool,
    video_control: u64,
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
    navigation_request: u64,
    page_task: Option<iced::task::Handle>,
    preview_task: Option<iced::task::Handle>,
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

pub fn diagnostic_logging_enabled() -> bool {
    moriq_app::diagnostic_logging_enabled()
}
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
    .theme(|_: &App| theme::application_theme())
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
    .style(|_: &App, _: &Theme| iced::theme::Style {
        background_color: iced::Color::WHITE,
        text_color: theme::TEXT,
    })
    .subscription(App::subscription)
    .run()
}
impl App {
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
                storage: Default::default(),
                journal: Default::default(),
                history: Default::default(),
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
                library_covers: HashMap::new(),
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
                selected: None,
                modifiers: iced::keyboard::Modifiers::empty(),
                preview: None,
                preview_background: None,
                preview_expanded: false,
                original_request: 0,
                original_size: None,
                playback: None,
                video_paused: false,
                video_muted: false,
                video_presented: 0,
                video_first_ready_ms: None,
                video_frame: None,
                video_upload: false,
                video_control: 0,
                dpi_scale: 1.0,
                gallery_feedback: Default::default(),
                regions: Default::default(),
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
                navigation_request: 0,
                page_task: None,
                preview_task: None,
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
    fn refresh(&mut self) -> Task<Message> {
        if let Some(handle) = self.page_task.take() {
            handle.abort();
        }
        let Some(library) = self.library.as_ref() else {
            return Task::none();
        };
        self.generation += 1;
        let generation = self.generation;
        let range = self.visible_range();

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
    fn reset_query(&mut self) -> Task<Message> {
        self.gallery_feedback.clear();
        self.clear_browsing();
        self.pending_scroll = None;
        self.surfaces.restoring_gallery = None;
        self.layout_generation += 1;
        if let Some(handle) = self.masonry_task.take() {
            handle.abort();
        }
        self.scroll_y = 0.0;
        self.offset = 0;
        self.tiles.clear();
        self.selected = None;
        self.manager.selection.clear();
        self.manager.selection_anchor = None;
        self.preview = None;
        self.masonry = None;
        let page = self.refresh();
        let layout = if self.screen == Screen::Assets {
            self.rebuild_masonry()
        } else {
            Task::none()
        };
        Task::batch([
            page,
            layout,
            iced::widget::operation::scroll_to(
                "gallery",
                scrollable::AbsoluteOffset { x: 0.0, y: 0.0 },
            ),
        ])
    }
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
            if self.playback.is_some() {
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
                    Some(Message::Shortcut(4))
                }
                iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key: iced::keyboard::Key::Character(value),
                    modifiers,
                    ..
                }) if matches!(status, iced::event::Status::Ignored)
                    && modifiers.command()
                    && value.eq_ignore_ascii_case("e") =>
                {
                    Some(Message::Shortcut(5))
                }
                iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key: iced::keyboard::Key::Character(value),
                    modifiers,
                    ..
                }) if matches!(status, iced::event::Status::Ignored) && modifiers.is_empty() => {
                    match value.as_str() {
                        "0" | "1" | "2" | "3" | "4" | "5" => {
                            Some(Message::Shortcut(10 + value.as_bytes()[0] - b'0'))
                        }
                        "f" | "F" => Some(Message::Shortcut(6)),
                        "b" | "B" => Some(Message::Shortcut(7)),
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
                    Some(Message::Shortcut(0))
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
                        iced::keyboard::key::Named::Delete => 1,
                        iced::keyboard::key::Named::Enter | iced::keyboard::key::Named::Space => 2,
                        _ => 3,
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
                iced::Event::Window(iced::window::Event::FileDropped(path)) => {
                    Some(Message::PickedImages(Some(vec![path])))
                }
                iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                    ..
                }) => Some(Message::ClosePreview),
                _ => None,
            }),
        ])
    }
    fn update_inner(&mut self, message: Message) -> Task<Message> {
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
                self.manager.multiselect = false;
                let task = self.update(Message::Select(id));
                if self.selected.as_ref().is_some_and(|a| a.id == id) {
                    if menu {
                        self.preview_expanded = false;
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
                if which == 3 {
                    return self.manage_message(ManageMessage::Help);
                }
                if self.screen != Screen::Assets || self.library.is_none() {
                    return Task::none();
                }
                match which {
                    0 if !self.preview_expanded => {
                        self.manager.multiselect = true;
                        self.manager.selection = self.tiles.iter().map(|t| t.asset.id).collect();
                    }
                    1 if !self.query.trash && !self.manager.busy && !self.preview_expanded => {
                        return self.manage_message(ManageMessage::Edit(Edit::Trash(true)));
                    }
                    2 if self.selected.is_some() => return self.set_viewer(!self.preview_expanded),
                    4 => {
                        let task = self.batch_message(batch::BatchMessage::Open);
                        let all = self.batch_message(batch::BatchMessage::All(true));
                        return Task::batch([task, all]);
                    }
                    5 => return self.manage_message(ManageMessage::ExportMenu),
                    6 if !self.query.trash => {
                        if let Some(asset) = &self.selected {
                            return self.manage_message(ManageMessage::Edit(Edit::Favorite(
                                !asset.favorite,
                            )));
                        }
                    }
                    7 => return self.batch_message(batch::BatchMessage::Open),
                    10..=15 if !self.query.trash => {
                        return self.manage_message(ManageMessage::Edit(Edit::Rating(which - 10)));
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
                if self.preview_expanded {
                    return self.set_viewer(false);
                }
                return self.travel(false);
            }
            Message::Forward => {
                if self.surface_open() || self.preview_expanded {
                    return Task::none();
                }
                return self.travel(true);
            }
            Message::Browse(direction) => {
                if self.dialog.is_none()
                    && self.manager.panel.is_none()
                    && let Some(asset) = &self.selected
                {
                    return self.load_neighborhood(asset.id, direction);
                }
            }
            Message::NeighborhoodLoaded(id, request, result) => {
                return self.browse_message(id, request, result);
            }
            Message::Modifiers(modifiers) => self.modifiers = modifiers,
            Message::Manage(event) => return self.manage_message(event),
            Message::SettingsLoaded(result) => match result {
                Ok(prefs) => {
                    self.on_demand_models = prefs.ai.on_demand;
                    self.retain_indexes = prefs.ai.retain_indexes;
                    let value = prefs.ai;
                    let service = self.service.clone();
                    self.preferences = prefs;
                    let cards_service = self.service.clone();
                    let paths = self
                        .preferences
                        .recent
                        .iter()
                        .map(|(_, path)| path.clone())
                        .collect();
                    self.library_covers
                        .retain(|path, _| self.preferences.recent.iter().any(|(_, p)| p == path));
                    return Task::batch([
                        Task::perform(async move { service.ai_policy(value).await }, |r| match r {
                            Ok(()) => Message::Noop,
                            Err(e) => Message::Notice(e),
                        }),
                        Task::perform(
                            async move { cards_service.library_cards(paths).await },
                            Message::LibraryCardsLoaded,
                        ),
                    ]);
                }
                Err(e) => self.error = Some(e),
            },
            Message::LibraryCardsLoaded(Ok(cards)) => {
                for (path, cover) in cards {
                    if self.preferences.recent.iter().any(|(_, p)| p == &path) {
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
            Message::LibraryCardsLoaded(Err(error)) => self.error = Some(error),
            Message::ShowLibraries if !self.manager.busy => {
                self.stop_viewer();
                self.library_picker = true;
                self.screen = Screen::Overview;
                self.preview_expanded = false;
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
            Message::WindowClose => {
                let playback = self.playback.take();
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
                if let Some(handle) = self.preview_task.take() {
                    handle.abort();
                }
                if let Some(handle) = self.masonry_task.take() {
                    handle.abort();
                }
                let service = self.service.clone();
                self.status = "正在安全关闭资源库…".into();
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
                    return Task::perform(async move { service.reveal(id).await }, |r| {
                        Message::PlatformDone("已请求在文件夹中显示资源库", r)
                    });
                }
            }
            Message::OpenSourceRepository => {
                return Task::perform(moriq_app::open_source_repository(), |r| {
                    Message::PlatformDone("已请求在浏览器中打开项目开源地址", r)
                });
            }
            Message::OpenModelsDirectory => {
                return Task::perform(moriq_app::open_models_directory(), |r| {
                    Message::PlatformDone("已请求打开模型文件夹", r)
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
                Ok(prefs) => self.preferences = prefs,
                Err(e) => self.error = Some(e),
            },
            Message::ShowCreate => {
                self.name = "我的素材库".into();
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
                if let Some(asset) = &self.selected {
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
                if let (Some(library), Some(asset)) = (&self.library, &mut self.selected) {
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
                return self.persist_notice("提示", message, 0, "");
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
                        .selected
                        .as_ref()
                        .is_some_and(|selected| selected.id == asset)
                {
                    self.dialog = None;
                }
                return self.update(Message::Saved(id, result));
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
                    async {
                        rfd::AsyncFileDialog::new()
                            .set_title("选择资源库保存位置")
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
                    async {
                        rfd::AsyncFileDialog::new()
                            .set_title("选择 .moriq 资源库目录")
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
                        self.stop_people_portraits();
                        self.ai.portraits.clear();
                        self.ai.portrait_request = self.ai.portrait_request.wrapping_add(1);
                        self.query = Query::default();
                        self.error = None;
                        self.status = "资源库已就绪".into();
                        let imports = std::mem::take(&mut self.launch.import_paths);
                        return Task::batch([
                            self.reset_query(),
                            self.refresh_organizer(),
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
                    self.preview_expanded = false;
                    return Task::batch([self.load_volume(), self.restore_page()]);
                }
                self.preview_expanded = false;
                return self.restore_page();
            }
            Message::Search(value) => {
                self.library_picker = false;
                self.ai.request = self.ai.request.wrapping_add(1);
                self.query.semantic_active = false;
                self.query.semantic_ids.clear();
                self.query.text = value;
                self.screen = Screen::Assets;
                return self.reset_query();
            }
            Message::AllAssets => {
                self.query = Query::default();
                self.screen = Screen::Assets;
                return self.reset_query();
            }
            Message::Favorites => {
                self.query = Query {
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
                    async {
                        rfd::AsyncFileDialog::new()
                            .set_title("导入图片 · 原文件保持不变")
                            .add_filter("图片", moriq_app::IMPORT_EXTENSIONS)
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
                let Some(library) = &self.library else {
                    return Task::none();
                };
                if paths.is_empty() {
                    self.manager.busy = false;
                    return Task::none();
                }
                let control = ImportControl {
                    referenced: self.manager.referenced_import,
                    ..ImportControl::with_preferences(self.preferences.import)
                };
                self.manager.busy = false;
                self.manager.panel = None;
                self.manager.import_seen = 0;
                self.import = Some(control.clone());
                let service = self.service.clone();
                let id = library.info.id;
                let project = self.query.project;
                self.screen = Screen::Assets;
                self.status = "正在扫描并导入，原文件保持不变…".into();
                return Task::perform(
                    async move { service.import_sources(id, paths, project, control).await },
                    Message::Imported,
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
                    self.preferences = prefs;
                    self.status = "导入设置已保存，下次导入生效".into();
                }
                Err(error) => self.error = Some(error),
                _ => {}
            },
            Message::CancelImport => {
                if let Some(import) = &self.import {
                    import.cancel.store(true, Ordering::Relaxed);
                    self.status = "正在完成当前文件，随后停止导入…".into();
                }
            }
            Message::Imported(report) => {
                let parallelism = self
                    .import
                    .as_ref()
                    .map(|job| {
                        format!(
                            " · 解码峰值 {} 张 / 上限 {} 张",
                            job.peak.load(Ordering::Relaxed),
                            job.workers
                        )
                    })
                    .unwrap_or_default();
                self.import = None;
                self.manager.referenced_import = false;
                self.status = format!(
                    "{}导入 {} 张 · 跳过 {} 张重复图片 · 失败 {} 张",
                    if report.cancelled {
                        "已停止 · "
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
                        "导入结果",
                        self.status.clone() + "\n" + &report.failures.join("\n"),
                        if report.failed > 0 { 2 } else { 0 },
                        if report.failed > 0 { "recover" } else { "" },
                    ),
                    self.reset_query(),
                    self.refresh_info(),
                    self.refresh_organizer(),
                ]);
            }
            Message::Jobs(event) => return self.job_message(event),
            Message::Ai(event) => return self.ai_message(event),
            Message::Regions(event) => return self.region_message(event),
            Message::DpiChanged(scale) if scale.is_finite() && (0.5..=8.0).contains(&scale) => {
                self.dpi_scale = scale;
                if self.regions.target_scale.is_some() {
                    self.regions.target_scale = Some(1.0 / scale);
                    self.regions.zoom_request += 1;
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
            Message::Tick => {
                if self.manager.panel != Some(manage::Panel::People) {
                    self.stop_people_portraits();
                } else if self.ai.portrait_cancel.load(Ordering::Acquire) {
                    return self.refresh_people_portraits();
                }
                if (self.screen != Screen::Assets || self.library_picker)
                    && (self.preview_expanded
                        || self.playback.is_some()
                        || self.regions.session.is_some())
                {
                    self.stop_viewer();
                }
                if self
                    .playback
                    .as_ref()
                    .is_some_and(|p| !p.running().unwrap_or(false))
                {
                    self.playback = None;
                    self.video_frame = None;
                    self.video_upload = false;
                    self.error = Some("内置播放器已停止。请重新打开视频；原文件保留。".into());
                }
                if !self.ai.loaded && !self.ai.loading {
                    return self.ai_message(AiMessage::Refresh);
                }
                if let Some(import) = &self.import {
                    let finished = import.finished.load(Ordering::Relaxed);
                    if finished > self.manager.import_seen
                        && !self.loading
                        && self
                            .manager
                            .import_refreshed
                            .is_none_or(|last| last.elapsed() >= Duration::from_secs(1))
                    {
                        self.manager.import_seen = finished;
                        self.manager.import_refreshed = Some(std::time::Instant::now());
                        return Task::batch([self.refresh(), self.refresh_info()]);
                    }
                }
                self.manager.synco_ticks = self.manager.synco_ticks.saturating_add(1);
                if self.manager.synco_ticks >= self.manager.synco_backoff.max(8)
                    && !self.manager.synco_running
                    && self.manager.organizer.synco.iter().any(|t| {
                        t.ack.is_some()
                            || ["queued", "importing", "editing", "rendering", "ready"]
                                .contains(&t.state.as_str())
                    })
                {
                    return self.manage_message(ManageMessage::SyncoPoll(false));
                }
                #[cfg(feature = "capture")]
                if self.launch.capture.is_some()
                    && (!self.loading || self.import.is_some())
                    && !self.switching
                {
                    self.capture_ticks = self.capture_ticks.saturating_add(1);
                    if self.launch.panel.as_deref() == Some("video-motion")
                        && self.capture_ticks == 6
                    {
                        return iced::window::oldest()
                            .and_then(iced::window::screenshot)
                            .map(Message::VideoProof);
                    }
                    if self.launch.panel.as_deref() == Some("video-controls") {
                        match self.capture_ticks {
                            4 | 8 => return self.update(Message::VideoPause),
                            6 => return self.update(Message::VideoSeek(0.6)),
                            9 | 10 => return self.update(Message::VideoMute),
                            _ => {}
                        }
                    }
                    if matches!(
                        self.launch.panel.as_deref(),
                        Some("viewer-return" | "gallery-scroll")
                    ) {
                        if self.capture_ticks == 2 {
                            self.scroll_y = 900.0;
                            self.pending_scroll = Some(900.0);
                            self.surfaces.restoring_gallery = Some(900.0);
                            return self.refresh();
                        }
                        if self.capture_ticks == 5 {
                            let index = self
                                .active_masonry()
                                .and_then(|m| {
                                    m.columns
                                        .iter()
                                        .flatten()
                                        .find(|p| p.top >= self.scroll_y)
                                        .map(|p| p.index)
                                })
                                .unwrap_or(
                                    (self.scroll_y / self.grid().row_height) as usize
                                        * self.grid().columns,
                                );
                            if let Some(id) = index
                                .checked_sub(self.offset)
                                .and_then(|i| self.tiles.get(i))
                                .map(|t| t.asset.id)
                            {
                                return self.update(
                                    if self.launch.panel.as_deref() == Some("viewer-return") {
                                        Message::OpenImage(id)
                                    } else {
                                        Message::Select(id)
                                    },
                                );
                            }
                        }
                        if self.capture_ticks == 8
                            && self.launch.panel.as_deref() == Some("viewer-return")
                        {
                            return self.update(Message::Back);
                        }
                    }
                    if self.capture_ticks == 8
                        && self.launch.panel.as_deref() == Some("delete-project-final")
                    {
                        return self.deletion_message(DeletionMessage::Next);
                    }
                    if self.capture_ticks == 8
                        && self.launch.panel.as_deref() == Some("actual-regions")
                    {
                        return self.region_message(RegionMessage::Actual);
                    }
                    if self.capture_ticks == 8
                        && self.launch.panel.as_deref() == Some("viewer-leave")
                    {
                        return self.commit_navigation(Message::Navigate(2));
                    }
                    if self.capture_ticks == 8 {
                        let scroll_target = match self.launch.panel.as_deref() {
                            Some("settings-bottom") => Some(("management", 100000.0)),
                            Some("details-ai") => Some(("details", 220.0)),
                            Some("details-bottom") => Some(("details", 100000.0)),
                            _ => None,
                        };
                        if let Some((id, y)) = scroll_target {
                            return iced::widget::operation::scroll_to(
                                id,
                                iced::widget::scrollable::AbsoluteOffset { x: 0.0, y },
                            );
                        }
                    }
                    if self.capture_ticks == 2 {
                        if self.launch.panel.as_deref() == Some("details-ai") {
                            self.detail_tab = 2;
                        }
                        match self.launch.panel.as_deref() {
                            Some("ai-refresh") => {
                                return self.job_message(tasks::JobMessage::Queue(
                                    "analysis:chinese-clip".into(),
                                ));
                            }
                            Some("people") => {
                                return self.ai_message(AiMessage::People);
                            }
                            Some("category-gallery") => {
                                if let Some((category, _)) =
                                    self.manager.organizer.ai_categories.first()
                                {
                                    return self.update(Message::Ai(AiMessage::Category(
                                        category.clone(),
                                    )));
                                }
                                self.capture_ticks = 1;
                                return self.job_tick();
                            }
                            Some("model-switch") if self.screen == Screen::Models => {
                                if let Some((_, path)) = self.preferences.recent.get(1) {
                                    return self.update(Message::Ai(AiMessage::SwitchLibrary(
                                        path.clone(),
                                    )));
                                }
                                self.capture_ticks = 1;
                                return Task::none();
                            }
                            Some("models-bottom") if self.screen == Screen::Models => {
                                return iced::widget::operation::scroll_to(
                                    "models",
                                    iced::widget::scrollable::AbsoluteOffset { x: 0., y: 420. },
                                );
                            }
                            Some("delete-project" | "delete-project-final") => {
                                let project = self
                                    .library
                                    .as_ref()
                                    .and_then(|l| {
                                        l.info
                                            .projects
                                            .iter()
                                            .find(|p| p.count == 0)
                                            .or_else(|| l.info.projects.first())
                                    })
                                    .map(|p| p.id);
                                if let Some(project) = project {
                                    self.manager.project_target = Some(project);
                                    self.manager.panel = Some(manage::Panel::Project);
                                    return self
                                        .deletion_message(DeletionMessage::Project(project));
                                }
                            }
                            Some("notifications") => {
                                return self.journal_message(JournalMessage::Open(true));
                            }
                            Some("activity") => {
                                return self.journal_message(JournalMessage::Open(false));
                            }
                            Some("storage") => return self.storage_message(StorageMessage::Open),
                            Some("rules") => return self.rule_message(RuleMessage::Open),
                            Some("location") => {
                                return self.manage_message(ManageMessage::Location);
                            }
                            Some(
                                "expanded" | "actual-regions" | "viewer-leave" | "video-motion"
                                | "video-controls",
                            ) => {
                                return self.set_viewer(true);
                            }
                            Some("compare") => return self.compare_message(CompareMessage::Open),
                            Some("help") => return self.manage_message(ManageMessage::Help),
                            _ => {}
                        }
                        self.manager.panel = match self.launch.panel.as_deref() {
                            Some("settings" | "settings-bottom") => {
                                self.dialog = Some(Dialog::Settings);
                                None
                            }
                            Some("export") => Some(manage::Panel::ExportOptions),
                            Some("tags") => Some(manage::Panel::Tags),
                            Some("projects") => Some(manage::Panel::Project),
                            Some("tools") => Some(manage::Panel::Menu),
                            Some("filters") => Some(manage::Panel::Filters),
                            Some("organize") => Some(manage::Panel::Organize),
                            Some("maintenance") => Some(manage::Panel::Maintenance),
                            Some("collections") => Some(manage::Panel::Collections),
                            Some("synco") => Some(manage::Panel::Synco),
                            Some("tasks") => Some(manage::Panel::Tasks),
                            Some("asset-menu") => Some(manage::Panel::AssetMenu),
                            Some("asset-projects") => Some(manage::Panel::AssetProjects),
                            Some("versions") => Some(manage::Panel::Versions),
                            Some("views") => Some(manage::Panel::ViewOptions),
                            _ => None,
                        };
                    }
                    if self.capture_ticks == 12 {
                        if self.launch.panel.as_deref() == Some("browse-stress")
                            && self.capture_stress_steps < 80
                        {
                            self.capture_ticks = 11;
                            let step = self.capture_stress_steps;
                            self.capture_stress_steps += 1;
                            if let Some(id) = self
                                .tiles
                                .get(step as usize % self.tiles.len().max(1))
                                .map(|t| t.asset.id)
                            {
                                return match step % 4 {
                                    0 | 1 => {
                                        let open = self.update(Message::OpenImage(id));
                                        let zoom = self.region_message(RegionMessage::Actual);
                                        Task::batch([open, zoom])
                                    }
                                    2 => self.set_viewer(false),
                                    _ => self.update(Message::Select(id)),
                                };
                            }
                        }
                        if self.launch.panel.as_deref() == Some("people")
                            && (self.manager.organizer.people.is_empty()
                                || self
                                    .manager
                                    .organizer
                                    .people
                                    .iter()
                                    .filter(|p| p.count > 0)
                                    .any(|p| !self.ai.portraits.contains_key(&p.id)))
                        {
                            self.capture_ticks = 11;
                            return self.job_tick();
                        }
                        if self.launch.panel.as_deref() == Some("ai-refresh")
                            && (!self.manager.organizer.jobs.iter().any(|job| {
                                job.kind == "analysis:chinese-clip" && job.state == "completed"
                            }) || self.manager.organizer.ai_categories.is_empty())
                        {
                            self.capture_ticks = 11;
                            return self.job_tick();
                        }
                        return iced::window::oldest()
                            .and_then(iced::window::screenshot)
                            .map(Message::Captured);
                    }
                }
                return self.job_tick();
            }
            #[cfg(feature = "capture")]
            Message::VideoProof(screenshot) => {
                if let Some(path) = self.launch.capture.as_ref() {
                    let path = path.with_extension("early.png");
                    let service = self.service.clone();
                    let request = self.original_request;
                    let state = format!(
                        "presented_frames={} progress={:?}\n",
                        self.video_presented,
                        self.playback.as_ref().map(|p| p.progress())
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
            Message::Captured(screenshot) => {
                if let Some(path) = self.launch.capture.take() {
                    let service = self.service.clone();
                    let playback = self.playback.clone();
                    let playback_state = format!(
                        "expanded={} request={} error={:?} presented_frames={} first_gpu_ready_ms={:?}\n",
                        self.preview_expanded,
                        self.original_request,
                        self.error,
                        self.video_presented,
                        self.video_first_ready_ms
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
                        self.drain_uploads();
                        if page.warnings > 0 {
                            self.status = format!("{} 张缩略图不可用，原片仍被保留", page.warnings);
                        }
                        if let Some(selected) = self.selected.as_mut()
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
                        let needed = self.visible_range();
                        if needed.start < self.offset || needed.end > self.offset + self.tiles.len()
                        {
                            return self.refresh();
                        }
                        if self.launch.select_first
                            && let Some(tile) = self.tiles.first()
                        {
                            let id = tile.asset.id;
                            self.launch.select_first = false;
                            return self.update(Message::Select(id));
                        }
                        return self.restore_scroll();
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            Message::Scrolled(y) => {
                self.gallery_feedback.clear();
                if self.preview_expanded
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
                self.viewport = size;
                return if self.screen == Screen::Assets {
                    Task::batch([self.refresh(), self.rebuild_masonry()])
                } else {
                    self.refresh()
                };
            }
            Message::Zoom(value) => {
                self.card_width = value;
                return self.reset_query();
            }
            Message::List(value) => {
                self.manager.panel = None;
                self.screen = Screen::Assets;
                self.list = value;
                return self.reset_query();
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
                        .selected
                        .as_ref()
                        .is_some_and(|a| a.id == id && a.version_id == version) =>
            {
                self.preview_task = None;
                match result {
                    Ok((preview, background)) => {
                        if self.original_size.is_none() || self.regions.session.is_some() {
                            self.preview = Some(preview);
                        }
                        self.preview_background = Some(background);
                    }
                    Err(e) => self.error = Some(e),
                }
            }
            Message::VideoFrame if !self.video_upload => {
                if let Some(playback) = &self.playback {
                    match playback.take_frame() {
                        Ok(Some(frame)) => {
                            let request = self.original_request;
                            self.video_upload = true;
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
                            self.playback = None;
                            self.video_frame = None;
                        }
                    }
                }
            }
            Message::VideoUploaded(request, result)
                if request == self.original_request && self.playback.is_some() =>
            {
                self.video_upload = false;
                match result {
                    Ok(frame) => {
                        if self.video_first_ready_ms.is_none() {
                            self.video_first_ready_ms =
                                self.playback.as_ref().map(|p| p.elapsed_ms());
                        }
                        self.video_frame = Some(frame);
                        self.video_presented += 1;
                    }
                    Err(error) => {
                        self.error = Some(error);
                        self.playback = None;
                        self.video_frame = None;
                    }
                }
            }
            Message::VideoPause | Message::VideoMute | Message::VideoSeek(_) => {
                if let Some(playback) = self.playback.clone() {
                    let seek = match message {
                        Message::VideoPause => {
                            let (position, duration) = playback.progress();
                            if duration > 0 && position + 80 >= duration {
                                self.video_paused = false;
                                Some(0)
                            } else {
                                self.video_paused = !self.video_paused;
                                None
                            }
                        }
                        Message::VideoMute => {
                            self.video_muted = !self.video_muted;
                            None
                        }
                        Message::VideoSeek(value) => {
                            Some((value.clamp(0., 1.) * playback.progress().1 as f32) as u64)
                        }
                        _ => None,
                    };
                    self.video_control += 1;
                    let revision = self.video_control;
                    let (paused, muted, request) =
                        (self.video_paused, self.video_muted, self.original_request);
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
            Message::VideoControlled(request, Err(error)) if request == self.original_request => {
                self.error = Some(error);
            }
            Message::PlaybackStarted(library, id, version, request, result)
                if self.preview_expanded
                    && !self.surface_open()
                    && self.original_request == request
                    && self.library.as_ref().is_some_and(|l| l.info.id == library)
                    && self
                        .selected
                        .as_ref()
                        .is_some_and(|a| a.id == id && a.version_id == version) =>
            {
                match result {
                    Ok(session) => {
                        self.video_paused = false;
                        self.video_muted = false;
                        self.video_presented = 0;
                        self.video_first_ready_ms = None;
                        self.video_frame = None;
                        self.video_upload = false;
                        self.playback = Some(session);
                    }
                    Err(e) => self.error = Some(e),
                }
            }
            Message::ClosePreview => {
                if self.surface_open() {
                    return self.close_surface();
                }
                if self.preview_expanded {
                    return self.set_viewer(false);
                }
                if let Some(handle) = self.preview_task.take() {
                    handle.abort();
                }
                self.clear_browsing();
                self.selected = None;
                self.preview = None;
                return self.refresh();
            }
            Message::ExpandPreview => return self.set_viewer(!self.preview_expanded),
            Message::Favorite(asset, value) => {
                if let Some(library) = &self.library {
                    let service = self.service.clone();
                    let id = library.info.id;
                    return self.mutation(async move { service.favorite(id, asset, value).await });
                }
            }
            Message::Rating(rating) => {
                if let (Some(library), Some(asset)) = (&self.library, &self.selected) {
                    let service = self.service.clone();
                    let id = library.info.id;
                    let asset = asset.id;
                    return self.mutation(async move { service.rating(id, asset, rating).await });
                }
            }
            Message::Tags(value) => self.tag_input = value,
            Message::SaveTags => {
                if let (Some(library), Some(asset)) = (&self.library, &self.selected) {
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
            Message::Saved(id, result)
                if self.library.as_ref().is_some_and(|l| l.info.id == id) =>
            {
                match result {
                    Ok(()) => {
                        self.status = "更改已保存到资源库".into();
                        let browse = self
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
                            self.refresh_organizer(),
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
            Message::ReducedMotion(enabled) => {
                self.preferences.reduced_motion = enabled;
                self.gallery_feedback.clear();
                self.surface_motion = None;
                let pending = self
                    .transition
                    .take()
                    .and_then(|mut transition| transition.pending.take());
                let navigation = pending
                    .map(|message| self.commit_navigation(*message))
                    .unwrap_or_else(Task::none);
                let service = self.service.clone();
                return Task::batch([
                    navigation,
                    Task::perform(
                        async move { service.reduced_motion(enabled).await },
                        Message::SettingsLoaded,
                    ),
                ]);
            }
            Message::DiagnosticLogging(enabled) => {
                self.preferences.diagnostic_logging = enabled;
                let service = self.service.clone();
                return Task::perform(
                    async move { service.diagnostic_logging(enabled).await },
                    Message::SettingsLoaded,
                );
            }
            // Disabled actions and receipts from an older request are intentionally ignored.
            Message::ShowLibraries
            | Message::OpenRecent(..)
            | Message::AiPolicySaved(..)
            | Message::LibraryRenamed(..)
            | Message::AssetRenamed(..)
            | Message::MasonryLoaded(..)
            | Message::Create
            | Message::Open
            | Message::Import
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
            | Message::InfoLoaded(_, Err(_))
            | Message::CreateProject
            | Message::ProjectCreated(..)
            | Message::Noop
            | Message::TransitionFrame(_) => {}
        }
        Task::none()
    }
    fn view(&self) -> Element<'_, Message> {
        self.shell()
    }
}
