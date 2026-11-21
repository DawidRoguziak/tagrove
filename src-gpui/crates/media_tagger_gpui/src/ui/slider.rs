//! `UiSlider`: a range input. Keys, a press on the track and a drag ask the owner for a
//! value with [`SliderEvent::Input`]; the slider keeps drawing the owner's value until
//! the owner calls [`Slider::set_value`], as `UiSlider` puts the committed value back
//! after every input.

use std::{cell::Cell, rc::Rc};

use gpui::{
    actions, canvas, div, prelude::*, px, relative, App, Bounds, Context, DispatchPhase,
    EventEmitter, FocusHandle, Focusable, KeyBinding, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, Role, SharedString, Window,
};
use media_tagger_client::{
    controls::{slider_visuals, SliderKey, SliderRange},
    theme::ThemeName,
};

use super::theme::{self, color, focus_ring};

/// The key context of a focused slider.
pub const KEY_CONTEXT: &str = "UiSlider";

actions!(
    ui_slider,
    [
        /// Arrow right and arrow up.
        Increase,
        /// Arrow left and arrow down.
        Decrease,
        PageUp,
        PageDown,
        Home,
        End,
    ]
);

/// The keys of a focused range input, in [`KEY_CONTEXT`].
pub fn key_bindings() -> Vec<KeyBinding> {
    let context = Some(KEY_CONTEXT);
    vec![
        KeyBinding::new("right", Increase, context),
        KeyBinding::new("up", Increase, context),
        KeyBinding::new("left", Decrease, context),
        KeyBinding::new("down", Decrease, context),
        KeyBinding::new("pageup", PageUp, context),
        KeyBinding::new("pagedown", PageDown, context),
        KeyBinding::new("home", Home, context),
        KeyBinding::new("end", End, context),
    ]
}

