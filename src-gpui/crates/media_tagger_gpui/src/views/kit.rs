//! The `kit` board (epic E05, `--scenario kit` in the E2E build): every shared control
//! in the dark theme on the left and the light theme on the right, for review against
//! `src/styles.css`. The controls work: the owner accepts what the slider, the
//! segmented control and the switch ask for, and the danger button opens a real modal
//! of its column's theme.

use gpui::{div, prelude::*, px, AnyElement, Context, Entity, SharedString, Subscription, Window};
use media_tagger_client::{
    controls::{AlertTone, ButtonVariant, ChipTone, ProgressValue, SliderRange, SpinnerSize},
    i18n::{I18n, Language},
    icons::IconName,
    kit::{KitTexts, KINDS, PROGRESS_VALUE, SLIDER_VALUE},
    overlay::ModalSize,
    theme::{ThemeName, TYPOGRAPHY},
};

use crate::ui::{
    alert::Alert,
    button::Button,
    chip::Chip,
    icon::Icon,
    layers,
    modal::{backdrop_color, modal_surface, Modal, ModalEvent, ModalState},
    progress::ProgressBar,
    segmented::{SegmentedControl, SegmentedEvent, SegmentedOption},
    slider::{Slider, SliderEvent},
    spinner::Spinner,
    switch::Switch,
    text_input::TextInput,
    theme::{self, app_root, color, weight},
};

/// One theme's column.
struct Column {
    theme: ThemeName,
    slider: Entity<Slider>,
    segmented: Entity<SegmentedControl>,
    empty_input: Entity<TextInput>,
    disabled_input: Entity<TextInput>,
    modal: Entity<ModalState>,
    switch_on: bool,
}

pub struct KitView {
    texts: KitTexts,
    columns: [Column; 2],
    _subscriptions: Vec<Subscription>,
}

