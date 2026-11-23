//! The measures of the lightbox, in CSS pixels: the Tailwind classes of
//! `LightboxModal.tsx`, `LightboxToolbar.tsx`, `LightboxActionRail.tsx`,
//! `LightboxMediaStage.tsx`, `LightboxDeleteConfirmDialog.tsx`, `LightboxInfoPanel.tsx`,
//! `LightboxTagPanel.tsx` and `MediaGroupSetter.tsx` and the `.lightbox-*` rules of
//! `src/styles.css`.

use super::media::StageKind;
use crate::theme::{Rgba, Shadow};

/// Tailwind's `sm` and `lg` breakpoints.
pub const BREAKPOINT_SM: f32 = 640.0;
pub const BREAKPOINT_LG: f32 = 1024.0;

/// The backdrop and the dialog on it.
pub mod shell {
    use super::{StageKind, BREAKPOINT_LG, BREAKPOINT_SM};

    /// `bg-neutral/62`.
    pub const BACKDROP_PERCENT: u8 = 62;
    /// `w-[min(calc(100vw-40px),…)]` and `h-[min(calc(100dvh-40px),…)]`.
    pub const VIEWPORT_INSET: f32 = 40.0;
    pub const MAX_WIDTH: f32 = 1800.0;
    pub const MAX_WIDTH_SM: f32 = 1860.0;
    pub const MAX_WIDTH_LG: f32 = 1920.0;
    pub const MAX_HEIGHT: f32 = 1180.0;
    pub const MAX_HEIGHT_LG: f32 = 1280.0;

    /// The size of the windowed dialog in a window of this size.
    pub fn size(viewport_width: f32, viewport_height: f32) -> (f32, f32) {
        let max_width = if viewport_width >= BREAKPOINT_LG {
            MAX_WIDTH_LG
        } else if viewport_width >= BREAKPOINT_SM {
            MAX_WIDTH_SM
        } else {
            MAX_WIDTH
        };
        let max_height = if viewport_width >= BREAKPOINT_LG {
            MAX_HEIGHT_LG
        } else {
            MAX_HEIGHT
        };
        (
            (viewport_width - VIEWPORT_INSET).min(max_width).max(0.0),
            (viewport_height - VIEWPORT_INSET).min(max_height).max(0.0),
        )
    }

    /// The backdrop's padding: `p-5` for a video, `p-2 sm:p-5` otherwise.
    pub fn backdrop_padding(kind: StageKind, viewport_width: f32) -> f32 {
        if kind.is_video() || viewport_width >= BREAKPOINT_SM {
            20.0
        } else {
            8.0
        }
    }
}

/// The video's surface: `--lightbox-video-surface` and the border of
/// `.lightbox-shell--video`.
pub mod video {
    use super::Rgba;

    pub const SURFACE: Rgba = Rgba::hex(0x0b0b0b);
    /// `rgb(240 240 232 / 0.14)`.
    pub const SHELL_BORDER: Rgba = Rgba::hex(0xf0f0e8).with_alpha_percent(14);
    /// `pt-11`: the strip above a windowed video that keeps the floating buttons
    /// clear of the surface.
    pub const TOP_RESERVE: f32 = 44.0;
    /// The details notice sits above the player's controls: `bottom-[88px]`.
    pub const DETAILS_NOTICE_BOTTOM: f32 = 88.0;
}

