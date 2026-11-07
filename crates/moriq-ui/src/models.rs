use super::*;
use crate::components::grouped_number as number;
use crate::components::*;
use crate::icons::{Icon, icon};
use crate::theme::*;
use iced::widget::{
    Column, Row, Space, button, column, container, progress_bar, row, text, text_input,
};
use iced::{Alignment, Border, Length};
pub(super) const MODEL_FILTER_ORDER: [u8; 3] = [2, 1, 0];
impl App {
    /// 构建模型管理页面及当前标签、搜索与筛选结果。
    pub(super) fn models(&self) -> Element<'_, Message> {
        let language = self.preferences.language;
        let can_organize = self.library.is_some()
            && (self.query.project.is_some()
                || self
                    .library
                    .as_ref()
                    .is_some_and(|l| l.info.projects.is_empty()))
            && !self.switching
            && self
                .ai
                .models
                .iter()
                .any(|model| model.installed && model.spec.id == "qwen35-4b");
        let categories = self
            .manager
            .organizer
            .ai_groups
            .iter()
            .filter(|group| group.count > 0)
            .collect::<Vec<_>>();
        let mut category_rows = Column::new().spacing(13);
        let category_columns = if self.viewport.width >= 1280.0 {
            3
        } else if self.viewport.width >= 900.0 {
            2
        } else {
            1
        };
        for chunk in categories.chunks(category_columns) {
            let mut cards = Row::new().spacing(16);
            for group in chunk {
                let label = self.group_label(group);
                let kind = if group.id == "_faces_analyzed" {
                    Icon::Person
                } else {
                    crate::categories::category_icon(&group.id)
                };
                let count = group.count;
                cards = cards.push(
                    button(
                        column![
                            if self.manager.category_covers.contains_key(&group.id) {
                                photo(self.manager.category_covers.get(&group.id), Length::Fill, 160.0)
                            } else {
                                Element::from(container(icon(kind, 24.0, BLUE)).padding([16, 16]))
                            },
                            container(
                                row![
                                    icon(kind, 18.0, BLUE),
                                    column![
                                        heading(label.clone(), 14),
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
                                .spacing(10)
                                .align_y(Alignment::Center)
                            )
                            .padding([12, 16])
                        ]
                        .spacing(0),
                    )
                    .padding(0)
                    .width(Length::Fill)
                    .style(secondary)
                    .on_press_maybe(
                        self.library
                            .as_ref()
                            .map(|_| Message::Ai(AiMessage::Group(group.id.clone()))),
                    ),
                );
            }
            // Keep the last row aligned to the grid even when only one group exists.
            for _ in chunk.len()..category_columns {
                cards = cards.push(Space::new().width(Length::Fill));
            }
            category_rows = category_rows.push(cards);
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
        if categories.is_empty() {
            category_rows = category_rows.push(
                container(column![
                    icon(Icon::Network, 28.0, BLUE),
                    heading(tr!(language, "分类将在这里呈现" => "Your categories will appear here"), 16),
                    muted(tr!(language, "整理素材后，按主题浏览照片；也可以先设置分类模板。" => "Organize your assets to browse by theme, or set up your categories first.")),
                ].spacing(12)).padding(24).width(Length::Fill).style(pale)
            );
        }
        let mut spaces = vec![manage::ProjectDestination {
            id: None,
            name: tr!(language,"全部素材 · 总览"=>"All assets · Overview").into(),
        }];
        if let Some(library) = &self.library {
            spaces.extend(
                library
                    .info
                    .projects
                    .iter()
                    .map(|p| manage::ProjectDestination {
                        id: Some(p.id),
                        name: manage::project_label(&library.info.projects, p.id),
                    }),
            );
        }
        let selected_space = spaces.iter().find(|p| p.id == self.query.project).cloned();
        let space_picker = row![
            heading(tr!(language,"整理空间"=>"Workspace"), 13),
            choice(spaces, selected_space, |p| Message::Ai(
                AiMessage::Workspace(p.id)
            ))
            .width(Length::Fill)
        ]
        .spacing(12)
        .align_y(Alignment::Center);
        let category_action = Message::Ai(AiMessage::Categories(
            crate::categories::CategoryMessage::Open,
        ));
        let mut organization = column![
                space_picker,
                row![
                    icon(Icon::Network, 34.0, BLUE),
                    column![
                        row![
                            heading(tr!(language, "智能整理" => "Smart organization"), 22),
                            chip(
                                if self.ai.models.iter().any(|model| model.installed) {
                                    tr!(language, "本地 AI" => "Local AI")
                                } else {
                                    tr!(language, "按需安装模型" => "Install models on demand")
                                },
                                BLUE
                            )
                        ]
                        .spacing(12)
                        .align_y(Alignment::Center),
                        muted(tr!(language, "自动分类、添加标签，让照片更容易找到。" => "Organize and tag your photos so they are easier to find."))
                    ]
                    .spacing(6)
                    .width(Length::Fill),
                    row![
                        form_action(Icon::Refresh, tr!(language, "重新分析" => "Analyze again"), Message::Ai(AiMessage::ChooseAnalysis), false)
                            .on_press_maybe(
                                (self.library.is_some() && !self.switching && self.ai.models.iter().any(|m|m.installed))
                                    .then_some(Message::Ai(AiMessage::ChooseAnalysis))
                            ),
                        form_action(Icon::Settings, tr!(language, "分类设置" => "Category settings"), category_action.clone(), false)
                            .on_press_maybe(self.library.as_ref().map(|_| category_action.clone())),
                        form_action(Icon::Person, tr!(language, "人脸分组" => "Group faces"), Message::Ai(AiMessage::OrganizeFaces), false)
                            .on_press_maybe((self.library.is_some() && !self.switching && self.face_model_ready()).then_some(Message::Ai(AiMessage::OrganizeFaces))),
                        form_action(Icon::Network, tr!(language, "人物管理" => "Manage people"), Message::Ai(AiMessage::People), false)
                            .on_press_maybe(self.library.as_ref().filter(|_| !self.switching).map(|_| Message::Ai(AiMessage::People))),
                    ].spacing(10).wrap()

                ]
                .spacing(15)
                .align_y(Alignment::Center),
            ].spacing(20);
        if self.manager.organizer.ai_outdated > 0 {
            organization = organization.push(muted(trf!(language,
                "{} 项旧分类使用了不同的识别定义，可重新分析更新。" => "{} classifications use older definitions. Analyze again to update.",
                self.manager.organizer.ai_outdated)));
        }
        if self
            .manager
            .organizer
            .jobs
            .iter()
            .any(|job| job.kind.model().is_some() && job.state.is_active())
        {
            organization = organization.push(self.model_job_controls());
        }
        let organization = container(
            organization
                .push(self.people_overview())
                .push(category_rows),
        )
        .padding(20)
        .width(Length::Fill)
        .style(panel);
        // Size the catalogue from the space left after navigation and the settings rail.
        let page_width = (self.viewport.width - self.sidebar_width() - 80.0).min(1376.0);
        let side_by_side = page_width >= 1040.0;
        let catalogue_width = if side_by_side {
            page_width - 364.0
        } else {
            page_width
        };
        let columns = if catalogue_width >= 780.0 && self.ai.models.len() > 1 {
            2
        } else {
            1
        };
        let visible: Vec<_> = self
            .ai
            .models
            .iter()
            .filter(|model| self.model_visible(model))
            .collect();
        let mut models = Column::new().spacing(16);
        for chunk in visible.chunks(columns) {
            let mut cards = Row::new().spacing(16);
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
        for index in MODEL_FILTER_ORDER {
            let (label, count) = match index {
                0 => (tr!(language,"已安装"=>"Installed"), installed),
                1 => (
                    tr!(language,"待安装"=>"Not installed"),
                    self.ai.models.len() - installed,
                ),
                _ => (tr!(language,"全部模型"=>"All models"), self.ai.models.len()),
            };
            tabs = tabs.push(
                button(text(format!("{label} {count}")).size(12))
                    .padding([7, 10])
                    .style(liquid_nav(self.model_filter == index))
                    .on_press(Message::ModelFilter(index)),
            );
        }
        let tabs = crate::liquid::selection(
            tabs.wrap(),
            MODEL_FILTER_ORDER
                .iter()
                .position(|id| *id == self.model_filter),
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
        let model_toolbar: Element<'_, Message> = if catalogue_width >= 650.0 {
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
                    column![
                        heading(tr!(language, "本地模型" => "Local models"), 20),
                        muted(
                            tr!(language, "按需安装，在本机完成照片分类与人物整理。" => "Install what you need to organize photos and people locally.")
                        )
                    ]
                    .spacing(6)
                    .width(Length::Fill),
                    icon_button(Icon::Refresh, Message::Ai(AiMessage::Refresh))
                        .on_press_maybe((!self.ai.loading).then_some(Message::Ai(AiMessage::Refresh)))
                ]
                .spacing(15)
                .align_y(Alignment::Center),
                model_toolbar,
                models
            ]
            .spacing(18),
        )
        .width(Length::Fill);
        let selected_tab = self.studio_tab;
        let tabs = row![
            action(
                tr!(language, "智能整理" => "Organize"),
                Message::StudioTab(0)
            )
            .style(liquid_nav(selected_tab == 0)),
            action(
                tr!(language, "模型与设置" => "Models & settings"),
                Message::StudioTab(1)
            )
            .style(liquid_nav(selected_tab == 1)),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        let content: Element<'_, Message> = if selected_tab == 1 {
            if side_by_side {
                row![
                    model_management,
                    container(self.model_settings()).width(340)
                ]
                .spacing(24)
                .into()
            } else {
                column![model_management, self.model_settings()]
                    .spacing(20)
                    .into()
            }
        } else {
            let mut content = column![].spacing(16);
            if installed == 0 {
                content = content.push(container(row![
                    muted(tr!(language, "安装本地模型，开启智能整理。" => "Install a local model to start organizing.")).width(Length::Fill),
                    action(tr!(language, "设置模型" => "Set up model"), Message::StudioTab(1)),
                ].spacing(12).align_y(Alignment::Center)).padding(14).style(pale));
            } else if self.library.is_some() && self.query.project.is_none() && !can_organize {
                content = content.push(muted(tr!(language, "选择一个项目，开始整理其中的素材。" => "Choose a project to organize its assets.")));
            }
            content.push(organization).into()
        };
        scroll(
            container(column![
                column![
                    heading(tr!(language, "AI 工作室" => "AI studio"), 28),
                    muted(tr!(language, "让整理更轻松，让灵感更好找。" => "Less organizing. More discovering."))
                ].spacing(8),
                row![
                    crate::liquid::selection(
                        tabs, Some(usize::from(selected_tab)),
                        crate::liquid::Treatment::Selection, self.preferences.reduced_motion,
                    ),
                    Space::new().width(Length::Fill),
                    form_action(Icon::Clock, tr!(language, "后台任务" => "Background tasks"), Message::Manage(ManageMessage::Tasks), false)
                        .style(flat),
                ].align_y(Alignment::Center),
                content,
            ].spacing(24)).max_width(1440).padding([28, 32]).width(Length::Fill),
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
    /// 在整理页直接展示当前项目的人物分组。
    fn people_overview(&self) -> Element<'_, Message> {
        let language = self.preferences.language;
        let people: Vec<_> = self
            .manager
            .organizer
            .people
            .iter()
            .filter(|p| p.count > 0)
            .take(24)
            .collect();
        let mut content = column![
            row![
                heading(tr!(language, "人物分组" => "People groups"), 16),
                Space::new().width(Length::Fill),
                action(
                    tr!(language, "人物管理" => "Manage people"),
                    Message::Ai(AiMessage::People)
                )
                .on_press_maybe(
                    self.library
                        .as_ref()
                        .map(|_| Message::Ai(AiMessage::People))
                )
            ]
            .align_y(Alignment::Center),
        ]
        .spacing(12);
        if people.is_empty() {
            content = content.push(muted(tr!(language, "暂无人物分组，点击“人脸分组”后，识别到的清晰人脸会显示在这里。" => "No people groups yet. Choose Group faces to find clear faces and display them here.")));
        } else {
            let columns = if self.viewport.width >= 1280.0 { 4 } else { 2 };
            for chunk in people.chunks(columns) {
                let mut cards = Row::new().spacing(12);
                for person in chunk {
                    cards = cards.push(self.person_card(person, false));
                }
                for _ in chunk.len()..columns {
                    cards = cards.push(Space::new().width(Length::Fill));
                }
                content = content.push(cards);
            }
        }
        content.into()
    }
    pub(super) fn face_model_ready(&self) -> bool {
        self.ai
            .models
            .iter()
            .any(|model| model.spec.id == "faces" && model.installed)
    }
    /// 判断模型是否满足当前安装状态和文本筛选。
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
    /// 构建模型说明、下载进度和安装卸载操作卡片。
    fn model_card<'a>(&'a self, model: &'a moriq_app::ModelInfo) -> Element<'a, Message> {
        let language = self.preferences.language;
        let id = &model.spec.id;
        let downloading = self.ai.downloads.get(id);
        let kind = if id == "faces" {
            Icon::Person
        } else {
            Icon::Sparkles
        };
        let badge = if downloading.is_some() {
            chip(tr!(language, "下载中" => "Downloading"), BLUE)
        } else if model.installed && id == "faces" {
            chip(tr!(language, "内置" => "Bundled"), GREEN)
        } else if model.installed {
            chip(tr!(language, "已安装" => "Installed"), GREEN)
        } else {
            chip(tr!(language, "未安装" => "Not installed"), MUTED)
        };
        let mut content = column![
            row![
                form_icon(kind, false),
                muted(if id == "faces" {
                    tr!(language, "人物整理 · CPU" => "People · CPU")
                } else {
                    tr!(language, "图像理解" => "Image understanding")
                })
                .width(Length::Fill),
                badge,
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            column![
                heading(model.spec.name.clone(), 19),
                // Reserve two lines so metadata and actions align across the catalogue.
                container(muted(model_description(language, id))).height(40),
            ]
            .spacing(8),
            row![
                muted(format!("{:.0} MiB", model.bytes as f64 / 1048576.,)),
                Space::new().width(Length::Fill),
                button(
                    row![
                        text(model.spec.license.clone())
                            .size(11)
                            .style(text_color(MUTED)),
                        icon(Icon::External, 12.0, MUTED),
                    ]
                    .spacing(5)
                    .align_y(Alignment::Center)
                )
                .on_press(Message::Manage(crate::manage::ManageMessage::Licenses))
                .padding([4, 0])
                .style(flat),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
            separator(),
        ]
        .spacing(16);
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
            let mut actions = Row::new().spacing(10);
            if id == "faces" {
                actions = actions
                    .push(
                        form_action(
                            Icon::Person,
                            tr!(language, "人脸分组" => "Group faces"),
                            Message::Ai(AiMessage::OrganizeFaces),
                            true,
                        )
                        .on_press_maybe(
                            (self.library.is_some() && !self.switching)
                                .then_some(Message::Ai(AiMessage::OrganizeFaces)),
                        ),
                    )
                    .push(
                        form_action(
                            Icon::Network,
                            tr!(language, "人物管理" => "Manage people"),
                            Message::Ai(AiMessage::People),
                            false,
                        )
                        .on_press_maybe(
                            self.library
                                .as_ref()
                                .map(|_| Message::Ai(AiMessage::People)),
                        ),
                    );
            } else {
                actions = actions
                    .push(
                        form_action(
                            Icon::Refresh,
                            tr!(language, "重新分析" => "Analyze again"),
                            Message::Ai(AiMessage::Organize),
                            true,
                        )
                        .on_press_maybe(self.library.as_ref().and_then(|_| qwen_queue_message(id))),
                    )
                    .push(
                        form_action(
                            Icon::Trash,
                            tr!(language, "卸载" => "Uninstall"),
                            Message::Ai(AiMessage::Uninstall(id.clone())),
                            false,
                        )
                        .style(flat),
                    );
            }
            content = content.push(actions.wrap());
        } else {
            content = content.push(form_action(
                Icon::Download,
                if model.resumable {
                    tr!(language, "继续安装" => "Resume installation")
                } else {
                    tr!(language, "安装" => "Install")
                },
                Message::Ai(AiMessage::Install(id.clone())),
                true,
            ));
        }
        iced::widget::tooltip(
            container(content)
                .padding(24)
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
    /// 将偏好、后台任务和运行环境分组展示，保持各自的信息层级。
    fn model_settings(&self) -> Element<'_, Message> {
        let language = self.preferences.language;
        let preferences = container(column![
            detail_heading(Icon::Settings, tr!(language, "模型设置" => "Model settings")),
            self.model_setting(
                Icon::Package,
                tr!(language, "国内下载加速" => "Download mirror"),
                if self.preferences.ai.use_mirror {
                    tr!(language, "HF-Mirror · hf-mirror.com" => "HF-Mirror · hf-mirror.com")
                } else {
                    tr!(language, "Hugging Face · huggingface.co" => "Hugging Face · huggingface.co")
                },
                self.preferences.ai.use_mirror,
                Message::ToggleModelMirror,
            ),
            self.model_setting(
                Icon::Cube,
                tr!(language, "按需加载模型" => "Load models on demand"),
                tr!(language, "闲置时释放，下次使用自动加载" => "Release when idle; reload when needed"),
                self.on_demand_models,
                Message::ToggleModelPolicy,
            ),
            self.model_setting(
                Icon::Clock,
                tr!(language, "卸载后保留索引" => "Keep indexes after uninstalling"),
                tr!(language, "保留已有的索引与整理结果" => "Preserve indexes and organization results"),
                self.retain_indexes,
                Message::ToggleRetainIndexes,
            ),
            separator(),
            muted(tr!(language, "下载源切换后，下次安装或继续下载时生效。" => "Source changes apply to the next install or resumed download.")).size(11),
        ].spacing(20)).padding(20).width(Length::Fill).style(panel);

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
        let mut tasks = column![
            row![
                detail_heading(
                    Icon::Refresh,
                    tr!(language, "后台任务" => "Background tasks")
                ),
                Space::new().width(Length::Fill),
                button(
                    text(tr!(language, "查看全部" => "View all"))
                        .size(11)
                        .style(text_color(BLUE))
                )
                .padding([5, 0])
                .style(flat)
                .on_press(Message::Manage(ManageMessage::Tasks)),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        ]
        .spacing(16);
        if self.jobs.error.is_some() {
            tasks = tasks.push(muted(tr!(language, "任务信息暂时无法读取，请在全部任务中查看。" => "Task information is unavailable. Check All tasks.")));
        } else if let Some(job) = current {
            let ratio = if job.total == 0 {
                0.0
            } else {
                (job.completed as f64 / job.total as f64).clamp(0.0, 1.0) as f32
            };
            let color = if job.state == JobState::Running {
                BLUE
            } else {
                MUTED
            };
            tasks = tasks
                .push(row![
                    chip(job_state(language, job.state), color),
                    Space::new().width(Length::Fill),
                    heading(if job.total == 0 { "—".into() } else { format!("{:.0}%", ratio * 100.0) }, 20),
                ].align_y(Alignment::Center))
                .push(heading(job_title(language, job.kind), 13))
                .push(progress_bar(0.0..=1.0, ratio).girth(6).style(theme::progress))
                .push(muted(trf!(language, "已处理 {} / {} 张" => "Processed {} / {} images", number(job.completed), number(job.total))).size(11));
            if job.failed > 0 {
                tasks = tasks.push(muted(trf!(language, "{} 张处理失败，可在全部任务中查看" => "{} images failed. See All tasks for details", number(job.failed))).size(11));
            }
            // The primary task is already expanded above; list only the remaining queue.
            for other in jobs.iter().filter(|other| other.id != job.id).take(2) {
                tasks = tasks.push(
                    row![
                        muted(job_title(language, other.kind))
                            .size(11)
                            .width(Length::Fill),
                        chip(job_state(language, other.state), MUTED),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                );
            }
            if jobs.len() > 1 {
                tasks = tasks.push(
                    muted(
                        trf!(language, "共 {} 项未完成任务" => "{} unfinished tasks", jobs.len()),
                    )
                    .size(11),
                );
            }
        } else {
            tasks = tasks.push(container(row![
                form_icon(Icon::Clock, false),
                column![
                    heading(tr!(language, "暂无后台任务" => "No background tasks"), 13),
                    muted(tr!(language, "开始整理后，在这里查看进度。" => "Start organizing to see progress here.")).size(11),
                ].spacing(6).width(Length::Fill),
            ].spacing(12).align_y(Alignment::Center)).padding([8, 0]));
        }
        let mut rail = column![
            preferences,
            container(tasks)
                .padding(20)
                .width(Length::Fill)
                .style(panel),
        ]
        .spacing(16)
        .width(Length::Fill);
        if let Some(hardware) = &self.ai.hardware {
            rail = rail.push(container(column![
                detail_heading(Icon::Cube, tr!(language, "运行环境" => "Runtime")),
                muted(hardware.provider.clone()).size(11),
                row![
                    muted(trf!(language, "{} 核 CPU" => "{} CPU cores", hardware.logical_cpus)).size(11),
                    Space::new().width(Length::Fill),
                    muted(trf!(language, "可用内存 {:.1} GiB" => "{:.1} GiB available", hardware.available_mib as f64 / 1024.0)).size(11),
                ].spacing(8),
            ].spacing(12)).padding(20).width(Length::Fill).style(form_card_style));
        }
        rail.into()
    }
    /// 构建单项模型设置行。
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
                heading(title, 13),
                text(description.to_string())
                    .size(11)
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

/// 取得指定模型的本地化用途说明。
fn model_description(language: Language, id: &str) -> &'static str {
    match id {
        "faces" => {
            tr!(language, "内置 CPU 人脸检测与特征匹配，离线按人物整理照片。" => "Bundled CPU face detection and feature matching for offline people grouping.")
        }
        "qwen35-4b" => {
            tr!(language, "本地生成图片分类、标签与描述。" => "Generate image categories, tags and descriptions locally.")
        }
        _ => tr!(language, "在本机分析图片内容。" => "Analyze image content locally."),
    }
}
/// 根据 Qwen 模型状态生成排队入口或提示消息。
fn qwen_queue_message(id: &str) -> Option<Message> {
    if id == "qwen35-4b" {
        Some(Message::Ai(AiMessage::Organize))
    } else if id == "faces" {
        Some(Message::Ai(AiMessage::OrganizeFaces))
    } else {
        None
    }
}
/// 把任务种类转换为当前语言的任务标题。
fn job_title(language: Language, kind: JobKind) -> &'static str {
    match kind {
        JobKind::Analysis(ModelId::ChineseClip) => {
            tr!(language, "内容分类与语义索引" => "Content categories and semantic index")
        }
        JobKind::Analysis(ModelId::Florence) => {
            tr!(language, "文字识别与图像描述" => "Text recognition and image descriptions")
        }
        JobKind::Analysis(ModelId::Faces) => {
            tr!(language, "CPU 人脸检测与分组" => "CPU face detection and grouping")
        }
        JobKind::QwenClassification => {
            tr!(language, "Qwen 媒介与主题分类" => "Qwen medium and theme classification")
        }
        JobKind::QwenDescription => {
            tr!(language, "Qwen 中文标签与描述" => "Qwen Chinese tags and descriptions")
        }
        JobKind::QwenFaces => {
            tr!(language,"Qwen 人脸识别与人物分组"=>"Qwen face recognition and people grouping")
        }
        JobKind::Fingerprints => tr!(language, "近似图片索引" => "Image similarity index"),
        JobKind::Thumbnails => tr!(language, "重建图片缩略图" => "Rebuild image thumbnails"),
        _ => tr!(language, "图片元数据索引" => "Image metadata index"),
    }
}
/// 把持久化任务状态转换为展示文本。
fn job_state(language: Language, state: JobState) -> &'static str {
    match state {
        JobState::Running => tr!(language, "分析中…" => "Analyzing…"),
        JobState::Queued => tr!(language, "等待中" => "Queued"),
        JobState::Paused => tr!(language, "已暂停" => "Paused"),

        _ => tr!(language, "等待恢复" => "Waiting to resume"),
    }
}
