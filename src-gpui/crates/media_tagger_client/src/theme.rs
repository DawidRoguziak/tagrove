//! Theme tokens of the two themes, `light` and `dark`, as Rust constants: colours,
//! radii, shadows, motion and the type scale of `src/styles.css`, the stylesheet of
//! the Solid frontend, so both interfaces draw the same design.
//!
//! The stylesheet stays the source: `theme/parity_tests.rs` parses it, resolves
//! `var()`, `color-mix()` and `rgb()`, and fails when a token differs on either side
//! or when the stylesheet gains a token this module does not carry.

mod tokens;

#[cfg(test)]
mod css;
#[cfg(test)]
mod parity_tests;

use std::time::Duration;

pub use tokens::{DARK, LIGHT, MOTION, RADII, TYPOGRAPHY};

/// An sRGB colour with alpha, 8 bits per channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    /// 255 is opaque.
    pub a: u8,
}

impl Rgba {
    pub const TRANSPARENT: Self = Self {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    };

    /// The opaque colour `0xRRGGBB` (`#rrggbb` in CSS).
    pub const fn hex(rgb: u32) -> Self {
        Self {
            r: (rgb >> 16) as u8,
            g: (rgb >> 8) as u8,
            b: rgb as u8,
            a: 0xff,
        }
    }

    /// This colour with the alpha `percent` / 100, rounded to 8 bits:
    /// `color-mix(in srgb, <colour> <percent>%, transparent)` and
    /// `rgb(r g b / <percent>%)` of an opaque colour.
    pub const fn with_alpha_percent(self, percent: u8) -> Self {
        Self {
            a: percent_of_255(percent),
            ..self
        }
    }

    /// `color-mix(in srgb, self <percent>%, other)` of two opaque colours, each channel
    /// rounded to 8 bits.
    pub const fn mix_srgb(self, percent: u8, other: Self) -> Self {
        const fn channel(a: u8, b: u8, percent: u8) -> u8 {
            let weighted = a as u32 * percent as u32 + b as u32 * (100 - percent as u32);
            ((weighted + 50) / 100) as u8
        }
        Self {
            r: channel(self.r, other.r, percent),
            g: channel(self.g, other.g, percent),
            b: channel(self.b, other.b, percent),
            a: 0xff,
        }
    }

    /// `0xRRGGBB`, without alpha.
    pub const fn to_rgb_u32(self) -> u32 {
        ((self.r as u32) << 16) | ((self.g as u32) << 8) | self.b as u32
    }

    /// `0xRRGGBBAA`, the argument of `gpui::rgba`.
    pub const fn to_rgba_u32(self) -> u32 {
        (self.to_rgb_u32() << 8) | self.a as u32
    }

    pub const fn is_opaque(self) -> bool {
        self.a == 0xff
    }
}

/// `percent`% of 255, rounded half up (CSS keeps the exact value; 8 bits is what a
/// renderer draws).
const fn percent_of_255(percent: u8) -> u8 {
    ((255 * percent as u32 + 50) / 100) as u8
}

/// The themes `src/styles.css` defines. Dark is the default, as in the Solid frontend
/// (`readInitialTheme`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ThemeName {
    Light,
    #[default]
    Dark,
}

impl ThemeName {
    pub const ALL: [Self; 2] = [Self::Light, Self::Dark];

    /// The `name` of the theme block in the stylesheet, the value of
    /// `data-theme` and of the stored preference.
    pub const fn css_name(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    /// The theme named exactly `name` (`readInitialTheme` accepts nothing else).
    pub fn from_css_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|theme| theme.css_name() == name)
    }

    pub const fn tokens(self) -> &'static ThemeTokens {
        match self {
            Self::Light => &LIGHT,
            Self::Dark => &DARK,
        }
    }
}

/// Everything that differs between the two themes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThemeTokens {
    pub name: ThemeName,
    pub colors: ThemeColors,
    pub shadows: Shadows,
}

