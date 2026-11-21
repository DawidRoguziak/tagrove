//! `UiSlider`: a native `<input type="range">` with the app's track and thumb.
//!
//! The slider is controlled: input asks the owner for a new value ([`SliderRange::key`],
//! [`SliderRange::value_at`]) and the slider keeps showing the owner's value until the
//! owner sets another (`UiSlider` puts the committed value back after every input).

use super::FocusRing;
use crate::theme::{Rgba, ThemeTokens};

/// `min`, `max` and `step` of a range input. `step` 1 is the HTML default.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SliderRange {
    pub min: f64,
    pub max: f64,
    pub step: f64,
}

/// The keys a focused range input answers (WebKit's `RangeInputType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SliderKey {
    /// Arrow right and arrow up.
    Increase,
    /// Arrow left and arrow down.
    Decrease,
    PageUp,
    PageDown,
    Home,
    End,
}

impl SliderRange {
    /// A range from `min` to `max` with step 1. A `max` below `min` becomes `min`.
    pub fn new(min: f64, max: f64) -> Self {
        Self::with_step(min, max, 1.0)
    }

    /// A step that is not positive counts as 1, as in HTML.
    pub fn with_step(min: f64, max: f64, step: f64) -> Self {
        let min = if min.is_finite() { min } else { 0.0 };
        let max = if max.is_finite() && max >= min {
            max
        } else {
            min
        };
        let step = if step.is_finite() && step > 0.0 {
            step
        } else {
            1.0
        };
        Self { min, max, step }
    }

    /// `value` as the input holds it: clamped to the range and on a step from `min`
    /// (half a step rounds up). The largest step value fits below `max`.
    pub fn sanitize(&self, value: f64) -> f64 {
        let value = if value.is_finite() { value } else { self.min };
        let steps = ((value - self.min) / self.step + 0.5).floor();
        let last = ((self.max - self.min) / self.step).floor();
        self.min + steps.clamp(0.0, last.max(0.0)) * self.step
    }

    /// Where `value` sits on the track, 0..=1: the share of the track the primary
    /// gradient fills (`(value - min) / (max - min) * 100%`).
    pub fn fraction(&self, value: f64) -> f64 {
        let span = self.max - self.min;
        if span <= 0.0 {
            return 0.0;
        }
        ((self.sanitize(value) - self.min) / span).clamp(0.0, 1.0)
    }

    /// The value at `fraction` (0..=1) of the track, as a click or a drag sets it.
    pub fn value_at(&self, fraction: f64) -> f64 {
        let fraction = if fraction.is_finite() {
            fraction.clamp(0.0, 1.0)
        } else {
            0.0
        };
        self.sanitize(self.min + fraction * (self.max - self.min))
    }

