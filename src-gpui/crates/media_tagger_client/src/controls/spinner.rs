//! Loading spinners and their delayed appearance.
//!
//! Two spinners exist in the Solid frontend: DaisyUI `loading loading-spinner
//! loading-xs` beside a status text (`SectionOperationStatus.tsx`), and the ring of a
//! gallery tile whose thumbnail is still loading (`GalleryTile.tsx`). The tile's ring
//! appears only after a delay and then fades in (`.thumbnail-loader`), so cached
//! thumbnails never flash a spinner.

use std::time::Duration;

use crate::theme::{CubicBezier, Rgba, ThemeTokens};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SpinnerSize {
    /// `loading-xs`: `width: calc(var(--size-selector) * 4)`, an arc in the text colour.
    ExtraSmall,
    /// The tile loader: `h-[26px] w-[26px] rounded-full border-[3px]
    /// border-base-content/25 border-t-primary animate-spin`.
    Tile,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpinnerVisuals {
    pub size: f32,
    /// The ring's width (`border-[3px]`, or the arc's `stroke-width: 3` of 24).
    pub stroke: f32,
    /// The full ring behind the arc; transparent when there is none.
    pub track: Rgba,
    /// The arc; `None` means the current text colour.
    pub arc: Option<Rgba>,
    /// One turn.
    pub period: Duration,
}

pub fn spinner_visuals(tokens: &ThemeTokens, size: SpinnerSize) -> SpinnerVisuals {
    match size {
        SpinnerSize::ExtraSmall => SpinnerVisuals {
            size: 16.0,
            stroke: 2.0,
            track: Rgba::TRANSPARENT,
            arc: None,
            // The mask's animateTransform turns once every 2s.
            period: Duration::from_secs(2),
        },
        SpinnerSize::Tile => SpinnerVisuals {
            size: 26.0,
            stroke: 3.0,
            track: tokens.colors.base_content.with_alpha_percent(25),
            arc: Some(tokens.colors.primary),
            // Tailwind's animate-spin: spin 1s linear infinite.
            period: Duration::from_secs(1),
        },
    }
}

/// Something that stays invisible for `delay`, then fades in over `fade`
/// (`animation: motion-fade-in <fade> <easing> <delay> forwards`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DelayedReveal {
    pub delay: Duration,
    pub fade: Duration,
    pub easing: CubicBezier,
}

/// `.thumbnail-loader { animation: motion-fade-in 150ms ease-out 300ms forwards; }`
pub const THUMBNAIL_LOADER: DelayedReveal = DelayedReveal {
    delay: Duration::from_millis(300),
    fade: Duration::from_millis(150),
    easing: CubicBezier::EASE_OUT,
};

impl DelayedReveal {
    /// The whole time until fully visible.
    pub fn total(self) -> Duration {
        self.delay + self.fade
    }

    /// The opacity `elapsed` after it was shown.
    pub fn opacity(self, elapsed: Duration) -> f32 {
        if elapsed < self.delay {
            return 0.0;
        }
        if self.fade.is_zero() {
            return 1.0;
        }
        let progress = (elapsed - self.delay).as_secs_f32() / self.fade.as_secs_f32();
        self.easing.sample(progress)
    }

    /// The opacity at `progress` (0..=1) of [`Self::total`]: what an animation that
    /// runs for the whole time draws.
    pub fn opacity_at_progress(self, progress: f32) -> f32 {
        self.opacity(self.total().mul_f32(progress.clamp(0.0, 1.0)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{controls::STYLESHEET, theme::DARK};

    #[test]
    fn the_stylesheet_still_delays_the_tile_loader() {
        assert!(STYLESHEET.contains(
            ".thumbnail-loader { opacity: 0; pointer-events: none; animation: motion-fade-in 150ms ease-out 300ms forwards; }"
        ));
    }

    #[test]
    fn the_loader_is_invisible_for_the_delay_then_fades_in() {
        let reveal = THUMBNAIL_LOADER;
        assert_eq!(reveal.opacity(Duration::ZERO), 0.0);
        assert_eq!(reveal.opacity(Duration::from_millis(299)), 0.0);
        let mid = reveal.opacity(Duration::from_millis(375));
        assert!(mid > 0.5 && mid < 1.0, "{mid}");
        assert_eq!(reveal.opacity(Duration::from_millis(450)), 1.0);
        assert_eq!(reveal.opacity(Duration::from_secs(5)), 1.0);
        assert_eq!(reveal.total(), Duration::from_millis(450));
        assert_eq!(reveal.opacity_at_progress(0.5), 0.0);
        assert_eq!(reveal.opacity_at_progress(1.0), 1.0);
    }

    #[test]
    fn spinner_sizes() {
        let tile = spinner_visuals(&DARK, SpinnerSize::Tile);
        assert_eq!((tile.size, tile.stroke), (26.0, 3.0));
        assert_eq!(tile.track, DARK.colors.base_content.with_alpha_percent(25));
        assert_eq!(tile.arc, Some(DARK.colors.primary));
        let small = spinner_visuals(&DARK, SpinnerSize::ExtraSmall);
        assert_eq!(small.size, 16.0);
        assert_eq!(small.arc, None);
    }
}
