//! The scroll state of the gallery grid (`src/features/gallery/createVirtualGrid.ts`).
//!
//! The grid scrolls inside a column the view measures: `lead` pixels above the grid
//! (the gallery's top padding and the load error banner), the rows, and `trail`
//! pixels below them (the status footer). [`VirtualGrid`] keeps one logical offset,
//! relative to the top of the grid, and derives everything else from it: the layout
//! for the measured width and the tile size, the rows to render, the indexes the
//! query model must hold pages for, and the fast-scroll pause.
//!
//! Rules it keeps:
//!
//! - A change of columns or tile size keeps the first visible index at the top
//!   ([`anchor_for_resize`]); above the grid top the scroll position stays where
//!   it is. A change of `lead` alone (the error banner) moves the grid, not the
//!   scroll position.
//! - Scrolling by the user pauses demand when one step covers half a viewport or the
//!   steps of the last [`MOTION_WINDOW`] cover a whole one, and while paused every
//!   further step keeps it paused. [`IDLE_DELAY`] without a step ends the pause.
//!   A paused viewport is published with `paused: true`: the query model remembers
//!   the range and requests no page, and tiles first shown then ask for cached
//!   thumbnails only.
//! - Programmatic jumps and layout anchors are not motion.
//!
//! The model has no clock: scroll inputs carry the time they happened at, and the
//! idle delay is a timer effect answered with [`GridInput::IdleElapsed`].

use std::{collections::VecDeque, time::Duration};

use super::geometry::{
    anchor_for_resize, grid_layout, index_to_row, pool_rows, row_range, row_top, to_library_range,
    GridLayout, LibraryRange, RowRange, GRID_GAP, OVERSCAN_ROWS,
};
use crate::{
    library::Viewport,
    model::{Model, RequestToken, Tokens},
};

/// Without a scroll step for this long, a fast-scroll pause ends.
pub const IDLE_DELAY: Duration = Duration::from_millis(150);
/// The steps of this long are added up to recognise fast scrolling.
pub const MOTION_WINDOW: Duration = Duration::from_millis(100);

/// What the view measured, in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GridGeometry {
    /// The width the tiles fill.
    pub width: f64,
    /// The height of the scrolling viewport.
    pub viewport: f64,
    /// What scrolls above the first row: padding and the load error banner.
    pub lead: f64,
    /// What scrolls below the last row: the status footer.
    pub trail: f64,
}

