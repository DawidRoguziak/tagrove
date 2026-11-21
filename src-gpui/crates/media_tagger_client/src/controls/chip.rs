//! `UiChip`: a small mono label, as the media kind on a gallery tile.

use crate::theme::{Rgba, ThemeTokens, RADII, TYPOGRAPHY};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ChipTone {
    #[default]
    Default,
    Video,
    Gif,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChipVisuals {
    pub background: Rgba,
    pub border: Rgba,
    pub text: Rgba,
    /// `px-2`
    pub padding_x: f32,
    /// `py-1`
    pub padding_y: f32,
    /// `rounded-[var(--radius-tile)]`
    pub radius: f32,
    /// `text-xs leading-none`: the line is as high as the font.
    pub font_size: f32,
    pub line_height: f32,
    /// `font-medium`
    pub font_weight: f32,
}

pub fn chip_visuals(tokens: &ThemeTokens, tone: ChipTone) -> ChipVisuals {
    let colors = &tokens.colors;
    let white_edge = Rgba::hex(0xffffff).with_alpha_percent(20);
    let (background, border, text) = match tone {
        ChipTone::Video => (
            colors.media_video_bg,
            white_edge,
            colors.media_video_content,
        ),
        ChipTone::Gif => (colors.media_gif_bg, white_edge, colors.media_gif_content),
        ChipTone::Default => (
            colors.surface_raised,
            colors.border_soft,
            colors.base_content,
        ),
    };
    ChipVisuals {
        background,
        border,
        text,
        padding_x: 8.0,
        padding_y: 4.0,
        radius: RADII.tile,
        font_size: TYPOGRAPHY.xs.size,
        line_height: TYPOGRAPHY.xs.size,
        font_weight: TYPOGRAPHY.weight_medium,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{controls::control_source, theme::ThemeName};

    #[test]
    fn the_solid_chip_still_has_the_classes_copied_here() {
        let source = control_source("UiChip.tsx");
        for class in [
            "inline-flex items-center border px-2 py-1 font-mono text-xs font-medium leading-none",
            "rounded-[var(--radius-tile)]",
            "border-white/20 bg-[var(--media-video-bg)] text-[var(--media-video-content)]",
            "border-white/20 bg-[var(--media-gif-bg)] text-[var(--media-gif-content)]",
            "border-[var(--border-soft)] bg-[var(--surface-raised)] text-base-content",
        ] {
            assert!(source.contains(class), "UiChip.tsx lost: {class}");
        }
    }

    #[test]
    fn each_tone_uses_its_tokens() {
        for theme in ThemeName::ALL {
            let tokens = theme.tokens();
            let video = chip_visuals(tokens, ChipTone::Video);
            assert_eq!(video.background, tokens.colors.media_video_bg);
            assert_eq!(video.text, tokens.colors.media_video_content);
            assert_eq!(video.border, Rgba::hex(0xffffff).with_alpha_percent(20));
            assert_eq!(
                chip_visuals(tokens, ChipTone::Gif).background,
                tokens.colors.media_gif_bg
            );
            let plain = chip_visuals(tokens, ChipTone::Default);
            assert_eq!(plain.background, tokens.colors.surface_raised);
            assert_eq!(plain.border, tokens.colors.border_soft);
            assert_eq!(
                (plain.padding_x, plain.padding_y, plain.radius),
                (8.0, 4.0, 4.0)
            );
            assert_eq!((plain.font_size, plain.line_height), (12.0, 12.0));
        }
    }
}
