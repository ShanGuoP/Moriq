use super::*;

#[test]
fn ctrl_click_and_context_menu_preserve_the_batch_and_plain_click_exits_additive_selection() {
    let mut app = crate::tests::app();
    app.screen = Screen::Assets;
    let a = crate::tests::selected_asset();
    let b = crate::tests::selected_asset();
    let c = crate::tests::selected_asset();
    app.tiles = [a.clone(), b.clone(), c.clone()]
        .into_iter()
        .map(|asset| Tile {
            asset,
            handle: None,
        })
        .collect();
    app.viewer.selected = Some(a.clone());
    drop(app.update(Message::Modifiers(iced::keyboard::Modifiers::CTRL)));
    drop(app.update(Message::CardPressed(b.id)));
    assert_eq!(app.selected_ids().len(), 2);
    assert!(app.selected_ids().contains(&a.id));
    assert!(app.selected_ids().contains(&b.id));
    drop(app.update(Message::Modifiers(iced::keyboard::Modifiers::empty())));
    app.scroll_y = 90_000.0;
    drop(app.update(Message::AssetContext(a.id)));
    assert_eq!(app.selected_ids().len(), 2);
    assert_eq!(app.scroll_y, 90_000.0);
    assert_eq!(app.manager.panel, Some(manage::Panel::AssetMenu));
    drop(app.update(Message::Manage(ManageMessage::Close)));
    drop(app.update(Message::Modifiers(iced::keyboard::Modifiers::CTRL)));
    drop(app.update(Message::CardPressed(b.id)));
    assert_eq!(app.selected_ids(), vec![a.id]);
    drop(app.update(Message::Modifiers(iced::keyboard::Modifiers::empty())));
    drop(app.update(Message::CardPressed(c.id)));
    assert!(!app.manager.selection.enabled);
    assert_eq!(app.viewer.selected.as_ref().unwrap().id, c.id);
    drop(app.update(Message::Manage(ManageMessage::ToggleMulti)));
    drop(app.update(Message::CardPressed(a.id)));
    drop(app.update(Message::CardPressed(b.id)));
    assert_eq!(app.selected_ids().len(), 2);
}

#[test]
fn opening_and_closing_context_menu_keeps_existing_scroll_widget_state() {
    use iced::advanced::widget::{Tree, tree};
    use std::collections::HashSet;
    fn states(tree: &Tree, tag: tree::Tag, result: &mut HashSet<usize>) {
        if tree.tag == tag
            && let tree::State::Some(state) = &tree.state
        {
            result.insert(&**state as *const dyn std::any::Any as *const () as usize);
        }
        for child in &tree.children {
            states(child, tag, result);
        }
    }
    let mut app = crate::tests::app();
    app.screen = Screen::Assets;
    app.scroll_y = 90_000.0;
    let a = crate::tests::selected_asset();
    app.tiles = vec![Tile {
        asset: a.clone(),
        handle: None,
    }];
    app.viewer.selected = Some(a.clone());
    let scroll: Element<'_, Message> = crate::components::scroll(iced::widget::Space::new()).into();
    let tag = scroll.as_widget().tag();
    let mut tree = Tree::new(app.shell());
    let mut before = HashSet::new();
    states(&tree, tag, &mut before);
    assert!(before.len() >= 2);
    for message in [
        Message::AssetContext(a.id),
        Message::Manage(ManageMessage::Close),
    ] {
        drop(app.update(message));
        tree.diff(app.shell());
        let mut after = HashSet::new();
        states(&tree, tag, &mut after);
        assert!(
            before.is_subset(&after),
            "opening a popup remounted an existing scrollable"
        );
        assert_eq!(app.scroll_y, 90_000.0);
    }
}

#[test]
fn back_from_library_root_opens_picker_and_forward_restores_projects() {
    let mut app = crate::tests::app();
    app.preferences.reduced_motion = true;
    app.screen = Screen::Overview;
    assert!(app.can_go_back());
    drop(app.update(Message::Back));
    assert!(app.library_picker);
    assert!(app.history.can_forward());
    drop(app.update(Message::Forward));
    assert!(!app.library_picker);
    assert_eq!(app.screen, Screen::Overview);
}

