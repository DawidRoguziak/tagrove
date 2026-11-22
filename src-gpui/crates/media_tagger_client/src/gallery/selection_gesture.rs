//! The pointer gestures of selection mode (`src/features/gallery/createGridHandlers.ts`)
//! as a state machine: the press that may become a rectangle, the rectangle with its
//! preview and edge scrolling, the long press that opens a tile, the click that
//! follows a gesture and must not count, Escape and Ctrl+A.
//!
//! [`SelectionGesture`] knows no window. The view tells it where the pointer is in
//! the gallery's viewport and where the grid lies in that viewport ([`GestureFrame`]),
//! with the time of each event on any monotonic clock; it answers with what to tell
//! the selection model ([`GestureEffect::Interact`]), what to open, how far to scroll
//! and which timer or frame to ask for.
//!
//! Rules it keeps (`docs/subsystems/search-tags-and-media-groups.md`, "Bulk
//! selection"):
//!
//! - A rectangle starts on a move only after the press was held for
//!   [`DRAG_HOLD_DELAY`] and moved [`MOVE_TOLERANCE`] pixels. Releasing sooner is a
//!   click; holding still starts nothing.
//! - The rectangle spans from the pressed point, which scrolls with the grid, to the
//!   pointer, clamped to the viewport. Its tiles are computed from global indexes
//!   ([`intersected_ranges`]), whatever is drawn or cached. While it is dragged, the
//!   pointer near the top or bottom edge scrolls the gallery ([`edge_scroll_speed`]).
//! - A press on a tile held for [`LONG_PRESS`] without a modifier and without moving
//!   opens that asset and leaves the selection alone.
//! - The click that ends a rectangle or a long press is swallowed once. The next
//!   primary press forgets a swallow that never came, also outside selection mode.
//! - A change of layout ends a held gesture but not the read of a released rectangle
//!   or its failed retry; only the preview goes.

use std::time::Duration;

use super::{
    selection::{IndexRange, Interaction},
    selection_geometry::{
        contains_index, edge_scroll_speed, intersected_ranges, rectangle_between, Point,
        SelectionGrid, SelectionRectangle,
    },
};
use crate::model::{Model, RequestToken, Tokens};

/// A press must be held this long before a move turns it into a rectangle.
pub const DRAG_HOLD_DELAY: Duration = Duration::from_millis(150);
/// Holding a tile this long without moving opens it in selection mode.
pub const LONG_PRESS: Duration = Duration::from_millis(500);
/// A pointer that moved less than this many pixels has not moved.
pub const MOVE_TOLERANCE: f64 = 5.0;
/// The longest time one animation frame scrolls for, so a late frame does not jump.
const MAX_FRAME_STEP: Duration = Duration::from_millis(32);

/// Where the grid lies in the gallery's viewport. Points of a gesture are relative
/// to the viewport's top-left corner.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GestureFrame {
    /// The top-left corner of the first tile. `y` is the grid's scroll position
    /// negated, so it is far below zero deep in a large result.
    pub origin: Point,
    /// The size of the viewport.
    pub width: f64,
    pub height: f64,
    pub grid: SelectionGrid,
}

impl GestureFrame {
    /// `point` of the viewport in grid coordinates.
    pub fn to_grid(&self, point: Point) -> Point {
        Point {
            x: point.x - self.origin.x,
            y: point.y - self.origin.y,
        }
    }
}

/// The loaded asset of the tile under a press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PressedTile {
    pub asset_id: i64,
    pub index: usize,
}

