use super::*;
use crate::components::{action, choice, localized_choice, muted};
use iced::{
    Element,
    widget::{checkbox, column, text_input},
};
impl BatchState {
    /// 构建批量编辑表单及确认入口。
    pub fn view(&self, context: Context<'_>) -> Element<'_, BatchMessage> {
        let language = context.language;
        let unchanged = ProjectTarget {
            id: None,
            label: tr!(language, "保持项目归属" => "Keep project membership").into(),
        };
        let mut projects = vec![unchanged.clone()];
        projects.extend(context.projects);
        let project = choice(
            projects,
            Some(self.project.clone().unwrap_or(unchanged)),
            BatchMessage::Project,
        );
        let scope = trf!(language, "当前选择 {} 项；全部查询结果 {} 项。任务创建时保存图片范围快照，之后新增素材不会自动加入。" => "{} items selected; {} results in total. The task saves a snapshot of the scope; new assets will not be included.",
            context.selected.len(),
            context.total,
        );
        let favorite = if self.favorite.is_empty() {
            "保持收藏状态".into()
        } else {
            self.favorite.clone()
        };
        let rating = if self.rating.is_empty() {
            "保持评分".into()
        } else {
            self.rating.clone()
        };
        column![
            project,
            muted(scope),
            checkbox(self.all)
                .label(tr!(language, "选择全部查询结果（跨越所有分页）" => "Select all results (across all pages)"))
                .on_toggle(BatchMessage::All),
            localized_choice(language,
                vec!["保持收藏状态".into(), "收藏".into(), "取消收藏".into()],
                Some(favorite),
                BatchMessage::Favorite,
            ),
            localized_choice(language,
                vec![
                    "保持评分".into(),
                    "0".into(),
                    "1".into(),
                    "2".into(),
                    "3".into(),
                    "4".into(),
                    "5".into()
                ],
                Some(rating),
                BatchMessage::Rating,
            ),
            text_input(tr!(language, "追加标签，逗号分隔" => "Add tags, separated by commas"), &self.add_tags)
                .style(crate::theme::input)
                .padding(10)
                .on_input(BatchMessage::AddTags),
            text_input(tr!(language, "移除标签，逗号分隔" => "Remove tags, separated by commas"), &self.remove_tags)
                .style(crate::theme::input)
                .padding(10)
                .on_input(BatchMessage::RemoveTags),
            text_input(tr!(language, "人工分类（留空保持原值）" => "Manual category (leave blank to keep)"), &self.category)
                .style(crate::theme::input)
                .padding(10)
                .on_input(BatchMessage::Category),
            checkbox(self.replace_notes)
                .label(tr!(language, "替换备注（可留空清除）" => "Replace notes (leave blank to clear)"))
                .on_toggle(BatchMessage::ReplaceNotes),
            text_input(tr!(language, "新的备注" => "New notes"), &self.notes)
                .style(crate::theme::input)
                .padding(10)
                .on_input(BatchMessage::Notes),
            muted(tr!(language, "以上字段一起保存；每个素材的修改在单个事务内完成。" => "These fields are saved together. Each asset is updated in a single transaction.")),
            action(tr!(language, "应用并加入后台任务" => "Apply and queue task"), BatchMessage::Apply).on_press_maybe((!self.saving).then_some(BatchMessage::Apply)),
        ]
        .spacing(12)
        .into()
    }
}
