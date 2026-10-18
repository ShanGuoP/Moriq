use super::*;
use crate::{
    components::*,
    icons::{Icon, icon},
    manage::Panel,
    theme::*,
};
use iced::widget::{Space, button, column, container, mouse_area, row, stack, text};
impl App {
    pub(super) fn compact_panel(&self) -> bool {
        matches!(
            self.manager.panel,
            Some(Panel::AssetMenu | Panel::Menu | Panel::ViewOptions | Panel::Import)
        )
    }
    pub(super) fn popup<'a>(&'a self, base: Element<'a, Message>) -> Element<'a, Message> {
        use ManageMessage::*;
        let mut content = column![].spacing(2);
        let title = if self.manager.panel == Some(Panel::AssetMenu) {
            self.selected
                .as_ref()
                .map(|a| a.name.as_str())
                .unwrap_or("素材操作")
        } else {
            match self.manager.panel {
                Some(Panel::Menu) => "资源库工具",
                Some(Panel::Import) => "导入",
                _ => "显示方式",
            }
        };
        let title = if title.chars().count() > 36 {
            format!("{}…", title.chars().take(35).collect::<String>())
        } else {
            title.into()
        };
        content = content
            .push(container(text(title).size(12).color(MUTED)).padding([8, 10]))
            .push(separator());
        let mut entries = Vec::<(Icon, &str, Message, bool, bool)>::new();
        match self.manager.panel {
            Some(Panel::AssetMenu) => {
                if self.selected.is_some() {
                    entries.extend([
                        (Icon::Image, "放大查看", Message::ExpandPreview, false, true),
                        (Icon::Edit, "重命名", Message::ShowRenameAsset, false, true),
                        (
                            Icon::Download,
                            "导出图片…",
                            Message::Manage(ExportMenu),
                            false,
                            true,
                        ),
                        (
                            Icon::Folder,
                            "项目归属…",
                            Message::Manage(AssetProjects),
                            false,
                            true,
                        ),
                        (
                            Icon::Collection,
                            "版本与对比…",
                            Message::Manage(Versions),
                            false,
                            true,
                        ),
                        (
                            Icon::Folder,
                            "在文件夹中显示",
                            Message::Manage(RevealAsset),
                            false,
                            true,
                        ),
                        (
                            Icon::Edit,
                            "发送到 Synco 精修",
                            Message::Manage(SendSynco),
                            false,
                            !self.manager.synco_running,
                        ),
                        (
                            Icon::Trash,
                            if self.query.trash {
                                "从回收站恢复"
                            } else {
                                "移入回收站"
                            },
                            Message::Manage(Edit(moriq_domain::Edit::Trash(!self.query.trash))),
                            true,
                            true,
                        ),
                    ]);
                    if self.query.trash {
                        entries.push((
                            Icon::Trash,
                            "永久删除…",
                            Message::Manage(Purge),
                            true,
                            true,
                        ));
                    }
                }
            }
            Some(Panel::ViewOptions) => entries.extend([
                (Icon::Grid, "瀑布流", Message::SetMasonry(true), false, true),
                (
                    Icon::Grid,
                    "固定网格",
                    Message::SetMasonry(false),
                    false,
                    true,
                ),
                (Icon::List, "列表", Message::List(true), false, true),
            ]),
            Some(Panel::Import) => entries.extend([
                (Icon::Image, "选择图片文件…", Message::Import, false, true),
                (
                    Icon::Folder,
                    "导入文件夹…",
                    Message::Manage(FolderImport),
                    false,
                    true,
                ),
                (
                    Icon::Folder,
                    "引用外部原片…",
                    Message::Manage(ReferenceImport),
                    false,
                    true,
                ),
            ]),
            Some(Panel::Menu) => entries.extend([
                (
                    Icon::Edit,
                    "编辑资源库信息…",
                    Message::ShowRenameLibrary,
                    false,
                    true,
                ),
                (
                    Icon::Image,
                    "更换资源库封面…",
                    Message::Manage(Cover),
                    false,
                    true,
                ),
                (
                    Icon::Folder,
                    "项目管理…",
                    Message::Manage(Project),
                    false,
                    true,
                ),
                (
                    Icon::Tag,
                    "标签与智能集合…",
                    Message::Manage(Collections),
                    false,
                    true,
                ),
                (
                    Icon::Download,
                    "导出所选素材…",
                    Message::Manage(ExportMenu),
                    false,
                    self.selected.is_some() || !self.manager.selection.is_empty(),
                ),
                (
                    Icon::Clock,
                    "Synco 交接任务…",
                    Message::Manage(Synco),
                    false,
                    true,
                ),
                (
                    Icon::Folder,
                    "备份与完整性检查…",
                    Message::Manage(Maintenance),
                    false,
                    true,
                ),
                (
                    Icon::Folder,
                    "空间明细…",
                    Message::Storage(StorageMessage::Open),
                    false,
                    true,
                ),
                (
                    Icon::Clock,
                    "活动记录…",
                    Message::Journal(JournalMessage::Open(false)),
                    false,
                    true,
                ),
                (
                    Icon::Clock,
                    "后台任务…",
                    Message::Manage(Tasks),
                    false,
                    true,
                ),
                (
                    Icon::Settings,
                    "搜索与快捷键…",
                    Message::Manage(Help),
                    false,
                    true,
                ),
                (
                    Icon::Package,
                    "开源组件声明…",
                    Message::Manage(Licenses),
                    false,
                    true,
                ),
            ]),
            _ => {}
        }
        let estimated = 55.0 + entries.len() as f32 * 36.0;
        for (kind, label, event, danger, enabled) in entries {
            let readonly = matches!(
                &event,
                Message::Manage(Tasks | Maintenance | Synco)
                    | Message::Storage(StorageMessage::Open)
                    | Message::Journal(JournalMessage::Open(_))
            );
            let enabled = enabled && (!self.manager.busy || readonly);
            let color = if !enabled {
                MUTED
            } else if danger {
                iced::Color::from_rgb8(194, 43, 43)
            } else {
                TEXT
            };
            content = content.push(
                button(
                    row![icon(kind, 15.0, color), text(label).size(13).color(color)]
                        .spacing(10)
                        .align_y(iced::Alignment::Center),
                )
                .width(iced::Length::Fill)
                .padding([9, 10])
                .style(flat)
                .on_press_maybe(enabled.then_some(event)),
            );
        }
        let width = (self.viewport.width - 24.0).clamp(180.0, 272.0);
        let height = (self.viewport.height - 32.0).max(120.0);
        let x = self
            .surfaces
            .anchor
            .x
            .clamp(12.0, (self.viewport.width - width - 12.0).max(12.0));
        let y = self.surfaces.anchor.y.clamp(
            12.0,
            (self.viewport.height - estimated.min(height) - 12.0).max(12.0),
        );
        let popup = container(scroll(content).height(iced::Length::Shrink))
            .padding(6)
            .width(width)
            .max_height((self.viewport.height - y - 12.0).max(120.0))
            .style(|_| container::Style {
                background: Some(iced::Color::WHITE.into()),
                border: iced::Border {
                    color: LINE,
                    width: 1.0,
                    radius: 8.0.into(),
                },
                shadow: iced::Shadow {
                    color: iced::Color::from_rgba8(30, 45, 70, 0.14),
                    offset: iced::Vector::new(0.0, 5.0),
                    blur_radius: 18.0,
                },
                ..Default::default()
            });
        stack![
            base,
            mouse_area(
                container(Space::new())
                    .width(iced::Length::Fill)
                    .height(iced::Length::Fill)
            )
            .on_press(Message::Manage(Close))
            .on_right_press(Message::Manage(Close)),
            container(mouse_area(popup).on_press(Message::Noop)).padding(iced::Padding {
                top: y,
                left: x,
                ..Default::default()
            })
        ]
        .into()
    }
}
