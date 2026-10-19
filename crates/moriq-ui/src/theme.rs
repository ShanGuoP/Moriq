use iced::widget::{button, container, scrollable, slider, text_input};
use iced::{Background, Border, Color, Theme};
pub const BLUE: Color = Color::from_rgb8(16, 111, 255);
pub const TEXT: Color = Color::from_rgb8(24, 32, 52);
pub const MUTED: Color = Color::from_rgb8(130, 143, 169);
pub const ICON: Color = Color::from_rgb8(81, 95, 126);
pub const LINE: Color = Color::from_rgb8(233, 238, 246);
pub const BG: Color = Color::from_rgb8(248, 250, 253);
pub const PALE: Color = Color::from_rgb8(231, 240, 255);
pub const GREEN: Color = Color::from_rgb8(0, 190, 144);
pub const PURPLE: Color = Color::from_rgb8(126, 83, 255);
pub fn application_theme() -> Theme {
    static THEME: std::sync::LazyLock<Theme> = std::sync::LazyLock::new(|| {
        Theme::custom(
            "MORIQ",
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
        background: Some(Color::from_rgb8(254, 254, 255).into()),
        text_color: Some(TEXT),
        border: Border {
            color: LINE,
            width: 1.0,
            radius: 10.0.into(),
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
        shadow: iced::Shadow {
            color: Color::from_rgba8(24, 42, 72, 0.10),
            offset: iced::Vector::new(0.0, 8.0),
            blur_radius: 28.0,
        },
        ..panel(theme)
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
                Color::from_rgb8(224, 235, 252)
            } else if matches!(status, button::Status::Hovered) {
                Color::from_rgb8(0, 95, 237)
            } else if matches!(status, button::Status::Pressed) {
                Color::from_rgb8(0, 78, 206)
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
        ..Default::default()
    }
}
pub fn secondary(_: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: Some(
            if matches!(status, button::Status::Hovered) {
                BG
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
            color: Color::from_rgb8(224, 231, 243),
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
                Color::from_rgb8(239, 244, 251)
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
        background: Color::from_rgb8(253, 254, 255).into(),
        border: Border {
            radius: 7.0.into(),
            width: 1.0,
            color: if matches!(status, text_input::Status::Focused { .. }) {
                BLUE
            } else {
                Color::from_rgb8(224, 231, 244)
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
    style.vertical_rail.background = Some(Color::from_rgb8(239, 242, 247).into());
    style.vertical_rail.scroller.background = Background::Color(Color::from_rgb8(151, 164, 185));
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
