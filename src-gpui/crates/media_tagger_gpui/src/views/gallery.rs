//! The gallery view (`AppGalleryView.tsx`): the workspace header and, below it, the
//! gallery. The grid itself is epic E09; until it exists the view shows the gallery's
//! empty state, which is what an empty library shows in the Tauri application too.
//!
//! The view is dropped when the settings page opens and made anew when it closes; it
//! keeps only what may be lost then (the slider entity, the drag gesture). Everything
//! else is read from the [`Runtime`].

use std::{cell::Cell, rc::Rc};

use gpui::{
    div, prelude::*, px, relative, AnyElement, Context, Entity, FocusHandle, SharedString,
    Subscription, Window,
};
use media_tagger_client::{
    controls::{ButtonVariant, SliderRange},
    i18n::I18n,
    runtime::{RuntimeInput, TILE_SIZE_MAX, TILE_SIZE_MIN, TILE_SIZE_STEP},
    shell::{
        count_chip, empty_state, identity, item_count, search_row, tools_row, EmptyState,
        GALLERY_PADDING_TOP, PRODUCT_NAME,
    },
    shortcuts::KeyContext,
    theme::{ThemeName, RADII, TYPOGRAPHY},
    window_chrome::DragSurface,
};

use super::chrome::{self, drag_surface, no_drag};
use crate::{
    runtime::{self, Runtime},
    ui::{
        button::Button,
        fonts,
        icon::Icon,
        logo::logo,
        slider::{Slider, SliderEvent},
        theme::{self, color, weight},
    },
};

/// Element id of the settings button (`#open-settings-button`).
pub const OPEN_SETTINGS_ID: &str = "open-settings-button";
/// Debug selectors (`VisualTestContext::debug_bounds`).
pub const HEADER_SELECTOR: &str = "workspace-header";
pub const SEARCH_SLOT_SELECTOR: &str = "workspace-search-slot";
pub const COUNT_SELECTOR: &str = "workspace-count";
pub const EMPTY_STATE_SELECTOR: &str = "gallery-empty-state";
pub const ADD_FIRST_FOLDER_ID: &str = "gallery-add-first-folder";

pub struct GalleryView {
    runtime: Entity<Runtime>,
    settings_opener: FocusHandle,
    tile_size: Entity<Slider>,
    drag: Rc<Cell<DragSurface>>,
    _subscriptions: Vec<Subscription>,
}

impl GalleryView {
    /// `settings_opener` is the settings button's focus handle; the shell keeps it, so
    /// the focus can go back to the button of the next gallery view.
    pub fn new(
        runtime: Entity<Runtime>,
        settings_opener: FocusHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (size, label) = {
            let runtime = runtime.read(cx);
            (
                runtime.model().tile_size(),
                runtime.i18n().t("controls.tileSize"),
            )
        };
        let tile_size = cx.new(|cx| {
            Slider::new(
                SliderRange::with_step(
                    f64::from(TILE_SIZE_MIN),
                    f64::from(TILE_SIZE_MAX),
                    f64::from(TILE_SIZE_STEP),
                ),
                f64::from(size),
                label,
                cx,
            )
        });
        let subscriptions = vec![
            cx.subscribe(&tile_size, |this, _, event, cx| {
                let SliderEvent::Input(value) = *event;
                this.runtime.update(cx, |runtime, cx| {
                    runtime.send(RuntimeInput::SetTileSize(value), cx)
                });
            }),
            cx.observe(&runtime, |this, runtime, cx| {
                let (size, label) = {
                    let runtime = runtime.read(cx);
                    (
                        runtime.model().tile_size(),
                        runtime.i18n().t("controls.tileSize"),
                    )
                };
                this.tile_size.update(cx, |slider, cx| {
                    slider.set_value(f64::from(size), cx);
                    slider.set_label(label, cx);
                });
                cx.notify();
            }),
            cx.observe_window_bounds(window, |_, _, cx| cx.notify()),
        ];
        Self {
            runtime,
            settings_opener,
            tile_size,
            drag: Rc::default(),
            _subscriptions: subscriptions,
        }
    }

