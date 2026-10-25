use super::*;
use crate::components::*;
use crate::icons::{Icon, icon};
use crate::theme::*;
use iced::widget::{Row, Space, button, column, container, image, row, text, text_input, tooltip};
use iced::{Alignment, Border, Color, ContentFit, Length, Theme};
impl App {
    pub(super) fn welcome(&self) -> Element<'_, Message> {
        let language = self.preferences.language;
        if !self.preferences.recent.is_empty() {
            let can_open = !self.switching
                && self.import.is_none()
                && !self.manager.busy
                && !self.libraries.busy;
            let card_width = library_card_width(self.viewport.width - self.sidebar_width() - 64.0);
            let mut cards = Row::new().spacing(18);
            for (name, path) in &self.preferences.recent {
                let cover_height = (card_width - 28.0) * 0.56;
                let cover: Element<'_, Message> =
                    if let Some(handle) = self.library_covers.get(path) {
                        image(handle.clone())
                            .width(Length::Fill)
                            .height(cover_height)
                            .content_fit(ContentFit::Cover)
                            .border_radius(13)
                            .into()
                    } else {
                        container(icon(Icon::Image, 36.0, MUTED))
                            .center_x(Length::Fill)
                            .center_y(cover_height)
                            .style(|theme| container::Style {
                                border: Border {
                                    radius: 13.0.into(),
                                    ..Default::default()
                                },
                                ..pale(theme)
                            })
                            .into()
                    };
                let location = display_path(path.parent().unwrap_or(path));
                let name_limit = (((card_width - 40.0) / 18.0) as usize).max(2);
                let path_limit = (((card_width - 40.0) / 12.0) as usize).max(2);
                let open = button(
                    column![
                        cover,
                        container(
                            column![
                                heading(compact(name, name_limit), 18)
                                    .width(Length::Fill)
                                    .height(26)
                                    .wrapping(text::Wrapping::None),
                                muted(compact(&location, path_limit))
                                    .size(12)
                                    .width(Length::Fill)
                                    .height(18)
                                    .wrapping(text::Wrapping::None),
                            ]
                            .spacing(6)
                            .width(Length::Fill)
                        )
                        .padding([0, 4])
                        .width(Length::Fill)
                        .clip(true),
                    ]
                    .spacing(12),
                )
                .padding(0)
                .width(Length::Fill)
                .style(library_open_style)
                .on_press_maybe(can_open.then(|| Message::OpenRecent(path.clone())));
                cards = cards.push(
                    container(
                        column![
                            tooltip(
                                open,
                                container(
                                    column![
                                        heading(name.clone(), 13),
                                        text(display_path(path))
                                            .size(12)
                                            .style(crate::theme::text_color(ICON))
                                            .wrapping(text::Wrapping::Glyph),
                                    ]
                                    .spacing(6),
                                )
                                .padding([10, 12])
                                .width(360)
                                .style(panel),
                                tooltip::Position::Bottom,
                            )
                            .gap(8)
                            .delay(Duration::from_millis(450))
                            .snap_within_viewport(true),
                            button(
                                row![
                                    icon(Icon::Settings, 18.0, MUTED),
                                    text(tr!(language, "管理资源库" => "Manage library")).size(13),
                                    Space::new().width(Length::Fill),
                                    icon(Icon::Right, 15.0, MUTED),
                                ]
                                .spacing(10)
                                .align_y(Alignment::Center)
                            )
                            .padding([10, 12])
                            .style(library_manage_style)
                            .width(Length::Fill)
                            .on_press_maybe(can_open.then(|| Message::Libraries(
                                crate::libraries::LibraryMessage::Open(path.clone())
                            )))
                        ]
                        .spacing(12),
                    )
                    .padding(14)
                    .width(card_width)
                    .style(library_card_style),
                );
            }
            return scroll(
                container(
                    column![
                        row![
                            heading(tr!(language, "我的资源库" => "My libraries"), 28),
                            Space::new().width(Length::Fill),
                            action(
                                tr!(language, "打开已有资源库" => "Open an existing library"),
                                Message::Open
                            )
                            .on_press_maybe(can_open.then_some(Message::Open)),
                            action(
                                tr!(language, "新建资源库" => "New library"),
                                Message::ShowCreate
                            )
                            .style(primary)
                            .on_press_maybe(can_open.then_some(Message::ShowCreate)),
                        ]
                        .spacing(12)
                        .align_y(Alignment::Center),
                        cards.wrap(),
                    ]
                    .spacing(30),
                )
                .padding(32)
                .width(Length::Fill),
            )
            .height(Length::Fill)
            .into();
        }
        let mut create = button(text(if self.switching {
            tr!(language, "正在打开…" => "Opening…")
        } else {
            tr!(language, "选择位置并创建" => "Choose location and create")
        }))
        .padding([13, 24])
        .style(primary);
        let mut open = action(
            tr!(language, "打开已有资源库" => "Open an existing library"),
            Message::Open,
        );
        if !self.switching {
            create = create.on_press(Message::Create);
        } else {
            open = button(text(
                tr!(language, "打开已有资源库" => "Open an existing library"),
            ))
            .padding(13)
            .style(secondary);
        }
        container(
            column![
                container(icon(Icon::Logo, 88.0, BLUE))
                    .center_x(100)
                    .center_y(100),
                Space::new().height(12),
                heading(tr!(language, "每一份灵感，都有归处。" => "A home for every inspiration."), 32),
                muted(tr!(language, "欢迎使用 Moriq Space。创建一个资源库，开始整理你的照片与创意。" => "Welcome to Moriq Space. Create a library to organize your photos and ideas.")),
                Space::new().height(12),
                container(
                    column![
                        heading(tr!(language, "创建你的第一个资源库" => "Create your first library"), 18),
                        text_input(tr!(language, "资源库名称" => "Library name"), &self.name)
                            .on_input(Message::Name)
                            .padding(13)
                            .style(input),
                        muted(tr!(language, "选择保存位置后，原片、标签和项目将一起保存在 .moriq 文件夹中。" => "After choosing a location, originals, tags and projects are stored together in a .moriq folder.")),
                        row![create, open].spacing(10),
                    ]
                    .spacing(18)
                )
                .padding(28)
                .width(580)
                .style(panel),
                muted(tr!(language, "本地存储  ·  原片不覆盖  ·  AI 模型按需安装" => "Local storage  ·  Originals preserved  ·  AI models on demand")),
            ]
            .spacing(16)
            .align_x(Alignment::Center),
        )
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
    }
}
fn library_card_width(available: f32) -> f32 {
    let available = available.max(1.0);
    let columns = ((available + 18.0) / 298.0).floor().max(1.0);
    ((available - (columns - 1.0) * 18.0) / columns).min(320.0)
}
fn library_card_style(theme: &Theme) -> container::Style {
    container::Style {
        border: Border {
            radius: 18.0.into(),
            ..panel(theme).border
        },
        shadow: iced::Shadow {
            color: Color::from_rgba8(25, 39, 65, 0.05),
            offset: iced::Vector::new(0.0, 5.0),
            blur_radius: 20.0,
        },
        ..panel(theme)
    }
}
fn library_open_style(theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: match status {
            button::Status::Hovered => Some(adaptive(theme, BG).into()),
            button::Status::Pressed => Some(adaptive(theme, PALE).into()),
            _ => None,
        },
        text_color: adaptive(theme, TEXT),
        border: Border {
            radius: 13.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}
fn library_manage_style(theme: &Theme, status: button::Status) -> button::Style {
    let base = secondary(theme, status);
    button::Style {
        background: Some(
            adaptive(
                theme,
                match status {
                    button::Status::Hovered | button::Status::Pressed => PALE,
                    _ => BG,
                },
            )
            .into(),
        ),
        border: Border {
            radius: 13.0.into(),
            color: if matches!(status, button::Status::Hovered | button::Status::Pressed) {
                adaptive(theme, CONTROL_LINE)
            } else {
                adaptive(theme, LINE)
            },
            ..base.border
        },
        ..base
    }
}
pub(super) fn compact(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value.to_owned();
    }
    format!("{}…", value.chars().take(limit - 1).collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_cards_stay_compact_and_fit_responsive_columns() {
        for available in [240.0, 440.0, 625.0, 626.0, 800.0, 1180.0, 2000.0] {
            let width = library_card_width(available);
            assert!(width <= available);
            assert!(width <= 320.0);
            let columns = ((available + 18.0) / 298.0).floor().max(1.0);
            assert!(width * columns + (columns - 1.0) * 18.0 <= available);
            if columns > 1.0 {
                assert!(width >= 280.0);
            }
        }
    }

    #[test]
    fn library_picker_cards_build_with_real_identity_and_independent_actions() {
        let mut app = crate::tests::app();
        app.library = None;
        app.library_picker = true;
        let path = PathBuf::from("D:/photos/中文目录/My library.moriq");
        app.preferences.recent = vec![
            ("同名且特别长的资源库名称以检查截断".into(), path.clone()),
            (
                "同名且特别长的资源库名称以检查截断".into(),
                PathBuf::from("E:/other.moriq"),
            ),
        ];
        app.library_covers.insert(
            path.clone(),
            image::Handle::from_rgba(1, 1, vec![180, 190, 210, 255]),
        );
        for language in [Language::SimplifiedChinese, Language::English] {
            app.preferences.language = language;
            for width in [720.0, 1180.0, 1660.0] {
                app.viewport.width = width;
                drop(app.welcome());
                app.switching = true;
                drop(app.welcome());
                app.switching = false;
            }
        }
        drop(app.library_message(crate::libraries::LibraryMessage::Open(path.clone())));
        assert_eq!(app.libraries.path, Some(path));
        assert_eq!(app.manager.panel, Some(manage::Panel::Libraries));
        assert!(app.library.is_none());
        for appearance in [Appearance::Light, Appearance::Dark] {
            let theme = application_theme(appearance);
            assert_eq!(library_card_style(&theme).border.radius, 18.0.into());
            for status in [
                button::Status::Active,
                button::Status::Hovered,
                button::Status::Disabled,
            ] {
                assert!(library_manage_style(&theme, status).background.is_some());
            }
        }
    }
}
