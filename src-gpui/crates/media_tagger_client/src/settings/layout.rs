//! The measures of the settings page, in CSS pixels: `src/styles.css` (`.settings-*`,
//! `.theme-*`, the base field) and the Tailwind classes of `SettingsPanel.tsx` and its
//! sections.

/// The page around the card: `px-4 pb-4 sm:px-6` (the window is never narrower than
/// `sm`).
pub const PAGE_PADDING_X: f32 = 24.0;
pub const PAGE_PADDING_BOTTOM: f32 = 16.0;
/// The bottom of the page view (`pb-8` of `AppSettingsView`).
pub const VIEW_PADDING_BOTTOM: f32 = 32.0;

/// `.settings-layout`: navigation and sections; two columns from 1000 px.
pub const LAYOUT_GAP: f32 = 16.0;
pub const LAYOUT_MAX_WIDTH: f32 = 1440.0;
pub const NAV_WIDTH: f32 = 240.0;

/// `.settings-nav` and its links.
pub mod nav {
    pub const PADDING: f32 = 8.0;
    pub const GAP: f32 = 4.0;
    /// `position: sticky; top: 96px` from the top of the page.
    pub const STICKY_TOP: f32 = 96.0;
    pub const LINK_GAP: f32 = 10.0;
    pub const LINK_PADDING_Y: f32 = 10.0;
    pub const LINK_PADDING_X: f32 = 12.0;
    pub const LINK_FONT_SIZE: f32 = 13.0;
    /// 20 px: the `h-4 w-4` the link gives `UiIcon` loses to the icon's own `h-5 w-5`,
    /// which comes later in the stylesheet. The icon is also what makes a link 40 px
    /// high (its text line is 19.5 px).
    pub const ICON_SIZE: f32 = 20.0;
    /// A link's height: the icon between the two paddings.
    pub const LINK_HEIGHT: f32 = ICON_SIZE + 2.0 * LINK_PADDING_Y;
    /// `box-shadow: inset 2px 0 var(--color-primary)` of the current link.
    pub const ACTIVE_EDGE: f32 = 2.0;
}

/// The column of sections (`grid gap-5`).
pub const SECTIONS_GAP: f32 = 20.0;

/// A section card: `grid gap-5 rounded-[var(--radius-surface)] border p-5`.
pub mod card {
    pub const PADDING: f32 = 20.0;
    pub const GAP: f32 = 20.0;
    /// The heading (`text-lg`, 18 px, `leading` 1.75rem).
    pub const HEADING_SIZE: f32 = 18.0;
    pub const HEADING_LINE_HEIGHT: f32 = 28.0;
    /// The description under it: `mt-1 text-sm leading-relaxed`.
    pub const DESCRIPTION_MARGIN_TOP: f32 = 4.0;
    pub const DESCRIPTION_SIZE: f32 = 13.0;
    pub const DESCRIPTION_LINE_HEIGHT: f32 = 1.625;
    /// The scan section's text column (`max-w-[680px]`).
    pub const SCAN_TEXT_MAX_WIDTH: f32 = 680.0;
    /// The highlight of the scan section opened by "Add first folder":
    /// `border-primary/60` with a 3 px ring of `primary/16`.
    pub const HIGHLIGHT_BORDER_PERCENT: u8 = 60;
    pub const HIGHLIGHT_RING: f32 = 3.0;
    pub const HIGHLIGHT_RING_PERCENT: u8 = 16;
}

/// The scan section's root list.
pub mod roots {
    /// `grid gap-3 border-t pt-4`.
    pub const BLOCK_GAP: f32 = 12.0;
    pub const BLOCK_PADDING_TOP: f32 = 16.0;
    /// The list heading: `text-xs font-semibold`.
    pub const HEADING_SIZE: f32 = 12.0;
    /// `grid gap-2`.
    pub const LIST_GAP: f32 = 8.0;
    /// A row: `gap-3 rounded border bg-base-200/38 p-2.5`.
    pub const ROW_GAP: f32 = 12.0;
    pub const ROW_PADDING: f32 = 10.0;
    pub const ROW_FILL_PERCENT: u8 = 38;
    /// The folder tile: `h-9 w-9 bg-primary/10` with an 18 px icon.
    pub const TILE_SIZE: f32 = 36.0;
    pub const TILE_FILL_PERCENT: u8 = 10;
    pub const ICON_SIZE: f32 = 18.0;
    /// The path: `text-[13px] font-mono`.
    pub const PATH_SIZE: f32 = 13.0;
    /// The row's actions: `gap-2`, the switch's label `gap-2 text-sm`.
    pub const ACTIONS_GAP: f32 = 8.0;
    /// "No scan paths": `p-5`.
    pub const EMPTY_PADDING: f32 = 20.0;
}

