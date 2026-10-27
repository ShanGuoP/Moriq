use super::*;
use crate::components::grouped_number as number;
use crate::components::localized_choice as pick_list;
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
    /// 取得当前实际展示的主页面身份。
    pub(super) fn content_screen(&self) -> Option<Screen> {
        if self.library_picker {
            None
        } else if self.screen == Screen::Models || self.library.is_some() {
            Some(self.screen)
        } else {
            None
        }
    }
    /// 构建固定页面外壳，保持导航、标题栏与内容区域布局一致。
    pub(super) fn shell(&self) -> Element<'_, Message> {
        let language = self.preferences.language;
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
                .style(move |theme: &Theme| container::Style {
                    background: Some(
                        Color {
                            a: alpha * 0.14,
                            ..theme.palette().background
                        }
                        .into()
                    ),
                    ..Default::default()
                })
        ]
        .into();
        let mut body = column![self.header(), content].height(Length::Fill);
        let status = if let Some(waiting) = self.favorite_wait_message() {
            waiting.to_owned()
        } else if let Some(import) = &self.import {
            trf!(language, "{} {} / {} · 解码 {} / 上限 {} · 峰值 {} · 等待内存 {}" => "{} {} / {} · Decoding {} / limit {} · Peak {} · Waiting for memory {}",
                if import.scanning.load(Ordering::Relaxed) {
                    tr!(language, "扫描并导入" => "Scanning and importing")
                } else {
                    tr!(language, "正在导入" => "Importing")
                },
                import.finished.load(Ordering::Relaxed),
                import.discovered.load(Ordering::Relaxed).max(import.total),
                import.active.load(Ordering::Relaxed),
                import.workers,
                import.peak.load(Ordering::Relaxed),
                import.waiting_memory.load(Ordering::Relaxed)
            )
        } else if let Some(control) = &self.lifecycle.copy {
            if control.total.load(Ordering::Acquire) == 0 {
                tr!(language, "正在准备备份…" => "Preparing backup…").into()
            } else {
                trf!(language, "备份 · {} / {} 个文件" => "Backup · {} / {} files",
                    control.finished.load(Ordering::Acquire),
                    control.total.load(Ordering::Acquire)
                )
            }
        } else if let Some(control) = &self.manager.maintenance {
            trf!(language, "{} · {} / {} 个版本" => "{} · {} / {} versions",
                if control.cancel.load(Ordering::Relaxed) {
                    tr!(language, "正在停止检查" => "Stopping check")
                } else {
                    tr!(language, "正在检查资源库" => "Checking library")
                },
                control.finished.load(Ordering::Relaxed),
                control.total.load(Ordering::Relaxed)
            )
        } else if let Some(control) = &self.storage.control {
            trf!(language, "空间统计 · 已检查 {} 个文件" => "Storage calculation · {} files checked",
                control.files.load(Ordering::Relaxed)
            )
        } else if self.loading {
            tr!(language, "正在读取素材…" => "Loading assets…").into()
        } else {
            crate::i18n::status(language, &self.status).to_owned()
        };
        let mut footer = row![muted(status), Space::new().width(Length::Fill)];
        if self.lifecycle.copy.is_some() {
            footer = footer.push(
                action(
                    tr!(language, "停止备份" => "Stop backup"),
                    Message::Lifecycle(LifecycleMessage::CancelCopy),
                )
                .padding([4, 10]),
            );
        }
        if !self.manager.organizer.errors.is_empty() {
            footer = footer.push(
                action(
                    tr!(language, "部分信息读取失败 · 查看详情" => "Some information could not be loaded · View details"),
                    Message::Manage(ManageMessage::Menu),
                )
                .padding([4, 10]),
            );
        }
        let queued = self.drop_import.pending_count();
        if queued > 0 {
            footer = footer.push(muted(
                trf!(language,"待导入 {queued} 项"=>"{queued} items queued"),
            ));
        }
        if self.import.is_some() || queued > 0 {
            footer = footer.push(
                action(
                    tr!(language, "停止导入" => "Stop import"),
                    Message::CancelImport,
                )
                .padding([4, 10]),
            );
        }
        if self.manager.maintenance.is_some() {
            footer = footer.push(
                action(
                    tr!(language, "查看检查进度" => "View check progress"),
                    Message::Manage(ManageMessage::Maintenance),
                )
                .padding([4, 10]),
            );
        }
        if self.storage.control.is_some() {
            footer = footer.push(
                action(
                    tr!(language, "查看空间统计" => "View storage calculation"),
                    Message::Storage(StorageMessage::Open),
                )
                .padding([4, 10]),
            );
        }
        if self.screen == Screen::Assets {
            footer = footer.push(icon(Icon::Maximize, 13.0, MUTED)).push(muted(
                trf!(language, "共 {} 项" => "{} items in total", self.total),
            ));
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
            .style(|theme| container::Style {
                background: Some(crate::theme::adaptive(theme, Color::WHITE).into()),
                text_color: Some(crate::theme::adaptive(theme, TEXT)),
                border: Border {
                    radius: 0.0.into(),
                    color: crate::theme::adaptive(theme, Color::WHITE),
                    width: 0.0,
                },
                ..Default::default()
            })
            .width(Length::Fill)
            .height(Length::Fill);
        let result: Element<'_, Message> = if let Some(dialog) = self.dialog {
            let project = dialog == Dialog::CreateProject;
            let title = match dialog {
                Dialog::CreateLibrary => tr!(language, "新建资源库" => "New library"),
                Dialog::CreateProject => tr!(language, "新建项目" => "New project"),
                Dialog::RenameLibrary => {
                    tr!(language, "编辑资源库信息" => "Edit library information")
                }
                Dialog::RenameAsset => tr!(language, "重命名素材" => "Asset renamed"),
                Dialog::Settings => tr!(language, "设置" => "Settings"),
            };
            let commit = match dialog {
                Dialog::CreateLibrary => Message::Create,
                Dialog::CreateProject => Message::CreateProject,
                Dialog::RenameLibrary => Message::SaveRenameLibrary,
                Dialog::RenameAsset => Message::SaveRenameAsset,
                Dialog::Settings => Message::Noop,
            };
            let description = match dialog {
                Dialog::CreateLibrary => {
                    tr!(language, "原图、标签和项目将一起保存在你选择的 .moriq 文件夹中。" => "Originals, tags and projects are stored together in the .moriq folder you choose.")
                }
                Dialog::CreateProject => {
                    tr!(language, "项目用于归类素材，原图仍保存在资源库中。" => "Projects organize assets. Originals remain in the library.")
                }
                Dialog::RenameLibrary => {
                    tr!(language, "修改资源库的显示名称，托管文件的位置保持不变。" => "Change the display name of this library. Managed files stay in their current location.")
                }
                Dialog::RenameAsset => {
                    tr!(language, "修改素材的显示名称，保留原片与所有版本。" => "Change the display name of this asset while preserving its original and all versions.")
                }
                Dialog::Settings => "",
            };
            let entry = text_input(
                if project {
                    tr!(language, "项目名称" => "Project name")
                } else {
                    tr!(language, "名称" => "Name")
                },
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
                    muted(tr!(language, "描述" => "Description")),
                    text_input(
                        tr!(language, "资源库描述" => "Library description"),
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
                        action(tr!(language, "取消" => "Cancel"), Message::HideCreate),
                        Space::new().width(Length::Fill),
                        button(
                            text(if dialog == Dialog::CreateLibrary {
                                tr!(language, "选择位置并创建" => "Choose location and create")
                            } else {
                                tr!(language, "保存" => "Save")
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
            // Keep the base at child zero when a menu or dialog opens.
            stack![base].into()
        };
        let result: Element<'_, Message> = if let Some(error) = &self.error {
            let toast = container(
                row![
                    scroll(
                        text(error)
                            .size(12)
                            .style(crate::theme::text_color(Color::from_rgb8(176, 46, 46)))
                    )
                    .height(Length::Shrink)
                    .width(Length::Fill),
                    icon_button(Icon::Close, Message::Dismiss)
                ]
                .spacing(12),
            )
            .padding(14)
            .width((self.viewport.width - 40.0).clamp(260.0, 480.0))
            .max_height(150)
            .style(|theme| container::Style {
                background: Some(crate::theme::adaptive(theme, Color::WHITE).into()),
                border: Border {
                    color: crate::theme::adaptive(theme, Color::from_rgb8(244, 208, 208)),
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
            stack![result].into()
        };
        container(self.drop_overlay(result))
            .padding(self.frame_padding())
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    }
    /// 构建顶部标题、导航和主要操作区域。
    fn header(&self) -> Element<'_, Message> {
        let language = self.preferences.language;
        // Fixed chrome slots: page-specific labels never move search or window controls.
        let search_width = (self.viewport.width - self.sidebar_width() - 668.0).clamp(160.0, 700.0);
        let search = container(
            row![
                icon(Icon::Search, 17.0, MUTED),
                text_input(
                    if self.screen == Screen::Models {
                        tr!(language, "搜索模型与功能说明…" => "Search models and capabilities…")
                    } else {
                        tr!(language, "搜索文件名、标签、描述或备注…" => "Search file names, tags, descriptions or notes…")
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
                        .style(crate::theme::text_color(MUTED))
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
                language,
                vec!["选择图片文件", "导入文件夹", "引用外部原片"],
                None::<&str>,
                |choice| match choice {
                    "导入文件夹" => Message::Manage(ManageMessage::FolderImport),
                    "引用外部原片" => Message::Manage(ManageMessage::ReferenceImport),
                    _ => Message::Import,
                },
            )
            .placeholder(tr!(language, "导入" => "Import"))
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
                        tr!(language, "导入中…" => "Importing…")
                    } else {
                        tr!(language, "导入" => "Import")
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
                tr!(language, "网格视图" => "Grid view"),
                Message::List(false),
                self.screen == Screen::Assets && !self.list,
            ),
            (
                Icon::List,
                tr!(language, "列表视图" => "List view"),
                Message::List(true),
                self.screen == Screen::Assets && self.list,
            ),
            (
                Icon::Filter,
                tr!(language, "筛选素材" => "Filter assets"),
                Message::Manage(ManageMessage::Filters),
                false,
            ),
            (
                Icon::More,
                tr!(language, "资源库工具" => "Library tools"),
                Message::Manage(ManageMessage::Menu),
                false,
            ),
            (
                Icon::Collection,
                tr!(language, "智能集合" => "Smart collections"),
                Message::Manage(ManageMessage::Collections),
                false,
            ),
            (
                Icon::Settings,
                tr!(language, "设置" => "Settings"),
                Message::ShowSettings,
                false,
            ),
            (
                Icon::Bell,
                tr!(language, "通知" => "Notifications"),
                Message::Journal(JournalMessage::Open(true)),
                self.journal.notifications.unread > 0,
            ),
        ] {
            tools = tools.push(
                iced::widget::tooltip(
                    crate::motion::feedback(
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
                        true,
                        self.preferences.reduced_motion,
                    ),
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
                            style.background = Some(
                                crate::theme::adaptive(theme, Color::from_rgb8(255, 224, 225))
                                    .into(),
                            );
                        }
                        style
                    }),
            );
        }
        let back_enabled = self.library.is_some() && !self.switching && self.can_go_back();
        let forward_enabled = self.library.is_some()
            && !self.switching
            && !self.surface_open()
            && !self.viewer.preview_expanded
            && self.history.can_forward();
        let back = container(
            row![
                crate::motion::feedback(
                    button(icon(Icon::Left, 18.0, ICON))
                        .style(flat)
                        .padding(9)
                        .on_press_maybe(back_enabled.then_some(Message::Back))
                        .width(36)
                        .height(36),
                    back_enabled,
                    self.preferences.reduced_motion
                ),
                crate::motion::feedback(
                    button(icon(Icon::Right, 18.0, ICON))
                        .style(flat)
                        .padding(9)
                        .on_press_maybe(forward_enabled.then_some(Message::Forward))
                        .width(36)
                        .height(36),
                    forward_enabled,
                    self.preferences.reduced_motion
                ),
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
        .style(|theme| container::Style {
            background: Some(crate::theme::adaptive(theme, Color::WHITE).into()),
            ..Default::default()
        })
        .into()
    }
    /// 构建单项侧栏导航入口及选中状态。
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
            text(label.to_string())
                .size(13)
                .style(crate::theme::text_color(if unavailable {
                    MUTED
                } else if active {
                    BLUE
                } else {
                    TEXT
                })),
            Space::new().width(Length::Fill)
        ]
        .spacing(11)
        .align_y(Alignment::Center);
        if let Some(count) = count {
            line = line.push(text(count).size(11).style(crate::theme::text_color(MUTED)));
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
            .style(theme::liquid_nav(active));
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
    /// 构建资源库、项目和分类导航区域。
    fn sidebar(&self) -> Element<'_, Message> {
        let language = self.preferences.language;
        let logo = container(
            mouse_area(
                row![icon(Icon::Logo, 29.0, BLUE), heading("Moriq Space", 17)]
                    .spacing(10)
                    .align_y(Alignment::Center),
            )
            .on_press(Message::WindowDrag),
        )
        .height(self.titlebar_height())
        .width(Length::Fill)
        .padding([0, 22])
        .align_y(Alignment::Center);
        let navigation = crate::liquid::selection(
            column![
                self.nav_row(
                    Icon::Folder,
                    tr!(language, "资源库" => "Library"),
                    None,
                    Message::ShowLibraries,
                    self.screen != Screen::Models
                ),
                self.nav_row(
                    Icon::Cube,
                    tr!(language, "AI 模型" => "AI Models"),
                    None,
                    Message::Navigate(2),
                    self.screen == Screen::Models
                )
            ]
            .spacing(5),
            Some(usize::from(self.screen == Screen::Models)),
            crate::liquid::Treatment::Selection,
            self.preferences.reduced_motion,
        );
        let mut content = column![navigation];
        content = content
            .push(Space::new().height(if self.screen == Screen::Assets { 6 } else { 13 }))
            .push(separator())
            .push(Space::new().height(if self.screen == Screen::Assets { 3 } else { 10 }));
        if self.screen == Screen::Models {
            content = content
                .push(heading(tr!(language, "资源库" => "Library"), 13))
                .push(Space::new().height(6));
            let can_switch = !self.switching && self.import.is_none() && !self.manager.busy;
            let libraries = self.model_libraries();
            if libraries.is_empty() {
                content = content.push(muted(tr!(language, "暂无资源库，打开后会显示在这里" => "No libraries yet. Open a library to see it here.")));
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
                                    .style(crate::theme::text_color(if active {
                                        BLUE
                                    } else {
                                        TEXT
                                    })),
                                text(compact(&location, 21))
                                    .size(10)
                                    .style(crate::theme::text_color(MUTED))
                                    .width(Length::Fill)
                                    .height(14)
                                    .wrapping(text::Wrapping::None),
                            ]
                            .spacing(4)
                            .width(Length::Fill)
                        )
                        .width(Length::Fill)
                        .clip(true),
                        text(if active {
                            tr!(language, "当前" => "Current")
                        } else {
                            ""
                        })
                        .size(10)
                        .style(crate::theme::text_color(BLUE))
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
                                    .style(crate::theme::text_color(ICON))
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
                    action(tr!(language, "打开其他资源库…" => "Open another library…"), Message::Open)
                        .on_press_maybe(can_switch.then_some(Message::Open)),
                )
                .push(muted(if self.switching {
                    tr!(language, "正在切换资源库…" => "Switching libraries…")
                } else {
                    tr!(language, "模型安装与设置全局可用" => "Model installation and settings are available globally")
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
            let can_open = !self.switching
                && self.import.is_none()
                && !self.manager.busy
                && !self.libraries.busy;
            content = content
                .push(
                    row![
                        muted(tr!(language, "我的资源库" => "My libraries")).size(11),
                        Space::new().width(Length::Fill),
                        iced::widget::tooltip(
                            button(icon(Icon::Folder, 15.0, ICON))
                                .padding(7)
                                .width(30)
                                .height(30)
                                .style(library_action)
                                .on_press_maybe(can_open.then_some(Message::Open)),
                            container(text(tr!(language, "打开资源库" => "Open library")).size(12))
                                .padding([7, 10])
                                .style(library_hint),
                            iced::widget::tooltip::Position::Right
                        )
                        .delay(Duration::from_millis(650))
                    ]
                    .align_y(Alignment::Center),
                )
                .push(
                    button(
                        row![
                            icon(Icon::Plus, 17.0, BLUE),
                            text(tr!(language, "新建资源库" => "New library"))
                                .size(13)
                                .style(crate::theme::text_color(BLUE))
                        ]
                        .spacing(9)
                        .align_y(Alignment::Center),
                    )
                    .on_press_maybe(can_open.then_some(Message::ShowCreate))
                    .padding([10, 12])
                    .width(Length::Fill)
                    .style(secondary),
                );
            if let Some(library) = &self.library {
                let name = &library.info.name;
                let count = library.info.count;
                let thumbnail = self.library_cover_for(&library.root);
                content = content.push(
                    button(
                        row![
                            photo(thumbnail, Length::Fixed(40.0), 40.0),
                            column![
                                heading(compact(name, 9), 13)
                                    .width(Length::Fill)
                                    .height(19)
                                    .wrapping(text::Wrapping::None),
                                muted(trf!(language, "{} 张素材" => "{} assets", number(count)))
                                    .size(11)
                            ]
                            .spacing(4)
                            .width(Length::Fill),
                            container(icon(Icon::Right, 13.0, BLUE)).center_y(40)
                        ]
                        .spacing(10)
                        .align_y(Alignment::Center),
                    )
                    .padding([10, 10])
                    .width(Length::Fill)
                    .style(theme::library_entry(true))
                    .on_press_maybe(can_open.then_some(Message::Navigate(0))),
                );
            }
            content = content.push(
                button(
                    row![
                        icon(Icon::Folder, 16.0, MUTED),
                        muted(tr!(language, "打开资源库" => "Open library")).size(12)
                    ]
                    .spacing(9)
                    .align_y(Alignment::Center),
                )
                .on_press_maybe(can_open.then_some(Message::Open))
                .padding([10, 12])
                .width(Length::Fill)
                .style(library_action),
            );
        } else if self.library.is_some() {
            content = content.push(
                row![
                    muted(if self.screen == Screen::Models {
                        tr!(language, "我的项目" => "My projects")
                    } else {
                        tr!(language, "项目" => "Projects")
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
                            text(number(project.count))
                                .size(11)
                                .style(crate::theme::text_color(MUTED))
                        ]
                        .align_y(Alignment::Center)
                        .into()
                    } else {
                        column![
                            text(manage::project_label(&library.info.projects, project.id))
                                .size(13)
                                .style(crate::theme::text_color(if selected {
                                    BLUE
                                } else {
                                    TEXT
                                })),
                            muted(trf!(language, "{} 张" => "{} photos", number(project.count)))
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
                    tr!(language, "智能集合" => "Smart collections")
                } else {
                    tr!(language, "素材库" => "Assets")
                }));
            let total = self.library.as_ref().map(|l| l.info.count).unwrap_or(0);
            let favorites = self.library.as_ref().map(|l| l.info.favorites).unwrap_or(0);
            let mut rows = if self.screen == Screen::Models {
                vec![
                    (
                        Icon::Clock,
                        tr!(language, "最近导入" => "Recently imported"),
                        self.manager.organizer.recent_count,
                        Message::Manage(ManageMessage::Builtin(Builtin::Recent)),
                    ),
                    (
                        Icon::Tag,
                        tr!(language, "未加标签" => "Untagged"),
                        self.manager.organizer.untagged_count,
                        Message::Manage(ManageMessage::Builtin(Builtin::Untagged)),
                    ),
                    (
                        Icon::Star,
                        tr!(language, "高评分" => "Highly rated"),
                        self.manager.organizer.high_rating_count,
                        Message::Manage(ManageMessage::Builtin(Builtin::HighRating)),
                    ),
                    (
                        Icon::Raw,
                        tr!(language, "RAW 文件" => "RAW files"),
                        self.manager.organizer.raw_count,
                        Message::Manage(ManageMessage::Builtin(Builtin::Raw)),
                    ),
                    (
                        Icon::Video,
                        tr!(language, "视频" => "Video"),
                        self.manager.organizer.video_count,
                        Message::Notice(tr!(language, "当前尚不支持视频预览" => "Video preview is currently unavailable").into()),
                    ),
                    (
                        Icon::Duplicate,
                        tr!(language, "重复文件" => "Duplicate files"),
                        0,
                        Message::Notice(tr!(language, "导入时会自动跳过完全相同的图片" => "Identical images are automatically skipped during import").into()),
                    ),
                ]
            } else {
                vec![
                    (
                        Icon::Folder,
                        tr!(language, "全部素材" => "All assets"),
                        total,
                        Message::AllAssets,
                    ),
                    (
                        Icon::Clock,
                        tr!(language, "最近添加" => "Recently added"),
                        self.manager.organizer.recent_count,
                        Message::Manage(ManageMessage::Builtin(Builtin::Recent)),
                    ),
                    (
                        Icon::Edit,
                        tr!(language, "最近编辑" => "Recently edited"),
                        self.manager.organizer.edited_count,
                        Message::Manage(ManageMessage::Builtin(Builtin::Edited)),
                    ),
                    (
                        Icon::Heart,
                        tr!(language, "收藏夹" => "Favorites"),
                        favorites,
                        Message::Favorites,
                    ),
                    (
                        Icon::Raw,
                        tr!(language, "RAW 文件" => "RAW files"),
                        self.manager.organizer.raw_count,
                        Message::Manage(ManageMessage::Builtin(Builtin::Raw)),
                    ),
                    (
                        Icon::Tag,
                        tr!(language, "标签" => "Tags"),
                        self.manager.organizer.tags.len() as u64,
                        Message::Manage(ManageMessage::Tags),
                    ),
                    (
                        Icon::Collection,
                        tr!(language, "智能集合" => "Smart collections"),
                        self.manager.organizer.collections.len() as u64,
                        Message::Manage(ManageMessage::Collections),
                    ),
                    (
                        Icon::Person,
                        tr!(language, "人脸识别" => "Face recognition"),
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
                        tr!(language, "视频" => "Video"),
                        self.manager.organizer.video_count,
                        Message::Manage(ManageMessage::Builtin(Builtin::Video)),
                    ),
                    (
                        Icon::Unsorted,
                        tr!(language, "未归类项目" => "Without project"),
                        self.manager.organizer.unassigned_count,
                        Message::Manage(ManageMessage::Builtin(Builtin::Unassigned)),
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
                            self.manager
                                .organizer
                                .ai_groups
                                .iter()
                                .find(|g| self.group_label(g) == *category)
                                .map_or_else(
                                    || Message::Ai(AiMessage::Category(category.clone())),
                                    |g| Message::Ai(AiMessage::Group(g.id.clone())),
                                ),
                        ),
                    );
                }
            }
            let row_count = rows.len();
            let mut navigation = column![].spacing(4);
            let mut active_row = None;
            for (index, (kind, label, count, message)) in rows.into_iter().enumerate() {
                let displayed = count;
                let active = match &message {
                    Message::AllAssets => self.query == Query::default(),
                    Message::Favorites => self.query.favorites_only,
                    Message::Manage(ManageMessage::Builtin(Builtin::Recent)) => {
                        self.query.since > 0
                    }
                    Message::Manage(ManageMessage::Builtin(Builtin::Untagged)) => {
                        self.query.untagged
                    }
                    Message::Manage(ManageMessage::Builtin(Builtin::HighRating)) => {
                        self.query.min_rating >= 4
                    }
                    Message::Manage(ManageMessage::Builtin(Builtin::Unassigned)) => {
                        self.query.unassigned
                    }
                    Message::Manage(ManageMessage::Builtin(Builtin::Edited)) => {
                        self.query.edited_since > 0
                    }
                    Message::Manage(ManageMessage::Builtin(Builtin::Raw)) => {
                        self.query.media_kind == MediaKind::Raw
                    }
                    Message::Manage(ManageMessage::Builtin(Builtin::Video)) => {
                        self.query.media_kind == MediaKind::Video
                    }
                    Message::Ai(AiMessage::People) => {
                        self.manager.panel == Some(manage::Panel::People)
                    }
                    Message::Ai(AiMessage::Group(id)) => self
                        .manager
                        .organizer
                        .ai_groups
                        .iter()
                        .find(|g| &g.id == id)
                        .is_some_and(|g| g.query == self.query),
                    Message::Ai(AiMessage::Category(category)) => {
                        self.query.category == *category
                            || AI_MEDIA
                                .iter()
                                .any(|(id, label)| *label == category && self.query.medium == *id)
                            || AI_THEMES.iter().any(|(id, label)| {
                                *label == category && self.query.themes.iter().any(|s| s == id)
                            })
                    }
                    _ => false,
                };
                let label = if matches!(&message, Message::Ai(AiMessage::Category(_))) {
                    crate::i18n::category(language, label)
                } else {
                    label
                };
                let active = active && self.screen == Screen::Assets;
                if active {
                    active_row = Some(index);
                }
                navigation = navigation.push(self.nav_row(
                    kind,
                    label,
                    Some(number(displayed)),
                    message,
                    active,
                ));
            }
            if self.screen == Screen::Assets {
                if self.query.trash {
                    active_row = Some(row_count + 1);
                }
                navigation = navigation.push(separator()).push(self.nav_row(
                    Icon::Trash,
                    tr!(language, "回收站" => "Trash"),
                    Some(number(self.manager.organizer.trash_count)),
                    Message::Manage(ManageMessage::Builtin(Builtin::Trash)),
                    self.query.trash,
                ));
            }
            content = content.push(crate::liquid::selection(
                navigation,
                active_row,
                crate::liquid::Treatment::Selection,
                self.preferences.reduced_motion,
            ));
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
                .push(muted(self.storage
                        .volume
                        .map(|(total, available)| {
                            trf!(language, "所在卷 · 可用 {} / {}" => "Volume · {} / {} available", size(available), size(total))
                        })
                        .unwrap_or_else(|| tr!(language, "正在查询卷容量…" => "Checking volume capacity…").into())))
                .push(action(tr!(language, "空间明细" => "Storage details"), Message::Storage(StorageMessage::Open)));
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
