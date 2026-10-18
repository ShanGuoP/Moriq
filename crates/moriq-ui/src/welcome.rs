use super::*;
use crate::components::*;
use crate::icons::{Icon, icon};
use crate::theme::*;
use iced::widget::{Space, button, column, container, row, text, text_input};
use iced::{Alignment, Length};
impl App {
    pub(super) fn welcome(&self) -> Element<'_, Message> {
        let mut create = button(text(if self.switching {
            self.tr("text_021")
        } else {
            self.tr("text_022")
        }))
        .padding([13, 24])
        .style(primary);
        let mut open = action(self.tr("text_023"), Message::Open);
        if !self.switching {
            create = create.on_press(Message::Create);
        } else {
            open = button(text(self.tr("text_023")))
                .padding(13)
                .style(secondary);
        }
        let mut recent = iced::widget::Column::new().spacing(6);
        for (name, path) in &self.preferences.recent {
            recent = recent.push(action(
                &format!("打开 {name}"),
                Message::OpenRecent(path.clone()),
            ));
        }
        container(
            column![
                container(icon(Icon::Logo, 88.0, BLUE))
                    .center_x(100)
                    .center_y(100),
                Space::new().height(12),
                heading(self.tr("text_024"), 32),
                muted(self.tr("text_025")),
                Space::new().height(12),
                container(
                    column![
                        heading(self.tr("text_026"), 18),
                        text_input(self.tr("text_016"), &self.name)
                            .on_input(Message::Name)
                            .padding(13)
                            .style(input),
                        muted(self.tr("text_027")),
                        row![create, open].spacing(10),
                    ]
                    .spacing(18)
                )
                .padding(28)
                .width(580)
                .style(panel),
                muted(self.tr("text_028")),
                recent,
            ]
            .spacing(16)
            .align_x(Alignment::Center),
        )
        .center_x(Length::Fill)
        .center_y(Length::Fill)
        .into()
    }
}
