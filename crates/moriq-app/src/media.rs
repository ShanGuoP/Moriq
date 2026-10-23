//! Media plans, preview caching and exportable captures.
use super::*;

impl Service {
    pub async fn region_session(
        &self,
        id: LibraryId,
        asset: AssetId,
        version: AssetVersionId,
    ) -> Result<Arc<RegionSession>> {
        self.region_session_cancellable(id, asset, version, Arc::new(AtomicBool::new(false)))
            .await
    }
    pub async fn region_session_cancellable(
        &self,
        id: LibraryId,
        asset: AssetId,
        version: AssetVersionId,
        cancelled: Arc<AtomicBool>,
    ) -> Result<Arc<RegionSession>> {
        self.read_plan(
            2048,
            move |active| {
                check(active, id)?
                    .version_source(asset, version)
                    .map_err(|e| e.to_string())
            },
            move |source| {
                source
                    .regions_cancellable(cancelled)
                    .map(Arc::new)
                    .map_err(|e| e.to_string())
            },
        )
        .await
    }
    pub async fn region_tile(
        &self,
        id: LibraryId,
        session: Arc<RegionSession>,
        region: ImageRegion,
    ) -> Result<Pixels> {
        self.region_tile_cancellable(id, session, region, Arc::new(AtomicBool::new(false)))
            .await
    }
    pub async fn region_tile_cancellable(
        &self,
        id: LibraryId,
        session: Arc<RegionSession>,
        region: ImageRegion,
        cancelled: Arc<AtomicBool>,
    ) -> Result<Pixels> {
        self.read_plan_sized(
            |session: &Arc<RegionSession>| session.tile_memory_mib(),
            move |active| {
                check(active, id)?;
                Ok(session)
            },
            move |session| {
                session
                    .tile_cancellable(region, cancelled)
                    .map_err(|e| e.to_string())
            },
        )
        .await
    }
    pub async fn person_portrait(
        &self,
        library: LibraryId,
        person: String,
    ) -> Result<Option<Pixels>> {
        let source = self
            .call(move |active| {
                check(active, library)?
                    .person_portrait_source(&person)
                    .map_err(|e| e.to_string())
            })
            .await?;
        let Some(source) = source else {
            return Ok(None);
        };
        let preview = self
            .preview_version(library, source.asset, source.version)
            .await?;
        tokio::task::spawn_blocking(move || {
            moriq_library::portraits::crop(&preview.image, source.bounds)
                .map(Some)
                .map_err(|e| e.to_string())
        })
        .await
        .map_err(|e| e.to_string())?
    }
    pub async fn preview_full(
        &self,
        library: LibraryId,
        asset: AssetId,
    ) -> Result<moriq_library::media::Preview> {
        self.preview_for(library, asset, None).await
    }
    pub async fn preview_version(
        &self,
        library: LibraryId,
        asset: AssetId,
        version: AssetVersionId,
    ) -> Result<moriq_library::media::Preview> {
        self.preview_for(library, asset, Some(version)).await
    }
    async fn preview_for(
        &self,
        library: LibraryId,
        asset: AssetId,
        version: Option<AssetVersionId>,
    ) -> Result<moriq_library::media::Preview> {
        let cache = self.media_cache.clone();
        let prepare_cache = cache.clone();
        // A disk derivative needs at most 64 MiB and must not queue behind a
        // full decoder's 1 GiB admission. Corrupt/missing caches escalate separately.
        let (version, cacheable, cached) = self
            .read_plan(
                64,
                move |active| {
                    let lib = check(active, library)?;
                    let source = match version {
                        Some(version) => lib.version_source(asset, version),
                        None => lib.media_source(asset),
                    }
                    .map_err(|e| e.to_string())?;
                    let cached = source
                        .cacheable()
                        .then(|| prepare_cache.get((library, source.version(), 1600)))
                        .flatten();
                    Ok((source, cached))
                },
                move |(source, cached)| {
                    Ok((
                        source.version(),
                        source.cacheable(),
                        cached.or_else(|| source.cached_preview()),
                    ))
                },
            )
            .await?;
        let key = (library, version, 1600);
        if let Some(value) = cached {
            if cacheable {
                cache.put(key, value.clone());
            }
            return Ok(value);
        }
        self.read_plan(
            1024,
            move |active| {
                check(active, library)?
                    .version_source(asset, version)
                    .map_err(|e| e.to_string())
            },
            move |source| {
                let key = (library, version, 1600);
                let cacheable = source.cacheable();
                if cacheable && let Some(value) = cache.get(key) {
                    return Ok(value);
                }
                let value = source.preview().map_err(|e| e.to_string())?;
                if cacheable {
                    cache.put(key, value.clone());
                }
                Ok(value)
            },
        )
        .await
    }
    pub async fn preview(&self, library: LibraryId, asset: AssetId) -> Result<Pixels> {
        self.preview_full(library, asset).await.map(|p| p.image)
    }
    pub async fn preview_original(
        &self,
        library: LibraryId,
        asset: AssetId,
        version: AssetVersionId,
    ) -> Result<Pixels> {
        self.read_plan(
            2048,
            move |active| {
                check(active, library)?
                    .version_source(asset, version)
                    .map_err(|e| e.to_string())
            },
            |source| {
                source
                    .preview_edge(0)
                    .map(|p| p.image)
                    .map_err(|e| e.to_string())
            },
        )
        .await
    }
    pub async fn playback(
        &self,
        library: LibraryId,
        asset: AssetId,
        version: AssetVersionId,
    ) -> Result<Arc<Playback>> {
        self.read_plan(
            64,
            move |active| {
                check(active, library)?
                    .version_source(asset, version)
                    .map_err(|e| e.to_string())
            },
            |s| s.playback().map_err(|e| e.to_string()),
        )
        .await
    }
    pub async fn compare_versions(
        &self,
        library: LibraryId,
        asset: AssetId,
        left: AssetVersionId,
        right: AssetVersionId,
    ) -> Result<(Pixels, Pixels)> {
        self.read_plan(
            1024,
            move |active| {
                let library = check(active, library)?;
                Ok((
                    library
                        .version_source(asset, left)
                        .map_err(|e| e.to_string())?,
                    library
                        .version_source(asset, right)
                        .map_err(|e| e.to_string())?,
                ))
            },
            move |(left, right)| {
                // Sequential decode: one decoder's peak allocation plus two bounded previews.
                Ok((
                    left.preview().map_err(|e| e.to_string())?.image,
                    right.preview().map_err(|e| e.to_string())?.image,
                ))
            },
        )
        .await
    }
    #[cfg(feature = "capture")]
    pub async fn save_capture_evidence(&self, path: PathBuf, text: String) -> Result<()> {
        tokio::task::spawn_blocking(move || std::fs::write(path, text).map_err(|e| e.to_string()))
            .await
            .map_err(|e| e.to_string())?
    }
    #[cfg(feature = "capture")]
    pub async fn save_capture(
        &self,
        path: PathBuf,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    ) -> Result<()> {
        self.call(move |_| {
            moriq_library::save_capture(&path, width, height, &rgba).map_err(|e| e.to_string())
        })
        .await
    }
}
