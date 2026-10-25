//! Overlay return paths are separate from library/query navigation.
use super::*;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Surface {
    pub panel: Option<manage::Panel>,
    pub dialog: Option<Dialog>,
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::manage::Panel;
    #[test]
    fn mov_opens_the_video_player_for_all_extension_cases() {
        for format in ["mov", "MOV", "mOv"] {
            let mut app = crate::tests::app();
            app.screen = Screen::Assets;
            let mut asset = crate::tests::selected_asset();
            asset.format = format.into();
            app.viewer.selected = Some(asset);
            let request = app.viewer.original_request;
            assert!(app.set_viewer(true).units() > 0);
            assert!(app.viewer.preview_expanded);
            assert!(app.viewer.original_request > request);
            assert!(app.viewer.regions.session.is_none());
            drop(app.set_viewer(false));
            assert!(!app.viewer.preview_expanded);
            assert!(app.viewer.playback.is_none());
        }
    }
    #[test]
    fn tag_filter_uses_exact_tag_and_more_returns_to_the_same_query() {
        let mut app = crate::tests::app();
        app.screen = Screen::Assets;
        app.query.text = "原搜索".into();
        app.query.min_rating = 3;
        drop(app.update(Message::TagFilter(TagChoice::Tag("cos".into()))));
        assert_eq!(app.query.tag, "cos");
        assert_eq!(app.query.text, "原搜索");
        assert_eq!(app.query.min_rating, 3);
        let query = app.query.clone();
        drop(app.update(Message::TagFilter(TagChoice::More)));
        assert_eq!(app.manager.panel, Some(Panel::Tags));
        assert_eq!(app.query, query);
        drop(app.update(Message::Back));
        assert!(app.manager.panel.is_none());
        assert_eq!(app.query, query);
        drop(app.update(Message::TagFilter(TagChoice::All)));
        assert!(app.query.tag.is_empty());
        assert_eq!(app.query.text, "原搜索");
        assert_eq!(app.query.min_rating, 3);
    }
    #[test]
    fn nested_panels_and_dialogs_return_to_their_actual_entry_in_order() {
        let mut app = crate::tests::app();
        app.screen = Screen::Assets;
        app.scroll_y = 4800.0;
        app.query.tag = "原查询".into();
        let selected = crate::tests::selected_asset();
        app.viewer.selected = Some(selected.clone());
        drop(app.update(Message::Manage(ManageMessage::AssetMenu)));
        assert!(app.compact_panel());
        drop(app.update(Message::Manage(ManageMessage::ExportMenu)));
        app.surfaces.y = 140.0;
        assert_eq!(app.manager.panel, Some(Panel::ExportOptions));
        drop(app.update(Message::ShowSettings));
        assert_eq!(app.dialog, Some(Dialog::Settings));
        drop(app.update(Message::Back));
        assert!(app.dialog.is_none());
        assert_eq!(app.manager.panel, Some(Panel::ExportOptions));
        assert_eq!(app.surfaces.y, 140.0);
        drop(app.update(Message::Back));
        assert_eq!(app.manager.panel, Some(Panel::AssetMenu));
        drop(app.update(Message::ShowRenameAsset));
        assert_eq!(app.dialog, Some(Dialog::RenameAsset));
        drop(app.update(Message::HideCreate));
        assert_eq!(app.manager.panel, Some(Panel::AssetMenu));
        drop(app.update(Message::Manage(ManageMessage::Close)));
        assert!(app.manager.panel.is_none());
        assert_eq!(app.scroll_y, 4800.0);
        assert_eq!(app.query.tag, "原查询");
        assert_eq!(app.viewer.selected.as_ref().unwrap().id, selected.id);
        assert!(!app.can_go_back());
    }
    #[test]
    fn viewer_back_restores_anchor_and_ignores_remount_zero_scroll_events() {
        let mut app = crate::tests::app();
        app.screen = Screen::Assets;
        app.scroll_y = 5200.0;
        app.query.tag = "收藏标签".into();
        let asset = crate::tests::selected_asset();
        app.viewer.selected = Some(asset.clone());
        app.manager.notes = "尚未保存的备注".into();
        app.manager.notes_dirty = true;
        drop(app.update(Message::ExpandPreview));
        assert!(app.viewer.preview_expanded);
        assert!(app.can_go_back());
        drop(app.update(Message::Scrolled(0.0)));
        assert_eq!(app.scroll_y, 5200.0);
        drop(app.update(Message::Back));
        assert!(!app.viewer.preview_expanded);
        assert_eq!(app.scroll_y, 5200.0);
        assert_eq!(app.pending_scroll, Some(5200.0));
        drop(app.update(Message::Scrolled(0.0)));
        assert_eq!(app.scroll_y, 5200.0);
        app.loading = false;
        drop(app.restore_scroll());
        assert!(app.pending_scroll.is_none());
        drop(app.update(Message::Scrolled(0.0)));
        assert_eq!(app.scroll_y, 5200.0);
        drop(app.update(Message::Scrolled(5200.0)));
        assert!(app.surfaces.restoring_gallery.is_none());
        assert_eq!(app.viewer.selected.as_ref().unwrap().id, asset.id);
        assert_eq!(app.query.tag, "收藏标签");
        assert_eq!(app.manager.notes, "尚未保存的备注");
        assert!(app.manager.notes_dirty);
    }
    #[test]
    fn page_history_restores_models_scroll_and_activity_origin() {
        let mut app = crate::tests::app();
        app.screen = Screen::Models;
        app.surfaces.page_y[2] = 620.0;
        drop(app.update(Message::AllAssets));
        if let Some(t) = &app.transition {
            let now = t.started + Duration::from_millis(70);
            drop(app.update(Message::TransitionFrame(now)));
        }
        drop(app.update(Message::Back));
        if let Some(t) = &app.transition {
            let now = t.started + Duration::from_millis(70);
            drop(app.update(Message::TransitionFrame(now)));
        }
        assert_eq!(app.screen, Screen::Models);
        assert_eq!(app.surfaces.page_y[2], 620.0);
        drop(app.update(Message::PageScrolled(2, 0.0, 1000.0)));
        assert_eq!(app.surfaces.page_y[2], 620.0);
        drop(app.update(Message::PageScrolled(2, 620.0, 1000.0)));
        assert!(app.surfaces.page_restore.is_none());
    }
    #[test]
    fn routed_panels_return_to_tools_and_do_not_change_query_or_selection() {
        for (event, panel) in [
            (ManageMessage::Filters, Panel::Filters),
            (ManageMessage::Organize, Panel::Organize),
            (ManageMessage::Collections, Panel::Collections),
            (ManageMessage::Project, Panel::Project),
            (ManageMessage::Maintenance, Panel::Maintenance),
            (ManageMessage::Synco, Panel::Synco),
            (ManageMessage::Tags, Panel::Tags),
            (ManageMessage::ExportMenu, Panel::ExportOptions),
            (ManageMessage::AssetProjects, Panel::AssetProjects),
            (ManageMessage::Versions, Panel::Versions),
            (ManageMessage::ViewOptions, Panel::ViewOptions),
            (ManageMessage::Help, Panel::Help),
            (ManageMessage::Licenses, Panel::Licenses),
            (ManageMessage::Tasks, Panel::Tasks),
        ] {
            let mut app = crate::tests::app();
            app.screen = Screen::Assets;
            let asset = crate::tests::selected_asset();
            app.viewer.selected = Some(asset.clone());
            app.query.tag = "保留查询".into();
            drop(app.update(Message::Manage(ManageMessage::Menu)));
            drop(app.update(Message::Manage(event)));
            assert_eq!(app.manager.panel, Some(panel));
            drop(app.view());
            drop(app.update(Message::Back));
            assert_eq!(app.manager.panel, Some(Panel::Menu));
            assert_eq!(app.query.tag, "保留查询");
            assert_eq!(app.viewer.selected.as_ref().unwrap().id, asset.id);
        }
    }
    #[test]
    fn form_drafts_are_independent_and_nested_rules_keep_unsaved_filter_values() {
        let mut app = crate::tests::app();
        app.screen = Screen::Assets;
        drop(app.update(Message::Manage(ManageMessage::Label("项目名".into()))));
        drop(app.update(Message::Manage(ManageMessage::CollectionName(
            "集合名".into(),
        ))));
        drop(app.update(Message::Manage(ManageMessage::ActiveTag("原标签".into()))));
        drop(app.update(Message::Manage(ManageMessage::TagName("新标签".into()))));
        assert_eq!(app.manager.label, "项目名");
        assert_eq!(app.manager.collection_name, "集合名");
        assert_eq!(app.manager.tag_new_name, "新标签");
        drop(app.update(Message::Manage(ManageMessage::Filters)));
        app.manager.filters.query.min_rating = 3;
        drop(app.update(Message::Manage(ManageMessage::Date(0, "2026-10-01".into()))));
        drop(app.update(Message::Rules(RuleMessage::Open)));
        drop(app.update(Message::Rules(RuleMessage::Apply)));
        assert_eq!(app.manager.panel, Some(Panel::Filters));
        assert_eq!(app.manager.filters.query.min_rating, 3);
        assert_eq!(app.manager.filters.date_start, "2026-10-01");
        assert!(app.manager.filters.query.rule.is_some());
        assert!(app.query.rule.is_none());
    }
    #[test]
    fn activity_asset_navigation_can_return_to_the_original_activity_panel() {
        let mut app = crate::tests::app();
        app.screen = Screen::Assets;
        app.query.tag = "来源标签".into();
        app.manager.panel = Some(Panel::Activity);
        app.browse_request = 9;
        let id = app.library.as_ref().unwrap().info.id;
        drop(app.update(Message::Journal(JournalMessage::Located(
            id,
            9,
            AssetId::default(),
            Ok(Some(false)),
        ))));
        assert!(app.manager.panel.is_none());
        assert_eq!(app.query, Query::default());
        drop(app.update(Message::Back));
        if let Some(t) = &app.transition {
            let now = t.started + Duration::from_millis(70);
            drop(app.update(Message::TransitionFrame(now)));
        }
        assert_eq!(app.manager.panel, Some(Panel::Activity));
        assert_eq!(app.query.tag, "来源标签");
    }
}
#[derive(Clone, Copy)]
struct ReturnSurface {
    surface: Surface,
    y: f32,
    anchor: iced::Point,
}
#[derive(Default, Clone)]
pub(super) struct Surfaces {
    stack: Vec<ReturnSurface>,
    pub y: f32,
    pub overlay_restore: Option<f32>,
    pub anchor: iced::Point,
    pub cursor: Option<iced::Point>,
    pub viewer_y: Option<f32>,
    pub restoring_gallery: Option<f32>,
    pub page_y: [f32; 3],
    pub page_restore: Option<(u8, f32)>,
}
impl App {
    pub(super) fn surface(&self) -> Surface {
        Surface {
            panel: self.manager.panel,
            dialog: self.dialog,
        }
    }
    pub(super) fn surface_open(&self) -> bool {
        self.dialog.is_some() || self.manager.panel.is_some()
    }
    pub(super) fn surface_has_parent(&self) -> bool {
        !self.surfaces.stack.is_empty()
    }
    pub(super) fn can_go_back(&self) -> bool {
        !self.libraries.busy
            && !(self.manager.panel == Some(manage::Panel::Delete) && self.manager.busy)
            && (self.surface_open() || self.viewer.preview_expanded || self.history.can_back())
    }
    pub(super) fn sync_surface(
        &mut self,
        before: Surface,
        y: f32,
        anchor: iced::Point,
        dismiss: bool,
    ) -> Task<Message> {
        let after = self.surface();
        if after.panel != Some(manage::Panel::People) {
            self.stop_people_portraits();
        }
        if after.panel.is_some() || after.dialog.is_some() {
            self.viewer.playback = None;
            self.viewer.video_frame = None;
            self.viewer.video_upload = false;
            self.viewer.original_request += 1;
        }
        if dismiss {
            self.surfaces.stack.clear();
            self.surfaces.y = 0.0;
            return Task::none();
        }
        if before == after {
            return Task::none();
        }
        if let Some(parent) = self.surfaces.stack.last().copied()
            && (after == parent.surface || (after.panel.is_none() && after.dialog.is_none()))
        {
            self.surfaces.stack.pop();
            self.manager.panel = parent.surface.panel;
            self.dialog = parent.surface.dialog;
            self.surfaces.y = parent.y;
            self.surfaces.overlay_restore = Some(parent.y);
            self.surfaces.anchor = parent.anchor;
            return iced::widget::operation::scroll_to(
                "management",
                scrollable::AbsoluteOffset {
                    x: 0.0,
                    y: parent.y,
                },
            );
        }
        if after.panel.is_some() || after.dialog.is_some() {
            if before.panel.is_some() || before.dialog.is_some() {
                if self.surfaces.stack.len() == 24 {
                    self.surfaces.stack.remove(0);
                }
                self.surfaces.stack.push(ReturnSurface {
                    surface: before,
                    y,
                    anchor,
                });
            } else {
                self.surfaces.stack.clear();
            }
            self.surfaces.y = 0.0;
            self.surfaces.anchor = self.surfaces.cursor.unwrap_or(iced::Point::new(
                (self.viewport.width - 320.0).max(12.0),
                86.0,
            ));
        } else {
            self.surfaces.stack.clear();
        }
        iced::widget::operation::scroll_to(
            "management",
            scrollable::AbsoluteOffset {
                x: 0.0,
                y: self.surfaces.y,
            },
        )
    }
    pub(super) fn restore_page(&mut self) -> Task<Message> {
        let (index, id) = match self.screen {
            Screen::Overview => (0, "overview"),
            Screen::Models => (2, "models"),
            Screen::Assets => return Task::none(),
        };
        let y = self.surfaces.page_y[index];
        self.surfaces.page_restore = Some((index as u8, y));
        iced::widget::operation::scroll_to(id, scrollable::AbsoluteOffset { x: 0.0, y })
    }
    pub(super) fn close_surface(&mut self) -> Task<Message> {
        if self.libraries.busy {
            return Task::none();
        }
        if self.dialog.take().is_some() {
            return Task::none();
        }
        if self.manager.panel == Some(manage::Panel::Delete) {
            return self.deletion_message(DeletionMessage::Previous);
        }
        self.compare.clear();
        self.manager.panel = None;
        Task::none()
    }
    pub(super) fn stop_viewer(&mut self) {
        self.viewer.close();
        if let Some(y) = self.surfaces.viewer_y.take() {
            self.scroll_y = y;
        }
    }
    pub(super) fn set_viewer(&mut self, open: bool) -> Task<Message> {
        if open {
            if !self.viewer.preview_expanded {
                self.surfaces.viewer_y = Some(self.scroll_y);
            }
            self.viewer.preview_expanded = true;
            self.manager.panel = None;
            if let (Some(library), Some(asset)) = (&self.library, &self.viewer.selected) {
                if !moriq_app::is_video_format(&asset.format) {
                    // The fit preview comes first. Prepare original-resolution tiles only
                    // when the viewport is magnified beyond the preview's resolution.
                    return Task::none();
                } else {
                    let (library, id, version) = (library.info.id, asset.id, asset.version_id);
                    self.viewer.stop_media();
                    let request = self.viewer.original_request;
                    let service = self.service.clone();
                    return Task::perform(
                        async move { service.playback(library, id, version).await },
                        move |result| {
                            Message::PlaybackStarted(library, id, version, request, result)
                        },
                    );
                }
            }
        } else if self.viewer.preview_expanded {
            self.viewer.close();
            let mut y = self.surfaces.viewer_y.take().unwrap_or(self.scroll_y);
            let height = self.active_masonry().map_or_else(
                || self.grid().total_height(self.total as usize),
                |m| m.height,
            );
            if height <= self.grid().viewport_height {
                y = 0.0;
            }
            self.scroll_y = y;
            self.pending_scroll = Some(y);
            self.surfaces.restoring_gallery = Some(y);
            // Recreate the Scrollable at the frozen viewport, never request the first page.
            return self.refresh();
        }
        Task::none()
    }
}
