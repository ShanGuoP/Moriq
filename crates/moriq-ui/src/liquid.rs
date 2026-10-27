//! A shared selection moves as one viscous shape; labels and hit targets stay fixed.
use crate::{Message, theme};
use iced::advanced::{
    Clipboard, Layout, Renderer as _, Shell, Widget, layout, mouse, overlay, renderer,
    widget::{self, tree},
};
use iced::{Border, Element, Event, Length, Rectangle, Renderer, Size, Theme, Vector, window};
use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
pub(super) enum Treatment {
    Selection,
    Underline,
    Thumb,
}

/// 为内容包装连续形变的选中指示，沿用减少动态效果偏好。
pub(super) fn selection<'a>(
    content: impl Into<Element<'a, Message>>,
    selected: Option<usize>,
    treatment: Treatment,
    reduced_motion: bool,
) -> Element<'a, Message> {
    Element::new(Liquid {
        content: content.into(),
        selected,
        treatment,
        reduced_motion,
    })
}

struct Liquid<'a> {
    content: Element<'a, Message>,
    selected: Option<usize>,
    treatment: Treatment,
    reduced_motion: bool,
}

const SETTLE: Duration = Duration::from_millis(420);

#[derive(Clone, Copy)]
struct Morph {
    from: Rectangle,
    target: Rectangle,
    started: Instant,
    sampled: Instant,
}
impl Morph {
    /// 判断选中指示是否仍在从当前形状过渡到目标形状。
    fn active(&self) -> bool {
        self.from != self.target && self.sampled.saturating_duration_since(self.started) < SETTLE
    }
    /// 按采样时间插值选中区域边界，生成带先后边缘与回弹的形变。
    fn bounds(&self) -> Rectangle {
        if !self.active() {
            return self.target;
        }
        let t = (self
            .sampled
            .saturating_duration_since(self.started)
            .as_secs_f32()
            / SETTLE.as_secs_f32())
        .clamp(0.0, 1.0);
        // The leading edge starts first; the trailing edge catches up with a small recoil.
        let head = 1.0 - (1.0 - t).powi(4);
        let tail_t = ((t - 0.08) / 0.92).clamp(0.0, 1.0);
        let tail = 1.0 - (1.0 - tail_t).powi(3);
        let recoil = (t * std::f32::consts::PI * 2.0).sin() * (1.0 - t).powi(2) * 0.08;
        let edge = |from: f32, to: f32, forward: bool| {
            from + (to - from) * (if forward { head } else { tail } + recoil)
        };
        let rightward = self.target.center_x() >= self.from.center_x();
        let downward = self.target.center_y() >= self.from.center_y();
        let left = edge(self.from.x, self.target.x, !rightward);
        let right = edge(
            self.from.x + self.from.width,
            self.target.x + self.target.width,
            rightward,
        );
        let top = edge(self.from.y, self.target.y, !downward);
        let bottom = edge(
            self.from.y + self.from.height,
            self.target.y + self.target.height,
            downward,
        );
        let mut bounds = Rectangle {
            x: left,
            y: top,
            width: (right - left).max(1.0),
            height: (bottom - top).max(1.0),
        };
        let squeeze = (t * std::f32::consts::PI).sin();
        if (self.target.center_x() - self.from.center_x()).abs()
            >= (self.target.center_y() - self.from.center_y()).abs()
        {
            let loss = bounds.height * 0.16 * squeeze;
            bounds.y += loss / 2.0;
            bounds.height -= loss;
        } else {
            let loss = bounds.width * 0.035 * squeeze;
            bounds.x += loss / 2.0;
            bounds.width -= loss;
        }
        bounds
    }
}

#[derive(Default)]
struct State {
    selected: Option<usize>,
    morph: Option<Morph>,
    layout_size: Size,
}
impl State {
    /// 从当前可见边界重新设定目标，避免快速切换选项产生跳变。
    fn retarget(
        &mut self,
        selected: Option<usize>,
        bounds: Option<Rectangle>,
        size: Size,
        reduced: bool,
        now: Instant,
    ) {
        let Some(target) = bounds else {
            self.selected = selected;
            self.morph = None;
            self.layout_size = size;
            return;
        };
        let reset = reduced || self.layout_size != size || self.morph.is_none();
        let changed = self.selected != selected;
        if let Some(morph) = &mut self.morph {
            morph.sampled = now;
        }
        if reset || changed {
            let from = if reset {
                target
            } else {
                self.morph.map_or(target, |morph| morph.bounds())
            };
            self.morph = Some(Morph {
                from,
                target,
                started: now,
                sampled: now,
            });
        } else if let Some(morph) = &mut self.morph
            && morph.target != target
        {
            // Resize, language, wrapping and scroll changes are layout changes, not clicks.
            *morph = Morph {
                from: target,
                target,
                started: now,
                sampled: now,
            };
        }
        self.selected = selected;
        self.layout_size = size;
    }
}

