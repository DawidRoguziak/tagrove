//! The placeholder view of epic E00.

use gpui::{div, prelude::*, rgb, Context, SharedString, Window};
use media_tagger_client::hello::HelloScreen;

/// Debug selector of the text element (`VisualTestContext::debug_bounds`).
pub const HELLO_TEXT_SELECTOR: &str = "hello-text";

pub struct HelloView {
    text: SharedString,
    background: u32,
    foreground: u32,
}

impl HelloView {
    pub fn new(screen: &HelloScreen) -> Self {
        Self {
            text: screen.text.clone().into(),
            background: screen.background.to_u32(),
            foreground: screen.foreground.to_u32(),
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}

impl Render for HelloView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(rgb(self.background))
            .text_color(rgb(self.foreground))
            .child(
                div()
                    .debug_selector(|| HELLO_TEXT_SELECTOR.into())
                    .child(self.text.clone()),
            )
    }
}
