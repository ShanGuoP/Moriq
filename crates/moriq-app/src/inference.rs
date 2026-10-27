//! The application owns model runtime access; storage only prepares and commits data.
use super::*;
use moriq_library::{
    analysis::InferenceResult,
    jobs::{Execution, Outcome, Work},
};

pub trait InferenceRunner: Send + Sync {
    /// 约定可取消的人脸推理接口；未配置执行器时返回不可用错误。
    fn qwen_faces(
        &self,
        _image: &std::path::Path,
        _references: &[(String, std::path::PathBuf)],
        _cancel: &AtomicBool,
    ) -> TaskResult<InferenceResult> {
        Err(TaskError::Runtime("未配置 Qwen 人脸推理".into()))
    }
    /// 约定使用指定分类配置执行 Qwen 任务，并返回带模型修订的回执。
    fn qwen_infer(
        &self,
        task: &str,
        image: &std::path::Path,
        profile: &AiProfile,
        cancel: &AtomicBool,
    ) -> TaskResult<InferenceResult>;
    /// 约定查询指定模型的固定修订信息。
    fn revision(&self, model: ModelId) -> TaskResult<String>;
    /// 约定检查指定模型当前是否可供推理。
    fn ready(&self, model: ModelId) -> TaskResult<()>;
    /// 约定模型推理接口，显式接收输入和取消标志。
    fn infer(
        &self,
        model: ModelId,
        input: serde_json::Value,
        cancel: &AtomicBool,
    ) -> TaskResult<InferenceResult>;
}

pub struct LocalInference;
impl InferenceRunner for LocalInference {
    /// 确认模型可用后执行本地人脸推理，并在返回前再次检查取消。
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
    /// 调用本地 Qwen 执行器，绑定实际模型修订并拒绝取消后的结果。
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
    /// 从内置模型清单读取修订，未知模型返回输入错误。
    fn revision(&self, model: ModelId) -> TaskResult<String> {
        moriq_ai::registry()
            .map_err(TaskError::Runtime)?
            .into_iter()
            .find(|spec| spec.id == model.as_str())
            .map(|spec| spec.revision)
            .ok_or_else(|| TaskError::InvalidInput("模型未登记".into()))
    }
    /// 验证指定模型已完整安装，统一转换运行时错误。
    fn ready(&self, model: ModelId) -> TaskResult<()> {
        moriq_ai::installed_path(model.as_str())
            .map(|_| ())
            .map_err(TaskError::Runtime)
    }
    /// 执行已安装模型推理，并返回带真实身份与修订的分析结果。
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

/// 按冻结工作种类分派媒体执行或模型推理，再封装待写入端提交的结果。
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
    /// 在请求 Future 被丢弃时发出取消信号，阻止后台推理继续发布结果。
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

impl Service {
    /// 为当前库的交互推理登记取消令牌，切库或请求销毁时可统一停止。
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

    /// 等待共享内存预算后推理文本向量，再校验库身份并执行语义检索。
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

    /// 准备图片向量并进行语义检索，沿用共享预算与取消控制。
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

    /// 读取当前版本的语义推荐，避免旧版本结果用于新素材。
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
    /// 在阻塞线程读取模型安装清单及本机能力。
    pub async fn models(&self) -> Result<(Vec<ModelInfo>, Hardware)> {
        tokio::task::spawn_blocking(|| Ok((moriq_ai::inventory()?, moriq_ai::hardware())))
            .await
            .map_err(|e| e.to_string())?
    }
    /// 按默认来源安装模型，沿用下载状态与并发保护。
    pub async fn install_model(&self, model: String, control: DownloadControl) -> Result<()> {
        let preferences = self.settings().await?;
        self.install_model_with_mirror(model, control, preferences.ai.use_mirror)
            .await
    }
    /// 按用户选择的来源安装模型，避免阻塞资源库写入端。
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
    /// 在阻塞线程卸载模型，沿用运行租约保护。
    pub async fn uninstall_model(&self, model: String) -> Result<()> {
        tokio::task::spawn_blocking(move || moriq_ai::uninstall(&model))
            .await
            .map_err(|e| e.to_string())?
    }
}
