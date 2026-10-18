use super::*;
use crate::components::*;
use crate::icons::{Icon, icon};
use crate::theme::*;
use iced::widget::{
    Column, Row, Space, button, column, container, progress_bar, row, stack, text, text_input,
};
use iced::{Alignment, Border, Color, Length};
impl App {
    pub(super) fn models(&self) -> Element<'_, Message> {
        let categories = [
            ("人像", "category-portrait", 1842, Icon::Person),
            ("风景", "category-landscape", 2163, Icon::Landscape),
            ("建筑", "category-building", 892, Icon::Building),
            ("插画", "category-illustration", 420, Icon::Sparkles),
            ("截图", "category-screen", 680, Icon::Raw),
            ("未分类", "category-other", 316, Icon::More),
        ];
        let mut category_rows = Column::new().spacing(13);
        for chunk in categories.chunks(3) {
            let mut cards = Row::new().spacing(16);
            for &(label, key, count, kind) in chunk {
                let cover = stack![
                    container(photo(self.artwork.get(key), Length::Fill, 96.0))
                        .height(110)
                        .width(Length::Fill),
                    container(column![
                        Space::new().height(78),
                        row![
                            Space::new().width(14),
                            container(icon(kind, 18.0, TEXT))
                                .center_x(32)
                                .center_y(32)
                                .style(|_| container::Style {
                                    background: Some(Color::WHITE.into()),
                                    border: Border {
                                        radius: 16.0.into(),
                                        ..Default::default()
                                    },
                                    ..Default::default()
                                }),
                            Space::new().width(Length::Fill)
                        ]
                    ])
                    .height(110)
                    .width(Length::Fill)
                ];
                cards = cards.push(
                    button(
                        column![
                            cover,
                            container(
                                row![
                                    column![
                                        heading(label, 14),
                                        muted(if self.launch.design_preview {
                                            format!("{} 张照片", number(count))
                                        } else {
                                            "尚未分析".into()
                                        })
                                    ]
                                    .spacing(4)
                                    .width(Length::Fill),
                                    icon(Icon::Right, 15.0, ICON)
                                ]
                                .align_y(Alignment::Center)
                            )
                            .padding([10, 16])
                        ]
                        .spacing(0),
                    )
                    .padding(0)
                    .width(Length::Fill)
                    .style(secondary)
                    .on_press_maybe(self.launch.design_preview.then_some(if label == "未分类" {
                        Message::AllAssets
                    } else {
                        Message::Search(label.into())
                    })),
                );
            }
            category_rows = category_rows.push(cards);
        }
        let organization = container(
            column![
                row![
                    icon(Icon::Network, 34.0, BLUE),
                    column![
                        row![
                            heading("智能整理", 22),
                            chip(
                                if self.launch.design_preview {
                                    "AI 驱动"
                                } else {
                                    "本地 AI · 未启用"
                                },
                                BLUE
                            )
                        ]
                        .spacing(12)
                        .align_y(Alignment::Center),
                        muted("安装并启用可信模型后可分析图片内容；当前分类与模型安装尚未开放。")
                    ]
                    .spacing(6)
                    .width(Length::Fill),
                    button(
                        row![
                            icon(Icon::Refresh, 17.0, BLUE),
                            text("重新整理").size(13).color(BLUE)
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center)
                    )
                    .padding([10, 14])
                    .style(secondary)
                    .on_press_maybe(self.launch.design_preview.then_some(Message::ModelTab(1))),
                    button(
                        row![
                            text("按内容类型分组").size(13),
                            icon(Icon::Down, 14.0, ICON)
                        ]
                        .spacing(17)
                        .align_y(Alignment::Center)
                    )
                    .padding([10, 14])
                    .style(secondary)
                    .on_press_maybe(self.launch.design_preview.then_some(Message::ModelTab(0)))
                ]
                .spacing(15)
                .align_y(Alignment::Center),
                category_rows
            ]
            .spacing(24),
        )
        .padding(iced::Padding {
            top: 20.0,
            right: 20.0,
            bottom: 12.0,
            left: 20.0,
        })
        .style(panel);
        let mut models = Column::new().spacing(10);
        let mut any_model = false;
        let registry = [
            (
                Icon::Settings,
                "Chinese-CLIP",
                "图像内容理解与分类，支持中文语义检索。",
                "v1.2.0 · 1.8 GB",
                BLUE,
            ),
            (
                Icon::Text,
                "OCR 文字识别",
                "识别照片中的文字，支持中英日韩等多语言。",
                "v1.1.0 · 820 MB",
                Color::from_rgb8(94, 64, 255),
            ),
            (
                Icon::Eye,
                "视觉相似模型",
                "基于向量相似度的图像相似搜索。",
                "v1.0.0 · 1.6 GB",
                BLUE,
            ),
            (
                Icon::Person,
                "人像识别",
                "识别人物、面部特征与相似人物聚类。",
                "v1.0.0 · 900 MB",
                BLUE,
            ),
            (
                Icon::Image,
                "场景与物体识别",
                "识别照片中的场景、物体、活动等内容。",
                "v1.0.0 · 1.1 GB",
                BLUE,
            ),
            (
                Icon::Sparkles,
                "图像描述生成",
                "生成照片的自然语言描述，便于搜索与管理。",
                "v1.0.0 · 1.3 GB",
                Color::from_rgb8(96, 73, 255),
            ),
        ];
        for (row_index, chunk) in registry.chunks(3).enumerate() {
            let mut cards = Row::new().spacing(11);
            for (column_index, &(kind, name, description, version, color)) in
                chunk.iter().enumerate()
            {
                let index = row_index * 3 + column_index;
                if (!self.launch.design_preview && self.model_filter < 2)
                    || (!self.model_search.is_empty()
                        && !name
                            .to_lowercase()
                            .contains(&self.model_search.to_lowercase())
                        && !description.contains(&self.model_search))
                {
                    cards = cards.push(Space::new().width(Length::Fill));
                    continue;
                }
                any_model = true;
                let installed = self.launch.design_preview && index < 2;
                let downloading = self.launch.design_preview && index == 2;
                let badge = if installed {
                    chip("已安装", Color::from_rgb8(70, 162, 103))
                } else if downloading {
                    chip("下载中 68%", BLUE)
                } else {
                    chip("未安装", MUTED)
                };
                let actions: Element<'_, Message> = if downloading {
                    row![
                        progress_bar(0.0..=1.0, 0.68)
                            .girth(5)
                            .style(theme::progress)
                            .length(Length::Fill),
                        icon_button(
                            Icon::Pause,
                            Message::Notice("设计预览不会创建真实下载任务。".into())
                        ),
                        icon_button(
                            Icon::Close,
                            Message::Notice("设计预览不会创建真实下载任务。".into())
                        )
                    ]
                    .spacing(7)
                    .align_y(Alignment::Center)
                    .into()
                } else {
                    row![
                        muted(if self.launch.design_preview {
                            version
                        } else {
                            "模型包尚未提供"
                        }),
                        Space::new().width(Length::Fill),
                        button(if installed {
                            row![text("卸载").size(12)]
                        } else {
                            row![
                                icon(
                                    Icon::Download,
                                    14.0,
                                    if self.launch.design_preview {
                                        Color::WHITE
                                    } else {
                                        MUTED
                                    }
                                ),
                                text(if self.launch.design_preview {
                                    "安装"
                                } else {
                                    "暂不可用"
                                })
                                .size(12)
                                .color(
                                    if self.launch.design_preview {
                                        Color::WHITE
                                    } else {
                                        MUTED
                                    }
                                )
                            ]
                            .spacing(5)
                            .align_y(Alignment::Center)
                        })
                        .padding([7, 13])
                        .style(move |theme, status| if installed {
                            secondary(theme, status)
                        } else {
                            primary(theme, status)
                        })
                        .on_press_maybe(
                            self.launch.design_preview.then_some(Message::Notice(
                                if self.launch.design_preview {
                                    "这是设计预览，不会安装或卸载真实模型。"
                                } else {
                                    "当前没有经过验证的可安装模型包。"
                                }
                                .into()
                            ))
                        )
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center)
                    .into()
                };
                cards = cards.push(
                    container(
                        column![
                            row![
                                container(icon(kind, 27.0, color))
                                    .padding(8)
                                    .style(move |_| container::Style {
                                        background: Some(Color { a: 0.06, ..color }.into()),
                                        border: Border {
                                            radius: 10.0.into(),
                                            ..Default::default()
                                        },
                                        ..Default::default()
                                    }),
                                column![
                                    row![
                                        heading(name, 13),
                                        Space::new().width(Length::Fill),
                                        badge
                                    ]
                                    .spacing(5)
                                    .align_y(Alignment::Center),
                                    text(description).size(11).color(MUTED)
                                ]
                                .spacing(8)
                                .width(Length::Fill)
                            ]
                            .spacing(9),
                            actions
                        ]
                        .spacing(12),
                    )
                    .padding(11)
                    .width(Length::Fill)
                    .style(panel),
                );
            }
            models = models.push(cards);
        }
        if !any_model {
            models = Column::new().push(
                container(muted(if self.model_search.is_empty() {
                    "当前没有此类模型包"
                } else {
                    "没有找到匹配的模型"
                }))
                .center_x(Length::Fill)
                .center_y(130)
                .style(pale),
            );
        }
        let mut tabs = Row::new().spacing(10);
        for (index, label) in if self.launch.design_preview {
            ["已安装模型 ③", "可用模型 ⑥", "全部模型 ⑨"]
        } else {
            ["已安装模型 0", "可用模型 0", "全部模型 6"]
        }
        .iter()
        .enumerate()
        {
            tabs = tabs.push(
                button(text(*label).size(12))
                    .padding([7, 15])
                    .style(if self.model_filter == index as u8 {
                        primary
                    } else {
                        flat
                    })
                    .on_press(Message::ModelFilter(index as u8)),
            );
        }
        let model_management = container(
            column![
                row![
                    icon(Icon::Cube, 34.0, BLUE),
                    column![
                        heading("AI 模型管理", 21),
                        muted(
                            "管理本地 AI 模型，按需安装，支持图像识别、相似搜索、文字识别等能力。"
                        )
                    ]
                    .spacing(6)
                ]
                .spacing(15)
                .align_y(Alignment::Center),
                row![
                    tabs,
                    Space::new().width(Length::Fill),
                    container(
                        row![
                            icon(Icon::Search, 15.0, MUTED),
                            text_input("搜索模型…", &self.model_search)
                                .on_input(Message::ModelSearch)
                                .padding(0)
                                .size(12)
                                .style(|theme, status| {
                                    let mut style = input(theme, status);
                                    style.border.width = 0.0;
                                    style
                                })
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center)
                    )
                    .padding([8, 12])
                    .width(210)
                    .style(panel)
                ]
                .align_y(Alignment::Center),
                models
            ]
            .spacing(12),
        )
        .padding(15)
        .width(Length::Fill)
        .style(panel);
        let settings = container(
            column![
                row![icon(Icon::Settings, 19.0, ICON), heading("模型设置", 14)]
                    .spacing(12)
                    .align_y(Alignment::Center),
                self.model_setting(
                    Icon::Package,
                    "按需下载模型",
                    "仅在你主动安装时下载模型",
                    self.on_demand_models,
                    Message::ToggleModelPolicy
                ),
                self.model_setting(
                    Icon::Clock,
                    "卸载模型后保留索引",
                    "保留已生成的索引与整理结果",
                    self.retain_indexes,
                    Message::ToggleRetainIndexes
                ),
                separator(),
                row![
                    icon(Icon::Refresh, 18.0, ICON),
                    heading("后台索引任务", 13),
                    Space::new().width(Length::Fill),
                    button(text("全部任务").size(11).color(BLUE))
                        .padding(0)
                        .style(flat)
                        .on_press(Message::Manage(ManageMessage::Tasks))
                ]
                .spacing(10)
                .align_y(Alignment::Center),
                row![
                    container(ring(
                        if self.launch.design_preview {
                            "36%".into()
                        } else {
                            "0%".into()
                        },
                        "",
                        if self.launch.design_preview {
                            0.36
                        } else {
                            0.0
                        },
                        72.0
                    ))
                    .width(72)
                    .height(72),
                    column![
                        heading(
                            if self.launch.design_preview {
                                "正在分析照片内容…"
                            } else {
                                "当前没有索引任务"
                            },
                            11
                        ),
                        progress_bar(
                            0.0..=1.0,
                            if self.launch.design_preview {
                                0.44
                            } else {
                                0.0
                            }
                        )
                        .girth(4)
                        .style(theme::progress),
                        text(if self.launch.design_preview {
                            "已处理 2,480 / 6,892 张   预计剩余 12 分钟"
                        } else {
                            "安装模型后可主动开始分析"
                        })
                        .size(10)
                        .color(MUTED)
                    ]
                    .spacing(9)
                    .width(Length::Fill)
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                heading(
                    if self.launch.design_preview {
                        "当前任务 (2)"
                    } else {
                        "当前任务 (0)"
                    },
                    11
                ),
                muted(if self.launch.design_preview {
                    "风景照片特征提取        分析中…"
                } else {
                    "暂无后台任务"
                })
            ]
            .spacing(17),
        )
        .padding(18)
        .width(336)
        .style(panel);
        scroll(
            column![
                column![
                    heading("AI 智能整理与模型管理", 26),
                    text("基于 AI 理解你的照片，自动整理、搜索与更多可能。")
                        .size(14)
                        .color(MUTED)
                ]
                .spacing(6),
                organization,
                row![model_management, settings]
                    .spacing(14)
                    .align_y(Alignment::Start)
            ]
            .spacing(16)
            .padding([20, 24]),
        )
        .id("models")
        .on_scroll(|v| {
            Message::PageScrolled(
                2,
                v.absolute_offset().y,
                (v.content_bounds().height - v.bounds().height).max(0.0),
            )
        })
        .height(Length::Fill)
        .into()
    }
    fn model_setting<'a>(
        &self,
        kind: Icon,
        title: &str,
        description: &str,
        on: bool,
        message: Message,
    ) -> Element<'a, Message> {
        row![
            icon(kind, 18.0, ICON),
            column![
                heading(title, 12),
                text(description.to_string()).size(10).color(MUTED)
            ]
            .spacing(4)
            .width(Length::Fill),
            button(
                container(Space::new().width(15).height(15)).style(|_| container::Style {
                    background: Some(Color::WHITE.into()),
                    border: Border {
                        radius: 8.0.into(),
                        ..Default::default()
                    },
                    ..Default::default()
                })
            )
            .padding(if on {
                iced::Padding {
                    top: 3.0,
                    right: 3.0,
                    bottom: 3.0,
                    left: 20.0,
                }
            } else {
                iced::Padding {
                    top: 3.0,
                    right: 20.0,
                    bottom: 3.0,
                    left: 3.0,
                }
            })
            .style(move |_, status| button::Style {
                background: Some(
                    if matches!(status, button::Status::Disabled) {
                        LINE
                    } else if on {
                        BLUE
                    } else {
                        LINE
                    }
                    .into()
                ),
                border: Border {
                    radius: 12.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .on_press_maybe(self.launch.design_preview.then_some(message))
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .into()
    }
}
fn number(value: u64) -> String {
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
