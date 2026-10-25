use super::*;

#[test]
fn changing_interface_keeps_library_query_selection_and_unsaved_form() {
    let mut app = crate::tests::app();
    let library = app.library.as_ref().unwrap().info.id;
    app.query.category = "人像".into();
    app.query.tag = "收藏".into();
    app.manager.notes = "未保存的备注".into();
    app.manager.notes_dirty = true;
    app.dialog = Some(Dialog::Settings);
    app.surfaces.y = 144.0;
    drop(app.update(Message::LanguageChanged(Language::English)));
    drop(app.update(Message::AppearanceChanged(Appearance::Dark)));
    assert_eq!(app.preferences.language, Language::English);
    assert_eq!(app.preferences.appearance, Appearance::Dark);
    assert_eq!(app.library.as_ref().unwrap().info.id, library);
    assert_eq!(app.query.category, "人像");
    assert_eq!(app.query.tag, "收藏");
    assert_eq!(app.manager.notes, "未保存的备注");
    assert!(app.manager.notes_dirty);
    assert_eq!(app.dialog, Some(Dialog::Settings));
    assert_eq!(app.surfaces.y, 144.0);
}

#[test]
fn rapid_changes_are_coalesced_and_stale_receipts_do_not_change_current_choice() {
    let mut app = crate::tests::app();
    drop(app.update(Message::LanguageChanged(Language::English)));
    assert_eq!(
        app.interface.saving,
        Some((Language::English, Appearance::Light))
    );
    drop(app.update(Message::AppearanceChanged(Appearance::Dark)));
    drop(app.update(Message::LanguageChanged(Language::SimplifiedChinese)));
    drop(app.update(Message::LanguageChanged(Language::English)));
    drop(app.update(Message::InterfaceSaved(
        Language::English,
        Appearance::Light,
        Ok(()),
    )));
    assert_eq!(
        app.interface.saving,
        Some((Language::English, Appearance::Dark))
    );
    // A duplicate or late save receipt must not mark the new choice as saved.
    drop(app.update(Message::InterfaceSaved(
        Language::English,
        Appearance::Light,
        Ok(()),
    )));
    assert_eq!(
        app.interface.saving,
        Some((Language::English, Appearance::Dark))
    );
    drop(app.update(Message::InterfaceSaved(
        Language::English,
        Appearance::Dark,
        Ok(()),
    )));
    assert!(app.interface.saving.is_none());
    assert_eq!(app.interface.saved, (Language::English, Appearance::Dark));
}

#[test]
fn unrelated_settings_receipts_preserve_interface_changes() {
    let mut app = crate::tests::app();
    drop(app.update(Message::LanguageChanged(Language::English)));
    drop(app.update(Message::AppearanceChanged(Appearance::Dark)));
    let prefs = UserSettings {
        diagnostic_logging: true,
        ..Default::default()
    };
    drop(app.update(Message::SettingsLoaded(Ok(prefs.clone()))));
    assert_eq!(app.preferences.language, Language::English);
    assert_eq!(app.preferences.appearance, Appearance::Dark);
    assert!(app.preferences.diagnostic_logging);
    drop(
        app.update(Message::Manage(ManageMessage::SyncoPreferencesSaved(Ok(
            prefs,
        )))),
    );
    assert_eq!(app.preferences.language, Language::English);
    assert_eq!(app.preferences.appearance, Appearance::Dark);
}

#[test]
fn failed_save_restores_the_last_saved_choice_and_reports_the_error() {
    let mut app = crate::tests::app();
    app.apply_settings(UserSettings {
        language: Language::English,
        ..Default::default()
    });
    drop(app.update(Message::AppearanceChanged(Appearance::Dark)));
    drop(app.update(Message::InterfaceSaved(
        Language::English,
        Appearance::Dark,
        Err("read only".into()),
    )));
    assert_eq!(app.preferences.appearance, Appearance::Light);
    assert_eq!(app.preferences.language, Language::English);
    assert!(
        app.error
            .as_ref()
            .unwrap()
            .contains("Could not save interface settings")
    );
    assert!(app.interface.saving.is_none());
}

