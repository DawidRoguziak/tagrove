//! The selection specification of the Solid frontend (epic E11), ported case by case
//! at the lowest layer that proves it:
//!
//! - `src/features/gallery/__tests__/selectionGeometry.test.ts`: the pure geometry;
//! - `src/features/gallery/__tests__/createGridHandlers.test.tsx`: the pointer gestures
//!   (`SelectionGesture`), alone or together with the selection model they drive
//!   (`Selection`), on the Solid test's own geometry: a 340 x 400 viewport at the
//!   window's origin, a grid of three 100 px columns with 10 px gaps 10 px from its
//!   top-left corner, 3000 assets, the pressed tile being asset 1 at index 0;
//! - the real core on a temporary profile for a range selection that is read and
//!   written through `apply_selection_mutation`.
//!
//! What only a window can show (Ctrl+A from the keyboard, modal layers, mouse buttons,
//! controls) is in `media_tagger_gpui/tests/gallery_selection.rs`.

use std::{collections::BTreeSet, time::Duration};

use media_tagger_client::{
    backend::BackendError,
    gallery::{
        contains_index, intersected_ranges, rectangle_between, GestureEffect, GestureFrame,
        GestureInput, Interaction, Point, PressedTile, QuerySnapshot, Selection, SelectionEffect,
        SelectionError, SelectionGesture, SelectionGrid, SelectionInput, SelectionRectangle,
        LONG_PRESS,
    },
    library::{QueryError, Ticket},
    model::{Model as _, RequestToken},
};

// selectionGeometry.test.ts

mod geometry {
    use super::*;

    /// `{ columnCount: 3, tilePixelSize: 100, tileGap: 10, assetCount: 8 }`.
    const GRID: SelectionGrid = SelectionGrid {
        column_count: 3,
        tile_size: 100.0,
        tile_gap: 10.0,
        asset_count: 8,
    };

    fn rect(left: f64, top: f64, width: f64, height: f64) -> SelectionRectangle {
        SelectionRectangle {
            left,
            top,
            width,
            height,
        }
    }

    /// "includes partial overlaps in every direction %s %s %s %s".
    #[test]
    fn includes_partial_overlaps_in_every_direction() {
        for (x, y, ex, ey) in [
            (95.0, 95.0, 115.0, 115.0),
            (115.0, 115.0, 95.0, 95.0),
            (115.0, 95.0, 95.0, 115.0),
            (95.0, 115.0, 115.0, 95.0),
        ] {
            let rectangle = rectangle_between(Point { x, y }, Point { x: ex, y: ey });
            assert_eq!(
                intersected_ranges(&rectangle, &GRID),
                [[0, 1], [3, 4]],
                "{x} {y} {ex} {ey}"
            );
        }
    }

    /// "excludes gaps, edge touches, and unused final-row slots".
    #[test]
    fn excludes_gaps_edge_touches_and_unused_final_row_slots() {
        assert!(intersected_ranges(&rect(100.0, 0.0, 10.0, 320.0), &GRID).is_empty());
        assert!(intersected_ranges(&rect(0.0, 100.0, 320.0, 10.0), &GRID).is_empty());
        assert!(intersected_ranges(&rect(220.0, 220.0, 100.0, 100.0), &GRID).is_empty());
        assert_eq!(
            intersected_ranges(&rect(-20.0, -20.0, 400.0, 500.0), &GRID),
            [[0, 7]]
        );
    }

    /// "finds sparse ranges with binary lookup".
    #[test]
    fn finds_sparse_ranges_with_binary_lookup() {
        let ranges = [[2, 3], [7, 10]];
        let found: Vec<bool> = [1, 2, 3, 4, 7, 10, 11]
            .into_iter()
            .map(|index| contains_index(&ranges, index))
            .collect();
        assert_eq!(found, [false, true, true, false, true, true, false]);
    }
}

// createGridHandlers.test.tsx

/// The Solid test's harness: the gesture, optionally the selection model it drives,
/// and what reached the outside world.
struct Harness {
    gesture: SelectionGesture,
    /// The selection model the gesture's interactions go to, when the case has one.
    selection: Option<Selection>,
    /// Every interaction the gesture sent, in order.
    interactions: Vec<Interaction>,
    /// Assets the gesture opened (`onSelect`).
    opened: Vec<(i64, usize)>,
    /// Every scroll the gesture asked for.
    scrolls: Vec<f64>,
    /// Range reads of the selection model waiting for an answer.
    reads: Vec<(Ticket, usize, usize)>,
    read_count: usize,
    /// Long-press timers asked for.
    long_presses: Vec<(RequestToken, Duration)>,
    /// `performance.now()` in milliseconds.
    clock: u64,
    /// The grid's logical scroll offset (`scrollTop` plus the Solid grid's shift).
    scroll: f64,
    /// Selection mode.
    enabled: bool,
}

