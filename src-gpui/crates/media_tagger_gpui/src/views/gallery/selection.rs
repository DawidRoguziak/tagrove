//! Selection mode in the gallery (`createGridHandlers.ts`, `TileSizeSlider.tsx`):
//! the bulk-actions toggle, the pointer gestures on the grid, Escape and Ctrl+A.
//!
//! What is selected is decided by `media_tagger_client::gallery::Selection` in the
//! runtime; what a press, a move and a release mean is decided by
//! `SelectionGesture`. This file turns GPUI's events into the gesture's inputs
//! (pointer positions relative to the viewport, the grid's place in it, the time on
//! the executor's clock) and runs what it answers: commands for the selection, the
//! long-press timer, edge scrolling, animation frames.
//!
//! A rectangle goes on outside the gallery, so the moves and the release are
//! window-wide listeners registered in every paint, which do nothing while no press
//! is held (as the position bar's drag does). The release is taken in the capture
//! phase: GPUI fires the click of the tile under the pointer in the bubble phase of
//! the same event, and the gesture must have ended before that click arrives, as a
//! `pointerup` comes before its `click`.
//!
//! While selection mode is on, the view holds a layer of the window's layer stack
//! that is not modal: Escape clears the selection unless a modal lies above, and
//! Ctrl+A selects everything only while that layer is the top one.

use gpui::{
    canvas, prelude::*, App, ClickEvent, Context, DispatchPhase, Modifiers, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, SharedString, Window,
};
use media_tagger_client::{
    controls::ButtonVariant,
    gallery::{
        selection_geometry::Point as GesturePoint, tile_index_at, GestureEffect, GestureFrame,
        GestureInput, GridInput, PressedTile, SelectionGrid, SelectionInput,
    },
    i18n::I18n,
    icons::IconName,
    model::{Model as _, RequestToken},
    runtime::RuntimeInput,
    shell::{bulk_toggle_keys, tools_row},
};

use super::{grid::GalleryEvent, GalleryView};
use crate::{
    runtime,
    ui::{
        button::Button,
        layers::{self, LayerOptions},
    },
};

/// Element id of the bulk-actions toggle.
pub const BULK_TOGGLE_ID: &str = "bulk-actions-toggle";

/// `event.ctrlKey || event.metaKey`.
fn ctrl_like(modifiers: Modifiers) -> bool {
    modifiers.control || modifiers.platform
}

/// What the gesture was last told of the runtime and the layout: selection mode and
/// the query epoch, and the grid's measures.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(super) struct GestureStamp {
    pub(super) mode: bool,
    pub(super) query_epoch: u64,
    pub(super) layout: Option<LayoutStamp>,
}

/// Columns, tile size, count, what scrolls above the grid, and the viewport's size.
pub(super) type LayoutStamp = (usize, f64, usize, f64, f64, f64);

impl GalleryView {
    pub(super) fn selection_enabled(&self, cx: &App) -> bool {
        self.runtime.read(cx).model().selection().enabled()
    }

    /// Where the grid lies in the viewport now.
    fn gesture_frame(&self) -> GestureFrame {
        let (width, height) = self
            .viewport_size
            .map(|size| {
                (
                    f64::from(f32::from(size.width)),
                    f64::from(f32::from(size.height)),
                )
            })
            .unwrap_or_default();
        GestureFrame {
            origin: GesturePoint {
                x: f64::from(media_tagger_client::gallery::layout::area::PADDING_LEFT),
                y: self.grid.row_y(0),
            },
            width,
            height,
            grid: SelectionGrid::of(self.grid.layout()),
        }
    }

    /// A window position relative to the viewport's top-left corner; `None` before
    /// the viewport was laid out.
    fn viewport_point(&self, position: gpui::Point<Pixels>) -> Option<GesturePoint> {
        let origin = self.viewport_origin.get()?;
        Some(GesturePoint {
            x: f64::from(f32::from(position.x - origin.x)),
            y: f64::from(f32::from(position.y - origin.y)),
        })
    }

