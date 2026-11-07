use super::*;
use crate::{
    components::*,
    icons::{Icon, icon},
    theme::*,
};
use iced::widget::{Space, checkbox, column, container, row};
use iced::{Alignment, Length};

pub(super) struct Selection {
    owner: LibraryId,
    project: Option<ProjectId>,
    selected: [bool; 3],
}

impl App {
    fn available_analysis(&self) -> [bool; 3] {
        let qwen = self
            .ai
            .models
            .iter()
            .any(|m| m.installed && m.spec.id == "qwen35-4b");
        [qwen, qwen, self.face_model_ready()]
    }
    /// 入口只冻结整理范围，选定的任务才会进入后台队列。
    pub(super) fn analysis_options_message(&mut self, event: AiMessage) -> Task<Message> {
        let language = self.preferences.language;
        match event {
            AiMessage::ChooseAnalysis if !self.switching => {
                let Some(library) = self.library.as_ref() else {
                    return Task::none();
                };
                if self.query.project.is_none() && !library.info.projects.is_empty() {
                    self.status =
                        tr!(language,"请先选择要整理的项目"=>"Select a project to organize").into();
                    return Task::none();
                }
                let available = self.available_analysis();
                self.ai.analysis_selection = Some(Selection {
                    owner: library.info.id,
                    project: self.query.project,
                    selected: [available[0], available[1], false],
                });
                self.dialog = Some(Dialog::AnalysisOptions);
            }
            AiMessage::SelectAnalysis(index, value) => {
                let available = self.available_analysis();
                if let Some(selection) = self.ai.analysis_selection.as_mut()
                    && let Some(slot) = selection.selected.get_mut(index as usize)
                {
                    *slot = value && available[index as usize];
                }
            }
            AiMessage::StartSelectedAnalysis
                if self.dialog == Some(Dialog::AnalysisOptions) && !self.switching =>
            {
                let Some(selection) = self.ai.analysis_selection.as_ref() else {
                    return Task::none();
                };
                if self.library.as_ref().map(|l| l.info.id) != Some(selection.owner)
                    || self.query.project != selection.project
                {
                    return Task::none();
                }
                let owner = selection.owner;
                let available = self.available_analysis();
                let kinds = [
                    JobKind::QwenClassification,
                    JobKind::QwenDescription,
                    JobKind::Analysis(ModelId::Faces),
                ];
                let kinds = kinds
                    .into_iter()
                    .enumerate()
                    .filter_map(|(i, kind)| (selection.selected[i] && available[i]).then_some(kind))
                    .collect::<Vec<_>>();
                if kinds.is_empty() {
                    return Task::none();
                }
                self.dialog = None;
                self.ai.analysis_selection = None;
                let service = self.service.clone();
                return Task::batch(kinds.into_iter().map(move |kind| {
                    let service = service.clone();
                    Task::perform(
                        async move {
                            service
                                .queue_media_job(owner, kind)
                                .await
                                .map_err(|e| e.to_string())
                        },
                        move |r| Message::Jobs(tasks::JobMessage::Stored(owner, r)),
                    )
                }));
            }
            _ => {}
        }
        Task::none()
    }
    /// 各分析类型独立选择，说明模型用途与已有人工结果的保留规则。
    pub(super) fn analysis_options(&self) -> Element<'_, Message> {
        let language = self.preferences.language;
        let Some(selection) = self.ai.analysis_selection.as_ref() else {
            return Space::new().into();
        };
        let available = self.available_analysis();
        let scope = self
            .library
            .as_ref()
            .and_then(|l| {
                l.info
                    .projects
                    .iter()
                    .find(|p| Some(p.id) == selection.project)
            })
            .map(|p| p.name.as_str())
            .unwrap_or(tr!(language,"全部素材"=>"All assets"));
        let mut body = column![
            row![
                icon(Icon::Sparkles, 24.0, BLUE),
                heading(tr!(language,"选择分析内容"=>"Choose analysis"), 22),
                Space::new().width(Length::Fill),
                icon_button(Icon::Close, Message::HideCreate)
            ]
            .spacing(12)
            .align_y(Alignment::Center),
            muted(trf!(language,"整理范围：{}"=>"Workspace: {}",scope)),
            separator()
        ]
        .spacing(18);
        for (index,(title,detail)) in [
            (tr!(language,"内容分类"=>"Content categories"),tr!(language,"Qwen · 识别成像媒介与照片主题"=>"Qwen · Identify medium and subjects")),
            (tr!(language,"标签与描述"=>"Tags and descriptions"),tr!(language,"Qwen · 生成标签和照片描述"=>"Qwen · Generate tags and descriptions")),
            (tr!(language,"人脸分组"=>"Face groups"),tr!(language,"CPU · 检测清晰人脸并按人物归组"=>"CPU · Detect clear faces and group people")),
        ].into_iter().enumerate() {
            let mut toggle = checkbox(selection.selected[index]).label(title);
            if available[index] { toggle = toggle.on_toggle(move |v|Message::Ai(AiMessage::SelectAnalysis(index as u8,v))); }
            let description = if available[index] { detail } else {
                tr!(language,"模型不可用，请先到模型与设置中准备"=>"Prepare the model in Models & settings first")
            };
            body=body.push(container(column![toggle,muted(description).size(12)]
                .spacing(7)).padding(15).width(Length::Fill).style(pale));
        }
        body=body.push(muted(tr!(language,"只运行勾选的部分。已有人工名称、分组纠正和原照片都会保留。"=>"Run only selected tasks. Manual names, grouping corrections and original photos are preserved.")));
        let enabled = selection
            .selected
            .iter()
            .zip(available)
            .any(|(selected, ready)| *selected && ready);
        body = body.push(
            row![
                Space::new().width(Length::Fill),
                action(tr!(language,"取消"=>"Cancel"), Message::HideCreate),
                form_action(
                    Icon::Play,
                    tr!(language,"开始分析"=>"Start analysis"),
                    Message::Ai(AiMessage::StartSelectedAnalysis),
                    true
                )
                .on_press_maybe(enabled.then_some(Message::Ai(AiMessage::StartSelectedAnalysis)))
            ]
            .spacing(10),
        );
        container(body)
            .padding(26)
            .width((self.viewport.width - 40.0).clamp(260.0, 520.0))
            .style(modal)
            .into()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    /// Qwen 卡片不启动人脸；外层选择只提交勾选项，取消和切换范围不能提交。
    fn qwen_and_selected_analysis_have_independent_admission() {
        let mut app = crate::tests::app();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        app.ai.models = runtime.block_on(app.service.models()).unwrap().0;
        for m in &mut app.ai.models {
            m.installed = true;
        }
        assert_eq!(app.ai_message(AiMessage::Organize).units(), 2);
        for mask in 0u8..8 {
            assert_eq!(app.ai_message(AiMessage::ChooseAnalysis).units(), 0);
            for i in 0..3 {
                drop(app.ai_message(AiMessage::SelectAnalysis(i, mask & (1 << i) != 0)));
            }
            drop(app.analysis_options());
            assert_eq!(
                app.ai_message(AiMessage::StartSelectedAnalysis).units(),
                mask.count_ones() as usize
            );
            if mask != 0 {
                assert!(app.dialog.is_none());
            }
        }
        drop(app.ai_message(AiMessage::ChooseAnalysis));
        drop(app.update(Message::HideCreate));
        assert_eq!(app.ai_message(AiMessage::StartSelectedAnalysis).units(), 0);
        drop(app.ai_message(AiMessage::ChooseAnalysis));
        app.query.project = Some(ProjectId::default());
        assert_eq!(app.ai_message(AiMessage::StartSelectedAnalysis).units(), 0);
        app.query.project = None;
        for m in &mut app.ai.models {
            m.installed = m.spec.id == "faces";
        }
        assert_eq!(app.ai_message(AiMessage::Organize).units(), 0);
    }
}