/// The colour tokens of one theme. Field docs name the CSS custom property.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeColors {
    /// `--color-favorite`
    pub favorite: Rgba,
    /// `--color-primary`
    pub primary: Rgba,
    /// `--color-primary-text`: primary as readable text on the base colours.
    pub primary_text: Rgba,
    /// `--color-primary-content`: text on `primary`.
    pub primary_content: Rgba,
    /// `--color-secondary`
    pub secondary: Rgba,
    /// `--color-secondary-content`
    pub secondary_content: Rgba,
    /// `--color-accent`
    pub accent: Rgba,
    /// `--color-accent-content`
    pub accent_content: Rgba,
    /// `--color-neutral`
    pub neutral: Rgba,
    /// `--color-neutral-content`
    pub neutral_content: Rgba,
    /// `--color-base-100`
    pub base_100: Rgba,
    /// `--color-base-200`
    pub base_200: Rgba,
    /// `--color-base-300`
    pub base_300: Rgba,
    /// `--color-base-content`: the default text colour.
    pub base_content: Rgba,
    /// `--color-info`
    pub info: Rgba,
    /// `--color-info-content`
    pub info_content: Rgba,
    /// `--color-success`
    pub success: Rgba,
    /// `--color-success-content`
    pub success_content: Rgba,
    /// `--color-warning`
    pub warning: Rgba,
    /// `--color-warning-content`
    pub warning_content: Rgba,
    /// `--color-error`
    pub error: Rgba,
    /// `--color-error-content`
    pub error_content: Rgba,
    /// `--media-gif-bg`: the GIF chip.
    pub media_gif_bg: Rgba,
    /// `--media-gif-content`
    pub media_gif_content: Rgba,
    /// `--media-video-bg`: the video chip.
    pub media_video_bg: Rgba,
    /// `--media-video-content`
    pub media_video_content: Rgba,
    /// `--text-muted`
    pub text_muted: Rgba,
    /// `--border-soft`
    pub border_soft: Rgba,
    /// `--border-strong`
    pub border_strong: Rgba,
    /// `--gallery-group-bg`: the band behind a media group in the gallery.
    pub gallery_group_bg: Rgba,
    /// `--gallery-group-edge`
    pub gallery_group_edge: Rgba,
    /// `--bg-0`
    pub bg_0: Rgba,
    /// `--bg-1`: the application background.
    pub bg_1: Rgba,
    /// `--bg-2`
    pub bg_2: Rgba,
    /// `--surface-solid`
    pub surface_solid: Rgba,
    /// `--surface-muted`
    pub surface_muted: Rgba,
    /// `--surface-raised`
    pub surface_raised: Rgba,
    /// `--window-chrome-bg`: the header and the window controls.
    pub window_chrome_bg: Rgba,
    /// `--tagator-suggestion-active-bg`: the active suggestion and settings link.
    pub tagator_suggestion_active_bg: Rgba,
    /// `--tagator-suggestion-active-ring`
    pub tagator_suggestion_active_ring: Rgba,
}

impl ThemeColors {
    /// Every colour with the name of its CSS custom property.
    pub const fn css_entries(&self) -> [(&'static str, Rgba); 40] {
        [
            ("--color-favorite", self.favorite),
            ("--color-primary", self.primary),
            ("--color-primary-text", self.primary_text),
            ("--color-primary-content", self.primary_content),
            ("--color-secondary", self.secondary),
            ("--color-secondary-content", self.secondary_content),
            ("--color-accent", self.accent),
            ("--color-accent-content", self.accent_content),
            ("--color-neutral", self.neutral),
            ("--color-neutral-content", self.neutral_content),
            ("--color-base-100", self.base_100),
            ("--color-base-200", self.base_200),
            ("--color-base-300", self.base_300),
            ("--color-base-content", self.base_content),
            ("--color-info", self.info),
            ("--color-info-content", self.info_content),
            ("--color-success", self.success),
            ("--color-success-content", self.success_content),
            ("--color-warning", self.warning),
            ("--color-warning-content", self.warning_content),
            ("--color-error", self.error),
            ("--color-error-content", self.error_content),
            ("--media-gif-bg", self.media_gif_bg),
            ("--media-gif-content", self.media_gif_content),
            ("--media-video-bg", self.media_video_bg),
            ("--media-video-content", self.media_video_content),
            ("--text-muted", self.text_muted),
            ("--border-soft", self.border_soft),
            ("--border-strong", self.border_strong),
            ("--gallery-group-bg", self.gallery_group_bg),
            ("--gallery-group-edge", self.gallery_group_edge),
            ("--bg-0", self.bg_0),
            ("--bg-1", self.bg_1),
            ("--bg-2", self.bg_2),
            ("--surface-solid", self.surface_solid),
            ("--surface-muted", self.surface_muted),
            ("--surface-raised", self.surface_raised),
            ("--window-chrome-bg", self.window_chrome_bg),
            (
                "--tagator-suggestion-active-bg",
                self.tagator_suggestion_active_bg,
            ),
            (
                "--tagator-suggestion-active-ring",
                self.tagator_suggestion_active_ring,
            ),
        ]
    }

    /// The colour of a CSS custom property such as `--color-base-200`.
    pub fn by_css_name(&self, name: &str) -> Option<Rgba> {
        self.css_entries()
            .into_iter()
            .find_map(|(css_name, color)| (css_name == name).then_some(color))
    }
}

/// The colour token `name` (a CSS custom property such as `--color-base-200`) of
/// `theme`.
pub fn color_token(theme: ThemeName, name: &str) -> Option<Rgba> {
    theme.tokens().colors.by_css_name(name)
}

/// One layer of a CSS `box-shadow`, lengths in CSS pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shadow {
    pub inset: bool,
    pub offset_x: f32,
    pub offset_y: f32,
    pub blur: f32,
    pub spread: f32,
    pub color: Rgba,
}

