//! `UiProgressBar`: DaisyUI `progress progress-primary` with a label beside it.

use crate::theme::{Rgba, ThemeTokens, TYPOGRAPHY};

/// What the bar shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProgressValue {
    /// `value` of `max`; drawn as a fraction clamped to 0..=1.
    Determinate { value: f64, max: f64 },
    /// No value (`<progress>` without one): a moving stripe.
    Indeterminate,
}

impl ProgressValue {
    /// `value` / `max` clamped to 0..=1 as a `<progress>` element draws it; `None`
    /// for an indeterminate bar. A `max` that is not positive counts as 1, as in HTML.
    pub fn fraction(self) -> Option<f64> {
        match self {
            Self::Indeterminate => None,
            Self::Determinate { value, max } => {
                let max = if max.is_finite() && max > 0.0 {
                    max
                } else {
                    1.0
                };
                let value = if value.is_finite() { value } else { 0.0 };
                Some((value / max).clamp(0.0, 1.0))
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProgressVisuals {
    /// The track: `color-mix(in oklab, currentcolor 20%, transparent)` with
    /// `currentcolor` the primary colour.
    pub track: Rgba,
    /// The filled part.
    pub fill: Rgba,
    pub label: Rgba,
    /// `h-1.5`
    pub height: f32,
    /// `rounded-[2px]`
    pub radius: f32,
    /// `max-w-[420px]` of the row.
    pub max_width: f32,
    /// `gap-3` between bar and label.
    pub gap: f32,
    /// `text-xs`
    pub label_size: f32,
    pub label_line_height: f32,
    /// The indeterminate stripe covers this share of the track (`currentColor -1% 10%`).
    pub stripe_width: f32,
    /// One sweep of the indeterminate stripe (`5s ease-in-out infinite`).
    pub stripe_period_ms: u64,
}

pub fn progress_visuals(tokens: &ThemeTokens) -> ProgressVisuals {
    let primary = tokens.colors.primary;
    ProgressVisuals {
        track: primary.with_alpha_percent(20),
        fill: primary,
        label: tokens.colors.base_content,
        height: 6.0,
        radius: 2.0,
        max_width: 420.0,
        gap: 12.0,
        label_size: TYPOGRAPHY.xs.size,
        label_line_height: TYPOGRAPHY.xs.line_height_px(),
        stripe_width: 0.11,
        stripe_period_ms: 5000,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{controls::control_source, theme::DARK};

    #[test]
    fn the_solid_progress_bar_still_has_the_classes_copied_here() {
        let source = control_source("UiProgressBar.tsx");
        for class in [
            "flex w-full max-w-[420px] items-center gap-3",
            "progress progress-primary h-1.5 min-w-0 flex-1 rounded-[2px]",
            "text-xs text-base-content",
        ] {
            assert!(source.contains(class), "UiProgressBar.tsx lost: {class}");
        }
    }

    #[test]
    fn the_fraction_is_clamped_like_a_progress_element() {
        let at = |value, max| ProgressValue::Determinate { value, max }.fraction();
        assert_eq!(at(25.0, 100.0), Some(0.25));
        assert_eq!(at(150.0, 100.0), Some(1.0));
        assert_eq!(at(-5.0, 100.0), Some(0.0));
        assert_eq!(at(0.5, 0.0), Some(0.5), "max 0 counts as 1");
        assert_eq!(at(f64::NAN, 10.0), Some(0.0));
        assert_eq!(ProgressValue::Indeterminate.fraction(), None);
    }

    #[test]
    fn the_bar_is_primary_on_a_tinted_track() {
        let visuals = progress_visuals(&DARK);
        assert_eq!(visuals.fill, DARK.colors.primary);
        assert_eq!(visuals.track, DARK.colors.primary.with_alpha_percent(20));
        assert_eq!(
            (visuals.height, visuals.radius, visuals.max_width),
            (6.0, 2.0, 420.0)
        );
    }
}
