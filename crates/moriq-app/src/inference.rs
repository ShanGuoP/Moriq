//! The application owns model runtime access; storage only prepares and commits data.
use super::*;
use moriq_library::{
    analysis::InferenceResult,
    jobs::{Execution, Outcome, Work},
};

pub trait InferenceRunner: Send + Sync {
    fn qwen_faces(
        &self,
        _image: &std::path::Path,
        _references: &[(String, std::path::PathBuf)],
        _cancel: &AtomicBool,
    ) -> TaskResult<InferenceResult> {
        Err(TaskError::Runtime("未配置 Qwen 人脸推理".into()))
    }
    fn qwen_infer(
        &self,
        task: &str,
        image: &std::path::Path,
        profile: &AiProfile,
        cancel: &AtomicBool,
    ) -> TaskResult<InferenceResult>;
    fn revision(&self, model: ModelId) -> TaskResult<String>;
    fn ready(&self, model: ModelId) -> TaskResult<()>;
    fn infer(
        &self,
        model: ModelId,
        input: serde_json::Value,
        cancel: &AtomicBool,
    ) -> TaskResult<InferenceResult>;
}

pub struct LocalInference;
impl InferenceRunner for LocalInference {
    fn qwen_faces(
        &self,
        image: &std::path::Path,
        references: &[(String, std::path::PathBuf)],
        cancel: &AtomicBool,
    ) -> TaskResult<InferenceResult> {
        self.ready(ModelId::Qwen35_4b)?;
        let revision = self.revision(ModelId::Qwen35_4b)?;
        let data = moriq_ai::llama_faces(image, references, cancel).map_err(TaskError::Runtime)?;
        if cancel.load(Ordering::Acquire) {
            return Err(TaskError::Cancelled);
        }
        Ok(InferenceResult {
            model: ModelId::Qwen35_4b,
            revision,
            data,
        })
    }
    fn qwen_infer(
        &self,
        task: &str,
        image: &std::path::Path,
        profile: &AiProfile,
        cancel: &AtomicBool,
    ) -> TaskResult<InferenceResult> {
        if cancel.load(Ordering::Acquire) {
            return Err(TaskError::Cancelled);
        }
        self.ready(ModelId::Qwen35_4b)?;
        let revision = self.revision(ModelId::Qwen35_4b)?;
        let data = moriq_ai::llama_infer_profile(task, image, cancel, profile)
            .map_err(TaskError::Runtime)?;
        if cancel.load(Ordering::Acquire) {
            return Err(TaskError::Cancelled);
        }
        Ok(InferenceResult {
            model: ModelId::Qwen35_4b,
            revision,
            data,
        })
    }
    fn revision(&self, model: ModelId) -> TaskResult<String> {
        moriq_ai::registry()
            .map_err(TaskError::Runtime)?
            .into_iter()
            .find(|spec| spec.id == model.as_str())
            .map(|spec| spec.revision)
            .ok_or_else(|| TaskError::InvalidInput("模型未登记".into()))
    }
    fn ready(&self, model: ModelId) -> TaskResult<()> {
        moriq_ai::installed_path(model.as_str())
            .map(|_| ())
            .map_err(TaskError::Runtime)
    }
    fn infer(
        &self,
        model: ModelId,
        input: serde_json::Value,
        cancel: &AtomicBool,
    ) -> TaskResult<InferenceResult> {
        if cancel.load(Ordering::Acquire) {
            return Err(TaskError::Cancelled);
        }
        self.ready(model)?;
        let revision = self.revision(model)?;
        let data = moriq_ai::infer_controlled(model.as_str(), input, cancel)
            .map_err(TaskError::Runtime)?;
        if cancel.load(Ordering::Acquire) {
            return Err(TaskError::Cancelled);
        }
        Ok(InferenceResult {
            model,
            revision,
            data,
        })
    }
}

pub(super) fn execute(
    work: Work,
    cancel: Arc<AtomicBool>,
    runner: &dyn InferenceRunner,
) -> Outcome {
    match work.execute(cancel.clone()) {
        Execution::Complete(outcome) => outcome,
        Execution::QwenFaces(request) => {
            let result = request
                .face_references(&cancel)
                .map_err(|e| TaskError::Storage(e.to_string()))
                .and_then(|references| {
                    runner.qwen_faces(request.image_path(), &references.items, &cancel)
                });
            request.complete_faces(result)
        }
        Execution::Inference(request) => {
            let result = runner.infer(
                request.model(),
                serde_json::json!({"image":request.image_path()}),
                &cancel,
            );
            request.complete(result)
        }
        Execution::QwenClassification(request) => {
            let profile = request.profile().cloned().unwrap_or_default();
            let result = runner.qwen_infer("classify", request.image_path(), &profile, &cancel);
            request.complete_qwen("qwen_classification", result)
        }
        Execution::QwenDescription(request) => {
            let result = runner.qwen_infer(
                "describe",
                request.image_path(),
                &AiProfile::default(),
                &cancel,
            );
            request.complete_qwen("qwen_description", result)
        }
    }
}

