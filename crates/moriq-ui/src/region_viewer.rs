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
    zoom: f32,
    offset: Vector,
    grab: Option<Point>,
    start: Vector,
    last: Option<regions::Viewport>,
}
pub(super) struct Viewer {
    pub zoom_request: u64,
    pub target_scale: Option<f32>,
    pub key: AssetVersionId,
    pub handle: image::Handle,
    pub width: u32,
    pub height: u32,
    pub tiles: Vec<(ImageRegion, image::Handle)>,
}
impl Viewer {
    fn geometry(&self, state: &State, bounds: Rectangle) -> (f32, Point) {
        let fit = (bounds.width / self.width.max(1) as f32)
            .min(bounds.height / self.height.max(1) as f32);
        let scale = (fit * state.zoom.max(0.1)).max(0.000001);
        let origin = Point::new(
            bounds.center_x() - self.width as f32 * scale / 2. + state.offset.x,
            bounds.center_y() - self.height as f32 * scale / 2. + state.offset.y,
        );
        (scale, origin)
    }
    fn clamp(&self, state: &mut State, bounds: Rectangle) {
        let (scale, _) = self.geometry(state, bounds);
        let x = ((self.width as f32 * scale - bounds.width) / 2.).max(0.);
        let y = ((self.height as f32 * scale - bounds.height) / 2.).max(0.);
        state.offset.x = state.offset.x.clamp(-x, x);
        state.offset.y = state.offset.y.clamp(-y, y);
    }
}
impl Widget<Message, Theme, iced::Renderer> for Viewer {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(State {
            zoom: 1.,
            ..Default::default()
        })
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
                zoom: 1.,
                ..Default::default()
            }
        }
        if state.command != self.zoom_request {
            let (scale, _) = self.geometry(state, bounds);
            let fit = scale / state.zoom;
            state.zoom = self
                .target_scale
                .map(|target| target / fit)
                .unwrap_or(1.0)
                .clamp(0.1, (32.0 / fit).max(1.0));
            state.offset = Vector::ZERO;
            state.command = self.zoom_request;
            self.clamp(state, bounds);
            shell.request_redraw();
        }
        match event {
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                if let Some(position) = cursor.position_over(bounds) {
                    let y = match delta {
                        mouse::ScrollDelta::Lines { y, .. }
                        | mouse::ScrollDelta::Pixels { y, .. } => *y,
                    };
                    if y == 0. {
                        return;
                    }
                    let (scale, origin) = self.geometry(state, bounds);
                    let image_x = (position.x - origin.x) / scale;
                    let image_y = (position.y - origin.y) / scale;
                    let fit = scale / state.zoom;
                    state.zoom = (if y > 0. {
                        state.zoom * 1.2
                    } else {
                        state.zoom / 1.2
                    })
                    .clamp(0.1, (32. / fit).max(1.));
                    let new_scale = fit * state.zoom;
                    state.offset = Vector::new(
                        position.x
                            - image_x * new_scale
                            - (bounds.center_x() - self.width as f32 * new_scale / 2.),
                        position.y
                            - image_y * new_scale
                            - (bounds.center_y() - self.height as f32 * new_scale / 2.),
                    );
                    self.clamp(state, bounds);
                    shell.capture_event();
                    shell.request_redraw();
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(position) = cursor.position_over(bounds) {
                    state.grab = Some(position);
                    state.start = state.offset;
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => state.grab = None,
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if let Some(start) = state.grab {
                    state.offset = state.start + (*position - start);
                    self.clamp(state, bounds);
                    shell.capture_event();
                    shell.request_redraw();
                }
            }
            _ => {}
        }
        let (scale, origin) = self.geometry(state, bounds);
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
        let (scale, origin) = self.geometry(state, bounds);
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

#[derive(Default)]
struct NativeState {
    last: Option<(AssetVersionId, bool, Rectangle)>,
}
pub(super) struct NativeSurface {
    pub key: AssetVersionId,
    pub ready: bool,
    pub handle: image::Handle,
}
impl Widget<Message, Theme, iced::Renderer> for NativeSurface {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<NativeState>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(NativeState::default())
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
        _: &Event,
        layout: Layout<'_>,
        _: mouse::Cursor,
        _: &iced::Renderer,
        _: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let bounds = layout
            .bounds()
            .intersection(viewport)
            .unwrap_or(Rectangle::new(Point::ORIGIN, Size::ZERO));
        let value = (self.key, self.ready, bounds);
        let state = tree.state.downcast_mut::<NativeState>();
        if state.last != Some(value) {
            state.last = Some(value);
            shell.publish(Message::VideoBounds(self.key, bounds));
        }
    }
    fn draw(
        &self,
        _: &Tree,
        renderer: &mut iced::Renderer,
        _: &Theme,
        _: &renderer::Style,
        layout: Layout<'_>,
        _: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let Some(size) = renderer.measure_image(&self.handle) else {
            return;
        };
        let scale = (bounds.width / size.width.max(1) as f32)
            .min(bounds.height / size.height.max(1) as f32);
        let width = size.width as f32 * scale;
        let height = size.height as f32 * scale;
        renderer.draw_image(
            iced::advanced::image::Image {
                handle: self.handle.clone(),
                filter_method: Default::default(),
                rotation: Radians(0.),
                border_radius: Default::default(),
                opacity: 1.,
                snap: false,
            },
            Rectangle::new(
                Point::new(
                    bounds.center_x() - width / 2.,
                    bounds.center_y() - height / 2.,
                ),
                Size::new(width, height),
            ),
            *viewport,
        );
    }
}
impl<'a> From<NativeSurface> for Element<'a, Message> {
    fn from(value: NativeSurface) -> Self {
        Element::new(value)
    }
}
