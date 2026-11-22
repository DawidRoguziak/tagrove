//! The order board (`GroupOrderBoard.tsx`) without a GUI: its grid of fixed-height
//! rows, which rows are drawn, where a drop lands, where the insertion line lies, what
//! the arrow keys on a grip do, and the pointer drag as a state machine.
//!
//! The board does not own the order: it reports the next one
//! ([`DragEffect::Reorder`], [`BoardLayout::key_move`] with
//! [`reorder_asset_ids_by_drop`]) and its owner keeps the draft.

use std::ops::Range;

use super::{group_order::reorder_asset_ids_by_drop, layout::board as measures};

/// The grid of a board `width` wide with `count` tiles.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoardLayout {
    pub width: f32,
    pub columns: usize,
    pub tile_width: f32,
    /// The height of a tile's preview.
    pub preview_height: f32,
    /// The height of a row of the list; the tiles of a row are a little shorter.
    pub row_height: f32,
    pub count: usize,
}

/// A rectangle in the board's content, from its top-left corner.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TileRect {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

/// The line that shows where a dragged tile will land.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InsertionLine {
    pub left: f32,
    pub top: f32,
    pub height: f32,
}

/// The arrow keys on a tile's grip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoardKey {
    Left,
    Right,
    Up,
    Down,
}

impl BoardLayout {
    /// As many columns as fit with tiles at least `min_tile_width` wide and
    /// [`measures::GAP`] between them; never fewer than one.
    pub fn new(width: f32, min_tile_width: f32, count: usize) -> Self {
        let gap = measures::GAP;
        let columns = (((width + gap) / (min_tile_width + gap)).floor() as usize).max(1);
        let tile_width = ((width - (columns - 1) as f32 * gap) / columns as f32).max(1.0);
        let preview_height = (tile_width * measures::PREVIEW_RATIO).round();
        Self {
            width,
            columns,
            tile_width,
            preview_height,
            row_height: preview_height + measures::ROW_EXTRA,
            count,
        }
    }

    pub fn row_count(&self) -> usize {
        self.count.div_ceil(self.columns)
    }

    /// The height of the whole list.
    pub fn total_height(&self) -> f32 {
        self.row_count() as f32 * self.row_height
    }

    /// The height of a tile: its preview, the caption (whose `h-11` holds its own
    /// line, the box being a border box) and the border.
    pub fn tile_height(&self) -> f32 {
        self.preview_height + measures::CAPTION_HEIGHT + 2.0 * measures::BORDER
    }

    pub fn row_of(&self, index: usize) -> usize {
        index / self.columns
    }

    pub fn row_top(&self, row: usize) -> f32 {
        row as f32 * self.row_height
    }

    /// The indexes of the tiles of `row`.
    pub fn row_indexes(&self, row: usize) -> Range<usize> {
        let first = (row * self.columns).min(self.count);
        first..(first + self.columns).min(self.count)
    }

    /// Where the tile at `index` lies in the content.
    pub fn tile_rect(&self, index: usize) -> TileRect {
        TileRect {
            left: (index % self.columns) as f32 * (self.tile_width + measures::GAP),
            top: self.row_top(self.row_of(index)),
            width: self.tile_width,
            height: self.tile_height(),
        }
    }

    /// The rows to draw for a list scrolled to `scroll_top` in a viewport
    /// `viewport_height` tall: the ones that reach into it and
    /// [`measures::OVERSCAN`] rows on each side.
    pub fn visible_rows(&self, scroll_top: f32, viewport_height: f32) -> Range<usize> {
        let rows = self.row_count();
        if rows == 0 {
            return 0..0;
        }
        let top = scroll_top.max(0.0);
        let bottom = top + viewport_height.max(1.0) - 1.0;
        let row_at = |position: f32| ((position / self.row_height).floor() as usize).min(rows - 1);
        let first = row_at(top).saturating_sub(measures::OVERSCAN);
        let last = (row_at(bottom) + measures::OVERSCAN).min(rows - 1);
        first..last + 1
    }

    /// The tile a drop at the content point `(x, y)` lands on, among the drawn
    /// `rows`: the tile under the point, else the nearest one, so the gaps between
    /// rows and the space after the last tile are valid drop areas.
    pub fn drop_target(&self, x: f32, y: f32, rows: Range<usize>) -> Option<usize> {
        let mut nearest = None;
        let mut shortest = f32::INFINITY;
        for index in rows.flat_map(|row| self.row_indexes(row)) {
            let rect = self.tile_rect(index);
            let dx = (rect.left - x).max(0.0).max(x - (rect.left + rect.width));
            let dy = (rect.top - y).max(0.0).max(y - (rect.top + rect.height));
            let distance = dx.hypot(dy);
            if distance < shortest {
                shortest = distance;
                nearest = Some(index);
            }
        }
        nearest
    }

