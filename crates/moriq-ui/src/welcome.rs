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
            return self.library_picker_cards();
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
impl App {
    fn library_picker_cards(&self) -> Element<'_, Message> {
        let language = self.preferences.language;
        let can_open =
            !self.switching && self.import.is_none() && !self.manager.busy && !self.libraries.busy;
        let width = library_card_width(self.viewport.width - self.sidebar_width() - 88.0);
        let cover_height = (width - 20.0) * 0.48;
        let mut cards = Row::new().spacing(20);
        for (name, path) in &self.preferences.recent {
            let (kind, accent, tint) = library_accent(name, path);
            let cover: Element<'_, Message> = if let Some(handle) = self.library_covers.get(path) {
                let mut photo = image(handle.clone())
                    .width(Length::Fill)
                    .height(cover_height)
                    .content_fit(ContentFit::Cover)
                    .border_radius(14);
                if let image::Handle::Rgba { width, height, .. } = handle
                    && height > width
                {
                    let crop_height = (*width as f32 * 0.48).round().max(1.0) as u32;
                    let y = ((*height - crop_height) as f32 * 0.22).round() as u32;
                    photo = photo.crop(iced::Rectangle {
                        x: 0,
                        y,
                        width: *width,
                        height: crop_height,
                    });
                }
                photo.into()
            } else {
                container(icon(Icon::Image, 32.0, MUTED))
                    .center_x(Length::Fill)
                    .center_y(cover_height)
                    .into()
            };
            let cover = button(
                container(cover)
                    .width(Length::Fill)
                    .height(cover_height)
                    .style(move |theme| container::Style {
                        background: Some(adaptive(theme, BG).into()),
                        border: Border {
                            radius: 14.0.into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    }),
            )
            .padding(0)
            .style(library_open_style)
            .width(Length::Fill)
            .on_press_maybe(can_open.then(|| Message::OpenRecent(path.clone())));
            let badge = container(library_badge(kind, accent))
                .center_x(58)
                .center_y(58)
                .style(move |theme| container::Style {
                    background: Some(adaptive(theme, tint).into()),
                    border: Border {
                        radius: 17.0.into(),
                        color: if theme.extended_palette().is_dark {
                            Color::from_rgb8(33, 41, 55)
                        } else {
                            Color::WHITE
                        },
                        width: 2.0,
                    },
                    ..Default::default()
                });
            let location = display_path(path.parent().unwrap_or(path));
            let name_limit = (((width - 113.0) / 15.0) as usize).max(2);
            let path_limit = (((width - 113.0) / 9.0) as usize).max(2);
            let identity = button(
                row![
                    badge,
                    column![
                        heading(compact(name, name_limit), 16)
                            .width(Length::Fill)
                            .height(21)
                            .wrapping(text::Wrapping::None),
                        muted(compact(&location, path_limit))
                            .size(12)
                            .height(17)
                            .wrapping(text::Wrapping::None)
                    ]
                    .spacing(3)
                    .padding(iced::Padding {
                        top: 24.0,
                        right: 0.0,
                        bottom: 0.0,
                        left: 0.0
                    })
                    .width(Length::Fill)
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            )
            .padding(0)
            .style(library_open_style)
            .width(Length::Fill)
            .on_press_maybe(can_open.then(|| Message::OpenRecent(path.clone())));
            let menu = button(icon(Icon::More, 19.0, MUTED))
                .padding([9, 7])
                .style(library_manage_style)
                .on_press_maybe(can_open.then(|| {
                    Message::Libraries(crate::libraries::LibraryMessage::Open(path.clone()))
                }));
            let identity = row![
                tooltip(
                    identity,
                    container(
                        column![heading(name.clone(), 13), text(display_path(path)).size(12)]
                            .spacing(5)
                    )
                    .padding(10)
                    .width(360)
                    .style(panel),
                    tooltip::Position::Bottom
                )
                .delay(Duration::from_millis(450))
                .snap_within_viewport(true),
                menu
            ]
            .spacing(5)
            .align_y(Alignment::Center);
            let body = iced::widget::stack![
                column![cover, Space::new().height(54)],
                container(identity)
                    .padding(iced::Padding {
                        top: cover_height - 15.0,
                        right: 7.0,
                        bottom: 0.0,
                        left: 10.0
                    })
                    .width(Length::Fill)
            ];
            let summary = self
                .library
                .as_ref()
                .filter(|l| l.root == *path)
                .map(|l| (l.info.count, l.info.updated_at))
                .or_else(|| self.library_summaries.get(path).copied());
            let count = summary
                .map(|(n, _)| trf!(language,"{} 张"=>"{} assets",grouped_number(n)))
                .unwrap_or_else(|| "—".into());
            let updated = summary
                .and_then(|(_, t)| i64::try_from(t).ok())
                .and_then(chrono::DateTime::from_timestamp_millis)
                .map(|t| {
                    t.with_timezone(&chrono::Local)
                        .format("%Y/%m/%d %H:%M")
                        .to_string()
                })
                .unwrap_or_else(|| "—".into());
            let footer = container(
                row![
                    icon(Icon::Image, 13.0, MUTED),
                    muted(count).size(11),
                    Space::new().width(Length::Fill),
                    icon(Icon::Clock, 13.0, MUTED),
                    muted(updated).size(10)
                ]
                .spacing(5)
                .align_y(Alignment::Center),
            )
            .padding([8, 10])
            .width(Length::Fill)
            .style(|theme| container::Style {
                border: Border {
                    radius: 11.0.into(),
                    ..Default::default()
                },
                background: Some(
                    if theme.extended_palette().is_dark {
                        Color::from_rgb8(39, 48, 63)
                    } else {
                        Color::from_rgb8(246, 248, 251)
                    }
                    .into(),
                ),
                ..Default::default()
            });
            cards = cards.push(
                container(column![body, footer].spacing(5))
                    .padding(10)
                    .width(width)
                    .style(library_card_style),
            );
        }
        let new_cover = container(
            column![
                container(library_badge_size(
                    Icon::Folder,
                    Color::from_rgb8(76, 156, 213),
                    58.0
                ))
                .center_x(70)
                .center_y(70),
                muted(tr!(language,"收好每一份新的灵感"=>"Make room for new inspiration")).size(13)
            ]
            .spacing(12)
            .align_x(Alignment::Center),
        )
        .center_x(Length::Fill)
        .center_y(cover_height)
        .style(|theme| container::Style {
            background: Some(cover_gradient(
                theme,
                (
                    Color::from_rgb8(232, 243, 250),
                    Color::from_rgb8(246, 249, 252),
                ),
            )),
            border: Border {
                radius: 14.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });
        let new_badge = container(library_badge(Icon::Plus, Color::from_rgb8(60, 185, 202)))
            .center_x(58)
            .center_y(58)
            .style(|theme| container::Style {
                background: Some(
                    if theme.extended_palette().is_dark {
                        Color::from_rgb8(34, 67, 78)
                    } else {
                        Color::from_rgb8(221, 247, 251)
                    }
                    .into(),
                ),
                border: Border {
                    radius: 17.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            });
        let new_identity = row![
            new_badge,
            column![
                heading(tr!(language,"新建资源库"=>"New library"), 16),
                muted(tr!(language,"创建一个新的资源库"=>"Create a new library")).size(12)
            ]
            .spacing(4)
            .width(Length::Fill),
            icon(Icon::Right, 19.0, MUTED)
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        let new_body = iced::widget::stack![
            column![new_cover, Space::new().height(54)],
            container(new_identity)
                .padding(iced::Padding {
                    top: cover_height - 15.0,
                    right: 12.0,
                    bottom: 0.0,
                    left: 10.0
                })
                .width(Length::Fill)
        ];
        let new_card = button(column![new_body, Space::new().height(29)].spacing(5))
            .padding(10)
            .width(width)
            .style(library_new_style)
            .on_press_maybe(can_open.then_some(Message::ShowCreate));
        cards = cards.push(new_card);
        container(
            scroll(
                container(
                    column![
                        row![
                            heading(tr!(language,"我的资源库"=>"My libraries"), 25),
                            Space::new().width(Length::Fill),
                            action(
                                tr!(language,"打开已有资源库"=>"Open an existing library"),
                                Message::Open
                            )
                            .on_press_maybe(can_open.then_some(Message::Open))
                        ]
                        .spacing(12)
                        .align_y(Alignment::Center),
                        cards.wrap()
                    ]
                    .spacing(22),
                )
                .padding(32)
                .width(Length::Fill),
            )
            .height(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(|theme| container::Style {
            background: Some(
                if theme.extended_palette().is_dark {
                    Color::from_rgb8(25, 32, 43)
                } else {
                    Color::from_rgb8(246, 248, 252)
                }
                .into(),
            ),
            ..Default::default()
        })
        .into()
    }
}
fn cover_gradient(theme: &Theme, tones: (Color, Color)) -> iced::Background {
    let dark = theme.extended_palette().is_dark;
    let base = if dark {
        Color::from_rgb8(31, 39, 52)
    } else {
        Color::WHITE
    };
    let mix = |color: Color| {
        let weight = if dark { 0.22 } else { 0.18 };
        Color::from_rgb(
            base.r * (1.0 - weight) + color.r * weight,
            base.g * (1.0 - weight) + color.g * weight,
            base.b * (1.0 - weight) + color.b * weight,
        )
    };
    iced::Background::Gradient(iced::Gradient::Linear(
        iced::gradient::Linear::new(iced::Radians(std::f32::consts::FRAC_PI_4))
            .add_stop(0.0, mix(tones.0))
            .add_stop(1.0, mix(tones.1)),
    ))
}
fn library_badge(kind: Icon, color: Color) -> Element<'static, Message> {
    library_badge_size(kind, color, 31.0)
}
fn library_badge_size(kind: Icon, color: Color, size: f32) -> Element<'static, Message> {
    let body = match kind {
        Icon::Landscape => {
            r#"<circle cx="23" cy="9" r="3" opacity=".55"/><path d="M3 26 12 9l8 17Z"/><path d="m14 26 8-13 8 13Z" opacity=".55"/>"#
        }
        Icon::Camera => {
            r#"<path d="m10 8 2-4h8l2 4h5a3 3 0 0 1 3 3v15a3 3 0 0 1-3 3H5a3 3 0 0 1-3-3V11a3 3 0 0 1 3-3Z"/><circle cx="16" cy="18" r="7" fill="white" opacity=".9"/><circle cx="16" cy="18" r="4"/><circle cx="25" cy="12" r="1.5" fill="white"/>"#
        }
        Icon::Sparkles => {
            r#"<path d="M16 2C8 2 2 8 2 16s6 14 14 14h2c4 0 5-4 2-6-2-2-1-5 2-5h3c4 0 6-3 5-7C28 6 22 2 16 2Z"/><circle cx="10" cy="10" r="2" fill="white"/><circle cx="18" cy="8" r="2" fill="white"/><circle cx="24" cy="13" r="2" fill="white"/><circle cx="7" cy="18" r="2" fill="white"/>"#
        }
        Icon::Star => {
            r#"<path d="M16 2a11 11 0 0 0-8 19c2 2 2 3 2 5h12c0-2 0-3 2-5a11 11 0 0 0-8-19Z"/><rect x="11" y="28" width="10" height="3" rx="1.5"/><path d="m11 13 5 5 5-5" fill="none" stroke="white" stroke-width="2" stroke-linecap="round"/>"#
        }
        Icon::Plus => r#"<path d="M12 3h8v9h9v8h-9v9h-8v-9H3v-8h9Z"/>"#,
        _ => {
            r#"<path d="M3 6a3 3 0 0 1 3-3h7l4 5h9a3 3 0 0 1 3 3v15a3 3 0 0 1-3 3H6a3 3 0 0 1-3-3Z" opacity=".58"/><path d="M2 12a3 3 0 0 1 3-3h22a3 3 0 0 1 3 3v14a3 3 0 0 1-3 3H5a3 3 0 0 1-3-3Z"/>"#
        }
    };
    let r = (color.r * 255.0) as u8;
    let g = (color.g * 255.0) as u8;
    let b = (color.b * 255.0) as u8;
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="32" viewBox="0 0 32 32"><defs><linearGradient id="color" x2="0" y2="1"><stop stop-color="rgb({r},{g},{b})" stop-opacity=".7"/><stop offset="1" stop-color="rgb({r},{g},{b})"/></linearGradient></defs><g fill="url(#color)">{body}</g></svg>"##
    );
    iced::widget::svg(iced::widget::svg::Handle::from_memory(svg.into_bytes()))
        .width(size)
        .height(size)
        .into()
}
fn library_accent(name: &str, path: &std::path::Path) -> (Icon, Color, Color) {
    let key = path
        .to_string_lossy()
        .bytes()
        .fold(0usize, |h, b| h.wrapping_mul(31).wrapping_add(b as usize))
        % 5;
    let key = if name.contains("旅行") || name.contains("风景") {
        1
    } else if name.contains("插画") || name.contains("绘画") {
        2
    } else if name.contains("摄影") || name.contains("人像") {
        3
    } else if name.contains("灵感") || name.contains("设计") {
        4
    } else {
        key
    };
    match key {
        1 => (
            Icon::Landscape,
            Color::from_rgb8(67, 166, 109),
            Color::from_rgb8(225, 248, 236),
        ),
        2 => (
            Icon::Sparkles,
            Color::from_rgb8(231, 103, 145),
            Color::from_rgb8(255, 234, 243),
        ),
        3 => (
            Icon::Camera,
            Color::from_rgb8(142, 93, 220),
            Color::from_rgb8(244, 235, 255),
        ),
        4 => (
            Icon::Star,
            Color::from_rgb8(221, 165, 28),
            Color::from_rgb8(255, 247, 222),
        ),
        _ => (Icon::Folder, BLUE, Color::from_rgb8(230, 240, 255)),
    }
}
fn library_new_style(theme: &Theme, status: button::Status) -> button::Style {
    let card = library_card_style(theme);
    button::Style {
        background: card.background,
        border: card.border,
        shadow: card.shadow,
        ..library_open_style(theme, status)
    }
}
fn library_card_width(available: f32) -> f32 {
    let available = available.max(1.0);
    let columns = ((available + 20.0) / 320.0).floor().clamp(1.0, 3.0);
    ((available - (columns - 1.0) * 20.0) / columns).min(440.0)
}
fn library_card_style(theme: &Theme) -> container::Style {
    container::Style {
        border: Border {
            radius: 24.0.into(),
            color: if theme.extended_palette().is_dark {
                Color::from_rgb8(47, 58, 76)
            } else {
                Color::WHITE
            },
            width: 1.0,
        },
        shadow: iced::Shadow {
            color: Color::from_rgba8(25, 39, 65, 0.05),
            offset: iced::Vector::new(0.0, 5.0),
            blur_radius: 20.0,
        },
        background: Some(
            if theme.extended_palette().is_dark {
                Color::from_rgb8(33, 41, 55)
            } else {
                Color::WHITE
            }
            .into(),
        ),
        text_color: Some(adaptive(theme, TEXT)),
        ..Default::default()
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
    button::Style {
        background: Some(
            if matches!(status, button::Status::Hovered | button::Status::Pressed) {
                adaptive(theme, BG)
            } else {
                Color::TRANSPARENT
            }
            .into(),
        ),
        text_color: adaptive(theme, MUTED),
        border: Border {
            radius: 10.0.into(),
            ..Default::default()
        },
        ..Default::default()
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
            assert!(width <= 440.0);
            let columns = ((available + 20.0) / 320.0).floor().clamp(1.0, 3.0);
            assert!(width * columns + (columns - 1.0) * 20.0 <= available);
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
            assert_eq!(library_card_style(&theme).border.radius, 24.0.into());
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
