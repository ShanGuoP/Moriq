//! User preferences and library cards.
use super::*;
#[derive(Debug, Clone)]
pub struct LibraryCard {
    pub path: PathBuf,
    pub cover: Option<Pixels>,
    pub summary: Option<(u64, u64)>,
}

impl Service {
    /// 读取资源库卡片所需的详细摘要和封面信息。
    pub async fn library_card_details(&self, paths: Vec<PathBuf>) -> Result<Vec<LibraryCard>> {
        let slot = self
            .read_slots
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| e.to_string())?;
        tokio::task::spawn_blocking(move || {
            let _slot = slot;
            paths
                .into_iter()
                .map(|path| LibraryCard {
                    cover: moriq_library::settings::card_cover(&path).unwrap_or_else(|error| {
                        tracing::debug!(%error,"library cover unavailable");
                        None
                    }),
                    summary: moriq_library::settings::card_summary(&path).ok(),
                    path,
                })
                .collect()
        })
        .await
        .map_err(|e| e.to_string())
    }
    /// 保存界面配置并返回更新后的设置。
    pub async fn interface_preferences(
        &self,
        language: Language,
        appearance: Appearance,
    ) -> Result<()> {
        self.call(move |_| {
            moriq_library::settings::interface(language, appearance)
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
        .await
    }
    /// 持久化减少动态效果的偏好。
    pub async fn reduced_motion(&self, enabled: bool) -> Result<UserSettings> {
        self.call(move |_| {
            moriq_library::settings::reduced_motion(enabled).map_err(|e| e.to_string())
        })
        .await
    }
    /// 保存诊断日志开关。
    pub async fn diagnostic_logging(&self, enabled: bool) -> Result<UserSettings> {
        self.call(move |_| {
            moriq_library::settings::diagnostic_logging(enabled).map_err(|e| e.to_string())
        })
        .await
    }
    /// 读取最近资源库列表并生成可展示的卡片资料。
    pub async fn library_cards(
        &self,
        paths: Vec<PathBuf>,
    ) -> Result<Vec<(PathBuf, Option<Pixels>)>> {
        Ok(self
            .library_card_details(paths)
            .await?
            .into_iter()
            .map(|card| (card.path, card.cover))
            .collect())
    }
    /// 保存 Synco 程序和连接偏好。
    pub async fn synco_preferences(&self, value: SyncoPreferences) -> Result<UserSettings> {
        self.call(move |_| {
            moriq_library::settings::synco_preferences(value).map_err(|e| e.to_string())
        })
        .await
    }
    /// 保存模型下载来源和加载策略。
    pub async fn ai_preferences(&self, value: AiPreferences) -> Result<UserSettings> {
        let preferences = self
            .call(move |_| {
                moriq_library::settings::ai_preferences(value).map_err(|e| e.to_string())
            })
            .await?;
        self.ai_policy(value).await?;
        Ok(preferences)
    }
    /// 更新实际模型加载策略，并与用户偏好保持一致。
    pub async fn ai_policy(&self, value: AiPreferences) -> Result<()> {
        tokio::task::spawn_blocking(move || moriq_ai::set_keep_loaded(!value.on_demand))
            .await
            .map_err(|e| e.to_string())?
    }
    /// 保存并应用导入并发和内存偏好。
    pub async fn import_preferences(&self, value: ImportPreferences) -> Result<UserSettings> {
        self.call(move |_| {
            moriq_library::settings::import_preferences(value).map_err(|e| e.to_string())
        })
        .await
    }
    /// 在文件线程读取用户设置，避免界面直接进行文件访问。
    pub async fn settings(&self) -> Result<UserSettings> {
        self.call(|_| moriq_library::settings::load().map_err(|e| e.to_string()))
            .await
    }
    /// 将已打开资源库登记到最近列表。
    pub async fn remember(&self, name: String, path: PathBuf) -> Result<UserSettings> {
        self.call(move |_| moriq_library::settings::remember(name, path).map_err(|e| e.to_string()))
            .await
    }
    /// 保存可复用分类模板，不修改当前资源库已有分类。
    pub async fn save_category_template(&self, profile: AiProfile) -> Result<AiProfile> {
        self.call(move |_| {
            moriq_library::settings::category_template(profile).map_err(|e| e.to_string())
        })
        .await
    }
    /// 检查指定目录并生成可供确认的资源库移除计划。
    pub async fn library_removal_plan(&self, path: PathBuf) -> Result<LibraryRemovalPlan> {
        self.call(move |_| {
            if !moriq_library::settings::load()
                .map_err(|e| e.to_string())?
                .recent
                .iter()
                .any(|(_, p)| p == &path)
            {
                return Err("资源库已不在列表中，请刷新后重试".into());
            }
            moriq_library::library_removal::inspect(&path).map_err(|e| e.to_string())
        })
        .await
    }
    /// 校验目标身份后关闭当前库，并停止其在途工作。
    pub async fn close_library(&self, id: LibraryId) -> Result<()> {
        let cancel = self.job_cancel.clone();
        let cache = self.media_cache.clone();
        self.call(move |active| {
            check(active, id)?;
            cancel.stop(None);
            cache.clear();
            *active = None;
            Ok(())
        })
        .await
    }
    /// 从最近列表移除或按已确认计划删除资源库，保护仍打开的库与外部原片。
    pub async fn remove_library(
        &self,
        path: PathBuf,
        plan: Option<LibraryRemovalPlan>,
    ) -> Result<UserSettings> {
        let candidate = path.clone();
        let plan = self
            .call(move |active| {
                if active.as_ref().is_some_and(|lib| {
                    candidate == lib.root()
                        || candidate.canonicalize().is_ok_and(|p| p == lib.root())
                }) {
                    return Err("请先关闭此资源库，再移除或删除".into());
                }
                if !moriq_library::settings::load()
                    .map_err(|e| e.to_string())?
                    .recent
                    .iter()
                    .any(|(_, p)| p == &candidate)
                {
                    return Err("资源库已不在列表中，请刷新后重试".into());
                }
                if let Some(plan) = &plan
                    && candidate.canonicalize().map_err(|e| e.to_string())? != plan.root
                {
                    return Err("删除路径已改变".into());
                }
                Ok(plan)
            })
            .await?;
        let deleted_files = plan.is_some();
        if let Some(plan) = plan {
            tokio::task::spawn_blocking(move || {
                for attempt in 0..=20 {
                    match moriq_library::library_removal::execute(&plan) {
                        Err(moriq_library::Error::Locked) if attempt < 20 => {
                            std::thread::sleep(std::time::Duration::from_millis(100))
                        }
                        result => return result.map_err(|e| e.to_string()),
                    }
                }
                Err("资源库仍在使用中".into())
            })
            .await
            .map_err(|e| e.to_string())??;
        }
        self.call(move |_| moriq_library::settings::forget(&path).map_err(|e| e.to_string()))
            .await
            .map_err(|error| {
                if deleted_files {
                    format!(
                        "库内文件已删除，但列表更新失败；请重新打开管理并从列表移除此记录：{error}"
                    )
                } else {
                    error
                }
            })
    }
    /// 更新当前资源库资料并同步最近列表中的展示信息。
    pub async fn update_profile(
        &self,
        id: LibraryId,
        name: String,
        description: String,
    ) -> Result<LibraryInfo> {
        self.call(move |active| {
            let lib = check(active, id)?;
            lib.update_profile(&name, &description)
                .map_err(|e| e.to_string())?;
            lib.info().map_err(|e| e.to_string())
        })
        .await
    }
}
