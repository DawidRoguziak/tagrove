//! The rows of the tag list's `flex flex-wrap gap-1.5` (`TagListChipList`), worked out
//! from the buttons' sizes, so that the view draws only the rows near the visible part
//! of the list (review findings r2-perf-1, r4-gpui-3 and r6-gpui-1): a frame then
//! builds and lays out a number of buttons that follows the list's height, not the
//! number of loaded tags. The browser scrolls the Solid list without laying it out
//! again; GPUI builds every drawn element again in every frame.

use std::ops::Range;

/// Rows drawn above and below the visible ones.
pub const OVERSCAN_ROWS: usize = 3;

/// The border box of a tag button as its content asks for it, before the row's width
/// limits it (`max-w-full`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChipSize {
    pub width: f32,
    pub height: f32,
}

/// The buttons packed into rows the way flex-wrap packs them, with each row's place
/// in the list's content (from the top of the first row; the list's padding is not
/// counted).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChipRows {
    width: f32,
    /// The first button of each row, then one past the last button.
    starts: Vec<usize>,
    tops: Vec<f32>,
    bottoms: Vec<f32>,
}

impl ChipRows {
    /// Packs buttons of `sizes` into rows `width` wide with `gap` between buttons and
    /// rows, as flex-wrap does: a button joins the current row while it fits there with
    /// the gap before it, and a button wider than a row is narrowed to it
    /// (`max-w-full`) and takes a row of its own. A row is as tall as its tallest
    /// button (`align-items: stretch`).
    pub fn pack(sizes: impl IntoIterator<Item = ChipSize>, width: f32, gap: f32) -> Self {
        let mut rows = Self {
            width,
            starts: Vec::new(),
            tops: Vec::new(),
            bottoms: Vec::new(),
        };
        let mut used = 0.0_f32;
        let mut index = 0;
        for size in sizes {
            let chip_width = size.width.min(width);
            let fits = !rows.starts.is_empty() && used + gap + chip_width <= width;
            if fits {
                used += gap + chip_width;
                let last = rows.bottoms.len() - 1;
                rows.bottoms[last] = rows.bottoms[last].max(rows.tops[last] + size.height);
            } else {
                let top = rows.bottoms.last().map_or(0.0, |bottom| bottom + gap);
                rows.starts.push(index);
                rows.tops.push(top);
                rows.bottoms.push(top + size.height);
                used = chip_width;
            }
            index += 1;
        }
        rows.starts.push(index);
        rows
    }

    /// The width the rows were packed for.
    pub fn width(&self) -> f32 {
        self.width
    }

    /// The number of rows.
    pub fn len(&self) -> usize {
        self.tops.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tops.is_empty()
    }

    /// The number of buttons.
    pub fn chip_count(&self) -> usize {
        self.starts.last().copied().unwrap_or(0)
    }

    /// The buttons of `row`.
    pub fn chips(&self, row: usize) -> Range<usize> {
        self.starts[row]..self.starts[row + 1]
    }

    /// The buttons of the rows `rows`.
    pub fn chips_of_rows(&self, rows: Range<usize>) -> Range<usize> {
        if rows.is_empty() {
            return 0..0;
        }
        self.starts[rows.start]..self.starts[rows.end]
    }

    pub fn top(&self, row: usize) -> f32 {
        self.tops[row]
    }

    pub fn height(&self, row: usize) -> f32 {
        self.bottoms[row] - self.tops[row]
    }

    /// The height of all rows with the gaps between them.
    pub fn content_height(&self) -> f32 {
        self.bottoms.last().copied().unwrap_or(0.0)
    }

    /// The row of the button at `chip`.
    pub fn row_of(&self, chip: usize) -> Option<usize> {
        (chip < self.chip_count()).then(|| self.starts.partition_point(|start| *start <= chip) - 1)
    }

    /// The rows that reach into `top..bottom` of the content.
    pub fn rows_between(&self, top: f32, bottom: f32) -> Range<usize> {
        let first = self.bottoms.partition_point(|bottom| *bottom <= top);
        let end = self.tops.partition_point(|row_top| *row_top < bottom);
        first..end.max(first)
    }

