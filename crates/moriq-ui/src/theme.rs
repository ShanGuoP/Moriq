use iced::widget::{button, container, scrollable, slider, text_input};
use iced::{Background, Border, Color, Theme};
pub const BLUE: Color = Color::from_rgb8(36, 105, 230);
pub const TEXT: Color = Color::from_rgb8(31, 38, 50);
pub const MUTED: Color = Color::from_rgb8(107, 119, 138);
pub const ICON: Color = Color::from_rgb8(88, 101, 122);
pub const LINE: Color = Color::from_rgb8(229, 233, 239);
pub const BG: Color = Color::from_rgb8(247, 248, 250);
pub const PALE: Color = Color::from_rgb8(234, 241, 253);
pub const GREEN: Color = Color::from_rgb8(0, 190, 144);
pub const PURPLE: Color = Color::from_rgb8(126, 83, 255);
pub const CONTROL_LINE: Color = Color::from_rgb8(216, 222, 232);
pub fn application_theme() -> Theme {
    static THEME: std::sync::LazyLock<Theme> = std::sync::LazyLock::new(|| {
        Theme::custom(
            "Moriq Space",
            iced::theme::Palette {
                background: Color::WHITE,
                text: TEXT,
                primary: BLUE,
                success: GREEN,
                danger: Color::from_rgb8(214, 48, 48),
                ..iced::theme::Palette::LIGHT
            },
        )
    });
    THEME.clone()
}
pub fn panel(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Color::WHITE.into()),
        text_color: Some(TEXT),
        border: Border {
            color: LINE,
            width: 1.0,
            radius: 12.0.into(),
        },
        ..Default::default()
    }
}
pub fn sidebar(_: &Theme) -> container::Style {
    container::Style {
        background: Some(BG.into()),
        text_color: Some(TEXT),
        ..Default::default()
    }
}
pub fn modal(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Color::WHITE.into()),
        border: Border {
            color: LINE,
            width: 1.0,
            radius: 16.0.into(),
        },
        shadow: iced::Shadow {
            color: Color::from_rgba8(26, 34, 48, 0.13),
            offset: iced::Vector::new(0.0, 12.0),
            blur_radius: 40.0,
        },
        ..panel(theme)
    }
}
pub fn form_card_style(_: &Theme) -> container::Style {
    container::Style {
        background: Some(Color::from_rgb8(251, 252, 253).into()),
        text_color: Some(TEXT),
        border: Border {
            color: LINE,
            width: 1.0,
            radius: 12.0.into(),
        },
        ..Default::default()
    }
}
pub fn selected_row(theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: Some(PALE.into()),
        border: Border {
            color: BLUE,
            width: 1.0,
            radius: 10.0.into(),
        },
        ..secondary(theme, status)
    }
}
pub fn pale(_: &Theme) -> container::Style {
    container::Style {
        background: Some(PALE.into()),
        text_color: Some(BLUE),
        border: Border {
            radius: 7.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}
pub fn primary(_: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: Some(
            if matches!(status, button::Status::Disabled) {
                Color::from_rgb8(232, 236, 242)
            } else if matches!(status, button::Status::Hovered) {
                Color::from_rgb8(28, 91, 213)
            } else if matches!(status, button::Status::Pressed) {
                Color::from_rgb8(25, 77, 183)
            } else {
                BLUE
            }
            .into(),
        ),
        text_color: if matches!(status, button::Status::Disabled) {
            MUTED
        } else {
            Color::WHITE
        },
        border: Border {
            radius: 7.0.into(),
            ..Default::default()
        },
        shadow: if matches!(status, button::Status::Active | button::Status::Hovered) {
            iced::Shadow {
                color: Color::from_rgba8(
                    36,
                    105,
                    230,
                    if matches!(status, button::Status::Hovered) {
                        0.18
                    } else {
                        0.10
                    },
                ),
                offset: iced::Vector::new(0.0, 2.0),
                blur_radius: 5.0,
            }
        } else {
            iced::Shadow::default()
        },
        ..Default::default()
    }
}
pub fn secondary(_: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: Some(
            if matches!(status, button::Status::Hovered) {
                Color::from_rgb8(246, 248, 251)
            } else if matches!(status, button::Status::Pressed) {
                PALE
            } else {
                Color::WHITE
            }
            .into(),
        ),
        text_color: if matches!(status, button::Status::Disabled) {
            MUTED
        } else {
            TEXT
        },
        border: Border {
            color: if matches!(status, button::Status::Hovered) {
                Color::from_rgb8(190, 201, 218)
            } else {
                CONTROL_LINE
            },
            width: 1.0,
            radius: 7.0.into(),
        },
        ..Default::default()
    }
}
pub fn destructive(_: &Theme, status: button::Status) -> button::Style {
    let (background, text_color) = match status {
        button::Status::Disabled => (Color::from_rgb8(250, 228, 228), MUTED),
        button::Status::Hovered => (Color::from_rgb8(190, 38, 38), Color::WHITE),
        button::Status::Pressed => (Color::from_rgb8(160, 28, 28), Color::WHITE),
        _ => (Color::from_rgb8(214, 48, 48), Color::WHITE),
    };
    button::Style {
        background: Some(background.into()),
        text_color,
        border: Border {
            radius: 7.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}
pub fn flat(_: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: match status {
            button::Status::Hovered => Some(PALE.into()),
            button::Status::Pressed => Some(Color::from_rgb8(213, 228, 252).into()),
            _ => None,
        },
        text_color: if matches!(status, button::Status::Disabled) {
            MUTED
        } else {
            ICON
        },
        border: Border {
            radius: 6.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}
pub fn nav(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |_, status| button::Style {
        background: Some(
            if matches!(status, button::Status::Pressed) {
                Color::from_rgb8(213, 228, 252)
            } else if active {
                PALE
            } else if matches!(status, button::Status::Hovered) {
                Color::from_rgb8(237, 241, 247)
            } else {
                Color::TRANSPARENT
            }
            .into(),
        ),
        text_color: if active { BLUE } else { TEXT },
        border: Border {
            radius: 7.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}
pub fn input(_: &Theme, status: text_input::Status) -> text_input::Style {
    text_input::Style {
        background: if matches!(status, text_input::Status::Focused { .. }) {
            Color::WHITE.into()
        } else {
            Color::from_rgb8(251, 252, 254).into()
        },
        border: Border {
            radius: 7.0.into(),
            width: 1.0,
            color: if matches!(status, text_input::Status::Focused { .. }) {
                BLUE
            } else {
                CONTROL_LINE
            },
        },
        icon: MUTED,
        placeholder: MUTED,
        value: TEXT,
        selection: PALE,
    }
}
pub fn scroll_style(theme: &Theme, status: scrollable::Status) -> scrollable::Style {
    let mut style = scrollable::default(theme, status);
    style.vertical_rail.background = None;
    style.vertical_rail.scroller.background = Background::Color(
        if matches!(
            status,
            scrollable::Status::Hovered { .. } | scrollable::Status::Dragged { .. }
        ) {
            Color::from_rgba8(118, 131, 151, 0.75)
        } else {
            Color::from_rgba8(146, 157, 175, 0.42)
        },
    );
    style.vertical_rail.scroller.border.radius = 4.0.into();
    style
}
pub fn zoom(_: &Theme, _: slider::Status) -> slider::Style {
    slider::Style {
        rail: slider::Rail {
            backgrounds: (BLUE.into(), Color::from_rgb8(222, 232, 248).into()),
            width: 2.0,
            border: Border {
                radius: 2.0.into(),
                ..Default::default()
            },
        },
        handle: slider::Handle {
            shape: slider::HandleShape::Circle { radius: 4.0 },
            background: BLUE.into(),
            border_width: 0.0,
            border_color: BLUE,
        },
    }
}
pub fn progress(_: &Theme) -> iced::widget::progress_bar::Style {
    iced::widget::progress_bar::Style {
        background: Color::from_rgb8(227, 233, 242).into(),
        bar: BLUE.into(),
        border: Border {
            radius: 4.0.into(),
            ..Default::default()
        },
    }
}