impl Shadow {
    /// An outer shadow.
    pub const fn drop(offset_y: f32, blur: f32, color: Rgba) -> Self {
        Self {
            inset: false,
            offset_x: 0.0,
            offset_y,
            blur,
            spread: 0.0,
            color,
        }
    }

    /// A ring of `width` drawn with `spread` (`0 0 0 <width>px`), inside the box when
    /// `inset`.
    pub const fn ring(inset: bool, width: f32, color: Rgba) -> Self {
        Self {
            inset,
            offset_x: 0.0,
            offset_y: 0.0,
            blur: 0.0,
            spread: width,
            color,
        }
    }
}

/// The box shadows of one theme; an empty list is `none`. Layers are in CSS order
/// (the first is drawn on top).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shadows {
    /// `--field-shadow`: the border of text fields and selects.
    pub field: &'static [Shadow],
    /// `--field-shadow-focus`
    pub field_focus: &'static [Shadow],
    /// `--shadow-topbar`
    pub topbar: &'static [Shadow],
    /// `--shadow-surface`
    pub surface: &'static [Shadow],
    /// `--shadow-floating`
    pub floating: &'static [Shadow],
    /// `--shadow-popover`
    pub popover: &'static [Shadow],
    /// `--shadow-modal`
    pub modal: &'static [Shadow],
    /// `--shadow-control`
    pub control: &'static [Shadow],
    /// `--shadow-tile`
    pub tile: &'static [Shadow],
    /// `--shadow-tile-hover`
    pub tile_hover: &'static [Shadow],
    /// `--shadow-chip`
    pub chip: &'static [Shadow],
}

impl Shadows {
    /// Every shadow with the name of its CSS custom property.
    pub const fn css_entries(&self) -> [(&'static str, &'static [Shadow]); 11] {
        [
            ("--field-shadow", self.field),
            ("--field-shadow-focus", self.field_focus),
            ("--shadow-topbar", self.topbar),
            ("--shadow-surface", self.surface),
            ("--shadow-floating", self.floating),
            ("--shadow-popover", self.popover),
            ("--shadow-modal", self.modal),
            ("--shadow-control", self.control),
            ("--shadow-tile", self.tile),
            ("--shadow-tile-hover", self.tile_hover),
            ("--shadow-chip", self.chip),
        ]
    }
}

/// Corner radii in CSS pixels; the same in both themes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Radii {
    /// `--radius-selector` (DaisyUI: checkboxes, toggles, badges).
    pub selector: f32,
    /// `--radius-field` (DaisyUI: buttons, inputs, selects).
    pub field: f32,
    /// `--radius-box` (DaisyUI: cards, modals, alerts).
    pub box_: f32,
    /// `--radius-tile`: gallery tiles.
    pub tile: f32,
    /// `--radius-control`
    pub control: f32,
    /// `--radius-surface`
    pub surface: f32,
    /// `--radius-panel`
    pub panel: f32,
}

