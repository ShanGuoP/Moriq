use super::*;
use crate::components::grouped_number as number;
use crate::components::*;
use crate::icons::{Icon, icon};
use crate::theme::*;
use iced::widget::{
    Column, Row, Space, button, column, container, image, mouse_area, row, slider, stack, text,
    text_input,
};
use iced::{Alignment, Border, Color, ContentFit, Length};
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagChoice {
    All,
    Tag(String),
    More,
}
impl std::fmt::Display for TagChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::All => f.write_str("全部标签"),
            Self::More => f.write_str("管理 / 更多标签…"),
            Self::Tag(tag) => f.write_str(tag),
        }
    }
}
impl App {
    pub(super) fn assets(&self) -> Element<'_, Message> {
        if self.preview_expanded && self.selected.is_some() {
            return self.large_preview();
        }
        let title = if self.query.media_kind == MediaKind::Raw {
            "RAW 素材".into()
        } else if self.query.media_kind == MediaKind::Video {
            "视频素材".into()
        } else if self.query.trash {
            "回收站".into()
        } else if self.query.favorites_only {
            "收藏夹".into()
        } else if !self.query.category.is_empty() {
            self.query.category.clone()
        } else if self.query.person.is_some() {
            "人物素材".into()
        } else if let Some(project) = self.query.project {
            self.library
                .as_ref()
                .and_then(|library| library.info.projects.iter().find(|p| p.id == project))
                .map(|p| p.name.clone())
                .unwrap_or_else(|| "全部素材".into())
        } else {
            "全部素材".into()
        };
        let titlebar = row![
            column![
                heading(title, 25),
                muted(format!("{} 张素材", number(self.total)))
            ]
            .spacing(4),
            Space::new().width(Length::Fill),
            icon(Icon::Edit, 13.0, MUTED),
            slider(160.0..=280.0, self.card_width, Message::Zoom)
                .width(110)
                .style(zoom),
            button(icon(Icon::Grid, 16.0, ICON))
                .padding(8)
                .style(secondary)
                .on_press(Message::SetMasonry(true)),
            button(icon(Icon::List, 16.0, ICON))
                .padding(8)
                .style(secondary)
                .on_press(Message::List(true)),
            icon_button(Icon::More, Message::Manage(ManageMessage::ViewOptions))
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        let mut filters = Row::new().spacing(8);
        for label in [
            "全部类型",
            "全部时间",
            "全部标签",
            "评分",
            "文件大小",
            "添加方式",
        ] {
            if label == "全部标签" {
                let mut values = vec![TagChoice::All];
                values.extend(
                    self.manager
                        .organizer
                        .tags
                        .iter()
                        .take(80)
                        .map(|(t, _)| TagChoice::Tag(t.clone())),
                );
                let selected = if self.query.tag.is_empty() {
                    TagChoice::All
                } else {
                    TagChoice::Tag(self.query.tag.clone())
                };
                if !values.contains(&selected) {
                    values.push(selected.clone());
                }
                values.push(TagChoice::More);
                filters = filters.push(
                    choice(values, Some(selected), Message::TagFilter)
                        .width(112)
                        .text_size(12),
                );
                continue;
            }
            let displayed = match label {
                "全部类型" if self.query.media_kind == MediaKind::Raw => "RAW".into(),
                "全部类型" if !self.query.format.is_empty() => self.query.format.to_uppercase(),
                "全部时间" if self.query.since > 0 || self.query.before > 0 => {
                    "已选时间".into()
                }
                "评分" if self.query.min_rating > 0 => format!("≥ {} 星", self.query.min_rating),
                "文件大小" if self.query.min_bytes > 0 || self.query.max_bytes > 0 => {
                    "已设大小".into()
                }
                "添加方式" if self.query.storage == 1 => "托管导入".into(),
                "添加方式" if self.query.storage == 2 => "外部引用".into(),
                _ => label.to_string(),
            };
            filters = filters.push(
                button(
                    row![
                        text(displayed).size(12).color(ICON),
                        icon(Icon::Down, 12.0, ICON)
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                )
                .padding([7, 10])
                .style(secondary)
                .on_press(Message::Manage(ManageMessage::Filters)),
            );
        }
        let gallery: Element<'_, Message> = if self.total == 0 {
            container(
                column![
                    icon(Icon::Image, 48.0, MUTED),
                    heading("让灵感，在这里相遇。", 22),
                    muted(if self.query == Query::default() {
                        "导入一些图片，开始整理你的素材。"
                    } else {
                        "没有找到匹配的素材，试试其他关键词。"
                    }),
                    action("导入图片", Message::Import)
                ]
                .spacing(16)
                .align_x(Alignment::Center),
            )
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
        } else {
            self.gallery()
        };
        let mut center = column![titlebar, filters.wrap()];
        if !self.launch.design_preview {
            center = center.push(
                row![
                    action(
                        if self.manager.multiselect {
                            "退出多选"
                        } else {
                            "多选"
                        },
                        Message::Manage(ManageMessage::ToggleMulti)
                    ),
                    action(
                        &format!(
                            "整理 {} 张",
                            if self.manager.multiselect {
                                self.manager.selection.len()
                            } else {
                                usize::from(self.selected.is_some())
                            }
                        ),
                        Message::Manage(ManageMessage::Organize)
                    ),
                    action("筛选与排序", Message::Manage(ManageMessage::Filters)),
                    action("保存智能集合", Message::Manage(ManageMessage::Collections)),
                ]
                .spacing(8),
            );
        }
        let center = center
            .push(gallery)
            .spacing(15)
            .padding(iced::Padding {
                top: 20.0,
                right: 24.0,
                bottom: 0.0,
                left: 24.0,
            })
            .height(Length::Fill)
            .width(Length::Fill);
        row![
            center,
            container(scroll(self.details()).id("details"))
                .padding([16, 16])
                .width(self.details_width())
                .height(Length::Fill)
                .style(|_| container::Style {
                    background: Some(Color::from_rgb8(252, 253, 255).into()),
                    border: Border {
                        color: LINE,
                        width: 1.0,
                        radius: 0.0.into()
                    },
                    ..Default::default()
                })
        ]
        .height(Length::Fill)
        .into()
    }
    fn gallery(&self) -> Element<'_, Message> {
        if !self.list
            && let Some(layout) = self.active_masonry()
        {
            let mut columns = Row::new().spacing(10);
            for (column_index, placements) in layout
                .visible(self.scroll_y, self.grid().viewport_height)
                .into_iter()
                .enumerate()
            {
                let mut column = Column::new().width(Length::Fill);
                let mut bottom = 0.0;
                for placement in placements {
                    column = column.push(Space::new().height((placement.top - bottom).max(0.0)));
                    let tile = placement
                        .index
                        .checked_sub(self.offset)
                        .and_then(|i| self.tiles.get(i));
                    column = column.push(self.image_card(tile, placement.height));
                    bottom = placement.top + placement.height;
                }
                column = column.push(Space::new().height((layout.height - bottom).max(0.0)));
                columns = columns.push(column.width(Length::FillPortion(
                    (layout.widths[column_index] / layout.width * 1000.0).round() as u16,
                )));
            }
            return scroll(columns)
                .id("gallery")
                .on_scroll(|viewport| {
                    Message::GalleryViewport(
                        viewport.absolute_offset().y,
                        (viewport.content_bounds().height - viewport.bounds().height).max(0.0),
                    )
                })
                .height(Length::Fill)
                .width(Length::Fill)
                .into();
        }
        let grid = self.grid();
        let range = grid.visible(self.total as usize, self.scroll_y);
        let start = range.start / grid.columns;
        let end = range.end.div_ceil(grid.columns);
        let mut column = Column::new()
            .width(Length::Fill)
            .push(Space::new().height(start as f32 * grid.row_height));
        for r in start..end {
            let mut cards = Row::new().spacing(10).height(grid.row_height);
            for c in 0..grid.columns {
                let index = r * grid.columns + c;
                if index >= self.total as usize {
                    cards = cards.push(Space::new().width(Length::Fill));
                    continue;
                }
                let tile = index
                    .checked_sub(self.offset)
                    .and_then(|i| self.tiles.get(i));
                let card = if self.list {
                    self.list_card(tile)
                } else {
                    self.image_card(tile, grid.row_height - 10.0)
                };
                cards = cards.push(container(card).width(Length::Fill));
            }
            column = column.push(cards);
        }
        column = column.push(Space::new().height(
            (grid.total_height(self.total as usize) - end as f32 * grid.row_height).max(0.0),
        ));
        scroll(column)
            .id("gallery")
            .on_scroll(|viewport| {
                Message::GalleryViewport(
                    viewport.absolute_offset().y,
                    (viewport.content_bounds().height - viewport.bounds().height).max(0.0),
                )
            })
            .height(Length::Fill)
            .width(Length::Fill)
            .into()
    }
    fn image_card<'a>(&self, tile: Option<&'a Tile>, height: f32) -> Element<'a, Message> {
        let demo = self.launch.design_preview;
        let Some(tile) = tile else {
            return photo(None, Length::Fill, height);
        };
        let selected = if self.manager.multiselect {
            self.manager.selection.contains(&tile.asset.id)
        } else {
            self.selected
                .as_ref()
                .is_some_and(|asset| asset.id == tile.asset.id)
        };
        let favorite_art: Element<'a, Message> =
            if self.launch.design_preview && !tile.asset.favorite {
                Space::new().width(16).height(16).into()
            } else {
                icon(
                    Icon::Heart,
                    16.0,
                    if tile.asset.favorite {
                        Color::from_rgb8(255, 105, 133)
                    } else {
                        Color::WHITE
                    },
                )
            };
        let favorite = button(favorite_art)
            .padding(7)
            .style(|_, status| button::Style {
                background: matches!(status, button::Status::Hovered)
                    .then_some(Color::from_rgba8(25, 40, 60, 0.18).into()),
                border: Border {
                    radius: 15.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .on_press(Message::Favorite(tile.asset.id, !tile.asset.favorite));
        let content = stack![
            photo(tile.handle.as_ref(), Length::Fill, height - 4.0),
            container(column![
                row![
                    Space::new().width(Length::Fill),
                    favorite,
                    button(icon(Icon::More, 16.0, Color::WHITE))
                        .padding(7)
                        .style(|_, _| button::Style {
                            background: Some(Color::from_rgba8(20, 30, 40, 0.45).into()),
                            border: Border {
                                radius: 7.0.into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        })
                        .on_press(Message::AssetContext(tile.asset.id))
                ],
                Space::new().height(Length::Fill),
                container(if self.launch.design_preview {
                    Space::new().width(14).height(14).into()
                } else {
                    icon(
                        if moriq_app::is_video_format(&tile.asset.format) {
                            Icon::Video
                        } else {
                            Icon::Image
                        },
                        14.0,
                        Color::WHITE,
                    )
                })
                .padding(5)
                .style(move |_| container::Style {
                    background: Some(
                        if demo {
                            Color::TRANSPARENT
                        } else {
                            Color::from_rgba8(20, 30, 40, 0.45)
                        }
                        .into()
                    ),
                    border: Border {
                        radius: 5.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
            ])
            .padding(4)
            .height(height - 4.0)
            .width(Length::Fill)
        ];
        let hover = self.gallery_feedback.amount(tile.asset.id);
        mouse_area(
            container(content)
                .padding(2)
                .height(height)
                .width(Length::Fill)
                .style(move |_| container::Style {
                    background: Some(Color::TRANSPARENT.into()),
                    border: Border {
                        color: if selected {
                            BLUE
                        } else {
                            Color {
                                a: hover * 0.55,
                                ..CONTROL_LINE
                            }
                        },
                        width: if selected { 1.5 } else { hover },
                        radius: 8.0.into(),
                    },
                    shadow: iced::Shadow {
                        color: Color::from_rgba8(26, 34, 48, 0.09 * hover),
                        offset: iced::Vector::new(0.0, 2.0 * hover),
                        blur_radius: 8.0 * hover,
                    },
                    ..Default::default()
                }),
        )
        .on_press(if self.manager.multiselect {
            Message::Manage(ManageMessage::Toggle(tile.asset.id))
        } else {
            Message::Select(tile.asset.id)
        })
        .on_enter(Message::CardHovered(tile.asset.id, true))
        .on_exit(Message::CardHovered(tile.asset.id, false))
        .on_double_click(Message::OpenImage(tile.asset.id))
        .on_right_press(Message::AssetContext(tile.asset.id))
        .into()
    }
    fn list_card<'a>(&self, tile: Option<&'a Tile>) -> Element<'a, Message> {
        let Some(tile) = tile else {
            return photo(None, Length::Fill, 72.0);
        };
        let hover = self.gallery_feedback.amount(tile.asset.id);
        let selected = if self.manager.multiselect {
            self.manager.selection.contains(&tile.asset.id)
        } else {
            self.selected
                .as_ref()
                .is_some_and(|asset| asset.id == tile.asset.id)
        };
        mouse_area(
            container(
                row![
                    photo(tile.handle.as_ref(), Length::Fixed(82.0), 62.0),
                    column![
                        heading(&tile.asset.name, 13),
                        muted(format!(
                            "{} · {} × {} · {}",
                            tile.asset.format.to_uppercase(),
                            tile.asset.width,
                            tile.asset.height,
                            size(tile.asset.bytes)
                        ))
                    ]
                    .spacing(6)
                    .width(Length::Fill),
                    icon_button(
                        Icon::Heart,
                        Message::Favorite(tile.asset.id, !tile.asset.favorite)
                    ),
                    icon_button(Icon::More, Message::AssetContext(tile.asset.id))
                ]
                .spacing(14)
                .align_y(Alignment::Center),
            )
            .padding(6)
            .width(Length::Fill)
            .style(move |_| container::Style {
                background: Some(
                    if selected {
                        PALE
                    } else {
                        Color::from_rgb(1.0 - hover * 0.018, 1.0 - hover * 0.012, 1.0)
                    }
                    .into(),
                ),
                border: Border {
                    color: if selected { BLUE } else { LINE },
                    width: 1.0,
                    radius: 7.0.into(),
                },
                ..Default::default()
            }),
        )
        .on_press(if self.manager.multiselect {
            Message::Manage(ManageMessage::Toggle(tile.asset.id))
        } else {
            Message::Select(tile.asset.id)
        })
        .on_enter(Message::CardHovered(tile.asset.id, true))
        .on_exit(Message::CardHovered(tile.asset.id, false))
        .on_double_click(Message::OpenImage(tile.asset.id))
        .on_right_press(Message::AssetContext(tile.asset.id))
        .into()
    }
    pub(super) fn details(&self) -> Element<'_, Message> {
        let Some(asset) = &self.selected else {
            return column![
                heading("素材详情", 16),
                container(
                    column![
                        icon(Icon::Image, 36.0, MUTED),
                        muted("选择一张图片，查看详细信息")
                    ]
                    .spacing(14)
                    .align_x(Alignment::Center)
                )
                .height(240)
                .center_x(Length::Fill)
                .center_y(240)
            ]
            .spacing(18)
            .into();
        };
        let video = moriq_app::is_video_format(&asset.format);
        let cover = stack![
            button(photo(self.preview.as_ref(), Length::Fill, 188.0))
                .padding(0)
                .style(flat)
                .on_press(Message::ExpandPreview),
            container(
                row![
                    Space::new().width(Length::Fill),
                    button(icon(Icon::Heart, 17.0, Color::WHITE))
                        .padding(6)
                        .style(flat)
                        .on_press(Message::Favorite(asset.id, !asset.favorite)),
                    button(icon(Icon::Maximize, 17.0, Color::WHITE))
                        .padding(6)
                        .style(flat)
                        .on_press(Message::ExpandPreview),
                    button(icon(Icon::More, 17.0, Color::WHITE))
                        .padding(6)
                        .style(flat)
                        .on_press(Message::Manage(ManageMessage::AssetMenu))
                ]
                .spacing(4)
            )
            .padding(7)
            .width(Length::Fill)
        ];
        let cover: Element<'_, Message> = if video {
            stack![
                cover,
                container(
                    button(
                        row![
                            icon(Icon::Play, 20.0, Color::WHITE),
                            text("播放视频").size(13)
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center)
                    )
                    .padding([10, 14])
                    .style(primary)
                    .on_press(Message::ExpandPreview)
                )
                .center_x(Length::Fill)
                .center_y(188)
            ]
            .into()
        } else {
            cover.into()
        };
        let mut strip = Row::new().spacing(4);
        let neighbors = if self.launch.design_preview {
            &self.tiles
        } else {
            &self.neighbors
        };
        for tile in neighbors.iter().take(7) {
            strip = strip.push(
                button(photo(tile.handle.as_ref(), Length::Fixed(28.0), 28.0))
                    .padding(2)
                    .style(move |_, _| button::Style {
                        border: Border {
                            color: if tile.asset.id == asset.id {
                                BLUE
                            } else {
                                Color::TRANSPARENT
                            },
                            width: 1.5,
                            radius: 5.0.into(),
                        },
                        ..Default::default()
                    })
                    .on_press(Message::Select(tile.asset.id)),
            );
        }
        let mut tabs = Row::new().spacing(8);
        for (tab, label) in ["基本信息", "EXIF 信息", "AI 识别", "相似推荐"]
            .iter()
            .enumerate()
        {
            let active = self.detail_tab == tab as u8;
            tabs = tabs.push(
                button(
                    column![
                        text(*label)
                            .size(11)
                            .color(if active { TEXT } else { MUTED }),
                        container(Space::new().height(2))
                            .width(Length::Fill)
                            .style(move |_| container::Style {
                                background: Some(
                                    if active { BLUE } else { Color::TRANSPARENT }.into()
                                ),
                                ..Default::default()
                            })
                    ]
                    .spacing(9),
                )
                .padding(0)
                .width(Length::Fill)
                .style(flat)
                .on_press(Message::DetailTab(tab as u8)),
            );
        }
        let mut info = Column::new().spacing(12);
        if self.detail_tab <= 1 {
            let demo = self.launch.design_preview;
            for (kind, label, value) in [
                (Icon::Raw, "文件类型", asset.format.to_uppercase()),
                (Icon::Tag, "文件大小", size(asset.bytes)),
                (
                    Icon::Image,
                    "分辨率",
                    if demo {
                        "6240 × 4160".into()
                    } else {
                        format!("{} × {}", asset.width, asset.height)
                    },
                ),
                (
                    Icon::Camera,
                    if demo { "拍摄时间" } else { "导入时间" },
                    if demo {
                        "2024年5月18日 17:24:36".into()
                    } else {
                        date(asset.imported_at)
                    },
                ),
                (
                    Icon::Camera,
                    "相机型号",
                    if demo { "Sony ILCE-7M4" } else { "—" }.into(),
                ),
                (
                    Icon::Eye,
                    "镜头",
                    if demo {
                        "FE 24–70mm F2.8 GM II"
                    } else {
                        "—"
                    }
                    .into(),
                ),
                (Icon::Eye, "光圈", if demo { "f/5.6" } else { "—" }.into()),
                (
                    Icon::Clock,
                    "快门",
                    if demo { "1/1250" } else { "—" }.into(),
                ),
                (Icon::Camera, "ISO", if demo { "100" } else { "—" }.into()),
                (Icon::Eye, "焦距", if demo { "48 mm" } else { "—" }.into()),
            ] {
                info = info.push(metadata(kind, label, value));
            }
            if !demo {
                info = Column::new().spacing(12);
                for (label, value) in [
                    ("文件类型", asset.format.to_uppercase()),
                    ("文件大小", size(asset.bytes)),
                    ("分辨率", format!("{} × {}", asset.width, asset.height)),
                    ("导入时间", date(asset.imported_at)),
                ] {
                    info = info.push(metadata(Icon::Image, label, value));
                }
                if self.detail_tab == 1 {
                    info = Column::new().spacing(12);
                    if self.manager.details.exif.is_empty() {
                        info = info.push(muted("此版本没有可读取的 EXIF 信息"));
                    }
                    for (label, value) in &self.manager.details.exif {
                        info = info.push(metadata(Icon::Camera, label, value.clone()));
                    }
                }
                let mut ratings = Row::new().spacing(5);
                for rating in 1..=5 {
                    ratings = ratings.push(
                        button(icon(
                            Icon::Star,
                            19.0,
                            if rating <= asset.rating {
                                Color::from_rgb8(246, 174, 50)
                            } else {
                                MUTED
                            },
                        ))
                        .padding(1)
                        .style(flat)
                        .on_press(Message::Rating(
                            if rating == asset.rating { 0 } else { rating },
                        )),
                    );
                }
                if self.detail_tab == 0 {
                    info = info.push(ratings);
                }
            }
        } else if self.detail_tab == 2 {
            info = info.push(self.ai_details());
        } else {
            info = info.push(self.ai_recommendations());
        }
        let mut tag_section = column![
            row![
                detail_heading(Icon::Tag, "标签"),
                Space::new().width(Length::Fill),
                icon_button(Icon::Plus, Message::ToggleTagEditor),
                button(
                    text(if self.manager.tags_collapsed {
                        "展开"
                    } else {
                        "折叠"
                    })
                    .size(11)
                )
                .padding([5, 6])
                .style(flat)
                .on_press(Message::Manage(ManageMessage::ToggleTags))
            ]
            .spacing(4)
            .align_y(Alignment::Center)
        ]
        .spacing(10);
        if asset.tags.is_empty() {
            tag_section = tag_section.push(muted("尚未添加标签"));
        }
        for tag in asset.tags.iter().filter(|_| !self.manager.tags_collapsed) {
            tag_section = tag_section.push(
                button(chip(tag, BLUE))
                    .padding(0)
                    .style(flat)
                    .on_press(Message::Manage(ManageMessage::Tag(tag.clone()))),
            );
        }
        if self.editing_tags {
            tag_section = tag_section
                .push(
                    text_input("用逗号分隔标签", &self.tag_input)
                        .on_input(Message::Tags)
                        .on_submit(Message::SaveTags)
                        .padding(10)
                        .size(12)
                        .style(input),
                )
                .push(
                    form_action(Icon::Save, "保存标签", Message::SaveTags, true)
                        .width(Length::Fill),
                );
        }
        let mut location_section = column![
            row![
                detail_heading(Icon::Pin, "地点"),
                Space::new().width(Length::Fill),
                icon_button(Icon::Plus, Message::Manage(ManageMessage::Location))
            ]
            .align_y(Alignment::Center)
        ]
        .spacing(10);
        if let Some(location) = self.manager.details.manual_location.as_ref().or(self
            .manager
            .details
            .exif_location
            .as_ref())
        {
            location_section = location_section
                .push(muted(if self.manager.details.manual_location.is_some() {
                    "用户补充地点"
                } else {
                    "原片 GPS"
                }))
                .push(
                    text(if location.label.is_empty() {
                        format!("{:.6}, {:.6}", location.latitude, location.longitude)
                    } else {
                        format!(
                            "{} · {:.6}, {:.6}",
                            location.label, location.latitude, location.longitude
                        )
                    })
                    .size(12),
                )
                .push(
                    form_action(
                        Icon::External,
                        "打开地图",
                        Message::Manage(ManageMessage::OpenMap),
                        false,
                    )
                    .width(Length::Fill),
                );
        } else {
            location_section = location_section.push(muted("未读取到 GPS，可补充地点。"));
        }
        let mut projects = column![
            row![
                detail_heading(Icon::Folder, "项目"),
                Space::new().width(Length::Fill),
                button(text("管理").size(12).color(BLUE))
                    .padding([5, 6])
                    .style(flat)
                    .on_press(Message::Manage(ManageMessage::AssetProjects))
            ]
            .align_y(Alignment::Center)
        ]
        .spacing(12);
        let mut project_count = 0;
        for project in self
            .library
            .as_ref()
            .into_iter()
            .flat_map(|l| l.info.projects.iter())
            .filter(|p| self.launch.design_preview || self.manager.details.projects.contains(&p.id))
        {
            project_count += 1;
            projects = projects.push(
                button(
                    row![
                        photo(
                            self.manager.project_covers.get(&project.id),
                            Length::Fixed(40.0),
                            36.0
                        ),
                        column![heading(&project.name, 12), muted("查看项目素材")]
                            .spacing(4)
                            .width(Length::Fill),
                        icon(Icon::Right, 14.0, ICON)
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                )
                .padding(4)
                .width(Length::Fill)
                .style(flat)
                .on_press(Message::Project(project.id)),
            );
        }
        if project_count == 0 {
            projects = projects.push(muted("尚未加入项目"));
        }
        let mut details = column![
            cover,
            row![
                heading(&asset.name, 15).width(Length::Fill),
                icon_button(Icon::Edit, Message::ShowRenameAsset)
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            row![
                chip(if video { "视频" } else { "照片" }, BLUE),
                chip(
                    if self.manager.details.referenced {
                        "外部引用 · 备份不含原片"
                    } else {
                        "托管原片"
                    },
                    MUTED
                )
            ]
            .spacing(7),
            muted(format!(
                "{}  |  {}",
                date(asset.imported_at),
                size(asset.bytes)
            )),
            strip,
            row![
                button(text("上一个").size(11))
                    .padding(5)
                    .style(flat)
                    .on_press_maybe(
                        self.neighbor_position
                            .filter(|(p, _)| *p > 0)
                            .map(|_| Message::Browse(-1))
                    ),
                Space::new().width(Length::Fill),
                muted(
                    self.neighbor_position
                        .map(|(p, total)| format!("{} / {}", p + 1, total))
                        .unwrap_or_default()
                ),
                Space::new().width(Length::Fill),
                button(text("下一个").size(11))
                    .padding(5)
                    .style(flat)
                    .on_press_maybe(
                        self.neighbor_position
                            .filter(|(p, total)| (*p as u64) + 1 < *total)
                            .map(|_| Message::Browse(1))
                    )
            ]
            .align_y(Alignment::Center),
            tabs,
            detail_card(info),
            detail_card(tag_section),
            detail_card(location_section),
            detail_card(projects)
        ]
        .spacing(14);
        if !self.launch.design_preview {
            let mut versions = column![detail_heading(Icon::Collection, "版本")].spacing(10);
            for (index, version) in self.manager.details.versions.iter().enumerate() {
                versions = versions.push(
                    action(
                        &format!(
                            "{}V{} · {}",
                            if version.id == asset.version_id {
                                "✓ "
                            } else {
                                ""
                            },
                            index + 1,
                            version.origin
                        ),
                        Message::Manage(ManageMessage::Version(version.id)),
                    )
                    .width(Length::Fill),
                );
            }
            if !video {
                versions = versions.push(
                    form_action(
                        Icon::Network,
                        "发送到 Synco 精修",
                        Message::Manage(ManageMessage::SendSynco),
                        false,
                    )
                    .width(Length::Fill),
                );
            }
            details = details
                .push(detail_card(versions))
                .push(detail_card(
                    column![
                        detail_heading(Icon::Text, "备注"),
                        text_input("记录用途、地点或拍摄说明", &self.manager.notes)
                            .on_input(|s| Message::Manage(ManageMessage::Notes(s)))
                            .padding(10)
                            .size(12)
                            .style(input),
                        form_action(
                            Icon::Save,
                            "保存备注",
                            Message::Manage(ManageMessage::SaveNotes),
                            true
                        )
                        .width(Length::Fill)
                        .on_press_maybe(
                            self.manager
                                .notes_dirty
                                .then_some(Message::Manage(ManageMessage::SaveNotes))
                        )
                    ]
                    .spacing(12),
                ))
                .push(detail_card(
                    column![
                        detail_heading(Icon::Settings, "素材操作"),
                        form_action(
                            Icon::Folder,
                            "编辑归属项目",
                            Message::Manage(ManageMessage::Organize),
                            false
                        )
                        .width(Length::Fill),
                        form_action(
                            Icon::Download,
                            "导出素材",
                            Message::Manage(ManageMessage::ExportMenu),
                            false
                        )
                        .width(Length::Fill),
                    ]
                    .spacing(10),
                ));
            if !video {
                details = details.push(
                    form_action(
                        Icon::Search,
                        "查找视觉近似",
                        Message::Manage(ManageMessage::Similar),
                        false,
                    )
                    .width(Length::Fill),
                );
            }
            details = details.push(
                form_action(
                    Icon::Trash,
                    if self.query.trash {
                        "恢复素材"
                    } else {
                        "移入回收站"
                    },
                    Message::Manage(ManageMessage::Edit(Edit::Trash(!self.query.trash))),
                    false,
                )
                .width(Length::Fill),
            );
        }
        if self.query.trash {
            details = details.push(
                action("永久删除素材…", Message::Manage(ManageMessage::Purge))
                    .style(destructive)
                    .width(Length::Fill),
            );
        }
        if self.manager.details.referenced {
            details = details.push(
                form_action(
                    Icon::Folder,
                    "重新定位外部原片",
                    Message::Manage(ManageMessage::RelocateReference),
                    false,
                )
                .width(Length::Fill),
            );
        }
        details.width(Length::Fill).into()
    }
    fn large_preview(&self) -> Element<'_, Message> {
        let video = self
            .selected
            .as_ref()
            .is_some_and(|a| moriq_app::is_video_format(&a.format));
        let handle = if video {
            self.video_frame
                .as_ref()
                .map(|a| a.handle())
                .or(self.preview.as_ref())
        } else {
            self.preview.as_ref()
        };
        let art: Element<'_, Message> = if let Some(handle) = handle {
            if !self.launch.design_preview
                && let Some(asset) = &self.selected
                && !moriq_app::is_video_format(&asset.format)
            {
                let (width, height) = self.original_size.unwrap_or((asset.width, asset.height));
                crate::region_viewer::Viewer {
                    zoom_request: self.regions.zoom_request,
                    target_scale: self.regions.target_scale,
                    key: asset.version_id,
                    handle: handle.clone(),
                    width,
                    height,
                    tiles: self.regions.images(),
                }
                .into()
            } else {
                image(handle.clone())
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .content_fit(ContentFit::Contain)
                    .into()
            }
        } else {
            Space::new().into()
        };
        let background: Element<'_, Message> = if let Some(handle) = &self.preview_background {
            image(handle.clone())
                .width(Length::Fill)
                .height(Length::Fill)
                .content_fit(ContentFit::Cover)
                .into()
        } else {
            Space::new().width(Length::Fill).height(Length::Fill).into()
        };
        let art: Element<'_, Message> = if video {
            container(art)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|_| container::Style {
                    background: Some(Color::BLACK.into()),
                    ..Default::default()
                })
                .into()
        } else {
            stack![
                background,
                container(Space::new())
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(|_| container::Style {
                        background: Some(Color::from_rgba(1.0, 1.0, 1.0, 0.60).into()),
                        ..Default::default()
                    }),
                art
            ]
            .into()
        };
        let content = column![
            row![
                container(heading(
                    self.selected
                        .as_ref()
                        .map(|a| a.name.as_str())
                        .unwrap_or_default(),
                    17
                ))
                .width(Length::Fill),
                form_action(Icon::Left, "上一张", Message::Browse(-1), false).on_press_maybe(
                    self.neighbor_position
                        .filter(|(p, _)| *p > 0)
                        .map(|_| Message::Browse(-1))
                ),
                muted(
                    self.neighbor_position
                        .map(|(p, total)| format!("{} / {}", p + 1, total))
                        .unwrap_or_default()
                ),
                form_action(Icon::Right, "下一张", Message::Browse(1), false).on_press_maybe(
                    self.neighbor_position
                        .filter(|(p, total)| (*p as u64) + 1 < *total)
                        .map(|_| Message::Browse(1))
                ),
                action("返回图库 · Esc", Message::ExpandPreview)
            ]
            .spacing(20)
            .align_y(Alignment::Center),
            row![
                muted(
                    match self
                        .original_size
                        .or_else(|| self.selected.as_ref().map(|a| (a.width, a.height)))
                    {
                        Some((w, h)) if video => format!("视频 {w} × {h}"),
                        Some((w, h)) => format!("原尺寸 {w} × {h} · 滚轮缩放 · 拖动平移"),
                        None => "预览".into(),
                    }
                ),
                muted(if self.regions.is_loading() {
                    "正在读取原尺寸区域…"
                } else if self.preview_task.is_some() {
                    "正在读取清晰预览…"
                } else {
                    ""
                }),
                Space::new().width(Length::Fill),
                button(text("100%").size(13))
                    .padding([8, 14])
                    .style(secondary)
                    .on_press_maybe((!video).then_some(Message::Regions(RegionMessage::Actual))),
                button(text("适应窗口").size(13))
                    .padding([8, 14])
                    .style(secondary)
                    .on_press_maybe((!video).then_some(Message::Regions(RegionMessage::Fit)))
            ]
            .spacing(12)
            .align_y(Alignment::Center),
            art
        ];
        let content = if video {
            let (position, duration) = self
                .playback
                .as_ref()
                .map(|p| p.progress())
                .unwrap_or_default();
            let progress = if duration == 0 {
                0.
            } else {
                position as f32 / duration as f32
            };
            content.push(
                row![
                    action(
                        if self.video_paused {
                            "播放"
                        } else {
                            "暂停"
                        },
                        Message::VideoPause
                    )
                    .on_press_maybe(self.playback.as_ref().map(|_| Message::VideoPause)),
                    action(
                        if self.video_muted {
                            "取消静音"
                        } else {
                            "静音"
                        },
                        Message::VideoMute
                    )
                    .on_press_maybe(self.playback.as_ref().map(|_| Message::VideoMute)),
                    muted(format!(
                        "{}:{:02} / {}:{:02}",
                        position / 60000,
                        position / 1000 % 60,
                        duration / 60000,
                        duration / 1000 % 60
                    )),
                    slider(0.0..=1.0, progress.clamp(0., 1.), Message::VideoSeek)
                        .step(0.001_f32)
                        .width(Length::Fill)
                ]
                .spacing(12)
                .align_y(Alignment::Center),
            )
        } else {
            content
        };
        content.padding(22).spacing(16).height(Length::Fill).into()
    }
}
