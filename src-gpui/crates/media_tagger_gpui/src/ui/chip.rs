//! `UiChip`.

use gpui::{div, prelude::*, px, App, SharedString, Window};
use media_tagger_client::{
    controls::{chip_visuals, ChipTone},
    theme::ThemeName,
};

use super::{
    fonts,
    theme::{self, color, weight},
};

#[derive(IntoElement)]
pub struct Chip {
    label: SharedString,
    tone: ChipTone,
    theme: Option<ThemeName>,
}

impl Chip {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            tone: ChipTone::default(),
            theme: None,
        }
    }

    pub fn tone(mut self, tone: ChipTone) -> Self {
        self.tone = tone;
        self
    }

    pub fn theme(mut self, theme: ThemeName) -> Self {
        self.theme = Some(theme);
        self
    }
}

impl RenderOnce for Chip {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = self.theme.unwrap_or_else(|| theme::active(cx));
        let visuals = chip_visuals(theme.tokens(), self.tone);
        div()
            .flex()
            .flex_none()
            .items_center()
            .border_1()
            .border_color(color(visuals.border))
            .bg(color(visuals.background))
            .text_color(color(visuals.text))
            .px(px(visuals.padding_x))
            .py(px(visuals.padding_y))
            .rounded(px(visuals.radius))
            .font_family(fonts::MONO)
            .text_size(px(visuals.font_size))
            .line_height(px(visuals.line_height))
            .font_weight(weight(visuals.font_weight))
            .whitespace_nowrap()
            .child(self.label)
    }
}