/// The side panel (`LightboxToolbar`).
pub mod sidebar {
    /// `grid-cols-[minmax(0,1fr)_340px]` and `w-[340px]`; a drawer is
    /// `w-[min(340px,100%)]`.
    pub const WIDTH: f32 = 340.0;
    /// `p-4`.
    pub const PADDING: f32 = 16.0;
    /// `gap-3` between header, scrolling part and confirmation.
    pub const GAP: f32 = 12.0;
    /// The header: `gap-2 pb-3`, the title `text-sm font-semibold`.
    pub const HEADER_GAP: f32 = 8.0;
    pub const HEADER_PADDING_BOTTOM: f32 = 12.0;
    /// `gap-1` between the header's buttons, each `h-8! w-8!`.
    pub const HEADER_BUTTON_GAP: f32 = 4.0;
    pub const BUTTON_SIZE: f32 = 32.0;
    /// The scrolling part: `gap-4 pr-2`.
    pub const SECTION_GAP: f32 = 16.0;
    pub const SCROLL_PADDING_RIGHT: f32 = 8.0;
    /// The media group section: `gap-1.5`.
    pub const GROUP_GAP: f32 = 6.0;
    /// The group fields and Apply: `h-8`.
    pub const FIELD_HEIGHT: f32 = 32.0;
    /// The tag section: `gap-2 border-t pt-4`.
    pub const TAG_GAP: f32 = 8.0;
    pub const TAG_PADDING_TOP: f32 = 16.0;
    /// The Information toggle: `border-t py-3 text-xs`.
    pub const INFO_TOGGLE_PADDING_Y: f32 = 12.0;
    /// The drawer of a narrow window lies on `bg-neutral/52`.
    pub const SCRIM_PERCENT: u8 = 52;
    /// `max-h-[calc(100dvh-100px)]` of the confirmation's box.
    pub const CONFIRM_VIEWPORT_INSET: f32 = 100.0;

    /// The width of the panel in a dialog `shell_width` wide.
    pub fn width(shell_width: f32) -> f32 {
        WIDTH.min(shell_width).max(0.0)
    }
}

/// The Information panel: `gap-1.5 … p-3 text-xs`, its rows `gap-2 font-mono text-xs`.
pub mod info {
    pub const GAP: f32 = 6.0;
    pub const PADDING: f32 = 12.0;
    pub const ROW_GAP: f32 = 8.0;
    /// `leading-relaxed` of the path.
    pub const PATH_LINE_HEIGHT: f32 = 1.625;
}

/// The delete confirmation: `gap-2.5 … border-error/32 bg-base-200/45 p-3`.
pub mod confirm {
    pub const GAP: f32 = 10.0;
    pub const PADDING: f32 = 12.0;
    pub const BORDER_PERCENT: u8 = 32;
    pub const BACKGROUND_PERCENT: u8 = 45;
    /// `text-[13px] leading-[1.4]`.
    pub const DESCRIPTION_SIZE: f32 = 13.0;
    pub const DESCRIPTION_LINE_HEIGHT: f32 = 1.4;
    /// `gap-1` between the label and the field, `h-10` of the field.
    pub const FIELD_GAP: f32 = 4.0;
    pub const FIELD_HEIGHT: f32 = 40.0;
    /// `gap-2` between Cancel and Confirm.
    pub const BUTTON_GAP: f32 = 8.0;
}

/// The action rail (`.lightbox-action-row`, `.lightbox-action-island`,
/// `.lightbox-media-summary`).
pub mod rail {
    /// `padding: 12px 16px 16px`.
    pub const ROW_PADDING_TOP: f32 = 12.0;
    pub const ROW_PADDING_X: f32 = 16.0;
    pub const ROW_PADDING_BOTTOM: f32 = 16.0;
    /// `gap: 8px; padding: 8px 12px`.
    pub const ISLAND_GAP: f32 = 8.0;
    pub const ISLAND_PADDING_Y: f32 = 8.0;
    pub const ISLAND_PADDING_X: f32 = 12.0;
    pub const BUTTON_SIZE: f32 = 32.0;
    /// `gap: 8px; padding: 4px 12px; border-radius: 4px; font: 11px var(--font-mono)`.
    pub const SUMMARY_GAP: f32 = 8.0;
    pub const SUMMARY_PADDING_Y: f32 = 4.0;
    pub const SUMMARY_PADDING_X: f32 = 12.0;
    pub const SUMMARY_RADIUS: f32 = 4.0;
    pub const SUMMARY_FONT_SIZE: f32 = 11.0;
    /// `transition-opacity duration-200` of the rail and of the floating buttons.
    pub const FADE_MS: u64 = 200;
    /// `@media (max-width: 599px)`: up to this window width the rail is compact: the
    /// row's padding is 8, the island's gap and padding are 4, and the summary takes a
    /// row of its own above the buttons.
    pub const COMPACT_MAX_WIDTH: f32 = 599.0;
    pub const COMPACT_ROW_PADDING: f32 = 8.0;
    pub const COMPACT_ISLAND_GAP: f32 = 4.0;
    pub const COMPACT_ISLAND_PADDING: f32 = 4.0;

