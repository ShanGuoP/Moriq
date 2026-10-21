use super::*;
use crate::icons::{Icon, icon};
use crate::theme::*;
use iced::widget::{Space, button, column, container, image, row, scrollable, svg, text};
use iced::{Alignment, Border, Color, ContentFit, Font, Length, font};
pub fn choice<'a, T: ToString + PartialEq + Clone + 'a>(
    values: Vec<T>,
    selected: Option<T>,
    changed: impl Fn(T) -> Message + 'a,
) -> iced::widget::PickList<'a, T, Vec<T>, T, Message> {
    iced::widget::pick_list(values, selected, changed)
        .text_size(13)
        .padding([8, 10])
        .menu_style(|_| iced::widget::overlay::menu::Style {
            background: Color::WHITE.into(),
            text_color: TEXT,
            selected_text_color: BLUE,
            selected_background: PALE.into(),
            border: Border {
                color: LINE,
                width: 1.0,
                radius: 7.0.into(),
            },
            shadow: iced::Shadow::default(),
        })
        .style(|_, status| iced::widget::pick_list::Style {
            text_color: TEXT,
            placeholder_color: MUTED,
            handle_color: ICON,
            background: Color::WHITE.into(),
            border: Border {
                color: if matches!(status, iced::widget::pick_list::Status::Active) {
                    CONTROL_LINE
                } else {
                    BLUE
                },
                width: 1.0,
                radius: 7.0.into(),
            },
        })
}
pub fn heading<'a>(value: impl Into<String>, size: u32) -> iced::widget::Text<'a> {
    text(value.into()).size(size).color(TEXT).font(Font {
        weight: font::Weight::Semibold,
        ..Font::DEFAULT
    })
}
pub fn muted<'a>(value: impl Into<String>) -> iced::widget::Text<'a> {
    text(value.into()).size(12).color(MUTED)
}
pub fn action<'a>(label: &str, message: Message) -> iced::widget::Button<'a, Message> {
    button(text(label.to_owned()).size(13))
        .padding([8, 12])
        .style(secondary)
        .on_press(message)
}
pub fn form_action<'a>(
    kind: Icon,
    label: &str,
    message: Message,
    prominent: bool,
) -> iced::widget::Button<'a, Message> {
    let color = if prominent { Color::WHITE } else { BLUE };
    button(
        row![icon(kind, 18.0, color), text(label.to_owned()).size(14)]
            .spacing(10)
            .align_y(Alignment::Center),
    )
    .padding([11, 16])
    .style(if prominent { primary } else { secondary })
    .on_press(message)
}
pub fn form_icon<'a>(kind: Icon, large: bool) -> Element<'a, Message> {
    let side = if large { 52 } else { 42 };
    container(icon(kind, if large { 26.0 } else { 22.0 }, BLUE))
        .center_x(side)
        .center_y(side)
        .style(|_| container::Style {
            background: Some(PALE.into()),
            border: Border {
                radius: 12.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}
pub fn form_heading<'a>(kind: Icon, title: &str, description: &str) -> Element<'a, Message> {
    row![
        form_icon(kind, false),
        column![heading(title, 16), muted(description)]
            .spacing(6)
            .width(Length::Fill)
    ]
    .spacing(16)
    .align_y(Alignment::Center)
    .into()
}
pub fn form_card<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content)
        .padding(18)
        .width(Length::Fill)
        .style(theme::form_card_style)
        .into()
}
pub fn detail_card<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content)
        .padding(12)
        .width(Length::Fill)
        .style(theme::form_card_style)
        .into()
}
pub fn detail_heading<'a>(kind: Icon, title: &str) -> Element<'a, Message> {
    row![icon(kind, 17.0, BLUE), heading(title, 14)]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
}
pub fn icon_button<'a>(kind: Icon, message: Message) -> iced::widget::Button<'a, Message> {
    button(icon(kind, 18.0, ICON))
        .padding(9)
        .style(flat)
        .on_press(message)
}
pub fn photo<'a>(
    handle: Option<&image::Handle>,
    width: Length,
    height: f32,
) -> Element<'a, Message> {
    if let Some(handle) = handle {
        image(handle.clone())
            .width(width)
            .height(height)
            .content_fit(ContentFit::Cover)
            .border_radius(7)
            .into()
    } else {
        container(icon(Icon::Image, 28.0, MUTED))
            .center_x(width)
            .center_y(height)
            .style(pale)
            .into()
    }
}
pub fn chip<'a>(label: impl Into<String>, color: Color) -> Element<'a, Message> {
    container(text(label.into()).size(11).color(color))
        .padding([3, 9])
        .style(move |_| container::Style {
            background: Some(Color { a: 0.08, ..color }.into()),
            border: Border {
                radius: 10.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}
pub fn section<'a>(title: &str, message: Message) -> Element<'a, Message> {
    row![
        heading(title, 17),
        Space::new().width(Length::Fill),
        button(
            row![
                text("查看全部").size(12).color(BLUE),
                icon(Icon::Right, 13.0, BLUE)
            ]
            .spacing(3)
            .align_y(Alignment::Center)
        )
        .padding(0)
        .style(flat)
        .on_press(message)
    ]
    .align_y(Alignment::Center)
    .into()
}
pub fn separator<'a>() -> Element<'a, Message> {
    container(Space::new().height(1))
        .width(Length::Fill)
        .style(|_| container::Style {
            background: Some(LINE.into()),
            ..Default::default()
        })
        .into()
}
pub fn scroll<'a>(
    content: impl Into<Element<'a, Message>>,
) -> iced::widget::Scrollable<'a, Message> {
    scrollable(content)
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::new()
                .width(10)
                .scroller_width(10)
                .spacing(6),
        ))
        .style(theme::scroll_style)
}
pub fn metadata<'a>(kind: Icon, label: &str, value: impl Into<String>) -> Element<'a, Message> {
    row![
        icon(kind, 14.0, MUTED),
        muted(label).width(74),
        text(value.into()).size(12).color(ICON)
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}
pub fn size(bytes: u64) -> String {
    if bytes >= 1_000_000_000 {
        format!("{:.1} GB", bytes as f64 / 1e9)
    } else if bytes >= 1_000_000 {
        format!("{:.1} MB", bytes as f64 / 1e6)
    } else if bytes >= 1000 {
        format!("{:.1} KB", bytes as f64 / 1000.0)
    } else {
        format!("{bytes} B")
    }
}
pub fn date(timestamp: u64) -> String {
    chrono::DateTime::from_timestamp_millis(timestamp as i64)
        .map(|time| {
            time.with_timezone(&chrono::Local)
                .format("%Y.%m.%d %H:%M")
                .to_string()
        })
        .unwrap_or_else(|| "—".into())
}
pub fn display_path(path: &std::path::Path) -> String {
    path.to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_string()
}
pub fn donut<'a>(value: String, caption: &str, filled: f32) -> Element<'a, Message> {
    ring(value, caption, filled, 140.0)
}
pub fn ring<'a>(value: String, caption: &str, filled: f32, diameter: f32) -> Element<'a, Message> {
    let mut label = column![heading(value, if diameter < 100.0 { 13 } else { 18 })]
        .spacing(3)
        .align_x(Alignment::Center);
    if !caption.is_empty() {
        label = label.push(muted(caption));
    }
    let circumference = 2.0 * std::f32::consts::PI * 54.0;
    let dash = circumference * filled.clamp(0.0, 1.0);
    let markup = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 140 140"><circle cx="70" cy="70" r="54" fill="none" stroke="#e4eaff" stroke-width="15"/><circle cx="70" cy="70" r="54" fill="none" stroke="#8866ff" stroke-width="15" stroke-dasharray="{} {}" transform="rotate(-90 70 70)"/><circle cx="70" cy="70" r="54" fill="none" stroke="#137bff" stroke-width="15" stroke-dasharray="{dash} {circumference}" transform="rotate(-90 70 70)"/></svg>"##,
        circumference * 0.89,
        circumference
    );
    iced::widget::stack![
        svg(svg::Handle::from_memory(markup.into_bytes()))
            .width(diameter)
            .height(diameter),
        container(label).center_x(diameter).center_y(diameter)
    ]
    .into()
}

pub(super) fn grouped_number(value: u64) -> String {
    let s = value.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}
