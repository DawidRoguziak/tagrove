//! The settings page (`AppSettingsView.tsx`): its header with Back, the logo, the heading
//! and the window controls. The sections below it (appearance, scan, thumbnails,
//! data) are epics E17 and E18.
//!
//! The page is a non-modal layer of the layer stack while it is shown, so Escape
//! closes it; the shell registers and releases that layer, because it outlives the
//! page and knows where the focus goes back to.

use std::{cell::Cell, rc::Rc};

use gpui::{div, prelude::*, px, relative, Context, Entity, SharedString, Subscription, Window};
use media_tagger_client::{
    controls::ButtonVariant, icons::IconName, runtime::RuntimeInput, shell::settings_header,
    shortcuts::KeyContext, theme::TYPOGRAPHY, window_chrome::DragSurface,
};

use super::chrome::{self, drag_surface, no_drag};
use crate::{
    runtime::{self, Runtime},
    ui::{
        button::Button,
        logo::logo,
        theme::{self, color, weight},
    },
};

/// Element id of the Back button.
pub const BACK_ID: &str = "settings-back";
/// Debug selectors (`VisualTestContext::debug_bounds`).
pub const HEADER_SELECTOR: &str = "settings-header";
pub const HEADING_SELECTOR: &str = "settings-heading";
pub const BODY_SELECTOR: &str = "settings-body";

pub struct SettingsView {
    runtime: Entity<Runtime>,
    drag: Rc<Cell<DragSurface>>,
    _subscriptions: Vec<Subscription>,
}

impl SettingsView {
    pub fn new(runtime: Entity<Runtime>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let subscriptions = vec![
            cx.observe(&runtime, |_, _, cx| cx.notify()),
            cx.observe_window_bounds(window, |_, _, cx| cx.notify()),
        ];
        Self {
            runtime,
            drag: Rc::default(),
            _subscriptions: subscriptions,
        }
    }

    /// Whether the scan section is to be highlighted (opened by "Add first folder").
    pub fn highlights_scan(&self, cx: &gpui::App) -> bool {
        self.runtime.read(cx).model().highlight_scan()
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = theme::active(cx);
        let colors = &theme.tokens().colors;
        let i18n = self.runtime.read(cx).i18n();
        let runtime = self.runtime.downgrade();
        let back = SharedString::from(i18n.t("common.back"));

        let header = drag_surface(
            div()
                .debug_selector(|| HEADER_SELECTOR.to_owned())
                .flex_none()
                .bg(color(colors.window_chrome_bg))
                .border_b_1()
                .border_color(color(colors.border_soft))
                .px(px(settings_header::PADDING_X))
                .py(px(settings_header::PADDING_Y)),
            &self.drag,
        )
        .child(
            div()
                .flex()
                .w_full()
                .items_center()
                .gap(px(settings_header::GAP))
                .child(
                    no_drag(
                        div()
                            .id("settings-back-slot")
                            .debug_selector(|| BACK_ID.to_owned()),
                    )
                    .child(
                        Button::new(BACK_ID, back.clone())
                            .variant(ButtonVariant::Ghost)
                            .leading_icon(IconName::ArrowLeft, settings_header::BACK_ICON_SIZE)
                            .aria_label(back)
                            .on_click(move |_, _, cx| {
                                runtime::send(&runtime, RuntimeInput::CloseSettings, cx)
                            }),
                    ),
                )
                .child(logo(settings_header::LOGO_SIZE))
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .border_l_1()
                        .border_color(color(colors.border_soft))
                        .pl(px(settings_header::HEADING_PADDING_LEFT))
                        .child(
                            div()
                                .debug_selector(|| HEADING_SELECTOR.to_owned())
                                .truncate()
                                .text_size(px(settings_header::HEADING_SIZE))
                                .line_height(relative(settings_header::HEADING_LINE_HEIGHT))
                                .font_weight(weight(TYPOGRAPHY.heading_weight))
                                .child(i18n.t("settings.page.heading")),
                        )
                        .child(
                            div()
                                .mt(px(settings_header::DESCRIPTION_MARGIN_TOP))
                                .text_size(px(TYPOGRAPHY.xs.size))
                                .text_color(color(colors.text_muted))
                                .child(i18n.t("settings.page.description")),
                        ),
                )
                .child(chrome::window_controls(
                    &i18n,
                    theme,
                    chrome::window_state(window),
                )),
        );

        div()
            .key_context(KeyContext::Settings.name())
            .size_full()
            .flex()
            .flex_col()
            .child(header)
            // The sections of epics E17 and E18.
            .child(
                div()
                    .id(BODY_SELECTOR)
                    .debug_selector(|| BODY_SELECTOR.to_owned())
                    .flex_1()
                    .min_h_0()
                    .pt(px(settings_header::BODY_PADDING_TOP))
                    .overflow_y_scroll(),
            )
    }
}