#[derive(Debug, Clone, Copy, Default)]
struct Keys {
    ctrl: bool,
    meta: bool,
    shift: bool,
}

const NO_KEYS: Keys = Keys {
    ctrl: false,
    meta: false,
    shift: false,
};

impl Harness {
    fn new() -> Self {
        Self {
            gesture: SelectionGesture::new(),
            selection: None,
            interactions: Vec::new(),
            opened: Vec::new(),
            scrolls: Vec::new(),
            reads: Vec::new(),
            read_count: 0,
            long_presses: Vec::new(),
            clock: 0,
            scroll: 0.0,
            enabled: true,
        }
    }

    /// With a selection model in selection mode, session 1 of 3000 results.
    fn with_selection() -> Self {
        let mut harness = Self::new();
        let mut selection = Selection::new(QuerySnapshot {
            query_epoch: 1,
            identity_epoch: 0,
            failure_epoch: 0,
            session_id: Some(1),
            total: 3000,
        });
        selection.handle(SelectionInput::ToggleMode);
        harness.selection = Some(selection);
        harness
    }

    fn selection(&self) -> &Selection {
        self.selection.as_ref().expect("a selection model")
    }

    /// The grid 10 px from the viewport's corner, moved up by the scroll offset.
    fn frame(&self) -> GestureFrame {
        GestureFrame {
            origin: Point {
                x: 10.0,
                y: 10.0 - self.scroll,
            },
            width: 340.0,
            height: 400.0,
            grid: SelectionGrid {
                column_count: 3,
                tile_size: 100.0,
                tile_gap: 10.0,
                asset_count: 3000,
            },
        }
    }

    fn at(&self) -> Duration {
        Duration::from_millis(self.clock)
    }

    fn select(&mut self, input: SelectionInput) {
        let Some(selection) = self.selection.as_mut() else {
            return;
        };
        for effect in selection.handle(input) {
            if let SelectionEffect::ReadIdRange { ticket, start, end } = effect {
                self.reads.push((ticket, start, end));
                self.read_count += 1;
            }
        }
    }

    fn gesture(&mut self, input: GestureInput) {
        let effects = self.gesture.handle(input);
        for effect in effects {
            match effect {
                GestureEffect::Interact(interaction) => {
                    self.interactions.push(interaction.clone());
                    self.select(SelectionInput::Interact(interaction));
                }
                GestureEffect::Open { asset_id, index } => self.opened.push((asset_id, index)),
                GestureEffect::LongPressTimer { token, delay } => {
                    self.long_presses.push((token, delay));
                }
                GestureEffect::ScrollBy(delta) => {
                    self.scrolls.push(delta);
                    self.scroll += delta;
                    let frame = self.frame();
                    self.gesture.handle(GestureInput::Scrolled(frame));
                }
                GestureEffect::RequestFrame => {}
            }
        }
    }

    /// A primary press on the tile (asset 1 at index 0) at `(x, y)`.
    fn down_with(&mut self, x: f64, y: f64, keys: Keys) {
        let modified = keys.ctrl || keys.meta || keys.shift;
        let input = GestureInput::Press {
            at: self.at(),
            point: Point { x, y },
            frame: self.frame(),
            enabled: self.enabled,
            additive: keys.ctrl || keys.meta,
            long_press: (!modified).then_some(PressedTile {
                asset_id: 1,
                index: 0,
            }),
        };
        self.gesture(input);
    }

    fn down(&mut self) {
        self.down_with(15.0, 15.0, NO_KEYS);
    }

    fn move_to(&mut self, x: f64, y: f64) {
        let input = GestureInput::Move {
            at: self.at(),
            point: Point { x, y },
            frame: self.frame(),
        };
        self.gesture(input);
    }

    fn up(&mut self, x: f64, y: f64) {
        let input = GestureInput::Release {
            point: Point { x, y },
            frame: self.frame(),
        };
        self.gesture(input);
    }