    /// Whether the rail is compact in a window `viewport_width` wide.
    pub fn compact(viewport_width: f32) -> bool {
        viewport_width <= COMPACT_MAX_WIDTH
    }
}

/// What floats over the media column.
pub mod floating {
    use super::{Rgba, Shadow};

    /// The reopen and close buttons: `right-3 top-1.5 gap-2`, each 32 px.
    pub const CONTROLS_RIGHT: f32 = 12.0;
    pub const CONTROLS_TOP: f32 = 6.0;
    pub const CONTROLS_GAP: f32 = 8.0;
    pub const CONTROL_SIZE: f32 = 32.0;
    /// The favorite and group failures: `left-3 top-3 gap-2
    /// max-w-[min(28rem,calc(100%-6rem))]`.
    pub const ALERTS_INSET: f32 = 12.0;
    pub const ALERTS_GAP: f32 = 8.0;
    pub const ALERTS_MAX_WIDTH: f32 = 448.0;
    pub const ALERTS_SIDE_RESERVE: f32 = 96.0;
    /// The navigation notice: `bottom-3 left-3 rounded-lg bg-base-100 p-3 shadow-lg`,
    /// its Retry `btn btn-sm ml-3`.
    pub const NOTICE_INSET: f32 = 12.0;
    pub const NOTICE_PADDING: f32 = 12.0;
    pub const NOTICE_RADIUS: f32 = 8.0;
    pub const NOTICE_BUTTON_MARGIN: f32 = 12.0;
    /// Tailwind's `shadow-lg` without its negative spreads.
    pub const NOTICE_SHADOW: &[Shadow] = &[
        Shadow::drop(10.0, 15.0, Rgba::hex(0x000000).with_alpha_percent(10)),
        Shadow::drop(4.0, 6.0, Rgba::hex(0x000000).with_alpha_percent(10)),
    ];
    /// The details notice beside a shown medium: `left-4 bottom-4 gap-3 px-3 py-2
    /// text-xs border-warning/52 bg-base-100/92`.
    pub const DETAILS_NOTICE_INSET: f32 = 16.0;
    pub const DETAILS_NOTICE_GAP: f32 = 12.0;
    pub const DETAILS_NOTICE_PADDING_X: f32 = 12.0;
    pub const DETAILS_NOTICE_PADDING_Y: f32 = 8.0;
    pub const DETAILS_NOTICE_BORDER_PERCENT: u8 = 52;
    pub const DETAILS_NOTICE_BACKGROUND_PERCENT: u8 = 92;

    /// The width the failure alerts may take in a media column `column_width` wide.
    pub fn alerts_max_width(column_width: f32) -> f32 {
        ALERTS_MAX_WIDTH
            .min(column_width - ALERTS_SIDE_RESERVE)
            .max(0.0)
    }
}