#[test]
fn asset_cover_save_is_single_flight_scoped_and_refreshes_the_library_card() {
    let mut app = crate::tests::app();
    let owner = app.library.as_ref().unwrap().info.id;
    let root = app.library.as_ref().unwrap().root.clone();
    app.viewer.selected = Some(crate::tests::selected_asset());
    app.manager.panel = Some(manage::Panel::AssetMenu);
    app.manager.notes = "尚未保存的备注".into();
    app.manager.notes_dirty = true;
    app.query.trash = true;
    drop(app.manage_message(ManageMessage::AssetCover));
    assert!(app.manager.cover_saving.is_none());
    app.query.trash = false;
    drop(app.manage_message(ManageMessage::AssetCover));
    let pending = app.manager.cover_saving.unwrap();
    assert_eq!(pending.0, owner);
    assert!(app.manager.panel.is_none());
    drop(app.manage_message(ManageMessage::AssetCover));
    assert_eq!(app.manager.cover_saving, Some(pending));
    drop(app.manage_message(ManageMessage::CoverSaved(
        owner,
        pending.1.wrapping_add(1),
        Ok(()),
    )));
    drop(app.manage_message(ManageMessage::CoverSaved(
        LibraryId::default(),
        pending.1,
        Ok(()),
    )));
    assert_eq!(app.manager.cover_saving, Some(pending));
    drop(app.manage_message(ManageMessage::CoverSaved(
        owner,
        pending.1,
        Err("missing source".into()),
    )));
    assert!(app.manager.cover_saving.is_none());
    assert_eq!(app.error.as_deref(), Some("missing source"));
    app.error = None;
    drop(app.manage_message(ManageMessage::AssetCover));
    let (_, request) = app.manager.cover_saving.unwrap();
    drop(app.manage_message(ManageMessage::CoverSaved(owner, request, Ok(()))));
    assert!(app.manager.cover_saving.is_none());
    let handle = image::Handle::from_rgba(1, 1, vec![40, 80, 120, 255]);
    drop(app.manage_message(ManageMessage::CoverLoaded(
        owner,
        app.cover_request,
        Ok(Some(handle)),
    )));
    assert!(app.manager.cover.is_some());
    assert!(app.library_covers.contains_key(&root));
    let stale = app.library_cards_request.wrapping_sub(1);
    drop(app.update(Message::LibraryCardsLoaded(
        stale,
        Ok(vec![(root.clone(), None)]),
    )));
    assert!(app.library_covers.contains_key(&root));
    assert!(app.manager.notes_dirty);
    assert_eq!(app.manager.notes, "尚未保存的备注");
    let other = LibraryId::default();
    app.library.as_mut().unwrap().info.id = other;
    let before = app.cover_request;
    drop(app.manage_message(ManageMessage::CoverSaved(owner, request, Ok(()))));
    assert_eq!(app.cover_request, before);
}

#[test]
fn model_tabs_put_all_first_installed_last_without_swapping_filter_ids() {
    let mut app = crate::tests::app();
    assert_eq!(crate::models::MODEL_FILTER_ORDER, [2, 1, 0]);
    assert_eq!(app.model_filter, 2);
    drop(app.update(Message::ModelFilter(0)));
    assert_eq!(
        crate::models::MODEL_FILTER_ORDER
            .iter()
            .position(|id| *id == app.model_filter),
        Some(2)
    );
    drop(app.update(Message::ModelFilter(2)));
    assert_eq!(
        crate::models::MODEL_FILTER_ORDER
            .iter()
            .position(|id| *id == app.model_filter),
        Some(0)
    );
}

#[test]
fn configurable_cards_and_sidebar_share_order_names_and_stable_group_queries() {
    let mut app = crate::tests::app();
    let query = Query {
        themes: vec!["theme_stage".into()],
        ..Query::default()
    };
    app.manager.organizer.ai_groups = vec![AiGroup {
        id: "theme_stage".into(),
        label: "舞台表演".into(),
        query: query.clone(),
        count: 4,
        cover: None,
    }];
    assert_eq!(app.sidebar_categories(), vec![("舞台表演".into(), 4)]);
    drop(app.ai_message(AiMessage::Group("theme_stage".into())));
    assert_eq!(app.query, query);
    app.manager.organizer.ai_groups[0].label = "演出现场".into();
    assert_eq!(app.sidebar_categories(), vec![("演出现场".into(), 4)]);
    assert_eq!(app.query.themes, vec!["theme_stage"]);
}

