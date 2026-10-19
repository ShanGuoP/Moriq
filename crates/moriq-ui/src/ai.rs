use super::*;
use crate::components::*;
use iced::Length;
use iced::widget::{column, container, progress_bar, row, scrollable, text_input};
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct AiState {
    pub loaded: bool,
    pub loading: bool,
    pub models: Vec<moriq_app::ModelInfo>,
    pub hardware: Option<moriq_app::Hardware>,
    pub downloads: HashMap<String, moriq_app::DownloadControl>,
    pub request: u64,
    pub search_generation: u64,
    pub category: String,
    pub category_version: Option<AssetVersionId>,
    pub person_name: String,
    pub person: Option<PersonChoice>,
    pub target: Option<PersonChoice>,
    pub recommendations: Option<(AssetVersionId, Vec<(AssetId, String)>)>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonChoice {
    pub id: String,
    pub name: String,
    label: String,
}
impl PersonChoice {
    fn person(p: &Person) -> Self {
        let name = p.name.clone();
        let label = if name.is_empty() || name == "未命名人物" {
            format!(
                "未命名人物 · {} · {} 张",
                &p.id[..p.id.len().min(8)],
                p.count
            )
        } else {
            format!("{} · {} 张", name, p.count)
        };
        Self {
            id: p.id.clone(),
            name,
            label,
        }
    }
}
impl std::fmt::Display for PersonChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}
#[derive(Debug, Clone)]
pub enum AiMessage {
    Refresh,
    Loaded(Result<(Vec<moriq_app::ModelInfo>, moriq_app::Hardware), String>),
    Install(String),
    Pause(String),
    Uninstall(String),
    Installed(String, Result<(), String>),
    Semantic,
    ImageSearch,
    SearchLoaded(LibraryId, u64, Result<Vec<AssetId>, String>),
    Category(String),
    CategoryInput(String),
    SaveCategory,
    People,
    Person(PersonChoice),
    Target(PersonChoice),
    Name(String),
    Rename,
    Merge,
    Assign(String, Option<String>),
    PersonFilter(String),
    Stored(LibraryId, Result<(), String>),
    Recommended(
        LibraryId,
        AssetId,
        AssetVersionId,
        Result<Vec<(AssetId, String)>, String>,
    ),
}
impl App {
    pub(super) fn ai_message(&mut self, event: AiMessage) -> Task<Message> {
        let service = self.service.clone();
        match event {
            AiMessage::Recommended(owner, asset, version, result)
                if self.library.as_ref().is_some_and(|l| l.info.id == owner)
                    && self
                        .selected
                        .as_ref()
                        .is_some_and(|a| a.id == asset && a.version_id == version) =>
            {
                match result {
                    Ok(items) => self.ai.recommendations = Some((version, items)),
                    Err(e) => tracing::warn!(%e,"recommendation lookup deferred"),
                }
            }
            AiMessage::Refresh => {
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
                return Task::perform(
                    async move { service.install_model(model, control).await },
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
            AiMessage::Semantic | AiMessage::ImageSearch => {
                let Some(library) = &self.library else {
                    return Task::none();
                };
                let owner = library.info.id;
                let image = matches!(event, AiMessage::ImageSearch);
                let selected = self.selected.as_ref().map(|a| a.id);
                if image && selected.is_none() {
                    return Task::none();
                }
                let text = self.query.text.clone();
                self.ai.request = self.ai.request.wrapping_add(1);
                let request = self.ai.request;
                self.ai.search_generation = self.generation;
                self.status = "正在本机计算相似度…".into();
                return Task::perform(
                    async move {
                        if image {
                            service
                                .search_by_image(owner, selected.ok_or("请选择图片")?)
                                .await
                        } else {
                            service.semantic_search(owner, text).await
                        }
                    },
                    move |r| Message::Ai(AiMessage::SearchLoaded(owner, request, r)),
                );
            }
            AiMessage::SearchLoaded(owner, request, result)
                if self.library.as_ref().is_some_and(|l| l.info.id == owner)
                    && request == self.ai.request
                    && self.generation == self.ai.search_generation =>
            {
                match result {
                    Ok(ids) => {
                        self.query.semantic_ids = ids;
                        self.query.semantic_active = true;
                        self.query.text.clear();
                        self.screen = Screen::Assets;
                        self.manager.panel = None;
                        self.status = "按语义相似度排序".into();
                        return self.reset_query();
                    }
                    Err(e) => self.error = Some(e),
                }
            }
            AiMessage::Category(category) => {
                self.query = Query {
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
                                .merge_people(id, source, target.ok_or("请选择合并目标")?)
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
                    self.refresh_organizer(),
                    self.refresh(),
                    self.refresh_details(),
                ]);
            }
            _ => {}
        }
        Task::none()
    }
    pub(super) fn real_models(&self) -> Element<'_, Message> {
        let mut content = column![
            heading("本地 AI", 24),
            muted("模型由你选择安装。分析、文字识别和相似度计算在本机执行。")
        ]
        .spacing(14);
        content = content
            .push(
                iced::widget::checkbox(self.on_demand_models)
                    .label("按需加载，闲置后释放模型")
                    .on_toggle(|_| Message::ToggleModelPolicy),
            )
            .push(
                iced::widget::checkbox(self.retain_indexes)
                    .label("卸载模型时保留分析结果和索引")
                    .on_toggle(|_| Message::ToggleRetainIndexes),
            );
        if let Some(hardware) = &self.ai.hardware {
            content = content.push(muted(format!(
                "{} · {} 核 · 可用内存 {} MiB",
                hardware.provider, hardware.logical_cpus, hardware.available_mib
            )));
        }
        for model in &self.ai.models {
            if !self.model_search.is_empty()
                && !format!("{} {}", model.spec.name, model.spec.id)
                    .to_lowercase()
                    .contains(&self.model_search.to_lowercase())
            {
                continue;
            }
            let id = model.spec.id.clone();
            let mut card = column![
                heading(model.spec.name.clone(), 17),
                muted(format!(
                    "{} · {:.0} MiB · 建议内存 {} MiB",
                    model.spec.license,
                    model.bytes as f64 / 1048576.,
                    model.spec.minimum_memory_mib
                )),
                muted(model.spec.source.clone())
            ]
            .spacing(9);
            if let Some(control) = self.ai.downloads.get(&id) {
                let done = control.downloaded.load(Ordering::Relaxed);
                let total = control.total.load(Ordering::Relaxed).max(1);
                card = card
                    .push(progress_bar(
                        0.0..=1.0,
                        (done as f64 / total as f64).min(1.) as f32,
                    ))
                    .push(muted(format!(
                        "下载 {:.1} / {:.1} MiB",
                        done as f64 / 1048576.,
                        total as f64 / 1048576.
                    )))
                    .push(action("暂停并保留断点", Message::Ai(AiMessage::Pause(id))));
            } else if model.installed {
                card = card.push(
                    row![
                        action(
                            "分析 / 重新整理全库",
                            Message::Jobs(tasks::JobMessage::Queue(format!("analysis:{id}")))
                        ),
                        action(
                            if self.retain_indexes {
                                "卸载模型（保留分析结果）"
                            } else {
                                "卸载模型并清除自动分析索引"
                            },
                            Message::Ai(AiMessage::Uninstall(id))
                        )
                    ]
                    .spacing(8),
                );
            } else {
                card = card.push(action(
                    if model.resumable {
                        "继续下载安装"
                    } else {
                        "下载并安装"
                    },
                    Message::Ai(AiMessage::Install(id)),
                ));
            }
            content = content.push(
                container(card)
                    .padding(18)
                    .width(Length::Fill)
                    .style(crate::theme::panel),
            );
        }
        content = content.push(heading("自动分类", 18));
        for (category, count) in &self.manager.organizer.ai_categories {
            content = content.push(action(
                &format!("{category} · {count}"),
                Message::Ai(AiMessage::Category(category.clone())),
            ));
        }
        content = content.push(action("人物管理与人工纠正", Message::Ai(AiMessage::People)));
        scrollable(content.padding(24)).height(Length::Fill).into()
    }
    pub(super) fn refresh_people_choices(&mut self) {
        let people = &self.manager.organizer.people;
        self.ai.person = self
            .ai
            .person
            .as_ref()
            .and_then(|old| people.iter().find(|p| p.id == old.id))
            .map(PersonChoice::person);
        self.ai.target = self
            .ai
            .target
            .as_ref()
            .and_then(|old| people.iter().find(|p| p.id == old.id))
            .map(PersonChoice::person);
    }
    pub(super) fn people_panel(&self) -> Element<'_, Message> {
        let choices: Vec<_> = self
            .manager
            .organizer
            .people
            .iter()
            .map(PersonChoice::person)
            .collect();
        let mut view = column![
            muted("选择人物命名或合并；图片详情内可纠正单个人脸的归属。"),
            choice(choices.clone(), self.ai.person.clone(), |p| Message::Ai(
                AiMessage::Person(p)
            )),
            text_input("人物名称", &self.ai.person_name)
                .on_input(|v| Message::Ai(AiMessage::Name(v))),
            action("保存名称", Message::Ai(AiMessage::Rename)),
            choice(choices, self.ai.target.clone(), |p| Message::Ai(
                AiMessage::Target(p)
            ))
            .placeholder("合并到人物"),
            action("合并到所选人物", Message::Ai(AiMessage::Merge))
        ]
        .spacing(10);
        for person in &self.manager.organizer.people {
            view = view.push(action(
                &PersonChoice::person(person).to_string(),
                Message::Ai(AiMessage::PersonFilter(person.id.clone())),
            ));
        }
        view.into()
    }
    pub(super) fn ai_details(&self) -> Element<'_, Message> {
        let mut view = column![
            heading("AI 分析与整理", 14),
            muted(self.manager.details.ai_caption.clone()),
            muted(self.manager.details.ocr.clone()),
            text_input("人工分类（可批量修改）", &self.ai.category)
                .on_input(|v| Message::Ai(AiMessage::CategoryInput(v))),
            action("保存分类", Message::Ai(AiMessage::SaveCategory)),
            action("以此图搜索相似图片", Message::Ai(AiMessage::ImageSearch)),
            action("管理人物", Message::Ai(AiMessage::People))
        ]
        .spacing(8);
        let choices: Vec<_> = self
            .manager
            .organizer
            .people
            .iter()
            .map(PersonChoice::person)
            .collect();
        for face in &self.manager.details.faces {
            let face_id = face.id.clone();
            view = view
                .push(muted(format!(
                    "人脸：{}",
                    if face.name.is_empty() {
                        "未命名"
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
                    "此人脸单独分组",
                    Message::Ai(AiMessage::Assign(face.id.clone(), None)),
                ));
        }
        view.into()
    }
    pub(super) fn ai_recommendations(&self) -> Element<'_, Message> {
        let mut view = column![
            heading("内容相似推荐", 14),
            muted("根据已建立的图片内容索引推荐。"),
            action("以此图搜索更多", Message::Ai(AiMessage::ImageSearch)),
            action(
                "查找构图 / 尺寸近似",
                Message::Manage(ManageMessage::Similar)
            )
        ]
        .spacing(8);
        let mut found = false;
        if let Some((version, items)) = &self.ai.recommendations {
            if self
                .selected
                .as_ref()
                .is_some_and(|a| a.version_id == *version)
                && !items.is_empty()
            {
                found = true;
                for (id, name) in items {
                    view = view.push(action(
                        name,
                        Message::Journal(journal::JournalMessage::OpenAsset(*id)),
                    ));
                }
            }
        }

        if !found {
            view = view.push(muted(
                "暂无推荐。可在本地 AI 中安装并运行中文图片检索模型，为图库建立内容索引。",
            ));
        }
        view.into()
    }
}
