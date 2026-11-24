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
    /// The text of a daisyUI `.input` element, inside a container that draws that
    /// element's frame, focus outline and the 0.55 of `input:disabled` over both (the
    /// rename field of the duplicate resolver, `input input-sm`). The measures are
    /// those of [`FieldChrome::Bare`]. Disabled, the field does not dim itself a second
    /// time: it takes the colours of daisyUI's `.input:disabled`, 40 % of the base
    /// content for the text and 20 % for the placeholder.
    Framed,
    /// A field whose utility classes replace the frame of `input` with a border on the
    /// dialog's own background, as the tag list's filter does (`border
    /// border-base-content/12 bg-base-100 px-3 py-2.5 shadow-[var(--shadow-surface)]
    /// focus-visible:border-primary/40 focus-visible:shadow-[var(--field-shadow-focus)]`).
    Outlined,
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
    /// A 1px border, drawn inside the field's bounds; only [`FieldChrome::Outlined`]
    /// has one. With a border, the inset shadows lie inside it (in the padding box), as
    /// CSS draws them.
    pub border: Option<Rgba>,
    pub text: Rgba,
    /// `input::placeholder`
    pub placeholder: Rgba,
    /// The caret has the colour of the text (`caret-color: auto`).
    pub caret: Rgba,
    /// `::selection`
    pub selection_background: Rgba,
    pub selection_text: Rgba,
    /// The border and the focus ring, which the stylesheet draws as box shadows
    /// (`--field-shadow`, `--field-shadow-focus`). Empty for [`FieldChrome::Bare`];
    /// [`FieldChrome::Outlined`] has `--shadow-surface` at rest.
    pub shadows: &'static [Shadow],
    pub radius: f32,
    pub padding_x: f32,
    pub padding_y: f32,
    /// `input:disabled` is drawn at 0.55. A [`FieldChrome::Framed`] field leaves that
    /// to its container.
    pub opacity: f32,
}

/// `padding: 8px 10px` of `input`.
const PADDING_Y: f32 = 8.0;
const PADDING_X: f32 = 10.0;
/// Tailwind `px-1`, the horizontal padding of the tag editor's inner field.
const BARE_PADDING_X: f32 = 4.0;
/// Tailwind `px-3 py-2.5` of an outlined field.
const OUTLINED_PADDING_X: f32 = 12.0;
const OUTLINED_PADDING_Y: f32 = 10.0;
/// `border-base-content/12` and `focus-visible:border-primary/40`.
const OUTLINE_PERCENT: u8 = 12;
const OUTLINE_FOCUS_PERCENT: u8 = 40;
/// `opacity` of `input:disabled`.
const DISABLED_OPACITY: f32 = 0.55;
/// daisyUI's `.input:disabled`: `color: color-mix(in oklab, var(--color-base-content)
/// 40%, transparent)`, and its placeholder `color: var(--color-base-content); opacity:
/// 0.2`.
pub const FRAMED_DISABLED_TEXT_PERCENT: u8 = 40;
pub const FRAMED_DISABLED_PLACEHOLDER_PERCENT: u8 = 20;

