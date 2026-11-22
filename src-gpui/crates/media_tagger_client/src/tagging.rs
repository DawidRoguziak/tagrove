//! The shared tag editor (`src/features/tagging/TagEditor.tsx`): the assigned tags as
//! chips and a borderless field in one frame, with the suggestions of the search field.
//!
//! The lightbox (epic E16) and the bulk dock (epic E14) use it. The session of its field
//! is a [`Combobox`](crate::search::Combobox) with [`TAG_EDITOR_COMBOBOX`]: no item is
//! active until an arrow key, a pick goes to the owner and the list stays open, and the
//! assigned tags are never suggested. This module holds what the editor decides without a
//! GUI: what the frame shows, when editing is disabled, the measures and colours of both
//! variants and where the suggestion list goes in the window.

use crate::{
    search::ComboboxConfig,
    theme::{Rgba, ThemeTokens},
};

/// The two places the editor is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TagEditorVariant {
    /// The lightbox's side panel (`lightbox-tag-list`).
    Lightbox,
    /// The bulk dock (`bulk-tag-list`).
    #[default]
    Bulk,
}

impl TagEditorVariant {
    /// `data-testid` of the frame.
    pub const fn test_id(self) -> &'static str {
        match self {
            Self::Lightbox => "lightbox-tag-list",
            Self::Bulk => "bulk-tag-list",
        }
    }
}

/// How the field of a tag editor suggests: picks go to the owner, the list stays open
/// after one, and no item is active before an arrow key.
pub const TAG_EDITOR_COMBOBOX: ComboboxConfig = ComboboxConfig {
    auto_select_first: false,
    pick_to_owner: true,
    keep_open_on_pick: true,
};

/// Where the suggestion list opens (`suggestionsPlacement`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SuggestionPlacement {
    Above,
    #[default]
    Below,
}

/// What the frame shows before the field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorContent {
    /// `loadingText`: only the text, no chip.
    Loading,
    /// The chips of the assigned tags.
    Tags,
    /// `emptyText` without tags.
    Empty,
    /// Nothing but the field.
    Nothing,
}

/// The content of the frame: the loading text wins, then the tags, then the empty
/// text.
pub fn editor_content(loading: bool, tag_count: usize, has_empty_text: bool) -> EditorContent {
    if loading {
        EditorContent::Loading
    } else if tag_count > 0 {
        EditorContent::Tags
    } else if has_empty_text {
        EditorContent::Empty
    } else {
        EditorContent::Nothing
    }
}

/// `editingDisabled`: disabled, or loading.
pub fn editing_disabled(disabled: bool, loading: bool) -> bool {
    disabled || loading
}

/// Measures of the editor, in CSS pixels.
pub mod layout {
    /// The frame: `flex flex-wrap items-start gap-1.5 rounded-[var(--radius-control)]
    /// border p-2`.
    pub const GAP: f32 = 6.0;
    pub const PADDING: f32 = 8.0;
    /// Lightbox: `max-h-[200px] min-h-[48px]`.
    pub const LIGHTBOX_MAX_HEIGHT: f32 = 200.0;
    pub const LIGHTBOX_MIN_HEIGHT: f32 = 48.0;
    /// Bulk dock: `max-h-36`.
    pub const BULK_MAX_HEIGHT: f32 = 144.0;
    /// A chip: `gap-1.5 px-2 py-1 font-mono text-xs font-medium leading-4`.
    pub const CHIP_GAP: f32 = 6.0;
    pub const CHIP_PADDING_X: f32 = 8.0;
    pub const CHIP_PADDING_Y: f32 = 4.0;
    pub const CHIP_LINE_HEIGHT: f32 = 16.0;
    /// The remove button: `h-5 w-5` with an icon `h-3 w-3`.
    pub const REMOVE_SIZE: f32 = 20.0;
    pub const REMOVE_ICON_SIZE: f32 = 12.0;
    /// The field's box: `flex-[1_1_9rem] max-w-full`; the input `h-8`.
    pub const INPUT_BASIS: f32 = 144.0;
    pub const INPUT_HEIGHT: f32 = 32.0;
    /// The status text of the lightbox variant: `px-1`.
    pub const LIGHTBOX_STATUS_PADDING_X: f32 = 4.0;
}

/// The frame's colours of a variant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EditorVisuals {
    pub background: Rgba,
    pub border: Rgba,
    pub max_height: f32,
    pub min_height: f32,
    pub status_padding_x: f32,
}

pub fn editor_visuals(tokens: &ThemeTokens, variant: TagEditorVariant) -> EditorVisuals {
    let colors = &tokens.colors;
    match variant {
        // `bg-base-200/38`
        TagEditorVariant::Lightbox => EditorVisuals {
            background: colors.base_200.with_alpha_percent(38),
            border: colors.border_soft,
            max_height: layout::LIGHTBOX_MAX_HEIGHT,
            min_height: layout::LIGHTBOX_MIN_HEIGHT,
            status_padding_x: layout::LIGHTBOX_STATUS_PADDING_X,
        },
        // `bg-[var(--surface-muted)]`
        TagEditorVariant::Bulk => EditorVisuals {
            background: colors.surface_muted,
            border: colors.border_soft,
            max_height: layout::BULK_MAX_HEIGHT,
            min_height: 0.0,
            status_padding_x: 0.0,
        },
    }
}

/// The suggestion list of an editor (`SearchSuggestionsList`, `viewport` strategy):
/// fixed in the window, as wide as the field, kept 12 pixels from the window's edges
/// and 8 from the field.
pub mod list {
    pub const VIEWPORT_EDGE_GAP: f32 = 12.0;
    pub const INPUT_LIST_GAP: f32 = 8.0;
    pub const MAX_HEIGHT: f32 = 288.0;
}

