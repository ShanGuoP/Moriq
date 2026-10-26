use super::*;
use crate::{
    components::*,
    manage::{ProjectDestination, project_label},
    theme::*,
};
use iced::widget::{column, row, text_input};
#[derive(Clone, PartialEq, Eq)]
struct TemplateChoice {
    template: ProjectTemplate,
    label: String,
}
impl std::fmt::Display for TemplateChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}
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
        let mut selector = row![
            choice(project_choices, selected, |v| v
                .id
                .map(|id| Message::Manage(ProjectMenu(id)))
                .unwrap_or(Message::Noop))
            .width(iced::Length::Fill)
        ]
        .spacing(10)
        .align_y(iced::Alignment::Center);
        if let Some(project) = current {
            selector = selector.push(
                action(
                    tr!(language,"打开项目"=>"Open"),
                    Message::Project(project.id),
                )
                .on_press_maybe((!self.manager.busy).then_some(Message::Project(project.id))),
            );
        }
        let mut content = column![
            column![
                heading(tr!(language,"选择项目"=>"Select project"), 13),
                selector
            ]
            .spacing(8)
        ]
        .spacing(16);
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
        if let Some(project) = current {
            let name_changed =
                !self.manager.label.trim().is_empty() && self.manager.label.trim() != project.name;
            content=content.push(form_card(column![
                row![heading(tr!(language,"项目设置"=>"Project settings"),16),iced::widget::Space::new().width(iced::Length::Fill),
                    chip(trf!(language,"{} 项素材"=>"{} assets",project.count),BLUE)].align_y(iced::Alignment::Center),
                column![heading(tr!(language,"项目名称"=>"Name"),13),row![
                    text_input(tr!(language,"项目名称"=>"Project name"),&self.manager.label).padding(10).size(14).style(input)
                        .on_input_maybe((!self.manager.busy).then_some(|s|Message::Manage(Label(s)))).width(iced::Length::Fill),
                    action(tr!(language,"保存名称"=>"Save name"),Message::Manage(RenameProject))
                        .on_press_maybe((name_changed && !self.manager.busy).then_some(Message::Manage(RenameProject)))
                ].spacing(10).align_y(iced::Alignment::Center)].spacing(8),
                column![heading(tr!(language,"上级项目"=>"Parent project"),13),row![
                    choice(parents,selected,|p|Message::Manage(Parent(p.id))).width(iced::Length::Fill),
                    action(tr!(language,"保存层级"=>"Save parent"),Message::Manage(MoveProject))
                        .on_press_maybe((changed && !self.manager.busy).then_some(Message::Manage(MoveProject)))
                ].spacing(10).align_y(iced::Alignment::Center)].spacing(8),
                muted(tr!(language,"修改名称与层级仅调整整理结构，不会搬动或改写原片。"=>"Name and hierarchy changes organize the library without moving or modifying originals."))
            ].spacing(16)));
        }
        let mut destinations = vec![ProjectDestination {
            id: None,
            name: tr!(language,"顶层项目"=>"Top level").into(),
        }];
        destinations.extend(projects.iter().map(|p| ProjectDestination {
            id: Some(p.id),
            name: project_label(projects, p.id),
        }));
        let new_parent = destinations
            .iter()
            .find(|p| p.id == self.manager.new_project_parent)
            .cloned();
        let mut templates = vec![
            TemplateChoice {
                template: ProjectTemplate::Default,
                label: tr!(language,"默认分类模板"=>"Default category template").into(),
            },
            TemplateChoice {
                template: ProjectTemplate::Portrait,
                label: tr!(language,"人像摄影"=>"Portrait photography").into(),
            },
            TemplateChoice {
                template: ProjectTemplate::Product,
                label: tr!(language,"产品拍摄"=>"Product photography").into(),
            },
            TemplateChoice {
                template: ProjectTemplate::Travel,
                label: tr!(language,"旅行记录"=>"Travel").into(),
            },
        ];
        templates.extend(projects.iter().map(|p| TemplateChoice {
            template: ProjectTemplate::Existing(p.id),
            label: trf!(language,"复制配置：{}"=>"Copy settings: {}",p.name),
        }));
        let selected_template = templates
            .iter()
            .find(|t| t.template == self.manager.new_project_template)
            .cloned();
        content = content.push(form_card(
            column![
                heading(tr!(language,"新建项目"=>"New project"), 16),
                column![
                    heading(tr!(language,"整理模板"=>"Organization template"),13),
                    choice(templates,selected_template,|t|Message::Manage(NewProjectTemplate(t.template))).width(iced::Length::Fill),
                    muted(tr!(language,"只复制分类与识别定义，创建后独立修改，不随原项目变化。"=>"Copies categories and recognition definitions. Later changes stay independent."))
                ].spacing(8),
                column![
                    heading(tr!(language,"创建位置"=>"Create under"), 13),
                    choice(destinations, new_parent, |p| Message::Manage(NewParent(
                        p.id
                    )))
                    .width(iced::Length::Fill)
                ]
                .spacing(8),
                row![
                    text_input(
                        tr!(language,"输入新项目名称"=>"New project name"),
                        &self.manager.new_project_name
                    )
                    .padding(10)
                    .size(14)
                    .style(input)
                    .on_input_maybe(
                        (!self.manager.busy).then_some(|s| Message::Manage(NewProjectName(s)))
                    )
                    .width(iced::Length::Fill),
                    form_action(
                        crate::icons::Icon::Plus,
                        tr!(language,"创建项目"=>"Create"),
                        Message::Manage(NewChildProject),
                        true
                    )
                    .on_press_maybe(
                        (!self.manager.busy && !self.manager.new_project_name.trim().is_empty())
                            .then_some(Message::Manage(NewChildProject))
                    )
                ]
                .spacing(10)
                .align_y(iced::Alignment::Center)
            ]
            .spacing(14),
        ));
        if current.is_some() {
            content=content.push(form_card(column![
                heading(tr!(language,"危险操作"=>"Danger zone"),14),
                muted(tr!(language,"永久删除会影响项目及其内容。继续后将展示具体范围，并要求再次确认。"=>"Permanent deletion affects this project and its contents. Review the exact scope and confirm again before deletion.")),
                form_action(crate::icons::Icon::Trash,tr!(language,"删除项目及内容…"=>"Delete project and contents…"),Message::Manage(DeleteProject),true)
                    .style(destructive).on_press_maybe((!self.manager.busy).then_some(Message::Manage(DeleteProject)))
            ].spacing(10)));
        }
        content.into()
    }
}
