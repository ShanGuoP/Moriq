//! One in-app confirmation owns the frozen target throughout both confirmation steps.
use super::*;
use crate::{components::*, manage::Panel, theme::*};
use iced::widget::{button, column, row, text};
#[derive(Clone)]
enum Target {
    Project(ProjectDeletionPlan),
    Assets(Vec<AssetId>),
    AllTrash(Vec<AssetId>),
}
#[derive(Default)]
pub(super) struct Deletion {
    request: u64,
    library: Option<LibraryId>,
    target: Option<Target>,
    project: Option<ProjectId>,
    previous: Option<Panel>,
    loading: bool,
    running: bool,
    pub final_step: bool,
    error: Option<String>,
}
impl Deletion {
    /// 清除删除确认计划、错误和提交状态。
    pub fn reset(&mut self) {
        *self = Self {
            request: self.request + 1,
            ..Default::default()
        };
    }
}
#[derive(Debug, Clone)]
pub enum DeletionMessage {
    EmptyTrash,
    TrashPlanned(LibraryId, u64, Result<Vec<AssetId>, String>),
    Project(ProjectId),
    Assets(Vec<AssetId>),
    Planned(LibraryId, u64, Result<ProjectDeletionPlan, String>),
    Next,
    Previous,
    Confirm,
    Cancel,
    Finished(LibraryId, u64, Result<PurgeReport, String>),
}
impl App {
    /// 处理删除计划加载、二次确认与完成回执，拒绝旧库或旧范围结果。
    pub(super) fn deletion_message(&mut self, event: DeletionMessage) -> Task<Message> {
        let language = self.preferences.language;
        use DeletionMessage::*;
        match event {
            EmptyTrash => {
                if self.manager.busy || self.switching || self.import.is_some() {
                    return Task::none();
                }
                let Some(library) = self.library.as_ref().map(|l| l.info.id) else {
                    return Task::none();
                };
                self.deletion.reset();
                self.deletion.library = Some(library);
                self.deletion.previous = self.manager.panel;
                self.deletion.loading = true;
                self.manager.panel = Some(Panel::Delete);
                let request = self.deletion.request;
                let service = self.service.clone();
                return Task::perform(
                    async move { service.trash_snapshot(library).await },
                    move |r| Message::Deletion(TrashPlanned(library, request, r)),
                );
            }
            TrashPlanned(library, request, result) => {
                if !self.deletion_matches(library, request) || !self.deletion.loading {
                    return Task::none();
                }
                self.deletion.loading = false;
                match result {
                    Ok(ids) if !ids.is_empty() => {
                        self.deletion.target = Some(Target::AllTrash(ids))
                    }
                    Ok(_) => {
                        self.deletion.error =
                            Some(tr!(language, "回收站已为空" => "Trash is empty").into())
                    }
                    Err(e) => self.deletion.error = Some(e),
                }
            }
            Project(project) => {
                if self.manager.busy || self.switching || self.import.is_some() {
                    return Task::none();
                }
                let Some(library) = self.library.as_ref().map(|l| l.info.id) else {
                    return Task::none();
                };
                let previous = if self.manager.panel == Some(Panel::Delete) {
                    self.deletion.previous
                } else {
                    self.manager.panel
                };
                self.deletion.reset();
                self.deletion.library = Some(library);
                self.deletion.project = Some(project);
                self.deletion.previous = previous;
                self.deletion.loading = true;
                self.manager.panel = Some(Panel::Delete);
                let request = self.deletion.request;
                let service = self.service.clone();
                return Task::perform(
                    async move { service.project_deletion_plan(library, project).await },
                    move |r| Message::Deletion(Planned(library, request, r)),
                );
            }
            Assets(ids) => {
                if self.manager.busy || self.switching || !self.query.trash {
                    return Task::none();
                }
                let Some(library) = self.library.as_ref().map(|l| l.info.id) else {
                    return Task::none();
                };
                if ids.is_empty() {
                    self.error = Some(
                        tr!(language, "请先选择回收站内的素材" => "Select an asset in Trash first")
                            .into(),
                    );
                    return Task::none();
                }
                self.deletion.reset();
                self.deletion.library = Some(library);
                self.deletion.previous = self.manager.panel;
                self.deletion.target = Some(Target::Assets(ids));
                self.manager.panel = Some(Panel::Delete);
            }
            Planned(library, request, result) => {
                if !self.deletion_matches(library, request) || !self.deletion.loading {
                    return Task::none();
                }
                self.deletion.loading = false;
                match result {
                    Ok(plan) if Some(plan.root) == self.deletion.project => {
                        self.deletion.target = Some(Target::Project(plan))
                    }
                    Ok(_) => {
                        self.deletion.error = Some(tr!(language, "项目目标已变化，请重新打开删除确认。" => "The target project has changed. Please reopen the deletion confirmation.").into())
                    }
                    Err(e) => self.deletion.error = Some(e),
                }
            }
            Next => {
                if self.deletion_ready() {
                    self.deletion.final_step = true;
                }
            }
            Previous => {
                if self.deletion.running {
                    return Task::none();
                }
                if self.deletion.final_step {
                    self.deletion.final_step = false;
                } else {
                    return self.deletion_message(Cancel);
                }
            }
            Confirm => {
                if !self.deletion_ready() || !self.deletion.final_step {
                    return Task::none();
                }
                let Some(target) = self.deletion.target.clone() else {
                    return Task::none();
                };
                let Some(library) = self.deletion.library else {
                    return Task::none();
                };
                let request = self.deletion.request;
                let service = self.service.clone();
                self.deletion.running = true;
                self.manager.busy = true;
                return Task::perform(
                    async move {
                        match target {
                            Target::Project(plan) => {
                                service.delete_project_contents(library, plan).await
                            }
                            Target::Assets(ids) => service.purge(library, ids).await,
                            Target::AllTrash(ids) => service.empty_trash(library, ids).await,
                        }
                    },
                    move |r| Message::Deletion(Finished(library, request, r)),
                );
            }
            Finished(library, request, result) => {
                if !self.deletion_matches(library, request) || !self.deletion.running {
                    return Task::none();
                }
                self.deletion.running = false;
                self.manager.busy = false;
                match result {
                    Ok(report) => {
                        let target = self.deletion.target.take();
                        self.deletion.reset();
                        self.manager.panel = None;
                        self.dialog = None;
                        return match target {
                            Some(Target::Project(plan)) => self.manage_message(
                                ManageMessage::ProjectDeleted(library, plan.projects, Ok(report)),
                            ),
                            Some(Target::Assets(_) | Target::AllTrash(_)) => {
                                self.manage_message(ManageMessage::Purged(library, Ok(report)))
                            }
                            _ => Task::none(),
                        };
                    }
                    Err(e) => {
                        self.deletion.error = Some(e);
                        self.deletion.final_step = false;
                    }
                }
            }
            Cancel => {
                if self.manager.panel != Some(Panel::Delete) || self.deletion.running {
                    return Task::none();
                }
                let previous = self.deletion.previous;
                self.deletion.reset();
                self.manager.panel = previous;
            }
        }
        Task::none()
    }
    /// 判断已加载删除计划是否仍对应当前目标项目和库身份。
    fn deletion_matches(&self, library: LibraryId, request: u64) -> bool {
        self.manager.panel == Some(Panel::Delete)
            && self.deletion.library == Some(library)
            && self.deletion.request == request
            && self.library.as_ref().is_some_and(|l| l.info.id == library)
    }
    /// 检查当前删除计划和操作状态是否允许提交。
    fn deletion_ready(&self) -> bool {
        self.deletion
            .library
            .is_some_and(|id| self.deletion_matches(id, self.deletion.request))
            && self.deletion.target.is_some()
            && !self.deletion.loading
            && !self.deletion.running
            && self.deletion.error.is_none()
            && !self.switching
    }
    /// 展示项目删除范围、共享素材影响和确认操作。
    pub(super) fn deletion_panel(&self) -> Element<'_, Message> {
        let language = self.preferences.language;
        let mut content = column![].spacing(14);
        if self.deletion.loading {
            content = content.push(muted(
                tr!(language, "正在检查删除范围…" => "Checking deletion scope…"),
            ));
        }
        if let Some(target) = &self.deletion.target {
            match target {
                Target::AllTrash(ids) => {
                    content=content.push(heading(tr!(language, "清空整库回收站" => "Empty library trash"),18)).push(text(trf!(language, "将永久删除确认时回收站中的 {} 项素材及其全部版本，保留库外引用原片。期间被恢复的素材会跳过。" => "Permanently delete the {} assets currently in Trash and all their versions. External originals remain untouched. Assets restored during deletion will be skipped.",ids.len())).size(14));
                }
                Target::Project(plan) => {
                    content = content.push(heading(
                        trf!(language, "删除项目「{}」" => "Delete project “{}”", plan.name),
                        18,
                    ));
                    if plan.projects.len() > 1 {
                        content = content.push(muted(trf!(language, "同时删除 {} 个子项目。" => "Also delete {} subprojects.",
                            plan.projects.len() - 1
                        )));
                    }
                    if plan.assets.is_empty() {
                        content = content.push(
                            text(tr!(language, "没有需要删除的独占图片，将移除项目及其归属关系。" => "There are no exclusive images to delete. The project and its memberships will be removed.")).size(14),
                        );
                    } else {
                        content=content.push(text(trf!(language, "将永久删除 {} 张独占图片及其全部 {} 个版本，清除库内文件与数据库记录。" => "Permanently delete {} exclusive images and all {} versions, removing library files and catalog records.",plan.assets.len(),plan.versions.len())).size(14));
                    }
                    if !plan.shared.is_empty() {
                        content = content.push(muted(trf!(language, "{} 张图片也属于其他项目，将保留这些图片和其他项目的关系。" => "{} images also belong to other projects. These images and their other memberships will be kept.",
                            plan.shared.len()
                        )));
                    }
                    if plan.referenced > 0 {
                        content = content
                            .push(muted(trf!(language, "{} 个外部引用原文件保留。" => "{} external original files will be kept.", plan.referenced)));
                    }
                }
                Target::Assets(ids) => {
                    content = content
                        .push(heading(trf!(language, "永久删除 {} 张图片" => "Permanently delete {} images", ids.len()), 18))
                        .push(muted(
                            tr!(language, "将删除所选图片的托管原片、全部版本和库内副本；外部引用原文件保留。" => "Delete the selected images' managed originals, all versions and library copies. External original files will be kept."),
                        ));
                }
            }
            if self.deletion.final_step {
                content = content.push(
                    text(
                        if matches!(target,Target::Project(plan) if plan.assets.is_empty()) {
                            tr!(language, "请再次确认。删除后无法恢复此项目。" => "Confirm again. This project cannot be restored after deletion.")
                        } else {
                            tr!(language, "请再次确认。删除后无法从回收站恢复，图片需要重新导入。" => "Confirm again. These images cannot be restored from Trash and will need to be imported again.")
                        },
                    )
                    .size(14)
                    .style(crate::theme::text_color(iced::Color::from_rgb8(190, 38, 38))),
                );
            } else {
                content = content.push(muted(tr!(language, "请核对范围，下一步将再次确认。" => "Check the scope. A final confirmation follows.")));
            }
        }
        if let Some(error) = &self.deletion.error {
            content = content.push(text(error).size(14).style(crate::theme::text_color(
                iced::Color::from_rgb8(190, 38, 38),
            )));
            if let Some(project) = self.deletion.project {
                content = content.push(action(
                    tr!(language, "重试" => "Retry"),
                    Message::Deletion(DeletionMessage::Project(project)),
                ));
            }
        }
        if self.deletion.running {
            content = content.push(muted(
                tr!(language, "正在删除，请稍候…" => "Deleting, please wait…"),
            ));
        }
        content
            .push(
                row![
                    button(tr!(language, "取消" => "Cancel"))
                        .padding([9, 16])
                        .style(secondary)
                        .on_press_maybe(
                            (!self.deletion.running)
                                .then_some(Message::Deletion(DeletionMessage::Cancel))
                        ),
                    button(if self.deletion.final_step {
                        tr!(language, "永久删除" => "Delete permanently")
                    } else {
                        tr!(language, "继续确认" => "Continue")
                    })
                    .padding([9, 16])
                    .style(destructive)
                    .on_press_maybe(self.deletion_ready().then_some(Message::Deletion(
                        if self.deletion.final_step {
                            DeletionMessage::Confirm
                        } else {
                            DeletionMessage::Next
                        }
                    )))
                ]
                .spacing(12),
            )
            .into()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    /// 验证回收站删除成功后关闭确认界面，并丢弃无效菜单历史。
    fn successful_trash_deletion_closes_the_confirmation_and_discards_menu_history() {
        let mut app = crate::tests::app();
        app.screen = Screen::Assets;
        app.query.trash = true;
        let a = crate::tests::selected_asset();
        app.viewer.selected = Some(a.clone());
        app.tiles = vec![Tile {
            asset: a.clone(),
            handle: None,
        }];
        drop(app.update(Message::AssetContext(a.id)));
        drop(app.update(Message::Manage(ManageMessage::Purge)));
        let request = app.deletion.request;
        let owner = app.library.as_ref().unwrap().info.id;
        drop(app.update(Message::Deletion(DeletionMessage::Next)));
        drop(app.update(Message::Deletion(DeletionMessage::Confirm)));
        drop(app.update(Message::Deletion(DeletionMessage::Finished(
            owner,
            request,
            Ok(PurgeReport {
                deleted: 1,
                cleanup_pending: false,
            }),
        ))));
        assert!(app.manager.panel.is_none());
        assert!(app.dialog.is_none());
        assert!(!app.surface_has_parent());
        assert!(app.deletion.target.is_none());
        assert!(!app.manager.busy);
    }
    /// 构造指定项目和素材范围的测试删除计划。
    fn plan(root: ProjectId) -> ProjectDeletionPlan {
        ProjectDeletionPlan {
            root,
            name: "空项目".into(),
            projects: vec![root],
            assets: vec![],
            shared: vec![],
            versions: vec![],
            aliases: vec![],
            referenced: 0,
        }
    }
    #[test]
    /// 验证两步删除确认冻结同一目标，可取消并拒绝旧回执。
    fn both_confirmation_steps_freeze_target_cancel_and_reject_stale_receipts() {
        let mut app = crate::tests::app();
        let library = app.library.as_ref().unwrap().info.id;
        let project = ProjectId::default();
        let selected = AssetId::default();
        app.manager.selection.assets.insert(selected);
        app.manager.panel = Some(Panel::Project);
        drop(app.deletion_message(DeletionMessage::Project(project)));
        let request = app.deletion.request;
        drop(app.deletion_message(DeletionMessage::Planned(
            library,
            request - 1,
            Ok(plan(project)),
        )));
        drop(app.deletion_message(DeletionMessage::Planned(
            LibraryId::default(),
            request,
            Ok(plan(project)),
        )));
        assert!(app.deletion.loading);
        drop(app.deletion_message(DeletionMessage::Planned(
            library,
            request,
            Ok(plan(project)),
        )));
        assert!(app.deletion_ready());
        drop(app.deletion_message(DeletionMessage::Confirm));
        assert!(!app.deletion.running);
        drop(app.deletion_message(DeletionMessage::Next));
        assert!(app.deletion.final_step);
        app.manager.project_target = Some(ProjectId::default());
        assert!(matches!(&app.deletion.target,Some(Target::Project(p)) if p.root==project));
        drop(app.deletion_message(DeletionMessage::Cancel));
        assert_eq!(app.manager.panel, Some(Panel::Project));
        assert!(app.manager.selection.assets.contains(&selected));
        assert!(app.deletion.target.is_none());
        drop(app.deletion_message(DeletionMessage::Planned(
            library,
            request,
            Ok(plan(project)),
        )));
        assert!(app.deletion.target.is_none());
        drop(app.deletion_message(DeletionMessage::Project(project)));
        let request = app.deletion.request;
        drop(app.deletion_message(DeletionMessage::Planned(
            library,
            request,
            Ok(plan(project)),
        )));
        drop(app.deletion_message(DeletionMessage::Next));
        drop(app.deletion_message(DeletionMessage::Confirm));
        assert!(app.deletion.running);
        drop(app.deletion_message(DeletionMessage::Cancel));
        assert_eq!(app.manager.panel, Some(Panel::Delete));
        drop(app.deletion_message(DeletionMessage::Finished(
            library,
            request,
            Err("范围已改变".into()),
        )));
        assert!(!app.manager.busy);
        assert!(!app.deletion.final_step);
        assert_eq!(app.deletion.error.as_deref(), Some("范围已改变"));
        assert!(!app.deletion_ready());
    }
}
