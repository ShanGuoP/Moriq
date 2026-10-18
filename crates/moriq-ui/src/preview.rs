use super::*;
use std::{ops::Range, sync::Arc};
impl App {
    pub(super) fn gallery_width(&self) -> f32 {
        (self.viewport.width
            - self.frame_padding() * 2.0
            - self.sidebar_width()
            - self.details_width()
            - 64.0) // Page padding plus the embedded 10 px scrollbar and its 6 px gap.
            .max(220.0)
    }
    pub(super) fn details_width(&self) -> f32 {
        if self.viewport.width < 1280.0 {
            270.0
        } else {
            306.0
        }
    }
    pub(super) fn sidebar_width(&self) -> f32 {
        248.0
    }
    pub(super) fn frame_padding(&self) -> f32 {
        0.0
    }
    pub(super) fn titlebar_height(&self) -> f32 {
        64.0
    }
    pub(super) fn gallery_geometry(&self) -> (usize, f32) {
        let width = self.gallery_width();
        let columns = (width / (self.card_width + 10.0)).floor().clamp(1.0, 8.0) as usize;
        (
            columns,
            (width - (columns - 1) as f32 * 10.0) / columns as f32,
        )
    }
    pub(super) fn active_masonry(&self) -> Option<&moriq_render::MasonryIndex> {
        if self.import.is_some() {
            return None;
        }
        let (columns, width) = self.gallery_geometry();
        self.masonry.as_deref().filter(|layout| {
            self.masonry_mode
                && !self.list
                && layout.columns.len() == columns
                && (layout.width - width).abs() < 1.0
                && layout
                    .page_range(self.scroll_y, self.grid().viewport_height)
                    .len()
                    <= 128
        })
    }
    pub(super) fn visible_range(&self) -> Range<usize> {
        self.active_masonry()
            .map(|layout| layout.page_range(self.scroll_y, self.grid().viewport_height))
            .unwrap_or_else(|| self.grid().visible(self.total as usize, self.scroll_y))
    }
    pub(super) fn rebuild_masonry(&mut self) -> Task<Message> {
        if let Some(handle) = self.masonry_task.take() {
            handle.abort();
        }
        let Some(library) = &self.library else {
            return Task::none();
        };
        self.layout_generation += 1;
        if self.import.is_some() {
            return Task::none();
        }
        let generation = self.layout_generation;
        let (columns, width) = self.gallery_geometry();
        let service = self.service.clone();
        let id = library.info.id;
        let query = self.query.clone();
        let task = if self.launch.design_preview {
            let sizes: Vec<_> = (0..self.total as usize)
                .filter_map(|i| {
                    self.preview_source
                        .get(i % self.preview_source.len().max(1))
                })
                .map(|tile| (tile.asset.width, tile.asset.height))
                .collect();
            Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        Arc::new(moriq_render::MasonryIndex::new(
                            &sizes, columns, width, 10.0,
                        ))
                    })
                    .await
                    .map_err(|e| e.to_string())
                },
                move |result| Message::MasonryLoaded(generation, result),
            )
        } else {
            Task::perform(
                async move { service.masonry(id, query, columns, width).await },
                move |result| Message::MasonryLoaded(generation, result),
            )
        };
        let (task, handle) = task.abortable();
        self.masonry_task = Some(handle);
        task
    }
    pub(super) fn refresh_preview(&mut self, range: Range<usize>) {
        let filtered: Vec<_> = self
            .preview_source
            .iter()
            .filter(|tile| {
                (!self.query.favorites_only || tile.asset.favorite)
                    && (self.query.text.is_empty()
                        || tile.asset.name.contains(&self.query.text)
                        || tile
                            .asset
                            .tags
                            .iter()
                            .any(|tag| tag.contains(&self.query.text)))
            })
            .cloned()
            .collect();
        self.total = if self.query == Query::default() {
            12480
        } else {
            filtered.len() as u64
        };
        self.offset = range.start.min(self.total as usize);
        self.tiles = (self.offset..range.end.max(self.offset + 40).min(self.total as usize))
            .filter_map(|i| filtered.get(i % filtered.len().max(1)).cloned())
            .collect();
    }
    pub(super) fn install_preview(&mut self) -> Task<Message> {
        if self.artwork.images.is_empty() {
            return Task::none();
        }
        if self.preview_source.is_empty() {
            let sources = [
                (1, 245, 173),
                (5, 194, 125),
                (9, 213, 181),
                (13, 223, 161),
                (6, 194, 225),
                (14, 223, 165),
                (2, 245, 172),
                (10, 213, 133),
                (11, 213, 159),
                (15, 223, 181),
                (3, 245, 157),
                (7, 194, 179),
                (12, 213, 172),
                (4, 245, 167),
                (16, 223, 99),
                (8, 194, 132),
            ];
            self.preview_source = sources
                .iter()
                .enumerate()
                .map(|(index, &(number, width, height))| Tile {
                    asset: Asset {
                        id: AssetId::default(),
                        version_id: AssetVersionId::default(),
                        name: if index == 0 {
                            "DSC08912.jpg".into()
                        } else {
                            format!("DSC{:05}.jpg", 8912 + index)
                        },
                        format: "jpg".into(),
                        width,
                        height,
                        bytes: 6_800_000,
                        favorite: false,
                        rating: 0,
                        tags: vec!["风景".into(), "雪山".into(), "湖泊".into(), "旅行".into()],
                        imported_at: 1716024276000,
                    },
                    handle: self.artwork.get(&format!("photo-{number:02}")).cloned(),
                })
                .collect();
        }
        self.library = Some(Opened {
            root: PathBuf::from(r"D:\MORIQ\Libraries\我的素材库.moriq"),
            info: LibraryInfo {
                id: LibraryId::default(),
                name: "我的素材库".into(),
                count: 12480,
                favorites: 892,
                bytes: 248_000_000_000,
                created_at: 1710203040000,
                updated_at: 1710203040000,
                projects: [
                    ("旅行摄影", 328),
                    ("城市笔记", 189),
                    ("自然风光", 412),
                    ("人文纪实", 203),
                    ("产品灵感", 97),
                ]
                .into_iter()
                .map(|(name, count)| Project {
                    visited_at: 0,
                    parent: None,
                    id: ProjectId::default(),
                    name: name.into(),
                    count,
                })
                .collect(),
            },
        });
        self.total = 12480;
        self.status = "设计预览 · 示例数据".into();
        self.model_filter = 0;
        self.screen = match self.launch.screen {
            1 => Screen::Assets,
            2 => Screen::Models,
            _ => Screen::Overview,
        };
        self.dialog = None;
        self.scroll_y = 0.0;
        self.refresh_preview(0..80);
        if self.screen == Screen::Assets
            && let Some(first) = self.preview_source.first()
        {
            self.selected = Some(first.asset.clone());
            self.tag_input = first.asset.tags.join(", ");
            self.preview = self.artwork.get("detail-mountain").cloned();
        }
        if self.screen == Screen::Assets {
            self.rebuild_masonry()
        } else {
            Task::none()
        }
    }
}