impl Radii {
    pub const fn css_entries(&self) -> [(&'static str, f32); 7] {
        [
            ("--radius-selector", self.selector),
            ("--radius-field", self.field),
            ("--radius-box", self.box_),
            ("--radius-tile", self.tile),
            ("--radius-control", self.control),
            ("--radius-surface", self.surface),
            ("--radius-panel", self.panel),
        ]
    }
}

/// A CSS `cubic-bezier(x1, y1, x2, y2)` easing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CubicBezier {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
}

impl CubicBezier {
    /// CSS `linear`.
    pub const LINEAR: Self = Self {
        x1: 0.0,
        y1: 0.0,
        x2: 1.0,
        y2: 1.0,
    };
    /// CSS `ease-out`.
    pub const EASE_OUT: Self = Self {
        x1: 0.0,
        y1: 0.0,
        x2: 0.58,
        y2: 1.0,
    };

    /// The eased progress at `progress` (the x of the curve, clamped to 0..=1), as a
    /// browser computes a transition: the curve is solved for x and y is returned.
    pub fn sample(self, progress: f32) -> f32 {
        let x = progress.clamp(0.0, 1.0);
        if x == 0.0 || x == 1.0 {
            return x;
        }
        // B(t) = 3(1-t)²t·p1 + 3(1-t)t²·p2 + t³ for one coordinate.
        let curve = |t: f32, p1: f32, p2: f32| {
            let inverse = 1.0 - t;
            3.0 * inverse * inverse * t * p1 + 3.0 * inverse * t * t * p2 + t * t * t
        };
        // x(t) is monotonic for 0 <= x1, x2 <= 1, so bisection finds t.
        let (mut low, mut high) = (0.0_f32, 1.0_f32);
        let mut t = x;
        for _ in 0..32 {
            let at = curve(t, self.x1, self.x2);
            if (at - x).abs() < 1e-6 {
                break;
            }
            if at < x {
                low = t;
            } else {
                high = t;
            }
            t = (low + high) / 2.0;
        }
        curve(t, self.y1, self.y2)
    }
}

/// Animation timing. Motion never animates geometry, only opacity and colours
/// (docs/frontend/architecture-and-ui-conventions.md).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Motion {
    /// `--motion-feedback`: hover, press and colour changes.
    pub feedback: Duration,
    /// `--motion-entrance`: fading in.
    pub entrance: Duration,
    /// `--motion-exit`: fading out.
    pub exit: Duration,
    /// `--motion-ease`
    pub ease: CubicBezier,
    /// `--modal-entrance` of the lightbox backdrop, which replaces `entrance` there.
    pub lightbox_entrance: Duration,
    /// `--modal-exit` of the lightbox backdrop.
    pub lightbox_exit: Duration,
}

/// A step of the type scale: size in CSS pixels and line height as a multiple of it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextStep {
    pub size: f32,
    pub line_height: f32,
}

impl TextStep {
    /// The line height in CSS pixels.
    pub fn line_height_px(self) -> f32 {
        self.size * self.line_height
    }
}

/// Fonts and the type scale (Tailwind `text-*` utilities as `src/styles.css`
/// configures them, plus the element styles of its base layer).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Typography {
    /// `font-family` of `:root`, in fallback order. Registering the bundled fonts is
    /// epic E05.
    pub sans: &'static [&'static str],
    /// `--font-mono`
    pub mono: &'static [&'static str],
    /// `font-size` of `:root`: the size of `1rem`.
    pub root_size: f32,
    /// `line-height` of `:root`.
    pub root_line_height: f32,
    /// `font-size` of `body`: the size of ordinary text.
    pub body_size: f32,
    /// `text-xs` (Tailwind default, not overridden).
    pub xs: TextStep,
    /// `text-sm` (`--text-sm`).
    pub sm: TextStep,
    /// `text-base` (`--text-base`).
    pub base: TextStep,
    /// `text-lg` (`--text-lg`).
    pub lg: TextStep,
    /// `text-xl` (Tailwind default, not overridden).
    pub xl: TextStep,
    /// `text-2xl` (`--text-2xl`).
    pub xxl: TextStep,
    /// `font-size` of `h1`.
    pub h1_size: f32,
    /// `font-size` of `h2` and `h3`.
    pub h2_size: f32,
    /// `font-weight` of `h1`, `h2` and `h3`.
    pub heading_weight: f32,
    /// `letter-spacing` of `h1`, `h2` and `h3`, in `em`.
    pub heading_letter_spacing: f32,
    /// Tailwind `font-medium`.
    pub weight_medium: f32,
    /// Tailwind `font-semibold`.
    pub weight_semibold: f32,
    /// Tailwind `font-bold`.
    pub weight_bold: f32,
}

