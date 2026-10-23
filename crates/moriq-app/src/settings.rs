//! User preferences and library cards.
use super::*;

impl Service {
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
