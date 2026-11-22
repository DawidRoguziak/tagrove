//! The measures of the dock, the order board and the larger order view, in CSS pixels
//! (`.bulk-dock*` of `src/styles.css` and the Tailwind classes of `BulkDock.tsx`,
//! `GroupOrderBoard.tsx` and `GroupOrderModal.tsx`).

/// `.bulk-dock`, `.bulk-dock--expanded`, `.bulk-dock--tall`.
pub mod dock {
    /// `bottom: 16px`; also the least space kept on each side and above.
    pub const MARGIN: f32 = 16.0;
    /// The collapsed chip: `width: 100px`.
    pub const CHIP_WIDTH: f32 = 100.0;
    pub const CHIP_PADDING: f32 = 6.0;
    /// `border-radius: 999px`.
    pub const CHIP_RADIUS: f32 = 999.0;
    /// `.bulk-dock-chip`: `min-height: 28px; gap: 6px; font-size: 12px`.
    pub const CHIP_MIN_HEIGHT: f32 = 28.0;
    pub const CHIP_GAP: f32 = 6.0;
    pub const CHIP_FONT_SIZE: f32 = 12.0;
    /// `UiIcon` with `h-4 w-4`: its own `h-5 w-5` comes later in the stylesheet and
    /// wins, so the icon is 20 pixels.
    pub const CHIP_ICON_SIZE: f32 = 20.0;
    /// `gap: 10px` between the dock's parts.
    pub const GAP: f32 = 10.0;
    /// `width: min(760px, calc(100% - 32px))`.
    pub const MAX_WIDTH: f32 = 760.0;
    pub const PADDING: f32 = 12.0;
    /// `height: min(72cqh, 680px)`.
    pub const TALL_SHARE: f32 = 0.72;
    pub const TALL_MAX_HEIGHT: f32 = 680.0;

    /// The width of the expanded dock in a viewport `viewport_width` wide.
    pub fn expanded_width(viewport_width: f32) -> f32 {
        MAX_WIDTH.min(viewport_width - 2.0 * MARGIN).max(0.0)
    }

    /// `max-height: calc(100cqh - 32px)`.
    pub fn max_height(viewport_height: f32) -> f32 {
        (viewport_height - 2.0 * MARGIN).max(0.0)
    }

    /// The fixed height of the dock on the group tab, never more than
    /// [`max_height`].
    pub fn tall_height(viewport_height: f32) -> f32 {
        (TALL_SHARE * viewport_height)
            .min(TALL_MAX_HEIGHT)
            .min(max_height(viewport_height))
    }
}

/// The header row: the tag field, the heart and the counter.
pub mod header {
    pub const GAP: f32 = 8.0;
    /// The heart: `h-[50px]! w-10!`.
    pub const HEART_HEIGHT: f32 = 50.0;
    pub const HEART_WIDTH: f32 = 40.0;
    /// `.bulk-dock-count`.
    pub const COUNT_HEIGHT: f32 = 50.0;
    pub const COUNT_MIN_WIDTH: f32 = 56.0;
    pub const COUNT_PADDING_X: f32 = 12.0;
    pub const COUNT_GAP: f32 = 6.0;
    pub const COUNT_FONT_SIZE: f32 = 13.0;
    /// `UiIcon` with `h-3.5 w-3.5`: drawn at its own `h-5 w-5`, as above.
    pub const COUNT_ICON_SIZE: f32 = 20.0;
    pub const COUNT_DISABLED_OPACITY: f32 = 0.5;
}

/// `.bulk-dock-tabs` and `.bulk-dock-tab`.
pub mod tabs {
    pub const GAP: f32 = 4.0;
    pub const PADDING_X: f32 = 12.0;
    pub const PADDING_Y: f32 = 6.0;
    pub const FONT_SIZE: f32 = 13.0;
    /// The selected tab's line, which lies on the tablist's own 1px line.
    pub const INDICATOR: f32 = 2.0;
    pub const RULE: f32 = 1.0;
}

/// The group tab.
pub mod group {
    /// `gap-3` between the rows of the panel.
    pub const GAP: f32 = 12.0;
    /// `gap-2` in the key row and in the order panel.
    pub const ROW_GAP: f32 = 8.0;
    /// The key field: `h-9`.
    pub const INPUT_HEIGHT: f32 = 36.0;
    /// The key row is `grid-cols-[minmax(0,1fr)_40px_auto]`: the "Generate UUID" button
    /// (36 pixels wide) sits at the start of a fixed track of this width.
    pub const KEY_BUTTON_TRACK: f32 = 40.0;
    /// The order panel keeps room for a row of tiles: `min-h-40`.
    pub const ORDER_PANEL_MIN_HEIGHT: f32 = 160.0;
    /// The smallest tile of the inline board and of the larger view.
    pub const INLINE_MIN_TILE_WIDTH: f32 = 150.0;
    pub const MODAL_MIN_TILE_WIDTH: f32 = 280.0;
}

