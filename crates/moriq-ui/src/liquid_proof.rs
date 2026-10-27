//! Native capture diagnostics for the real widget, only in development capture builds.
use super::*;

impl App {
    /// 按诊断步骤触发真实选中操作，供形变验收使用。
    pub(super) fn liquid_proof_step(&mut self, step: u8) -> Task<Message> {
        if step == 0 {
            return Task::batch([
                self.update(Message::ModelFilter(0)),
                Task::perform(
                    async {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    },
                    |_| Message::LiquidProof(10),
                ),
            ]);
        }
        if step == 1 {
            return Task::batch([
                self.update(Message::ModelFilter(2)),
                Task::perform(
                    async {
                        tokio::time::sleep(Duration::from_millis(45)).await;
                    },
                    |_| Message::LiquidProof(2),
                ),
            ]);
        }
        let frame = if step == 10 { 0 } else { step };
        iced::window::oldest()
            .and_then(iced::window::screenshot)
            .map(move |screenshot| Message::LiquidFrame(frame, screenshot))
    }

    /// 按采样时刻记录选中形变帧证据。
    pub(super) fn liquid_proof_frame(
        &mut self,
        step: u8,
        screenshot: iced::window::Screenshot,
    ) -> Task<Message> {
        let Some(path) = self.launch.capture.as_ref() else {
            return Task::none();
        };
        let path = path.with_file_name(format!("motion-{step}.png"));
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
                    0 => (180, 1),
                    2 => (60, 3),
                    3 => (80, 4),
                    4 => (300, 5),
                    _ => return Ok(None),
                };
                tokio::time::sleep(Duration::from_millis(delay)).await;
                Ok(Some(next))
            },
            |result: Result<Option<u8>, String>| match result {
                Ok(Some(step)) => Message::LiquidProof(step),
                Ok(None) => Message::Noop,
                Err(error) => Message::Notice(error),
            },
        )
    }
}
