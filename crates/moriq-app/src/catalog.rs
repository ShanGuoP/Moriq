//! Catalog queries and metadata transactions.
use super::*;

impl Service {
    /// 在写入队列中校验资源库身份后，按复制选项创建目标项目副本。
    pub async fn copy_to_project(
        &self,
        id: LibraryId,
        assets: Vec<AssetId>,
        project: ProjectId,
        annotations: bool,
    ) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .copy_to_project(&assets, project, annotations)
                .map_err(|e| e.to_string())
        })
        .await
    }
    /// 通过写入端创建带独立配置的项目，返回更新后的管理视图。
    pub async fn create_project_configured(
        &self,
        id: LibraryId,
        name: String,
        parent: Option<ProjectId>,
        template: ProjectTemplate,
    ) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .create_project_configured(&name, parent, template)
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
        .await
    }
    /// 保存当前项目的分类配置，并协调受影响任务的取消与界面刷新。
    pub async fn save_ai_profile(&self, id: LibraryId, profile: AiProfile) -> Result<AiProfile> {
        let cancellation = self.job_cancel.clone();
        self.call(move |active| {
            let (profile, cancelled) = check(active, id)?
                .save_ai_profile(profile)
                .map_err(|e| e.to_string())?;
            for job in cancelled {
                cancellation.stop(Some((id, &job)));
            }
            Ok(profile)
        })
        .await
    }
    /// 提交指定版本的人工 AI 内容修改，拒绝过期资源库或项目上下文。
    pub async fn edit_ai_content(
        &self,
        id: LibraryId,
        asset: AssetId,
        version: AssetVersionId,
        edit: AiEdit,
    ) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .edit_ai_content(asset, version, edit)
                .map_err(|e| e.to_string())
        })
        .await
    }
    /// 从当前资源库读取持久化浏览历史。
    pub async fn saved_navigation(&self, id: LibraryId) -> Result<Option<SavedNavigation>> {
        self.call(move |active| {
            check(active, id)?
                .saved_navigation()
                .map_err(|e| e.to_string())
        })
        .await
    }
    /// 在校验资源库身份后保存浏览历史。
    pub async fn save_navigation(&self, id: LibraryId, value: SavedNavigation) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .save_navigation(&value)
                .map_err(|e| e.to_string())
        })
        .await
    }
    /// 把当前范围的批量编辑提交为持久化任务。
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
    /// 按编辑命令调整虚拟文件夹及其成员，并返回最新管理结果。
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
    /// 保存人工素材分类并取得后续展示所需的管理状态。
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
    /// 经写入队列修改当前项目人物名称。
    pub async fn rename_person(&self, id: LibraryId, person: String, name: String) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .rename_person(&person, &name)
                .map_err(|e| e.to_string())
        })
        .await
    }
    /// 校验资源库后合并人物分组及人脸关联。
    pub async fn merge_people(&self, id: LibraryId, source: String, target: String) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .merge_people(&source, &target)
                .map_err(|e| e.to_string())
        })
        .await
    }
    /// 读取素材当前版本的人脸与人物归属记录。
    pub async fn faces(&self, id: LibraryId, asset: AssetId) -> Result<Vec<Face>> {
        self.call(move |active| check(active, id)?.faces(asset).map_err(|e| e.to_string()))
            .await
    }
    /// 通过写入端保存人工人脸归属。
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
    /// 按用户选择清理 AI 数据，并协调相关后台工作。
    pub async fn clear_ai_indexes(&self, id: LibraryId, model: String) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .clear_ai_indexes(&model)
                .map_err(|e| e.to_string())
        })
        .await
    }
    /// 准备相似性读取计划，在受预算约束的工作线程比较素材。
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
    /// 读取首页所需的轻量素材摘要，不替换图库查询快照。
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
    /// 校验素材与版本身份后切换展示版本。
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
    /// 校验资源库身份后保存用户编辑的说明文本。
    pub async fn description(&self, id: LibraryId, value: String) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .description(&value)
                .map_err(|e| e.to_string())
        })
        .await
    }
    /// 读取当前资源库的自定义封面像素。
    pub async fn cover(&self, id: LibraryId) -> Result<Option<Pixels>> {
        self.dispatch(true, move |active| {
            check(active, id)?.cover().map_err(|e| e.to_string())
        })
        .await
    }
    /// 把指定图片设置为当前资源库的独立封面，返回保存结果。
    pub async fn set_cover(&self, id: LibraryId, path: PathBuf) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .set_cover(&path)
                .map_err(|e| e.to_string())
        })
        .await
    }
    /// 冻结封面版本、在媒体线程生成图片，再复核资源库身份后提交封面。
    pub async fn set_asset_cover(
        &self,
        id: LibraryId,
        asset: AssetId,
        version: AssetVersionId,
    ) -> Result<()> {
        let prepared = self
            .read_plan(
                1024,
                move |active| {
                    check(active, id)?
                        .asset_cover_plan(asset, version)
                        .map_err(|e| e.to_string())
                },
                |plan| plan.render().map_err(|e| e.to_string()),
            )
            .await?;
        self.call(move |active| {
            check(active, id)?
                .commit_asset_cover(prepared)
                .map_err(|e| e.to_string())
        })
        .await
    }
    /// 通过写入队列取得项目、分类、标签和任务等管理摘要。
    pub async fn organizer(&self, id: LibraryId) -> Result<Organizer> {
        self.call(move |active| check(active, id)?.organizer().map_err(|e| e.to_string()))
            .await
    }
    /// 先冻结素材详情计划，再在媒体线程读取 EXIF 等文件信息。
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
    /// 提交素材资料修改，沿用资源库身份校验与写入顺序。
    pub async fn edit(&self, id: LibraryId, assets: Vec<AssetId>, edit: Edit) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .edit(&assets, &edit)
                .map_err(|e| e.to_string())
        })
        .await
    }
    /// 通过当前资源库写入端修改项目名称。
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
    /// 提交项目结构删除，并返回更新后的管理摘要。
    pub async fn delete_project(&self, id: LibraryId, project: ProjectId) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .delete_project(project)
                .map_err(|e| e.to_string())
        })
        .await
    }
    /// 读取项目完整删除范围，供用户确认后再次提交。
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
    /// 按已确认计划删除项目内容，拒绝范围已变化的请求。
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
    /// 创建或更新当前项目智能集合的查询条件。
    pub async fn collection(&self, id: LibraryId, name: String, query: Query) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .save_collection(&name, &query)
                .map_err(|e| e.to_string())
        })
        .await
    }
    /// 删除当前项目智能集合并刷新管理数据。
    pub async fn delete_collection(&self, id: LibraryId, collection: String) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .delete_collection(&collection)
                .map_err(|e| e.to_string())
        })
        .await
    }
    /// 使用默认选项导出所选素材。
    pub async fn export(
        &self,
        id: LibraryId,
        assets: Vec<AssetId>,
        parent: PathBuf,
    ) -> Result<PathBuf> {
        self.export_options(id, assets, parent, ExportOptions::default())
            .await
    }
    /// 冻结导出版本并等待共享内存预算，在工作线程输出文件。
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
    /// 读取当前查询中两个锚点之间的选择范围。
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
    /// 通过写入队列在指定父级创建子项目。
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
    /// 校验项目关系后调整父级，避免形成项目树循环。
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
    /// 经写入端重命名或合并标签，并刷新关联视图。
    pub async fn rename_tag(&self, id: LibraryId, source: String, target: String) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .rename_tag(&source, &target)
                .map_err(|e| e.to_string())
        })
        .await
    }
    /// 删除标签及其关联，保留素材文件。
    pub async fn delete_tag(&self, id: LibraryId, name: String) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .delete_tag(&name)
                .map_err(|e| e.to_string())
        })
        .await
    }
    /// 保存当前资源库显示名称，并更新对应摘要。
    pub async fn rename_library(&self, id: LibraryId, name: String) -> Result<LibraryInfo> {
        self.call(move |active| {
            let library = check(active, id)?;
            library.rename(&name).map_err(|e| e.to_string())?;
            library.info().map_err(|e| e.to_string())
        })
        .await
    }
    /// 校验库身份后修改素材名称。
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
    /// 请求系统文件管理器打开或定位指定路径。
    pub async fn reveal(&self, library: LibraryId) -> Result<()> {
        let path = self
            .call(move |active| Ok(check(active, library)?.root().to_path_buf()))
            .await?;
        launch::open(path.into_os_string(), false).await
    }
    /// 先解析素材路径，再请求系统文件管理器定位该文件。
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
    /// 读取查询对应尺寸并计算瀑布流布局，供界面有界展示。
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
    /// 提交素材当前版本的人工地点修改。
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
    /// 校验地点并打开对应地图链接。
    pub async fn open_map(&self, id: LibraryId, location: AssetLocation) -> Result<()> {
        let url = self
            .call(move |active| {
                check(active, id)?;
                location.map_url().ok_or_else(|| "坐标无效".into())
            })
            .await?;
        launch::open(url.into(), false).await
    }
    /// 读取当前资源库的活动或通知游标页面。
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
    /// 写入资源库通知，保留关联素材和重试信息。
    pub async fn notice(&self, id: LibraryId, notice: Notice) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .notice(&notice)
                .map_err(|e| e.to_string())
        })
        .await
    }
    /// 按已展示截止位置更新通知已读状态。
    pub async fn read_notifications(&self, id: LibraryId, through: i64) -> Result<()> {
        self.call(move |active| {
            check(active, id)?
                .read_notifications(through)
                .map_err(|e| e.to_string())
        })
        .await
    }
    /// 登记项目最近访问时间，不修改素材内容。
    pub async fn visit_project(&self, id: LibraryId, project: ProjectId) -> Result<LibraryInfo> {
        self.call(move |active| {
            let lib = check(active, id)?;
            lib.visit_project(project).map_err(|e| e.to_string())?;
            lib.info().map_err(|e| e.to_string())
        })
        .await
    }
    /// 提交素材移入或恢复回收站的状态变更。
    pub async fn asset_trash_state(&self, id: LibraryId, asset: AssetId) -> Result<Option<bool>> {
        self.call(move |active| {
            check(active, id)?
                .asset_trash_state(asset)
                .map_err(|e| e.to_string())
        })
        .await
    }
    /// 在写入队列中读取资源库基本资料。
    pub async fn info(&self, id: LibraryId) -> Result<LibraryInfo> {
        self.call(move |active| check(active, id)?.info().map_err(|e| e.to_string()))
            .await
    }
    /// 读取当前查询的一页素材，并验证请求仍属于当前资源库。
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
    /// 读取素材查看器当前锚点附近的素材和位置。
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
    /// 提交单项收藏状态变更。
    pub async fn favorite(&self, library: LibraryId, asset: AssetId, value: bool) -> Result<()> {
        self.call(move |active| {
            check(active, library)?
                .set_favorite(asset, value)
                .map_err(|e| e.to_string())
        })
        .await
    }
    /// 校验并提交素材评分变更。
    pub async fn rating(&self, library: LibraryId, asset: AssetId, value: u8) -> Result<()> {
        self.call(move |active| {
            check(active, library)?
                .set_rating(asset, value)
                .map_err(|e| e.to_string())
        })
        .await
    }
    /// 提交人工标签和自动标签取舍。
    pub async fn tags(&self, library: LibraryId, asset: AssetId, value: Vec<String>) -> Result<()> {
        self.call(move |active| {
            check(active, library)?
                .set_tags(asset, &value)
                .map_err(|e| e.to_string())
        })
        .await
    }
    /// 在当前资源库创建项目并返回其身份。
    pub async fn project(&self, library: LibraryId, name: String) -> Result<ProjectId> {
        self.call(move |active| {
            check(active, library)?
                .create_project(&name)
                .map_err(|e| e.to_string())
        })
        .await
    }
}