/// `GroupOrderBoard`.
pub mod board {
    /// `gap-3` between tiles; rows are laid out by their own height below.
    pub const GAP: f32 = 12.0;
    /// The preview is `round(tile width * 0.66)` tall.
    pub const PREVIEW_RATIO: f32 = 0.66;
    /// A row of the virtual list is the preview plus this.
    pub const ROW_EXTRA: f32 = 56.0;
    /// The caption under the preview: `h-11`, its 1px line (`border-t`) included.
    pub const CAPTION_HEIGHT: f32 = 44.0;
    pub const CAPTION_PADDING_X: f32 = 12.0;
    pub const CAPTION_GAP: f32 = 8.0;
    pub const BORDER: f32 = 1.0;
    /// The grip: `h-8 w-8` with an icon of `h-5 w-5`.
    pub const HANDLE_SIZE: f32 = 32.0;
    pub const HANDLE_ICON_SIZE: f32 = 20.0;
    /// `border-base-content/15`, `hover:bg-base-content/10`, `opacity-50`.
    pub const BORDER_PERCENT: u8 = 15;
    pub const HANDLE_HOVER_PERCENT: u8 = 10;
    pub const DRAGGED_OPACITY: f32 = 0.5;
    pub const LOCKED_HANDLE_OPACITY: f32 = 0.5;
    /// Rows drawn above and below the visible ones.
    pub const OVERSCAN: usize = 2;
    /// The viewport height assumed before the board was laid out.
    pub const INITIAL_VIEWPORT: f32 = 560.0;
    /// The width assumed before the board was laid out.
    pub const INITIAL_WIDTH: f32 = 960.0;
    /// A press becomes a drag after this many pixels.
    pub const DRAG_THRESHOLD: f32 = 5.0;
    /// Edge scrolling: within this distance of the top or bottom edge, up to
    /// `EDGE_SPEED` pixels per frame.
    pub const EDGE: f32 = 56.0;
    pub const EDGE_SPEED: f32 = 18.0;
    /// The insertion line: `w-1`, 2px below the row's top, 16px shorter than the row.
    pub const LINE_WIDTH: f32 = 4.0;
    pub const LINE_TOP: f32 = 2.0;
    pub const LINE_SHORTER: f32 = 16.0;
    /// Where the line lies beside its target: 4px after it, or 8px before it.
    pub const LINE_AFTER: f32 = 4.0;
    pub const LINE_BEFORE: f32 = 8.0;
    /// The ghost under the pointer: `h-28 w-28 border-2 opacity-90`, 14px off.
    pub const GHOST_SIZE: f32 = 112.0;
    pub const GHOST_OFFSET: f32 = 14.0;
    pub const GHOST_BORDER: f32 = 2.0;
    pub const GHOST_OPACITY: f32 = 0.9;
    /// `bg-base-100/55` behind the loader of a thumbnail on its way.
    pub const LOADER_VEIL_PERCENT: u8 = 55;
}

/// `GroupOrderModal`.
pub mod order_modal {
    /// `h-[90vh]`, inside the overlay's padding.
    pub const HEIGHT_SHARE: f32 = 0.9;
    /// `gap-4`.
    pub const GAP: f32 = 16.0;
    /// The footer: `gap-3 pt-4`, its buttons `gap-2`.
    pub const FOOTER_GAP: f32 = 12.0;
    pub const FOOTER_PADDING_TOP: f32 = 16.0;
    pub const BUTTON_GAP: f32 = 8.0;

    /// The height of the dialog in a viewport `viewport_height` tall with
    /// `overlay_padding` around the dialog.
    pub fn height(viewport_height: f32, overlay_padding: f32) -> f32 {
        (HEIGHT_SHARE * viewport_height)
            .min(viewport_height - 2.0 * overlay_padding)
            .max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::{dock, order_modal};

    #[test]
    fn the_dock_never_outgrows_its_viewport() {
        assert_eq!(dock::expanded_width(1440.0), 760.0);
        assert_eq!(dock::expanded_width(600.0), 568.0);
        assert_eq!(dock::tall_height(1000.0), 680.0);
        assert_eq!(dock::tall_height(500.0), 360.0);
        // A very low viewport: the tall dock still keeps its margins.
        assert_eq!(dock::tall_height(100.0), 68.0);
        assert_eq!(dock::max_height(100.0), 68.0);
        assert_eq!(order_modal::height(1000.0, 20.0), 900.0);
    }
}
