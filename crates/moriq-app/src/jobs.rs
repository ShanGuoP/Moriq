//! Typed background-job admission, control and execution.
use super::*;

impl Service {
    pub async fn queue_media_job(&self, id: LibraryId, kind: JobKind) -> TaskResult<()> {
        if let Some(model) = kind.model() {
            let runner = self.inference.clone();
            tokio::task::spawn_blocking(move || runner.ready(model))
                .await
                .map_err(|e| TaskError::Runtime(e.to_string()))??;
        }
        self.call_task(move |active| {
            check_task(active, id)?
                .enqueue_media_job(kind, 0)
                .map(|_| ())
                .map_err(storage_error)
        })
        .await
    }
    pub async fn control_job(
        &self,
        id: LibraryId,
        job: String,
        command: JobCommand,
    ) -> TaskResult<()> {
        let cancellation = self.job_cancel.clone();
        self.call_task(move |active| {
            check_task(active, id)?
                .control_job(&job, command)
                .map_err(storage_error)?;
            if matches!(command, JobCommand::Pause | JobCommand::Cancel) {
                cancellation.stop(Some((id, &job)));
            }
            Ok(())
        })
        .await
    }
    pub async fn control_model_jobs(&self, id: LibraryId, command: JobCommand) -> TaskResult<()> {
        let cancellation = self.job_cancel.clone();
        self.call_task(move |active| {
            let jobs = check_task(active, id)?
                .control_model_jobs(command)
                .map_err(storage_error)?;
            if matches!(command, JobCommand::Pause | JobCommand::Cancel) {
                for job in jobs {
                    cancellation.stop(Some((id, &job)));
                }
            }
            Ok(())
        })
        .await
    }
    pub async fn job_failures(&self, id: LibraryId, job: String) -> Result<Vec<JobFailure>> {
        self.call(move |active| {
            check(active, id)?
                .job_failures(&job)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn drive_job_step(&self, id: LibraryId) -> TaskResult<()> {
        let service = self.clone();
        // A dropped UI future must not strand an in-flight checkpoint in an open library.
        tokio::spawn(async move {
            let _slot = service
                .job_slot
                .clone()
                .acquire_owned()
                .await
                .map_err(|e| TaskError::Runtime(e.to_string()))?;
            let started = std::time::Instant::now();
            for _ in 0..64 {
                let cancellation = service.job_cancel.clone();
                let work = service
                    .call_task(move |active| {
                        let work = check_task(active, id)?
                            .claim_job_step()
                            .map_err(storage_error)?;
                        Ok(work.map(|work| {
                            let cancel = cancellation.register(id, work.abort_token().0);
                            (work, cancel)
                        }))
                    })
                    .await?;
                let Some((work, cancel)) = work else {
                    return Ok(());
                };
                if let Some(model) = work.kind().model() {
                    let runner = service.inference.clone();
                    let ready = tokio::task::spawn_blocking(move || runner.ready(model))
                        .await
                        .map_err(|e| TaskError::Runtime(e.to_string()))?;
                    if let Err(error) = ready {
                        let (job, generation) = work.abort_token();
                        service
                            .call_task(move |active| {
                                check_task(active, id)?
                                    .abort_job(&job, generation, &error.to_string())
                                    .map_err(storage_error)
                            })
                            .await?;
                        return Ok(());
                    }
                }
                let budget = work.memory_mib();
                let (job, generation) = work.abort_token();
                let mut waiting =
                    Box::pin(service.import_memory.clone().acquire_many_owned(budget));
                let memory = loop {
                    if cancel.load(Ordering::Acquire) {
                        return Ok(());
                    }
                    if let Ok(result) =
                        tokio::time::timeout(std::time::Duration::from_millis(25), &mut waiting)
                            .await
                    {
                        break result.map_err(|e| TaskError::Runtime(e.to_string()))?;
                    }
                };
                let execution_cancel = cancel.clone();
                let runner = service.inference.clone();
                let outcome = tokio::task::spawn_blocking(move || {
                    let _permit = memory;
                    inference::execute(work, execution_cancel, runner.as_ref())
                })
                .await;
                let outcome = match outcome {
                    Ok(outcome) => outcome,
                    Err(error) => {
                        let message = error.to_string();
                        let detail = message.clone();
                        let _ = service
                            .call_task(move |active| {
                                check_task(active, id)?
                                    .abort_job(&job, generation, &detail)
                                    .map_err(storage_error)
                            })
                            .await;
                        return Err(TaskError::Runtime(message));
                    }
                };
                if cancel.load(Ordering::Acquire) {
                    return Ok(());
                }
                service
                    .call_task(move |active| {
                        check_task(active, id)?
                            .finish_job_step(outcome)
                            .map_err(storage_error)
                    })
                    .await?;
                if started.elapsed() >= std::time::Duration::from_millis(250) {
                    break;
                }
            }
            Ok(())
        })
        .await
        .map_err(|e| TaskError::Runtime(e.to_string()))?
    }
    pub async fn background_jobs(&self, id: LibraryId) -> Result<Vec<BackgroundJob>> {
        self.call(move |active| {
            check(active, id)?
                .background_jobs()
                .map_err(|e| e.to_string())
        })
        .await
    }
}
