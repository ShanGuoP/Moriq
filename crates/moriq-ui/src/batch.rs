use super::*;
use crate::components::*;
use iced::widget::{checkbox, column, text_input};
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectTarget {
    id: Option<ProjectId>,
    label: String,
}
impl std::fmt::Display for ProjectTarget {
    /// 输出批量操作目标项目的显示标签。
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}
#[derive(Default)]
pub(super) struct BatchState {
    all: bool,
    favorite: String,
    rating: String,
    notes: String,
    replace_notes: bool,
    category: String,
    add_tags: String,
    remove_tags: String,
    project: Option<ProjectTarget>,
    owner: Option<LibraryId>,
}
#[derive(Debug, Clone)]
pub enum BatchMessage {
    Open,
    All(bool),
    Favorite(String),
    Rating(String),
    Notes(String),
    ReplaceNotes(bool),
    Category(String),
    AddTags(String),
    RemoveTags(String),
    Project(ProjectTarget),
    Apply,
    Stored(LibraryId, Result<(), String>),
}
impl App {
    /// 维护批量编辑草稿、选择范围和提交回执。
    pub(super) fn batch_message(&mut self, event: BatchMessage) -> Task<Message> {
        let language = self.preferences.language;
        match event {
            BatchMessage::Open => {
                self.batch = BatchState {
                    owner: self.library.as_ref().map(|l| l.info.id),
                    ..Default::default()
                };
                self.manager.panel = Some(manage::Panel::Batch)
            }
            BatchMessage::All(value) => self.batch.all = value,
            BatchMessage::Favorite(value) => self.batch.favorite = value,
            BatchMessage::Rating(value) => self.batch.rating = value,
            BatchMessage::Notes(value) => self.batch.notes = value,
            BatchMessage::ReplaceNotes(value) => self.batch.replace_notes = value,
            BatchMessage::Category(value) => self.batch.category = value,
            BatchMessage::AddTags(value) => self.batch.add_tags = value,
            BatchMessage::RemoveTags(value) => self.batch.remove_tags = value,
            BatchMessage::Project(value) => self.batch.project = Some(value),
            BatchMessage::Apply => {
                let Some(library) = &self.library else {
                    return Task::none();
                };
                let id = library.info.id;
                if self.batch.owner != Some(id) {
                    return Task::none();
                }
                let split = |s: &str| {
                    s.split([',', '，'])
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                };
                let patch = BatchPatch {
                    favorite: match self.batch.favorite.as_str() {
                        "收藏" => Some(true),
                        "取消收藏" => Some(false),
                        _ => None,
                    },
                    rating: self.batch.rating.parse::<u8>().ok(),
                    notes: self.batch.replace_notes.then(|| self.batch.notes.clone()),
                    category: (!self.batch.category.trim().is_empty())
                        .then(|| self.batch.category.trim().into()),
                    add_tags: split(&self.batch.add_tags),
                    remove_tags: split(&self.batch.remove_tags),
                    project: self.batch.project.as_ref().and_then(|p| p.id),
                };
                let scope = if self.batch.all {
                    BatchScope::Query(Box::new(self.query.clone()))
                } else {
                    BatchScope::Assets(self.selected_ids())
                };
                let service = self.service.clone();
                return Task::perform(
                    async move { service.batch_edit(id, scope, patch).await },
                    move |r| Message::Batch(BatchMessage::Stored(id, r)),
                );
            }
            BatchMessage::Stored(owner, result)
                if self.library.as_ref().is_some_and(|l| l.info.id == owner) =>
            {
                match result {
                    Ok(()) => {
                        self.manager.panel = Some(manage::Panel::Tasks);
                        self.status = tr!(language, "批量修改已加入后台任务，可暂停并在重启后继续" => "Batch edits queued. You can pause and resume after restarting.").into()
                    }
                    Err(e) => self.error = Some(e),
                }
                return self.refresh_organizer();
            }
            _ => {}
        }
        Task::none()
    }
    /// 构建批量编辑表单及确认入口。
    pub(super) fn batch_panel(&self) -> Element<'_, Message> {
        let language = self.preferences.language;
        let unchanged = ProjectTarget {
            id: None,
            label: tr!(language, "保持项目归属" => "Keep project membership").into(),
        };
        let mut projects = vec![unchanged.clone()];
        if let Some(library) = &self.library {
            projects.extend(library.info.projects.iter().map(|p| ProjectTarget {
                id: Some(p.id),
                label: manage::project_label(&library.info.projects, p.id),
            }));
        }
        let project = choice(
            projects,
            Some(self.batch.project.clone().unwrap_or(unchanged)),
            |v| Message::Batch(BatchMessage::Project(v)),
        );
        let scope = trf!(language, "当前选择 {} 项；全部查询结果 {} 项。任务创建时保存图片范围快照，之后新增素材不会自动加入。" => "{} items selected; {} results in total. The task saves a snapshot of the scope; new assets will not be included.",
            self.selected_ids().len(),
            self.total,
        );
        let favorite = if self.batch.favorite.is_empty() {
            "保持收藏状态".into()
        } else {
            self.batch.favorite.clone()
        };
        let rating = if self.batch.rating.is_empty() {
            "保持评分".into()
        } else {
            self.batch.rating.clone()
        };
        column![
            project,
            muted(scope),
            checkbox(self.batch.all)
                .label(tr!(language, "选择全部查询结果（跨越所有分页）" => "Select all results (across all pages)"))
                .on_toggle(|v| Message::Batch(BatchMessage::All(v))),
            localized_choice(language,
                vec!["保持收藏状态".into(), "收藏".into(), "取消收藏".into()],
                Some(favorite),
                |v| Message::Batch(BatchMessage::Favorite(v)),
            ),
            localized_choice(language,
                vec![
                    "保持评分".into(),
                    "0".into(),
                    "1".into(),
                    "2".into(),
                    "3".into(),
                    "4".into(),
                    "5".into()
                ],
                Some(rating),
                |v| Message::Batch(BatchMessage::Rating(v)),
            ),
            text_input(tr!(language, "追加标签，逗号分隔" => "Add tags, separated by commas"), &self.batch.add_tags)
                .style(crate::theme::input)
                .padding(10)
                .on_input(|v| Message::Batch(BatchMessage::AddTags(v))),
            text_input(tr!(language, "移除标签，逗号分隔" => "Remove tags, separated by commas"), &self.batch.remove_tags)
                .style(crate::theme::input)
                .padding(10)
                .on_input(|v| Message::Batch(BatchMessage::RemoveTags(v))),
            text_input(tr!(language, "人工分类（留空保持原值）" => "Manual category (leave blank to keep)"), &self.batch.category)
                .style(crate::theme::input)
                .padding(10)
                .on_input(|v| Message::Batch(BatchMessage::Category(v))),
            checkbox(self.batch.replace_notes)
                .label(tr!(language, "替换备注（可留空清除）" => "Replace notes (leave blank to clear)"))
                .on_toggle(|v| Message::Batch(BatchMessage::ReplaceNotes(v))),
            text_input(tr!(language, "新的备注" => "New notes"), &self.batch.notes)
                .style(crate::theme::input)
                .padding(10)
                .on_input(|v| Message::Batch(BatchMessage::Notes(v))),
            muted(tr!(language, "以上字段一起保存；每个素材的修改在单个事务内完成。" => "These fields are saved together. Each asset is updated in a single transaction.")),
            action(tr!(language, "应用并加入后台任务" => "Apply and queue task"), Message::Batch(BatchMessage::Apply)),
        ]
        .spacing(12)
        .into()
    }
}
