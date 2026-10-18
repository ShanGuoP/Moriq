//! Session history stores metadata and IDs, never decoded pictures or database handles.
use super::*;
use std::collections::{HashSet, VecDeque};

const HISTORY_LIMIT: usize = 32;

#[derive(Clone)]
pub(super) struct Location {
    surface: surfaces::Surface,
    surfaces: surfaces::Surfaces,
    expanded: bool,
    screen: Screen,
    query: Query,
    scroll_y: f32,
    total: u64,
    selected: Option<AssetId>,
    selection: HashSet<AssetId>,
    anchor: Option<AssetId>,
    multiselect: bool,
    list: bool,
    masonry: bool,
    card_width: f32,
    model_tab: u8,
    model_search: String,
    model_filter: u8,
    notes: String,
    notes_dirty: bool,
}
impl Location {
    fn same_destination(&self, other: &Self) -> bool {
        self.screen == other.screen
            && self.query == other.query
            && self.list == other.list
            && self.masonry == other.masonry
            && self.model_tab == other.model_tab
            && self.model_search == other.model_search
            && self.model_filter == other.model_filter
            && self.surface == other.surface
    }
}
#[derive(Default)]
pub(super) struct History {
    back: VecDeque<Location>,
    forward: VecDeque<Location>,
    searching: bool,
}
impl History {
    pub fn can_back(&self) -> bool {
        !self.back.is_empty()
    }
    pub fn can_forward(&self) -> bool {
        !self.forward.is_empty()
    }
    fn push(stack: &mut VecDeque<Location>, location: Location) {
        if stack.len() == HISTORY_LIMIT {
            stack.pop_front();
        }
        stack.push_back(location);
    }
}
impl App {
    fn location(&self) -> Location {
        Location {
            surface: self.surface(),
            surfaces: self.surfaces.clone(),
            expanded: self.preview_expanded,
            screen: self.screen,
            query: self.query.clone(),
            scroll_y: self.scroll_y,
            total: self.total,
            selected: self.selected.as_ref().map(|a| a.id),
            selection: self.manager.selection.clone(),
            anchor: self.manager.selection_anchor,
            multiselect: self.manager.multiselect,
            list: self.list,
            masonry: self.masonry_mode,
            card_width: self.card_width,
            model_tab: self.model_tab,
            model_search: self.model_search.clone(),
            model_filter: self.model_filter,
            notes: self.manager.notes.clone(),
            notes_dirty: self.manager.notes_dirty,
        }
    }
    pub(super) fn commit_navigation(&mut self, message: Message) -> Task<Message> {
        let navigating = matches!(
            message,
            Message::Navigate(_)
                | Message::AllAssets
                | Message::Favorites
                | Message::Project(_)
                | Message::Search(_)
                | Message::ModelSearch(_)
                | Message::ModelFilter(_)
                | Message::ModelTab(_)
                | Message::List(_)
                | Message::SetMasonry(_)
                | Message::TagFilter(TagChoice::All | TagChoice::Tag(_))
                | Message::Rules(RuleMessage::Apply | RuleMessage::Clear)
                | Message::Journal(JournalMessage::Located(_, _, _, Ok(Some(_))))
                | Message::Manage(
                    ManageMessage::ApplyFilter
                        | ManageMessage::ClearFilters
                        | ManageMessage::Builtin(_)
                        | ManageMessage::Tag(_)
                        | ManageMessage::UseCollection(_)
                )
        ) && !(self.manager.rule_in_filter
            && matches!(
                &message,
                Message::Rules(RuleMessage::Apply | RuleMessage::Clear)
            ));
        let searching = matches!(message, Message::Search(_) | Message::ModelSearch(_));
        let before = navigating.then(|| self.location());
        let surface = self.surface();
        let surface_y = self.surfaces.y;
        let anchor = self.surfaces.anchor;
        let page_return = matches!(&message, Message::Back | Message::Forward)
            && !self.surface_open()
            && !self.preview_expanded;
        let dismiss = navigating
            || matches!(
                &message,
                Message::ExpandPreview
                    | Message::FocusSearch
                    | Message::OpenImage(_)
                    | Message::Deletion(DeletionMessage::Finished(_, _, Ok(_)))
                    | Message::Manage(
                        ManageMessage::ProjectDeleted(_, _, Ok(_))
                            | ManageMessage::Purged(_, Ok(_))
                    )
            );
        let library = self.library.as_ref().map(|l| l.info.id);
        let task = self.update_inner(message);
        if library != self.library.as_ref().map(|l| l.info.id) {
            self.history = History::default();
        } else if library.is_some()
            && let Some(before) = before
        {
            if !before.same_destination(&self.location()) {
                if !searching || !self.history.searching {
                    History::push(&mut self.history.back, before);
                }
                self.history.forward.clear();
            }
            self.history.searching = searching;
        }
        if page_return {
            return task;
        }
        let overlay = self.sync_surface(
            surface,
            surface_y,
            anchor,
            dismiss || library != self.library.as_ref().map(|l| l.info.id),
        );
        Task::batch([task, overlay])
    }
    pub(super) fn travel(&mut self, forward: bool) -> Task<Message> {
        if self.library.is_none()
            || self.switching
            || self.manager.panel == Some(manage::Panel::Delete)
        {
            return Task::none();
        }
        let destination = if forward {
            self.history.forward.pop_back()
        } else {
            self.history.back.pop_back()
        };
        let Some(destination) = destination else {
            return Task::none();
        };
        let current = self.location();
        History::push(
            if forward {
                &mut self.history.back
            } else {
                &mut self.history.forward
            },
            current,
        );
        self.history.searching = false;
        self.screen = destination.screen;
        self.query = destination.query;
        self.list = destination.list;
        self.masonry_mode = destination.masonry;
        self.card_width = destination.card_width;
        self.model_tab = destination.model_tab;
        self.model_search = destination.model_search;
        self.model_filter = destination.model_filter;
        self.total = destination.total;
        self.dialog = None;
        self.manager.panel = None;
        let reset = self.reset_query();
        self.surfaces = destination.surfaces;
        self.manager.panel = destination.surface.panel;
        self.dialog = destination.surface.dialog;
        self.preview_expanded = destination.expanded;
        self.scroll_y = destination.scroll_y;
        self.pending_scroll = Some(destination.scroll_y);
        self.surfaces.restoring_gallery = Some(destination.scroll_y);
        self.manager.selection = destination.selection;
        self.manager.selection_anchor = destination.anchor;
        self.manager.multiselect = destination.multiselect;
        self.restored_notes = destination
            .selected
            .filter(|_| destination.notes_dirty)
            .map(|id| (id, destination.notes));
        let browse = destination
            .selected
            .map(|id| self.load_neighborhood(id, 0))
            .unwrap_or_else(Task::none);
        // Abort the initial top-of-page request and request the restored viewport instead.
        let restore_page = self.restore_page();
        Task::batch([
            reset,
            self.refresh(),
            browse,
            restore_page,
            iced::widget::operation::scroll_to(
                "management",
                scrollable::AbsoluteOffset {
                    x: 0.0,
                    y: self.surfaces.y,
                },
            ),
        ])
    }
    pub(super) fn restore_scroll(&mut self) -> Task<Message> {
        if self.screen == Screen::Assets
            && !self.loading
            && self.masonry_task.is_none()
            && let Some(y) = self.pending_scroll.take()
        {
            return iced::widget::operation::scroll_to(
                "gallery",
                scrollable::AbsoluteOffset { x: 0.0, y },
            );
        }
        Task::none()
    }
}
