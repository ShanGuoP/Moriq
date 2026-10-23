//! Small layout translations preserve child state, hit testing and popup coordinates.
use super::Message;
use iced::advanced::Renderer as _;
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer, widget};
use iced::{Element, Event, Length, Rectangle, Renderer, Size, Theme, Transformation, Vector};
use std::time::{Duration, Instant};

pub(super) fn translated(content: Element<'_, Message>, y: f32) -> Element<'_, Message> {
    Element::new(Translated {
        content,
        y,
        feedback: None,
        owner: None,
    })
}

struct Translated<'a> {
    content: Element<'a, Message>,
    y: f32,
    feedback: Option<(bool, bool)>,
    owner: Option<moriq_domain::AssetId>,
}

/// Draw-only feedback keeps the button's original hit target and tooltip anchor.
pub(super) fn feedback<'a>(
    content: impl Into<Element<'a, Message>>,
    enabled: bool,
    reduced: bool,
) -> Element<'a, Message> {
    Element::new(Translated {
        content: content.into(),
        y: 0.0,
        feedback: Some((enabled, reduced)),
        owner: None,
    })
}

pub(super) fn asset_feedback<'a>(
    content: impl Into<Element<'a, Message>>,
    owner: moriq_domain::AssetId,
    reduced: bool,
) -> Element<'a, Message> {
    Element::new(Translated {
        content: content.into(),
        y: 0.0,
        feedback: Some((true, reduced)),
        owner: Some(owner),
    })
}

impl Widget<Message, Theme, Renderer> for Translated<'_> {
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<Feedback>()
    }
    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(Feedback {
            owner: self.owner,
            ..Default::default()
        })
    }
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }
    fn size_hint(&self) -> Size<Length> {
        self.content.as_widget().size_hint()
    }
    fn children(&self) -> Vec<widget::Tree> {
        vec![widget::Tree::new(&self.content)]
    }
    fn diff(&self, tree: &mut widget::Tree) {
        let state = tree.state.downcast_mut::<Feedback>();
        if state.owner != self.owner {
            // Recycled gallery slots must not inherit another asset's held button.
            *state = Feedback {
                owner: self.owner,
                ..Default::default()
            };
            tree.children = vec![widget::Tree::new(&self.content)];
        } else {
            tree.diff_children(std::slice::from_ref(&self.content));
        }
    }
    fn layout(
        &mut self,
        tree: &mut widget::Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let child = self
            .content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits);
        layout::Node::with_children(
            child.size(),
            vec![child.translate(Vector::new(0.0, self.y))],
        )
    }
    fn draw(
        &self,
        tree: &widget::Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let draw = |renderer: &mut Renderer| {
            self.content.as_widget().draw(
                &tree.children[0],
                renderer,
                theme,
                style,
                layout.children().next().unwrap_or(layout),
                cursor,
                viewport,
            )
        };
        if self
            .feedback
            .is_some_and(|(enabled, reduced)| enabled && !reduced)
        {
            let state = tree.state.downcast_ref::<Feedback>();
            let (scale, lift) = state.transform();
            let center = layout.bounds().center();
            renderer.with_transformation(
                Transformation::translate(center.x, center.y + lift)
                    * Transformation::scale(scale)
                    * Transformation::translate(-center.x, -center.y),
                draw,
            );
        } else {
            draw(renderer);
        }
    }

    fn update(
        &mut self,
        tree: &mut widget::Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        if let Some((enabled, reduced)) = self.feedback {
            let now = match event {
                Event::Window(iced::window::Event::RedrawRequested(now)) => *now,
                _ => Instant::now(),
            };
            let state = tree.state.downcast_mut::<Feedback>();
            let inside = match event {
                Event::Touch(
                    iced::touch::Event::FingerPressed { position, .. }
                    | iced::touch::Event::FingerMoved { position, .. }
                    | iced::touch::Event::FingerLifted { position, .. }
                    | iced::touch::Event::FingerLost { position, .. },
                ) => layout.bounds().contains(*position),
                _ => cursor.is_over(layout.bounds()),
            };
            state.update(event, inside, enabled, reduced, now);
            if state.active() {
                shell.request_redraw_at(now + Duration::from_millis(16));
            }
        }
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout.children().next().unwrap_or(layout),
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }
    fn operate(
        &mut self,
        tree: &mut widget::Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        self.content.as_widget_mut().operate(
            &mut tree.children[0],
            layout.children().next().unwrap_or(layout),
            renderer,
            operation,
        );
    }
    fn mouse_interaction(
        &self,
        tree: &widget::Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout.children().next().unwrap_or(layout),
            cursor,
            viewport,
            renderer,
        )
    }
    fn overlay<'a>(
        &'a mut self,
        tree: &'a mut widget::Tree,
        layout: Layout<'a>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'a, Message, Theme, Renderer>> {
        self.content.as_widget_mut().overlay(
            &mut tree.children[0],
            layout.children().next().unwrap_or(layout),
            renderer,
            viewport,
            translation,
        )
    }
}

