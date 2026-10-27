use super::*;
use crate::components::localized_choice as pick_list;
use crate::{components::*, icons::Icon, theme::*};
use iced::widget::{
    Column, Space, button, checkbox, container, mouse_area, row, stack, text, text_input,
};
use iced::{Alignment, Length};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Panel {
    AiCategories,
    Libraries,
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
    /// 输出素材复制目标项目的显示名称。
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}
#[derive(Default)]
pub(super) struct Manager {
    pub similar: Vec<SimilarAsset>,
    pub panel: Option<Panel>,
    pub selection: selection::SelectionState,
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
    pub cover_saving: Option<(LibraryId, u64)>,
    pub label: String,
    pub collection_name: String,
    pub tag_new_name: String,
    pub new_project_name: String,
    pub new_project_parent: Option<ProjectId>,
    pub new_project_template: ProjectTemplate,
    pub copy_without_annotations: bool,
    pub active_tag: String,
    pub export: export::ExportState,
    pub parent: Option<ProjectId>,
    pub project_target: Option<ProjectId>,
    pub latitude: String,
    pub longitude: String,
    pub location_label: String,
    pub filters: filters::FilterDraft,
    pub rule_draft: Option<Rule>,
    pub rule_in_filter: bool,
    pub busy: bool,
    pub maintenance: Option<moriq_app::MaintenanceControl>,
    pub referenced_import: bool,
    pub import_seen: usize,
    pub import_refreshed: Option<std::time::Instant>,
    pub report: String,
    pub report_panel: Option<Panel>,
    pub synco: selection::SyncoState,
}
#[derive(Debug, Clone)]
pub enum ManageMessage {
    NewProjectTemplate(ProjectTemplate),
    CopyAnnotations(bool),
    CopyProject(ProjectId),
    AiMediumFilter(String),
    AiThemeFilter(String),
    SyncoExecutable,
    SyncoExecutablePicked(Option<PathBuf>),
    SyncoAutoStart(bool),
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
    AssetCover,
    PickedCover(LibraryId, Option<PathBuf>),
    CoverLoaded(LibraryId, u64, Result<Option<image::Handle>, String>),
    CoverSaved(LibraryId, u64, Result<(), String>),
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
    Filter(FilterChange),
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
    ExportOption(ExportChange),
    Date(u8, String),
    Builtin(Builtin),
    Edit(Edit),
    Edited(LibraryId, bool, Result<(), String>),
    RenameProject,
    Parent(Option<ProjectId>),
    NewChildProject,
    NewProjectName(String),
    NewParent(Option<ProjectId>),
    MoveProject,
    DeleteProject,
    FolderImport,
    ReferenceImport,
    Export,
    Backup,
    PickedDestination(LibraryId, bool, Option<PathBuf>),
    Published(LibraryId, bool, Result<PathBuf, String>),
    Recover,
    CancelRecovery,
    Recovered(LibraryId, Result<ImportReport, String>),
    Check(bool),
    CancelCheck,
    Purge,