    /// The rows to draw, in order and without overlap: the ones in the visible part
    /// `scroll_top..scroll_top + viewport_height` of the content with
    /// [`OVERSCAN_ROWS`] on each side, and the row of the focused button `focused`
    /// with the rows next to it wherever the list is scrolled, so that the focus keeps
    /// its button and Tab and Shift+Tab find the buttons next to it.
    pub fn rows_to_draw(
        &self,
        scroll_top: f32,
        viewport_height: f32,
        focused: Option<usize>,
    ) -> Vec<Range<usize>> {
        let visible = self.rows_between(scroll_top, scroll_top + viewport_height.max(0.0));
        let mut ranges = Vec::with_capacity(2);
        ranges.push(
            visible.start.saturating_sub(OVERSCAN_ROWS)
                ..(visible.end + OVERSCAN_ROWS).min(self.len()),
        );
        if let Some(row) = focused.and_then(|chip| self.row_of(chip)) {
            ranges.push(row.saturating_sub(1)..(row + 2).min(self.len()));
        }
        ranges.retain(|range| !range.is_empty());
        ranges.sort_by_key(|range| range.start);
        let mut merged: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
        for range in ranges {
            match merged.last_mut() {
                Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
                _ => merged.push(range),
            }
        }
        merged
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sizes(widths: &[f32]) -> Vec<ChipSize> {
        widths
            .iter()
            .map(|width| ChipSize {
                width: *width,
                height: 26.0,
            })
            .collect()
    }

    fn ranges(bounds: &[(usize, usize)]) -> Vec<Range<usize>> {
        bounds.iter().map(|(start, end)| *start..*end).collect()
    }

    #[test]
    fn buttons_wrap_like_flex_wrap() {
        // 100 + 6 + 100 = 206 fits 210; a third does not.
        let rows = ChipRows::pack(sizes(&[100.0, 100.0, 100.0, 50.0]), 210.0, 6.0);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows.chips(0), 0..2);
        assert_eq!(rows.chips(1), 2..4);
        assert_eq!((rows.top(1), rows.height(1)), (32.0, 26.0));
        assert_eq!(rows.content_height(), 58.0);
        // Exactly the width fits.
        let exact = ChipRows::pack(sizes(&[100.0, 104.0]), 210.0, 6.0);
        assert_eq!(exact.len(), 1);
    }

    #[test]
    fn a_button_wider_than_the_row_takes_a_row_of_its_own() {
        let mut boxes = sizes(&[40.0, 500.0, 40.0]);
        boxes[1].height = 42.0;
        let rows = ChipRows::pack(boxes, 210.0, 6.0);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows.chips(1), 1..2);
        assert_eq!((rows.top(1), rows.height(1)), (32.0, 42.0));
        assert_eq!(rows.top(2), 80.0);
    }

    #[test]
    fn a_row_is_as_tall_as_its_tallest_button() {
        let mut boxes = sizes(&[40.0, 40.0]);
        boxes[1].height = 30.0;
        let rows = ChipRows::pack(boxes, 210.0, 6.0);
        assert_eq!(rows.height(0), 30.0);
    }

    #[test]
    fn nothing_to_pack_gives_no_rows() {
        let rows = ChipRows::pack(Vec::new(), 210.0, 6.0);
        assert!(rows.is_empty());
        assert_eq!(rows.content_height(), 0.0);
        assert_eq!(rows.chip_count(), 0);
        assert_eq!(rows.row_of(0), None);
        assert!(rows.rows_to_draw(0.0, 100.0, None).is_empty());
    }

    #[test]
    fn rows_of_buttons_are_found_by_index_and_position() {
        // Two buttons a row, rows 32 px apart.
        let rows = ChipRows::pack(sizes(&[100.0; 20]), 210.0, 6.0);
        assert_eq!(rows.len(), 10);
        assert_eq!(rows.row_of(0), Some(0));
        assert_eq!(rows.row_of(5), Some(2));
        assert_eq!(rows.row_of(19), Some(9));
        assert_eq!(rows.row_of(20), None);
        // The gap after row 0 (26..32) belongs to no row.
        assert_eq!(rows.rows_between(0.0, 26.0), 0..1);
        assert_eq!(rows.rows_between(26.0, 32.0), 1..1);
        assert_eq!(rows.rows_between(30.0, 70.0), 1..3);
        assert_eq!(rows.rows_between(1_000.0, 2_000.0), 10..10);
        assert_eq!(rows.chips_of_rows(2..4), 4..8);
    }

    #[test]
    fn the_drawn_rows_follow_the_view_not_the_number_of_buttons() {
        let rows = ChipRows::pack(sizes(&[100.0; 20_000]), 210.0, 6.0);
        let drawn = rows.rows_to_draw(32.0 * 5_000.0, 320.0, None);
        assert_eq!(drawn, ranges(&[(4_997, 5_013)]));
        let count: usize = drawn.iter().map(|range| range.len()).sum();
        assert!(count <= 10 + 2 * OVERSCAN_ROWS + 1);
        assert_eq!(rows.rows_to_draw(0.0, 320.0, None), ranges(&[(0, 13)]));
        assert_eq!(
            rows.rows_to_draw(32.0 * 10_000.0, 320.0, None),
            ranges(&[(9_997, 10_000)])
        );
    }

    #[test]
    fn the_focused_row_and_its_neighbours_stay_drawn() {
        let rows = ChipRows::pack(sizes(&[100.0; 2_000]), 210.0, 6.0);
        // Focus on button 1001 (row 500) while the top of the list is shown.
        assert_eq!(
            rows.rows_to_draw(0.0, 320.0, Some(1_001)),
            ranges(&[(0, 13), (499, 502)])
        );
        // Next to the visible rows: one range.
        assert_eq!(rows.rows_to_draw(0.0, 320.0, Some(27)), ranges(&[(0, 15)]));
        // The first row has no row above it.
        assert_eq!(
            rows.rows_to_draw(32.0 * 500.0, 320.0, Some(0)),
            ranges(&[(0, 2), (497, 513)])
        );
    }
}