    /// An animation frame 16 ms later (`tick()`).
    fn tick(&mut self) {
        self.clock += 16;
        let at = self.at();
        self.gesture(GestureInput::Frame { at });
    }

    /// Whether an animation frame would do anything: `frame` is undefined in the
    /// Solid test when it would not.
    fn frame_runs(&mut self) -> bool {
        let at = Duration::from_millis(self.clock);
        // Without a drag a frame changes nothing, so asking is free.
        !self.gesture.handle(GestureInput::Frame { at }).is_empty()
    }

    /// The click on the tile; `detail` 0 is a click from the keyboard.
    fn click_with(&mut self, detail: u32, keys: Keys) {
        let input = GestureInput::TileClick {
            asset_id: 1,
            index: 0,
            ctrl_like: keys.ctrl || keys.meta,
            shift: keys.shift,
            pointer: detail != 0,
            enabled: self.enabled,
        };
        self.gesture(input);
    }

    fn click(&mut self, detail: u32) {
        self.click_with(detail, NO_KEYS);
    }

    fn ranges(&self) -> Vec<[usize; 2]> {
        self.gesture.preview().expect("a preview").ranges.clone()
    }

    fn commits(&self) -> usize {
        self.interactions
            .iter()
            .filter(|interaction| matches!(interaction, Interaction::RectangleCommit { .. }))
            .count()
    }

    fn answer(&mut self, result: impl FnOnce(usize, usize) -> Result<Vec<i64>, QueryError>) {
        let (ticket, start, end) = self.reads.remove(0);
        self.select(SelectionInput::IdRangeRead {
            ticket,
            result: result(start, end),
        });
    }
}

fn click_interaction() -> Interaction {
    Interaction::Click {
        asset_id: 1,
        asset_index: 0,
        ctrl_like: false,
        shift: false,
    }
}

fn failed_range() -> Result<Vec<i64>, QueryError> {
    Err(QueryError::Backend(BackendError::rejected("failed range")))
}

mod gestures {
    use super::*;

    /// "maps a rectangle through the current logical shift without treating scroll as
    /// layout cancellation": deep in the result, and while the grid scrolls under a
    /// held rectangle.
    #[test]
    fn maps_a_rectangle_through_the_scroll_offset_without_treating_scroll_as_a_layout_change() {
        let mut h = Harness::new();
        h.scroll = 11_000.0;
        h.down();
        h.clock += 150;
        h.move_to(150.0, 150.0);
        assert_eq!(h.ranges(), [[300, 301], [303, 304]]);
        assert_eq!(h.gesture.preview().unwrap().rectangle.top, 15.0);

        h.scroll = 11_110.0;
        h.move_to(150.0, 150.0);
        h.tick();
        assert_eq!(h.ranges(), [[300, 301], [303, 304], [306, 307]]);
        // The press cancels whatever was before it; the scroll after it cancels nothing.
        let start = h
            .interactions
            .iter()
            .position(|interaction| *interaction == Interaction::RectangleStart)
            .expect("the press started a gesture");
        assert!(!h.interactions[start..].contains(&Interaction::RectangleCancel));

        h.move_to(150.0, 399.0);
        h.tick();
        assert!(
            h.scrolls.last().is_some_and(|delta| *delta > 0.0),
            "{:?}",
            h.scrolls
        );
        h.up(150.0, 150.0);
        assert!(matches!(
            h.interactions.last(),
            Some(Interaction::RectangleCommit { .. })
        ));
    }

    /// "requires both a 150 ms hold and five pixels of movement".
    #[test]
    fn requires_both_a_150_ms_hold_and_five_pixels_of_movement() {
        let mut h = Harness::new();
        h.down();
        h.clock += 149;
        h.move_to(18.0, 19.0);
        assert!(h.gesture.preview().is_none());
        assert!(!h.gesture.dragging(), "no pointer capture");
        assert!(!h.frame_runs());
        h.clock += 1;
        assert!(h.gesture.preview().is_none());
        h.move_to(18.0, 17.0);
        assert!(h.gesture.preview().is_none());
        h.move_to(18.0, 19.0);
        assert!(h.gesture.preview().is_some());
        assert!(h.gesture.dragging(), "the pointer is captured");
    }

