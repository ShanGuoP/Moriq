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
    fn saved(&self) -> SavedVisit {
        SavedVisit {
            screen: match self.screen {
                Screen::Overview => 0,
                Screen::Assets => 1,
                Screen::Models => 2,
            },
            query: self.query.clone(),
            scroll_y: self.scroll_y,
            selected: self.selected,
            list: self.list,
            masonry: self.masonry,
            card_width: self.card_width,
        }
    }
    fn from_saved(value: SavedVisit) -> Self {
        Self {
            surface: surfaces::Surface {
                panel: None,
                dialog: None,
            },
            surfaces: Default::default(),
            expanded: false,
            screen: match value.screen {
                1 => Screen::Assets,
                2 => Screen::Models,
                _ => Screen::Overview,
            },
            query: value.query,
            scroll_y: if value.scroll_y.is_finite() {
                value.scroll_y.clamp(0., 1e9)
            } else {
                0.
            },
            total: 0,
            selected: value.selected,
            selection: Default::default(),
            anchor: None,
            multiselect: false,
            list: value.list,
            masonry: value.masonry,
            card_width: if value.card_width.is_finite() {
                value.card_width.clamp(120., 600.)
            } else {
                216.
            },
            model_tab: 0,
            model_search: String::new(),
            model_filter: 0,
            notes: String::new(),
            notes_dirty: false,
        }
    }
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
    restoring: bool,
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
    pub(super) fn navigation_snapshot(&self) -> SavedNavigation {
        SavedNavigation {
            current: self.location().saved(),
            back: self.history.back.iter().map(Location::saved).collect(),
            forward: self.history.forward.iter().map(Location::saved).collect(),
        }
    }
    pub(super) fn persist_navigation(&self) -> Task<Message> {
        if self.launch.design_preview || self.launch.capture.is_some() || self.history.restoring {
            return Task::none();
        }
        let Some(library) = &self.library else {
            return Task::none();
        };
        let id = library.info.id;
        let value = self.navigation_snapshot();
        let service = self.service.clone();
        Task::perform(
            async move { service.save_navigation(id, value).await },
            move |r| Message::NavigationSaved(id, r),
        )
    }
    pub(super) fn navigation_loaded(
        &mut self,
        id: LibraryId,
        generation: u64,
        result: Result<Option<SavedNavigation>, String>,
    ) -> Task<Message> {
        if self.library.as_ref().is_none_or(|l| l.info.id != id) {
            return Task::none();
        }
        self.history.restoring = false;
        if self.generation != generation {
            return Task::none();
        }
        match result {
            Ok(Some(value)) => {
                self.history.back = value
                    .back
                    .into_iter()
                    .take(HISTORY_LIMIT)
                    .map(Location::from_saved)
                    .collect();
                self.history.forward = value
                    .forward
                    .into_iter()
                    .take(HISTORY_LIMIT)
                    .map(Location::from_saved)
                    .collect();
                return self.restore_location(Location::from_saved(value.current));
            }
            Err(e) => self.error = Some(e),
            _ => {}
        }
        Task::none()
    }
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
                | Message::Folder(FolderMessage::Open(_))
                | Message::Metadata(MetadataMessage::Apply | MetadataMessage::Clear)
                | Message::Ai(
                    AiMessage::Category(_)
                        | AiMessage::PersonFilter(_)
                        | AiMessage::SearchLoaded(_, _, Ok(_))
                )
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
            self.jobs = crate::tasks::Scheduler {
                request: self.jobs.request + 1,
                ..Default::default()
            };
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
        let navigation = if library != self.library.as_ref().map(|l| l.info.id)
            && !self.launch.design_preview
            && self.launch.capture.is_none()
            && self.launch.screen == 0
        {
            if let Some(library) = &self.library {
                self.history.restoring = true;
                let owner = library.info.id;
                let generation = self.generation;
                let service = self.service.clone();
                Task::perform(
                    async move { service.saved_navigation(owner).await },
                    move |r| Message::NavigationLoaded(owner, generation, r),
                )
            } else {
                Task::none()
            }
        } else if navigating || page_return {
            self.persist_navigation()
        } else {
            Task::none()
        };
        if page_return {
            return Task::batch([task, navigation]);
        }
        let overlay = self.sync_surface(
            surface,
            surface_y,
            anchor,
            dismiss || library != self.library.as_ref().map(|l| l.info.id),
        );
        Task::batch([task, overlay, navigation])
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
        self.restore_location(destination)
    }
    fn restore_location(&mut self, destination: Location) -> Task<Message> {
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
