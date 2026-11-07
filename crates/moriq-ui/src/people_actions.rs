use super::*;
use crate::{
    components::*,
    icons::{Icon, icon},
    theme::*,
};
use iced::widget::{Space, button, column, container, row, text, text_input, tooltip};
use iced::{Alignment, Length};
const RED: iced::Color = iced::Color::from_rgb8(199, 67, 67);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    Rename,
    Delete,
}

pub(super) struct Editor {
    owner: LibraryId,
    project: Option<ProjectId>,
    request: u64,
    person: String,
    original_name: String,
    name: String,
    count: u64,
    action: Action,
    busy: bool,
    error: Option<String>,
}

impl App {
    /// 人物操作沿用当前页面及管理弹窗的返回路径，异步结果绑定库和项目。
    pub(super) fn person_edit_message(&mut self, event: AiMessage) -> Task<Message> {
        match event {
            AiMessage::EditPerson(person, action) if !self.switching => {
                let Some(owner) = self.library.as_ref().map(|l| l.info.id) else {
                    return Task::none();
                };
                if self
                    .ai
                    .person_edit
                    .as_ref()
                    .is_some_and(|e| e.busy && e.owner == owner && e.project == self.query.project)
                {
                    return Task::none();
                }
                let Some(person) = self
                    .manager
                    .organizer
                    .people
                    .iter()
                    .find(|p| p.id == person)
                else {
                    return Task::none();
                };
                self.ai.person_edit_request = self.ai.person_edit_request.wrapping_add(1);
                self.ai.person_edit = Some(Editor {
                    owner,
                    project: self.query.project,
                    request: self.ai.person_edit_request,
                    person: person.id.clone(),
                    original_name: person.name.clone(),
                    name: if person.name == "未命名人物" {
                        String::new()
                    } else {
                        person.name.clone()
                    },
                    count: person.count,
                    action,
                    busy: false,
                    error: None,
                });
                self.dialog = Some(Dialog::EditPerson);
            }
            AiMessage::EditPersonName(name) => {
                if let Some(editor) = self.ai.person_edit.as_mut().filter(|e| !e.busy) {
                    editor.name = name;
                    editor.error = None;
                }
            }
            AiMessage::SavePersonEdit
                if self.dialog == Some(Dialog::EditPerson) && !self.switching =>
            {
                let Some(editor) = self.ai.person_edit.as_mut() else {
                    return Task::none();
                };
                if editor.busy
                    || self.library.as_ref().map(|l| l.info.id) != Some(editor.owner)
                    || self.query.project != editor.project
                {
                    return Task::none();
                }
                let name = editor.name.trim().to_owned();
                if editor.action == Action::Rename && (name.is_empty() || name.len() > 128) {
                    return Task::none();
                }
                editor.busy = true;
                editor.error = None;
                let (owner, project, request, person, action) = (
                    editor.owner,
                    editor.project,
                    editor.request,
                    editor.person.clone(),
                    editor.action,
                );
                let service = self.service.clone();
                return Task::perform(
                    async move {
                        match action {
                            Action::Rename => service.rename_person(owner, person, name).await,
                            Action::Delete => service.delete_person(owner, person).await,
                        }
                    },
                    move |result| {
                        Message::Ai(AiMessage::PersonEdited(owner, project, request, result))
                    },
                );
            }
            AiMessage::PersonEdited(owner, project, request, result)
                if self.library.as_ref().map(|l| l.info.id) == Some(owner)
                    && self.query.project == project
                    && self.ai.person_edit.as_ref().is_some_and(|e| {
                        e.request == request && e.owner == owner && e.project == project
                    }) =>
            {
                let Some(editor) = self.ai.person_edit.as_mut() else {
                    return Task::none();
                };
                editor.busy = false;
                if let Err(error) = result {
                    if self.dialog == Some(Dialog::EditPerson) {
                        editor.error = Some(error);
                    } else {
                        self.error = Some(error);
                    }
                    return Task::none();
                }
                let person = editor.person.clone();
                let action = editor.action;
                let name = editor.name.trim().to_owned();
                match action {
                    Action::Rename => {
                        if let Some(p) = self
                            .manager
                            .organizer
                            .people
                            .iter_mut()
                            .find(|p| p.id == person)
                        {
                            p.name = name;
                        }
                    }
                    Action::Delete => {
                        self.manager.organizer.people.retain(|p| p.id != person);
                        self.ai.portraits.remove(&person);
                        self.stop_people_portraits();
                    }
                }
                self.refresh_people_choices();
                self.ai.person_edit = None;
                if self.dialog == Some(Dialog::EditPerson) {
                    self.dialog = None;
                }
                let gallery = if action == Action::Delete
                    && self.query.person.as_deref() == Some(person.as_str())
                {
                    self.query.person = None;
                    self.reset_query()
                } else {
                    Task::none()
                };
                return Task::batch([gallery, self.refresh_organizer(), self.refresh_details()]);
            }
            _ => {}
        }
        Task::none()
    }

