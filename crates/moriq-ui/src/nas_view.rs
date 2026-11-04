use super::*;
use crate::{
    components::*,
    icons::{Icon, icon},
    theme::*,
};
use iced::Length;
use iced::widget::{column, container, row, text_input};

impl App {
    /// 构建连接与来源卡片，并按当前连接、导入状态控制可用操作。
    pub(super) fn nas_panel(&self) -> Element<'_, Message> {
        let language = self.preferences.language;
        let ready = !self.manager.busy && !self.manager.nas_path.trim().is_empty();
        let connection = container(column![
            form_heading(Icon::Database,
                tr!(language, "连接你的照片来源" => "Connect your photo source"),
                tr!(language, "将 NAS 中的照片带进你的创作空间。" => "Bring photos from your NAS into your creative workspace.")),
            text_input(r"\\server\photos", &self.manager.nas_path)
                .on_input(|value| Message::Manage(ManageMessage::NasPath(value)))
                .padding(14).style(input),
            form_action(Icon::Plus,
                if self.manager.busy { tr!(language, "正在连接…" => "Connecting…") }
                else { tr!(language, "连接共享目录" => "Connect share") },
                Message::Manage(ManageMessage::NasConnect), true)
                .on_press_maybe(ready.then_some(Message::Manage(ManageMessage::NasConnect))),
            muted(tr!(language, "通过局域网或 VPN 访问，使用 Windows 已保存的登录凭据。" => "Access over LAN or VPN with your saved Windows credentials.")),
        ].spacing(16)).padding(22).width(Length::Fill).style(form_card_style);
        let mut body = column![
            connection,
            heading(tr!(language, "已保存的来源" => "Saved sources"), 16),
        ]
        .spacing(20);
        if self.manager.nas_sources.is_empty() {
            body = body.push(container(column![
                icon(Icon::Folder, 28.0, MUTED),
                heading(tr!(language, "从一个共享文件夹开始" => "Start with a shared folder"), 16),
                muted(tr!(language, "连接后，在这里选择远程浏览或下载到本地。" => "Once connected, choose remote browsing or a local download here.")),
            ].spacing(10)).padding(24).width(Length::Fill).style(panel));
        }
        for source in &self.manager.nas_sources {
            let available = !self.manager.busy && self.import.is_none();
            body = body.push(container(column![
                row![form_icon(Icon::Folder, false), column![heading(source.label.clone(), 15),
                    muted(tr!(language, "SMB · 已保存连接" => "SMB · Saved connection"))].spacing(5)].spacing(12),
                form_action(Icon::Image, tr!(language, "远程浏览并登记" => "Browse remotely"),
                    Message::Manage(ManageMessage::NasBrowse(source.id.clone(), true)), true)
                    .on_press_maybe(available.then(|| Message::Manage(ManageMessage::NasBrowse(source.id.clone(), true))))
                    .width(Length::Fill),
                muted(tr!(language, "原片保留在 NAS，本地保存缩略图与已查看的预览。" => "Keep originals on the NAS and cache previews locally.")),
                form_action(Icon::Download, tr!(language, "下载文件夹到本地" => "Download folder locally"),
                    Message::Manage(ManageMessage::NasBrowse(source.id.clone(), false)), false)
                    .on_press_maybe(available.then(|| Message::Manage(ManageMessage::NasBrowse(source.id.clone(), false))))
                    .width(Length::Fill),
                muted(tr!(language, "保存为托管原片，支持离线使用及资源库备份。" => "Store managed originals for offline use and library backups.")),
            ].spacing(12)).padding(22).width(Length::Fill).style(panel));
        }
        body.into()
    }
}