/// A short scalar transition can reverse from its sampled value without jumping.
pub(super) struct Value {
    from: f32,
    target: f32,
    started: Instant,
    sampled: Instant,
    duration: Duration,
}
impl Default for Value {
    fn default() -> Self {
        let now = Instant::now();
        Self {
            from: 0.0,
            target: 0.0,
            started: now,
            sampled: now,
            duration: Duration::ZERO,
        }
    }
}
impl Value {
    pub fn get(&self) -> f32 {
        let t = if self.duration.is_zero() {
            1.0
        } else {
            (self
                .sampled
                .saturating_duration_since(self.started)
                .as_secs_f32()
                / self.duration.as_secs_f32())
            .clamp(0.0, 1.0)
        };
        self.from + (self.target - self.from) * (1.0 - (1.0 - t).powi(3))
    }
    pub fn set(&mut self, target: f32, now: Instant, duration: Duration, reduced: bool) {
        self.sample(now);
        if reduced {
            self.from = target;
            self.target = target;
            self.duration = Duration::ZERO;
            return;
        }
        if self.target == target {
            return;
        }
        self.from = self.get();
        self.target = target;
        self.started = now;
        self.duration = duration;
    }
    pub fn sample(&mut self, now: Instant) {
        self.sampled = now;
    }
    pub fn active(&self) -> bool {
        (self.from - self.target).abs() > f32::EPSILON
            && self.sampled.saturating_duration_since(self.started) < self.duration
    }
}
struct Feedback {
    hover: Value,
    press: Value,
    held: bool,
    touch: Option<iced::touch::Finger>,
    focused: bool,
    owner: Option<moriq_domain::AssetId>,
}
impl Default for Feedback {
    fn default() -> Self {
        Self {
            hover: Default::default(),
            press: Default::default(),
            held: false,
            touch: None,
            focused: true,
            owner: None,
        }
    }
}
impl Feedback {
    fn update(&mut self, event: &Event, inside: bool, enabled: bool, reduced: bool, now: Instant) {
        match event {
            Event::Window(iced::window::Event::Focused) => self.focused = true,
            Event::Window(iced::window::Event::Unfocused) => self.focused = false,
            _ => {}
        }
        let mut hovered = (inside || self.touch.is_some()) && enabled && self.focused;
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) if hovered => {
                self.held = true
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => self.held = false,
            Event::Mouse(mouse::Event::CursorLeft)
            | Event::Window(iced::window::Event::Unfocused) => {
                self.held = false;
                self.touch = None;
                hovered = false;
            }
            Event::Touch(iced::touch::Event::FingerPressed { id, .. }) if hovered => {
                self.held = true;
                self.touch = Some(*id);
            }
            Event::Touch(
                iced::touch::Event::FingerLifted { id, .. }
                | iced::touch::Event::FingerLost { id, .. },
            ) if self.touch == Some(*id) => {
                self.held = false;
                self.touch = None;
                hovered = false;
            }
            _ => {}
        }
        if !enabled {
            self.held = false;
            self.touch = None;
        }
        self.hover.set(
            f32::from(hovered),
            now,
            Duration::from_millis(130),
            reduced || !enabled,
        );
        self.press.set(
            f32::from(self.held && hovered),
            now,
            Duration::from_millis(if self.held { 65 } else { 180 }),
            reduced || !enabled,
        );
    }
    fn active(&self) -> bool {
        self.hover.active() || self.press.active()
    }
    fn transform(&self) -> (f32, f32) {
        (
            1.0 - 0.055 * self.press.get(),
            -0.75 * self.hover.get() * (1.0 - self.press.get()),
        )
    }
}

