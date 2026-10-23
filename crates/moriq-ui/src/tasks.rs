use super::*;
use crate::components::*;
use iced::widget::column;
#[derive(Default)]
pub(super) struct TaskState {
    pub loading: bool,
    pub error: Option<String>,
    pub request: u64,
    pub ticks: u8,
    pub(super) analysis_progress: std::collections::HashMap<String, (u64, u64, String)>,
    pub(super) failures: Option<(LibraryId, String, Vec<JobFailure>)>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
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
    fn analysis_progress_refreshes_categories_and_filtered_layout_without_polling_loops() {
        let mut app = crate::tests::app();
        let owner = app.library.as_ref().unwrap().info.id;
        app.screen = Screen::Assets;
        app.query.category = "风景".into();
        app.scroll_y = 500.0;
        app.jobs.request = 7;
        let mut job = BackgroundJob {
            id: "analysis-job".into(),
            kind: "analysis:chinese-clip".into(),
            state: "running".into(),
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
        job.state = "completed".into();
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
    Queue(String),
    Control(String, JobCommand),
    Stored(LibraryId, Result<(), String>),
    Refreshed(LibraryId, u64, Result<Vec<BackgroundJob>, String>),
    Failures(String),
    FailuresLoaded(LibraryId, String, Result<Vec<JobFailure>, String>),
}
impl App {
    pub(super) fn job_tick(&mut self) -> Task<Message> {
        self.jobs.ticks = self.jobs.ticks.wrapping_add(1);
        if self.jobs.loading || !self.jobs.ticks.is_multiple_of(4) {
            return Task::none();
        }
        let Some(library) = &self.library else {
            return Task::none();
        };
        let id = library.info.id;
        self.jobs.loading = true;
        self.jobs.request += 1;
        let request = self.jobs.request;
        let service = self.service.clone();
        Task::perform(async move { service.background_jobs(id).await }, move |r| {
            Message::Jobs(JobMessage::Refreshed(id, request, r))
        })
    }
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
                    async move { service.queue_media_job(id, kind).await },
                    move |r| Message::Jobs(JobMessage::Stored(id, r)),
                );
            }
            JobMessage::Control(job, command) => {
                let service = self.service.clone();
                return Task::perform(
                    async move { service.control_job(id, job, command).await },
                    move |r| Message::Jobs(JobMessage::Stored(id, r)),
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
                            .filter(|job| job.kind.starts_with("analysis:"))
                            .map(|job| {
                                (
                                    job.id.clone(),
                                    (job.completed, job.failed, job.state.clone()),
                                )
                            })
                            .collect::<std::collections::HashMap<_, _>>();
                        let changed = progress.iter().any(|(id, value)| {
                            self.jobs.analysis_progress.get(id) != Some(value)
                                && (value.0 > 0
                                    || matches!(value.2.as_str(), "completed" | "cancelled"))
                        });
                        self.jobs.analysis_progress = progress;
                        self.manager.organizer.jobs = jobs;
                        self.jobs.error = None;
                        self.manager
                            .organizer
                            .errors
                            .retain(|(section, _)| *section != OrganizerSection::Jobs);
                        if changed {
                            let gallery = if self.screen == Screen::Assets
                                && (!self.query.category.is_empty() || self.query.person.is_some())
                            {
                                self.masonry = None;
                                Task::batch([self.refresh(), self.rebuild_masonry()])
                            } else {
                                Task::none()
                            };
                            return Task::batch([
                                self.refresh_organizer(),
                                self.refresh_details(),
                                gallery,
                            ]);
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
                Message::Jobs(JobMessage::Queue("integrity".into())),
            ))
            .push(crate::components::action(
                tr!(language, "建立/更新近似图片索引" => "Build / update image similarity index"),
                Message::Jobs(JobMessage::Queue("fingerprints".into())),
            ))
            .push(crate::components::action(
                tr!(language, "重建全部图片缩略图" => "Rebuild all image thumbnails"),
                Message::Jobs(JobMessage::Queue("thumbnails".into())),
            ))
            .push(muted(
                tr!(language, "索引与缩略图任务按优先级逐项执行；暂停在当前图片完成后生效，重开资源库继续。" => "Indexing and thumbnail tasks run by priority, one image at a time. Pausing takes effect after the current image; reopen the library to resume."),
            ));
        let mut count = 0;
        for job in &self.manager.organizer.jobs {
            count += 1;
            let title = match job.kind.as_str() {
                "integrity" => tr!(language, "原文件完整性检查" => "Original file integrity check"),
                "batch" => tr!(language, "批量字段编辑" => "Batch field editing"),
                "metadata" => tr!(language, "EXIF 与 GPS 索引" => "EXIF and GPS index"),
                "fingerprints" => tr!(language, "近似图片索引" => "Image similarity index"),
                "thumbnails" => tr!(language, "缩略图重建" => "Thumbnail rebuild"),
                "analysis:chinese-clip" => {
                    tr!(language, "自动分类与语义索引" => "Automatic categories and semantic index")
                }
                "analysis:florence" => {
                    tr!(language, "图片描述、OCR 与物体识别" => "Image descriptions, OCR and object recognition")
                }
                "analysis:faces" => {
                    tr!(language, "人脸识别与人物聚类" => "Face recognition and people clustering")
                }
                _ => tr!(language, "后台处理" => "Background processing"),
            };
            view = view
                .push(heading(
                    format!("{title} · {} / {}", job.completed, job.total),
                    15,
                ))
                .push(muted(
                    trf!(language, "{} · 优先级 {} · 失败 {}" => "{} · Priority {} · {} failed",
                        job.state, job.priority, job.failed
                    ),
                ));
            if !job.error.is_empty() {
                view = view.push(muted(&job.error));
            }
            if job.failed > 0 {
                view = view.push(action(
                    tr!(language, "查看失败明细（前100项）" => "View failures (first 100)"),
                    Message::Jobs(JobMessage::Failures(job.id.clone())),
                ));
                if matches!(job.state.as_str(), "completed" | "cancelled") {
                    view = view.push(action(
                        tr!(language, "仅重试失败素材" => "Retry failed assets only"),
                        Message::Jobs(JobMessage::Control(job.id.clone(), JobCommand::RetryFailed)),
                    ));
                }
                if let Some((owner, selected, items)) = &self.jobs.failures
                    && self.library.as_ref().is_some_and(|l| l.info.id == *owner)
                    && *selected == job.id
                {
                    for item in items {
                        view = view.push(muted(format!("{}：{}", item.name, item.message)));
                        if let Some(asset) = item.asset {
                            view = view.push(action(
                                tr!(language, "查看此素材" => "View this asset"),
                                Message::Journal(journal::JournalMessage::OpenAsset(asset)),
                            ));
                        }
                    }
                }
            }
            if matches!(job.state.as_str(), "queued" | "running" | "paused") {
                let command = if job.state == "paused" {
                    JobCommand::Resume
                } else {
                    JobCommand::Pause
                };
                view = view
                    .push(crate::components::action(
                        if job.state == "paused" {
                            tr!(language, "继续任务" => "Resume task")
                        } else {
                            tr!(language, "暂停任务" => "Pause task")
                        },
                        Message::Jobs(JobMessage::Control(job.id.clone(), command)),
                    ))
                    .push(crate::components::action(
                        tr!(language, "提高优先级" => "Raise priority"),
                        Message::Jobs(JobMessage::Control(
                            job.id.clone(),
                            JobCommand::Priority((job.priority + 10).min(100)),
                        )),
                    ))
                    .push(crate::components::action(
                        tr!(language, "取消任务" => "Cancel task"),
                        Message::Jobs(JobMessage::Control(job.id.clone(), JobCommand::Cancel)),
                    ));
                view = view.push(action(
                    tr!(language, "降低优先级" => "Lower priority"),
                    Message::Jobs(JobMessage::Control(
                        job.id.clone(),
                        JobCommand::Priority((job.priority - 10).max(-100)),
                    )),
                ));
            }
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
