//! Durable per-library scheduling. A step owns its frozen media and generation token.
use super::*;
pub struct Work {
    profile: Option<AiProfile>,
    readers: std::sync::Arc<read_pool::ReadPool>,
    library: LibraryId,
    root: PathBuf,
    job: String,
    generation: i64,
    seq: i64,
    version: String,
    kind: JobKind,
    payload: Option<serde_json::Value>,
    source: std::result::Result<Option<media::Source>, String>,
}
pub struct Outcome {
    work: Work,
    image: std::result::Result<Option<image::DynamicImage>, String>,
    analysis: Option<std::result::Result<serde_json::Value, String>>,
    revision: Option<String>,
}
impl Work {
    pub fn run_cancelable(self, flag: std::sync::Arc<std::sync::atomic::AtomicBool>) -> Outcome {
        let _scope = crate::cancel::Scope::enter(flag.clone());
        self.run_controlled(&flag)
    }
    pub fn abort_token(&self) -> (String, i64) {
        (self.job.clone(), self.generation)
    }
    pub fn memory_mib(&self) -> u32 {
        self.kind.memory_mib()
    }
    pub fn run(self) -> Outcome {
        self.run_controlled(&std::sync::atomic::AtomicBool::new(false))
    }
    pub fn run_controlled(mut self, cancel: &std::sync::atomic::AtomicBool) -> Outcome {
        if cancel.load(std::sync::atomic::Ordering::Acquire) {
            return Outcome {
                work: self,
                image: Err("任务已停止".into()),
                analysis: None,
                revision: None,
            };
        }
        if self.kind == JobKind::Integrity {
            let result = match std::mem::replace(&mut self.source, Ok(None)) {
                Err(e) => Err(e),
                Ok(None) => Ok(None),
                Ok(Some(source)) => source
                    .verify_integrity()
                    .map(|_| None)
                    .map_err(|e| e.to_string()),
            };
            return Outcome {
                work: self,
                image: result,
                analysis: None,
                revision: None,
            };
        }
        if self.kind == JobKind::Batch {
            let result = self.payload.take().ok_or_else(|| "批量任务参数缺失".into());
            return Outcome {
                work: self,
                image: Ok(None),
                analysis: Some(result),
                revision: None,
            };
        }
        if self.kind == JobKind::Metadata {
            let analysis = match std::mem::replace(&mut self.source, Ok(None)) {
                Err(e) => Err(e),
                Ok(None) => Ok(serde_json::json!({})),
                Ok(Some(source)) => source.metadata_record().map_err(|e| e.to_string()),
            };
            return Outcome {
                work: self,
                image: Ok(None),
                analysis: Some(analysis),
                revision: None,
            };
        }
        if self.kind.model().is_some() {
            return Outcome {
                work: self,
                image: Ok(None),
                analysis: Some(Err("分析任务需要应用层推理执行器".into())),
                revision: None,
            };
        }
        let image = match std::mem::replace(&mut self.source, Ok(None)) {
            Err(error) => Err(error),
            Ok(None) => Ok(None),
            Ok(Some(source)) => source
                .preview_edge(256)
                .map(|p| {
                    image::RgbaImage::from_raw(p.image.width, p.image.height, p.image.rgba.to_vec())
                        .map(image::DynamicImage::ImageRgba8)
                })
                .map_err(|e| e.to_string()),
        };
        Outcome {
            work: self,
            image,
            analysis: None,
            revision: None,
        }
    }
}
pub enum Execution {
    Complete(Outcome),
    Inference(InferenceWork),
    QwenClassification(InferenceWork),
    QwenDescription(InferenceWork),
    QwenFaces(InferenceWork),
}
pub struct InferenceWork {
    work: Work,
    image: analysis::PreparedImage,
    model: ModelId,
}
impl InferenceWork {
    pub fn face_references(
        &self,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> Result<crate::qwen_faces::References> {
        crate::qwen_faces::references(&self.work.readers, cancel)
    }
    pub fn complete_faces(self, result: TaskResult<analysis::InferenceResult>) -> Outcome {
        let mut revision = None;
        let result = result
            .and_then(|output| output.checked(ModelId::Qwen35_4b))
            .map(|output| {
                revision = Some(output.revision);
                output.data
            })
            .map_err(|e| e.to_string());
        Outcome {
            work: self.work,
            image: Ok(None),
            analysis: Some(result),
            revision,
        }
    }
    pub fn profile(&self) -> Option<&AiProfile> {
        self.work.profile.as_ref()
    }
    pub fn model(&self) -> ModelId {
        self.model
    }
    pub fn image_path(&self) -> &Path {
        self.image.path()
    }
    pub fn complete(self, result: TaskResult<analysis::InferenceResult>) -> Outcome {
        let mut revision = None;
        let result = result
            .and_then(|output| output.checked(self.model))
            .and_then(|mut output| {
                if output.model == ModelId::Faces {
                    analysis::suggest_people(
                        &self.work.readers,
                        &self.work.version,
                        &output.revision,
                        &mut output.data,
                    )
                    .map_err(|e| TaskError::Storage(e.to_string()))?;
                }
                revision = Some(output.revision);
                Ok(output.data)
            })
            .map_err(|e| e.to_string());
        Outcome {
            work: self.work,
            image: Ok(None),
            analysis: Some(result),
            revision,
        }
    }
    pub fn complete_qwen(
        self,
        task: &'static str,
        result: TaskResult<analysis::InferenceResult>,
    ) -> Outcome {
        let mut revision = None;
        let result = result
            .and_then(|output| output.checked(ModelId::Qwen35_4b))
            .and_then(|output| {
                revision = Some(output.revision);
                match self.work.profile.as_ref() {
                    Some(profile) => analysis::qwen_payload_for_profile(task, output.data, profile),
                    None => analysis::qwen_payload(task, output.data),
                }
            })
            .map_err(|e| e.to_string());
        Outcome {
            work: self.work,
            image: Ok(None),
            analysis: Some(result),
            revision,
        }
    }
}
impl Work {
    pub fn kind(&self) -> JobKind {
        self.kind
    }
    pub fn execute(mut self, cancel: std::sync::Arc<std::sync::atomic::AtomicBool>) -> Execution {
        let _scope = crate::cancel::Scope::enter(cancel.clone());
        if cancel.load(std::sync::atomic::Ordering::Acquire) {
            return Execution::Complete(self.run_controlled(&cancel));
        }
        let Some(model) = self.kind.model() else {
            return Execution::Complete(self.run_controlled(&cancel));
        };
        if matches!(
            self.kind,
            JobKind::QwenClassification | JobKind::QwenDescription | JobKind::QwenFaces
        ) {
            return self.execute_qwen();
        }
        let source = std::mem::replace(&mut self.source, Ok(None));
        match source {
            Ok(Some(source)) => match analysis::prepare_image(source) {
                Ok(image) => Execution::Inference(InferenceWork {
                    work: self,
                    image,
                    model,
                }),
                Err(error) => Execution::Complete(Outcome {
                    work: self,
                    image: Ok(None),
                    analysis: Some(Err(error.to_string())),
                    revision: None,
                }),
            },
            Ok(None) => Execution::Complete(Outcome {
                work: self,
                image: Ok(None),
                analysis: Some(Ok(serde_json::json!({}))),
                revision: None,
            }),
            Err(error) => Execution::Complete(Outcome {
                work: self,
                image: Ok(None),
                analysis: Some(Err(error)),
                revision: None,
            }),
        }
    }
    fn execute_qwen(mut self) -> Execution {
        let source = std::mem::replace(&mut self.source, Ok(None));
        let image = match source {
            Ok(Some(source)) => analysis::prepare_image(source).map_err(|e| e.to_string()),
            Ok(None) => Err("Qwen 任务缺少输入素材".into()),
            Err(error) => Err(error),
        };
        match image {
            Ok(image) => match self.kind {
                JobKind::QwenFaces => Execution::QwenFaces(InferenceWork {
                    work: self,
                    image,
                    model: ModelId::Qwen35_4b,
                }),
                JobKind::QwenClassification => Execution::QwenClassification(InferenceWork {
                    work: self,
                    image,
                    model: ModelId::Qwen35_4b,
                }),
                _ => Execution::QwenDescription(InferenceWork {
                    work: self,
                    image,
                    model: ModelId::Qwen35_4b,
                }),
            },
            Err(error) => Execution::Complete(Outcome {
                work: self,
                image: Ok(None),
                analysis: Some(Err(error)),
                revision: None,
            }),
        }
    }
}
impl Library {
    pub fn pause_jobs_after_error(&mut self, error: &str) -> Result<()> {
        let error = error.chars().take(4096).collect::<String>();
        self.db.execute("UPDATE background_jobs SET state='paused',inflight=0,generation=generation+1,error=?1 WHERE state IN('queued','running')",[error])?;
        Ok(())
    }
    pub fn job_failures(&self, job: &str) -> Result<Vec<JobFailure>> {
        let mut stmt=self.db.prepare("SELECT a.id,coalesce(a.name,e.version_id),e.message FROM background_job_errors e LEFT JOIN \
                asset_versions v ON v.id=e.version_id LEFT JOIN assets a ON a.id=v.asset_id WHERE e.job_id=?1 ORDER \
                BY e.seq LIMIT 100")?;

        let rows = stmt
            .query_map([job], |r| {
                Ok((
                    r.get::<_, Option<String>>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(asset, name, message)| {
                Ok(JobFailure {
                    asset: asset.map(|id| parse_id(&id).map(AssetId)).transpose()?,
                    name,
                    message,
                })
            })
            .collect()
    }
    fn retry_failed_job(&mut self, old: &str) -> Result<String> {
        let tx = self.db.transaction()?;
        let source:Option<(String,
i32)>=tx.query_row("SELECT kind,priority FROM background_jobs WHERE id=?1 AND state IN('completed','cancelled')",
[old],
|r|Ok((r.get(0)?,
r.get(1)?))).optional()?;

        let Some((kind, priority)) = source else {
            return Err(Error::Invalid(
                "请等待原任务完成或先取消，再重试失败项".into(),
            ));
        };
        let id = Uuid::new_v4().to_string();
        tx.execute("INSERT INTO background_jobs(id,kind,state,priority,created_at) VALUES(?1,?2,'queued',?3,?4)",params![id,kind.as_str(),priority,now_ms()?])?;
        tx.execute("INSERT INTO background_job_items SELECT ?1,row_number() OVER(ORDER BY e.seq),e.version_id FROM \
                background_job_errors e JOIN asset_versions v ON v.id=e.version_id JOIN assets a ON a.id=v.asset_id \
                WHERE e.job_id=?2 AND a.deleted_at IS NULL",
params![id,
old])?;

        let count: u64 = tx.query_row(
            "SELECT count(*) FROM background_job_items WHERE job_id=?1",
            [&id],
            |r| r.get(0),
        )?;
        if count == 0 {
            return Err(Error::Invalid("没有仍可处理的失败素材".into()));
        }
        tx.execute(
            "UPDATE background_jobs SET total=?2 WHERE id=?1",
            params![id, count],
        )?;
        tx.execute("INSERT INTO background_job_payload SELECT ?1,json FROM background_job_payload WHERE job_id=?2",params![id,old])?;
        tx.commit()?;
        Ok(id)
    }
    pub fn abort_job(&self, job: &str, generation: i64, error: &str) -> Result<()> {
        self.db.execute("UPDATE background_jobs SET state='paused',inflight=0,generation=generation+1,error=?3 WHERE id=?1 \
                AND generation=?2 AND inflight=1",
params![job,
generation,
error.chars().take(2000).collect::<String>()])?;

        Ok(())
    }
    pub fn enqueue_media_job(&mut self, kind: JobKind, priority: i32) -> Result<String> {
        let profile = if kind == JobKind::QwenClassification {
            Some(self.ai_profile()?)
        } else {
            None
        };
        if kind == JobKind::Batch || !(-100..=100).contains(&priority) {
            return Err(Error::Invalid("后台任务类型或优先级无效".into()));
        }
        let existing:Option<String>=self.db.query_row("SELECT id FROM background_jobs WHERE kind=?1 AND state IN('queued','running','paused') LIMIT 1",
[kind.as_str()],
|r|r.get(0)).optional()?;

        if let Some(id) = existing {
            return Ok(id);
        }
        let id = Uuid::new_v4().to_string();
        let tx = self.db.transaction()?;
        tx.execute("INSERT INTO background_jobs(id,kind,state,priority,created_at) VALUES(?1,?2,'queued',?3,?4)",params![id,kind.as_str(),priority,now_ms()?])?;
        if let Some(profile) = profile {
            tx.execute(
                "INSERT INTO background_job_payload VALUES(?1,?2)",
                params![id, serde_json::json!({"ai_profile":profile}).to_string()],
            )?;
        }
        let formats = if kind == JobKind::QwenFaces {
            format!(
                "v.format NOT IN({}) AND a.display_version_id=v.id",
                video::sql_formats()
            )
        } else if kind == JobKind::Integrity {
            "1=1".into()
        } else {
            format!("v.format NOT IN({})", video::sql_formats())
        };
        tx.execute(&format!("INSERT INTO background_job_items(job_id,seq,version_id) SELECT ?1,row_number() OVER(ORDER BY \
                v.id),v.id FROM asset_versions v JOIN assets a ON a.id=v.asset_id WHERE a.deleted_at IS NULL AND \
                {formats}"),
[&id])?;

        tx.execute("UPDATE background_jobs SET total=(SELECT count(*) FROM background_job_items WHERE job_id=?1) WHERE id=?1",[&id])?;
        tx.commit()?;
        Ok(id)
    }
    pub fn background_jobs(&self) -> Result<Vec<BackgroundJob>> {
        self.db.prepare("SELECT id,kind,state,priority,checkpoint,total,failed,error FROM background_jobs ORDER BY CASE WHEN \
                state IN('queued','running') THEN 0 WHEN state='paused' THEN 1 ELSE 2 END,created_at DESC LIMIT 100")?.query_map([],
|r|Ok(BackgroundJob{
id:r.get(0)?,
kind:parse_protocol(r.get(1)?, 1)?,
state:parse_protocol(r.get(2)?, 2)?,
priority:r.get(3)?,
completed:r.get(4)?,
total:r.get(5)?,
failed:r.get(6)?,
error:r.get(7)?}
))?.collect::<std::result::Result<Vec<_>,
_>>().map_err(Error::from)
    }
    pub fn control_model_jobs(&mut self, command: JobCommand) -> Result<Vec<String>> {
        let sql = match command {
            JobCommand::Pause => {
                "UPDATE background_jobs SET state='paused',inflight=0,generation=generation+1 WHERE kind LIKE 'analysis:%' AND state IN('queued','running') RETURNING id"
            }
            JobCommand::Resume => {
                "UPDATE background_jobs SET state='queued',inflight=0,generation=generation+1 WHERE kind LIKE 'analysis:%' AND state='paused' RETURNING id"
            }
            JobCommand::Cancel => {
                "UPDATE background_jobs SET state='cancelled',inflight=0,generation=generation+1 WHERE kind LIKE 'analysis:%' AND state IN('queued','running','paused') RETURNING id"
            }
            _ => return Err(Error::Invalid("不支持的批量模型任务操作".into())),
        };
        let mut statement = self.db.prepare(sql)?;
        statement
            .query_map([], |r| r.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Error::from)
    }
    pub fn control_job(&mut self, id: &str, command: JobCommand) -> Result<()> {
        if matches!(command, JobCommand::RetryFailed) {
            return self.retry_failed_job(id).map(|_| ());
        }
        let changed=match command{
            JobCommand::Pause=>self.db.execute("UPDATE background_jobs SET state='paused',inflight=CASE WHEN kind LIKE 'analysis:%' THEN 0 ELSE inflight END,generation=generation+CASE WHEN kind LIKE 'analysis:%' THEN 1 ELSE 0 END WHERE id=?1 AND state IN('queued','running')",[id])?,
            JobCommand::Resume=>self.db.execute("UPDATE background_jobs SET state='queued',inflight=0,generation=generation+1 WHERE id=?1 AND state='paused'",[id])?,
            JobCommand::Cancel=>self.db.execute("UPDATE background_jobs SET state='cancelled',inflight=0,generation=generation+1 WHERE id=?1 AND \
                state IN('queued','running','paused')",
[id])?,

            JobCommand::Priority(p) if (-100..=100).contains(&p)=>self.db.execute("UPDATE background_jobs SET priority=?2 WHERE id=?1 AND state IN('queued','running','paused')",
params![id,
p])?,

            _=>return Err(Error::Invalid("任务优先级应在 -100～100 之间".into()))};
        if changed == 0 {
            return Err(Error::Invalid("任务状态已改变，请刷新后重试".into()));
        }
        Ok(())
    }
    pub fn claim_job_step(&mut self) -> Result<Option<Work>> {
        let next:Option<(String,
String,
i64,
i64)>=self.db.query_row("SELECT id,kind,generation,checkpoint FROM background_jobs WHERE state IN('queued','running') AND \
                inflight=0 ORDER BY priority DESC,created_at,id LIMIT 1",
[],
|r|Ok((r.get(0)?,
r.get(1)?,
r.get(2)?,
r.get(3)?))).optional()?;

        let Some((job, kind, generation, checkpoint)) = next else {
            return Ok(None);
        };
        let kind: JobKind = match kind.parse() {
            Ok(kind) => kind,
            Err(error) => {
                self.db.execute(
                    "UPDATE background_jobs SET state='paused',error=?2 WHERE id=?1",
                    params![job, error],
                )?;
                return Ok(None);
            }
        };
        let item:Option<(i64,
String)>=self.db.query_row("SELECT seq,version_id FROM background_job_items WHERE job_id=?1 AND seq>?2 ORDER BY seq LIMIT 1",
params![job,
checkpoint],
|r|Ok((r.get(0)?,
r.get(1)?))).optional()?;

        let Some((seq, version)) = item else {
            let tx = self.db.transaction()?;
            tx.execute(
                "UPDATE background_jobs SET state='completed',checkpoint=total WHERE id=?1",
                [&job],
            )?;
            tx.execute("DELETE FROM background_job_items WHERE job_id=?1", [&job])?;
            tx.commit()?;
            return Ok(None);
        };
        let asset: Option<String> = self
            .db
            .query_row(
                "SELECT asset_id FROM asset_versions WHERE id=?1",
                [&version],
                |r| r.get(0),
            )
            .optional()?;
        let payload = if kind == JobKind::Batch {
            let value: String = self.db.query_row(
                "SELECT json FROM background_job_payload WHERE job_id=?1",
                [&job],
                |r| r.get(0),
            )?;
            Some(serde_json::from_str(&value).map_err(|e| Error::Invalid(e.to_string()))?)
        } else {
            None
        };
        let source = if kind == JobKind::Batch {
            Ok(None)
        } else {
            asset
                .map(|asset| {
                    self.version_source(
                        AssetId(parse_id(&asset)?),
                        AssetVersionId(parse_id(&version)?),
                    )
                })
                .transpose()
                .map_err(|e| e.to_string())
        };
        let profile = if kind == JobKind::QwenClassification {
            let stored: Option<String> = self
                .db
                .query_row(
                    "SELECT json FROM background_job_payload WHERE job_id=?1",
                    [&job],
                    |r| r.get(0),
                )
                .optional()?;
            let profile = if let Some(stored) = stored {
                let value: serde_json::Value =
                    serde_json::from_str(&stored).map_err(|e| Error::Invalid(e.to_string()))?;
                serde_json::from_value::<AiProfile>(value["ai_profile"].clone())
                    .map_err(|e| Error::Invalid(e.to_string()))?
            } else {
                self.ai_profile()?
            };
            profile.validate().map_err(|e| Error::Invalid(e.into()))?;
            if crate::ai_profile::classification_hash(&profile)?
                != crate::ai_profile::classification_hash(&self.ai_profile()?)?
            {
                self.db.execute("UPDATE background_jobs SET state='cancelled',generation=generation+1,inflight=0,error='分类定义已更新，请重新整理' WHERE id=?1",[&job])?;
                return Ok(None);
            }
            Some(profile)
        } else {
            None
        };
        self.db.execute(
            "UPDATE background_jobs SET state='running',inflight=1 WHERE id=?1",
            [&job],
        )?;
        Ok(Some(Work {
            profile,
            readers: self.readers.clone(),
            library: self.id(),
            root: self.root.clone(),
            job,
            kind,
            payload,
            generation,
            seq,
            version,
            source,
        }))
    }
    pub fn finish_job_step(&mut self, outcome: Outcome) -> Result<()> {
        if outcome.work.library != self.id() || outcome.work.root != self.root {
            return Err(Error::Invalid("后台任务所属资源库已改变".into()));
        }
        let job = outcome.work.job.clone();
        let generation = outcome.work.generation;
        let result = self.finish_job_step_inner(outcome);
        if let Err(error) = &result {
            let message = error.to_string().chars().take(2000).collect::<String>();
            let _=self.db.execute("UPDATE background_jobs SET state='paused',inflight=0,error=?3 WHERE id=?1 AND generation=?2 AND inflight=1",params![job,generation,message]);
        }
        result
    }
    fn finish_job_step_inner(&mut self, outcome: Outcome) -> Result<()> {
        if outcome.work.library != self.id() || outcome.work.root != self.root {
            return Err(Error::Invalid("后台任务所属资源库已改变".into()));
        }
        let work = outcome.work;
        let state:Option<String>=self.db.query_row("SELECT state FROM background_jobs WHERE id=?1 AND generation=?2 AND inflight=1 AND state \
                IN('running','paused','queued')",
params![work.job,
work.generation],
|r|r.get(0)).optional()?;

        if state.is_none() {
            return Ok(());
        }
        if let Some(profile) = &work.profile
            && crate::ai_profile::classification_hash(profile)?
                != crate::ai_profile::classification_hash(&self.ai_profile()?)?
        {
            self.db.execute("UPDATE background_jobs SET state='cancelled',generation=generation+1,inflight=0,error='分类定义已更新，请重新整理' WHERE id=?1",[&work.job])?;
            return Ok(());
        }
        let mut failure = String::new();
        let mut copies = project_isolation::Copies::default();
        let tx = self.db.transaction()?;
        tx.execute_batch("PRAGMA defer_foreign_keys=ON;")?;
        if let Some(result) = outcome.analysis {
            match result {
                Ok(result) => {
                    if result.as_object().is_some_and(|m| !m.is_empty()) {
                        if work.kind == JobKind::Batch {
                            batch::apply(&tx, &self.root, &mut copies, &work.version, &result)?;
                        } else if work.kind == JobKind::Metadata {
                            metadata::store(&tx, &work.version, &result)?;
                        } else if work.kind == JobKind::QwenFaces {
                            crate::qwen_faces::store(
                                &tx,
                                &work.version,
                                outcome
                                    .revision
                                    .as_deref()
                                    .ok_or_else(|| Error::Invalid("人脸任务缺少模型修订".into()))?,
                                &result,
                            )?;
                        } else {
                            let model = work
                                .kind
                                .model()
                                .map(ModelId::as_str)
                                .ok_or_else(|| Error::Invalid("分析任务类型无效".into()))?;
                            analysis::store(
                                &tx,
                                &work.version,
                                model,
                                outcome
                                    .revision
                                    .as_deref()
                                    .ok_or_else(|| Error::Invalid("分析回执缺少模型修订".into()))?,
                                &result,
                            )?;
                        }
                    }
                }
                Err(e) => failure = e.chars().take(2000).collect(),
            }
        }
        match outcome.image {
            Ok(Some(image)) => {
                let exists: bool = tx.query_row(
                    "SELECT EXISTS(SELECT 1 FROM asset_versions WHERE id=?1)",
                    [&work.version],
                    |r| r.get(0),
                )?;
                if exists {
                    if work.kind == JobKind::Fingerprints {
                        similarity::store(&tx, &work.version, &similarity::signature(&image))?;
                    } else {
                        let path =
                            safe_path(&self.root, &format!("thumbnails/256/{}.png", work.version))?;
                        let mut temp = tempfile::NamedTempFile::new_in(
                            path.parent()
                                .ok_or_else(|| Error::Invalid("缓存目录无效".into()))?,
                        )?;
                        image.write_to(&mut temp, ImageFormat::Png)?;
                        temp.as_file().sync_all()?;
                        temp.persist(path).map_err(|e| Error::Io(e.error))?;
                    }
                }
            }
            Ok(None) => {}
            Err(error) => failure = error.chars().take(2000).collect(),
        }
        if !failure.is_empty() {
            tx.execute("INSERT INTO background_job_errors VALUES(?1,?2,?3,?4) ON CONFLICT(job_id,seq) DO UPDATE SET \
                message=excluded.message",
params![work.job,
work.seq,
work.version,
failure])?;
        }
        tx.execute("UPDATE background_jobs SET checkpoint=?3,inflight=0,failed=failed+?4,error=CASE WHEN ?4 THEN ?5 \
                ELSE error END WHERE id=?1 AND generation=?2",
params![work.job,
work.generation,
work.seq,
!failure.is_empty(),
failure])?;

        tx.commit()?;
        self.snapshot = None;
        copies.commit();
        Ok(())
    }
}

fn parse_protocol<T: std::str::FromStr<Err = &'static str>>(
    value: String,
    column: usize,
) -> rusqlite::Result<T> {
    value.parse().map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Text,
            Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
        )
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn metadata_completion_after_purge_advances_without_recreating_the_version() {
        use image::ImageEncoder;
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("exif.png");
        let mut output = File::create(&source).unwrap();
        let mut encoder = image::codecs::png::PngEncoder::new(&mut output);
        let mut exif =
            b"II\x2a\0\x08\0\0\0\x01\0\x0f\x01\x02\0\x06\0\0\0\x1a\0\0\0\0\0\0\0".to_vec();
        exif.extend_from_slice(b"AUDIT\0");
        encoder.set_exif_metadata(exif).unwrap();
        encoder
            .write_image(&[230, 80, 30], 1, 1, image::ExtendedColorType::Rgb8)
            .unwrap();
        drop(output);
        let mut lib = Library::create(dir.path(), "元数据删除竞态").unwrap();
        lib.import(&source, None).unwrap();
        let asset = lib.page(&Query::default(), 0, 1).unwrap().assets.remove(0);
        let job = lib.enqueue_media_job(JobKind::Metadata, 0).unwrap();
        let outcome = lib.claim_job_step().unwrap().unwrap().run();
        assert_eq!(
            outcome.analysis.as_ref().unwrap().as_ref().unwrap()["make"],
            "AUDIT"
        );
        lib.edit(&[asset.id], &Edit::Trash(true)).unwrap();
        lib.purge(&[asset.id]).unwrap();
        lib.finish_job_step(outcome).unwrap();
        assert!(lib.claim_job_step().unwrap().is_none());
        let completed = lib
            .background_jobs()
            .unwrap()
            .into_iter()
            .find(|j| j.id == job)
            .unwrap();
        assert_eq!(
            (
                completed.state.as_str(),
                completed.completed,
                completed.failed
            ),
            ("completed", 1, 0)
        );
        let count: u64 = lib
            .db
            .query_row(
                "SELECT count(*) FROM version_exif WHERE version_id=?1",
                [asset.version_id.to_string()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
        assert!(source.exists());
    }
    fn library(dir: &Path) -> Library {
        let mut lib = Library::create(dir, "任务").unwrap();
        for n in 0..3 {
            let source = dir.join(format!("{n}.png"));
            image::RgbImage::from_pixel(30 + n, 20, image::Rgb([n as u8 * 40, 80, 120]))
                .save(&source)
                .unwrap();
            lib.import(&source, None).unwrap();
        }
        lib
    }
    #[test]
    fn priority_pause_reopen_resume_and_cancel_reject_late_media() {
        let dir = tempfile::tempdir().unwrap();
        let mut lib = library(dir.path());
        let low = lib.enqueue_media_job(JobKind::Fingerprints, 0).unwrap();
        let high = lib.enqueue_media_job(JobKind::Thumbnails, 20).unwrap();
        let work = lib.claim_job_step().unwrap().unwrap();
        assert_eq!(work.job, high);
        lib.control_job(&high, JobCommand::Pause).unwrap();
        lib.finish_job_step(work.run()).unwrap();
        let paused = lib
            .background_jobs()
            .unwrap()
            .into_iter()
            .find(|j| j.id == high)
            .unwrap();
        assert_eq!(paused.state, JobState::Paused);
        assert_eq!(paused.completed, 1);
        let work = lib.claim_job_step().unwrap().unwrap();
        assert_eq!(work.job, low);
        let outcome = work.run();
        lib.control_job(&low, JobCommand::Cancel).unwrap();
        lib.finish_job_step(outcome).unwrap();
        assert_eq!(
            lib.background_jobs()
                .unwrap()
                .into_iter()
                .find(|j| j.id == low)
                .unwrap()
                .completed,
            0
        );
        let root = lib.root.clone();
        drop(lib);
        let mut lib = Library::open(&root).unwrap();
        lib.control_job(&high, JobCommand::Resume).unwrap();
        for _ in 0..4 {
            if let Some(work) = lib.claim_job_step().unwrap() {
                lib.finish_job_step(work.run()).unwrap();
            }
        }
        let complete = lib
            .background_jobs()
            .unwrap()
            .into_iter()
            .find(|j| j.id == high)
            .unwrap();
        assert_eq!(complete.state, JobState::Completed);
        assert_eq!(complete.completed, 3);
        assert_eq!(complete.failed, 0);
    }
    #[test]
    fn crashed_claim_is_requeued_and_wrong_library_and_missing_sources_do_not_stall() {
        let dir = tempfile::tempdir().unwrap();
        let mut lib = library(dir.path());
        let id = lib.enqueue_media_job(JobKind::Fingerprints, 0).unwrap();
        let work = lib.claim_job_step().unwrap().unwrap();
        let root = lib.root.clone();
        drop(work);
        drop(lib);
        let mut lib = Library::open(&root).unwrap();
        assert_eq!(lib.background_jobs().unwrap()[0].state, JobState::Queued);
        let work = lib.claim_job_step().unwrap().unwrap();
        let mut other = Library::create(dir.path(), "其他").unwrap();
        assert!(other.finish_job_step(work.run()).is_err());
        lib.control_job(&id, JobCommand::Pause).unwrap();
        lib.control_job(&id, JobCommand::Resume).unwrap();
        let version: String = lib
            .db
            .query_row(
                "SELECT version_id FROM background_job_items WHERE job_id=?1 ORDER BY seq LIMIT 1",
                [&id],
                |r| r.get(0),
            )
            .unwrap();
        let source = lib.version_path(&version, false).unwrap();
        fs::remove_file(source).unwrap();
        for _ in 0..5 {
            if let Some(work) = lib.claim_job_step().unwrap() {
                lib.finish_job_step(work.run()).unwrap();
            }
        }
        let result = lib.background_jobs().unwrap().remove(0);
        assert_eq!(result.state, JobState::Completed);
        assert_eq!(result.failed, 1);
    }
    #[test]
    fn bulk_model_controls_preserve_checkpoints_and_leave_other_jobs_alone() {
        let dir = tempfile::tempdir().unwrap();
        let mut lib = Library::create(dir.path(), "controls").unwrap();
        let a = lib
            .enqueue_media_job(JobKind::QwenClassification, 0)
            .unwrap();
        let b = lib.enqueue_media_job(JobKind::QwenDescription, 0).unwrap();
        let ordinary = lib.enqueue_media_job(JobKind::Metadata, 0).unwrap();
        let paused = lib.control_model_jobs(JobCommand::Pause).unwrap();
        assert!(paused.contains(&a));
        assert!(paused.contains(&b));
        let jobs = lib.background_jobs().unwrap();
        assert_eq!(
            jobs.iter().find(|j| j.id == ordinary).unwrap().state,
            JobState::Queued
        );
        assert_eq!(
            jobs.iter().find(|j| j.id == a).unwrap().state,
            JobState::Paused
        );
        lib.control_model_jobs(JobCommand::Resume).unwrap();
        lib.control_model_jobs(JobCommand::Cancel).unwrap();
        assert!(
            lib.control_model_jobs(JobCommand::Resume)
                .unwrap()
                .is_empty()
        );
        let root = lib.root().to_path_buf();
        drop(lib);
        let lib = Library::open(&root).unwrap();
        assert_eq!(
            lib.background_jobs()
                .unwrap()
                .iter()
                .find(|j| j.id == a)
                .unwrap()
                .state,
            JobState::Cancelled
        );
    }
}