impl KitView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let texts = KitTexts::new(&I18n::new(Language::En));
        let mut subscriptions = Vec::new();
        let columns = [ThemeName::Dark, ThemeName::Light].map(|theme| {
            let slider = cx.new(|cx| {
                let mut slider = Slider::new(
                    SliderRange::new(0.0, 100.0),
                    SLIDER_VALUE,
                    texts.slider.clone(),
                    cx,
                );
                slider.set_theme(Some(theme), cx);
                slider
            });
            subscriptions.push(cx.subscribe(&slider, |_, slider, event, cx| {
                let SliderEvent::Input(value) = *event;
                slider.update(cx, |slider, cx| slider.set_value(value, cx));
            }));
            let options = KINDS
                .iter()
                .zip(&texts.kinds)
                .map(|(value, label)| SegmentedOption::new(*value, label.clone()))
                .collect();
            let segmented = cx.new(|cx| {
                let mut control = SegmentedControl::new(
                    texts.media_kind.clone(),
                    options,
                    Some(KINDS[1].into()),
                    cx,
                );
                control.set_theme(Some(theme), cx);
                control
            });
            subscriptions.push(cx.subscribe(&segmented, |_, control, event, cx| {
                let SegmentedEvent::Change(value) = event.clone();
                control.update(cx, |control, cx| control.set_value(Some(value), cx));
            }));
            let empty_input = cx.new(|cx| {
                let mut input = TextInput::new(theme, window, cx);
                input.set_placeholder(texts.placeholder.clone(), cx);
                input.set_label(texts.placeholder.clone(), cx);
                input
            });
            let disabled_input = cx.new(|cx| {
                let mut input = TextInput::new(theme, window, cx);
                input.set_text(&texts.confirm_word, cx);
                input.set_disabled(true, window, cx);
                input
            });
            let modal = cx.new(ModalState::new);
            subscriptions.push(
                cx.subscribe_in(&modal, window, |_, modal, event, window, cx| {
                    if *event == ModalEvent::Dismiss {
                        modal.update(cx, |modal, cx| modal.set_open(false, window, cx));
                    }
                }),
            );
            Column {
                theme,
                slider,
                segmented,
                empty_input,
                disabled_input,
                modal,
                switch_on: true,
            }
        });
        layers::focus_root(window, cx);
        subscriptions
            .push(cx.on_focus_lost(window, |_, window, cx| layers::focus_root(window, cx)));
        Self {
            texts,
            columns,
            _subscriptions: subscriptions,
        }
    }

    /// Opens the dialog of a column (0 dark, 1 light), as its danger button does.
    pub fn open_dialog(&mut self, column: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(column) = self.columns.get(column) {
            column
                .modal
                .update(cx, |modal, cx| modal.set_open(true, window, cx));
        }
    }

    fn column(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let column = &self.columns[index];
        let texts = &self.texts;
        let theme = column.theme;
        let tokens = theme.tokens();
        let id = |name: &'static str| SharedString::from(format!("{name}-{}", theme.css_name()));
        let entity = cx.entity().downgrade();
        let modal = column.modal.clone();

        let heading = div()
            .text_size(px(TYPOGRAPHY.h2_size))
            .font_weight(weight(TYPOGRAPHY.heading_weight))
            .child(if theme == ThemeName::Dark {
                texts.dark.clone()
            } else {
                texts.light.clone()
            });

        let buttons = row()
            .child(
                Button::new(id("primary"), texts.primary.clone())
                    .variant(ButtonVariant::Primary)
                    .theme(theme),
            )
            .child(Button::new(id("secondary"), texts.secondary.clone()).theme(theme))
            .child(
                Button::new(id("ghost"), texts.ghost.clone())
                    .variant(ButtonVariant::Ghost)
                    .theme(theme),
            )
            .child(
                Button::new(id("danger"), texts.danger.clone())
                    .variant(ButtonVariant::Danger)
                    .theme(theme)
                    .on_click({
                        let modal = modal.clone();
                        move |_, window, cx| {
                            modal.update(cx, |modal, cx| modal.set_open(true, window, cx))
                        }
                    }),
            )
            .child(
                Button::new(id("disabled"), texts.disabled.clone())
                    .variant(ButtonVariant::Primary)
                    .disabled(true)
                    .theme(theme),
            );

        let icon_buttons = row()
            .child(
                Button::icon(id("settings"), IconName::Settings, texts.settings.clone())
                    .theme(theme),
            )
            .child(
                Button::icon(id("favorite"), IconName::Heart, texts.favorite.clone())
                    .active(true)
                    .theme(theme),
            )
            .child(
                Button::icon(id("bulk"), IconName::BulkActions, texts.bulk.clone())
                    .active(true)
                    .theme(theme),
            )
            .child(
                Button::icon(id("delete"), IconName::Trash, texts.delete.clone())
                    .danger(true)
                    .theme(theme),
            )
            .child(Button::icon(id("close"), IconName::Close, texts.close.clone()).theme(theme))
            .child(Chip::new(texts.chip_default.clone()).theme(theme))
            .child(
                Chip::new(texts.chip_video.clone())
                    .tone(ChipTone::Video)
                    .theme(theme),
            )
            .child(
                Chip::new(texts.chip_gif.clone())
                    .tone(ChipTone::Gif)
                    .theme(theme),
            );

        let icons = row().gap(px(12.)).children(
            IconName::ALL
                .into_iter()
                .map(|icon| Icon::new(icon).color(color(tokens.colors.base_content))),
        );

        let alerts = div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                Alert::new(id("info"), texts.info_message.clone())
                    .title(texts.info_title.clone())
                    .theme(theme),
            )
            .child(
                Alert::new(id("warning"), texts.warning_message.clone())
                    .tone(AlertTone::Warning)
                    .title(texts.warning_title.clone())
                    .theme(theme),
            )
            .child(
                Alert::new(id("error"), texts.error_message.clone())
                    .tone(AlertTone::Error)
                    .title(texts.error_title.clone())
                    .theme(theme),
            );

        let progress = div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                ProgressBar::new(
                    id("progress"),
                    ProgressValue::Determinate {
                        value: PROGRESS_VALUE as f64,
                        max: 100.0,
                    },
                )
                .label(texts.progress.clone())
                .theme(theme),
            )
            .child(
                ProgressBar::new(id("indeterminate"), ProgressValue::Indeterminate)
                    .label(texts.loading.clone())
                    .theme(theme),
            );

        let switch_on = column.switch_on;
        let toggles = row()
            .gap(px(16.))
            .child(div().w(px(200.)).child(column.slider.clone()))
            .child(column.segmented.clone());
        let switches = row()
            .gap(px(16.))
            .child(
                Switch::new(id("switch-on"), switch_on, texts.switch_label.clone())
                    .text(texts.switch.clone())
                    .theme(theme)
                    .on_change(move |checked, _, cx| {
                        entity
                            .update(cx, |kit, cx| {
                                kit.columns[index].switch_on = checked;
                                cx.notify();
                            })
                            .ok();
                    }),
            )
            .child(
                Switch::new(id("switch-off"), false, texts.switch_label.clone())
                    .disabled(true)
                    .theme(theme),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(px(TYPOGRAPHY.xs.size))
                    .text_color(color(tokens.colors.text_muted))
                    .child(Spinner::new(id("spinner-small"), SpinnerSize::ExtraSmall).theme(theme))
                    .child(texts.pending.clone()),
            )
            .child(Spinner::new(id("spinner-tile"), SpinnerSize::Tile).theme(theme));

        let inputs = row()
            .child(div().w(px(240.)).child(column.empty_input.clone()))
            .child(div().w(px(120.)).child(column.disabled_input.clone()));

        // The surface of a dialog on its backdrop, without covering the window.
        let dialog_preview = div()
            .flex()
            .justify_center()
            .p(px(16.))
            .rounded(px(8.))
            .bg(backdrop_color(theme))
            .child(dialog_content(texts, theme, 400.0, true));

        let open_modal = Modal::new(id("modal"), &column.modal)
            .label(texts.dialog_heading.clone())
            .size(ModalSize::Small)
            .theme(theme)
            .child(dialog_content(texts, theme, 0.0, false));

        app_root(div(), theme)
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .gap(px(14.))
            .p(px(20.))
            .child(heading)
            .child(buttons)
            .child(icon_buttons)
            .child(icons)
            .child(toggles)
            .child(switches)
            .child(inputs)
            .child(progress)
            .child(alerts)
            .child(dialog_preview)
            .child(open_modal)
            .into_any_element()
    }
}