impl Liquid<'_> {
    /// 借用包装控件的子控件，统一转发操作。
    fn child<'a>(layout: Layout<'a>) -> Layout<'a> {
        layout.children().next().unwrap_or(layout)
    }
    /// 根据选中项和实际布局查找指示区域。
    fn target(&self, layout: Layout<'_>) -> Option<Rectangle> {
        let index = self.selected?;
        let origin = layout.bounds().position();
        let mut bounds = Self::child(layout).children().nth(index)?.bounds();
        bounds.x -= origin.x;
        bounds.y -= origin.y;
        if matches!(self.treatment, Treatment::Underline) {
            bounds.y += bounds.height - 2.5;
            bounds.height = 2.5;
        }
        Some(bounds)
    }
    /// 按选中、下划线或滑块样式绘制当前形变区域。
    fn paint(&self, renderer: &mut Renderer, theme: &Theme, bounds: Rectangle) {
        let (color, radius) = match self.treatment {
            Treatment::Selection => (
                theme::adaptive(theme, theme::PALE),
                bounds.height.min(bounds.width) / 2.0,
            ),
            Treatment::Underline => (theme::adaptive(theme, theme::BLUE), 2.0),
            Treatment::Thumb => (iced::Color::WHITE, bounds.height / 2.0),
        };
        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border: Border {
                    radius: radius.into(),
                    ..Default::default()
                },
                snap: false,
                ..Default::default()
            },
            color,
        );
    }
}