#[test]
fn category_editor_is_a_draft_and_rejects_old_library_save_receipts() {
    use crate::categories::CategoryMessage as C;
    let mut app = crate::tests::app();
    let owner = app.library.as_ref().unwrap().info.id;
    app.manager.organizer.ai_groups = vec![AiGroup {
        id: "_unclassified".into(),
        label: "未分类".into(),
        query: Query::default(),
        count: 0,
        cover: None,
    }];
    let original = app.manager.organizer.ai_profile.clone();
    drop(app.category_message(C::Open));
    drop(app.category_message(C::Add));
    let id = app.ai.categories.selected.clone().unwrap();
    drop(app.category_message(C::Name("演出现场".into())));
    drop(app.category_message(C::Move(-1)));
    drop(app.category_message(C::Enabled(false)));
    assert_eq!(app.manager.organizer.ai_profile, original);
    assert!(
        app.ai
            .categories
            .profile
            .categories
            .iter()
            .any(|c| c.id == id && !c.enabled)
    );
    let request = app.ai.categories.request;
    drop(app.category_message(C::Saved(
        LibraryId::default(),
        request,
        Ok(original.clone()),
    )));
    assert!(app.ai.categories.dirty);
    drop(app.category_message(C::Saved(owner, request.wrapping_sub(1), Ok(original))));
    assert!(app.ai.categories.dirty);
    app.ai.themes = vec![id];
    let saved = app.ai.categories.profile.clone();
    drop(app.category_message(C::Saved(owner, request, Ok(saved))));
    assert!(app.ai.themes.is_empty());
    assert!(!app.ai.categories.dirty);
}

#[test]
fn content_filter_ids_are_independent_of_labels_and_support_theme_intersection() {
    let mut q = Query::default();
    FilterChange::Medium("photography").apply(&mut q, 0);
    FilterChange::Theme("portrait").apply(&mut q, 0);
    FilterChange::Theme("cosplay").apply(&mut q, 0);
    assert_eq!(q.medium, "photography");
    assert_eq!(q.themes, vec!["portrait", "cosplay"]);
    FilterChange::Theme("portrait").apply(&mut q, 0);
    assert_eq!(q.themes, vec!["cosplay"]);
    assert_eq!(q.medium, "photography");
}

#[test]
fn switching_versions_cannot_save_the_previous_versions_content_draft() {
    let mut app = crate::tests::app();
    let mut asset = crate::tests::selected_asset();
    app.ai.category_version = Some(asset.version_id);
    app.ai.description = "旧版未保存内容".into();
    asset.version_id = AssetVersionId::default();
    app.viewer.selected = Some(asset);
    drop(app.ai_message(AiMessage::SaveDescription));
    assert!(!app.ai.content_saving);
    drop(app.ai_message(AiMessage::DescriptionAction(
        iced::widget::text_editor::Action::Edit(iced::widget::text_editor::Edit::Paste(
            std::sync::Arc::new("错误写入".into()),
        )),
    )));
    assert_eq!(app.ai.description, "旧版未保存内容");
}

#[test]
fn content_drafts_survive_refresh_and_late_save_receipts_do_not_clear_new_drafts() {
    let mut app = crate::tests::app();
    let owner = app.library.as_ref().unwrap().info.id;
    let asset = crate::tests::selected_asset();
    app.viewer.selected = Some(asset.clone());
    let details = AssetDetails {
        medium: "photography".into(),
        themes: vec!["portrait".into()],
        ai_caption: "自动原文".into(),
        ..AssetDetails::default()
    };
    drop(app.manage_message(ManageMessage::DetailsLoaded(
        owner,
        asset.id,
        asset.version_id,
        Ok(details.clone()),
    )));
    drop(app.ai_message(AiMessage::DescriptionAction(
        iced::widget::text_editor::Action::SelectAll,
    )));
    drop(app.ai_message(AiMessage::DescriptionAction(
        iced::widget::text_editor::Action::Edit(iced::widget::text_editor::Edit::Paste(
            std::sync::Arc::new("尚未保存".into()),
        )),
    )));
    drop(app.ai_message(AiMessage::Theme("cosplay".into())));
    drop(app.manage_message(ManageMessage::DetailsLoaded(
        owner,
        asset.id,
        asset.version_id,
        Ok(details),
    )));
    assert_eq!(app.ai.description, "尚未保存");
    assert!(app.ai.themes.contains(&"cosplay".into()));
    app.ai.content_request = 7;
    app.ai.content_saving = true;
    drop(app.ai_message(AiMessage::ContentStored(
        owner,
        asset.id,
        asset.version_id,
        6,
        2,
        Ok(()),
    )));
    assert!(app.ai.description_dirty);
    assert!(app.ai.content_saving);
    drop(app.ai_message(AiMessage::ContentStored(
        owner,
        asset.id,
        asset.version_id,
        7,
        2,
        Ok(()),
    )));
    assert!(!app.ai.description_dirty);
    assert!(app.ai.classification_dirty);
}