fn row() -> gpui::Div {
    div().flex().flex_wrap().items_center().gap(px(8.))
}

/// The content of the removal dialog: heading, description and its two buttons. With
/// `surface`, drawn on the dialog surface `width` wide.
fn dialog_content(texts: &KitTexts, theme: ThemeName, width: f32, surface: bool) -> AnyElement {
    let tokens = theme.tokens();
    let suffix = if surface { "preview" } else { "modal" };
    let id = |name: &str| SharedString::from(format!("{name}-{suffix}-{}", theme.css_name()));
    let body = div()
        .flex()
        .flex_col()
        .gap(px(12.))
        .text_color(color(tokens.colors.base_content))
        .child(
            div()
                .text_size(px(TYPOGRAPHY.h2_size))
                .font_weight(weight(TYPOGRAPHY.heading_weight))
                .child(texts.dialog_heading.clone()),
        )
        .child(
            div()
                .text_size(px(TYPOGRAPHY.sm.size))
                .text_color(color(tokens.colors.text_muted))
                .child(texts.dialog_description.clone()),
        )
        .child(
            div()
                .flex()
                .justify_end()
                .gap(px(8.))
                .child(Button::new(id("cancel"), texts.dialog_cancel.clone()).theme(theme))
                .child(
                    Button::new(id("confirm"), texts.dialog_confirm.clone())
                        .variant(ButtonVariant::Danger)
                        .theme(theme),
                ),
        );
    if surface {
        modal_surface(theme, 1440.0)
            .w(px(width))
            .child(body)
            .into_any_element()
    } else {
        body.into_any_element()
    }
}

impl Render for KitView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dark = self.column(0, cx);
        let light = self.column(1, cx);
        app_root(layers::root(cx), theme::active(cx))
            .flex()
            .child(dark)
            .child(light)
    }
}