    /// The insertion line for the tile at `dragged` over the tile at `target`:
    /// after a later target, before an earlier one. None for a tile over itself.
    pub fn insertion_line(&self, dragged: usize, target: usize) -> Option<InsertionLine> {
        if dragged == target || dragged >= self.count || target >= self.count {
            return None;
        }
        let rect = self.tile_rect(target);
        let beside = if dragged < target {
            self.tile_width + measures::LINE_AFTER
        } else {
            -measures::LINE_BEFORE
        };
        Some(InsertionLine {
            left: (rect.left + beside)
                .min(self.width - measures::LINE_WIDTH)
                .max(0.0),
            top: rect.top + measures::LINE_TOP,
            height: self.row_height - measures::LINE_SHORTER,
        })
    }

    /// The index an arrow key moves the tile at `index` to: one tile sideways, one
    /// row up or down. None at the edges of the order.
    pub fn key_target(&self, index: usize, key: BoardKey) -> Option<usize> {
        let target = match key {
            BoardKey::Left => index.checked_sub(1)?,
            BoardKey::Right => index + 1,
            BoardKey::Up => index.checked_sub(self.columns)?,
            BoardKey::Down => index + self.columns,
        };
        (target < self.count).then_some(target)
    }

    /// An arrow key on the grip of `id`: the next order and the row the tile is in
    /// afterwards, which the board scrolls into view. None when the key moves nothing.
    pub fn key_move(&self, order: &[i64], id: i64, key: BoardKey) -> Option<(Vec<i64>, usize)> {
        let index = order.iter().position(|other| *other == id)?;
        let target = self.key_target(index, key)?;
        let next = reorder_asset_ids_by_drop(order, id, *order.get(target)?);
        Some((next, self.row_of(target)))
    }

    /// The scroll position that shows `row` with the least movement
    /// (`scrollToIndex` with `auto`).
    pub fn reveal_row(&self, row: usize, scroll_top: f32, viewport_height: f32) -> f32 {
        if row >= self.row_count() {
            return scroll_top;
        }
        let start = self.row_top(row);
        let end = start + self.row_height;
        if start < scroll_top {
            start
        } else if end > scroll_top + viewport_height {
            end - viewport_height
        } else {
            scroll_top
        }
    }
}

/// How far the board scrolls in one frame while a tile is dragged at `y` between the
/// board's `top` and `bottom` edges: faster the nearer the pointer is to an edge.
pub fn edge_scroll_speed(y: f32, top: f32, bottom: f32) -> f32 {
    let (edge, speed) = (measures::EDGE, measures::EDGE_SPEED);
    if y < top || y > bottom {
        0.0
    } else if y < top + edge {
        -speed * (1.0 - (y - top) / edge)
    } else if y > bottom - edge {
        speed * (1.0 - (bottom - y) / edge)
    } else {
        0.0
    }
}

/// What a drag asks of the board.
#[derive(Debug, Clone, PartialEq)]
pub enum DragEffect {
    /// The drop: `dragged` goes to the place of `target`.
    Reorder { dragged: i64, target: i64 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Held {
    id: i64,
    start: (f32, f32),
    position: (f32, f32),
    active: bool,
    target: Option<i64>,
}

/// The pointer drag of a tile: a press, a move of at least
/// [`measures::DRAG_THRESHOLD`] pixels, and a release over a tile.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct BoardDrag {
    held: Option<Held>,
}

impl BoardDrag {
    pub fn new() -> Self {
        Self::default()
    }

    /// A press is held, dragging or not yet.
    pub fn held(&self) -> bool {
        self.held.is_some()
    }

    /// The tile being dragged, once the press moved far enough.
    pub fn dragged(&self) -> Option<i64> {
        self.held.filter(|held| held.active).map(|held| held.id)
    }

    /// The tile the dragged one is over.
    pub fn target(&self) -> Option<i64> {
        self.held
            .filter(|held| held.active)
            .and_then(|held| held.target)
    }

    /// The pointer's last position, while a tile is dragged.
    pub fn position(&self) -> Option<(f32, f32)> {
        self.held
            .filter(|held| held.active)
            .map(|held| held.position)
    }

