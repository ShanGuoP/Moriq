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
    /// 清除活动与通知分页状态，并使旧读取回执失效。
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
    /// 按当前筛选和游标异步读取活动或通知页面。
    pub(super) fn load_events(&mut self, notices: bool, before: Option<i64>) -> Task<Message> {
        let Some(library) = &self.library else {
            return Task::none();
        };

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
    /// 把界面操作结果保存为不带素材关联的资源库通知。
    pub(super) fn persist_notice(
        &self,
        title: &str,
        detail: String,
        level: u8,
        retry: &str,
    ) -> Task<Message> {
        self.persist_asset_notice(title, detail, level, retry, None)
    }
    /// 保存带素材身份和重试信息的资源库通知。
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
    /// 处理活动分页、已读和关联素材导航，核对读取回执身份。
    pub(super) fn journal_message(&mut self, message: JournalMessage) -> Task<Message> {
        let language = self.preferences.language;
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
                    Ok(None) => self.status = tr!(language, "关联素材已被永久删除，活动记录仍保留。" => "The linked asset was permanently deleted. The activity record is retained.").into(),
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
    /// 构建活动或通知列表及加载更多入口。
    pub(super) fn event_list(&self, notices: bool) -> Element<'_, Message> {
        let language = self.preferences.language;
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
                    muted(trf!(language, "{} 条未读" => "{} unread", page.unread)),
                    Space::new().width(Length::Fill),
                    action(
                        tr!(language, "全部标为已读" => "Mark all as read"),
                        Message::Journal(JournalMessage::Read)
                    )
                ]
                .align_y(Alignment::Center),
            );
        }
        if self.journal.loading[index] {
            content = content.push(muted(tr!(language, "正在读取…" => "Loading…")));
        }
        if page.items.is_empty() && !self.journal.loading[index] {
            content = content.push(muted(if notices {
                tr!(language, "暂无通知" => "No notifications")
            } else {
                tr!(language, "暂无活动记录" => "No activity yet")
            }));
        }
        for event in &page.items {
            let title = if notices {
                event.kind.as_str()
            } else {
                event_label(language, &event.kind)
            };
            let mut controls = row![muted(date(event.at)), Space::new().width(Length::Fill)]
                .spacing(8)
                .align_y(Alignment::Center);
            if let Some(asset) = event.asset {
                controls = controls.push(action(
                    tr!(language, "打开素材" => "Open asset"),
                    Message::Journal(JournalMessage::OpenAsset(asset)),
                ));
            }
            if let Some(project) = event.project
                && self
                    .library
                    .as_ref()
                    .is_some_and(|l| l.info.projects.iter().any(|p| p.id == project))
            {
                controls = controls.push(action(
                    tr!(language, "打开项目" => "Open project"),
                    Message::Project(project),
                ));
            }
            if !event.retry.is_empty() {
                controls =
                    controls.push(
                        button(text(tr!(language, "重试" => "Retry")).size(12))
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
                button(text(tr!(language, "较新" => "Newer")).size(12))
                    .style(secondary)
                    .padding(8)
                    .on_press_maybe(
                        (!self.journal.cursors[index].is_empty() && !self.journal.loading[index])
                            .then_some(Message::Journal(JournalMessage::Page(notices, false)))
                    ),
                Space::new().width(Length::Fill),
                button(text(tr!(language, "较早" => "Older")).size(12))
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
/// 把活动类型转换为可读的本地化标签。
pub(super) fn event_label(language: Language, kind: &str) -> &str {
    match kind {
        "import" => tr!(language, "导入素材" => "Assets imported"),
        "favorite" => tr!(language, "收藏变更" => "Favorites updated"),
        "rating" => tr!(language, "评分变更" => "Rating updated"),
        "rename" => tr!(language, "重命名素材" => "Asset renamed"),
        "notes" => tr!(language, "备注更新" => "Notes updated"),
        "trash" => tr!(language, "回收站变更" => "Trash updated"),
        "purge" => tr!(language, "永久删除" => "Delete permanently"),
        "membership" => tr!(language, "项目归属变更" => "Project membership updated"),
        "project" => tr!(language, "项目管理" => "Project management"),
        "version" => tr!(language, "新增素材版本" => "Asset version added"),
        "tag" => tr!(language, "标签变更" => "Tags updated"),
        _ => tr!(language, "整理活动" => "Organization activity"),
    }
}
