//! What the shared controls of `src/ui/controls` look like and how they respond, from
//! the theme tokens, without a GUI: buttons, icon buttons, chips, alerts, the progress
//! bar, the slider, the segmented control, the switch and the loading spinner. The
//! GPUI adapter (`media_tagger_gpui::ui`) draws these values and routes input to the
//! state machines here.
//!
//! Each value names the Tailwind class or the stylesheet rule it comes from. The rules
//! of DaisyUI that the Solid controls inherit (`btn`, `toggle`, `progress`,
//! `loading`) are resolved here for the classes the controls use; tests read
//! `src/styles.css` and the control sources and fail when a copied rule changes.

mod alert;
mod button;
mod chip;
mod progress;
mod segmented;
mod slider;
mod spinner;
mod switch;

pub use alert::{alert_visuals, AlertTone, AlertVisuals};
pub use button::{
    button_visuals, icon_button_visuals, ButtonState, ButtonVariant, ButtonVisuals, IconButtonKind,
    ICON_BUTTON_ICON_SIZE, ICON_BUTTON_SIZE,
};
pub use chip::{chip_visuals, ChipTone, ChipVisuals};
pub use progress::{progress_visuals, ProgressValue, ProgressVisuals};
pub use segmented::{
    segmented_visuals, HighlightTrack, Measure, Rect, SegmentedKey, SegmentedVisuals,
};
pub use slider::{slider_visuals, SliderKey, SliderRange, SliderVisuals};
pub use spinner::{spinner_visuals, DelayedReveal, SpinnerSize, SpinnerVisuals, THUMBNAIL_LOADER};
pub use switch::{switch_visuals, SwitchState, SwitchVisuals};

use crate::theme::{Rgba, ThemeTokens};

/// The outline of `:focus-visible`, drawn outside the element: `outline: <width>px solid
/// <color>; outline-offset: <offset>px`. The radius follows the element's own.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FocusRing {
    pub color: Rgba,
    pub width: f32,
    pub offset: f32,
}

impl FocusRing {
    /// `:focus-visible { outline: 2px solid var(--color-primary); outline-offset: 3px; }`
    /// of the stylesheet's base layer.
    pub fn base(tokens: &ThemeTokens) -> Self {
        Self {
            color: tokens.colors.primary,
            width: 2.0,
            offset: 3.0,
        }
    }

    /// How far the outer edge of the ring is from the element's edge.
    pub fn outset(self) -> f32 {
        self.offset + self.width
    }
}

/// The stylesheet the controls copy rules from; tests check the rules are still there.
#[cfg(test)]
pub(crate) const STYLESHEET: &str = include_str!("../../../../src/styles.css");

/// Reads a Solid control's source; tests check the classes are still there.
#[cfg(test)]
pub(crate) fn control_source(file: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../src/ui/controls")
        .join(file);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::{FocusRing, STYLESHEET};
    use crate::theme::DARK;

    #[test]
    fn the_base_focus_ring_is_the_stylesheet_outline() {
        assert!(STYLESHEET.contains(
            ":focus-visible { outline: 2px solid var(--color-primary); outline-offset: 3px; }"
        ));
        let ring = FocusRing::base(&DARK);
        assert_eq!(ring.color, DARK.colors.primary);
        assert_eq!((ring.width, ring.offset, ring.outset()), (2.0, 3.0, 5.0));
    }
}
