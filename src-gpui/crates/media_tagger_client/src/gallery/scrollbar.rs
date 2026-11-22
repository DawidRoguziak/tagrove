//! The position bar beside the grid (`GalleryScrollbar.tsx`).
//!
//! The Solid gallery draws this bar only when its grid is taller than WebKit's
//! physical scroll window; otherwise the webview's native bar shows the position. The
//! GPUI grid has no native scroller (ADR 0042), so it always draws this bar, whenever
//! there is something to scroll ([`scrollbar_visible`]).
//!
//! The bar maps the whole scroll range, from [`VirtualGrid::min_offset`] (the top of
//! the margin above the grid) to [`VirtualGrid::max_offset`] (the footer's end), onto
//! its track. With no margin that is the Solid bar's `offset / maxOffset`.
//!
//! [`VirtualGrid::min_offset`]: super::VirtualGrid::min_offset
//! [`VirtualGrid::max_offset`]: super::VirtualGrid::max_offset

use super::ScrollEdge;

/// The thumb is never shorter than this (`MIN_THUMB`).
pub const SCROLLBAR_MIN_THUMB: f64 = 32.0;
/// The track starts this far below the top of the viewport and ends as far above its
/// bottom (`INSET`).
pub const SCROLLBAR_INSET: f64 = 4.0;
/// `w-2.5`
pub const SCROLLBAR_WIDTH: f32 = 10.0;
/// `right-1`
pub const SCROLLBAR_RIGHT: f32 = 4.0;

/// Whether the bar is drawn: a result whose content scrolls.
pub fn scrollbar_visible(count: usize, min_offset: f64, max_offset: f64) -> bool {
    count > 0 && max_offset > min_offset
}

/// `aria-valuenow`: the first visible item, from 1 (`aria-valuemin` is 1 and
/// `aria-valuemax` the result's count).
pub fn scrollbar_value_now(first_visible: usize, count: usize) -> usize {
    count.min(first_visible + 1)
}

/// The track and the thumb for one scroll position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollbarMetrics {
    /// The track's height.
    pub track: f64,
    /// The thumb's height.
    pub thumb: f64,
    /// How far the thumb's top can move; at least 1.
    pub travel: f64,
    /// Where the position is in the scroll range, 0..=1.
    pub fraction: f64,
    min_offset: f64,
    span: f64,
}

impl ScrollbarMetrics {
    /// The bar of a viewport `viewport` high at `offset` in `min_offset..=max_offset`.
    pub fn new(viewport: f64, offset: f64, min_offset: f64, max_offset: f64) -> Self {
        let track = (viewport - 2.0 * SCROLLBAR_INSET).max(0.0);
        let span = (max_offset - min_offset).max(0.0);
        let thumb = (track * viewport / (span + viewport))
            .max(SCROLLBAR_MIN_THUMB)
            .min(track);
        let thumb = if thumb.is_nan() { track } else { thumb };
        let travel = (track - thumb).max(1.0);
        let fraction = if span > 0.0 {
            ((offset - min_offset) / span).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let fraction = if fraction.is_nan() { 0.0 } else { fraction };
        Self {
            track,
            thumb,
            travel,
            fraction,
            min_offset,
            span,
        }
    }

    /// The thumb's top, below the track's top.
    pub fn thumb_top(&self) -> f64 {
        self.fraction * self.travel
    }

    /// The offset at `fraction` of the range (`scrollToFraction`).
    pub fn offset_at(&self, fraction: f64) -> f64 {
        let fraction = if fraction.is_nan() {
            0.0
        } else {
            fraction.clamp(0.0, 1.0)
        };
        self.min_offset + fraction * self.span
    }

    /// A primary press at `y` below the track's top. A press beside the thumb moves
    /// the thumb's centre to the pointer (the returned offset); the drag that follows
    /// starts from the fraction returned either way.
    pub fn press(&self, y: f64) -> (Option<f64>, f64) {
        let top = self.thumb_top();
        if y < top || y > top + self.thumb {
            let fraction = ((y - self.thumb / 2.0) / self.travel).clamp(0.0, 1.0);
            (Some(self.offset_at(fraction)), fraction)
        } else {
            (None, self.fraction)
        }
    }
}

/// A drag of the thumb: it moves with the pointer from where it was grabbed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollbarDrag {
    /// The pointer's position when the drag began, on any vertical axis the moves
    /// use too.
    pub start_y: f64,
    /// The fraction the drag began at.
    pub start_fraction: f64,
}

impl ScrollbarDrag {
    /// The offset for the pointer at `y` (same axis as `start_y`).
    pub fn offset_at(&self, metrics: &ScrollbarMetrics, y: f64) -> f64 {
        metrics.offset_at(self.start_fraction + (y - self.start_y) / metrics.travel)
    }
}

