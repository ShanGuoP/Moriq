use super::*;
use crate::components::grouped_number as number;
use crate::components::*;
use crate::icons::{Icon, icon};
use crate::theme::*;
use iced::widget::{Column, Row, Space, button, column, container, image, row, stack, text};
use iced::{Alignment, Border, Color, ContentFit, Length};
impl App {
    pub(super) fn overview(&self) -> Element<'_, Message> {
        let language = self.preferences.language;
        let Some(library) = &self.library else {
            return self.welcome();
        };
        let info = &library.info;
        let cover = self
            .manager
            .cover
            .as_ref()
            .or_else(|| self.tiles.first().and_then(|t| t.handle.as_ref()));
        let picture: Element<'_, Message> = if let Some(cover) = cover {
            image(cover.clone())
                .width(Length::Fill)
                .height(265)
                .content_fit(ContentFit::Cover)
                .border_radius(9)
                .into()
        } else {
            Space::new().height(265).width(Length::Fill).into()
        };
        let title = row![
            text(format!("{}.moriq", info.name))
                .size(26)
                .style(crate::theme::text_color(Color::WHITE))
                .font(iced::Font {
                    weight: iced::font::Weight::Semibold,
                    ..iced::Font::DEFAULT
                }),
            button(icon(Icon::Edit, 16.0, Color::WHITE))
                .padding(3)
                .style(flat)
                .on_press(Message::ShowRenameLibrary)
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        let description = text(if self.manager.organizer.description.is_empty() {
            tr!(language, "收集、整理与保存你的创作素材。" => "Collect, organize and preserve your creative assets.")
        } else {
            &self.manager.organizer.description
        })
        .size(14)
        .line_height(1.7)
        .style(crate::theme::text_color(Color::from_rgba8(255, 255, 255, 0.9)));
        let location = row![
            icon(Icon::Folder, 16.0, Color::WHITE),
            text(display_path(&library.root))
                .size(13)
                .style(crate::theme::text_color(Color::WHITE)),
            button(icon(Icon::Copy, 14.0, Color::WHITE))
                .padding(0)
                .style(flat)
                .on_press(Message::CopyLibraryPath)
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        let hero_info = row![
            container(photo(cover, Length::Fixed(110.0), 110.0))
                .padding(2)
                .style(|theme| container::Style {
                    border: Border {
                        color: crate::theme::adaptive(theme, Color::WHITE),
                        width: 2.0,
                        radius: 9.0.into()
                    },
                    ..Default::default()
                }),
            container(column![
                title,
                Space::new().height(8),
                description,
                Space::new().height(18),
                location
            ])
            .padding(iced::Padding {
                top: 6.0,
                right: 0.0,
                bottom: 0.0,
                left: 0.0
            })
            .width(Length::Fill)
        ]
        .spacing(22)
        .align_y(Alignment::Start);
        let stats = row![
            hero_stat(
                Icon::Image,
                BLUE,
                number(info.count),
                tr!(language, "图片素材" => "Image assets")
            ),
            hero_stat(
                Icon::Folder,
                GREEN,
                info.projects.len().to_string(),
                tr!(language, "项目数量" => "Projects")
            ),
            hero_stat(
                Icon::Star,
                Color::from_rgb8(246, 159, 11),
                number(info.favorites),
                tr!(language, "收藏数量" => "Favorites")
            ),
            hero_stat(
                Icon::Database,
                PURPLE,
                self.storage
                    .stats
                    .as_ref()
                    .map(|s| size(s.allocated))
                    .unwrap_or_else(|| size(info.bytes)),
                if self.storage.stats.is_some() {
                    tr!(language, "库内占用" => "Library storage")
                } else {
                    tr!(language, "素材登记量" => "Registered asset size")
                }
            )
        ]
        .spacing(6);
        let hero = stack![
            picture,
            container(column![hero_info, Space::new().height(Length::Fill), stats].spacing(10))
                .padding(iced::Padding {
                    top: 32.0,
                    right: 20.0,
                    bottom: 10.0,
                    left: 20.0
                })
                .height(265)
                .width(Length::Fill)
                .style(|_| container::Style {
                    background: Some(Color::from_rgba8(20, 49, 79, 0.37).into()),
                    border: Border {
                        radius: 9.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            container(row![
                Space::new().width(Length::Fill),
                button(
                    row![
                        icon(Icon::Image, 14.0, ICON),
                        text(tr!(language, "更换封面" => "Change cover")).size(13)
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center)
                )
                .padding([9, 16])
                .style(secondary)
                .on_press(Message::Manage(ManageMessage::Cover))
            ])
            .padding(iced::Padding {
                top: 14.0,
                right: 24.0,
                bottom: 0.0,
                left: 0.0
            })
            .width(Length::Fill)
        ];
        let quick = row![
            self.quick_action(
                Icon::Person,
                BLUE,
                tr!(language, "导入图片" => "Import images"),
                tr!(language, "支持文件与整个文件夹" => "Supports files and entire folders"),
                Message::Manage(ManageMessage::ImportMenu)
            ),
            self.quick_action(
                Icon::Plus,
                GREEN,
                tr!(language, "新建项目" => "New project"),
                tr!(language, "创建项目管理素材" => "Create a project to organize assets"),
                Message::ShowProject
            ),
            self.quick_action(
                Icon::Collection,
                PURPLE,
                tr!(language, "创建智能集合" => "Create smart collection"),
                tr!(language, "保存搜索与筛选规则" => "Save search and filter rules"),
                Message::Manage(ManageMessage::Collections)
            ),
            self.quick_action(
                Icon::Folder,
                BLUE,
                tr!(language, "打开资源库" => "Open library"),
                tr!(language, "在文件夹中查看" => "View in folder"),
                Message::RevealLibrary
            )
        ]
        .spacing(12);
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
                    button(photo(image, Length::Fixed(144.0), 256.0))
                        .padding(0)
                        .style(flat)
                        .on_press(Message::Project(project.id)),
                    container(
                        row![
                            button(
                                column![
                                    heading(title, 13),
                                    muted(trf!(language, "{} 张" => "{} photos", number(count)))
                                ]
                                .spacing(4)
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
                .width(144)
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
                button(column![
                    photo(tile.handle.as_ref(), Length::Fixed(144.0), 256.0),
                    container(text(&tile.asset.name).size(12)).padding(6)
                ])
                .padding(0)
                .style(secondary)
                .on_press(Message::Favorites)
                .width(144),
            );
        }

        let favorites: Element<'_, Message> = if !self.manager.favorites.is_empty() {
            iced::widget::scrollable(favorites)
                .direction(iced::widget::scrollable::Direction::Horizontal(
                    Default::default(),
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
        let mut smart = Column::new().spacing(7);
        for (label, count, builtin) in [
            (
                tr!(language, "最近导入" => "Recently imported"),
                self.manager.organizer.recent_count,
                Builtin::Recent,
            ),
            (
                tr!(language, "未加标签" => "Untagged"),
                self.manager.organizer.untagged_count,
                Builtin::Untagged,
            ),
            (
                tr!(language, "高评分" => "Highly rated"),
                self.manager.organizer.high_rating_count,
                Builtin::HighRating,
            ),
        ] {
            smart = smart.push(action(
                &format!("{label}  {count}"),
                Message::Manage(ManageMessage::Builtin(builtin)),
            ));
        }
        for collection in self.manager.organizer.collections.iter().take(3) {
            smart = smart.push(action(
                &collection.name,
                Message::Manage(ManageMessage::UseCollection(collection.id.clone())),
            ));
        }

        let bottom = row![
            column![
                column![
                    section(
                        self.preferences.language,
                        tr!(language, "收藏夹" => "Favorites"),
                        Message::Favorites
                    ),
                    favorites
                ]
                .spacing(10),
                Space::new().height(18),
                section(
                    self.preferences.language,
                    tr!(language, "标签概览" => "Tag overview"),
                    Message::Manage(ManageMessage::Tags)
                )
            ]
            .width(Length::FillPortion(3)),
            column![
                section(
                    self.preferences.language,
                    tr!(language, "智能集合" => "Smart collections"),
                    Message::Manage(ManageMessage::Collections)
                ),
                smart
            ]
            .spacing(10)
            .width(Length::FillPortion(2))
        ]
        .spacing(36);
        let mut tags = Row::new().spacing(7);

        for (tag, count) in self.manager.organizer.tags.iter().take(8) {
            tags = tags.push(
                button(chip(format!("# {tag} {count}"), BLUE))
                    .padding(0)
                    .style(flat)
                    .on_press(Message::Manage(ManageMessage::Tag(tag.clone()))),
            );
        }

        let center = column![
            hero,
            Space::new().height(18),
            column![
                heading(tr!(language, "快速操作" => "Quick actions"), 17),
                quick
            ]
            .spacing(9),
            Space::new().height(26),
            column![
                section(
                    self.preferences.language,
                    tr!(language, "项目 · 最近访问优先" => "Projects · Recently visited first"),
                    Message::Manage(ManageMessage::Project)
                ),
                iced::widget::scrollable(projects)
                    .direction(iced::widget::scrollable::Direction::Horizontal(
                        Default::default()
                    ))
                    .width(Length::Fill)
            ]
            .spacing(10),
            Space::new().height(18),
            bottom,
            Space::new().height(4),
            tags
        ]
        .spacing(0);
        let mut layout = row![
            scroll(center)
                .id("overview")
                .on_scroll(|v| Message::PageScrolled(
                    0,
                    v.absolute_offset().y,
                    (v.content_bounds().height - v.bounds().height).max(0.0)
                ))
                .height(Length::Fill)
                .width(Length::Fill)
        ];
        if self.viewport.width >= 1480.0 {
            layout = layout.push(scroll(self.library_details()).width(376));
        }
        layout
            .spacing(18)
            .padding(iced::Padding {
                top: 20.0,
                right: 24.0,
                bottom: 20.0,
                left: 24.0,
            })
            .height(Length::Fill)
            .into()
    }
    fn quick_action<'a>(
        &self,
        kind: Icon,
        color: Color,
        title: &str,
        description: &str,
        message: Message,
    ) -> Element<'a, Message> {
        button(
            row![
                container(
                    container(icon(kind, 24.0, Color::WHITE))
                        .padding(8)
                        .style(move |_| container::Style {
                            background: Some(color.into()),
                            border: Border {
                                radius: 10.0.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        })
                )
                .padding(6)
                .style(move |_| container::Style {
                    background: Some(Color { a: 0.09, ..color }.into()),
                    border: Border {
                        radius: 12.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
                column![heading(title, 13), muted(description)]
                    .spacing(6)
                    .width(Length::Fill),
                icon(Icon::Right, 15.0, ICON)
            ]
            .spacing(10)
            .align_y(Alignment::Center),
        )
        .padding([11, 12])
        .width(Length::Fill)
        .style(secondary)
        .on_press(message)
        .into()
    }
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
                    action(tr!(language, "编辑" => "Edit"), Message::ShowRenameLibrary)
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
fn hero_stat<'a>(kind: Icon, color: Color, value: String, label: &str) -> Element<'a, Message> {
    container(
        row![
            container(icon(kind, 23.0, color))
                .padding(8)
                .style(|theme| container::Style {
                    background: Some(crate::theme::adaptive(theme, Color::WHITE).into()),
                    border: Border {
                        radius: 8.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                }),
            column![
                text(value)
                    .size(18)
                    .style(crate::theme::text_color(Color::WHITE)),
                text(label.to_string())
                    .size(13)
                    .style(crate::theme::text_color(Color::from_rgba8(
                        255, 255, 255, 0.78
                    )))
            ]
            .spacing(4)
        ]
        .spacing(12)
        .align_y(Alignment::Center),
    )
    .padding([14, 14])
    .width(Length::Fill)
    .style(|_| container::Style {
        background: Some(Color::from_rgba8(255, 255, 255, 0.18).into()),
        border: Border {
            radius: 8.0.into(),
            ..Default::default()
        },
        ..Default::default()
    })
    .into()
}
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