/// Keep only the current and departing card, independent of library size.
#[derive(Default)]
pub(super) struct HoverFeedback {
    current: Option<(moriq_domain::AssetId, Value)>,
    leaving: Option<(moriq_domain::AssetId, Value)>,
}
impl HoverFeedback {
    const DURATION: Duration = Duration::from_millis(120);
    pub fn amount(&self, id: moriq_domain::AssetId) -> f32 {
        self.current
            .iter()
            .chain(self.leaving.iter())
            .find(|(key, _)| *key == id)
            .map_or(0.0, |(_, value)| value.get())
    }
    pub fn set(&mut self, id: moriq_domain::AssetId, inside: bool, now: Instant, reduced: bool) {
        self.sample(now);
        if inside {
            if let Some((key, value)) = &mut self.current
                && *key == id
            {
                value.set(1.0, now, Self::DURATION, reduced);
                if reduced {
                    self.leaving = None;
                }
                return;
            }
            let mut entering = if self.leaving.as_ref().is_some_and(|(key, _)| *key == id) {
                self.leaving
                    .take()
                    .map(|(_, value)| value)
                    .unwrap_or_default()
            } else {
                Value::default()
            };
            if let Some((key, mut value)) = self.current.take() {
                value.set(0.0, now, Self::DURATION, reduced);
                self.leaving = Some((key, value));
            }
            entering.set(1.0, now, Self::DURATION, reduced);
            self.current = Some((id, entering));
        } else if self.current.as_ref().is_some_and(|(key, _)| *key == id)
            && let Some((key, mut value)) = self.current.take()
        {
            value.set(0.0, now, Self::DURATION, reduced);
            self.leaving = Some((key, value));
        }
        if reduced {
            self.leaving = None;
        }
    }
    pub fn sample(&mut self, now: Instant) {
        for (_, value) in self.current.iter_mut().chain(self.leaving.iter_mut()) {
            value.sample(now);
        }
        if self
            .leaving
            .as_ref()
            .is_some_and(|(_, value)| !value.active())
        {
            self.leaving = None;
        }
    }
    pub fn active(&self) -> bool {
        self.current
            .iter()
            .chain(self.leaving.iter())
            .any(|(_, value)| value.active())
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use moriq_domain::AssetId;
    use std::time::{Duration, Instant};
    #[test]
    fn hover_reverses_continuously_ignores_stale_exit_and_stops_at_rest() {
        let mut motion = HoverFeedback::default();
        let id = AssetId::default();
        let other = AssetId::default();
        let start = Instant::now();
        motion.set(id, true, start, false);
        assert!(motion.active());
        motion.sample(start + Duration::from_millis(60));
        let before = motion.amount(id);
        assert!(before > 0.8 && before < 1.0);
        motion.set(id, false, start + Duration::from_millis(60), false);
        assert_eq!(motion.amount(id), before);
        motion.sample(start + Duration::from_millis(180));
        assert!(!motion.active());
        assert_eq!(motion.amount(id), 0.0);
        motion.set(other, true, start + Duration::from_millis(200), false);
        motion.set(id, false, start + Duration::from_millis(210), false);
        motion.sample(start + Duration::from_millis(340));
        assert_eq!(motion.amount(other), 1.0);
        assert!(!motion.active());
    }
    #[test]
    fn reduced_motion_keeps_feedback_without_a_frame_subscription() {
        let id = AssetId::default();
        let mut motion = HoverFeedback::default();
        motion.set(id, true, Instant::now(), true);
        assert_eq!(motion.amount(id), 1.0);
        assert!(!motion.active());
        motion.set(id, false, Instant::now(), true);
        assert_eq!(motion.amount(id), 0.0);
        assert!(!motion.active());
    }
    #[test]
    fn crossing_cards_and_reentering_preserve_both_visible_values() {
        let now = Instant::now();
        let a = AssetId::default();
        let b = AssetId::default();
        let mut feedback = HoverFeedback::default();
        feedback.set(a, true, now, false);
        feedback.sample(now + Duration::from_millis(50));
        let a_before = feedback.amount(a);
        feedback.set(b, true, now + Duration::from_millis(50), false);
        assert_eq!(feedback.amount(a), a_before);
        assert_eq!(feedback.amount(b), 0.0);
        feedback.sample(now + Duration::from_millis(90));
        let a_before = feedback.amount(a);
        let b_before = feedback.amount(b);
        feedback.set(a, true, now + Duration::from_millis(90), false);
        assert_eq!(feedback.amount(a), a_before);
        assert_eq!(feedback.amount(b), b_before);
        feedback.set(b, false, now + Duration::from_millis(90), false);
        assert_eq!(feedback.amount(a), a_before);
        feedback.sample(now + Duration::from_millis(250));
        assert_eq!(feedback.amount(a), 1.0);
        assert_eq!(feedback.amount(b), 0.0);
        assert!(!feedback.active());
        assert!(feedback.leaving.is_none());
    }
    #[test]
    fn press_release_reverses_continuously_and_focus_loss_does_not_leave_a_stuck_press() {
        let now = Instant::now();
        let press = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        let release = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
        let mut feedback = Feedback::default();
        feedback.update(&press, true, true, false, now);
        feedback.update(
            &Event::Window(iced::window::Event::RedrawRequested(
                now + Duration::from_millis(35),
            )),
            true,
            true,
            false,
            now + Duration::from_millis(35),
        );
        let transform = feedback.transform();
        assert!(transform.0 < 1.0 && transform.0 > 0.94);
        feedback.update(&release, true, true, false, now + Duration::from_millis(35));
        assert_eq!(feedback.transform(), transform);
        feedback.update(
            &Event::Window(iced::window::Event::Unfocused),
            true,
            true,
            false,
            now + Duration::from_millis(40),
        );
        feedback.update(
            &Event::Window(iced::window::Event::RedrawRequested(
                now + Duration::from_millis(300),
            )),
            true,
            true,
            false,
            now + Duration::from_millis(300),
        );
        assert!(!feedback.held);
        assert_eq!(feedback.transform(), (1.0, 0.0));
        assert!(!feedback.active());
        feedback.update(
            &Event::Window(iced::window::Event::Focused),
            true,
            true,
            true,
            now + Duration::from_millis(305),
        );
        feedback.update(&press, true, true, true, now + Duration::from_millis(310));
        assert!(!feedback.active());
        feedback.update(
            &release,
            true,
            false,
            false,
            now + Duration::from_millis(320),
        );
        assert_eq!(feedback.transform(), (1.0, 0.0));
        assert!(!feedback.active());
    }
    #[test]
    fn recycling_an_asset_button_clears_held_gestures_but_stable_identity_keeps_feedback() {
        let first_id = AssetId::default();
        let second_id = AssetId::default();
        let button = |id| {
            asset_feedback(
                iced::widget::button(iced::widget::Space::new()).on_press(Message::Noop),
                id,
                false,
            )
        };
        let first = button(first_id);
        let mut tree = widget::Tree::new(&first);
        tree.state.downcast_mut::<Feedback>().held = true;
        first.as_widget().diff(&mut tree);
        assert!(tree.state.downcast_ref::<Feedback>().held);
        let second = button(second_id);
        second.as_widget().diff(&mut tree);
        let state = tree.state.downcast_ref::<Feedback>();
        assert!(!state.held);
        assert_eq!(state.owner, Some(second_id));
        assert_eq!(state.transform(), (1.0, 0.0));
        assert!(!state.active());
    }
}