#[cfg(test)]
mod tests {
    use super::{color_token, Rgba, ThemeName};

    #[test]
    fn colours_pack_as_rrggbb_and_rrggbbaa() {
        let color = Rgba::hex(0x141618);

        assert_eq!(
            color,
            Rgba {
                r: 0x14,
                g: 0x16,
                b: 0x18,
                a: 0xff
            }
        );
        assert_eq!(color.to_rgb_u32(), 0x141618);
        assert_eq!(color.to_rgba_u32(), 0x141618ff);
        assert!(color.is_opaque());
        assert_eq!(color.with_alpha_percent(25).to_rgba_u32(), 0x14161840);
        assert!(!Rgba::TRANSPARENT.is_opaque());
    }

    #[test]
    fn alpha_and_mixing_round_to_the_nearest_step() {
        let black = Rgba::hex(0x000000);
        let white = Rgba::hex(0xffffff);

        assert_eq!(black.with_alpha_percent(12).a, 31, "30.6");
        assert_eq!(black.with_alpha_percent(28).a, 71, "71.4");
        assert_eq!(black.with_alpha_percent(100).a, 255);
        assert_eq!(black.with_alpha_percent(0).a, 0);
        assert_eq!(
            white.mix_srgb(50, black),
            Rgba::hex(0x808080),
            "127.5 rounds up"
        );
        assert_eq!(white.mix_srgb(100, black), white);
        assert_eq!(white.mix_srgb(0, black), black);
    }

    #[test]
    fn theme_names_round_trip_and_dark_is_the_default() {
        for theme in ThemeName::ALL {
            assert_eq!(ThemeName::from_css_name(theme.css_name()), Some(theme));
            assert_eq!(theme.tokens().name, theme);
        }
        assert_eq!(ThemeName::from_css_name("Dark"), None);
        assert_eq!(ThemeName::from_css_name("solarized"), None);
        assert_eq!(ThemeName::default(), ThemeName::Dark);
    }

    #[test]
    fn colour_tokens_are_found_by_their_css_name() {
        assert_eq!(
            color_token(ThemeName::Dark, "--color-base-200"),
            Some(Rgba::hex(0x141618))
        );
        assert_eq!(
            color_token(ThemeName::Light, "--color-base-200"),
            Some(Rgba::hex(0xf4f6f5))
        );
        assert_eq!(color_token(ThemeName::Dark, "--color-base"), None);
        assert_eq!(color_token(ThemeName::Dark, ""), None);
    }

    #[test]
    fn easing_curves_start_at_zero_end_at_one_and_follow_their_shape() {
        use super::{CubicBezier, MOTION};

        for curve in [CubicBezier::LINEAR, CubicBezier::EASE_OUT, MOTION.ease] {
            assert_eq!(curve.sample(0.0), 0.0);
            assert_eq!(curve.sample(1.0), 1.0);
            assert_eq!(curve.sample(-1.0), 0.0);
            assert_eq!(curve.sample(2.0), 1.0);
        }
        assert!((CubicBezier::LINEAR.sample(0.3) - 0.3).abs() < 1e-4);
        // ease-out and the app's cubic-bezier(0.2, 0, 0, 1) run ahead of linear.
        assert!(CubicBezier::EASE_OUT.sample(0.5) > 0.5);
        assert!(MOTION.ease.sample(0.5) > 0.8);
        let mut previous = 0.0;
        for step in 1..=20 {
            let value = MOTION.ease.sample(step as f32 / 20.0);
            assert!(value >= previous, "monotonic at {step}");
            previous = value;
        }
    }

    #[test]
    fn css_names_are_unique() {
        for theme in ThemeName::ALL {
            let tokens = theme.tokens();
            let mut names: Vec<&str> = tokens
                .colors
                .css_entries()
                .iter()
                .map(|(name, _)| *name)
                .chain(tokens.shadows.css_entries().iter().map(|(name, _)| *name))
                .collect();
            let count = names.len();
            names.sort_unstable();
            names.dedup();
            assert_eq!(names.len(), count, "{}", theme.css_name());
        }
    }
}