#[test]
fn sidebar_uses_actual_classifications_including_custom_names() {
    let mut app = crate::tests::app();
    app.manager.organizer.ai_categories = vec![
        ("猫".into(), 3),
        ("人像".into(), 2),
        ("风景".into(), 0),
        ("".into(), 1),
    ];
    assert_eq!(
        app.sidebar_categories(),
        vec![("猫".into(), 3), ("人像".into(), 2)]
    );
    app.manager.organizer.ai_categories.clear();
    assert!(app.sidebar_categories().is_empty());
}

#[test]
fn person_portraits_reject_previous_library_and_request_receipts() {
    let mut app = crate::tests::app();
    let owner = app.library.as_ref().unwrap().info.id;
    app.ai.portrait_request = 4;
    app.ai.portrait_cancel.store(false, Ordering::Release);
    let pixels = moriq_app::Pixels {
        width: 1,
        height: 1,
        rgba: vec![10, 20, 30, 255].into(),
    };
    for (library, request) in [(LibraryId::default(), 4), (owner, 3)] {
        drop(app.ai_message(AiMessage::PortraitLoaded(
            library,
            request,
            "person".into(),
            Ok(Some(pixels.clone())),
        )));
        assert!(app.ai.portraits.is_empty());
    }
    drop(app.ai_message(AiMessage::PortraitLoaded(
        owner,
        4,
        "person".into(),
        Ok(Some(pixels.clone())),
    )));
    assert!(app.ai.portraits.contains_key("person"));
    app.ai.portrait_cancel.store(true, Ordering::Release);
    drop(app.ai_message(AiMessage::PortraitLoaded(
        owner,
        4,
        "other".into(),
        Ok(Some(pixels)),
    )));
    assert!(!app.ai.portraits.contains_key("other"));
}

#[test]
fn leaving_assets_stops_viewer_and_rejects_late_worker_receipts() {
    for destination in [Message::Navigate(2), Message::ShowLibraries] {
        let mut app = crate::tests::app();
        app.screen = Screen::Assets;
        app.viewer.selected = Some(crate::tests::selected_asset());
        app.scroll_y = 320.;
        drop(app.set_viewer(true));
        drop(app.region_message(RegionMessage::Actual));
        let selected = app.viewer.selected.clone().unwrap();
        let owner = app.library.as_ref().unwrap().info.id;
        let request = app.viewer.original_request;
        drop(app.commit_navigation(destination));
        assert!(!app.viewer.preview_expanded);
        assert!(app.viewer.regions.session.is_none());
        assert!(app.viewer.original_request > request);
        drop(app.update(Message::PlaybackStarted(
            owner,
            selected.id,
            selected.version_id,
            request,
            Err("late".into()),
        )));
        assert!(app.viewer.playback.is_none());
        assert!(app.error.is_none());
    }
}

#[test]
fn stale_organizer_and_cover_projections_cannot_overwrite_newer_counts() {
    let mut app = crate::tests::app();
    let id = app.library.as_ref().unwrap().info.id;
    app.projection_request = 8;
    app.overview_request = 8;
    app.manager.organizer.trash_count = 3;
    drop(app.update(Message::Manage(ManageMessage::OrganizerLoaded(
        id,
        7,
        Ok(Organizer {
            trash_count: 99,
            ..Default::default()
        }),
    ))));
    assert_eq!(app.manager.organizer.trash_count, 3);
    drop(app.update(Message::Manage(ManageMessage::OverviewLoaded(
        id,
        7,
        Err("old error".into()),
    ))));
    assert!(app.error.is_none());
}

