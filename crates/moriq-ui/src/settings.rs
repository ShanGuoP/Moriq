use super::*;
use crate::{components::*, icons::Icon, theme::*};
use iced::widget::{Space, button, checkbox, column, container, row, text};
use iced::{Alignment, Length};

#[derive(Default)]
pub(super) struct InterfaceState {
    // The current selection lives in UserSettings; this tracks its persistence.
    pub initialized: bool,
    pub saving: Option<(Language, Appearance)>,
    pub saved: (Language, Appearance),
    pub close_after_save: bool,
}

impl App {
    pub(super) fn set_reduced_motion(&mut self, enabled: bool) -> Task<Message> {
        self.preferences.reduced_motion = enabled;
        self.gallery_feedback.clear();
        self.surface_motion = None;
        let pending = self
            .transition
            .take()
            .and_then(|mut transition| transition.pending.take());
        let navigation = pending
            .map(|message| self.commit_navigation(*message))
            .unwrap_or_else(Task::none);
        let service = self.service.clone();
        Task::batch([
            navigation,
            Task::perform(
                async move { service.reduced_motion(enabled).await },
                Message::SettingsLoaded,
            ),
        ])
    }

    pub(super) fn set_language(&mut self, language: Language) -> Task<Message> {
        self.preferences.language = language;
        self.interface.initialized = true;
        self.refresh_people_choices();
        self.save_interface()
    }

    pub(super) fn set_appearance(&mut self, appearance: Appearance) -> Task<Message> {
        self.preferences.appearance = appearance;
        self.interface.initialized = true;
        self.save_interface()
    }

    pub(super) fn finish_interface_save(
        &mut self,
        language: Language,
        appearance: Appearance,
        result: Result<(), String>,
    ) -> Task<Message> {
        if self.interface.saving != Some((language, appearance)) {
            return Task::none();
        }
        self.interface.saving = None;
        let current = (self.preferences.language, self.preferences.appearance);
        match result {
            Ok(()) => self.interface.saved = (language, appearance),
            Err(error) => {
                self.error = Some(format!(
                    "{}: {error}",
                    tr!(self.preferences.language,
                    "无法保存界面设置" => "Could not save interface settings")
                ));
                if current == (language, appearance) {
                    (self.preferences.language, self.preferences.appearance) = self.interface.saved;
                    self.refresh_people_choices();
                    self.interface.close_after_save = false;
                    return Task::none();
                }
            }
        }
        if current != (language, appearance) {
            return self.save_interface();
        }
        if self.interface.close_after_save {
            self.interface.close_after_save = false;
            return self.update_inner(Message::WindowClose);
        }
        Task::none()
    }

    pub(super) fn set_diagnostic_logging(&mut self, enabled: bool) -> Task<Message> {
        self.preferences.diagnostic_logging = enabled;
        let service = self.service.clone();
        Task::perform(
            async move { service.diagnostic_logging(enabled).await },
            Message::SettingsLoaded,
        )
    }

    pub(super) fn apply_settings(&mut self, mut settings: UserSettings) {
        // Other preference receipts can arrive after an interface edit.
        if self.interface.initialized {
            settings.language = self.preferences.language;
            settings.appearance = self.preferences.appearance;
        } else {
            self.interface.saved = (settings.language, settings.appearance);
            self.interface.initialized = true;
        }
        self.preferences = settings;
    }

    pub(super) fn save_interface(&mut self) -> Task<Message> {
        if self.interface.saving.is_some() {
            return Task::none();
        }
        let language = self.preferences.language;
        let appearance = self.preferences.appearance;
        if (language, appearance) == self.interface.saved {
            return Task::none();
        }
        self.interface.saving = Some((language, appearance));
        let service = self.service.clone();
        Task::perform(
            async move { service.interface_preferences(language, appearance).await },
            move |result| Message::InterfaceSaved(language, appearance, result),
        )
    }

