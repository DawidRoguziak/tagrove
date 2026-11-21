//! `UiAlert`: an inline message with an optional title.

use crate::theme::{Rgba, ThemeTokens, RADII, TYPOGRAPHY};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AlertTone {
    #[default]
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AlertVisuals {
    /// `bg-<tone>/14`
    pub background: Rgba,
    /// `border-<tone>/52`
    pub border: Rgba,
    /// The title: `text-xs font-semibold leading-5 text-base-content`.
    pub title: Rgba,
    /// The message: `text-xs leading-5 text-base-content/78`.
    pub body: Rgba,
    pub radius: f32,
    /// `px-3.5`
    pub padding_x: f32,
    /// `py-3`
    pub padding_y: f32,
    pub font_size: f32,
    /// `leading-5`
    pub line_height: f32,
    pub title_weight: f32,
    /// `mt-0.5` of the message below a title.
    pub title_gap: f32,
}

pub fn alert_visuals(tokens: &ThemeTokens, tone: AlertTone) -> AlertVisuals {
    let colors = &tokens.colors;
    let tint = match tone {
        AlertTone::Info => colors.info,
        AlertTone::Warning => colors.warning,
        AlertTone::Error => colors.error,
    };
    AlertVisuals {
        background: tint.with_alpha_percent(14),
        border: tint.with_alpha_percent(52),
        title: colors.base_content,
        body: colors.base_content.with_alpha_percent(78),
        radius: RADII.control,
        padding_x: 14.0,
        padding_y: 12.0,
        font_size: TYPOGRAPHY.xs.size,
        line_height: 20.0,
        title_weight: TYPOGRAPHY.weight_semibold,
        title_gap: 2.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{controls::control_source, theme::ThemeName};

    #[test]
    fn the_solid_alert_still_has_the_classes_copied_here() {
        let source = control_source("UiAlert.tsx");
        for class in [
            "rounded-[var(--radius-control)] border px-3.5 py-3 text-base-content",
            "border-warning/52 bg-warning/14",
            "border-error/52 bg-error/14",
            "border-info/52 bg-info/14",
            "m-0 text-xs font-semibold leading-5 text-base-content",
            "m-0 text-xs leading-5 text-base-content/78",
            "mt-0.5",
            r#"tone: "info""#,
        ] {
            assert!(source.contains(class), "UiAlert.tsx lost: {class}");
        }
    }

    #[test]
    fn each_tone_tints_border_and_background() {
        for theme in ThemeName::ALL {
            let tokens = theme.tokens();
            for (tone, tint) in [
                (AlertTone::Info, tokens.colors.info),
                (AlertTone::Warning, tokens.colors.warning),
                (AlertTone::Error, tokens.colors.error),
            ] {
                let visuals = alert_visuals(tokens, tone);
                assert_eq!(visuals.border, tint.with_alpha_percent(52));
                assert_eq!(visuals.background, tint.with_alpha_percent(14));
                assert_eq!(visuals.title, tokens.colors.base_content);
                assert_eq!(
                    visuals.body,
                    tokens.colors.base_content.with_alpha_percent(78)
                );
            }
        }
        assert_eq!(AlertTone::default(), AlertTone::Info);
    }
}
