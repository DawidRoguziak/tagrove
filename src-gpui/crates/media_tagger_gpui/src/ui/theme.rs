//! The active theme as a GPUI global, and the conversions from the client's tokens to
//! GPUI values. The tokens themselves are `media_tagger_client::theme`.
//!
//! Every control reads the active theme when it renders, unless it is given one
//! (the `kit` board draws both themes side by side). [`set_active`] redraws every
//! window; a text field made with `TextInput::themed` follows it as well.

use gpui::{
    div, point, prelude::*, px, relative, rgba, App, BoxShadow, Div, FontWeight, Global, Hsla,
};

/// The application background (`--bg-1`) of `theme`.
pub fn background(theme: ThemeName) -> Hsla {
    color(theme.tokens().colors.bg_1)
}
use media_tagger_client::{
    controls::FocusRing,
    theme::{Rgba, Shadow, ThemeName, TYPOGRAPHY},
};

use super::fonts;

/// The theme the application draws with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ActiveTheme(pub ThemeName);

impl Global for ActiveTheme {}

/// Sets the default theme unless one is set. Part of `ui::init`.
pub fn init(cx: &mut App) {
    if !cx.has_global::<ActiveTheme>() {
        cx.set_global(ActiveTheme::default());
    }
}

/// The active theme; dark before [`init`].
pub fn active(cx: &App) -> ThemeName {
    cx.try_global::<ActiveTheme>()
        .map(|theme| theme.0)
        .unwrap_or_default()
}

/// Switches the theme of every window.
pub fn set_active(theme: ThemeName, cx: &mut App) {
    if cx.try_global::<ActiveTheme>().map(|active| active.0) == Some(theme) {
        return;
    }
    cx.set_global(ActiveTheme(theme));
    cx.refresh_windows();
}

pub fn color(color: Rgba) -> Hsla {
    rgba(color.to_rgba_u32()).into()
}

pub fn box_shadow(shadow: &Shadow) -> BoxShadow {
    BoxShadow {
        color: color(shadow.color),
        offset: point(px(shadow.offset_x), px(shadow.offset_y)),
        blur_radius: px(shadow.blur),
        spread_radius: px(shadow.spread),
        inset: shadow.inset,
    }
}

pub fn box_shadows(shadows: &[Shadow]) -> Vec<BoxShadow> {
    shadows.iter().map(box_shadow).collect()
}

/// The shadows of an element filled with `fill` and drawn at `opacity`. CSS draws an
/// outer `box-shadow` only outside the border box, while GPUI draws it beneath the whole
/// element (`fs_shadow` of its renderer has no hole), so a fill that lets the
/// background through would show the shadow inside. Such an element keeps only its
/// inset shadows: a faint edge outside it is lost instead of its colour inside.
pub fn shadows_beneath(shadows: &[Shadow], fill: Rgba, opacity: f32) -> Vec<BoxShadow> {
    let see_through = fill.a < u8::MAX || opacity < 1.0;
    shadows
        .iter()
        .filter(|shadow| shadow.inset || !see_through)
        .map(box_shadow)
        .collect()
}

pub fn weight(weight: f32) -> FontWeight {
    FontWeight(weight)
}

/// Styles `root`, a window's root element, in `theme`: the font, size, line height and
/// colours of `:root` and `body` (Inter, 13px, line height 1.5, `--bg-1`).
pub fn app_root(root: Div, theme: ThemeName) -> Div {
    let colors = &theme.tokens().colors;
    root.size_full()
        .font_family(fonts::SANS)
        .text_size(px(TYPOGRAPHY.body_size))
        .line_height(relative(TYPOGRAPHY.root_line_height))
        .text_color(color(colors.base_content))
        .bg(color(colors.bg_1))
}

/// The widths of the borders of the element a [`focus_ring`] is drawn around, in
/// pixels. An element without a border is [`RingBorder::NONE`].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct RingBorder {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl RingBorder {
    pub const NONE: Self = Self::all(0.0);

    /// The same border on every side (`border`).
    pub const fn all(width: f32) -> Self {
        Self {
            top: width,
            right: width,
            bottom: width,
            left: width,
        }
    }

    /// A border on the top edge only (`border-t`).
    pub const fn top(width: f32) -> Self {
        Self {
            top: width,
            ..Self::NONE
        }
    }

    /// A border on the bottom edge only (`border-b-2`).
    pub const fn bottom(width: f32) -> Self {
        Self {
            bottom: width,
            ..Self::NONE
        }
    }
}

/// The debug selector of the ring [`focus_ring`] draws, unless its caller gives it
/// another. One element has the keyboard focus at a time, so one ring is drawn.
pub const FOCUS_RING_SELECTOR: &str = "focus-ring";

/// The `:focus-visible` outline of an element with corner radius `radius` and the
/// borders `border`, as a child that the element must be `relative` for. It draws
/// outside the element and takes no input.
///
/// A CSS outline is measured from the border box. An absolutely placed child is laid
/// out from the padding box, inside the borders, so each side is moved out by the
/// border under it; without that the ring of a bordered element lies that much closer
/// than the outline. `radius` is the element's own, of its border box.
pub fn focus_ring(ring: FocusRing, radius: f32, border: RingBorder) -> Div {
    let outset = ring.outset();
    let frame = div()
        .debug_selector(|| FOCUS_RING_SELECTOR.to_owned())
        .absolute()
        .top(px(-(outset + border.top)))
        .left(px(-(outset + border.left)))
        .right(px(-(outset + border.right)))
        .bottom(px(-(outset + border.bottom)))
        .rounded(px(radius + outset))
        .border_color(color(ring.color));
    if ring.width >= 2.0 {
        frame.border_2()
    } else {
        frame.border_1()
    }
}

/// [`focus_ring`] for an element without a corner radius: the outline of a square box
/// has square corners.
pub fn square_focus_ring(ring: FocusRing, border: RingBorder) -> Div {
    focus_ring(ring, -ring.outset(), border)
}

#[cfg(test)]
mod tests {
    use media_tagger_client::theme::{Rgba, Shadow, ThemeName};

    use super::shadows_beneath;

    #[test]
    fn a_see_through_element_keeps_only_its_inset_shadows() {
        let colors = ThemeName::Light.tokens().colors;
        let outer = ThemeName::Light.tokens().shadows.control;
        assert!(!outer.is_empty() && outer.iter().all(|shadow| !shadow.inset));
        assert_eq!(
            shadows_beneath(outer, colors.primary, 1.0).len(),
            outer.len()
        );
        assert!(shadows_beneath(outer, colors.primary.with_alpha_percent(12), 1.0).is_empty());
        assert!(shadows_beneath(outer, colors.primary, 0.5).is_empty());
        assert!(shadows_beneath(outer, Rgba::TRANSPARENT, 1.0).is_empty());

        let ring = [Shadow::ring(true, 1.0, colors.border_strong)];
        assert_eq!(shadows_beneath(&ring, Rgba::TRANSPARENT, 0.5).len(), 1);
    }
}
