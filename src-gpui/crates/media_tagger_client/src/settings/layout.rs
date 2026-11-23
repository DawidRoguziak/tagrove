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
    /// A card: `gap 8px; padding 8px; font-size 12px; font-weight 600`.
    pub const CARD_GAP: f32 = 8.0;
    pub const CARD_PADDING: f32 = 8.0;
    pub const CARD_FONT_SIZE: f32 = 12.0;
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
