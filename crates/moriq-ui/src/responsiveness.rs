//! Coalesce rapid input before starting catalog reads. Receipts are versioned and cancellable.
use super::*;

#[derive(Default)]
pub(super) struct State {
    search: Pending,
    geometry: Pending,
    original_view: Option<SearchOrigin>,
    pub search_error: Option<String>,
}
struct SearchOrigin {
    query: Query,
    screen: Screen,
    picker: bool,
}

#[derive(Default)]
struct Pending {
    request: u64,
    task: Option<iced::task::Handle>,
}
impl Pending {
    /// 中止当前延迟任务并推进请求序号，使旧计时回执失效。
    fn cancel(&mut self) {
        self.request = self.request.wrapping_add(1);
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
    /// 替换尚未完成的延迟任务，合并连续搜索或几何输入。
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
    /// 仅接受当前延迟任务的完成回执，并清除对应句柄。
    fn finish(&mut self, request: u64) -> bool {
        request == self.request && self.task.take().is_some()
    }
}
impl State {
    /// 取消搜索和几何延迟，同时清除替换上下文与搜索错误。
    pub fn cancel(&mut self) {
        self.search.cancel();
        self.geometry.cancel();
        self.original_view = None;
        self.search_error = None;
    }
    /// 判断是否仍存在待触发的搜索或几何更新。
    pub fn pending(&self) -> bool {
        self.search.task.is_some() || self.geometry.task.is_some()
    }
    /// 判断当前是否保留旧页面等待新搜索结果提交。
    pub fn replacing(&self) -> bool {
        self.original_view.is_some()
    }
    /// 清除搜索错误并结束旧页面保留状态。
    pub fn commit_search(&mut self) -> bool {
        self.search_error = None;
        self.original_view.take().is_some()
    }
}
impl App {
    /// 推进分页和布局世代，取消旧读取并清空待上传结果。
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
    /// 更新搜索草稿并安排延迟读取；保留旧画面但暂停其素材操作。
    pub(super) fn search_changed(&mut self, value: String) -> Task<Message> {
        if self.query.text == value
            && !self.query.semantic_active
            && !self.responsiveness.replacing()
        {
            return Task::none();
        }
        let original = self
            .responsiveness
            .original_view
            .take()
            .unwrap_or_else(|| SearchOrigin {
                query: self.query.clone(),
                screen: self.screen,
                picker: self.library_picker,
            });
        self.responsiveness.cancel();
        self.responsiveness.original_view = Some(original);
        self.library_picker = false;
        self.query.semantic_active = false;
        self.query.semantic_ids.clear();
        self.query.text = value;
        self.screen = Screen::Assets;
        // Keep the displayed page stable, but suspend its actions until the new page commits.
        self.invalidate_catalog_loads();
        if let Some(task) = self.browse_task.take() {
            task.abort();
        }
        self.browse_request = self.browse_request.wrapping_add(1);
        self.loading = self.library.is_some();
        self.responsiveness
            .search
            .schedule(Duration::from_millis(140), Message::SearchReady)
    }
    /// 仅在当前搜索回执与页面匹配时刷新图库。
    pub(super) fn search_ready(&mut self, request: u64) -> Task<Message> {
        if self.responsiveness.search.finish(request)
            && self.screen == Screen::Assets
            && !self.library_picker
        {
            return self.refresh();
        }
        Task::none()
    }
    /// 在可见图库中合并窗口或缩放变化，避免隐藏页面重复读取。
    pub(super) fn geometry_changed(&mut self) -> Task<Message> {
        if self.screen != Screen::Assets
            || self.library_picker
            || self.library.is_none()
            || self.responsiveness.replacing()
        {
            return Task::none();
        }
        self.responsiveness
            .geometry
            .schedule(Duration::from_millis(90), Message::GeometryReady)
    }
    /// 取消未提交搜索并恢复原页面与查询，按需要补发当前视口请求。
    pub(super) fn cancel_search(&mut self) -> Task<Message> {
        if let Some(origin) = self.responsiveness.original_view.take() {
            self.invalidate_catalog_loads();
            self.responsiveness.cancel();
            self.query = origin.query;
            self.screen = origin.screen;
            self.library_picker = origin.picker;
            self.discard_cancelled_search_history();
            let range = self.visible_range();
            let needs_layout = self.masonry_mode
                && !self.list
                && self.masonry.is_some()
                && self.active_masonry().is_none();
            if needs_layout
                || range.start < self.offset
                || range.end > self.offset + self.tiles.len()
            {
                return self.geometry_changed();
            }
        }
        Task::none()
    }
    /// 在新查询尚未提交时阻止旧画面的素材动作，避免写入错误对象。
    pub(super) fn blocks_retained_action(&self, message: &Message) -> bool {
        self.responsiveness.replacing()
            && matches!(
                message,
                Message::Select(_)
                    | Message::CardPressed(_)
                    | Message::OpenImage(_)
                    | Message::AssetContext(_)
                    | Message::Favorite(..)
                    | Message::Rating(_)
                    | Message::SaveTags
                    | Message::SaveRenameAsset
                    | Message::Browse(_)
                    | Message::ExpandPreview
                    | Message::Scrolled(_)
                    | Message::GalleryViewport(..)
                    | Message::Shortcut(_)
                    | Message::Manage(
                        manage::ManageMessage::Edit(_)
                            | manage::ManageMessage::CopyProject(_)
                            | manage::ManageMessage::Toggle(_)
                            | manage::ManageMessage::ToggleMulti
                            | manage::ManageMessage::SelectVisible
                            | manage::ManageMessage::ClearSelection
                            | manage::ManageMessage::SaveNotes
                            | manage::ManageMessage::Organize
                            | manage::ManageMessage::AssetMenu
                            | manage::ManageMessage::AssetProjects
                            | manage::ManageMessage::Versions
                            | manage::ManageMessage::Version(_)
                            | manage::ManageMessage::SaveLocation
                            | manage::ManageMessage::ClearLocation
                            | manage::ManageMessage::AssetCover
                            | manage::ManageMessage::SendSynco
                            | manage::ManageMessage::Export
                            | manage::ManageMessage::RelocateReference
                    )
                    | Message::Batch(BatchMessage::Open | BatchMessage::Apply)
                    | Message::Compare(CompareMessage::Open)
                    | Message::Folder(FolderMessage::Members(..) | FolderMessage::Cover(_))
                    | Message::Ai(
                        AiMessage::SaveClassification
                            | AiMessage::SaveDescription
                            | AiMessage::EditContent(_)
                    )
            )
    }
    /// 仅接受当前几何更新回执，并刷新分页与瀑布流布局。
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
    /// 验证欢迎页延后读取模型清单，点击模型页时即时发起读取。
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
    /// 验证连续搜索仅最新回执触发读取，并拒绝旧结果。
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
        assert!(app.masonry_task.is_none());
        assert!(!app.responsiveness.pending());
    }
    #[test]
    /// 验证离开搜索会取消延迟动作，避免之后替换用户目的地。
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
    /// 构造已有素材与选择的界面状态，供搜索和导航连续性验证。
    fn populated() -> (App, Asset) {
        let mut app = crate::tests::app();
        app.screen = Screen::Assets;
        let asset = crate::tests::selected_asset();
        app.tiles = vec![Tile {
            asset: asset.clone(),
            handle: Some(image::Handle::from_rgba(1, 1, vec![0; 4])),
        }];
        app.viewer.selected = Some(asset.clone());
        app.manager.notes = "未保存的备注".into();
        app.manager.notes_dirty = true;
        app.scroll_y = 300.0;
        (app, asset)
    }
    #[test]
    /// 验证搜索期间保留旧画面和草稿，新结果提交前禁用旧素材动作。
    fn search_keeps_the_display_and_draft_but_blocks_old_asset_actions_until_commit() {
        let (mut app, asset) = populated();
        app.neighbor_position = Some((4, 20));
        app.neighbors = app.tiles.clone();
        for text in ["new", "newer"] {
            drop(app.update(Message::Search(text.into())));
            assert_eq!(app.tiles[0].asset.id, asset.id);
            assert!(app.tiles[0].handle.is_some());
            assert_eq!(app.scroll_y, 300.0);
            assert_eq!(app.manager.notes, "未保存的备注");
            assert!(app.manager.notes_dirty);
            assert_eq!(app.update(Message::Favorite(asset.id, true)).units(), 0);
            assert!(!app.tiles[0].asset.favorite);
            assert!(
                app.blocks_retained_action(&Message::Manage(manage::ManageMessage::Edit(
                    Edit::Trash(true)
                )))
            );
            assert!(!app.blocks_retained_action(&Message::Saved(
                app.library.as_ref().unwrap().info.id,
                Ok(())
            )));
        }
        let request = app.responsiveness.search.request;
        drop(app.update(Message::SearchReady(request)));
        assert!(app.responsiveness.replacing());
        drop(app.update(Message::PageLoaded(
            app.generation,
            Ok(LoadedPage {
                total: 1,
                offset: 0,
                tiles: vec![moriq_app::Tile {
                    asset: asset.clone(),
                    thumbnail: None,
                }],
                warnings: 0,
            }),
        )));
        assert!(!app.responsiveness.replacing());
        assert_eq!(app.scroll_y, 0.0);
        assert_eq!(app.viewer.selected.as_ref().unwrap().id, asset.id);
        assert_eq!(app.manager.notes, "未保存的备注");
        assert!(app.tiles[0].handle.is_some());
        assert!(app.neighbors.is_empty());
        assert!(app.neighbor_position.is_none());
        assert!(app.browse_task.is_some());
    }
    #[test]
    /// 验证搜索失败保留旧页，取消恢复上下文并拒绝迟到重试。
    fn failed_search_keeps_old_page_and_cancel_restores_context_and_rejects_late_retry() {
        let (mut app, asset) = populated();
        let original = app.query.clone();
        drop(app.update(Message::Search("new".into())));
        let request = app.responsiveness.search.request;
        drop(app.update(Message::SearchReady(request)));
        drop(app.update(Message::PageLoaded(
            app.generation,
            Err("storage unavailable".into()),
        )));
        assert!(app.responsiveness.search_error.is_some());
        assert!(app.responsiveness.replacing());
        assert_eq!(app.tiles[0].asset.id, asset.id);
        assert!(app.error.is_none());
        drop(app.update(Message::Search("new".into())));
        let retry = app.responsiveness.search.request;
        drop(app.update(Message::CancelSearch));
        assert_eq!(app.query, original);
        assert_eq!(app.scroll_y, 300.0);
        assert_eq!(app.viewer.selected.as_ref().unwrap().id, asset.id);
        assert_eq!(app.manager.notes, "未保存的备注");
        assert!(!app.history.can_back());
        assert_eq!(app.update(Message::SearchReady(retry)).units(), 0);
        assert!(app.page_task.is_none());
    }
    #[test]
    /// 验证 Escape 按层关闭浮层与查看器，保留选择和笔记草稿。
    fn escape_closes_one_surface_then_viewer_without_discarding_selection_and_notes() {
        let (mut app, asset) = populated();
        app.viewer.preview_expanded = true;
        drop(app.update(Message::ShowSettings));
        drop(app.update(Message::ClosePreview));
        assert!(app.dialog.is_none());
        assert!(app.viewer.preview_expanded);
        drop(app.update(Message::ClosePreview));
        assert!(!app.viewer.preview_expanded);
        let generation = app.generation;
        drop(app.update(Message::ClosePreview));
        assert_eq!(app.generation, generation);
        assert_eq!(app.viewer.selected.as_ref().unwrap().id, asset.id);
        assert!(app.manager.notes_dirty);
    }
    #[test]
    /// 验证首页发起的搜索取消后回到首页，不留下空图库。
    fn cancelling_search_from_overview_returns_to_overview_without_an_empty_gallery() {
        let mut app = crate::tests::app();
        drop(app.update(Message::Search("new".into())));
        drop(app.update(Message::CancelSearch));
        assert_eq!(app.screen, Screen::Overview);
        assert!(!app.history.can_back());
    }
    #[test]
    /// 验证搜索时后退恢复原列表，保留当前资源库。
    fn back_during_search_returns_to_the_original_list_without_leaving_the_library() {
        let (mut app, asset) = populated();
        drop(app.update(Message::Search("new".into())));
        drop(app.update(Message::Back));
        assert!(!app.library_picker);
        assert_eq!(app.screen, Screen::Assets);
        assert_eq!(app.viewer.selected.as_ref().unwrap().id, asset.id);
        assert_eq!(app.scroll_y, 300.0);
        assert!(!app.responsiveness.replacing());
        assert!(!app.history.can_back());
    }
    #[test]
    /// 验证后台工作期间帮助和嵌套返回可用，且不会提前释放忙碌状态。
    fn background_work_keeps_help_and_nested_return_available_without_releasing_its_busy_state() {
        let mut app = crate::tests::app();
        app.manager.panel = Some(manage::Panel::Maintenance);
        app.manager.busy = true;
        drop(app.update(Message::Manage(manage::ManageMessage::Help)));
        assert_eq!(app.manager.panel, Some(manage::Panel::Help));
        drop(app.shell());
        drop(app.update(Message::Back));
        assert_eq!(app.manager.panel, Some(manage::Panel::Maintenance));
        assert!(app.manager.busy);
    }
    #[test]
    /// 验证首页切库取消隐藏分页，并拒绝这些分页的迟到回执。
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
    /// 验证尺寸和缩放变化合并处理，不清空选择或加载隐藏图库。
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