    Purged(LibraryId, Result<PurgeReport, String>),
    Checked(LibraryId, Result<MaintenanceReport, String>),
}
impl App {
    /// 刷新项目、标签、分类和任务摘要，绑定当前库请求身份。
    pub(super) fn refresh_organizer(&mut self) -> Task<Message> {
        self.sync_project_workspace();
        let Some(library) = &self.library else {
            return Task::none();
        };

        let id = library.info.id;
        let service = self.service.clone();
        self.projection_request = self.projection_request.wrapping_add(1);
        let request = self.projection_request;
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
            Task::perform(async move { service.organizer(id).await }, move |r| {
                Message::Manage(ManageMessage::OrganizerLoaded(id, request, r))
            }),
        ])
    }
    /// 刷新受外观或素材变化影响的图片资料。
    pub(super) fn refresh_visuals(&mut self) -> Task<Message> {
        let Some(library) = &self.library else {
            return Task::none();
        };
        let id = library.info.id;
        let service = self.service.clone();
        self.overview_request = self.overview_request.wrapping_add(1);
        let request = self.overview_request;
        Task::perform(async move { service.overview(id).await }, move |result| {
            Message::Manage(ManageMessage::OverviewLoaded(id, request, result))
        })
    }
    /// 异步读取当前资源库封面，并拒绝旧库封面回执。
    pub(super) fn refresh_cover(&mut self) -> Task<Message> {
        let Some(library) = &self.library else {
            return Task::none();
        };
        let id = library.info.id;
        let service = self.service.clone();
        self.cover_request = self.cover_request.wrapping_add(1);
        let request = self.cover_request;
        Task::perform(
            async move {
                service
                    .cover(id)
                    .await
                    .map(|image| image.map(|p| image::Handle::from_rgba(p.width, p.height, p.rgba)))
            },
            move |result| Message::Manage(ManageMessage::CoverLoaded(id, request, result)),
        )
    }
    /// 刷新素材查询相关资料和可见内容。
    pub(super) fn refresh_catalog(&mut self) -> Task<Message> {
        Task::batch([self.refresh_organizer(), self.refresh_visuals()])
    }
    /// 刷新资源库资料及管理摘要。
    pub(super) fn refresh_library(&mut self) -> Task<Message> {
        Task::batch([self.refresh_catalog(), self.refresh_cover()])
    }
    /// 重新读取选中素材详情，保留请求和版本身份。
    pub(super) fn refresh_details(&self) -> Task<Message> {
        let (Some(library), Some(asset)) = (&self.library, &self.viewer.selected) else {
            return Task::none();
        };

        let id = library.info.id;
        let version = asset.version_id;
        let asset = asset.id;
        let service = self.service.clone();
        Task::perform(async move { service.details(id, asset).await }, move |r| {
            Message::Manage(ManageMessage::DetailsLoaded(id, asset, version, r))
        })
    }

    /// 按当前单选或批量选择状态收集有效素材身份。
    pub(super) fn selected_ids(&self) -> Vec<AssetId> {
        if self.manager.selection.enabled {
            self.manager.selection.assets.iter().copied().collect()
        } else {
            self.viewer.selected.iter().map(|a| a.id).collect()
        }
    }
    /// 处理素材编辑、项目管理和上下文操作，维护独立草稿与请求回执。
    pub(super) fn manage_message(&mut self, event: ManageMessage) -> Task<Message> {
        if self.libraries.busy {
            return Task::none();
        }
        let language = self.preferences.language;
        use ManageMessage::*;
        if let PickedDestination(expected, _, _) | PickedReference(expected, _, _, _) = &event
            && self.library.as_ref().is_none_or(|l| l.info.id != *expected)
        {
            return Task::none();
        }
        match event {
            NewProjectTemplate(template) => self.manager.new_project_template = template,
            CopyAnnotations(value) => self.manager.copy_without_annotations = !value,
            CopyProject(project) => {
                if let Some(library) = &self.library {
                    let owner = library.info.id;
                    let assets = self.selected_ids();
                    let annotations = !self.manager.copy_without_annotations;
                    let service = self.service.clone();
                    return self.mutation(async move {
                        service
                            .copy_to_project(owner, assets, project, annotations)
                            .await
                    });
                }
            }
            AiMediumFilter(id) => {
                if let Some((id, _)) = AI_MEDIA.iter().find(|(key, _)| *key == id) {
                    FilterChange::Medium(id).apply(&mut self.manager.filters.query, 0);
                } else if id.is_empty() {
                    FilterChange::Medium("").apply(&mut self.manager.filters.query, 0);
                }
            }
            AiThemeFilter(id) => {
                if let Some((id, _)) = AI_THEMES.iter().find(|(key, _)| *key == id) {
                    FilterChange::Theme(id).apply(&mut self.manager.filters.query, 0);
                } else {
                    let query = &mut self.manager.filters.query;
                    query.category.clear();
                    if query.themes.contains(&id) {
                        query.themes.retain(|s| s != &id);
                    } else {
                        query.themes.push(id);
                    }
                }
            }
            SyncoExecutable => {
                return Task::perform(
                    async move {
                        rfd::AsyncFileDialog::new()
                            .set_title(
                                tr!(language, "选择 Synco 程序" => "Select the Synco application"),
                            )
                            .add_filter(
                                tr!(language, "Synco 程序" => "Synco application"),
                                &["exe"],
                            )
                            .pick_file()
                            .await
                            .map(|f| f.path().to_path_buf())
                    },
                    |r| Message::Manage(SyncoExecutablePicked(r)),
                );
            }
            SyncoExecutablePicked(Some(path)) => {
                self.preferences.synco.executable = Some(path);
                return self
                    .queue_preference(preferences::Change::Synco(self.preferences.synco.clone()));
            }
            SyncoAutoStart(value) => {
                self.preferences.synco.auto_start = value;
                return self
                    .queue_preference(preferences::Change::Synco(self.preferences.synco.clone()));
            }
            SyncoRetry(task) if !self.manager.synco.running => {
                if let Some(library) = &self.library {
                    let id = library.info.id;
                    let service = self.service.clone();
                    self.manager.synco.running = true;
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
                if let (Some(library), Some(asset)) = (&self.library, &self.viewer.selected) {
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
                    && self.viewer.selected.as_ref().is_some_and(|a| a.id == asset)
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
                if let (Some(library), Some(asset)) = (&self.library, &self.viewer.selected) {
                    let value = if matches!(event, ClearLocation) {
                        None
                    } else {
                        let (Ok(latitude), Ok(longitude)) = (
                            self.manager.latitude.trim().parse(),
                            self.manager.longitude.trim().parse(),
                        ) else {
                            self.error = Some(tr!(language, "经纬度应为十进制数字" => "Latitude and longitude must be decimal numbers").into());
                            return Task::none();
                        };
                        let value = AssetLocation {
                            latitude,
                            longitude,
                            label: self.manager.location_label.trim().into(),
                        };
                        if !value.valid() {
                            self.error =
                                Some(tr!(language, "纬度应为 -90～90，经度 -180～180，名称最多 256 字" => "Latitude must be -90 to 90, longitude -180 to 180, and the name at most 256 characters").into());
                            return Task::none();
                        }
                        Some(value)
                    };
                    let id = library.info.id;
                    let asset = asset.id;
                    let version = self.viewer.selected.as_ref().map(|a| a.version_id);
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
                            .viewer
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
                        move |r| {
                            Message::PlatformDone(
                                tr!(language, "已请求在浏览器中打开地图" => "Requested to open the map in your browser"),
                                r,
                            )
                        },
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
                        self.status = trf!(language, "已删除项目及 {} 张独占素材{}" => "Deleted the project and {} exclusive assets{}",
                            report.deleted,
                            if report.cleanup_pending {
                                tr!(language, "，文件清理将在重开后继续" => "; file cleanup will continue when the library reopens")
                            } else {
                                ""
                            }
                        );
                        return Task::batch([
                            self.reset_query(),
                            self.refresh_info(),
                            self.refresh_catalog(),
                        ]);
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            ToggleTags => self.manager.tags_collapsed = !self.manager.tags_collapsed,
            AssetMenu | AssetProjects | Versions if self.viewer.selected.is_some() => {
                if !matches!(event, AssetMenu) {
                    self.manager.selection.enabled = false;
                }
                self.manager.panel = Some(match event {
                    AssetProjects => Panel::AssetProjects,
                    Versions => Panel::Versions,
                    _ => Panel::AssetMenu,
                });
                return self.refresh_details();
            }
            ViewOptions => self.manager.panel = Some(Panel::ViewOptions),
            RevealAsset => {
                if let (Some(library), Some(asset)) = (&self.library, &self.viewer.selected) {
                    let library = library.info.id;
                    let asset = asset.id;
                    let service = self.service.clone();
                    return Task::perform(
                        async move { service.reveal_asset(library, asset).await },
                        move |r| {
                            Message::PlatformDone(
                                tr!(language, "已请求在文件夹中显示所选图片" => "Requested to reveal the selected image"),
                                r,
                            )
                        },
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
                        self.manager.selection.anchor = None;
                        self.viewer.selected = None;
                        self.viewer.preview = None;
                        self.manager.report = trf!(language, "已永久删除 {} 项。{}" => "Permanently deleted {} items. {}",
                            report.deleted,
                            if report.cleanup_pending {
                                tr!(language, "暂存文件清理未完成，下次打开资源库将继续。" => "Staging cleanup is incomplete and will continue when the library reopens.")
                            } else {
                                ""
                            }
                        );
                        self.status = self.manager.report.clone();
                        return Task::batch([
                            self.refresh(),
                            self.refresh_info(),
                            self.refresh_catalog(),
                            self.rebuild_masonry(),
                        ]);
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            Parent(parent) => self.manager.parent = parent,
            NewParent(parent) if !self.manager.busy => self.manager.new_project_parent = parent,
            NewParent(_) => {}
            NewProjectName(value) => self.manager.new_project_name = value,
            NewChildProject | MoveProject => {
                if let Some(library) = &self.library {
                    let id = library.info.id;
                    let parent = if matches!(event, NewChildProject) {
                        self.manager.new_project_parent
                    } else {
                        self.manager.parent
                    };
                    let name = self.manager.new_project_name.clone();
                    let project = self.manager.project_target;
                    let service = self.service.clone();
                    if matches!(event, NewChildProject) {
                        let template = self.manager.new_project_template;
                        return self.mutation(async move {
                            service
                                .create_project_configured(id, name, parent, template)
                                .await
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
                if let (Some(library), Some(asset)) = (&self.library, &self.viewer.selected) {
                    let (id, asset, version) = (library.info.id, asset.id, asset.version_id);
                    return Task::perform(
                        async move {
                            rfd::AsyncFileDialog::new()
                                .set_title(tr!(language, "重新定位原片 · 内容必须与登记副本一致" => "Relocate original · Content must match the registered copy"))
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
                self.manager.export.options = Default::default();
                self.manager.panel = Some(Panel::ExportOptions);
            }
            ExportOption(change) => self.manager.export.apply(change),
            Date(field, value) => {
                if field == 0 {
                    self.manager.filters.date_start = value;
                } else {
                    self.manager.filters.date_end = value;
                }
            }

            ImportMenu => self.manager.panel = Some(Panel::Import),
            Synco => {
                self.manager.panel = Some(Panel::Synco);
                return self.refresh_organizer();
            }
            SendSynco if !self.manager.synco.running => {
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
                    self.manager.synco.running = true;
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
                        tr!(language, "已发送到 Synco" => "Sent to Synco"),
                        tr!(language, "发送副本与任务已保存，可在任务面板查看。" => "The copy and task were saved. View them in the task panel.").into(),
                        0,
                        "",
                        asset,
                    ),
                    Err(e) => {
                        self.persist_asset_notice(tr!(language, "Synco 发送失败" => "Failed to send to Synco"), e.clone(), 2, "synco", asset)
                    }
                };
                self.manager.synco.running = false;
                if let Err(e) = result {
                    self.error = Some(e);
                }
                return Task::batch([
                    notice,
                    self.refresh_organizer(),
                    self.manage_message(SyncoPoll(true)),
                ]);
            }
            SyncoPoll(manual) if !self.manager.synco.running => {
                if let Some(l) = &self.library {
                    let id = l.info.id;
                    let service = self.service.clone();
                    self.manager.synco.start();
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
                        tr!(language, "Synco 任务更新" => "Synco task update"),
                        trf!(language, "已领取并确认 {} 个任务结果" => "Received and acknowledged {} task results", count),
                        0,
                        "",
                    ),
                    Err(e) if manual => {
                        self.persist_notice(tr!(language, "Synco 检查失败" => "Synco check failed"), e.clone(), 2, "synco")
                    }
                    _ => Task::none(),
                };
                self.manager.synco.finish(result.is_ok());
                match result {
                    Ok(count) => {
                        if manual || count > 0 {
                            self.manager.report = trf!(language, "Synco 检查完成，已确认接收 {count} 个通知" => "Synco check complete; acknowledged {count} notifications");
                        }
                    }
                    Err(e) => {
                        self.manager.report = e;
                    }
                }
                return Task::batch([notice, self.refresh_catalog(), self.refresh_details()]);
            }
            Version(version) => {
                if let (Some(l), Some(a)) = (&self.library, &self.viewer.selected) {
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
                    if self.viewer.selected.as_ref().is_some_and(|a| a.id == asset) {
                        // Resolve fresh dimensions and metadata; never reselect an older image.
                        return Task::batch([self.load_neighborhood(asset, 0), self.refresh()]);
                    }
                    return self.refresh();
                }
            }
            Description(value) => self.manager.description_draft = Some(value),
            AssetCover
                if !self.switching
                    && !self.manager.busy
                    && self.manager.cover_saving.is_none()
                    && !self.query.trash =>
            {
                if let (Some(library), Some(asset)) = (&self.library, &self.viewer.selected) {
                    let (id, asset, version) = (library.info.id, asset.id, asset.version_id);
                    self.cover_write_request = self.cover_write_request.wrapping_add(1);
                    let request = self.cover_write_request;
                    self.manager.cover_saving = Some((id, request));
                    self.manager.panel = None;
                    self.status =
                        tr!(language,"正在设置资源库封面…"=>"Setting library cover…").into();
                    let service = self.service.clone();
                    return Task::perform(
                        async move { service.set_asset_cover(id, asset, version).await },
                        move |r| Message::Manage(CoverSaved(id, request, r)),
                    );
                }
            }
            Cover if !self.switching && self.manager.cover_saving.is_none() => {
                if let Some(l) = &self.library {
                    let id = l.info.id;
                    return Task::perform(
                        async move {
                            rfd::AsyncFileDialog::new()
                                .set_title(
                                    tr!(language, "选择资源库封面" => "Choose a library cover"),
                                )
                                .add_filter(
                                    tr!(language, "图片" => "Images"),
                                    &["jpg", "jpeg", "png", "webp", "bmp"],
                                )
                                .pick_file()
                                .await
                                .map(|p| p.path().to_path_buf())
                        },
                        move |p| Message::Manage(PickedCover(id, p)),
                    );
                }
            }
            PickedCover(id, Some(path))
                if !self.switching
                    && self.manager.cover_saving.is_none()
                    && self.library.as_ref().is_some_and(|l| l.info.id == id) =>
            {
                self.cover_write_request = self.cover_write_request.wrapping_add(1);
                let request = self.cover_write_request;
                self.manager.cover_saving = Some((id, request));
                let service = self.service.clone();
                return Task::perform(async move { service.set_cover(id, path).await }, move |r| {
                    Message::Manage(CoverSaved(id, request, r))
                });
            }
            CoverSaved(id, request, result)
                if self.manager.cover_saving == Some((id, request))
                    && self.library.as_ref().is_some_and(|l| l.info.id == id) =>
            {
                self.manager.cover_saving = None;
                match result {
                    Ok(()) => {
                        self.library_cards_request = self.library_cards_request.wrapping_add(1);
                        self.status =
                            tr!(language,"已设为资源库封面"=>"Library cover updated").into();
                        return self.refresh_cover();
                    }
                    Err(e) => {
                        self.status = tr!(language,"封面设置失败，请检查后重试"=>"Cover update failed; check the error and retry").into();
                        self.error = Some(e);
                    }
                }
            }
            CoverLoaded(id, request, Ok(handle))
                if request == self.cover_request
                    && self.library.as_ref().is_some_and(|l| l.info.id == id) =>
            {
                if let Some(library) = &self.library {
                    if let Some(cover) = &handle {
                        self.library_covers
                            .insert(library.root.clone(), cover.clone());
                    } else {
                        self.library_covers.remove(&library.root);
                    }
                }
                self.manager.cover = handle
            }
            CoverLoaded(id, request, Err(e))
                if request == self.cover_request
                    && self.library.as_ref().is_some_and(|l| l.info.id == id) =>
            {
                self.error = Some(e)
            }
            Recover if !self.manager.busy => {
                if let Some(l) = &self.library {
                    let id = l.info.id;
                    let service = self.service.clone();
                    self.manager.busy = true;
                    let control = ImportControl::with_preferences(self.preferences.import);
                    self.lifecycle.recovery = Some(control.clone());
                    return Task::perform(
                        async move { service.recover_controlled(id, control).await },
                        move |r| Message::Manage(Recovered(id, r)),
                    );
                }
            }
            CancelRecovery => self.lifecycle.cancel_recovery(),
            Recovered(id, result) if self.library.as_ref().is_some_and(|l| l.info.id == id) => {
                self.lifecycle.recovery = None;
                self.manager.report_panel = Some(Panel::Maintenance);
                self.manager.busy = false;
                match result {
                    Ok(r) => {
                        if r.cancelled {
                            self.status = tr!(language, "恢复已停止，未完成项目可稍后重试" => "Recovery stopped. Unfinished items can be resumed later.").into();
                        }
                        self.manager.report = trf!(language, "已恢复 {}，跳过重复 {}，失败 {}。\n{}" => "Restored {}, skipped {} duplicates, failed {}.\n{}",
                            r.imported,
                            r.duplicates,
                            r.failed,
                            r.failures.join("\n")
                        );
                        return Task::batch([
                            self.refresh(),
                            self.refresh_info(),
                            self.refresh_catalog(),
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
                self.manager.filters.query = self.query.clone();
                self.manager.filters.date_start = String::new();
                self.manager.filters.date_end = String::new();
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
                self.manager.selection.toggle_mode();
            }
            RangeSelected(id, generation, result)
                if self.library.as_ref().is_some_and(|l| l.info.id == id)
                    && self.generation == generation
                    && self.manager.selection.enabled =>
            {
                match result {
                    Ok(ids) => self.manager.selection.assets.extend(ids),
                    Err(error) => self.error = Some(error),
                }
            }
            Toggle(id) => {
                if self.modifiers.shift()
                    && let (Some(anchor), Some(library)) =
                        (self.manager.selection.anchor, &self.library)
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
                self.manager.selection.toggle(id);
            }
            SelectVisible => {
                self.manager.selection.enabled = true;
                let range = self.visible_range();
                for (i, tile) in self.tiles.iter().enumerate() {
                    if range.contains(&(i + self.offset)) {
                        self.manager.selection.assets.insert(tile.asset.id);
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
                if request == self.overview_request
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
                if request == self.overview_request
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
                        .viewer
                        .selected
                        .as_ref()
                        .is_some_and(|a| a.id == asset && a.version_id == version) =>
            {
                if !self.manager.notes_dirty {
                    self.manager.notes = result.notes.clone();
                }
                let version_changed = self.ai.category_version != Some(version);
                if version_changed {
                    self.ai.category = result.category.clone();
                    self.ai.category_version = Some(version);
                    self.ai.classification_dirty = false;
                    self.ai.description_dirty = false;
                    self.ai.content_saving = false;
                    self.ai.content_request = self.ai.content_request.wrapping_add(1);
                }
                if !self.ai.classification_dirty && !self.ai.content_saving {
                    self.ai.medium = if result.medium.is_empty() {
                        "uncertain".into()
                    } else {
                        result.medium.clone()
                    };
                    self.ai.themes = result.themes.clone();
                }
                if !self.ai.description_dirty && !self.ai.content_saving {
                    self.ai.description = result.ai_caption.clone();
                    if version_changed || self.ai.description_editor.text() != self.ai.description {
                        self.ai.description_editor =
                            iced::widget::text_editor::Content::with_text(&self.ai.description);
                    }
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
            Filter(change) => change.apply(
                &mut self.manager.filters.query,
                chrono::Utc::now().timestamp_millis().max(0) as u64,
            ),
            Untagged(v) => self.manager.filters.query.untagged = v,
            Unassigned(v) => self.manager.filters.query.unassigned = v,
            ApplyFilter => {
                use chrono::TimeZone;
                let parse = |value: &str, end: bool| -> std::result::Result<u64, String> {
                    let date = chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")
                        .map_err(|_| tr!(language, "日期格式应为 YYYY-MM-DD" => "Date format must be YYYY-MM-DD").to_string())?;
                    let date = if end {
                        date.succ_opt()
                            .ok_or(tr!(language, "日期超出范围" => "Date is out of range"))?
                    } else {
                        date
                    };
                    let time = date
                        .and_hms_opt(0, 0, 0)
                        .ok_or(tr!(language, "日期无效" => "Invalid date"))?;
                    let utc = chrono::Local
                        .from_local_datetime(&time)
                        .single()
                        .ok_or(tr!(language, "当地日期存在时区歧义，请选择其他日期" => "This local time is ambiguous. Choose another date."))?
                        .timestamp_millis();
                    u64::try_from(utc).map_err(|_| tr!(language, "日期不能早于 1970 年" => "Dates cannot be earlier than 1970").into())
                };
                for (value, end) in [
                    (&self.manager.filters.date_start, false),
                    (&self.manager.filters.date_end, true),
                ] {
                    if !value.trim().is_empty() {
                        match parse(value.trim(), end) {
                            Ok(at) => {
                                if end {
                                    self.manager.filters.query.before = at
                                } else {
                                    self.manager.filters.query.since = at
                                }
                            }
                            Err(e) => {
                                self.error = Some(e);
                                return Task::none();
                            }
                        }
                    }
                }
                if self.manager.filters.query.before > 0
                    && self.manager.filters.query.before <= self.manager.filters.query.since
                {
                    self.error = Some(tr!(language, "结束日期应晚于或等于开始日期" => "The end date must be on or after the start date").into());
                    return Task::none();
                }
                self.query = self.manager.filters.query.clone();
                self.manager.panel = None;
                self.screen = Screen::Assets;
                return self.reset_query();
            }
            ClearFilters => {
                self.query = Query {
                    project: self.query.project,
                    ..Default::default()
                };
                self.manager.panel = None;
                return self.reset_query();
            }
            Builtin(which) => {
                let project = self.query.project;
                self.query = which.query(chrono::Utc::now().timestamp_millis().max(0) as u64);
                self.query.project = project;
                self.screen = Screen::Assets;
                self.manager.panel = None;
                return self.reset_query();
            }
            Tag(tag) => {
                self.query = Query {
                    project: self.query.project,
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
                    async move {
                        rfd::AsyncFileDialog::new()
                            .set_title(tr!(language, "引用外部原片 · 请保留源文件及磁盘位置" => "Reference external originals · Keep the source files and drive location"))
                            .add_filter(tr!(language, "媒体" => "Media"), moriq_app::IMPORT_EXTENSIONS)
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
                    async move {
                        rfd::AsyncFileDialog::new()
                            .set_title(tr!(language, "选择要导入的图片文件夹" => "Choose an image folder to import"))
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
                if backup {
                    let cancelled = self
                        .lifecycle
                        .copy
                        .take()
                        .is_some_and(|control| control.cancel.load(Ordering::Acquire));
                    if cancelled && result.is_err() {
                        self.manager.busy = false;
                        self.status = tr!(language, "备份已停止" => "Backup stopped").into();
                        return Task::none();
                    }
                }
                self.manager.report_panel = Some(if backup {
                    Panel::Maintenance
                } else {
                    Panel::ExportOptions
                });
                let notice = match &result {
                    Ok(path) => self.persist_notice(
                        tr!(language, "文件输出完成" => "Export complete"),
                        display_path(path),
                        0,
                        "",
                    ),
                    Err(e) => self.persist_notice(
                        tr!(language, "文件输出失败" => "Export failed"),
                        e.clone(),
                        2,
                        "",
                    ),
                };
                self.manager.busy = false;
                match result {
                    Ok(path) => {
                        self.manager.report =
                            trf!(language, "已完成：{}" => "Completed: {}", display_path(&path))
                    }
                    Err(e) => self.error = Some(e),
                }
                return notice;
            }
            Checked(id, result) if self.library.as_ref().is_some_and(|l| l.info.id == id) => {
                self.manager.report_panel = Some(Panel::Maintenance);
                let notice = match &result {
                    Ok(r) => self.persist_notice(
                        tr!(language, "资源库检查完成" => "Library check complete"),
                        trf!(language, "检查 {} 个版本，缺失 {}，损坏 {}，修复 {}，{}" => "Checked {} versions: {} missing, {} corrupt, {} repaired, {}",
                            r.checked,
                            r.missing,
                            r.damaged,
                            r.repaired,
                            if r.cancelled {
                                tr!(language, "已停止" => "Stopped")
                            } else {
                                tr!(language, "已完成" => "Completed")
                            }
                        ),
                        if r.missing + r.damaged > 0 { 1 } else { 0 },
                        "maintenance",
                    ),
                    Err(e) => self.persist_notice(tr!(language, "资源库检查失败" => "Library check failed"), e.clone(), 2, "maintenance"),
                };
                self.manager.busy = false;
                self.manager.maintenance = None;
                match result {
                    Ok(r) => {
                        self.manager.report = trf!(language, "{}检查 {} 个版本；缺失 {}，校验不符 {}，修复缓存 {}。\n{}" => "{}Checked {} versions; {} missing, {} checksum mismatches, {} caches repaired.\n{}",
                            if r.cancelled {
                                tr!(language, "已停止，已" => "Stopped. ")
                            } else {
                                tr!(language, "已完成，已" => "Completed. ")
                            },
                            r.checked,
                            r.missing,
                            r.damaged,
                            r.repaired,
                            r.warnings.join("\n")
                        );
                        self.status = trf!(language, "资源库检查{}：{} 个版本，修复 {} 项缓存" => "Library check {}: {} versions, {} caches repaired",
                            if r.cancelled {
                                tr!(language, "已停止" => "Stopped")
                            } else {
                                tr!(language, "已完成" => "Completed")
                            },
                            r.checked,
                            r.repaired
                        );
                        if r.repaired > 0 {
                            return Task::batch([
                                notice,
                                self.refresh(),
                                self.refresh_info(),
                                self.refresh_catalog(),
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
                    self.viewer.selected = None;
                    self.viewer.preview = None;
                }
                return self.update(Message::VisualsSaved(id, result));
            }
            SaveNotes => {
                if let (Some(l), Some(a)) = (&self.library, &self.viewer.selected) {
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
                    && self.viewer.selected.as_ref().is_some_and(|a| a.id == asset)
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
                                            tr!(language, "选择库外备份位置" => "Choose a backup location outside the library")
                                        } else {
                                            tr!(language, "选择导出位置" => "Choose an export location")
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
                        let options = self.manager.export.options.clone();
                        let control = moriq_app::BackupControl::default();
                        if backup {
                            self.lifecycle.copy = Some(control.clone());
                        }
                        return Task::perform(
                            async move {
                                if backup {
                                    service.copy_library(id, parent, false, control).await
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
    /// 构建当前管理面板或对话框，保持底层浏览上下文。
    pub(super) fn management_overlay<'a>(
        &'a self,
        base: Element<'a, Message>,
    ) -> Element<'a, Message> {
        let language = self.preferences.language;
        if self.compact_panel() {
            return self.popup(base);
        }
        let mut content = Column::new().spacing(18);
        for (section, _) in &self.manager.organizer.errors {
            let (panel, label) = match section {
                OrganizerSection::Folders => {
                    (Panel::Collections, tr!(language, "分组" => "Groups"))
                }
                OrganizerSection::Jobs => (
                    Panel::Tasks,
                    tr!(language, "后台任务" => "Background tasks"),
                ),
                OrganizerSection::People => (Panel::People, tr!(language, "人物" => "People")),
                OrganizerSection::Categories => {
                    (Panel::Menu, tr!(language, "分类" => "Categories"))
                }
                OrganizerSection::Tags => (Panel::Tags, tr!(language, "标签" => "Tags")),
                OrganizerSection::Collections => (
                    Panel::Collections,
                    tr!(language, "智能集合" => "Smart collections"),
                ),
                OrganizerSection::Statistics => {
                    (Panel::Maintenance, tr!(language, "统计" => "Statistics"))
                }
                OrganizerSection::Synco => {
                    (Panel::Synco, tr!(language, "Synco 交接" => "Synco handoff"))
                }
                OrganizerSection::Profile => (
                    Panel::Maintenance,
                    tr!(language, "资源库信息" => "Library information"),
                ),
            };
            if self.manager.panel == Some(panel) || self.manager.panel == Some(Panel::Menu) {
                content = content.push(muted(trf!(language, "{label}暂时无法读取。请重新打开资源库重试，诊断信息已写入日志。" => "{label} is temporarily unavailable. Reopen the library to retry. Diagnostics were written to the log."
                )));
            }
        }
        let title = match self.manager.panel {
            Some(Panel::Libraries) => tr!(language,"管理资源库"=>"Manage library"),
            Some(Panel::AiCategories) => tr!(language,"自定义分类"=>"Customize categories"),
            Some(Panel::Activity) => tr!(language, "活动记录" => "Activity"),
            Some(Panel::Notifications) => tr!(language, "通知中心" => "Notifications"),
            Some(Panel::Location) => tr!(language, "编辑地点" => "Edit location"),
            Some(Panel::Storage) => tr!(language, "资源库空间明细" => "Library storage details"),
            Some(Panel::Rules) => tr!(language, "组合规则" => "Combined rules"),
            Some(Panel::Help) => tr!(language, "搜索与快捷键" => "Search and shortcuts"),
            Some(Panel::Licenses) => tr!(language, "开源组件声明" => "Open source notices"),
            Some(Panel::Compare) => {
                tr!(language, "版本并排对比" => "Compare versions side by side")
            }
            Some(Panel::Tasks) => tr!(language, "后台任务" => "Background tasks"),
            Some(Panel::Delete) => {
                if self.deletion.final_step {
                    tr!(language, "最后确认：永久删除" => "Final confirmation: permanent deletion")
                } else {
                    tr!(language, "确认删除" => "Confirm deletion")
                }
            }
            Some(Panel::Menu) => tr!(language, "资源库工具" => "Library tools"),
            Some(Panel::Similar) => tr!(language, "视觉近似图片" => "Visually similar images"),
            Some(Panel::People) => {
                tr!(language, "人物管理与人工纠正" => "People and recognition corrections")
            }
            Some(Panel::Metadata) => tr!(language, "EXIF 与 GPS 检索" => "EXIF and GPS search"),
            Some(Panel::Batch) => tr!(language, "批量编辑多个字段" => "Batch edit multiple fields"),
            Some(Panel::AssetMenu) => tr!(language, "素材操作" => "Asset actions"),
            Some(Panel::AssetProjects) => tr!(language, "素材所属项目" => "Asset projects"),
            Some(Panel::Versions) => tr!(language, "素材版本" => "Asset versions"),
            Some(Panel::ViewOptions) => tr!(language, "显示方式" => "Display options"),
            Some(Panel::Import) => tr!(language, "导入素材" => "Assets imported"),
            Some(Panel::Filters) => tr!(language, "筛选与排序" => "Filter and sort"),
            Some(Panel::Organize) => tr!(language, "整理选中的素材" => "Organize selected assets"),
            Some(Panel::Tags) => tr!(language, "标签管理" => "Tag management"),
            Some(Panel::ExportOptions) => tr!(language, "导出素材" => "Export assets"),
            Some(Panel::Collections) => {
                tr!(language, "标签与智能集合" => "Tags and smart collections")
            }
            Some(Panel::Project) => tr!(language, "项目管理" => "Project management"),
            Some(Panel::Synco) => {
                tr!(language, "Synco 编辑与版本交接" => "Synco editing and version handoff")
            }
            _ => tr!(language, "备份与完整性检查" => "Backup and integrity"),
        };
        let back: Element<'_, Message> = if self.surface_has_parent() {
            icon_button(Icon::Left, Message::Back)
                .on_press_maybe(self.can_go_back().then_some(Message::Back))
                .into()
        } else {
            Space::new().width(0).into()
        };
        let (kind, description) = match self.manager.panel {
            Some(Panel::Libraries) => (
                Icon::Database,
                tr!(language,"移除列表记录，或单独确认永久删除。"=>"Forget a list entry or separately confirm permanent deletion."),
            ),
            Some(Panel::AiCategories) => (
                Icon::Tag,
                tr!(language, "定义你的分类方式，同步到智能整理与素材侧栏。" => "Define categories for organize cards and the asset sidebar."),
            ),
            Some(Panel::Collections) => (
                Icon::Tag,
                tr!(language, "管理你的整理方式，让素材更有条理。" => "Manage your organization tools and keep assets in order."),
            ),
            Some(Panel::People) => (
                Icon::Person,
                tr!(language, "管理已识别的人物，支持修改名称、合并人物或手动纠正识别结果。" => "Manage recognized people: rename, merge or manually correct recognition results."),
            ),
            Some(Panel::Tags) => (
                Icon::Tag,
                tr!(language, "管理标签名称与素材归类，支持重命名、合并或删除。" => "Manage tag names and asset categories: rename, merge or delete."),
            ),
            Some(Panel::Filters | Panel::Metadata | Panel::Rules) => (
                Icon::Filter,
                tr!(language, "组合搜索条件，快速找到需要的素材。" => "Combine filters to quickly find the assets you need."),
            ),
            Some(Panel::Project | Panel::AssetProjects) => (
                Icon::Folder,
                tr!(language, "管理素材所属项目与项目层级。" => "Manage asset membership and the project hierarchy."),
            ),
            Some(Panel::Tasks) => (
                Icon::Clock,
                tr!(language, "查看处理进度，管理后台任务。" => "View progress and manage background tasks."),
            ),
            Some(Panel::ExportOptions) => (
                Icon::Download,
                tr!(language, "选择输出格式、尺寸与整理方式。" => "Choose output format, dimensions and folder layout."),
            ),
            Some(Panel::Activity) => (
                Icon::Clock,
                tr!(language, "查看资源库内的操作与处理记录。" => "View library operations and processing history."),
            ),
            Some(Panel::Notifications) => (
                Icon::Bell,
                tr!(language, "查看任务结果与需要关注的消息。" => "View task results and messages that need attention."),
            ),
            Some(Panel::Location) => (
                Icon::Pin,
                tr!(language, "查看和编辑素材的地理位置。" => "View and edit asset locations."),
            ),
            Some(Panel::Storage | Panel::Maintenance) => (
                Icon::Database,
                tr!(language, "管理本地资源、备份与资源库完整性。" => "Manage local storage, backups and library integrity."),
            ),
            Some(Panel::Delete) => (
                Icon::Trash,
                tr!(language, "核对本次操作的范围与影响后再继续。" => "Review the scope and impact of this operation before continuing."),
            ),
            Some(Panel::Versions | Panel::Compare) => (
                Icon::Image,
                tr!(language, "查看素材版本，保留原片与编辑记录。" => "View asset versions while preserving originals and edit history."),
            ),
            Some(Panel::Synco) => (
                Icon::Network,
                tr!(language, "发送素材副本，并接收编辑后的新版本。" => "Send asset copies and receive edited versions."),
            ),
            Some(Panel::Similar) => (
                Icon::Search,
                tr!(language, "查看与当前素材接近的图片。" => "View images similar to the current asset."),
            ),
            Some(Panel::Batch | Panel::Organize) => (
                Icon::Edit,
                tr!(language, "统一整理所选素材的标签、评分与其他信息。" => "Edit tags, ratings and other information for selected assets."),
            ),
            Some(Panel::Licenses) => (
                Icon::Package,
                tr!(language, "查看第三方组件与模型的授权说明。" => "View third party component and model licenses."),
            ),
            _ => (
                Icon::Settings,
                tr!(language, "管理资源库与应用工具。" => "Manage library and application tools."),
            ),
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
            let selected = if self.manager.selection.enabled {
                !self.manager.selection.assets.is_empty()
            } else {
                self.viewer.selected.is_some()
            };
            let available = match &event {
                ManageMessage::Menu
                | ManageMessage::Help
                | ManageMessage::Licenses
                | ManageMessage::Tasks
                | ManageMessage::ViewOptions
                | ManageMessage::Filters => true,
                ManageMessage::CancelCheck => true,
                ManageMessage::SaveCollection => {
                    !self.manager.busy && !self.manager.collection_name.trim().is_empty()
                }
                ManageMessage::Edit(_)
                | ManageMessage::Export
                | ManageMessage::ExportMenu
                | ManageMessage::Purge => !self.manager.busy && selected,
                ManageMessage::SendSynco => {
                    !self.manager.busy && !self.manager.synco.running && selected
                }
                ManageMessage::SaveNotes => {
                    !self.manager.busy && self.viewer.selected.is_some() && self.manager.notes_dirty
                }
                ManageMessage::RenameTag => {
                    !self.manager.busy
                        && !self.manager.active_tag.is_empty()
                        && !self.manager.tag_new_name.trim().is_empty()
                }
                ManageMessage::DeleteTag => {
                    !self.manager.busy && !self.manager.active_tag.is_empty()
                }
                ManageMessage::SyncoPoll(_) => !self.manager.synco.running,
                _ => !self.manager.busy,
            };
            button(text(label.to_string()).size(13))
                .padding([8, 12])
                .style(secondary)
                .on_press_maybe(available.then_some(Message::Manage(event)))
        };
        use ManageMessage::*;
        match self.manager.panel {
            Some(Panel::Libraries) => {
                content = content.push(self.library_actions_form());
            }
            Some(Panel::AiCategories) => {
                content = content.push(self.category_editor());
            }
            Some(Panel::People) => content = content.push(self.people_panel()),
            Some(Panel::Metadata) => content = content.push(self.metadata_panel()),
            Some(Panel::Batch) => content = content.push(self.batch_panel()),
            Some(Panel::Tasks) => content = content.push(self.tasks_panel()),
            Some(Panel::Delete) => content = content.push(self.deletion_panel()),
            Some(Panel::Compare) => content = content.push(self.compare_panel()),
            Some(Panel::Licenses) => {
                content=content.push(muted(tr!(language, "发行包的 licenses 文件夹内提供第三方组件说明、逐项依赖清单与许可文本。模型权重的授权声明以各模型原始来源为准。" => "The distribution's licenses folder contains third party notices, a dependency inventory and license texts. Model weight licenses are provided by their original sources.")))
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
                    tr!(language, "Ctrl+K：定位全局搜索。搜索文件名、标签和备注，特殊符号按字面匹配。" => "Ctrl+K: Focus global search. Search file names, tags and notes. Special characters are matched literally."),
                    tr!(language, "精确标签、RAW、日期与评分请在筛选中选择；复杂条件使用组合规则。" => "Use filters for exact tags, RAW, dates and ratings. Use combined rules for more complex queries."),
                    tr!(language, "Alt+← / →：后退与前进。← / →：按当前排序浏览相邻素材。" => "Alt+← / →: Back and forward. ← / →: Browse neighboring assets in the current sort order."),
                    tr!(language, "Ctrl+A：选择当前已加载的素材（最多 128 张）。Shift+单击：范围选择。" => "Ctrl+A: Select loaded assets (up to 128). Shift+click: Select a range."),
                    tr!(language, "Ctrl+单击：加入或取消多选。右键已选素材：操作整个选中范围。" => "Ctrl+click: Add to or remove from selection. Right-click a selected asset: Act on the entire selection."),
                    tr!(language, "Ctrl+Shift+A：对全部查询结果进行批量编辑。B：打开多字段批量编辑。" => "Ctrl+Shift+A: Batch edit all search results. B: Open multi-field batch editing."),
                    tr!(language, "0～5：给选中范围评分。F：切换收藏。Ctrl+E：导出所选素材。" => "0–5: Rate the selection. F: Toggle favorite. Ctrl+E: Export selected assets."),
                    tr!(language, "Enter / 空格：放大选中图片。Delete：将选中范围移入回收站。" => "Enter / Space: Enlarge the selected image. Delete: Move the selection to Trash."),
                    tr!(language, "Esc：先关闭放大查看，再关闭面板或详情。编辑文字时不触发选择、删除或相邻浏览。" => "Esc: Close the enlarged view, then the panel or details. Selection, deletion and browsing shortcuts are disabled while editing text."),
                    tr!(language, "AI 分析需先在模型管理中安装模型，再建立索引；自然语言搜索使用语义搜索入口。" => "Install models and build indexes before running AI analysis. Use semantic search for natural language queries."),
                ] {
                    content = content.push(muted(line));
                }
            }
            Some(Panel::Similar) => {
                content=content.push(muted(tr!(language, "根据画面指纹、比例和颜色查找尺寸/压缩变体，最多显示100项；结果供人工判断，不自动合并或删除。" => "Find size and compression variants using visual fingerprints, aspect ratio and color. Up to 100 results are shown for manual review; nothing is merged or deleted automatically.")))
                    .push(crate::components::action(tr!(language, "建立/更新全库近似索引" => "Build / update library similarity index"),Message::Jobs(crate::tasks::JobMessage::Queue(JobKind::Fingerprints))));
                if self.manager.similar.is_empty() {
                    content = content.push(muted(
                        tr!(language, "没有找到已索引的近似图片。旧图片可先建立索引，再重新查询。" => "No indexed similar images found. Build an index for older images and try again."),
                    ));
                }
                for item in &self.manager.similar {
                    content = content.push(crate::components::action(
                        &trf!(language, "{} · 差异 {}" => "{} · Difference {}", item.name, item.distance),
                        Message::Journal(crate::journal::JournalMessage::OpenAsset(item.id)),
                    ));
                }
            }
            Some(Panel::Storage) => content = content.push(self.storage_panel()),
            Some(Panel::Rules) => content = content.push(self.rules_panel()),
            Some(Panel::Location) => {
                content = content.push(muted(
                    tr!(language, "补充地点仅保存在当前图片版本中，原片 EXIF 保持不变。清除后重新显示原片 GPS。" => "Custom locations are saved only to this image version. Original EXIF is preserved. Clearing the custom location restores the original GPS display."),
                ));
                for (field, label, value) in [
                    (
                        0,
                        tr!(language, "纬度（-90～90）" => "Latitude (-90 to 90)"),
                        &self.manager.latitude,
                    ),
                    (
                        1,
                        tr!(language, "经度（-180～180）" => "Longitude (-180 to 180)"),
                        &self.manager.longitude,
                    ),
                    (
                        2,
                        tr!(language, "地点名称（可选）" => "Location name (optional)"),
                        &self.manager.location_label,
                    ),
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
                            action(tr!(language, "保存补充地点" => "Save custom location"), SaveLocation),
                            action(tr!(language, "清除补充地点" => "Clear custom location"), ClearLocation)
                        ]
                        .spacing(10),
                    )
                    .push(muted(
                        tr!(language, "仅在点击“打开地图”时将坐标交给浏览器；不会自动联网查询地名。" => "Coordinates are sent to your browser only when you click Open map. Place names are never looked up automatically."),
                    ));
            }
            Some(Panel::Activity) => content = content.push(self.event_list(false)),
            Some(Panel::Notifications) => content = content.push(self.event_list(true)),
            Some(Panel::AssetProjects) => {
                if let Some(asset) = &self.viewer.selected {
                    content = content.push(heading(&asset.name, 15));
                    content = content.push(muted(tr!(language, "加入其他项目会创建独立副本，后续修改互不影响。" => "Adding to another project creates an independent copy; later edits stay separate.")));
                    content=content.push(
                        checkbox(!self.manager.copy_without_annotations)
                            .label(tr!(language,"同时复制标签、描述与收藏评分"=>"Include tags, descriptions, favorites and ratings"))
                            .on_toggle(|value|Message::Manage(CopyAnnotations(value)))
                    );
                    content=content.push(muted(tr!(language,"分类按名称和类型匹配；未匹配项留待整理。人物分组独立创建，集合关系不复制。"=>"Categories match by name and type; unmatched items await organization. People are independent and collection membership is not copied.")));
                    if self.manager.details.projects.is_empty() {
                        content = content.push(muted(tr!(language, "这张素材尚未加入项目" => "This asset does not belong to a project yet")));
                    }
                    if let Some(library) = &self.library {
                        for project in &library.info.projects {
                            let belongs = self.manager.details.projects.contains(&project.id);
                            let copied = self.manager.details.project_copies.contains(&project.id);
                            let edit = if belongs {
                                Edit(moriq_domain::Edit::Project(project.id, false))
                            } else {
                                CopyProject(project.id)
                            };
                            content = content.push(
                                row![
                                    crate::components::action(
                                        &project_label(&library.info.projects, project.id),
                                        Message::Project(project.id)
                                    ),
                                    Space::new().width(Length::Fill),
                                    action(
                                        if belongs {
                                            tr!(language, "移出" => "Remove")
                                        } else if copied {
                                            tr!(language, "已有独立副本" => "Independent copy exists")
                                        } else {
                                            tr!(language, "复制到项目" => "Copy to project")
                                        },
                                        edit.clone()
                                    )
                                    .on_press_maybe((!copied || belongs).then_some(Message::Manage(edit)))
                                ]
                                .align_y(Alignment::Center),
                            );
                        }
                    }
                }
            }
            Some(Panel::Versions) => {
                content = content.push(
                    button(tr!(language, "并排对比版本…" => "Compare versions side by side…"))
                        .style(secondary)
                        .on_press(Message::Compare(crate::compare::CompareMessage::Open)),
                );
                if let Some(asset) = &self.viewer.selected {
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
                    content = content.push(muted(tr!(language, "切换展示版本会保留原片与全部精修版本。" => "Changing the displayed version preserves originals and all edited versions.")));
                }
            }
            Some(Panel::Synco) => {
                content=content.push(checkbox(self.preferences.synco.auto_start).label(tr!(language, "发送时自动启动 Synco" => "Start Synco automatically when sending")).on_toggle(|v|Message::Manage(SyncoAutoStart(v))))
                    .push(action(tr!(language, "选择 Synco 程序" => "Select the Synco application"),SyncoExecutable))
                    .push(muted(self.preferences.synco.executable.as_ref().map(|p|p.display().to_string()).unwrap_or_else(||tr!(language, "自动查找安装位置" => "Find installation automatically").into())))
                    .push(muted(tr!(language, "需要 Synco 0.4.1 或更新版本。每张图片使用独立任务；精修结果保留为新版本，完成交接后自动回收发送副本。" => "Requires Synco 0.4.1 or later. Each image uses a separate task. Edits become new versions; sent copies are cleaned up after handoff.")))
                    .push(row![action(tr!(language, "发送选中图片" => "Send selected images"),SendSynco),action(tr!(language, "立即检查 / 重试连接" => "Check now / retry connection"),SyncoPoll(true))].spacing(8));
                if self.manager.synco.running {
                    content = content.push(muted(
                        tr!(language, "正在与 Synco 交接…" => "Handing off to Synco…"),
                    ));
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
                            tr!(language, "使用原交接版本创建新任务重试" => "Retry as a new task using the original handoff version"),
                            SyncoRetry(task.id.clone()),
                        ));
                    }
                }
            }
            Some(Panel::Filters) => {
                content = content.push(muted(
                    tr!(language, "主题（同时满足所选项）" => "Themes (match all selected)"),
                ));
                let profile = &self.manager.organizer.ai_profile;
                content = content.push(
                    row![
                        muted(tr!(language,"媒介"=>"Medium")),
                        choice_labels(
                            std::iter::once(String::new())
                                .chain(
                                    profile
                                        .categories
                                        .iter()
                                        .filter(|c| c.kind == AiCategoryKind::Medium)
                                        .map(|c| c.id.clone())
                                )
                                .collect(),
                            Some(self.manager.filters.query.medium.clone()),
                            move |id| profile.categories.iter().find(|c| &c.id == id).map_or_else(
                                || tr!(language,"全部媒介"=>"All media").to_string(),
                                |c| crate::categories::category_label(language, c)
                            ),
                            |id| Message::Manage(AiMediumFilter(id))
                        )
                    ]
                    .spacing(12),
                );
                for c in profile
                    .categories
                    .iter()
                    .filter(|c| c.kind == AiCategoryKind::Theme && c.enabled)
                {
                    let id = c.id.clone();
                    content = content.push(
                        iced::widget::checkbox(self.manager.filters.query.themes.contains(&id))
                            .label(crate::categories::category_label(language, c))
                            .on_toggle(move |_| Message::Manage(AiThemeFilter(id.clone()))),
                    );
                }
                content = content.push(crate::components::action(
                    tr!(language, "EXIF 与 GPS 高级检索" => "Advanced EXIF and GPS search"),
                    Message::Metadata(MetadataMessage::Open),
                ));
                for (label, values, selected) in filters::rows(&self.manager.filters.query) {
                    content = content.push(
                        row![
                            text(crate::i18n::option_label(language, label))
                                .size(13)
                                .width(140),
                            pick_list(language, values, selected, |choice| Message::Manage(
                                Filter(choice)
                            ))
                            .width(Length::Fill)
                        ]
                        .align_y(Alignment::Center),
                    );
                }
                content=content.push(muted(tr!(language, "旧库如尚无颜色索引，请到备份与检查中运行修复。" => "If an older library has no color index, run repair in Backup and integrity.")));
                content = content.push(
                    row![
                        text_input(
                            tr!(language, "开始日期 YYYY-MM-DD" => "Start date YYYY-MM-DD"),
                            &self.manager.filters.date_start
                        )
                        .style(crate::theme::input)
                        .padding(10)
                        .on_input(|v| Message::Manage(Date(0, v))),
                        text_input(
                            tr!(language, "结束日期 YYYY-MM-DD" => "End date YYYY-MM-DD"),
                            &self.manager.filters.date_end
                        )
                        .style(crate::theme::input)
                        .padding(10)
                        .on_input(|v| Message::Manage(Date(1, v)))
                    ]
                    .spacing(8),
                );
                content = content
                    .push(
                        checkbox(self.manager.filters.query.untagged)
                            .label(tr!(language, "只看未加标签" => "Untagged only"))
                            .on_toggle(|v| Message::Manage(Untagged(v))),
                    )
                    .push(
                        checkbox(self.manager.filters.query.unassigned)
                            .label(tr!(language, "只看未归入项目" => "Without project only"))
                            .on_toggle(|v| Message::Manage(Unassigned(v))),
                    );
            }
            Some(Panel::Organize) => {
                content = content.push(crate::components::action(
                    tr!(language, "多字段编辑 / 选择全部查询结果" => "Multi-field editing / select all results"),
                    Message::Batch(BatchMessage::Open),
                ));
                content = content
                    .push(muted(trf!(language, "已选择 {} 张。多选模式中，点击图片可加入或移除选择。" => "{} images selected. Click an image in selection mode to add or remove it.",
                        self.selected_ids().len()
                    )))
                    .push(
                        row![
                            action(tr!(language, "选中已加载区域" => "Select loaded area"), SelectVisible),
                            action(tr!(language, "取消选择" => "Deselect"), ClearSelection)
                        ]
                        .spacing(8),
                    )
                    .push(
                        row![
                            action(tr!(language, "收藏" => "Favorite"), Edit(moriq_domain::Edit::Favorite(true))),
                            action(tr!(language, "取消收藏" => "Unfavorite"), Edit(moriq_domain::Edit::Favorite(false)))
                        ]
                        .spacing(8),
                    );
                let mut ratings = row![text(tr!(language, "评分" => "Rating")).size(13)].spacing(7);
                for n in 0..=5 {
                    ratings =
                        ratings.push(action(&n.to_string(), Edit(moriq_domain::Edit::Rating(n))));
                }
                content = content.push(ratings);
                content = content
                    .push(
                        text_input(tr!(language, "要追加的标签，用逗号分隔" => "Tags to add, separated by commas"), &self.manager.label)
                            .on_input(|s| Message::Manage(Label(s)))
                            .padding(10)
                            .style(input),
                    )
                    .push(action(
                        tr!(language, "追加标签" => "Add tags"),
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
                    tr!(language, "移除填写的标签" => "Remove entered tags"),
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
                                action(
                                    tr!(language, "加入" => "Add"),
                                    Edit(moriq_domain::Edit::Project(p.id, true))
                                ),
                                action(
                                    tr!(language, "移出" => "Remove"),
                                    Edit(moriq_domain::Edit::Project(p.id, false))
                                )
                            ]
                            .spacing(8),
                        );
                    }
                }
                content = content
                    .push(separator())
                    .push(muted(
                        tr!(language, "移入回收站保留原片、版本、标签和项目关系，可随时恢复。" => "Trash preserves originals, versions, tags and project membership. You can restore them at any time."),
                    ))
                    .push(
                        row![
                            action(tr!(language, "移入回收站" => "Move to Trash"), Edit(moriq_domain::Edit::Trash(true))),
                            action(tr!(language, "从回收站恢复" => "Restore from Trash"), Edit(moriq_domain::Edit::Trash(false))),
                            action(tr!(language, "导出原片" => "Export originals"), Export)
                        ]
                        .spacing(8),
                    );
                if self.query.trash {
                    content = content.push(action(
                        tr!(language, "永久删除所选素材…" => "Permanently delete selected assets…"),
                        Purge,
                    ));
                    content = content.push(crate::components::action(
                        tr!(language, "清空整库回收站…" => "Empty library trash…"),
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
                content=content.push(choice(names,Some(self.manager.active_tag.clone()),|v|Message::Manage(ActiveTag(v))).placeholder(tr!(language, "选择标签" => "Select tag")))
                    .push(text_input(tr!(language, "新名称，或要合并到的已有标签" => "New name, or an existing tag to merge into"),&self.manager.tag_new_name).on_input(|v|Message::Manage(TagName(v))).padding(10).style(input))
                    .push(muted(tr!(language, "目标已存在时合并素材关系，不会产生重复标签。删除标签会移除归类，原片保持不变。" => "If the target tag exists, memberships are merged without duplicates. Deleting a tag removes its categorization and preserves originals.")))
                    .push(row![action(tr!(language, "重命名 / 合并" => "Rename / merge"),RenameTag),action(tr!(language, "删除标签及归类" => "Delete tag and memberships"),DeleteTag)].spacing(8));
            }
            Some(Panel::ExportOptions) => {
                let options = self.manager.export.options.clone();
                for (label, values, selected) in export::rows(&options) {
                    content = content.push(row![
                        text(crate::i18n::option_label(language, label))
                            .width(160)
                            .size(13),
                        pick_list(language, values, Some(selected), |choice| Message::Manage(
                            ExportOption(choice)
                        ))
                    ]);
                }
                content=content.push(
                    checkbox(options.preserve_exif)
                        .label(tr!(language, "保留 EXIF（JPEG / PNG / WebP 嵌入，其他格式附带 .exif）" => "Preserve EXIF (embedded in JPEG / PNG / WebP; .exif sidecar for other formats)"))
                        .on_toggle(|v|Message::Manage(ExportOption(ExportChange::PreserveExif(v))))
                )
                    .push(text_input(tr!(language, "目录模板，例如 {year}/{month}/{camera}" => "Folder template, e.g. {year}/{month}/{camera}"),
&self.manager.export.options.directory_template)
                        .style(crate::theme::input)
                        .padding(10)
                        .on_input(|v|Message::Manage(ExportOption(ExportChange::Template(v)))))
                    .push(muted(tr!(language, "留空使用上方目录选择。变量：{date}、{year}、{month}、{day} 为导入日期，{format}、{camera}、{project}。转换图统一嵌入 sRGB；不复制私有 MakerNote 和旧缩略图。" => "Leave blank to use the layout above. Variables: {date}, {year}, {month}, {day} use import dates; also {format}, {camera}, {project}. Converted images embed sRGB. Private MakerNotes and old thumbnails are excluded.")))
                    .push(muted(tr!(language, "最长边 0 表示不缩放；原文件配合缩放或旋转时输出 PNG。JPEG 透明区域使用白色背景，WebP 为无损编码。" => "Set the longest edge to 0 to keep the original size. Resizing or rotating an original file produces PNG. JPEG uses white for transparency; WebP encoding is lossless.")))
                    .push(action(tr!(language, "选择位置并导出" => "Choose location and export"),Export));
            }
            Some(Panel::Collections) => {
                content = content.push(self.folders_panel()).push(separator());
                let mut collections = iced::widget::column![form_heading(
                    Icon::Sparkles,
                    tr!(language, "智能集合" => "Smart collections"),
                    tr!(language, "将当前的搜索、筛选及排序保存为智能集合；集合会随素材变化自动更新。" => "Save the current search, filters and sort order as a smart collection. Results update automatically as assets change.")
                )]
                .spacing(16)
                .push(
                    text_input(tr!(language, "集合名称" => "Collection name"), &self.manager.collection_name)
                        .on_input(|s| Message::Manage(CollectionName(s)))
                        .padding(10)
                        .style(input),
                )
                .push(
                    row![
                        form_action(
                            Icon::Filter,
                            tr!(language, "编辑组合条件…" => "Edit combined rules…"),
                            Message::Rules(crate::rules::RuleMessage::Open),
                            true
                        ),
                        form_action(
                            Icon::Collection,
                            tr!(language, "保存当前条件" => "Save current filters"),
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
                        action(
                            tr!(language, "删除规则" => "Delete rule"),
                            DeleteCollection(c.id.clone())
                        )
                    ]);
                }
                content = content
                    .push(collections)
                    .push(separator())
                    .push(form_heading(
                        Icon::Tag,
                        tr!(language, "标签" => "Tags"),
                        tr!(language, "管理标签，支持重命名、合并或删除标签。" => "Manage tags: rename, merge or delete."),
                    ))
                    .push(
                        form_action(
                            Icon::Tag,
                            tr!(language, "重命名、合并或删除标签" => "Rename, merge or delete tags"),
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
                    tr!(language, "清空整库回收站…" => "Empty library trash…"),
                    Message::Deletion(DeletionMessage::EmptyTrash),
                ));
                content = content.push(self.lifecycle_panel());
                content = content.push(action(tr!(language, "恢复中断的导入（每次最多 100 项）" => "Resume interrupted imports (up to 100 at a time)"), Recover));
                content=content.push(muted(tr!(language, "备份包含托管原片、版本、项目、标签和 AI 数据，不包含可再生缓存及外部引用原片。引用文件需要另行备份。备份库可直接打开。" => "Backups include managed originals, versions, projects, tags and AI data. Regenerable caches and external originals are excluded. Back up referenced files separately. Backup libraries can be opened directly.")))
                    .push(action(tr!(language, "选择库外位置备份" => "Back up outside the library"),Backup))
                    .push(muted(tr!(language, "检查会在后台逐个校验版本文件，较大的资源库可能需要较长时间。" => "Version files are checked individually in the background. Large libraries may take some time.")))
                    .push(row![action(tr!(language, "完整性检查" => "Integrity check"),Check(false)),action(tr!(language, "校验并修复缩略图" => "Verify and repair thumbnails"),Check(true))].spacing(8))
                    .push(muted(trf!(language, "保留的中断导入记录：{}" => "Retained interrupted import records: {}",self.manager.organizer.interrupted)));
            }
            _ => {}
        }
        if let Some(control) = &self.manager.maintenance
            && self.manager.panel == Some(Panel::Maintenance)
        {
            let total = control.total.load(Ordering::Relaxed);
            let finished = control.finished.load(Ordering::Relaxed);
            content = content.push(muted(if total == 0 {
                tr!(language, "正在检查数据库、准备文件清单…" => "Checking the database and preparing the file list…").to_string()
            } else {
                trf!(language, "已检查 {finished} / {total} 个版本" => "Checked {finished} / {total} versions")
            }));
            if total > 0 {
                content = content
                    .push(iced::widget::progress_bar(0.0..=total as f32, finished as f32).girth(5));
            }
            if control.cancel.load(Ordering::Relaxed) {
                content = content.push(muted(tr!(language, "正在停止并清理临时文件…" => "Stopping and cleaning up temporary files…")));
            } else {
                content = content.push(action(
                    tr!(language, "停止检查" => "Stop check"),
                    CancelCheck,
                ));
            }
        } else if self.manager.busy {
            content = content.push(muted(
                tr!(language, "正在处理，请等待完成…" => "Processing, please wait…"),
            ));
        }
        if !self.manager.report.is_empty() && self.manager.report_panel == self.manager.panel {
            content = content
                .push(separator())
                .push(text(&self.manager.report).size(12));
        }
        let categories = self.manager.panel == Some(Panel::AiCategories);
        let has_footer = self.manager.panel == Some(Panel::Filters);
        // Reserve the title, padding and fixed actions before sizing the scroll area.
        let body_height = (self.viewport.height
            - if categories {
                350.0
            } else if has_footer {
                300.0
            } else {
                240.0
            })
        .max(80.0);
        let body_content: Element<'_, Message> = if matches!(
            self.manager.panel,
            Some(
                Panel::People
                    | Panel::Collections
                    | Panel::Compare
                    | Panel::AiCategories
                    | Panel::Project
                    | Panel::Libraries
            )
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
        if categories {
            form = form.push(self.category_footer());
        }
        if has_footer {
            form = form.push(
                row![
                    action(tr!(language, "清空条件" => "Clear filters"), ClearFilters),
                    button(tr!(language, "组合条件…" => "Combined rules…"))
                        .padding([8, 12])
                        .style(secondary)
                        .on_press(Message::Rules(crate::rules::RuleMessage::Open)),
                    Space::new().width(Length::Fill),
                    action(tr!(language, "应用" => "Apply"), ApplyFilter)
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
                        Some(Panel::AiCategories) => 920.0,
                        Some(Panel::Project) => 760.0,
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

/// 按项目身份取得当前管理摘要中的显示名称。
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
