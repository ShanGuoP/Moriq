use super::*;
use crate::icons::{Icon, icon};
use crate::theme::*;
use iced::widget::{Space, button, column, container, image, row, scrollable, svg, text};
use iced::{Alignment, Border, Color, ContentFit, Font, Length, font};
type Choice<'a, T, M = Message> = iced::widget::PickList<'a, T, Vec<T>, T, M>;

/// 构建统一样式的选择控件，并把选中值映射为消息。
pub fn choice<'a, T: ToString + PartialEq + Clone + 'a, M: Clone + 'a>(
    values: Vec<T>,
    selected: Option<T>,
    changed: impl Fn(T) -> M + 'a,
) -> Choice<'a, T, M> {
    iced::widget::pick_list(values, selected, changed)
        .text_size(13)
        .padding([8, 10])
        .menu_style(|theme| iced::widget::overlay::menu::Style {
            background: adaptive(theme, Color::WHITE).into(),
            text_color: adaptive(theme, TEXT),
            selected_text_color: adaptive(theme, BLUE),
            selected_background: adaptive(theme, PALE).into(),
            border: Border {
                color: adaptive(theme, LINE),
                width: 1.0,
                radius: 7.0.into(),
            },
            shadow: iced::Shadow::default(),
        })
        .style(|theme, status| iced::widget::pick_list::Style {
            text_color: adaptive(theme, TEXT),
            placeholder_color: adaptive(theme, MUTED),
            handle_color: adaptive(theme, ICON),
            background: adaptive(theme, Color::WHITE).into(),
            border: Border {
                color: if matches!(status, iced::widget::pick_list::Status::Active) {
                    adaptive(theme, CONTROL_LINE)
                } else {
                    adaptive(theme, BLUE)
                },
                width: 1.0,
                radius: 7.0.into(),
            },
        })
}
/// 生成使用统一字号和前景色的标题文本。
pub fn heading<'a>(value: impl Into<String>, size: u32) -> iced::widget::Text<'a> {
    text(value.into())
        .size(size)
        .style(text_color(TEXT))
        .font(Font {
            weight: font::Weight::Semibold,
            ..Font::DEFAULT
        })
}
/// 生成弱化颜色的辅助说明文本。
pub fn muted<'a>(value: impl Into<String>) -> iced::widget::Text<'a> {
    text(value.into()).size(12).style(text_color(MUTED))
}
/// 构建带统一外观和反馈的常规操作按钮。
pub fn action<'a, M: Clone + 'a>(label: &str, message: M) -> iced::widget::Button<'a, M> {
    button(text(label.to_owned()).size(13))
        .padding([8, 12])
        .style(secondary)
        .on_press(message)
}
/// 构建表单底部操作按钮，保持提交与取消的样式一致。
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
/// 构建表单中的图标元素。
pub fn form_icon<'a>(kind: Icon, large: bool) -> Element<'a, Message> {
    let side = if large { 52 } else { 42 };
    container(icon(kind, if large { 26.0 } else { 22.0 }, BLUE))
        .center_x(side)
        .center_y(side)
        .style(|theme| container::Style {
            background: Some(adaptive(theme, PALE).into()),
            border: Border {
                radius: 12.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into()
}
/// 构建表单标题与辅助信息区域。
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
/// 把内容包装为统一边距和背景的表单卡片。
pub fn form_card<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content)
        .padding(18)
        .width(Length::Fill)
        .style(theme::form_card_style)
        .into()
}
/// 包装详情区域中的一组相关字段。
pub fn detail_card<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    container(content)
        .padding(12)
        .width(Length::Fill)
        .style(theme::form_card_style)
        .into()
}
/// 构建详情分组标题。
pub fn detail_heading<'a>(kind: Icon, title: &str) -> Element<'a, Message> {
    row![icon(kind, 17.0, BLUE), heading(title, 14)]
        .spacing(8)
        .align_y(Alignment::Center)
        .into()
}
/// 构建带提示和消息的图标按钮，保留有效点击范围。
pub fn icon_button<'a>(kind: Icon, message: Message) -> iced::widget::Button<'a, Message> {
    button(icon(kind, 18.0, ICON))
        .padding(9)
        .style(flat)
        .on_press(message)
}
/// 把图片句柄包装为指定尺寸与裁剪方式的图片控件。
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
/// 构建紧凑的标签或状态标记。
pub fn chip<'a>(label: impl Into<String>, color: Color) -> Element<'a, Message> {
    container(text(label.into()).size(11).style(text_color(color)))
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
/// 构建带标题的内容分组。
pub fn section<'a>(language: Language, title: &str, message: Message) -> Element<'a, Message> {
    row![
        heading(title, 17),
        Space::new().width(Length::Fill),
        button(
            row![
                text(tr!(language, "查看全部" => "View all"))
                    .size(12)
                    .style(text_color(BLUE)),
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
/// 构建统一颜色和尺寸的分隔线。
pub fn separator<'a>() -> Element<'a, Message> {
    container(Space::new().height(1))
        .width(Length::Fill)
        .style(|theme| container::Style {
            background: Some(adaptive(theme, LINE).into()),
            ..Default::default()
        })
        .into()
}
/// 包装统一滚动样式的内容区域。
pub fn scroll<'a>(
    content: impl Into<Element<'a, Message>>,
) -> iced::widget::Scrollable<'a, Message> {
    scrollable(content)
        .direction(scrollable::Direction::Vertical(
            scrollable::Scrollbar::new()
                .width(16)
                .scroller_width(14)
                .spacing(6),
        ))
        .style(theme::scroll_style)
}
/// 构建元数据名称和值的展示行。
pub fn metadata<'a>(kind: Icon, label: &str, value: impl Into<String>) -> Element<'a, Message> {
    row![
        icon(kind, 14.0, MUTED),
        muted(label).width(74),
        text(value.into())
            .width(Length::Fill)
            .wrapping(text::Wrapping::Glyph)
            .size(12)
            .style(text_color(ICON))
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}
/// 将字节数量转换为便于阅读的容量文本。
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
/// 把登记时间转换为界面显示文本。
pub fn date(timestamp: u64) -> String {
    chrono::DateTime::from_timestamp_millis(timestamp as i64)
        .map(|time| {
            time.with_timezone(&chrono::Local)
                .format("%Y.%m.%d %H:%M")
                .to_string()
        })
        .unwrap_or_else(|| "—".into())
}
/// 将本机路径转换为可显示的文本，处理平台路径前缀。
pub fn display_path(path: &std::path::Path) -> String {
    path.to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_string()
}
/// 绘制容量或分类占比的环形图。
pub fn donut<'a>(value: String, caption: &str, filled: f32) -> Element<'a, Message> {
    ring(value, caption, filled, 140.0)
}
/// 构建指定直径、填充比例和中心说明的环形统计控件。
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

/// 为数值添加分组间隔，便于阅读较大计数。
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

#[derive(Clone)]
pub struct LocalizedOption<T> {
    value: T,
    label: String,
}
impl<T: PartialEq> PartialEq for LocalizedOption<T> {
    /// 按选择项的稳定值比较相等性，不依赖翻译后的标签。
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}
impl<T> std::fmt::Display for LocalizedOption<T> {
    /// 输出选择项的可读标签，供选择控件展示。
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}
/// 按当前语言构建选择项显示值，保留内部稳定身份。
pub fn localized_choice<'a, T: ToString + PartialEq + Clone + 'a, M: Clone + 'a>(
    language: Language,
    values: Vec<T>,
    selected: Option<T>,
    changed: impl Fn(T) -> M + 'a,
) -> Choice<'a, LocalizedOption<T>, M> {
    choice_labels(
        values,
        selected,
        move |value| crate::i18n::option_label(language, &value.to_string()).to_owned(),
        changed,
    )
}
/// 为一组固定选择值生成本地化标签。
pub fn choice_labels<'a, T: PartialEq + Clone + 'a, M: Clone + 'a>(
    values: Vec<T>,
    selected: Option<T>,
    label: impl Fn(&T) -> String,
    changed: impl Fn(T) -> M + 'a,
) -> Choice<'a, LocalizedOption<T>, M> {
    let localize = |value: T| LocalizedOption {
        label: label(&value),
        value,
    };
    choice(
        values.into_iter().map(&localize).collect(),
        selected.map(localize),
        move |option| changed(option.value),
    )
}
