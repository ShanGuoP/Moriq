use super::*;
use crate::components::*;
use iced::futures::{StreamExt, stream};
use iced::widget::{column, text_editor, text_input};
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct AiState {
    pub categories: crate::categories::State,
    pub loaded: bool,
    pub loading: bool,
    pub models: Vec<moriq_app::ModelInfo>,
    pub hardware: Option<moriq_app::Hardware>,
    pub downloads: HashMap<String, moriq_app::DownloadControl>,
    pub category: String,
    pub category_version: Option<AssetVersionId>,
    pub medium: String,
    pub themes: Vec<String>,
    pub description: String,
    pub description_editor: text_editor::Content,
    pub classification_dirty: bool,
    pub description_dirty: bool,
    pub content_saving: bool,
    pub content_request: u64,
    pub person_name: String,
    pub person: Option<PersonChoice>,
    pub target: Option<PersonChoice>,
    pub portraits: HashMap<String, image::Handle>,
    pub portrait_request: u64,
    pub portrait_cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub portrait_task: Option<iced::task::Handle>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonChoice {
    pub id: String,
    pub name: String,
    label: String,
}
impl PersonChoice {
    /// 根据人物名称、身份及素材数量构造本地化的选择项标签。
    fn person(language: Language, p: &Person) -> Self {
        let name = p.name.clone();
        let label = if name.is_empty() || name == "未命名人物" {
            trf!(language, "未命名人物 · {} · {} 张" => "Unnamed person · {} · {} photos",
                &p.id[..p.id.len().min(8)],
                p.count
            )
        } else {
            trf!(language, "{} · {} 张" => "{} · {} photos", name, p.count)
        };
        Self {
            id: p.id.clone(),
            name,
            label,
        }
    }
}
impl std::fmt::Display for PersonChoice {
    /// 输出人物选择项的显示标签。
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}
#[derive(Debug, Clone)]
pub enum AiMessage {
    Workspace(Option<ProjectId>),
    Categories(crate::categories::CategoryMessage),
    Group(String),
    SwitchLibrary(PathBuf),
    LibraryOpened(Result<Opened, String>),
    Organize,
    Refresh,
    Loaded(Result<(Vec<moriq_app::ModelInfo>, moriq_app::Hardware), String>),
    Install(String),
    Pause(String),
    Uninstall(String),
    Installed(String, Result<(), String>),
    Category(String),
    CategoryInput(String),
    SaveCategory,
    Medium(String),
    Theme(String),
    DescriptionAction(text_editor::Action),
    SaveClassification,
    SaveDescription,
    EditContent(AiEdit),
    ContentStored(
        LibraryId,
        AssetId,
        AssetVersionId,
        u64,
        u8,
        Result<(), String>,
    ),
    People,
    Person(PersonChoice),
    Target(PersonChoice),
    Name(String),
    Rename,
    Merge,
    Assign(String, Option<String>),
    PersonFilter(String),
    PortraitLoaded(
        LibraryId,
        u64,
        String,
        Result<Option<moriq_app::Pixels>, String>,
    ),
    Stored(LibraryId, Result<(), String>),
}
impl App {
    /// 模型页侧栏切换整理空间，其他页面打开项目素材。
    pub(super) fn sidebar_project_message(&self, project: ProjectId) -> Message {
        if self.screen == Screen::Models {
            Message::Ai(AiMessage::Workspace(Some(project)))
        } else {
            Message::Project(project)
        }
    }

    /// 构建模型页面的资源库切换列表，保留当前库身份。
    pub(super) fn model_libraries(&self) -> Vec<(String, PathBuf, bool)> {
        let mut entries = self
            .preferences
            .recent
            .iter()
            .map(|(name, path)| {
                let active = self
                    .library
                    .as_ref()
                    .is_some_and(|library| library.root == *path);
                let name = if active {
                    self.library
                        .as_ref()
                        .map_or(name.clone(), |library| library.info.name.clone())
                } else {
                    name.clone()
                };
                (name, path.clone(), active)
            })
            .collect::<Vec<_>>();
        if let Some(library) = &self.library
            && !entries.iter().any(|(_, path, _)| *path == library.root)
        {
            entries.insert(0, (library.info.name.clone(), library.root.clone(), true));
        }
        entries
    }
    /// 处理 AI 内容编辑、人物管理和推理回执，核对库、版本与请求身份。
    pub(super) fn ai_message(&mut self, event: AiMessage) -> Task<Message> {
        let language = self.preferences.language;
        let service = self.service.clone();
        let content_ready = !self.switching
            && self
                .viewer
                .selected
                .as_ref()
                .is_some_and(|a| self.ai.category_version == Some(a.version_id));
        match event {
            AiMessage::Workspace(project) => {
                self.query = Query {
                    project,
                    ..Default::default()
                };
                self.manager.panel = None;
                self.screen = Screen::Models;
                return Task::batch([self.reset_query(), self.refresh_visuals()]);
            }
            AiMessage::Categories(event) => return self.category_message(event),
            AiMessage::Group(id) => {
                let Some(group) = self.manager.organizer.ai_groups.iter().find(|g| g.id == id)
                else {
                    return Task::none();
                };
                self.query = group.query.clone();
                self.screen = Screen::Assets;
                return self.reset_query();
            }
            AiMessage::Medium(value) if !self.ai.content_saving && content_ready => {
                self.ai.medium = value;
                self.ai.classification_dirty = true;
            }
            AiMessage::Theme(value) if !self.ai.content_saving && content_ready => {
                if self.ai.themes.contains(&value) {
                    self.ai.themes.retain(|s| s != &value);
                } else {
                    self.ai.themes.push(value);
                }
                self.ai.classification_dirty = true;
            }
            AiMessage::DescriptionAction(action) if !self.ai.content_saving && content_ready => {
                let changed = action.is_edit();
                self.ai.description_editor.perform(action);
                if changed {
                    self.ai.description = self.ai.description_editor.text();
                    self.ai.description_dirty = true;
                }
            }
            AiMessage::SaveClassification
            | AiMessage::SaveDescription
            | AiMessage::EditContent(_)
                if !self.ai.content_saving && content_ready =>
            {
                let Some(owner) = self.library.as_ref().map(|l| l.info.id) else {
                    return Task::none();
                };
                let Some(asset) = self.viewer.selected.as_ref() else {
                    return Task::none();
                };
                let asset_id = asset.id;
                let version = asset.version_id;
                let (edit, fields) = match event {
                    AiMessage::EditContent(edit) => {
                        let fields = match &edit {
                            AiEdit::Medium(_) | AiEdit::Themes(_) => 1,
                            AiEdit::Description(_) => 2,
                            _ => 0,
                        };
                        (edit, fields)
                    }
                    AiMessage::SaveClassification => (
                        AiEdit::Classification(self.ai.medium.clone(), self.ai.themes.clone()),
                        1,
                    ),
                    _ => (AiEdit::Description(Some(self.ai.description.clone())), 2),
                };
                self.ai.content_saving = true;
                self.ai.content_request = self.ai.content_request.wrapping_add(1);
                let request = self.ai.content_request;
                return Task::perform(
                    async move {
                        service
                            .edit_ai_content(owner, asset_id, version, edit)
                            .await
                    },
                    move |r| {
                        Message::Ai(AiMessage::ContentStored(
                            owner, asset_id, version, request, fields, r,
                        ))
                    },
                );
            }
            AiMessage::ContentStored(owner, asset, version, request, fields, result)
                if self.library.as_ref().is_some_and(|l| l.info.id == owner) =>
            {
                if self.ai.content_request == request
                    && self
                        .viewer
                        .selected
                        .as_ref()
                        .is_some_and(|a| a.id == asset && a.version_id == version)
                {
                    self.ai.content_saving = false;
                    if result.is_ok() {
                        if fields == 1 {
                            self.ai.classification_dirty = false;
                        }
                        if fields == 2 {
                            self.ai.description_dirty = false;
                        }
                    }
                }
                if let Err(error) = result {
                    self.error = Some(error);
                }
                return Task::batch([
                    self.refresh_catalog(),
                    self.refresh(),
                    self.refresh_details(),
                ]);
            }
            AiMessage::PortraitLoaded(owner, request, person, result)
                if self.library.as_ref().is_some_and(|l| l.info.id == owner)
                    && request == self.ai.portrait_request
                    && !self.ai.portrait_cancel.load(Ordering::Acquire) =>
            {
                if let Ok(Some(pixels)) = result {
                    self.ai.portraits.insert(
                        person,
                        image::Handle::from_rgba(pixels.width, pixels.height, pixels.rgba),
                    );
                } else {
                    self.ai.portraits.remove(&person);
                }
            }
            AiMessage::SwitchLibrary(path)
                if self.import.is_none() && !self.switching && !self.manager.busy =>
            {
                if self
                    .library
                    .as_ref()
                    .is_some_and(|library| library.root == path)
                {
                    self.library_picker = false;
                    self.screen = Screen::Models;
                    return Task::none();
                }
                self.switching = true;
                self.transition = None;
                return Task::perform(async move { service.open(path).await }, |r| {
                    Message::Ai(AiMessage::LibraryOpened(r))
                });
            }
            AiMessage::LibraryOpened(result) => {
                let opened = result.is_ok();
                let task = self.update_inner(Message::Opened(result));
                if opened {
                    self.screen = Screen::Models;
                    self.viewer.preview_expanded = false;
                    self.library_picker = false;
                }
                return task;
            }
            AiMessage::Organize => {
                if self.query.project.is_none()
                    && self
                        .library
                        .as_ref()
                        .is_some_and(|l| !l.info.projects.is_empty())
                {
                    self.status =
                        tr!(language,"请先选择要整理的项目"=>"Select a project to organize").into();
                    return Task::none();
                }
                let Some(library) = &self.library else {
                    return Task::none();
                };
                let owner = library.info.id;
                return Task::batch(self.ai.models.iter().filter_map(|model| {
                    if !model.installed {
                        return None;
                    }
                    if model.spec.id == "qwen35-4b" {
                        let classification = service.clone();
                        let description = service.clone();
                        let faces = service.clone();
                        return Some(Task::batch([
                            Task::perform(
                                async move {
                                    faces
                                        .queue_media_job(owner, JobKind::QwenFaces)
                                        .await
                                        .map_err(|e| e.to_string())
                                },
                                move |r| Message::Jobs(tasks::JobMessage::Stored(owner, r)),
                            ),
                            Task::perform(
                                async move {
                                    classification
                                        .queue_media_job(owner, JobKind::QwenClassification)
                                        .await
                                        .map_err(|e| e.to_string())
                                },
                                move |r| Message::Jobs(tasks::JobMessage::Stored(owner, r)),
                            ),
                            Task::perform(
                                async move {
                                    description
                                        .queue_media_job(owner, JobKind::QwenDescription)
                                        .await
                                        .map_err(|e| e.to_string())
                                },
                                move |r| Message::Jobs(tasks::JobMessage::Stored(owner, r)),
                            ),
                        ]));
                    }
                    None
                }));
            }
            AiMessage::Refresh => {
                if self.ai.loading {
                    return Task::none();
                }
                self.ai.loading = true;
                return Task::perform(async move { service.models().await }, |r| {
                    Message::Ai(AiMessage::Loaded(r))
                });
            }
            AiMessage::Loaded(result) => {
                self.ai.loading = false;
                self.ai.loaded = true;
                match result {
                    Ok((models, hardware)) => {
                        self.ai.models = models;
                        self.ai.hardware = Some(hardware)
                    }
                    Err(e) => self.error = Some(e),
                }
            }
            AiMessage::Install(model) => {
                if self.ai.downloads.contains_key(&model) {
                    return Task::none();
                }
                let control = moriq_app::DownloadControl::default();
                self.ai.downloads.insert(model.clone(), control.clone());
                let receipt = model.clone();
                let use_mirror = self.preferences.ai.use_mirror;
                return Task::perform(
                    async move {
                        service
                            .install_model_with_mirror(model, control, use_mirror)
                            .await
                    },
                    move |r| Message::Ai(AiMessage::Installed(receipt.clone(), r)),
                );
            }
            AiMessage::Pause(model) => {
                if let Some(control) = self.ai.downloads.get(&model) {
                    control.stop.store(true, Ordering::Relaxed);
                }
            }
            AiMessage::Uninstall(model) => {
                if self.ai.downloads.contains_key(&model) {
                    return Task::none();
                }
                let receipt = model.clone();
                let owner = self.library.as_ref().map(|l| l.info.id);
                let retain = self.retain_indexes;
                return Task::perform(
                    async move {
                        service.uninstall_model(model.clone()).await?;
                        if !retain && let Some(owner) = owner {
                            service.clear_ai_indexes(owner, model).await?;
                        }
                        Ok(())
                    },
                    move |r| Message::Ai(AiMessage::Installed(receipt.clone(), r)),
                );
            }
            AiMessage::Installed(model, result) => {
                self.ai.downloads.remove(&model);
                if let Err(e) = result {
                    self.error = Some(e)
                }
                return self.ai_message(AiMessage::Refresh);
            }
            AiMessage::Category(category) => {
                if self.library.is_none() {
                    return Task::none();
                }
                self.query = Query {
                    project: self.query.project,
                    category,
                    ..Query::default()
                };
                self.screen = Screen::Assets;
                return self.reset_query();
            }
            AiMessage::CategoryInput(value) => self.ai.category = value,
            AiMessage::SaveCategory => {
                let Some(library) = &self.library else {
                    return Task::none();
                };
                let id = library.info.id;
                let ids = self.selected_ids();
                let category = self.ai.category.clone();
                return Task::perform(
                    async move { service.set_category(id, ids, category).await },
                    move |r| Message::Ai(AiMessage::Stored(id, r)),
                );
            }
            AiMessage::People => {
                if self.library.is_none() {
                    return Task::none();
                }
                self.manager.panel = Some(manage::Panel::People);
                return self.refresh_organizer();
            }
            AiMessage::Person(person) => {
                self.ai.person_name = person.name.clone();
                self.ai.person = Some(person)
            }
            AiMessage::Target(person) => self.ai.target = Some(person),
            AiMessage::Name(name) => self.ai.person_name = name,
            AiMessage::PersonFilter(person) => {
                self.query = Query {
                    project: self.query.project,
                    person: Some(person),
                    ..Query::default()
                };
                self.screen = Screen::Assets;
                self.manager.panel = None;
                return self.reset_query();
            }
            AiMessage::Rename | AiMessage::Merge => {
                let Some(library) = &self.library else {
                    return Task::none();
                };
                let id = library.info.id;
                let Some(person) = &self.ai.person else {
                    return Task::none();
                };
                let source = person.id.clone();
                let name = self.ai.person_name.clone();
                let target = self.ai.target.as_ref().map(|p| p.id.clone());
                let merge = matches!(event, AiMessage::Merge);
                return Task::perform(
                    async move {
                        if merge {
                            service
                                .merge_people(id, source, target.ok_or(tr!(language, "请选择合并目标" => "Please select a person to merge into"))?)
                                .await
                        } else {
                            service.rename_person(id, source, name).await
                        }
                    },
                    move |r| Message::Ai(AiMessage::Stored(id, r)),
                );
            }
            AiMessage::Assign(face, person) => {
                let Some(library) = &self.library else {
                    return Task::none();
                };
                let id = library.info.id;
                return Task::perform(
                    async move { service.assign_face(id, face, person).await },
                    move |r| Message::Ai(AiMessage::Stored(id, r)),
                );
            }
            AiMessage::Stored(owner, result)
                if self.library.as_ref().is_some_and(|l| l.info.id == owner) =>
            {
                if let Err(e) = result {
                    self.error = Some(e)
                }
                return Task::batch([
                    self.refresh_catalog(),
                    self.refresh(),
                    self.refresh_details(),
                ]);
            }
            _ => {}
        }
        Task::none()
    }
    /// 刷新当前项目可用的人物选择项。
    pub(super) fn refresh_people_choices(&mut self) {
        let language = self.preferences.language;
        let people = &self.manager.organizer.people;
        self.ai.person = self
            .ai
            .person
            .as_ref()
            .and_then(|old| people.iter().find(|p| p.id == old.id))
            .map(|person| PersonChoice::person(language, person));
        self.ai.target = self
            .ai
            .target
            .as_ref()
            .and_then(|old| people.iter().find(|p| p.id == old.id))
            .map(|person| PersonChoice::person(language, person));
    }
    /// 按当前项目的真实分类配置构建侧栏分组。
    pub(super) fn sidebar_categories(&self) -> Vec<(String, u64)> {
        if !self.manager.organizer.ai_groups.is_empty() {
            return self
                .manager
                .organizer
                .ai_groups
                .iter()
                .map(|g| (self.group_label(g), g.count))
                .collect();
        }
        self.manager
            .organizer
            .ai_categories
            .iter()
            .filter(|(name, count)| *count > 0 && !name.trim().is_empty())
            .cloned()
            .collect()
    }
    /// 为当前人物列表启动有界头像读取，并绑定请求身份。
    pub(super) fn refresh_people_portraits(&mut self) -> Task<Message> {
        self.stop_people_portraits();
        if self.manager.panel != Some(manage::Panel::People) {
            return Task::none();
        }
        let Some(owner) = self.library.as_ref().map(|l| l.info.id) else {
            return Task::none();
        };
        self.ai.portrait_request = self.ai.portrait_request.wrapping_add(1);
        let request = self.ai.portrait_request;
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.ai.portrait_cancel = cancel.clone();
        self.ai.portraits.retain(|id, _| {
            self.manager
                .organizer
                .people
                .iter()
                .any(|p| p.id == *id && p.count > 0)
        });
        let people = self
            .manager
            .organizer
            .people
            .iter()
            .filter(|p| p.count > 0)
            .map(|p| p.id.clone())
            .collect::<Vec<_>>();
        let service = self.service.clone();
        let (task, handle) = Task::run(
            stream::iter(people)
                .map(move |person| {
                    let service = service.clone();
                    let cancel = cancel.clone();
                    async move {
                        let result = if cancel.load(Ordering::Acquire) {
                            Ok(None)
                        } else {
                            service.person_portrait(owner, person.clone()).await
                        };
                        (person, result)
                    }
                })
                .buffer_unordered(2),
            move |(person, result)| {
                Message::Ai(AiMessage::PortraitLoaded(owner, request, person, result))
            },
        )
        .abortable();
        self.ai.portrait_task = Some(handle.abort_on_drop());
        task
    }
    /// 取消人物头像读取并使旧回执失效。
    pub(super) fn stop_people_portraits(&mut self) {
        self.ai.portrait_cancel.store(true, Ordering::Release);
        if let Some(task) = self.ai.portrait_task.take() {
            // Drop pending read plans as well as rejecting late images. Otherwise
            // waiting for decoder memory could retain the previous library lease.
            task.abort();
        }
    }
    /// 构建人物分组管理面板及其操作入口。
    pub(super) fn people_panel(&self) -> Element<'_, Message> {
        let language = self.preferences.language;
        use crate::{
            icons::{Icon, icon},
            theme::*,
        };
        use iced::widget::{Space, row};
        use iced::{Alignment, Length};
        let choices: Vec<_> = self
            .manager
            .organizer
            .people
            .iter()
            .map(|person| PersonChoice::person(language, person))
            .collect();
        let can_rename = self.ai.person.is_some() && !self.ai.person_name.trim().is_empty();
        let can_recognize = self.library.is_some()
            && !self.switching
            && self
                .ai
                .models
                .iter()
                .any(|m| m.spec.id == "qwen35-4b" && m.installed);
        let can_merge = self
            .ai
            .person
            .as_ref()
            .zip(self.ai.target.as_ref())
            .is_some_and(|(a, b)| a.id != b.id);
        let mut view = column![
            form_card(
                column![
                    form_heading(
                        Icon::Person,
                        tr!(language, "修改人物名称" => "Rename person"),
                        tr!(language, "选择一个已识别的人物并设置新的名称。" => "Select a recognized person and enter a new name.")
                    ),
                    choice(choices.clone(), self.ai.person.clone(), |p| Message::Ai(
                        AiMessage::Person(p)
                    ))
                    .placeholder(tr!(language, "选择人物" => "Select person"))
                    .width(Length::Fill),
                    row![
                        text_input(tr!(language, "人物名称" => "Person name"), &self.ai.person_name)
                            .padding(11)
                            .style(input)
                            .on_input(|v| Message::Ai(AiMessage::Name(v)))
                            .width(Length::Fill),
                        form_action(Icon::Save, tr!(language, "保存名称" => "Save name"), Message::Ai(AiMessage::Rename), true)
                            .on_press_maybe(can_rename.then_some(Message::Ai(AiMessage::Rename)))
                    ]
                    .spacing(12)
                    .align_y(Alignment::Center)
                ]
                .spacing(16)
            ),
            form_card(
                column![
                    form_heading(
                        Icon::Network,
                        tr!(language, "合并人物" => "Merge people"),
                        tr!(language, "将当前选中的人物合并到另一个人物下，统一其所有照片。" => "Merge the selected person into another person to group their photos together.")
                    ),
                    row![
                        choice(choices, self.ai.target.clone(), |p| Message::Ai(
                            AiMessage::Target(p)
                        ))
                        .placeholder(tr!(language, "合并到人物" => "Merge into"))
                        .width(Length::Fill),
                        form_action(
                            Icon::Network,
                            tr!(language, "合并到所选人物" => "Merge into selected person"),
                            Message::Ai(AiMessage::Merge),
                            false
                        )
                        .on_press_maybe(can_merge.then_some(Message::Ai(AiMessage::Merge)))
                    ]
                    .spacing(12)
                    .align_y(Alignment::Center)
                ]
                .spacing(16)
            ),
            row![
                icon(Icon::Person, 20.0, MUTED),
                heading(tr!(language, "已识别的人物" => "Recognized people"), 15),
                chip(self.manager.organizer.people.len().to_string(), ICON),
                Space::new().width(Length::Fill),
                action(tr!(language,"Qwen 识别与分组"=>"Recognize with Qwen"), Message::Jobs(tasks::JobMessage::Queue(JobKind::QwenFaces)))
                    .on_press_maybe(can_recognize.then_some(Message::Jobs(tasks::JobMessage::Queue(JobKind::QwenFaces))))
            ]
            .spacing(10)
            .align_y(Alignment::Center)
        ]
        .spacing(18);
        if self.manager.organizer.people.is_empty() {
            view = view.push(muted(
                tr!(language, "暂无人物分组。安装 Qwen 后点击“Qwen 识别与分组”，识别完成后可命名、合并或纠正。" => "No people groups yet. Install Qwen and choose Recognize with Qwen, then name, merge or correct the results."),
            ));
        }
        for person in &self.manager.organizer.people {
            let thumbnail: Element<'_, Message> =
                if let Some(handle) = self.ai.portraits.get(&person.id) {
                    photo(Some(handle), iced::Length::Fixed(58.), 58.)
                } else {
                    iced::widget::container(crate::icons::icon(
                        crate::icons::Icon::Person,
                        22.,
                        crate::theme::MUTED,
                    ))
                    .center_x(58)
                    .center_y(58)
                    .style(crate::theme::pale)
                    .into()
                };
            view = view.push(
                iced::widget::button(
                    iced::widget::row![
                        thumbnail,
                        column![
                            heading(
                                if person.name.is_empty() || person.name == "未命名人物" {
                                    trf!(language, "未命名人物 · {}" => "Unnamed person · {}", &person.id[..person.id.len().min(8)])
                                } else {
                                    person.name.clone()
                                },
                                15
                            ),
                            row![
                                icon(Icon::Image, 14.0, MUTED),
                                muted(trf!(language, "{} 张" => "{} photos", person.count))
                            ]
                            .spacing(8)
                        ]
                        .spacing(8)
                        .width(Length::Fill),
                        icon(Icon::Right, 18.0, ICON)
                    ]
                    .spacing(12)
                    .align_y(iced::Alignment::Center),
                )
                .padding([8, 10])
                .width(iced::Length::Fill)
                .style(
                    if self.ai.person.as_ref().is_some_and(|p| p.id == person.id) {
                        selected_row
                    } else {
                        secondary
                    },
                )
                .on_press(Message::Ai(AiMessage::PersonFilter(person.id.clone()))),
            );
        }
        view.into()
    }
    /// 构建素材详情中的 AI 分类、描述与人工编辑区域。
    pub(super) fn ai_details(&self) -> Element<'_, Message> {
        let language = self.preferences.language;
        use crate::icons::Icon;
        use crate::theme::{BLUE, MUTED, TEXT};
        use iced::widget::{Space, row};
        use iced::{Alignment, Length};
        if let Some(project) = self.manager.details.projects.first().copied()
            && self.query.project != Some(project)
        {
            return column![muted(tr!(language,"此素材使用所属项目的分类和人物规则。"=>"This asset uses its project's categories and people.")),action(tr!(language,"进入所属项目整理"=>"Open owning project"),Message::Project(project))].spacing(10).into();
        }
        let video = self
            .viewer
            .selected
            .as_ref()
            .is_some_and(|a| moriq_app::is_video_format(&a.format));
        let mut view = column![detail_heading(
            Icon::Sparkles,
            tr!(language, "AI 分析与整理" => "AI analysis and organization")
        )]
        .spacing(12);
        if self
            .viewer
            .selected
            .as_ref()
            .is_none_or(|a| self.ai.category_version != Some(a.version_id))
        {
            return view.push(muted(tr!(language, "正在读取当前版本的分析内容…" => "Loading analysis for the current version..."))).into();
        }
        let details = &self.manager.details;
        let profile = &self.manager.organizer.ai_profile;
        let editable = !self.ai.content_saving;
        let mut classification = column![
            row![
                heading(tr!(language,"分类"=>"Classification"), 13),
                Space::new().width(Length::Fill),
                chip(
                    if details.manual_medium || details.manual_themes {
                        tr!(language,"人工"=>"Manual")
                    } else {
                        "AI"
                    },
                    BLUE
                )
            ]
            .align_y(Alignment::Center),
            muted(tr!(language,"媒介"=>"Medium")),
            choice_labels(
                profile
                    .categories
                    .iter()
                    .filter(|c| c.kind == AiCategoryKind::Medium)
                    .map(|c| c.id.clone())
                    .collect(),
                Some(self.ai.medium.clone()),
                move |id| profile.categories.iter().find(|c| &c.id == id).map_or_else(
                    || id.clone(),
                    |c| crate::categories::category_label(language, c)
                ),
                |id| Message::Ai(AiMessage::Medium(id)),
            )
            .width(Length::Fill),
            muted(tr!(language,"主题 · 可多选"=>"Themes · multiple selection"))
        ]
        .spacing(9);
        let mut themes = iced::widget::Row::new().spacing(6);
        for category in profile
            .categories
            .iter()
            .filter(|c| c.kind == AiCategoryKind::Theme && c.enabled)
        {
            let selected = self.ai.themes.contains(&category.id);
            themes = themes.push(
                iced::widget::button(
                    iced::widget::text(crate::categories::category_label(language, category))
                        .size(12),
                )
                .padding([6, 10])
                .style(if selected {
                    crate::theme::selected_row
                } else {
                    crate::theme::secondary
                })
                .on_press_maybe(
                    editable.then(|| Message::Ai(AiMessage::Theme(category.id.clone()))),
                ),
            );
        }
        classification = classification.push(themes.wrap());
        classification = classification.push(
            action(
                tr!(language,"保存分类"=>"Save classification"),
                Message::Ai(AiMessage::SaveClassification),
            )
            .width(Length::Fill)
            .on_press_maybe(
                (self.ai.classification_dirty && editable)
                    .then_some(Message::Ai(AiMessage::SaveClassification)),
            ),
        );
        let mut restore = iced::widget::Row::new().spacing(6);
        if details.manual_medium {
            restore = restore.push(
                action(
                    tr!(language,"重置媒介"=>"Reset medium"),
                    Message::Ai(AiMessage::EditContent(AiEdit::Medium(None))),
                )
                .padding([5, 8])
                .style(crate::theme::flat)
                .on_press_maybe(
                    editable.then_some(Message::Ai(AiMessage::EditContent(AiEdit::Medium(None)))),
                ),
            );
        }
        if details.manual_themes {
            restore = restore.push(
                action(
                    tr!(language,"重置主题"=>"Reset themes"),
                    Message::Ai(AiMessage::EditContent(AiEdit::Themes(None))),
                )
                .padding([5, 8])
                .style(crate::theme::flat)
                .on_press_maybe(
                    editable.then_some(Message::Ai(AiMessage::EditContent(AiEdit::Themes(None)))),
                ),
            );
        }
        if details.manual_medium || details.manual_themes {
            classification = classification.push(restore.wrap());
        }
        view = view.push(classification).push(separator());
        let mut editor = text_editor(&self.ai.description_editor).size(13).padding(10).height(132)
            .placeholder(tr!(language,"补充画面描述，不会覆盖备注…"=>"Describe this image; notes stay unchanged...")).style(crate::theme::editor);
        if editable {
            editor = editor.on_action(|a| Message::Ai(AiMessage::DescriptionAction(a)));
        }
        view = view
            .push(
                row![
                    heading(tr!(language,"画面描述"=>"Description"), 13),
                    Space::new().width(Length::Fill),
                    chip(
                        if details.manual_description {
                            tr!(language,"人工"=>"Manual")
                        } else {
                            "AI"
                        },
                        BLUE
                    )
                ]
                .align_y(Alignment::Center),
            )
            .push(editor)
            .push(
                action(
                    tr!(language,"保存描述"=>"Save description"),
                    Message::Ai(AiMessage::SaveDescription),
                )
                .width(Length::Fill)
                .on_press_maybe(
                    (self.ai.description_dirty && editable)
                        .then_some(Message::Ai(AiMessage::SaveDescription)),
                ),
            );
        if details.manual_description {
            view = view.push(
                action(
                    tr!(language,"恢复 AI 原文"=>"Restore AI description"),
                    Message::Ai(AiMessage::EditContent(AiEdit::Description(None))),
                )
                .style(crate::theme::flat)
                .on_press_maybe(editable.then_some(Message::Ai(AiMessage::EditContent(
                    AiEdit::Description(None),
                )))),
            );
        }
        let ai_tags: Vec<_> = details
            .ai_tags
            .iter()
            .filter(|t| !details.manual_tags.contains(t))
            .collect();
        view = view.push(separator()).push(
            row![
                heading(tr!(language,"识别标签"=>"AI tags"), 13),
                Space::new().width(Length::Fill),
                chip(ai_tags.len().to_string(), MUTED)
            ]
            .align_y(Alignment::Center),
        );
        if ai_tags.is_empty() {
            view = view.push(muted(
                tr!(language,"暂无待整理的 AI 标签"=>"No AI tags to organize"),
            ));
        } else {
            view = view.push(muted(tr!(language,"标签图标保留为人工，叉号移除。"=>"Tag icon keeps a manual tag; × removes it.")));
        }
        for tag in ai_tags {
            let promote = Message::Ai(AiMessage::EditContent(AiEdit::PromoteTag(tag.clone())));
            let reject = Message::Ai(AiMessage::EditContent(AiEdit::RejectTag(tag.clone())));
            let compact = |kind, event| {
                iced::widget::button(crate::icons::icon(kind, 15.0, MUTED))
                    .padding(6)
                    .style(crate::theme::flat)
                    .on_press_maybe(editable.then_some(event))
            };
            view = view.push(
                iced::widget::container(
                    row![
                        iced::widget::text(tag.as_str())
                            .size(13)
                            .width(Length::Fill)
                            .style(crate::theme::text_color(TEXT)),
                        iced::widget::tooltip(
                            compact(Icon::Tag, promote),
                            muted(tr!(language,"保留为人工标签"=>"Keep as manual tag")),
                            iced::widget::tooltip::Position::Left
                        ),
                        iced::widget::tooltip(
                            compact(Icon::Close, reject),
                            muted(tr!(language,"移除 AI 标签"=>"Remove AI tag")),
                            iced::widget::tooltip::Position::Left
                        )
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center),
                )
                .padding([5, 8])
                .width(Length::Fill)
                .style(crate::theme::pale),
            );
        }
        view = view.push(
            action(
                tr!(language,"恢复已移除的 AI 标签"=>"Restore removed AI tags"),
                Message::Ai(AiMessage::EditContent(AiEdit::ResetRejectedTags)),
            )
            .style(crate::theme::flat)
            .on_press_maybe(editable.then_some(Message::Ai(AiMessage::EditContent(
                AiEdit::ResetRejectedTags,
            )))),
        );
        if !self.manager.details.ocr.is_empty() {
            view = view.push(muted(self.manager.details.ocr.clone()));
        }
        if self.manager.details.ai_caption.is_empty() && self.manager.details.ocr.is_empty() {
            view = view.push(muted(if video {
                tr!(language, "视频支持人工分类，Qwen 图片分析适用于照片。" => "Videos support manual categories. Qwen image analysis applies to photos.")
            } else {
                tr!(language, "暂无识别描述，可在 AI 模型页运行分析。" => "No description yet. Run analysis on the AI Models page.")
            }));
        }
        view = view
            .push(separator())
            .push(heading(tr!(language,"传统分类 · 批量编辑"=>"Legacy category · batch edit"),12))
            .push(
                text_input(tr!(language, "人工分类（可批量修改）" => "Manual category (supports batch editing)"), &self.ai.category)
                    .style(crate::theme::input)
                    .padding(10)
                    .size(12)
                    .on_input(|v| Message::Ai(AiMessage::CategoryInput(v))),
            )
            .push(
                form_action(
                    Icon::Save,
                    tr!(language, "保存分类" => "Save category"),
                    Message::Ai(AiMessage::SaveCategory),
                    true,
                )
                .width(Length::Fill),
            );
        if !self.manager.details.faces.is_empty() {
            view = view.push(
                form_action(
                    Icon::Person,
                    tr!(language, "管理人物分组" => "Manage people groups"),
                    Message::Ai(AiMessage::People),
                    false,
                )
                .width(Length::Fill),
            );
        }
        let choices: Vec<_> = self
            .manager
            .organizer
            .people
            .iter()
            .map(|person| PersonChoice::person(language, person))
            .collect();
        for face in &self.manager.details.faces {
            let face_id = face.id.clone();
            view = view
                .push(muted(trf!(language, "人脸：{}" => "Face: {}",
                    if face.name.is_empty() {
                        tr!(language, "未命名" => "Unnamed")
                    } else {
                        &face.name
                    }
                )))
                .push(choice(
                    choices.clone(),
                    choices
                        .iter()
                        .find(|p| Some(&p.id) == face.person.as_ref())
                        .cloned(),
                    move |p| Message::Ai(AiMessage::Assign(face_id.clone(), Some(p.id))),
                ))
                .push(action(
                    tr!(language, "此人脸单独分组" => "Group this face separately"),
                    Message::Ai(AiMessage::Assign(face.id.clone(), None)),
                ));
        }
        view.into()
    }
    /// 构建当前素材的相似推荐区域。
    pub(super) fn ai_recommendations(&self) -> Element<'_, Message> {
        let language = self.preferences.language;
        column![
            heading(tr!(language, "图片近似查找" => "Image similarity"), 14),
            muted(tr!(language, "根据构图与尺寸查找近似图片。" => "Find similar images by composition and dimensions.")),
            action(
                tr!(language, "查找构图 / 尺寸近似" => "Find similar composition / dimensions"),
                Message::Manage(ManageMessage::Similar)
            )
        ].spacing(8).into()
    }
}