    /// Applies a gesture input and what it asks for. Returns whether it asked for an
    /// animation frame, which only a caller with the window can schedule.
    pub(super) fn gesture_input(&mut self, input: GestureInput, cx: &mut Context<Self>) -> bool {
        let mut wants_frame = false;
        for effect in self.gesture.handle(input) {
            match effect {
                GestureEffect::Interact(interaction) => {
                    self.runtime.update(cx, |runtime, cx| {
                        runtime.send(
                            RuntimeInput::Selection(SelectionInput::Interact(interaction)),
                            cx,
                        )
                    });
                }
                GestureEffect::Open { asset_id, index } => {
                    cx.emit(GalleryEvent::OpenAsset { asset_id, index });
                }
                GestureEffect::LongPressTimer { token, delay } => {
                    self.long_press_timer = Some(cx.spawn(async move |this, cx| {
                        cx.background_executor().timer(delay).await;
                        this.update(cx, |this, cx| this.long_press_elapsed(token, cx))
                            .ok();
                    }));
                }
                GestureEffect::ScrollBy(delta) => {
                    let at = self.now(cx);
                    self.grid_input(GridInput::ScrollBy { delta, at }, cx);
                    self.refresh(cx);
                    // The pressed point moved with the grid.
                    let frame = self.gesture_frame();
                    self.gesture.handle(GestureInput::Scrolled(frame));
                }
                GestureEffect::RequestFrame => wants_frame = true,
            }
        }
        cx.notify();
        wants_frame
    }

    /// [`Self::gesture_input`] where the window is at hand: also schedules the frame.
    fn gesture_input_in(
        &mut self,
        input: GestureInput,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.gesture_input(input, cx) && !self.gesture_frame_pending {
            self.gesture_frame_pending = true;
            cx.on_next_frame(window, |this, window, cx| {
                this.gesture_frame_pending = false;
                let at = this.now(cx);
                this.gesture_input_in(GestureInput::Frame { at }, window, cx);
            });
        }
    }

    fn long_press_elapsed(&mut self, token: RequestToken, cx: &mut Context<Self>) {
        // The asset at the pressed index now: only the one that was pressed may open.
        let asset_id = self.gesture.long_press_index(token).and_then(|index| {
            self.runtime
                .read(cx)
                .model()
                .query()
                .asset_at(index)
                .map(|asset| asset.id)
        });
        self.gesture_input(GestureInput::LongPressElapsed { token, asset_id }, cx);
    }

    /// A primary press in the viewport that no control above it took.
    pub(super) fn selection_press(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let on_control = self.control_pressed.replace(false);
        let Some(point) = self.viewport_point(event.position) else {
            return;
        };
        let frame = self.gesture_frame();
        let (enabled, long_press) = {
            let model = self.runtime.read(cx).model();
            // A gesture starts on the grid and the space around its tiles: not on a
            // control, not in the empty state and not on the footer below the grid.
            let enabled = model.selection().enabled()
                && !on_control
                && model.query().total() > 0
                && point.y < self.grid.trail_y();
            // Modifier presses are range and toggle gestures: they never open a tile.
            let long_press = (enabled && !event.modifiers.modified())
                .then(|| tile_index_at(frame.to_grid(point), &frame.grid))
                .flatten()
                .and_then(|index| {
                    let asset = model.query().asset_at(index)?;
                    Some(PressedTile {
                        asset_id: asset.id,
                        index,
                    })
                });
            (enabled, long_press)
        };
        let at = self.now(cx);
        self.gesture_input_in(
            GestureInput::Press {
                at,
                point,
                frame,
                enabled,
                additive: ctrl_like(event.modifiers),
                long_press,
            },
            window,
            cx,
        );
    }

