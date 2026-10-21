use super::*;
use crate::{components::*, icons::Icon, theme::*};
use iced::widget::{Space, button, checkbox, column, container, row, text};
use iced::{Alignment, Length};

impl App {
    pub(super) fn settings_form(&self) -> Element<'_, Message> {
        let performance = column![
            form_heading(Icon::Database, "导入性能", "调整导入时的并发与内存限制，以获得更稳定的处理体验。"),
            row![
                column![heading("同时处理上限", 14), muted("同时处理的图片数量，实际数量受内存与系统资源限制。")].spacing(6).width(Length::Fill),
                choice(
                    std::iter::once("自动".to_string()).chain((1..=32).map(|n| n.to_string())).collect(),
                    Some(if self.preferences.import.workers == 0 { "自动".into() } else { self.preferences.import.workers.to_string() }),
                    |v| Message::ImportPreference(0, v)
                ).width(110)
            ].spacing(16).align_y(Alignment::Center),
            separator(),
            row![
                column![heading("图片处理内存 · MiB", 14), muted("用于图片分析与生成缩略图的内存上限。")].spacing(6).width(Length::Fill),
                choice(vec!["256".to_string(), "512".into(), "1024".into(), "2048".into(), "4096".into()],
                    Some(self.preferences.import.memory_mib.to_string()), |v| Message::ImportPreference(1, v)).width(110)
            ].spacing(16).align_y(Alignment::Center),
            container(column![
                muted(format!("本机自动上限 {}。并发数是上限，实际数量受内存预算限制；保存后在下次导入生效。", moriq_app::ImportControl::default().workers)),
                muted("新配置默认 2048 MiB；超高像素 RAW 仍可能超过解码器自身上限。")
            ].spacing(6)).padding(14).width(Length::Fill).style(pale),
        ].spacing(16);
        let performance = if let Some(job) = &self.import {
            performance.push(muted(format!(
                "当前批次：最多 {} 张 / {} MiB；单张最高估算 {} MiB，解码峰值 {} 张，等待内存 {} 张。",
                job.workers, job.memory_mib, job.largest_mib.load(Ordering::Relaxed),
                job.peak.load(Ordering::Relaxed), job.waiting_memory.load(Ordering::Relaxed)
            )))
        } else {
            performance
        };
        let performance = performance.push(form_action(
            Icon::Save,
            "保存导入设置",
            Message::SaveImportPreferences,
            true,
        ));
        let library_path = self
            .library
            .as_ref()
            .map(|l| display_path(&l.root))
            .unwrap_or_else(|| "尚未打开资源库".into());
        let model_path = moriq_app::model_directory()
            .map(|p| display_path(&p))
            .unwrap_or_else(|e| e);
        let content = column![
            form_card(
                row![
                    form_heading(Icon::Globe, "界面语言", "应用界面当前使用的语言。"),
                    container(text("简体中文").size(14))
                        .padding([10, 16])
                        .style(crate::theme::panel)
                ]
                .spacing(16)
                .align_y(Alignment::Center)
            ),
            form_card(
                row![
                    form_heading(
                        Icon::Text,
                        "诊断日志",
                        "启用后会记录详细的运行日志，用于问题排查（重启后生效）。"
                    ),
                    checkbox(self.preferences.diagnostic_logging)
                        .on_toggle(Message::DiagnosticLogging)
                ]
                .spacing(16)
                .align_y(Alignment::Center)
            ),
            form_card(
                row![
                    form_heading(
                        Icon::Grid,
                        "减少动态效果",
                        "关闭页面转场和卡片过渡，保留清晰的选中反馈。"
                    ),
                    checkbox(self.preferences.reduced_motion).on_toggle(Message::ReducedMotion),
                ]
                .spacing(16)
                .align_y(Alignment::Center)
            ),
            form_card(performance),
            form_card(
                column![
                    form_heading(
                        Icon::Folder,
                        "资源库位置",
                        "管理素材文件的存储位置，包含照片、视频、元数据等内容。"
                    ),
                    container(
                        row![
                            text(library_path).size(13).color(ICON).width(Length::Fill),
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
                        form_action(Icon::Folder, "打开资源库", Message::Open, false),
                        form_action(Icon::Plus, "新建资源库", Message::ShowCreate, false)
                    ]
                    .spacing(10)
                ]
                .spacing(14)
            ),
            form_card(
                column![
                    form_heading(
                        Icon::Cube,
                        "模型与索引",
                        "模型按需安装，可在 AI 模型页选择下载源、安装模型并管理索引任务。"
                    ),
                    container(
                        row![
                            text(model_path).size(13).color(ICON).width(Length::Fill),
                            icon_button(Icon::Copy, Message::CopyModelsPath)
                        ]
                        .spacing(10)
                        .align_y(Alignment::Center)
                    )
                    .padding([4, 12])
                    .width(Length::Fill)
                    .style(crate::theme::form_card_style),
                    row![
                        form_action(Icon::Cube, "查看模型管理", Message::Navigate(2), false),
                        form_action(
                            Icon::Folder,
                            "打开模型文件夹",
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
                        heading("设置", 24),
                        muted("管理应用偏好、性能配置与本地资源")
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
                    button(text("杉果派 · 项目主页").size(12).color(BLUE))
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
