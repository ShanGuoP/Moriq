//! Page-only transitions. Chrome and layout stay fixed; no screenshot or media allocation.
use super::*;
use std::time::Instant;
pub(super) const FADE_IN: Duration = Duration::from_millis(130);
pub(super) struct Entrance {
    pub started: Instant,
    sampled: Instant,
    offset_from: f32,
    dim_from: f32,
}
impl Entrance {
    fn new(now: Instant, offset_from: f32, dim_from: f32) -> Self {
        Self {
            started: now,
            sampled: now,
            offset_from,
            dim_from,
        }
    }
    fn offset(&self) -> f32 {
        self.offset_from * self.remaining()
    }
    fn dim(&self) -> f32 {
        0.22 + (self.dim_from - 0.22) * self.remaining()
    }
    pub fn remaining(&self) -> f32 {
        let t = (self
            .sampled
            .saturating_duration_since(self.started)
            .as_secs_f32()
            / 0.16)
            .clamp(0.0, 1.0);
        (1.0 - t).powi(3)
    }
}
pub(super) struct PageTransition {
    pub started: Instant,
    pub sampled: Instant,
    from: f32,
    offset_from: f32,
}
impl PageTransition {
    pub fn incoming(now: Instant, from: f32) -> Self {
        Self {
            started: now,
            sampled: now,
            from: from.clamp(0.0, 1.0),
            offset_from: 6.0 * from.clamp(0.0, 1.0),
        }
    }
    pub fn elapsed(&self) -> Duration {
        self.sampled.saturating_duration_since(self.started)
    }
    fn progress(&self) -> f32 {
        let t = (self.elapsed().as_secs_f32() / FADE_IN.as_secs_f32()).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }
    pub fn offset(&self) -> f32 {
        self.offset_from * (1.0 - self.progress())
    }
    pub fn alpha(&self) -> f32 {
        self.from * (1.0 - self.progress())
    }
}
impl App {
    pub(super) fn surface_offset(&self) -> f32 {
        self.surface_motion.as_ref().map_or(0.0, Entrance::offset)
    }
    pub(super) fn backdrop_alpha(&self) -> f32 {
        self.surface_motion.as_ref().map_or(0.22, Entrance::dim)
    }

