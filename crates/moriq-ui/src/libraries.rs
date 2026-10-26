//! Registry removal and separately confirmed permanent removal.
use super::*;
use crate::{components::*, icons::Icon};
use iced::Length;
use iced::widget::{Space, column, text_input};
#[derive(Default)]
pub(super) struct State {
    pub path: Option<PathBuf>,
    pub name: String,
    pub plan: Option<moriq_app::LibraryRemovalPlan>,
    pub confirmation: String,
    pub final_step: bool,
    pub busy: bool,
    pub request: u64,
    pub error: Option<String>,
}
#[derive(Debug, Clone)]
pub enum LibraryMessage {
    Open(PathBuf),
    Plan(u64, Result<moriq_app::LibraryRemovalPlan, String>),
    Next,
    Name(String),
    Forget,
    Delete,
    Closed(u64, bool, Result<(), String>),
    Finished(u64, Result<UserSettings, String>),
}
fn msg(value: LibraryMessage) -> Message {
    Message::Libraries(value)
}
impl App {
    pub(super) fn library_message(&mut self, event: LibraryMessage) -> Task<Message> {
        use LibraryMessage::*;
        match event {
            Open(path)
                if !self.switching
                    && self.import.is_none()
                    && !self.manager.busy
                    && !self.libraries.busy =>
            {
                let Some((name, _)) = self.preferences.recent.iter().find(|(_, p)| *p == path)
                else {
                    return Task::none();
                };
                let request = self.libraries.request.wrapping_add(1);
                self.libraries = State {
                    path: Some(path.clone()),
                    name: name.clone(),
                    request,
                    ..Default::default()
                };
                self.manager.panel = Some(manage::Panel::Libraries);
                let service = self.service.clone();
                return Task::perform(
                    async move { service.library_removal_plan(path).await },
                    move |r| msg(Plan(request, r)),
                );
            }
            Plan(request, result) if request == self.libraries.request && !self.libraries.busy => {
                match result {
                    Ok(plan) => {
                        self.libraries.name = plan.name.clone();
                        self.libraries.plan = Some(plan)
                    }
                    Err(e) => self.libraries.error = Some(e),
                }
            }
            Next if !self.libraries.busy && self.libraries.plan.is_some() => {
                self.libraries.final_step = true
            }
            Name(value) if !self.libraries.busy => self.libraries.confirmation = value,
            Forget | Delete if !self.libraries.busy && !self.switching => {
                let permanent = matches!(event, Delete);
                if permanent
                    && (!self.libraries.final_step
                        || self
                            .libraries
                            .plan
                            .as_ref()
                            .is_none_or(|p| self.libraries.confirmation.trim() != p.name))
                {
                    return Task::none();
                }
                let Some(path) = self.libraries.path.clone() else {
                    return Task::none();
                };
                if !permanent
                    && self.library.as_ref().is_some_and(|l| l.root == path)
                    && (self.manager.notes_dirty
                        || self.ai.description_dirty
                        || self.ai.classification_dirty)
                {
                    self.libraries.error=Some(tr!(self.preferences.language,"当前素材有未保存修改，请先保存后再移除资源库。"=>"Save the current asset edits before removing this library.").into());
                    return Task::none();
                }
                self.libraries.busy = true;
                self.switching = true;
                self.libraries.error = None;
                let request = self.libraries.request;
                let service = self.service.clone();
                let active = self
                    .library
                    .as_ref()
                    .filter(|l| {
                        l.root == path
                            || l.root
                                .canonicalize()
                                .is_ok_and(|root| path.canonicalize().is_ok_and(|p| p == root))
                    })
                    .map(|l| l.info.id);
                self.stop_viewer();
                return Task::perform(
                    async move {
                        if let Some(id) = active {
                            service.close_library(id).await
                        } else {
                            Ok(())
                        }
                    },
                    move |r| msg(Closed(request, permanent, r)),
                );
            }
            Closed(request, permanent, result)
                if request == self.libraries.request && self.libraries.busy =>
            {
                if let Err(e) = result {
                    self.switching = false;
                    self.libraries.busy = false;
                    self.libraries.error = Some(e);
                    return Task::none();
                }
                let Some(path) = self.libraries.path.clone() else {
                    return Task::none();
                };
                if self.library.as_ref().is_some_and(|l| {
                    l.root == path
                        || l.root
                            .canonicalize()
                            .is_ok_and(|root| path.canonicalize().is_ok_and(|p| p == root))
                }) {
                    self.stop_people_portraits();
                    self.lifecycle.cancel_copy();
                    self.library = None;
                    self.viewer = Default::default();
                    let panel = self.manager.panel;
                    self.manager = Default::default();
                    self.manager.panel = panel;
                    self.ai.category_version = None;
                    self.masonry = None;
                    self.neighbors.clear();
                    self.tiles.clear();
                    self.query = Default::default();
                    self.total = 0;
                    self.history = Default::default();
                    self.library_picker = true;
                    self.screen = Screen::Overview;
                }
                let plan = if permanent {
                    self.libraries.plan.clone()
                } else {
                    None
                };
                let service = self.service.clone();
                return Task::perform(
                    async move { service.remove_library(path, plan).await },
                    move |r| msg(Finished(request, r)),
                );
            }
            Finished(request, result)
                if request == self.libraries.request && self.libraries.busy =>
            {
                self.switching = false;
                self.libraries.busy = false;
                match result {
                    Ok(settings) => {
                        self.apply_settings(settings);
                        if let Some(path) = self.libraries.path.take() {
                            self.library_covers.remove(&path);
                            self.library_summaries.remove(&path);
                        }
                        self.manager.panel = None;
                        self.status =
                            tr!(self.preferences.language,"资源库已移除"=>"Library removed").into();
                    }
                    Err(e) => self.libraries.error = Some(e),
                }
            }
            _ => {}
        }
        Task::none()
    }
    pub(super) fn library_actions_form(&self) -> Element<'_, Message> {
        use LibraryMessage::*;
        let language = self.preferences.language;
        let state = &self.libraries;
        let mut view = column![
            heading(&state.name, 18),
            muted(
                state
                    .path
                    .as_ref()
                    .map(|p| display_path(p))
                    .unwrap_or_default()
            )
        ]
        .spacing(16);
        if let Some(plan) = &state.plan {
            view = view.push(chip(
                trf!(language,"{} 项素材（含回收站）"=>"{} assets, including trash",plan.assets),
                crate::theme::BLUE,
            ));
        }
        if !state.final_step {
            view=view.push(form_card(column![heading(tr!(language,"从列表移除"=>"Remove from list"),15),
                muted(tr!(language,"只移除本机记录，文件全部保留，之后可重新打开。若是当前资源库，将先关闭。"=>"Forget this local entry without deleting any files. It can be opened again; the current library closes first.")),
                action(tr!(language,"从列表移除"=>"Remove from list"),msg(Forget)).on_press_maybe((!state.busy).then_some(msg(Forget)))].spacing(12)))
                .push(form_card(column![heading(tr!(language,"永久删除资源库"=>"Delete library permanently"),15),
                    muted(tr!(language,"删除此目录内的托管原片、版本、标签、项目与索引。库外引用原片和其他资源库不受影响。此操作不可撤销。"=>"Deletes managed originals, versions, tags, projects and indexes inside this folder. External referenced originals and other libraries are preserved. This cannot be undone.")),
                    action(tr!(language,"继续确认删除…"=>"Review permanent deletion…"),msg(Next)).style(crate::theme::destructive)
                        .on_press_maybe((!state.busy && state.plan.is_some()).then_some(msg(Next)))].spacing(12)));
        } else {
            view=view.push(heading(tr!(language,"最后确认：永久删除"=>"Final confirmation: permanent deletion"),16))
                .push(muted(tr!(language,"请输入完整资源库名称以确认。目录中的托管原片也将永久删除。"=>"Type the full library name to confirm. Managed originals in this folder will also be permanently deleted.")))
                .push(text_input(tr!(language,"资源库名称"=>"Library name"),&state.confirmation).padding(10).style(crate::theme::input)
                    .on_input_maybe((!state.busy).then_some(|v|msg(Name(v)))))
                .push(form_action(Icon::Trash,tr!(language,"永久删除资源库及库内文件"=>"Permanently delete library and its files"),msg(Delete),true)
                    .style(crate::theme::destructive).on_press_maybe((!state.busy && state.confirmation.trim()==state.name).then_some(msg(Delete))));
        }
        if state.busy {
            view=view.push(muted(tr!(language,"正在关闭资源库并处理，请稍候…"=>"Closing the library and processing, please wait...")));
        }
        if let Some(error) = &state.error {
            view = view.push(muted(error));
        }
        view.push(Space::new().height(2)).width(Length::Fill).into()
    }
}