/// An end of the scroll range (Home and End).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollEdge {
    Start,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GridInput {
    /// The measured geometry, the result's total and the tile size of the slider.
    Layout {
        geometry: GridGeometry,
        count: usize,
        tile_size: f64,
    },
    /// The user scrolled by `delta` pixels (positive: towards the end) at `at`, a
    /// time on any monotonic clock.
    ScrollBy {
        delta: f64,
        at: Duration,
    },
    /// The user moved the scroll position to `offset` at `at` (a position bar).
    ScrollTo {
        offset: f64,
        at: Duration,
    },
    /// The user went to an end of the result at `at`.
    ScrollToEdge {
        edge: ScrollEdge,
        at: Duration,
    },
    /// Shows the row of global index `index` at the top; not motion.
    JumpToIndex(usize),
    /// Pauses demand for a drag that jumps over many rows, until
    /// [`GridInput::ResumeDemand`] or [`IDLE_DELAY`] without another hold.
    HoldDemand,
    ResumeDemand,
    /// The idle timer started with this token fired.
    IdleElapsed(RequestToken),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridEffect {
    /// Send [`GridInput::IdleElapsed`] with this token after `delay`.
    IdleTimer {
        token: RequestToken,
        delay: Duration,
    },
}

#[derive(Debug, Clone, Copy)]
struct MotionSample {
    at: Duration,
    distance: f64,
}

#[derive(Debug, Default)]
pub struct VirtualGrid {
    geometry: GridGeometry,
    tile_size: f64,
    count: usize,
    layout: GridLayout,
    /// Logical pixels from the top of the grid to the top of the viewport.
    offset: f64,
    rows: Option<RowRange>,
    paused: bool,
    motion: VecDeque<MotionSample>,
    tokens: Tokens,
    /// The idle timer that ends the pause; a timer with another token is obsolete.
    idle: Option<RequestToken>,
}

impl VirtualGrid {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn geometry(&self) -> GridGeometry {
        self.geometry
    }

    pub fn layout(&self) -> &GridLayout {
        &self.layout
    }

    /// The rows to render; `None` when there is nothing to show.
    pub fn rows(&self) -> Option<RowRange> {
        self.rows
    }

    /// The global indexes of [`rows`](Self::rows).
    pub fn range(&self) -> Option<LibraryRange> {
        self.rows.map(|rows| to_library_range(&self.layout, &rows))
    }

    /// What the query model is told: the range with the pause flag, or `None` when
    /// no row is rendered.
    pub fn viewport(&self) -> Option<Viewport> {
        self.range().map(|range| Viewport {
            start_index: range.start_index,
            end_index: range.end_index,
            visible_start_index: range.visible_start_index,
            visible_end_index: range.visible_end_index,
            paused: self.paused,
        })
    }

    /// Fast scrolling: page and thumbnail demand wait.
    pub fn paused(&self) -> bool {
        self.paused
    }

    /// Logical pixels from the top of the grid to the top of the viewport; negative
    /// while the margin above the grid is in view.
    pub fn offset(&self) -> f64 {
        self.offset
    }

    /// The smallest offset: the top of what scrolls.
    pub fn min_offset(&self) -> f64 {
        -self.geometry.lead
    }

    /// The largest offset: the footer's end at the bottom of the viewport.
    pub fn max_offset(&self) -> f64 {
        (self.layout.content_height + self.geometry.trail - self.geometry.viewport)
            .max(self.min_offset())
    }

    /// Pixels scrolled from the top of what scrolls (`scrollTop`).
    pub fn scroll_top(&self) -> f64 {
        self.offset + self.geometry.lead
    }

    /// Where the top of `row` is, in pixels below the top of the viewport. A small
    /// number for every rendered row, whatever the offset.
    pub fn row_y(&self, row: usize) -> f64 {
        row_top(&self.layout, row) - self.offset
    }

    /// Where the top of what scrolls above the grid is, below the top of the viewport.
    pub fn lead_y(&self) -> f64 {
        -self.geometry.lead - self.offset
    }

    /// Where the top of what follows the grid is, below the top of the viewport. A
    /// result shorter than the viewport keeps it at the viewport's bottom
    /// (`.workspace-body { min-height: 100% }`).
    pub fn trail_y(&self) -> f64 {
        (self.layout.content_height - self.offset).max(self.geometry.viewport - self.geometry.trail)
    }

    /// The logical offset of the row holding `index`.
    pub fn offset_of_index(&self, index: usize) -> f64 {
        row_top(&self.layout, index_to_row(&self.layout, index))
    }

    /// Most rows the viewport can render at once.
    pub fn pool_rows(&self) -> usize {
        pool_rows(&self.layout, self.geometry.viewport, OVERSCAN_ROWS)
    }

    fn clamp(&self, offset: f64) -> f64 {
        // NaN never reaches the state: it falls back to the top.
        if offset.is_nan() {
            return self.min_offset();
        }
        offset.clamp(self.min_offset(), self.max_offset())
    }

    fn publish(&mut self) {
        self.rows = row_range(
            &self.layout,
            self.offset,
            self.geometry.viewport,
            OVERSCAN_ROWS,
        );
    }

    fn idle_timer(&mut self) -> GridEffect {
        let token = self.tokens.issue();
        self.idle = Some(token);
        GridEffect::IdleTimer {
            token,
            delay: IDLE_DELAY,
        }
    }

    /// Moves to `target` without counting it as motion.
    fn jump(&mut self, target: f64) {
        self.offset = self.clamp(target);
        self.motion.clear();
        self.publish();
    }

    /// Moves to `target` as a step of the user's scrolling.
    fn scroll(&mut self, target: f64, at: Duration) -> Vec<GridEffect> {
        let next = self.clamp(target);
        let distance = (next - self.offset).abs();
        self.offset = next;
        self.publish();
        if distance == 0.0 {
            return Vec::new();
        }
        while self
            .motion
            .front()
            .is_some_and(|sample| at.saturating_sub(sample.at) > MOTION_WINDOW)
        {
            self.motion.pop_front();
        }
        self.motion.push_back(MotionSample { at, distance });
        let viewport = self.geometry.viewport;
        let travelled: f64 = self.motion.iter().map(|sample| sample.distance).sum();
        let fast = viewport > 0.0 && (distance >= viewport / 2.0 || travelled >= viewport);
        if fast || self.idle.is_some() {
            self.paused = true;
            vec![self.idle_timer()]
        } else {
            Vec::new()
        }
    }

    fn on_layout(&mut self, geometry: GridGeometry, count: usize, tile_size: f64) {
        let next = grid_layout(
            geometry.width,
            tile_size,
            GRID_GAP,
            if geometry.width > 0.0 { count } else { 0 },
        );
        let prior = self.layout;
        // The scroll position (`scrollTop`) is what stays when only the margin
        // above the grid changes.
        let mut offset = self.scroll_top() - geometry.lead;
        if (prior.columns != next.columns || prior.size != next.size) && self.offset > 0.0 {
            offset = anchor_for_resize(&prior, &next, self.offset);
        }
        self.geometry = geometry;
        self.tile_size = tile_size;
        self.count = count;
        self.layout = next;
        self.offset = self.clamp(offset);
        // A layout anchor is not motion, and it does not end a pause either.
        self.publish();
    }

    fn end_pause(&mut self) {
        self.idle = None;
        self.motion.clear();
        self.paused = false;
    }
}

impl Model for VirtualGrid {
    type Input = GridInput;
    type Effect = GridEffect;

    fn handle(&mut self, input: GridInput) -> Vec<GridEffect> {
        match input {
            GridInput::Layout {
                geometry,
                count,
                tile_size,
            } => {
                self.on_layout(geometry, count, tile_size);
                Vec::new()
            }
            GridInput::ScrollBy { delta, at } => {
                if delta == 0.0 || delta.is_nan() {
                    return Vec::new();
                }
                self.scroll(self.offset + delta, at)
            }
            GridInput::ScrollTo { offset, at } => self.scroll(offset, at),
            GridInput::ScrollToEdge { edge, at } => {
                let target = match edge {
                    ScrollEdge::Start => self.min_offset(),
                    ScrollEdge::End => self.max_offset(),
                };
                self.scroll(target, at)
            }
            GridInput::JumpToIndex(index) => {
                self.jump(self.offset_of_index(index));
                Vec::new()
            }
            GridInput::HoldDemand => {
                self.paused = true;
                vec![self.idle_timer()]
            }
            GridInput::ResumeDemand => {
                if self.idle.is_some() || self.paused {
                    self.end_pause();
                }
                Vec::new()
            }
            GridInput::IdleElapsed(token) => {
                if self.idle == Some(token) {
                    self.end_pause();
                }
                Vec::new()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{GridEffect, GridGeometry, GridInput, ScrollEdge, VirtualGrid, IDLE_DELAY};
    use crate::{
        backend::DEFAULT_QUERY_PAGE_SIZE,
        library::Viewport,
        model::{Model, RequestToken},
    };

    const GEOMETRY: GridGeometry = GridGeometry {
        width: 988.0,
        viewport: 600.0,
        lead: 20.0,
        trail: 137.0,
    };

    fn grid(count: usize) -> VirtualGrid {
        let mut grid = VirtualGrid::new();
        grid.handle(GridInput::Layout {
            geometry: GEOMETRY,
            count,
            tile_size: 188.0,
        });
        grid
    }

    fn ms(value: u64) -> Duration {
        Duration::from_millis(value)
    }

    fn idle_token(effects: &[GridEffect]) -> RequestToken {
        match effects {
            [GridEffect::IdleTimer { token, delay }] => {
                assert_eq!(*delay, IDLE_DELAY);
                *token
            }
            other => panic!("expected one idle timer, got {other:?}"),
        }
    }

    /// The pages of `DEFAULT_QUERY_PAGE_SIZE` a viewport's range touches.
    fn pages(viewport: Viewport) -> Vec<usize> {
        let first = viewport.start_index / DEFAULT_QUERY_PAGE_SIZE;
        let last = viewport.end_index / DEFAULT_QUERY_PAGE_SIZE;
        (first..=last)
            .map(|page| page * DEFAULT_QUERY_PAGE_SIZE)
            .collect()
    }

    #[test]
    fn the_first_layout_starts_at_the_top_of_the_margin() {
        let grid = grid(10_000);

        assert_eq!(grid.offset(), -20.0);
        assert_eq!(grid.scroll_top(), 0.0);
        assert_eq!(grid.lead_y(), 0.0);
        assert_eq!(grid.row_y(0), 20.0);
        let viewport = grid.viewport().expect("rows are rendered");
        assert_eq!(
            viewport,
            Viewport {
                start_index: 0,
                end_index: 34,
                visible_start_index: 0,
                visible_end_index: 14,
                paused: false,
            }
        );
        assert_eq!(pages(viewport), [0]);
    }

    #[test]
    fn the_visible_range_names_the_pages_to_request_at_any_depth() {
        let mut grid = grid(1_000_000);

        // Row 100 000 of 200 000: index 500 000, in the page at offset 499 968.
        grid.handle(GridInput::JumpToIndex(500_000));
        let viewport = grid.viewport().expect("rows");
        assert_eq!(viewport.visible_start_index, 500_000);
        assert_eq!(viewport.start_index, 499_980);
        assert_eq!(viewport.end_index, 500_034);
        assert_eq!(pages(viewport), [499_968]);
        assert!(!viewport.paused);

        // A range across a page boundary names both pages and nothing else.
        grid.handle(GridInput::JumpToIndex(500_090));
        let viewport = grid.viewport().expect("rows");
        assert_eq!(
            (viewport.start_index, viewport.end_index),
            (500_070, 500_124)
        );
        assert_eq!(pages(viewport), [499_968, 500_096]);

        // The end of the result: the last page only.
        grid.handle(GridInput::ScrollToEdge {
            edge: ScrollEdge::End,
            at: ms(0),
        });
        let viewport = grid.viewport().expect("rows");
        assert_eq!(viewport.end_index, 999_999);
        assert_eq!(pages(viewport), [999_936]);
    }

    #[test]
    fn the_offset_is_exact_at_the_millionth_slot() {
        let mut grid = VirtualGrid::new();
        grid.handle(GridInput::Layout {
            geometry: GridGeometry {
                width: 588.0,
                ..GEOMETRY
            },
            count: 1_000_000,
            tile_size: 340.0,
        });
        assert_eq!(grid.layout().columns, 1);

        grid.handle(GridInput::JumpToIndex(999_990));
        assert_eq!(grid.offset(), 999_990.0 * 600.0);
        // One wheel step of a third of a pixel is kept, far beyond f32 precision.
        grid.handle(GridInput::ScrollBy {
            delta: 0.25,
            at: ms(0),
        });
        assert_eq!(grid.offset(), 999_990.0 * 600.0 + 0.25);
        assert_eq!(grid.row_y(999_990), -0.25);
        assert_eq!(grid.row_y(999_991), 599.75);
        assert_eq!(grid.rows().expect("rows").visible_first, 999_990);
    }

    #[test]
    fn scrolling_is_clamped_to_the_margin_and_the_footer() {
        let mut grid = grid(10_000);
        let content = grid.layout().content_height;

        grid.handle(GridInput::ScrollBy {
            delta: -500.0,
            at: ms(0),
        });
        assert_eq!(grid.offset(), -20.0);

        grid.handle(GridInput::ScrollToEdge {
            edge: ScrollEdge::End,
            at: ms(10),
        });
        assert_eq!(grid.offset(), content + 137.0 - 600.0);
        assert_eq!(grid.trail_y(), 600.0 - 137.0);

        grid.handle(GridInput::ScrollTo {
            offset: f64::NAN,
            at: ms(500),
        });
        assert_eq!(grid.offset(), -20.0);
    }

    #[test]
    fn a_short_result_does_not_scroll_and_keeps_the_footer_at_the_bottom() {
        let mut grid = grid(7);

        assert_eq!(grid.max_offset(), grid.min_offset());
        grid.handle(GridInput::ScrollBy {
            delta: 300.0,
            at: ms(0),
        });
        assert_eq!(grid.offset(), -20.0);
        assert_eq!(grid.trail_y(), 600.0 - 137.0);
        assert!(!grid.paused());
    }

    #[test]
    fn a_resize_keeps_the_first_visible_index_and_a_margin_change_keeps_the_position() {
        let mut grid = grid(10_000);
        grid.handle(GridInput::ScrollBy {
            delta: 2_020.0,
            at: ms(0),
        });
        assert_eq!(grid.offset(), 2_000.0);
        assert_eq!(grid.range().expect("rows").visible_start_index, 50);

        // Three columns: index 50 is the first of row 16.
        grid.handle(GridInput::Layout {
            geometry: GridGeometry {
                width: 588.0,
                ..GEOMETRY
            },
            count: 10_000,
            tile_size: 188.0,
        });
        assert_eq!(grid.layout().columns, 3);
        assert_eq!(grid.offset(), 16.0 * 200.0);

        // The error banner adds 50 px above the grid: the scroll position stays.
        let scroll_top = grid.scroll_top();
        grid.handle(GridInput::Layout {
            geometry: GridGeometry {
                width: 588.0,
                lead: 70.0,
                ..GEOMETRY
            },
            count: 10_000,
            tile_size: 188.0,
        });
        assert_eq!(grid.scroll_top(), scroll_top);
        assert_eq!(grid.offset(), 16.0 * 200.0 - 50.0);
    }

    #[test]
    fn a_shrinking_result_clamps_the_offset_and_an_empty_one_renders_nothing() {
        let mut grid = grid(10_000);
        grid.handle(GridInput::JumpToIndex(9_000));

        grid.handle(GridInput::Layout {
            geometry: GEOMETRY,
            count: 12,
            tile_size: 188.0,
        });
        assert_eq!(grid.layout().rows, 3);
        assert_eq!(grid.offset(), 588.0 + 137.0 - 600.0);

        grid.handle(GridInput::Layout {
            geometry: GEOMETRY,
            count: 0,
            tile_size: 188.0,
        });
        assert_eq!(grid.rows(), None);
        assert_eq!(grid.viewport(), None);
    }

    #[test]
    fn a_half_viewport_jump_pauses_until_the_idle_delay_passes() {
        let mut grid = grid(10_000);

        let effects = grid.handle(GridInput::ScrollBy {
            delta: 300.0,
            at: ms(0),
        });
        let first = idle_token(&effects);
        assert!(grid.paused());
        let paused = grid.viewport().expect("rows");
        assert!(paused.paused);

        // Another step while paused restarts the timer, however small.
        let effects = grid.handle(GridInput::ScrollBy {
            delta: 1.0,
            at: ms(40),
        });
        let second = idle_token(&effects);
        let paused = grid.viewport().expect("rows");
        grid.handle(GridInput::IdleElapsed(first));
        assert!(grid.paused(), "an obsolete timer ends nothing");

        // The same range is published again, now without the pause.
        grid.handle(GridInput::IdleElapsed(second));
        assert!(!grid.paused());
        assert_eq!(
            grid.viewport(),
            Some(Viewport {
                paused: false,
                ..paused
            })
        );
    }

    #[test]
    fn slow_scrolling_stays_active_and_distance_adds_up_within_the_window() {
        let mut grid = grid(10_000);

        // 250 px every 60 ms: never a whole viewport within 100 ms.
        for step in 0..6 {
            let effects = grid.handle(GridInput::ScrollBy {
                delta: 250.0,
                at: ms(step * 60),
            });
            assert!(effects.is_empty());
            assert!(!grid.paused());
        }

        // Direction changes count as distance: 3 x 250 px within 100 ms.
        let mut effects = Vec::new();
        for (step, delta) in [250.0, -250.0, 250.0].into_iter().enumerate() {
            effects = grid.handle(GridInput::ScrollBy {
                delta,
                at: ms(1_000 + step as u64 * 30),
            });
        }
        idle_token(&effects);
        assert!(grid.paused());
    }

    #[test]
    fn jumps_and_layout_anchors_are_not_motion() {
        let mut grid = grid(10_000);

        assert!(grid.handle(GridInput::JumpToIndex(5_000)).is_empty());
        assert!(!grid.paused());
        assert_eq!(grid.offset(), 1_000.0 * 200.0);

        grid.handle(GridInput::Layout {
            geometry: GEOMETRY,
            count: 10_000,
            tile_size: 340.0,
        });
        assert!(!grid.paused());
        // A small step after a jump is slow scrolling again.
        let effects = grid.handle(GridInput::ScrollBy {
            delta: 10.0,
            at: ms(5),
        });
        assert!(effects.is_empty());
    }

    #[test]
    fn a_hold_pauses_demand_until_release_or_rest() {
        let mut grid = grid(10_000);

        let token = idle_token(&grid.handle(GridInput::HoldDemand));
        assert!(grid.paused());
        grid.handle(GridInput::ResumeDemand);
        assert!(!grid.paused());
        grid.handle(GridInput::IdleElapsed(token));
        assert!(!grid.paused());

        let token = idle_token(&grid.handle(GridInput::HoldDemand));
        grid.handle(GridInput::IdleElapsed(token));
        assert!(!grid.paused());
        assert!(grid.handle(GridInput::ResumeDemand).is_empty());
    }
}
