//! `UiButton` and `UiIconButton` (`btn btn-sm` with the classes of each variant).

use super::FocusRing;
use crate::theme::{Rgba, Shadow, ThemeTokens, RADII, TYPOGRAPHY};

/// `variant` of `UiButton`. `Default` and `Secondary` look the same; `Secondary` is
/// what a button without a variant gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ButtonVariant {
    Default,
    Primary,
    #[default]
    Secondary,
    Ghost,
    Danger,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct ButtonState {
    pub hovered: bool,
    pub disabled: bool,
}

/// Colours and measures of a button in one state. Lengths are CSS pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ButtonVisuals {
    pub background: Rgba,
    pub border: Rgba,
    pub text: Rgba,
    pub shadows: &'static [Shadow],
    /// `disabled:opacity-50`
    pub opacity: f32,
    /// `h-8`
    pub height: f32,
    /// `px-3` (and `btn-sm`'s `--btn-p`); 0 for an icon button.
    pub padding_x: f32,
    /// `border`
    pub border_width: f32,
    /// `rounded-[var(--radius-control)]`
    pub radius: f32,
    /// `btn-sm`: `--fontsize: .75rem`
    pub font_size: f32,
    /// `font-semibold`
    pub font_weight: f32,
    /// `tracking-[-0.01em]`, in pixels.
    pub letter_spacing: f32,
    /// `btn`: `gap: .375rem` between an icon and the label.
    pub gap: f32,
    /// `btn:focus-visible`: a 2px outline at 2px in `--btn-color`, which these buttons
    /// leave unset, so the base content colour.
    pub focus_ring: FocusRing,
}

const HEIGHT: f32 = 32.0;
const PADDING_X: f32 = 12.0;
const FONT_SIZE: f32 = 12.0;

pub fn button_visuals(
    tokens: &ThemeTokens,
    variant: ButtonVariant,
    state: ButtonState,
) -> ButtonVisuals {
    let colors = &tokens.colors;
    // Hover never applies to a disabled button (`.btn:disabled` has no pointer events).
    let hovered = state.hovered && !state.disabled;
    let (background, border, text, shadows) = match variant {
        ButtonVariant::Primary => (
            if hovered {
                colors.primary.with_alpha_percent(88)
            } else {
                colors.primary
            },
            colors.primary,
            colors.primary_content,
            tokens.shadows.control,
        ),
        ButtonVariant::Danger => (
            colors
                .error
                .with_alpha_percent(if hovered { 16 } else { 8 }),
            colors
                .error
                .with_alpha_percent(if hovered { 50 } else { 28 }),
            colors.error,
            &[][..],
        ),
        ButtonVariant::Ghost => (
            if hovered {
                colors.base_content.with_alpha_percent(10)
            } else {
                Rgba::TRANSPARENT
            },
            Rgba::TRANSPARENT,
            if hovered {
                colors.base_content
            } else {
                colors.text_muted
            },
            &[][..],
        ),
        ButtonVariant::Default | ButtonVariant::Secondary => (
            colors.surface_raised,
            if hovered {
                colors.border_strong
            } else {
                colors.border_soft
            },
            colors.base_content,
            tokens.shadows.control,
        ),
    };
    ButtonVisuals {
        background,
        border,
        text,
        shadows,
        opacity: if state.disabled { 0.5 } else { 1.0 },
        height: HEIGHT,
        padding_x: PADDING_X,
        border_width: 1.0,
        radius: RADII.control,
        font_size: FONT_SIZE,
        font_weight: TYPOGRAPHY.weight_semibold,
        letter_spacing: -0.01 * FONT_SIZE,
        gap: 6.0,
        focus_ring: FocusRing {
            color: colors.base_content,
            width: 2.0,
            offset: 2.0,
        },
    }
}

/// `h-9 w-9` of `UiIconButton`.
pub const ICON_BUTTON_SIZE: f32 = 36.0;
/// `h-5 w-5` of the icon inside.
pub const ICON_BUTTON_ICON_SIZE: f32 = 20.0;

