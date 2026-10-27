use iced::widget::{button, container, scrollable, slider, text_editor, text_input};
use iced::{Background, Border, Color, Theme};
use moriq_domain::Appearance;
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
/// 根据持久化外观偏好选择应用主题。
pub fn application_theme(appearance: Appearance) -> Theme {
    if appearance == Appearance::Dark {
        static DARK: std::sync::LazyLock<Theme> = std::sync::LazyLock::new(|| {
            Theme::custom(
                "Moriq Space Dark",
                iced::theme::Palette {
                    background: Color::from_rgb8(27, 34, 45),
                    text: Color::from_rgb8(230, 234, 240),
                    primary: Color::from_rgb8(116, 166, 255),
                    success: Color::from_rgb8(57, 216, 175),
                    danger: Color::from_rgb8(255, 133, 139),
                    ..iced::theme::Palette::DARK
                },
            )
        });
        return DARK.clone();
    }
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
/// Resolve the existing semantic palette through the theme supplied by Iced.
///
/// 按当前明暗主题选择相应颜色值。
pub fn adaptive(theme: &Theme, color: Color) -> Color {
    if !theme.extended_palette().is_dark {
        return color;
    }
    let resolved = if color == TEXT {
        theme.palette().text
    } else if color == MUTED {
        Color::from_rgb8(157, 169, 187)
    } else if color == ICON {
        Color::from_rgb8(186, 197, 214)
    } else if color == LINE {
        Color::from_rgb8(48, 56, 69)
    } else if color == CONTROL_LINE {
        Color::from_rgb8(65, 76, 93)
    } else if color == BG {
        Color::from_rgb8(19, 24, 33)
    } else if color == PALE {
        Color::from_rgb8(29, 49, 82)
    } else if color == BLUE {
        theme.palette().primary
    } else if color == PURPLE {
        Color::from_rgb8(178, 153, 255)
    } else if color == GREEN {
        theme.palette().success
    } else if color.r > 0.97 && color.g > 0.97 && color.b > 0.97 {
        theme.palette().background
    } else if color.r > 0.8 && color.g > 0.8 && color.b > 0.8 {
        Color::from_rgb8(40, 52, 71)
    } else if color.r > 0.65 && color.g < 0.25 && color.b < 0.25 {
        theme.palette().danger
    } else {
        color
    };
    Color {
        a: color.a,
        ..resolved
    }
}
/// 返回当前主题的主要前景色。
pub fn foreground(theme: &Theme, color: Color) -> Color {
    // White text and icons over photographs and primary buttons stay white.
    if color == Color::WHITE {
        color
    } else {
        adaptive(theme, color)
    }
}
/// 为指定语义选择当前主题的文字颜色。
pub fn text_color(color: Color) -> impl Fn(&Theme) -> iced::widget::text::Style {
    move |theme| iced::widget::text::Style {
        color: Some(foreground(theme, color)),
    }
}
/// 生成常规内容面板背景与边框样式。
pub fn panel(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(adaptive(theme, Color::WHITE).into()),
        text_color: Some(adaptive(theme, TEXT)),
        border: Border {
            color: adaptive(theme, LINE),
            width: 1.0,
            radius: 12.0.into(),
        },
        ..Default::default()
    }
}
/// 生成侧栏容器样式。
pub fn sidebar(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(adaptive(theme, BG).into()),
        text_color: Some(adaptive(theme, TEXT)),
        ..Default::default()
    }
}
/// 生成对话框容器样式。
pub fn modal(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(adaptive(theme, Color::WHITE).into()),
        border: Border {
            color: adaptive(theme, LINE),
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
/// 生成表单卡片的背景、边框和圆角样式。
pub fn form_card_style(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(adaptive(theme, Color::from_rgb8(251, 252, 253)).into()),
        text_color: Some(adaptive(theme, TEXT)),
        border: Border {
            color: adaptive(theme, LINE),
            width: 1.0,
            radius: 12.0.into(),
        },
        ..Default::default()
    }
}
/// 生成当前选中行的视觉样式。
pub fn selected_row(theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: Some(adaptive(theme, PALE).into()),
        border: Border {
            color: adaptive(theme, BLUE),
            width: 1.0,
            radius: 10.0.into(),
        },
        ..secondary(theme, status)
    }
}
/// 生成弱化背景的按钮样式。
pub fn pale(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(adaptive(theme, PALE).into()),
        text_color: Some(adaptive(theme, BLUE)),
        border: Border {
            radius: 7.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}
/// 根据交互状态生成主操作按钮样式。
pub fn primary(theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: Some(
            if matches!(status, button::Status::Disabled) {
                adaptive(theme, Color::from_rgb8(232, 236, 242))
            } else if matches!(status, button::Status::Hovered) {
                adaptive(theme, Color::from_rgb8(28, 91, 213))
            } else if matches!(status, button::Status::Pressed) {
                adaptive(theme, Color::from_rgb8(25, 77, 183))
            } else {
                BLUE
            }
            .into(),
        ),
        text_color: if matches!(status, button::Status::Disabled) {
            adaptive(theme, MUTED)
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
/// 根据交互状态生成次操作按钮样式。
pub fn secondary(theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: Some(
            if matches!(status, button::Status::Hovered) {
                adaptive(theme, Color::from_rgb8(246, 248, 251))
            } else if matches!(status, button::Status::Pressed) {
                adaptive(theme, PALE)
            } else {
                adaptive(theme, Color::WHITE)
            }
            .into(),
        ),
        text_color: if matches!(status, button::Status::Disabled) {
            adaptive(theme, MUTED)
        } else {
            adaptive(theme, TEXT)
        },
        border: Border {
            color: if matches!(status, button::Status::Hovered) {
                adaptive(theme, Color::from_rgb8(190, 201, 218))
            } else {
                adaptive(theme, CONTROL_LINE)
            },
            width: 1.0,
            radius: 7.0.into(),
        },
        ..Default::default()
    }
}
/// 根据交互状态生成删除等破坏性操作按钮样式。
pub fn destructive(theme: &Theme, status: button::Status) -> button::Style {
    let (background, text_color) = match status {
        button::Status::Disabled => (
            adaptive(theme, Color::from_rgb8(250, 228, 228)),
            adaptive(theme, MUTED),
        ),
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
/// 生成无突出背景的轻量按钮样式。
pub fn flat(theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: match status {
            button::Status::Hovered => Some(adaptive(theme, PALE).into()),
            button::Status::Pressed => {
                Some(adaptive(theme, Color::from_rgb8(213, 228, 252)).into())
            }
            _ => None,
        },
        text_color: if matches!(status, button::Status::Disabled) {
            adaptive(theme, MUTED)
        } else {
            adaptive(theme, ICON)
        },
        border: Border {
            radius: 6.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}
/// Quiet, consistently sized controls for library management and sidebar actions.
///
/// 生成资源库卡片操作按钮样式。
pub fn library_action(theme: &Theme, status: button::Status) -> button::Style {
    let mut style = flat(theme, status);
    style.border.radius = 9.0.into();
    style.background = Some(
        adaptive(
            theme,
            match status {
                button::Status::Hovered => Color::from_rgb8(233, 239, 248),
                button::Status::Pressed => Color::from_rgb8(222, 232, 248),
                _ => Color::from_rgb8(246, 248, 251),
            },
        )
        .into(),
    );
    style
}
/// 生成资源库卡片辅助入口样式。
pub fn library_hint(theme: &Theme) -> container::Style {
    container::Style {
        border: Border {
            radius: 8.0.into(),
            color: adaptive(theme, LINE),
            width: 1.0,
        },
        shadow: iced::Shadow {
            color: Color::from_rgba8(25, 39, 65, 0.06),
            offset: iced::Vector::new(0.0, 3.0),
            blur_radius: 10.0,
        },
        ..panel(theme)
    }
}
/// 生成资源库列表入口的交互样式。
pub fn library_entry(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let mut style = nav(active)(theme, status);
        style.border = Border {
            radius: 11.0.into(),
            color: if active {
                adaptive(theme, BLUE).scale_alpha(0.15)
            } else {
                Color::TRANSPARENT
            },
            width: 1.0,
        };
        style
    }
}
/// 生成普通导航项的交互样式。
pub fn nav(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| button::Style {
        background: Some(
            if matches!(status, button::Status::Pressed) {
                adaptive(theme, Color::from_rgb8(213, 228, 252))
            } else if active {
                adaptive(theme, PALE)
            } else if matches!(status, button::Status::Hovered) {
                adaptive(theme, Color::from_rgb8(237, 241, 247))
            } else {
                Color::TRANSPARENT
            }
            .into(),
        ),
        text_color: if active {
            adaptive(theme, BLUE)
        } else {
            adaptive(theme, TEXT)
        },
        border: Border {
            radius: 7.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}
/// Selection geometry belongs to the liquid group; the real button keeps input and focus.
///
/// 生成与连续选中指示配合的导航项样式。
pub fn liquid_nav(active: bool) -> impl Fn(&Theme, button::Status) -> button::Style {
    move |theme, status| {
        let mut style = nav(active)(theme, status);
        style.background = match status {
            button::Status::Hovered if !active => {
                Some(adaptive(theme, PALE).scale_alpha(0.45).into())
            }
            button::Status::Pressed => Some(adaptive(theme, BLUE).scale_alpha(0.08).into()),
            _ => None,
        };
        style
    }
}
/// 根据焦点和交互状态生成单行输入框样式。
pub fn input(theme: &Theme, status: text_input::Status) -> text_input::Style {
    text_input::Style {
        background: if matches!(status, text_input::Status::Focused { .. }) {
            adaptive(theme, Color::WHITE).into()
        } else {
            adaptive(theme, Color::from_rgb8(251, 252, 254)).into()
        },
        border: Border {
            radius: 7.0.into(),
            width: 1.0,
            color: if matches!(status, text_input::Status::Focused { .. }) {
                adaptive(theme, BLUE)
            } else {
                adaptive(theme, CONTROL_LINE)
            },
        },
        icon: adaptive(theme, MUTED),
        placeholder: adaptive(theme, MUTED),
        value: adaptive(theme, TEXT),
        selection: adaptive(theme, PALE),
    }
}
/// 根据交互状态生成多行编辑器样式。
pub fn editor(theme: &Theme, status: text_editor::Status) -> text_editor::Style {
    let field = input(
        theme,
        if matches!(status, text_editor::Status::Focused { .. }) {
            text_input::Status::Focused { is_hovered: false }
        } else {
            text_input::Status::Active
        },
    );
    text_editor::Style {
        background: field.background,
        border: field.border,
        placeholder: field.placeholder,
        value: field.value,
        selection: field.selection,
    }
}
/// 生成滚动轨道和滑块样式。
pub fn scroll_style(theme: &Theme, status: scrollable::Status) -> scrollable::Style {
    let mut style = scrollable::default(theme, status);
    style.vertical_rail.background = Some(Background::Color(adaptive(
        theme,
        Color::from_rgba8(146, 157, 175, 0.10),
    )));
    style.vertical_rail.scroller.background = Background::Color(
        if matches!(
            status,
            scrollable::Status::Hovered { .. } | scrollable::Status::Dragged { .. }
        ) {
            Color::from_rgba8(118, 131, 151, 0.75)
        } else {
            Color::from_rgba8(146, 157, 175, 0.65)
        },
    );
    style.vertical_rail.scroller.border.radius = 4.0.into();
    style
}
/// 生成缩放控件样式。
pub fn zoom(theme: &Theme, _: slider::Status) -> slider::Style {
    slider::Style {
        rail: slider::Rail {
            backgrounds: (
                adaptive(theme, BLUE).into(),
                adaptive(theme, Color::from_rgb8(222, 232, 248)).into(),
            ),
            width: 2.0,
            border: Border {
                radius: 2.0.into(),
                ..Default::default()
            },
        },
        handle: slider::Handle {
            shape: slider::HandleShape::Circle { radius: 4.0 },
            background: adaptive(theme, BLUE).into(),
            border_width: 0.0,
            border_color: adaptive(theme, BLUE),
        },
    }
}
/// 生成任务进度条样式。
pub fn progress(theme: &Theme) -> iced::widget::progress_bar::Style {
    iced::widget::progress_bar::Style {
        background: adaptive(theme, Color::from_rgb8(227, 233, 242)).into(),
        bar: adaptive(theme, BLUE).into(),
        border: Border {
            radius: 4.0.into(),
            ..Default::default()
        },
    }
}
