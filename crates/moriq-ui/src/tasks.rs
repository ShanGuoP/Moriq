use super::*;
use crate::components::*;
use iced::widget::column;
#[derive(Default)]
pub(super) struct Scheduler {
    pub driving: bool,
    pub request: u64,
    pub ticks: u8,
    pub(super) failures: Option<(LibraryId, String, Vec<JobFailure>)>,
}
#[derive(Debug, Clone)]
pub enum JobMessage {
    Queue(String),
    Control(String, JobCommand),
    Stored(LibraryId, Result<(), String>),
    Step(LibraryId, u64, Result<(), String>),
    Failures(String),
    FailuresLoaded(LibraryId, String, Result<Vec<JobFailure>, String>),
}
impl App {
    pub(super) fn job_tick(&mut self) -> Task<Message> {
        self.jobs.ticks = self.jobs.ticks.wrapping_add(1);
        if self.jobs.driving
            || self.jobs.ticks % 4 != 0
            || !self
                .manager
                .organizer
                .jobs
                .iter()
                .any(|j| matches!(j.state.as_str(), "queued" | "running"))
        {
            return Task::none();
        }
        let Some(library) = &self.library else {
            return Task::none();
        };
        let id = library.info.id;
        self.jobs.driving = true;
        self.jobs.request += 1;
        let request = self.jobs.request;
        let service = self.service.clone();
        Task::perform(async move { service.drive_job_step(id).await }, move |r| {
            Message::Jobs(JobMessage::Step(id, request, r))
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
            JobMessage::Step(owner, request, result)
                if owner == id && request == self.jobs.request =>
            {
                self.jobs.driving = false;
                if let Err(e) = result {
                    self.error = Some(e)
                }
                return self.refresh_organizer();
            }
            _ => {}
        }
        Task::none()
    }
    pub(super) fn tasks_panel(&self) -> Element<'_, Message> {
        let mut view = column![muted(
            "此处显示当前导入、AI 分析、索引、资源检查、空间统计与 Synco 交接。"
        )]
        .spacing(12);
        view = view
            .push(action(
                "原文件完整性检查（可暂停与断点继续）",
                Message::Jobs(JobMessage::Queue("integrity".into())),
            ))
            .push(crate::components::action(
                "建立/更新近似图片索引",
                Message::Jobs(JobMessage::Queue("fingerprints".into())),
            ))
            .push(crate::components::action(
                "重建全部图片缩略图",
                Message::Jobs(JobMessage::Queue("thumbnails".into())),
            ))
            .push(muted(
                "索引与缩略图任务按优先级逐项执行；暂停在当前图片完成后生效，重开资源库继续。",
            ));
        let mut count = 0;
        for job in &self.manager.organizer.jobs {
            count += 1;
            let title = match job.kind.as_str() {
                "integrity" => "原文件完整性检查",
                "batch" => "批量字段编辑",
                "metadata" => "EXIF 与 GPS 索引",
                "fingerprints" => "近似图片索引",
                "thumbnails" => "缩略图重建",
                "analysis:chinese-clip" => "自动分类与语义索引",
                "analysis:florence" => "图片描述、OCR 与物体识别",
                "analysis:faces" => "人脸识别与人物聚类",
                _ => "后台处理",
            };
            view = view
                .push(heading(
                    format!("{title} · {} / {}", job.completed, job.total),
                    15,
                ))
                .push(muted(format!(
                    "{} · 优先级 {} · 失败 {}",
                    job.state, job.priority, job.failed
                )));
            if !job.error.is_empty() {
                view = view.push(muted(&job.error));
            }
            if job.failed > 0 {
                view = view.push(action(
                    "查看失败明细（前100项）",
                    Message::Jobs(JobMessage::Failures(job.id.clone())),
                ));
                if matches!(job.state.as_str(), "completed" | "cancelled") {
                    view = view.push(action(
                        "仅重试失败素材",
                        Message::Jobs(JobMessage::Control(job.id.clone(), JobCommand::RetryFailed)),
                    ));
                }
                if let Some((owner, selected, items)) = &self.jobs.failures {
                    if self.library.as_ref().is_some_and(|l| l.info.id == *owner)
                        && *selected == job.id
                    {
                        for item in items {
                            view = view.push(muted(format!("{}：{}", item.name, item.message)));
                            if let Some(asset) = item.asset {
                                view = view.push(action(
                                    "查看此素材",
                                    Message::Journal(journal::JournalMessage::OpenAsset(asset)),
                                ));
                            }
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
                            "继续任务"
                        } else {
                            "暂停任务"
                        },
                        Message::Jobs(JobMessage::Control(job.id.clone(), command)),
                    ))
                    .push(crate::components::action(
                        "提高优先级",
                        Message::Jobs(JobMessage::Control(
                            job.id.clone(),
                            JobCommand::Priority((job.priority + 10).min(100)),
                        )),
                    ))
                    .push(crate::components::action(
                        "取消任务",
                        Message::Jobs(JobMessage::Control(job.id.clone(), JobCommand::Cancel)),
                    ));
                view = view.push(action(
                    "降低优先级",
                    Message::Jobs(JobMessage::Control(
                        job.id.clone(),
                        JobCommand::Priority((job.priority - 10).max(-100)),
                    )),
                ));
            }
        }
        if let Some(control) = &self.import {
            count += 1;
            view = view.push(heading("图片导入", 15)).push(muted(format!(
                "已处理 {}，已发现 {}，同时处理 {}",
                control.finished.load(Ordering::Relaxed),
                control.discovered.load(Ordering::Relaxed),
                control.active.load(Ordering::Relaxed)
            )));
            if !control.cancel.load(Ordering::Relaxed) {
                view = view.push(action("停止导入", Message::CancelImport));
            } else {
                view = view.push(muted("正在停止并收尾…"));
            }
        }
        if self.manager.maintenance.is_some() {
            count += 1;
            view = view.push(action(
                "查看资源检查进度",
                Message::Manage(ManageMessage::Maintenance),
            ));
        }
        if self.storage.control.is_some() {
            count += 1;
            view = view.push(action(
                "查看空间统计进度",
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
                &format!("查看 {} 个 Synco 交接任务", pending),
                Message::Manage(ManageMessage::Synco),
            ));
        }
        if count == 0 {
            view = view.push(muted("当前没有后台任务。"));
        }
        view.into()
    }
}