/// The keys of a focused bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ScrollbarKey {
    /// Arrow down: one row.
    Down,
    /// Arrow up: one row back.
    Up,
    /// Page down: one viewport.
    PageDown,
    PageUp,
    Home,
    End,
}

/// Where a key of the bar scrolls to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ScrollbarTarget {
    Offset(f64),
    Edge(ScrollEdge),
}

/// The target of `key` from `offset`: a row (`stride`, tile plus gap) or a viewport
/// up or down, or an end.
pub fn scrollbar_key_target(
    key: ScrollbarKey,
    offset: f64,
    stride: f64,
    viewport: f64,
) -> ScrollbarTarget {
    match key {
        ScrollbarKey::Down => ScrollbarTarget::Offset(offset + stride),
        ScrollbarKey::Up => ScrollbarTarget::Offset(offset - stride),
        ScrollbarKey::PageDown => ScrollbarTarget::Offset(offset + viewport),
        ScrollbarKey::PageUp => ScrollbarTarget::Offset(offset - viewport),
        ScrollbarKey::Home => ScrollbarTarget::Edge(ScrollEdge::Start),
        ScrollbarKey::End => ScrollbarTarget::Edge(ScrollEdge::End),
    }
}

/// Looks of the bar (`GalleryScrollbar.tsx`).
pub mod visuals {
    use crate::{
        controls::FocusRing,
        theme::{Rgba, ThemeTokens},
    };

    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct ScrollbarVisuals {
        /// `bg-base-content/5`
        pub track: Rgba,
        /// `hover:bg-base-content/10`
        pub track_hover: Rgba,
        /// `bg-[var(--border-strong)]`
        pub thumb: Rgba,
        /// `focus-visible:outline-2 focus-visible:outline-primary outline-offset-2`
        pub focus_ring: FocusRing,
    }

    pub fn scrollbar_visuals(tokens: &ThemeTokens) -> ScrollbarVisuals {
        let colors = &tokens.colors;
        ScrollbarVisuals {
            track: colors.base_content.with_alpha_percent(5),
            track_hover: colors.base_content.with_alpha_percent(10),
            thumb: colors.border_strong,
            focus_ring: FocusRing {
                color: colors.primary,
                width: 2.0,
                offset: 2.0,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gallery::gallery_source;

    #[test]
    fn the_thumb_is_proportional_with_a_minimum_and_follows_the_offset() {
        // 600 px viewport over a million pixels: the minimum thumb.
        let bar = ScrollbarMetrics::new(600.0, 0.0, 0.0, 1_000_000.0);
        assert_eq!((bar.track, bar.thumb, bar.travel), (592.0, 32.0, 560.0));
        let half = ScrollbarMetrics::new(600.0, 500_000.0, 0.0, 1_000_000.0);
        assert_eq!(half.thumb_top(), 280.0);
        // Two viewports of content: the thumb is half the track.
        let short = ScrollbarMetrics::new(600.0, -20.0, -20.0, 580.0);
        assert_eq!((short.thumb, short.fraction), (296.0, 0.0));
        let end = ScrollbarMetrics::new(600.0, 580.0, -20.0, 580.0);
        assert_eq!(end.thumb_top(), 296.0);
        // Nothing to scroll: the thumb fills the track.
        let none = ScrollbarMetrics::new(600.0, 0.0, 0.0, 0.0);
        assert_eq!((none.thumb, none.fraction), (592.0, 0.0));
        assert!(!scrollbar_visible(7, 0.0, 0.0));
        assert!(!scrollbar_visible(0, 0.0, 10.0));
        assert!(scrollbar_visible(7, -20.0, 10.0));
    }

    #[test]
    fn the_value_is_the_first_visible_item() {
        assert_eq!(scrollbar_value_now(0, 25_000), 1);
        assert_eq!(scrollbar_value_now(12_500, 25_000), 12_501);
        assert_eq!(scrollbar_value_now(25_000, 25_000), 25_000);
    }

    #[test]
    fn the_solid_bar_still_has_the_classes_copied_here() {
        let source = gallery_source("GalleryScrollbar.tsx");
        for class in [
            "const MIN_THUMB = 32;",
            "const INSET = 4;",
            "absolute right-1 w-2.5 touch-none rounded-full bg-base-content/5 outline-offset-2 hover:bg-base-content/10 focus-visible:outline-2 focus-visible:outline-primary",
            "absolute inset-x-0 top-0 rounded-full bg-[var(--border-strong)]",
        ] {
            assert!(source.contains(class), "{class}");
        }
    }
}
