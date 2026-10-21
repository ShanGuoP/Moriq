use super::*;
use crate::components::choice as pick_list;
use crate::{components::*, icons::Icon, theme::*};
use iced::widget::{
    Column, Space, button, checkbox, container, mouse_area, row, stack, text, text_input,
};
use iced::{Alignment, Length};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Panel {
    Menu,
    Import,
    Filters,
    Organize,
    Collections,
    Project,
    Maintenance,
    Synco,
    Tags,
    ExportOptions,
    AssetMenu,
    AssetProjects,
    Versions,
    ViewOptions,
    Activity,
    Notifications,
    Location,
    Storage,
    Rules,
    Help,
    Licenses,
    Compare,
    Delete,
    Tasks,
    Similar,
    People,
    Metadata,
    Batch,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ProjectDestination {
    pub id: Option<ProjectId>,
    pub name: String,
}
impl std::fmt::Display for ProjectDestination {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}
#[derive(Default)]
pub(super) struct Manager {
    pub similar: Vec<SimilarAsset>,
    pub panel: Option<Panel>,
    pub selection: HashSet<AssetId>,
    pub multiselect: bool,
    pub selection_anchor: Option<AssetId>,
    pub organizer: Organizer,
    pub favorites: Vec<Tile>,
    pub project_covers: std::collections::HashMap<ProjectId, image::Handle>,
    pub category_covers: std::collections::HashMap<String, image::Handle>,
    pub details: AssetDetails,
    pub notes: String,
    pub notes_dirty: bool,
    pub tags_collapsed: bool,
    pub description_draft: Option<String>,
    pub cover: Option<image::Handle>,
    pub label: String,
    pub collection_name: String,
    pub tag_new_name: String,
    pub new_project_name: String,
    pub active_tag: String,
    pub date_start: String,
    pub date_end: String,
    pub export_options: ExportOptions,
    pub parent: Option<ProjectId>,
    pub project_target: Option<ProjectId>,
    pub latitude: String,
    pub longitude: String,
    pub location_label: String,
    pub filter: Query,
    pub rule_draft: Option<Rule>,
    pub rule_in_filter: bool,
    pub busy: bool,
    pub maintenance: Option<moriq_app::MaintenanceControl>,
    pub referenced_import: bool,
    pub import_seen: usize,
    pub import_refreshed: Option<std::time::Instant>,
    pub report: String,
    pub report_panel: Option<Panel>,
    pub synco_running: bool,
    pub synco_ticks: u16,
    pub synco_backoff: u16,
}
#[derive(Debug, Clone)]
pub enum ManageMessage {
    SyncoExecutable,
    SyncoExecutablePicked(Option<PathBuf>),
    SyncoAutoStart(bool),
    SyncoPreferencesSaved(Result<UserSettings, String>),
    SyncoRetry(String),
    Similar,
    SimilarLoaded(LibraryId, AssetId, Result<Vec<SimilarAsset>, String>),
    Tasks,
    Location,
    LocationInput(u8, String),
    SaveLocation,
    ClearLocation,
    LocationSaved(LibraryId, AssetId, AssetVersionId, Result<(), String>),
    OpenMap,
    Help,
    Licenses,
    AssetMenu,
    AssetProjects,
    Versions,
    ViewOptions,
    RevealAsset,
    Menu,
    ImportMenu,
    Synco,
    SendSynco,
    SyncoPrepared(LibraryId, Option<AssetId>, Result<(), String>),
    SyncoPoll(bool),
    SyncoDone(LibraryId, bool, Result<usize, String>),
    Version(AssetVersionId),
    VersionSelected(LibraryId, AssetId, AssetVersionId, Result<(), String>),
    Description(String),
    Cover,
    PickedCover(LibraryId, Option<PathBuf>),
    CoverLoaded(LibraryId, u64, Result<Option<image::Handle>, String>),
    Close,
    Filters,
    Organize,
    Collections,
    Project,
    ProjectMenu(ProjectId),

    ProjectDeleted(LibraryId, Vec<ProjectId>, Result<PurgeReport, String>),
    Maintenance,
    ToggleMulti,
    Toggle(AssetId),
    SelectVisible,
    RangeSelected(LibraryId, u64, Result<Vec<AssetId>, String>),
    ClearSelection,
    OrganizerLoaded(LibraryId, u64, Result<Organizer, String>),
    OverviewLoaded(LibraryId, u64, Result<moriq_app::LoadedOverview, String>),
    DetailsLoaded(
        LibraryId,
        AssetId,
        AssetVersionId,
        Result<AssetDetails, String>,
    ),
    Filter(u8, String),
    ApplyFilter,
    ClearFilters,
    Untagged(bool),
    Unassigned(bool),
    Notes(String),
    SaveNotes,
    NotesSaved(LibraryId, AssetId, String, Result<(), String>),
    Label(String),
    CollectionName(String),
    TagName(String),
    SaveCollection,
    UseCollection(String),
    DeleteCollection(String),
    Tag(String),
    Tags,
    ToggleTags,
    ActiveTag(String),
    RenameTag,
    DeleteTag,
    RelocateReference,
    PickedReference(LibraryId, AssetId, AssetVersionId, Option<PathBuf>),
    ExportMenu,
    ExportOption(u8, String),
    Date(u8, String),
    Builtin(u8),
    Edit(Edit),
    Edited(LibraryId, bool, Result<(), String>),
    RenameProject,
    Parent(Option<ProjectId>),
    NewChildProject,
    NewProjectName(String),
    MoveProject,
    DeleteProject,
    FolderImport,
    ReferenceImport,
    Export,
    Backup,
    PickedDestination(LibraryId, bool, Option<PathBuf>),
    Published(LibraryId, bool, Result<PathBuf, String>),
    Recover,
    Recovered(LibraryId, Result<ImportReport, String>),
    Check(bool),
    CancelCheck,
    Purge,

    Purged(LibraryId, Result<PurgeReport, String>),
    Checked(LibraryId, Result<MaintenanceReport, String>),
}
impl App {
    pub(super) fn refresh_organizer(&mut self) -> Task<Message> {
        let Some(library) = &self.library else {
            return Task::none();
        };
        if self.launch.design_preview {
            return Task::none();
        }
        let id = library.info.id;
        let service = self.service.clone();
        self.projection_request = self.projection_request.wrapping_add(1);
        let request = self.projection_request;
        let overview_service = service.clone();
        let cover_service = service.clone();
        let activity = if self.manager.panel == Some(Panel::Activity) {
            Task::none()
        } else {
            self.load_events(false, None)
        };
        let notices = if self.manager.panel == Some(Panel::Notifications) {
            Task::none()
        } else {
            self.load_events(true, None)
        };
        Task::batch([
            activity,
            notices,
            Task::perform(
                async move { overview_service.overview(id).await },
                move |r| Message::Manage(ManageMessage::OverviewLoaded(id, request, r)),
            ),
            Task::perform(async move { service.organizer(id).await }, move |r| {
                Message::Manage(ManageMessage::OrganizerLoaded(id, request, r))
            }),
            Task::perform(
                async move {
                    cover_service
                        .cover(id)
                        .await
                        .map(|p| p.map(|p| image::Handle::from_rgba(p.width, p.height, p.rgba)))
                },
                move |r| Message::Manage(ManageMessage::CoverLoaded(id, request, r)),
            ),
        ])
    }
    pub(super) fn refresh_details(&self) -> Task<Message> {
        let (Some(library), Some(asset)) = (&self.library, &self.selected) else {
            return Task::none();
        };
        if self.launch.design_preview {
            return Task::none();
        }
        let id = library.info.id;
        let version = asset.version_id;
        let asset = asset.id;
        let service = self.service.clone();
        let recommendations = self.service.clone();
        Task::batch([
            Task::perform(async move { service.details(id, asset).await }, move |r| {
                Message::Manage(ManageMessage::DetailsLoaded(id, asset, version, r))
            }),
            Task::perform(
                async move { recommendations.recommendations(id, asset, version).await },
                move |r| Message::Ai(crate::ai::AiMessage::Recommended(id, asset, version, r)),
            ),
        ])
    }
    pub(super) fn selected_ids(&self) -> Vec<AssetId> {
        if self.manager.multiselect {
            self.manager.selection.iter().copied().collect()
        } else {
            self.selected.iter().map(|a| a.id).collect()
        }
    }
    pub(super) fn manage_message(&mut self, event: ManageMessage) -> Task<Message> {
        use ManageMessage::*;
        if let PickedDestination(expected, _, _) | PickedReference(expected, _, _, _) = &event
            && self.library.as_ref().is_none_or(|l| l.info.id != *expected)
        {
            return Task::none();
        }
        match event {
            SyncoExecutable => {
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .set_title("选择 Synco 程序")
                            .add_filter("Synco 程序", &["exe"])
                            .pick_file()
                            .await
                            .map(|f| f.path().to_path_buf())
                    },
                    |r| Message::Manage(SyncoExecutablePicked(r)),
                );
            }
            SyncoExecutablePicked(Some(path)) => {
                self.preferences.synco.executable = Some(path);
                let value = self.preferences.synco.clone();
                let service = self.service.clone();
                return Task::perform(async move { service.synco_preferences(value).await }, |r| {
                    Message::Manage(SyncoPreferencesSaved(r))
                });
            }
            SyncoAutoStart(value) => {
                self.preferences.synco.auto_start = value;
                let value = self.preferences.synco.clone();
                let service = self.service.clone();
                return Task::perform(async move { service.synco_preferences(value).await }, |r| {
                    Message::Manage(SyncoPreferencesSaved(r))
                });
            }
            SyncoPreferencesSaved(result) => match result {
                Ok(prefs) => self.preferences = prefs,
                Err(e) => self.error = Some(e),
            },
            SyncoRetry(task) if !self.manager.synco_running => {
                if let Some(library) = &self.library {
                    let id = library.info.id;
                    let service = self.service.clone();
                    self.manager.synco_running = true;
                    return Task::perform(
                        async move { service.retry_synco(id, task).await },
                        move |r| Message::Manage(SyncoPrepared(id, None, r)),
                    );
                }
            }
            Tasks => {
                self.manager.panel = Some(Panel::Tasks);
                return self.refresh_organizer();
            }
            Help => self.manager.panel = Some(Panel::Help),
            Similar => {
                if let (Some(library), Some(asset)) = (&self.library, &self.selected) {
                    let (id, asset) = (library.info.id, asset.id);
                    let service = self.service.clone();
                    self.manager.similar.clear();
                    self.manager.panel = Some(Panel::Similar);
                    return Task::perform(
                        async move { service.similar_assets(id, asset).await },
                        move |r| Message::Manage(SimilarLoaded(id, asset, r)),
                    );
                }
            }
            SimilarLoaded(id, asset, result)
                if self.library.as_ref().is_some_and(|l| l.info.id == id)
                    && self.selected.as_ref().is_some_and(|a| a.id == asset)
                    && self.manager.panel == Some(Panel::Similar) =>
            {
                match result {
                    Ok(items) => self.manager.similar = items,
                    Err(e) => self.error = Some(e),
                }
            }
            Licenses => self.manager.panel = Some(Panel::Licenses),
            Location => {
                let location = self.manager.details.manual_location.as_ref().or(self
                    .manager
                    .details
                    .exif_location
                    .as_ref());
                self.manager.latitude =
                    location.map(|v| v.latitude.to_string()).unwrap_or_default();
                self.manager.longitude = location
                    .map(|v| v.longitude.to_string())
                    .unwrap_or_default();
                self.manager.location_label = location.map(|v| v.label.clone()).unwrap_or_default();
                self.manager.panel = Some(Panel::Location);
            }
            LocationInput(field, value) => match field {
                0 => self.manager.latitude = value,
                1 => self.manager.longitude = value,
                _ => self.manager.location_label = value,
            },
            SaveLocation | ClearLocation => {
                if let (Some(library), Some(asset)) = (&self.library, &self.selected) {
                    let value = if matches!(event, ClearLocation) {
                        None
                    } else {
                        let (Ok(latitude), Ok(longitude)) = (
                            self.manager.latitude.trim().parse(),
                            self.manager.longitude.trim().parse(),
                        ) else {
                            self.error = Some("经纬度应为十进制数字".into());
                            return Task::none();
                        };
                        let value = AssetLocation {
                            latitude,
                            longitude,
                            label: self.manager.location_label.trim().into(),
                        };
                        if !value.valid() {
                            self.error =
                                Some("纬度应为 -90～90，经度 -180～180，名称最多 256 字".into());
                            return Task::none();
                        }
                        Some(value)
                    };
                    let id = library.info.id;
                    let asset = asset.id;
                    let version = self.selected.as_ref().map(|a| a.version_id);
                    if let Some(version) = version {
                        let service = self.service.clone();
                        return Task::perform(
                            async move { service.set_location(id, asset, version, value).await },
                            move |r| Message::Manage(LocationSaved(id, asset, version, r)),
                        );
                    }
                }
            }
            LocationSaved(id, asset, version, result)
                if self.library.as_ref().is_some_and(|l| l.info.id == id) =>
            {
                match result {
                    Ok(()) => {
                        if self
                            .selected
                            .as_ref()
                            .is_some_and(|a| a.id == asset && a.version_id == version)
                            && self.manager.panel == Some(Panel::Location)
                        {
                            self.manager.panel = None;
                        }
                        return Task::batch([
                            self.refresh_details(),
                            self.refresh_info(),
                            self.refresh_organizer(),
                        ]);
                    }
                    Err(e) => self.error = Some(e),
                }
            }
            OpenMap => {
                if let (Some(library), Some(location)) = (
                    &self.library,
                    self.manager.details.manual_location.as_ref().or(self
                        .manager
                        .details
                        .exif_location
                        .as_ref()),
                ) {
                    let id = library.info.id;
                    let location = location.clone();
                    let service = self.service.clone();
                    return Task::perform(
                        async move { service.open_map(id, location).await },
                        |r| Message::PlatformDone("已请求在浏览器中打开地图", r),
                    );
                }
            }
            DeleteProject => {
                if let Some(project) = self.manager.project_target {
                    return self
                        .deletion_message(crate::deletion::DeletionMessage::Project(project));
                }
            }
            ProjectDeleted(id, projects, result)
                if self.library.as_ref().is_some_and(|l| l.info.id == id) =>
            {
                self.manager.busy = false;
                match result {
                    Ok(report) => {
                        self.manager.panel = None;
                        self.manager.project_target = None;
                        self.history = Default::default();
                        if self.query.project.is_some_and(|id| projects.contains(&id)) {
                            self.query = Query::default();
                        }
                        self.status = format!(
                            "已删除项目及 {} 张独占素材{}",
                            report.deleted,
                            if report.cleanup_pending {
                                "，文件清理将在重开后继续"
                            } else {
                                ""
                            }
                        );
                        return Task::batch([
                            self.reset_query(),
                            self.refresh_info(),
                            self.refresh_organizer(),
                        ]);
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            ToggleTags => self.manager.tags_collapsed = !self.manager.tags_collapsed,
            AssetMenu | AssetProjects | Versions if self.selected.is_some() => {
                // These menus belong to the detail image, not to the batch selection.
                self.manager.multiselect = false;
                self.manager.panel = Some(match event {
                    AssetProjects => Panel::AssetProjects,
                    Versions => Panel::Versions,
                    _ => Panel::AssetMenu,
                });
                return self.refresh_details();
            }
            ViewOptions => self.manager.panel = Some(Panel::ViewOptions),
            RevealAsset => {
                if let (Some(library), Some(asset)) = (&self.library, &self.selected) {
                    let library = library.info.id;
                    let asset = asset.id;
                    let service = self.service.clone();
                    return Task::perform(
                        async move { service.reveal_asset(library, asset).await },
                        |r| Message::PlatformDone("已请求在文件夹中显示所选图片", r),
                    );
                }
            }
            Purge if self.query.trash => {
                return self.deletion_message(crate::deletion::DeletionMessage::Assets(
                    self.selected_ids(),
                ));
            }
            Purged(id, result)
                if self
                    .library
                    .as_ref()
                    .is_some_and(|library| library.info.id == id) =>
            {
                self.manager.busy = false;
                match result {
                    Ok(report) => {
                        self.manager.selection.clear();
                        self.manager.selection_anchor = None;
                        self.selected = None;
                        self.preview = None;
                        self.manager.report = format!(
                            "已永久删除 {} 项。{}",
                            report.deleted,
                            if report.cleanup_pending {
                                "暂存文件清理未完成，下次打开资源库将继续。"
                            } else {
                                ""
                            }
                        );
                        self.status = self.manager.report.clone();
                        return Task::batch([
                            self.refresh(),
                            self.refresh_info(),
                            self.refresh_organizer(),
                            self.rebuild_masonry(),
                        ]);
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            Parent(parent) => self.manager.parent = parent,
            NewProjectName(value) => self.manager.new_project_name = value,
            NewChildProject | MoveProject => {
                if self.launch.design_preview {
                    return Task::none();
                }
                if let Some(library) = &self.library {
                    let id = library.info.id;
                    let parent = self.manager.parent;
                    let name = self.manager.new_project_name.clone();
                    let project = self.manager.project_target;
                    let service = self.service.clone();
                    if matches!(event, NewChildProject) {
                        return self.mutation(async move {
                            service.create_child_project(id, name, parent).await
                        });
                    }
                    if let Some(project) = project {
                        return self.mutation(async move {
                            service.move_project(id, project, parent).await
                        });
                    }
                }
            }
            Tags => {
                self.manager.panel = Some(Panel::Tags);
                return self.refresh_organizer();
            }
            ActiveTag(name) => {
                self.manager.tag_new_name = name.clone();
                self.manager.active_tag = name;
            }
            RenameTag | DeleteTag => {
                if let Some(library) = &self.library {
                    if self.launch.design_preview {
                        return Task::none();
                    }
                    let id = library.info.id;
                    let source = self.manager.active_tag.clone();
                    let target = self.manager.tag_new_name.clone();
                    let service = self.service.clone();
                    let delete = matches!(event, DeleteTag);
                    return self.mutation(async move {
                        if delete {
                            service.delete_tag(id, source).await
                        } else {
                            service.rename_tag(id, source, target).await
                        }
                    });
                }
            }
            RelocateReference => {
                if let (Some(library), Some(asset)) = (&self.library, &self.selected) {
                    let (id, asset, version) = (library.info.id, asset.id, asset.version_id);
                    return Task::perform(
                        async {
                            rfd::AsyncFileDialog::new()
                                .set_title("重新定位原片 · 内容必须与登记副本一致")
                                .pick_file()
                                .await
                                .map(|f| f.path().to_path_buf())
                        },
                        move |path| Message::Manage(PickedReference(id, asset, version, path)),
                    );
                }
            }
            PickedReference(id, asset, version, Some(path)) => {
                let service = self.service.clone();
                return self.mutation(async move {
                    service.relocate_reference(id, asset, version, path).await
                });
            }
            ExportMenu => {
                self.manager.export_options = Default::default();
                self.manager.panel = Some(Panel::ExportOptions);
            }
            ExportOption(field, value) => {
                let options = &mut self.manager.export_options;
                match field {
                    0 => {
                        options.format = match value.as_str() {
                            "JPEG" => ExportFormat::Jpeg,
                            "PNG" => ExportFormat::Png,
                            "WebP" => ExportFormat::Webp,
                            "TIFF" => ExportFormat::Tiff,
                            "BMP" => ExportFormat::Bmp,
                            _ => ExportFormat::Original,
                        }
                    }
                    1 => options.max_edge = value.parse().unwrap_or(0),
                    2 => options.quality = value.parse().unwrap_or(90),
                    3 => options.rotate = value.parse().unwrap_or(0),
                    4 => {
                        options.resampling = match value.as_str() {
                            "Lanczos3" => Resampling::Lanczos3,
                            "双三次" => Resampling::Bicubic,
                            "最近邻" => Resampling::Nearest,
                            _ => Resampling::Area,
                        }
                    }
                    5 => {
                        options.layout = match value.as_str() {
                            "按格式" => ExportLayout::Format,
                            "按导入日期" => ExportLayout::ImportDate,
                            _ => ExportLayout::Flat,
                        }
                    }
                    6 => options.metadata_sidecar = value == "附带整理信息 JSON",
                    7 => options.preserve_exif = value == "true",
                    8 => options.directory_template = value,
                    _ => {}
                }
            }
            Date(field, value) => {
                if field == 0 {
                    self.manager.date_start = value;
                } else {
                    self.manager.date_end = value;
                }
            }

            ImportMenu => self.manager.panel = Some(Panel::Import),
            Synco => {
                self.manager.panel = Some(Panel::Synco);
                return self.refresh_organizer();
            }
            SendSynco if !self.manager.synco_running && !self.launch.design_preview => {
                if let Some(l) = &self.library {
                    let id = l.info.id;
                    let assets = self.selected_ids();
                    let asset = if assets.len() == 1 {
                        assets.first().copied()
                    } else {
                        None
                    };
                    let project = self.query.project;
                    let service = self.service.clone();
                    self.manager.synco_running = true;
                    self.manager.panel = Some(Panel::Synco);
                    return Task::perform(
                        async move { service.prepare_synco(id, assets, project).await },
                        move |r| Message::Manage(SyncoPrepared(id, asset, r)),
                    );
                }
            }
            SyncoPrepared(id, asset, result)
                if self.library.as_ref().is_some_and(|l| l.info.id == id) =>
            {
                let notice = match &result {
                    Ok(()) => self.persist_asset_notice(
                        "已发送到 Synco",
                        "发送副本与任务已保存，可在任务面板查看。".into(),
                        0,
                        "",
                        asset,
                    ),
                    Err(e) => {
                        self.persist_asset_notice("Synco 发送失败", e.clone(), 2, "synco", asset)
                    }
                };
                self.manager.synco_running = false;
                if let Err(e) = result {
                    self.error = Some(e);
                }
                return Task::batch([
                    notice,
                    self.refresh_organizer(),
                    self.manage_message(SyncoPoll(true)),
                ]);
            }
            SyncoPoll(manual) if !self.manager.synco_running && !self.launch.design_preview => {
                if let Some(l) = &self.library {
                    let id = l.info.id;
                    let service = self.service.clone();
                    self.manager.synco_running = true;
                    self.manager.synco_ticks = 0;
                    return Task::perform(async move { service.synco_poll(id).await }, move |r| {
                        Message::Manage(SyncoDone(id, manual, r))
                    });
                }
            }
            SyncoDone(id, manual, result)
                if self.library.as_ref().is_some_and(|l| l.info.id == id) =>
            {
                self.manager.report_panel = Some(Panel::Synco);
                let notice = match &result {
                    Ok(count) if *count > 0 => self.persist_notice(
                        "Synco 任务更新",
                        format!("已领取并确认 {} 个任务结果", count),
                        0,
                        "",
                    ),
                    Err(e) if manual => {
                        self.persist_notice("Synco 检查失败", e.clone(), 2, "synco")
                    }
                    _ => Task::none(),
                };
                self.manager.synco_running = false;
                self.manager.synco_ticks = 0;
                match result {
                    Ok(count) => {
                        self.manager.synco_backoff = 8;
                        if manual || count > 0 {
                            self.manager.report =
                                format!("Synco 检查完成，已确认接收 {count} 个通知");
                        }
                    }
                    Err(e) => {
                        self.manager.synco_backoff =
                            (self.manager.synco_backoff.max(8) * 2).min(120);
                        self.manager.report = e;
                    }
                }
                return Task::batch([notice, self.refresh_organizer(), self.refresh_details()]);
            }
            Version(version) => {
                if let (Some(l), Some(a)) = (&self.library, &self.selected) {
                    let id = l.info.id;
                    let asset = a.id;
                    let service = self.service.clone();
                    return Task::perform(
                        async move { service.display_version(id, asset, version).await },
                        move |r| Message::Manage(VersionSelected(id, asset, version, r)),
                    );
                }
            }
            VersionSelected(id, asset, _version, result)
                if self.library.as_ref().is_some_and(|l| l.info.id == id) =>
            {
                if let Err(e) = result {
                    self.error = Some(e);
                } else {
                    if self.selected.as_ref().is_some_and(|a| a.id == asset) {
                        // Resolve fresh dimensions and metadata; never reselect an older image.
                        return Task::batch([self.load_neighborhood(asset, 0), self.refresh()]);
                    }
                    return self.refresh();
                }
            }
            Description(value) => self.manager.description_draft = Some(value),
            Cover => {
                if !self.launch.design_preview
                    && let Some(l) = &self.library
                {
                    let id = l.info.id;
                    return Task::perform(
                        async {
                            rfd::AsyncFileDialog::new()
                                .set_title("选择资源库封面")
                                .add_filter("图片", &["jpg", "jpeg", "png", "webp", "bmp"])
                                .pick_file()
                                .await
                                .map(|p| p.path().to_path_buf())
                        },
                        move |p| Message::Manage(PickedCover(id, p)),
                    );
                }
            }
            PickedCover(id, Some(path))
                if self.library.as_ref().is_some_and(|l| l.info.id == id) =>
            {
                let service = self.service.clone();
                return self.mutation(async move { service.set_cover(id, path).await });
            }
            CoverLoaded(id, request, Ok(handle))
                if request == self.projection_request
                    && self.library.as_ref().is_some_and(|l| l.info.id == id) =>
            {
                self.manager.cover = handle
            }
            CoverLoaded(id, request, Err(e))
                if request == self.projection_request
                    && self.library.as_ref().is_some_and(|l| l.info.id == id) =>
            {
                self.error = Some(e)
            }
            Recover if !self.manager.busy && !self.launch.design_preview => {
                if let Some(l) = &self.library {
                    let id = l.info.id;
                    let service = self.service.clone();
                    self.manager.busy = true;
                    return Task::perform(async move { service.recover(id).await }, move |r| {
                        Message::Manage(Recovered(id, r))
                    });
                }
            }
            Recovered(id, result) if self.library.as_ref().is_some_and(|l| l.info.id == id) => {
                self.manager.report_panel = Some(Panel::Maintenance);
                self.manager.busy = false;
                match result {
                    Ok(r) => {
                        self.manager.report = format!(
                            "已恢复 {}，跳过重复 {}，失败 {}。\n{}",
                            r.imported,
                            r.duplicates,
                            r.failed,
                            r.failures.join("\n")
                        );
                        return Task::batch([
                            self.refresh(),
                            self.refresh_info(),
                            self.refresh_organizer(),
                            self.rebuild_masonry(),
                        ]);
                    }
                    Err(e) => self.error = Some(e),
                }
            }
            Menu => self.manager.panel = Some(Panel::Menu),
            Close => {
                if self.manager.panel == Some(Panel::Delete) {
                    return self.deletion_message(crate::deletion::DeletionMessage::Cancel);
                }
                let activity = self.manager.panel == Some(Panel::Activity);
                let close = self.close_surface();
                if activity {
                    return Task::batch([close, self.load_events(false, None)]);
                }
                return close;
            }
            Filters => {
                self.manager.filter = self.query.clone();
                self.manager.date_start = String::new();
                self.manager.date_end = String::new();
                self.manager.panel = Some(Panel::Filters);
            }
            Organize => self.manager.panel = Some(Panel::Organize),
            Collections => {
                self.manager.panel = Some(Panel::Collections);
                return self.refresh_organizer();
            }
            Project | ProjectMenu(_) => {
                self.manager.project_target = match event {
                    ProjectMenu(id) => Some(id),
                    _ => self.query.project,
                };
                self.manager.parent = self
                    .library
                    .as_ref()
                    .and_then(|l| {
                        l.info
                            .projects
                            .iter()
                            .find(|p| Some(p.id) == self.manager.project_target)
                    })
                    .and_then(|p| p.parent);
                self.manager.label = self
                    .library
                    .as_ref()
                    .and_then(|l| {
                        l.info
                            .projects
                            .iter()
                            .find(|p| Some(p.id) == self.manager.project_target)
                    })
                    .map(|p| p.name.clone())
                    .unwrap_or_default();
                self.manager.panel = Some(Panel::Project);
            }
            Maintenance => {
                self.manager.panel = Some(Panel::Maintenance);
                return self.refresh_organizer();
            }
            ToggleMulti => {
                self.manager.panel = None;
                self.manager.multiselect = !self.manager.multiselect;
                self.manager.selection.clear();
            }
            RangeSelected(id, generation, result)
                if self.library.as_ref().is_some_and(|l| l.info.id == id)
                    && self.generation == generation
                    && self.manager.multiselect =>
            {
                match result {
                    Ok(ids) => self.manager.selection.extend(ids),
                    Err(error) => self.error = Some(error),
                }
            }
            Toggle(id) => {
                if self.modifiers.shift()
                    && !self.launch.design_preview
                    && let (Some(anchor), Some(library)) =
                        (self.manager.selection_anchor, &self.library)
                {
                    let library = library.info.id;
                    let generation = self.generation;
                    let query = self.query.clone();
                    let service = self.service.clone();
                    return Task::perform(
                        async move { service.selection_range(library, query, anchor, id).await },
                        move |result| Message::Manage(RangeSelected(library, generation, result)),
                    );
                }
                self.manager.selection_anchor = Some(id);
                if !self.manager.selection.remove(&id) {
                    self.manager.selection.insert(id);
                }
            }
            SelectVisible => {
                self.manager.multiselect = true;
                let range = self.visible_range();
                for (i, tile) in self.tiles.iter().enumerate() {
                    if range.contains(&(i + self.offset)) {
                        self.manager.selection.insert(tile.asset.id);
                    }
                }
            }
            ClearSelection => self.manager.selection.clear(),
            Label(value) => self.manager.label = value,
            CollectionName(value) => self.manager.collection_name = value,
            TagName(value) => self.manager.tag_new_name = value,
            Notes(value) => {
                self.manager.notes = value;
                self.manager.notes_dirty = true;
            }
            OverviewLoaded(id, request, Ok(result))
                if request == self.projection_request
                    && self.library.as_ref().is_some_and(|l| l.info.id == id) =>
            {
                self.manager.favorites = result
                    .favorites
                    .into_iter()
                    .map(|t| Tile {
                        asset: t.asset,
                        handle: t
                            .thumbnail
                            .map(|p| image::Handle::from_rgba(p.width, p.height, p.rgba)),
                    })
                    .collect();
                self.manager.category_covers = result
                    .categories
                    .into_iter()
                    .filter_map(|(label, pixels)| {
                        pixels.map(|pixels| {
                            (
                                label,
                                image::Handle::from_rgba(pixels.width, pixels.height, pixels.rgba),
                            )
                        })
                    })
                    .collect();
                self.manager.project_covers = result
                    .projects
                    .into_iter()
                    .filter_map(|(id, p)| {
                        p.map(|p| (id, image::Handle::from_rgba(p.width, p.height, p.rgba)))
                    })
                    .collect();
            }
            OverviewLoaded(id, request, Err(e))
                if request == self.projection_request
                    && self.library.as_ref().is_some_and(|l| l.info.id == id) =>
            {
                self.manager.category_covers.clear();
                self.error = Some(e)
            }
            OrganizerLoaded(id, request, Ok(result))
                if request == self.projection_request
                    && self.library.as_ref().is_some_and(|l| l.info.id == id) =>
            {
                self.manager.organizer = result;
                self.refresh_people_choices();
                return Task::batch([
                    self.folder_message(FolderMessage::Covers),
                    self.refresh_people_portraits(),
                ]);
            }
            DetailsLoaded(id, asset, version, Ok(result))
                if self.library.as_ref().is_some_and(|l| l.info.id == id)
                    && self
                        .selected
                        .as_ref()
                        .is_some_and(|a| a.id == asset && a.version_id == version) =>
            {
                if !self.manager.notes_dirty {
                    self.manager.notes = result.notes.clone();
                }
                if self.ai.category_version != Some(version) {
                    self.ai.category = result.category.clone();
                    self.ai.category_version = Some(version);
                }
                self.manager.details = result;
            }
            OrganizerLoaded(id, request, Err(e))
                if request == self.projection_request
                    && self.library.as_ref().is_some_and(|l| l.info.id == id) =>
            {
                self.error = Some(e)
            }
            DetailsLoaded(id, _, _, Err(e))
                if self.library.as_ref().is_some_and(|l| l.info.id == id) =>
            {
                self.error = Some(e)
            }
            Filter(field, value) => {
                let q = &mut self.manager.filter;
                match field {
                    0 => {
                        q.media_kind = if value == "RAW" {
                            MediaKind::Raw
                        } else if value == "视频" {
                            MediaKind::Video
                        } else {
                            MediaKind::All
                        };
                        q.format = if value == "全部类型" || value == "RAW" || value == "视频"
                        {
                            String::new()
                        } else {
                            value.to_lowercase()
                        }
                    }
                    1 => q.min_rating = value.parse().unwrap_or(0),
                    2 => {
                        q.since = chrono::Utc::now().timestamp_millis().max(0) as u64
                            - match value.as_str() {
                                "近 7 天" => 7 * 86400000,
                                "近 30 天" => 30 * 86400000,
                                _ => chrono::Utc::now().timestamp_millis().max(0) as u64,
                            }
                    }
                    3 => {
                        q.min_bytes = if value == "大于 10 MB" {
                            10_000_000
                        } else {
                            0
                        };
                        q.max_bytes = if value == "小于 1 MB" { 1_000_000 } else { 0 };
                    }
                    4 => {
                        q.orientation = match value.as_str() {
                            "横图" => 1,
                            "竖图" => 2,
                            "方图" => 3,
                            _ => 0,
                        }
                    }
                    5 => {
                        q.sort = match value.as_str() {
                            "最早导入" => SortOrder::Oldest,
                            "名称" => SortOrder::Name,
                            "文件最大" => SortOrder::Largest,
                            "评分最高" => SortOrder::Rating,
                            _ => SortOrder::Newest,
                        }
                    }
                    6 => q.min_width = value.parse().unwrap_or(0),
                    8 => {
                        q.storage = match value.as_str() {
                            "托管导入" => 1,
                            "外部引用" => 2,
                            _ => 0,
                        }
                    }
                    7 => {
                        q.color = if value == "全部颜色" {
                            String::new()
                        } else {
                            value
                        }
                    }
                    _ => {}
                }
            }
            Untagged(v) => self.manager.filter.untagged = v,
            Unassigned(v) => self.manager.filter.unassigned = v,
            ApplyFilter => {
                use chrono::TimeZone;
                let parse = |value: &str, end: bool| -> std::result::Result<u64, String> {
                    let date = chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
                        .map_err(|_| "日期格式应为 YYYY-MM-DD".to_string())?;
                    let date = if end {
                        date.succ_opt().ok_or("日期超出范围")?
                    } else {
                        date
                    };
                    let time = date.and_hms_opt(0, 0, 0).ok_or("日期无效")?;
                    let utc = chrono::Local
                        .from_local_datetime(&time)
                        .single()
                        .ok_or("当地日期存在时区歧义，请选择其他日期")?
                        .timestamp_millis();
                    u64::try_from(utc).map_err(|_| "日期不能早于 1970 年".into())
                };
                for (value, end) in [
                    (&self.manager.date_start, false),
                    (&self.manager.date_end, true),
                ] {
                    if !value.trim().is_empty() {
                        match parse(value.trim(), end) {
                            Ok(at) => {
                                if end {
                                    self.manager.filter.before = at
                                } else {
                                    self.manager.filter.since = at
                                }
                            }
                            Err(e) => {
                                self.error = Some(e);
                                return Task::none();
                            }
                        }
                    }
                }
                if self.manager.filter.before > 0
                    && self.manager.filter.before <= self.manager.filter.since
                {
                    self.error = Some("结束日期应晚于或等于开始日期".into());
                    return Task::none();
                }
                self.query = self.manager.filter.clone();
                self.manager.panel = None;
                self.screen = Screen::Assets;
                return self.reset_query();
            }
            ClearFilters => {
                self.query = Query::default();
                self.manager.panel = None;
                return self.reset_query();
            }
            Builtin(which) => {
                self.query = Query::default();
                self.screen = Screen::Assets;
                self.manager.panel = None;
                match which {
                    0 => {
                        self.query.since = (chrono::Utc::now().timestamp_millis().max(0) as u64)
                            .saturating_sub(7 * 86400000)
                    }
                    1 => self.query.untagged = true,
                    2 => self.query.min_rating = 4,
                    3 => self.query.trash = true,
                    4 => self.query.unassigned = true,
                    6 => self.query.media_kind = MediaKind::Raw,
                    7 => self.query.media_kind = MediaKind::Video,
                    5 => {
                        self.query.edited_since = (chrono::Utc::now().timestamp_millis().max(0)
                            as u64)
                            .saturating_sub(7 * 86400000)
                    }
                    _ => {}
                }
                return self.reset_query();
            }
            Tag(tag) => {
                self.query = Query {
                    tag,
                    ..Default::default()
                };
                self.screen = Screen::Assets;
                self.manager.panel = None;
                return self.reset_query();
            }
            UseCollection(id) => {
                if let Some(c) = self
                    .manager
                    .organizer
                    .collections
                    .iter()
                    .find(|c| c.id == id)
                {
                    self.query = c.query.clone();
                    self.screen = Screen::Assets;
                    self.manager.panel = None;
                    return self.reset_query();
                }
            }
            ReferenceImport
                if self.library.is_some()
                    && self.import.is_none()
                    && !self.switching
                    && !self.manager.busy =>
            {
                self.manager.referenced_import = true;
                self.manager.busy = true;
                self.manager.panel = None;
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .set_title("引用外部原片 · 请保留源文件及磁盘位置")
                            .add_filter("媒体", moriq_app::IMPORT_EXTENSIONS)
                            .pick_files()
                            .await
                            .map(|files| {
                                files.into_iter().map(|f| f.path().to_path_buf()).collect()
                            })
                    },
                    Message::PickedImages,
                );
            }
            FolderImport if self.import.is_none() && !self.manager.busy => {
                self.manager.referenced_import = false;
                self.manager.busy = true;
                self.manager.panel = None;
                return Task::perform(
                    async {
                        rfd::AsyncFileDialog::new()
                            .set_title("选择要导入的图片文件夹")
                            .pick_folder()
                            .await
                            .map(|f| vec![f.path().to_path_buf()])
                    },
                    Message::PickedImages,
                );
            }
            Published(id, backup, result)
                if self.library.as_ref().is_some_and(|l| l.info.id == id) =>
            {
                self.manager.report_panel = Some(if backup {
                    Panel::Maintenance
                } else {
                    Panel::ExportOptions
                });
                let notice = match &result {
                    Ok(path) => self.persist_notice("文件输出完成", display_path(path), 0, ""),
                    Err(e) => self.persist_notice("文件输出失败", e.clone(), 2, ""),
                };
                self.manager.busy = false;
                match result {
                    Ok(path) => self.manager.report = format!("已完成：{}", display_path(&path)),
                    Err(e) => self.error = Some(e),
                }
                return notice;
            }
            Checked(id, result) if self.library.as_ref().is_some_and(|l| l.info.id == id) => {
                self.manager.report_panel = Some(Panel::Maintenance);
                let notice = match &result {
                    Ok(r) => self.persist_notice(
                        "资源库检查完成",
                        format!(
                            "检查 {} 个版本，缺失 {}，损坏 {}，修复 {}，{}",
                            r.checked,
                            r.missing,
                            r.damaged,
                            r.repaired,
                            if r.cancelled {
                                "已停止"
                            } else {
                                "已完成"
                            }
                        ),
                        if r.missing + r.damaged > 0 { 1 } else { 0 },
                        "maintenance",
                    ),
                    Err(e) => self.persist_notice("资源库检查失败", e.clone(), 2, "maintenance"),
                };
                self.manager.busy = false;
                self.manager.maintenance = None;
                match result {
                    Ok(r) => {
                        self.manager.report = format!(
                            "{}检查 {} 个版本；缺失 {}，校验不符 {}，修复缓存 {}。\n{}",
                            if r.cancelled {
                                "已停止，已"
                            } else {
                                "已完成，已"
                            },
                            r.checked,
                            r.missing,
                            r.damaged,
                            r.repaired,
                            r.warnings.join("\n")
                        );
                        self.status = format!(
                            "资源库检查{}：{} 个版本，修复 {} 项缓存",
                            if r.cancelled {
                                "已停止"
                            } else {
                                "已完成"
                            },
                            r.checked,
                            r.repaired
                        );
                        if r.repaired > 0 {
                            return Task::batch([
                                notice,
                                self.refresh(),
                                self.refresh_info(),
                                self.refresh_organizer(),
                                self.rebuild_masonry(),
                            ]);
                        }
                    }
                    Err(e) => self.error = Some(e),
                }
                return notice;
            }
            CancelCheck => {
                if let Some(control) = &self.manager.maintenance {
                    control.cancel.store(true, Ordering::Relaxed);
                }
            }
            Edit(edit) => {
                if self.launch.design_preview {
                    self.error = Some("请打开真实资源库后整理素材".into());
                    return Task::none();
                }
                let ids = self.selected_ids();
                let Some(library) = &self.library else {
                    return Task::none();
                };
                let id = library.info.id;
                let service = self.service.clone();
                let trash = matches!(edit, moriq_domain::Edit::Trash(_));
                return Task::perform(async move { service.edit(id, ids, edit).await }, move |r| {
                    Message::Manage(Edited(id, trash, r))
                });
            }
            Edited(id, trash, result) if self.library.as_ref().is_some_and(|l| l.info.id == id) => {
                if trash && result.is_ok() {
                    self.clear_browsing();
                    self.manager.panel = None;
                    self.manager.selection.clear();
                    self.selected = None;
                    self.preview = None;
                }
                return self.update(Message::Saved(id, result));
            }
            SaveNotes => {
                if !self.launch.design_preview
                    && let (Some(l), Some(a)) = (&self.library, &self.selected)
                {
                    let id = l.info.id;
                    let asset = a.id;
                    let value = self.manager.notes.clone();
                    let sent = value.clone();
                    let service = self.service.clone();
                    return Task::perform(
                        async move {
                            service
                                .edit(id, vec![asset], moriq_domain::Edit::Notes(value))
                                .await
                        },
                        move |r| Message::Manage(NotesSaved(id, asset, sent.clone(), r)),
                    );
                }
            }
            NotesSaved(id, asset, value, result)
                if self.library.as_ref().is_some_and(|l| l.info.id == id) =>
            {
                if result.is_ok()
                    && self.selected.as_ref().is_some_and(|a| a.id == asset)
                    && self.manager.notes == value
                {
                    self.manager.notes_dirty = false;
                }
                return self.update(Message::Saved(id, result));
            }

            action @ (SaveCollection
            | DeleteCollection(_)
            | RenameProject
            | Export
            | Backup
            | PickedDestination(..)
            | Check(_)) => {
                if self.launch.design_preview {
                    self.error = Some("请打开真实资源库后使用此功能".into());
                    return Task::none();
                }
                let Some(library) = &self.library else {
                    return Task::none();
                };
                let id = library.info.id;
                let service = self.service.clone();
                let label = if matches!(action, SaveCollection) {
                    self.manager.collection_name.clone()
                } else {
                    self.manager.label.clone()
                };
                match action {
                    SaveCollection => {
                        let query = self.query.clone();
                        return self
                            .mutation(async move { service.collection(id, label, query).await });
                    }
                    DeleteCollection(collection) => {
                        return self.mutation(async move {
                            service.delete_collection(id, collection).await
                        });
                    }
                    RenameProject => {
                        if let Some(project) = self.manager.project_target {
                            return self.mutation(async move {
                                service.rename_project(id, project, label).await
                            });
                        }
                    }
                    Export | Backup => {
                        let backup = matches!(action, Backup);
                        if !self.manager.busy {
                            return Task::perform(
                                async move {
                                    rfd::AsyncFileDialog::new()
                                        .set_title(if backup {
                                            "选择库外备份位置"
                                        } else {
                                            "选择导出位置"
                                        })
                                        .pick_folder()
                                        .await
                                        .map(|f| f.path().to_path_buf())
                                },
                                move |p| Message::Manage(PickedDestination(id, backup, p)),
                            );
                        }
                    }
                    PickedDestination(_, backup, Some(parent)) if !self.manager.busy => {
                        self.manager.busy = true;
                        let ids = self.selected_ids();
                        let options = self.manager.export_options.clone();
                        return Task::perform(
                            async move {
                                if backup {
                                    service.backup(id, parent).await
                                } else {
                                    service.export_options(id, ids, parent, options).await
                                }
                            },
                            move |r| Message::Manage(Published(id, backup, r)),
                        );
                    }
                    Check(repair) if !self.manager.busy => {
                        self.manager.busy = true;
                        self.manager.report.clear();
                        let control = moriq_app::MaintenanceControl::default();
                        self.manager.maintenance = Some(control.clone());
                        return Task::perform(
                            async move { service.maintenance_controlled(id, repair, control).await },
                            move |r| Message::Manage(Checked(id, r)),
                        );
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        Task::none()
    }
    pub(super) fn management_overlay<'a>(
        &'a self,
        base: Element<'a, Message>,
    ) -> Element<'a, Message> {
        if self.compact_panel() {
            return self.popup(base);
        }
        let mut content = Column::new().spacing(18);
        for (section, _) in &self.manager.organizer.errors {
            let (panel, label) = match section {
                OrganizerSection::Folders => (Panel::Collections, "分组"),
                OrganizerSection::Jobs => (Panel::Tasks, "后台任务"),
                OrganizerSection::People => (Panel::People, "人物"),
                OrganizerSection::Categories => (Panel::Menu, "分类"),
                OrganizerSection::Tags => (Panel::Tags, "标签"),
                OrganizerSection::Collections => (Panel::Collections, "智能集合"),
                OrganizerSection::Statistics => (Panel::Maintenance, "统计"),
                OrganizerSection::Synco => (Panel::Synco, "Synco 交接"),
                OrganizerSection::Profile => (Panel::Maintenance, "资源库信息"),
            };
            if self.manager.panel == Some(panel) || self.manager.panel == Some(Panel::Menu) {
                content = content.push(muted(format!(
                    "{label}暂时无法读取。请重新打开资源库重试，诊断信息已写入日志。"
                )));
            }
        }
        let title = match self.manager.panel {
            Some(Panel::Activity) => "活动记录",
            Some(Panel::Notifications) => "通知中心",
            Some(Panel::Location) => "编辑地点",
            Some(Panel::Storage) => "资源库空间明细",
            Some(Panel::Rules) => "组合规则",
            Some(Panel::Help) => "搜索与快捷键",
            Some(Panel::Licenses) => "开源组件声明",
            Some(Panel::Compare) => "版本并排对比",
            Some(Panel::Tasks) => "后台任务",
            Some(Panel::Delete) => {
                if self.deletion.final_step {
                    "最后确认：永久删除"
                } else {
                    "确认删除"
                }
            }
            Some(Panel::Menu) => "资源库工具",
            Some(Panel::Similar) => "视觉近似图片",
            Some(Panel::People) => "人物管理与人工纠正",
            Some(Panel::Metadata) => "EXIF 与 GPS 检索",
            Some(Panel::Batch) => "批量编辑多个字段",
            Some(Panel::AssetMenu) => "素材操作",
            Some(Panel::AssetProjects) => "素材所属项目",
            Some(Panel::Versions) => "素材版本",
            Some(Panel::ViewOptions) => "显示方式",
            Some(Panel::Import) => "导入素材",
            Some(Panel::Filters) => "筛选与排序",
            Some(Panel::Organize) => "整理选中的素材",
            Some(Panel::Tags) => "标签管理",
            Some(Panel::ExportOptions) => "导出素材",
            Some(Panel::Collections) => "标签与智能集合",
            Some(Panel::Project) => "项目管理",
            Some(Panel::Synco) => "Synco 编辑与版本交接",
            _ => "备份与完整性检查",
        };
        let back: Element<'_, Message> = if self.surface_has_parent() {
            icon_button(Icon::Left, Message::Back)
                .on_press_maybe(self.can_go_back().then_some(Message::Back))
                .into()
        } else {
            Space::new().width(0).into()
        };
        let (kind, description) = match self.manager.panel {
            Some(Panel::Collections) => (Icon::Tag, "管理你的整理方式，让素材更有条理。"),
            Some(Panel::People) => (
                Icon::Person,
                "管理已识别的人物，支持修改名称、合并人物或手动纠正识别结果。",
            ),
            Some(Panel::Tags) => (
                Icon::Tag,
                "管理标签名称与素材归类，支持重命名、合并或删除。",
            ),
            Some(Panel::Filters | Panel::Metadata | Panel::Rules) => {
                (Icon::Filter, "组合搜索条件，快速找到需要的素材。")
            }
            Some(Panel::Project | Panel::AssetProjects) => {
                (Icon::Folder, "管理素材所属项目与项目层级。")
            }
            Some(Panel::Tasks) => (Icon::Clock, "查看处理进度，管理后台任务。"),
            Some(Panel::ExportOptions) => (Icon::Download, "选择输出格式、尺寸与整理方式。"),
            Some(Panel::Activity) => (Icon::Clock, "查看资源库内的操作与处理记录。"),
            Some(Panel::Notifications) => (Icon::Bell, "查看任务结果与需要关注的消息。"),
            Some(Panel::Location) => (Icon::Pin, "查看和编辑素材的地理位置。"),
            Some(Panel::Storage | Panel::Maintenance) => {
                (Icon::Database, "管理本地资源、备份与资源库完整性。")
            }
            Some(Panel::Delete) => (Icon::Trash, "核对本次操作的范围与影响后再继续。"),
            Some(Panel::Versions | Panel::Compare) => {
                (Icon::Image, "查看素材版本，保留原片与编辑记录。")
            }
            Some(Panel::Synco) => (Icon::Network, "发送素材副本，并接收编辑后的新版本。"),
            Some(Panel::Similar) => (Icon::Search, "查看与当前素材接近的图片。"),
            Some(Panel::Batch | Panel::Organize) => {
                (Icon::Edit, "统一整理所选素材的标签、评分与其他信息。")
            }
            Some(Panel::Licenses) => (Icon::Package, "查看第三方组件与模型的授权说明。"),
            _ => (Icon::Settings, "管理资源库与应用工具。"),
        };
        let header = row![
            back,
            form_icon(kind, true),
            iced::widget::column![heading(title, 22), muted(description)]
                .spacing(7)
                .width(Length::Fill),
            icon_button(Icon::Close, Message::Manage(ManageMessage::Close)).on_press_maybe(
                (self.manager.panel != Some(Panel::Delete) || !self.manager.busy)
                    .then_some(Message::Manage(ManageMessage::Close))
            )
        ]
        .spacing(14)
        .align_y(Alignment::Center);
        let action = |label: &str, event| {
            let selected = if self.manager.multiselect {
                !self.manager.selection.is_empty()
            } else {
                self.selected.is_some()
            };
            let available = match &event {
                ManageMessage::CancelCheck => true,
                ManageMessage::SaveCollection => {
                    !self.manager.busy && !self.manager.collection_name.trim().is_empty()
                }
                ManageMessage::Edit(_)
                | ManageMessage::Export
                | ManageMessage::ExportMenu
                | ManageMessage::Purge => !self.manager.busy && selected,
                ManageMessage::SendSynco => {
                    !self.manager.busy && !self.manager.synco_running && selected
                }
                ManageMessage::SaveNotes => {
                    !self.manager.busy && self.selected.is_some() && self.manager.notes_dirty
                }
                ManageMessage::RenameTag => {
                    !self.manager.busy
                        && !self.manager.active_tag.is_empty()
                        && !self.manager.tag_new_name.trim().is_empty()
                }
                ManageMessage::DeleteTag => {
                    !self.manager.busy && !self.manager.active_tag.is_empty()
                }
                ManageMessage::SyncoPoll(_) => !self.manager.synco_running,
                _ => !self.manager.busy,
            };
            button(text(label.to_string()).size(13))
                .padding([8, 12])
                .style(secondary)
                .on_press_maybe(available.then_some(Message::Manage(event)))
        };
        use ManageMessage::*;
        match self.manager.panel {
            Some(Panel::People) => content = content.push(self.people_panel()),
            Some(Panel::Metadata) => content = content.push(self.metadata_panel()),
            Some(Panel::Batch) => content = content.push(self.batch_panel()),
            Some(Panel::Tasks) => content = content.push(self.tasks_panel()),
            Some(Panel::Delete) => content = content.push(self.deletion_panel()),
            Some(Panel::Compare) => content = content.push(self.compare_panel()),
            Some(Panel::Licenses) => {
                content=content.push(muted("发行包的 licenses 文件夹内提供第三方组件说明、逐项依赖清单与许可文本。模型权重的授权声明以各模型原始来源为准。"))
                    .push(text(include_str!("../../../THIRD_PARTY_NOTICES.md")).size(12))
                    .push(text(include_str!("../../../assets/licenses/ONNX-Runtime-MIT.txt")).size(12))
                    .push(text(include_str!("../../../assets/licenses/file-id-MIT.txt")).size(12));
                for model in &self.ai.models {
                    content = content.push(
                        text(format!(
                            "{} · {}\n{}",
                            model.spec.name, model.spec.license, model.spec.source
                        ))
                        .size(12),
                    );
                }
            }
            Some(Panel::Help) => {
                for line in [
                    "Ctrl+K：定位全局搜索。搜索文件名、标签和备注，特殊符号按字面匹配。",
                    "精确标签、RAW、日期与评分请在筛选中选择；复杂条件使用组合规则。",
                    "Alt+← / →：后退与前进。← / →：按当前排序浏览相邻素材。",
                    "Ctrl+A：选择当前已加载的素材（最多 128 张）。Shift+单击：范围选择。",
                    "Ctrl+Shift+A：对全部查询结果进行批量编辑。B：打开多字段批量编辑。",
                    "0～5：给选中范围评分。F：切换收藏。Ctrl+E：导出所选素材。",
                    "Enter / 空格：放大选中图片。Delete：将选中范围移入回收站。",
                    "Esc：先关闭放大查看，再关闭面板或详情。编辑文字时不触发选择、删除或相邻浏览。",
                    "AI 分析需先在模型管理中安装模型，再建立索引；自然语言搜索使用语义搜索入口。",
                ] {
                    content = content.push(muted(line));
                }
            }
            Some(Panel::Similar) => {
                content=content.push(muted("根据画面指纹、比例和颜色查找尺寸/压缩变体，最多显示100项；结果供人工判断，不自动合并或删除。"))
                    .push(crate::components::action("建立/更新全库近似索引",Message::Jobs(crate::tasks::JobMessage::Queue("fingerprints".into()))));
                if self.manager.similar.is_empty() {
                    content = content.push(muted(
                        "没有找到已索引的近似图片。旧图片可先建立索引，再重新查询。",
                    ));
                }
                for item in &self.manager.similar {
                    content = content.push(crate::components::action(
                        &format!("{} · 差异 {}", item.name, item.distance),
                        Message::Journal(crate::journal::JournalMessage::OpenAsset(item.id)),
                    ));
                }
            }
            Some(Panel::Storage) => content = content.push(self.storage_panel()),
            Some(Panel::Rules) => content = content.push(self.rules_panel()),
            Some(Panel::Location) => {
                content = content.push(muted(
                    "补充地点仅保存在当前图片版本中，原片 EXIF 保持不变。清除后重新显示原片 GPS。",
                ));
                for (field, label, value) in [
                    (0, "纬度（-90～90）", &self.manager.latitude),
                    (1, "经度（-180～180）", &self.manager.longitude),
                    (2, "地点名称（可选）", &self.manager.location_label),
                ] {
                    content = content.push(
                        text_input(label, value)
                            .padding(10)
                            .style(input)
                            .on_input(move |s| Message::Manage(LocationInput(field, s))),
                    );
                }
                content = content
                    .push(
                        row![
                            action("保存补充地点", SaveLocation),
                            action("清除补充地点", ClearLocation)
                        ]
                        .spacing(10),
                    )
                    .push(muted(
                        "仅在点击“打开地图”时将坐标交给浏览器；不会自动联网查询地名。",
                    ));
            }
            Some(Panel::Activity) => content = content.push(self.event_list(false)),
            Some(Panel::Notifications) => content = content.push(self.event_list(true)),
            Some(Panel::AssetProjects) => {
                if let Some(asset) = &self.selected {
                    content = content.push(heading(&asset.name, 15));
                    if self.manager.details.projects.is_empty() {
                        content = content.push(muted("这张素材尚未加入项目"));
                    }
                    if let Some(library) = &self.library {
                        for project in &library.info.projects {
                            let belongs = self.manager.details.projects.contains(&project.id);
                            content = content.push(
                                row![
                                    crate::components::action(
                                        &project_label(&library.info.projects, project.id),
                                        Message::Project(project.id)
                                    ),
                                    Space::new().width(Length::Fill),
                                    action(
                                        if belongs { "移出" } else { "加入" },
                                        Edit(moriq_domain::Edit::Project(project.id, !belongs))
                                    )
                                ]
                                .align_y(Alignment::Center),
                            );
                        }
                    }
                }
            }
            Some(Panel::Versions) => {
                content = content.push(
                    button("并排对比版本…")
                        .style(secondary)
                        .on_press(Message::Compare(crate::compare::CompareMessage::Open)),
                );
                if let Some(asset) = &self.selected {
                    content = content.push(heading(&asset.name, 15));
                    for (index, version) in self.manager.details.versions.iter().enumerate() {
                        content = content.push(action(
                            &format!(
                                "{}V{} · {}",
                                if version.id == asset.version_id {
                                    "✓ "
                                } else {
                                    ""
                                },
                                index + 1,
                                version.origin
                            ),
                            Version(version.id),
                        ));
                    }
                    content = content.push(muted("切换展示版本会保留原片与全部精修版本。"));
                }
            }
            Some(Panel::Synco) => {
                content=content.push(checkbox(self.preferences.synco.auto_start).label("发送时自动启动 Synco").on_toggle(|v|Message::Manage(SyncoAutoStart(v))))
                    .push(action("选择 Synco 程序",SyncoExecutable))
                    .push(muted(self.preferences.synco.executable.as_ref().map(|p|p.display().to_string()).unwrap_or_else(||"自动查找安装位置".into())))
                    .push(muted("需要 Synco 0.4.1 或更新版本。每张图片使用独立任务；精修结果保留为新版本，完成交接后自动回收发送副本。"))
                    .push(row![action("发送选中图片",SendSynco),action("立即检查 / 重试连接",SyncoPoll(true))].spacing(8));
                if self.manager.synco_running {
                    content = content.push(muted("正在与 Synco 交接…"));
                }
                for task in &self.manager.organizer.synco {
                    content = content
                        .push(separator())
                        .push(heading(&task.name, 13))
                        .push(muted(format!("{}  ·  {}", task.state, task.id)));
                    if !task.error.is_empty() {
                        content = content.push(text(&task.error).size(12));
                    }
                    if matches!(task.state.as_str(), "failed" | "cancelled") {
                        content = content.push(action(
                            "使用原交接版本创建新任务重试",
                            SyncoRetry(task.id.clone()),
                        ));
                    }
                }
            }
            Some(Panel::Filters) => {
                content = content.push(crate::components::action(
                    "EXIF 与 GPS 高级检索",
                    Message::Metadata(MetadataMessage::Open),
                ));
                let q = &self.manager.filter;
                for (field, label, values, selected) in [
                    (
                        0,
                        "格式",
                        vec![
                            "全部类型",
                            "JPG",
                            "PNG",
                            "WEBP",
                            "GIF",
                            "BMP",
                            "TIFF",
                            "HEIC",
                            "HEIF",
                            "AVIF",
                            "RAW",
                            "视频",
                            "MP4",
                            "MOV",
                            "MKV",
                            "WEBM",
                            "AVI",
                        ],
                        if q.media_kind == MediaKind::Raw {
                            "RAW"
                        } else if q.media_kind == MediaKind::Video {
                            "视频"
                        } else if q.format.is_empty() {
                            "全部类型"
                        } else {
                            match q.format.as_str() {
                                "jpg" => "JPG",
                                "png" => "PNG",
                                "webp" => "WEBP",
                                "gif" => "GIF",
                                "tiff" => "TIFF",
                                "heic" => "HEIC",
                                "heif" => "HEIF",
                                "avif" => "AVIF",
                                "mp4" => "MP4",
                                "mov" => "MOV",
                                "mkv" => "MKV",
                                "webm" => "WEBM",
                                "avi" => "AVI",
                                _ => "BMP",
                            }
                        },
                    ),
                    (
                        1,
                        "最低评分",
                        vec!["0", "1", "2", "3", "4", "5"],
                        match q.min_rating {
                            1 => "1",
                            2 => "2",
                            3 => "3",
                            4 => "4",
                            5 => "5",
                            _ => "0",
                        },
                    ),
                    (
                        2,
                        "导入时间",
                        vec!["全部时间", "近 7 天", "近 30 天"],
                        if q.since == 0 {
                            "全部时间"
                        } else {
                            "近 7 天"
                        },
                    ),
                    (
                        3,
                        "文件大小",
                        vec!["全部大小", "小于 1 MB", "大于 10 MB"],
                        if q.min_bytes > 0 {
                            "大于 10 MB"
                        } else if q.max_bytes > 0 {
                            "小于 1 MB"
                        } else {
                            "全部大小"
                        },
                    ),
                    (
                        4,
                        "图片比例",
                        vec!["全部比例", "横图", "竖图", "方图"],
                        match q.orientation {
                            1 => "横图",
                            2 => "竖图",
                            3 => "方图",
                            _ => "全部比例",
                        },
                    ),
                    (
                        5,
                        "排序",
                        vec!["最新导入", "最早导入", "名称", "文件最大", "评分最高"],
                        match q.sort {
                            SortOrder::Newest => "最新导入",
                            SortOrder::Oldest => "最早导入",
                            SortOrder::Name => "名称",
                            SortOrder::Largest => "文件最大",
                            SortOrder::Rating => "评分最高",
                        },
                    ),
                ] {
                    content = content.push(
                        row![
                            text(label).size(13).width(100),
                            pick_list(values, Some(selected), move |v| Message::Manage(Filter(
                                field,
                                v.into()
                            )))
                            .width(Length::Fill)
                        ]
                        .align_y(Alignment::Center),
                    );
                }
                content = content
                    .push(row![
                        text("最低宽度 / 像素").size(13).width(140),
                        pick_list(
                            vec!["0", "1920", "3840", "7680"],
                            Some(match q.min_width {
                                1920 => "1920",
                                3840 => "3840",
                                7680 => "7680",
                                _ => "0",
                            }),
                            |v| Message::Manage(Filter(6, v.into()))
                        )
                        .width(Length::Fill)
                    ])
                    .push(row![
                        text("主色调").size(13).width(140),
                        pick_list(
                            vec![
                                "全部颜色",
                                "黑",
                                "白",
                                "灰",
                                "红",
                                "橙",
                                "黄",
                                "绿",
                                "青",
                                "蓝",
                                "紫",
                                "粉"
                            ],
                            Some(if q.color.is_empty() {
                                "全部颜色"
                            } else {
                                q.color.as_str()
                            }),
                            |v| Message::Manage(Filter(7, v.into()))
                        )
                        .width(Length::Fill)
                    ])
                    .push(muted("旧库如尚无颜色索引，请到备份与检查中运行修复。"));
                content = content.push(row![
                    text("添加方式").size(13).width(140),
                    pick_list(
                        vec!["全部方式", "托管导入", "外部引用"],
                        Some(match q.storage {
                            1 => "托管导入",
                            2 => "外部引用",
                            _ => "全部方式",
                        }),
                        |v| Message::Manage(Filter(8, v.into()))
                    )
                    .width(Length::Fill)
                ]);
                content = content.push(
                    row![
                        text_input("开始日期 YYYY-MM-DD", &self.manager.date_start)
                            .style(crate::theme::input)
                            .padding(10)
                            .on_input(|v| Message::Manage(Date(0, v))),
                        text_input("结束日期 YYYY-MM-DD", &self.manager.date_end)
                            .style(crate::theme::input)
                            .padding(10)
                            .on_input(|v| Message::Manage(Date(1, v)))
                    ]
                    .spacing(8),
                );
                content = content
                    .push(
                        checkbox(q.untagged)
                            .label("只看未加标签")
                            .on_toggle(|v| Message::Manage(Untagged(v))),
                    )
                    .push(
                        checkbox(q.unassigned)
                            .label("只看未归入项目")
                            .on_toggle(|v| Message::Manage(Unassigned(v))),
                    );
            }
            Some(Panel::Organize) => {
                content = content.push(crate::components::action(
                    "多字段编辑 / 选择全部查询结果",
                    Message::Batch(BatchMessage::Open),
                ));
                content = content
                    .push(muted(format!(
                        "已选择 {} 张。多选模式中，点击图片可加入或移除选择。",
                        self.selected_ids().len()
                    )))
                    .push(
                        row![
                            action("选中已加载区域", SelectVisible),
                            action("取消选择", ClearSelection)
                        ]
                        .spacing(8),
                    )
                    .push(
                        row![
                            action("收藏", Edit(moriq_domain::Edit::Favorite(true))),
                            action("取消收藏", Edit(moriq_domain::Edit::Favorite(false)))
                        ]
                        .spacing(8),
                    );
                let mut ratings = row![text("评分").size(13)].spacing(7);
                for n in 0..=5 {
                    ratings =
                        ratings.push(action(&n.to_string(), Edit(moriq_domain::Edit::Rating(n))));
                }
                content = content.push(ratings);
                content = content
                    .push(
                        text_input("要追加的标签，用逗号分隔", &self.manager.label)
                            .on_input(|s| Message::Manage(Label(s)))
                            .padding(10)
                            .style(input),
                    )
                    .push(action(
                        "追加标签",
                        Edit(moriq_domain::Edit::AddTags(
                            self.manager
                                .label
                                .split([',', '，'])
                                .map(|t| t.trim().to_string())
                                .filter(|t| !t.is_empty())
                                .collect(),
                        )),
                    ));
                content = content.push(action(
                    "移除填写的标签",
                    Edit(moriq_domain::Edit::RemoveTags(
                        self.manager
                            .label
                            .split([',', '，'])
                            .map(str::trim)
                            .filter(|v| !v.is_empty())
                            .map(str::to_string)
                            .collect(),
                    )),
                ));
                if let Some(library) = &self.library {
                    for p in &library.info.projects {
                        content = content.push(
                            row![
                                text(&p.name).size(13).width(Length::Fill),
                                action("加入", Edit(moriq_domain::Edit::Project(p.id, true))),
                                action("移出", Edit(moriq_domain::Edit::Project(p.id, false)))
                            ]
                            .spacing(8),
                        );
                    }
                }
                content = content
                    .push(separator())
                    .push(muted(
                        "移入回收站保留原片、版本、标签和项目关系，可随时恢复。",
                    ))
                    .push(
                        row![
                            action("移入回收站", Edit(moriq_domain::Edit::Trash(true))),
                            action("从回收站恢复", Edit(moriq_domain::Edit::Trash(false))),
                            action("导出原片", Export)
                        ]
                        .spacing(8),
                    );
                if self.query.trash {
                    content = content.push(action("永久删除所选素材…", Purge));
                    content = content.push(crate::components::action(
                        "清空整库回收站…",
                        Message::Deletion(DeletionMessage::EmptyTrash),
                    ));
                }
            }
            Some(Panel::Tags) => {
                let names: Vec<String> = self
                    .manager
                    .organizer
                    .tags
                    .iter()
                    .map(|t| t.0.clone())
                    .collect();
                content=content.push(pick_list(names,Some(self.manager.active_tag.clone()),|v|Message::Manage(ActiveTag(v))).placeholder("选择标签"))
                    .push(text_input("新名称，或要合并到的已有标签",&self.manager.tag_new_name).on_input(|v|Message::Manage(TagName(v))).padding(10).style(input))
                    .push(muted("目标已存在时合并素材关系，不会产生重复标签。删除标签会移除归类，原片保持不变。"))
                    .push(row![action("重命名 / 合并",RenameTag),action("删除标签及归类",DeleteTag)].spacing(8));
            }
            Some(Panel::ExportOptions) => {
                let options = self.manager.export_options.clone();
                for (field, label, values, selected) in [
                    (
                        0,
                        "格式",
                        vec!["原文件", "JPEG", "PNG", "WebP", "TIFF", "BMP"],
                        match options.format {
                            ExportFormat::Original => "原文件",
                            ExportFormat::Jpeg => "JPEG",
                            ExportFormat::Png => "PNG",
                            ExportFormat::Webp => "WebP",
                            ExportFormat::Tiff => "TIFF",
                            ExportFormat::Bmp => "BMP",
                        }
                        .to_string(),
                    ),
                    (
                        1,
                        "最长边 / 像素",
                        vec!["0", "1280", "1920", "2560", "3840"],
                        options.max_edge.to_string(),
                    ),
                    (
                        2,
                        "JPEG 质量",
                        vec!["70", "80", "90", "95", "100"],
                        options.quality.to_string(),
                    ),
                    (
                        3,
                        "顺时针旋转",
                        vec!["0", "90", "180", "270"],
                        options.rotate.to_string(),
                    ),
                    (
                        4,
                        "重采样",
                        vec!["面积平均", "Lanczos3", "双三次", "最近邻"],
                        match options.resampling {
                            Resampling::Area => "面积平均",
                            Resampling::Lanczos3 => "Lanczos3",
                            Resampling::Bicubic => "双三次",
                            Resampling::Nearest => "最近邻",
                        }
                        .into(),
                    ),
                    (
                        5,
                        "目录布局",
                        vec!["平铺", "按格式", "按导入日期"],
                        match options.layout {
                            ExportLayout::Flat => "平铺",
                            ExportLayout::Format => "按格式",
                            ExportLayout::ImportDate => "按导入日期",
                        }
                        .into(),
                    ),
                    (
                        6,
                        "整理信息",
                        vec!["不附带", "附带整理信息 JSON"],
                        if options.metadata_sidecar {
                            "附带整理信息 JSON"
                        } else {
                            "不附带"
                        }
                        .into(),
                    ),
                ] {
                    content = content.push(row![
                        text(label).width(160).size(13),
                        pick_list(
                            values.into_iter().map(str::to_string).collect::<Vec<_>>(),
                            Some(selected),
                            move |v| Message::Manage(ExportOption(field, v))
                        )
                    ]);
                }
                content=content.push(checkbox(options.preserve_exif).label("保留 EXIF（JPEG / PNG / WebP 嵌入，其他格式附带 .exif）").on_toggle(|v|Message::Manage(ExportOption(7,
v.to_string()))))
                    .push(text_input("目录模板，例如 {year}/{month}/{camera}",
&self.manager.export_options.directory_template).style(crate::theme::input).padding(10).on_input(|v|Message::Manage(ExportOption(8,
v))))
                    .push(muted("留空使用上方目录选择。变量：{date}、{year}、{month}、{day} 为导入日期，{format}、{camera}、{project}。转换图统一嵌入 sRGB；不复制私有 MakerNote 和旧缩略图。"))
                    .push(muted("最长边 0 表示不缩放；原文件配合缩放或旋转时输出 PNG。JPEG 透明区域使用白色背景，WebP 为无损编码。"))
                    .push(action("选择位置并导出",Export));
            }
            Some(Panel::Collections) => {
                content = content.push(self.folders_panel()).push(separator());
                let mut collections = iced::widget::column![form_heading(
                    Icon::Sparkles,
                    "智能集合",
                    "将当前的搜索、筛选及排序保存为智能集合；集合会随素材变化自动更新。"
                )]
                .spacing(16)
                .push(
                    text_input("集合名称", &self.manager.collection_name)
                        .on_input(|s| Message::Manage(CollectionName(s)))
                        .padding(10)
                        .style(input),
                )
                .push(
                    row![
                        form_action(
                            Icon::Filter,
                            "编辑组合条件…",
                            Message::Rules(crate::rules::RuleMessage::Open),
                            true
                        ),
                        form_action(
                            Icon::Collection,
                            "保存当前条件",
                            Message::Manage(SaveCollection),
                            false
                        )
                        .on_press_maybe(
                            (!self.manager.busy && !self.manager.collection_name.trim().is_empty())
                                .then_some(Message::Manage(SaveCollection))
                        )
                    ]
                    .spacing(12),
                );
                for c in &self.manager.organizer.collections {
                    collections = collections.push(row![
                        action(&c.name, UseCollection(c.id.clone())),
                        Space::new().width(Length::Fill),
                        action("删除规则", DeleteCollection(c.id.clone()))
                    ]);
                }
                content = content
                    .push(collections)
                    .push(separator())
                    .push(form_heading(
                        Icon::Tag,
                        "标签",
                        "管理标签，支持重命名、合并或删除标签。",
                    ))
                    .push(
                        form_action(
                            Icon::Tag,
                            "重命名、合并或删除标签",
                            Message::Manage(Tags),
                            false,
                        )
                        .width(Length::Fill),
                    );
                for (tag, count) in &self.manager.organizer.tags {
                    content = content.push(action(&format!("#{tag}  {count}"), Tag(tag.clone())));
                }
            }
            Some(Panel::Project) => content = content.push(self.project_form()),
            Some(Panel::Maintenance) => {
                content = content.push(crate::components::action(
                    "清空整库回收站…",
                    Message::Deletion(DeletionMessage::EmptyTrash),
                ));
                content = content.push(self.lifecycle_panel());
                content = content.push(action("恢复中断的导入（每次最多 100 项）", Recover));
                content=content.push(muted("备份包含托管原片、版本、项目、标签和 AI 数据，不包含可再生缓存及外部引用原片。引用文件需要另行备份。备份库可直接打开。"))
                    .push(action("选择库外位置备份",Backup))
                    .push(muted("检查会在后台逐个校验版本文件，较大的资源库可能需要较长时间。"))
                    .push(row![action("完整性检查",Check(false)),action("校验并修复缩略图",Check(true))].spacing(8))
                    .push(muted(format!("保留的中断导入记录：{}",self.manager.organizer.interrupted)));
            }
            _ => {}
        }
        if let Some(control) = &self.manager.maintenance
            && self.manager.panel == Some(Panel::Maintenance)
        {
            let total = control.total.load(Ordering::Relaxed);
            let finished = control.finished.load(Ordering::Relaxed);
            content = content.push(muted(if total == 0 {
                "正在检查数据库、准备文件清单…".to_string()
            } else {
                format!("已检查 {finished} / {total} 个版本")
            }));
            if total > 0 {
                content = content
                    .push(iced::widget::progress_bar(0.0..=total as f32, finished as f32).girth(5));
            }
            if control.cancel.load(Ordering::Relaxed) {
                content = content.push(muted("正在停止并清理临时文件…"));
            } else {
                content = content.push(action("停止检查", CancelCheck));
            }
        } else if self.manager.busy {
            content = content.push(muted("正在处理，请等待完成…"));
        }
        if !self.manager.report.is_empty() && self.manager.report_panel == self.manager.panel {
            content = content
                .push(separator())
                .push(text(&self.manager.report).size(12));
        }
        let has_footer = self.manager.panel == Some(Panel::Filters);
        // Reserve the title, padding and fixed actions before sizing the scroll area.
        let body_height = (self.viewport.height - if has_footer { 300.0 } else { 240.0 }).max(80.0);
        let body_content: Element<'_, Message> = if matches!(
            self.manager.panel,
            Some(Panel::People | Panel::Collections | Panel::Compare)
        ) {
            content.into()
        } else {
            form_card(content)
        };
        let body = container(
            scroll(body_content)
                .id("management")
                .on_scroll(|v| {
                    Message::OverlayScrolled(
                        v.absolute_offset().y,
                        (v.content_bounds().height - v.bounds().height).max(0.0),
                    )
                })
                .height(Length::Shrink),
        )
        .max_height(body_height);
        let mut form = iced::widget::column![header, separator(), body].spacing(20);
        if has_footer {
            form = form.push(
                row![
                    action("清空条件", ClearFilters),
                    button("组合条件…")
                        .padding([8, 12])
                        .style(secondary)
                        .on_press(Message::Rules(crate::rules::RuleMessage::Open)),
                    Space::new().width(Length::Fill),
                    action("应用", ApplyFilter)
                ]
                .spacing(10)
                .height(44)
                .align_y(Alignment::Center),
            );
        }
        let modal = container(form)
            .padding(28)
            .width(if self.manager.panel == Some(Panel::Compare) {
                (self.viewport.width - 48.0).clamp(280.0, 1200.0)
            } else {
                (self.viewport.width - 40.0).clamp(
                    260.0,
                    match self.manager.panel {
                        Some(Panel::Location | Panel::Delete | Panel::AssetProjects) => 560.0,
                        Some(Panel::Versions | Panel::Tasks) => 580.0,
                        Some(Panel::People | Panel::Collections) => 700.0,
                        _ => 640.0,
                    },
                )
            })
            .max_height((self.viewport.height - 80.0).max(160.0))
            .style(crate::theme::modal);
        stack![
            base,
            mouse_area(
                container(Space::new())
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(|_| container::Style {
                        background: Some(
                            iced::Color::from_rgba8(25, 40, 64, self.backdrop_alpha()).into()
                        ),
                        ..Default::default()
                    })
            )
            .on_press(Message::Manage(Close)),
            container(
                mouse_area(crate::motion::translated(
                    modal.into(),
                    self.surface_offset()
                ))
                .on_press(Message::Noop)
            )
            .center_x(Length::Fill)
            .center_y(Length::Fill)
        ]
        .into()
    }
}

pub(super) fn project_label(projects: &[moriq_domain::Project], id: ProjectId) -> String {
    let mut parts = Vec::new();
    let mut cursor = Some(id);
    let mut seen = std::collections::HashSet::new();
    while let Some(id) = cursor {
        if !seen.insert(id) {
            break;
        }
        let Some(project) = projects.iter().find(|p| p.id == id) else {
            break;
        };
        parts.push(project.name.as_str());
        cursor = project.parent;
    }
    parts.reverse();
    parts.join(" / ")
}
