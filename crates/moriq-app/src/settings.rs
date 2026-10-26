//! User preferences and library cards.
use super::*;
#[derive(Debug, Clone)]
pub struct LibraryCard {
    pub path: PathBuf,
    pub cover: Option<Pixels>,
    pub summary: Option<(u64, u64)>,
}

impl Service {
    pub async fn library_card_details(&self, paths: Vec<PathBuf>) -> Result<Vec<LibraryCard>> {
        tokio::task::spawn_blocking(move || {
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
    pub async fn reduced_motion(&self, enabled: bool) -> Result<UserSettings> {
        self.call(move |_| {
            moriq_library::settings::reduced_motion(enabled).map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn diagnostic_logging(&self, enabled: bool) -> Result<UserSettings> {
        self.call(move |_| {
            moriq_library::settings::diagnostic_logging(enabled).map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn library_cards(
        &self,
        paths: Vec<PathBuf>,
    ) -> Result<Vec<(PathBuf, Option<Pixels>)>> {
        tokio::task::spawn_blocking(move || {
            paths
                .into_iter()
                .map(|path| {
                    let cover =
                        moriq_library::settings::card_cover(&path).unwrap_or_else(|error| {
                            tracing::debug!(%error, "library card cover unavailable");
                            None
                        });
                    (path, cover)
                })
                .collect()
        })
        .await
        .map_err(|e| e.to_string())
    }
    pub async fn synco_preferences(&self, value: SyncoPreferences) -> Result<UserSettings> {
        self.call(move |_| {
            moriq_library::settings::synco_preferences(value).map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn ai_preferences(&self, value: AiPreferences) -> Result<UserSettings> {
        let preferences = self
            .call(move |_| {
                moriq_library::settings::ai_preferences(value).map_err(|e| e.to_string())
            })
            .await?;
        self.ai_policy(value).await?;
        Ok(preferences)
    }
    pub async fn ai_policy(&self, value: AiPreferences) -> Result<()> {
        tokio::task::spawn_blocking(move || moriq_ai::set_keep_loaded(!value.on_demand))
            .await
            .map_err(|e| e.to_string())?
    }
    pub async fn import_preferences(&self, value: ImportPreferences) -> Result<UserSettings> {
        self.call(move |_| {
            moriq_library::settings::import_preferences(value).map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn settings(&self) -> Result<UserSettings> {
        self.call(|_| moriq_library::settings::load().map_err(|e| e.to_string()))
            .await
    }
    pub async fn remember(&self, name: String, path: PathBuf) -> Result<UserSettings> {
        self.call(move |_| moriq_library::settings::remember(name, path).map_err(|e| e.to_string()))
            .await
    }
    pub async fn save_category_template(&self, profile: AiProfile) -> Result<AiProfile> {
        self.call(move |_| {
            moriq_library::settings::category_template(profile).map_err(|e| e.to_string())
        })
        .await
    }
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