/// `.settings-operation`: a row with a description and its actions.
pub mod operation {
    pub const GAP: f32 = 16.0;
    pub const PADDING: f32 = 12.0;
    pub const TEXT_SIZE: f32 = 12.0;
    pub const TEXT_LINE_HEIGHT: f32 = 1.5;
    pub const ACTIONS_GAP: f32 = 8.0;
}

/// The video tool line: `flex gap-3 text-xs`.
pub const VIDEO_TOOLS_GAP: f32 = 12.0;

/// `SectionOperationStatus`: `grid gap-1`, the loading line `gap-2 text-xs`.
pub mod status {
    pub const GAP: f32 = 4.0;
    pub const LOADER_GAP: f32 = 8.0;
    pub const TEXT_SIZE: f32 = 12.0;
}

/// The theme cards (`.theme-choices`, `.theme-choice`, `.theme-preview`).
pub mod theme_choice {
    /// The fieldset: `max-w-[480px]`, two columns with a 12 px gap.
    pub const MAX_WIDTH: f32 = 480.0;
    pub const GAP: f32 = 12.0;
    /// The legend: `text-xs font-semibold`, 8 px above the cards.
    pub const LEGEND_SIZE: f32 = 12.0;
    pub const LEGEND_MARGIN_BOTTOM: f32 = 8.0;
    /// A card: `gap 8px; padding 8px; border: 1px solid; font-size 12px; font-weight
    /// 600`.
    pub const CARD_GAP: f32 = 8.0;
    pub const CARD_PADDING: f32 = 8.0;
    pub const CARD_BORDER: f32 = 1.0;
    pub const CARD_FONT_SIZE: f32 = 12.0;
    /// The card of the focused radio: `.choice-input:focus-visible + span { outline:
    /// 2px solid var(--color-primary); outline-offset: 2px; }`, 2 px off the card's
    /// border and not the 3 px of the base `:focus-visible` rule.
    pub const FOCUS_OUTLINE_OFFSET: f32 = 2.0;
    /// The preview: 64 px high, a 14 px bar over a tile row and a 24 % panel, 5 px
    /// gaps, radius 6.
    pub const PREVIEW_HEIGHT: f32 = 64.0;
    pub const PREVIEW_BAR: f32 = 14.0;
    pub const PREVIEW_GAP: f32 = 5.0;
    pub const PREVIEW_RADIUS: f32 = 6.0;
    pub const PREVIEW_PANEL_FRACTION: f32 = 0.24;
    /// The tiles: 28 % wide stripes with 5 % gaps, radius 3.
    pub const PREVIEW_TILE_FRACTION: f32 = 0.28;
    pub const PREVIEW_TILE_STEP: f32 = 0.33;
    pub const PREVIEW_TILE_RADIUS: f32 = 3.0;

    /// The colours of `.theme-preview--light` and `--dark`: workspace, panel, tile,
    /// border.
    pub const LIGHT: [u32; 4] = [0xf4f6f5, 0xffffff, 0xe8eeeb, 0xccd7d0];
    pub const DARK: [u32; 4] = [0x141618, 0x202224, 0x232528, 0x2c2e31];
}

/// The language select (`.theme-select h-9 w-full max-w-[480px] text-sm` over the
/// base field: `padding: 8px 10px`, `padding-right: 30px`, a 14 px chevron 8 px from
/// the right).
pub mod select {
    pub const HEIGHT: f32 = 36.0;
    pub const MAX_WIDTH: f32 = 480.0;
    pub const PADDING_X: f32 = 10.0;
    pub const PADDING_RIGHT: f32 = 30.0;
    pub const CHEVRON_SIZE: f32 = 14.0;
    pub const CHEVRON_RIGHT: f32 = 8.0;
    pub const FONT_SIZE: f32 = 13.0;
    /// The open list's rows.
    pub const OPTION_HEIGHT: f32 = 28.0;
    /// The chevron's stroke (`--select-chevron` of each theme), drawn at 1.8 on a
    /// 24-unit box.
    pub const CHEVRON_LIGHT: u32 = 0x455460;
    pub const CHEVRON_DARK: u32 = 0xadb8c0;
    pub const CHEVRON_STROKE: f32 = 1.8;
}

