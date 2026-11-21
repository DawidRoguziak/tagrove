//! Measures and texts of what the grid draws: the area around the tiles
//! (`GalleryGrid.tsx`), the tile (`GalleryTile.tsx`), the status footer
//! (`GalleryStatusFooter.tsx`) and the load error banner. Lengths are the CSS pixels
//! of the Tailwind classes named beside them.

use std::time::Duration;

use media_tagger_core::models::AssetSummary;

use crate::{
    controls::ChipTone,
    i18n::{I18n, Params},
    theme::CubicBezier,
    thumbs::ThumbnailKey,
};

/// The gallery section around the grid.
pub mod area {
    /// `sm:pl-5`: the window is never narrower than its 1000 px minimum.
    pub const PADDING_LEFT: f32 = 20.0;
    /// `sm:pt-5`.
    pub const PADDING_TOP: f32 = 20.0;
    /// `TIMELINE_CLEARANCE`: the right padding that keeps every column clear of the
    /// timeline rail (`TIMELINE_RIGHT` 24 + `TIMELINE_WIDTH` 10), with or without a
    /// rail.
    pub const PADDING_RIGHT: f32 = 34.0;
    /// One step of a wheel that reports lines, in pixels.
    pub const WHEEL_LINE: f32 = 20.0;
    /// One step of the Up and Down keys.
    pub const KEY_LINE: f64 = 40.0;
    /// What Page Up and Page Down keep in view of the page before: the viewport's
    /// height times this is the step.
    pub const PAGE_FRACTION: f64 = 0.875;
}

/// `GalleryTile`.
pub mod tile {
    /// `bg-base-300/95`.
    pub const FILL_PERCENT: u8 = 95;
    /// The loader's veil: `bg-base-100/60`.
    pub const LOADER_VEIL_PERCENT: u8 = 60;
    /// The media chip: `right-1 bottom-1`.
    pub const CHIP_INSET: f32 = 4.0;
    /// A slot whose asset is not loaded: `bg-base-300/70`.
    pub const PLACEHOLDER_FILL_PERCENT: u8 = 70;
    /// The backplate of a media group reaches this far beyond its tiles.
    pub const GROUP_PLATE_OUTSET: f32 = 4.0;
}

/// `animate-pulse` of a slot whose asset is not loaded:
/// `pulse 2s cubic-bezier(0.4, 0, 0.6, 1) infinite`, opacity 1 → 0.5 → 1.
pub const PLACEHOLDER_PULSE: Duration = Duration::from_secs(2);
pub const PLACEHOLDER_PULSE_EASE: CubicBezier = CubicBezier {
    x1: 0.4,
    y1: 0.0,
    x2: 0.6,
    y2: 1.0,
};

/// The opacity of a pulsing placeholder at `progress` (0..=1) of one period.
pub fn placeholder_pulse_opacity(progress: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    // Each half of the keyframes is eased on its own.
    let half = if progress < 0.5 {
        progress * 2.0
    } else {
        (1.0 - progress) * 2.0
    };
    1.0 - 0.5 * PLACEHOLDER_PULSE_EASE.sample(half)
}

/// `GalleryStatusFooter`.
pub mod footer {
    /// `min-h-[136px]` is below the content's own height: `py-4`, the 40 px mark, the
    /// 28 px heading line and the 21 px count line, with two `gap-2`.
    pub const HEIGHT: f32 = 137.0;
    pub const PADDING_Y: f32 = 16.0;
    pub const PADDING_X: f32 = 20.0;
    pub const GAP: f32 = 8.0;
    /// The rule with the mark: `max-w-sm gap-4`.
    pub const RULE_MAX_WIDTH: f32 = 384.0;
    pub const RULE_GAP: f32 = 16.0;
    /// `size-10` with a `size-6` icon.
    pub const MARK_SIZE: f32 = 40.0;
    pub const MARK_ICON_SIZE: f32 = 24.0;
    /// `text-xl font-semibold`.
    pub const TITLE_SIZE: f32 = 20.0;
    pub const TITLE_LINE_HEIGHT: f32 = 28.0;
    /// `font-mono text-[14px]` at the body's line height of 1.5.
    pub const COUNT_SIZE: f32 = 14.0;
    pub const COUNT_LINE_HEIGHT: f32 = 21.0;
    /// `size-1.5`.
    pub const DOT_SIZE: f32 = 6.0;
}

