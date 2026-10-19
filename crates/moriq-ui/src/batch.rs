use super::*;
use crate::components::*;
use iced::widget::{checkbox, column, text_input};
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectTarget {
    id: Option<ProjectId>,
    label: String,
}
impl std::fmt::Display for ProjectTarget {
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
    pub(super) fn batch_message(&mut self, event: BatchMessage) -> Task<Message> {
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
                    BatchScope::Query(self.query.clone())
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
                        self.status = "批量修改已加入后台任务，可暂停并在重启后继续".into()
                    }
                    Err(e) => self.error = Some(e),
                }
                return self.refresh_organizer();
            }
            _ => {}
        }
        Task::none()
    }
    pub(super) fn batch_panel(&self) -> Element<'_, Message> {
        let unchanged = ProjectTarget {
            id: None,
            label: "保持项目归属".into(),
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
        column![project,muted(format!("当前选择 {} 项；全部查询结果 {} 项。任务创建时保存图片范围快照，之后新增素材不会自动加入。",self.selected_ids().len(),self.total)),checkbox(self.batch.all).label("选择全部查询结果（跨越所有分页）").on_toggle(|v|Message::Batch(BatchMessage::All(v))),choice(vec!["保持收藏状态".to_string(),"收藏".into(),"取消收藏".into()],Some(if self.batch.favorite.is_empty(){"保持收藏状态".into()}else{self.batch.favorite.clone()}),|v|Message::Batch(BatchMessage::Favorite(v))),choice(vec!["保持评分".to_string(),"0".into(),"1".into(),"2".into(),"3".into(),"4".into(),"5".into()],Some(if self.batch.rating.is_empty(){"保持评分".into()}else{self.batch.rating.clone()}),|v|Message::Batch(BatchMessage::Rating(v))),text_input("追加标签，逗号分隔",&self.batch.add_tags).on_input(|v|Message::Batch(BatchMessage::AddTags(v))),text_input("移除标签，逗号分隔",&self.batch.remove_tags).on_input(|v|Message::Batch(BatchMessage::RemoveTags(v))),text_input("人工分类（留空保持原值）",&self.batch.category).on_input(|v|Message::Batch(BatchMessage::Category(v))),checkbox(self.batch.replace_notes).label("替换备注（可留空清除）").on_toggle(|v|Message::Batch(BatchMessage::ReplaceNotes(v))),text_input("新的备注",&self.batch.notes).on_input(|v|Message::Batch(BatchMessage::Notes(v))),muted("以上字段一起保存；每个素材的修改在单个事务内完成。"),action("应用并加入后台任务",Message::Batch(BatchMessage::Apply))].spacing(12).into()
    }
}
