//! Application-shell adapter. The feature only receives its explicit context.
use super::*;
pub(crate) use crate::features::batch::{BatchMessage, BatchState};
use crate::features::batch::{Context, Effect};
impl App {
    fn batch_context(&self) -> Context<'_> {
        Context {
            owner: self
                .library
                .as_ref()
                .map(|library| (library.info.id, self.query.project)),
            query: &self.query,
            selected: self.selected_ids(),
            total: self.total,
            language: self.preferences.language,
            projects: self
                .library
                .as_ref()
                .map(|library| {
                    library
                        .info
                        .projects
                        .iter()
                        .map(|project| {
                            crate::features::batch::ProjectTarget::new(
                                project.id,
                                manage::project_label(&library.info.projects, project.id),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default(),
        }
    }
    pub(super) fn batch_message(&mut self, event: BatchMessage) -> Task<Message> {
        let context = self.batch_context();
        let query = context.query.clone();
        let context = Context {
            query: &query,
            ..context
        };
        let effect = self.batch.update(event, context);
        match effect {
            Effect::None => Task::none(),
            Effect::Open => {
                self.manager.panel = Some(manage::Panel::Batch);
                Task::none()
            }
            Effect::Submit(submission) => {
                let service = self.service.for_project(submission.owner.1);
                let request = submission.request;
                let owner = submission.owner;
                Task::perform(
                    async move {
                        service
                            .batch_edit(owner.0, submission.scope, submission.patch)
                            .await
                    },
                    move |result| Message::Batch(BatchMessage::Stored(owner, request, result)),
                )
            }
            Effect::Completed => {
                self.manager.panel = Some(manage::Panel::Tasks);
                self.status = tr!(self.preferences.language, "批量修改已加入后台任务，可暂停并在重启后继续" => "Batch edits queued. You can pause and resume after restarting.").into();
                self.refresh_organizer()
            }
            Effect::Failed(error) => {
                self.error = Some(error);
                Task::none()
            }
        }
    }
    pub(super) fn batch_panel(&self) -> Element<'_, Message> {
        self.batch.view(self.batch_context()).map(Message::Batch)
    }
}