    /// "keeps quick repeated clicks immediate despite accidental movement".
    #[test]
    fn keeps_quick_repeated_clicks_immediate_despite_accidental_movement() {
        let mut h = Harness::new();
        for detail in [1, 2] {
            h.down();
            h.clock += 50;
            h.move_to(21.0, 15.0);
            h.up(21.0, 15.0);
            h.click(detail);
            assert_eq!(
                h.interactions.last(),
                Some(&click_interaction()),
                "{detail}"
            );
            assert!(h.gesture.preview().is_none());
            assert!(!h.gesture.dragging());
            assert!(!h.frame_runs());
        }
        let clicks = h
            .interactions
            .iter()
            .filter(|interaction| matches!(interaction, Interaction::Click { .. }))
            .count();
        assert_eq!(clicks, 2);
        assert_eq!(h.commits(), 0);

        h.clock += 200;
        h.move_to(150.0, 150.0);
        assert!(h.gesture.preview().is_none(), "no press is held");
        h.down();
        h.move_to(150.0, 150.0);
        assert!(h.gesture.preview().is_none(), "the hold starts again");
        h.clock += 150;
        h.move_to(150.0, 150.0);
        assert!(h.gesture.preview().is_some());
    }

    /// "keeps a stationary hold a click".
    #[test]
    fn keeps_a_stationary_hold_a_click() {
        let mut h = Harness::new();
        h.down();
        h.clock += 1000;
        h.move_to(15.0, 15.0);
        assert!(h.gesture.preview().is_none());
        assert!(!h.frame_runs());
        h.up(15.0, 15.0);
        h.click(1);
        assert_eq!(h.interactions.last(), Some(&click_interaction()));
    }

    /// "captures modifiers, shrinks, commits once, and suppresses the generated click"
    /// for Ctrl, Meta and no modifier.
    #[test]
    fn captures_modifiers_shrinks_commits_once_and_suppresses_the_generated_click() {
        for keys in [
            Keys {
                ctrl: true,
                ..NO_KEYS
            },
            Keys {
                meta: true,
                ..NO_KEYS
            },
            NO_KEYS,
        ] {
            let additive = keys.ctrl || keys.meta;
            let mut h = Harness::new();
            h.down_with(15.0, 15.0, keys);
            h.clock += 150;
            h.move_to(240.0, 250.0);
            h.tick();
            assert_eq!(h.ranges(), [[0, 8]], "{keys:?}");
            h.move_to(125.0, 125.0);
            h.tick();
            assert_eq!(h.ranges(), [[0, 1], [3, 4]], "{keys:?}");
            h.up(125.0, 125.0);
            assert_eq!(
                h.interactions.last(),
                Some(&Interaction::RectangleCommit {
                    ranges: vec![[0, 1], [3, 4]],
                    additive,
                }),
                "{keys:?}"
            );
            assert_eq!(h.commits(), 1);
            assert!(h.gesture.preview().is_some(), "{keys:?}");

            h.interactions.clear();
            h.click_with(1, keys);
            assert!(h.interactions.is_empty(), "{keys:?}: the drag's own click");
            // The model took the ranges.
            h.gesture(GestureInput::CommitSettled);
            assert!(h.gesture.preview().is_none(), "{keys:?}");
            h.click(0);
            assert_eq!(h.interactions.len(), 1, "{keys:?}");
        }
    }

    /// "handles the first new click after a rectangle without a trailing click, bulk
    /// enabled: %s" for both modes.
    #[test]
    fn handles_the_first_new_click_after_a_rectangle_without_a_trailing_click() {
        for enabled in [false, true] {
            let mut h = Harness::new();
            h.down();
            h.clock += 150;
            h.move_to(125.0, 125.0);
            h.up(125.0, 125.0);
            assert_eq!(
                h.interactions.last(),
                Some(&Interaction::RectangleCommit {
                    ranges: vec![[0, 1], [3, 4]],
                    additive: false,
                })
            );
            // The window never delivered the click that belongs to the drag.
            if !enabled {
                h.enabled = false;
                h.gesture(GestureInput::Reset);
            }
            h.interactions.clear();
            h.clock += 500;
            h.down();
            h.up(15.0, 15.0);
            h.click(1);
            if enabled {
                assert_eq!(h.interactions.last(), Some(&click_interaction()));
                assert!(h.opened.is_empty());
            } else {
                assert_eq!(h.opened, [(1, 0)]);
                assert!(h.interactions.is_empty());
            }
        }
    }