/// Cancels detached blocking inference if the UI drops its request future.
struct CancelOnDrop(Arc<AtomicBool>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

impl Service {
    async fn inference_cancel(&self, id: LibraryId) -> Result<CancelOnDrop> {
        let cancellation = self.job_cancel.clone();
        self.call(move |active| {
            check(active, id)?;
            Ok(CancelOnDrop(
                cancellation.register(id, "interactive".into()),
            ))
        })
        .await
    }

    pub async fn semantic_search(&self, id: LibraryId, text: String) -> Result<Vec<AssetId>> {
        if text.trim().is_empty() || text.len() > 4096 {
            return Err("请输入1～4096字节的自然语言描述".into());
        }
        let cancel = self.inference_cancel(id).await?;
        let memory = self
            .import_memory
            .clone()
            .acquire_many_owned(JobKind::Analysis(ModelId::ChineseClip).memory_mib())
            .await
            .map_err(|e| e.to_string())?;
        let runner = self.inference.clone();
        let flag = cancel.0.clone();
        let result = tokio::task::spawn_blocking(move || {
            let _memory = memory;
            runner.infer(
                ModelId::ChineseClip,
                serde_json::json!({"text":text}),
                &flag,
            )
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?
        .checked(ModelId::ChineseClip)
        .map_err(|e| e.to_string())?;
        if cancel.0.load(Ordering::Acquire) {
            return Err(TaskError::Cancelled.to_string());
        }
        let query = moriq_library::analysis::query_vector(&result.data["text_vector"])
            .map_err(|e| e.to_string())?;
        self.read_plan(
            128,
            move |active| {
                check(active, id)?
                    .semantic_plan(result.revision)
                    .map_err(|e| e.to_string())
            },
            move |plan| plan.search(&query, None).map_err(|e| e.to_string()),
        )
        .await
    }

    pub async fn search_by_image(&self, id: LibraryId, asset: AssetId) -> Result<Vec<AssetId>> {
        let cancel = self.inference_cancel(id).await?;
        let flag = cancel.0.clone();
        let runner = self.inference.clone();
        let result = self
            .read_plan(
                3072,
                move |active| {
                    check(active, id)?
                        .media_source(asset)
                        .map_err(|e| e.to_string())
                },
                move |source| {
                    let input = moriq_library::analysis::prepare_image(source)
                        .map_err(|e| e.to_string())?;
                    runner
                        .infer(
                            ModelId::ChineseClip,
                            serde_json::json!({"image":input.path()}),
                            &flag,
                        )
                        .map_err(|e| e.to_string())
                },
            )
            .await?
            .checked(ModelId::ChineseClip)
            .map_err(|e| e.to_string())?;
        if cancel.0.load(Ordering::Acquire) {
            return Err(TaskError::Cancelled.to_string());
        }
        let query = moriq_library::analysis::query_vector(&result.data["image_vector"])
            .map_err(|e| e.to_string())?;
        self.read_plan(
            128,
            move |active| {
                check(active, id)?
                    .semantic_plan(result.revision)
                    .map_err(|e| e.to_string())
            },
            move |plan| plan.search(&query, Some(asset)).map_err(|e| e.to_string()),
        )
        .await
    }

    pub async fn recommendations(
        &self,
        id: LibraryId,
        asset: AssetId,
        version: AssetVersionId,
    ) -> Result<Vec<(AssetId, String)>> {
        let runner = self.inference.clone();
        let revision = tokio::task::spawn_blocking(move || runner.revision(ModelId::ChineseClip))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        self.read_plan(
            64,
            move |active| {
                check(active, id)?
                    .semantic_plan(revision)
                    .map_err(|e| e.to_string())
            },
            move |plan| {
                plan.recommendations(asset, version)
                    .map_err(|e| e.to_string())
            },
        )
        .await
    }
}

// Model installation and runtime policy.

impl Service {
    pub async fn models(&self) -> Result<(Vec<ModelInfo>, Hardware)> {
        tokio::task::spawn_blocking(|| Ok((moriq_ai::inventory()?, moriq_ai::hardware())))
            .await
            .map_err(|e| e.to_string())?
    }
    pub async fn install_model(&self, model: String, control: DownloadControl) -> Result<()> {
        let preferences = self.settings().await?;
        self.install_model_with_mirror(model, control, preferences.ai.use_mirror)
            .await
    }
    pub async fn install_model_with_mirror(
        &self,
        model: String,
        control: DownloadControl,
        use_mirror: bool,
    ) -> Result<()> {
        tokio::task::spawn_blocking(move || {
            moriq_ai::install_with_mirror(&model, &control, use_mirror)
        })
        .await
        .map_err(|e| e.to_string())?
    }
    pub async fn uninstall_model(&self, model: String) -> Result<()> {
        tokio::task::spawn_blocking(move || moriq_ai::uninstall(&model))
            .await
            .map_err(|e| e.to_string())?
    }
}