/// A rectangle in window coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    pub fn bottom(&self) -> f32 {
        self.top + self.height
    }
}

/// Where the list goes: its left edge, width and the most it may be high; its top edge
/// below the field, or its bottom edge above it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ListPlacement {
    pub left: f32,
    pub width: f32,
    pub max_height: f32,
    /// `Below`: the top edge. `Above`: the bottom edge. Window coordinates.
    pub edge: f32,
    pub placement: SuggestionPlacement,
}

/// `updatePosition` of `SearchSuggestionsList` for the field at `anchor` in a window
/// `viewport_width` by `viewport_height`.
pub fn list_placement(
    anchor: Rect,
    viewport_width: f32,
    viewport_height: f32,
    placement: SuggestionPlacement,
) -> ListPlacement {
    let width = anchor
        .width
        .min(viewport_width - list::VIEWPORT_EDGE_GAP * 2.0)
        .max(0.0);
    let left = anchor
        .left
        .max(list::VIEWPORT_EDGE_GAP)
        .min(list::VIEWPORT_EDGE_GAP.max(viewport_width - list::VIEWPORT_EDGE_GAP - width));
    let (available, edge) = match placement {
        SuggestionPlacement::Above => (
            anchor.top - list::INPUT_LIST_GAP - list::VIEWPORT_EDGE_GAP,
            anchor.top - list::INPUT_LIST_GAP,
        ),
        SuggestionPlacement::Below => (
            viewport_height - anchor.bottom() - list::INPUT_LIST_GAP - list::VIEWPORT_EDGE_GAP,
            anchor.bottom() + list::INPUT_LIST_GAP,
        ),
    };
    ListPlacement {
        left,
        width,
        max_height: available.clamp(0.0, list::MAX_HEIGHT),
        edge,
        placement,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeName;

    fn source() -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../src/features/tagging/TagEditor.tsx");
        std::fs::read_to_string(path).expect("read TagEditor.tsx")
    }

    #[test]
    fn the_solid_editor_still_has_the_classes_copied_here() {
        let source = source();
        for class in [
            "flex max-h-[200px] min-h-[48px] flex-wrap items-start gap-1.5 overflow-auto rounded-[var(--radius-control)] border border-[var(--border-soft)] bg-base-200/38 p-2",
            "flex min-h-0 max-h-36 flex-wrap items-start gap-1.5 overflow-y-auto overscroll-contain rounded-[var(--radius-control)] border border-[var(--border-soft)] bg-[var(--surface-muted)] p-2",
            "px-2 py-1 font-mono text-xs font-medium leading-4 text-primary-text shadow-[var(--shadow-chip)]",
            "inline-flex h-5 w-5 shrink-0",
            "h-3 w-3",
            "flex-[1_1_9rem] max-w-full",
            "h-8 w-full min-w-0 border-0 bg-transparent px-1",
            "keepSuggestionsOpenOnPick",
            "autoSelectFirstSuggestion={false}",
        ] {
            assert!(source.contains(class), "TagEditor.tsx lost: {class}");
        }
    }

    #[test]
    fn the_content_prefers_loading_then_tags_then_the_empty_text() {
        assert_eq!(editor_content(true, 3, true), EditorContent::Loading);
        assert_eq!(editor_content(false, 3, true), EditorContent::Tags);
        assert_eq!(editor_content(false, 0, true), EditorContent::Empty);
        assert_eq!(editor_content(false, 0, false), EditorContent::Nothing);
        assert!(editing_disabled(true, false) && editing_disabled(false, true));
        assert!(!editing_disabled(false, false));
    }

    #[test]
    fn the_variants_differ_in_height_and_background() {
        let tokens = ThemeName::Dark.tokens();
        let lightbox = editor_visuals(tokens, TagEditorVariant::Lightbox);
        let bulk = editor_visuals(tokens, TagEditorVariant::Bulk);
        assert_eq!((lightbox.max_height, lightbox.min_height), (200.0, 48.0));
        assert_eq!((bulk.max_height, bulk.min_height), (144.0, 0.0));
        assert_eq!(bulk.background, tokens.colors.surface_muted);
        assert_eq!(TagEditorVariant::Bulk.test_id(), "bulk-tag-list");
        assert_eq!(
            TAG_EDITOR_COMBOBOX,
            ComboboxConfig {
                auto_select_first: false,
                pick_to_owner: true,
                keep_open_on_pick: true,
            }
        );
    }

    #[test]
    fn the_list_keeps_to_the_window() {
        let anchor = Rect {
            left: 100.0,
            top: 500.0,
            width: 300.0,
            height: 32.0,
        };
        let below = list_placement(anchor, 1440.0, 900.0, SuggestionPlacement::Below);
        assert_eq!((below.left, below.width, below.edge), (100.0, 300.0, 540.0));
        assert_eq!(below.max_height, 288.0);
        let above = list_placement(anchor, 1440.0, 600.0, SuggestionPlacement::Above);
        assert_eq!(above.edge, 492.0);
        assert_eq!(above.max_height, 288.0);
        let low = list_placement(anchor, 1440.0, 600.0, SuggestionPlacement::Below);
        assert_eq!(low.max_height, 600.0 - 532.0 - 8.0 - 12.0);
        let narrow = list_placement(
            Rect {
                left: 0.0,
                ..anchor
            },
            200.0,
            900.0,
            SuggestionPlacement::Below,
        );
        assert_eq!((narrow.left, narrow.width), (12.0, 176.0));
        let cramped = list_placement(
            Rect {
                top: 10.0,
                ..anchor
            },
            1440.0,
            900.0,
            SuggestionPlacement::Above,
        );
        assert_eq!(cramped.max_height, 0.0);
    }
}
