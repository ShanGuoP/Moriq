//! Catalog queries and metadata transactions.
use super::*;

impl Service {
    pub async fn saved_navigation(&self, id: LibraryId) -> Result<Option<SavedNavigation>> {
        self.call(move |active| {
            check(active, id)?
                .saved_navigation()
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn save_navigation(&self, id: LibraryId, value: SavedNavigation) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .save_navigation(&value)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn batch_edit(
        &self,
        id: LibraryId,
        scope: BatchScope,
        patch: BatchPatch,
    ) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .enqueue_batch(scope, patch)
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn folder_edit(&self, id: LibraryId, edit: FolderEdit) -> Result<()> {
        self.call(move |active| {
            let library = check(active, id)?;
            match edit {
                FolderEdit::Create(name, favorite) => {
                    library.create_virtual_folder(&name, favorite).map(|_| ())
                }
                FolderEdit::Rename(folder, name) => library.rename_virtual_folder(&folder, &name),
                FolderEdit::Delete(folder) => library.delete_virtual_folder(&folder),
                FolderEdit::Members(folder, assets, add) => {
                    library.virtual_folder_members(&folder, &assets, add)
                }
                FolderEdit::Cover(folder, asset) => library.virtual_folder_cover(&folder, asset),
            }
            .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn set_category(
        &self,
        id: LibraryId,
        assets: Vec<AssetId>,
        category: String,
    ) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .set_category(&assets, &category)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn rename_person(&self, id: LibraryId, person: String, name: String) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .rename_person(&person, &name)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn merge_people(&self, id: LibraryId, source: String, target: String) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .merge_people(&source, &target)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn faces(&self, id: LibraryId, asset: AssetId) -> Result<Vec<Face>> {
        self.call(move |active| check(active, id)?.faces(asset).map_err(|e| e.to_string()))
            .await
    }
    pub async fn assign_face(
        &self,
        id: LibraryId,
        face: String,
        person: Option<String>,
    ) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .assign_face(&face, person.as_deref())
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn clear_ai_indexes(&self, id: LibraryId, model: String) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .clear_ai_indexes(&model)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn similar_assets(&self, id: LibraryId, asset: AssetId) -> Result<Vec<SimilarAsset>> {
        self.read_plan(
            64,
            move |active| {
                check(active, id)?
                    .similarity_plan()
                    .map_err(|e| e.to_string())
            },
            move |plan| plan.search(asset).map_err(|e| e.to_string()),
        )
        .await
    }
    pub async fn overview(&self, id: LibraryId) -> Result<LoadedOverview> {
        self.read_plan_sized(
            |(_, _, _, thumbnails): &(_, _, _, moriq_library::media::Thumbnails)| {
                thumbnails.memory_mib()
            },
            move |active| {
                let lib = check(active, id)?;
                let moriq_library::overview::Assets {
                    favorites,
                    categories,
                } = lib.overview_assets().map_err(|e| e.to_string())?;
                let projects = lib.project_covers().map_err(|e| e.to_string())?;
                let versions = favorites
                    .iter()
                    .map(|a| a.version_id)
                    .chain(projects.iter().filter_map(|(_, v)| *v))
                    .chain(categories.iter().filter_map(|(_, v)| *v))
                    .collect::<Vec<_>>();
                let thumbnails = lib.thumbnail_plan(&versions).map_err(|e| e.to_string())?;
                Ok((favorites, projects, categories, thumbnails))
            },
            move |(favorites, projects, categories, thumbnails)| {
                let (images, _warnings) = thumbnails.read();
                let mut images = images.into_iter();
                let favorites = favorites
                    .into_iter()
                    .map(|asset| Tile {
                        asset,
                        thumbnail: images.next().flatten(),
                    })
                    .collect();
                let projects = projects
                    .into_iter()
                    .map(|(id, version)| (id, version.and_then(|_| images.next().flatten())))
                    .collect();
                let categories = categories
                    .into_iter()
                    .map(|(label, version)| (label, version.and_then(|_| images.next().flatten())))
                    .collect();
                Ok(LoadedOverview {
                    categories,
                    favorites,
                    projects,
                })
            },
        )
        .await
    }
    pub async fn display_version(
        &self,
        id: LibraryId,
        asset: AssetId,
        version: AssetVersionId,
    ) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .display_version(asset, version)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn description(&self, id: LibraryId, value: String) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .description(&value)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn cover(&self, id: LibraryId) -> Result<Option<Pixels>> {
        self.dispatch(true, move |active| {
            check(active, id)?.cover().map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn set_cover(&self, id: LibraryId, path: PathBuf) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .set_cover(&path)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn organizer(&self, id: LibraryId) -> Result<Organizer> {
        self.call(move |active| check(active, id)?.organizer().map_err(|e| e.to_string()))
            .await
    }
    pub async fn details(&self, id: LibraryId, asset: AssetId) -> Result<AssetDetails> {
        self.read_plan(
            32,
            move |active| {
                check(active, id)?
                    .details_plan(asset)
                    .map_err(|e| e.to_string())
            },
            move |plan| Ok(plan.read()),
        )
        .await
    }
    pub async fn edit(&self, id: LibraryId, assets: Vec<AssetId>, edit: Edit) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .edit(&assets, &edit)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn rename_project(
        &self,
        id: LibraryId,
        project: ProjectId,
        name: String,
    ) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .rename_project(project, &name)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn delete_project(&self, id: LibraryId, project: ProjectId) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .delete_project(project)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn project_deletion_plan(
        &self,
        id: LibraryId,
        project: ProjectId,
    ) -> Result<ProjectDeletionPlan> {
        self.call(move |active| {
            check(active, id)?
                .project_deletion_plan(project)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn delete_project_contents(
        &self,
        id: LibraryId,
        plan: ProjectDeletionPlan,
    ) -> Result<PurgeReport> {
        self.call(move |active| {
            check(active, id)?
                .delete_project_contents(&plan)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn collection(&self, id: LibraryId, name: String, query: Query) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .save_collection(&name, &query)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn delete_collection(&self, id: LibraryId, collection: String) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .delete_collection(&collection)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn export(
        &self,
        id: LibraryId,
        assets: Vec<AssetId>,
        parent: PathBuf,
    ) -> Result<PathBuf> {
        self.export_options(id, assets, parent, ExportOptions::default())
            .await
    }
    pub async fn export_options(
        &self,
        id: LibraryId,
        assets: Vec<AssetId>,
        parent: PathBuf,
        options: ExportOptions,
    ) -> Result<PathBuf> {
        let original = options.format == ExportFormat::Original
            && options.max_edge == 0
            && options.rotate == 0;
        self.read_plan(
            if original { 64 } else { 4096 },
            move |active| {
                check(active, id)?
                    .export_plan(&assets)
                    .map_err(|e| e.to_string())
            },
            move |plan| {
                moriq_library::export::run(plan, &parent, options).map_err(|e| e.to_string())
            },
        )
        .await
    }
    pub async fn selection_range(
        &self,
        id: LibraryId,
        query: Query,
        first: AssetId,
        last: AssetId,
    ) -> Result<Vec<AssetId>> {
        self.call(move |active| {
            check(active, id)?
                .selection_range(&query, first, last)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn create_child_project(
        &self,
        id: LibraryId,
        name: String,
        parent: Option<ProjectId>,
    ) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .create_child_project(&name, parent)
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn move_project(
        &self,
        id: LibraryId,
        project: ProjectId,
        parent: Option<ProjectId>,
    ) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .move_project(project, parent)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn rename_tag(&self, id: LibraryId, source: String, target: String) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .rename_tag(&source, &target)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn delete_tag(&self, id: LibraryId, name: String) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .delete_tag(&name)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn rename_library(&self, id: LibraryId, name: String) -> Result<LibraryInfo> {
        self.call(move |active| {
            let library = check(active, id)?;
            library.rename(&name).map_err(|e| e.to_string())?;
            library.info().map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn rename_asset(
        &self,
        library: LibraryId,
        asset: AssetId,
        name: String,
    ) -> Result<()> {
        self.call(move |active| {
            check(active, library)?
                .rename_asset(asset, &name)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn reveal(&self, library: LibraryId) -> Result<()> {
        let path = self
            .call(move |active| Ok(check(active, library)?.root().to_path_buf()))
            .await?;
        launch::open(path.into_os_string(), false).await
    }
    pub async fn reveal_asset(&self, library: LibraryId, asset: AssetId) -> Result<()> {
        let path = self
            .call(move |active| {
                check(active, library)?
                    .asset_path(asset)
                    .map_err(|e| e.to_string())
            })
            .await?;
        launch::open(path.into_os_string(), true).await
    }
    pub async fn masonry(
        &self,
        library: LibraryId,
        query: Query,
        columns: usize,
        width: f32,
    ) -> Result<Arc<moriq_render::MasonryIndex>> {
        let sizes = self
            .dispatch(true, move |active| {
                check(active, library)?
                    .layout_sizes(&query)
                    .map_err(|e| e.to_string())
            })
            .await?;
        tokio::task::spawn_blocking(move || {
            Arc::new(moriq_render::MasonryIndex::new(
                &sizes, columns, width, 10.0,
            ))
        })
        .await
        .map_err(|e| e.to_string())
    }
    pub async fn set_location(
        &self,
        id: LibraryId,
        asset: AssetId,
        version: AssetVersionId,
        value: Option<AssetLocation>,
    ) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .set_location(asset, version, value.as_ref())
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn open_map(&self, id: LibraryId, location: AssetLocation) -> Result<()> {
        let url = self
            .call(move |active| {
                check(active, id)?;
                location.map_url().ok_or_else(|| "坐标无效".into())
            })
            .await?;
        launch::open(url.into(), false).await
    }
    pub async fn events(
        &self,
        id: LibraryId,
        notices: bool,
        before: Option<i64>,
    ) -> Result<EventPage> {
        self.call(move |active| {
            check(active, id)?
                .events(notices, before)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn notice(&self, id: LibraryId, notice: Notice) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .notice(&notice)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn read_notifications(&self, id: LibraryId, through: i64) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .read_notifications(through)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn visit_project(&self, id: LibraryId, project: ProjectId) -> Result<LibraryInfo> {
        self.call(move |active| {
            let lib = check(active, id)?;
            lib.visit_project(project).map_err(|e| e.to_string())?;
            lib.info().map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn asset_trash_state(&self, id: LibraryId, asset: AssetId) -> Result<Option<bool>> {
        self.call(move |active| {
            check(active, id)?
                .asset_trash_state(asset)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn info(&self, id: LibraryId) -> Result<LibraryInfo> {
        self.call(move |active| check(active, id)?.info().map_err(|e| e.to_string()))
            .await
    }
    pub async fn page(
        &self,
        id: LibraryId,
        query: Query,
        offset: usize,
        limit: usize,
    ) -> Result<LoadedPage> {
        self.read_plan_sized(
            |(_, thumbnails): &(_, moriq_library::media::Thumbnails)| thumbnails.memory_mib(),
            move |active| {
                let lib = check(active, id)?;
                let page = lib.page(&query, offset, limit).map_err(|e| e.to_string())?;
                let versions = page.assets.iter().map(|a| a.version_id).collect::<Vec<_>>();
                let thumbnails = lib.thumbnail_plan(&versions).map_err(|e| e.to_string())?;
                Ok((page, thumbnails))
            },
            move |(page, thumbnails)| {
                let (images, warnings) = thumbnails.read();
                Ok(LoadedPage {
                    total: page.total,
                    offset: page.offset,
                    warnings,
                    tiles: page
                        .assets
                        .into_iter()
                        .zip(images)
                        .map(|(asset, thumbnail)| Tile { asset, thumbnail })
                        .collect(),
                })
            },
        )
        .await
    }
    pub async fn neighborhood(
        &self,
        id: LibraryId,
        query: Query,
        anchor: AssetId,
        direction: i8,
    ) -> Result<Option<LoadedNeighborhood>> {
        self.read_plan_sized(
            |plan: &Option<(_, moriq_library::media::Thumbnails)>| {
                plan.as_ref()
                    .map_or(16, |(_, thumbnails)| thumbnails.memory_mib())
            },
            move |active| {
                let lib = check(active, id)?;
                let Some(result) = lib
                    .neighborhood(&query, anchor, direction)
                    .map_err(|e| e.to_string())?
                else {
                    return Ok(None);
                };
                let thumbnails = lib
                    .thumbnail_plan(
                        &result
                            .page
                            .assets
                            .iter()
                            .map(|a| a.version_id)
                            .collect::<Vec<_>>(),
                    )
                    .map_err(|e| e.to_string())?;
                Ok(Some((result, thumbnails)))
            },
            move |plan| {
                let Some((result, thumbnails)) = plan else {
                    return Ok(None);
                };
                let (images, warnings) = thumbnails.read();
                Ok(Some(LoadedNeighborhood {
                    selected: result.selected,
                    position: result.position,
                    page: LoadedPage {
                        total: result.page.total,
                        offset: result.page.offset,
                        warnings,
                        tiles: result
                            .page
                            .assets
                            .into_iter()
                            .zip(images)
                            .map(|(asset, thumbnail)| Tile { asset, thumbnail })
                            .collect(),
                    },
                }))
            },
        )
        .await
    }
    pub async fn favorite(&self, library: LibraryId, asset: AssetId, value: bool) -> Result<()> {
        self.call(move |active| {
            check(active, library)?
                .set_favorite(asset, value)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn rating(&self, library: LibraryId, asset: AssetId, value: u8) -> Result<()> {
        self.call(move |active| {
            check(active, library)?
                .set_rating(asset, value)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn tags(&self, library: LibraryId, asset: AssetId, value: Vec<String>) -> Result<()> {
        self.call(move |active| {
            check(active, library)?
                .set_tags(asset, &value)
                .map_err(|e| e.to_string())
        })
        .await
    }
    pub async fn project(&self, library: LibraryId, name: String) -> Result<ProjectId> {
        self.call(move |active| {
            check(active, library)?
                .create_project(&name)
                .map_err(|e| e.to_string())
        })
        .await
    }
}