#[test]
fn metadata_saves_preserve_inflight_images_and_visual_changes_only_reload_images() {
    let mut app = crate::tests::app();
    let owner = app.library.as_ref().unwrap().info.id;
    drop(app.refresh_library());
    let images = app.overview_request;
    let cover = app.cover_request;
    let metadata = app.projection_request;
    // A rating/tag/notes receipt must not reload unchanged category photos,
    // or discard an image projection which started before the metadata save.
    drop(app.update(Message::Saved(owner, Ok(()))));
    assert!(app.projection_request > metadata);
    assert_eq!(app.overview_request, images);
    assert_eq!(app.cover_request, cover);
    drop(app.update(Message::Manage(ManageMessage::OverviewLoaded(
        owner,
        images,
        Ok(moriq_app::LoadedOverview {
            favorites: Vec::new(),
            projects: Vec::new(),
            categories: vec![(
                "风景".into(),
                Some(moriq_app::Pixels {
                    width: 1,
                    height: 1,
                    rgba: vec![10, 20, 30, 255].into(),
                }),
            )],
        }),
    ))));
    assert!(app.manager.category_covers.contains_key("风景"));
    drop(app.update(Message::VisualsSaved(owner, Ok(()))));
    assert!(app.overview_request > images);
    assert_eq!(app.cover_request, cover);
    drop(app.update(Message::Manage(ManageMessage::OverviewLoaded(
        owner,
        images,
        Err("old image receipt".into()),
    ))));
    assert!(app.error.is_none());
    let latest = app.overview_request;
    drop(app.update(Message::VisualsSaved(owner, Err("save failed".into()))));
    assert_eq!(app.overview_request, latest);
    assert!(app.manager.category_covers.contains_key("风景"));
}

#[test]
fn backup_cancel_and_library_switch_stop_copy_without_releasing_busy_before_completion() {
    let mut app = crate::tests::app();
    let owner = app.library.as_ref().unwrap().info.id;
    let control = moriq_app::BackupControl::default();
    app.lifecycle.copy = Some(control.clone());
    app.manager.busy = true;
    drop(app.lifecycle_message(LifecycleMessage::CancelCopy));
    assert!(control.cancel.load(Ordering::Acquire));
    assert!(app.manager.busy);
    drop(app.lifecycle_message(LifecycleMessage::Copied(owner, Err("cancelled".into()))));
    assert!(!app.manager.busy);
    assert!(app.lifecycle.copy.is_none());
    assert!(app.error.is_none());
    let other = moriq_app::BackupControl::default();
    app.lifecycle.copy = Some(other.clone());
    app.lifecycle.cancel_copy();
    assert!(other.cancel.load(Ordering::Acquire));
    assert!(app.lifecycle.copy.is_none());
}

#[test]
fn new_query_discards_old_scroll_restoration_and_refresh_does_not_cancel_saved_navigation() {
    let mut app = crate::tests::app();
    app.pending_scroll = Some(999.);
    app.surfaces.restoring_gallery = Some(999.);
    drop(app.reset_query());
    assert!(app.pending_scroll.is_none());
    assert!(app.surfaces.restoring_gallery.is_none());
    let id = app.library.as_ref().unwrap().info.id;
    let mut saved = app.navigation_snapshot();
    saved.current.query.text = "restored from disk".into();
    app.navigation_request = 10;
    app.generation += 1;
    drop(app.navigation_loaded(id, 10, Ok(Some(saved.clone()))));
    assert!(app.error.is_none());
    assert_eq!(app.query.text, "restored from disk");
    drop(app.commit_navigation(Message::AllAssets));
    drop(app.navigation_loaded(id, 10, Ok(Some(saved))));
    assert!(app.query.text.is_empty());
}

#[test]
fn reference_import_menu_sets_mode_and_cancel_clears_it() {
    let mut app = crate::tests::app();
    let task = app.update_inner(Message::Manage(ManageMessage::ReferenceImport));
    drop(task);
    assert!(app.manager.referenced_import);
    assert!(app.manager.busy);
    drop(app.update_inner(Message::PickedImages(None)));
    assert!(!app.manager.referenced_import);
    assert!(!app.manager.busy);
}

