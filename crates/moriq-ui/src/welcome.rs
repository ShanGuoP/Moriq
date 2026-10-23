use super::*;
use crate::components::*;
use crate::icons::{Icon, icon};
use crate::theme::*;
use iced::widget::{Row, Space, button, column, container, row, text, text_input, tooltip};
use iced::{Alignment, Length};
impl App {
    pub(super) fn welcome(&self) -> Element<'_, Message> {
        let language = self.preferences.language;
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
                );
            }
            return scroll(
                container(
                    column![
                        row![
                            column![
                                heading(tr!(language, "我的资源库" => "My libraries"), 28),
                                muted(trf!(language, "{} 个资源库 · 选择一个，继续整理灵感" => "{} libraries · Choose one to continue organizing inspiration",
                                    self.preferences.recent.len()
                                ))
                            ]
                            .spacing(9),
                            Space::new().width(Length::Fill),
                            action(tr!(language, "打开已有资源库" => "Open an existing library"), Message::Open)
                                .on_press_maybe(can_open.then_some(Message::Open)),
                            action(tr!(language, "新建资源库" => "New library"), Message::ShowCreate)
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
pub(super) fn compact(value: &str, limit: usize) -> String {
    if value.chars().count() <= limit {
        return value.to_owned();
    }
    format!("{}…", value.chars().take(limit - 1).collect::<String>())
}
