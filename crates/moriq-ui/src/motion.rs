//! Small layout translations preserve child state, hit testing and popup coordinates.
use super::Message;
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, overlay, renderer, widget};
use iced::{Element, Event, Length, Rectangle, Renderer, Size, Theme, Vector};

pub(super) fn translated(content: Element<'_, Message>, y: f32) -> Element<'_, Message> {
    Element::new(Translated { content, y })
}

struct Translated<'a> {
    content: Element<'a, Message>,
    y: f32,
}

impl Widget<Message, Theme, Renderer> for Translated<'_> {
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
        tree.diff_children(std::slice::from_ref(&self.content));
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
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout.children().next().unwrap_or(layout),
            cursor,
            viewport,
        );
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

/// One transient card owns hover feedback; no animation state is kept per asset.
pub(super) struct HoverFeedback {
    id: Option<moriq_domain::AssetId>,
    from: f32,
    target: f32,
    started: std::time::Instant,
    sampled: std::time::Instant,
}
impl Default for HoverFeedback {
    fn default() -> Self {
        let now = std::time::Instant::now();
        Self {
            id: None,
            from: 0.0,
            target: 0.0,
            started: now,
            sampled: now,
        }
    }
}
impl HoverFeedback {
    const DURATION: std::time::Duration = std::time::Duration::from_millis(120);
    fn value(&self) -> f32 {
        let t = (self
            .sampled
            .saturating_duration_since(self.started)
            .as_secs_f32()
            / Self::DURATION.as_secs_f32())
        .clamp(0.0, 1.0);
        let ease = 1.0 - (1.0 - t).powi(3);
        self.from + (self.target - self.from) * ease
    }
    pub fn amount(&self, id: moriq_domain::AssetId) -> f32 {
        if self.id == Some(id) {
            self.value()
        } else {
            0.0
        }
    }
    pub fn set(
        &mut self,
        id: moriq_domain::AssetId,
        inside: bool,
        now: std::time::Instant,
        reduced: bool,
    ) {
        if !inside && self.id != Some(id) {
            return;
        }
        self.sampled = now;
        let from = self.amount(id);
        self.id = Some(id);
        self.target = if inside { 1.0 } else { 0.0 };
        self.from = if reduced { self.target } else { from };
        self.started = now;
        if reduced {
            self.sampled += Self::DURATION;
        }
    }
    pub fn sample(&mut self, now: std::time::Instant) {
        self.sampled = now;
        if !self.active() && self.target == 0.0 {
            self.id = None;
        }
    }
    pub fn active(&self) -> bool {
        self.id.is_some()
            && (self.from - self.target).abs() > f32::EPSILON
            && self.sampled.saturating_duration_since(self.started) < Self::DURATION
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
}
