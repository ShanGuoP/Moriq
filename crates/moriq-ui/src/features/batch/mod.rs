//! Batch editing owns its draft and receipts; it receives no root App or Service.
use moriq_domain::{AssetId, BatchPatch, BatchScope, Language, LibraryId, ProjectId, Query};
#[cfg(test)]
mod tests;
mod view;

type Owner = (LibraryId, Option<ProjectId>);
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectTarget {
    id: Option<ProjectId>,
    label: String,
}
impl ProjectTarget {
    pub fn new(id: ProjectId, label: String) -> Self {
        Self {
            id: Some(id),
            label,
        }
    }
}
impl std::fmt::Display for ProjectTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}
pub(crate) struct Context<'a> {
    pub owner: Option<Owner>,
    pub query: &'a Query,
    pub selected: Vec<AssetId>,
    pub total: u64,
    pub language: Language,
    pub projects: Vec<ProjectTarget>,
}
#[derive(Default)]
pub(crate) struct BatchState {
    all: bool,
    favorite: String,
    rating: String,
    notes: String,
    replace_notes: bool,
    category: String,
    add_tags: String,
    remove_tags: String,
    project: Option<ProjectTarget>,
    owner: Option<Owner>,
    request: u64,
    saving: bool,
}
#[derive(Debug, Clone)]
pub(crate) enum BatchMessage {
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
    Stored(Owner, u64, Result<(), String>),
}
pub(crate) struct Submission {
    pub owner: Owner,
    pub request: u64,
    pub scope: BatchScope,
    pub patch: BatchPatch,
}
pub(crate) enum Effect {
    None,
    Open,
    Submit(Box<Submission>),
    Completed,
    Failed(String),
}
impl BatchState {
    pub fn update(&mut self, event: BatchMessage, context: Context<'_>) -> Effect {
        match event {
            BatchMessage::Open => {
                *self = Self {
                    owner: context.owner,
                    request: self.request.wrapping_add(1),
                    ..Default::default()
                };
                return Effect::Open;
            }
            BatchMessage::All(value) => self.all = value,
            BatchMessage::Favorite(value) => self.favorite = value,
            BatchMessage::Rating(value) => self.rating = value,
            BatchMessage::Notes(value) => self.notes = value,
            BatchMessage::ReplaceNotes(value) => self.replace_notes = value,
            BatchMessage::Category(value) => self.category = value,
            BatchMessage::AddTags(value) => self.add_tags = value,
            BatchMessage::RemoveTags(value) => self.remove_tags = value,
            BatchMessage::Project(value) => self.project = Some(value),
            BatchMessage::Apply if !self.saving && self.owner == context.owner => {
                let Some(owner) = self.owner else {
                    return Effect::None;
                };
                let split = |value: &str| {
                    value
                        .split([',', '，'])
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                        .collect()
                };
                let patch = BatchPatch {
                    favorite: match self.favorite.as_str() {
                        "收藏" => Some(true),
                        "取消收藏" => Some(false),
                        _ => None,
                    },
                    rating: self.rating.parse::<u8>().ok(),
                    notes: self.replace_notes.then(|| self.notes.clone()),
                    category: (!self.category.trim().is_empty())
                        .then(|| self.category.trim().into()),
                    add_tags: split(&self.add_tags),
                    remove_tags: split(&self.remove_tags),
                    project: self.project.as_ref().and_then(|p| p.id),
                };
                let scope = if self.all {
                    BatchScope::Query(Box::new(context.query.clone()))
                } else {
                    BatchScope::Assets(context.selected)
                };
                self.saving = true;
                self.request = self.request.wrapping_add(1);
                return Effect::Submit(Box::new(Submission {
                    owner,
                    request: self.request,
                    scope,
                    patch,
                }));
            }
            BatchMessage::Stored(owner, request, result)
                if self.saving
                    && self.owner == Some(owner)
                    && context.owner == Some(owner)
                    && request == self.request =>
            {
                self.saving = false;
                return match result {
                    Ok(()) => Effect::Completed,
                    Err(error) => Effect::Failed(error),
                };
            }
            _ => {}
        }
        Effect::None
    }
}
