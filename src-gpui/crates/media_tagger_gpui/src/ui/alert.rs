//! `UiAlert`: a message with an optional title, announced as an alert.

use gpui::{div, prelude::*, px, App, ElementId, Role, SharedString, Window};
use media_tagger_client::{
    controls::{alert_visuals, AlertTone},
    theme::ThemeName,
};

use super::theme::{self, color, weight};

#[derive(IntoElement)]
pub struct Alert {
    id: ElementId,
    tone: AlertTone,
    title: Option<SharedString>,
    message: SharedString,
    theme: Option<ThemeName>,
}

impl Alert {
    pub fn new(id: impl Into<ElementId>, message: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            tone: AlertTone::default(),
            title: None,
            message: message.into(),
            theme: None,
        }
    }

    pub fn tone(mut self, tone: AlertTone) -> Self {
        self.tone = tone;
        self
    }

    pub fn title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn theme(mut self, theme: ThemeName) -> Self {
        self.theme = Some(theme);
        self
    }
}

impl RenderOnce for Alert {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = self.theme.unwrap_or_else(|| theme::active(cx));
        let visuals = alert_visuals(theme.tokens(), self.tone);
        let has_title = self.title.is_some();
        div()
            .id(self.id)
            .role(Role::Alert)
            .flex()
            .flex_col()
            .rounded(px(visuals.radius))
            .border_1()
            .border_color(color(visuals.border))
            .bg(color(visuals.background))
            .px(px(visuals.padding_x))
            .py(px(visuals.padding_y))
            .text_size(px(visuals.font_size))
            .line_height(px(visuals.line_height))
            .when_some(self.title, |alert, title| {
                alert.child(
                    div()
                        .text_color(color(visuals.title))
                        .font_weight(weight(visuals.title_weight))
                        .child(title),
                )
            })
            .child(
                div()
                    .text_color(color(visuals.body))
                    .when(has_title, |body| body.mt(px(visuals.title_gap)))
                    .child(self.message),
            )
    }
}