pub fn field_visuals(tokens: &ThemeTokens, chrome: FieldChrome, state: FieldState) -> FieldVisuals {
    let colors = &tokens.colors;
    let focused = state.focused && !state.disabled;
    let (background, border, shadows, padding_x, padding_y) = match chrome {
        FieldChrome::Boxed => (
            colors.surface_raised,
            None,
            if focused {
                tokens.shadows.field_focus
            } else {
                tokens.shadows.field
            },
            PADDING_X,
            PADDING_Y,
        ),
        FieldChrome::Bare | FieldChrome::Framed => {
            (Rgba::TRANSPARENT, None, &[][..], BARE_PADDING_X, PADDING_Y)
        }
        FieldChrome::Outlined => {
            let (border, shadows) = if focused {
                (
                    colors.primary.with_alpha_percent(OUTLINE_FOCUS_PERCENT),
                    tokens.shadows.field_focus,
                )
            } else {
                (
                    colors.base_content.with_alpha_percent(OUTLINE_PERCENT),
                    tokens.shadows.surface,
                )
            };
            (
                colors.base_100,
                Some(border),
                shadows,
                OUTLINED_PADDING_X,
                OUTLINED_PADDING_Y,
            )
        }
    };
    let framed_disabled = chrome == FieldChrome::Framed && state.disabled;
    FieldVisuals {
        background,
        border,
        text: if framed_disabled {
            colors
                .base_content
                .with_alpha_percent(FRAMED_DISABLED_TEXT_PERCENT)
        } else {
            colors.base_content
        },
        placeholder: if framed_disabled {
            colors
                .base_content
                .with_alpha_percent(FRAMED_DISABLED_PLACEHOLDER_PERCENT)
        } else {
            colors.text_muted
        },
        caret: colors.base_content,
        selection_background: colors.primary,
        selection_text: colors.primary_content,
        shadows,
        radius: RADII.control,
        padding_x,
        padding_y,
        opacity: if state.disabled && !framed_disabled {
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
    fn the_tag_list_filter_still_has_the_classes_of_an_outlined_field() {
        const TAG_LIST_MODAL: &str =
            include_str!("../../../../../src/features/search/tag-list/TagListModal.tsx");
        for class in [
            " border border-base-content/12 bg-base-100 px-3 py-2.5 ",
            " shadow-[var(--shadow-surface)] ",
            " focus-visible:border-primary/40 ",
            " focus-visible:shadow-[var(--field-shadow-focus)]",
        ] {
            assert!(
                TAG_LIST_MODAL.contains(class),
                "TagListModal.tsx lost: {class}"
            );
        }
    }

    #[test]
    fn an_outlined_field_has_a_border_on_the_base_background() {
        for theme in ThemeName::ALL {
            let tokens = theme.tokens();
            let colors = &tokens.colors;
            let rest = field_visuals(tokens, FieldChrome::Outlined, FieldState::default());

            assert_eq!(rest.background, colors.base_100);
            assert_eq!(
                rest.border,
                Some(colors.base_content.with_alpha_percent(12))
            );
            assert_eq!(rest.shadows, tokens.shadows.surface);
            assert_eq!(rest.placeholder, colors.text_muted);
            assert_eq!(
                (rest.radius, rest.padding_x, rest.padding_y),
                (6.0, 12.0, 10.0)
            );

            let focused = field_visuals(tokens, FieldChrome::Outlined, FOCUSED);
            assert_eq!(focused.border, Some(colors.primary.with_alpha_percent(40)));
            assert_eq!(focused.shadows, tokens.shadows.field_focus);

            let disabled = field_visuals(tokens, FieldChrome::Outlined, DISABLED);
            assert_eq!(disabled.border, rest.border);
            assert_eq!(disabled.opacity, 0.55);
        }
    }

    /// The rename field of the duplicate resolver: its frame carries the 0.55 of
    /// `input:disabled`, so the text inside is 40 % of the base content once (0.22 in
    /// all), not 0.55 twice (0.30).
    #[test]
    fn a_framed_field_takes_the_disabled_colours_of_the_daisy_input_and_no_opacity() {
        const ASSET_CARD: &str =
            include_str!("../../../../../src/features/settings/DuplicateAssetCard.tsx");
        assert!(
            ASSET_CARD.contains("class=\"input input-sm "),
            "DuplicateAssetCard.tsx: the rename field is no daisyUI input any more"
        );
        for theme in ThemeName::ALL {
            let tokens = theme.tokens();
            let content = tokens.colors.base_content;
            let bare = field_visuals(tokens, FieldChrome::Bare, FieldState::default());
            for state in [FieldState::default(), FOCUSED] {
                assert_eq!(field_visuals(tokens, FieldChrome::Framed, state), bare);
            }

            let disabled = field_visuals(tokens, FieldChrome::Framed, DISABLED);
            assert_eq!(disabled.opacity, 1.0);
            assert_eq!(disabled.text, content.with_alpha_percent(40));
            assert_eq!(disabled.placeholder, content.with_alpha_percent(20));
            assert_eq!(
                (disabled.background, disabled.padding_x, disabled.padding_y),
                (bare.background, bare.padding_x, bare.padding_y)
            );
            assert!(disabled.shadows.is_empty());
            // The inner field of the tag editor is an `input` of its own: 0.55.
            let bare_disabled = field_visuals(tokens, FieldChrome::Bare, DISABLED);
            assert_eq!(bare_disabled.opacity, 0.55);
            assert_eq!(bare_disabled.text, content);
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
