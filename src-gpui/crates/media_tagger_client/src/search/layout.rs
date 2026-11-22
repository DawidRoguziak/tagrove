//! Measures of the search header and its suggestion list, in CSS pixels, from
//! `SearchTagatorWrapper.tsx`, `SearchSuggestionsList.tsx`,
//! `SearchSuggestionOption.tsx` and `src/styles.css`.

/// `.workspace-search-field`: `gap-2 rounded-[var(--radius-control)] border
/// border-[var(--border-strong)] bg-[var(--surface-muted)] px-2
/// focus-within:border-primary`.
pub mod field {
    pub const GAP: f32 = 8.0;
    pub const PADDING_X: f32 = 8.0;
    /// The frame's border (`border`).
    pub const BORDER: f32 = 1.0;
    /// The magnifier. Its `h-4 w-4` loses to `UiIcon`'s own `h-5 w-5` (both
    /// classes are on the element and Tailwind emits `h-5` later), so the Tauri
    /// application draws it 20 pixels square.
    pub const ICON_SIZE: f32 = 20.0;
    /// The input (`filter-input font-mono h-8 px-1 py-2`).
    pub const INPUT_HEIGHT: f32 = 32.0;
    /// The clear button (`h-8 w-8`, transparent).
    pub const CLEAR_SIZE: f32 = 32.0;
    /// The clear button's icon (`UiIconButton`'s `h-5 w-5`).
    pub const CLEAR_ICON_SIZE: f32 = 20.0;
    /// The Search button (`h-7 text-xs`).
    pub const SUBMIT_HEIGHT: f32 = 28.0;
    pub const SUBMIT_FONT_SIZE: f32 = 12.0;

    /// Where the input's box starts, from the frame's left edge: border, padding,
    /// magnifier and gap. The suggestion list hangs below this box, so it starts
    /// there too.
    pub fn input_left() -> f32 {
        BORDER + PADDING_X + ICON_SIZE + GAP
    }
}

/// `.search-shortcut`: the key hint.
pub mod shortcut_hint {
    pub const RADIUS: f32 = 3.0;
    pub const PADDING_Y: f32 = 1.0;
    pub const PADDING_X: f32 = 5.0;
    pub const FONT_SIZE: f32 = 11.0;
    /// The `font` shorthand resets the line height to `normal`: JetBrains Mono's
    /// ascent and descent at 11 pixels, rounded each (11 + 3), not the root's 1.5.
    /// The hint is 16 pixels high in the Tauri capture.
    pub const LINE_HEIGHT: f32 = 14.0;
    /// The hint of a platform that is not macOS (`navigator.platform`); key names
    /// are not translated in the Tauri interface either.
    pub const TEXT: &str = "Ctrl K";
}

/// The tools row's own part: media kind, favourites and the divider before the tag
/// list (`.workspace-tools` children).
pub mod tools {
    /// The favourites button (`h-8 w-8`).
    pub const FAVORITES_SIZE: f32 = 32.0;
    /// The divider: `mx-1 h-5 border-l`. It is drawn as a filled box one pixel
    /// wide: GPUI draws no left border on a box only as wide as that border (its
    /// quad shader picks the right border, of width 0, for the middle column).
    pub const DIVIDER_MARGIN_X: f32 = 4.0;
    pub const DIVIDER_HEIGHT: f32 = 20.0;
    pub const DIVIDER_WIDTH: f32 = 1.0;
}

/// The validation alert under the header (`px-5 pb-2`).
pub mod alert {
    pub const PADDING_X: f32 = 20.0;
    pub const PADDING_BOTTOM: f32 = 8.0;
}

/// The suggestion list (`SearchSuggestionsList`, inline strategy, below the input).
pub mod list {
    /// `top-[calc(100%+8px)]`.
    pub const GAP: f32 = 8.0;
    /// `max-h-[min(18rem,45vh)]`: 288 px or 45 % of the window's height.
    pub const MAX_HEIGHT: f32 = 288.0;
    pub const MAX_HEIGHT_VIEWPORT_FRACTION: f32 = 0.45;
    /// `p-1.5`, `gap-0.5`.
    pub const PADDING: f32 = 6.0;
    pub const ITEM_GAP: f32 = 2.0;
    /// An option: `px-2.5 py-[7px]`.
    pub const OPTION_PADDING_X: f32 = 10.0;
    pub const OPTION_PADDING_Y: f32 = 7.0;
    /// The active option's `inset 0 0 0 1px` ring.
    pub const ACTIVE_RING_WIDTH: f32 = 1.0;
    /// `hover:bg-base-content/12`.
    pub const HOVER_FILL_PERCENT: u8 = 12;

    /// The list's height limit in a window `viewport_height` tall.
    pub fn max_height(viewport_height: f32) -> f32 {
        MAX_HEIGHT.min(viewport_height * MAX_HEIGHT_VIEWPORT_FRACTION)
    }
}

/// Splits `value` into runs at the highlighted byte ranges (`renderHighlightedMatch`):
/// `(text, highlighted)` in order. Ranges are clamped to the text and to character
/// boundaries; overlapping ones are merged.
pub fn highlight_runs<'a>(
    value: &'a str,
    ranges: &[std::ops::Range<usize>],
) -> Vec<(&'a str, bool)> {
    let mut sorted: Vec<_> = ranges.to_vec();
    sorted.sort_by_key(|range| range.start);
    let floor = |mut at: usize| {
        at = at.min(value.len());
        while !value.is_char_boundary(at) {
            at -= 1;
        }
        at
    };
    let mut runs = Vec::new();
    let mut cursor = 0;
    for range in sorted {
        let start = floor(range.start).max(cursor);
        let end = floor(range.end);
        if start > cursor {
            runs.push((&value[cursor..start], false));
        }
        if end > start {
            runs.push((&value[start..end], true));
        }
        cursor = cursor.max(end);
    }
    if cursor < value.len() {
        runs.push((&value[cursor..], false));
    }
    runs
}

#[cfg(test)]
#[allow(clippy::single_range_in_vec_init)]
mod tests {
    use super::{field, highlight_runs, list};

    #[test]
    fn the_input_starts_37_pixels_into_the_field_as_in_the_tauri_header() {
        // Measured in the `search-applied` capture of the Tauri application: the
        // field's frame starts at x 216, the magnifier covers 225 to 245 and the
        // input's box (and the suggestion list under it) starts at 253.
        assert_eq!(field::ICON_SIZE, 20.0);
        assert_eq!(field::input_left(), 253.0 - 216.0);
    }

    #[test]
    fn the_key_hint_is_16_pixels_high_as_in_the_tauri_header() {
        use super::shortcut_hint::{LINE_HEIGHT, PADDING_Y};
        assert_eq!(LINE_HEIGHT + 2.0 * PADDING_Y, 16.0);
    }

    #[test]
    fn runs_cover_the_text_once() {
        assert_eq!(
            highlight_runs("scatter", &[1..4]),
            [("s", false), ("cat", true), ("ter", false)]
        );
        assert_eq!(highlight_runs("cat", &[]), [("cat", false)]);
        assert_eq!(
            highlight_runs("cart", &[3..4, 0..2, 1..2]),
            [("ca", true), ("r", false), ("t", true)]
        );
        assert_eq!(highlight_runs("żółw", &[1..3, 0..99]), [("żółw", true)]);
    }

    #[test]
    fn the_list_is_at_most_288_pixels_or_45_percent_of_the_window() {
        assert_eq!(list::max_height(1000.0), 288.0);
        assert_eq!(list::max_height(600.0), 270.0);
    }
}