    /// "Escape clears selection during %s" for a held press, a drag and a released
    /// rectangle whose commit is pending.
    #[test]
    fn escape_clears_the_selection_during_a_press_a_drag_and_a_pending_commit() {
        for stage in ["pressed", "dragging", "pending"] {
            let mut h = Harness::new();
            h.down();
            if stage != "pressed" {
                h.clock += 150;
                h.move_to(150.0, 150.0);
            }
            if stage == "pending" {
                h.up(150.0, 150.0);
            }
            if stage != "pressed" {
                assert!(h.gesture.preview().is_some(), "{stage}");
            }
            h.gesture(GestureInput::Escape);
            assert!(h.gesture.preview().is_none(), "{stage}");
            assert!(!h.gesture.dragging(), "{stage}");
            assert!(!h.frame_runs(), "{stage}");
            assert_eq!(h.interactions.last(), Some(&Interaction::Clear), "{stage}");

            h.interactions.clear();
            if stage == "pending" {
                h.gesture(GestureInput::CommitSettled);
            } else {
                h.up(150.0, 150.0);
                h.click(1);
            }
            assert!(h.gesture.preview().is_none(), "{stage}");
            assert!(h.interactions.is_empty(), "{stage}: {:?}", h.interactions);
        }
    }

    /// "selects through unloaded rows during edge scrolling".
    #[test]
    fn selects_through_unloaded_rows_during_edge_scrolling() {
        let mut h = Harness::new();
        h.down();
        h.clock += 150;
        h.move_to(330.0, 399.0);
        h.tick();
        assert!(h.scroll > 0.0);
        h.scroll = 66_000.0;
        h.tick();
        let preview = h.gesture.preview().expect("a preview");
        assert!(preview.ranges[0][1] > 1536, "{:?}", preview.ranges);
        assert_eq!(preview.rectangle.top, 0.0);
        assert!(preview.rectangle.height <= 400.0);
    }

    /// "cancellation while %s" / "cancels on %s" for a held press and a drag. Pointer
    /// cancel, lost capture and window blur are `CancelHeld`; a query change and
    /// selection mode turned off are `Reset`; a new layout, a moved grid origin and
    /// a window resize are `LayoutChanged`.
    #[test]
    fn a_held_press_or_drag_is_cancelled_by_every_cause() {
        for stage in ["pressed", "dragging"] {
            for cause in [
                "pointerCancel",
                "lostPointerCapture",
                "blur",
                "query",
                "layout",
                "origin",
                "resize",
                "disabled",
            ] {
                let mut h = Harness::new();
                h.down();
                h.clock += if stage == "dragging" { 150 } else { 149 };
                h.move_to(150.0, 150.0);
                let input = match cause {
                    "pointerCancel" | "lostPointerCapture" | "blur" => GestureInput::CancelHeld,
                    "query" => GestureInput::Reset,
                    "disabled" => {
                        h.enabled = false;
                        GestureInput::Reset
                    }
                    _ => GestureInput::LayoutChanged,
                };
                h.gesture(input);
                assert_eq!(
                    h.interactions.last(),
                    Some(&Interaction::RectangleCancel),
                    "{stage} {cause}"
                );
                assert!(!h.gesture.dragging(), "{stage} {cause}");
                assert!(!h.frame_runs(), "{stage} {cause}");

                h.clock += 200;
                h.move_to(200.0, 200.0);
                h.up(200.0, 200.0);
                assert!(h.gesture.preview().is_none(), "{stage} {cause}");
                assert_eq!(h.commits(), 0, "{stage} {cause}");
            }
        }
    }

    /// "ignores controls, scrollbar, secondary pointers and normal-mode dragging", the
    /// part the gesture decides: a press at or beyond the viewport's right edge (where
    /// the native scrollbar is) starts nothing, and outside selection mode a drag
    /// selects nothing and the click opens the tile.
    #[test]
    fn ignores_the_scrollbar_edge_and_normal_mode_dragging() {
        let mut h = Harness::new();
        h.down_with(340.0, 20.0, NO_KEYS);
        assert!(h.interactions.is_empty());
        assert!(!h.gesture.held());

        h.enabled = false;
        h.gesture(GestureInput::Reset);
        h.interactions.clear();
        h.down();
        h.clock += 200;
        h.move_to(200.0, 200.0);
        h.up(200.0, 200.0);
        h.click(1);
        assert!(h.interactions.is_empty(), "{:?}", h.interactions);
        assert_eq!(h.opened, [(1, 0)]);
    }
}

