use super::*;
use crate::components::choice as pick_list;
use crate::components::grouped_number as number;
use crate::components::*;
use crate::icons::{Icon, icon};
use crate::theme::*;
use crate::welcome::compact;
use iced::widget::{
    Column, Row, Space, button, column, container, mouse_area, progress_bar, row, stack, text,
    text_input,
};
use iced::{Alignment, Border, Color, Length};
impl App {
    pub(super) fn content_screen(&self) -> Option<Screen> {
        if self.library_picker {
            None
        } else if self.screen == Screen::Models || self.library.is_some() {
            Some(self.screen)
        } else {
            None
        }
    }
    pub(super) fn shell(&self) -> Element<'_, Message> {
        let content = match self.content_screen() {
            None => self.welcome(),
            Some(Screen::Overview) => self.overview(),
            Some(Screen::Assets) => self.assets(),
            Some(Screen::Models) => self.models(),
        };
        let alpha = self
            .transition
            .as_ref()
            .map_or(0.0, transition::PageTransition::alpha);
        let offset = self
            .transition
            .as_ref()
            .map_or(0.0, transition::PageTransition::offset);
        // Always retain the wrapper and stack, so animation frames do not remount scroll/input state.
        let content = container(crate::motion::translated(content, offset)).clip(true);
        let content: Element<'_, Message> = stack![
            content,
            container(Space::new())
                .width(Length::Fill)
                .height(Length::Fill)
                .style(move |_| container::Style {
                    background: Some(Color::from_rgba(1.0, 1.0, 1.0, alpha * 0.14).into()),
                    ..Default::default()
                })
        ]
        .into();
        let mut body = column![self.header(), content].height(Length::Fill);
        let status = if let Some(import) = &self.import {
            format!(
                "{} {} / {} · 解码 {} / 上限 {} · 峰值 {} · 等待内存 {}",
                if import.scanning.load(Ordering::Relaxed) {
                    "扫描并导入"
                } else {
                    "正在导入"
                },
                import.finished.load(Ordering::Relaxed),
                import.discovered.load(Ordering::Relaxed).max(import.total),
                import.active.load(Ordering::Relaxed),
                import.workers,
                import.peak.load(Ordering::Relaxed),
                import.waiting_memory.load(Ordering::Relaxed)
            )
        } else if let Some(control) = &self.manager.maintenance {
            format!(
                "{} · {} / {} 个版本",
                if control.cancel.load(Ordering::Relaxed) {
                    "正在停止检查"
                } else {
                    "正在检查资源库"
                },
                control.finished.load(Ordering::Relaxed),
                control.total.load(Ordering::Relaxed)
            )
        } else if let Some(control) = &self.storage.control {
            format!(
                "空间统计 · 已检查 {} 个文件",
                control.files.load(Ordering::Relaxed)
            )
        } else if self.loading {
            "正在读取素材…".into()
        } else {
            self.status.clone()
        };
        let mut footer = row![muted(status), Space::new().width(Length::Fill)];
        if !self.manager.organizer.errors.is_empty() {
            footer = footer.push(
                action(
                    "部分信息读取失败 · 查看详情",
                    Message::Manage(ManageMessage::Menu),
                )
                .padding([4, 10]),
            );
        }
        if self.import.is_some() {
            footer = footer.push(action("停止导入", Message::CancelImport).padding([4, 10]));
        }
        if self.manager.maintenance.is_some() {
            footer = footer.push(
                action("查看检查进度", Message::Manage(ManageMessage::Maintenance))
                    .padding([4, 10]),
            );
        }
        if self.storage.control.is_some() {
            footer = footer.push(
                action("查看空间统计", Message::Storage(StorageMessage::Open)).padding([4, 10]),
            );
        }
        if self.screen == Screen::Assets {
            footer = footer
                .push(icon(Icon::Maximize, 13.0, MUTED))
                .push(muted(format!("共 {} 项", self.total)));
        }
        {
            body = body.push(
                container(footer.spacing(12).align_y(Alignment::Center))
                    .padding([5, 20])
                    .height(40)
                    .width(Length::Fill)
                    .style(sidebar),
            );
        }
        let base = container(row![self.sidebar(), body].height(Length::Fill))
            .style(|_| container::Style {
                background: Some(Color::WHITE.into()),
                text_color: Some(TEXT),
                border: Border {
                    radius: 0.0.into(),
                    color: Color::WHITE,
                    width: 0.0,
                },
                ..Default::default()
            })
            .width(Length::Fill)
            .height(Length::Fill);
        let result: Element<'_, Message> = if let Some(dialog) = self.dialog {
            let project = dialog == Dialog::CreateProject;
            let title = match dialog {
                Dialog::CreateLibrary => "新建资源库",
                Dialog::CreateProject => "新建项目",
                Dialog::RenameLibrary => "编辑资源库信息",
                Dialog::RenameAsset => "重命名素材",
                Dialog::Settings => "设置",
            };
            let commit = match dialog {
                Dialog::CreateLibrary => Message::Create,
                Dialog::CreateProject => Message::CreateProject,
                Dialog::RenameLibrary => Message::SaveRenameLibrary,
                Dialog::RenameAsset => Message::SaveRenameAsset,
                Dialog::Settings => Message::Noop,
            };
            let description = match dialog {
                Dialog::CreateLibrary => "原图、标签和项目将一起保存在你选择的 .moriq 文件夹中。",
                Dialog::CreateProject => "项目用于归类素材，原图仍保存在资源库中。",
                Dialog::RenameLibrary => "修改资源库的显示名称，托管文件的位置保持不变。",
                Dialog::RenameAsset => "修改素材的显示名称，保留原片与所有版本。",
                Dialog::Settings => "",
            };
            let entry = text_input(
                if project { "项目名称" } else { "名称" },
                if project {
                    &self.project_name
                } else {
                    &self.name
                },
            )
            .on_input(if project {
                Message::ProjectName
            } else {
                Message::Name
            })
            .on_submit(commit.clone())
            .padding(12)
            .style(input)
            .size(14);
            let profile: Element<'_, Message> = if dialog == Dialog::RenameLibrary {
                column![
                    muted("描述"),
                    text_input(
                        "资源库描述",
                        self.manager
                            .description_draft
                            .as_deref()
                            .unwrap_or(&self.manager.organizer.description)
                    )
                    .on_input(|s| Message::Manage(ManageMessage::Description(s)))
                    .padding(10)
                    .style(input)
                ]
                .spacing(8)
                .into()
            } else {
                Space::new().height(0).into()
            };
            let modal = container(
                column![
                    row![
                        form_icon(
                            if project {
                                Icon::Collection
                            } else {
                                Icon::Folder
                            },
                            true
                        ),
                        column![heading(title, 22), muted(description)]
                            .spacing(6)
                            .width(Length::Fill),
                        icon_button(Icon::Close, Message::HideCreate)
                    ]
                    .spacing(16)
                    .align_y(Alignment::Center),
                    separator(),
                    entry,
                    profile,
                    row![
                        action("取消", Message::HideCreate),
                        Space::new().width(Length::Fill),
                        button(
                            text(if dialog == Dialog::CreateLibrary {
                                "选择位置并创建"
                            } else {
                                "保存"
                            })
                            .size(14)
                        )
                        .padding([10, 18])
                        .style(primary)
                        .on_press(commit)
                    ]
                    .align_y(Alignment::Center)
                ]
                .spacing(20),
            )
            .padding(28)
            .width((self.viewport.width - 40.0).clamp(260.0, 580.0))
            .style(crate::theme::modal);
            let modal: Element<'_, Message> = if dialog == Dialog::Settings {
                self.settings_form()
            } else {
                modal.into()
            };
            stack![
                base,
                mouse_area(
                    container(Space::new())
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .style(|_| container::Style {
                            background: Some(
                                Color::from_rgba8(25, 40, 64, self.backdrop_alpha()).into()
                            ),
                            ..Default::default()
                        })
                )
                .on_press(Message::HideCreate),
                container(
                    mouse_area(crate::motion::translated(modal, self.surface_offset()))
                        .on_press(Message::Noop)
                )
                .center_x(Length::Fill)
                .center_y(Length::Fill)
            ]
            .into()
        } else if self.manager.panel.is_some() {
            self.management_overlay(base.into())
        } else {
            base.into()
        };
        let result: Element<'_, Message> = if let Some(error) = &self.error {
            let toast = container(
                row![
                    scroll(text(error).size(12).color(Color::from_rgb8(176, 46, 46)))
                        .height(Length::Shrink)
                        .width(Length::Fill),
                    icon_button(Icon::Close, Message::Dismiss)
                ]
                .spacing(12),
            )
            .padding(14)
            .width((self.viewport.width - 40.0).clamp(260.0, 480.0))
            .max_height(150)
            .style(|_| container::Style {
                background: Some(Color::WHITE.into()),
                border: Border {
                    color: Color::from_rgb8(244, 208, 208),
                    width: 1.0,
                    radius: 8.0.into(),
                },
                shadow: iced::Shadow {
                    color: Color::from_rgba8(30, 45, 70, 0.12),
                    offset: iced::Vector::new(0.0, 4.0),
                    blur_radius: 16.0,
                },
                ..Default::default()
            });
            stack![
                result,
                container(mouse_area(toast).on_press(Message::Noop))
                    .padding(iced::Padding {
                        bottom: 42.0,
                        right: 18.0,
                        ..Default::default()
                    })
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_x(Alignment::End)
                    .align_y(Alignment::End)
            ]
            .into()
        } else {
            result
        };
        container(result)
            .padding(self.frame_padding())
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
    fn header(&self) -> Element<'_, Message> {
        // Fixed chrome slots: page-specific labels never move search or window controls.
        let search_width = (self.viewport.width - self.sidebar_width() - 668.0).clamp(160.0, 700.0);
        let search = container(
            row![
                icon(Icon::Search, 17.0, MUTED),
                button(text("语义").size(11))
                    .on_press_maybe(
                        (self.screen != Screen::Models
                            && self
                                .ai
                                .models
                                .iter()
                                .any(|m| m.spec.id == "chinese-clip" && m.installed))
                        .then_some(Message::Ai(AiMessage::Semantic))
                    )
                    .style(flat),
                text_input(
                    if self.screen == Screen::Models {
                        "搜索模型与功能说明…"
                    } else {
                        "搜索文件名、标签或备注…"
                    },
                    if self.screen == Screen::Models {
                        &self.model_search
                    } else {
                        &self.query.text
                    }
                )
                .id("global-search")
                .on_input(|value| if self.screen == Screen::Models {
                    Message::ModelSearch(value)
                } else {
                    Message::Search(value)
                })
                .padding(0)
                .size(13)
                .style(|theme, status| {
                    let mut style = input(theme, status);
                    style.border.width = 0.0;
                    style.background = Color::TRANSPARENT.into();
                    style
                }),
                container(
                    text(if cfg!(windows) { "Ctrl K" } else { "⌘ K" })
                        .size(11)
                        .color(MUTED)
                )
                .padding([2, 6])
                .style(sidebar)
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .padding([7, 12])
        .width(search_width)
        .height(36)
        .style(panel);
        let import: Element<'_, Message> = if self.library.is_some()
            && self.import.is_none()
            && !self.switching
            && !self.manager.busy
        {
            pick_list(
                vec!["选择图片文件", "导入文件夹", "引用外部原片"],
                None::<&str>,
                |choice| match choice {
                    "导入文件夹" => Message::Manage(ManageMessage::FolderImport),
                    "引用外部原片" => Message::Manage(ManageMessage::ReferenceImport),
                    _ => Message::Import,
                },
            )
            .placeholder("导入")
            .width(96)
            .text_size(13)
            .padding([8, 12])
            .style(|_, _| iced::widget::pick_list::Style {
                text_color: Color::WHITE,
                placeholder_color: Color::WHITE,
                handle_color: Color::WHITE,
                background: BLUE.into(),
                border: Border {
                    radius: 7.0.into(),
                    ..Default::default()
                },
            })
            .into()
        } else {
            button(
                container(
                    text(if self.import.is_some() {
                        "导入中…"
                    } else {
                        "导入"
                    })
                    .size(13),
                )
                .center_x(Length::Fill)
                .center_y(Length::Fill),
            )
            .width(96)
            .height(36)
            .style(primary)
            .into()
        };
        let mut tools = Row::new()
            .spacing(4)
            .align_y(Alignment::Center)
            .push(container(import).width(96).center_y(36));
        for (kind, label, message, active) in [
            (
                Icon::Grid,
                "网格视图",
                Message::List(false),
                self.screen == Screen::Assets && !self.list,
            ),
            (
                Icon::List,
                "列表视图",
                Message::List(true),
                self.screen == Screen::Assets && self.list,
            ),
            (
                Icon::Filter,
                "筛选素材",
                Message::Manage(ManageMessage::Filters),
                false,
            ),
            (
                Icon::More,
                "资源库工具",
                Message::Manage(ManageMessage::Menu),
                false,
            ),
            (
                Icon::Collection,
                "智能集合",
                Message::Manage(ManageMessage::Collections),
                false,
            ),
            (Icon::Settings, "设置", Message::ShowSettings, false),
            (
                Icon::Bell,
                "通知",
                Message::Journal(JournalMessage::Open(true)),
                self.journal.notifications.unread > 0,
            ),
        ] {
            tools = tools.push(
                iced::widget::tooltip(
                    button(icon(kind, 18.0, if active { Color::WHITE } else { ICON }))
                        .width(36)
                        .height(36)
                        .padding(9)
                        .style(move |theme, status| {
                            if active {
                                primary(theme, status)
                            } else {
                                flat(theme, status)
                            }
                        })
                        .on_press(message),
                    container(text(label).size(12))
                        .padding([6, 10])
                        .style(panel),
                    iced::widget::tooltip::Position::Bottom,
                )
                .gap(8),
            );
        }
        let mut controls = Row::new().spacing(4).align_y(Alignment::Center);
        for (kind, message) in [
            (Icon::Minimize, Message::WindowMinimize),
            (Icon::Maximize, Message::WindowMaximize),
            (Icon::Close, Message::WindowClose),
        ] {
            controls = controls.push(
                button(icon(kind, 18.0, ICON))
                    .width(32)
                    .height(36)
                    .padding([9, 7])
                    .on_press(message)
                    .style(move |theme, status| {
                        let mut style = flat(theme, status);
                        if matches!(kind, Icon::Close) && matches!(status, button::Status::Hovered)
                        {
                            style.background = Some(Color::from_rgb8(255, 224, 225).into());
                        }
                        style
                    }),
            );
        }
        let back = container(
            row![
                button(icon(Icon::Left, 18.0, ICON))
                    .style(flat)
                    .padding(9)
                    .on_press_maybe(
                        (self.library.is_some() && !self.switching && self.can_go_back())
                            .then_some(Message::Back)
                    )
                    .width(36)
                    .height(36),
                button(icon(Icon::Right, 18.0, ICON))
                    .style(flat)
                    .padding(9)
                    .on_press_maybe(
                        (self.library.is_some()
                            && !self.switching
                            && !self.surface_open()
                            && !self.preview_expanded
                            && self.history.can_forward())
                        .then_some(Message::Forward)
                    )
                    .width(36)
                    .height(36)
            ]
            .spacing(4),
        )
        .width(80);
        container(
            mouse_area(
                row![
                    back,
                    search,
                    Space::new().width(Length::Fill),
                    tools.width(376),
                    controls.width(104)
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            )
            .on_press(Message::WindowDrag),
        )
        .padding([0, 20])
        .center_y(self.titlebar_height())
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(Color::WHITE.into()),
            ..Default::default()
        })
        .into()
    }
    fn nav_row(
        &self,
        kind: Icon,
        label: &str,
        count: Option<String>,
        message: Message,
        active: bool,
    ) -> Element<'_, Message> {
        let unavailable = matches!(&message, Message::Notice(_));
        let color = if unavailable {
            MUTED
        } else if active {
            BLUE
        } else {
            ICON
        };
        let mut line = row![
            icon(kind, 18.0, color),
            text(label.to_string()).size(13).color(if unavailable {
                MUTED
            } else if active {
                BLUE
            } else {
                TEXT
            }),
            Space::new().width(Length::Fill)
        ]
        .spacing(11)
        .align_y(Alignment::Center);
        if let Some(count) = count {
            line = line.push(text(count).size(11).color(MUTED));
        }
        let reason = if unavailable {
            match &message {
                Message::Notice(reason) => Some(reason.clone()),
                _ => None,
            }
        } else {
            None
        };
        let control = button(line)
            .on_press_maybe((!unavailable).then_some(message))
            .width(Length::Fill)
            .padding([6, 12])
            .style(theme::nav(active));
        if let Some(reason) = reason {
            iced::widget::tooltip(
                control,
                container(text(reason).size(12)).padding(8).style(panel),
                iced::widget::tooltip::Position::Right,
            )
            .into()
        } else {
            control.into()
        }
    }
    fn sidebar(&self) -> Element<'_, Message> {
        let logo = container(
            mouse_area(
                row![icon(Icon::Logo, 34.0, BLUE), heading("Moriq Space", 18)]
                    .spacing(7)
                    .align_y(Alignment::Center),
            )
            .on_press(Message::WindowDrag),
        )
        .height(self.titlebar_height())
        .width(Length::Fill)
        .padding([0, 22])
        .align_y(Alignment::Center);
        let mut content = column![
            self.nav_row(
                Icon::Folder,
                "资源库",
                None,
                Message::ShowLibraries,
                self.screen != Screen::Models
            ),
            self.nav_row(
                Icon::Cube,
                "AI 模型",
                None,
                Message::Navigate(2),
                self.screen == Screen::Models
            )
        ]
        .spacing(5);
        content = content
            .push(Space::new().height(if self.screen == Screen::Assets { 6 } else { 13 }))
            .push(separator())
            .push(Space::new().height(if self.screen == Screen::Assets { 3 } else { 10 }));
        if self.screen == Screen::Models {
            content = content
                .push(heading("资源库", 13))
                .push(Space::new().height(6));
            let can_switch = !self.switching && self.import.is_none() && !self.manager.busy;
            let libraries = self.model_libraries();
            if libraries.is_empty() {
                content = content.push(muted("暂无资源库，打开后会显示在这里"));
            }
            for (name, path, active) in libraries {
                let cover = photo(self.library_covers.get(&path), Length::Fixed(34.0), 34.0);
                let location = display_path(path.parent().unwrap_or(&path));
                let entry = button(
                    row![
                        cover,
                        container(
                            column![
                                text(compact(&name, 12))
                                    .size(13)
                                    .width(Length::Fill)
                                    .height(18)
                                    .wrapping(text::Wrapping::None)
                                    .color(if active { BLUE } else { TEXT }),
                                text(compact(&location, 21))
                                    .size(10)
                                    .color(MUTED)
                                    .width(Length::Fill)
                                    .height(14)
                                    .wrapping(text::Wrapping::None),
                            ]
                            .spacing(4)
                            .width(Length::Fill)
                        )
                        .width(Length::Fill)
                        .clip(true),
                        text(if active { "当前" } else { "" }).size(10).color(BLUE)
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .width(Length::Fill)
                .padding([8, 7])
                .style(theme::nav(active))
                .on_press_maybe(
                    (can_switch && !active)
                        .then(|| Message::Ai(AiMessage::SwitchLibrary(path.clone()))),
                );
                content = content.push(
                    iced::widget::tooltip(
                        entry,
                        container(
                            column![
                                heading(name, 13),
                                text(display_path(&path))
                                    .size(12)
                                    .color(ICON)
                                    .wrapping(text::Wrapping::Glyph)
                            ]
                            .spacing(6),
                        )
                        .padding(10)
                        .width(330)
                        .style(panel),
                        iced::widget::tooltip::Position::Right,
                    )
                    .delay(Duration::from_millis(450))
                    .snap_within_viewport(true),
                );
            }
            content = content
                .push(
                    action("打开其他资源库…", Message::Open)
                        .on_press_maybe(can_switch.then_some(Message::Open)),
                )
                .push(muted(if self.switching {
                    "正在切换资源库…"
                } else {
                    "模型安装与设置全局可用"
                }))
                .push(Space::new().height(10))
                .push(separator())
                .push(Space::new().height(8));
            if self.library.is_some() {
                content = content.push(self.nav_row(
                    Icon::Clock,
                    "Synco",
                    None,
                    Message::Manage(ManageMessage::Synco),
                    false,
                ));
            }
        }
        if self.screen == Screen::Overview {
            content = content
                .push(
                    row![
                        heading("我的资源库", 13),
                        Space::new().width(Length::Fill),
                        icon_button(Icon::More, Message::Open)
                    ]
                    .align_y(Alignment::Center),
                )
                .push(
                    button(
                        row![
                            icon(Icon::Plus, 17.0, BLUE),
                            text("新建资源库").size(13).color(BLUE)
                        ]
                        .spacing(7)
                        .align_y(Alignment::Center),
                    )
                    .on_press(Message::ShowCreate)
                    .padding(9)
                    .width(Length::Fill)
                    .style(secondary),
                );
            if let Some(library) = &self.library {
                let name = &library.info.name;
                let count = library.info.count;

                let thumbnail = self.tiles.first().and_then(|tile| tile.handle.as_ref());
                content = content.push(
                    button(
                        row![
                            photo(thumbnail, Length::Fixed(48.0), 48.0),
                            column![
                                heading(format!("{name}.moriq"), 13),
                                muted(format!("{} 张", number(count)))
                            ]
                            .spacing(6)
                        ]
                        .spacing(10)
                        .align_y(Alignment::Center),
                    )
                    .padding([7, 8])
                    .width(Length::Fill)
                    .style(theme::nav(true))
                    .on_press(Message::Navigate(0)),
                );
            }
            content = content.push(
                button(row![icon(Icon::Folder, 16.0, MUTED), muted("打开已有资源库…")].spacing(10))
                    .on_press(Message::Open)
                    .padding(10)
                    .style(flat),
            );
        } else if self.library.is_some() {
            content = content.push(
                row![
                    muted(if self.screen == Screen::Models {
                        "我的项目"
                    } else {
                        "项目"
                    }),
                    Space::new().width(Length::Fill),
                    icon_button(Icon::Plus, Message::ShowProject)
                ]
                .align_y(Alignment::Center),
            );
            if let Some(library) = &self.library {
                for project in library
                    .info
                    .projects
                    .iter()
                    .take(if self.screen == Screen::Models { 5 } else { 4 })
                {
                    let thumbnail = self.manager.project_covers.get(&project.id);
                    let compact = self.screen == Screen::Models;
                    let thumb_size = if compact { 28.0 } else { 40.0 };
                    let selected = self.query.project == Some(project.id);
                    let data: Element<'_, Message> = if compact {
                        row![
                            text(manage::project_label(&library.info.projects, project.id))
                                .size(13),
                            Space::new().width(Length::Fill),
                            text(number(project.count)).size(11).color(MUTED)
                        ]
                        .align_y(Alignment::Center)
                        .into()
                    } else {
                        column![
                            text(manage::project_label(&library.info.projects, project.id))
                                .size(13)
                                .color(if selected { BLUE } else { TEXT }),
                            muted(format!("{} 张", number(project.count)))
                        ]
                        .spacing(5)
                        .into()
                    };
                    content = content.push(
                        button(
                            row![
                                photo(thumbnail, Length::Fixed(thumb_size), thumb_size),
                                container(data).width(Length::Fill)
                            ]
                            .spacing(10)
                            .align_y(Alignment::Center),
                        )
                        .padding([6, 10])
                        .width(Length::Fill)
                        .style(theme::nav(selected))
                        .on_press(Message::Project(project.id)),
                    );
                }
            }
            content = content
                .push(Space::new().height(12))
                .push(separator())
                .push(Space::new().height(10))
                .push(muted(if self.screen == Screen::Models {
                    "智能集合"
                } else {
                    "素材库"
                }));
            let total = self.library.as_ref().map(|l| l.info.count).unwrap_or(0);
            let favorites = self.library.as_ref().map(|l| l.info.favorites).unwrap_or(0);
            let mut rows = if self.screen == Screen::Models {
                vec![
                    (
                        Icon::Clock,
                        "最近导入",
                        self.manager.organizer.recent_count,
                        Message::Manage(ManageMessage::Builtin(0)),
                    ),
                    (
                        Icon::Tag,
                        "未加标签",
                        self.manager.organizer.untagged_count,
                        Message::Manage(ManageMessage::Builtin(1)),
                    ),
                    (
                        Icon::Star,
                        "高评分",
                        self.manager.organizer.high_rating_count,
                        Message::Manage(ManageMessage::Builtin(2)),
                    ),
                    (
                        Icon::Raw,
                        "RAW 文件",
                        self.manager.organizer.raw_count,
                        Message::Manage(ManageMessage::Builtin(6)),
                    ),
                    (
                        Icon::Video,
                        "视频",
                        self.manager.organizer.video_count,
                        Message::Notice("当前尚不支持视频预览".into()),
                    ),
                    (
                        Icon::Image,
                        "截图",
                        self.category_count("截图"),
                        Message::Ai(AiMessage::Category("截图".into())),
                    ),
                    (
                        Icon::Duplicate,
                        "重复文件",
                        0,
                        Message::Notice("导入时会自动跳过完全相同的图片".into()),
                    ),
                ]
            } else {
                vec![
                    (Icon::Folder, "全部素材", total, Message::AllAssets),
                    (
                        Icon::Clock,
                        "最近添加",
                        self.manager.organizer.recent_count,
                        Message::Manage(ManageMessage::Builtin(0)),
                    ),
                    (
                        Icon::Edit,
                        "最近编辑",
                        self.manager.organizer.edited_count,
                        Message::Manage(ManageMessage::Builtin(5)),
                    ),
                    (Icon::Heart, "收藏夹", favorites, Message::Favorites),
                    (
                        Icon::Raw,
                        "RAW 文件",
                        self.manager.organizer.raw_count,
                        Message::Manage(ManageMessage::Builtin(6)),
                    ),
                    (
                        Icon::Tag,
                        "标签",
                        self.manager.organizer.tags.len() as u64,
                        Message::Manage(ManageMessage::Tags),
                    ),
                    (
                        Icon::Collection,
                        "智能集合",
                        self.manager.organizer.collections.len() as u64,
                        Message::Manage(ManageMessage::Collections),
                    ),
                    (
                        Icon::Person,
                        "人脸识别",
                        self.manager
                            .organizer
                            .people
                            .iter()
                            .filter(|p| p.count > 0)
                            .count() as u64,
                        Message::Ai(AiMessage::People),
                    ),
                    (
                        Icon::Video,
                        "视频",
                        self.manager.organizer.video_count,
                        Message::Manage(ManageMessage::Builtin(7)),
                    ),
                    (
                        Icon::Unsorted,
                        "未归类项目",
                        self.manager.organizer.unassigned_count,
                        Message::Manage(ManageMessage::Builtin(4)),
                    ),
                ]
            };
            let categories = self.sidebar_categories();
            if self.screen != Screen::Models {
                for (index, (category, count)) in categories.iter().enumerate() {
                    let kind = match category.as_str() {
                        "人像" => Icon::Person,
                        "风景" => Icon::Landscape,
                        "城市" | "建筑" => Icon::Building,
                        "植物" => Icon::Plant,
                        "插画" => Icon::Sparkles,
                        "截图" | "动物" => Icon::Image,
                        "美食" => Icon::Food,
                        "夜景" => Icon::Moon,
                        _ => Icon::Tag,
                    };
                    rows.insert(
                        8 + index,
                        (
                            kind,
                            category.as_str(),
                            *count,
                            Message::Ai(AiMessage::Category(category.clone())),
                        ),
                    );
                }
            }
            for (kind, label, count, message) in rows {
                let displayed = if label == "全部素材"
                    || label == "收藏夹"
                    || [
                        "最近添加",
                        "最近导入",
                        "高评分",
                        "标签",
                        "智能集合",
                        "未分类",
                        "未归类项目",
                        "RAW 文件",
                        "未加标签",
                        "视频",
                        "人脸识别",
                    ]
                    .contains(&label)
                    || matches!(&message, Message::Ai(AiMessage::Category(_)))
                {
                    count
                } else {
                    0
                };
                let active = (label == "全部素材" && self.query == Query::default())
                    || (label == "收藏夹" && self.query.favorites_only)
                    || (label == "RAW 文件" && self.query.media_kind == MediaKind::Raw)
                    || (label == "视频" && self.query.media_kind == MediaKind::Video)
                    || (label == "人脸识别" && self.manager.panel == Some(manage::Panel::People))
                    || matches!(&message, Message::Ai(AiMessage::Category(category)) if self.query.category == *category);
                content = content.push(self.nav_row(
                    kind,
                    label,
                    Some(number(displayed)),
                    message,
                    active && self.screen == Screen::Assets,
                ));
            }
            if self.screen == Screen::Assets {
                content = content.push(separator()).push(self.nav_row(
                    Icon::Trash,
                    "回收站",
                    Some(number(self.manager.organizer.trash_count)),
                    Message::Manage(ManageMessage::Builtin(3)),
                    false,
                ));
            }
        }
        let mut bottom = Column::new().spacing(8);
        if self.screen == Screen::Models && self.library.is_some() {
            bottom = bottom
                .push(
                    progress_bar(
                        0.0..=1.0,
                        self.storage.volume.filter(|(total, _)| *total > 0).map_or(
                            0.0,
                            |(total, available)| {
                                total.saturating_sub(available) as f32 / total as f32
                            },
                        ),
                    )
                    .girth(4)
                    .style(theme::progress),
                )
                .push(muted(
                    self.storage
                        .volume
                        .map(|(total, available)| {
                            format!("所在卷 · 可用 {} / {}", size(available), size(total))
                        })
                        .unwrap_or_else(|| "正在查询卷容量…".into()),
                ))
                .push(action("空间明细", Message::Storage(StorageMessage::Open)));
        }

        container(
            column![
                logo,
                container(scroll(content.spacing(4)))
                    .padding([16, 16])
                    .height(Length::Fill),
                container(bottom).padding([10, 28])
            ]
            .spacing(0),
        )
        .width(self.sidebar_width())
        .height(Length::Fill)
        .style(sidebar)
        .into()
    }
}
