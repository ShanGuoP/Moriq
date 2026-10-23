use super::*;
use crate::components::grouped_number as number;
use crate::components::*;
use crate::icons::{Icon, icon};
use crate::theme::*;
use iced::widget::{
    Column, Row, Space, button, column, container, progress_bar, row, stack, text, text_input,
};
use iced::{Alignment, Border, Color, Length};
impl App {
    pub(super) fn models(&self) -> Element<'_, Message> {
        let language = self.preferences.language;
        let can_organize = self.library.is_some()
            && !self.switching
            && self.ai.models.iter().any(|model| model.installed);
        let categories = [
            ("人像", Icon::Person),
            ("风景", Icon::Landscape),
            ("建筑", Icon::Building),
            ("插画", Icon::Sparkles),
            ("截图", Icon::Raw),
            ("未分类", Icon::More),
        ];
        let mut category_rows = Column::new().spacing(13);
        for chunk in categories.chunks(3) {
            let mut cards = Row::new().spacing(16);
            for &(label, kind) in chunk {
                let count = self.category_count(label);
                let cover = stack![
                    container(photo(
                        self.manager.category_covers.get(label),
                        Length::Fill,
                        96.0
                    ))
                    .height(110)
                    .width(Length::Fill),
                    container(column![
                        Space::new().height(78),
                        row![
                            Space::new().width(14),
                            container(icon(kind, 18.0, TEXT))
                                .center_x(32)
                                .center_y(32)
                                .style(|theme| container::Style {
                                    background: Some(
                                        crate::theme::adaptive(theme, Color::WHITE).into()
                                    ),
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
                                        heading(crate::i18n::category(language, label), 14),
                                        muted(if self.library.is_some() {
                                            trf!(language, "{} 张素材" => "{} assets", number(count))
                                        } else {
                                            tr!(language, "选择资源库后查看" => "Select a library to view").into()
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
                    .on_press_maybe(
                        self.library
                            .as_ref()
                            .map(|_| Message::Ai(AiMessage::Category(label.into()))),
                    ),
                );
            }
            category_rows = category_rows.push(cards);
        }
        let mut more_categories = Row::new().spacing(8);
        let mut has_more_categories = false;
        for (label, count) in &self.manager.organizer.ai_categories {
            if !categories.iter().any(|(name, _)| name == label) {
                has_more_categories = true;
                more_categories = more_categories.push(action(
                    &format!("{label} · {count}"),
                    Message::Ai(AiMessage::Category(label.clone())),
                ));
            }
        }
        if has_more_categories {
            category_rows = category_rows.push(more_categories.wrap());
        }
        if self
            .manager
            .organizer
            .errors
            .iter()
            .any(|(section, _)| *section == OrganizerSection::Categories)
        {
            category_rows =
                category_rows.push(muted(tr!(language, "分类信息暂时无法读取，请重新打开资源库重试。" => "Categories are temporarily unavailable. Reopen the library to retry.")));
        }
        let organization = container(
            column![
                row![
                    icon(Icon::Network, 34.0, BLUE),
                    column![
                        row![
                            heading(tr!(language, "智能整理" => "Smart organization"), 22),
                            chip(
                                if can_organize {
                                    tr!(language, "本地 AI" => "Local AI")
                                } else {
                                    tr!(language, "按需安装模型" => "Install models on demand")
                                },
                                BLUE
                            )
                        ]
                        .spacing(12)
                        .align_y(Alignment::Center),
                        muted(tr!(language, "在本机理解照片内容，自动分类、识别文字并建立相似搜索索引。" => "Understand photos locally: categorize content, recognize text and build similarity indexes."))
                    ]
                    .spacing(6)
                    .width(Length::Fill),
                    button(
                        row![
                            icon(Icon::Refresh, 17.0, BLUE),
                            text(tr!(language, "重新整理" => "Organize again")).size(13).style(crate::theme::text_color(BLUE))
                        ]
                        .spacing(8)
                        .align_y(Alignment::Center)
                    )
                    .padding([10, 14])
                    .style(secondary)
                    .on_press_maybe(can_organize.then_some(Message::Ai(AiMessage::Organize))),
                    self.model_grouping()
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
        let columns = if self.viewport.width >= 1480.0 { 3 } else { 2 };
        let visible: Vec<_> = self
            .ai
            .models
            .iter()
            .filter(|model| self.model_visible(model))
            .collect();
        let mut models = Column::new().spacing(11);
        for chunk in visible.chunks(columns) {
            let mut cards = Row::new().spacing(11);
            for model in chunk {
                cards = cards.push(self.model_card(model));
            }
            for _ in chunk.len()..columns {
                cards = cards.push(Space::new().width(Length::Fill));
            }
            models = models.push(cards);
        }
        if visible.is_empty() {
            models = models.push(
                container(muted(if self.ai.loading {
                    tr!(language, "正在读取模型…" => "Loading models…")
                } else if self.ai.models.is_empty() {
                    tr!(language, "模型列表尚未就绪，请刷新重试" => "Model list is not ready. Refresh and try again.")
                } else if self.model_search.is_empty() {
                    tr!(language, "当前没有此类模型，可切换到全部模型安装" => "No models in this category. Switch to All models to install one.")
                } else {
                    tr!(language, "没有找到匹配的模型" => "No matching models")
                }))
                .center_x(Length::Fill)
                .center_y(130)
                .style(pale),
            );
        }
        let installed = self
            .ai
            .models
            .iter()
            .filter(|model| model.installed)
            .count();
        let mut tabs = Row::new().spacing(6);
        for (index, (label, count)) in [
            (tr!(language, "已安装模型" => "Installed models"), installed),
            (
                tr!(language, "可用模型" => "Available models"),
                self.ai.models.len() - installed,
            ),
            (
                tr!(language, "全部模型" => "All models"),
                self.ai.models.len(),
            ),
        ]
        .into_iter()
        .enumerate()
        {
            tabs = tabs.push(
                button(text(format!("{label} {count}")).size(12))
                    .padding([7, 10])
                    .style(liquid_nav(self.model_filter == index as u8))
                    .on_press(Message::ModelFilter(index as u8)),
            );
        }
        let tabs = crate::liquid::selection(
            tabs.wrap(),
            Some(usize::from(self.model_filter)),
            crate::liquid::Treatment::Selection,
            self.preferences.reduced_motion,
        );
        let search = container(
            row![
                icon(Icon::Search, 15.0, MUTED),
                text_input(
                    tr!(language, "搜索模型…" => "Search models…"),
                    &self.model_search
                )
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
            .align_y(Alignment::Center),
        )
        .padding([8, 12])
        .width(210)
        .style(panel);
        let model_toolbar: Element<'_, Message> = if self.viewport.width >= 1400.0 {
            row![tabs, Space::new().width(Length::Fill), search]
                .spacing(8)
                .align_y(Alignment::Center)
                .into()
        } else {
            column![tabs, search].spacing(10).into()
        };
        let model_management = container(
            column![
                row![
                    icon(Icon::Cube, 34.0, BLUE),
                    column![
                        heading(tr!(language, "AI 模型管理" => "AI model management"), 21),
                        muted(
                            tr!(language, "管理本地 AI 模型，按需安装，支持图像识别、相似搜索、文字识别等能力。" => "Manage local AI models. Install image recognition, similarity search and OCR capabilities as needed.")
                        )
                    ]
                    .spacing(6)
                ]
                .spacing(15)
                .align_y(Alignment::Center),
                model_toolbar,
                models
            ]
            .spacing(12),
        )
        .padding(15)
        .width(Length::Fill)
        .style(panel);
        let settings = self.model_settings();
        scroll(
            column![
                column![
                    heading(tr!(language, "AI 智能整理与模型管理" => "AI organization and models"), 26),
                    text(tr!(language, "基于 AI 理解你的照片，自动整理、搜索与更多可能。" => "Understand, organize and search your photos with AI."))
                        .size(14)
                        .style(crate::theme::text_color(MUTED))
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
    fn model_grouping(&self) -> Element<'_, Message> {
        let language = self.preferences.language;
        if self.library.is_none() {
            button(
                text(tr!(language, "选择资源库后分组" => "Select a library to group assets"))
                    .size(13),
            )
            .padding([8, 12])
            .style(secondary)
            .into()
        } else {
            crate::components::localized_choice(
                language,
                vec!["按内容类型分组", "人物管理与纠正"],
                Some("按内容类型分组"),
                |value| {
                    if value == "人物管理与纠正" {
                        Message::Ai(AiMessage::People)
                    } else {
                        Message::Noop
                    }
                },
            )
            .into()
        }
    }
    pub(super) fn category_count(&self, label: &str) -> u64 {
        let categories = &self.manager.organizer.ai_categories;
        if label == "未分类" {
            let classified: u64 = categories
                .iter()
                .filter(|(name, _)| name != "未分类")
                .map(|(_, count)| *count)
                .sum();
            self.library
                .as_ref()
                .map_or(0, |library| library.info.count.saturating_sub(classified))
        } else {
            categories
                .iter()
                .find(|(name, _)| name == label)
                .map_or(0, |(_, count)| *count)
        }
    }
    fn model_visible(&self, model: &moriq_app::ModelInfo) -> bool {
        let language = self.preferences.language;
        (match self.model_filter {
            0 => model.installed,
            1 => !model.installed,
            _ => true,
        }) && format!(
            "{} {} {}",
            model.spec.name,
            model.spec.id,
            model_description(language, &model.spec.id)
        )
        .to_lowercase()
        .contains(&self.model_search.trim().to_lowercase())
    }
    fn model_card<'a>(&'a self, model: &'a moriq_app::ModelInfo) -> Element<'a, Message> {
        let language = self.preferences.language;
        let id = &model.spec.id;
        let downloading = self.ai.downloads.get(id);
        let kind = match id.as_str() {
            "florence" => Icon::Text,
            "faces" => Icon::Person,
            _ => Icon::Settings,
        };
        let color = if id == "florence" { PURPLE } else { BLUE };
        let badge = if downloading.is_some() {
            chip(tr!(language, "下载中" => "Downloading"), BLUE)
        } else if model.installed {
            chip(tr!(language, "已安装" => "Installed"), GREEN)
        } else {
            chip(tr!(language, "未安装" => "Not installed"), MUTED)
        };
        let mut content = column![
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
                        heading(model.spec.name.clone(), 13).width(Length::Fill),
                        badge
                    ]
                    .spacing(5),
                    text(model_description(language, id))
                        .size(11)
                        .style(crate::theme::text_color(MUTED))
                ]
                .spacing(8)
                .width(Length::Fill)
            ]
            .spacing(9),
            row![
                muted(format!(
                    "{:.0} MiB · {}",
                    model.bytes as f64 / 1048576.,
                    model.spec.license
                )),
                Space::new().width(Length::Fill),
                action(
                    tr!(language, "查看许可" => "View license"),
                    Message::Manage(crate::manage::ManageMessage::Licenses)
                )
            ]
            .spacing(6),
        ]
        .spacing(12);
        if let Some(control) = downloading {
            let done = control.downloaded.load(Ordering::Relaxed);
            let total = control.total.load(Ordering::Relaxed);
            let ratio = if total == 0 {
                0.
            } else {
                (done as f64 / total as f64).min(1.) as f32
            };
            content = content
                .push(
                    progress_bar(0.0..=1.0, ratio)
                        .girth(5)
                        .style(theme::progress),
                )
                .push(
                    row![
                        muted(format!(
                            "{:.1} / {:.1} MiB",
                            done as f64 / 1048576.,
                            total as f64 / 1048576.
                        ))
                        .width(Length::Fill),
                        button(
                            row![
                                icon(Icon::Pause, 13.0, ICON),
                                text(if control.stop.load(Ordering::Relaxed) {
                                    tr!(language, "暂停中…" => "Pausing…")
                                } else {
                                    tr!(language, "暂停" => "Pause")
                                })
                                .size(12)
                            ]
                            .spacing(5)
                            .align_y(Alignment::Center)
                        )
                        .padding([7, 10])
                        .style(secondary)
                        .on_press_maybe(
                            (!control.stop.load(Ordering::Relaxed))
                                .then(|| Message::Ai(AiMessage::Pause(id.clone())))
                        )
                    ]
                    .spacing(5)
                    .align_y(Alignment::Center),
                );
        } else if model.installed {
            content = content.push(
                row![
                    button(text(tr!(language, "重新分析" => "Analyze again")).size(13))
                        .padding([8, 12])
                        .style(secondary)
                        .on_press_maybe(
                            self.library
                                .as_ref()
                                .and_then(|_| id.parse::<ModelId>().ok())
                                .map(|model| Message::Jobs(tasks::JobMessage::Queue(
                                    JobKind::Analysis(model)
                                )))
                        ),
                    action(
                        tr!(language, "卸载" => "Uninstall"),
                        Message::Ai(AiMessage::Uninstall(id.clone()))
                    )
                ]
                .spacing(7)
                .wrap(),
            );
        } else {
            content = content.push(row![
                Space::new().width(Length::Fill),
                action(
                    if model.resumable {
                        tr!(language, "继续安装" => "Resume installation")
                    } else {
                        tr!(language, "安装" => "Install")
                    },
                    Message::Ai(AiMessage::Install(id.clone()))
                )
                .style(primary)
            ]);
        }
        iced::widget::tooltip(
            container(content)
                .padding(11)
                .width(Length::Fill)
                .style(panel),
            container(
                column![
                    muted(trf!(language, "模型版本：{}" => "Model version: {}", model.spec.revision)),
                    muted(trf!(language, "建议内存：{} MiB" => "Recommended memory: {} MiB", model.spec.minimum_memory_mib)),
                    text(model.spec.source.clone())
                        .size(11)
                        .wrapping(text::Wrapping::Glyph),
                ]
                .spacing(6),
            )
            .padding(12)
            .width(340)
            .style(panel),
            iced::widget::tooltip::Position::Bottom,
        )
        .delay(Duration::from_millis(600))
        .snap_within_viewport(true)
        .into()
    }
    fn model_settings(&self) -> Element<'_, Message> {
        let language = self.preferences.language;
        let jobs: Vec<_> = self
            .manager
            .organizer
            .jobs
            .iter()
            .filter(|job| {
                (job.kind.model().is_some()
                    || matches!(
                        job.kind,
                        JobKind::Fingerprints | JobKind::Thumbnails | JobKind::Metadata
                    ))
                    && !job.state.is_finished()
            })
            .collect();
        let current = jobs
            .iter()
            .find(|job| job.state == JobState::Running)
            .or_else(|| jobs.first());
        let ratio = current.map_or(0., |job| {
            if job.total == 0 {
                0.
            } else {
                (job.completed as f64 / job.total as f64).min(1.) as f32
            }
        });
        let mut content = column![
            row![icon(Icon::Settings, 19.0, ICON), heading(tr!(language, "模型设置" => "Model settings"), 14)]
                .spacing(12)
                .align_y(Alignment::Center),
            self.model_setting(
                Icon::Package,
                tr!(language, "国内下载加速" => "Download mirror"),
                if self.preferences.ai.use_mirror {
                    tr!(language, "下载源：HF-Mirror · hf-mirror.com" => "Source: HF-Mirror · hf-mirror.com")
                } else {
                    tr!(language, "下载源：Hugging Face · huggingface.co" => "Source: Hugging Face · huggingface.co")
                },
                self.preferences.ai.use_mirror,
                Message::ToggleModelMirror
            ),
            muted(tr!(language, "切换源在下次安装或继续时生效，保留下载断点。" => "The new source applies to the next install or resume. Download progress is preserved.")),
            self.model_setting(
                Icon::Package,
                tr!(language, "按需加载模型" => "Load models on demand"),
                tr!(language, "闲置时释放模型，下次使用再加载" => "Release idle models and reload them when needed"),
                self.on_demand_models,
                Message::ToggleModelPolicy
            ),
            self.model_setting(
                Icon::Clock,
                tr!(language, "卸载模型后保留索引" => "Keep indexes after uninstalling models"),
                tr!(language, "保留已生成的索引与整理结果" => "Preserve generated indexes and organization results"),
                self.retain_indexes,
                Message::ToggleRetainIndexes
            ),
            separator(),
            row![
                icon(Icon::Refresh, 18.0, ICON),
                heading(tr!(language, "后台索引任务" => "Background indexing tasks"), 13),
                Space::new().width(Length::Fill),
                button(text(tr!(language, "全部任务" => "All tasks")).size(11).style(crate::theme::text_color(BLUE)))
                    .padding(0)
                    .style(flat)
                    .on_press(Message::Manage(ManageMessage::Tasks))
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            row![
                ring(
                    if current.is_some() {
                        format!("{:.0}%", ratio * 100.)
                    } else {
                        "—".into()
                    },
                    "",
                    ratio,
                    72.
                ),
                column![
                    heading(
                        current.map_or(tr!(language, "当前没有索引任务" => "No indexing tasks"), |job| job_title(language, job.kind)),
                        11
                    ),
                    progress_bar(0.0..=1.0, ratio)
                        .girth(4)
                        .style(theme::progress),
                    text(current.map_or_else(
                        || tr!(language, "安装模型后可主动开始分析" => "Install a model to start analysis").into(),
                        |job| trf!(language, "已处理 {} / {} 张 · 失败 {} 张" => "Processed {} / {} images · {} failed",
                            job.completed, job.total, job.failed
                        )
                    ))
                    .size(10)
                    .style(crate::theme::text_color(MUTED)),
                ]
                .spacing(9)
                .width(Length::Fill)
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            heading(trf!(language, "当前任务 ({})" => "Current tasks ({})", jobs.len()), 11),
        ]
        .spacing(17);
        if self.jobs.error.is_some() {
            content = content.push(muted(tr!(language, "任务信息暂时无法读取，请在全部任务中查看。" => "Task information is temporarily unavailable. Check All tasks.")));
        }
        for job in jobs.iter().take(3) {
            content = content.push(muted(format!(
                "{} · {}",
                job_title(language, job.kind),
                job_state(language, job.state)
            )));
        }
        if jobs.is_empty() {
            content = content.push(muted(
                tr!(language, "暂无后台任务" => "No background tasks"),
            ));
        }
        if let Some(hardware) = &self.ai.hardware {
            content = content.push(separator()).push(muted(trf!(language, "{} · {} 核 · 可用内存 {} MiB" => "{} · {} cores · {} MiB available memory",
                hardware.provider, hardware.logical_cpus, hardware.available_mib
            )));
        }
        content = content.push(action(
            tr!(language, "刷新模型列表" => "Refresh models"),
            Message::Ai(AiMessage::Refresh),
        ));
        container(content)
            .padding(18)
            .width(336)
            .style(panel)
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
                text(description.to_string())
                    .size(10)
                    .style(crate::theme::text_color(MUTED))
            ]
            .spacing(4)
            .width(Length::Fill),
            button(crate::liquid::selection(
                row![
                    Space::new().width(15).height(15),
                    Space::new().width(15).height(15),
                ]
                .spacing(2)
                .padding(3),
                Some(usize::from(on)),
                crate::liquid::Treatment::Thumb,
                self.preferences.reduced_motion,
            ))
            .padding(0)
            .style(move |theme, status| button::Style {
                background: Some(
                    if matches!(status, button::Status::Disabled) {
                        crate::theme::adaptive(theme, LINE)
                    } else if on {
                        crate::theme::adaptive(theme, BLUE)
                    } else {
                        crate::theme::adaptive(theme, LINE)
                    }
                    .into()
                ),
                border: Border {
                    radius: 12.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .on_press(message)
        ]
        .spacing(10)
        .align_y(Alignment::Center)
        .into()
    }
}

fn model_description(language: Language, id: &str) -> &'static str {
    match id {
        "chinese-clip" => {
            tr!(language, "图像内容分类、中文语义检索与相似搜索。" => "Image categorization, Chinese semantic search and similarity search.")
        }
        "florence" => {
            tr!(language, "识别照片中的文字，并生成图像描述。" => "Recognize text in photos and generate image descriptions.")
        }
        "faces" => {
            tr!(language, "人脸检测、特征提取与人物聚类。" => "Face detection, feature extraction and people clustering.")
        }
        _ => tr!(language, "在本机分析图片内容。" => "Analyze image content locally."),
    }
}
fn job_title(language: Language, kind: JobKind) -> &'static str {
    match kind {
        JobKind::Analysis(ModelId::ChineseClip) => {
            tr!(language, "内容分类与语义索引" => "Content categories and semantic index")
        }
        JobKind::Analysis(ModelId::Florence) => {
            tr!(language, "文字识别与图像描述" => "Text recognition and image descriptions")
        }
        JobKind::Analysis(ModelId::Faces) => {
            tr!(language, "人物识别与聚类" => "People recognition and clustering")
        }
        JobKind::Fingerprints => tr!(language, "近似图片索引" => "Image similarity index"),
        JobKind::Thumbnails => tr!(language, "重建图片缩略图" => "Rebuild image thumbnails"),
        _ => tr!(language, "图片元数据索引" => "Image metadata index"),
    }
}
fn job_state(language: Language, state: JobState) -> &'static str {
    match state {
        JobState::Running => tr!(language, "分析中…" => "Analyzing…"),
        JobState::Queued => tr!(language, "等待中" => "Queued"),
        JobState::Paused => tr!(language, "已暂停" => "Paused"),

        _ => tr!(language, "等待恢复" => "Waiting to resume"),
    }
}