/// The gesture and the selection model together, as the gallery wires them.
mod with_the_selection {
    use super::*;

    /// "cancels a failed selection retry on explicit %s" for a query change, selection
    /// mode turned off, a press, a tile click and the owner going away.
    #[test]
    fn cancels_a_failed_selection_retry_on_explicit_input() {
        for cause in ["query", "disabled", "press", "click", "unmount"] {
            let mut h = Harness::with_selection();
            h.select(SelectionInput::Interact(Interaction::RectangleCommit {
                ranges: vec![[0, 1]],
                additive: false,
            }));
            h.answer(|_, _| failed_range());
            let error = h.selection().error().expect("the read failed").to_string();
            assert!(error.contains("failed range"), "{cause}: {error}");

            match cause {
                "query" => {
                    h.select(SelectionInput::Sync(QuerySnapshot {
                        query_epoch: 2,
                        identity_epoch: 0,
                        failure_epoch: 0,
                        session_id: Some(1),
                        total: 3000,
                    }));
                    h.gesture(GestureInput::Reset);
                }
                "disabled" => {
                    h.select(SelectionInput::ToggleMode);
                    h.enabled = false;
                    h.gesture(GestureInput::Reset);
                }
                "press" => h.down(),
                "click" => h.click(1),
                // The gallery that goes away cancels its gesture work
                // (`createGridHandlers`' cleanup sends `rectangle-cancel`); whether the
                // GPUI view does so is checked in media_tagger_gpui's tests.
                _ => h.select(SelectionInput::Interact(Interaction::RectangleCancel)),
            }
            assert_eq!(h.selection().error(), None, "{cause}");
            h.select(SelectionInput::Retry);
            assert_eq!(h.read_count, 1, "{cause}: the retry was dropped");
        }
    }

    /// "keeps a released rectangle read through layout changes while removing its old
    /// preview".
    #[test]
    fn keeps_a_released_rectangle_read_through_layout_changes_while_removing_its_preview() {
        let mut h = Harness::with_selection();
        h.down();
        h.clock += 150;
        h.move_to(125.0, 125.0);
        h.up(125.0, 125.0);
        assert!(h.selection().busy());
        assert!(h.gesture.preview().is_some());

        h.interactions.clear();
        h.gesture(GestureInput::LayoutChanged);
        assert!(h.gesture.preview().is_none());
        assert!(h.interactions.is_empty());
        assert!(h.selection().busy());

        h.answer(|start, end| {
            assert_eq!((start, end), (0, 4), "one read of the envelope");
            Ok(vec![1, 2, 3, 4, 5])
        });
        assert_eq!(&*h.selection().ids(), [1, 2, 4, 5]);
        assert!(!h.selection().busy());
    }

    /// "keeps a released selection read through window blur".
    #[test]
    fn keeps_a_released_selection_read_through_window_blur() {
        let mut h = Harness::with_selection();
        h.down();
        h.clock += 150;
        h.move_to(125.0, 125.0);
        h.up(125.0, 125.0);
        assert!(h.selection().busy());

        h.gesture(GestureInput::CancelHeld);
        assert!(h.selection().busy());
        h.answer(|_, _| Ok(vec![1, 2, 3, 4, 5]));
        assert_eq!(&*h.selection().ids(), [1, 2, 4, 5]);
        assert!(!h.selection().busy());
    }

    /// "keeps a failed selection read and its Retry through window blur".
    #[test]
    fn keeps_a_failed_selection_read_and_its_retry_through_window_blur() {
        let mut h = Harness::with_selection();
        h.select(SelectionInput::Interact(Interaction::RectangleCommit {
            ranges: vec![[0, 1]],
            additive: false,
        }));
        h.answer(|_, _| failed_range());
        assert!(h
            .selection()
            .error()
            .is_some_and(|error| error.to_string().contains("failed range")));

        h.gesture(GestureInput::CancelHeld);
        assert!(h
            .selection()
            .error()
            .is_some_and(|error| error.to_string().contains("failed range")));
        h.select(SelectionInput::Retry);
        assert_eq!(h.read_count, 2);
        h.answer(|_, _| Ok(vec![1, 2]));
        assert_eq!(h.selection().error(), None);
        assert_eq!(&*h.selection().ids(), [1, 2]);
    }