/// The danger zone: `gap-4 border-error/20 bg-error/5 p-5`, the heading `text-base`.
pub mod danger {
    pub const GAP: f32 = 16.0;
    pub const BORDER_PERCENT: u8 = 20;
    pub const FILL_PERCENT: u8 = 5;
    pub const HEADING_SIZE: f32 = 13.0;
    pub const HINT_MAX_WIDTH: f32 = 760.0;
    pub const HINT_SIZE: f32 = 12.0;
}

/// The import/export and the duplicate card: `grid gap-4 ... p-5`, a tighter column
/// than the other cards'.
pub mod data {
    pub const GAP: f32 = 16.0;
    /// The warning under the import/export status: `text-xs leading-[1.4] text-error`.
    pub const HINT_SIZE: f32 = 12.0;
    pub const HINT_LINE_HEIGHT: f32 = 1.4;
    /// The duplicate section's Start button and its last result: `gap-2`, `text-xs`.
    pub const START_GAP: f32 = 8.0;
    pub const RESULT_SIZE: f32 = 12.0;
}

/// The duplicate resolver (`DuplicateResolverDialog.tsx`, `DuplicateGroupCard.tsx`,
/// `DuplicateAssetCard.tsx`).
pub mod resolver {
    /// The dialog: `grid h-[min(88vh,900px)] grid-rows-[auto_auto_auto_1fr] gap-3`.
    pub const HEIGHT_SHARE: f32 = 0.88;
    pub const MAX_HEIGHT: f32 = 900.0;
    pub const GAP: f32 = 12.0;

    /// The dialog's height, padding included, in a viewport `viewport_height` high.
    pub fn height(viewport_height: f32) -> f32 {
        (viewport_height * HEIGHT_SHARE).min(MAX_HEIGHT)
    }

    /// The header: heading over the summary (`grid gap-1`), the actions beside them
    /// (`gap-2`).
    pub const TITLE_GAP: f32 = 4.0;
    pub const ACTIONS_GAP: f32 = 8.0;
    /// The summary and the hint line: `text-xs`.
    pub const SMALL_SIZE: f32 = 12.0;
    /// The list: `overflow-auto pr-1`, each group `pb-2` below it. Rows are estimated
    /// at 360 px until they are measured, and two more than fit are kept drawn.
    pub const LIST_PADDING_RIGHT: f32 = 4.0;
    pub const ROW_GAP: f32 = 8.0;
    pub const ESTIMATED_ROW: f32 = 360.0;
    pub const OVERSCAN_ROWS: f32 = 2.0;
    /// How many files of a group one row of the list holds. A group with more is drawn
    /// as several rows that join into one card, so the cards built, their rename fields
    /// and the previews asked for depend on what is near the visible part and not on
    /// the size of the group.
    pub const ROW_ASSETS: usize = 8;
    /// "No duplicate groups found": `rounded border border-base-content/20
    /// bg-base-200/40 p-3 text-sm`.
    pub const EMPTY_PADDING: f32 = 12.0;
    pub const EMPTY_BORDER_PERCENT: u8 = 20;
    pub const EMPTY_FILL_PERCENT: u8 = 40;

    /// A group card: `grid gap-2 rounded border p-3`, `border-base-content/20
    /// bg-base-100/75`, or `border-primary/40 bg-primary/10` once it is resolved.
    pub const GROUP_GAP: f32 = 8.0;
    pub const GROUP_PADDING: f32 = 12.0;
    pub const GROUP_BORDER_PERCENT: u8 = 20;
    pub const GROUP_FILL_PERCENT: u8 = 75;
    pub const RESOLVED_BORDER_PERCENT: u8 = 40;
    pub const RESOLVED_FILL_PERCENT: u8 = 10;
    /// The group's name: `text-sm`.
    pub const GROUP_NAME_SIZE: f32 = 13.0;
    /// The "Ready" badge: `border-primary/40 bg-primary/15 px-2 py-0.5 text-[10px]
    /// font-semibold uppercase tracking-[0.12em]`.
    pub const BADGE_PADDING_X: f32 = 8.0;
    pub const BADGE_PADDING_Y: f32 = 2.0;
    pub const BADGE_SIZE: f32 = 10.0;
    pub const BADGE_LINE_HEIGHT: f32 = 1.5;
    pub const BADGE_TRACKING_EM: f32 = 0.12;
    pub const BADGE_FILL_PERCENT: u8 = 15;