    /// 内外人物卡片共用操作按钮；浏览、改名和删除的点击区域互不嵌套。
    pub(super) fn person_card<'a>(
        &'a self,
        person: &'a Person,
        large: bool,
    ) -> Element<'a, Message> {
        let language = self.preferences.language;
        let name = self.person_display_name(person);
        let label = if name.chars().count() > 26 {
            format!("{}…", name.chars().take(25).collect::<String>())
        } else {
            name.clone()
        };
        let edge = if large { 64.0 } else { 52.0 };
        let open = button(row![
            photo(self.ai.portraits.get(&person.id), Length::Fixed(edge), edge),
            column![text(label).size(if large { 15 } else { 13 }),
                muted(trf!(language, "{} 张素材" => "{} photos", crate::components::grouped_number(person.count)))
            ].spacing(7).width(Length::Fill),
        ].spacing(12).align_y(Alignment::Center)).padding(12).style(flat).width(Length::Fill)
            .on_press(Message::Ai(AiMessage::PersonFilter(person.id.clone())));
        let controls =
            column![
                tooltip(
                    icon_button(
                        Icon::Edit,
                        Message::Ai(AiMessage::EditPerson(person.id.clone(), Action::Rename))
                    )
                    .padding(6)
                    .on_press_maybe((!self.switching).then_some(Message::Ai(
                        AiMessage::EditPerson(person.id.clone(), Action::Rename)
                    ))),
                    text(tr!(language, "改名" => "Rename")).size(12),
                    tooltip::Position::Left
                ),
                tooltip(
                    icon_button(
                        Icon::Trash,
                        Message::Ai(AiMessage::EditPerson(person.id.clone(), Action::Delete))
                    )
                    .padding(6)
                    .on_press_maybe((!self.switching).then_some(Message::Ai(
                        AiMessage::EditPerson(person.id.clone(), Action::Delete)
                    ))),
                    text(tr!(language, "删除人物分组" => "Delete person group")).size(12),
                    tooltip::Position::Left
                ),
            ]
            .spacing(2);
        container(
            row![
                tooltip(open, text(name).size(12), tooltip::Position::Top),
                container(controls).padding([0, 8])
            ]
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .style(pale)
        .into()
    }

    fn person_display_name(&self, person: &Person) -> String {
        let language = self.preferences.language;
        if person.name.is_empty() || person.name == "未命名人物" {
            trf!(language, "未命名人物 · {}" => "Unnamed person · {}", person.id.chars().take(8).collect::<String>())
        } else {
            person.name.clone()
        }
    }

    /// 轻量操作对话框提供明确的影响范围、就地错误和防重复提交状态。
    pub(super) fn person_editor(&self) -> Element<'_, Message> {
        let language = self.preferences.language;
        let Some(editor) = self.ai.person_edit.as_ref() else {
            return Space::new().into();
        };
        let deleting = editor.action == Action::Delete;
        let valid = deleting || (!editor.name.trim().is_empty() && editor.name.trim().len() <= 128);
        let title = if deleting {
            tr!(language, "删除人物分组？" => "Delete person group?")
        } else {
            tr!(language, "修改人物名称" => "Rename person")
        };
        let mut body = column![
            row![
                icon(
                    if deleting { Icon::Trash } else { Icon::Edit },
                    22.0,
                    if deleting { RED } else { BLUE }
                ),
                heading(title, 22),
                Space::new().width(Length::Fill),
                icon_button(Icon::Close, Message::HideCreate),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
            separator(),
            row![
                photo(
                    self.ai.portraits.get(&editor.person),
                    Length::Fixed(56.0),
                    56.0
                ),
                column![
                    heading(editor.original_name.clone(), 15),
                    muted(trf!(language,"{} 张素材"=>"{} photos",editor.count))
                ]
                .spacing(6)
            ]
            .spacing(12)
            .align_y(Alignment::Center),
        ]
        .spacing(18);
        if deleting {
            body = body.push(muted(tr!(language,
                "只移除这个人物分组，原照片、文件和标签都会保留。已有照片的人脸将取消归组，重新分析也会保留这次人工操作。" =>
                "Remove this person group while keeping all photos, files and tags. Existing faces become unassigned, and reanalysis preserves this manual change.")));
        } else {
            let mut entry = text_input(tr!(language,"输入人物名称"=>"Enter a name"), &editor.name)
                .padding(12)
                .style(input);
            if !editor.busy {
                entry = entry.on_input(|v| Message::Ai(AiMessage::EditPersonName(v)));
            }
            if valid && !editor.busy {
                entry = entry.on_submit(Message::Ai(AiMessage::SavePersonEdit));
            }
            body = body.push(entry).push(muted(tr!(language,"名称会同步更新到人物列表与照片筛选中。"=>"The name updates in person lists and photo filters.")));
        }
        if let Some(error) = &editor.error {
            body = body.push(text(error).size(13).style(text_color(RED)));
        }
        let submit = action(
            if editor.busy {
                tr!(language,"正在保存…"=>"Saving…")
            } else if deleting {
                tr!(language,"删除分组"=>"Delete group")
            } else {
                tr!(language,"保存名称"=>"Save name")
            },
            Message::Ai(AiMessage::SavePersonEdit),
        )
        .style(if deleting { destructive } else { primary })
        .on_press_maybe((valid && !editor.busy).then_some(Message::Ai(AiMessage::SavePersonEdit)));
        body = body.push(
            row![
                Space::new().width(Length::Fill),
                action(tr!(language,"取消"=>"Cancel"), Message::HideCreate),
                submit
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        );
        container(body)
            .padding(26)
            .width((self.viewport.width - 40.0).clamp(260.0, 480.0))
            .style(modal)
            .into()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    /// 内外改名和删除保留入口、取消无写入、重复提交及过期结果被拒绝。
    fn person_actions_preserve_context_and_validate_receipts() {
        for panel in [None, Some(manage::Panel::People)] {
            for action in [Action::Rename, Action::Delete] {
                let mut app = crate::tests::app();
                app.screen = Screen::Models;
                app.manager.panel = panel;
                app.surfaces.y = 240.0;
                app.manager.organizer.people = vec![Person {
                    id: "p".into(),
                    name: "Old".into(),
                    count: 4,
                }];
                let owner = app.library.as_ref().unwrap().info.id;
                drop(app.update(Message::Ai(AiMessage::EditPerson("p".into(), action))));
                assert_eq!(app.dialog, Some(Dialog::EditPerson));
                assert_eq!(app.manager.panel, panel);
                drop(app.person_editor());
                drop(app.update(Message::HideCreate));
                assert_eq!(
                    app.person_edit_message(AiMessage::SavePersonEdit).units(),
                    0
                );
                assert_eq!(app.manager.organizer.people[0].name, "Old");
                drop(app.update(Message::Ai(AiMessage::EditPerson("p".into(), action))));
                drop(app.update(Message::Ai(AiMessage::EditPersonName("  New  ".into()))));
                assert_eq!(
                    app.person_edit_message(AiMessage::SavePersonEdit).units(),
                    1
                );
                assert_eq!(
                    app.person_edit_message(AiMessage::SavePersonEdit).units(),
                    0
                );
                let request = app.ai.person_edit_request;
                drop(app.update(Message::Ai(AiMessage::PersonEdited(
                    owner,
                    None,
                    request - 1,
                    Ok(()),
                ))));
                assert!(app.ai.person_edit.as_ref().unwrap().busy);
                drop(app.update(Message::Ai(AiMessage::PersonEdited(
                    owner,
                    None,
                    request,
                    Ok(()),
                ))));
                assert!(app.dialog.is_none());
                assert_eq!(app.manager.panel, panel);
                match action {
                    Action::Rename => assert_eq!(app.manager.organizer.people[0].name, "New"),
                    Action::Delete => assert!(app.manager.organizer.people.is_empty()),
                }
            }
        }
    }
}
