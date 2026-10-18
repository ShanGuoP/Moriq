//! Native presentation. All disk, catalog and decoding work belongs to Service.
mod assets;
mod browsing;
mod compare;
mod deletion;
mod popups;
mod project_form;
mod surfaces;
mod tasks;
pub use deletion::DeletionMessage;
mod components;
mod gallery;
pub use gallery::TagChoice;
mod i18n;
mod icons;
mod journal;
mod rules;
pub use compare::CompareMessage;
mod storage;
pub use rules::RuleMessage;
pub use storage::StorageMessage;
mod manage;
mod models;
mod navigation;
pub use journal::JournalMessage;
pub use manage::ManageMessage;
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
use std::{path::PathBuf, sync::atomic::Ordering, time::Duration};

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
mod tests {
    use super::*;
    pub(super) fn app() -> App {
        let (mut app, task) = App::new(
            Service::start().unwrap(),
            LaunchOptions::default(),
            std::sync::Arc::new(i18n::Locale::load().unwrap()),
        );
        drop(task);
        app.library = Some(Opened {
            root: PathBuf::from("test.moriq"),
            info: LibraryInfo {
                id: LibraryId::default(),
                name: "Test".into(),
                count: 100000,
                favorites: 0,
                bytes: 0,
                created_at: 0,
                updated_at: 0,
                projects: vec![],
            },
        });
        app.total = 100000;
        app
    }
    #[test]
    fn maintenance_and_purge_receipts_cannot_reset_another_library_selection() {
        let mut app = app();
        let control = moriq_app::MaintenanceControl::default();
        app.manager.maintenance = Some(control.clone());
        app.manager.busy = true;
        let selected = AssetId::default();
        app.manager.selection.insert(selected);
        let old = LibraryId::default();
        drop(app.update(Message::Manage(ManageMessage::Checked(
            old,
            Ok(MaintenanceReport::default()),
        ))));
        drop(app.update(Message::Manage(ManageMessage::Purged(
            old,
            Ok(PurgeReport::default()),
        ))));
        drop(app.update(Message::Deletion(DeletionMessage::Finished(
            old,
            0,
            Ok(PurgeReport::default()),
        ))));
        assert!(app.manager.busy);
        assert!(app.manager.maintenance.is_some());
        assert!(app.manager.selection.contains(&selected));
        drop(app.update(Message::Manage(ManageMessage::CancelCheck)));
        assert!(control.cancel.load(Ordering::Relaxed));
    }
    #[test]
    fn page_chrome_is_identical_and_empty_library_navigation_does_not_move_content() {
        let mut app = app();
        let mut metrics = Vec::new();
        for screen in [Screen::Overview, Screen::Assets, Screen::Models] {
            app.screen = screen;
            metrics.push((
                app.sidebar_width(),
                app.frame_padding(),
                app.titlebar_height(),
            ));
        }
        assert!(metrics.windows(2).all(|pair| pair[0] == pair[1]));
        app.library = None;
        drop(app.update(Message::Navigate(0)));
        drop(app.update(Message::Navigate(2)));
        assert_eq!(app.screen, Screen::Models);
        assert!(app.transition.is_none());
    }
    #[test]
    fn rapid_page_navigation_retargets_without_jumps_or_committing_old_destinations() {
        use std::time::Duration;
        let mut app = app();
        app.screen = Screen::Overview;
        drop(app.update(Message::Navigate(2)));
        let start = app.transition.as_ref().unwrap().started;
        assert_eq!(app.screen, Screen::Overview);
        drop(app.update(Message::TransitionFrame(start + Duration::from_millis(35))));
        let alpha = app.transition.as_ref().unwrap().alpha();
        assert!((alpha - 0.5).abs() < 0.01);
        drop(app.update(Message::Navigate(1)));
        assert_eq!(app.transition.as_ref().unwrap().alpha(), alpha);
        drop(app.update(Message::TransitionFrame(start + Duration::from_millis(70))));
        assert_eq!(app.screen, Screen::Assets);
        assert_eq!(app.transition.as_ref().unwrap().alpha(), 1.0);
        drop(app.update(Message::TransitionFrame(start + Duration::from_millis(180))));
        assert!(app.transition.is_none());
        drop(app.update(Message::Navigate(2)));
        let start = app.transition.as_ref().unwrap().started;
        drop(app.update(Message::TransitionFrame(start + Duration::from_millis(35))));
        drop(app.update(Message::Navigate(1)));
        assert!(app.transition.as_ref().unwrap().pending.is_none());
        let start = app.transition.as_ref().unwrap().started;
        drop(app.update(Message::TransitionFrame(start + Duration::from_millis(110))));
        assert!(app.transition.is_none());
        assert_eq!(app.screen, Screen::Assets);
    }
    #[test]
    fn old_query_results_cannot_replace_new_view() {
        let mut app = app();
        app.generation = 3;
        drop(app.update(Message::PageLoaded(
            2,
            Ok(LoadedPage {
                total: 0,
                offset: 0,
                tiles: vec![],
                warnings: 0,
            }),
        )));
        assert_eq!(app.total, 100000);
        assert_eq!(app.generation, 3);
    }
    #[test]
    fn range_selection_receipts_obey_library_and_query_identity() {
        let mut app = app();
        let library = app.library.as_ref().unwrap().info.id;
        let asset = AssetId::default();
        app.manager.multiselect = true;
        app.generation = 5;
        drop(app.update(Message::Manage(ManageMessage::RangeSelected(
            library,
            4,
            Ok(vec![asset]),
        ))));
        drop(app.update(Message::Manage(ManageMessage::RangeSelected(
            LibraryId::default(),
            5,
            Ok(vec![asset]),
        ))));
        assert!(app.manager.selection.is_empty());
        drop(app.update(Message::Manage(ManageMessage::RangeSelected(
            library,
            5,
            Ok(vec![asset]),
        ))));
        assert!(app.manager.selection.contains(&asset));
        app.manager.selection_anchor = Some(asset);
        drop(app.reset_query());
        assert!(app.manager.selection_anchor.is_none());
    }
    #[test]
    fn large_displays_keep_viewport_inside_page_budget() {
        let mut app = app();
        for (width, height) in [(1360.0, 800.0), (3840.0, 2160.0), (7680.0, 4320.0)] {
            app.viewport = Size::new(width, height);
            for list in [false, true] {
                app.list = list;
                let grid = app.grid();
                assert!(grid.visible(500000, 90000.0).len() <= 128);
            }
        }
    }
    #[test]
    fn scroll_moved_during_load_requests_current_viewport() {
        let mut app = app();
        app.generation = 3;
        app.loading = true;
        app.scroll_y = 90000.0;
        drop(app.update(Message::PageLoaded(
            3,
            Ok(LoadedPage {
                total: 100000,
                offset: 0,
                tiles: vec![],
                warnings: 0,
            }),
        )));
        assert!(app.loading);
        assert_eq!(app.generation, 4);
    }
    #[test]
    fn saved_edit_survives_query_change_but_rejects_other_library() {
        let mut app = app();
        app.generation = 8;
        let current = app.library.as_ref().unwrap().info.id;
        drop(app.update(Message::Saved(
            LibraryId::default(),
            Err("stale failure".into()),
        )));
        assert!(app.error.is_none());
        drop(app.update(Message::Saved(current, Ok(()))));
        assert_eq!(app.generation, 9);
        assert!(app.loading);
    }
    #[test]
    fn background_save_does_not_close_settings_dialog() {
        let mut app = app();
        let id = app.library.as_ref().unwrap().info.id;
        app.dialog = Some(Dialog::Settings);
        drop(app.update(Message::Saved(id, Ok(()))));
        assert_eq!(app.dialog, Some(Dialog::Settings));
    }
    pub(super) fn selected_asset() -> Asset {
        Asset {
            id: AssetId::default(),
            version_id: AssetVersionId::default(),
            name: "测试".into(),
            format: "png".into(),
            width: 20,
            height: 12,
            bytes: 30,
            favorite: false,
            rating: 0,
            tags: vec![],
            imported_at: 0,
        }
    }
    #[test]
    fn image_gestures_target_clicked_card_and_reject_missing_or_modal_targets() {
        let mut app = app();
        app.screen = Screen::Assets;
        let first = selected_asset();
        let second = selected_asset();
        app.tiles = vec![
            Tile {
                asset: first.clone(),
                handle: None,
            },
            Tile {
                asset: second.clone(),
                handle: None,
            },
        ];
        app.selected = Some(first.clone());
        app.manager.multiselect = true;
        app.manager.selection.insert(first.id);
        drop(app.update(Message::AssetContext(second.id)));
        assert_eq!(app.selected.as_ref().unwrap().id, second.id);
        assert_eq!(app.manager.panel, Some(manage::Panel::AssetMenu));
        assert!(!app.manager.multiselect);
        assert!(!app.preview_expanded);
        drop(app.update(Message::OpenImage(first.id)));
        assert_eq!(app.selected.as_ref().unwrap().id, second.id);
        drop(app.update(Message::Manage(ManageMessage::Close)));
        drop(app.update(Message::OpenImage(first.id)));
        assert_eq!(app.selected.as_ref().unwrap().id, first.id);
        assert!(app.preview_expanded);
        drop(app.update(Message::OpenImage(AssetId::default())));
        assert_eq!(app.selected.as_ref().unwrap().id, first.id);
        app.dialog = Some(Dialog::Settings);
        drop(app.update(Message::AssetContext(second.id)));
        assert_eq!(app.selected.as_ref().unwrap().id, first.id);
        assert!(app.manager.panel.is_none());
    }
    #[test]
    fn journal_pages_and_related_asset_navigation_reject_stale_receipts() {
        let mut app = app();
        let id = app.library.as_ref().unwrap().info.id;
        drop(app.update(Message::Journal(JournalMessage::Open(true))));
        let page = EventPage {
            unread: 8,
            ..Default::default()
        };
        for (lib, request) in [(LibraryId::default(), 1), (id, 0)] {
            drop(app.update(Message::Journal(JournalMessage::Loaded(
                lib,
                true,
                request,
                true,
                Ok(page.clone()),
            ))));
        }
        assert_eq!(app.journal.notifications.unread, 0);
        drop(app.update(Message::Journal(JournalMessage::Loaded(
            id,
            true,
            1,
            true,
            Ok(page.clone()),
        ))));
        assert_eq!(app.journal.notifications.unread, 8);
        let reopened = app.library.clone().unwrap();
        drop(app.update(Message::Opened(Ok(reopened))));
        drop(app.update(Message::Journal(JournalMessage::Loaded(
            id,
            true,
            1,
            true,
            Ok(page),
        ))));
        assert_eq!(app.journal.notifications.unread, 0);
        let asset = AssetId::default();
        drop(app.update(Message::Journal(JournalMessage::OpenAsset(asset))));
        let request = app.browse_request;
        drop(app.update(Message::Search("当前搜索".into())));
        drop(app.update(Message::Journal(JournalMessage::Located(
            id,
            request,
            asset,
            Ok(Some(true)),
        ))));
        assert_eq!(app.query.text, "当前搜索");
        assert!(!app.query.trash);
    }
    #[test]
    fn project_menu_keeps_gallery_context_and_cancelled_or_stale_delete_preserves_data() {
        let mut app = app();
        let project = ProjectId::default();
        let current = ProjectId::default();
        app.library.as_mut().unwrap().info.projects.push(Project {
            visited_at: 0,
            id: project,
            parent: None,
            name: "空项目".into(),
            count: 0,
        });
        app.query.project = Some(current);
        app.screen = Screen::Assets;
        let selected = selected_asset();
        let id = selected.id;
        app.selected = Some(selected);
        drop(app.update(Message::Manage(ManageMessage::ProjectMenu(project))));
        assert_eq!(app.query.project, Some(current));
        assert_eq!(app.manager.project_target, Some(project));
        assert_eq!(app.manager.label, "空项目");
        assert_eq!(app.selected.as_ref().unwrap().id, id);
        app.manager.busy = true;
        drop(app.update(Message::Deletion(DeletionMessage::Planned(
            LibraryId::default(),
            0,
            Err("已取消".into()),
        ))));
        drop(app.update(Message::Manage(ManageMessage::ProjectDeleted(
            LibraryId::default(),
            vec![current],
            Ok(PurgeReport::default()),
        ))));
        assert!(app.manager.busy);
        assert_eq!(app.query.project, Some(current));
        app.manager.busy = false;
        app.manager.panel = Some(manage::Panel::Delete);
        drop(app.update(Message::Deletion(DeletionMessage::Cancel)));
        assert!(!app.manager.busy);
        assert_eq!(app.selected.as_ref().unwrap().id, id);
    }
    fn settle(app: &mut App) {
        if let Some(t) = &app.transition {
            let start = t.started;
            drop(app.update(Message::TransitionFrame(start + Duration::from_millis(70))));
        }
        app.transition = None;
    }
    #[test]
    fn history_restores_query_scroll_selection_and_draft_then_truncates_forward() {
        let mut app = app();
        app.screen = Screen::Assets;
        app.query.tag = "人物".into();
        app.scroll_y = 4500.0;
        let asset = selected_asset();
        let id = asset.id;
        app.selected = Some(asset);
        app.manager.selection.insert(id);
        app.manager.selection_anchor = Some(id);
        app.manager.multiselect = true;
        app.manager.notes = "未保存备注".into();
        app.manager.notes_dirty = true;
        drop(app.update(Message::Favorites));
        settle(&mut app);
        assert!(app.history.can_back());
        assert!(app.query.favorites_only);
        drop(app.update(Message::Back));
        settle(&mut app);
        assert_eq!(app.query.tag, "人物");
        assert_eq!(app.scroll_y, 4500.0);
        assert!(app.manager.selection.contains(&id));
        assert_eq!(app.manager.selection_anchor, Some(id));
        assert!(app.manager.multiselect);
        assert_eq!(app.restored_notes, Some((id, "未保存备注".into())));
        assert!(app.history.can_forward());
        drop(app.update(Message::Forward));
        settle(&mut app);
        assert!(app.query.favorites_only);
        drop(app.update(Message::Back));
        settle(&mut app);
        drop(app.update(Message::Manage(ManageMessage::Tag("风景".into()))));
        assert!(!app.history.can_forward());
    }
    #[test]
    fn search_history_is_coalesced_and_new_library_clears_both_stacks() {
        let mut app = app();
        app.screen = Screen::Assets;
        for text in ["日", "日落", "日落海边"] {
            drop(app.update(Message::Search(text.into())));
        }
        drop(app.update(Message::Back));
        settle(&mut app);
        assert_eq!(app.query, Query::default());
        assert!(!app.history.can_back());
        assert!(app.history.can_forward());
        let mut other = app.library.clone().unwrap();
        other.info.id = LibraryId::default();
        drop(app.update(Message::Opened(Ok(other))));
        assert!(!app.history.can_back());
        assert!(!app.history.can_forward());
        assert!(app.pending_scroll.is_none());
    }
    #[test]
    fn neighborhood_receipts_reject_old_requests_queries_and_libraries() {
        let mut app = app();
        app.screen = Screen::Assets;
        let asset = selected_asset();
        let id = asset.id;
        let library = app.library.as_ref().unwrap().info.id;
        let result = moriq_app::LoadedNeighborhood {
            selected: id,
            position: 128,
            page: LoadedPage {
                total: 300,
                offset: 125,
                tiles: vec![moriq_app::Tile {
                    asset: asset.clone(),
                    thumbnail: None,
                }],
                warnings: 0,
            },
        };
        app.browse_request = 5;
        for (lib, request) in [(library, 4), (LibraryId::default(), 5)] {
            drop(app.update(Message::NeighborhoodLoaded(
                lib,
                request,
                Ok(Some(result.clone())),
            )));
            assert!(app.selected.is_none());
        }
        drop(app.update(Message::NeighborhoodLoaded(
            library,
            5,
            Ok(Some(result.clone())),
        )));
        assert_eq!(app.selected.as_ref().unwrap().id, id);
        assert_eq!(app.neighbor_position, Some((128, 300)));
        drop(app.update(Message::ClosePreview));
        drop(app.update(Message::NeighborhoodLoaded(
            library,
            5,
            Ok(Some(result.clone())),
        )));
        assert!(app.selected.is_none());
        drop(app.update(Message::Search("新的查询".into())));
        drop(app.update(Message::NeighborhoodLoaded(library, 5, Ok(Some(result)))));
        assert!(app.selected.is_none());
        assert!(app.neighbors.is_empty());
    }
    #[test]
    fn changing_query_invalidates_pending_layout_and_version_receipt_does_not_reselect() {
        let mut app = app();
        let asset = selected_asset();
        let id = asset.id;
        app.selected = Some(asset);
        let library = app.library.as_ref().unwrap().info.id;
        drop(app.update(Message::Manage(ManageMessage::VersionSelected(
            library,
            AssetId::default(),
            AssetVersionId::default(),
            Ok(()),
        ))));
        assert_eq!(app.selected.as_ref().unwrap().id, id);
        let generation = app.layout_generation;
        drop(app.update(Message::Search("新查询".into())));
        assert!(app.layout_generation > generation);
        drop(app.update(Message::MasonryLoaded(
            generation,
            Ok(std::sync::Arc::new(moriq_render::MasonryIndex::new(
                &[(10, 10)],
                1,
                100.0,
                10.0,
            ))),
        )));
        assert!(app.masonry.is_none());
    }
    #[test]
    fn old_version_preview_cannot_replace_the_new_version() {
        let mut app = app();
        let id = app.library.as_ref().unwrap().info.id;
        let asset = selected_asset();
        let old = AssetVersionId::default();
        app.selected = Some(asset.clone());
        drop(app.update(Message::PreviewLoaded(
            id,
            asset.id,
            old,
            Ok(image::Handle::from_rgba(1, 1, vec![0; 4])),
        )));
        assert!(app.preview.is_none());
    }
    #[test]
    fn background_details_and_save_receipts_preserve_newer_unsaved_notes() {
        let mut app = app();
        let id = app.library.as_ref().unwrap().info.id;
        let asset = selected_asset();
        app.selected = Some(asset.clone());
        drop(app.update(Message::Manage(ManageMessage::Notes(
            "用户刚输入的新内容".into(),
        ))));
        drop(app.update(Message::Manage(ManageMessage::DetailsLoaded(
            id,
            asset.id,
            asset.version_id,
            Ok(AssetDetails {
                notes: "旧数据库内容".into(),
                ..Default::default()
            }),
        ))));
        drop(app.update(Message::Manage(ManageMessage::NotesSaved(
            id,
            asset.id,
            "旧数据库内容".into(),
            Ok(()),
        ))));
        assert_eq!(app.manager.notes, "用户刚输入的新内容");
        assert!(app.manager.notes_dirty);
        drop(app.update(Message::Manage(ManageMessage::OrganizerLoaded(
            LibraryId::default(),
            Ok(Organizer {
                trash_count: 100,
                ..Default::default()
            }),
        ))));
        assert_eq!(app.manager.organizer.trash_count, 0);
    }
}
#[derive(Debug, Clone)]
pub enum Message {
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
    OpenRecent(PathBuf),
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
    PreviewLoaded(
        LibraryId,
        AssetId,
        AssetVersionId,
        Result<image::Handle, String>,
    ),
    FullPreviewLoaded(
        LibraryId,
        AssetId,
        AssetVersionId,
        Result<(image::Handle, image::Handle), String>,
    ),
    ClosePreview,
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
    Language,
    Captured(iced::window::Screenshot),
    CaptureSaved(Result<(), String>),
    ResourcesLoaded(Result<assets::Artwork, String>),
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
    HideProject,
    DesignPreview,
    Notice(String),
    DetailTab(u8),
    ModelTab(u8),
    SetMasonry(bool),
    CopyLibraryPath,
    RevealLibrary,
    ToggleTagEditor,
    ToggleModelPolicy,
    ToggleRetainIndexes,
    PlatformDone(&'static str, Result<(), String>),
    OpenSourceRepository,
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
    pub design_preview: bool,
    pub panel: Option<String>,
}
#[derive(Clone)]
struct Tile {
    asset: Asset,
    handle: Option<image::Handle>,
}
pub struct App {
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
    modifiers: iced::keyboard::Modifiers,
    manager: manage::Manager,
    preferences: UserSettings,
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
    english: bool,
    launch: LaunchOptions,
    capture_ticks: u8,
    page_task: Option<iced::task::Handle>,
    preview_task: Option<iced::task::Handle>,
    locale: std::sync::Arc<i18n::Locale>,
    artwork: assets::Artwork,
    masonry: Option<std::sync::Arc<moriq_render::MasonryIndex>>,
    masonry_task: Option<iced::task::Handle>,
    masonry_mode: bool,
    dialog: Option<Dialog>,
    detail_tab: u8,
    model_tab: u8,
    preview_source: Vec<Tile>,
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

pub fn run(launch: LaunchOptions) -> iced::Result {
    let initial_size = Size::new(1660.0, 940.0);
    let window_icon = launch.window_icon.clone();
    let service = Service::start().map_err(iced::Error::ExecutorCreationFailed)?;
    let locale = std::sync::Arc::new(i18n::Locale::load().map_err(|error| {
        iced::Error::ExecutorCreationFailed(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            error,
        ))
    })?);
    iced::application(
        move || App::new(service.clone(), launch.clone(), locale.clone()),
        App::update,
        App::view,
    )
    .title("MORIQ 墨屿")
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
    fn new(
        service: Service,
        launch: LaunchOptions,
        locale: std::sync::Arc<i18n::Locale>,
    ) -> (Self, Task<Message>) {
        let settings_service = service.clone();
        let settings = Task::perform(
            async move { settings_service.settings().await },
            Message::SettingsLoaded,
        );
        let resource_service = service.clone();
        let resources = Task::perform(
            async move {
                resource_service
                    .resources(assets::bundled())
                    .await
                    .map(|items| assets::Artwork {
                        images: items
                            .into_iter()
                            .map(|(key, pixels)| {
                                (
                                    key,
                                    image::Handle::from_rgba(
                                        pixels.width,
                                        pixels.height,
                                        pixels.rgba,
                                    ),
                                )
                            })
                            .collect(),
                    })
            },
            Message::ResourcesLoaded,
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
                manager: Default::default(),
                preferences: Default::default(),
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
                english: false,
                launch,
                capture_ticks: 0,
                page_task: None,
                preview_task: None,
                locale,
                artwork: Default::default(),
                masonry: None,
                masonry_task: None,
                masonry_mode: true,
                dialog: None,
                detail_tab: 0,
                model_tab: 0,
                preview_source: vec![],
                layout_generation: 0,
                editing_tags: false,
                on_demand_models: true,
                retain_indexes: true,
                model_search: String::new(),
                model_filter: 2,
            },
            Task::batch([open, resources, settings]),
        )
    }
    fn tr<'a>(&'a self, key: &'a str) -> &'a str {
        self.locale.text(self.english, key)
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
        if self.launch.design_preview {
            self.refresh_preview(range);
            self.loading = false;
            return Task::none();
        }
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
        if self.launch.design_preview {
            return Task::none();
        }
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
        self.clear_browsing();
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
            if self.transition.is_some() {
                iced::time::every(Duration::from_millis(16)).map(Message::TransitionFrame)
            } else {
                Subscription::none()
            },
            iced::window::close_requests().map(|_| Message::WindowClose),
            iced::window::resize_events().map(|(_, size)| Message::Resized(size)),
            iced::time::every(Duration::from_millis(250)).map(|_| Message::Tick),
            iced::event::listen_with(|event, status, _| match event {
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
                            | iced::keyboard::key::Named::F1
                    ) =>
                {
                    Some(Message::Shortcut(match key {
                        iced::keyboard::key::Named::Delete => 1,
                        iced::keyboard::key::Named::Enter => 2,
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
            event @ (Message::OpenImage(_) | Message::AssetContext(_)) => {
                if self.dialog.is_some()
                    || self.manager.panel.is_some()
                    || self.switching
                    || self.screen != Screen::Assets
                {
                    return Task::none();
                }
                let (id, menu) = match event {
                    Message::OpenImage(id) => (id, false),
                    Message::AssetContext(id) => (id, true),
                    _ => unreachable!(),
                };
                if !self
                    .tiles
                    .iter()
                    .chain(self.neighbors.iter())
                    .chain(self.preview_source.iter())
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
                    self.english = prefs.english;
                    self.preferences = prefs;
                }
                Err(e) => self.error = Some(e),
            },
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
                return Task::perform(async move { service.close().await }, Message::ShutdownReady);
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
            Message::RevealLibrary => {
                if self.launch.design_preview {
                    self.error =
                        Some("设计预览不对应实际资源库目录。请先创建或打开自己的资源库。".into());
                    return Task::none();
                }
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
            Message::PlatformDone(success, result) => match result {
                Ok(()) => self.status = success.into(),
                Err(error) => self.error = Some(error),
            },
            Message::ToggleTagEditor => self.editing_tags = !self.editing_tags,
            Message::ToggleModelPolicy => self.on_demand_models = !self.on_demand_models,
            Message::ToggleRetainIndexes => {
                if self.launch.design_preview {
                    self.retain_indexes = !self.retain_indexes;
                } else {
                    self.retain_indexes = true;
                    self.error = Some("标签、OCR 和索引属于资源库数据，卸载模型时会保留。".into());
                }
            }
            Message::ShowCreate => {
                self.name = "我的素材库".into();
                self.dialog = Some(Dialog::CreateLibrary);
            }
            Message::HideCreate | Message::HideProject => self.dialog = None,
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
                    if self.launch.design_preview {
                        library.info.name = self.name.clone();
                        self.dialog = None;
                        return Task::none();
                    }
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
                    if self.launch.design_preview {
                        asset.name = self.name.clone();
                        for tile in &mut self.preview_source {
                            if tile.asset.id == asset.id {
                                tile.asset.name = self.name.clone();
                            }
                        }
                        self.dialog = None;
                        return self.refresh();
                    }
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
            Message::ModelTab(tab) => {
                self.model_tab = tab;
                self.model_filter = if tab == 2 { 0 } else { 2 };
            }
            Message::ModelSearch(query) => self.model_search = query,
            Message::ModelFilter(filter) => self.model_filter = filter,
            Message::SetMasonry(value) => {
                self.manager.panel = None;
                self.masonry_mode = value;
                self.list = false;
                return self.reset_query();
            }
            Message::DesignPreview => {
                self.launch.design_preview = true;
                return self.install_preview();
            }
            Message::ResourcesLoaded(result) => match result {
                Ok(artwork) => {
                    self.artwork = artwork;
                    if self.launch.design_preview {
                        return self.install_preview();
                    }
                }
                Err(error) => self.error = Some(error),
            },
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
                return Task::perform(async move { service.open(path).await }, Message::Opened);
            }
            Message::PickedParent(None) | Message::PickedLibrary(None) => self.switching = false,
            Message::Opened(result) => {
                self.switching = false;
                match result {
                    Ok(library) => {
                        self.history = Default::default();
                        self.journal.reset();
                        self.storage.reset();
                        self.deletion.reset();
                        self.surfaces = Default::default();
                        self.launch.design_preview = false;
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
                        self.query = Query::default();
                        self.error = None;
                        self.status = self.tr("text_001").into();
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
                if self.launch.design_preview {
                    self.dialog = Some(Dialog::CreateLibrary);
                    return Task::none();
                }
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
                if self.launch.design_preview {
                    self.error = Some("请先创建或打开真实资源库，再导入图片".into());
                    return Task::none();
                }
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
                1 => self.preferences.import.memory_mib = value.parse().unwrap_or(1024),
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
            Message::Tick => {
                if let Some(import) = &self.import {
                    let finished = import.finished.load(Ordering::Relaxed);
                    if finished > self.manager.import_seen && !self.loading {
                        self.manager.import_seen = finished;
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
                if self.launch.capture.is_some()
                    && (!self.loading || self.import.is_some())
                    && !self.switching
                    && (!self.launch.design_preview || self.library.is_some())
                {
                    self.capture_ticks = self.capture_ticks.saturating_add(1);
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
                    if self.capture_ticks == 2 {
                        match self.launch.panel.as_deref() {
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
                            Some("expanded") => {
                                self.preview_expanded = true;
                            }
                            Some("compare") => return self.compare_message(CompareMessage::Open),
                            Some("help") => return self.manage_message(ManageMessage::Help),
                            _ => {}
                        }
                        self.manager.panel = match self.launch.panel.as_deref() {
                            Some("settings") => {
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
                        return iced::window::oldest()
                            .and_then(iced::window::screenshot)
                            .map(Message::Captured);
                    }
                }
            }
            Message::Captured(screenshot) => {
                if let Some(path) = self.launch.capture.take() {
                    let service = self.service.clone();
                    return Task::perform(
                        async move {
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
                        self.tiles = page
                            .tiles
                            .into_iter()
                            .map(|tile| Tile {
                                asset: tile.asset,
                                handle: tile
                                    .thumbnail
                                    .map(|p| image::Handle::from_rgba(p.width, p.height, p.rgba)),
                            })
                            .collect();
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
                    .chain(self.preview_source.iter())
                    .find(|t| t.asset.id == id)
                    .cloned();
                self.neighbor_position = None;
                self.neighbors.clear();
                let focus = tile
                    .map(|tile| self.focus_tile(tile))
                    .unwrap_or_else(Task::none);
                return Task::batch([focus, self.load_neighborhood(id, 0)]);
            }
            Message::PreviewLoaded(library, id, version, result)
                if self.library.as_ref().is_some_and(|l| l.info.id == library)
                    && self
                        .selected
                        .as_ref()
                        .is_some_and(|a| a.id == id && a.version_id == version) =>
            {
                match result {
                    Ok(handle) => self.preview = Some(handle),
                    Err(error) => self.error = Some(error),
                }
            }
            Message::FullPreviewLoaded(library, id, version, result)
                if self.library.as_ref().is_some_and(|l| l.info.id == library)
                    && self
                        .selected
                        .as_ref()
                        .is_some_and(|a| a.id == id && a.version_id == version) =>
            {
                match result {
                    Ok((preview, background)) => {
                        self.preview = Some(preview);
                        self.preview_background = Some(background);
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
                if self.launch.design_preview {
                    for tile in &mut self.preview_source {
                        if tile.asset.id == asset {
                            tile.asset.favorite = value;
                        }
                    }
                    if let Some(selected) = self.selected.as_mut().filter(|a| a.id == asset) {
                        selected.favorite = value;
                    }
                    return self.refresh();
                }
                if let Some(library) = &self.library {
                    let service = self.service.clone();
                    let id = library.info.id;
                    return self.mutation(async move { service.favorite(id, asset, value).await });
                }
            }
            Message::Rating(rating) => {
                if self.launch.design_preview {
                    if let Some(selected) = self.selected.as_mut() {
                        selected.rating = rating;
                    }
                    return Task::none();
                }
                if let (Some(library), Some(asset)) = (&self.library, &self.selected) {
                    let service = self.service.clone();
                    let id = library.info.id;
                    let asset = asset.id;
                    return self.mutation(async move { service.rating(id, asset, rating).await });
                }
            }
            Message::Tags(value) => self.tag_input = value,
            Message::SaveTags => {
                if self.launch.design_preview {
                    if let Some(selected) = self.selected.as_mut() {
                        selected.tags = self
                            .tag_input
                            .split([',', '，'])
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                    }
                    return Task::none();
                }
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
                        self.status = self.tr("text_002").into();
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
                if self.launch.design_preview {
                    self.error = Some("这是设计预览，请打开真实资源库后创建项目。".into());
                    return Task::none();
                }
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
            Message::Language => self.english = !self.english,
            _ => {}
        }
        Task::none()
    }
    fn view(&self) -> Element<'_, Message> {
        self.shell()
    }
}
