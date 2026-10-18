use super::*;
use crate::{
    components::*,
    manage::{ProjectDestination, project_label},
    theme::*,
};
use iced::widget::{button, column, row, text_input};
impl App {
    pub(super) fn project_form(&self) -> Element<'_, Message> {
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
            name: "选择要管理的项目".into(),
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
            muted("管理项目"),
            choice(project_choices, selected, |v| v
                .id
                .map(|id| Message::Manage(ProjectMenu(id)))
                .unwrap_or(Message::Noop))
        ]
        .spacing(12);
        if let Some(project) = current {
            content = content
                .push(muted(format!("{} 张素材", project.count)))
                .push(action("打开项目", Message::Project(project.id)))
                .push(
                    text_input("项目名称", &self.manager.label)
                        .padding(10)
                        .style(input)
                        .on_input(|s| Message::Manage(Label(s))),
                )
                .push(
                    button("保存名称")
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
            name: "顶层".into(),
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
            .push(muted("上级项目"))
            .push(
                row![
                    choice(parents, selected, |p| Message::Manage(Parent(p.id))),
                    button("保存层级")
                        .padding([8, 12])
                        .style(secondary)
                        .on_press_maybe(
                            (changed && !self.manager.busy).then_some(Message::Manage(MoveProject))
                        )
                ]
                .spacing(10),
            )
            .push(muted("层级调整不会搬动或改写原片。"))
            .push(separator())
            .push(muted("在所选上级下新建项目"))
            .push(
                text_input("新项目名称", &self.manager.new_project_name)
                    .padding(10)
                    .style(input)
                    .on_input(|s| Message::Manage(NewProjectName(s))),
            )
            .push(
                button("新建项目")
                    .padding([8, 12])
                    .style(secondary)
                    .on_press_maybe(
                        (!self.manager.busy && !self.manager.new_project_name.trim().is_empty())
                            .then_some(Message::Manage(NewChildProject)),
                    ),
            );
        if current.is_some() {
            content = content.push(separator()).push(
                button("永久删除项目及内容…")
                    .padding([8, 12])
                    .style(destructive)
                    .on_press_maybe((!self.manager.busy).then_some(Message::Manage(DeleteProject))),
            );
        }
        content.into()
    }
}
