use super::*;
/// 构造未连接用户生产数据的应用状态，供界面消息和视图测试使用。
pub(super) fn app() -> App {
    let (mut app, task) = App::new(Service::start().unwrap(), LaunchOptions::default());
    drop(task);
    app.library = Some(Opened {
        root: PathBuf::from("test.moriq"),
        info: LibraryInfo {
            id: LibraryId::default(),
            name: "Test".into(),
            count: 100000,
            favorites: 0,
            bytes: 0,
            created_at: 0,
            updated_at: 0,
            projects: vec![],
        },
    });
    app.total = 100000;
    app
}
#[test]
/// 验证未打开资源库也可管理模型，资源库操作保持独立。
fn global_models_open_without_library_and_keep_library_actions_separate() {
    let mut app = app();
    app.library = None;
    app.library_picker = true;
    drop(app.update(Message::Navigate(2)));
    assert_eq!(app.content_screen(), Some(Screen::Models));
    assert!(!app.library_picker);
    drop(app.shell());
    let old_policy = app.on_demand_models;
    drop(app.update(Message::ToggleModelPolicy));
    assert_eq!(app.on_demand_models, !old_policy);
    let control = moriq_app::DownloadControl::default();
    app.ai.downloads.insert("faces".into(), control.clone());
    drop(app.update(Message::Ai(AiMessage::Pause("faces".into()))));
    assert!(control.stop.load(Ordering::Relaxed));
    drop(app.update(Message::Ai(AiMessage::Category("人像".into()))));
    drop(app.update(Message::Ai(AiMessage::People)));
    assert_eq!(app.content_screen(), Some(Screen::Models));
    assert!(app.manager.panel.is_none());
    drop(app.update(Message::ShowLibraries));
    assert_eq!(app.content_screen(), None);
    assert_eq!(app.screen, Screen::Overview);
    drop(app.update(Message::Navigate(2)));
    assert_eq!(app.content_screen(), Some(Screen::Models));
}
#[test]
/// 验证模型侧栏展示全部资源库，切换时保留全局模型状态。
fn model_sidebar_has_all_libraries_and_switch_keeps_global_model_state() {
    let mut app = app();
    app.screen = Screen::Models;
    let old = app.library.clone().unwrap();
    app.preferences.recent = (0..18)
        .map(|n| ("同名库".into(), PathBuf::from(format!("{n}.moriq"))))
        .collect();
    assert_eq!(app.model_libraries().len(), 19);
    assert_eq!(
        app.model_libraries()
            .iter()
            .filter(|(_, _, active)| *active)
            .count(),
        1
    );
    app.model_search = "CLIP".into();
    app.ai.downloads.insert("faces".into(), Default::default());
    let mut next = old.clone();
    next.root = PathBuf::from("17.moriq");
    next.info.id = LibraryId::default();
    next.info.name = "新的名称".into();
    app.switching = true;
    drop(app.update(Message::Ai(AiMessage::LibraryOpened(Ok(next.clone())))));
    assert_eq!(app.content_screen(), Some(Screen::Models));
    assert!(!app.switching);
    assert!(!app.library_picker);
    assert_eq!(app.library.as_ref().unwrap().info.id, next.info.id);
    assert_eq!(app.model_search, "CLIP");
    assert!(app.ai.downloads.contains_key("faces"));
    assert!(
        app.model_libraries()
            .iter()
            .any(|(name, path, active)| name == "新的名称" && *path == next.root && *active)
    );
    drop(app.update(Message::Ai(AiMessage::LibraryOpened(Err(
        "路径不存在".into(),
    )))));
    assert_eq!(app.library.as_ref().unwrap().info.id, next.info.id);
    assert_eq!(app.content_screen(), Some(Screen::Models));
    assert!(app.error.is_some());
    app.manager.busy = true;
    drop(app.update(Message::Ai(AiMessage::SwitchLibrary(old.root))));
    assert!(!app.switching);
}
#[test]
/// 验证更改模型加载策略不会重置下载来源。
fn changing_model_load_policy_keeps_selected_download_source() {
    let mut app = app();
    assert!(app.preferences.ai.use_mirror);
    drop(app.update(Message::ToggleModelMirror));
    assert!(!app.preferences.ai.use_mirror);
    drop(app.update(Message::ToggleModelPolicy));
    drop(app.update(Message::ToggleRetainIndexes));
    assert!(!app.preferences.ai.use_mirror);
}
#[test]
/// 验证分类封面拒绝其他库结果，分类为空时清除图片。
fn category_covers_reject_other_libraries_and_clear_when_category_is_empty() {
    use moriq_app::LoadedOverview;
    let mut app = app();
    let owner = app.library.as_ref().unwrap().info.id;
    let loaded = || LoadedOverview {
        categories: vec![(
            "风景".into(),
            Some(moriq_app::Pixels {
                width: 1,
                height: 1,
                rgba: vec![10, 20, 30, 255].into(),
            }),
        )],
        ..Default::default()
    };
    drop(app.update(Message::Manage(ManageMessage::OverviewLoaded(
        LibraryId::default(),
        app.projection_request,
        Ok(loaded()),
    ))));
    assert!(app.manager.category_covers.is_empty());
    drop(app.update(Message::Manage(ManageMessage::OverviewLoaded(
        owner,
        app.projection_request,
        Ok(loaded()),
    ))));
    assert!(app.manager.category_covers.contains_key("风景"));
    drop(app.update(Message::Manage(ManageMessage::OverviewLoaded(
        owner,
        app.projection_request,
        Ok(LoadedOverview::default()),
    ))));
    assert!(app.manager.category_covers.is_empty());
}
#[test]
/// 验证资源库选择器保留图库状态，并可通过历史返回。
fn library_picker_keeps_gallery_state_and_returns_through_history() {
    let mut app = app();
    app.screen = Screen::Assets;
    app.scroll_y = 640.0;
    app.query.text = "旅行".into();
    drop(app.commit_navigation(Message::ShowLibraries));
    assert!(app.library_picker);
    assert_eq!(app.query.text, "旅行");
    drop(app.travel(false));
    assert!(!app.library_picker);
    assert_eq!(app.screen, Screen::Assets);
    assert_eq!(app.scroll_y, 640.0);
    assert_eq!(app.query.text, "旅行");
    drop(app.travel(true));
    assert!(app.library_picker);
    drop(app.commit_navigation(Message::Navigate(2)));
    assert!(!app.library_picker);
    assert_eq!(app.screen, Screen::Models);
}
#[test]
/// 验证旧库维护和删除回执不会重置新库选择。
fn maintenance_and_purge_receipts_cannot_reset_another_library_selection() {
    let mut app = app();
    let control = moriq_app::MaintenanceControl::default();
    app.manager.maintenance = Some(control.clone());
    app.manager.busy = true;
    let selected = AssetId::default();
    app.manager.selection.assets.insert(selected);
    let old = LibraryId::default();
    drop(app.update(Message::Manage(ManageMessage::Checked(
        old,
        Ok(MaintenanceReport::default()),
    ))));
    drop(app.update(Message::Manage(ManageMessage::Purged(
        old,
        Ok(PurgeReport::default()),
    ))));
    drop(app.update(Message::Deletion(DeletionMessage::Finished(
        old,
        0,
        Ok(PurgeReport::default()),
    ))));
    assert!(app.manager.busy);
    assert!(app.manager.maintenance.is_some());
    assert!(app.manager.selection.assets.contains(&selected));
    drop(app.update(Message::Manage(ManageMessage::CancelCheck)));
    assert!(control.cancel.load(Ordering::Relaxed));
}
#[test]
/// 验证各页外壳一致，全局导航不留下延迟动作。
fn page_chrome_is_identical_and_global_navigation_has_no_delayed_action() {
    let mut app = app();
    let mut metrics = Vec::new();
    for screen in [Screen::Overview, Screen::Assets, Screen::Models] {
        app.screen = screen;
        metrics.push((
            app.sidebar_width(),
            app.frame_padding(),
            app.titlebar_height(),
        ));
    }
    assert!(metrics.windows(2).all(|pair| pair[0] == pair[1]));
    app.library = None;
    drop(app.update(Message::Navigate(0)));
    drop(app.update(Message::Navigate(2)));
    assert_eq!(app.screen, Screen::Models);
    assert!(app.transition.is_some());
}
#[test]
/// 验证快速导航即时提交，旧动画帧不能改变目标。
fn rapid_page_navigation_commits_immediately_and_old_frames_cannot_retarget() {
    use crate::transition::FADE_IN;
    let mut app = app();
    app.screen = Screen::Overview;
    drop(app.update(Message::Navigate(2)));
    assert_eq!(app.screen, Screen::Models);
    let start = app.transition.as_ref().unwrap().started;
    drop(app.update(Message::TransitionFrame(start + FADE_IN / 2)));
    let alpha = app.transition.as_ref().unwrap().alpha();
    drop(app.update(Message::Navigate(1)));
    assert_eq!(app.screen, Screen::Assets);
    assert_eq!(app.transition.as_ref().unwrap().alpha(), alpha);
    drop(app.update(Message::TransitionFrame(start + FADE_IN * 2)));
    assert!(app.transition.is_none());
    assert_eq!(app.screen, Screen::Assets);
}

