use super::*;

#[cfg(test)]
#[test]
#[allow(clippy::unwrap_used)]
/// 验证选择并打开同一版本会复用进行中或已完成的预览。
fn selecting_and_opening_same_version_reuses_pending_and_completed_preview() {
    let mut app = crate::tests::app();
    app.screen = Screen::Assets;
    let asset = crate::tests::selected_asset();
    let tile = Tile {
        asset: asset.clone(),
        handle: Some(image::Handle::from_rgba(1, 1, vec![0; 4])),
    };
    app.tiles = vec![tile.clone()];
    drop(app.focus_tile(tile.clone()));
    let request = app.viewer.original_request;
    assert!(app.viewer.preview_task.is_some());
    drop(app.update(Message::OpenImage(asset.id)));
    assert_eq!(app.viewer.original_request, request);
    assert!(app.viewer.preview_task.is_some());
    let owner = app.library.as_ref().unwrap().info.id;
    drop(app.update(Message::FullPreviewLoaded(
        owner,
        asset.id,
        asset.version_id,
        Ok((
            image::Handle::from_rgba(1, 1, vec![0; 4]),
            image::Handle::from_rgba(1, 1, vec![0; 4]),
        )),
    )));
    assert!(app.viewer.preview_task.is_none());
    assert_eq!(app.focus_tile(tile.clone()).units(), 0);
    let mut next = tile;
    next.asset.version_id = AssetVersionId::default();
    drop(app.focus_tile(next));
    assert!(app.viewer.original_request > request);
    assert!(app.viewer.preview_background.is_none());
}

impl App {
    /// 取消相邻素材读取并清除浏览锚点，使旧结果无法重定向查看器。
    pub(super) fn clear_browsing(&mut self) {
        self.viewer.clear();
        self.surfaces.viewer_y = None;
        self.surfaces.restoring_gallery = None;
        self.compare.clear();
        if let Some(handle) = self.browse_task.take() {
            handle.abort();
        }
        self.browse_request += 1;
        self.neighbors.clear();
        self.neighbor_position = None;
        self.pending_scroll = None;
        self.restored_notes = None;
        self.editing_tags = false;
    }
    /// 按当前查询与素材锚点异步读取相邻素材。
    pub(super) fn load_neighborhood(&mut self, anchor: AssetId, direction: i8) -> Task<Message> {
        let Some(library) = &self.library else {
            return Task::none();
        };

        if let Some(handle) = self.browse_task.take() {
            handle.abort();
        }
        self.browse_request += 1;
        let request = self.browse_request;
        let id = library.info.id;
        let query = self.query.clone();
        let service = self.service.clone();
        let (task, handle) = Task::perform(
            async move { service.neighborhood(id, query, anchor, direction).await },
            move |result| Message::NeighborhoodLoaded(id, request, result),
        )
        .abortable();
        self.browse_task = Some(handle);
        task
    }
    /// 把焦点移到指定有效素材，并协调选择和预览状态。
    pub(super) fn focus_tile(&mut self, mut tile: Tile) -> Task<Message> {
        if let Some(value) = self.pending_favorite(tile.asset.id) {
            tile.asset.favorite = value;
        }
        if self.viewer.selected.as_ref().is_some_and(|asset| {
            asset.id == tile.asset.id && asset.version_id == tile.asset.version_id
        }) && (self.viewer.preview_task.is_some() || self.viewer.preview_background.is_some())
        {
            // Selection, double-click and neighborhood receipts refer to the same load.
            self.viewer.selected = Some(tile.asset);
            return Task::none();
        }
        self.viewer.focus();

        let same = self
            .viewer
            .selected
            .as_ref()
            .is_some_and(|a| a.id == tile.asset.id);
        let same_version = self
            .viewer
            .selected
            .as_ref()
            .is_some_and(|a| a.version_id == tile.asset.version_id);
        if !same {
            self.viewer.preview_background = None;
            self.manager.notes.clear();
            self.manager.notes_dirty = false;
            self.manager.details = Default::default();
            self.editing_tags = false;
        }
        if !same_version {
            self.viewer.preview_background = None;
            self.manager.details = Default::default();
        }
        let asset = tile.asset;
        if !same || !self.editing_tags {
            self.tag_input = asset.tags.join(", ");
        }
        if !same_version || self.viewer.preview.is_none() {
            self.viewer.preview = tile.handle;
        }
        let id = asset.id;
        let version = asset.version_id;
        self.viewer.selected = Some(asset);
        if let Some((restored, notes)) = self.restored_notes.take()
            && restored == id
        {
            self.manager.notes = notes;
            self.manager.notes_dirty = true;
        }

        let Some(library) = &self.library else {
            return Task::none();
        };
        let library = library.info.id;
        let service = self.service.clone();
        let (task, handle) = Task::perform(
            async move {
                service
                    .preview_version(library, id, version)
                    .await
                    .map(|p| {
                        (
                            image::Handle::from_rgba(p.image.width, p.image.height, p.image.rgba),
                            image::Handle::from_rgba(
                                p.background.width,
                                p.background.height,
                                p.background.rgba,
                            ),
                        )
                    })
            },
            move |result| Message::FullPreviewLoaded(library, id, version, result),
        )
        .abortable();
        self.viewer.preview_task = Some(handle);
        let enlarged = if self.viewer.preview_expanded {
            self.set_viewer(true)
        } else {
            Task::none()
        };
        Task::batch([task, self.refresh_details(), enlarged])
    }
    /// 处理前后浏览及回执，仅接受当前查询和请求的相邻结果。
    pub(super) fn browse_message(
        &mut self,
        library: LibraryId,
        request: u64,
        result: Result<Option<moriq_app::LoadedNeighborhood>, String>,
    ) -> Task<Message> {
        let language = self.preferences.language;
        if self.browse_request != request
            || !self.library.as_ref().is_some_and(|l| l.info.id == library)
        {
            return Task::none();
        }
        self.browse_task = None;
        match result {
            Ok(Some(result)) => {
                self.neighbor_position = Some((result.position, result.page.total));
                self.neighbors = result
                    .page
                    .tiles
                    .into_iter()
                    .map(|tile| Tile {
                        asset: tile.asset,
                        handle: tile
                            .thumbnail
                            .map(|p| image::Handle::from_rgba(p.width, p.height, p.rgba)),
                    })
                    .collect();
                if let Some(tile) = self
                    .neighbors
                    .iter()
                    .find(|t| t.asset.id == result.selected)
                    .cloned()
                {
                    return self.focus_tile(tile);
                }
            }
            Ok(None) => {
                self.viewer.selected = None;
                self.viewer.preview = None;
                self.neighbors.clear();
                self.neighbor_position = None;
                self.restored_notes = None;
                self.status = tr!(language, "原先选择的素材已不在此查询中" => "The previously selected asset is no longer in these results").into();
            }
            Err(error) => self.error = Some(error),
        }
        Task::none()
    }
}
