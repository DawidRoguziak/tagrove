//! The values of `src/styles.css`. Edit them only together with the stylesheet;
//! `parity_tests.rs` fails when the two differ.

use std::time::Duration;

use super::{
    CubicBezier, Motion, Radii, Rgba, Shadow, Shadows, TextStep, ThemeColors, ThemeName,
    ThemeTokens, Typography,
};

const WHITE: Rgba = Rgba::hex(0xffffff);
const BLACK: Rgba = Rgba::hex(0x000000);
const MEDIA_GIF_BG: Rgba = Rgba::hex(0x1d4ed8);
const MEDIA_VIDEO_BG: Rgba = Rgba::hex(0x6d28d9);

/// The colours of `@plugin "daisyui/theme" { name: "light" }`, `[data-theme="light"]`
/// and the aliases of `:root`.
const LIGHT_COLORS: ThemeColors = {
    let primary = Rgba::hex(0x087f5b);
    let base_100 = Rgba::hex(0xffffff);
    let base_200 = Rgba::hex(0xf4f6f5);
    let base_300 = Rgba::hex(0xe8eeeb);
    ThemeColors {
        favorite: Rgba::hex(0xc64b57),
        primary,
        primary_text: Rgba::hex(0x076c4d),
        primary_content: WHITE,
        secondary: Rgba::hex(0x087f5b),
        secondary_content: WHITE,
        accent: Rgba::hex(0x087f5b),
        accent_content: WHITE,
        neutral: Rgba::hex(0x17241e),
        neutral_content: WHITE,
        base_100,
        base_200,
        base_300,
        base_content: Rgba::hex(0x17241e),
        info: Rgba::hex(0x28627e),
        info_content: WHITE,
        success: Rgba::hex(0x28623b),
        success_content: WHITE,
        warning: Rgba::hex(0x805a12),
        warning_content: WHITE,
        error: Rgba::hex(0xb13c38),
        error_content: WHITE,
        media_gif_bg: MEDIA_GIF_BG,
        media_gif_content: WHITE,
        media_video_bg: MEDIA_VIDEO_BG,
        media_video_content: WHITE,
        text_muted: Rgba::hex(0x52635a),
        border_soft: Rgba::hex(0xccd7d0),
        border_strong: Rgba::hex(0xccd7d0),
        // color-mix(in oklab, var(--color-primary) 16%, transparent)
        gallery_group_bg: primary.with_alpha_percent(16),
        gallery_group_edge: Rgba::TRANSPARENT,
        bg_0: base_200,
        bg_1: base_200,
        bg_2: base_100,
        surface_solid: base_100,
        surface_muted: base_200,
        surface_raised: base_300,
        window_chrome_bg: base_100,
        // color-mix(in srgb, var(--color-primary) 14%, var(--surface-solid))
        tagator_suggestion_active_bg: primary.mix_srgb(14, base_100),
        tagator_suggestion_active_ring: primary,
    }
};

/// The colours of `@plugin "daisyui/theme" { name: "dark" }`, `[data-theme="dark"]`
/// and the aliases of `:root`.
const DARK_COLORS: ThemeColors = {
    let primary = Rgba::hex(0x10b981);
    let base_100 = Rgba::hex(0x202224);
    let base_200 = Rgba::hex(0x141618);
    let base_300 = Rgba::hex(0x232528);
    ThemeColors {
        favorite: Rgba::hex(0xd95d67),
        primary,
        primary_text: Rgba::hex(0x10b981),
        primary_content: Rgba::hex(0x06251b),
        secondary: Rgba::hex(0x34d399),
        secondary_content: Rgba::hex(0x101812),
        accent: Rgba::hex(0x10b981),
        accent_content: Rgba::hex(0x06251b),
        neutral: Rgba::hex(0x101214),
        neutral_content: Rgba::hex(0xf4f6f5),
        base_100,
        base_200,
        base_300,
        base_content: Rgba::hex(0xf4f6f5),
        info: Rgba::hex(0x89bed7),
        info_content: Rgba::hex(0x10202b),
        success: Rgba::hex(0x83c994),
        success_content: Rgba::hex(0x102017),
        warning: Rgba::hex(0xe0b86c),
        warning_content: Rgba::hex(0x241b0c),
        error: Rgba::hex(0xee9790),
        error_content: Rgba::hex(0x2b1110),
        media_gif_bg: MEDIA_GIF_BG,
        media_gif_content: WHITE,
        media_video_bg: MEDIA_VIDEO_BG,
        media_video_content: WHITE,
        text_muted: Rgba::hex(0xa5b0aa),
        border_soft: Rgba::hex(0x2c2e31),
        border_strong: Rgba::hex(0x2c2e31),
        gallery_group_bg: Rgba::hex(0x20382b),
        gallery_group_edge: Rgba::hex(0x52735c),
        bg_0: base_200,
        bg_1: base_200,
        bg_2: base_100,
        surface_solid: base_100,
        surface_muted: base_200,
        surface_raised: base_300,
        window_chrome_bg: base_100,
        tagator_suggestion_active_bg: primary.mix_srgb(14, base_100),
        tagator_suggestion_active_ring: primary,
    }
};

