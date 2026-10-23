//! Zoom/pan geometry follows Iced's MIT-licensed image viewer; tiles are app-owned.
use super::*;
use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, renderer,
    widget::tree::{self, Tree},
};
use iced::advanced::{Renderer as _, image::Renderer as _};
use iced::{Event, Length, Point, Radians, Rectangle, Vector};
#[derive(Default)]
struct State {
    command: u64,
    key: Option<AssetVersionId>,
    camera: viewer_motion::Camera,
    grab: Option<Point>,
    start: Vector,
    last: Option<regions::Viewport>,
}
pub(super) struct Viewer {
    pub reduced_motion: bool,
    pub zoom_request: u64,
    pub target_scale: Option<f32>,
    pub key: AssetVersionId,
    pub handle: image::Handle,
    pub width: u32,
    pub height: u32,
    pub tiles: Vec<(ImageRegion, image::Handle)>,
}
impl Widget<Message, Theme, iced::Renderer> for Viewer {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }
    fn layout(
        &mut self,
        _: &mut Tree,
        _: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.resolve(Length::Fill, Length::Fill, Size::ZERO))
    }
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _: &iced::Renderer,
        _: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let state = tree.state.downcast_mut::<State>();
        if state.key != Some(self.key) {
            *state = State {
                key: Some(self.key),
                ..Default::default()
            }
        }
        let now = match event {
            Event::Window(iced::window::Event::RedrawRequested(now)) => *now,
            _ => std::time::Instant::now(),
        };
        if state.camera.configure(self.width, self.height, bounds, now) {
            state.grab = None;
        }
        #[cfg(feature = "capture")]
        let was_active = state.camera.active();
        state.camera.sample(now, self.reduced_motion);
        if state.command != self.zoom_request {
            state
                .camera
                .command(self.target_scale, now, self.reduced_motion);
            state.command = self.zoom_request;
            state.grab = None;
            shell.request_redraw();
        }
        match event {
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                if let Some(position) = cursor.position_over(bounds) {
                    let steps = match delta {
                        mouse::ScrollDelta::Lines { y, .. } => *y,
                        mouse::ScrollDelta::Pixels { y, .. } => *y / 80.0,
                    };
                    if !steps.is_finite() || steps == 0.0 {
                        return;
                    }
                    state
                        .camera
                        .zoom(steps, position, bounds, now, self.reduced_motion);
                    state.grab = None;
                    shell.capture_event();
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(position) = cursor.position_over(bounds) {
                    state.camera.interrupt(now);
                    state.grab = Some(position);
                    state.start = state.camera.pose.offset;
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => state.grab = None,
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if let Some(start) = state.grab {
                    state.camera.pan(state.start + (*position - start));
                    shell.capture_event();
                    shell.request_redraw();
                }
            }
            Event::Window(iced::window::Event::Unfocused) => {
                state.grab = None;
                state.camera.interrupt(now);
            }
            _ => {}
        }
        #[cfg(feature = "capture")]
        if (was_active || state.camera.active())
            && matches!(
                event,
                Event::Window(iced::window::Event::RedrawRequested(_))
            )
        {
            tracing::info!(
                zoom = state.camera.pose.zoom,
                target = state.camera.target.zoom,
                active = state.camera.active(),
                "motion-viewer-frame"
            );
        }
        if state.camera.active() {
            shell.request_redraw_at(now + Duration::from_millis(16));
        }
        // Preload the destination once. Intermediate redraws only move existing GPU images.
        let (scale, origin) = state.camera.geometry(state.camera.target, bounds);
        let x = ((bounds.x - origin.x) / scale)
            .floor()
            .max(0.)
            .min(self.width.saturating_sub(1) as f32) as u32;
        let y = ((bounds.y - origin.y) / scale)
            .floor()
            .max(0.)
            .min(self.height.saturating_sub(1) as f32) as u32;
        let right = ((bounds.x + bounds.width - origin.x) / scale)
            .ceil()
            .clamp(x as f32 + 1., self.width.max(x + 1) as f32) as u32;
        let bottom = ((bounds.y + bounds.height - origin.y) / scale)
            .ceil()
            .clamp(y as f32 + 1., self.height.max(y + 1) as f32) as u32;
        let viewport = regions::Viewport {
            x,
            y,
            width: right - x,
            height: bottom - y,
            scale,
        };
        if state.last != Some(viewport) {
            state.last = Some(viewport);
            #[cfg(feature = "capture")]
            tracing::info!(
                scale = viewport.scale,
                x = viewport.x,
                y = viewport.y,
                width = viewport.width,
                height = viewport.height,
                "motion-viewer-viewport"
            );
            shell.publish(Message::Regions(regions::RegionMessage::Viewport(
                self.key, viewport,
            )));
        }
    }
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _: &Rectangle,
        _: &iced::Renderer,
    ) -> mouse::Interaction {
        if tree.state.downcast_ref::<State>().grab.is_some() {
            mouse::Interaction::Grabbing
        } else if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Grab
        } else {
            mouse::Interaction::None
        }
    }
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        _: &Theme,
        _: &renderer::Style,
        layout: Layout<'_>,
        _: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        let (scale, origin) = state.camera.geometry(state.camera.pose, bounds);
        renderer.with_layer(bounds, |renderer| {
            let mut draw = |handle: image::Handle, rectangle: Rectangle| {
                renderer.draw_image(
                    iced::advanced::image::Image {
                        handle,
                        filter_method: Default::default(),
                        rotation: Radians(0.),
                        border_radius: Default::default(),
                        opacity: 1.,
                        snap: false,
                    },
                    rectangle,
                    *viewport,
                )
            };
            draw(
                self.handle.clone(),
                Rectangle::new(
                    origin,
                    Size::new(self.width as f32 * scale, self.height as f32 * scale),
                ),
            );
            for (region, handle) in &self.tiles {
                draw(
                    handle.clone(),
                    Rectangle::new(
                        Point::new(
                            origin.x + region.x as f32 * scale,
                            origin.y + region.y as f32 * scale,
                        ),
                        Size::new(region.width as f32 * scale, region.height as f32 * scale),
                    ),
                );
            }
        });
    }
}
impl<'a> From<Viewer> for Element<'a, Message> {
    fn from(value: Viewer) -> Self {
        Element::new(value)
    }
}
