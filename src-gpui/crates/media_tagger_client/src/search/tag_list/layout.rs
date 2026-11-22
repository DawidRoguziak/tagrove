//! Measures and colours of the tag list dialog (`TagListModal.tsx`,
//! `TagListChipList.tsx`), in CSS pixels.

use super::{ListMetrics, TagSelectionMode};
use crate::{
    gallery::layout::area,
    overlay::ModalSize,
    theme::{Rgba, ThemeTokens},
};

/// The dialog: `flex max-h-[calc(100vh-2rem)] flex-col gap-3 overflow-hidden`, size
/// medium.
pub mod dialog {
    /// `gap-3` between header, field, summary, list and buttons.
    pub const GAP: f32 = 12.0;
    /// The header: `flex items-start justify-between gap-3`.
    pub const HEADER_GAP: f32 = 12.0;
    /// The field's label and input: `grid gap-1.5`.
    pub const FIELD_GAP: f32 = 6.0;
    /// `calc(100vh-2rem)`.
    pub const VIEWPORT_MARGIN: f32 = 32.0;
    /// The dialog's border.
    pub const BORDER: f32 = 1.0;
    /// The list: `p-2`.
    pub const LIST_PADDING: f32 = 8.0;
    /// A message in the list: `px-1 py-2`.
    pub const MESSAGE_PADDING_X: f32 = 4.0;
    pub const MESSAGE_PADDING_Y: f32 = 8.0;
    /// The buttons: `flex items-center justify-end gap-2`.
    pub const FOOTER_GAP: f32 = 8.0;
    /// The filter: `px-3 py-2.5 text-sm`.
    pub const INPUT_HEIGHT: f32 = 42.0;
}

/// The height the dialog's content may take inside the window: the window's height
/// less 2 rem, the dialog's padding and its border (`box-sizing: border-box`).
pub fn content_max_height(viewport_width: f32, viewport_height: f32) -> f32 {
    (viewport_height
        - dialog::VIEWPORT_MARGIN
        - 2.0 * ModalSize::content_padding(viewport_width)
        - 2.0 * dialog::BORDER)
        .max(0.0)
}

/// A tag button: `inline-flex rounded-[var(--radius-control)] border px-3 py-1 text-xs
/// font-medium`, in a `flex flex-wrap gap-1.5`.
pub mod chip {
    pub const GAP: f32 = 6.0;
    pub const PADDING_X: f32 = 12.0;
    pub const PADDING_Y: f32 = 4.0;
    /// `border`.
    pub const BORDER: f32 = 1.0;
    /// `focus-visible:ring-2 focus-visible:ring-primary/35`.
    pub const FOCUS_RING_WIDTH: f32 = 2.0;
    pub const FOCUS_RING_PERCENT: u8 = 35;
}

/// The colours of a tag button in one mode, at rest and under the pointer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChipVisuals {
    pub border: Rgba,
    pub background: Rgba,
    pub text: Rgba,
    pub hover_border: Rgba,
    pub hover_background: Rgba,
    pub focus_ring: Rgba,
}

/// `tagClasses` of `TagListChipList`.
pub fn chip_visuals(tokens: &ThemeTokens, mode: Option<TagSelectionMode>) -> ChipVisuals {
    let colors = &tokens.colors;
    let focus_ring = colors.primary.with_alpha_percent(chip::FOCUS_RING_PERCENT);
    match mode {
        Some(TagSelectionMode::Include) => {
            let border = colors.primary.with_alpha_percent(45);
            ChipVisuals {
                border,
                background: colors.primary.with_alpha_percent(12),
                text: colors.primary_text,
                hover_border: border,
                hover_background: colors.primary.with_alpha_percent(20),
                focus_ring,
            }
        }
        Some(TagSelectionMode::Exclude) => ChipVisuals {
            border: colors.error.with_alpha_percent(45),
            background: colors.error.with_alpha_percent(12),
            text: colors.error,
            hover_border: colors.error.with_alpha_percent(60),
            hover_background: colors.error.with_alpha_percent(18),
            focus_ring,
        },
        None => ChipVisuals {
            border: colors.base_content.with_alpha_percent(12),
            background: colors.base_100,
            text: colors.base_content,
            hover_border: colors.base_content.with_alpha_percent(24),
            hover_background: colors.base_100.with_alpha_percent(80),
            focus_ring,
        },
    }
}

/// A key that scrolls the list while one of its tag buttons has the focus: the
/// browser's keyboard scrolling of the list's `overflow-y-auto` box, which the
/// gallery's keys under the dialog would otherwise swallow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListScrollKey {
    LineUp,
    LineDown,
    PageUp,
    PageDown,
    Start,
    End,
}

/// The list's scroll top after `key`: the gallery's steps (`area::KEY_LINE`, a page of
/// `area::PAGE_FRACTION`), kept between the top and the end of the content.
pub fn key_scroll_top(metrics: ListMetrics, key: ListScrollKey) -> f32 {
    let line = area::KEY_LINE as f32;
    let page = metrics.viewport_height * area::PAGE_FRACTION as f32;
    let max = max_scroll_top(metrics);
    let top = match key {
        ListScrollKey::LineUp => metrics.scroll_top - line,
        ListScrollKey::LineDown => metrics.scroll_top + line,
        ListScrollKey::PageUp => metrics.scroll_top - page,
        ListScrollKey::PageDown => metrics.scroll_top + page,
        ListScrollKey::Start => 0.0,
        ListScrollKey::End => max,
    };
    top.clamp(0.0, max)
}

