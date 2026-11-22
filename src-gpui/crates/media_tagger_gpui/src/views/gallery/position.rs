//! The position bar and the month timeline at the right edge of the grid
//! (`GalleryScrollbar.tsx`, `GalleryTimelineScrubber.tsx`).
//!
//! What they show and where a press, a drag or a key goes are decided by
//! `media_tagger_client::gallery::{scrollbar, timeline}`; this file draws them and
//! turns pointer events into inputs. Both are tab stops. While one has the focus it
//! handles the gallery's scroll actions itself (Home, End, Page Up, Page Down, Up,
//! Down; GPUI hands an action to the deepest listener on the focus path), so the
//! keys move the bar by rows and pages and the timeline by months and years, as in
//! the Solid gallery.
//!
//! A drag goes on outside the element, so the element registers window-wide mouse
//! listeners in every paint (as Zed's `ui` scrollbar does), and they do nothing
//! while no drag lasts. Registering them only during a drag would lose a release
//! that arrives before the next frame is drawn (GPUI hands mouse events to the
//! listeners of the last drawn frame), and the drag would outlive the button.
//!
//! Both come before the tiles in the view's children, so the tab order is bar,
//! timeline, then the load error banner's Retry, as in the Solid DOM. What reaches
//! over the tiles (the month label, the year labels, the focus rings) is `deferred`
//! and so painted above them, like the Solid rail's `z-10`.

use std::{cell::Cell, rc::Rc};

use gpui::{
    canvas, deferred, div, prelude::*, px, AnyElement, Bounds, Context, DispatchPhase, Hsla,
    MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Orientation, Pixels, Role,
    SharedString, Window,
};
use media_tagger_client::{
    gallery::{
        scrollbar::{
            visuals::scrollbar_visuals, SCROLLBAR_INSET, SCROLLBAR_RIGHT, SCROLLBAR_WIDTH,
        },
        scrollbar_key_target, scrollbar_value_now, scrollbar_visible,
        timeline::{
            visuals::{self as timeline_visuals, timeline_visuals as rail_visuals},
            TIMELINE_INSET, TIMELINE_RIGHT, TIMELINE_WIDTH,
        },
        GridInput, ScrollbarDrag, ScrollbarKey, ScrollbarMetrics, ScrollbarTarget, TimelineEffect,
        TimelineInput, TimelineKey,
    },
    i18n::I18n,
    model::Model as _,
    theme::{ThemeName, RADII, TYPOGRAPHY},
};

use super::{
    grid::{LineDown, LineUp, PageDown, PageUp, ScrollEnd, ScrollStart},
    GalleryView,
};
use crate::ui::{
    fonts,
    theme::{box_shadows, color, focus_ring},
};

/// Debug selectors (`VisualTestContext::debug_bounds`).
pub const SCROLLBAR_SELECTOR: &str = "gallery-scrollbar";
pub const SCROLLBAR_THUMB_SELECTOR: &str = "gallery-scroll-thumb";
pub const TIMELINE_SELECTOR: &str = "gallery-timeline";
pub const TIMELINE_MARKER_SELECTOR: &str = "gallery-timeline-marker";
pub const TIMELINE_LABEL_SELECTOR: &str = "gallery-timeline-label";

/// The debug selector of the tick of `year` on the timeline.
pub fn timeline_tick_selector(year: u32) -> String {
    format!("gallery-timeline-tick-{year}")
}

/// Where an element was laid out in the last frame, for pointer positions.
pub(super) type LastBounds = Rc<Cell<Option<Bounds<Pixels>>>>;

/// Records the bounds of the element it fills.
fn bounds_probe(bounds: &LastBounds) -> impl IntoElement {
    let bounds = bounds.clone();
    canvas(move |area, _, _| bounds.set(Some(area)), |_, _, _, _| {})
        .absolute()
        .top_0()
        .left_0()
        .size_full()
}

/// Registers window-wide listeners for the moves and the release of a drag. They
/// are registered in every frame, so `on_move` and `on_release` must do nothing
/// while no drag lasts.
fn drag_listeners(
    view: gpui::WeakEntity<GalleryView>,
    on_move: fn(&mut GalleryView, f64, &mut Window, &mut Context<GalleryView>),
    on_release: fn(&mut GalleryView, &mut Window, &mut Context<GalleryView>),
) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |_, _, window, _| {
            let moved = view.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble && event.dragging() {
                    let y = f64::from(f32::from(event.position.y));
                    moved
                        .update(cx, |view, cx| on_move(view, y, window, cx))
                        .ok();
                }
            });
            let released = view.clone();
            window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble && event.button == MouseButton::Left {
                    released
                        .update(cx, |view, cx| on_release(view, window, cx))
                        .ok();
                }
            });
        },
    )
    .absolute()
    .size_0()
}

