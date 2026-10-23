use super::*;
use crate::{
    components::*,
    manage::{ProjectDestination, project_label},
    theme::*,
};
use iced::widget::{button, column, row, text_input};
impl App {
    pub(super) fn project_form(&self) -> Element<'_, Message> {
        let language = self.preferences.language;
        use ManageMessage::*;
        let projects = self
            .library
            .as_ref()
            .map(|l| l.info.projects.as_slice())
            .unwrap_or(&[]);
        let current = projects
            .iter()
            .find(|p| Some(p.id) == self.manager.project_target);
        let mut project_choices = vec![ProjectDestination {
            id: None,
            name: tr!(language, "选择要管理的项目" => "Select a project to manage").into(),
        }];
        project_choices.extend(projects.iter().map(|p| ProjectDestination {
            id: Some(p.id),
            name: project_label(projects, p.id),
        }));
        let selected = project_choices
            .iter()
            .find(|c| c.id == self.manager.project_target)
            .cloned();
        let mut content = column![
            muted(tr!(language, "管理项目" => "Manage project")),
            choice(project_choices, selected, |v| v
                .id
                .map(|id| Message::Manage(ProjectMenu(id)))
                .unwrap_or(Message::Noop))
        ]
        .spacing(12);
        if let Some(project) = current {
            content = content
                .push(muted(
                    trf!(language, "{} 张素材" => "{} assets", project.count),
                ))
                .push(action(
                    tr!(language, "打开项目" => "Open project"),
                    Message::Project(project.id),
                ))
                .push(
                    text_input(
                        tr!(language, "项目名称" => "Project name"),
                        &self.manager.label,
                    )
                    .padding(10)
                    .style(input)
                    .on_input(|s| Message::Manage(Label(s))),
                )
                .push(
                    button(tr!(language, "保存名称" => "Save name"))
                        .padding([8, 12])
                        .style(secondary)
                        .on_press_maybe(
                            (!self.manager.busy
                                && !self.manager.label.trim().is_empty()
                                && self.manager.label.trim() != project.name)
                                .then_some(Message::Manage(RenameProject)),
                        ),
                );
        }
        let mut parents = vec![ProjectDestination {
            id: None,
            name: tr!(language, "顶层" => "Top level").into(),
        }];
        parents.extend(
            projects
                .iter()
                .filter(|candidate| {
                    let mut cursor = Some(candidate.id);
                    for _ in 0..=projects.len() {
                        let Some(id) = cursor else {
                            return true;
                        };
                        if Some(id) == self.manager.project_target {
                            return false;
                        }
                        cursor = projects.iter().find(|p| p.id == id).and_then(|p| p.parent);
                    }
                    false
                })
                .map(|p| ProjectDestination {
                    id: Some(p.id),
                    name: project_label(projects, p.id),
                }),
        );
        let selected = parents
            .iter()
            .find(|p| p.id == self.manager.parent)
            .cloned();
        let changed = current.is_some_and(|p| p.parent != self.manager.parent);
        content = content
            .push(separator())
            .push(muted(tr!(language, "上级项目" => "Parent project")))
            .push(
                row![
                    choice(parents, selected, |p| Message::Manage(Parent(p.id))),
                    button(tr!(language, "保存层级" => "Save hierarchy"))
                        .padding([8, 12])
                        .style(secondary)
                        .on_press_maybe(
                            (changed && !self.manager.busy).then_some(Message::Manage(MoveProject))
                        )
                ]
                .spacing(10),
            )
            .push(muted(tr!(language, "层级调整不会搬动或改写原片。" => "Changing the hierarchy does not move or modify originals.")))
            .push(separator())
            .push(muted(tr!(language, "在所选上级下新建项目" => "Create project under selected parent")))
            .push(
                text_input(tr!(language, "新项目名称" => "New project name"), &self.manager.new_project_name)
                    .padding(10)
                    .style(input)
                    .on_input(|s| Message::Manage(NewProjectName(s))),
            )
            .push(
                button(tr!(language, "新建项目" => "New project"))
                    .padding([8, 12])
                    .style(secondary)
                    .on_press_maybe(
                        (!self.manager.busy && !self.manager.new_project_name.trim().is_empty())
                            .then_some(Message::Manage(NewChildProject)),
                    ),
            );
        if current.is_some() {
            content = content.push(separator()).push(
                button(tr!(language, "永久删除项目及内容…" => "Delete project and contents permanently…"))
                    .padding([8, 12])
                    .style(destructive)
                    .on_press_maybe((!self.manager.busy).then_some(Message::Manage(DeleteProject))),
            );
        }
        content.into()
    }
}
