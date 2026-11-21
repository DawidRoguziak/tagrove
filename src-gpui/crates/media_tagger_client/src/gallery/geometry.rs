//! The square-tile grid (`src/features/gallery/gridGeometry.ts`).
//!
//! Every function is pure and O(1): rows have one height, so the row of an offset and
//! the offset of an index are divisions, at any result size.

/// The gap between tiles, horizontally and vertically.
pub const GRID_GAP: f64 = 12.0;
/// Rows rendered above and below the rows in view.
pub const OVERSCAN_ROWS: usize = 4;

/// How `count` square tiles fill a width.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridLayout {
    pub columns: usize,
    /// The side of a tile.
    pub size: f64,
    pub gap: f64,
    /// The distance between the tops of two rows: `size + gap`.
    pub stride: f64,
    pub count: usize,
    pub rows: usize,
    /// The height of all rows, without a gap after the last one.
    pub content_height: f64,
}

impl Default for GridLayout {
    /// The layout before the first measurement: nothing to show.
    fn default() -> Self {
        grid_layout(0.0, 1.0, GRID_GAP, 0)
    }
}

/// Rows of a scroll position, inclusive on both ends. `first..=last` are rendered
/// (the rows in view plus overscan), `visible_first..=visible_last` are in view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowRange {
    pub first: usize,
    pub last: usize,
    pub visible_first: usize,
    pub visible_last: usize,
}

impl RowRange {
    /// How many rows are rendered.
    pub fn len(&self) -> usize {
        self.last - self.first + 1
    }

    /// A range always holds at least one row; an empty grid has no range.
    pub fn is_empty(&self) -> bool {
        false
    }
}

/// Global indexes of a row range, inclusive on both ends (`LibraryRange`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LibraryRange {
    pub start_index: usize,
    pub end_index: usize,
    pub visible_start_index: usize,
    pub visible_end_index: usize,
}

impl LibraryRange {
    /// Whether `index` is in a row in view (not only in the overscan).
    pub fn is_visible(&self, index: usize) -> bool {
        (self.visible_start_index..=self.visible_end_index).contains(&index)
    }
}

/// Square tiles fill the width: as many columns as fit at `tile_min`, then stretched.
pub fn grid_layout(width: f64, tile_min: f64, gap: f64, count: usize) -> GridLayout {
    let tile_min = tile_min.max(1.0);
    let columns = (((width + gap) / (tile_min + gap)).floor() as usize).max(1);
    let size = ((width.max(tile_min) - gap * (columns - 1) as f64) / columns as f64)
        .floor()
        .max(1.0);
    let stride = size + gap;
    let rows = count.div_ceil(columns);
    GridLayout {
        columns,
        size,
        gap,
        stride,
        count,
        rows,
        content_height: if rows > 0 {
            rows as f64 * stride - gap
        } else {
            0.0
        },
    }
}

/// The offset of the top of `row`.
pub fn row_top(layout: &GridLayout, row: usize) -> f64 {
    row as f64 * layout.stride
}

/// The row that holds global index `index`.
pub fn index_to_row(layout: &GridLayout, index: usize) -> usize {
    index / layout.columns
}

/// Rows intersecting `[offset, offset + viewport)` in grid coordinates, plus
/// `overscan` rows on both sides. `offset` may be negative while the margin above
/// the grid is in view. `None`: the grid has no rows.
pub fn row_range(
    layout: &GridLayout,
    offset: f64,
    viewport: f64,
    overscan: usize,
) -> Option<RowRange> {
    if layout.rows == 0 {
        return None;
    }
    let last = layout.rows - 1;
    let row_of = |position: f64| -> usize {
        // Saturating: an offset beyond the end, or a negative one, stays a row.
        (position / layout.stride).floor().max(0.0) as usize
    };
    let visible_first = row_of(offset).min(last);
    let end = ((offset + viewport.max(0.0)) / layout.stride).ceil() - 1.0;
    let visible_last = (end.max(0.0) as usize).max(visible_first).min(last);
    Some(RowRange {
        first: visible_first.saturating_sub(overscan),
        last: visible_last.saturating_add(overscan).min(last),
        visible_first,
        visible_last,
    })
}

/// Inclusive global indexes for page and thumbnail demand.
pub fn to_library_range(layout: &GridLayout, rows: &RowRange) -> LibraryRange {
    let end = |row: usize| ((row + 1) * layout.columns - 1).min(layout.count.saturating_sub(1));
    LibraryRange {
        start_index: rows.first * layout.columns,
        end_index: end(rows.last),
        visible_start_index: rows.visible_first * layout.columns,
        visible_end_index: end(rows.visible_last),
    }
}

