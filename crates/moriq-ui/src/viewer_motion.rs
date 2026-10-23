//! A bounded camera transition; decoder requests follow its destination, not every frame.
use iced::{Point, Rectangle, Size, Vector};
use std::time::{Duration, Instant};
const DURATION: Duration = Duration::from_millis(150);
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Pose {
    pub zoom: f32,
    pub offset: Vector,
}
impl Default for Pose {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            offset: Vector::ZERO,
        }
    }
}
#[derive(Clone, Copy)]
struct Flight {
    from: Pose,
    started: Instant,
}
#[derive(Default)]
pub(super) struct Camera {
    pub pose: Pose,
    pub target: Pose,
    flight: Option<Flight>,
    dimensions: Size,
    viewport: Size,
}
impl Camera {
    pub fn configure(&mut self, width: u32, height: u32, bounds: Rectangle, now: Instant) -> bool {
        let dimensions = Size::new(width.max(1) as f32, height.max(1) as f32);
        let viewport = bounds.size();
        if self.dimensions == dimensions && self.viewport == viewport {
            return false;
        }
        self.sample(now, false);
        self.dimensions = dimensions;
        self.viewport = viewport;
        self.pose = self.clamped(self.pose);
        self.target = self.pose;
        self.flight = None;
        true
    }
    fn fit(&self) -> f32 {
        (self.viewport.width / self.dimensions.width.max(1.0))
            .min(self.viewport.height / self.dimensions.height.max(1.0))
            .max(0.000001)
    }
    pub fn geometry(&self, pose: Pose, bounds: Rectangle) -> (f32, Point) {
        let scale = self.fit() * pose.zoom;
        (
            scale,
            Point::new(
                bounds.center_x() - self.dimensions.width * scale / 2.0 + pose.offset.x,
                bounds.center_y() - self.dimensions.height * scale / 2.0 + pose.offset.y,
            ),
        )
    }
    fn clamped(&self, mut pose: Pose) -> Pose {
        pose.zoom = pose.zoom.clamp(0.1, (32.0 / self.fit()).max(1.0));
        let scale = self.fit() * pose.zoom;
        let x = ((self.dimensions.width * scale - self.viewport.width) / 2.0).max(0.0);
        let y = ((self.dimensions.height * scale - self.viewport.height) / 2.0).max(0.0);
        pose.offset.x = pose.offset.x.clamp(-x, x);
        pose.offset.y = pose.offset.y.clamp(-y, y);
        pose
    }
    fn move_to(&mut self, pose: Pose, now: Instant, reduced: bool) {
        self.sample(now, reduced);
        let pose = self.clamped(pose);
        if !reduced && self.target == pose && self.flight.is_some() {
            return;
        }
        self.target = pose;
        if reduced || self.pose == pose {
            self.pose = pose;
            self.flight = None;
        } else {
            self.flight = Some(Flight {
                from: self.pose,
                started: now,
            });
        }
    }
    pub fn sample(&mut self, now: Instant, reduced: bool) {
        if let Some(flight) = self.flight {
            let t = if reduced {
                1.0
            } else {
                (now.saturating_duration_since(flight.started).as_secs_f32()
                    / DURATION.as_secs_f32())
                .clamp(0.0, 1.0)
            };
            let amount = 1.0 - (1.0 - t).powi(3);
            self.pose = self.clamped(Pose {
                zoom: flight.from.zoom + (self.target.zoom - flight.from.zoom) * amount,
                offset: flight.from.offset + (self.target.offset - flight.from.offset) * amount,
            });
            if t >= 1.0 {
                self.pose = self.target;
                self.flight = None;
            }
        }
    }
    pub fn zoom(
        &mut self,
        steps: f32,
        position: Point,
        bounds: Rectangle,
        now: Instant,
        reduced: bool,
    ) {
        if !steps.is_finite() || steps == 0.0 {
            return;
        }
        self.sample(now, reduced);
        let (scale, origin) = self.geometry(self.pose, bounds);
        let image = (position - origin) * (1.0 / scale);
        let zoom = (self.target.zoom * 1.2_f32.powf(steps.clamp(-4.0, 4.0)))
            .clamp(0.1, (32.0 / self.fit()).max(1.0));
        let scale = self.fit() * zoom;
        let offset = Vector::new(
            position.x
                - image.x * scale
                - (bounds.center_x() - self.dimensions.width * scale / 2.0),
            position.y
                - image.y * scale
                - (bounds.center_y() - self.dimensions.height * scale / 2.0),
        );
        self.move_to(Pose { zoom, offset }, now, reduced);
    }
    pub fn command(&mut self, scale: Option<f32>, now: Instant, reduced: bool) {
        let zoom = scale
            .filter(|v| v.is_finite() && *v > 0.0)
            .map_or(1.0, |scale| scale / self.fit());
        self.move_to(
            Pose {
                zoom,
                offset: Vector::ZERO,
            },
            now,
            reduced,
        );
    }
    pub fn interrupt(&mut self, now: Instant) {
        self.sample(now, false);
        self.target = self.pose;
        self.flight = None;
    }
    pub fn pan(&mut self, offset: Vector) {
        self.pose = self.clamped(Pose {
            offset,
            ..self.pose
        });
        self.target = self.pose;
        self.flight = None;
    }
    pub fn active(&self) -> bool {
        self.flight.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn camera() -> (Camera, Rectangle, Instant) {
        let now = Instant::now();
        let bounds = Rectangle::with_size(Size::new(800.0, 600.0));
        let mut camera = Camera::default();
        camera.configure(4000, 3000, bounds, now);
        (camera, bounds, now)
    }
    #[test]
    fn wheel_keeps_pointer_anchor_and_retargets_without_jumps_or_lost_steps() {
        let (mut camera, bounds, now) = camera();
        let pointer = Point::new(520.0, 360.0);
        let (scale, origin) = camera.geometry(camera.pose, bounds);
        let anchor = (pointer - origin) * (1.0 / scale);
        camera.zoom(1.0, pointer, bounds, now, false);
        assert_eq!(camera.pose.zoom, 1.0);
        camera.sample(now + Duration::from_millis(60), false);
        let before = camera.pose;
        let (scale, origin) = camera.geometry(camera.pose, bounds);
        let projected = Point::new(origin.x + anchor.x * scale, origin.y + anchor.y * scale);
        assert!((projected.x - pointer.x).abs() < 0.001 && (projected.y - pointer.y).abs() < 0.001);
        camera.zoom(1.0, pointer, bounds, now + Duration::from_millis(60), false);
        assert_eq!(camera.pose, before);
        assert!((camera.target.zoom - 1.44).abs() < 0.0001);
        camera.sample(now + Duration::from_millis(210), false);
        assert!(!camera.active());
        assert_eq!(camera.pose, camera.target);
    }
    #[test]
    fn drag_interrupts_at_visible_position_and_fit_reduced_motion_and_resize_settle() {
        let (mut camera, bounds, now) = camera();
        camera.command(Some(1.0), now, false);
        camera.sample(now + Duration::from_millis(60), false);
        let before = camera.pose;
        camera.interrupt(now + Duration::from_millis(60));
        assert_eq!(camera.pose, before);
        assert!(!camera.active());
        camera.pan(Vector::new(99999.0, -99999.0));
        let (_, origin) = camera.geometry(camera.pose, bounds);
        assert!(
            origin.x <= bounds.x + 0.01
                && origin.y + 3000.0 * camera.fit() * camera.pose.zoom
                    >= bounds.y + bounds.height - 0.01
        );
        camera.command(None, now, true);
        assert_eq!(camera.pose, Pose::default());
        assert!(!camera.active());
        camera.command(Some(1.0), now, false);
        camera.sample(now + Duration::from_millis(20), false);
        camera.sample(now + Duration::from_millis(20), true);
        assert_eq!(camera.pose.zoom, 5.0);
        assert!(!camera.active());
        camera.zoom(-1.0, bounds.center(), bounds, now, false);
        camera.configure(
            4000,
            3000,
            Rectangle::with_size(Size::new(900.0, 600.0)),
            now + Duration::from_millis(20),
        );
        assert!(!camera.active());
        assert_eq!(camera.pose, camera.target);
    }
    #[test]
    fn pixel_steps_are_small_and_invalid_input_cannot_poison_geometry() {
        let (mut camera, bounds, now) = camera();
        camera.zoom(1.0 / 80.0, bounds.center(), bounds, now, true);
        assert!(camera.pose.zoom > 1.0 && camera.pose.zoom < 1.01);
        let pose = camera.pose;
        camera.zoom(f32::NAN, bounds.center(), bounds, now, false);
        assert_eq!(camera.pose, pose);
        assert!(!camera.active());
    }
    #[test]
    fn zooming_back_from_a_panned_image_never_drifts_outside_its_edges() {
        let (mut camera, bounds, now) = camera();
        camera.command(Some(1.0), now, true);
        camera.pan(Vector::new(1500.0, 1000.0));
        camera.command(None, now, false);
        for frame in 1..=10 {
            camera.sample(now + Duration::from_millis(frame * 15), false);
            let (scale, origin) = camera.geometry(camera.pose, bounds);
            assert!(origin.x <= bounds.x + 0.01);
            assert!(origin.y <= bounds.y + 0.01);
            assert!(origin.x + 4000.0 * scale >= bounds.x + bounds.width - 0.01);
            assert!(origin.y + 3000.0 * scale >= bounds.y + bounds.height - 0.01);
        }
        assert_eq!(camera.pose, Pose::default());
        assert!(!camera.active());
    }
}