#[test]
/// 验证旧查询回执不能替换新查询画面。
fn old_query_results_cannot_replace_new_view() {
    let mut app = app();
    app.generation = 3;
    drop(app.update(Message::PageLoaded(
        2,
        Ok(LoadedPage {
            total: 0,
            offset: 0,
            tiles: vec![],
            warnings: 0,
        }),
    )));
    assert_eq!(app.total, 100000);
    assert_eq!(app.generation, 3);
}
#[test]
/// 验证范围选择回执同时匹配库与查询身份。
fn range_selection_receipts_obey_library_and_query_identity() {
    let mut app = app();
    let library = app.library.as_ref().unwrap().info.id;
    let asset = AssetId::default();
    app.manager.selection.enabled = true;
    app.generation = 5;
    drop(app.update(Message::Manage(ManageMessage::RangeSelected(
        library,
        4,
        Ok(vec![asset]),
    ))));
    drop(app.update(Message::Manage(ManageMessage::RangeSelected(
        LibraryId::default(),
        5,
        Ok(vec![asset]),
    ))));
    assert!(app.manager.selection.assets.is_empty());
    drop(app.update(Message::Manage(ManageMessage::RangeSelected(
        library,
        5,
        Ok(vec![asset]),
    ))));
    assert!(app.manager.selection.assets.contains(&asset));
    app.manager.selection.anchor = Some(asset);
    drop(app.reset_query());
    assert!(app.manager.selection.anchor.is_none());
}
#[test]
/// 验证大显示区域仍遵守有界分页预算。
fn large_displays_keep_viewport_inside_page_budget() {
    let mut app = app();
    for (width, height) in [(1360.0, 800.0), (3840.0, 2160.0), (7680.0, 4320.0)] {
        app.viewport = Size::new(width, height);
        for list in [false, true] {
            app.list = list;
            let grid = app.grid();
            assert!(grid.visible(500000, 90000.0).len() <= 128);
        }
    }
}
#[test]
/// 验证加载期间滚动变化后请求当前视口范围。
fn scroll_moved_during_load_requests_current_viewport() {
    let mut app = app();
    app.screen = Screen::Assets;
    app.generation = 3;
    app.loading = true;
    app.scroll_y = 90000.0;
    drop(app.update(Message::PageLoaded(
        3,
        Ok(LoadedPage {
            total: 100000,
            offset: 0,
            tiles: vec![],
            warnings: 0,
        }),
    )));
    assert!(app.loading);
    assert_eq!(app.generation, 4);
}
#[test]
/// 验证编辑保存跨同库查询切换有效，但拒绝其他库回执。
fn saved_edit_survives_query_change_but_rejects_other_library() {
    let mut app = app();
    app.screen = Screen::Assets;
    app.generation = 8;
    let current = app.library.as_ref().unwrap().info.id;
    drop(app.update(Message::Saved(
        LibraryId::default(),
        Err("stale failure".into()),
    )));
    assert!(app.error.is_none());
    drop(app.update(Message::Saved(current, Ok(()))));
    assert_eq!(app.generation, 9);
    assert!(app.loading);
}
#[test]
/// 验证后台保存完成不会关闭设置对话框。
fn background_save_does_not_close_settings_dialog() {
    let mut app = app();
    let id = app.library.as_ref().unwrap().info.id;
    app.dialog = Some(Dialog::Settings);
    drop(app.update(Message::Saved(id, Ok(()))));
    assert_eq!(app.dialog, Some(Dialog::Settings));
}
/// 向测试应用放入可选择素材，供手势和详情验证。
pub(super) fn selected_asset() -> Asset {
    Asset {
        id: AssetId::default(),
        version_id: AssetVersionId::default(),
        name: "测试".into(),
        format: "png".into(),
        width: 20,
        height: 12,
        bytes: 30,
        favorite: false,
        rating: 0,
        tags: vec![],
        imported_at: 0,
    }
}
#[test]
/// 验证图片手势作用于实际点击卡片，缺失或被浮层阻挡时拒绝。
fn image_gestures_target_clicked_card_and_reject_missing_or_modal_targets() {
    let mut app = app();
    app.screen = Screen::Assets;
    let first = selected_asset();
    let second = selected_asset();
    app.tiles = vec![
        Tile {
            asset: first.clone(),
            handle: None,
        },
        Tile {
            asset: second.clone(),
            handle: None,
        },
    ];
    app.viewer.selected = Some(first.clone());
    app.manager.selection.enabled = true;
    app.manager.selection.assets.insert(first.id);
    drop(app.update(Message::AssetContext(second.id)));
    assert_eq!(app.viewer.selected.as_ref().unwrap().id, first.id);
    assert_eq!(app.manager.panel, Some(manage::Panel::AssetMenu));
    assert!(app.manager.selection.enabled);
    assert_eq!(app.selected_ids(), vec![second.id]);
    assert!(!app.viewer.preview_expanded);
    drop(app.update(Message::OpenImage(first.id)));
    assert_eq!(app.viewer.selected.as_ref().unwrap().id, first.id);
    drop(app.update(Message::Manage(ManageMessage::Close)));
    drop(app.update(Message::OpenImage(first.id)));
    assert_eq!(app.viewer.selected.as_ref().unwrap().id, first.id);
    assert!(app.viewer.preview_expanded);
    drop(app.update(Message::OpenImage(AssetId::default())));
    assert_eq!(app.viewer.selected.as_ref().unwrap().id, first.id);
    app.dialog = Some(Dialog::Settings);
    drop(app.update(Message::AssetContext(second.id)));
    assert_eq!(app.viewer.selected.as_ref().unwrap().id, first.id);
    assert!(app.manager.panel.is_none());
}
#[test]
/// 验证活动分页与关联素材导航拒绝旧回执。
fn journal_pages_and_related_asset_navigation_reject_stale_receipts() {
    let mut app = app();
    let id = app.library.as_ref().unwrap().info.id;
    drop(app.update(Message::Journal(JournalMessage::Open(true))));
    let page = EventPage {
        unread: 8,
        ..Default::default()
    };
    for (lib, request) in [(LibraryId::default(), 1), (id, 0)] {
        drop(app.update(Message::Journal(JournalMessage::Loaded(
            lib,
            true,
            request,
            true,
            Ok(page.clone()),
        ))));
    }
    assert_eq!(app.journal.notifications.unread, 0);
    drop(app.update(Message::Journal(JournalMessage::Loaded(
        id,
        true,
        1,
        true,
        Ok(page.clone()),
    ))));
    assert_eq!(app.journal.notifications.unread, 8);
    let reopened = app.library.clone().unwrap();
    drop(app.update(Message::Opened(Ok(reopened))));
    drop(app.update(Message::Journal(JournalMessage::Loaded(
        id,
        true,
        1,
        true,
        Ok(page),
    ))));
    assert_eq!(app.journal.notifications.unread, 0);
    let asset = AssetId::default();
    drop(app.update(Message::Journal(JournalMessage::OpenAsset(asset))));
    let request = app.browse_request;
    drop(app.update(Message::Search("当前搜索".into())));
    drop(app.update(Message::Journal(JournalMessage::Located(
        id,
        request,
        asset,
        Ok(Some(true)),
    ))));
    assert_eq!(app.query.text, "当前搜索");
    assert!(!app.query.trash);
}
#[test]
/// 验证项目菜单保留图库上下文，取消或过期删除不改数据。
fn project_menu_keeps_gallery_context_and_cancelled_or_stale_delete_preserves_data() {
    let mut app = app();
    let project = ProjectId::default();
    let current = ProjectId::default();
    app.library.as_mut().unwrap().info.projects.push(Project {
        visited_at: 0,
        id: project,
        parent: None,
        name: "空项目".into(),
        count: 0,
    });
    app.query.project = Some(current);
    app.screen = Screen::Assets;
    let selected = selected_asset();
    let id = selected.id;
    app.viewer.selected = Some(selected);
    drop(app.update(Message::Manage(ManageMessage::ProjectMenu(project))));
    assert_eq!(app.query.project, Some(current));
    assert_eq!(app.manager.project_target, Some(project));
    assert_eq!(app.manager.label, "空项目");
    assert_eq!(app.viewer.selected.as_ref().unwrap().id, id);
    app.manager.busy = true;
    drop(app.update(Message::Deletion(DeletionMessage::Planned(
        LibraryId::default(),
        0,
        Err("已取消".into()),
    ))));
    drop(app.update(Message::Manage(ManageMessage::ProjectDeleted(
        LibraryId::default(),
        vec![current],
        Ok(PurgeReport::default()),
    ))));
    assert!(app.manager.busy);
    assert_eq!(app.query.project, Some(current));
    app.manager.busy = false;
    app.manager.panel = Some(manage::Panel::Delete);
    drop(app.update(Message::Deletion(DeletionMessage::Cancel)));
    assert!(!app.manager.busy);
    assert_eq!(app.viewer.selected.as_ref().unwrap().id, id);
}
/// 推进测试界面至预期静止状态，供后续导航断言使用。
fn settle(app: &mut App) {
    if let Some(t) = &app.transition {
        let start = t.started;
        drop(app.update(Message::TransitionFrame(start + Duration::from_millis(70))));
    }
    app.transition = None;
}
#[test]
/// 验证历史恢复查询、滚动、选择与草稿，新导航清除前进分支。
fn history_restores_query_scroll_selection_and_draft_then_truncates_forward() {
    let mut app = app();
    app.screen = Screen::Assets;
    app.query.tag = "人物".into();
    app.scroll_y = 4500.0;
    let asset = selected_asset();
    let id = asset.id;
    app.viewer.selected = Some(asset);
    app.manager.selection.assets.insert(id);
    app.manager.selection.anchor = Some(id);
    app.manager.selection.enabled = true;
    app.manager.notes = "未保存备注".into();
    app.manager.notes_dirty = true;
    drop(app.update(Message::Favorites));
    settle(&mut app);
    assert!(app.history.can_back());
    assert!(app.query.favorites_only);
    drop(app.update(Message::Back));
    settle(&mut app);
    assert_eq!(app.query.tag, "人物");
    assert_eq!(app.scroll_y, 4500.0);
    assert!(app.manager.selection.assets.contains(&id));
    assert_eq!(app.manager.selection.anchor, Some(id));
    assert!(app.manager.selection.enabled);
    assert_eq!(app.restored_notes, Some((id, "未保存备注".into())));
    assert!(app.history.can_forward());
    drop(app.update(Message::Forward));
    settle(&mut app);
    assert!(app.query.favorites_only);
    drop(app.update(Message::Back));
    settle(&mut app);
    drop(app.update(Message::Manage(ManageMessage::Tag("风景".into()))));
    assert!(!app.history.can_forward());
}
#[test]
/// 验证连续搜索合并历史，切到新库清空前后栈。
fn search_history_is_coalesced_and_new_library_clears_both_stacks() {
    let mut app = app();
    app.screen = Screen::Assets;
    for text in ["日", "日落", "日落海边"] {
        drop(app.update(Message::Search(text.into())));
    }
    drop(app.update(Message::Back));
    settle(&mut app);
    assert_eq!(app.query, Query::default());
    assert!(!app.history.can_back());
    assert!(app.history.can_forward());
    let mut other = app.library.clone().unwrap();
    other.info.id = LibraryId::default();
    drop(app.update(Message::Opened(Ok(other))));
    assert!(!app.history.can_back());
    assert!(!app.history.can_forward());
    assert!(app.pending_scroll.is_none());
}
#[test]
/// 验证相邻浏览拒绝旧请求、旧查询和其他库回执。
fn neighborhood_receipts_reject_old_requests_queries_and_libraries() {
    let mut app = app();
    app.screen = Screen::Assets;
    let asset = selected_asset();
    let id = asset.id;
    let library = app.library.as_ref().unwrap().info.id;
    let result = moriq_app::LoadedNeighborhood {
        selected: id,
        position: 128,
        page: LoadedPage {
            total: 300,
            offset: 125,
            tiles: vec![moriq_app::Tile {
                asset: asset.clone(),
                thumbnail: None,
            }],
            warnings: 0,
        },
    };
    app.browse_request = 5;
    for (lib, request) in [(library, 4), (LibraryId::default(), 5)] {
        drop(app.update(Message::NeighborhoodLoaded(
            lib,
            request,
            Ok(Some(result.clone())),
        )));
        assert!(app.viewer.selected.is_none());
    }
    drop(app.update(Message::NeighborhoodLoaded(
        library,
        5,
        Ok(Some(result.clone())),
    )));
    assert_eq!(app.viewer.selected.as_ref().unwrap().id, id);
    assert_eq!(app.neighbor_position, Some((128, 300)));
    drop(app.update(Message::ClosePreview));
    drop(app.update(Message::NeighborhoodLoaded(
        library,
        5,
        Ok(Some(result.clone())),
    )));
    assert_eq!(app.viewer.selected.as_ref().unwrap().id, id);
    drop(app.update(Message::Search("新的查询".into())));
    drop(app.update(Message::NeighborhoodLoaded(library, 5, Ok(Some(result)))));
    assert_eq!(app.viewer.selected.as_ref().unwrap().id, id);
}
#[test]
/// 验证查询变化使旧布局失效，版本回执不会重新选择素材。
fn changing_query_invalidates_pending_layout_and_version_receipt_does_not_reselect() {
    let mut app = app();
    let asset = selected_asset();
    let id = asset.id;
    app.viewer.selected = Some(asset);
    let library = app.library.as_ref().unwrap().info.id;
    drop(app.update(Message::Manage(ManageMessage::VersionSelected(
        library,
        AssetId::default(),
        AssetVersionId::default(),
        Ok(()),
    ))));
    assert_eq!(app.viewer.selected.as_ref().unwrap().id, id);
    let generation = app.layout_generation;
    drop(app.update(Message::Search("新查询".into())));
    assert!(app.layout_generation > generation);
    drop(app.update(Message::MasonryLoaded(
        generation,
        Ok(std::sync::Arc::new(moriq_render::MasonryIndex::new(
            &[(10, 10)],
            1,
            100.0,
            10.0,
        ))),
    )));
    assert!(app.masonry.is_none());
}
#[test]
/// 验证旧版本预览不能替换当前版本图像。
fn old_version_preview_cannot_replace_the_new_version() {
    let mut app = app();
    let id = app.library.as_ref().unwrap().info.id;
    let asset = selected_asset();
    let old = AssetVersionId::default();
    app.viewer.selected = Some(asset.clone());
    drop(app.update(Message::FullPreviewLoaded(
        id,
        asset.id,
        old,
        Ok((
            image::Handle::from_rgba(1, 1, vec![0; 4]),
            image::Handle::from_rgba(1, 1, vec![0; 4]),
        )),
    )));
    assert!(app.viewer.preview.is_none());
}
#[test]
/// 验证后台详情和保存回执保留用户较新的未保存笔记。
fn background_details_and_save_receipts_preserve_newer_unsaved_notes() {
    let mut app = app();
    let id = app.library.as_ref().unwrap().info.id;
    let asset = selected_asset();
    app.viewer.selected = Some(asset.clone());
    drop(app.update(Message::Manage(ManageMessage::Notes(
        "用户刚输入的新内容".into(),
    ))));
    drop(app.update(Message::Manage(ManageMessage::DetailsLoaded(
        id,
        asset.id,
        asset.version_id,
        Ok(AssetDetails {
            notes: "旧数据库内容".into(),
            ..Default::default()
        }),
    ))));
    drop(app.update(Message::Manage(ManageMessage::NotesSaved(
        id,
        asset.id,
        "旧数据库内容".into(),
        Ok(()),
    ))));
    assert_eq!(app.manager.notes, "用户刚输入的新内容");
    assert!(app.manager.notes_dirty);
    drop(app.update(Message::Manage(ManageMessage::OrganizerLoaded(
        LibraryId::default(),
        app.projection_request,
        Ok(Organizer {
            trash_count: 100,
            ..Default::default()
        }),
    ))));
    assert_eq!(app.manager.organizer.trash_count, 0);
}
