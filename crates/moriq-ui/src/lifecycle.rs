use super::*;
use crate::components::*;
use iced::widget::{column, row, text_input};
#[derive(Default)]
pub(super) struct LifecycleState {
    orphans: Option<std::sync::Arc<moriq_app::OrphanPlan>>,
    orphan_owner: Option<LibraryId>,
    purge_confirm: bool,
    purge_owner: Option<LibraryId>,
    old_root: String,
    new_root: String,
}
#[derive(Debug, Clone)]
pub enum LifecycleMessage {
    ScanOrphans,
    OrphansScanned(
        LibraryId,
        Result<std::sync::Arc<moriq_app::OrphanPlan>, String>,
    ),
    Quarantine,
    Restore,
    ArmPurge,
    Purge,
    OrphansFinished(LibraryId, Result<OrphanReport, String>),
    Copy(bool),
    Picked(LibraryId, bool, Option<PathBuf>),
    Copied(LibraryId, Result<PathBuf, String>),
    Old(String),
    New(String),
    Reconnect,
    Reconnected(LibraryId, Result<(u64, u64), String>),
}
impl App {
    fn orphan_controls<'a>(
        &'a self,
        mut view: iced::widget::Column<'a, Message>,
    ) -> Element<'a, Message> {
        let explanation = "仅检查 assets / versions 中超过 24 小时且没有登记、导入日志或删除恢复记录引用的文件。\
            先隔离，可恢复；永久清理隔离区才释放空间。";
        view = view
            .push(heading("孤儿文件清理", 16))
            .push(muted(explanation))
            .push(action(
                "扫描孤儿文件",
                Message::Lifecycle(LifecycleMessage::ScanOrphans),
            ));

        if self.lifecycle.orphan_owner == self.library.as_ref().map(|l| l.info.id)
            && let Some(plan) = &self.lifecycle.orphans
        {
            view = view.push(muted(format!(
                "发现 {} 个候选文件，共 {} 字节。",
                plan.count, plan.bytes
            )));
            for name in &plan.examples {
                view = view.push(muted(name.clone()));
            }
            if plan.count > 0 {
                view = view.push(action(
                    "隔离这些未登记文件（可恢复）",
                    Message::Lifecycle(LifecycleMessage::Quarantine),
                ));
            }
        }
        if self.manager.organizer.quarantined_files > 0 {
            view = view
                .push(muted(format!(
                    "隔离区 {} 个文件，{} 字节。",
                    self.manager.organizer.quarantined_files,
                    self.manager.organizer.quarantined_bytes
                )))
                .push(action(
                    "恢复隔离文件到原位置",
                    Message::Lifecycle(LifecycleMessage::Restore),
                ));
            if self.lifecycle.purge_confirm
                && self.lifecycle.purge_owner == self.library.as_ref().map(|l| l.info.id)
            {
                view = view
                    .push(muted("永久删除隔离区中的这些文件后无法恢复。"))
                    .push(action(
                        "确认永久清理隔离区",
                        Message::Lifecycle(LifecycleMessage::Purge),
                    ));
            } else {
                view = view.push(action(
                    "永久清理隔离区…",
                    Message::Lifecycle(LifecycleMessage::ArmPurge),
                ));
            }
        }
        view.into()
    }
    pub(super) fn lifecycle_message(&mut self, event: LifecycleMessage) -> Task<Message> {
        let Some(library) = &self.library else {
            return Task::none();
        };
        let id = library.info.id;
        let service = self.service.clone();
        match event {
            LifecycleMessage::ScanOrphans if !self.manager.busy => {
                self.manager.busy = true;
                self.lifecycle.orphans = None;
                self.lifecycle.orphan_owner = Some(id);
                self.lifecycle.purge_confirm = false;
                return Task::perform(async move { service.orphan_plan(id).await }, move |r| {
                    Message::Lifecycle(LifecycleMessage::OrphansScanned(id, r))
                });
            }
            LifecycleMessage::OrphansScanned(owner, result) if owner == id => {
                self.manager.busy = false;
                match result {
                    Ok(plan) => self.lifecycle.orphans = Some(plan),
                    Err(e) => self.error = Some(e),
                }
            }
            LifecycleMessage::Quarantine
                if !self.manager.busy && self.lifecycle.orphan_owner == Some(id) =>
            {
                if let Some(plan) = self.lifecycle.orphans.take() {
                    self.manager.busy = true;
                    return Task::perform(
                        async move { service.quarantine_orphans(id, plan).await },
                        move |r| Message::Lifecycle(LifecycleMessage::OrphansFinished(id, r)),
                    );
                }
            }
            LifecycleMessage::ArmPurge if !self.manager.busy => {
                self.lifecycle.purge_confirm = true;
                self.lifecycle.purge_owner = Some(id);
            }
            LifecycleMessage::Restore | LifecycleMessage::Purge if !self.manager.busy => {
                let purge = matches!(event, LifecycleMessage::Purge);
                if purge
                    && (!self.lifecycle.purge_confirm || self.lifecycle.purge_owner != Some(id))
                {
                    return Task::none();
                }
                self.lifecycle.purge_confirm = false;
                self.lifecycle.purge_owner = None;
                self.manager.busy = true;
                return Task::perform(
                    async move { service.restore_orphans(id, purge).await },
                    move |r| Message::Lifecycle(LifecycleMessage::OrphansFinished(id, r)),
                );
            }
            LifecycleMessage::OrphansFinished(owner, result) if owner == id => {
                self.manager.busy = false;
                match result {
                    Ok(report) => {
                        self.manager.report = format!(
                            "处理 {} 个文件（{} 字节），跳过已改变、重新登记或不可访问的 {} 项。",
                            report.moved, report.bytes, report.skipped
                        );
                        self.manager.report_panel = Some(manage::Panel::Maintenance)
                    }
                    Err(e) => self.error = Some(e),
                }
                return self.refresh_organizer();
            }
            LifecycleMessage::Old(value) => self.lifecycle.old_root = value,
            LifecycleMessage::New(value) => self.lifecycle.new_root = value,
            LifecycleMessage::Copy(independent) if !self.manager.busy => {
                return Task::perform(
                    async move {
                        rfd::AsyncFileDialog::new()
                            .set_title(if independent {
                                "选择独立克隆位置"
                            } else {
                                "选择迁移副本位置"
                            })
                            .pick_folder()
                            .await
                            .map(|f| f.path().to_path_buf())
                    },
                    move |r| Message::Lifecycle(LifecycleMessage::Picked(id, independent, r)),
                );
            }
            LifecycleMessage::Picked(owner, independent, Some(path))
                if owner == id && !self.manager.busy =>
            {
                self.manager.busy = true;
                return Task::perform(
                    async move {
                        if independent {
                            service.clone_library(id, path).await
                        } else {
                            service.backup(id, path).await
                        }
                    },
                    move |r| Message::Lifecycle(LifecycleMessage::Copied(id, r)),
                );
            }
            LifecycleMessage::Copied(owner, result) if owner == id => {
                self.manager.busy = false;
                match result {
                    Ok(path) => {
                        self.manager.report = format!(
                            "副本已创建：{}\n独立克隆使用新的资源库身份；迁移副本保留原身份。原库保留。外部引用原片仍位于原磁盘。",
                            path.display()
                        );
                        self.manager.report_panel = Some(manage::Panel::Maintenance)
                    }
                    Err(e) => self.error = Some(e),
                }
            }
            LifecycleMessage::Reconnect if !self.manager.busy => {
                self.manager.busy = true;
                let old = PathBuf::from(self.lifecycle.old_root.trim());
                let new = PathBuf::from(self.lifecycle.new_root.trim());
                return Task::perform(
                    async move { service.reconnect_root(id, old, new).await },
                    move |r| Message::Lifecycle(LifecycleMessage::Reconnected(id, r)),
                );
            }
            LifecycleMessage::Reconnected(owner, result) if owner == id => {
                self.manager.busy = false;
                match result {
                    Ok((connected, skipped)) => {
                        self.manager.report = format!(
                            "已重新关联 {connected} 个引用版本；跳过缺失、内容不一致或期间已改变的 {skipped} 项。原片通过 SHA-256 校验后才会关联。"
                        );
                        self.manager.report_panel = Some(manage::Panel::Maintenance)
                    }
                    Err(e) => self.error = Some(e),
                }
                return Task::batch([self.refresh(), self.refresh_details()]);
            }
            _ => {}
        }
        Task::none()
    }
    pub(super) fn lifecycle_panel(&self) -> Element<'_, Message> {
        let description = "独立克隆可与原库分别使用；迁移副本保留资源库身份，切换前需关闭原库。\
            原库会保留，外部引用文件需另行迁移。";
        let view = column![
            heading("资源库克隆与迁移", 16),
            muted(description),
            row![
                action(
                    "创建独立克隆",
                    Message::Lifecycle(LifecycleMessage::Copy(true))
                ),
                action(
                    "创建迁移副本",
                    Message::Lifecycle(LifecycleMessage::Copy(false))
                ),
            ]
            .spacing(8),
            heading("离线磁盘重新关联", 16),
            muted("磁盘盘符或目录改变后，按原目录结构映射到新位置。仅关联内容校验一致的文件。"),
            text_input("原目录，例如 E:\\Photos", &self.lifecycle.old_root)
                .style(crate::theme::input)
                .padding(10)
                .on_input(|v| Message::Lifecycle(LifecycleMessage::Old(v))),
            text_input("新目录，例如 F:\\Photos", &self.lifecycle.new_root)
                .style(crate::theme::input)
                .padding(10)
                .on_input(|v| Message::Lifecycle(LifecycleMessage::New(v))),
            action(
                "校验并重新关联整个目录",
                Message::Lifecycle(LifecycleMessage::Reconnect)
            ),
        ]
        .spacing(10);

        self.orphan_controls(view)
    }
}