    pub fn tile_size_slider(&self) -> &Entity<Slider> {
        &self.tile_size
    }

    fn header(&self, i18n: &I18n<'_>, theme: ThemeName, window: &Window, cx: &App) -> AnyElement {
        let colors = &theme.tokens().colors;
        let model = self.runtime.read(cx).model();
        let runtime = self.runtime.downgrade();

        let identity = div()
            .flex()
            .flex_none()
            .w(px(search_row::IDENTITY_WIDTH))
            .self_stretch()
            .items_center()
            .gap(px(identity::GAP))
            .text_size(px(identity::FONT_SIZE))
            .font_weight(weight(identity::FONT_WEIGHT))
            .child(logo(identity::LOGO_SIZE))
            .child(PRODUCT_NAME);

        let open_settings = {
            let runtime = runtime.clone();
            Button::icon(
                OPEN_SETTINGS_ID,
                media_tagger_client::icons::IconName::Settings,
                i18n.t("topBar.openSettings"),
            )
            .track_focus(&self.settings_opener)
            .on_click(move |_, _, cx| runtime::send(&runtime, RuntimeInput::OpenSettings, cx))
        };

        let search = drag_surface(
            div()
                .flex()
                .items_center()
                .gap(px(search_row::COLUMN_GAP))
                .min_h(px(search_row::MIN_HEIGHT))
                .py(px(search_row::PADDING_Y))
                .px(px(search_row::PADDING_X)),
            &self.drag,
        )
        .child(identity)
        // The search field and its toolbar are epic E12.
        .child(
            div()
                .debug_selector(|| SEARCH_SLOT_SELECTOR.to_owned())
                .flex_1()
                .min_w_0(),
        )
        .child(
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap(px(search_row::ACTIONS_GAP))
                .child(
                    no_drag(
                        div()
                            .id("open-settings-slot")
                            .debug_selector(|| OPEN_SETTINGS_ID.to_owned()),
                    )
                    .child(open_settings),
                )
                .child(chrome::window_controls(
                    i18n,
                    theme,
                    chrome::window_state(window),
                )),
        );

        let count = div()
            .debug_selector(|| COUNT_SELECTOR.to_owned())
            .flex()
            .flex_none()
            .items_center()
            .gap(px(count_chip::GAP))
            .py(px(count_chip::PADDING_Y))
            .px(px(count_chip::PADDING_X))
            .rounded(px(count_chip::RADIUS))
            .bg(color(colors.surface_muted))
            .font_family(fonts::MONO)
            .text_size(px(count_chip::FONT_SIZE))
            .text_color(color(colors.text_muted))
            .child(
                div()
                    .size(px(count_chip::DOT_SIZE))
                    .flex_none()
                    .rounded_full()
                    .bg(color(colors.primary)),
            )
            .child(item_count(i18n, model.query().total()));

        let tile_size = div()
            .flex()
            .items_center()
            .gap(px(tools_row::TILE_SIZE_GAP))
            .border_l_1()
            .border_color(color(colors.border_soft))
            .pl(px(tools_row::TILE_SIZE_PADDING_LEFT))
            .child(
                div()
                    .text_size(px(TYPOGRAPHY.xs.size))
                    .text_color(color(colors.text_muted))
                    .child(i18n.t("controls.tileSize")),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .w(px(tools_row::SLIDER_WIDTH))
                    .py(px(tools_row::SLIDER_PADDING_Y))
                    .child(self.tile_size.clone()),
            );

        let tools = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(tools_row::GAP))
            .min_h(px(tools_row::MIN_HEIGHT))
            .py(px(tools_row::PADDING_Y))
            .px(px(tools_row::PADDING_X))
            .border_t_1()
            .border_color(color(colors.border_soft))
            // Media kind, favourites and the tag list come first (E12, E13).
            .child(count)
            .child(
                div()
                    .ml_auto()
                    .flex()
                    .items_center()
                    .gap(px(tools_row::SIZING_GAP))
                    // The bulk-actions toggle goes here (E11, E14).
                    .child(tile_size),
            );