    /// An asset card: `grid gap-3 rounded border border-base-content/15 bg-base-200/35
    /// p-2.5`, two columns (`150px minmax(0,1fr)`) from 768 px (`md`).
    pub const ASSET_GAP: f32 = 12.0;
    pub const ASSET_PADDING: f32 = 10.0;
    pub const ASSET_BORDER_PERCENT: u8 = 15;
    pub const ASSET_FILL_PERCENT: u8 = 35;
    pub const TWO_COLUMNS_FROM: f32 = 768.0;
    /// The preview: `h-[150px] w-[150px] border-base-content/15 bg-base-100/80`, its
    /// loader on `bg-base-100/60`.
    pub const PREVIEW_SIZE: f32 = 150.0;
    pub const PREVIEW_FILL_PERCENT: u8 = 80;
    pub const LOADER_VEIL_PERCENT: u8 = 60;
    /// The details column: `grid content-start gap-2.5`, lines `text-xs`.
    pub const DETAILS_GAP: f32 = 10.0;
    /// The rename field: `input input-sm h-10 w-full border border-base-content/20
    /// bg-base-100/70`. daisyUI's `.input` replaces the frame of the stylesheet's
    /// `input`: `padding-inline: 0.75rem`, no shadow (`--depth: 0`) and, while it has
    /// the focus, `outline: 2px solid` in the base content colour, 2 px off the
    /// border; `.input-sm` makes the text 12 px. Disabled it keeps the 0.55 opacity of
    /// `input:disabled` over the frame and the text, and daisyUI's `.input:disabled`
    /// makes the text 40 % of the base content (`text::FieldChrome::Framed`): 0.22 in
    /// all. The border and the fill stay those of the utility classes, which win over
    /// daisyUI's layer.
    pub const INPUT_HEIGHT: f32 = 40.0;
    pub const INPUT_PADDING_X: f32 = 12.0;
    pub const INPUT_FONT_SIZE: f32 = 12.0;
    pub const INPUT_BORDER_PERCENT: u8 = 20;
    pub const INPUT_FILL_PERCENT: u8 = 70;
    pub const INPUT_OUTLINE_WIDTH: f32 = 2.0;
    pub const INPUT_OUTLINE_OFFSET: f32 = 2.0;
    pub const INPUT_DISABLED_OPACITY: f32 = 0.55;
    pub const BUTTONS_GAP: f32 = 8.0;
}

/// The About section.
pub mod about {
    /// The product name: `mt-1 text-xl font-semibold`.
    pub const NAME_SIZE: f32 = 20.0;
    /// The metadata list: `gap-x-5 gap-y-2 text-sm`.
    pub const LIST_GAP_X: f32 = 20.0;
    pub const LIST_GAP_Y: f32 = 8.0;
    pub const TEXT_SIZE: f32 = 13.0;
    pub const LINKS_GAP: f32 = 8.0;
    /// A folding text: `rounded border p-3`, its text `mt-3 max-h-80 text-xs`.
    pub const DETAILS_PADDING: f32 = 12.0;
    pub const DETAILS_TEXT_MARGIN_TOP: f32 = 12.0;
    pub const DETAILS_TEXT_MAX_HEIGHT: f32 = 320.0;
    pub const DETAILS_TEXT_SIZE: f32 = 12.0;
    /// The `<summary>` disclosure marker WebKit draws before the label (`list-style:
    /// disclosure-closed`/`disclosure-open`): a filled triangle in an 11 px tall box,
    /// 10 px wide when closed, and the label 15 px after the marker's start.
    pub const MARKER_WIDTH: f32 = 15.0;
    pub const MARKER_HEIGHT: f32 = 11.0;
    pub const MARKER_TRIANGLE: f32 = 10.0;
}

/// The heading of a dialog: an `h3` with no size class, so the 18 px of `h2, h3` on the
/// root's line height of 1.5. That is 27 px, one less than a section's `text-lg`
/// heading, which brings its own line height.
pub mod dialog {
    pub const HEADING_SIZE: f32 = 18.0;
    pub const HEADING_LINE_HEIGHT: f32 = 27.0;
}

