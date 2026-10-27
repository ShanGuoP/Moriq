//! Library-owned classification editor; both navigation surfaces use the same projection.
use super::*;
use crate::components::*;
use crate::icons::{Icon, icon};
use crate::theme::{BLUE, MUTED, TEXT};
use iced::Length;
use iced::widget::{Column, Space, button, column, container, row, text_editor, text_input};

#[derive(Default)]
pub(super) struct State {
    pub owner: Option<LibraryId>,
    pub profile: AiProfile,
    pub selected: Option<String>,
    pub dirty: bool,
    pub busy: bool,
    pub request: u64,
    pub error: Option<String>,
    pub notice: Option<String>,
    pub definition_editor: text_editor::Content,
}
#[derive(Debug, Clone)]
pub enum CategoryMessage {
    Open,
    Select(String),
    Add,
    Name(String),
    Definition(text_editor::Action),
    Enabled(bool),
    Move(i8),
    Limit(u8),
    Save,
    SaveTemplate,
    ApplyTemplate,
    TemplateStored(LibraryId, u64, Result<AiProfile, String>),
    Saved(LibraryId, u64, Result<AiProfile, String>),
}
/// 把分类编辑器内部操作转换为应用消息。
fn message(event: CategoryMessage) -> Message {
    Message::Ai(AiMessage::Categories(event))
}
impl State {
    /// 将当前选中分类同步到编辑控件，保留对应草稿。
    fn sync_editor(&mut self) {
        let value = self
            .selected
            .as_ref()
            .and_then(|id| self.profile.categories.iter().find(|c| &c.id == id))
            .map_or("", |c| c.definition.as_str());
        self.definition_editor = text_editor::Content::with_text(value);
    }
}
/// 按分类身份取得当前配置中的显示名称。
pub(super) fn category_label(language: Language, c: &AiCategory) -> String {
    if AI_MEDIA
        .iter()
        .chain(AI_THEMES)
        .any(|(id, label)| *id == c.id && *label == c.label)
    {
        crate::i18n::category(language, &c.label).into()
    } else {
        c.label.clone()
    }
}
/// 为媒介或主题分类选择相应图标。
pub(super) fn category_icon(id: &str) -> crate::icons::Icon {
    use crate::icons::Icon;
    match id {
        "portrait" | "cosplay" => Icon::Person,
        "landscape" => Icon::Landscape,
        "architecture" => Icon::Building,
        "plant" => Icon::Plant,
        "food" => Icon::Food,
        "illustration" => Icon::Sparkles,
        "3d_render" => Icon::Cube,
        "screenshot" | "interface_document" => Icon::Raw,
        "photography" | "animal" => Icon::Image,
        _ => Icon::Tag,
    }
}
impl App {
    /// 维护分类编辑草稿、启用状态和保存回执，拒绝其他库的旧结果。
    pub(super) fn category_message(&mut self, event: CategoryMessage) -> Task<Message> {
        let language = self.preferences.language;
        let Some(owner) = self.library.as_ref().map(|l| l.info.id) else {
            return Task::none();
        };
        if matches!(event, CategoryMessage::Open) {
            if self.manager.organizer.ai_groups.is_empty() || self.switching {
                return self.refresh_organizer();
            }
            if self.ai.categories.busy && self.ai.categories.owner == Some(owner) {
                return Task::none();
            }
            let profile = self.manager.organizer.ai_profile.clone();
            let request = self.ai.categories.request.wrapping_add(1);
            self.ai.categories = State {
                owner: Some(owner),
                selected: profile.categories.first().map(|c| c.id.clone()),
                profile,
                request,
                ..State::default()
            };
            self.ai.categories.sync_editor();
            self.manager.panel = Some(manage::Panel::AiCategories);
            return Task::none();
        }
        if let CategoryMessage::TemplateStored(receipt, request, result) = event {
            if receipt != owner
                || self.ai.categories.owner != Some(owner)
                || request != self.ai.categories.request
            {
                return Task::none();
            }
            self.ai.categories.busy = false;
            match result {
                Ok(profile) => {
                    self.preferences.category_template = Some(profile);
                    self.ai.categories.notice=Some(tr!(language,"模板已保存。新建项目可套用；当前项目的修改仍需保存并同步。"=>"Template saved for new projects. Save and sync to apply edits to this project.").into());
                }
                Err(e) => self.ai.categories.error = Some(e),
            }
            return Task::none();
        }
        if let CategoryMessage::Saved(receipt, request, result) = event {
            if receipt != owner
                || self.ai.categories.owner != Some(owner)
                || request != self.ai.categories.request
            {
                return Task::none();
            }
            self.ai.categories.busy = false;
            match result {
                Ok(profile) => {
                    self.ai.themes.retain(|id| {
                        profile
                            .categories
                            .iter()
                            .any(|c| c.id == *id && c.kind == AiCategoryKind::Theme && c.enabled)
                    });
                    self.ai.categories.profile = profile;
                    self.ai.categories.sync_editor();
                    self.ai.categories.dirty = false;
                    self.ai.categories.error = None;
                }
                Err(error) => {
                    self.ai.categories.error = Some(error);
                    return Task::none();
                }
            }
            return Task::batch([
                self.refresh_catalog(),
                self.refresh(),
                self.refresh_details(),
            ]);
        }
        if self.ai.categories.owner != Some(owner) || self.ai.categories.busy || self.switching {
            return Task::none();
        }
        let state = &mut self.ai.categories;
        state.error = None;
        let index = state
            .selected
            .as_ref()
            .and_then(|id| state.profile.categories.iter().position(|c| &c.id == id));
        match event {
            CategoryMessage::ApplyTemplate => {
                if let Some(template) = &self.preferences.category_template {
                    match state.profile.with_template(template) {
                        Ok(profile) => {
                            state.profile = profile;
                            state.selected = state.profile.categories.first().map(|c| c.id.clone());
                            state.sync_editor();
                            state.dirty = true;
                            state.notice = None;
                        }
                        Err(e) => state.error = Some(e.into()),
                    }
                }
            }
            CategoryMessage::SaveTemplate => {
                if let Err(e) = state.profile.validate() {
                    state.error = Some(e.into());
                    return Task::none();
                }
                state.busy = true;
                state.request = state.request.wrapping_add(1);
                let request = state.request;
                let profile = state.profile.clone();
                let service = self.service.clone();
                return Task::perform(
                    async move { service.save_category_template(profile).await },
                    move |r| message(CategoryMessage::TemplateStored(owner, request, r)),
                );
            }
            CategoryMessage::Select(id) => {
                if state.profile.categories.iter().any(|c| c.id == id) {
                    state.selected = Some(id);
                    state.sync_editor();
                }
            }
            CategoryMessage::Add => {
                if state.profile.categories.len() >= 64 {
                    state.error=Some(tr!(language,"最多 64 个分类（含已禁用分类）"=>"At most 64 categories, including disabled ones").into());
                    return Task::none();
                }
                let base = tr!(language,"新分类"=>"New category");
                let mut label = base.to_string();
                let mut n = 2;
                while state.profile.categories.iter().any(|c| c.label == label) {
                    label = format!("{base} {n}");
                    n += 1;
                }
                let id = format!("theme_{}", AssetId::default().0.simple());
                state.profile.categories.push(AiCategory {
                    id: id.clone(),
                    kind: AiCategoryKind::Theme,
                    definition: label.clone(),
                    label,
                    enabled: true,
                });
                state.selected = Some(id);
                state.sync_editor();
                state.dirty = true;
            }
            CategoryMessage::Name(value) => {
                if let Some(i) = index {
                    if state.profile.categories[i].definition == state.profile.categories[i].label {
                        state.profile.categories[i].definition = value.clone();
                        state.sync_editor();
                    }
                    state.profile.categories[i].label = value;
                    state.dirty = true;
                }
            }
            CategoryMessage::Definition(action) => {
                if let Some(i) = index {
                    let changed = action.is_edit();
                    state.definition_editor.perform(action);
                    if changed {
                        state.profile.categories[i].definition = state.definition_editor.text();
                        state.dirty = true;
                    }
                }
            }
            CategoryMessage::Enabled(value) => {
                if let Some(i) = index {
                    state.profile.categories[i].enabled = value;
                    state.dirty = true;
                }
            }
            CategoryMessage::Move(delta) => {
                if let Some(i) = index {
                    let next = (i as i64 + i64::from(delta))
                        .clamp(0, state.profile.categories.len().saturating_sub(1) as i64)
                        as usize;
                    if i != next {
                        state.profile.categories.swap(i, next);
                        state.dirty = true;
                    }
                }
            }
            CategoryMessage::Limit(value) => {
                state.profile.max_themes = value;
                state.dirty = true;
            }
            CategoryMessage::Save => {
                if let Err(error) = state.profile.validate() {
                    state.error = Some(error.into());
                    return Task::none();
                }
                state.busy = true;
                state.request = state.request.wrapping_add(1);
                state.error = None;
                let request = state.request;
                let profile = state.profile.clone();
                let service = self.service.clone();
                return Task::perform(
                    async move { service.save_ai_profile(owner, profile).await },
                    move |r| message(CategoryMessage::Saved(owner, request, r)),
                );
            }
            _ => {}
        }
        Task::none()
    }
    /// 构建分类名称、定义和主题限制的编辑区。
    pub(super) fn category_editor(&self) -> Element<'_, Message> {
        let language = self.preferences.language;
        let state = &self.ai.categories;
        let narrow = self.viewport.width < 780.0;
        let height = (self.viewport.height - 495.0).clamp(150.0, 320.0);
        let mut items = Column::new().spacing(4).width(Length::Fill);
        for c in &state.profile.categories {
            let selected = state.selected.as_ref() == Some(&c.id);
            let mut line = row![
                icon(
                    category_icon(&c.id),
                    17.0,
                    if selected { BLUE } else { MUTED }
                ),
                iced::widget::text(category_label(language, c))
                    .size(13)
                    .style(crate::theme::text_color(if c.enabled {
                        TEXT
                    } else {
                        MUTED
                    }))
                    .width(Length::Fill)
            ]
            .spacing(9)
            .align_y(iced::Alignment::Center);
            if !c.enabled {
                line = line.push(chip(tr!(language,"已禁用"=>"Disabled"), MUTED));
            }
            items = items.push(
                button(line)
                    .width(Length::Fill)
                    .padding([10, 10])
                    .style(if selected {
                        crate::theme::selected_row
                    } else {
                        crate::theme::flat
                    })
                    .on_press_maybe(
                        (!state.busy).then(|| message(CategoryMessage::Select(c.id.clone()))),
                    ),
            );
        }
        let enabled = state
            .profile
            .categories
            .iter()
            .filter(|c| c.enabled)
            .count();
        let list = column![
            row![
                heading(tr!(language,"分类列表"=>"Categories"), 14),
                Space::new().width(Length::Fill),
                muted(format!("{enabled} / {}", state.profile.categories.len()))
            ]
            .align_y(iced::Alignment::Center),
            scroll(items)
                .id("ai-category-list")
                .height(if narrow { 150.0 } else { height }),
            form_action(
                Icon::Plus,
                tr!(language,"新增主题分类"=>"Add theme"),
                message(CategoryMessage::Add),
                false
            )
            .width(Length::Fill)
            .on_press_maybe(
                (!state.busy && state.profile.categories.len() < 64)
                    .then_some(message(CategoryMessage::Add))
            )
        ]
        .spacing(12);
        let mut form = Column::new().spacing(16).width(Length::Fill);
        if let Some((index, c)) = state.selected.as_ref().and_then(|id| {
            state
                .profile
                .categories
                .iter()
                .enumerate()
                .find(|(_, c)| &c.id == id)
        }) {
            let kind = if c.kind == AiCategoryKind::Medium {
                tr!(language,"固定媒介"=>"Fixed medium")
            } else {
                tr!(language,"主题分类"=>"Theme")
            };
            form = form.push(
                row![
                    heading(tr!(language,"分类设置"=>"Category settings"), 16),
                    Space::new().width(Length::Fill),
                    chip(kind, BLUE)
                ]
                .align_y(iced::Alignment::Center),
            );
            form = form.push(column![
                heading(tr!(language,"分类名称"=>"Name"),13),
                text_input(tr!(language,"为分类命名"=>"Name this category"),&c.label)
                    .padding(10).size(14).style(crate::theme::input)
                    .on_input_maybe((!state.busy).then_some(|s| message(CategoryMessage::Name(s)))),
                muted(tr!(language,"同步显示在智能整理卡片与素材侧栏。"=>"Shown on organize cards and in the asset sidebar."))
            ].spacing(7));
            let mut editor = text_editor(&state.definition_editor).size(14).padding(10).height(112)
                .placeholder(tr!(language,"描述哪些内容应归入此分类，可补充与其他分类的区别。"=>"Describe what belongs here and how it differs from other categories."))
                .style(crate::theme::editor);
            if !state.busy {
                editor = editor.on_action(|a| message(CategoryMessage::Definition(a)));
            }
            form = form.push(column![
                row![heading(tr!(language,"识别说明"=>"Recognition definition"),13),
                    Space::new().width(Length::Fill),muted(format!("{} / 512",c.definition.chars().count()))],
                editor,
                muted(tr!(language,"此说明用于引导 Qwen 归类，不会修改素材原文件。"=>"Guides Qwen classification without changing original files."))
            ].spacing(7));
            form = form.push(separator());
            form = form.push(
                iced::widget::checkbox(c.enabled)
                    .label(tr!(language,"参与识别并显示在卡片和侧栏"=>"Enable recognition and show on cards and sidebar"))
                    .text_size(13)
                    .on_toggle_maybe(
                        (!state.busy).then_some(|v| message(CategoryMessage::Enabled(v))),
                    ),
            );
            form = form.push(muted(tr!(language,"关闭后不再参与新的自动归类，同时隐藏入口；已有识别记录、原片和人工标签保留。保存并同步后生效。"=>"When disabled, this category is excluded from new recognition and hidden from navigation. Existing records, originals and manual tags are preserved. Save and sync to apply.")));
            form = form.push(
                row![
                    muted(tr!(language,"显示顺序"=>"Display order")),
                    Space::new().width(Length::Fill),
                    action(
                        tr!(language,"上移"=>"Up"),
                        message(CategoryMessage::Move(-1))
                    )
                    .on_press_maybe(
                        (!state.busy && index > 0).then_some(message(CategoryMessage::Move(-1)))
                    ),
                    action(
                        tr!(language,"下移"=>"Down"),
                        message(CategoryMessage::Move(1))
                    )
                    .on_press_maybe(
                        (!state.busy && index + 1 < state.profile.categories.len())
                            .then_some(message(CategoryMessage::Move(1)))
                    )
                ]
                .spacing(8)
                .align_y(iced::Alignment::Center),
            );
        }
        let panes: Element<'_, Message> = if narrow {
            column![form_card(list), form_card(form)].spacing(16).into()
        } else {
            row![
                container(form_card(list)).width(240),
                container(form_card(form)).width(Length::Fill)
            ]
            .spacing(18)
            .into()
        };
        let tools = row![
            action(
                tr!(language,"保存为模板"=>"Save as template"),
                message(CategoryMessage::SaveTemplate)
            )
            .on_press_maybe((!state.busy).then_some(message(CategoryMessage::SaveTemplate))),
            action(
                tr!(language,"套用已存模板"=>"Apply saved template"),
                message(CategoryMessage::ApplyTemplate)
            )
            .on_press_maybe(
                (!state.busy && self.preferences.category_template.is_some())
                    .then_some(message(CategoryMessage::ApplyTemplate))
            )
        ]
        .spacing(10)
        .wrap();
        let mut view=column![tools,muted(tr!(language,"分类配置仅影响当前整理空间。模板可复制到新项目，已有项目不会自动变化。"=>"Categories affect this workspace only. Templates can be copied into new projects; existing projects stay independent.")),panes,
            muted(tr!(language,"修改识别说明或启用状态后，旧分类任务会停止；需手动重新整理以更新已有素材。"=>"Changing definitions or enabled categories stops old classification jobs. Run Organize again to update existing assets."))].spacing(14);
        if let Some(notice) = &state.notice {
            view = view.push(muted(notice));
        }
        view.into()
    }
    /// 构建分类表单的模板、取消和保存操作。
    pub(super) fn category_footer(&self) -> Element<'_, Message> {
        let language = self.preferences.language;
        let state = &self.ai.categories;
        let status = state.error.clone().unwrap_or_else(|| {
            if state.busy {
                tr!(language,"正在保存…"=>"Saving...").into()
            } else if state.dirty {
                tr!(language,"有未保存的修改"=>"Unsaved changes").into()
            } else {
                tr!(language,"已同步到当前整理空间"=>"Synced with this workspace").into()
            }
        });
        let controls = row![
            muted(tr!(language,"每张图片最多主题数"=>"Themes per image")),
            choice(
                (1..=8).collect::<Vec<u8>>(),
                Some(state.profile.max_themes),
                |n| message(CategoryMessage::Limit(n))
            )
        ]
        .spacing(10)
        .align_y(iced::Alignment::Center);
        let save = form_action(
            Icon::Save,
            tr!(language,"保存并同步"=>"Save and sync"),
            message(CategoryMessage::Save),
            true,
        )
        .on_press_maybe((state.dirty && !state.busy).then_some(message(CategoryMessage::Save)));
        let actions: Element<'_, Message> = if self.viewport.width < 780.0 {
            column![controls, save.width(Length::Fill)]
                .spacing(10)
                .into()
        } else {
            row![controls, Space::new().width(Length::Fill), save]
                .spacing(12)
                .align_y(iced::Alignment::Center)
                .into()
        };
        column![separator(), muted(status), actions]
            .spacing(10)
            .into()
    }
    /// 把分类分组类型转换为当前语言的标题。
    pub(super) fn group_label(&self, group: &AiGroup) -> String {
        let builtin = AI_MEDIA
            .iter()
            .chain(AI_THEMES)
            .find(|(id, _)| *id == group.id);
        if builtin.is_some_and(|(_, label)| *label == group.label) || group.id == "_unclassified" {
            crate::i18n::category(self.preferences.language, &group.label).into()
        } else {
            group.label.clone()
        }
    }
}