    fn selection_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.gesture.held() {
            return;
        }
        if !event.dragging() {
            // The button went up where the window did not see it.
            self.gesture_input(GestureInput::CancelHeld, cx);
            return;
        }
        let Some(point) = self.viewport_point(event.position) else {
            return;
        };
        let frame = self.gesture_frame();
        let at = self.now(cx);
        self.gesture_input_in(GestureInput::Move { at, point, frame }, window, cx);
    }

    fn selection_release(
        &mut self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.gesture.held() {
            return;
        }
        let Some(point) = self.viewport_point(event.position) else {
            self.gesture_input(GestureInput::CancelHeld, cx);
            return;
        };
        let frame = self.gesture_frame();
        self.gesture_input_in(GestureInput::Release { point, frame }, window, cx);
    }

    /// A tile was activated: it opens outside selection mode and toggles in it,
    /// unless this is the click that ends a rectangle or a long press.
    pub(super) fn tile_clicked(
        &mut self,
        asset_id: i64,
        index: usize,
        event: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let modifiers = event.modifiers();
        let enabled = self.selection_enabled(cx);
        self.gesture_input_in(
            GestureInput::TileClick {
                asset_id,
                index,
                ctrl_like: ctrl_like(modifiers),
                shift: modifiers.shift,
                pointer: !matches!(event, ClickEvent::Keyboard(_)),
                enabled,
            },
            window,
            cx,
        );
    }

    /// Ctrl+A: every result of the query, while selection mode is on and no other
    /// layer lies above the gallery. Otherwise the key goes on.
    pub(super) fn select_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let on_top = self
            .selection_layer
            .is_some_and(|layer| layers::is_top(layer, cx));
        if !self.selection_enabled(cx) || !on_top {
            cx.propagate();
            return;
        }
        self.gesture_input_in(GestureInput::SelectAll, window, cx);
    }

    /// The window lost its focus: a held gesture ends; a released range read and its
    /// retry go on.
    pub(super) fn window_deactivated(&mut self, cx: &mut Context<Self>) {
        self.gesture_input(GestureInput::CancelHeld, cx);
    }

    /// Holds the selection layer exactly while selection mode is on: Escape clears
    /// the selection (and keeps the mode) unless another layer lies above.
    pub(super) fn sync_selection_layer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match (self.selection_enabled(cx), self.selection_layer) {
            (true, None) => {
                let view = cx.weak_entity();
                self.selection_layer = Some(layers::register(
                    LayerOptions::nonmodal(move |_, cx| {
                        view.update(cx, |view, cx| {
                            view.gesture_input(GestureInput::Escape, cx);
                        })
                        .ok();
                    }),
                    window,
                    cx,
                ));
            }
            (false, Some(layer)) => {
                self.selection_layer = None;
                layers::release(layer, None, window, cx);
            }
            _ => {}
        }
    }

    /// Tells the gesture what changed in the runtime and in the layout since it was
    /// last told. Part of [`Self::refresh`].
    pub(super) fn sync_gesture(&mut self, cx: &mut Context<Self>) {
        let (mode, query_epoch, busy) = {
            let model = self.runtime.read(cx).model();
            (
                model.selection().enabled(),
                model.query().query_epoch(),
                model.selection().busy(),
            )
        };
        if (self.gesture_stamp.mode, self.gesture_stamp.query_epoch) != (mode, query_epoch) {
            self.gesture_stamp.mode = mode;
            self.gesture_stamp.query_epoch = query_epoch;
            self.gesture_input(GestureInput::Reset, cx);
        }
        let layout = self.viewport_size.map(|size| {
            let layout = self.grid.layout();
            (
                layout.columns,
                layout.size,
                layout.count,
                self.grid.geometry().lead,
                f64::from(f32::from(size.width)),
                f64::from(f32::from(size.height)),
            )
        });
        if self.gesture_stamp.layout != layout {
            // The first layout changes nothing that a gesture could have seen.
            let known = self.gesture_stamp.layout.is_some();
            self.gesture_stamp.layout = layout;
            if known {
                self.gesture_input(GestureInput::LayoutChanged, cx);
            }
        }
        // The preview of a released rectangle goes once the model took its ranges.
        if self.gesture.awaiting_commit() && !busy {
            self.gesture.handle(GestureInput::CommitSettled);
        }
    }

    /// The window-wide listeners of a held gesture. They are registered in every
    /// frame, so they do nothing while no press is held.
    pub(super) fn selection_listeners(&self, cx: &Context<Self>) -> impl IntoElement {
        let view = cx.entity().downgrade();
        canvas(
            |_, _, _| {},
            move |_, _, window, _| {
                let moved = view.clone();
                window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                    if phase == DispatchPhase::Bubble {
                        moved
                            .update(cx, |view, cx| view.selection_move(event, window, cx))
                            .ok();
                    }
                });
                let released = view.clone();
                window.on_mouse_event(move |event: &MouseUpEvent, phase, window, cx| {
                    // Before the click of the tile under the pointer, which GPUI fires
                    // in the bubble phase of this event.
                    if phase == DispatchPhase::Capture && event.button == MouseButton::Left {
                        released
                            .update(cx, |view, cx| view.selection_release(event, window, cx))
                            .ok();
                    }
                });
            },
        )
        .absolute()
        .size_0()
    }

    /// The bulk-actions toggle of the tools row: primary while selection mode is on.
    pub(super) fn bulk_toggle(&self, i18n: &I18n<'_>, cx: &App) -> Button {
        let enabled = self.selection_enabled(cx);
        let (label, name) = bulk_toggle_keys(enabled);
        let runtime = self.runtime.downgrade();
        Button::new(BULK_TOGGLE_ID, SharedString::from(i18n.t(label)))
            .variant(if enabled {
                ButtonVariant::Primary
            } else {
                ButtonVariant::Ghost
            })
            .leading_icon(IconName::BulkActions, tools_row::BULK_TOGGLE_ICON_SIZE)
            .gap(tools_row::BULK_TOGGLE_GAP)
            .aria_label(SharedString::from(i18n.t(name)))
            .pressed(enabled)
            .debug_selector(BULK_TOGGLE_ID)
            .on_click(move |_, _, cx| {
                runtime::send(
                    &runtime,
                    RuntimeInput::Selection(SelectionInput::ToggleMode),
                    cx,
                )
            })
    }
}