/// The two confirmations: `grid gap-2.5`, description `text-[13px] leading-[1.4]`,
/// the remove dialog's path box `px-2.5 py-2 text-xs`, buttons `gap-2`.
pub mod confirm {
    pub const GAP: f32 = 10.0;
    pub const DESCRIPTION_SIZE: f32 = 13.0;
    pub const DESCRIPTION_LINE_HEIGHT: f32 = 1.4;
    pub const PATH_PADDING_X: f32 = 10.0;
    pub const PATH_PADDING_Y: f32 = 8.0;
    pub const PATH_SIZE: f32 = 12.0;
    pub const PATH_BORDER_PERCENT: u8 = 20;
    pub const PATH_FILL_PERCENT: u8 = 50;
    pub const BUTTONS_GAP: f32 = 8.0;
}

#[cfg(test)]
mod tests {
    use super::{dialog, resolver};
    use crate::theme::TYPOGRAPHY;

    const STYLESHEET: &str = include_str!("../../../../../src/styles.css");
    const ASSET_CARD: &str =
        include_str!("../../../../../src/features/settings/DuplicateAssetCard.tsx");

    #[test]
    fn a_dialog_heading_is_an_h3_on_the_root_line_height() {
        assert!(STYLESHEET.contains("h2, h3 { font-size: 18px; }"));
        assert_eq!(dialog::HEADING_SIZE, TYPOGRAPHY.h2_size);
        assert_eq!(
            dialog::HEADING_LINE_HEIGHT,
            TYPOGRAPHY.h2_size * TYPOGRAPHY.root_line_height
        );
        // No size class on the headings: `text-lg` would bring its own line height.
        for (name, source) in [
            (
                "DuplicateResolverDialog.tsx",
                include_str!("../../../../../src/features/settings/DuplicateResolverDialog.tsx"),
            ),
            (
                "DuplicateDeleteConfirmDialog.tsx",
                include_str!(
                    "../../../../../src/features/settings/DuplicateDeleteConfirmDialog.tsx"
                ),
            ),
            (
                "ImportDbOverwriteConfirmDialog.tsx",
                include_str!(
                    "../../../../../src/features/settings/ImportDbOverwriteConfirmDialog.tsx"
                ),
            ),
            (
                "ClearLibraryConfirmDialog.tsx",
                include_str!("../../../../../src/features/settings/ClearLibraryConfirmDialog.tsx"),
            ),
            (
                "RemoveScanRootConfirmDialog.tsx",
                include_str!(
                    "../../../../../src/features/settings/RemoveScanRootConfirmDialog.tsx"
                ),
            ),
        ] {
            let heading = source
                .split("<h3 ")
                .nth(1)
                .and_then(|rest| rest.split('>').next())
                .unwrap_or_else(|| panic!("{name} has a heading"));
            assert!(heading.contains("class=\"m-0\""), "{name}: {heading}");
        }
    }

    #[test]
    fn the_rename_field_still_has_the_classes_its_frame_copies() {
        assert!(
            ASSET_CARD.contains(
                "class=\"input input-sm h-10 w-full rounded-[var(--radius-control)] border \
                 border-base-content/20 bg-base-100/70\""
            ),
            "DuplicateAssetCard.tsx changed the rename field's classes"
        );
        assert!(STYLESHEET.contains("--depth: 0;"), "daisyUI's field shadow");
        assert!(STYLESHEET
            .contains("input:disabled, select:disabled { cursor: not-allowed; opacity: 0.55; }"));
        assert_eq!(resolver::INPUT_FONT_SIZE, TYPOGRAPHY.xs.size);
        assert_eq!(resolver::INPUT_DISABLED_OPACITY, 0.55);
        // The text of the disabled field: 40 % of the base content under that opacity.
        let text = f32::from(crate::text::FRAMED_DISABLED_TEXT_PERCENT) / 100.0;
        assert!((text * resolver::INPUT_DISABLED_OPACITY - 0.22).abs() < 1e-6);
    }

    #[test]
    fn a_theme_card_has_the_outline_of_a_choice_input_around_its_border() {
        assert!(STYLESHEET.contains(
            ".choice-input:focus-visible + span { outline: 2px solid var(--color-primary); outline-offset: 2px; }"
        ));
        assert!(STYLESHEET.contains(
            ".theme-choice > span { display: grid; gap: 8px; padding: 8px; border: 1px solid var(--border-soft);"
        ));
        assert_eq!(
            (
                super::theme_choice::FOCUS_OUTLINE_OFFSET,
                super::theme_choice::CARD_BORDER
            ),
            (2.0, 1.0)
        );
    }
}