    fn import_settings(&self) -> Element<'_, Message> {
        let language = self.preferences.language;
        let workers = localized_choice(
            language,
            std::iter::once("自动".to_string())
                .chain((1..=32).map(|n| n.to_string()))
                .collect(),
            Some(if self.preferences.import.workers == 0 {
                "自动".into()
            } else {
                self.preferences.import.workers.to_string()
            }),
            |value| Message::ImportPreference(0, value),
        )
        .width(110);
        let memory = localized_choice(
            language,
            [256, 512, 1024, 2048, 4096]
                .into_iter()
                .map(|value| value.to_string())
                .collect(),
            Some(self.preferences.import.memory_mib.to_string()),
            |value| Message::ImportPreference(1, value),
        )
        .width(110);
        let performance = column![
            form_heading(
                Icon::Database,
                tr!(language, "导入性能" => "Import performance"),
                tr!(language, "调整导入时的并发与内存限制，以获得更稳定的处理体验。" => "Adjust concurrency and memory limits for stable import processing.")
            ),
            row![
                column![
                    heading(tr!(language, "同时处理上限" => "Concurrent images"), 14),
                    muted(tr!(language, "同时处理的图片数量，实际数量受内存与系统资源限制。" => "Maximum images processed at once, subject to memory and system resources."))
                ]
                .spacing(6)
                .width(Length::Fill),
                workers
            ]
            .spacing(16)
            .align_y(Alignment::Center),
            separator(),
            row![
                column![
                    heading(tr!(language, "图片处理内存 · MiB" => "Image processing memory · MiB"), 14),
                    muted(tr!(language, "用于图片分析与生成缩略图的内存上限。" => "Maximum memory for image analysis and thumbnail generation."))
                ]
                .spacing(6)
                .width(Length::Fill),
                memory
            ]
            .spacing(16)
            .align_y(Alignment::Center),
            container(
                column![
                    muted(trf!(language, "本机自动上限 {}。并发数是上限，实际数量受内存预算限制；保存后在下次导入生效。" => "Automatic limit on this computer: {}. Actual concurrency depends on the memory budget. Saved changes apply to the next import.", moriq_app::ImportControl::default().workers)),
                    muted(tr!(language, "新配置默认 2048 MiB；超高像素 RAW 仍可能超过解码器自身上限。" => "New settings default to 2048 MiB. Very high resolution RAW files may still exceed decoder limits."))
                ]
                .spacing(6)
            )
            .padding(14)
            .width(Length::Fill)
            .style(pale),
        ]
        .spacing(16);
        let performance = if let Some(job) = &self.import {
            performance.push(muted(trf!(language, "当前批次：最多 {} 张 / {} MiB；单张最高估算 {} MiB，解码峰值 {} 张，等待内存 {} 张。" => "Current batch: up to {} images / {} MiB; largest estimate {} MiB, peak decoding {}, waiting for memory {}.",
                job.workers, job.memory_mib, job.largest_mib.load(Ordering::Relaxed),
                job.peak.load(Ordering::Relaxed), job.waiting_memory.load(Ordering::Relaxed)
            )))
        } else {
            performance
        };
        performance
            .push(form_action(
                Icon::Save,
                tr!(language, "保存导入设置" => "Save import settings"),
                Message::SaveImportPreferences,
                true,
            ))
            .into()
    }

    pub(super) fn settings_form(&self) -> Element<'_, Message> {
        let language = self.preferences.language;
        let library_path = self
            .library
            .as_ref()
            .map(|l| display_path(&l.root))
            .unwrap_or_else(|| tr!(language, "尚未打开资源库" => "No library open").into());
        let model_path = moriq_app::model_directory()
            .map(|p| display_path(&p))
            .unwrap_or_else(|e| e);
        let content = column![
            preference_card(
                Icon::Globe,
                tr!(language, "界面语言" => "Interface language"),
                tr!(language, "选择应用界面语言，立即生效。" => "Choose the interface language. Changes apply immediately."),
                localized_choice(
                    language,
                    vec![Language::SimplifiedChinese, Language::English],
                    Some(self.preferences.language),
                    Message::LanguageChanged
                )
                .width(150)
            ),
            preference_card(
                Icon::Moon,
                tr!(language, "外观主题" => "Appearance"),
                tr!(language, "选择浅色或暗色主题，立即生效。" => "Choose a light or dark theme. Changes apply immediately."),
                crate::liquid::selection(row![
                    button(text(tr!(language, "浅色" => "Light")).size(13))
                        .padding([9, 12])
                        .style(liquid_nav(self.preferences.appearance == Appearance::Light))
                        .on_press(Message::AppearanceChanged(Appearance::Light)),
                    button(text(tr!(language, "暗色" => "Dark")).size(13))
                        .padding([9, 12])
                        .style(liquid_nav(self.preferences.appearance == Appearance::Dark))
                        .on_press(Message::AppearanceChanged(Appearance::Dark))
                ]
                .spacing(4),
                    Some(usize::from(self.preferences.appearance == Appearance::Dark)),
                    crate::liquid::Treatment::Selection,
                    self.preferences.reduced_motion
                )
            ),
            preference_card(
                Icon::Text,
                tr!(language, "诊断日志" => "Diagnostic logging"),
                tr!(language, "启用后会记录详细的运行日志，用于问题排查（重启后生效）。" => "Record detailed logs for troubleshooting. Takes effect after restarting."),
                checkbox(self.preferences.diagnostic_logging).on_toggle(Message::DiagnosticLogging)
            ),
            preference_card(
                Icon::Grid,
                tr!(language, "减少动态效果" => "Reduce motion"),
                tr!(language, "关闭页面转场和卡片过渡，保留清晰的选中反馈。" => "Disable page and card transitions while keeping selection feedback."),
                checkbox(self.preferences.reduced_motion).on_toggle(Message::ReducedMotion)
            ),
            form_card(self.import_settings()),
            form_card(
                column![
                    form_heading(
                        Icon::Folder,
                        tr!(language, "资源库位置" => "Library location"),
                        tr!(language, "管理素材文件的存储位置，包含照片、视频、元数据等内容。" => "Manage the storage location for photos, videos, metadata and other assets.")
                    ),
                    container(
                        row![
                            text(library_path).size(13).style(crate::theme::text_color(ICON)).width(Length::Fill),
                            icon_button(Icon::Copy, Message::CopyLibraryPath).on_press_maybe(
                                self.library.as_ref().map(|_| Message::CopyLibraryPath)
                            )
                        ]
                        .spacing(10)
                        .align_y(Alignment::Center)
                    )
                    .padding([4, 12])
                    .width(Length::Fill)
                    .style(crate::theme::form_card_style),
                    row![
                        form_action(Icon::Folder, tr!(language, "打开资源库" => "Open library"), Message::Open, false),
                        form_action(Icon::Plus, tr!(language, "新建资源库" => "New library"), Message::ShowCreate, false)
                    ]
                    .spacing(10)
                ]
                .spacing(14)
            ),
            form_card(
                column![
                    form_heading(
                        Icon::Cube,
                        tr!(language, "模型与索引" => "Models and indexes"),
                        tr!(language, "模型按需安装，可在 AI 模型页选择下载源、安装模型并管理索引任务。" => "Install models on demand. Choose download sources and manage models and indexing on the AI Models page.")
                    ),
                    container(
                        row![
                            text(model_path).size(13).style(crate::theme::text_color(ICON)).width(Length::Fill),
                            icon_button(Icon::Copy, Message::CopyModelsPath)
                        ]
                        .spacing(10)
                        .align_y(Alignment::Center)
                    )
                    .padding([4, 12])
                    .width(Length::Fill)
                    .style(crate::theme::form_card_style),
                    row![
                        form_action(Icon::Cube, tr!(language, "查看模型管理" => "Manage models"), Message::Navigate(2), false),
                        form_action(
                            Icon::Folder,
                            tr!(language, "打开模型文件夹" => "Open model folder"),
                            Message::OpenModelsDirectory,
                            false
                        )
                    ]
                    .spacing(10)
                ]
                .spacing(14)
            )
        ]
        .spacing(14);
        container(
            column![
                row![
                    form_icon(Icon::Settings, true),
                    column![
                        heading(tr!(language, "设置" => "Settings"), 24),
                        muted(tr!(language, "管理应用偏好、性能配置与本地资源" => "Manage preferences, performance and local resources"))
                    ]
                    .spacing(6)
                    .width(Length::Fill),
                    icon_button(Icon::Close, Message::HideCreate)
                ]
                .spacing(16)
                .align_y(Alignment::Center),
                container(
                    scroll(content)
                        .id("management")
                        .on_scroll(|v| Message::OverlayScrolled(
                            v.absolute_offset().y,
                            (v.content_bounds().height - v.bounds().height).max(0.0)
                        ))
                        .height(Length::Shrink)
                )
                .max_height((self.viewport.height - 300.0).clamp(100.0, 820.0)),
                separator(),
                row![
                    muted(concat!("Moriq Space  ", env!("CARGO_PKG_VERSION"))),
                    Space::new().width(Length::Fill),
                    button(text(tr!(language, "杉果派 · 项目主页" => "Shanguopai · Project home")).size(12).style(crate::theme::text_color(BLUE)))
                        .padding([4, 0])
                        .style(flat)
                        .on_press(Message::OpenSourceRepository)
                ]
                .spacing(12)
                .height(28)
                .align_y(Alignment::Center)
            ]
            .spacing(20),
        )
        .padding(28)
        .width((self.viewport.width - 40.0).clamp(260.0, 700.0))
        .max_height((self.viewport.height - 64.0).max(160.0))
        .style(crate::theme::modal)
        .into()
    }
}

fn preference_card<'a>(
    kind: Icon,
    title: &str,
    description: &str,
    control: impl Into<Element<'a, Message>>,
) -> Element<'a, Message> {
    form_card(
        row![form_heading(kind, title, description), control.into()]
            .spacing(16)
            .align_y(Alignment::Center),
    )
}