/// The rectangle while it is dragged and until its ranges are committed.
#[derive(Debug, Clone, PartialEq)]
pub struct Preview {
    /// What to draw, in viewport coordinates, clipped to the viewport.
    pub rectangle: SelectionRectangle,
    /// The tiles it covers, as ascending global index ranges.
    pub ranges: Vec<IndexRange>,
    /// Ctrl or the platform key was held at the press: the ranges are added.
    pub additive: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GestureInput {
    /// A primary press in the gallery's viewport, outside its controls.
    Press {
        at: Duration,
        point: Point,
        frame: GestureFrame,
        /// Selection mode is on.
        enabled: bool,
        /// Ctrl or the platform key is held.
        additive: bool,
        /// The loaded tile under the pointer when no modifier is held.
        long_press: Option<PressedTile>,
    },
    /// The pointer moved, anywhere in the window, with the press still held.
    Move {
        at: Duration,
        point: Point,
        frame: GestureFrame,
    },
    /// The primary button was released, anywhere in the window.
    Release { point: Point, frame: GestureFrame },
    /// An animation frame, asked for with [`GestureEffect::RequestFrame`].
    Frame { at: Duration },
    /// The grid moved under the pointer (the scroll of a frame was applied).
    Scrolled(GestureFrame),
    /// The long-press timer fired. `asset_id` is the asset at the pressed index now:
    /// only the asset that was pressed may open.
    LongPressElapsed {
        token: RequestToken,
        asset_id: Option<i64>,
    },
    /// The window lost the pointer or the focus: ends a held gesture only.
    CancelHeld,
    /// The grid's measures changed (columns, tile size, count, the banner above it).
    LayoutChanged,
    /// Selection mode or the query epoch changed: ends a held gesture and hides the
    /// preview of a released one.
    Reset,
    /// Escape in selection mode: clears the selection.
    Escape,
    /// Ctrl+A in selection mode.
    SelectAll,
    /// A tile was activated.
    TileClick {
        asset_id: i64,
        index: usize,
        ctrl_like: bool,
        shift: bool,
        /// By the pointer; a click from the keyboard is never swallowed.
        pointer: bool,
        /// Selection mode is on.
        enabled: bool,
    },
    /// The selection model is no longer busy with the committed ranges.
    CommitSettled,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GestureEffect {
    /// Tell the selection model.
    Interact(Interaction),
    /// Open the asset (the lightbox).
    Open { asset_id: i64, index: usize },
    /// Send [`GestureInput::LongPressElapsed`] with this token after `delay`.
    LongPressTimer {
        token: RequestToken,
        delay: Duration,
    },
    /// Scroll the gallery by this many pixels, then send [`GestureInput::Scrolled`].
    ScrollBy(f64),
    /// Send [`GestureInput::Frame`] at the next animation frame.
    RequestFrame,
}

#[derive(Debug)]
struct Held {
    pressed_at: Duration,
    /// The pressed point in grid coordinates: it scrolls with the grid.
    start: Point,
    /// The pressed point in the viewport.
    origin: Point,
    latest: Point,
    frame: GestureFrame,
    additive: bool,
    dragging: bool,
    last_frame: Duration,
    long_press: Option<(RequestToken, PressedTile)>,
}

#[derive(Debug, Default)]
pub struct SelectionGesture {
    held: Option<Held>,
    /// Advances whenever older gesture work becomes obsolete.
    generation: u64,
    suppress_click: bool,
    preview: Option<Preview>,
    /// The generation of the committed rectangle whose preview waits for the model.
    awaiting_commit: Option<u64>,
    tokens: Tokens,
}

impl SelectionGesture {
    pub fn new() -> Self {
        Self::default()
    }

    /// A press is held, as a rectangle or not yet.
    pub fn held(&self) -> bool {
        self.held.is_some()
    }

    /// A rectangle is being dragged.
    pub fn dragging(&self) -> bool {
        self.held.as_ref().is_some_and(|held| held.dragging)
    }

    /// The rectangle to draw and the tiles it covers.
    pub fn preview(&self) -> Option<&Preview> {
        self.preview.as_ref()
    }

    /// The preview of a released rectangle is shown until the selection model has
    /// taken its ranges; the view sends [`GestureInput::CommitSettled`] then.
    pub fn awaiting_commit(&self) -> bool {
        self.awaiting_commit.is_some()
    }

    /// The next pointer click on a tile will be swallowed.
    pub fn suppresses_click(&self) -> bool {
        self.suppress_click
    }

    /// The index whose asset the long press of `token` would open.
    pub fn long_press_index(&self, token: RequestToken) -> Option<usize> {
        let (pending, tile) = self.held.as_ref()?.long_press?;
        (pending == token).then_some(tile.index)
    }

    /// Whether the tile at `index` is drawn selected: under the rectangle, or
    /// selected before when the rectangle adds or none is shown.
    pub fn is_preview_selected(&self, index: usize, selected: bool) -> bool {
        match &self.preview {
            Some(preview) => {
                contains_index(&preview.ranges, index) || (preview.additive && selected)
            }
            None => selected,
        }
    }

    /// Ends whatever is held or shown and tells the selection model.
    fn cancel(&mut self, effects: &mut Vec<GestureEffect>) {
        self.generation += 1;
        if self.dragging() {
            self.suppress_click = true;
        }
        self.held = None;
        self.preview = None;
        self.awaiting_commit = None;
        effects.push(GestureEffect::Interact(Interaction::RectangleCancel));
    }

    fn update_preview(&mut self) {
        let Some(held) = &self.held else {
            return;
        };
        let frame = &held.frame;
        let pointer = Point {
            x: held.latest.x.clamp(0.0, frame.width.max(0.0)),
            y: held.latest.y.clamp(0.0, frame.height.max(0.0)),
        };
        let rect = rectangle_between(held.start, frame.to_grid(pointer));
        let left = (frame.origin.x + rect.left).max(0.0);
        let top = (frame.origin.y + rect.top).max(0.0);
        let right = (frame.origin.x + rect.left + rect.width).min(frame.width);
        let bottom = (frame.origin.y + rect.top + rect.height).min(frame.height);
        self.preview = Some(Preview {
            rectangle: SelectionRectangle {
                left,
                top,
                width: (right - left).max(0.0),
                height: (bottom - top).max(0.0),
            },
            ranges: intersected_ranges(&rect, &frame.grid),
            additive: held.additive,
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn press(
        &mut self,
        at: Duration,
        point: Point,
        frame: GestureFrame,
        enabled: bool,
        additive: bool,
        long_press: Option<PressedTile>,
        effects: &mut Vec<GestureEffect>,
    ) {
        // A new press must not use up click suppression left by an earlier drag.
        self.suppress_click = false;
        if !enabled || point.x >= frame.width || point.y >= frame.height {
            return;
        }
        self.cancel(effects);
        self.suppress_click = false;
        effects.push(GestureEffect::Interact(Interaction::RectangleStart));
        // A long press on a tile opens it without touching the selection. Modifier
        // presses are range and toggle gestures, so they never open the lightbox.
        let long_press = long_press.map(|tile| {
            let token = self.tokens.issue();
            effects.push(GestureEffect::LongPressTimer {
                token,
                delay: LONG_PRESS,
            });
            (token, tile)
        });
        self.held = Some(Held {
            pressed_at: at,
            start: frame.to_grid(point),
            origin: point,
            latest: point,
            frame,
            additive,
            dragging: false,
            last_frame: at,
            long_press,
        });
    }

    fn pointer_move(
        &mut self,
        at: Duration,
        point: Point,
        frame: GestureFrame,
        effects: &mut Vec<GestureEffect>,
    ) {
        let Some(held) = &mut self.held else {
            return;
        };
        held.latest = point;
        held.frame = frame;
        let moved = (point.x - held.origin.x).hypot(point.y - held.origin.y) >= MOVE_TOLERANCE;
        if moved {
            held.long_press = None;
        }
        if !held.dragging && moved && at.saturating_sub(held.pressed_at) >= DRAG_HOLD_DELAY {
            held.dragging = true;
            held.last_frame = at;
            self.suppress_click = true;
            effects.push(GestureEffect::RequestFrame);
        }
        if self.dragging() {
            self.update_preview();
        }
    }

    fn release(&mut self, point: Point, frame: GestureFrame, effects: &mut Vec<GestureEffect>) {
        let Some(held) = &mut self.held else {
            return;
        };
        if !held.dragging {
            self.cancel(effects);
            return;
        }
        held.latest = point;
        held.frame = frame;
        self.update_preview();
        self.held = None;
        if let Some(preview) = &self.preview {
            // The preview stays until the model has taken the ranges, so the tiles do
            // not flicker between the release and a range read.
            self.awaiting_commit = Some(self.generation);
            effects.push(GestureEffect::Interact(Interaction::RectangleCommit {
                ranges: preview.ranges.clone(),
                additive: preview.additive,
            }));
        }
    }

    fn frame(&mut self, at: Duration, effects: &mut Vec<GestureEffect>) {
        let Some(held) = self.held.as_mut().filter(|held| held.dragging) else {
            return;
        };
        let elapsed = at.saturating_sub(held.last_frame).min(MAX_FRAME_STEP);
        held.last_frame = at;
        let delta =
            edge_scroll_speed(held.latest.y, 0.0, held.frame.height) * elapsed.as_secs_f64();
        if delta != 0.0 && delta.is_finite() {
            effects.push(GestureEffect::ScrollBy(delta));
        }
        effects.push(GestureEffect::RequestFrame);
    }

    fn long_press_elapsed(
        &mut self,
        token: RequestToken,
        asset_id: Option<i64>,
        effects: &mut Vec<GestureEffect>,
    ) {
        let Some(held) = &mut self.held else {
            return;
        };
        let Some((_, tile)) = held.long_press.filter(|(pending, _)| *pending == token) else {
            // The timer of an earlier press, or of one that moved.
            return;
        };
        held.long_press = None;
        // Scrolling during the hold can put another asset under the pressed index.
        if held.dragging || asset_id != Some(tile.asset_id) {
            return;
        }
        self.cancel(effects);
        // The release after a long press must not toggle the tile.
        self.suppress_click = true;
        effects.push(GestureEffect::Open {
            asset_id: tile.asset_id,
            index: tile.index,
        });
    }
}

impl Model for SelectionGesture {
    type Input = GestureInput;
    type Effect = GestureEffect;

    fn handle(&mut self, input: GestureInput) -> Vec<GestureEffect> {
        let mut effects = Vec::new();
        match input {
            GestureInput::Press {
                at,
                point,
                frame,
                enabled,
                additive,
                long_press,
            } => self.press(
                at,
                point,
                frame,
                enabled,
                additive,
                long_press,
                &mut effects,
            ),
            GestureInput::Move { at, point, frame } => {
                self.pointer_move(at, point, frame, &mut effects);
            }
            GestureInput::Release { point, frame } => self.release(point, frame, &mut effects),
            GestureInput::Frame { at } => self.frame(at, &mut effects),
            GestureInput::Scrolled(frame) => {
                if let Some(held) = &mut self.held {
                    held.frame = frame;
                    if held.dragging {
                        self.update_preview();
                    }
                }
            }
            GestureInput::LongPressElapsed { token, asset_id } => {
                self.long_press_elapsed(token, asset_id, &mut effects);
            }
            GestureInput::CancelHeld => {
                if self.held.is_some() {
                    self.cancel(&mut effects);
                }
            }
            GestureInput::LayoutChanged => {
                if self.held.is_some() {
                    self.cancel(&mut effects);
                } else {
                    // Released ranges use global indexes. Layout only invalidates their
                    // preview, not a pending read or the retry kept after a failed one.
                    self.generation += 1;
                    self.preview = None;
                    self.awaiting_commit = None;
                }
            }
            GestureInput::Reset => {
                if self.held.is_some() {
                    self.cancel(&mut effects);
                } else {
                    // The selection model cancels its own reads on both changes. A
                    // cancel from here would arrive after whatever the same change
                    // already started in the model, and end that instead.
                    self.generation += 1;
                    self.preview = None;
                    self.awaiting_commit = None;
                }
            }
            GestureInput::Escape => {
                if self.held.is_some() {
                    self.suppress_click = true;
                }
                self.cancel(&mut effects);
                effects.push(GestureEffect::Interact(Interaction::Clear));
            }
            GestureInput::SelectAll => {
                self.cancel(&mut effects);
                effects.push(GestureEffect::Interact(Interaction::SelectAll));
            }
            GestureInput::TileClick {
                asset_id,
                index,
                ctrl_like,
                shift,
                pointer,
                enabled,
            } => {
                if self.suppress_click && pointer {
                    self.suppress_click = false;
                } else if !enabled {
                    effects.push(GestureEffect::Open { asset_id, index });
                } else {
                    self.generation += 1;
                    self.held = None;
                    self.preview = None;
                    self.awaiting_commit = None;
                    effects.push(GestureEffect::Interact(Interaction::Click {
                        asset_id,
                        asset_index: index,
                        ctrl_like,
                        shift,
                    }));
                }
            }
            GestureInput::CommitSettled => {
                if self.awaiting_commit.take() == Some(self.generation) {
                    self.preview = None;
                }
            }
        }
        effects
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{
        GestureEffect, GestureFrame, GestureInput, PressedTile, SelectionGesture, DRAG_HOLD_DELAY,
        LONG_PRESS,
    };
    use crate::{
        gallery::{
            selection::Interaction,
            selection_geometry::{Point, SelectionGrid, SelectionRectangle},
        },
        model::{Model as _, RequestToken},
    };

    /// Four columns of 100 px tiles with 10 px gaps, 20 px from the left and the top
    /// of an 800 x 600 viewport; 1000 assets.
    fn frame(scrolled: f64) -> GestureFrame {
        GestureFrame {
            origin: Point {
                x: 20.0,
                y: 20.0 - scrolled,
            },
            width: 800.0,
            height: 600.0,
            grid: SelectionGrid {
                column_count: 4,
                tile_size: 100.0,
                tile_gap: 10.0,
                asset_count: 1_000,
            },
        }
    }

    fn at(point: (f64, f64)) -> Point {
        Point {
            x: point.0,
            y: point.1,
        }
    }

    fn ms(value: u64) -> Duration {
        Duration::from_millis(value)
    }

    fn press(gesture: &mut SelectionGesture, point: (f64, f64)) -> Vec<GestureEffect> {
        gesture.handle(GestureInput::Press {
            at: ms(0),
            point: at(point),
            frame: frame(0.0),
            enabled: true,
            additive: false,
            long_press: None,
        })
    }

    fn move_to(gesture: &mut SelectionGesture, time: u64, point: (f64, f64)) -> Vec<GestureEffect> {
        gesture.handle(GestureInput::Move {
            at: ms(time),
            point: at(point),
            frame: frame(0.0),
        })
    }

    fn release(gesture: &mut SelectionGesture, point: (f64, f64)) -> Vec<GestureEffect> {
        gesture.handle(GestureInput::Release {
            point: at(point),
            frame: frame(0.0),
        })
    }

    fn interact(interaction: Interaction) -> GestureEffect {
        GestureEffect::Interact(interaction)
    }

    fn click(gesture: &mut SelectionGesture, index: usize, enabled: bool) -> Vec<GestureEffect> {
        gesture.handle(GestureInput::TileClick {
            asset_id: index as i64 + 1,
            index,
            ctrl_like: false,
            shift: false,
            pointer: true,
            enabled,
        })
    }

    fn long_press_token(effects: &[GestureEffect]) -> RequestToken {
        effects
            .iter()
            .find_map(|effect| match effect {
                GestureEffect::LongPressTimer { token, delay } => {
                    assert_eq!(*delay, LONG_PRESS);
                    Some(*token)
                }
                _ => None,
            })
            .expect("a long-press timer")
    }

    #[test]
    fn a_press_tells_the_model_and_a_quick_release_is_left_to_the_click() {
        let mut gesture = SelectionGesture::new();
        assert_eq!(
            press(&mut gesture, (50.0, 50.0)),
            [
                interact(Interaction::RectangleCancel),
                interact(Interaction::RectangleStart)
            ]
        );
        assert!(gesture.held() && !gesture.dragging());

        assert_eq!(
            release(&mut gesture, (50.0, 50.0)),
            [interact(Interaction::RectangleCancel)]
        );
        assert!(!gesture.held() && !gesture.suppresses_click());
        assert_eq!(
            click(&mut gesture, 0, true),
            [interact(Interaction::Click {
                asset_id: 1,
                asset_index: 0,
                ctrl_like: false,
                shift: false
            })]
        );
    }

    #[test]
    fn a_rectangle_needs_the_hold_and_the_movement() {
        // Moved far, but too soon.
        let mut gesture = SelectionGesture::new();
        press(&mut gesture, (50.0, 50.0));
        assert!(move_to(&mut gesture, 149, (200.0, 200.0)).is_empty());
        assert!(!gesture.dragging() && gesture.preview().is_none());
        // The same pointer position after the hold starts it.
        assert_eq!(
            move_to(&mut gesture, 150, (200.0, 200.0)),
            [GestureEffect::RequestFrame]
        );
        assert!(gesture.dragging());

        // Held long enough, but moved less than five pixels.
        let mut gesture = SelectionGesture::new();
        press(&mut gesture, (50.0, 50.0));
        assert!(move_to(&mut gesture, 400, (53.0, 53.0)).is_empty());
        assert!(!gesture.dragging());
        assert_eq!(move_to(&mut gesture, 410, (53.0, 54.0)).len(), 1);
        assert!(gesture.dragging());
        assert_eq!(DRAG_HOLD_DELAY, ms(150));
    }

    #[test]
    fn the_preview_covers_the_tiles_under_the_rectangle_in_any_direction() {
        let mut gesture = SelectionGesture::new();
        press(&mut gesture, (150.0, 150.0));
        move_to(&mut gesture, 200, (300.0, 260.0));
        let preview = gesture.preview().expect("a preview").clone();
        // Grid coordinates (130, 130)..(280, 240): columns 1..2, rows 1..2.
        assert_eq!(preview.ranges, [[5, 6], [9, 10]]);
        assert_eq!(
            preview.rectangle,
            SelectionRectangle {
                left: 150.0,
                top: 150.0,
                width: 150.0,
                height: 110.0
            }
        );
        assert!(gesture.is_preview_selected(5, false));
        assert!(
            !gesture.is_preview_selected(4, true),
            "a plain rectangle replaces"
        );

        // Up and to the left of the pressed point, then shrinking.
        move_to(&mut gesture, 220, (30.0, 30.0));
        assert_eq!(gesture.preview().unwrap().ranges, [[0, 1], [4, 5]]);
        move_to(&mut gesture, 240, (140.0, 140.0));
        assert_eq!(gesture.preview().unwrap().ranges, [[5, 5]]);
    }

    #[test]
    fn releasing_a_rectangle_commits_its_ranges_and_swallows_the_click_that_follows() {
        let mut gesture = SelectionGesture::new();
        gesture.handle(GestureInput::Press {
            at: ms(0),
            point: at((150.0, 150.0)),
            frame: frame(0.0),
            enabled: true,
            additive: true,
            long_press: None,
        });
        move_to(&mut gesture, 200, (200.0, 200.0));
        assert!(
            gesture.is_preview_selected(900, true),
            "an additive rectangle keeps"
        );
        assert_eq!(
            release(&mut gesture, (300.0, 260.0)),
            [interact(Interaction::RectangleCommit {
                ranges: vec![[5, 6], [9, 10]],
                additive: true
            })]
        );
        assert!(!gesture.held() && gesture.awaiting_commit());
        assert!(
            gesture.preview().is_some(),
            "shown until the model took the ranges"
        );

        assert!(
            click(&mut gesture, 5, true).is_empty(),
            "the drag's own click"
        );
        assert_eq!(click(&mut gesture, 5, true).len(), 1);

        gesture.handle(GestureInput::CommitSettled);
        assert!(gesture.preview().is_none() && !gesture.awaiting_commit());
    }

    #[test]
    fn a_new_press_forgets_a_swallow_that_never_came_also_outside_selection_mode() {
        let mut gesture = SelectionGesture::new();
        press(&mut gesture, (150.0, 150.0));
        move_to(&mut gesture, 200, (300.0, 260.0));
        release(&mut gesture, (300.0, 260.0));
        assert!(gesture.suppresses_click());

        let effects = gesture.handle(GestureInput::Press {
            at: ms(900),
            point: at((50.0, 50.0)),
            frame: frame(0.0),
            enabled: false,
            additive: false,
            long_press: None,
        });
        assert!(effects.is_empty(), "no gesture outside selection mode");
        assert!(!gesture.suppresses_click() && !gesture.held());
        assert_eq!(
            click(&mut gesture, 0, false),
            [GestureEffect::Open {
                asset_id: 1,
                index: 0
            }]
        );
    }

    #[test]
    fn a_press_beyond_the_viewport_starts_nothing() {
        let mut gesture = SelectionGesture::new();
        assert!(press(&mut gesture, (800.0, 50.0)).is_empty());
        assert!(press(&mut gesture, (50.0, 600.0)).is_empty());
        assert!(!gesture.held());
    }

    #[test]
    fn a_keyboard_click_is_never_swallowed() {
        let mut gesture = SelectionGesture::new();
        press(&mut gesture, (150.0, 150.0));
        move_to(&mut gesture, 200, (300.0, 260.0));
        release(&mut gesture, (300.0, 260.0));
        let effects = gesture.handle(GestureInput::TileClick {
            asset_id: 3,
            index: 2,
            ctrl_like: false,
            shift: false,
            pointer: false,
            enabled: true,
        });
        assert_eq!(effects.len(), 1);
        assert!(
            gesture.suppresses_click(),
            "still waiting for the pointer's click"
        );
    }

    #[test]
    fn a_long_press_opens_the_pressed_asset_and_leaves_the_selection_alone() {
        let mut gesture = SelectionGesture::new();
        let tile = PressedTile {
            asset_id: 6,
            index: 5,
        };
        let effects = gesture.handle(GestureInput::Press {
            at: ms(0),
            point: at((150.0, 150.0)),
            frame: frame(0.0),
            enabled: true,
            additive: false,
            long_press: Some(tile),
        });
        let token = long_press_token(&effects);
        assert_eq!(gesture.long_press_index(token), Some(5));

        assert_eq!(
            gesture.handle(GestureInput::LongPressElapsed {
                token,
                asset_id: Some(6)
            }),
            [
                interact(Interaction::RectangleCancel),
                GestureEffect::Open {
                    asset_id: 6,
                    index: 5
                }
            ]
        );
        assert!(!gesture.held());
        // The release and its click do nothing.
        assert!(release(&mut gesture, (150.0, 150.0)).is_empty());
        assert!(click(&mut gesture, 5, true).is_empty());
    }

    #[test]
    fn a_long_press_is_cancelled_by_movement_a_release_or_another_asset_under_the_index() {
        let tile = PressedTile {
            asset_id: 6,
            index: 5,
        };
        let start = |gesture: &mut SelectionGesture| {
            let effects = gesture.handle(GestureInput::Press {
                at: ms(0),
                point: at((150.0, 150.0)),
                frame: frame(0.0),
                enabled: true,
                additive: false,
                long_press: Some(tile),
            });
            long_press_token(&effects)
        };
        let fire = |gesture: &mut SelectionGesture, token, asset_id| {
            gesture.handle(GestureInput::LongPressElapsed { token, asset_id })
        };

        // Moved five pixels before the hold delay: no rectangle, and no long press.
        let mut gesture = SelectionGesture::new();
        let token = start(&mut gesture);
        move_to(&mut gesture, 50, (156.0, 150.0));
        assert_eq!(gesture.long_press_index(token), None);
        assert!(fire(&mut gesture, token, Some(6)).is_empty());
        assert!(gesture.held());

        // Released before the timer.
        let mut gesture = SelectionGesture::new();
        let token = start(&mut gesture);
        release(&mut gesture, (150.0, 150.0));
        assert!(fire(&mut gesture, token, Some(6)).is_empty());
        assert_eq!(click(&mut gesture, 5, true).len(), 1);

        // The grid scrolled: another asset, or none, sits at the pressed index.
        for now in [Some(7), None] {
            let mut gesture = SelectionGesture::new();
            let token = start(&mut gesture);
            assert!(fire(&mut gesture, token, now).is_empty());
            assert!(gesture.held() && !gesture.suppresses_click());
        }

        // The timer of an earlier press.
        let mut gesture = SelectionGesture::new();
        let old = start(&mut gesture);
        release(&mut gesture, (150.0, 150.0));
        start(&mut gesture);
        assert!(fire(&mut gesture, old, Some(6)).is_empty());
    }

    #[test]
    fn the_edges_scroll_while_dragging_and_the_rectangle_follows_the_grid() {
        let mut gesture = SelectionGesture::new();
        press(&mut gesture, (150.0, 150.0));
        move_to(&mut gesture, 200, (300.0, 590.0));
        // 16 ms at full speed below the bottom edge zone's middle.
        let effects = gesture.handle(GestureInput::Frame { at: ms(216) });
        let expected = 900.0 * ((590.0 - 600.0 + 48.0) / 48.0) * 0.016;
        match effects.as_slice() {
            [GestureEffect::ScrollBy(delta), GestureEffect::RequestFrame] => {
                assert!((delta - expected).abs() < 1e-9, "{delta}");
            }
            other => panic!("{other:?}"),
        }
        // A late frame scrolls for 32 ms at most.
        let effects = gesture.handle(GestureInput::Frame { at: ms(1_216) });
        assert!(
            matches!(effects[0], GestureEffect::ScrollBy(delta) if (delta - expected * 2.0).abs() < 1e-9)
        );

        // The grid moved 220 px: the pressed point went up with it.
        gesture.handle(GestureInput::Scrolled(frame(220.0)));
        let preview = gesture.preview().expect("a preview");
        // Grid rows 1..7 now lie between the pressed point and the pointer.
        assert_eq!(
            preview.ranges,
            [
                [5, 6],
                [9, 10],
                [13, 14],
                [17, 18],
                [21, 22],
                [25, 26],
                [29, 30]
            ]
        );
        assert_eq!(
            preview.rectangle,
            SelectionRectangle {
                left: 150.0,
                top: 0.0,
                width: 150.0,
                height: 590.0
            },
            "clipped to the viewport"
        );

        // In the middle nothing scrolls, and frames go on while the drag lasts.
        move_to(&mut gesture, 1_300, (300.0, 300.0));
        assert_eq!(
            gesture.handle(GestureInput::Frame { at: ms(1_316) }),
            [GestureEffect::RequestFrame]
        );
        release(&mut gesture, (300.0, 300.0));
        assert!(gesture
            .handle(GestureInput::Frame { at: ms(1_332) })
            .is_empty());
    }

    #[test]
    fn the_pointer_outside_the_viewport_is_clamped_to_it() {
        let mut gesture = SelectionGesture::new();
        press(&mut gesture, (150.0, 150.0));
        move_to(&mut gesture, 200, (2_000.0, -300.0));
        let preview = gesture.preview().expect("a preview");
        assert_eq!(
            preview.rectangle,
            SelectionRectangle {
                left: 150.0,
                top: 0.0,
                width: 650.0,
                height: 150.0
            }
        );
        assert_eq!(preview.ranges, [[1, 3], [5, 7]]);
    }

    #[test]
    fn escape_clears_and_a_held_drag_does_not_click_afterwards() {
        let mut gesture = SelectionGesture::new();
        assert_eq!(
            gesture.handle(GestureInput::Escape),
            [
                interact(Interaction::RectangleCancel),
                interact(Interaction::Clear)
            ]
        );
        assert!(!gesture.suppresses_click());

        press(&mut gesture, (150.0, 150.0));
        move_to(&mut gesture, 200, (300.0, 260.0));
        gesture.handle(GestureInput::Escape);
        assert!(!gesture.held() && gesture.preview().is_none());
        assert!(release(&mut gesture, (300.0, 260.0)).is_empty());
        assert!(click(&mut gesture, 5, true).is_empty());
    }

    #[test]
    fn select_all_cancels_a_gesture_first() {
        let mut gesture = SelectionGesture::new();
        press(&mut gesture, (150.0, 150.0));
        assert_eq!(
            gesture.handle(GestureInput::SelectAll),
            [
                interact(Interaction::RectangleCancel),
                interact(Interaction::SelectAll)
            ]
        );
        assert!(!gesture.held());
    }

    #[test]
    fn a_layout_change_ends_a_held_gesture_but_only_hides_a_released_preview() {
        let mut gesture = SelectionGesture::new();
        press(&mut gesture, (150.0, 150.0));
        move_to(&mut gesture, 200, (300.0, 260.0));
        assert_eq!(
            gesture.handle(GestureInput::LayoutChanged),
            [interact(Interaction::RectangleCancel)]
        );
        assert!(!gesture.held() && gesture.preview().is_none());

        press(&mut gesture, (150.0, 150.0));
        move_to(&mut gesture, 200, (300.0, 260.0));
        release(&mut gesture, (300.0, 260.0));
        assert!(
            gesture.handle(GestureInput::LayoutChanged).is_empty(),
            "the released read and its retry are the model's"
        );
        assert!(gesture.preview().is_none() && !gesture.awaiting_commit());
    }

    #[test]
    fn losing_the_window_ends_a_held_gesture_only() {
        let mut gesture = SelectionGesture::new();
        assert!(gesture.handle(GestureInput::CancelHeld).is_empty());
        press(&mut gesture, (150.0, 150.0));
        move_to(&mut gesture, 200, (300.0, 260.0));
        assert_eq!(
            gesture.handle(GestureInput::CancelHeld),
            [interact(Interaction::RectangleCancel)]
        );
        assert!(
            gesture.suppresses_click(),
            "a cancelled drag still swallows its click"
        );
    }

    #[test]
    fn a_mode_or_query_change_cancels_whatever_is_shown() {
        let mut gesture = SelectionGesture::new();
        press(&mut gesture, (150.0, 150.0));
        move_to(&mut gesture, 200, (300.0, 260.0));
        assert_eq!(
            gesture.handle(GestureInput::Reset),
            [interact(Interaction::RectangleCancel)]
        );
        assert!(!gesture.held() && gesture.preview().is_none());
        assert!(gesture.suppresses_click());

        // After the release the model owns the committed ranges and cancels them
        // itself: only the preview goes.
        press(&mut gesture, (150.0, 150.0));
        move_to(&mut gesture, 200, (300.0, 260.0));
        release(&mut gesture, (300.0, 260.0));
        assert!(gesture.handle(GestureInput::Reset).is_empty());
        assert!(gesture.preview().is_none());
        // The settle of the cancelled commit changes nothing.
        gesture.handle(GestureInput::CommitSettled);
        assert!(gesture.preview().is_none() && !gesture.awaiting_commit());
        assert!(SelectionGesture::new()
            .handle(GestureInput::Reset)
            .is_empty());
    }

    #[test]
    fn a_tile_click_ends_the_preview_of_an_older_rectangle() {
        let mut gesture = SelectionGesture::new();
        press(&mut gesture, (150.0, 150.0));
        move_to(&mut gesture, 200, (300.0, 260.0));
        release(&mut gesture, (300.0, 260.0));
        click(&mut gesture, 5, true);
        assert!(
            gesture.preview().is_some(),
            "the swallowed click changes nothing"
        );
        click(&mut gesture, 5, true);
        assert!(gesture.preview().is_none());
    }
}