pub fn init(cx: &mut App) {
    cx.bind_keys(key_bindings());
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SliderEvent {
    /// The value a key, a press or a drag asks for (`onInput`).
    Input(f64),
}

pub struct Slider {
    range: SliderRange,
    value: f64,
    label: SharedString,
    disabled: bool,
    theme: Option<ThemeName>,
    focus_handle: FocusHandle,
    /// Where the thumb's centre can go, as the last frame drew the track.
    thumb_span: Rc<Cell<Option<Bounds<Pixels>>>>,
    dragging: bool,
}

impl EventEmitter<SliderEvent> for Slider {}

impl Focusable for Slider {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Slider {
    /// `label` is the accessible name (`aria-label`).
    pub fn new(
        range: SliderRange,
        value: f64,
        label: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            range,
            value: range.sanitize(value),
            label: label.into(),
            disabled: false,
            theme: None,
            focus_handle: cx.focus_handle().tab_stop(true),
            thumb_span: Rc::default(),
            dragging: false,
        }
    }

    pub fn value(&self) -> f64 {
        self.value
    }

    pub fn range(&self) -> SliderRange {
        self.range
    }

    /// The owner's value; the slider draws it.
    pub fn set_value(&mut self, value: f64, cx: &mut Context<Self>) {
        let value = self.range.sanitize(value);
        if value != self.value {
            self.value = value;
            cx.notify();
        }
    }

    pub fn set_range(&mut self, range: SliderRange, cx: &mut Context<Self>) {
        self.range = range;
        self.value = range.sanitize(self.value);
        cx.notify();
    }

    pub fn set_label(&mut self, label: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.label = label.into();
        cx.notify();
    }

    pub fn set_disabled(&mut self, disabled: bool, cx: &mut Context<Self>) {
        self.disabled = disabled;
        self.dragging &= !disabled;
        cx.notify();
    }

    pub fn set_theme(&mut self, theme: Option<ThemeName>, cx: &mut Context<Self>) {
        self.theme = theme;
        cx.notify();
    }

    fn key(&mut self, key: SliderKey, cx: &mut Context<Self>) {
        if self.disabled {
            return;
        }
        if let Some(value) = self.range.key(self.value, key) {
            cx.emit(SliderEvent::Input(value));
        }
    }

    /// The value under the pointer: the thumb's centre follows it.
    fn input_at(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(span) = self.thumb_span.get() else {
            return;
        };
        let width = f32::from(span.size.width);
        let fraction = if width > 0.0 {
            f32::from(position.x - span.origin.x) / width
        } else {
            0.0
        };
        let value = self.range.value_at(f64::from(fraction));
        if value != self.value {
            cx.emit(SliderEvent::Input(value));
        }
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.disabled {
            return;
        }
        window.focus(&self.focus_handle, cx);
        self.dragging = true;
        self.input_at(event.position, cx);
        cx.notify();
    }
}

impl Render for Slider {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = self.theme.unwrap_or_else(|| theme::active(cx));
        let visuals = slider_visuals(theme.tokens());
        let fraction = self.range.fraction(self.value) as f32;
        let focus_visible = !self.disabled
            && self.focus_handle.is_focused(window)
            && window.last_input_was_keyboard();
        let thumb = px(visuals.thumb_size);
        let ring = visuals.thumb_ring_width;
        let track_height = px(visuals.track_height);

        // The thumb's centre runs from half a thumb in from each end.
        let span = self.thumb_span.clone();
        let thumb_lane = div()
            .absolute()
            .top_0()
            .bottom_0()
            .left(thumb / 2.)
            .right(thumb / 2.)
            .child(canvas(move |bounds, _, _| span.set(Some(bounds)), |_, _, _, _| {}).size_full())
            .child(
                div()
                    .absolute()
                    .left(relative(fraction))
                    .top((track_height - thumb) / 2.)
                    .ml(-thumb / 2.)
                    .size(thumb)
                    .rounded_full()
                    .border_1()
                    .border_color(color(visuals.thumb_border))
                    .bg(color(visuals.thumb))
                    .shadow(vec![gpui::BoxShadow {
                        color: color(visuals.thumb_ring),
                        offset: gpui::point(px(0.), px(0.)),
                        blur_radius: px(0.),
                        spread_radius: px(ring),
                        inset: false,
                    }]),
            );

        let entity = cx.entity();
        let slider = div()
            .id("slider")
            .role(Role::Slider)
            .aria_label(self.label.clone())
            .aria_numeric_value(self.value)
            .aria_min_numeric_value(self.range.min)
            .aria_max_numeric_value(self.range.max)
            .aria_numeric_value_step(self.range.step)
            .key_context(KEY_CONTEXT)
            .relative()
            .w_full()
            .h(track_height)
            .rounded_full()
            .bg(color(visuals.track))
            .opacity(if self.disabled { 0.5 } else { 1.0 })
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left_0()
                    .w(relative(fraction))
                    .rounded_l_full()
                    .bg(color(visuals.fill)),
            )
            .child(thumb_lane)
            .when(focus_visible, |slider| {
                slider.child(focus_ring(visuals.focus_ring, visuals.track_height / 2.0))
            })
            // The track is 4px high; the thumb and a little around the track take
            // presses too.
            .child(
                div()
                    .id("slider-hit")
                    .absolute()
                    .left_0()
                    .right_0()
                    .top((track_height - thumb) / 2.)
                    .h(thumb)
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down)),
            );
        if self.disabled {
            return slider.cursor_not_allowed();
        }
        slider
            .track_focus(&self.focus_handle)
            .cursor_pointer()
            .on_action(cx.listener(|this, _: &Increase, _, cx| this.key(SliderKey::Increase, cx)))
            .on_action(cx.listener(|this, _: &Decrease, _, cx| this.key(SliderKey::Decrease, cx)))
            .on_action(cx.listener(|this, _: &PageUp, _, cx| this.key(SliderKey::PageUp, cx)))
            .on_action(cx.listener(|this, _: &PageDown, _, cx| this.key(SliderKey::PageDown, cx)))
            .on_action(cx.listener(|this, _: &Home, _, cx| this.key(SliderKey::Home, cx)))
            .on_action(cx.listener(|this, _: &End, _, cx| this.key(SliderKey::End, cx)))
            .when(self.dragging, |slider| {
                // A drag goes on outside the slider, so it listens on the window.
                slider.child(
                    canvas(
                        |_, _, _| {},
                        move |_, _, window, _| {
                            let moved = entity.clone();
                            window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                                if phase == DispatchPhase::Bubble && event.dragging() {
                                    moved.update(cx, |this, cx| this.input_at(event.position, cx));
                                }
                            });
                            let released = entity.clone();
                            window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                                if phase == DispatchPhase::Bubble
                                    && event.button == MouseButton::Left
                                {
                                    released.update(cx, |this, cx| {
                                        this.dragging = false;
                                        cx.notify();
                                    });
                                }
                            });
                        },
                    )
                    .absolute()
                    .size_0(),
                )
            })
    }
}
