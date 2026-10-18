use super::*;
use crate::{components::*, theme::*};
use iced::widget::{Space, button, column, row, text};
use iced::{Alignment, Length};
#[derive(Default)]
pub(super) struct Storage {
    pub stats: Option<StorageStats>,
    pub volume: Option<(u64, u64)>,
    pub control: Option<moriq_app::StorageControl>,
    request: u64,
    volume_request: u64,
}
impl Storage {
    pub fn reset(&mut self) {
        if let Some(control) = &self.control {
            control.cancel.store(true, Ordering::Relaxed);
        }
        *self = Self {
            request: self.request + 1,
            volume_request: self.volume_request + 1,
            ..Default::default()
        };
    }
}
#[derive(Debug, Clone)]
pub enum StorageMessage {
    Open,
    Refresh,
    Stop,
    Loaded(LibraryId, u64, Result<StorageStats, String>),
    Volume(LibraryId, u64, Result<(u64, u64), String>),
}
impl App {
    pub(super) fn load_volume(&mut self) -> Task<Message> {
        let Some(lib) = &self.library else {
            return Task::none();
        };
        let id = lib.info.id;
        self.storage.volume_request += 1;
        let request = self.storage.volume_request;
        let service = self.service.clone();
        Task::perform(async move { service.volume(id).await }, move |r| {
            Message::Storage(StorageMessage::Volume(id, request, r))
        })
    }
    pub(super) fn storage_message(&mut self, event: StorageMessage) -> Task<Message> {
        match event {
            StorageMessage::Open => {
                self.manager.panel = Some(manage::Panel::Storage);
                if self.storage.stats.is_none() && self.storage.control.is_none() {
                    return self.storage_message(StorageMessage::Refresh);
                }
            }
            StorageMessage::Refresh if self.storage.control.is_none() => {
                if let Some(lib) = &self.library {
                    let id = lib.info.id;
                    self.storage.request += 1;
                    let request = self.storage.request;
                    let control = moriq_app::StorageControl::default();
                    self.storage.control = Some(control.clone());
                    let service = self.service.clone();
                    return Task::perform(
                        async move { service.storage(id, control).await },
                        move |r| Message::Storage(StorageMessage::Loaded(id, request, r)),
                    );
                }
            }
            StorageMessage::Stop => {
                if let Some(control) = &self.storage.control {
                    control.cancel.store(true, Ordering::Relaxed);
                }
            }
            StorageMessage::Loaded(id, request, result)
                if self.library.as_ref().is_some_and(|l| l.info.id == id)
                    && self.storage.request == request =>
            {
                self.storage.control = None;
                match result {
                    Ok(stats) => {
                        self.storage.volume = Some((stats.volume_total, stats.volume_available));
                        self.storage.stats = Some(stats);
                    }
                    Err(e) => self.error = Some(e),
                }
            }
            StorageMessage::Volume(id, request, result)
                if self.library.as_ref().is_some_and(|l| l.info.id == id)
                    && self.storage.volume_request == request =>
            {
                match result {
                    Ok(volume) => self.storage.volume = Some(volume),
                    Err(e) => self.status = format!("卷容量暂不可用：{e}"),
                }
            }
            _ => {}
        }
        Task::none()
    }
    pub(super) fn storage_panel(&self) -> Element<'_, Message> {
        let mut content=column![muted("统计当前资源库内的实际文件分配空间，硬链接按文件身份去重。外部引用原片仅显示登记量，不扫描来源磁盘；采样期间文件可能变化。")].spacing(12);
        if let Some(control) = &self.storage.control {
            content = content.push(
                row![
                    muted(format!(
                        "已检查 {} 个文件",
                        control.files.load(Ordering::Relaxed)
                    )),
                    action("停止统计", Message::Storage(StorageMessage::Stop))
                ]
                .spacing(12),
            );
        }
        if let Some(stats) = &self.storage.stats {
            content = content.push(heading(
                format!(
                    "库内分配空间 {}{}",
                    size(stats.allocated),
                    if stats.cancelled {
                        " · 已停止，部分结果"
                    } else {
                        ""
                    }
                ),
                18,
            ));
            for (category, bytes) in &stats.categories {
                content = content.push(
                    row![
                        text(category).size(13),
                        Space::new().width(Length::Fill),
                        muted(size(*bytes))
                    ]
                    .align_y(Alignment::Center),
                );
            }
            for (label, value) in [
                ("库内唯一文件字节量", size(stats.unique_file_bytes)),
                ("托管版本登记量", size(stats.managed_registered)),
                ("外部引用登记量（库外）", size(stats.referenced_registered)),
                ("所在卷容量", size(stats.volume_total)),
                ("所在卷可用空间", size(stats.volume_available)),
                ("硬链接重复路径", stats.linked_paths.to_string()),
                ("未统计/变化/跳过的条目", stats.warnings.to_string()),
                ("采样时间", date(stats.measured_at)),
            ] {
                content = content.push(row![
                    muted(label),
                    Space::new().width(Length::Fill),
                    text(value).size(13)
                ]);
            }
        }
        content
            .push(
                button(text("重新统计").size(13))
                    .padding(9)
                    .style(secondary)
                    .on_press_maybe(
                        self.storage
                            .control
                            .is_none()
                            .then_some(Message::Storage(StorageMessage::Refresh)),
                    ),
            )
            .push(action(
                "检查或修复缓存",
                Message::Manage(ManageMessage::Maintenance),
            ))
            .push(muted("恢复日志、交接副本和 AI 成果不会作为缓存自动删除。"))
            .into()
    }
}
