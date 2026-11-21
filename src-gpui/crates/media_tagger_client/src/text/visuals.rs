//! What a text field looks like in each state, from the theme tokens: the element
//! rules of `src/styles.css` for `input`, which the Tauri interface draws its fields
//! with. The tests of this module read the stylesheet and fail when a rule changes.

use crate::theme::{Rgba, Shadow, ThemeTokens, RADII};

/// The width of the caret in pixels (WebKit draws it one pixel wide).
pub const CARET_WIDTH: f32 = 1.0;

/// The frame around the text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FieldChrome {
    /// A field of its own: background, border and focus ring of `input`.
    #[default]
    Boxed,
    /// A field inside a container that draws the frame and the focus ring itself, as
    /// the tag editor does (`border-0 bg-transparent px-1 shadow-none`).
    Bare,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct FieldState {
    pub focused: bool,
    pub disabled: bool,
}

/// Colours and measures of a field in one state. Lengths are CSS pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FieldVisuals {
    /// `background` of `input`; transparent for [`FieldChrome::Bare`].
    pub background: Rgba,
    pub text: Rgba,
    /// `input::placeholder`
    pub placeholder: Rgba,
    /// The caret has the colour of the text (`caret-color: auto`).
    pub caret: Rgba,
    /// `::selection`
    pub selection_background: Rgba,
    pub selection_text: Rgba,
    /// The border and the focus ring, which the stylesheet draws as box shadows
    /// (`--field-shadow`, `--field-shadow-focus`). Empty for [`FieldChrome::Bare`].
    pub shadows: &'static [Shadow],
    pub radius: f32,
    pub padding_x: f32,
    pub padding_y: f32,
    /// `input:disabled` is drawn at 0.55.
    pub opacity: f32,
}

/// `padding: 8px 10px` of `input`.
const PADDING_Y: f32 = 8.0;
const PADDING_X: f32 = 10.0;
/// Tailwind `px-1`, the horizontal padding of the tag editor's inner field.
const BARE_PADDING_X: f32 = 4.0;
/// `opacity` of `input:disabled`.
const DISABLED_OPACITY: f32 = 0.55;

pub fn field_visuals(tokens: &ThemeTokens, chrome: FieldChrome, state: FieldState) -> FieldVisuals {
    let colors = &tokens.colors;
    let boxed = chrome == FieldChrome::Boxed;
    FieldVisuals {
        background: if boxed {
            colors.surface_raised
        } else {
            Rgba::TRANSPARENT
        },
        text: colors.base_content,
        placeholder: colors.text_muted,
        caret: colors.base_content,
        selection_background: colors.primary,
        selection_text: colors.primary_content,
        shadows: match (boxed, state.focused && !state.disabled) {
            (false, _) => &[],
            (true, false) => tokens.shadows.field,
            (true, true) => tokens.shadows.field_focus,
        },
        radius: RADII.control,
        padding_x: if boxed { PADDING_X } else { BARE_PADDING_X },
        padding_y: PADDING_Y,
        opacity: if state.disabled {
            DISABLED_OPACITY
        } else {
            1.0
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{ThemeName, DARK, LIGHT};

    const STYLESHEET: &str = include_str!("../../../../../src/styles.css");

    const FOCUSED: FieldState = FieldState {
        focused: true,
        disabled: false,
    };
    const DISABLED: FieldState = FieldState {
        focused: false,
        disabled: true,
    };

    #[test]
    fn the_stylesheet_still_has_the_rules_this_module_copies() {
        for rule in [
            "border-radius: var(--radius-control);",
            "background: var(--surface-raised);",
            "color: var(--color-base-content);",
            "padding: 8px 10px;",
            "box-shadow: var(--field-shadow);",
            "input::placeholder { color: var(--text-muted); }",
            "input:disabled, select:disabled { cursor: not-allowed; opacity: 0.55; }",
            "box-shadow: var(--field-shadow-focus);",
            "::selection { background: var(--color-primary); color: var(--color-primary-content); }",
        ] {
            assert!(STYLESHEET.contains(rule), "src/styles.css lost: {rule}");
        }
    }

    #[test]
    fn a_boxed_field_uses_the_input_tokens_of_its_theme() {
        for theme in ThemeName::ALL {
            let tokens = theme.tokens();
            let visuals = field_visuals(tokens, FieldChrome::Boxed, FieldState::default());

            assert_eq!(visuals.background, tokens.colors.surface_raised);
            assert_eq!(visuals.text, tokens.colors.base_content);
            assert_eq!(visuals.caret, tokens.colors.base_content);
            assert_eq!(visuals.placeholder, tokens.colors.text_muted);
            assert_eq!(visuals.selection_background, tokens.colors.primary);
            assert_eq!(visuals.selection_text, tokens.colors.primary_content);
            assert_eq!(visuals.shadows, tokens.shadows.field);
            assert_eq!(
                (visuals.radius, visuals.padding_x, visuals.padding_y),
                (6.0, 10.0, 8.0)
            );
            assert_eq!(visuals.opacity, 1.0);
        }
        assert_ne!(
            field_visuals(&LIGHT, FieldChrome::Boxed, FieldState::default()).background,
            field_visuals(&DARK, FieldChrome::Boxed, FieldState::default()).background
        );
    }

    #[test]
    fn focus_swaps_the_border_for_the_focus_ring() {
        let visuals = field_visuals(&DARK, FieldChrome::Boxed, FOCUSED);

        assert_eq!(visuals.shadows, DARK.shadows.field_focus);
        assert_eq!(visuals.shadows[0].color, DARK.colors.primary);
        assert_eq!(visuals.opacity, 1.0);
    }

    #[test]
    fn a_disabled_field_is_dimmed_and_never_shows_the_focus_ring() {
        let both = FieldState {
            focused: true,
            disabled: true,
        };
        for state in [DISABLED, both] {
            let visuals = field_visuals(&LIGHT, FieldChrome::Boxed, state);

            assert_eq!(visuals.opacity, 0.55);
            assert_eq!(visuals.shadows, LIGHT.shadows.field);
        }
    }

    #[test]
    fn a_bare_field_draws_no_frame_in_any_state() {
        for state in [FieldState::default(), FOCUSED, DISABLED] {
            let visuals = field_visuals(&DARK, FieldChrome::Bare, state);

            assert_eq!(visuals.background, Rgba::TRANSPARENT);
            assert!(visuals.shadows.is_empty());
            assert_eq!(visuals.padding_x, 4.0);
            assert_eq!(visuals.text, DARK.colors.base_content);
        }
    }
}
