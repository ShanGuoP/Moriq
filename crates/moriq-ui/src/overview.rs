use super::*;
use crate::components::grouped_number as number;
use crate::components::*;
use crate::icons::{Icon, icon};
use crate::theme::*;
use iced::widget::{Column, Row, Space, button, column, container, row, text, tooltip};
use iced::{Alignment, Length};
impl App {
    /// 构建首页资源库摘要、项目和收藏分类区域。
    pub(super) fn overview(&self) -> Element<'_, Message> {
        let language = self.preferences.language;
        let Some(library) = &self.library else {
            return self.welcome();
        };
        let info = &library.info;
        let hero = row![
            column![
                muted(tr!(language, "资源库" => "LIBRARY")).size(11),
                heading(info.name.clone(), 28),
                muted(trf!(language, "{} 张素材  ·  {} 个项目  ·  {}" => "{} assets  ·  {} projects  ·  {}",
                    number(info.count), info.projects.len(), size(info.bytes))),
            ].spacing(8).width(Length::Fill),
            form_action(Icon::Image, tr!(language, "浏览素材" => "Explore assets"), Message::AllAssets, true),
            action(tr!(language, "资料与活动" => "Details & activity"), Message::ToggleOverviewDetails)
                .style(liquid_nav(self.overview_details)),
        ].spacing(12).align_y(Alignment::Center);
        let quick = row![
            form_action(
                Icon::Plus,
                tr!(language, "新建项目" => "New project"),
                Message::ShowProject,
                false
            ),
            form_action(
                Icon::Image,
                tr!(language, "导入素材" => "Import assets"),
                Message::Manage(ManageMessage::ImportMenu),
                false
            ),
            action(
                tr!(language, "智能集合" => "Smart collections"),
                Message::Manage(ManageMessage::Collections)
            ),
            action(
                tr!(language, "标签" => "Tags"),
                Message::Manage(ManageMessage::Tags)
            ),
        ]
        .spacing(8)
        .wrap();
        let mut projects = Row::new().spacing(14);
        let mut recent_projects: Vec<_> = info.projects.iter().collect();
        recent_projects.sort_by(|a, b| {
            b.visited_at
                .cmp(&a.visited_at)
                .then_with(|| a.name.cmp(&b.name))
        });
        for project in recent_projects.into_iter().take(4) {
            let title = &project.name;
            let image = self.manager.project_covers.get(&project.id);
            let count = project.count;
            projects = projects.push(
                container(column![
                    button(photo(image, Length::Fixed(224.0), 150.0))
                        .padding(0)
                        .style(flat)
                        .on_press(Message::Project(project.id)),
                    container(
                        row![
                            button(
                                column![
                                    heading(title, 13)
                                        .width(Length::Fill)
                                        .wrapping(text::Wrapping::Glyph),
                                    muted(trf!(language, "{} 张" => "{} photos", number(count)))
                                ]
                                .spacing(4)
                                .width(Length::Fill)
                            )
                            .padding(0)
                            .style(flat)
                            .width(Length::Fill)
                            .on_press(Message::Project(project.id)),
                            button(icon(Icon::More, 16.0, ICON))
                                .padding(4)
                                .style(flat)
                                .on_press(Message::Manage(ManageMessage::ProjectMenu(project.id)))
                        ]
                        .align_y(Alignment::Center)
                    )
                    .padding([8, 9])
                ])
                .width(224)
                .style(panel),
            );
        }
        if info.projects.is_empty() {
            projects = projects.push(
                container(
                    column![
                        muted(tr!(language, "创建项目，开始整理一次拍摄或一个新想法。" => "Create a project for a shoot or a new idea.")),
                        action(tr!(language, "新建项目" => "New project"), Message::ShowProject)
                    ]
                    .spacing(10),
                )
                .padding(20)
                .width(Length::Fill)
                .style(panel),
            );
        }
        let mut favorites = Row::new().spacing(10);