/// The list's scroll top that shows a tag button which got the keyboard focus, moving
/// as little as possible (the browser's focus scrolling, `block: "nearest"`).
/// `item_top` is the button's top in the content, `item_height` its height; a button
/// taller than the list is aligned to its top.
pub fn reveal_scroll_top(metrics: ListMetrics, item_top: f32, item_height: f32) -> f32 {
    let bottom = item_top + item_height;
    let top = if item_height > metrics.viewport_height || item_top < metrics.scroll_top {
        item_top
    } else if bottom > metrics.scroll_top + metrics.viewport_height {
        bottom - metrics.viewport_height
    } else {
        metrics.scroll_top
    };
    top.clamp(0.0, max_scroll_top(metrics))
}

fn max_scroll_top(metrics: ListMetrics) -> f32 {
    (metrics.content_height - metrics.viewport_height).max(0.0)
}

/// The i18n key of a mode's name (`tagList.states.*`).
pub fn state_key(mode: Option<TagSelectionMode>) -> &'static str {
    match mode {
        Some(TagSelectionMode::Include) => "tagList.states.included",
        Some(TagSelectionMode::Exclude) => "tagList.states.excluded",
        None => "tagList.states.inactive",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeName;

    #[test]
    fn the_solid_chip_list_still_has_the_classes_copied_here() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../src/features/search/tag-list/TagListChipList.tsx");
        let source = std::fs::read_to_string(&path).expect("read TagListChipList.tsx");
        for class in [
            "rounded-[var(--radius-control)] border px-3 py-1 text-xs font-medium",
            "focus-visible:ring-2 focus-visible:ring-primary/35",
            "border-primary/45 bg-primary/12 text-primary-text hover:bg-primary/20",
            "border-error/45 bg-error/12 text-error hover:border-error/60 hover:bg-error/18",
            "border-base-content/12 bg-base-100 text-base-content hover:border-base-content/24 hover:bg-base-100/80",
            "flex flex-wrap gap-1.5",
        ] {
            assert!(source.contains(class), "TagListChipList.tsx lost: {class}");
        }
    }

    #[test]
    fn each_mode_has_its_colours() {
        for theme in ThemeName::ALL {
            let tokens = theme.tokens();
            let include = chip_visuals(tokens, Some(TagSelectionMode::Include));
            assert_eq!(include.text, tokens.colors.primary_text);
            let exclude = chip_visuals(tokens, Some(TagSelectionMode::Exclude));
            assert_eq!(exclude.text, tokens.colors.error);
            let inactive = chip_visuals(tokens, None);
            assert_eq!(inactive.background, tokens.colors.base_100);
        }
        assert_eq!(state_key(None), "tagList.states.inactive");
    }

    fn metrics(scroll_top: f32) -> ListMetrics {
        ListMetrics {
            content_height: 1000.0,
            viewport_height: 200.0,
            scroll_top,
        }
    }

    #[test]
    fn keys_scroll_the_list_by_lines_pages_and_to_its_edges() {
        use ListScrollKey::*;
        assert_eq!(key_scroll_top(metrics(100.0), LineDown), 140.0);
        assert_eq!(key_scroll_top(metrics(100.0), LineUp), 60.0);
        assert_eq!(key_scroll_top(metrics(100.0), PageDown), 275.0);
        assert_eq!(key_scroll_top(metrics(100.0), PageUp), 0.0);
        assert_eq!(key_scroll_top(metrics(100.0), Start), 0.0);
        assert_eq!(key_scroll_top(metrics(100.0), End), 800.0);
        assert_eq!(key_scroll_top(metrics(790.0), LineDown), 800.0);
        let fits = ListMetrics {
            content_height: 150.0,
            viewport_height: 200.0,
            scroll_top: 0.0,
        };
        assert_eq!(key_scroll_top(fits, PageDown), 0.0);
    }

    #[test]
    fn a_focused_button_is_revealed_with_the_least_scroll() {
        // Visible: nothing moves.
        assert_eq!(reveal_scroll_top(metrics(100.0), 150.0, 30.0), 100.0);
        // Below the list: its bottom meets the list's bottom.
        assert_eq!(reveal_scroll_top(metrics(100.0), 290.0, 30.0), 120.0);
        // Above the list: its top meets the list's top.
        assert_eq!(reveal_scroll_top(metrics(100.0), 40.0, 30.0), 40.0);
        // Taller than the list: its top.
        assert_eq!(reveal_scroll_top(metrics(100.0), 400.0, 300.0), 400.0);
        // Never past the end.
        assert_eq!(reveal_scroll_top(metrics(0.0), 990.0, 30.0), 800.0);
    }

    #[test]
    fn the_content_fits_the_window_less_two_rem() {
        assert_eq!(content_max_height(1440.0, 900.0), 900.0 - 32.0 - 48.0 - 2.0);
        assert_eq!(content_max_height(500.0, 900.0), 900.0 - 32.0 - 40.0 - 2.0);
        assert_eq!(content_max_height(1440.0, 10.0), 0.0);
    }
}
