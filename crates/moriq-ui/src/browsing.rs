use super::*;

impl App {
    pub(super) fn clear_browsing(&mut self) {
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
        self.preview_expanded = false;
        self.preview_background = None;
        self.editing_tags = false;
        if let Some(handle) = self.preview_task.take() {
            handle.abort();
        }
    }
    pub(super) fn load_neighborhood(&mut self, anchor: AssetId, direction: i8) -> Task<Message> {
        let Some(library) = &self.library else {
            return Task::none();
        };
        if self.launch.design_preview {
            return Task::none();
        }
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
    pub(super) fn focus_tile(&mut self, tile: Tile) -> Task<Message> {
        if let Some(handle) = self.preview_task.take() {
            handle.abort();
        }
        let same = self
            .selected
            .as_ref()
            .is_some_and(|a| a.id == tile.asset.id);
        let same_version = self
            .selected
            .as_ref()
            .is_some_and(|a| a.version_id == tile.asset.version_id);
        if !same {
            self.preview_background = None;
            self.manager.notes.clear();
            self.manager.notes_dirty = false;
            self.manager.details = Default::default();
            self.editing_tags = false;
        }
        if !same_version {
            self.manager.details = Default::default();
        }
        let asset = tile.asset;
        if !same || !self.editing_tags {
            self.tag_input = asset.tags.join(", ");
        }
        if !same_version || self.preview.is_none() {
            self.preview = tile.handle;
        }
        let id = asset.id;
        let version = asset.version_id;
        self.selected = Some(asset);
        if let Some((restored, notes)) = self.restored_notes.take()
            && restored == id
        {
            self.manager.notes = notes;
            self.manager.notes_dirty = true;
        }
        if self.launch.design_preview {
            if self
                .preview_source
                .first()
                .is_some_and(|tile| tile.asset.id == id)
            {
                self.preview = self.artwork.get("detail-mountain").cloned();
            }
            return Task::none();
        }
        let Some(library) = &self.library else {
            return Task::none();
        };
        let library = library.info.id;
        let service = self.service.clone();
        let (task, handle) = Task::perform(
            async move {
                service.preview_full(library, id).await.map(|p| {
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
        self.preview_task = Some(handle);
        Task::batch([task, self.refresh_details()])
    }
    pub(super) fn browse_message(
        &mut self,
        library: LibraryId,
        request: u64,
        result: Result<Option<moriq_app::LoadedNeighborhood>, String>,
    ) -> Task<Message> {
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
                self.selected = None;
                self.preview = None;
                self.neighbors.clear();
                self.neighbor_position = None;
                self.restored_notes = None;
                self.status = "原先选择的素材已不在此查询中".into();
            }
            Err(error) => self.error = Some(error),
        }
        Task::none()
    }
}
