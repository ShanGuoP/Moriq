//! Owns preview requests, region cancellation and video resources as one lifecycle.
use super::*;

#[derive(Default)]
pub(super) struct ViewerState {
    pub regions: regions::Regions,
    pub selected: Option<Asset>,
    pub preview: Option<image::Handle>,
    pub preview_background: Option<image::Handle>,
    pub preview_expanded: bool,
    pub original_request: u64,
    pub original_size: Option<(u32, u32)>,
    pub playback: Option<std::sync::Arc<moriq_app::Playback>>,
    pub video_paused: bool,
    pub video_muted: bool,
    pub video_presented: u64,
    pub video_first_ready_ms: Option<u128>,
    pub video_frame: Option<iced_runtime::image::Allocation>,
    pub video_upload: bool,
    pub video_control: u64,
    pub preview_task: Option<iced::task::Handle>,
}
impl ViewerState {
    /// Invalidate original-media receipts and release region/video resources.
    ///
    /// 停止视频播放并释放原图会话等媒体资源。
    pub fn stop_media(&mut self) {
        self.regions.reset();
        self.original_request = self.original_request.wrapping_add(1);
        self.original_size = None;
        self.playback = None;
        self.video_frame = None;
        self.video_upload = false;
        self.video_paused = false;
        self.video_presented = 0;
        self.video_first_ready_ms = None;
        self.video_control = self.video_control.wrapping_add(1);
    }
    /// 开始指定素材的视频播放，绑定库、版本与播放请求身份。
    pub fn start_playback(&mut self, session: std::sync::Arc<moriq_app::Playback>) {
        self.video_paused = false;
        self.video_muted = false;
        self.video_presented = 0;
        self.video_first_ready_ms = None;
        self.video_frame = None;
        self.video_upload = false;
        self.playback = Some(session);
    }
    /// 中止尚未完成的预览读取并使旧回执失效。
    pub fn cancel_preview(&mut self) {
        if let Some(handle) = self.preview_task.take() {
            handle.abort();
        }
    }
    /// 切换当前查看素材并申请对应预览，清理上一素材的专属媒体状态。
    pub fn focus(&mut self) {
        self.stop_media();
        self.cancel_preview();
    }
    /// 关闭展开查看器，按既有策略保留可复用的适应窗口预览。
    pub fn close(&mut self) {
        self.stop_media();
        self.preview_expanded = false;
    }
    /// 完全清除查看器图片、选择关联及媒体回执状态。
    pub fn clear(&mut self) {
        self.close();
        self.cancel_preview();
        self.selected = None;
        self.preview = None;
        self.preview_background = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    /// 验证关闭保留适应窗口预览，完全清除会释放图像并使媒体回执失效。
    fn close_preserves_fit_preview_while_clear_discards_it_and_invalidates_media() {
        let mut viewer = ViewerState {
            selected: Some(crate::tests::selected_asset()),
            preview: Some(image::Handle::from_rgba(1, 1, vec![0; 4])),
            preview_expanded: true,
            original_size: Some((3000, 2000)),
            video_upload: true,
            video_paused: true,
            video_presented: 5,
            video_first_ready_ms: Some(1),
            ..Default::default()
        };
        let request = viewer.original_request;
        viewer.close();
        assert!(viewer.selected.is_some());
        assert!(viewer.preview.is_some());
        assert!(!viewer.preview_expanded);
        assert!(viewer.original_request > request);
        assert!(viewer.original_size.is_none());
        assert!(!viewer.video_upload);
        assert_eq!(viewer.video_presented, 0);
        assert!(!viewer.video_paused);
        let request = viewer.original_request;
        viewer.clear();
        assert!(viewer.selected.is_none());
        assert!(viewer.preview.is_none());
        assert!(viewer.original_request > request);
    }
}
