use super::*;
use crate::components::*;
use crate::icons::{Icon, icon};
use crate::theme::*;
use iced::widget::{Row, Space, button, column, container, row, text, text_input, tooltip};
use iced::{Alignment, Length};
impl App {
    pub(super) fn welcome(&self) -> Element<'_, Message> {
        if !self.preferences.recent.is_empty() {
            let can_open = !self.switching && self.import.is_none() && !self.manager.busy;
            let mut cards = Row::new().spacing(20);
            for (name, path) in &self.preferences.recent {
                let cover = photo(self.library_covers.get(path), Length::Fill, 150.0);
                let location = display_path(path.parent().unwrap_or(path));
                let short_location = compact(&location, 18);
                let card = button(
                    column![
                        cover,
                        container(
                            column![
                                heading(compact(name, 13), 17)
                                    .width(Length::Fill)
                                    .height(24)
                                    .wrapping(text::Wrapping::None),
                                muted(short_location)
                                    .width(Length::Fill)
                                    .height(18)
                                    .wrapping(text::Wrapping::None),
                            ]
                            .spacing(6)
                            .width(Length::Fill)
                        )
                        .padding([4, 2])
                        .width(Length::Fill)
                        .clip(true),
                    ]
                    .spacing(12),
                )
                .padding(12)
                .width(252)
                .style(secondary)
                .on_press_maybe(can_open.then(|| Message::OpenRecent(path.clone())));
                cards = cards.push(
                    tooltip(
                        card,
                        container(
                            column![
                                heading(name.clone(), 13),
                                text(display_path(path))
                                    .size(12)
                                    .color(ICON)
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
                );
            }
            return scroll(
                container(
                    column![
                        row![
                            column![
                                heading("我的资源库", 28),
                                muted(format!(
                                    "{} 个资源库 · 选择一个，继续整理灵感",
                                    self.preferences.recent.len()
                                ))
                            ]
                            .spacing(9),
                            Space::new().width(Length::Fill),
                            action("打开已有资源库", Message::Open)
                                .on_press_maybe(can_open.then_some(Message::Open)),
                            action("新建资源库", Message::ShowCreate)
                                .style(primary)
                                .on_press_maybe(can_open.then_some(Message::ShowCreate)),
                        ]
                        .spacing(12)
                        .align_y(Alignment::Center),
                        separator(),
                        cards.wrap(),
                    ]
                    .spacing(26),
                )
                .padding(32)
                .width(Length::Fill),
            )
            .height(Length::Fill)
            .into();
        }
        let mut create = button(text(if self.switching {
            "正在打开…"
        } else {
            "选择位置并创建"
        }))
        .padding([13, 24])
        .style(primary);
        let mut open = action("打开已有资源库", Message::Open);
        if !self.switching {
            create = create.on_press(Message::Create);
        } else {
            open = button(text("打开已有资源库")).padding(13).style(secondary);
        }
        container(
            column![
                container(icon(Icon::Logo, 88.0, BLUE))
                    .center_x(100)
                    .center_y(100),
                Space::new().height(12),
                heading("每一份灵感，都有归处。", 32),
                muted("欢迎使用 Moriq Space。创建一个资源库，开始整理你的照片与创意。"),
                Space::new().height(12),
                container(
                    column![
                        heading("创建你的第一个资源库", 18),
                        text_input("资源库名称", &self.name)
                            .on_input(Message::Name)
                            .padding(13)
                            .style(input),
                        muted("选择保存位置后，原片、标签和项目将一起保存在 .moriq 文件夹中。"),
                        row![create, open].spacing(10),
                    ]
                    .spacing(18)
                )
                .padding(28)
                .width(580)
                .style(panel),
                muted("本地存储  ·  原片不覆盖  ·  AI 模型按需安装"),
            ]
            .spacing(16)
            .align_x(Alignment::Center),
        )
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
    }
}
pub(super) fn compact(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value.to_owned();
    }
    format!("{}…", value.chars().take(limit - 1).collect::<String>())
}