        div()
            .debug_selector(|| HEADER_SELECTOR.to_owned())
            .relative()
            .flex_none()
            .bg(color(colors.window_chrome_bg))
            .border_b_1()
            .border_color(color(colors.border_strong))
            .child(search)
            .child(tools)
            .into_any_element()
    }

    fn empty_state(
        &self,
        state: EmptyState,
        i18n: &I18n<'_>,
        theme: ThemeName,
        window: &Window,
    ) -> AnyElement {
        use media_tagger_client::shell::empty_state as layout;
        let colors = &theme.tokens().colors;
        let height = (f32::from(window.viewport_size().height) - layout::VIEWPORT_INSET).max(0.0);
        let runtime = self.runtime.downgrade();
        div()
            .debug_selector(|| EMPTY_STATE_SELECTOR.to_owned())
            .mx_auto()
            .w_full()
            .max_w(px(layout::MAX_WIDTH))
            .min_h(px(height))
            .p(px(layout::PADDING))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(layout::GAP))
            .text_center()
            .child(
                div()
                    .mb(px(layout::ICON_TILE_MARGIN_BOTTOM))
                    .size(px(layout::ICON_TILE_SIZE))
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .rounded(px(RADII.control))
                    .bg(color(
                        colors
                            .primary
                            .with_alpha_percent(layout::ICON_TILE_FILL_PERCENT),
                    ))
                    .border_1()
                    .border_color(color(
                        colors
                            .primary
                            .with_alpha_percent(layout::ICON_TILE_RING_PERCENT),
                    ))
                    .child(
                        Icon::new(state.icon())
                            .size(layout::ICON_SIZE)
                            .color(color(colors.primary_text)),
                    ),
            )
            .child(
                div()
                    .text_size(px(layout::TITLE_SIZE))
                    .line_height(relative(layout::TITLE_LINE_HEIGHT))
                    .font_weight(weight(TYPOGRAPHY.heading_weight))
                    .text_color(color(colors.base_content))
                    .child(i18n.t(state.title_key())),
            )
            .child(
                div()
                    .max_w(px(layout::DESCRIPTION_MAX_WIDTH))
                    .text_size(px(layout::DESCRIPTION_SIZE))
                    .line_height(relative(layout::DESCRIPTION_LINE_HEIGHT))
                    .text_color(color(colors.text_muted))
                    .child(i18n.t(state.description_key())),
            )
            .when_some(state.action_key(), |element, key| {
                element.child(
                    Button::new(ADD_FIRST_FOLDER_ID, SharedString::from(i18n.t(key)))
                        .variant(ButtonVariant::Primary)
                        .debug_selector(ADD_FIRST_FOLDER_ID)
                        .on_click(move |_, _, cx| {
                            runtime::send(&runtime, RuntimeInput::OpenFirstFolderSettings, cx)
                        }),
                )
            })
            .into_any_element()
    }
}

use gpui::App;

impl Render for GalleryView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = theme::active(cx);
        let (i18n, empty) = {
            let runtime = self.runtime.read(cx);
            (runtime.i18n(), empty_state(runtime.model()))
        };
        let header = self.header(&i18n, theme, window, cx);
        let body = match empty {
            Some(state) => self.empty_state(state, &i18n, theme, window),
            // The virtual grid of epic E09.
            None => div().into_any_element(),
        };
        div()
            .key_context(KeyContext::Gallery.name())
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(header)
            .child(
                div()
                    .id("workspace-viewport")
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .pt(px(GALLERY_PADDING_TOP))
                    .child(body),
            )
    }
}