#[test]
fn closing_waits_until_the_latest_interface_choice_has_saved() {
    let mut app = crate::tests::app();
    drop(app.update(Message::LanguageChanged(Language::English)));
    drop(app.update(Message::AppearanceChanged(Appearance::Dark)));
    drop(app.update(Message::WindowClose));
    assert!(app.interface.close_after_save);
    drop(app.update(Message::InterfaceSaved(
        Language::English,
        Appearance::Light,
        Ok(()),
    )));
    assert!(app.interface.close_after_save);
    assert_eq!(
        app.interface.saving,
        Some((Language::English, Appearance::Dark))
    );
    drop(app.update(Message::InterfaceSaved(
        Language::English,
        Appearance::Dark,
        Ok(()),
    )));
    assert!(!app.interface.close_after_save);
    assert!(app.interface.saving.is_none());
}

#[test]
fn english_filters_and_export_options_keep_their_canonical_values() {
    let mut app = crate::tests::app();
    app.preferences.language = Language::English;
    for change in [
        FilterChange::Media(filters::MediaChoice::Video),
        FilterChange::Orientation(filters::Orientation::Portrait),
        FilterChange::Sort(SortOrder::Rating),
        FilterChange::Color(filters::ColorChoice::Red),
    ] {
        drop(app.manage_message(ManageMessage::Filter(change)));
    }
    assert_eq!(app.manager.filters.query.media_kind, MediaKind::Video);
    assert_eq!(app.manager.filters.query.orientation, 2);
    assert_eq!(app.manager.filters.query.sort, SortOrder::Rating);
    assert_eq!(app.manager.filters.query.color, "红");
    drop(
        app.manage_message(ManageMessage::ExportOption(ExportChange::Resampling(
            Resampling::Bicubic,
        ))),
    );
    drop(
        app.manage_message(ManageMessage::ExportOption(ExportChange::Layout(
            ExportLayout::Format,
        ))),
    );
    assert_eq!(app.manager.export.options.resampling, Resampling::Bicubic);
    assert_eq!(app.manager.export.options.layout, ExportLayout::Format);
    assert_eq!(i18n::category(Language::English, "人像"), "People");
    assert_eq!(
        i18n::category(Language::English, "我自己的分类"),
        "我自己的分类"
    );
}

#[test]
fn all_pages_and_management_surfaces_build_in_both_languages_and_themes() {
    for language in [Language::SimplifiedChinese, Language::English] {
        for appearance in [Appearance::Light, Appearance::Dark] {
            let mut app = crate::tests::app();
            app.preferences.language = language;
            app.preferences.appearance = appearance;
            app.ai.categories.selected = Some("portrait".into());
            app.manager.organizer.ai_groups = vec![AiGroup {
                id: "theme_custom".into(),
                label: "自定义舞台".into(),
                query: Query {
                    themes: vec!["theme_custom".into()],
                    ..Query::default()
                },
                count: 0,
                cover: None,
            }];
            for screen in [Screen::Overview, Screen::Assets, Screen::Models] {
                app.screen = screen;
                drop(app.view());
            }
            for panel in [
                manage::Panel::AiCategories,
                manage::Panel::Libraries,
                manage::Panel::Project,
                manage::Panel::Filters,
                manage::Panel::ExportOptions,
                manage::Panel::Tasks,
                manage::Panel::People,
                manage::Panel::Collections,
                manage::Panel::Maintenance,
                manage::Panel::Menu,
            ] {
                app.manager.panel = Some(panel);
                drop(app.view());
            }
            app.viewer.selected = Some(crate::tests::selected_asset());
            app.ai.category_version = app.viewer.selected.as_ref().map(|a| a.version_id);
            app.detail_tab = 2;
            app.manager.details.ai_tags = vec!["较长的标签也应在窄侧栏中自然换行".into()];
            for width in [720.0, 1180.0, 1660.0] {
                app.viewport.width = width;
                app.manager.panel = Some(manage::Panel::AiCategories);
                drop(app.view());
                app.manager.panel = None;
                drop(app.view());
            }
            app.manager.panel = None;
            app.dialog = Some(Dialog::Settings);
            drop(app.view());
            app.dialog = None;
            app.library = None;
            app.library_picker = true;
            drop(app.view());
        }
    }
}