/// What an icon button shows besides its icon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum IconButtonKind {
    /// A secondary button.
    #[default]
    Plain,
    /// `active`: tinted primary; the heart is tinted with the favourite colour and filled.
    Active { heart: bool },
    /// `danger`: the danger variant; `active` has no effect then.
    Danger,
}

/// `UiIconButton`: a square button without horizontal padding. An active one keeps its
/// tint on hover (the classes carry `!`).
pub fn icon_button_visuals(
    tokens: &ThemeTokens,
    kind: IconButtonKind,
    state: ButtonState,
) -> ButtonVisuals {
    let colors = &tokens.colors;
    let variant = match kind {
        IconButtonKind::Danger => ButtonVariant::Danger,
        _ => ButtonVariant::Secondary,
    };
    let mut visuals = button_visuals(tokens, variant, state);
    visuals.height = ICON_BUTTON_SIZE;
    visuals.padding_x = 0.0;
    if let IconButtonKind::Active { heart } = kind {
        let (tint, text) = if heart {
            (colors.favorite, colors.favorite)
        } else {
            (colors.primary, colors.primary_text)
        };
        visuals.border = tint.with_alpha_percent(40);
        visuals.background = tint.with_alpha_percent(12);
        visuals.text = text;
    }
    visuals
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        controls::control_source,
        theme::{ThemeName, DARK, LIGHT},
    };

    #[test]
    fn the_solid_buttons_still_have_the_classes_copied_here() {
        let button = control_source("UiButton.tsx");
        for class in [
            "btn btn-sm h-8 min-h-8 rounded-[var(--radius-control)] border px-3 normal-case font-semibold tracking-[-0.01em] disabled:cursor-not-allowed disabled:opacity-50",
            "shadow-[var(--shadow-control)] border-primary bg-primary text-primary-content hover:border-primary hover:bg-primary/88",
            "border-error/28 bg-error/8 text-error hover:border-error/50 hover:bg-error/16",
            "border-transparent bg-transparent shadow-none text-[var(--text-muted)] hover:bg-base-content/10 hover:text-base-content",
            "border-[var(--border-soft)] bg-[var(--surface-raised)] text-base-content shadow-[var(--shadow-control)] hover:border-[var(--border-strong)] hover:bg-[var(--surface-raised)]",
            r#"variant: "secondary""#,
        ] {
            assert!(button.contains(class), "UiButton.tsx lost: {class}");
        }
        let icon_button = control_source("UiIconButton.tsx");
        for class in [
            "h-9 w-9 min-h-9 shrink-0 p-0",
            "border-favorite/40! bg-favorite/12! text-favorite!",
            "border-primary/40! bg-primary/12! text-primary-text!",
            r#"iconClassName: "h-5 w-5 shrink-0""#,
        ] {
            assert!(
                icon_button.contains(class),
                "UiIconButton.tsx lost: {class}"
            );
        }
    }

    #[test]
    fn each_variant_uses_the_tokens_of_its_classes() {
        for theme in ThemeName::ALL {
            let tokens = theme.tokens();
            let colors = &tokens.colors;
            let rest = ButtonState::default();
            let hover = ButtonState {
                hovered: true,
                disabled: false,
            };

            let primary = button_visuals(tokens, ButtonVariant::Primary, rest);
            assert_eq!(
                (primary.background, primary.border, primary.text),
                (colors.primary, colors.primary, colors.primary_content)
            );
            assert_eq!(primary.shadows, tokens.shadows.control);
            assert_eq!(
                button_visuals(tokens, ButtonVariant::Primary, hover).background,
                colors.primary.with_alpha_percent(88)
            );

            let danger = button_visuals(tokens, ButtonVariant::Danger, rest);
            assert_eq!(danger.text, colors.error);
            assert_eq!(danger.background, colors.error.with_alpha_percent(8));
            assert_eq!(danger.border, colors.error.with_alpha_percent(28));
            let danger_hover = button_visuals(tokens, ButtonVariant::Danger, hover);
            assert_eq!(danger_hover.background, colors.error.with_alpha_percent(16));
            assert_eq!(danger_hover.border, colors.error.with_alpha_percent(50));

            let ghost = button_visuals(tokens, ButtonVariant::Ghost, rest);
            assert_eq!(ghost.background, Rgba::TRANSPARENT);
            assert_eq!(ghost.text, colors.text_muted);
            assert!(ghost.shadows.is_empty());
            let ghost_hover = button_visuals(tokens, ButtonVariant::Ghost, hover);
            assert_eq!(ghost_hover.text, colors.base_content);
            assert_eq!(
                ghost_hover.background,
                colors.base_content.with_alpha_percent(10)
            );

            for variant in [ButtonVariant::Default, ButtonVariant::Secondary] {
                let secondary = button_visuals(tokens, variant, rest);
                assert_eq!(secondary.background, colors.surface_raised);
                assert_eq!(secondary.border, colors.border_soft);
                assert_eq!(secondary.text, colors.base_content);
                assert_eq!(
                    button_visuals(tokens, variant, hover).border,
                    colors.border_strong
                );
            }
        }
        assert_eq!(ButtonVariant::default(), ButtonVariant::Secondary);
    }

    #[test]
    fn a_disabled_button_is_dimmed_and_ignores_hover() {
        let disabled_hover = ButtonState {
            hovered: true,
            disabled: true,
        };
        let visuals = button_visuals(&LIGHT, ButtonVariant::Primary, disabled_hover);

        assert_eq!(visuals.opacity, 0.5);
        assert_eq!(visuals.background, LIGHT.colors.primary);
    }

    #[test]
    fn buttons_have_the_small_daisyui_measures() {
        let visuals = button_visuals(&DARK, ButtonVariant::Secondary, ButtonState::default());

        assert_eq!(
            (
                visuals.height,
                visuals.padding_x,
                visuals.radius,
                visuals.font_size
            ),
            (32.0, 12.0, 6.0, 12.0)
        );
        assert_eq!(visuals.font_weight, 600.0);
        assert!((visuals.letter_spacing + 0.12).abs() < 1e-6);
        assert_eq!(visuals.focus_ring.color, DARK.colors.base_content);
        assert_eq!(
            (visuals.focus_ring.width, visuals.focus_ring.offset),
            (2.0, 2.0)
        );
    }

    #[test]
    fn icon_buttons_are_square_and_tinted_when_active() {
        let rest = ButtonState::default();
        let plain = icon_button_visuals(&DARK, IconButtonKind::Plain, rest);
        assert_eq!((plain.height, plain.padding_x), (36.0, 0.0));
        assert_eq!(plain.background, DARK.colors.surface_raised);

        let active = icon_button_visuals(&DARK, IconButtonKind::Active { heart: false }, rest);
        assert_eq!(active.border, DARK.colors.primary.with_alpha_percent(40));
        assert_eq!(
            active.background,
            DARK.colors.primary.with_alpha_percent(12)
        );
        assert_eq!(active.text, DARK.colors.primary_text);

        let heart = icon_button_visuals(&LIGHT, IconButtonKind::Active { heart: true }, rest);
        assert_eq!(heart.text, LIGHT.colors.favorite);
        assert_eq!(
            heart.background,
            LIGHT.colors.favorite.with_alpha_percent(12)
        );

        let hovered_active = icon_button_visuals(
            &LIGHT,
            IconButtonKind::Active { heart: true },
            ButtonState {
                hovered: true,
                disabled: false,
            },
        );
        assert_eq!(
            hovered_active.background, heart.background,
            "the tint wins on hover"
        );

        let danger = icon_button_visuals(&DARK, IconButtonKind::Danger, rest);
        assert_eq!(danger.text, DARK.colors.error);
    }
}