    fn is_page_navigation(message: &Message) -> bool {
        matches!(
            message,
            Message::Navigate(_)
                | Message::AllAssets
                | Message::Favorites
                | Message::Project(_)
                | Message::List(_)
                | Message::Back
                | Message::Forward
        )
    }
    fn page_changes(&self, message: &Message) -> bool {
        match message {
            Message::Back => {
                !self.surface_open() && !self.viewer.preview_expanded && self.history.can_back()
            }
            Message::Forward => {
                !self.surface_open() && !self.viewer.preview_expanded && self.history.can_forward()
            }
            Message::Navigate(to) => {
                self.library_picker
                    || self.screen
                        != match to {
                            1 => Screen::Assets,
                            2 => Screen::Models,
                            _ => Screen::Overview,
                        }
            }
            Message::AllAssets => self.screen != Screen::Assets || self.query != Query::default(),
            Message::Favorites => self.screen != Screen::Assets || !self.query.favorites_only,
            Message::Project(id) => {
                self.screen != Screen::Assets || self.query.project != Some(*id)
            }
            Message::List(list) => self.screen != Screen::Assets || self.list != *list,
            _ => false,
        }
    }
    pub(super) fn update(&mut self, message: Message) -> Task<Message> {
        if self.defer_favorite_navigation(&message) {
            return Task::none();
        }
        if let Message::Drop(event) = message {
            return self.drop_message(event);
        }
        if let Message::TransitionFrame(now) = message {
            self.gallery_feedback.sample(now);
            if let Some(entrance) = &mut self.surface_motion {
                entrance.sampled = now;
                if entrance.remaining() <= 0.0 {
                    self.surface_motion = None;
                }
            }
            let Some(mut transition) = self.transition.take() else {
                return Task::none();
            };
            transition.sampled = now;
            if transition.elapsed() >= FADE_IN {
                return Task::none();
            }
            self.transition = Some(transition);
            return Task::none();
        }
        if matches!(message, Message::WindowClose) {
            self.transition = None;
            self.surface_motion = None;
        }
        if matches!(message, Message::ShowLibraries) {
            self.transition = None;
        }
        // Commit the destination on the click; only its entrance is animated.
        let page_changed = self.library.is_some()
            && Self::is_page_navigation(&message)
            && self.page_changes(&message);
        let animation = self.transition.as_ref().map(|t| (t.alpha(), t.offset()));
        let before = (self.content_screen(), self.model_tab);
        if Self::is_page_navigation(&message) {
            self.gallery_feedback.clear();
        }
        let surface = self.surface();
        let was_open = self.surface_open();
        let dim = if was_open && !self.compact_panel() {
            self.backdrop_alpha()
        } else {
            0.0
        };
        let offset = if was_open && self.surface_motion.is_none() {
            6.0
        } else {
            self.surface_offset()
        };
        let library = self.library.as_ref().map(|l| l.info.id);
        let task = self.commit_navigation(message);
        if self.preferences.reduced_motion {
            self.transition = None;
            self.surface_motion = None;
        }
        if surface != self.surface() {
            self.gallery_feedback.clear();
            self.surface_motion = (self.surface_open() && !self.preferences.reduced_motion)
                .then(|| Entrance::new(Instant::now(), if was_open { offset } else { 10.0 }, dim));
            if self.surface_open() {
                // A newly opened form owns input immediately; never leave it via an older click.
                self.transition = None;
            }
        }
        let current = self.library.as_ref().map(|l| l.info.id);
        if current != library {
            // Never run a queued action against a different library.
            self.gallery_feedback.clear();
            self.transition = current
                .filter(|_| !self.preferences.reduced_motion)
                .map(|_| PageTransition::incoming(Instant::now(), 0.8));
        } else if !self.preferences.reduced_motion
            && (page_changed || before != (self.content_screen(), self.model_tab))
        {
            let (alpha, offset) = animation.unwrap_or((0.65, 3.9));
            let mut entrance = PageTransition::incoming(Instant::now(), alpha);
            entrance.offset_from = offset;
            self.transition = Some(entrance);
        }
        task
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    fn global_picker_replaces_pending_navigation_and_animation_stops() {
        let mut app = crate::tests::app();
        app.screen = Screen::Assets;
        drop(app.update(Message::Navigate(2)));
        assert_eq!(app.screen, Screen::Models);
        drop(app.update(Message::ShowLibraries));
        assert!(app.library_picker);
        let start = app.transition.as_ref().unwrap().started;
        drop(app.update(Message::TransitionFrame(start + Duration::from_millis(200))));
        assert!(app.library_picker);
        assert_eq!(app.screen, Screen::Overview);
        assert!(app.transition.is_none());
        drop(app.update(Message::Navigate(0)));
        let start = app.transition.as_ref().unwrap().started;
        drop(app.update(Message::TransitionFrame(start + FADE_IN)));
        assert!(!app.library_picker);
    }
    #[test]
    fn opening_form_cancels_old_destination_and_closing_stops_motion() {
        let mut app = crate::tests::app();
        app.screen = Screen::Assets;
        drop(app.update(Message::Navigate(2)));
        drop(app.update(Message::ShowSettings));
        assert!(app.transition.is_none());
        assert_eq!(app.surface_offset(), 10.0);
        let start = app.surface_motion.as_ref().unwrap().started;
        drop(app.update(Message::TransitionFrame(start + Duration::from_millis(80))));
        assert!((app.surface_offset() - 1.25).abs() < 0.01);
        drop(app.update(Message::TransitionFrame(start + Duration::from_millis(160))));
        assert!(app.surface_motion.is_none());
        assert_eq!(app.screen, Screen::Models);
        drop(app.update(Message::HideCreate));
        assert_eq!(app.surface_offset(), 0.0);
        assert!(app.dialog.is_none());
        drop(app.update(Message::ShowSettings));
        drop(app.update(Message::HideCreate));
        assert!(app.surface_motion.is_none());
    }
    #[test]
    fn interrupting_entrance_preserves_current_position_and_dimming() {
        let mut app = crate::tests::app();
        app.screen = Screen::Assets;
        let start = Instant::now();
        let mut entrance = PageTransition::incoming(start, 0.8);
        entrance.sampled = start + Duration::from_millis(35);
        let alpha = entrance.alpha();
        let offset = entrance.offset();
        app.transition = Some(entrance);
        drop(app.update(Message::Navigate(2)));
        let next = app.transition.as_ref().unwrap();
        assert_eq!(next.alpha(), alpha);
        assert_eq!(next.offset(), offset);
        drop(app.update(Message::Navigate(1)));
        let cancelled = app.transition.as_ref().unwrap();
        assert_eq!(app.screen, Screen::Assets);
        assert_eq!(cancelled.alpha(), alpha);
        assert_eq!(cancelled.offset(), offset);
    }
    #[test]
    fn nested_surfaces_keep_backdrop_and_rapid_changes_keep_the_current_offset() {
        let mut app = crate::tests::app();
        app.screen = Screen::Assets;
        drop(app.update(Message::Manage(ManageMessage::Filters)));
        let start = app.surface_motion.as_ref().unwrap().started;
        drop(app.update(Message::TransitionFrame(start + Duration::from_millis(60))));
        let dim = app.backdrop_alpha();
        let offset = app.surface_offset();
        drop(app.update(Message::ShowSettings));
        assert!((app.backdrop_alpha() - dim).abs() < 0.00001);
        assert!((app.surface_offset() - offset).abs() < 0.00001);
        let start = app.surface_motion.as_ref().unwrap().started;
        drop(app.update(Message::TransitionFrame(start + Duration::from_millis(200))));
        assert!((app.backdrop_alpha() - 0.22).abs() < 0.00001);
        drop(app.update(Message::Back));
        assert_eq!(app.manager.panel, Some(manage::Panel::Filters));
        assert!((app.backdrop_alpha() - 0.22).abs() < 0.00001);
        assert_eq!(app.surface_offset(), 6.0);
    }
}
