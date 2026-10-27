//! Opt-in native GPU screenshots exercise the normal viewer and surface actions.
use super::*;
impl App {
    /// 按显式诊断场景触发真实界面动作。
    pub(super) fn motion_proof_step(&mut self, step: u8) -> Task<Message> {
        let (action, delay, next) = match step {
            0 => (Some(Message::ExpandPreview), 250, 1),
            2 => (Some(Message::Regions(RegionMessage::Actual)), 35, 3),
            5 => (Some(Message::Regions(RegionMessage::Fit)), 35, 6),
            8 => {
                let close = self.update(Message::ExpandPreview);
                let open = self.update(Message::Manage(ManageMessage::Filters));
                return Task::batch([close, open, Self::motion_proof_after(70, 9)]);
            }
            9 => {
                let before = (self.backdrop_alpha(), self.surface_offset());
                let open = self.update(Message::ShowSettings);
                tracing::info!(
                    before_dim = before.0,
                    after_dim = self.backdrop_alpha(),
                    before_offset = before.1,
                    after_offset = self.surface_offset(),
                    "motion-surface-retarget"
                );
                return Task::batch([open, Self::motion_proof_after(45, 10)]);
            }
            11 => (Some(Message::Back), 200, 12),
            13 => {
                let close = self.update(Message::Back);
                return Task::batch([close, Self::motion_proof_after(200, 14)]);
            }
            14 => {
                return iced::window::oldest()
                    .and_then(iced::window::screenshot)
                    .map(Message::Captured);
            }
            _ => {
                return iced::window::oldest()
                    .and_then(iced::window::screenshot)
                    .map(move |frame| Message::MotionFrame(step, frame));
            }
        };
        let action = action
            .map(|message| self.update(message))
            .unwrap_or_else(Task::none);
        Task::batch([action, Self::motion_proof_after(delay, next)])
    }
    /// 在动作完成后安排下一次诊断采样。
    fn motion_proof_after(delay: u64, next: u8) -> Task<Message> {
        Task::perform(
            async move {
                tokio::time::sleep(Duration::from_millis(delay)).await;
            },
            move |_| Message::MotionProof(next),
        )
    }
    /// 采集当前动画时刻的诊断帧和状态证据。
    pub(super) fn motion_proof_frame(
        &self,
        step: u8,
        screenshot: iced::window::Screenshot,
    ) -> Task<Message> {
        let Some(path) = self.launch.capture.as_ref() else {
            return Task::none();
        };
        let path = path.with_file_name(format!("interaction-{step}.png"));
        let service = self.service.clone();
        Task::perform(
            async move {
                service
                    .save_capture(
                        path,
                        screenshot.size.width,
                        screenshot.size.height,
                        screenshot.rgba.to_vec(),
                    )
                    .await?;
                let (delay, next) = match step {
                    1 => (40, 2),
                    3 => (180, 4),
                    4 => (40, 5),
                    6 => (180, 7),
                    7 => (40, 8),
                    10 => (180, 11),
                    12 => (40, 13),
                    _ => return Err("未知动效诊断步骤".into()),
                };
                tokio::time::sleep(Duration::from_millis(delay)).await;
                Ok(next)
            },
            |result: Result<u8, String>| match result {
                Ok(next) => Message::MotionProof(next),
                Err(error) => Message::Notice(error),
            },
        )
    }
}
