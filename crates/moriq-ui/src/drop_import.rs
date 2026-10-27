//! Window drops form a quiet-period batch; queued batches retain their library/project.
use super::*;
use crate::{
    components::*,
    icons::{Icon, icon},
    theme::*,
};
use iced::widget::{Space, column, container, stack, text};
use iced::{Alignment, Length};
use std::{
    collections::{HashSet, VecDeque},
    time::Instant,
};
const QUIET: Duration = Duration::from_millis(120);
#[derive(Debug, Clone)]
pub enum DropMessage {
    Hover(PathBuf),
    Left,
    File(PathBuf),
    Flush(u64),
}
/// 把平台文件拖放事件转换为内部队列事件。
pub(super) fn window_event(event: iced::window::Event) -> Option<DropMessage> {
    match event {
        iced::window::Event::FileHovered(path) => Some(DropMessage::Hover(path)),
        iced::window::Event::FilesHoveredLeft => Some(DropMessage::Left),
        iced::window::Event::FileDropped(path) => Some(DropMessage::File(path)),
        _ => None,
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct Target {
    library: LibraryId,
    project: Option<ProjectId>,
}
struct Batch {
    target: Target,
    paths: Vec<PathBuf>,
    seen: HashSet<PathBuf>,
}
#[derive(Default)]
pub(super) struct State {
    hovered: HashSet<PathBuf>,
    pending: VecDeque<Batch>,
    ticket: u64,
    timer: bool,
    ready: bool,
    last: Option<Instant>,
}
enum Flush {
    Stale,
    Wait(Duration),
    Ready,
}
impl State {
    /// 清空拖放队列与接收上下文，并使延迟回执失效。
    pub fn clear(&mut self) {
        let ticket = self.ticket.wrapping_add(1);
        *self = Self {
            ticket,
            ..Default::default()
        };
    }
    /// 仅清除拖放悬停提示，保留已登记批次。
    pub fn clear_hover(&mut self) {
        self.hovered.clear();
    }
    /// 判断当前窗口是否正在显示文件拖入反馈。
    pub fn hovering(&self) -> bool {
        !self.hovered.is_empty()
    }
    /// 返回尚未开始处理的拖放项目数量。
    pub fn pending_count(&self) -> usize {
        self.pending.iter().map(|b| b.paths.len()).sum()
    }
    /// 判断队列中是否存在已经过静默收集期的批次。
    pub fn has_ready(&self) -> bool {
        self.ready && !self.pending.is_empty()
    }
    /// 把平台分散发送的文件事件收集到当前批次，并冻结接收项目。
    fn push(&mut self, target: Target, path: PathBuf, now: Instant) -> Option<u64> {
        if self
            .pending
            .back()
            .is_none_or(|batch| batch.target != target)
        {
            self.pending.push_back(Batch {
                target,
                paths: vec![],
                seen: HashSet::new(),
            });
        }
        if let Some(batch) = self.pending.back_mut()
            && batch.seen.insert(path.clone())
        {
            batch.paths.push(path);
        }
        self.last = Some(now);
        self.ready = false;
        if self.timer {
            None
        } else {
            self.timer = true;
            Some(self.ticket)
        }
    }
    /// 在静默期结束时封存当前批次，准备按队列顺序导入。
    fn flush(&mut self, ticket: u64, now: Instant) -> Flush {
        if ticket != self.ticket || !self.timer {
            return Flush::Stale;
        }
        if let Some(last) = self.last {
            let elapsed = now.saturating_duration_since(last);
            if elapsed < QUIET {
                return Flush::Wait(QUIET - elapsed);
            }
        }
        self.timer = false;
        self.ready = true;
        Flush::Ready
    }
    /// 取出下一个可以处理的拖放批次。
    fn pop(&mut self) -> Option<Batch> {
        if !self.ready {
            return None;
        }
        let batch = self.pending.pop_front();
        self.ready = !self.pending.is_empty();
        batch
    }
}
#[derive(Clone, Copy)]
enum Blocked {
    Library,
    Switching,
    Busy,
    Surface,
    Trash,
    Stopping,
}
impl Blocked {
    /// 生成当前拖放反馈提示文本。
    fn text(self, language: Language) -> &'static str {
        match self {
            Self::Library => {
                tr!(language,"请先创建或打开资源库，再拖入图片"=>"Create or open a library before dropping images")
            }
            Self::Switching => {
                tr!(language,"正在切换资源库，请稍后再拖入"=>"Wait until the library finishes switching")
            }
            Self::Busy => {
                tr!(language,"请等待当前操作完成后再拖入"=>"Wait for the current operation before dropping files")
            }
            Self::Surface => {
                tr!(language,"请先关闭当前面板，再拖入图片"=>"Close the current panel before dropping images")
            }
            Self::Trash => {
                tr!(language,"请先切换到素材库或项目，再拖入图片"=>"Switch to the library or a project before dropping images")
            }
            Self::Stopping => {
                tr!(language,"正在停止导入，请稍后再拖入"=>"Wait until the current import has stopped")
            }
        }
    }
}
impl App {
    /// 计算当前页面允许接收文件的资源库和项目目标。
    fn drop_target(&self) -> Option<Target> {
        self.library.as_ref().map(|library| Target {
            library: library.info.id,
            project: (self.screen == Screen::Assets)
                .then_some(self.query.project)
                .flatten(),
        })
    }
    /// 判断当前界面或忙碌状态是否阻止开始拖放导入。
    fn drop_blocked(&self) -> Option<Blocked> {
        if self.library.is_none() || self.library_picker {
            Some(Blocked::Library)
        } else if self.switching {
            Some(Blocked::Switching)
        } else if self
            .import
            .as_ref()
            .is_some_and(|control| control.cancel.load(Ordering::Acquire))
        {
            Some(Blocked::Stopping)
        } else if self.manager.busy {
            Some(Blocked::Busy)
        } else if self.surface_open() {
            Some(Blocked::Surface)
        } else if self.screen == Screen::Assets && self.query.trash {
            Some(Blocked::Trash)
        } else {
            None
        }
    }
    /// 处理拖入、离开、文件到达和静默期回执。
    pub(super) fn drop_message(&mut self, message: DropMessage) -> Task<Message> {
        match message {
            DropMessage::Hover(path) => {
                self.drop_import.hovered.insert(path);
            }
            DropMessage::Left => self.drop_import.clear_hover(),
            DropMessage::File(path) => {
                self.drop_import.clear_hover();
                if let Some(blocked) = self.drop_blocked() {
                    self.error = Some(blocked.text(self.preferences.language).into());
                    return Task::none();
                }
                let Some(target) = self.drop_target() else {
                    return Task::none();
                };
                let timer = self.drop_import.push(target, path, Instant::now());
                let count = self.drop_import.pending_count();
                self.status = trf!(self.preferences.language,"已接收 {count} 项，准备导入…"=>"Received {count} items for import…");
                if let Some(ticket) = timer {
                    return Self::flush_drops_after(ticket, QUIET);
                }
            }
            DropMessage::Flush(ticket) => match self.drop_import.flush(ticket, Instant::now()) {
                Flush::Stale => {}
                Flush::Wait(delay) => return Self::flush_drops_after(ticket, delay),
                Flush::Ready => return self.start_next_drop(),
            },
        }
        Task::none()
    }
    /// 安排一次带世代标识的静默期回执，合并平台连续文件事件。
    fn flush_drops_after(ticket: u64, delay: Duration) -> Task<Message> {
        Task::perform(
            async move {
                tokio::time::sleep(delay).await;
            },
            move |_| Message::Drop(DropMessage::Flush(ticket)),
        )
    }
    /// 检查已有批次、资源库和任务状态是否允许启动下一次导入。
    pub(super) fn can_start_drop(&self) -> bool {
        self.drop_blocked().is_none()
    }
    /// 按队列顺序启动已就绪批次，保留批次原始库与项目归属。
    pub(super) fn start_next_drop(&mut self) -> Task<Message> {
        if !self.drop_import.has_ready() {
            return Task::none();
        }
        let owner = self.library.as_ref().map(|library| library.info.id);
        if self
            .drop_import
            .pending
            .front()
            .is_some_and(|batch| Some(batch.target.library) != owner)
        {
            self.drop_import.clear();
            return Task::none();
        }
        if self.import.is_some() || self.drop_blocked().is_some() {
            return Task::none();
        }
        let Some(batch) = self.drop_import.pop() else {
            return Task::none();
        };
        self.transition = None;
        self.stop_viewer();
        if self.screen != Screen::Assets {
            self.query = Query {
                project: batch.target.project,
                ..Default::default()
            };
        }
        self.start_import(batch.paths, batch.target.project, false)
    }
    /// 把拖放来源交给统一导入管线，沿用进度、取消和错误处理。
    pub(super) fn start_import(
        &mut self,
        paths: Vec<PathBuf>,
        project: Option<ProjectId>,
        referenced: bool,
    ) -> Task<Message> {
        let Some(library) = &self.library else {
            return Task::none();
        };
        if paths.is_empty() {
            self.manager.busy = false;
            self.manager.referenced_import = false;
            return Task::none();
        }
        let control = ImportControl {
            referenced,
            ..ImportControl::with_preferences(self.preferences.import)
        };
        self.manager.busy = false;
        self.manager.panel = None;
        self.manager.referenced_import = referenced;
        self.manager.import_seen = 0;
        self.import = Some(control.clone());
        let service = self.service.clone();
        let id = library.info.id;
        self.screen = Screen::Assets;
        self.status=tr!(self.preferences.language,"正在扫描并导入，原文件保持不变…"=>"Scanning and importing. Original files are preserved…").into();
        Task::perform(
            async move { service.import_sources(id, paths, project, control).await },
            Message::Imported,
        )
    }
    /// 构建文件拖入提示层，避免破坏底层浏览和表单状态。
    pub(super) fn drop_overlay<'a>(&self, base: Element<'a, Message>) -> Element<'a, Message> {
        let language = self.preferences.language;
        let overlay: Element<'a, Message> = if self.drop_import.hovering() {
            let blocked = self.drop_blocked();
            let title = blocked.map_or_else(
                || {
                    if self.import.is_some() {
                        tr!(language,"释放以加入导入队列"=>"Release to queue for import")
                    } else {
                        tr!(language,"释放以导入图片"=>"Release to import images")
                    }
                },
                |reason| reason.text(language),
            );
            let description = if blocked.is_some() {
                tr!(language,"文件尚未导入，原文件保持不变。"=>"No files have been imported. Originals are preserved.")
            } else {
                tr!(language,"支持多张图片和文件夹 · 原文件保持不变"=>"Multiple images and folders · Originals are preserved")
            };
            let destination = self
                .drop_target()
                .and_then(|target| target.project)
                .and_then(|id| {
                    self.library
                        .as_ref()?
                        .info
                        .projects
                        .iter()
                        .find(|project| project.id == id)
                        .map(|project| project.name.clone())
                });
            let destination = destination.map_or_else(
                || tr!(language,"当前资源库"=>"Current library").to_owned(),
                |name| trf!(language,"项目：{name}"=>"Project: {name}"),
            );
            container(
                container(
                    column![
                        icon(Icon::Image, 42.0, BLUE),
                        heading(title, 22),
                        muted(description),
                        text(destination).size(13).style(text_color(BLUE))
                    ]
                    .spacing(14)
                    .align_x(Alignment::Center),
                )
                .padding(30)
                .max_width(620)
                .style(|theme| container::Style {
                    background: Some(adaptive(theme, iced::Color::WHITE).into()),
                    border: iced::Border {
                        color: adaptive(theme, BLUE),
                        width: 2.0,
                        radius: 14.0.into(),
                    },
                    ..Default::default()
                }),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(|theme| container::Style {
                background: Some(adaptive(theme, PALE).scale_alpha(0.82).into()),
                ..Default::default()
            })
            .into()
        } else {
            Space::new().width(Length::Fill).height(Length::Fill).into()
        };
        // The base child is always at the same tree position: hovering must not remount forms or the viewer.
        stack![base, overlay].into()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use iced::futures::StreamExt;
    /// 构造测试拖放事件使用的独立文件路径。
    fn file(name: &str) -> PathBuf {
        PathBuf::from(name)
    }
    /// 准备当前用例可接收回执的状态或已就绪的替身响应。
    fn ready(app: &mut App) -> Task<Message> {
        app.drop_import.last = Some(Instant::now() - QUIET);
        let ticket = app.drop_import.ticket;
        app.update(Message::Drop(DropMessage::Flush(ticket)))
    }
    /// 构造导入完成回执，供队列和状态切换验证。
    async fn imported(task: Task<Message>) -> ImportReport {
        let mut stream = iced_runtime::task::into_stream(task).unwrap();
        while let Some(action) = stream.next().await {
            if let iced_runtime::Action::Output(Message::Imported(report)) = action {
                return report;
            }
        }
        panic!("import task produced no completion")
    }
    #[test]
    /// 验证窗口拖放与选择器回执分开，悬停离开不丢弃已收集文件。
    fn window_drop_events_are_distinct_from_picker_receipts_and_hover_leave_keeps_files() {
        let path = file("A.PNG");
        assert!(
            matches!(window_event(iced::window::Event::FileDropped(path.clone())),Some(DropMessage::File(value)) if value==path)
        );
        assert!(
            matches!(window_event(iced::window::Event::FileHovered(path.clone())),Some(DropMessage::Hover(value)) if value==path)
        );
        assert!(matches!(
            window_event(iced::window::Event::FilesHoveredLeft),
            Some(DropMessage::Left)
        ));
        assert!(window_event(iced::window::Event::Focused).is_none());
        let mut app = crate::tests::app();
        drop(app.update(Message::Drop(DropMessage::File(path.clone()))));
        drop(app.update(Message::Drop(DropMessage::Hover(path))));
        drop(app.update(Message::Drop(DropMessage::Left)));
        assert_eq!(app.drop_import.pending_count(), 1);
        assert!(!app.drop_import.hovering());
    }
    #[test]
    /// 验证连续拖放共用静默计时器，重置后忽略旧封存回执。
    fn bursts_use_one_timer_wait_for_quiet_and_ignore_stale_flush_after_reset() {
        let now = Instant::now();
        let owner = LibraryId::default();
        let target = Target {
            library: owner,
            project: None,
        };
        let mut state = State::default();
        let ticket = state.push(target, file("a.png"), now).unwrap();
        assert!(state.push(target, file("b.png"), now + QUIET / 2).is_none());
        assert!(state.push(target, file("a.png"), now + QUIET / 2).is_none());
        assert_eq!(state.pending_count(), 2);
        assert!(matches!(state.flush(ticket, now + QUIET), Flush::Wait(_)));
        assert!(!state.has_ready());
        assert!(matches!(state.flush(ticket, now + QUIET * 2), Flush::Ready));
        assert_eq!(
            state.pop().unwrap().paths,
            vec![file("a.png"), file("b.png")]
        );
        state.clear();
        assert!(matches!(state.flush(ticket, now + QUIET * 3), Flush::Stale));
    }
    #[test]
    /// 验证阻塞上下文不导入或修改草稿，取消会清空待处理队列。
    fn blocked_contexts_do_not_import_or_change_unsaved_forms_and_cancel_clears_the_queue() {
        let mut app = crate::tests::app();
        app.manager.notes = "unsaved".into();
        app.manager.notes_dirty = true;
        app.dialog = Some(Dialog::Settings);
        drop(app.update(Message::Drop(DropMessage::File(file("a.png")))));
        assert_eq!(app.dialog, Some(Dialog::Settings));
        assert_eq!(app.manager.notes, "unsaved");
        assert_eq!(app.drop_import.pending_count(), 0);
        assert!(app.import.is_none());
        app.dialog = None;
        app.manager.busy = true;
        drop(app.update(Message::Drop(DropMessage::File(file("a.png")))));
        assert_eq!(app.drop_import.pending_count(), 0);
        assert!(app.manager.busy);
        app.manager.busy = false;
        app.library_picker = true;
        drop(app.update(Message::Drop(DropMessage::File(file("a.png")))));
        assert_eq!(app.drop_import.pending_count(), 0);
        app.library_picker = false;
        drop(app.update(Message::Drop(DropMessage::File(file("a.png")))));
        let ticket = app.drop_import.ticket;
        let control = ImportControl::default();
        app.import = Some(control.clone());
        drop(app.update(Message::CancelImport));
        assert!(control.cancel.load(Ordering::Acquire));
        assert_eq!(app.drop_import.pending_count(), 0);
        drop(app.update(Message::Drop(DropMessage::Flush(ticket))));
        assert_eq!(app.drop_import.pending_count(), 0);
        app.import = None;
        drop(app.update(Message::Drop(DropMessage::File(file("a.png")))));
        let old = app.drop_import.ticket;
        app.library.as_mut().unwrap().info.id = LibraryId::default();
        app.drop_import.last = Some(Instant::now() - QUIET);
        drop(app.update(Message::Drop(DropMessage::Flush(old))));
        assert_eq!(app.drop_import.pending_count(), 0);
        assert!(app.import.is_none());
    }
    #[test]
    /// 验证拖放悬停仅显示提示，不关闭预览或修改编辑表单。
    fn hovering_is_informative_without_closing_preview_or_editing_forms() {
        let mut app = crate::tests::app();
        app.viewer.selected = Some(crate::tests::selected_asset());
        app.viewer.preview_expanded = true;
        let request = app.viewer.original_request;
        app.manager.notes = "draft".into();
        app.manager.notes_dirty = true;
        drop(app.update(Message::Drop(DropMessage::Hover(file("A.PNG")))));
        drop(app.shell());
        assert_eq!(app.viewer.original_request, request);
        assert!(app.viewer.preview_expanded);
        assert_eq!(app.manager.notes, "draft");
        assert!(app.drop_import.hovering());
        drop(app.update(Message::ClosePreview));
        assert!(!app.drop_import.hovering());
        assert!(app.viewer.preview_expanded);
        app.library = None;
        drop(app.update(Message::Drop(DropMessage::File(file("A.PNG")))));
        assert!(app.import.is_none());
        assert_eq!(app.drop_import.pending_count(), 0);
        assert!(app.error.is_some());
    }
    #[test]
    /// 验证拖放及迟到文件夹批次保存到原接收项目，来源保持不变。
    fn dropped_files_and_late_folder_batch_are_persisted_in_the_original_project_with_sources_preserved()
     {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let dir = tempfile::tempdir().unwrap();
            let sources = dir.path().join("sources");
            std::fs::create_dir(&sources).unwrap();
            let folder = sources.join("folder.with.dots");
            std::fs::create_dir(&folder).unwrap();
            let a = sources.join("A.PNG");
            let b = sources.join("B.png");
            let c = folder.join("C.png");
            for (path, color) in [
                (&a, [255, 0, 0, 255]),
                (&b, [0, 0, 255, 255]),
                (&c, [0, 255, 0, 255]),
            ] {
                ::image::RgbaImage::from_pixel(32, 24, ::image::Rgba(color))
                    .save(path)
                    .unwrap();
            }
            let originals: Vec<_> = [&a, &b, &c].map(|path| std::fs::read(path).unwrap()).into();
            std::fs::copy(&a, folder.join("duplicate.png")).unwrap();
            std::fs::write(folder.join("notes.txt"), "skip nested non-images").unwrap();
            let service = Service::start().unwrap();
            let library = service
                .create(dir.path().into(), "drag drop".into())
                .await
                .unwrap();
            let owner = library.info.id;
            let target = service.project(owner, "destination".into()).await.unwrap();
            let other = service
                .project(owner, "different project".into())
                .await
                .unwrap();
            let (mut app, initial) = App::new(service.clone(), LaunchOptions::default());
            drop(initial);
            app.library = Some(library);
            app.screen = Screen::Assets;
            app.query.project = Some(target);
            app.manager.referenced_import = true;
            let event = window_event(iced::window::Event::FileDropped(a.clone())).unwrap();
            drop(app.update(Message::Drop(event)));
            let first = ready(&mut app);
            assert!(app.import.is_some());
            assert!(!app.import.as_ref().unwrap().referenced);
            for path in [&b, &b, &folder] {
                drop(app.update(Message::Drop(DropMessage::File(path.clone()))));
            }
            assert_eq!(app.drop_import.pending_count(), 2);
            assert_eq!(ready(&mut app).units(), 0);
            assert_eq!(app.drop_import.pending_count(), 2);
            app.query.project = Some(other);
            let first = tokio::time::timeout(Duration::from_secs(10), imported(first))
                .await
                .unwrap();
            assert_eq!((first.imported, first.failed), (1, 0));
            let followup = app.update(Message::Imported(first));
            assert!(app.import.is_some());
            assert_eq!(app.drop_import.pending_count(), 0);
            let second = tokio::time::timeout(Duration::from_secs(10), imported(followup))
                .await
                .unwrap();
            assert_eq!(
                (second.imported, second.duplicates, second.failed),
                (2, 1, 0)
            );
            drop(app.update(Message::Imported(second)));
            assert!(app.import.is_none());
            let query = |project| Query {
                project: Some(project),
                ..Default::default()
            };
            let page = service.page(owner, query(target), 0, 10).await.unwrap();
            assert_eq!(page.total, 3);
            assert_eq!(
                service
                    .page(owner, query(other), 0, 10)
                    .await
                    .unwrap()
                    .total,
                0
            );
            for tile in page.tiles {
                assert!(
                    !service
                        .details(owner, tile.asset.id)
                        .await
                        .unwrap()
                        .referenced
                );
            }
            for (path, original) in [&a, &b, &c].into_iter().zip(originals) {
                assert_eq!(std::fs::read(path).unwrap(), original);
            }
            let unsupported = sources.join("unsupported.txt");
            std::fs::write(&unsupported, "not a picture").unwrap();
            drop(app.update(Message::Drop(DropMessage::File(unsupported))));
            let report = imported(ready(&mut app)).await;
            assert_eq!((report.imported, report.failed), (0, 1));
            assert!(report.failures[0].contains("不支持的文件类型"));
            service.close().await.unwrap();
        });
    }
}