    /// A primary press on the tile of `id`. Refused while the board is locked or
    /// another press is held. Returns whether the press was taken.
    pub fn press(&mut self, id: i64, x: f32, y: f32, board_locked: bool) -> bool {
        if board_locked || self.held.is_some() {
            return false;
        }
        self.held = Some(Held {
            id,
            start: (x, y),
            position: (x, y),
            active: false,
            target: None,
        });
        true
    }

    /// The pointer moved to `(x, y)`. Returns whether a tile is dragged now.
    pub fn moved(&mut self, x: f32, y: f32) -> bool {
        let Some(held) = &mut self.held else {
            return false;
        };
        held.position = (x, y);
        if !held.active && (x - held.start.0).hypot(y - held.start.1) >= measures::DRAG_THRESHOLD {
            held.active = true;
        }
        held.active
    }

    /// The tile under the dragged one, as the board worked it out for this frame.
    pub fn set_target(&mut self, target: Option<i64>) {
        if let Some(held) = self.held.as_mut().filter(|held| held.active) {
            held.target = target;
        }
    }

    /// The button went up at `(x, y)` over the tile `target`. A press that never
    /// became a drag, or a release over no tile, changes nothing.
    pub fn release(&mut self, x: f32, y: f32, target: Option<i64>) -> Option<DragEffect> {
        self.moved(x, y);
        let held = self.held.take()?;
        let target = target?;
        held.active.then_some(DragEffect::Reorder {
            dragged: held.id,
            target,
        })
    }

    /// The press ended without a drop: the pointer was cancelled, the window lost
    /// its focus, or the board went inactive.
    pub fn cancel(&mut self) {
        self.held = None;
    }
}

#[cfg(test)]
mod tests {
    use super::{edge_scroll_speed, BoardDrag, BoardKey, BoardLayout, DragEffect};

    #[test]
    fn the_grid_fits_as_many_columns_as_the_smallest_tile_allows() {
        let layout = BoardLayout::new(960.0, 280.0, 10);
        assert_eq!(layout.columns, 3);
        assert_eq!(layout.tile_width, 312.0);
        assert_eq!(layout.preview_height, 206.0);
        assert_eq!(layout.row_height, 262.0);
        // The preview, the 44px caption with its line inside, and the 1px border: 10px
        // are left under the tile before the next row.
        assert_eq!(layout.tile_height(), 206.0 + 44.0 + 2.0);
        assert_eq!(layout.tile_rect(4).height, 252.0);
        assert_eq!(layout.row_count(), 4);
        assert_eq!(layout.total_height(), 4.0 * 262.0);
        assert_eq!(layout.row_indexes(3), 9..10);
        assert_eq!(BoardLayout::new(100.0, 150.0, 3).columns, 1);
        assert_eq!(BoardLayout::new(736.0, 150.0, 3).columns, 4);
    }

    #[test]
    fn only_the_rows_near_the_viewport_are_drawn() {
        let layout = BoardLayout::new(960.0, 280.0, 3000);
        assert_eq!(layout.visible_rows(0.0, 560.0), 0..5);
        let far = layout.visible_rows(262.0 * 500.0, 560.0);
        assert_eq!(far, 498..505);
        assert_eq!(
            BoardLayout::new(960.0, 280.0, 0).visible_rows(0.0, 560.0),
            0..0
        );
        // Scrolled past the end: the last rows.
        assert_eq!(layout.visible_rows(1.0e9, 560.0), 997..1000);
    }

    #[test]
    fn a_drop_lands_on_the_tile_under_the_pointer_or_the_nearest_one() {
        let layout = BoardLayout::new(960.0, 280.0, 5);
        let rows = layout.visible_rows(0.0, 560.0);
        assert_eq!(layout.drop_target(10.0, 10.0, rows.clone()), Some(0));
        assert_eq!(layout.drop_target(400.0, 10.0, rows.clone()), Some(1));
        // The gap between the first two columns: the nearer tile.
        assert_eq!(layout.drop_target(314.0, 10.0, rows.clone()), Some(0));
        // The space after the last tile of the second row.
        assert_eq!(layout.drop_target(700.0, 400.0, rows.clone()), Some(4));
        assert_eq!(layout.drop_target(10.0, 10.0, 0..0), None);
    }

    #[test]
    fn the_insertion_line_lies_after_a_later_target_and_before_an_earlier_one() {
        let layout = BoardLayout::new(960.0, 280.0, 6);
        let after = layout.insertion_line(0, 1).unwrap();
        assert_eq!(after.left, 324.0 + 312.0 + 4.0);
        assert_eq!((after.top, after.height), (2.0, 246.0));
        let before = layout.insertion_line(4, 3).unwrap();
        // Before the first column: clamped to the board's edge.
        assert_eq!(before.left, 0.0);
        assert_eq!(before.top, 264.0);
        // After the last column: clamped inside the board.
        assert_eq!(layout.insertion_line(0, 2).unwrap().left, 956.0);
        assert_eq!(layout.insertion_line(2, 2), None);
    }

