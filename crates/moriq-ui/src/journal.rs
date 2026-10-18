use super::*;
use crate::{components::*, theme::*};
use iced::widget::{Space, button, column, row, text};
use iced::{Alignment, Length};

#[derive(Default)]
pub(super) struct Journal {
    pub activity: EventPage,
    pub notifications: EventPage,
    pub recent: Vec<LibraryEvent>,
    requests: [u64; 2],
    cursors: [Vec<Option<i64>>; 2],
    loading: [bool; 2],
    newest_notice: i64,
}
impl Journal {
    pub fn reset(&mut self) {
        let requests = self.requests.map(|n| n + 1);
        *self = Self {
            requests,
            ..Default::default()
        };
    }
}
#[derive(Debug, Clone)]
pub enum JournalMessage {
    Open(bool),
    Loaded(LibraryId, bool, u64, bool, Result<EventPage, String>),
    Page(bool, bool),
    Read,
    Stored(LibraryId, Result<(), String>),
    OpenAsset(AssetId),
    Located(LibraryId, u64, AssetId, Result<Option<bool>, String>),
    Retry(String),
}
impl App {
    pub(super) fn load_events(&mut self, notices: bool, before: Option<i64>) -> Task<Message> {
        let Some(library) = &self.library else {
            return Task::none();
        };
        if self.launch.design_preview {
            return Task::none();
        }
        let index = usize::from(notices);
        self.journal.requests[index] += 1;
        let request = self.journal.requests[index];
        self.journal.loading[index] = true;
        let id = library.info.id;
        let service = self.service.clone();
        Task::perform(
            async move { service.events(id, notices, before).await },
            move |result| {
                Message::Journal(JournalMessage::Loaded(
                    id,
                    notices,
                    request,
                    before.is_none(),
                    result,
                ))
            },
        )
    }
    pub(super) fn persist_notice(
        &self,
        title: &str,
        detail: String,
        level: u8,
        retry: &str,
    ) -> Task<Message> {
        self.persist_asset_notice(title, detail, level, retry, None)
    }
    pub(super) fn persist_asset_notice(
        &self,
        title: &str,
        detail: String,
        level: u8,
        retry: &str,
        asset: Option<AssetId>,
    ) -> Task<Message> {
        let Some(library) = &self.library else {
            return Task::none();
        };
        if self.launch.design_preview {
            return Task::none();
        }
        let id = library.info.id;
        let service = self.service.clone();
        let notice = Notice {
            title: title.into(),
            detail: detail.chars().take(16000).collect(),
            level,
            asset,
            retry: retry.into(),
        };
        Task::perform(
            async move { service.notice(id, notice).await },
            move |result| Message::Journal(JournalMessage::Stored(id, result)),
        )
    }
    pub(super) fn journal_message(&mut self, message: JournalMessage) -> Task<Message> {
        use JournalMessage::*;
        match message {
            Open(notices) => {
                self.manager.panel = Some(if notices {
                    manage::Panel::Notifications
                } else {
                    manage::Panel::Activity
                });
                self.journal.cursors[usize::from(notices)].clear();
                return self.load_events(notices, None);
            }
            Loaded(id, notices, request, head, result)
                if self.library.as_ref().is_some_and(|l| l.info.id == id)
                    && self.journal.requests[usize::from(notices)] == request =>
            {
                self.journal.loading[usize::from(notices)] = false;
                match result {
                    Ok(page) => {
                        if notices {
                            self.journal.newest_notice = self
                                .journal
                                .newest_notice
                                .max(page.items.first().map_or(0, |e| e.id));
                            self.journal.notifications = page;
                        } else {
                            if head {
                                self.journal.recent = page.items.iter().take(5).cloned().collect();
                            }
                            self.journal.activity = page;
                        }
                    }
                    Err(e) => self.error = Some(e),
                }
            }
            Page(notices, older) if !self.journal.loading[usize::from(notices)] => {
                let index = usize::from(notices);
                let page = if notices {
                    &self.journal.notifications
                } else {
                    &self.journal.activity
                };
                let cursor = if older && page.more {
                    let next = page.items.last().map(|e| e.id);
                    self.journal.cursors[index].push(next);
                    next
                } else if !older {
                    self.journal.cursors[index].pop();
                    self.journal.cursors[index].last().copied().flatten()
                } else {
                    return Task::none();
                };
                return self.load_events(notices, cursor);
            }
            Read => {
                if let Some(library) = &self.library {
                    let id = library.info.id;
                    let through = self.journal.newest_notice;
                    let service = self.service.clone();
                    return Task::perform(
                        async move { service.read_notifications(id, through).await },
                        move |r| Message::Journal(Stored(id, r)),
                    );
                }
            }
            Stored(id, result) if self.library.as_ref().is_some_and(|l| l.info.id == id) => {
                if let Err(e) = result {
                    self.error = Some(e);
                }
                self.journal.cursors[1].clear();
                return self.load_events(true, None);
            }
            OpenAsset(asset) => {
                if let Some(library) = &self.library {
                    self.browse_request += 1;
                    let request = self.browse_request;
                    let id = library.info.id;
                    let service = self.service.clone();
                    return Task::perform(
                        async move { service.asset_trash_state(id, asset).await },
                        move |r| Message::Journal(Located(id, request, asset, r)),
                    );
                }
            }
            Located(id, request, asset, result)
                if self.library.as_ref().is_some_and(|l| l.info.id == id)
                    && request == self.browse_request =>
            {
                match result {
                    Ok(Some(trash)) => {
                        self.manager.panel = None;
                        self.screen = Screen::Assets;
                        self.query = Query {
                            trash,
                            ..Default::default()
                        };
                        return Task::batch([self.reset_query(), self.load_neighborhood(asset, 0)]);
                    }
                    Ok(None) => self.status = "关联素材已被永久删除，活动记录仍保留。".into(),
                    Err(e) => self.error = Some(e),
                }
            }
            Retry(action) => {
                return self.update(Message::Manage(match action.as_str() {
                    "recover" => ManageMessage::Recover,
                    "maintenance" => ManageMessage::Check(false),
                    "synco" => ManageMessage::SyncoPoll(true),
                    _ => return Task::none(),
                }));
            }
            _ => {}
        }
        Task::none()
    }
    pub(super) fn event_list(&self, notices: bool) -> Element<'_, Message> {
        let index = usize::from(notices);
        let page = if notices {
            &self.journal.notifications
        } else {
            &self.journal.activity
        };
        let mut content = column![].spacing(12);
        if notices {
            content = content.push(
                row![
                    muted(format!("{} 条未读", page.unread)),
                    Space::new().width(Length::Fill),
                    action("全部标为已读", Message::Journal(JournalMessage::Read))
                ]
                .align_y(Alignment::Center),
            );
        }
        if self.journal.loading[index] {
            content = content.push(muted("正在读取…"));
        }
        if page.items.is_empty() && !self.journal.loading[index] {
            content = content.push(muted(if notices {
                "暂无通知"
            } else {
                "暂无活动记录"
            }));
        }
        for event in &page.items {
            let title = if notices {
                event.kind.as_str()
            } else {
                event_label(&event.kind)
            };
            let mut controls = row![muted(date(event.at)), Space::new().width(Length::Fill)]
                .spacing(8)
                .align_y(Alignment::Center);
            if let Some(asset) = event.asset {
                controls = controls.push(action(
                    "打开素材",
                    Message::Journal(JournalMessage::OpenAsset(asset)),
                ));
            }
            if let Some(project) = event.project
                && self
                    .library
                    .as_ref()
                    .is_some_and(|l| l.info.projects.iter().any(|p| p.id == project))
            {
                controls = controls.push(action("打开项目", Message::Project(project)));
            }
            if !event.retry.is_empty() {
                controls =
                    controls.push(
                        button(text("重试").size(12))
                            .style(secondary)
                            .padding(7)
                            .on_press_maybe((!self.manager.busy && !self.switching).then(|| {
                                Message::Journal(JournalMessage::Retry(event.retry.clone()))
                            })),
                    );
            }
            content = content.push(
                column![
                    heading(
                        format!("{}{}", if event.unread { "● " } else { "" }, title),
                        14
                    ),
                    text(&event.detail).size(13),
                    controls,
                    separator()
                ]
                .spacing(8),
            );
        }
        content
            .push(row![
                button(text("较新").size(12))
                    .style(secondary)
                    .padding(8)
                    .on_press_maybe(
                        (!self.journal.cursors[index].is_empty() && !self.journal.loading[index])
                            .then_some(Message::Journal(JournalMessage::Page(notices, false)))
                    ),
                Space::new().width(Length::Fill),
                button(text("较早").size(12))
                    .style(secondary)
                    .padding(8)
                    .on_press_maybe(
                        (page.more && !self.journal.loading[index])
                            .then_some(Message::Journal(JournalMessage::Page(notices, true)))
                    )
            ])
            .into()
    }
}
pub(super) fn event_label(kind: &str) -> &str {
    match kind {
        "import" => "导入素材",
        "favorite" => "收藏变更",
        "rating" => "评分变更",
        "rename" => "重命名素材",
        "notes" => "备注更新",
        "trash" => "回收站变更",
        "purge" => "永久删除",
        "membership" => "项目归属变更",
        "project" => "项目管理",
        "version" => "新增素材版本",
        "tag" => "标签变更",
        _ => "整理活动",
    }
}
