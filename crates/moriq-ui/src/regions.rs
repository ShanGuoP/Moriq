use super::*;

#[cfg(test)]
#[test]
#[allow(clippy::unwrap_used)]
/// 验证适应窗口不启动原图解码，重复缩放不重启同一会话。
fn fit_view_does_not_decode_original_and_repeated_zoom_does_not_restart_it() {
    let mut app = crate::tests::app();
    let mut asset = crate::tests::selected_asset();
    asset.width = 6000;
    asset.height = 4000;
    let version = asset.version_id;
    app.viewer.selected = Some(asset);
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
    assert!(!app.viewer.regions.loading);
    let zoomed = Viewport {
        scale: 1.0,
        ..viewport
    };
    assert!(
        app.region_message(RegionMessage::Viewport(version, zoomed))
            .units()
            > 0
    );
    let request = app.viewer.regions.request;
    assert!(app.viewer.regions.loading);
    assert_eq!(
        app.region_message(RegionMessage::Viewport(version, zoomed))
            .units(),
        0
    );
    assert_eq!(app.viewer.regions.request, request);
    assert_eq!(app.viewer.regions.viewport, Some(zoomed));
    drop(app.set_viewer(false));
    assert!(!app.viewer.regions.loading);

    drop(app.set_viewer(true));
    drop(app.region_message(RegionMessage::Actual));
    assert!(app.viewer.regions.loading);
    assert_eq!(app.viewer.regions.target_scale, Some(1.0 / app.dpi_scale));
    assert!(app.viewer.regions.zoom_request > 0);
    let cancelled = app.viewer.regions.cancelled.clone();
    drop(app.set_viewer(false));
    assert!(cancelled.load(Ordering::Acquire));
    assert!(!app.viewer.regions.cancelled.load(Ordering::Acquire));
    drop(app.set_viewer(true));
    drop(app.region_message(RegionMessage::Actual));
    let closing = app.viewer.regions.cancelled.clone();
    drop(app.update(Message::WindowClose));
    assert!(closing.load(Ordering::Acquire));
    assert!(!app.viewer.preview_expanded);
    assert!(app.viewer.regions.session_task.is_none());
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
    cancelled: Arc<std::sync::atomic::AtomicBool>,
    session_task: Option<iced::task::Handle>,
    tile_tasks: HashMap<ImageRegion, iced::task::Handle>,
}
impl Regions {
    /// 判断原图会话或图块请求是否仍在进行。
    pub fn is_loading(&self) -> bool {
        self.loading || !self.pending.is_empty()
    }
    /// 取消原图读取、释放图块和会话，并推进请求世代。
    pub fn reset(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        if let Some(handle) = self.session_task.take() {
            handle.abort();
        }
        for (_, handle) in self.tile_tasks.drain() {
            handle.abort();
        }
        *self = Self {
            request: self.request.wrapping_add(1),
            ..Default::default()
        };
    }
    /// 返回当前可供查看器绘制的图块句柄。
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
    /// 为当前素材版本启动原图会话读取。
    pub(super) fn begin_regions(&mut self) -> Task<Message> {
        let (Some(library), Some(asset)) = (&self.library, &self.viewer.selected) else {
            return Task::none();
        };
        let (owner, id, version) = (library.info.id, asset.id, asset.version_id);
        if self.viewer.regions.owner == Some(owner)
            && self.viewer.regions.version == Some(version)
            && (self.viewer.regions.session.is_some()
                || self.viewer.regions.loading
                || self.viewer.regions.session_failed)
        {
            return Task::none();
        }
        let viewport = self.viewer.regions.viewport;
        let target_scale = self.viewer.regions.target_scale;
        let zoom_request = self.viewer.regions.zoom_request;
        self.viewer.regions.reset();
        self.viewer.regions.viewport = viewport;
        self.viewer.regions.target_scale = target_scale;
        self.viewer.regions.zoom_request = zoom_request;
        self.viewer.regions.loading = true;
        self.viewer.original_size = None;
        self.viewer.regions.owner = Some(owner);
        self.viewer.regions.version = Some(version);
        let request = self.viewer.regions.request;
        let service = self.service.clone();
        let cancelled = self.viewer.regions.cancelled.clone();
        let (task, handle) = Task::perform(
            async move {
                service
                    .region_session_cancellable(owner, id, version, cancelled)
                    .await
            },
            move |r| Message::Regions(RegionMessage::Loaded(owner, id, version, request, r)),
        )
        .abortable();
        self.viewer.regions.session_task = Some(handle);
        task
    }
    /// 处理会话与图块回执，拒绝旧素材、旧版本和旧请求结果。
    pub(super) fn region_message(&mut self, event: RegionMessage) -> Task<Message> {
        match event {
            RegionMessage::Actual => {
                self.viewer.regions.session_failed = false;
                self.viewer.regions.target_scale = Some(1. / self.dpi_scale);
                self.viewer.regions.zoom_request += 1;
                if self
                    .viewer
                    .selected
                    .as_ref()
                    .is_some_and(|asset| asset.width.max(asset.height) > 1600)
                {
                    return self.begin_regions();
                }
                return Task::none();
            }
            RegionMessage::Fit => {
                self.viewer.regions.target_scale = None;
                self.viewer.regions.zoom_request += 1;
                return Task::none();
            }
            RegionMessage::Loaded(owner, asset, version, request, result)
                if self.viewer.preview_expanded
                    && self.library.as_ref().is_some_and(|l| l.info.id == owner)
                    && self
                        .viewer
                        .selected
                        .as_ref()
                        .is_some_and(|a| a.id == asset && a.version_id == version)
                    && self.viewer.regions.request == request =>
            {
                self.viewer.regions.loading = false;
                self.viewer.regions.session_task = None;
                match result {
                    Ok(session) => {
                        self.viewer.original_size = Some((session.width, session.height));
                        self.viewer.regions.session = Some(session)
                    }
                    Err(e) => {
                        self.viewer.regions.session_failed = true;
                        self.error = Some(e);
                        return Task::none();
                    }
                }
            }
            RegionMessage::Viewport(version, viewport)
                if self.viewer.preview_expanded
                    && self
                        .viewer
                        .selected
                        .as_ref()
                        .is_some_and(|a| a.version_id == version) =>
            {
                self.viewer.regions.viewport = Some(viewport);
                if let Some(asset) = &self.viewer.selected
                    && viewport.scale.is_finite()
                    && viewport.scale * self.dpi_scale * asset.width.max(asset.height) as f32
                        > 1600.0
                    && self.viewer.regions.session.is_none()
                {
                    return self.begin_regions();
                }
            }
            RegionMessage::Tile(owner, version, request, region, result)
                if self.viewer.preview_expanded
                    && self.viewer.regions.owner == Some(owner)
                    && self.viewer.regions.version == Some(version)
                    && self.viewer.regions.request == request =>
            {
                self.viewer.regions.pending.remove(&region);
                self.viewer.regions.tile_tasks.remove(&region);
                match result {
                    Ok(handle) => {
                        self.viewer.regions.clock += 1;
                        let age = self.viewer.regions.clock;
                        self.viewer.regions.tiles.insert(region, (handle, age));
                        while self.viewer.regions.tiles.len() > 128 {
                            let candidate = self
                                .viewer
                                .regions
                                .tiles
                                .iter()
                                .filter(|(r, _)| !self.viewer.regions.visible.contains(r))
                                .min_by_key(|(_, (_, age))| *age)
                                .map(|(r, _)| *r);
                            if let Some(old) = candidate {
                                self.viewer.regions.tiles.remove(&old);
                            } else {
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        self.viewer.regions.failed.insert(region);
                        self.error = Some(e)
                    }
                }
            }
            _ => return Task::none(),
        }
        self.schedule_regions()
    }
    /// 根据目标视口安排有界图块读取，复用已有块并取消过期请求。
    fn schedule_regions(&mut self) -> Task<Message> {
        let (Some(session), Some(viewport), Some(owner), Some(version)) = (
            self.viewer.regions.session.clone(),
            self.viewer.regions.viewport,
            self.viewer.regions.owner,
            self.viewer.regions.version,
        ) else {
            return Task::none();
        };
        self.viewer.regions.visible.clear();
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
                self.viewer.regions.visible.push(ImageRegion {
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
        self.viewer.regions.visible.sort_by(|a, b| {
            let distance = |r: &ImageRegion| {
                (r.x as f64 + r.width as f64 / 2. - center.0).powi(2)
                    + (r.y as f64 + r.height as f64 / 2. - center.1).powi(2)
            };
            distance(a).total_cmp(&distance(b))
        });
        self.viewer.regions.visible.truncate(128);
        let mut tasks = Vec::new();
        for region in self.viewer.regions.visible.clone() {
            if self.viewer.regions.pending.len() >= 4 {
                break;
            }
            if self.viewer.regions.tiles.contains_key(&region)
                || self.viewer.regions.pending.contains(&region)
                || self.viewer.regions.failed.contains(&region)
            {
                continue;
            }
            self.viewer.regions.pending.insert(region);
            let request = self.viewer.regions.request;
            let service = self.service.clone();
            let session = session.clone();
            let cancelled = self.viewer.regions.cancelled.clone();
            let (task, handle) = Task::perform(
                async move {
                    service
                        .region_tile_cancellable(owner, session, region, cancelled)
                        .await
                        .map(|p| image::Handle::from_rgba(p.width, p.height, p.rgba))
                },
                move |r| Message::Regions(RegionMessage::Tile(owner, version, request, region, r)),
            )
            .abortable();
            self.viewer.regions.tile_tasks.insert(region, handle);
            tasks.push(task);
        }
        Task::batch(tasks)
    }
}
