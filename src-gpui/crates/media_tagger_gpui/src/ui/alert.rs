//! `UiAlert`: a message with an optional title, announced as an alert.

use gpui::{div, prelude::*, px, AnyElement, App, ElementId, Role, SharedString, Window};
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
    debug_selector: Option<&'static str>,
    /// What follows the message inside the alert (a Retry button).
    children: Vec<AnyElement>,
}

impl ParentElement for Alert {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl Alert {
    pub fn new(id: impl Into<ElementId>, message: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            tone: AlertTone::default(),
            title: None,
            message: message.into(),
            theme: None,
            debug_selector: None,
            children: Vec::new(),
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

    /// A debug selector for tests (`data-testid`); a no-op outside test builds.
    pub fn debug_selector(mut self, selector: &'static str) -> Self {
        self.debug_selector = Some(selector);
        self
    }
}

impl RenderOnce for Alert {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = self.theme.unwrap_or_else(|| theme::active(cx));
        let visuals = alert_visuals(theme.tokens(), self.tone);
        let has_title = self.title.is_some();
        let has_message = !self.message.is_empty();
        let has_children = !self.children.is_empty();
        div()
            .id(self.id)
            .role(Role::Alert)
            .when_some(self.debug_selector, |alert, selector| {
                alert.debug_selector(move || selector.to_owned())
            })
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
            .when(has_message || has_children, |alert| {
                alert.child(
                    div()
                        .flex()
                        .flex_col()
                        .items_start()
                        .gap(px(8.0))
                        .text_color(color(visuals.body))
                        .when(has_title, |body| body.mt(px(visuals.title_gap)))
                        .when(has_message, |body| body.child(self.message))
                        .children(self.children),
                )
            })
    }
}