    #[test]
    fn arrow_keys_move_a_tile_sideways_and_by_rows() {
        let layout = BoardLayout::new(960.0, 280.0, 7);
        let order = [1, 2, 3, 4, 5, 6, 7];
        assert_eq!(
            layout.key_move(&order, 1, BoardKey::Right),
            Some((vec![2, 1, 3, 4, 5, 6, 7], 0))
        );
        assert_eq!(
            layout.key_move(&order, 1, BoardKey::Down),
            Some((vec![2, 3, 4, 1, 5, 6, 7], 1))
        );
        assert_eq!(
            layout.key_move(&order, 5, BoardKey::Up),
            Some((vec![1, 5, 2, 3, 4, 6, 7], 0))
        );
        assert_eq!(layout.key_move(&order, 1, BoardKey::Left), None);
        assert_eq!(layout.key_move(&order, 1, BoardKey::Up), None);
        assert_eq!(layout.key_move(&order, 7, BoardKey::Right), None);
        // No tile a row below the fifth: the key does nothing.
        assert_eq!(layout.key_move(&order, 5, BoardKey::Down), None);
        assert_eq!(layout.key_move(&order, 9, BoardKey::Right), None);
    }

    #[test]
    fn a_moved_row_is_scrolled_into_view_with_the_least_movement() {
        let layout = BoardLayout::new(960.0, 280.0, 300);
        assert_eq!(layout.reveal_row(0, 500.0, 560.0), 0.0);
        assert_eq!(layout.reveal_row(2, 0.0, 560.0), 3.0 * 262.0 - 560.0);
        assert_eq!(layout.reveal_row(1, 100.0, 560.0), 100.0);
        assert_eq!(layout.reveal_row(1000, 100.0, 560.0), 100.0);
    }

    #[test]
    fn the_board_scrolls_faster_the_nearer_the_pointer_is_to_an_edge() {
        assert_eq!(edge_scroll_speed(100.0, 100.0, 600.0), -18.0);
        assert_eq!(edge_scroll_speed(128.0, 100.0, 600.0), -9.0);
        assert_eq!(edge_scroll_speed(300.0, 100.0, 600.0), 0.0);
        assert_eq!(edge_scroll_speed(572.0, 100.0, 600.0), 9.0);
        assert_eq!(edge_scroll_speed(600.0, 100.0, 600.0), 18.0);
        assert_eq!(edge_scroll_speed(50.0, 100.0, 600.0), 0.0);
    }

    #[test]
    fn a_press_becomes_a_drag_after_five_pixels_and_drops_on_release() {
        let mut drag = BoardDrag::new();
        assert!(!drag.press(1, 0.0, 0.0, true));
        assert!(drag.press(1, 10.0, 10.0, false));
        assert!(!drag.press(2, 0.0, 0.0, false));
        assert!(!drag.moved(13.0, 13.0));
        assert_eq!(drag.dragged(), None);
        assert!(drag.moved(13.0, 14.0));
        assert_eq!(drag.dragged(), Some(1));
        drag.set_target(Some(3));
        assert_eq!(drag.target(), Some(3));
        assert_eq!(drag.position(), Some((13.0, 14.0)));
        assert_eq!(
            drag.release(20.0, 20.0, Some(3)),
            Some(DragEffect::Reorder {
                dragged: 1,
                target: 3
            })
        );
        assert!(!drag.held());
    }

    #[test]
    fn a_click_a_release_over_nothing_and_a_cancel_drop_nothing() {
        let mut drag = BoardDrag::new();
        drag.press(1, 10.0, 10.0, false);
        assert_eq!(drag.release(11.0, 11.0, Some(2)), None);
        // The release itself may be the move that crosses the threshold.
        drag.press(1, 10.0, 10.0, false);
        assert_eq!(
            drag.release(30.0, 10.0, Some(2)),
            Some(DragEffect::Reorder {
                dragged: 1,
                target: 2
            })
        );
        drag.press(1, 10.0, 10.0, false);
        drag.moved(40.0, 40.0);
        assert_eq!(drag.release(40.0, 40.0, None), None);
        drag.press(1, 10.0, 10.0, false);
        drag.moved(40.0, 40.0);
        drag.cancel();
        assert!(!drag.held() && drag.dragged().is_none());
    }
}