/// The load error banner above the grid (`role="alert"`).
pub mod error_banner {
    /// `px-4 py-2`, `gap-3`, `mb-3`, `border-error/40 bg-error/10`.
    pub const PADDING_X: f32 = 16.0;
    pub const PADDING_Y: f32 = 8.0;
    pub const GAP: f32 = 12.0;
    pub const MARGIN_BOTTOM: f32 = 12.0;
    pub const BORDER_PERCENT: u8 = 40;
    pub const FILL_PERCENT: u8 = 10;
    /// `btn btn-xs btn-outline`: 24 px high, 8 px inline padding, 11 px text.
    pub const BUTTON_HEIGHT: f32 = 24.0;
    pub const BUTTON_PADDING_X: f32 = 8.0;
    pub const BUTTON_FONT_SIZE: f32 = 11.0;
    /// The banner's height: the button, the padding and the 1 px border.
    pub const HEIGHT: f32 = BUTTON_HEIGHT + 2.0 * PADDING_Y + 2.0;
    /// What the banner adds above the grid.
    pub const LEAD: f32 = HEIGHT + MARGIN_BOTTOM;
}

/// What scrolls above the first row: the section's top padding and, while the load
/// failed, the error banner.
pub fn lead(load_failed: bool) -> f32 {
    area::PADDING_TOP + if load_failed { error_banner::LEAD } else { 0.0 }
}

/// The width the tiles fill in a viewport `viewport_width` wide.
pub fn tile_area_width(viewport_width: f32) -> f32 {
    (viewport_width - area::PADDING_LEFT - area::PADDING_RIGHT).max(0.0)
}

/// The chip in a tile's corner: videos and GIFs show static thumbnails (plan decision
/// D8) and say what they are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaBadge {
    Video,
    Gif,
}

impl MediaBadge {
    /// The badge of an asset of `kind` (`AssetSummary::kind`); images have none.
    pub fn of_kind(kind: &str) -> Option<Self> {
        match kind {
            "video" => Some(Self::Video),
            "gif" => Some(Self::Gif),
            _ => None,
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Video => "gallery.videoChip",
            Self::Gif => "gallery.gifChip",
        }
    }

    pub fn tone(self) -> ChipTone {
        match self {
            Self::Video => ChipTone::Video,
            Self::Gif => ChipTone::Gif,
        }
    }
}

/// The picture a tile of `asset` shows.
pub fn thumbnail_key(asset: &AssetSummary) -> ThumbnailKey {
    ThumbnailKey::new(asset.id, asset.thumb_version.clone())
}

/// The media group of an asset, when it has one (`media_group_key?.trim()`).
pub fn media_group(asset: &AssetSummary) -> Option<&str> {
    asset
        .media_group_key
        .as_deref()
        .map(str::trim)
        .filter(|key| !key.is_empty())
}

/// The backplates of one row: for every run of adjacent lanes whose assets share a
/// media group, its first and last lane. `groups` holds the group of each lane, in
/// lane order.
pub fn group_plates<'a>(groups: impl IntoIterator<Item = Option<&'a str>>) -> Vec<(usize, usize)> {
    let mut plates: Vec<(usize, usize)> = Vec::new();
    let mut previous: Option<&str> = None;
    for (lane, group) in groups.into_iter().enumerate() {
        match (group, plates.last_mut()) {
            (Some(key), Some(last)) if previous == Some(key) && last.1 + 1 == lane => {
                last.1 = lane;
            }
            (Some(_), _) => plates.push((lane, lane)),
            (None, _) => {}
        }
        previous = group;
    }
    plates
}

/// `gallery.resultCount` with the count formatted for the language.
pub fn result_count(i18n: &I18n<'_>, total: usize) -> String {
    let count = i64::try_from(total).unwrap_or(i64::MAX);
    i18n.t_with(
        "gallery.resultCount",
        &Params::new()
            .with_count(count)
            .with("formattedCount", i18n.format_count(count)),
    )
}

