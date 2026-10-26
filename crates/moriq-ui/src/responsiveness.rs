//! Coalesce rapid input before starting catalog reads. Receipts are versioned and cancellable.
use super::*;

#[derive(Default)]
pub(super) struct State {
    search: Pending,
    geometry: Pending,
}

#[derive(Default)]
struct Pending {
    request: u64,
    task: Option<iced::task::Handle>,
}
impl Pending {
    fn cancel(&mut self) {
        self.request = self.request.wrapping_add(1);
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
    fn schedule(&mut self, delay: Duration, receipt: fn(u64) -> Message) -> Task<Message> {
        self.cancel();
        let request = self.request;
        let (task, handle) =
            Task::perform(async move { tokio::time::sleep(delay).await }, move |_| {
                receipt(request)
            })
            .abortable();
        self.task = Some(handle);
        task
    }
    fn finish(&mut self, request: u64) -> bool {
        request == self.request && self.task.take().is_some()
    }
}
impl State {
    pub fn cancel(&mut self) {
        self.search.cancel();
        self.geometry.cancel();
    }
    pub fn pending(&self) -> bool {
        self.search.task.is_some() || self.geometry.task.is_some()
    }
}
impl App {
    pub(super) fn invalidate_catalog_loads(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.layout_generation = self.layout_generation.wrapping_add(1);
        if let Some(task) = self.page_task.take() {
            task.abort();
        }
        if let Some(task) = self.masonry_task.take() {
            task.abort();
        }
        self.uploads.clear();
        self.loading = false;
    }
    pub(super) fn search_changed(&mut self, value: String) -> Task<Message> {
        if self.query.text == value && !self.query.semantic_active {
            return Task::none();
        }
        self.responsiveness.cancel();
        self.library_picker = false;
        self.query.semantic_active = false;
        self.query.semantic_ids.clear();
        self.query.text = value;
        self.screen = Screen::Assets;
        // Old results must not remain actionable while the new query is pending.
        self.invalidate_catalog_loads();
        self.clear_browsing();
        self.manager.selection.clear();
        self.tiles.clear();
        self.masonry = None;
        self.scroll_y = 0.0;
        self.offset = 0;
        self.loading = self.library.is_some();
        self.responsiveness
            .search
            .schedule(Duration::from_millis(140), Message::SearchReady)
    }
    pub(super) fn search_ready(&mut self, request: u64) -> Task<Message> {
        if self.responsiveness.search.finish(request)
            && self.screen == Screen::Assets
            && !self.library_picker
        {
            return self.reset_query();
        }
        Task::none()
    }
    pub(super) fn geometry_changed(&mut self) -> Task<Message> {
        if self.screen != Screen::Assets
            || self.library_picker
            || self.library.is_none()
            || self.responsiveness.search.task.is_some()
        {
            return Task::none();
        }
        self.responsiveness
            .geometry
            .schedule(Duration::from_millis(90), Message::GeometryReady)
    }
    pub(super) fn geometry_ready(&mut self, request: u64) -> Task<Message> {
        if self.responsiveness.geometry.finish(request)
            && self.screen == Screen::Assets
            && !self.library_picker
        {
            return Task::batch([self.refresh(), self.rebuild_masonry()]);
        }
        Task::none()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn welcome_defers_model_inventory_but_opening_models_starts_it_on_the_click() {
        let mut app = crate::tests::app();
        app.library = None;
        drop(app.tick());
        assert!(!app.ai.loading);
        drop(app.update(Message::Navigate(2)));
        assert_eq!(app.screen, Screen::Models);
        assert!(app.ai.loading);
    }
    #[test]
    fn search_burst_rejects_old_results_and_only_latest_receipt_starts_reads() {
        let mut app = crate::tests::app();
        app.screen = Screen::Assets;
        let old_generation = app.generation;
        let mut requests = Vec::new();
        for text in ["海", "海边", "海边日落"] {
            drop(app.update(Message::Search(text.into())));
            requests.push(app.responsiveness.search.request);
            assert!(app.page_task.is_none());
            assert!(app.masonry_task.is_none());
            assert!(app.tiles.is_empty());
        }
        drop(app.update(Message::PageLoaded(
            old_generation,
            Ok(LoadedPage {
                total: 7,
                offset: 0,
                tiles: vec![],
                warnings: 0,
            }),
        )));
        assert_eq!(app.total, 100000);
        for request in &requests[..2] {
            assert_eq!(app.update(Message::SearchReady(*request)).units(), 0);
        }
        assert_eq!(app.refresh().units(), 0);
        drop(app.update(Message::SearchReady(requests[2])));
        assert_eq!(app.query.text, "海边日落");
        assert!(app.page_task.is_some());
        assert!(app.masonry_task.is_some());
        assert!(!app.responsiveness.pending());
    }
    #[test]
    fn leaving_search_cancels_delayed_action_before_it_can_replace_the_destination() {
        let mut app = crate::tests::app();
        drop(app.update(Message::Search("old search".into())));
        let request = app.responsiveness.search.request;
        drop(app.update(Message::Navigate(2)));
        let generation = app.generation;
        assert_eq!(app.screen, Screen::Models);
        assert!(!app.responsiveness.pending());
        assert_eq!(app.update(Message::SearchReady(request)).units(), 0);
        assert_eq!(app.generation, generation);
        assert_eq!(app.screen, Screen::Models);
    }
    #[test]
    fn opening_another_library_on_overview_cancels_hidden_pages_and_rejects_their_receipts() {
        let mut app = crate::tests::app();
        app.screen = Screen::Assets;
        drop(app.refresh());
        let old_request = app.generation;
        let mut next = app.library.clone().unwrap();
        next.info.id = LibraryId::default();
        next.info.count = 42;
        drop(app.update(Message::Opened(Ok(next))));
        assert_eq!(app.screen, Screen::Overview);
        assert!(!app.loading);
        assert!(app.page_task.is_none());
        drop(app.update(Message::PageLoaded(
            old_request,
            Ok(LoadedPage {
                total: 7,
                offset: 0,
                tiles: vec![],
                warnings: 0,
            }),
        )));
        assert_eq!(app.total, 42);
    }
    #[test]
    fn resize_and_zoom_coalesce_without_clearing_selection_or_loading_hidden_gallery() {
        let mut app = crate::tests::app();
        let generation = app.generation;
        drop(app.update(Message::Resized(Size::new(1400.0, 800.0))));
        assert_eq!(app.generation, generation);
        assert!(!app.responsiveness.pending());
        app.screen = Screen::Assets;
        let asset = crate::tests::selected_asset();
        app.viewer.selected = Some(asset.clone());
        app.scroll_y = 400.0;
        for width in 1401..1420 {
            drop(app.update(Message::Resized(Size::new(width as f32, 800.0))));
        }
        let old_request = app.responsiveness.geometry.request;
        drop(app.update(Message::Zoom(240.0)));
        assert_eq!(app.generation, generation);
        assert_eq!(app.scroll_y, 400.0);
        assert_eq!(app.viewer.selected.as_ref().unwrap().id, asset.id);
        assert_eq!(app.update(Message::GeometryReady(old_request)).units(), 0);
        let request = app.responsiveness.geometry.request;
        drop(app.update(Message::GeometryReady(request)));
        assert_eq!(app.generation, generation + 1);
        assert!(app.page_task.is_some());
        assert!(app.masonry_task.is_some());
    }
}
