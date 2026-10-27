//! Startup metadata arrives incrementally; one unavailable library cannot hold every card.
use super::*;
use iced::futures::{StreamExt, stream};

impl App {
    /// 异步加载最近资源库卡片与封面，不在启动界面同步读取所有库。
    pub(super) fn load_library_cards(&mut self) -> Task<Message> {
        if let Some(task) = self.library_cards_task.take() {
            task.abort();
        }
        self.library_cards_request = self.library_cards_request.wrapping_add(1);
        let request = self.library_cards_request;
        let paths: Vec<_> = self
            .preferences
            .recent
            .iter()
            .map(|(_, path)| path.clone())
            .collect();
        self.library_covers.retain(|path, _| paths.contains(path));
        self.library_summaries
            .retain(|path, _| paths.contains(path));
        let service = self.service.clone();
        let stream = stream::iter(paths)
            .map(move |path| {
                let service = service.clone();
                async move {
                    Message::LibraryCardsLoaded(
                        request,
                        service.library_card_details(vec![path]).await,
                    )
                }
            })
            .buffer_unordered(3);
        let (task, handle) = Task::run(stream, |message| message).abortable();
        self.library_cards_task = Some(handle);
        task
    }
}