impl GalleryView {
    /// The bar for the grid's current position.
    fn scrollbar_metrics(&self) -> ScrollbarMetrics {
        ScrollbarMetrics::new(
            self.grid.geometry().viewport,
            self.grid.offset(),
            self.grid.min_offset(),
            self.grid.max_offset(),
        )
    }

    fn first_visible(&self) -> usize {
        self.grid
            .range()
            .map(|range| range.visible_start_index)
            .unwrap_or(0)
    }

    fn scrollbar_key(&mut self, key: ScrollbarKey, cx: &mut Context<Self>) {
        let target = scrollbar_key_target(
            key,
            self.grid.offset(),
            self.grid.layout().stride,
            self.grid.geometry().viewport,
        );
        let at = self.now(cx);
        let input = match target {
            ScrollbarTarget::Offset(offset) => GridInput::ScrollTo { offset, at },
            ScrollbarTarget::Edge(edge) => GridInput::ScrollToEdge { edge, at },
        };
        self.scroll(input, cx);
    }

    /// A primary press on the bar's track at window position `y`.
    fn scrollbar_press(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        // Like the Solid bar's prevented pointerdown, the press keeps the focus where
        // it is, and the grid's own press handler does not run.
        cx.stop_propagation();
        let Some(bounds) = self.scrollbar_bounds.get() else {
            return;
        };
        let y = f64::from(f32::from(event.position.y));
        let metrics = self.scrollbar_metrics();
        let (target, start_fraction) = metrics.press(y - f64::from(f32::from(bounds.top())));
        self.scrollbar_drag = Some(ScrollbarDrag {
            start_y: y,
            start_fraction,
        });
        match target {
            Some(offset) => {
                let at = self.now(cx);
                self.scroll(GridInput::ScrollTo { offset, at }, cx);
            }
            None => cx.notify(),
        }
    }

    fn scrollbar_drag_to(&mut self, y: f64, _: &mut Window, cx: &mut Context<Self>) {
        let Some(drag) = self.scrollbar_drag else {
            return;
        };
        let offset = drag.offset_at(&self.scrollbar_metrics(), y);
        let at = self.now(cx);
        self.scroll(GridInput::ScrollTo { offset, at }, cx);
    }

    fn scrollbar_release(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.scrollbar_drag.take().is_some() {
            cx.notify();
        }
    }

    /// Applies a timeline input and what it asks for.
    pub(super) fn timeline_input(
        &mut self,
        input: TimelineInput,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for effect in self.timeline.handle(input) {
            match effect {
                TimelineEffect::Jump(index) => {
                    self.grid.handle(GridInput::JumpToIndex(index));
                    self.refresh(cx);
                    let first_visible = self.first_visible();
                    self.timeline
                        .handle(TimelineInput::Jumped { first_visible });
                }
                TimelineEffect::HoldDemand => self.scroll(GridInput::HoldDemand, cx),
                TimelineEffect::ResumeDemand => self.scroll(GridInput::ResumeDemand, cx),
                TimelineEffect::RequestFrame => cx.on_next_frame(window, |this, window, cx| {
                    this.timeline_input(TimelineInput::Frame, window, cx)
                }),
            }
        }
        cx.notify();
    }

    fn timeline_key(&mut self, key: TimelineKey, window: &mut Window, cx: &mut Context<Self>) {
        self.timeline_input(TimelineInput::Key(key), window, cx);
    }

    /// `y` in the window as pixels below the rail's top.
    fn rail_y(&self, y: f64) -> Option<f64> {
        self.timeline_bounds
            .get()
            .map(|bounds| y - f64::from(f32::from(bounds.top())))
    }

    fn timeline_press(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        let Some(y) = self.rail_y(f64::from(f32::from(event.position.y))) else {
            return;
        };
        // The keys that follow a click move the timeline.
        window.focus(&self.timeline_focus, cx);
        self.timeline_input(TimelineInput::Press { y }, window, cx);
    }

    fn timeline_drag_to(&mut self, y: f64, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(y) = self.rail_y(y) {
            self.timeline_input(TimelineInput::PointerMove { y }, window, cx);
        }
    }

    /// A window-wide pointer move with a button down: it moves the timeline only
    /// while a drag of the rail lasts (hovering is the rail's own listener).
    fn timeline_drag_move(&mut self, y: f64, window: &mut Window, cx: &mut Context<Self>) {
        if self.timeline.dragging() {
            self.timeline_drag_to(y, window, cx);
        }
    }

