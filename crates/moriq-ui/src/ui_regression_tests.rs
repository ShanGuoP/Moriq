use super::*;

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
