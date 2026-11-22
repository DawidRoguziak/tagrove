//! The geometry of rectangle selection (`src/features/gallery/selectionGeometry.ts`).
//!
//! Everything is computed from global indexes and the grid's measures, never from
//! the tiles that happen to be drawn or the pages that happen to be cached, so a
//! rectangle that reaches beyond the rendered rows selects what it covers. Points
//! and rectangles are in grid coordinates: logical pixels from the left edge of the
//! first column and the top of the first row, in `f64` like every offset of the
//! gallery.

use super::{geometry::GridLayout, selection::IndexRange};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SelectionRectangle {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

/// The measures of the grid a rectangle is laid over (`GridGeometry` of
/// `selectionGeometry.ts`).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct SelectionGrid {
    pub column_count: usize,
    /// The side of a tile.
    pub tile_size: f64,
    pub tile_gap: f64,
    pub asset_count: usize,
}

impl SelectionGrid {
    pub fn of(layout: &GridLayout) -> Self {
        Self {
            column_count: layout.columns,
            tile_size: layout.size,
            tile_gap: layout.gap,
            asset_count: layout.count,
        }
    }

    fn stride(&self) -> f64 {
        self.tile_size + self.tile_gap
    }
}

/// The rectangle with `start` and `end` as opposite corners.
pub fn rectangle_between(start: Point, end: Point) -> SelectionRectangle {
    SelectionRectangle {
        left: start.x.min(end.x),
        top: start.y.min(end.y),
        width: (end.x - start.x).abs(),
        height: (end.y - start.y).abs(),
    }
}

/// The index ranges of the tiles `rect` overlaps by a positive area, one per row
/// unless consecutive rows join (a rectangle across every column), ascending.
pub fn intersected_ranges(rect: &SelectionRectangle, grid: &SelectionGrid) -> Vec<IndexRange> {
    let SelectionGrid {
        column_count,
        tile_size,
        asset_count,
        ..
    } = *grid;
    let stride = grid.stride();
    let measurable = [rect.left, rect.top, rect.width, rect.height, stride]
        .iter()
        .all(|value| value.is_finite());
    if !measurable
        || rect.width <= 0.0
        || rect.height <= 0.0
        || asset_count == 0
        || column_count == 0
        || stride <= 0.0
    {
        return Vec::new();
    }
    // A tile is overlapped when the rectangle starts before its far edge and ends
    // after its near edge; touching edges do not count.
    let first_of = |near: f64| (((near - tile_size) / stride).floor() + 1.0).max(0.0);
    let last_of = |far: f64, limit: usize| ((far / stride).ceil() - 1.0).min(limit as f64);
    let first_column = first_of(rect.left);
    let last_column = last_of(rect.left + rect.width, column_count - 1);
    let first_row = first_of(rect.top);
    let last_row = last_of(
        rect.top + rect.height,
        asset_count.div_ceil(column_count) - 1,
    );
    if first_column > last_column || first_row > last_row {
        return Vec::new();
    }
    // Both ends are whole numbers within the grid now.
    let (first_column, last_column) = (first_column as usize, last_column as usize);
    let mut ranges: Vec<IndexRange> = Vec::new();
    for row in first_row as usize..=last_row as usize {
        let start = row * column_count + first_column;
        let end = (asset_count - 1).min(row * column_count + last_column);
        if start > end {
            continue;
        }
        match ranges.last_mut() {
            Some(previous) if previous[1] + 1 == start => previous[1] = end,
            _ => ranges.push([start, end]),
        }
    }
    ranges
}