impl Widget<Message, Theme, Renderer> for Liquid<'_> {
    /// 返回控件树状态类型标识，支持重建时复用已有状态。
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }
    /// 建立选中指示控件的初始持久状态。
    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }
    /// 沿用子控件声明的尺寸约束。
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }
    /// 转发子控件的建议尺寸。
    fn size_hint(&self) -> Size<Length> {
        self.content.as_widget().size_hint()
    }
    /// 为包装控件建立子控件树。
    fn children(&self) -> Vec<widget::Tree> {
        vec![widget::Tree::new(&self.content)]
    }
    /// 对齐新旧子控件树，保留有效交互状态。
    fn diff(&self, tree: &mut widget::Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }
    /// 布局子内容并依据实际位置更新选中指示目标。
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
        layout::Node::with_children(child.size(), vec![child])
    }
    /// 绘制形变指示和子内容，沿用当前裁剪范围。
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
        let state = tree.state.downcast_ref::<State>();
        let bounds = if self.reduced_motion {
            self.target(layout)
        } else {
            state
                .morph
                .map(|morph| morph.bounds())
                .or_else(|| self.target(layout))
        };
        if let Some(clip) = layout.bounds().intersection(viewport) {
            let origin = layout.bounds().position();
            renderer.with_layer(clip, |renderer| {
                if let Some(bounds) = bounds {
                    self.paint(
                        renderer,
                        theme,
                        Rectangle {
                            x: bounds.x + origin.x,
                            y: bounds.y + origin.y,
                            ..bounds
                        },
                    );
                }
                // Keep both in one layer: separate GPU layers can paint the blob over text.
                self.content.as_widget().draw(
                    &tree.children[0],
                    renderer,
                    theme,
                    style,
                    Self::child(layout),
                    cursor,
                    viewport,
                );
            });
        }
    }
    /// 转发输入事件并推进必要动画，只在未稳定时请求重绘。
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
        let now = match event {
            Event::Window(window::Event::RedrawRequested(now)) => *now,
            _ => Instant::now(),
        };
        let target = self.target(layout);
        let state = tree.state.downcast_mut::<State>();
        #[cfg(feature = "capture")]
        let was_active = state.morph.is_some_and(|morph| morph.active());
        state.retarget(
            self.selected,
            target,
            layout.bounds().size(),
            self.reduced_motion,
            now,
        );
        #[cfg(feature = "capture")]
        if matches!(event, Event::Window(window::Event::RedrawRequested(_)))
            && let Some(morph) = state.morph
            && (was_active || morph.active())
        {
            let bounds = morph.bounds();
            tracing::info!(selected = ?self.selected, x = bounds.x, y = bounds.y,
                width = bounds.width, height = bounds.height, active = morph.active(),
                elapsed_ms = now.saturating_duration_since(morph.started).as_millis(),
                "liquid-selection-frame");
        }
        if state.morph.is_some_and(|morph| morph.active()) {
            shell.request_redraw_at(now + Duration::from_millis(16));
        }
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            Self::child(layout),
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }
    /// 把焦点、滚动等树操作转发给子控件。
    fn operate(
        &mut self,
        tree: &mut widget::Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn widget::Operation,
    ) {
        self.content.as_widget_mut().operate(
            &mut tree.children[0],
            Self::child(layout),
            renderer,
            operation,
        );
    }
    /// 返回子内容对应的鼠标交互状态。
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
            Self::child(layout),
            cursor,
            viewport,
            renderer,
        )
    }
    /// 转发子控件浮层，并保持其布局坐标一致。
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
            Self::child(layout),
            renderer,
            viewport,
            translation,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 构造选中形变测试所需的固定布局槽位。
    fn slot(x: f32) -> Rectangle {
        Rectangle {
            x,
            y: 0.0,
            width: 60.0,
            height: 32.0,
        }
    }

    #[test]
    /// 验证首次挂载与减少动态效果模式不会启动选中动画。
    fn first_mount_and_reduced_motion_never_start_an_animation() {
        let now = Instant::now();
        let size = Size::new(200.0, 32.0);
        let mut state = State::default();
        state.retarget(Some(0), Some(slot(0.0)), size, false, now);
        assert!(!state.morph.is_some_and(|morph| morph.active()));
        state.retarget(Some(1), Some(slot(80.0)), size, true, now);
        assert_eq!(state.morph.map(|morph| morph.bounds()), Some(slot(80.0)));
        assert!(!state.morph.is_some_and(|morph| morph.active()));
    }

    #[test]
    /// 验证选中切换会伸缩并最终稳定，静止后不继续请求帧。
    fn moving_selection_stretches_squeezes_and_settles_without_idle_frames() {
        let now = Instant::now();
        let morph = Morph {
            from: slot(0.0),
            target: slot(80.0),
            started: now,
            sampled: now + Duration::from_millis(90),
        };
        let middle = morph.bounds();
        assert!(middle.width > morph.target.width);
        assert!(middle.height < morph.target.height);
        assert!(middle.x > morph.from.x && middle.x < morph.target.x);
        assert!(morph.active());
        let settled = Morph {
            sampled: now + SETTLE,
            ..morph
        };
        assert_eq!(settled.bounds(), morph.target);
        assert!(!settled.active());
    }

    #[test]
    /// 验证反向选择从当前可见形状改向，不发生位置跳变。
    fn reversing_retargets_the_current_shape_without_jumping() {
        let now = Instant::now();
        let size = Size::new(200.0, 32.0);
        let mut state = State::default();
        state.retarget(Some(0), Some(slot(0.0)), size, false, now);
        state.retarget(Some(1), Some(slot(80.0)), size, false, now);
        let reverse_at = now + Duration::from_millis(90);
        let before = state.morph.map(|morph| {
            Morph {
                sampled: reverse_at,
                ..morph
            }
            .bounds()
        });
        state.retarget(Some(0), Some(slot(0.0)), size, false, reverse_at);
        assert_eq!(state.morph.map(|morph| morph.bounds()), before);
        state.retarget(Some(0), Some(slot(0.0)), size, false, reverse_at + SETTLE);
        assert_eq!(state.morph.map(|morph| morph.bounds()), Some(slot(0.0)));
        assert!(!state.morph.is_some_and(|morph| morph.active()));
    }

    #[test]
    /// 验证窗口尺寸变化或选中项消失时清理旧几何状态。
    fn resize_and_missing_selection_drop_stale_geometry() {
        let now = Instant::now();
        let mut state = State::default();
        state.retarget(Some(0), Some(slot(0.0)), Size::new(200.0, 32.0), false, now);
        state.retarget(
            Some(1),
            Some(slot(80.0)),
            Size::new(200.0, 32.0),
            false,
            now,
        );
        let resized = Rectangle {
            y: 40.0,
            ..slot(0.0)
        };
        state.retarget(Some(1), Some(resized), Size::new(80.0, 72.0), false, now);
        assert_eq!(state.morph.map(|morph| morph.bounds()), Some(resized));
        assert!(!state.morph.is_some_and(|morph| morph.active()));
        state.retarget(None, None, Size::new(80.0, 72.0), false, now);
        assert!(state.morph.is_none());
    }

    #[test]
    /// 验证开启减少动态效果会结束正在进行的选中形变。
    fn enabling_reduced_motion_stops_an_in_flight_selection() {
        let now = Instant::now();
        let size = Size::new(200.0, 32.0);
        let mut state = State::default();
        state.retarget(Some(0), Some(slot(0.0)), size, false, now);
        state.retarget(Some(1), Some(slot(80.0)), size, false, now);
        state.retarget(
            Some(1),
            Some(slot(80.0)),
            size,
            true,
            now + Duration::from_millis(70),
        );
        assert_eq!(state.morph.map(|morph| morph.bounds()), Some(slot(80.0)));
        assert!(!state.morph.is_some_and(|morph| morph.active()));
    }
}