/// The media stage.
pub mod stage {
    /// The centred notices: `p-6` around, `px-5 py-4 text-sm font-semibold` inside,
    /// `gap-3` before a button.
    pub const NOTICE_MARGIN: f32 = 24.0;
    pub const NOTICE_PADDING_X: f32 = 20.0;
    pub const NOTICE_PADDING_Y: f32 = 16.0;
    pub const NOTICE_GAP: f32 = 12.0;
    /// `border-warning/52 bg-warning/14` and `border-error/52 bg-error/14`.
    pub const NOTICE_BORDER_PERCENT: u8 = 52;
    pub const NOTICE_BACKGROUND_PERCENT: u8 = 14;
    /// The spinner while the details load: `h-[34px] w-[34px] border-[3px]`.
    pub const SPINNER_SIZE: f32 = 34.0;
    /// The GIF chip sits this far from the stage's top left corner.
    pub const CHIP_INSET: f32 = 12.0;
    /// The video placeholder: the poster is at most this share of the stage, with
    /// the label and the metadata lines under it.
    pub const POSTER_SHARE: f32 = 0.6;
    pub const PLACEHOLDER_GAP: f32 = 12.0;
    pub const PLACEHOLDER_LINE_GAP: f32 = 4.0;
    pub const PLACEHOLDER_TEXT_PERCENT: u8 = 72;

    /// The largest box of the poster in a stage of this size.
    pub fn poster_box(stage_width: f32, stage_height: f32) -> (f32, f32) {
        (
            (stage_width * POSTER_SHARE).max(0.0),
            (stage_height * POSTER_SHARE).max(0.0),
        )
    }

    /// The poster of an asset `width` x `height` (16:9 without stored dimensions)
    /// fitted into [`poster_box`].
    pub fn poster_size(
        width: Option<i64>,
        height: Option<i64>,
        stage_width: f32,
        stage_height: f32,
    ) -> (f32, f32) {
        let (width, height) = match (width, height) {
            (Some(width), Some(height)) if width > 0 && height > 0 => (width as f32, height as f32),
            _ => (16.0, 9.0),
        };
        let (box_width, box_height) = poster_box(stage_width, stage_height);
        let scale = (box_width / width).min(box_height / height);
        (width * scale, height * scale)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dialog_keeps_twenty_pixels_around_itself_up_to_its_largest_size() {
        assert_eq!(shell::size(1440.0, 900.0), (1400.0, 860.0));
        assert_eq!(shell::size(3840.0, 2160.0), (1920.0, 1280.0));
        assert_eq!(shell::size(900.0, 1400.0), (860.0, 1180.0));
        assert_eq!(shell::size(600.0, 500.0), (560.0, 460.0));
        assert_eq!(shell::size(20.0, 20.0), (0.0, 0.0));
        assert_eq!(shell::backdrop_padding(StageKind::Image, 1440.0), 20.0);
        assert_eq!(shell::backdrop_padding(StageKind::Image, 600.0), 8.0);
        assert_eq!(shell::backdrop_padding(StageKind::Video, 600.0), 20.0);
    }

    #[test]
    fn the_drawer_is_never_wider_than_the_dialog() {
        assert_eq!(sidebar::width(1400.0), 340.0);
        assert_eq!(sidebar::width(300.0), 300.0);
        assert_eq!(floating::alerts_max_width(1060.0), 448.0);
        assert_eq!(floating::alerts_max_width(400.0), 304.0);
    }

    #[test]
    fn the_poster_keeps_the_assets_shape_inside_its_box() {
        let close = |(width, height): (f32, f32), expected: (f32, f32)| {
            assert!(
                (width - expected.0).abs() < 0.01 && (height - expected.1).abs() < 0.01,
                "{width} x {height} is not {expected:?}"
            );
        };
        close(
            stage::poster_size(Some(160), Some(90), 1000.0, 1000.0),
            (600.0, 337.5),
        );
        close(
            stage::poster_size(Some(90), Some(160), 1000.0, 800.0),
            (270.0, 480.0),
        );
        close(
            stage::poster_size(None, Some(90), 1600.0, 900.0),
            (960.0, 540.0),
        );
    }

    #[test]
    fn the_video_surface_is_the_stylesheets() {
        assert_eq!(video::SURFACE.to_rgb_u32(), 0x0b0b0b);
        assert_eq!(video::SHELL_BORDER.a, 36);
    }
}