#[test]
fn reduced_motion_navigation_commits_immediately_and_does_not_leave_stale_actions() {
    let mut app = crate::tests::app();
    app.screen = Screen::Assets;
    drop(app.update(Message::Navigate(2)));
    assert!(app.transition.as_ref().unwrap().pending.is_some());
    drop(app.update(Message::ReducedMotion(true)));
    assert_eq!(app.screen, Screen::Models);
    assert!(app.transition.is_none());
    drop(app.update(Message::Navigate(1)));
    assert_eq!(app.screen, Screen::Assets);
    assert!(app.transition.is_none());
    drop(app.update(Message::ShowSettings));
    assert_eq!(app.dialog, Some(Dialog::Settings));
    assert!(app.surface_motion.is_none());
}

#[test]
fn scrolling_or_replacing_query_clears_virtualized_card_feedback() {
    let mut app = crate::tests::app();
    let id = AssetId::default();
    app.gallery_feedback
        .set(id, true, std::time::Instant::now(), false);
    assert!(app.gallery_feedback.active());
    drop(app.update_inner(Message::Scrolled(120.0)));
    assert!(!app.gallery_feedback.active());
    app.gallery_feedback
        .set(id, true, std::time::Instant::now(), false);
    drop(app.reset_query());
    assert!(!app.gallery_feedback.active());
}

#[test]
fn category_multiline_editor_follows_selection_and_preserves_explicit_definitions() {
    use crate::categories::CategoryMessage as C;
    use iced::widget::text_editor::{Action, Edit};
    let mut app = crate::tests::app();
    app.manager.organizer.ai_groups = vec![AiGroup {
        id: "_unclassified".into(),
        label: "未分类".into(),
        query: Query::default(),
        count: 0,
        cover: None,
    }];
    drop(app.category_message(C::Open));
    let first = app.ai.categories.selected.clone().unwrap();
    drop(app.category_message(C::Move(-1)));
    assert!(!app.ai.categories.dirty);
    drop(app.category_message(C::Definition(Action::SelectAll)));
    assert!(!app.ai.categories.dirty);
    drop(app.category_message(C::Add));
    let custom = app.ai.categories.selected.clone().unwrap();
    drop(app.category_message(C::Name("演出现场".into())));
    assert_eq!(app.ai.categories.definition_editor.text(), "演出现场");
    drop(app.category_message(C::Definition(Action::SelectAll)));
    let definition = "舞台表演与现场灯光。\n不包括软件界面截图。";
    drop(app.category_message(C::Definition(Action::Edit(Edit::Paste(
        std::sync::Arc::new(definition.into()),
    )))));
    drop(app.category_message(C::Name("舞台记录".into())));
    drop(app.category_message(C::Select(first)));
    drop(app.category_message(C::Select(custom.clone())));
    assert_eq!(app.ai.categories.definition_editor.text(), definition);
    let c = app
        .ai
        .categories
        .profile
        .categories
        .iter()
        .find(|c| c.id == custom)
        .unwrap();
    assert_eq!(c.definition, definition);
    assert_eq!(c.label, "舞台记录");
    app.ai.categories.busy = true;
    drop(app.category_message(C::Definition(Action::Edit(Edit::Insert('X')))));
    assert_eq!(app.ai.categories.definition_editor.text(), definition);
}

#[test]
fn description_editor_preserves_multiline_drafts_without_dirtying_on_cursor_actions() {
    use iced::widget::text_editor::{Action, Edit};
    let mut app = crate::tests::app();
    let asset = crate::tests::selected_asset();
    app.viewer.selected = Some(asset.clone());
    let owner = app.library.as_ref().unwrap().info.id;
    let loaded = AssetDetails {
        ai_caption: "原始描述".into(),
        ..AssetDetails::default()
    };
    drop(app.manage_message(ManageMessage::DetailsLoaded(
        owner,
        asset.id,
        asset.version_id,
        Ok(loaded.clone()),
    )));
    assert_eq!(app.ai.description_editor.text(), "原始描述");
    drop(app.ai_message(AiMessage::DescriptionAction(Action::SelectAll)));
    assert!(!app.ai.description_dirty);
    let draft = "用户补充的第一行。\n完整展示第二行。";
    drop(
        app.ai_message(AiMessage::DescriptionAction(Action::Edit(Edit::Paste(
            std::sync::Arc::new(draft.into()),
        )))),
    );
    drop(app.manage_message(ManageMessage::DetailsLoaded(
        owner,
        asset.id,
        asset.version_id,
        Ok(loaded),
    )));
    assert_eq!(app.ai.description, draft);
    assert_eq!(app.ai.description_editor.text(), draft);
    app.ai.content_saving = true;
    drop(
        app.ai_message(AiMessage::DescriptionAction(Action::Edit(Edit::Insert(
            'X',
        )))),
    );
    assert_eq!(app.ai.description, draft);
    app.ai.content_saving = false;
    let mut next = asset.clone();
    next.version_id = AssetVersionId::default();
    app.viewer.selected = Some(next.clone());
    drop(app.manage_message(ManageMessage::DetailsLoaded(
        owner,
        next.id,
        next.version_id,
        Ok(AssetDetails::default()),
    )));
    assert_eq!(app.ai.description_editor.text(), "");
    assert!(!app.ai.description_dirty);
}

