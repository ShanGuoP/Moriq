use super::*;
use crate::{
    components::*,
    icons::{Icon, icon},
    manage::Panel,
    theme::*,
};
use iced::widget::{Space, button, column, container, mouse_area, row, stack, text};
impl App {
    /// 判断当前浮层是否采用紧凑展示布局。
    pub(super) fn compact_panel(&self) -> bool {
        matches!(
            self.manager.panel,
            Some(Panel::AssetMenu | Panel::Menu | Panel::ViewOptions | Panel::Import)
        )
    }
    /// 构建当前上下文菜单或弹出区域，保留底层控件树状态。
    pub(super) fn popup<'a>(&'a self, base: Element<'a, Message>) -> Element<'a, Message> {
        let language = self.preferences.language;
        use ManageMessage::*;
        let mut content = column![].spacing(2);
        let multiple = self.manager.selection.enabled;
        let title = if self.manager.panel == Some(Panel::AssetMenu) && multiple {
            tr!(language, "所选素材" => "Selected assets")
        } else if self.manager.panel == Some(Panel::AssetMenu) {
            self.viewer
                .selected
                .as_ref()
                .map(|a| a.name.as_str())
                .unwrap_or(tr!(language, "素材操作" => "Asset actions"))
        } else {
            match self.manager.panel {
                Some(Panel::Menu) => tr!(language, "资源库工具" => "Library tools"),
                Some(Panel::Import) => tr!(language, "导入" => "Import"),
                _ => tr!(language, "显示方式" => "Display options"),
            }
        };
        let title = if self.manager.panel == Some(Panel::AssetMenu) && multiple {
            format!("{title} · {}", self.selected_ids().len())
        } else {
            title.to_owned()
        };
        let title = if title.chars().count() > 36 {
            format!("{}…", title.chars().take(35).collect::<String>())
        } else {
            title
        };
        content = content
            .push(
                container(text(title).size(12).style(crate::theme::text_color(MUTED)))
                    .padding([8, 10]),
            )
            .push(separator());
        let mut entries = Vec::<(Icon, &str, Message, bool, bool)>::new();
        match self.manager.panel {
            Some(Panel::AssetMenu) => {
                if !self.selected_ids().is_empty() {
                    entries.extend([
                        (
                            Icon::Image,
                            tr!(language, "放大查看" => "Enlarge"),
                            Message::ExpandPreview,
                            false,
                            !multiple,
                        ),
                        (
                            Icon::Edit,
                            tr!(language, "重命名" => "Rename"),
                            Message::ShowRenameAsset,
                            false,
                            !multiple,
                        ),
                        (
                            Icon::Download,
                            tr!(language, "导出图片…" => "Export images…"),
                            Message::Manage(ExportMenu),
                            false,
                            true,
                        ),
                        (
                            Icon::Folder,
                            tr!(language, "项目归属…" => "Project membership…"),
                            Message::Manage(AssetProjects),
                            false,
                            !multiple,
                        ),
                        (
                            Icon::Image,
                            tr!(language, "设为资源库封面" => "Set as library cover"),
                            Message::Manage(AssetCover),
                            false,
                            !multiple
                                && !self.query.trash
                                && !self.switching
                                && self.manager.cover_saving.is_none(),
                        ),
                        (
                            Icon::Collection,
                            tr!(language, "版本与对比…" => "Versions and comparison…"),
                            Message::Manage(Versions),
                            false,
                            !multiple,
                        ),
                        (
                            Icon::Folder,
                            tr!(language, "在文件夹中显示" => "Show in folder"),
                            Message::Manage(RevealAsset),
                            false,
                            !multiple,
                        ),
                        (
                            Icon::Edit,
                            tr!(language, "发送到 Synco 精修" => "Send to Synco for editing"),
                            Message::Manage(SendSynco),
                            false,
                            !self.manager.synco.running,
                        ),
                        (
                            Icon::Trash,
                            if self.query.trash {
                                tr!(language, "从回收站恢复" => "Restore from Trash")
                            } else {
                                tr!(language, "移入回收站" => "Move to Trash")
                            },
                            Message::Manage(Edit(moriq_domain::Edit::Trash(!self.query.trash))),
                            true,
                            true,
                        ),
                    ]);
                    if multiple && !self.query.trash {
                        entries.extend([
                            (
                                Icon::Heart,
                                tr!(language, "收藏所选素材" => "Favorite selected assets"),
                                Message::Manage(Edit(moriq_domain::Edit::Favorite(true))),
                                false,
                                true,
                            ),
                            (
                                Icon::Heart,
                                tr!(language, "取消所选收藏" => "Unfavorite selected assets"),
                                Message::Manage(Edit(moriq_domain::Edit::Favorite(false))),
                                false,
                                true,
                            ),
                            (
                                Icon::Collection,
                                tr!(language, "批量处理…" => "Batch actions…"),
                                Message::Batch(batch::BatchMessage::Open),
                                false,
                                true,
                            ),
                        ]);
                    }
                    if self.query.trash {
                        entries.push((
                            Icon::Trash,
                            tr!(language, "永久删除…" => "Delete permanently…"),
                            Message::Manage(Purge),
                            true,
                            true,
                        ));
                    }
                }
            }
            Some(Panel::ViewOptions) => entries.extend([
                (
                    Icon::Grid,
                    tr!(language, "瀑布流" => "Masonry"),
                    Message::SetMasonry(true),
                    false,
                    true,
                ),
                (
                    Icon::Grid,
                    tr!(language, "固定网格" => "Fixed grid"),
                    Message::SetMasonry(false),
                    false,
                    true,
                ),
                (
                    Icon::List,
                    tr!(language, "列表" => "List"),
                    Message::List(true),
                    false,
                    true,
                ),
            ]),
            Some(Panel::Import) => entries.extend([
                (
                    Icon::Image,
                    tr!(language, "选择图片文件…" => "Choose image files…"),
                    Message::Import,
                    false,
                    true,
                ),
                (
                    Icon::Folder,
                    tr!(language, "导入文件夹…" => "Import folder…"),
                    Message::Manage(FolderImport),
                    false,
                    true,
                ),
                (
                    Icon::Folder,
                    tr!(language, "引用外部原片…" => "Reference external originals…"),
                    Message::Manage(ReferenceImport),
                    false,
                    true,
                ),
            ]),
            Some(Panel::Menu) => entries.extend([
                (
                    Icon::Edit,
                    tr!(language, "编辑资源库信息…" => "Edit library information…"),
                    Message::ShowRenameLibrary,
                    false,
                    true,
                ),
                (
                    Icon::Image,
                    tr!(language, "更换资源库封面…" => "Change library cover…"),
                    Message::Manage(Cover),
                    false,
                    self.manager.cover_saving.is_none(),
                ),
                (
                    Icon::Folder,
                    tr!(language, "项目管理…" => "Manage projects…"),
                    Message::Manage(Project),
                    false,
                    true,
                ),
                (
                    Icon::Tag,
                    tr!(language, "标签与智能集合…" => "Tags and smart collections…"),
                    Message::Manage(Collections),
                    false,
                    true,
                ),
                (
                    Icon::Download,
                    tr!(language, "导出所选素材…" => "Export selected assets…"),
                    Message::Manage(ExportMenu),
                    false,
                    self.viewer.selected.is_some() || !self.manager.selection.assets.is_empty(),
                ),
                (
                    Icon::Clock,
                    tr!(language, "Synco 交接任务…" => "Synco handoff tasks…"),
                    Message::Manage(Synco),
                    false,
                    true,
                ),
                (
                    Icon::Folder,
                    tr!(language, "备份与完整性检查…" => "Backup and integrity…"),
                    Message::Manage(Maintenance),
                    false,
                    true,
                ),
                (
                    Icon::Folder,
                    tr!(language, "空间明细…" => "Storage details…"),
                    Message::Storage(StorageMessage::Open),
                    false,
                    true,
                ),
                (
                    Icon::Clock,
                    tr!(language, "活动记录…" => "Activity…"),
                    Message::Journal(JournalMessage::Open(false)),
                    false,
                    true,
                ),
                (
                    Icon::Clock,
                    tr!(language, "后台任务…" => "Background tasks…"),
                    Message::Manage(Tasks),
                    false,
                    true,
                ),
                (
                    Icon::Settings,
                    tr!(language, "搜索与快捷键…" => "Search and shortcuts…"),
                    Message::Manage(Help),
                    false,
                    true,
                ),
                (
                    Icon::Package,
                    tr!(language, "开源组件声明…" => "Open source notices…"),
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
                    row![
                        icon(kind, 15.0, color),
                        text(label).size(13).style(crate::theme::text_color(color))
                    ]
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
            .style(|theme| container::Style {
                background: Some(iced::Color::WHITE.into()),
                border: iced::Border {
                    color: crate::theme::adaptive(theme, LINE),
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
            container(
                mouse_area(crate::motion::translated(
                    popup.into(),
                    -0.4 * self.surface_offset()
                ))
                .on_press(Message::Noop)
            )
            .padding(iced::Padding {
                top: y,
                left: x,
                ..Default::default()
            })
        ]
        .into()
    }
}