        for tile in self.manager.favorites.iter().take(4) {
            favorites = favorites.push(
                tooltip(
                    button(
                        column![
                            photo(tile.handle.as_ref(), Length::Fill, 150.0),
                            container(
                                text(crate::welcome::compact(&tile.asset.name, 20))
                                    .size(12)
                                    .line_height(text::LineHeight::Absolute(16.0.into()))
                                    .width(Length::Fill)
                                    .height(32)
                                    .wrapping(text::Wrapping::Glyph)
                            )
                            .padding([6, 8])
                            .height(44)
                            .width(Length::Fill)
                            .clip(true)
                        ]
                        .width(Length::Fill),
                    )
                    .padding(0)
                    .style(secondary)
                    .on_press(Message::Favorites)
                    .width(224),
                    container(
                        text(&tile.asset.name)
                            .size(12)
                            .width(Length::Fill)
                            .wrapping(text::Wrapping::Glyph),
                    )
                    .padding([8, 10])
                    .width(320)
                    .style(panel),
                    tooltip::Position::Top,
                )
                .gap(8)
                .delay(Duration::from_millis(450))
                .snap_within_viewport(true),
            );
        }

        let favorites: Element<'_, Message> = if !self.manager.favorites.is_empty() {
            iced::widget::scrollable(favorites)
                .direction(iced::widget::scrollable::Direction::Horizontal(
                    iced::widget::scrollable::Scrollbar::new().spacing(8),
                ))
                .width(Length::Fill)
                .into()
        } else {
            container(muted(tr!(language, "收藏喜欢的素材，让灵感随手可得。" => "Favorite assets to keep inspiration at hand.")))
                    .height(105)
                    .width(Length::Fill)
                    .center_y(105)
                    .padding(16)
                    .style(panel)
                    .into()
        };
        let mut center = column![
            hero,
            separator(),
            row![
                heading(tr!(language, "最近项目" => "Recent projects"), 18),
                Space::new().width(Length::Fill),
                action(
                    tr!(language, "全部项目" => "All projects"),
                    Message::Manage(ManageMessage::Project)
                )
            ]
            .align_y(Alignment::Center),
            iced::widget::scrollable(projects)
                .direction(iced::widget::scrollable::Direction::Horizontal(
                    iced::widget::scrollable::Scrollbar::new().spacing(8)
                ))
                .width(Length::Fill),
            quick,
        ]
        .spacing(22);
        if !self.manager.favorites.is_empty() {
            center = center
                .push(separator())
                .push(section(
                    language,
                    tr!(language, "收藏" => "Favorites"),
                    Message::Favorites,
                ))
                .push(favorites);
        } else {
            center = center.push(action(
                tr!(language, "查看收藏" => "View favorites"),
                Message::Favorites,
            ));
        }
        if self.overview_details {
            center = center.push(separator()).push(self.library_details());
        }
        scroll(
            container(center)
                .max_width(1200)
                .padding([28, 32])
                .width(Length::Fill),
        )
        .id("overview")
        .on_scroll(|v| {
            Message::PageScrolled(
                0,
                v.absolute_offset().y,
                (v.content_bounds().height - v.bounds().height).max(0.0),
            )
        })
        .height(Length::Fill)
        .into()
    }
    /// 构建资源库信息与维护入口区域。
    fn library_details(&self) -> Element<'_, Message> {
        let language = self.preferences.language;
        let Some(library) = &self.library else {
            return Space::new().into();
        };
        let info = &library.info;
        let info_panel = container(
            column![
                row![
                    heading(tr!(language, "资源库信息" => "Library information"), 17),
                    Space::new().width(Length::Fill),
                    action(tr!(language, "编辑" => "Edit"), Message::ShowRenameLibrary),
                    icon_button(Icon::Folder, Message::RevealLibrary),
                    icon_button(Icon::Image, Message::Manage(ManageMessage::Cover))
                ]
                .align_y(Alignment::Center),
                info_row(
                    tr!(language, "名称" => "Name"),
                    format!("{}.moriq", info.name)
                ),
                info_row(
                    tr!(language, "描述" => "Description"),
                    if self.manager.organizer.description.is_empty() {
                        tr!(language, "暂无描述" => "No description").into()
                    } else {
                        self.manager.organizer.description.clone()
                    }
                ),
                separator(),
                info_row(
                    tr!(language, "创建时间" => "Created"),
                    date(info.created_at)
                ),
                info_row(
                    tr!(language, "更新时间" => "Updated"),
                    date(info.updated_at)
                ),
                row![
                    muted(tr!(language, "资源库路径" => "Library path")).width(80),
                    text(display_path(&library.root))
                        .size(10)
                        .style(crate::theme::text_color(ICON)),
                    button(icon(Icon::Copy, 13.0, ICON))
                        .padding(0)
                        .style(flat)
                        .on_press(Message::CopyLibraryPath)
                ]
                .spacing(8)
            ]
            .spacing(16),
        )
        .padding(18)
        .width(Length::Fill)
        .style(panel);
        let storage_value = self
            .storage
            .stats
            .as_ref()
            .map(|s| size(s.allocated))
            .unwrap_or_else(|| tr!(language, "待统计" => "Not calculated").into());
        let mut legend = Column::new().spacing(14);
        if let Some(stats) = &self.storage.stats {
            for (label, bytes) in &stats.categories {
                legend = legend.push(
                    row![
                        text(label).size(11).width(Length::Fill),
                        muted(size(*bytes))
                    ]
                    .spacing(8),
                );
            }
            legend = legend.push(muted(trf!(language, "采样 {}{}" => "Sampled {}{}",
                date(stats.measured_at),
                if stats.cancelled || stats.warnings > 0 {
                    tr!(language, " · 部分结果" => " · Partial results")
                } else {
                    ""
                }
            )));
        } else {
            legend = legend.push(muted(trf!(language, "素材登记量 {}，含外部引用；尚未统计实际磁盘空间。" => "Registered assets: {}, including external references. Actual disk usage has not been calculated.",
                    size(info.bytes)
                )));
        }

        let storage = container(
            column![
                section(
                    self.preferences.language,
                    tr!(language, "存储使用情况" => "Storage usage"),
                    Message::Storage(StorageMessage::Open)
                ),
                row![
                    donut(
                        storage_value,
                        tr!(language, "库内占用" => "Library storage"),
                        self.storage
                            .stats
                            .as_ref()
                            .filter(|s| s.allocated > 0)
                            .map_or(0.0, |s| {
                                s.categories
                                    .first()
                                    .map_or(0.0, |(_, bytes)| *bytes as f32 / s.allocated as f32)
                            })
                    ),
                    container(legend).width(Length::Fill)
                ]
                .spacing(12)
                .align_y(Alignment::Center)
            ]
            .spacing(10),
        )
        .padding(18)
        .style(panel);
        let mut activities = Column::new().spacing(10);

        for event in &self.journal.recent {
            let message = event
                .asset
                .map(|id| Message::Journal(JournalMessage::OpenAsset(id)))
                .unwrap_or(Message::Journal(JournalMessage::Open(false)));
            activities = activities.push(
                button(
                    column![
                        heading(journal::event_label(language, &event.kind), 12),
                        text(&event.detail).size(12),
                        muted(date(event.at))
                    ]
                    .spacing(4),
                )
                .padding(4)
                .style(flat)
                .on_press(message),
            );
        }
        if self.journal.recent.is_empty() {
            activities = activities.push(muted(
                tr!(language, "暂无已记录的活动" => "No recorded activity"),
            ));
        }

        column![
            info_panel,
            storage,
            container(
                column![
                    section(
                        self.preferences.language,
                        tr!(language, "最近活动" => "Recent activity"),
                        Message::Journal(JournalMessage::Open(false))
                    ),
                    activities
                ]
                .spacing(14)
            )
            .padding(18)
            .style(panel)
        ]
        .spacing(14)
        .into()
    }
}
/// 构建资源库资料的名称和值展示行。
fn info_row<'a>(label: &str, value: String) -> Element<'a, Message> {
    row![
        muted(label).width(80),
        text(value)
            .size(13)
            .line_height(1.6)
            .style(crate::theme::text_color(ICON))
    ]
    .spacing(8)
    .into()
}
