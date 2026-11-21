//! The switch of the settings page: a checkbox drawn by DaisyUI as
//! `toggle toggle-sm toggle-primary`.
//!
//! The Solid code has no switch component; the settings page writes the classes on an
//! `<input type="checkbox">` (`ScanSettingsSection.tsx`). Its measures come from the
//! DaisyUI rules with `--size-selector: .25rem` (DaisyUI's default) and `--border: 1px`.

use crate::theme::{Rgba, ThemeTokens};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct SwitchState {
    pub checked: bool,
    pub disabled: bool,
}

/// Lengths are CSS pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SwitchVisuals {
    /// `toggle-sm`: `--size: calc(var(--size-selector) * 5)`.
    pub height: f32,
    /// `width: calc((var(--size) * 2) - (var(--border) + var(--toggle-p)) * 2)`
    pub width: f32,
    /// `--toggle-p: calc(var(--size) * .125)`
    pub padding: f32,
    pub border_width: f32,
    /// `radius-selector + min(toggle-p, …) + min(border, …)`
    pub radius: f32,
    /// The knob: a square as high as the content box, `border-radius: var(--radius-selector)`.
    pub knob_size: f32,
    pub knob_radius: f32,
    /// `currentColor`: `--input-color`, the border and the knob.
    pub color: Rgba,
    /// The track behind the knob: `var(--color-base-100)` when checked.
    pub background: Rgba,
    /// `.toggle:disabled` draws the knob as an outline.
    pub knob_filled: bool,
    /// The knob sits at the end (checked) or the start.
    pub knob_at_end: bool,
    /// `.toggle:disabled { opacity: .3 }`
    pub opacity: f32,
    /// `.toggle:focus-visible { outline-offset: 2px; outline: 2px solid }` in
    /// `currentColor`.
    pub focus_ring: super::FocusRing,
}

const SIZE_SELECTOR: f32 = 4.0;
const BORDER: f32 = 1.0;
/// `--radius-selector` of both themes.
const RADIUS_SELECTOR: f32 = 6.0;

pub fn switch_visuals(tokens: &ThemeTokens, state: SwitchState) -> SwitchVisuals {
    let colors = &tokens.colors;
    let height = SIZE_SELECTOR * 5.0;
    let padding = height * 0.125;
    let radius_max = RADIUS_SELECTOR * 3.0;
    let color = if state.checked {
        colors.primary
    } else {
        // --input-color: color-mix(in oklab, var(--color-base-content) 50%, #0000)
        colors.base_content.with_alpha_percent(50)
    };
    SwitchVisuals {
        height,
        width: height * 2.0 - (BORDER + padding) * 2.0,
        padding,
        border_width: BORDER,
        radius: RADIUS_SELECTOR + padding.min(radius_max) + BORDER.min(radius_max),
        knob_size: height - 2.0 * (padding + BORDER),
        knob_radius: RADIUS_SELECTOR,
        color,
        background: if state.checked {
            colors.base_100
        } else {
            Rgba::TRANSPARENT
        },
        knob_filled: !state.disabled,
        knob_at_end: state.checked,
        opacity: if state.disabled { 0.3 } else { 1.0 },
        focus_ring: super::FocusRing {
            color,
            width: 2.0,
            offset: 2.0,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{ThemeName, DARK};

    #[test]
    fn the_settings_page_still_uses_these_classes() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../src/features/settings/sections/ScanSettingsSection.tsx");
        let source = std::fs::read_to_string(&path).expect("read ScanSettingsSection.tsx");
        assert!(source.contains(r#"class="toggle toggle-sm toggle-primary""#));
        assert!(super::super::STYLESHEET.contains(r#"@plugin "daisyui" { themes: false; }"#));
    }

    #[test]
    fn the_small_toggle_measures() {
        let visuals = switch_visuals(&DARK, SwitchState::default());
        assert_eq!(
            (visuals.height, visuals.width, visuals.padding),
            (20.0, 33.0, 2.5)
        );
        assert_eq!((visuals.radius, visuals.knob_size), (9.5, 13.0));
    }

    #[test]
    fn checked_is_primary_on_base_and_unchecked_is_half_the_text_colour() {
        for theme in ThemeName::ALL {
            let tokens = theme.tokens();
            let on = switch_visuals(
                tokens,
                SwitchState {
                    checked: true,
                    disabled: false,
                },
            );
            assert_eq!(on.color, tokens.colors.primary);
            assert_eq!(on.background, tokens.colors.base_100);
            assert!(on.knob_at_end && on.knob_filled);

            let off = switch_visuals(tokens, SwitchState::default());
            assert_eq!(off.color, tokens.colors.base_content.with_alpha_percent(50));
            assert_eq!(off.background, Rgba::TRANSPARENT);
            assert!(!off.knob_at_end);

            let disabled = switch_visuals(
                tokens,
                SwitchState {
                    checked: true,
                    disabled: true,
                },
            );
            assert_eq!(disabled.opacity, 0.3);
            assert!(!disabled.knob_filled);
        }
    }
}
