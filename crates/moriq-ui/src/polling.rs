//! Lightweight background polling, separate from diagnostic capture scenarios.
use super::*;
impl App {
    /// 按当前页面与在途任务状态安排必要轮询，避免隐藏页面重复读取。
    pub(super) fn tick(&mut self) -> Task<Message> {
        let language = self.preferences.language;
        if self.drop_import.has_ready() && self.import.is_none() && self.can_start_drop() {
            return self.start_next_drop();
        }
        if self.manager.panel != Some(manage::Panel::People) {
            self.stop_people_portraits();
        } else if self.ai.portrait_cancel.load(Ordering::Acquire) {
            return self.refresh_people_portraits();
        }
        if (self.screen != Screen::Assets || self.library_picker)
            && (self.viewer.preview_expanded
                || self.viewer.playback.is_some()
                || self.viewer.regions.session.is_some())
        {
            self.stop_viewer();
        }
        if self
            .viewer
            .playback
            .as_ref()
            .is_some_and(|p| !p.running().unwrap_or(false))
        {
            self.viewer.playback = None;
            self.viewer.video_frame = None;
            self.viewer.video_upload = false;
            self.error = Some(tr!(language, "内置播放器已停止。请重新打开视频；原文件保留。" => "The built-in player stopped. Reopen the video to retry. The original file is preserved.").into());
        }
        if !self.ai.loaded
            && !self.ai.loading
            && (self.library.is_some() || self.screen == Screen::Models)
        {
            return self.ai_message(AiMessage::Refresh);
        }
        if let Some(import) = &self.import {
            let finished = import.finished.load(Ordering::Relaxed);
            if finished > self.manager.import_seen
                && !self.loading
                && self
                    .manager
                    .import_refreshed
                    .is_none_or(|last| last.elapsed() >= Duration::from_secs(1))
            {
                self.manager.import_seen = finished;
                self.manager.import_refreshed = Some(std::time::Instant::now());
                return Task::batch([self.refresh(), self.refresh_info()]);
            }
        }
        if self.manager.synco.poll_due()
            && self.manager.organizer.synco.iter().any(|t| {
                t.ack.is_some()
                    || ["queued", "importing", "editing", "rendering", "ready"]
                        .contains(&t.state.as_str())
            })
        {
            return self.manage_message(ManageMessage::SyncoPoll(false));
        }
        #[cfg(feature = "capture")]
        if self.launch.capture.is_some()
            && (!self.loading
                || self.import.is_some()
                || self.launch.panel.as_deref() == Some("interaction-continuity"))
            && !self.switching
        {
            return self.capture_tick();
        }
        self.job_tick()
    }
}
