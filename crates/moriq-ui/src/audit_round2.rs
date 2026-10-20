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
        app.selected = Some(crate::tests::selected_asset());
        app.scroll_y = 320.;
        drop(app.set_viewer(true));
        drop(app.region_message(RegionMessage::Actual));
        let selected = app.selected.clone().unwrap();
        let owner = app.library.as_ref().unwrap().info.id;
        let request = app.original_request;
        drop(app.commit_navigation(destination));
        assert!(!app.preview_expanded);
        assert!(app.regions.session.is_none());
        assert!(app.original_request > request);
        drop(app.update(Message::PlaybackStarted(
            owner,
            selected.id,
            selected.version_id,
            request,
            Err("late".into()),
        )));
        assert!(app.playback.is_none());
        assert!(app.error.is_none());
    }
}

#[test]
fn stale_organizer_and_cover_projections_cannot_overwrite_newer_counts() {
    let mut app = crate::tests::app();
    let id = app.library.as_ref().unwrap().info.id;
    app.projection_request = 8;
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
