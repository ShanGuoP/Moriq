use super::*;
use std::ops::Range;
impl App {
    /// 计算扣除侧栏和详情栏后的图库可用宽度。
    pub(super) fn gallery_width(&self) -> f32 {
        (self.viewport.width
            - self.frame_padding() * 2.0
            - self.sidebar_width()
            - self.details_width()
            - 64.0) // Page padding plus the embedded 10 px scrollbar and its 6 px gap.
            .max(220.0)
    }
    /// 按窗口与当前状态计算详情栏宽度。
    pub(super) fn details_width(&self) -> f32 {
        if self.viewport.width < 1280.0 {
            270.0
        } else {
            306.0
        }
    }
    /// 返回当前布局需要的侧栏宽度。
    pub(super) fn sidebar_width(&self) -> f32 {
        248.0
    }
    /// 返回当前窗口内容区的外侧留白。
    pub(super) fn frame_padding(&self) -> f32 {
        0.0
    }
    /// 返回标题栏在当前布局中的占用高度。
    pub(super) fn titlebar_height(&self) -> f32 {
        64.0
    }
    /// 计算图库视口的有效尺寸，统一布局和可见范围判断。
    pub(super) fn gallery_geometry(&self) -> (usize, f32) {
        let width = self.gallery_width();
        let columns = (width / (self.card_width + 10.0)).floor().clamp(1.0, 8.0) as usize;
        (
            columns,
            (width - (columns - 1) as f32 * 10.0) / columns as f32,
        )
    }
    /// 仅返回与当前查询及几何参数一致的瀑布流索引。
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
    /// 根据当前布局和滚动位置计算需要加载的素材范围。
    pub(super) fn visible_range(&self) -> Range<usize> {
        self.active_masonry()
            .map(|layout| layout.page_range(self.scroll_y, self.grid().viewport_height))
            .unwrap_or_else(|| self.grid().visible(self.total as usize, self.scroll_y))
    }
    /// 异步读取尺寸并重建瀑布流索引，拒绝旧查询与旧几何回执。
    pub(super) fn rebuild_masonry(&mut self) -> Task<Message> {
        if !self.masonry_mode
            || self.list
            || self.screen != Screen::Assets
            || self.library_picker
            || self.responsiveness.pending()
        {
            return Task::none();
        }
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
        let task = Task::perform(
            async move { service.masonry(id, query, columns, width).await },
            move |result| Message::MasonryLoaded(generation, result),
        );
        let (task, handle) = task.abortable();
        self.masonry_task = Some(handle);
        task
    }
}