/// Binary search: whether `index` lies in one of the ascending ranges.
pub fn contains_index(ranges: &[IndexRange], index: usize) -> bool {
    ranges
        .binary_search_by(|[start, end]| {
            if index < *start {
                std::cmp::Ordering::Greater
            } else if index > *end {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

/// How fast the gallery scrolls while a rectangle is dragged with the pointer at
/// `y`, in pixels per second: up to 900 inside the 48 px (or half the viewport)
/// next to the `top` and the `bottom` edge and beyond them, nothing in between.
pub fn edge_scroll_speed(y: f64, top: f64, bottom: f64) -> f64 {
    let edge = 48.0_f64.min((bottom - top) / 2.0);
    if y < top + edge {
        -900.0 * ((top + edge - y) / edge).min(1.0)
    } else if y > bottom - edge {
        900.0 * ((y - bottom + edge) / edge).min(1.0)
    } else {
        0.0
    }
}

/// The global index of the tile under `point`, or `None` over a gap, the margins or
/// the space after the last tile.
pub fn tile_index_at(point: Point, grid: &SelectionGrid) -> Option<usize> {
    let stride = grid.stride();
    if grid.column_count == 0 || stride <= 0.0 || point.x < 0.0 || point.y < 0.0 {
        return None;
    }
    let lane = |position: f64| {
        let lane = (position / stride).floor();
        (position - lane * stride < grid.tile_size).then_some(lane)
    };
    let column = lane(point.x).filter(|column| *column < grid.column_count as f64)?;
    let row = lane(point.y)?;
    let index = row * grid.column_count as f64 + column;
    (index < grid.asset_count as f64).then_some(index as usize)
}

#[cfg(test)]
mod tests {
    use super::{
        contains_index, edge_scroll_speed, intersected_ranges, rectangle_between, tile_index_at,
        Point, SelectionGrid, SelectionRectangle,
    };

    const GRID: SelectionGrid = SelectionGrid {
        column_count: 4,
        tile_size: 100.0,
        tile_gap: 10.0,
        asset_count: 10,
    };

    fn rect(left: f64, top: f64, width: f64, height: f64) -> SelectionRectangle {
        SelectionRectangle {
            left,
            top,
            width,
            height,
        }
    }

    #[test]
    fn a_rectangle_is_normalized_whatever_the_drag_direction() {
        let forward = rectangle_between(Point { x: 10.0, y: 20.0 }, Point { x: 60.0, y: 90.0 });
        let backward = rectangle_between(Point { x: 60.0, y: 90.0 }, Point { x: 10.0, y: 20.0 });
        assert_eq!(forward, rect(10.0, 20.0, 50.0, 70.0));
        assert_eq!(forward, backward);
    }

    #[test]
    fn tiles_with_any_overlap_are_selected_and_touching_edges_are_not() {
        // One pixel into the first tile.
        assert_eq!(
            intersected_ranges(&rect(99.0, 99.0, 5.0, 5.0), &GRID),
            [[0, 0]]
        );
        // Starting exactly at the far edge of the first tile: only the gap.
        assert!(intersected_ranges(&rect(100.0, 0.0, 10.0, 50.0), &GRID).is_empty());
        // Ending exactly at the near edge of the second tile.
        assert_eq!(
            intersected_ranges(&rect(50.0, 50.0, 60.0, 10.0), &GRID),
            [[0, 0]]
        );
        assert_eq!(
            intersected_ranges(&rect(50.0, 50.0, 61.0, 10.0), &GRID),
            [[0, 1]]
        );
    }

    #[test]
    fn rows_stay_separate_unless_the_rectangle_spans_every_column() {
        assert_eq!(
            intersected_ranges(&rect(120.0, 50.0, 150.0, 120.0), &GRID),
            [[1, 2], [5, 6]]
        );
        assert_eq!(
            intersected_ranges(&rect(-50.0, -50.0, 1_000.0, 1_000.0), &GRID),
            [[0, 9]]
        );
        // The last row is short: the range ends with the last asset.
        assert_eq!(
            intersected_ranges(&rect(120.0, 230.0, 300.0, 50.0), &GRID),
            [[9, 9]]
        );
        assert!(intersected_ranges(&rect(230.0, 230.0, 200.0, 50.0), &GRID).is_empty());
    }

    #[test]
    fn empty_rectangles_and_grids_select_nothing() {
        assert!(intersected_ranges(&rect(0.0, 0.0, 0.0, 50.0), &GRID).is_empty());
        assert!(intersected_ranges(&rect(0.0, 0.0, 50.0, 0.0), &GRID).is_empty());
        assert!(intersected_ranges(&rect(-500.0, 0.0, 100.0, 50.0), &GRID).is_empty());
        assert!(intersected_ranges(&rect(0.0, -500.0, 100.0, 50.0), &GRID).is_empty());
        assert!(intersected_ranges(&rect(0.0, 5_000.0, 100.0, 50.0), &GRID).is_empty());
        assert!(intersected_ranges(&rect(f64::NAN, 0.0, 100.0, 50.0), &GRID).is_empty());
        let empty = SelectionGrid {
            asset_count: 0,
            ..GRID
        };
        assert!(intersected_ranges(&rect(0.0, 0.0, 50.0, 50.0), &empty).is_empty());
        let no_columns = SelectionGrid {
            column_count: 0,
            ..GRID
        };
        assert!(intersected_ranges(&rect(0.0, 0.0, 50.0, 50.0), &no_columns).is_empty());
    }

    #[test]
    fn a_rectangle_far_down_a_million_results_uses_global_indexes() {
        let grid = SelectionGrid {
            column_count: 6,
            tile_size: 221.0,
            tile_gap: 12.0,
            asset_count: 1_000_000,
        };
        let row = 150_000.0;
        let ranges = intersected_ranges(&rect(240.0, row * 233.0 + 5.0, 300.0, 240.0), &grid);
        assert_eq!(ranges, [[900_001, 900_002], [900_007, 900_008]]);
    }

    #[test]
    fn contains_index_searches_ascending_ranges() {
        let ranges = [[0, 5], [10, 30], [100, 100]];
        for index in [0, 5, 10, 30, 100] {
            assert!(contains_index(&ranges, index), "{index}");
        }
        for index in [6, 9, 31, 99, 101] {
            assert!(!contains_index(&ranges, index), "{index}");
        }
        assert!(!contains_index(&[], 0));
    }

    #[test]
    fn the_edges_scroll_faster_the_closer_the_pointer_is() {
        assert_eq!(edge_scroll_speed(300.0, 0.0, 600.0), 0.0);
        assert_eq!(edge_scroll_speed(48.0, 0.0, 600.0), 0.0);
        assert_eq!(edge_scroll_speed(24.0, 0.0, 600.0), -450.0);
        assert_eq!(edge_scroll_speed(-100.0, 0.0, 600.0), -900.0);
        assert_eq!(edge_scroll_speed(576.0, 0.0, 600.0), 450.0);
        assert_eq!(edge_scroll_speed(900.0, 0.0, 600.0), 900.0);
        // A viewport lower than two edge zones splits in the middle.
        assert_eq!(edge_scroll_speed(10.0, 0.0, 40.0), -450.0);
        assert_eq!(edge_scroll_speed(30.0, 0.0, 40.0), 450.0);
    }

    #[test]
    fn the_tile_under_a_point_ignores_gaps_and_the_space_after_the_last_tile() {
        assert_eq!(tile_index_at(Point { x: 0.0, y: 0.0 }, &GRID), Some(0));
        assert_eq!(tile_index_at(Point { x: 99.9, y: 99.9 }, &GRID), Some(0));
        assert_eq!(tile_index_at(Point { x: 105.0, y: 50.0 }, &GRID), None);
        assert_eq!(tile_index_at(Point { x: 50.0, y: 105.0 }, &GRID), None);
        assert_eq!(tile_index_at(Point { x: 115.0, y: 115.0 }, &GRID), Some(5));
        assert_eq!(tile_index_at(Point { x: 115.0, y: 225.0 }, &GRID), Some(9));
        assert_eq!(tile_index_at(Point { x: 225.0, y: 225.0 }, &GRID), None);
        assert_eq!(tile_index_at(Point { x: 445.0, y: 5.0 }, &GRID), None);
        assert_eq!(tile_index_at(Point { x: -1.0, y: 5.0 }, &GRID), None);
        assert_eq!(tile_index_at(Point { x: 5.0, y: -1.0 }, &GRID), None);
    }
}
