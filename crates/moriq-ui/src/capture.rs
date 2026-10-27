//! Development-only native capture scenarios. Never compiled into the shipping app.
use super::*;
impl App {
    /// 记录搜索交互的诊断快照，供验收核对展示与请求状态。
    pub(super) fn capture_search_snapshot(
        &self,
        screenshot: iced::window::Screenshot,
    ) -> Task<Message> {
        let Some(path) = self
            .launch
            .capture
            .as_ref()
            .map(|path| path.with_extension("pending.png"))
        else {
            return Task::none();
        };
        let state = format!(
            "tiles={} selected={:?} draft={} replacing={} loading={}\n",
            self.tiles.len(),
            self.viewer.selected.as_ref().map(|asset| asset.id),
            self.manager.notes_dirty,
            self.responsiveness.replacing(),
            self.loading
        );
        let service = self.service.clone();
        Task::perform(
            async move {
                service
                    .save_capture_evidence(path.with_extension("txt"), state)
                    .await?;
                service
                    .save_capture(
                        path,
                        screenshot.size.width,
                        screenshot.size.height,
                        screenshot.rgba.to_vec(),
                    )
                    .await
            },
            |result| Message::PlatformDone("交互验证截图已保存", result),
        )
    }
    /// 推进显式开启的截图验收步骤，满足条件后保存证据。
    pub(super) fn capture_tick(&mut self) -> Task<Message> {
        self.capture_ticks = self.capture_ticks.saturating_add(1);
        if self.launch.panel.as_deref() == Some("interaction-continuity") {
            match self.capture_ticks {
                3 => {
                    self.manager.notes = "交互验证草稿".into();
                    self.manager.notes_dirty = true;
                    // Hold this request to inspect a slow search without timing-dependent screenshots.
                    drop(self.update(Message::Search("no-matching-item".into())));
                    return iced::window::oldest()
                        .and_then(iced::window::screenshot)
                        .map(Message::InteractionSnapshot);
                }
                6 => return self.update(Message::CancelSearch),
                8 => {
                    let query = self
                        .viewer
                        .selected
                        .as_ref()
                        .map(|asset| asset.name.clone())
                        .unwrap_or_default();
                    return self.update(Message::Search(query));
                }
                11 if self.loading || self.responsiveness.replacing() => {
                    self.capture_ticks = 10;
                    return Task::none();
                }
                _ => {}
            }
        }
        if self.capture_ticks == 2
            && matches!(
                self.launch.panel.as_deref(),
                Some("library-picker" | "library-picker-compact")
            )
        {
            let show = self.update(Message::ShowLibraries);
            if self.launch.panel.as_deref() == Some("library-picker-compact") {
                return Task::batch([
                    show,
                    iced::window::oldest()
                        .and_then(|id| iced::window::resize(id, Size::new(1180.0, 740.0))),
                ]);
            }
            return show;
        }
        if self.launch.panel.as_deref() == Some("responsiveness") {
            match self.capture_ticks {
                2 => return self.update(Message::Navigate(2)),
                3 => return self.update(Message::Navigate(1)),
                4 => {
                    let tasks = ["will-not-match", "synth", "synthetic"]
                        .into_iter()
                        .map(|text| self.update(Message::Search(text.into())))
                        .collect::<Vec<_>>();
                    return Task::batch(tasks);
                }
                6 => {
                    if self.loading || self.responsiveness.pending() {
                        self.capture_ticks = 5;
                        return Task::none();
                    }
                    let tasks = (210..=230)
                        .map(|width| self.update(Message::Zoom(width as f32)))
                        .collect::<Vec<_>>();
                    return Task::batch(tasks);
                }
                9 if self.loading || self.responsiveness.pending() => {
                    self.capture_ticks = 8;
                    return Task::none();
                }
                _ => {}
            }
        }
        if self.capture_ticks == 2
            && matches!(
                self.launch.panel.as_deref(),
                Some("workspace-models" | "workspace-categories" | "workspace-assets")
            )
            && let Some(project) = self
                .library
                .as_ref()
                .and_then(|l| l.info.projects.first())
                .map(|p| p.id)
        {
            if self.launch.panel.as_deref() == Some("workspace-assets") {
                return self.update(Message::Project(project));
            }
            return self.ai_message(AiMessage::Workspace(Some(project)));
        }
        if self.capture_ticks == 5 && self.launch.panel.as_deref() == Some("workspace-categories") {
            if self.manager.organizer.ai_groups.is_empty() {
                self.capture_ticks = 4;
            }
            return self.category_message(categories::CategoryMessage::Open);
        }
        if self.launch.panel.as_deref() == Some("motion-proof") {
            if self.capture_ticks == 2 {
                if self.viewer.selected.is_none() {
                    self.capture_ticks = 1;
                    return Task::none();
                }
                return self.motion_proof_step(0);
            }
            if self.capture_ticks >= 12 {
                self.capture_ticks = 11;
                return Task::none();
            }
        }
        if self.launch.panel.as_deref() == Some("liquid-proof") && self.capture_ticks == 2 {
            if !self.ai.loaded {
                self.capture_ticks = 1;
                return if self.ai.loading {
                    Task::none()
                } else {
                    self.ai_message(AiMessage::Refresh)
                };
            }
            return self.liquid_proof_step(0);
        }
        if self.launch.panel.as_deref() == Some("interface-switch") {
            let event = match self.capture_ticks {
                2 => Some(Message::ShowSettings),
                3 | 6 => Some(Message::LanguageChanged(Language::English)),
                4 | 8 => Some(Message::AppearanceChanged(Appearance::Dark)),
                5 => Some(Message::LanguageChanged(Language::SimplifiedChinese)),
                7 => Some(Message::AppearanceChanged(Appearance::Light)),
                _ => None,
            };
            if let Some(event) = event {
                return self.update(event);
            }
        }
        if self.launch.panel.as_deref() == Some("video-motion") && self.capture_ticks == 6 {
            return iced::window::oldest()
                .and_then(iced::window::screenshot)
                .map(Message::VideoProof);
        }
        if self.launch.panel.as_deref() == Some("video-controls") {
            match self.capture_ticks {
                4 | 8 => return self.update(Message::VideoPause),
                6 => return self.update(Message::VideoSeek(0.6)),
                9 | 10 => return self.update(Message::VideoMute),
                _ => {}
            }
        }
        if matches!(
            self.launch.panel.as_deref(),
            Some("viewer-return" | "gallery-scroll" | "gallery-context" | "gallery-bottom")
        ) {
            if self.capture_ticks == 2 {
                let y = if matches!(
                    self.launch.panel.as_deref(),
                    Some("gallery-context" | "gallery-bottom")
                ) {
                    (self.active_masonry().map_or_else(
                        || self.grid().total_height(self.total as usize),
                        |layout| layout.height,
                    ) - self.grid().viewport_height)
                        .max(0.0)
                } else {
                    900.0
                };
                self.scroll_y = y;
                self.pending_scroll = Some(y);
                self.surfaces.restoring_gallery = Some(y);
                return self.refresh();
            }
            if self.capture_ticks == 5 {
                let index = self
                    .active_masonry()
                    .and_then(|m| {
                        m.columns
                            .iter()
                            .flatten()
                            .find(|p| p.top >= self.scroll_y)
                            .map(|p| p.index)
                    })
                    .unwrap_or(
                        (self.scroll_y / self.grid().row_height) as usize * self.grid().columns,
                    );
                if let Some(id) = index
                    .checked_sub(self.offset)
                    .and_then(|i| self.tiles.get(i))
                    .map(|t| t.asset.id)
                {
                    return self.update(if self.launch.panel.as_deref() == Some("viewer-return") {
                        Message::OpenImage(id)
                    } else {
                        Message::Select(id)
                    });
                }
            }
            if self.capture_ticks == 8 && self.launch.panel.as_deref() == Some("viewer-return") {
                return self.update(Message::Back);
            }
            if self.capture_ticks == 8
                && self.launch.panel.as_deref() == Some("gallery-context")
                && let Some(asset) = &self.viewer.selected
            {
                return self.update(Message::AssetContext(asset.id));
            }
        }
        if self.capture_ticks == 8 && self.launch.panel.as_deref() == Some("delete-project-final") {
            return self.deletion_message(DeletionMessage::Next);
        }
        if self.capture_ticks == 8 && self.launch.panel.as_deref() == Some("actual-regions") {
            return self.region_message(RegionMessage::Actual);
        }
        if self.capture_ticks == 8 && self.launch.panel.as_deref() == Some("viewer-leave") {
            return self.commit_navigation(Message::Navigate(2));
        }
        if self.capture_ticks == 6 && self.launch.panel.as_deref() == Some("library-delete") {
            return self.library_message(libraries::LibraryMessage::Next);
        }
        if self.capture_ticks == 8 {
            let scroll_target = match self.launch.panel.as_deref() {
                Some("overview-filenames" | "overview-filenames-compact") => {
                    Some(("overview", 100000.0))
                }
                Some("settings-bottom") => Some(("management", 100000.0)),
                Some("details-ai") => Some(("details", 220.0)),
                Some("details-ai-tags") => Some(("details", 620.0)),
                Some("details-bottom") => Some(("details", 100000.0)),
                _ => None,
            };
            if let Some((id, y)) = scroll_target {
                return iced::widget::operation::scroll_to(
                    id,
                    iced::widget::scrollable::AbsoluteOffset { x: 0.0, y },
                );
            }
        }
        if self.capture_ticks == 2 {
            if matches!(
                self.launch.panel.as_deref(),
                Some("details-ai" | "details-ai-tags")
            ) {
                self.detail_tab = 2;
            }
            match self.launch.panel.as_deref() {
                Some("overview-filenames" | "overview-filenames-compact") => {
                    let size = if self.launch.panel.as_deref() == Some("overview-filenames-compact")
                    {
                        Size::new(1180.0, 740.0)
                    } else {
                        Size::new(1600.0, 900.0)
                    };
                    return iced::window::oldest()
                        .and_then(move |id| iced::window::resize(id, size));
                }
                Some("drop-hover") => {
                    return self.update(Message::Drop(DropMessage::Hover(PathBuf::from(
                        "drop-feedback.png",
                    ))));
                }
                Some("ai-refresh") => {
                    return self.job_message(tasks::JobMessage::Queue(JobKind::QwenClassification));
                }
                Some("categories" | "categories-compact") => {
                    if self.manager.organizer.ai_groups.is_empty() {
                        self.capture_ticks = 1;
                    }
                    let open = self.category_message(categories::CategoryMessage::Open);
                    if self.launch.panel.as_deref() == Some("categories-compact") {
                        return Task::batch([
                            open,
                            iced::window::oldest()
                                .and_then(|id| iced::window::resize(id, Size::new(1180.0, 740.0))),
                        ]);
                    }
                    return open;
                }
                Some("projects") => {
                    if let Some(id) = self
                        .library
                        .as_ref()
                        .and_then(|l| l.info.projects.first())
                        .map(|p| p.id)
                    {
                        return self.manage_message(ManageMessage::ProjectMenu(id));
                    }
                }
                Some("library-manage" | "library-delete") => {
                    if let Some(root) = self.library.as_ref().map(|l| l.root.clone()) {
                        return self.library_message(libraries::LibraryMessage::Open(root));
                    }
                }
                Some("people") => {
                    return self.ai_message(AiMessage::People);
                }
                Some("category-gallery") => {
                    if let Some((category, _)) = self.manager.organizer.ai_categories.first() {
                        return self.update(Message::Ai(AiMessage::Category(category.clone())));
                    }
                    self.capture_ticks = 1;
                    return self.job_tick();
                }
                Some("model-switch") if self.screen == Screen::Models => {
                    if let Some((_, path)) = self.preferences.recent.get(1) {
                        return self.update(Message::Ai(AiMessage::SwitchLibrary(path.clone())));
                    }
                    self.capture_ticks = 1;
                    return Task::none();
                }
                Some("models-bottom") if self.screen == Screen::Models => {
                    return iced::widget::operation::scroll_to(
                        "models",
                        iced::widget::scrollable::AbsoluteOffset { x: 0., y: 420. },
                    );
                }
                Some("delete-project" | "delete-project-final") => {
                    let project = self
                        .library
                        .as_ref()
                        .and_then(|l| {
                            l.info
                                .projects
                                .iter()
                                .find(|p| p.count == 0)
                                .or_else(|| l.info.projects.first())
                        })
                        .map(|p| p.id);
                    if let Some(project) = project {
                        self.manager.project_target = Some(project);
                        self.manager.panel = Some(manage::Panel::Project);
                        return self.deletion_message(DeletionMessage::Project(project));
                    }
                }
                Some("notifications") => {
                    return self.journal_message(JournalMessage::Open(true));
                }
                Some("activity") => {
                    return self.journal_message(JournalMessage::Open(false));
                }
                Some("storage") => return self.storage_message(StorageMessage::Open),
                Some("rules") => return self.rule_message(RuleMessage::Open),
                Some("location") => {
                    return self.manage_message(ManageMessage::Location);
                }
                Some(
                    "expanded" | "actual-regions" | "viewer-leave" | "video-motion"
                    | "video-controls",
                ) => {
                    return self.set_viewer(true);
                }
                Some("compare") => return self.compare_message(CompareMessage::Open),
                Some("help") => return self.manage_message(ManageMessage::Help),
                _ => {}
            }
            self.manager.panel = match self.launch.panel.as_deref() {
                Some("settings" | "settings-bottom") => {
                    self.dialog = Some(Dialog::Settings);
                    None
                }
                Some("export") => Some(manage::Panel::ExportOptions),
                Some("tags") => Some(manage::Panel::Tags),
                Some("projects") => Some(manage::Panel::Project),
                Some("tools") => Some(manage::Panel::Menu),
                Some("filters") => Some(manage::Panel::Filters),
                Some("organize") => Some(manage::Panel::Organize),
                Some("maintenance") => Some(manage::Panel::Maintenance),
                Some("collections") => Some(manage::Panel::Collections),
                Some("synco") => Some(manage::Panel::Synco),
                Some("tasks") => Some(manage::Panel::Tasks),
                Some("asset-menu") => Some(manage::Panel::AssetMenu),
                Some("asset-projects") => Some(manage::Panel::AssetProjects),
                Some("versions") => Some(manage::Panel::Versions),
                Some("views") => Some(manage::Panel::ViewOptions),
                _ => None,
            };
        }
        if self.capture_ticks == 12 {
            if self.launch.panel.as_deref() == Some("browse-stress")
                && self.capture_stress_steps < 80
            {
                self.capture_ticks = 11;
                let step = self.capture_stress_steps;
                self.capture_stress_steps += 1;
                if let Some(id) = self
                    .tiles
                    .get(step as usize % self.tiles.len().max(1))
                    .map(|t| t.asset.id)
                {
                    return match step % 4 {
                        0 | 1 => {
                            let open = self.update(Message::OpenImage(id));
                            let zoom = self.region_message(RegionMessage::Actual);
                            Task::batch([open, zoom])
                        }
                        2 => self.set_viewer(false),
                        _ => self.update(Message::Select(id)),
                    };
                }
            }
            if self.launch.panel.as_deref() == Some("people")
                && (self.manager.organizer.people.is_empty()
                    || self
                        .manager
                        .organizer
                        .people
                        .iter()
                        .filter(|p| p.count > 0)
                        .any(|p| !self.ai.portraits.contains_key(&p.id)))
            {
                self.capture_ticks = 11;
                return self.job_tick();
            }
            if self.launch.panel.as_deref() == Some("ai-refresh")
                && (!self.manager.organizer.jobs.iter().any(|job| {
                    job.kind == JobKind::QwenClassification && job.state == JobState::Completed
                }) || self.manager.organizer.ai_categories.is_empty())
            {
                self.capture_ticks = 11;
                return self.job_tick();
            }
            return iced::window::oldest()
                .and_then(iced::window::screenshot)
                .map(Message::Captured);
        }
        self.job_tick()
    }
}
