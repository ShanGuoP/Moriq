use super::*;

#[cfg(test)]
#[test]
#[allow(clippy::unwrap_used)]
fn fit_view_does_not_decode_original_and_repeated_zoom_does_not_restart_it() {
    let mut app = crate::tests::app();
    let mut asset = crate::tests::selected_asset();
    asset.width = 6000;
    asset.height = 4000;
    let version = asset.version_id;
    app.selected = Some(asset);
    assert_eq!(app.set_viewer(true).units(), 0);
    let viewport = Viewport {
        x: 0,
        y: 0,
        width: 6000,
        height: 4000,
        scale: 0.1,
    };
    assert_eq!(
        app.region_message(RegionMessage::Viewport(version, viewport))
            .units(),
        0
    );
    assert!(!app.regions.loading);
    let zoomed = Viewport {
        scale: 1.0,
        ..viewport
    };
    assert!(
        app.region_message(RegionMessage::Viewport(version, zoomed))
            .units()
            > 0
    );
    let request = app.regions.request;
    assert!(app.regions.loading);
    assert_eq!(
        app.region_message(RegionMessage::Viewport(version, zoomed))
            .units(),
        0
    );
    assert_eq!(app.regions.request, request);
    assert_eq!(app.regions.viewport, Some(zoomed));
    drop(app.set_viewer(false));
    assert!(!app.regions.loading);

    drop(app.set_viewer(true));
    drop(app.region_message(RegionMessage::Actual));
    assert!(app.regions.loading);
    assert_eq!(app.regions.target_scale, Some(1.0 / app.dpi_scale));
    assert!(app.regions.zoom_request > 0);
}
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub scale: f32,
}
#[derive(Default)]
pub(super) struct Regions {
    pub zoom_request: u64,
    pub target_scale: Option<f32>,
    pub session: Option<Arc<moriq_app::RegionSession>>,
    loading: bool,
    session_failed: bool,
    owner: Option<LibraryId>,
    version: Option<AssetVersionId>,
    request: u64,
    viewport: Option<Viewport>,
    visible: Vec<ImageRegion>,
    tiles: HashMap<ImageRegion, (image::Handle, u64)>,
    pending: HashSet<ImageRegion>,
    failed: HashSet<ImageRegion>,
    clock: u64,
}
impl Regions {
    pub fn reset(&mut self) {
        *self = Self {
            request: self.request.wrapping_add(1),
            ..Default::default()
        };
    }
    pub fn images(&self) -> Vec<(ImageRegion, image::Handle)> {
        self.visible
            .iter()
            .filter_map(|r| self.tiles.get(r).map(|(h, _)| (*r, h.clone())))
            .collect()
    }
}
#[derive(Debug, Clone)]
pub enum RegionMessage {
    Actual,
    Fit,
    Loaded(
        LibraryId,
        AssetId,
        AssetVersionId,
        u64,
        Result<Arc<moriq_app::RegionSession>, String>,
    ),
    Viewport(AssetVersionId, Viewport),
    Tile(
        LibraryId,
        AssetVersionId,
        u64,
        ImageRegion,
        Result<image::Handle, String>,
    ),
}
impl App {
    pub(super) fn begin_regions(&mut self) -> Task<Message> {
        let (Some(library), Some(asset)) = (&self.library, &self.selected) else {
            return Task::none();
        };
        let (owner, id, version) = (library.info.id, asset.id, asset.version_id);
        if self.regions.owner == Some(owner)
            && self.regions.version == Some(version)
            && (self.regions.session.is_some()
                || self.regions.loading
                || self.regions.session_failed)
        {
            return Task::none();
        }
        let viewport = self.regions.viewport;
        let target_scale = self.regions.target_scale;
        let zoom_request = self.regions.zoom_request;
        self.regions.reset();
        self.regions.viewport = viewport;
        self.regions.target_scale = target_scale;
        self.regions.zoom_request = zoom_request;
        self.regions.loading = true;
        self.original_size = None;
        self.regions.owner = Some(owner);
        self.regions.version = Some(version);
        let request = self.regions.request;
        let service = self.service.clone();
        Task::perform(
            async move { service.region_session(owner, id, version).await },
            move |r| Message::Regions(RegionMessage::Loaded(owner, id, version, request, r)),
        )
    }
    pub(super) fn region_message(&mut self, event: RegionMessage) -> Task<Message> {
        match event {
            RegionMessage::Actual => {
                self.regions.session_failed = false;
                self.regions.target_scale = Some(1. / self.dpi_scale);
                self.regions.zoom_request += 1;
                if self
                    .selected
                    .as_ref()
                    .is_some_and(|asset| asset.width.max(asset.height) > 1600)
                {
                    return self.begin_regions();
                }
                return Task::none();
            }
            RegionMessage::Fit => {
                self.regions.target_scale = None;
                self.regions.zoom_request += 1;
                return Task::none();
            }
            RegionMessage::Loaded(owner, asset, version, request, result)
                if self.preview_expanded
                    && self.library.as_ref().is_some_and(|l| l.info.id == owner)
                    && self
                        .selected
                        .as_ref()
                        .is_some_and(|a| a.id == asset && a.version_id == version)
                    && self.regions.request == request =>
            {
                self.regions.loading = false;
                match result {
                    Ok(session) => {
                        self.original_size = Some((session.width, session.height));
                        self.regions.session = Some(session)
                    }
                    Err(e) => {
                        self.regions.session_failed = true;
                        self.error = Some(e);
                        return Task::none();
                    }
                }
            }
            RegionMessage::Viewport(version, viewport)
                if self.preview_expanded
                    && self
                        .selected
                        .as_ref()
                        .is_some_and(|a| a.version_id == version) =>
            {
                self.regions.viewport = Some(viewport);
                if let Some(asset) = &self.selected
                    && viewport.scale.is_finite()
                    && viewport.scale * self.dpi_scale * asset.width.max(asset.height) as f32
                        > 1600.0
                    && self.regions.session.is_none()
                {
                    return self.begin_regions();
                }
            }
            RegionMessage::Tile(owner, version, request, region, result)
                if self.preview_expanded
                    && self.regions.owner == Some(owner)
                    && self.regions.version == Some(version)
                    && self.regions.request == request =>
            {
                self.regions.pending.remove(&region);
                match result {
                    Ok(handle) => {
                        self.regions.clock += 1;
                        let age = self.regions.clock;
                        self.regions.tiles.insert(region, (handle, age));
                        while self.regions.tiles.len() > 128 {
                            let candidate = self
                                .regions
                                .tiles
                                .iter()
                                .filter(|(r, _)| !self.regions.visible.contains(r))
                                .min_by_key(|(_, (_, age))| *age)
                                .map(|(r, _)| *r);
                            if let Some(old) = candidate {
                                self.regions.tiles.remove(&old);
                            } else {
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        self.regions.failed.insert(region);
                        self.error = Some(e)
                    }
                }
            }
            _ => return Task::none(),
        }
        self.schedule_regions()
    }
    fn schedule_regions(&mut self) -> Task<Message> {
        let (Some(session), Some(viewport), Some(owner), Some(version)) = (
            self.regions.session.clone(),
            self.regions.viewport,
            self.regions.owner,
            self.regions.version,
        ) else {
            return Task::none();
        };
        self.regions.visible.clear();
        let physical_scale = viewport.scale * self.dpi_scale;
        if !physical_scale.is_finite()
            || physical_scale <= 1600.0 / session.width.max(session.height) as f32
        {
            return Task::none();
        }
        let ratio = (0.5 / physical_scale).clamp(1.0, 64.0) as u32;
        let level = 1u32 << ratio.ilog2();
        let span = 512 * level;
        let right = (u64::from(viewport.x) + u64::from(viewport.width))
            .min(u64::from(session.width)) as u32;
        let bottom = (u64::from(viewport.y) + u64::from(viewport.height))
            .min(u64::from(session.height)) as u32;
        for y in (viewport.y / span * span..bottom).step_by(span as usize) {
            for x in (viewport.x / span * span..right).step_by(span as usize) {
                self.regions.visible.push(ImageRegion {
                    x,
                    y,
                    width: span.min(session.width - x),
                    height: span.min(session.height - y),
                    level,
                });
            }
        }
        let center = (
            viewport.x as f64 + viewport.width as f64 / 2.,
            viewport.y as f64 + viewport.height as f64 / 2.,
        );
        self.regions.visible.sort_by(|a, b| {
            let distance = |r: &ImageRegion| {
                (r.x as f64 + r.width as f64 / 2. - center.0).powi(2)
                    + (r.y as f64 + r.height as f64 / 2. - center.1).powi(2)
            };
            distance(a).total_cmp(&distance(b))
        });
        self.regions.visible.truncate(128);
        let mut tasks = Vec::new();
        for region in self.regions.visible.clone() {
            if self.regions.pending.len() >= 4 {
                break;
            }
            if self.regions.tiles.contains_key(&region)
                || self.regions.pending.contains(&region)
                || self.regions.failed.contains(&region)
            {
                continue;
            }
            self.regions.pending.insert(region);
            let request = self.regions.request;
            let service = self.service.clone();
            let session = session.clone();
            tasks.push(Task::perform(
                async move {
                    service
                        .region_tile(owner, session, region)
                        .await
                        .map(|p| image::Handle::from_rgba(p.width, p.height, p.rgba))
                },
                move |r| Message::Regions(RegionMessage::Tile(owner, version, request, region, r)),
            ));
        }
        Task::batch(tasks)
    }
}