#[cfg(test)]
mod tests {
    use super::{
        error_banner, footer, group_plates, lead, placeholder_pulse_opacity, result_count,
        tile_area_width, MediaBadge,
    };
    use crate::{
        controls::ChipTone,
        gallery::{grid_layout, GRID_GAP},
        i18n::{I18n, Language, Params},
    };

    #[test]
    fn the_default_window_shows_six_columns_of_the_default_tile_size() {
        // 1440 px wide, as the Tauri window: 1386 px of tiles.
        let width = tile_area_width(1440.0);
        assert_eq!(width, 1386.0);
        let layout = grid_layout(f64::from(width), 188.0, GRID_GAP, 7);
        assert_eq!((layout.columns, layout.size, layout.rows), (6, 221.0, 2));
        let small = grid_layout(f64::from(width), 140.0, GRID_GAP, 7);
        assert_eq!((small.columns, small.size, small.rows), (9, 143.0, 1));
        let large = grid_layout(f64::from(width), 340.0, GRID_GAP, 7);
        assert_eq!((large.columns, large.size, large.rows), (3, 454.0, 3));
    }

    #[test]
    fn the_error_banner_adds_its_height_and_margin_above_the_grid() {
        assert_eq!(lead(false), 20.0);
        assert_eq!(error_banner::HEIGHT, 42.0);
        assert_eq!(lead(true), 74.0);
    }

    #[test]
    fn the_footer_is_as_high_as_its_content() {
        let content = footer::MARK_SIZE
            + footer::GAP
            + footer::TITLE_LINE_HEIGHT
            + footer::GAP
            + footer::COUNT_LINE_HEIGHT;
        assert_eq!(footer::HEIGHT, content + 2.0 * footer::PADDING_Y);
    }

    #[test]
    fn videos_and_gifs_have_a_chip_and_images_none() {
        assert_eq!(MediaBadge::of_kind("image"), None);
        assert_eq!(MediaBadge::of_kind(""), None);
        let english = I18n::new(Language::En);
        for (kind, badge, tone, text) in [
            ("video", MediaBadge::Video, ChipTone::Video, "Video"),
            ("gif", MediaBadge::Gif, ChipTone::Gif, "GIF"),
        ] {
            assert_eq!(MediaBadge::of_kind(kind), Some(badge));
            assert_eq!(badge.tone(), tone);
            assert_eq!(english.t(badge.label_key()), text);
        }
    }

    #[test]
    fn backplates_join_adjacent_tiles_of_one_group() {
        assert_eq!(
            group_plates([Some("a"), Some("a"), None, Some("a"), Some("b"), Some("b")]),
            [(0, 1), (3, 3), (4, 5)]
        );
        assert_eq!(group_plates([None, None]), []);
        assert_eq!(group_plates([Some("a")]), [(0, 0)]);
    }

    #[test]
    fn the_result_count_uses_the_plural_and_the_digit_grouping_of_the_language() {
        let english = I18n::new(Language::En);
        assert_eq!(result_count(&english, 1), "1 item");
        assert_eq!(result_count(&english, 1_000_000), "1,000,000 items");
        let polish = I18n::new(Language::Pl);
        assert_ne!(result_count(&polish, 2), result_count(&polish, 5));
        for key in ["gallery.endOfResults", "gallery.loadError", "gallery.retry"] {
            assert!(english.resolve(key, &Params::new()).is_some(), "{key}");
        }
    }

    #[test]
    fn a_placeholder_pulses_between_full_and_half_opacity() {
        assert_eq!(placeholder_pulse_opacity(0.0), 1.0);
        assert!((placeholder_pulse_opacity(0.5) - 0.5).abs() < 1e-6);
        assert_eq!(placeholder_pulse_opacity(1.0), 1.0);
        let quarter = placeholder_pulse_opacity(0.25);
        assert!(quarter < 1.0 && quarter > 0.5, "{quarter}");
    }
}
