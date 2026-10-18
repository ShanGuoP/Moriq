//! Page-only transitions. Chrome and layout stay fixed; no screenshot or media allocation.
use super::*;
use std::time::Instant;
pub(super) const FADE_OUT: Duration = Duration::from_millis(70);
pub(super) const FADE_IN: Duration = Duration::from_millis(110);
pub(super) struct PageTransition {
    pub pending: Option<Box<Message>>,
    pub started: Instant,
    pub sampled: Instant,
    from: f32,
}
impl PageTransition {
    pub fn outgoing(message: Message, now: Instant, from: f32) -> Self {
        Self {
            pending: Some(Box::new(message)),
            started: now,
            sampled: now,
            from: from.clamp(0.0, 1.0),
        }
    }
    pub fn incoming(now: Instant, from: f32) -> Self {
        Self {
            pending: None,
            started: now,
            sampled: now,
            from: from.clamp(0.0, 1.0),
        }
    }
    pub fn elapsed(&self) -> Duration {
        self.sampled.saturating_duration_since(self.started)
    }
    pub fn alpha(&self) -> f32 {
        let duration = if self.pending.is_some() {
            FADE_OUT
        } else {
            FADE_IN
        };
        let t = (self.elapsed().as_secs_f32() / duration.as_secs_f32()).clamp(0.0, 1.0);
        let eased = t * t * (3.0 - 2.0 * t);
        if self.pending.is_some() {
            self.from + (1.0 - self.from) * eased
        } else {
            self.from * (1.0 - eased)
        }
    }
}
impl App {
    fn is_page_navigation(message: &Message) -> bool {
        matches!(
            message,
            Message::Navigate(_)
                | Message::AllAssets
                | Message::Favorites
                | Message::Project(_)
                | Message::ModelTab(_)
                | Message::List(_)
                | Message::Back
                | Message::Forward
        )
    }
    fn page_changes(&self, message: &Message) -> bool {
        match message {
            Message::Back => {
                !self.surface_open() && !self.preview_expanded && self.history.can_back()
            }
            Message::Forward => {
                !self.surface_open() && !self.preview_expanded && self.history.can_forward()
            }
            Message::Navigate(to) => {
                self.screen
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
            Message::ModelTab(tab) => self.screen == Screen::Models && self.model_tab != *tab,
            Message::List(list) => self.screen != Screen::Assets || self.list != *list,
            _ => false,
        }
    }
    pub(super) fn update(&mut self, message: Message) -> Task<Message> {
        if let Message::TransitionFrame(now) = message {
            let Some(mut transition) = self.transition.take() else {
                return Task::none();
            };
            transition.sampled = now;
            if transition.pending.is_some() && transition.elapsed() >= FADE_OUT {
                if let Some(pending) = transition.pending.take() {
                    self.transition = Some(PageTransition::incoming(now, 1.0));
                    return self.commit_navigation(*pending);
                }
            } else if transition.pending.is_none() && transition.elapsed() >= FADE_IN {
                return Task::none();
            }
            self.transition = Some(transition);
            return Task::none();
        }
        if matches!(message, Message::WindowClose) {
            self.transition = None;
        }
        if self.library.is_some() && Self::is_page_navigation(&message) {
            if self.page_changes(&message) {
                if let Some(transition) = &mut self.transition
                    && transition.pending.is_some()
                {
                    // Keep opacity continuous, replacing only the destination on rapid navigation.
                    transition.pending = Some(Box::new(message));
                } else {
                    let alpha = self.transition.as_ref().map_or(0.0, PageTransition::alpha);
                    self.transition =
                        Some(PageTransition::outgoing(message, Instant::now(), alpha));
                }
                return Task::none();
            } else if self
                .transition
                .as_ref()
                .is_some_and(|t| t.pending.is_some())
            {
                let alpha = self.transition.as_ref().map_or(0.0, PageTransition::alpha);
                self.transition = Some(PageTransition::incoming(Instant::now(), alpha));
            }
        }
        let before = (self.screen, self.model_tab);
        let library = self.library.as_ref().map(|l| l.info.id);
        let task = self.commit_navigation(message);
        let current = self.library.as_ref().map(|l| l.info.id);
        if current != library {
            // Never run a queued action against a different library.
            self.transition = current.map(|_| PageTransition::incoming(Instant::now(), 0.3));
        } else if self.library.is_some()
            && before != (self.screen, self.model_tab)
            && self.transition.is_none()
        {
            self.transition = Some(PageTransition::incoming(Instant::now(), 0.25));
        }
        task
    }
}