#[test]
fn library_removal_requires_explicit_second_confirmation_and_rejects_stale_receipts() {
    use crate::libraries::LibraryMessage as L;
    let mut app = crate::tests::app();
    let original = app.library.clone().unwrap();
    app.preferences.recent = vec![("待删除库".into(), original.root.clone())];
    drop(app.library_message(L::Open(original.root.clone())));
    let request = app.libraries.request;
    let plan = moriq_app::LibraryRemovalPlan {
        root: original.root.clone(),
        id: original.info.id,
        name: "待删除库".into(),
        assets: 7,
    };
    drop(app.library_message(L::Plan(request, Ok(plan))));
    drop(app.library_message(L::Name("待删除库".into())));
    drop(app.library_message(L::Delete));
    assert!(!app.libraries.busy);
    drop(app.library_message(L::Next));
    drop(app.library_message(L::Name("错误名称".into())));
    drop(app.library_message(L::Delete));
    assert!(!app.libraries.busy);
    drop(app.library_message(L::Name("待删除库".into())));
    drop(app.library_message(L::Delete));
    assert!(app.libraries.busy);
    drop(app.library_message(L::Closed(request + 1, true, Ok(()))));
    assert!(app.library.is_some());
    drop(app.library_message(L::Closed(request, true, Err("locked".into()))));
    assert!(app.library.is_some());
    assert!(!app.switching);
    drop(app.library_message(L::Forget));
    assert!(app.libraries.busy);
    drop(app.library_message(L::Closed(request, false, Ok(()))));
    assert!(app.library.is_none());
    let prefs = UserSettings::default();
    drop(app.library_message(L::Finished(request, Ok(prefs))));
    assert!(!app.libraries.busy);
    assert!(app.preferences.recent.is_empty());
    assert!(app.manager.panel.is_none());
}
#[test]
fn new_project_destination_is_separate_from_the_edited_projects_parent() {
    let mut app = crate::tests::app();
    let parent = ProjectId::default();
    let destination = ProjectId::default();
    app.manager.parent = Some(parent);
    drop(app.manage_message(ManageMessage::NewParent(Some(destination))));
    assert_eq!(app.manager.parent, Some(parent));
    assert_eq!(app.manager.new_project_parent, Some(destination));
    app.manager.busy = true;
    drop(app.manage_message(ManageMessage::NewParent(None)));
    assert_eq!(app.manager.new_project_parent, Some(destination));
}

#[test]
fn applying_a_reusable_category_template_changes_only_the_current_draft() {
    use crate::categories::CategoryMessage as C;
    let mut app = crate::tests::app();
    app.manager.organizer.ai_groups = vec![AiGroup {
        id: "_unclassified".into(),
        label: "未分类".into(),
        query: Query::default(),
        count: 0,
        cover: None,
    }];
    let original = app.manager.organizer.ai_profile.clone();
    let mut template = AiProfile::default();
    template.categories.push(AiCategory {
        id: "theme_reusable".into(),
        kind: AiCategoryKind::Theme,
        label: "复用主题".into(),
        definition: "独立说明".into(),
        enabled: true,
    });
    app.preferences.category_template = Some(template);
    drop(app.category_message(C::Open));
    drop(app.category_message(C::ApplyTemplate));
    assert!(app.ai.categories.dirty);
    assert!(
        app.ai
            .categories
            .profile
            .categories
            .iter()
            .any(|c| c.id == "theme_reusable")
    );
    assert_eq!(app.manager.organizer.ai_profile, original);
}
