use super::*;
use crate::components::*;
use iced::widget::column;
impl App {
    pub(super) fn tasks_panel(&self) -> Element<'_, Message> {
        let mut view = column![muted(
            "此处显示当前导入、资源检查、空间统计与 Synco 交接。AI 分析任务尚未启用。"
        )]
        .spacing(12);
        let mut count = 0;
        if let Some(control) = &self.import {
            count += 1;
            view = view.push(heading("图片导入", 15)).push(muted(format!(
                "已处理 {}，已发现 {}，同时处理 {}",
                control.finished.load(Ordering::Relaxed),
                control.discovered.load(Ordering::Relaxed),
                control.active.load(Ordering::Relaxed)
            )));
            if !control.cancel.load(Ordering::Relaxed) {
                view = view.push(action("停止导入", Message::CancelImport));
            } else {
                view = view.push(muted("正在停止并收尾…"));
            }
        }
        if self.manager.maintenance.is_some() {
            count += 1;
            view = view.push(action(
                "查看资源检查进度",
                Message::Manage(ManageMessage::Maintenance),
            ));
        }
        if self.storage.control.is_some() {
            count += 1;
            view = view.push(action(
                "查看空间统计进度",
                Message::Storage(StorageMessage::Open),
            ));
        }
        let pending = self
            .manager
            .organizer
            .synco
            .iter()
            .filter(|t| {
                !matches!(t.state.as_str(), "delivered" | "failed" | "cancelled") || t.ack.is_some()
            })
            .count();
        if pending > 0 {
            count += pending;
            view = view.push(action(
                &format!("查看 {} 个 Synco 交接任务", pending),
                Message::Manage(ManageMessage::Synco),
            ));
        }
        if count == 0 {
            view = view.push(muted("当前没有后台任务。"));
        }
        view.into()
    }
}