    /// The value a key asks for from `value`, or `None` when the key changes nothing.
    /// Page keys move by a tenth of the range, at least one step.
    pub fn key(&self, value: f64, key: SliderKey) -> Option<f64> {
        let current = self.sanitize(value);
        let page = ((self.max - self.min) / 10.0).max(self.step);
        let next = match key {
            SliderKey::Increase => current + self.step,
            SliderKey::Decrease => current - self.step,
            SliderKey::PageUp => current + page,
            SliderKey::PageDown => current - page,
            SliderKey::Home => self.min,
            SliderKey::End => self.max,
        };
        let next = self.sanitize(next);
        (next != current).then_some(next)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SliderVisuals {
    /// `bg-[var(--border-strong)]`
    pub track: Rgba,
    /// The gradient up to the value: `var(--color-primary)`.
    pub fill: Rgba,
    /// `h-[4px] rounded-full`
    pub track_height: f32,
    /// The thumb: `h-3 w-3 rounded-full bg-primary`.
    pub thumb_size: f32,
    pub thumb: Rgba,
    /// `border border-[oklch(var(--b1))]`
    pub thumb_border: Rgba,
    pub thumb_border_width: f32,
    /// `shadow-[0_0_0_2px_oklch(var(--p)/0.24)]`
    pub thumb_ring: Rgba,
    pub thumb_ring_width: f32,
    /// The base layer's `:focus-visible` outline around the input.
    pub focus_ring: FocusRing,
}

pub fn slider_visuals(tokens: &ThemeTokens) -> SliderVisuals {
    let colors = &tokens.colors;
    SliderVisuals {
        track: colors.border_strong,
        fill: colors.primary,
        track_height: 4.0,
        thumb_size: 12.0,
        thumb: colors.primary,
        thumb_border: colors.base_100,
        thumb_border_width: 1.0,
        thumb_ring: colors.primary.with_alpha_percent(24),
        thumb_ring_width: 2.0,
        focus_ring: FocusRing::base(tokens),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{controls::control_source, theme::LIGHT};

    #[test]
    fn the_solid_slider_still_has_the_classes_copied_here() {
        let source = control_source("UiSlider.tsx");
        for class in [
            "m-0 h-[4px] w-full appearance-none rounded-full border-0 bg-[var(--border-strong)] p-0",
            "[&::-webkit-slider-thumb]:h-3 [&::-webkit-slider-thumb]:w-3",
            "[&::-webkit-slider-thumb]:border-[oklch(var(--b1))] [&::-webkit-slider-thumb]:bg-primary [&::-webkit-slider-thumb]:shadow-[0_0_0_2px_oklch(var(--p)/0.24)]",
            "linear-gradient(to right, var(--color-primary) ${progress()}%, transparent ${progress()}%)",
            "event.currentTarget.value = String(props.value);",
        ] {
            assert!(source.contains(class), "UiSlider.tsx lost: {class}");
        }
    }

    #[test]
    fn keys_step_page_and_jump_to_the_ends() {
        let range = SliderRange::new(0.0, 100.0);
        assert_eq!(range.key(20.0, SliderKey::Increase), Some(21.0));
        assert_eq!(range.key(20.0, SliderKey::Decrease), Some(19.0));
        assert_eq!(range.key(20.0, SliderKey::PageUp), Some(30.0));
        assert_eq!(range.key(20.0, SliderKey::PageDown), Some(10.0));
        assert_eq!(range.key(20.0, SliderKey::Home), Some(0.0));
        assert_eq!(range.key(20.0, SliderKey::End), Some(100.0));
        assert_eq!(range.key(100.0, SliderKey::Increase), None, "at the end");
        assert_eq!(range.key(0.0, SliderKey::Decrease), None);
        assert_eq!(range.key(95.0, SliderKey::PageUp), Some(100.0), "clamped");
        assert_eq!(range.key(0.0, SliderKey::Home), None);
    }

    #[test]
    fn a_small_range_pages_by_at_least_one_step() {
        let range = SliderRange::with_step(1.0, 5.0, 1.0);
        assert_eq!(range.key(2.0, SliderKey::PageUp), Some(3.0));
        let fine = SliderRange::with_step(0.0, 1.0, 0.25);
        assert_eq!(fine.key(0.25, SliderKey::Increase), Some(0.5));
        assert_eq!(fine.key(0.5, SliderKey::PageDown), Some(0.25));
    }

    #[test]
    fn values_snap_to_steps_inside_the_range() {
        let range = SliderRange::with_step(0.0, 10.0, 3.0);
        assert_eq!(range.sanitize(4.4), 3.0);
        assert_eq!(range.sanitize(4.5), 6.0);
        assert_eq!(range.sanitize(10.0), 9.0, "the last step below max");
        assert_eq!(range.sanitize(-4.0), 0.0);
        assert_eq!(range.sanitize(f64::NAN), 0.0);
        assert_eq!(SliderRange::with_step(0.0, 10.0, 0.0).step, 1.0);
        assert_eq!(SliderRange::new(5.0, 1.0).max, 5.0);
    }

    #[test]
    fn fraction_and_pointer_position_agree() {
        let range = SliderRange::new(10.0, 30.0);
        assert_eq!(range.fraction(20.0), 0.5);
        assert_eq!(range.fraction(40.0), 1.0);
        assert_eq!(range.value_at(0.5), 20.0);
        assert_eq!(range.value_at(0.26), 15.0);
        assert_eq!(range.value_at(-1.0), 10.0);
        assert_eq!(range.value_at(2.0), 30.0);
        assert_eq!(SliderRange::new(3.0, 3.0).fraction(3.0), 0.0);
    }

    #[test]
    fn the_thumb_is_primary_with_a_base_border_and_a_soft_ring() {
        let visuals = slider_visuals(&LIGHT);
        assert_eq!(visuals.track, LIGHT.colors.border_strong);
        assert_eq!(visuals.thumb, LIGHT.colors.primary);
        assert_eq!(visuals.thumb_border, LIGHT.colors.base_100);
        assert_eq!(
            visuals.thumb_ring,
            LIGHT.colors.primary.with_alpha_percent(24)
        );
        assert_eq!((visuals.track_height, visuals.thumb_size), (4.0, 12.0));
    }
}
