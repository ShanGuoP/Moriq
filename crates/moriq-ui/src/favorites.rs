//! Immediate feedback with one ordered write per asset and guarded rollback.
use super::*;
use std::collections::HashMap;

pub(super) struct Pending {
    saved: bool,
    desired: bool,
    sent: bool,
    request: u64,
}
#[derive(Default)]
pub(super) struct Favorites {
    pending: HashMap<AssetId, Pending>,
    request: u64,
    settled: HashMap<AssetId, (bool, u64)>,
    pub deferred: Option<Message>,
}
impl App {
    pub(super) fn defer_favorite_navigation(&mut self, message: &Message) -> bool {
        if !self.favorites.pending.is_empty()
            && matches!(
                message,
                Message::WindowClose
                    | Message::OpenRecent(_)
                    | Message::PickedLibrary(Some(_))
                    | Message::PickedParent(Some(_))
                    | Message::Ai(AiMessage::SwitchLibrary(_))
                    | Message::Libraries(
                        libraries::LibraryMessage::Forget | libraries::LibraryMessage::Delete
                    )
            )
        {
            self.favorites.deferred = Some(message.clone());
            return true;
        }
        false
    }
    pub(super) fn pending_favorite(&self, id: AssetId) -> Option<bool> {
        self.favorites
            .pending
            .get(&id)
            .map(|p| p.desired)
            .or_else(|| {
                self.favorites
                    .settled
                    .get(&id)
                    .filter(|(_, generation)| self.generation <= *generation)
                    .map(|(value, _)| *value)
            })
    }
    pub(super) fn paint_favorite(&mut self, id: AssetId, value: bool) {
        for tile in self.tiles.iter_mut().chain(self.neighbors.iter_mut()) {
            if tile.asset.id == id {
                tile.asset.favorite = value;
            }
        }
        if let Some(asset) = self.viewer.selected.as_mut().filter(|a| a.id == id) {
            asset.favorite = value;
        }
    }
    pub(super) fn favorite(&mut self, id: AssetId, value: bool) -> Task<Message> {
        let Some(owner) = self.library.as_ref().map(|l| l.info.id) else {
            return Task::none();
        };
        if let Some(pending) = self.favorites.pending.get_mut(&id) {
            pending.desired = value;
            self.paint_favorite(id, value);
            return Task::none();
        }
        let saved = self
            .tiles
            .iter()
            .chain(self.neighbors.iter())
            .find(|t| t.asset.id == id)
            .map(|t| t.asset.favorite)
            .or_else(|| {
                self.viewer
                    .selected
                    .as_ref()
                    .filter(|a| a.id == id)
                    .map(|a| a.favorite)
            });
        let Some(saved) = saved else {
            return Task::none();
        };
        self.favorites.request = self.favorites.request.wrapping_add(1);
        let request = self.favorites.request;
        self.favorites.pending.insert(
            id,
            Pending {
                saved,
                desired: value,
                sent: value,
                request,
            },
        );
        self.paint_favorite(id, value);
        self.status = tr!(self.preferences.language,"正在保存收藏…"=>"Saving favorite…").into();
        self.write_favorite(owner, id, request, value)
    }
    fn write_favorite(
        &self,
        owner: LibraryId,
        id: AssetId,
        request: u64,
        value: bool,
    ) -> Task<Message> {
        let service = self.service.clone();
        Task::perform(
            async move { service.favorite(owner, id, value).await },
            move |r| Message::FavoriteSaved(owner, id, request, r),
        )
    }
    pub(super) fn favorite_saved(
        &mut self,
        owner: LibraryId,
        id: AssetId,
        request: u64,
        result: Result<(), String>,
    ) -> Task<Message> {
        if self.library.as_ref().is_none_or(|l| l.info.id != owner)
            || self
                .favorites
                .pending
                .get(&id)
                .is_none_or(|p| p.request != request)
        {
            return Task::none();
        }
        let Some(mut pending) = self.favorites.pending.remove(&id) else {
            return Task::none();
        };
        match result {
            Err(error) => {
                self.favorites.deferred = None;
                self.paint_favorite(id, pending.saved);
                self.favorites
                    .settled
                    .insert(id, (pending.saved, self.generation));
                self.error = Some(error);
            }
            Ok(()) if pending.desired != pending.sent => {
                pending.saved = pending.sent;
                pending.sent = pending.desired;
                self.favorites.request = self.favorites.request.wrapping_add(1);
                pending.request = self.favorites.request;
                let task = self.write_favorite(owner, id, pending.request, pending.sent);
                self.favorites.pending.insert(id, pending);
                return task;
            }
            Ok(()) => {
                self.paint_favorite(id, pending.desired);
                self.favorites
                    .settled
                    .insert(id, (pending.desired, self.generation));
                self.status = tr!(self.preferences.language,"收藏已保存"=>"Favorite saved").into();
                if self.favorites.pending.is_empty()
                    && let Some(message) = self.favorites.deferred.take()
                {
                    return self.update(message);
                }
                // Do not decode/rebuild the gallery for a heart-color change.
                let gallery = if self.query != Query::default() {
                    self.refresh()
                } else {
                    Task::none()
                };
                return Task::batch([gallery, self.refresh_info()]);
            }
        }
        Task::none()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn closing_waits_for_the_latest_favorite_and_old_page_data_cannot_clear_its_heart() {
        let mut app = crate::tests::app();
        let owner = app.library.as_ref().unwrap().info.id;
        let asset = crate::tests::selected_asset();
        app.tiles = vec![Tile {
            asset: asset.clone(),
            handle: None,
        }];
        drop(app.update(Message::Favorite(asset.id, true)));
        let request = app.favorites.request;
        drop(app.update(Message::WindowClose));
        assert!(matches!(app.favorites.deferred, Some(Message::WindowClose)));
        drop(app.update(Message::FavoriteSaved(owner, asset.id, request, Ok(()))));
        assert!(app.favorites.deferred.is_none());
        assert_eq!(app.pending_favorite(asset.id), Some(true));
        let generation = app.generation;
        drop(app.update(Message::PageLoaded(
            generation,
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
        assert!(app.tiles[0].asset.favorite);
    }
    #[test]
    fn feedback_is_immediate_writes_are_coalesced_and_failed_latest_write_rolls_back() {
        let mut app = crate::tests::app();
        let owner = app.library.as_ref().unwrap().info.id;
        let asset = crate::tests::selected_asset();
        app.tiles = vec![Tile {
            asset: asset.clone(),
            handle: None,
        }];
        app.viewer.selected = Some(asset.clone());
        let generation = app.generation;
        drop(app.update(Message::Favorite(asset.id, true)));
        assert!(app.tiles[0].asset.favorite);
        assert!(app.viewer.selected.as_ref().unwrap().favorite);
        assert_eq!(app.generation, generation);
        let request = app.favorites.request;
        drop(app.update(Message::Favorite(asset.id, false)));
        assert!(!app.tiles[0].asset.favorite);
        drop(app.update(Message::FavoriteSaved(owner, asset.id, request, Ok(()))));
        let latest = app.favorites.request;
        assert!(latest > request);
        assert!(!app.tiles[0].asset.favorite);
        drop(app.update(Message::FavoriteSaved(
            owner,
            asset.id,
            request,
            Err("stale".into()),
        )));
        assert!(app.error.is_none());
        drop(app.update(Message::FavoriteSaved(
            owner,
            asset.id,
            latest,
            Err("save failed".into()),
        )));
        assert!(app.tiles[0].asset.favorite);
        assert_eq!(app.error.as_deref(), Some("save failed"));
        assert!(app.favorites.pending.is_empty());
    }
}