    /// A failed read keeps the selection from before (the Solid model's error case,
    /// seen from the gesture's side).
    #[test]
    fn a_failed_rectangle_read_keeps_the_selection_from_before() {
        let mut h = Harness::with_selection();
        h.click(1);
        assert_eq!(&*h.selection().ids(), [1]);
        h.select(SelectionInput::Interact(Interaction::RectangleCommit {
            ranges: vec![[3, 5]],
            additive: false,
        }));
        h.answer(|_, _| failed_range());
        assert_eq!(&*h.selection().ids(), [1]);
        assert!(matches!(
            h.selection().error(),
            Some(SelectionError::Read(_))
        ));
    }
}

/// "long press in bulk mode".
mod long_press {
    use super::*;

    fn kinds(interactions: &[Interaction]) -> BTreeSet<&'static str> {
        interactions
            .iter()
            .map(|interaction| match interaction {
                Interaction::Click { .. } => "click",
                Interaction::Clear => "clear",
                Interaction::SelectAll => "select-all",
                Interaction::RectangleStart => "rectangle-start",
                Interaction::RectangleCancel => "rectangle-cancel",
                Interaction::RectangleCommit { .. } => "rectangle-commit",
            })
            .collect()
    }

    /// The long-press timer of the last press fires with `asset_id` at the pressed
    /// index.
    fn fire(h: &mut Harness, asset_id: Option<i64>) {
        let (token, _) = *h.long_presses.last().expect("a long-press timer");
        h.gesture(GestureInput::LongPressElapsed { token, asset_id });
    }

    /// "opens the held tile and leaves the selection untouched, including the release
    /// click".
    #[test]
    fn opens_the_held_tile_and_leaves_the_selection_untouched_including_the_release_click() {
        let mut h = Harness::new();
        h.down();
        assert_eq!(h.long_presses.len(), 1);
        assert_eq!(h.long_presses[0].1, LONG_PRESS);
        assert_eq!(LONG_PRESS, Duration::from_millis(500));
        assert!(h.opened.is_empty());
        fire(&mut h, Some(1));
        assert_eq!(h.opened, [(1, 0)]);
        h.up(15.0, 15.0);
        h.click(1);
        // Only gesture bookkeeping reaches the selection: no toggle, commit or clear.
        assert_eq!(
            kinds(&h.interactions),
            BTreeSet::from(["rectangle-start", "rectangle-cancel"])
        );
    }

    /// "keeps a short press as a selection click".
    #[test]
    fn keeps_a_short_press_as_a_selection_click() {
        let mut h = Harness::new();
        h.down();
        h.clock += 400;
        h.up(15.0, 15.0);
        h.click(1);
        fire(&mut h, Some(1));
        assert!(h.opened.is_empty());
        assert!(kinds(&h.interactions).contains("click"));
    }

    /// "does not open after the pointer moved or with a modifier held". Which presses
    /// carry a long press at all (none with a modifier) is the view's to decide; here
    /// a modifier press asks for no timer.
    #[test]
    fn does_not_open_after_the_pointer_moved_or_with_a_modifier_held() {
        let mut h = Harness::new();
        h.down();
        h.move_to(15.0, 21.0);
        fire(&mut h, Some(1));
        h.up(15.0, 21.0);
        for keys in [
            Keys {
                ctrl: true,
                ..NO_KEYS
            },
            Keys {
                shift: true,
                ..NO_KEYS
            },
        ] {
            let timers = h.long_presses.len();
            h.down_with(15.0, 15.0, keys);
            assert_eq!(h.long_presses.len(), timers, "{keys:?}");
            fire(&mut h, Some(1));
            h.up(15.0, 15.0);
        }
        assert!(h.opened.is_empty());
    }

    /// "does not open when scrolling re-indexes the pooled tile during the hold".
    #[test]
    fn does_not_open_when_scrolling_puts_another_asset_under_the_pressed_index() {
        let mut h = Harness::new();
        h.down();
        fire(&mut h, Some(4));
        assert!(h.opened.is_empty());
    }

    /// "does not open outside bulk mode, where a plain click already opens the tile".
    #[test]
    fn does_not_open_outside_bulk_mode() {
        let mut h = Harness::new();
        h.enabled = false;
        h.down();
        assert!(h.long_presses.is_empty());
        assert!(h.opened.is_empty());
    }
}