    fn timeline_release(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.timeline.dragging() {
            self.timeline_input(TimelineInput::Release, window, cx);
        }
    }

    /// The position bar, when there is something to scroll.
    pub(super) fn scrollbar(
        &self,
        i18n: &I18n<'_>,
        theme: ThemeName,
        window: &Window,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let count = self.grid.layout().count;
        if !scrollbar_visible(count, self.grid.min_offset(), self.grid.max_offset()) {
            return None;
        }
        let visuals = scrollbar_visuals(theme.tokens());
        let metrics = self.scrollbar_metrics();
        let focus_visible =
            self.scrollbar_focus.is_focused(window) && window.last_input_was_keyboard();
        let value = scrollbar_value_now(self.first_visible(), count);
        let bar = div()
            .id(SCROLLBAR_SELECTOR)
            .debug_selector(|| SCROLLBAR_SELECTOR.to_owned())
            .role(Role::ScrollBar)
            .aria_label(SharedString::from(i18n.t("gallery.scrollbarLabel")))
            .aria_orientation(Orientation::Vertical)
            .aria_min_numeric_value(1.0)
            .aria_max_numeric_value(count as f64)
            .aria_numeric_value(value as f64)
            .track_focus(&self.scrollbar_focus)
            .absolute()
            .top(px(SCROLLBAR_INSET as f32))
            .right(px(SCROLLBAR_RIGHT))
            .w(px(SCROLLBAR_WIDTH))
            .h(px(metrics.track as f32))
            .rounded_full()
            .bg(color(visuals.track))
            .hover(move |style| style.bg(color(visuals.track_hover)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event, _, cx| this.scrollbar_press(event, cx)),
            )
            .on_action(
                cx.listener(|this, _: &LineDown, _, cx| this.scrollbar_key(ScrollbarKey::Down, cx)),
            )
            .on_action(
                cx.listener(|this, _: &LineUp, _, cx| this.scrollbar_key(ScrollbarKey::Up, cx)),
            )
            .on_action(cx.listener(|this, _: &PageDown, _, cx| {
                this.scrollbar_key(ScrollbarKey::PageDown, cx)
            }))
            .on_action(
                cx.listener(|this, _: &PageUp, _, cx| this.scrollbar_key(ScrollbarKey::PageUp, cx)),
            )
            .on_action(cx.listener(|this, _: &ScrollStart, _, cx| {
                this.scrollbar_key(ScrollbarKey::Home, cx)
            }))
            .on_action(
                cx.listener(|this, _: &ScrollEnd, _, cx| this.scrollbar_key(ScrollbarKey::End, cx)),
            )
            .child(bounds_probe(&self.scrollbar_bounds))
            .child(
                div()
                    .debug_selector(|| SCROLLBAR_THUMB_SELECTOR.to_owned())
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(metrics.thumb_top() as f32))
                    .h(px(metrics.thumb as f32))
                    .rounded_full()
                    .bg(color(visuals.thumb)),
            )
            .when(focus_visible, |bar| {
                bar.child(deferred(focus_ring(
                    visuals.focus_ring,
                    SCROLLBAR_WIDTH / 2.0,
                )))
            })
            .child(drag_listeners(
                cx.entity().downgrade(),
                Self::scrollbar_drag_to,
                Self::scrollbar_release,
            ));
        Some(bar.into_any_element())
    }

    /// The month timeline, when the result spans at least two months.
    pub(super) fn timeline_rail(
        &self,
        i18n: &I18n<'_>,
        theme: ThemeName,
        window: &Window,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let timeline = &self.timeline;
        if !timeline.visible() {
            return None;
        }
        let visuals = rail_visuals(theme.tokens());
        let language = i18n.language();
        let no_date = i18n.t("gallery.timelineNoDate");
        let focused = self.timeline_focus.is_focused(window);
        let focus_visible = focused && window.last_input_was_keyboard();
        let current_text = timeline.month_text(timeline.current(), language, &no_date);
        let track = timeline.track() as f32;
        let hovered = self.timeline_hovered;

        let ticks = timeline.ticks().into_iter().map(|tick| {
            let year = tick.year;
            div()
                .debug_selector(move || timeline_tick_selector(year))
                .absolute()
                .right_0()
                .top(px(tick.y as f32))
                .w(px(timeline_visuals::TICK_WIDTH))
                .h(px(timeline_visuals::TICK_HEIGHT))
                .bg(color(visuals.tick))
                .when(hovered, |tick| {
                    tick.child(deferred(
                        div()
                            .absolute()
                            .right(px(timeline_visuals::TICK_LABEL_RIGHT))
                            .top(px(timeline_visuals::TICK_LABEL_TOP))
                            .whitespace_nowrap()
                            .font_family(fonts::MONO)
                            .text_size(px(timeline_visuals::TICK_LABEL_SIZE))
                            .line_height(px(timeline_visuals::TICK_LABEL_SIZE))
                            .text_color(color(visuals.tick_label))
                            .child(year.to_string()),
                    ))
                })
        });

        let label = timeline.label(focused).map(|label| {
            let text = timeline.month_text(label.month, language, &no_date);
            // `-translate-y-1/2`: centred on `y`.
            let height = timeline_visuals::label_height();
            div()
                .debug_selector(|| TIMELINE_LABEL_SELECTOR.to_owned())
                .absolute()
                .right(px(timeline_visuals::LABEL_RIGHT))
                .top(px(label.y as f32 - height / 2.0))
                .whitespace_nowrap()
                .rounded(px(RADII.surface))
                .border_1()
                .border_color(color(visuals.label_border))
                .bg(color(visuals.label_fill))
                .px(px(timeline_visuals::LABEL_PADDING_X))
                .py(px(timeline_visuals::LABEL_PADDING_Y))
                .text_size(px(TYPOGRAPHY.xs.size))
                .line_height(px(timeline_visuals::label_line_height()))
                .shadow(box_shadows(visuals.label_shadow))
                .child(text)
        });

        let rail_hover: Hsla = color(visuals.rail_hover);
        let rail = div()
            .id(TIMELINE_SELECTOR)
            .debug_selector(|| TIMELINE_SELECTOR.to_owned())
            .role(Role::Slider)
            .aria_label(SharedString::from(i18n.t("gallery.timelineLabel")))
            .aria_orientation(Orientation::Vertical)
            .aria_min_numeric_value(1.0)
            .aria_max_numeric_value(timeline.value_max() as f64)
            .aria_numeric_value(timeline.value_now() as f64)
            .aria_value(SharedString::from(current_text))
            .track_focus(&self.timeline_focus)
            .absolute()
            .top(px(TIMELINE_INSET as f32))
            .right(px(TIMELINE_RIGHT))
            .w(px(TIMELINE_WIDTH))
            .h(px(track))
            .rounded_full()
            .hover(move |style| style.bg(rail_hover))
            .on_hover(cx.listener(|this, hovered: &bool, window, cx| {
                this.timeline_hovered = *hovered;
                if !*hovered {
                    this.timeline_input(TimelineInput::PointerLeave, window, cx);
                } else {
                    cx.notify();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event, window, cx| this.timeline_press(event, window, cx)),
            )
            // A press the rail ignores (any button but the primary one) does not steal
            // the focus: it suppresses GPUI's focus-on-press for the rail and for the
            // focusable viewport around it, and still bubbles on.
            .on_any_mouse_down(|event, window, _| {
                if event.button != MouseButton::Left {
                    window.prevent_default();
                }
            })
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                // A drag is followed by the window-wide listener.
                if !this.timeline.dragging() {
                    let y = f64::from(f32::from(event.position.y));
                    this.timeline_drag_to(y, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &LineDown, window, cx| {
                this.timeline_key(TimelineKey::Down, window, cx)
            }))
            .on_action(cx.listener(|this, _: &LineUp, window, cx| {
                this.timeline_key(TimelineKey::Up, window, cx)
            }))
            .on_action(cx.listener(|this, _: &PageDown, window, cx| {
                this.timeline_key(TimelineKey::PageDown, window, cx)
            }))
            .on_action(cx.listener(|this, _: &PageUp, window, cx| {
                this.timeline_key(TimelineKey::PageUp, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ScrollStart, window, cx| {
                this.timeline_key(TimelineKey::Home, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ScrollEnd, window, cx| {
                this.timeline_key(TimelineKey::End, window, cx)
            }))
            .child(bounds_probe(&self.timeline_bounds))
            .children(ticks)
            .child(
                div()
                    .debug_selector(|| TIMELINE_MARKER_SELECTOR.to_owned())
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(timeline.marker_y() as f32))
                    .h(px(timeline_visuals::MARKER_HEIGHT))
                    .rounded_full()
                    .bg(color(visuals.marker)),
            )
            .children(label.map(deferred))
            .when(focus_visible, |rail| {
                rail.child(deferred(focus_ring(
                    visuals.focus_ring,
                    TIMELINE_WIDTH / 2.0,
                )))
            })
            .child(drag_listeners(
                cx.entity().downgrade(),
                Self::timeline_drag_move,
                Self::timeline_release,
            ));
        Some(rail.into_any_element())
    }
}
