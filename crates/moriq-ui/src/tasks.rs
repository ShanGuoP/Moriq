use super::*;
use crate::components::*;
use iced::widget::column;
#[derive(Default)]
pub(super) struct TaskState {
    pub loading: bool,
    pub controls_busy: bool,
    pub error: Option<String>,
    pub request: u64,
    pub ticks: u8,
    pub(super) analysis_progress: std::collections::HashMap<String, (u64, u64, JobState)>,
    pub(super) analysis_gallery_dirty: Option<Query>,
    pub(super) analysis_masonry_generation: Option<u64>,
    pub(super) analysis_masonry: Option<std::sync::Arc<moriq_render::MasonryIndex>>,
    pub(super) failures: Option<(LibraryId, String, Vec<JobFailure>)>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    /// 验证空任务投影仍可刷新，且旧回执被拒绝。
    fn task_refresh_runs_with_empty_projection_and_rejects_stale_receipts() {
        let mut app = crate::tests::app();
        let id = app.library.as_ref().unwrap().info.id;
        app.jobs.ticks = 3;
        drop(app.job_tick());
        assert!(app.jobs.loading);
        let request = app.jobs.request;
        drop(app.job_message(JobMessage::Refreshed(
            LibraryId::default(),
            request,
            Err("other library".into()),
        )));
        drop(app.job_message(JobMessage::Refreshed(
            id,
            request - 1,
            Err("old request".into()),
        )));
        assert!(app.jobs.loading);
        assert!(app.jobs.error.is_none());
        drop(app.job_message(JobMessage::Refreshed(
            id,
            request,
            Err("job query failed".into()),
        )));
        assert!(!app.jobs.loading);
        assert_eq!(app.jobs.error.as_deref(), Some("job query failed"));
        assert!(app.error.is_none());
        app.jobs.ticks = 3;
        drop(app.job_tick());
        drop(app.job_message(JobMessage::Refreshed(id, app.jobs.request, Ok(vec![]))));
        assert!(app.jobs.error.is_none());
    }
    #[test]
    /// 验证分析进度刷新分类与筛选布局，不造成重复轮询。
    fn analysis_progress_refreshes_categories_and_filtered_layout_without_polling_loops() {
        let mut app = crate::tests::app();
        let owner = app.library.as_ref().unwrap().info.id;
        app.screen = Screen::Assets;
        app.query.category = "风景".into();
        app.scroll_y = 500.0;
        app.jobs.request = 7;
        let mut job = BackgroundJob {
            id: "analysis-job".into(),
            kind: JobKind::Analysis(ModelId::ChineseClip),
            state: JobState::Running,
            priority: 0,
            completed: 0,
            total: 15,
            failed: 0,
            error: String::new(),
        };
        assert_eq!(
            app.job_message(JobMessage::Refreshed(owner, 7, Ok(vec![job.clone()])))
                .units(),
            0
        );
        let generation = app.layout_generation;
        job.completed = 4;
        assert!(
            app.job_message(JobMessage::Refreshed(owner, 7, Ok(vec![job.clone()])))
                .units()
                > 0
        );
        assert!(app.layout_generation > generation);
        assert_eq!(app.query.category, "风景");
        assert_eq!(app.scroll_y, 500.0);
        assert_eq!(
            app.job_message(JobMessage::Refreshed(owner, 7, Ok(vec![job.clone()])))
                .units(),
            0
        );
        job.completed = 15;
        job.state = JobState::Completed;
        assert!(
            app.job_message(JobMessage::Refreshed(owner, 7, Ok(vec![job.clone()])))
                .units()
                > 0
        );
        assert_eq!(
            app.job_message(JobMessage::Refreshed(owner, 7, Ok(vec![job])))
                .units(),
            0
        );
    }
}
#[derive(Debug, Clone)]
pub enum JobMessage {
    Queue(JobKind),
    Control(String, JobCommand),
    ControlModels(JobCommand),
    Controlled(LibraryId, Result<(), String>),
    Stored(LibraryId, Result<(), String>),
    Refreshed(LibraryId, u64, Result<Vec<BackgroundJob>, String>),
    Failures(String),
    FailuresLoaded(LibraryId, String, Result<Vec<JobFailure>, String>),
}
impl App {
    /// 合并后台分析的图库更新，保留旧布局并等待在途页面读取完成。
    pub(super) fn refresh_analysis_gallery(&mut self) -> Task<Message> {
        let Some(query) = self.jobs.analysis_gallery_dirty.as_ref() else {
            return Task::none();
        };
        if query != &self.query || self.screen != Screen::Assets || self.library_picker {
            self.jobs.analysis_gallery_dirty = None;
            return Task::none();
        }
        if self.page_task.is_some()
            || self.masonry_task.is_some()
            || self.responsiveness.pending()
            || self.responsiveness.replacing()
        {
            return Task::none();
        }
        self.jobs.analysis_gallery_dirty = None;
        if self.masonry_mode && !self.list && self.import.is_none() {
            let task = self.rebuild_masonry();
            if self.masonry_task.is_some() {
                self.jobs.analysis_masonry_generation = Some(self.layout_generation);
            }
            task
        } else {
            self.refresh()
        }
    }