/// The shadows of `:root`, resolved against the colours of one theme.
macro_rules! shadows {
    ($colors:expr) => {
        Shadows {
            // inset 0 0 0 1px var(--border-strong)
            field: &[Shadow::ring(true, 1.0, $colors.border_strong)],
            // inset 0 0 0 1px var(--color-primary),
            // 0 0 0 2px color-mix(in srgb, var(--color-primary) 25%, transparent)
            field_focus: &[
                Shadow::ring(true, 1.0, $colors.primary),
                Shadow::ring(false, 2.0, $colors.primary.with_alpha_percent(25)),
            ],
            // 0 1px 0 var(--border-soft)
            topbar: &[Shadow::drop(1.0, 0.0, $colors.border_soft)],
            surface: &[],
            // 0 4px 16px rgb(0 0 0 / 12%)
            floating: &[Shadow::drop(4.0, 16.0, BLACK.with_alpha_percent(12))],
            // 0 8px 24px rgb(0 0 0 / 20%)
            popover: &[Shadow::drop(8.0, 24.0, BLACK.with_alpha_percent(20))],
            // 0 16px 48px rgb(0 0 0 / 28%)
            modal: &[Shadow::drop(16.0, 48.0, BLACK.with_alpha_percent(28))],
            // 0 1px 2px rgb(0 0 0 / 8%)
            control: &[Shadow::drop(1.0, 2.0, BLACK.with_alpha_percent(8))],
            tile: &[],
            // 0 0 0 1px var(--border-strong)
            tile_hover: &[Shadow::ring(false, 1.0, $colors.border_strong)],
            chip: &[],
        }
    };
}

pub const LIGHT: ThemeTokens = ThemeTokens {
    name: ThemeName::Light,
    colors: LIGHT_COLORS,
    shadows: shadows!(LIGHT_COLORS),
};

pub const DARK: ThemeTokens = ThemeTokens {
    name: ThemeName::Dark,
    colors: DARK_COLORS,
    shadows: shadows!(DARK_COLORS),
};

pub const RADII: Radii = Radii {
    selector: 6.0,
    field: 6.0,
    box_: 8.0,
    tile: 4.0,
    control: 6.0,
    surface: 8.0,
    panel: 8.0,
};

pub const MOTION: Motion = Motion {
    feedback: Duration::from_millis(100),
    entrance: Duration::from_millis(150),
    exit: Duration::from_millis(100),
    ease: CubicBezier {
        x1: 0.2,
        y1: 0.0,
        x2: 0.0,
        y2: 1.0,
    },
    lightbox_entrance: Duration::from_millis(75),
    lightbox_exit: Duration::from_millis(50),
};

pub const TYPOGRAPHY: Typography = Typography {
    sans: &[
        "Inter",
        "-apple-system",
        "BlinkMacSystemFont",
        "SF Pro Text",
        "Segoe UI Variable Text",
        "Segoe UI",
        "Noto Sans",
        "sans-serif",
    ],
    mono: &["JetBrains Mono", "Noto Sans Mono", "monospace"],
    root_size: 16.0,
    root_line_height: 1.5,
    body_size: 13.0,
    // Sizes from `@theme` where styles.css overrides them; the line heights are
    // Tailwind's `--text-*--line-height` ratios, which it does not override.
    xs: TextStep {
        size: 12.0,
        line_height: 1.0 / 0.75,
    },
    sm: TextStep {
        size: 13.0,
        line_height: 1.25 / 0.875,
    },
    base: TextStep {
        size: 13.0,
        line_height: 1.5,
    },
    lg: TextStep {
        size: 18.0,
        line_height: 1.75 / 1.125,
    },
    xl: TextStep {
        size: 20.0,
        line_height: 1.75 / 1.25,
    },
    xxl: TextStep {
        size: 24.0,
        line_height: 2.0 / 1.5,
    },
    h1_size: 24.0,
    h2_size: 18.0,
    heading_weight: 650.0,
    heading_letter_spacing: -0.025,
    weight_medium: 500.0,
    weight_semibold: 600.0,
    weight_bold: 700.0,
};