/// Most rows a viewport can render: every row it can intersect plus overscan on both
/// sides.
pub fn pool_rows(layout: &GridLayout, viewport: f64, overscan: usize) -> usize {
    let intersecting = (viewport.max(0.0) / layout.stride).ceil() as usize + 1;
    layout.rows.min(intersecting + 2 * overscan)
}

/// The offset that keeps the previous first visible global index at the top when
/// columns or tile size change. Offsets inside the margin above the grid are kept as
/// they are.
pub fn anchor_for_resize(previous: &GridLayout, next: &GridLayout, offset: f64) -> f64 {
    if offset <= 0.0 || (previous.columns == next.columns && previous.size == next.size) {
        return offset;
    }
    let first_index = (offset / previous.stride).floor() as usize * previous.columns;
    row_top(next, index_to_row(next, first_index))
}

#[cfg(test)]
mod tests {
    use super::{
        anchor_for_resize, grid_layout, pool_rows, row_range, row_top, to_library_range,
        GridLayout, LibraryRange, RowRange, GRID_GAP, OVERSCAN_ROWS,
    };

    fn layout(count: usize) -> GridLayout {
        grid_layout(988.0, 188.0, GRID_GAP, count)
    }

    #[test]
    fn a_million_slots_lay_out_in_exact_square_rows() {
        for (count, rows) in [(10_000, 2_000), (26_000, 5_200), (1_000_000, 200_000)] {
            let grid = layout(count);
            assert_eq!((grid.columns, grid.size, grid.stride), (5, 188.0, 200.0));
            assert_eq!(grid.rows, rows);
            assert_eq!(grid.content_height, rows as f64 * 200.0 - 12.0);
        }
        let one_column = grid_layout(588.0, 340.0, GRID_GAP, 1_000_000);
        assert_eq!((one_column.columns, one_column.size), (1, 588.0));
        assert_eq!(one_column.content_height, 1_000_000.0 * 600.0 - 12.0);
    }

    #[test]
    fn an_empty_grid_has_no_rows_and_no_range() {
        let grid = layout(0);
        assert_eq!((grid.rows, grid.content_height), (0, 0.0));
        assert_eq!(row_range(&grid, 0.0, 600.0, OVERSCAN_ROWS), None);
        assert_eq!(GridLayout::default().rows, 0);
    }

    #[test]
    fn a_middle_offset_maps_to_global_indexes_with_overscan() {
        let grid = layout(26_000);
        let rows = row_range(&grid, 20_000.0, 600.0, OVERSCAN_ROWS).expect("rows");
        assert_eq!(
            rows,
            RowRange {
                first: 96,
                last: 106,
                visible_first: 100,
                visible_last: 102
            }
        );
        assert_eq!(
            to_library_range(&grid, &rows),
            LibraryRange {
                start_index: 480,
                end_index: 534,
                visible_start_index: 500,
                visible_end_index: 514
            }
        );
        assert!(rows.len() <= pool_rows(&grid, 600.0, OVERSCAN_ROWS));
    }

    #[test]
    fn the_last_partial_row_and_an_offset_beyond_the_end_are_clamped() {
        let grid = layout(999_998);
        let rows =
            row_range(&grid, grid.content_height - 600.0, 600.0, OVERSCAN_ROWS).expect("rows");
        assert_eq!(rows.last, grid.rows - 1);
        assert_eq!(to_library_range(&grid, &rows).end_index, 999_997);
        let beyond = row_range(&grid, 1e12, 600.0, OVERSCAN_ROWS).expect("rows");
        assert_eq!(
            (beyond.visible_first, beyond.visible_last, beyond.last),
            (grid.rows - 1, grid.rows - 1, grid.rows - 1)
        );
        let above = row_range(&grid, -80.0, 600.0, OVERSCAN_ROWS).expect("rows");
        assert_eq!(
            (above.first, above.visible_first, above.visible_last),
            (0, 0, 2)
        );
    }

    #[test]
    fn a_resize_anchors_on_the_first_visible_index_without_precision_loss() {
        let before = layout(1_000_000);
        let after = grid_layout(588.0, 340.0, GRID_GAP, 1_000_000);
        let offset = row_top(&before, 123_457) + 99.0;
        assert_eq!(
            anchor_for_resize(&before, &after, offset),
            row_top(&after, 123_457 * 5)
        );
        assert_eq!(anchor_for_resize(&before, &after, -40.0), -40.0);
        assert_eq!(
            anchor_for_resize(&before, &layout(20_000), 2_345.0),
            2_345.0
        );
    }
}