    /// 刷新后台任务投影；任务执行由应用调度器独立驱动。
    pub(super) fn job_tick(&mut self) -> Task<Message> {
        let gallery = self.refresh_analysis_gallery();
        self.jobs.ticks = self.jobs.ticks.wrapping_add(1);
        if self.jobs.loading || !self.jobs.ticks.is_multiple_of(4) {
            return gallery;
        }
        let Some(library) = &self.library else {
            return gallery;
        };
        let id = library.info.id;
        self.jobs.loading = true;
        self.jobs.request += 1;
        let request = self.jobs.request;
        let service = self.service.clone();
        Task::batch([
            gallery,
            Task::perform(async move { service.background_jobs(id).await }, move |r| {
                Message::Jobs(JobMessage::Refreshed(id, request, r))
            }),
        ])
    }
    /// 处理任务控制与进度回执，刷新受结果影响的分类和布局。
    pub(super) fn job_message(&mut self, event: JobMessage) -> Task<Message> {
        let Some(library) = &self.library else {
            return Task::none();
        };
        let id = library.info.id;
        match event {
            JobMessage::Failures(job) => {
                let service = self.service.clone();
                let receipt = job.clone();
                self.jobs.failures = Some((id, job.clone(), Vec::new()));
                return Task::perform(
                    async move { service.job_failures(id, job).await },
                    move |r| Message::Jobs(JobMessage::FailuresLoaded(id, receipt.clone(), r)),
                );
            }
            JobMessage::FailuresLoaded(owner, job, result)
                if owner == id
                    && self
                        .jobs
                        .failures
                        .as_ref()
                        .is_some_and(|(library, selected, _)| {
                            *library == id && *selected == job
                        }) =>
            {
                match result {
                    Ok(items) => self.jobs.failures = Some((id, job, items)),
                    Err(e) => self.error = Some(e),
                }
            }
            JobMessage::Queue(kind) => {
                let service = self.service.clone();
                return Task::perform(
                    async move {
                        service
                            .queue_media_job(id, kind)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    move |r| Message::Jobs(JobMessage::Stored(id, r)),
                );
            }
            JobMessage::Controlled(owner, result) if owner == id => {
                self.jobs.controls_busy = false;
                if let Err(e) = result {
                    self.error = Some(e);
                }
                return self.refresh_organizer();
            }
            JobMessage::ControlModels(command) if !self.jobs.controls_busy => {
                self.jobs.controls_busy = true;
                let service = self.service.clone();
                return Task::perform(
                    async move {
                        service
                            .control_model_jobs(id, command)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    move |r| Message::Jobs(JobMessage::Controlled(id, r)),
                );
            }
            JobMessage::Control(job, command) if !self.jobs.controls_busy => {
                self.jobs.controls_busy = true;
                let service = self.service.clone();
                return Task::perform(
                    async move {
                        service
                            .control_job(id, job, command)
                            .await
                            .map_err(|e| e.to_string())
                    },
                    move |r| Message::Jobs(JobMessage::Controlled(id, r)),
                );
            }
            JobMessage::Stored(owner, result) if owner == id => {
                if let Err(e) = result {
                    self.error = Some(e)
                }
                return self.refresh_organizer();
            }
            JobMessage::Refreshed(owner, request, result)
                if owner == id && request == self.jobs.request =>
            {
                self.jobs.loading = false;
                match result {
                    Ok(jobs) => {
                        let progress = jobs
                            .iter()
                            .filter(|job| job.kind.model().is_some())
                            .map(|job| (job.id.clone(), (job.completed, job.failed, job.state)))
                            .collect::<std::collections::HashMap<_, _>>();
                        let mut faces_changed = false;
                        let mut content_changed = false;
                        for job in jobs.iter().filter(|job| job.kind.model().is_some()) {
                            let value = (job.completed, job.failed, job.state);
                            if self.jobs.analysis_progress.get(&job.id) != Some(&value)
                                && (job.completed > 0 || job.state.is_finished())
                            {
                                if matches!(
                                    job.kind,
                                    JobKind::Analysis(ModelId::Faces) | JobKind::QwenFaces
                                ) {
                                    faces_changed = true;
                                } else {
                                    content_changed = true;
                                }
                            }
                        }
                        self.jobs.analysis_progress = progress;
                        self.manager.organizer.jobs = jobs;
                        self.jobs.error = None;
                        self.manager
                            .organizer
                            .errors
                            .retain(|(section, _)| *section != OrganizerSection::Jobs);
                        if faces_changed || content_changed {
                            if self.screen == Screen::Assets
                                && ((faces_changed
                                    && (self.query.person.is_some()
                                        || self.query.face_analyzed.is_some()))
                                    || (content_changed
                                        && (!self.query.category.is_empty()
                                            || !self.query.medium.is_empty()
                                            || !self.query.themes.is_empty()
                                            || !self.query.text.is_empty()
                                            || !self.query.tag.is_empty())))
                            {
                                self.jobs.analysis_gallery_dirty = Some(self.query.clone());
                            }
                            let gallery = self.refresh_analysis_gallery();
                            let catalog = if content_changed {
                                self.refresh_catalog()
                            } else {
                                self.refresh_organizer()
                            };
                            return Task::batch([catalog, self.refresh_details(), gallery]);
                        }
                    }
                    Err(e) => {
                        if self.jobs.error.as_ref() != Some(&e) {
                            tracing::warn!(error = %e, "background job projection could not be loaded");
                        }
                        self.jobs.error = Some(e);
                    }
                }
            }
            _ => {}
        }
        Task::none()
    }
    /// 构建模型任务暂停、恢复和停止控件。
    pub(super) fn model_job_controls(&self) -> Element<'_, Message> {
        let language = self.preferences.language;
        let jobs: Vec<_> = self
            .manager
            .organizer
            .jobs
            .iter()
            .filter(|j| j.kind.model().is_some() && j.state.is_active())
            .collect();
        if jobs.is_empty() {
            return iced::widget::Space::new().height(0).into();
        }
        let paused = jobs.iter().filter(|j| j.state == JobState::Paused).count();
        let mut actions = iced::widget::Row::new().spacing(8);
        for (label, command, enabled) in [
            (
                tr!(language,"暂停全部"=>"Pause all"),
                JobCommand::Pause,
                paused < jobs.len(),
            ),
            (
                tr!(language,"继续任务"=>"Resume"),
                JobCommand::Resume,
                paused > 0,
            ),
            (
                tr!(language,"完全停止"=>"Stop all"),
                JobCommand::Cancel,
                true,
            ),
        ] {
            actions = actions.push(
                action(label, Message::Jobs(JobMessage::ControlModels(command)))
                    .on_press_maybe(
                        (enabled && !self.jobs.controls_busy && !self.switching)
                            .then_some(Message::Jobs(JobMessage::ControlModels(command))),
                    )
                    .style(if matches!(command, JobCommand::Cancel) {
                        crate::theme::destructive
                    } else {
                        crate::theme::secondary
                    }),
            );
        }
        form_card(column![heading(trf!(language,"{} 个模型任务 · {} 个暂停"=>"{} model tasks · {} paused",jobs.len(),paused),14),
            actions.wrap(),muted(tr!(language,"暂停后可继续未完成素材；完全停止不会自动恢复，已完成的分析结果保留。"=>"Pause to resume unfinished assets later. Stopped tasks never auto-resume; completed results are preserved."))].spacing(10))
    }
    /// 构建后台任务列表、进度和失败操作入口。
    pub(super) fn tasks_panel(&self) -> Element<'_, Message> {
        let language = self.preferences.language;
        let mut view = column![muted(
            tr!(language, "此处显示当前导入、AI 分析、索引、资源检查、空间统计与 Synco 交接。" => "Current imports, AI analysis, indexing, integrity checks, storage calculations and Synco handoffs appear here.")
        )]
        .spacing(12);
        if self.jobs.error.is_some() {
            view = view.push(muted(
                tr!(language, "任务列表暂时无法读取。请重新打开资源库重试，诊断信息已写入日志。" => "Task list is temporarily unavailable. Reopen the library to retry. Diagnostics were written to the log."),
            ));
        }
        view = view
            .push(action(
                tr!(language, "原文件完整性检查（可暂停与断点继续）" => "Original file integrity check (pause and resume supported)"),
                Message::Jobs(JobMessage::Queue(JobKind::Integrity)),
            ))
            .push(crate::components::action(
                tr!(language, "建立/更新近似图片索引" => "Build / update image similarity index"),
                Message::Jobs(JobMessage::Queue(JobKind::Fingerprints)),
            ))
            .push(crate::components::action(
                tr!(language, "重建全部图片缩略图" => "Rebuild all image thumbnails"),
                Message::Jobs(JobMessage::Queue(JobKind::Thumbnails)),
            ))
            .push(muted(
                tr!(language, "索引与缩略图任务按优先级逐项执行；普通任务在当前图片完成后暂停；模型任务会中断当前推理，保留已完成进度。" => "Indexing and thumbnail tasks run by priority, one image at a time. Ordinary tasks pause after the current image; model tasks interrupt inference and preserve completed progress."),
            ));
        let mut count = 0;
        for job in &self.manager.organizer.jobs {
            count += 1;
            let mut job_view = column![].spacing(12);
            let title = match job.kind {
                JobKind::Integrity => {
                    tr!(language, "原文件完整性检查" => "Original file integrity check")
                }
                JobKind::Batch => tr!(language, "批量字段编辑" => "Batch field editing"),
                JobKind::Metadata => tr!(language, "EXIF 与 GPS 索引" => "EXIF and GPS index"),
                JobKind::Fingerprints => tr!(language, "近似图片索引" => "Image similarity index"),
                JobKind::Thumbnails => tr!(language, "缩略图重建" => "Thumbnail rebuild"),
                JobKind::Analysis(ModelId::ChineseClip) => {
                    tr!(language, "自动分类与语义索引" => "Automatic categories and semantic index")
                }
                JobKind::Analysis(ModelId::Florence) => {
                    tr!(language, "图片描述、OCR 与物体识别" => "Image descriptions, OCR and object recognition")
                }
                JobKind::Analysis(ModelId::Faces) => {
                    tr!(language, "人脸识别与人物聚类" => "Face recognition and people clustering")
                }
                JobKind::Analysis(ModelId::Qwen35_4b) => {
                    tr!(language, "Qwen 图片内容分析" => "Qwen image content analysis")
                }
                JobKind::QwenClassification => {
                    tr!(language, "Qwen 媒介与主题分类" => "Qwen medium and theme classification")
                }
                JobKind::QwenDescription => {
                    tr!(language, "Qwen 中文标签与描述" => "Qwen Chinese tags and descriptions")
                }
                JobKind::QwenFaces => {
                    tr!(language,"Qwen 人脸识别与人物分组"=>"Qwen face recognition and people grouping")
                }
            };
            job_view = job_view
                .push(heading(
                    format!("{title} · {} / {}", job.completed, job.total),
                    15,
                ))
                .push(muted(
                    trf!(language, "{} · 优先级 {} · 失败 {}" => "{} · Priority {} · {} failed",
                        match job.state {
                            JobState::Queued => tr!(language, "等待中" => "Queued"),
                            JobState::Running => tr!(language, "进行中" => "Running"),
                            JobState::Paused => tr!(language, "已暂停" => "Paused"),
                            JobState::Completed => tr!(language, "已完成" => "Completed"),
                            JobState::Cancelled => tr!(language, "已停止" => "Stopped"),
                        }, job.priority, job.failed
                    ),
                ));
            if !job.error.is_empty() {
                job_view = job_view.push(muted(&job.error));
            }
            if job.failed > 0 {
                job_view = job_view.push(action(
                    tr!(language, "查看失败明细（前100项）" => "View failures (first 100)"),
                    Message::Jobs(JobMessage::Failures(job.id.clone())),
                ));
                if job.state.is_finished() {
                    job_view = job_view.push(action(
                        tr!(language, "仅重试失败素材" => "Retry failed assets only"),
                        Message::Jobs(JobMessage::Control(job.id.clone(), JobCommand::RetryFailed)),
                    ));
                }
                if let Some((owner, selected, items)) = &self.jobs.failures
                    && self.library.as_ref().is_some_and(|l| l.info.id == *owner)
                    && *selected == job.id
                {
                    for item in items {
                        job_view = job_view.push(muted(format!("{}：{}", item.name, item.message)));
                        if let Some(asset) = item.asset {
                            job_view = job_view.push(action(
                                tr!(language, "查看此素材" => "View this asset"),
                                Message::Journal(journal::JournalMessage::OpenAsset(asset)),
                            ));
                        }
                    }
                }
            }
            if job.total > 0 {
                job_view = job_view.push(
                    iced::widget::progress_bar(
                        0.0..=1.0,
                        (job.completed as f32 / job.total as f32).min(1.0),
                    )
                    .girth(5)
                    .style(crate::theme::progress),
                );
            }
            if job.state.is_active() {
                let command = if job.state == JobState::Paused {
                    JobCommand::Resume
                } else {
                    JobCommand::Pause
                };
                job_view = job_view.push(
                    iced::widget::row![
                        action(
                            if job.state == JobState::Paused {
                                tr!(language, "继续任务" => "Resume")
                            } else {
                                tr!(language, "暂停任务" => "Pause")
                            },
                            Message::Jobs(JobMessage::Control(job.id.clone(), command))
                        )
                        .style(crate::theme::primary),
                        action(
                            tr!(language, "优先执行" => "Prioritize"),
                            Message::Jobs(JobMessage::Control(
                                job.id.clone(),
                                JobCommand::Priority((job.priority + 10).min(100))
                            ))
                        ),
                        action(
                            tr!(language, "降低优先级" => "Lower priority"),
                            Message::Jobs(JobMessage::Control(
                                job.id.clone(),
                                JobCommand::Priority((job.priority - 10).max(-100))
                            ))
                        ),
                        action(
                            tr!(language, "完全停止" => "Stop"),
                            Message::Jobs(JobMessage::Control(job.id.clone(), JobCommand::Cancel))
                        )
                        .style(crate::theme::destructive),
                    ]
                    .spacing(8)
                    .wrap(),
                );
            }
            view = view.push(form_card(job_view));
        }
        if let Some(control) = &self.import {
            count += 1;
            view = view.push(heading(tr!(language, "图片导入" => "Image import"), 15)).push(muted(trf!(language, "已处理 {}，已发现 {}，解码 {} / 上限 {}，峰值 {}，等待内存 {}" => "Processed {}, discovered {}, decoding {} / limit {}, peak {}, waiting for memory {}",
                control.finished.load(Ordering::Relaxed),
                control.discovered.load(Ordering::Relaxed),
                control.active.load(Ordering::Relaxed),
                control.workers,
                control.peak.load(Ordering::Relaxed),
                control.waiting_memory.load(Ordering::Relaxed)
            )));
            if !control.cancel.load(Ordering::Relaxed) {
                view = view.push(action(
                    tr!(language, "停止导入" => "Stop import"),
                    Message::CancelImport,
                ));
            } else {
                view = view.push(muted(
                    tr!(language, "正在停止并收尾…" => "Stopping and finishing up…"),
                ));
            }
        }
        if self.manager.maintenance.is_some() {
            count += 1;
            view = view.push(action(
                tr!(language, "查看资源检查进度" => "View integrity check progress"),
                Message::Manage(ManageMessage::Maintenance),
            ));
        }
        if self.storage.control.is_some() {
            count += 1;
            view = view.push(action(
                tr!(language, "查看空间统计进度" => "View storage calculation progress"),
                Message::Storage(StorageMessage::Open),
            ));
        }
        let pending = self
            .manager
            .organizer
            .synco
            .iter()
            .filter(|t| {
                !matches!(t.state.as_str(), "delivered" | "failed" | "cancelled") || t.ack.is_some()
            })
            .count();
        if pending > 0 {
            count += pending;
            view = view.push(action(
                &trf!(language, "查看 {} 个 Synco 交接任务" => "View {} Synco handoff tasks", pending),
                Message::Manage(ManageMessage::Synco),
            ));
        }
        if count == 0 {
            view = view.push(muted(
                tr!(language, "当前没有后台任务。" => "No background tasks."),
            ));
        }
        view.into()
    }
}
