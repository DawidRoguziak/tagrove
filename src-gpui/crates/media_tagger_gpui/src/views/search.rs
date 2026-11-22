//! The search header (`SearchTagatorWrapper.tsx` with `SearchTagator.tsx`): the
//! search field with its suggestion list, the key hint, Clear and Search, and in the
//! tools row the media kind and favourites.
//!
//! The view only draws and forwards: the draft and the applied filters are the
//! runtime's [`Search`](media_tagger_client::search::Search), the suggestion session
//! is a [`Combobox`] of the client crate, and the suggestions are searched by the
//! runtime's suggestion client on the executor, never on the UI thread. The field
//! holds the text being edited; every change goes to the runtime as the draft at once.
//! A change of the draft that did not come from the field (Clear) is written back into
//! it.
//!
//! The view lives as long as the gallery view; its session state (focus, open list,
//! active item) goes with it, the draft stays in the runtime.

use std::{rc::Rc, sync::Arc};

use gpui::{
    actions, deferred, div, point, prelude::*, px, relative, AnyElement, App, BoxShadow, Context,
    Div, Entity, FocusHandle, Focusable, FontWeight, HighlightStyle, KeyBinding, MouseButton,
    Stateful, StyledText, Subscription, Window,
};
use media_tagger_client::{
    controls::{AlertTone, ButtonVariant},
    i18n::{I18n, Language},
    icons::IconName,
    runtime::RuntimeInput,
    search::{
        layout::{self, field, highlight_runs, list, shortcut_hint, tools},
        media_kind_label_key, media_kind_value, parse_media_kind_value, Combobox, ComboboxConfig,
        ComboboxEffect, ComboboxKey, EditorId, SearchCommand, TagSuggestion, MEDIA_KIND_OPTIONS,
    },
    shortcuts::{ShortcutCommand, SHORTCUTS},
    text::FieldChrome,
    theme::{ThemeName, RADII, TYPOGRAPHY},
};

use super::tag_list::TagListModal;
use crate::{
    runtime::Runtime,
    ui::{
        alert::Alert,
        button::Button,
        fonts,
        icon::Icon,
        segmented::{SegmentedControl, SegmentedEvent, SegmentedOption},
        text_input::{actions as field_actions, TextInput, TextInputEvent},
        theme::{self, color},
    },
};

actions!(
    search,
    [
        /// Ctrl+K: the focus goes to the search field.
        FocusSearch,
    ]
);

/// Element ids and debug selectors.
pub const FIELD_SELECTOR: &str = "search-field";
/// The input's box inside the field (`SearchTagator`'s `relative` wrapper).
pub const INPUT_SELECTOR: &str = "search-input";
/// The divider before the tag list's button in the tools row.
pub const DIVIDER_SELECTOR: &str = "search-tools-divider";
/// The Ctrl K hint.
pub const HINT_SELECTOR: &str = "search-shortcut-hint";
pub const LIST_SELECTOR: &str = "search-suggestions";
pub const CLEAR_ID: &str = "search-clear";
pub const SUBMIT_ID: &str = "search-submit";
pub const RETRY_ID: &str = "search-suggestions-retry";
pub const FAVORITES_ID: &str = "search-favorites";
pub const VALIDATION_SELECTOR: &str = "search-validation";

/// The debug selector of the suggestion at `index`.
pub fn option_selector(index: usize) -> String {
    format!("search-suggestion-{index}")
}

/// The shell's Ctrl+K and Meta+K from the shortcut registry.
pub fn key_bindings() -> Vec<KeyBinding> {
    SHORTCUTS
        .iter()
        .filter(|shortcut| shortcut.command == ShortcutCommand::FocusSearch)
        .map(|shortcut| {
            KeyBinding::new(
                shortcut.keystroke,
                FocusSearch,
                Some(shortcut.context.name()),
            )
        })
        .collect()
}

/// Binds the search keys. Part of `views::init`.
pub fn init(cx: &mut App) {
    cx.bind_keys(key_bindings());
}

pub struct SearchBar {
    runtime: Entity<Runtime>,
    input: Entity<TextInput>,
    kind: Entity<SegmentedControl>,
    /// The tag list's launcher and dialog (epic E13).
    tag_list: Entity<TagListModal>,
    combobox: Combobox,
    /// Tracked by the field's frame: its border is the primary colour while the
    /// focus is anywhere in it (`focus-within`).
    frame_focus: FocusHandle,
    language: Language,
    _subscriptions: Vec<Subscription>,
}

fn kind_options(i18n: &I18n<'_>) -> Vec<SegmentedOption> {
    MEDIA_KIND_OPTIONS
        .iter()
        .map(|kind| {
            SegmentedOption::new(media_kind_value(*kind), i18n.t(media_kind_label_key(*kind)))
        })
        .collect()
}

impl SearchBar {
    pub fn new(runtime: Entity<Runtime>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (i18n, language, draft, kind, vocabulary) = {
            let runtime = runtime.read(cx);
            let model = runtime.model();
            (
                runtime.i18n(),
                model.language(),
                model.search().filter_input().to_owned(),
                model.search().media_kind(),
                model.library().known_tags().clone(),
            )
        };
        let input = cx.new(|cx| {
            let mut input = TextInput::themed(window, cx);
            input.set_chrome(FieldChrome::Bare, cx);
            input.set_height(Some(field::INPUT_HEIGHT), cx);
            input.set_placeholder(i18n.t("search.placeholder"), cx);
            input.set_label(i18n.t("search.placeholder"), cx);
            input.set_text(&draft, cx);
            input
        });
        let kind = cx.new(|cx| {
            SegmentedControl::new(
                i18n.t("search.mediaKind"),
                kind_options(&i18n),
                Some(media_kind_value(kind).into()),
                cx,
            )
        });
        let tag_list = cx.new(|cx| TagListModal::new(runtime.clone(), window, cx));
        let mut combobox = Combobox::new(EditorId::next(), ComboboxConfig::SEARCH, vocabulary);
        // Nothing is focused or open yet, so this asks for nothing.
        let _ = combobox.set_value(&draft, draft.len());
        let input_focus = input.focus_handle(cx);
        let subscriptions = vec![
            cx.subscribe_in(&input, window, |this, input, event, window, cx| {
                let (text, caret) = {
                    let input = input.read(cx);
                    (input.text().to_owned(), input.cursor())
                };
                let effects = match event {
                    // A change the combobox made itself (a picked suggestion, a draft
                    // written back) is no typing.
                    TextInputEvent::Changed if text != this.combobox.value() => {
                        this.send(
                            RuntimeInput::Search(SearchCommand::Change(text.clone())),
                            cx,
                        );
                        this.combobox.input(&text, caret)
                    }
                    _ => this.combobox.caret_moved(caret),
                };
                this.run(effects, window, cx);
            }),
            // GPUI calls focus listeners while it finishes drawing a frame, when a
            // `notify` marks nothing dirty and asks for no new frame (and reaches no
            // observer). The session's reaction therefore runs right after the draw,
            // in the same effect flush, before any further input: a closed list leaves
            // the next frame instead of staying drawn over the tools row.
            cx.on_focus_in(&input_focus, window, |_, window, cx| {
                cx.defer_in(window, |this, window, cx| {
                    let caret = this.input.read(cx).cursor();
                    let effects = this.combobox.focus_in(caret);
                    this.run(effects, window, cx);
                });
            }),
            cx.on_focus_out(&input_focus, window, |_, _, window, cx| {
                cx.defer_in(window, |this, window, cx| {
                    let effects = this.combobox.focus_out();
                    this.run(effects, window, cx);
                });
            }),
            cx.subscribe(&kind, |this, _, event, cx| {
                let SegmentedEvent::Change(value) = event;
                if let Some(kind) = parse_media_kind_value(value) {
                    this.send(RuntimeInput::Search(SearchCommand::ApplyKind(kind)), cx);
                }
            }),
            cx.observe_in(&runtime, window, |this, _, window, cx| {
                this.sync(window, cx)
            }),
        ];
        cx.on_release(|this: &mut Self, cx| {
            let effects = this.combobox.dispose();
            for effect in effects {
                if let ComboboxEffect::Suggest(input) = effect {
                    this.runtime.update(cx, |runtime, cx| {
                        runtime.send(RuntimeInput::Suggestions(input), cx)
                    });
                }
            }
        })
        .detach();
        Self {
            runtime,
            input,
            kind,
            tag_list,
            combobox,
            frame_focus: cx.focus_handle(),
            language,
            _subscriptions: subscriptions,
        }
    }

    /// The suggestion session.
    pub fn combobox(&self) -> &Combobox {
        &self.combobox
    }

    /// Whether the field's frame has the primary border: the focus is in it and the
    /// window is active. WebKit matches `focus-within` only in an active window, so
    /// the Tauri field of an inactive window keeps its normal border.
    pub fn frame_highlighted(&self, window: &Window, cx: &App) -> bool {
        self.frame_focus.contains_focused(window, cx) && window.is_window_active()
    }

    pub fn input(&self) -> &Entity<TextInput> {
        &self.input
    }

    pub fn kind_control(&self) -> &Entity<SegmentedControl> {
        &self.kind
    }

    pub fn runtime(&self) -> &Entity<Runtime> {
        &self.runtime
    }

    /// The tag list's launcher and dialog.
    pub fn tag_list(&self) -> &Entity<TagListModal> {
        &self.tag_list
    }

    /// Ctrl+K: the focus goes into the field.
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.input.focus_handle(cx), cx);
    }

    fn send(&self, input: RuntimeInput, cx: &mut Context<Self>) {
        self.runtime
            .update(cx, |runtime, cx| runtime.send(input, cx));
    }

    /// Runs what the combobox asks for.
    fn run(&mut self, effects: Vec<ComboboxEffect>, window: &mut Window, cx: &mut Context<Self>) {
        for effect in effects {
            match effect {
                ComboboxEffect::Suggest(input) => self.send(RuntimeInput::Suggestions(input), cx),
                ComboboxEffect::ReplaceToken {
                    range, text, value, ..
                } => {
                    self.input
                        .update(cx, |input, cx| input.replace_range(range, &text, cx));
                    self.send(RuntimeInput::Search(SearchCommand::Change(value)), cx);
                }
                ComboboxEffect::Submit => {
                    self.send(RuntimeInput::Search(SearchCommand::Submit), cx)
                }
                ComboboxEffect::FocusInput => self.focus(window, cx),
                // The search field replaces the token itself.
                ComboboxEffect::Pick(_) => {}
            }
        }
        cx.notify();
    }

    /// Brings the view in line with the runtime: the known tags, the answers to this
    /// field, a draft changed elsewhere, the media kind and the language.
    fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let editor = self.combobox.editor();
        let (events, focus) = self.runtime.update(cx, |runtime, _| {
            (
                runtime.take_suggestion_events_for(editor),
                runtime.take_search_focus(),
            )
        });
        let mut effects = Vec::new();
        if focus {
            self.focus(window, cx);
            // Focus listeners run only in an active window, and the window of a
            // comparison scenario on a display without a window manager never
            // becomes one. WebKit sends `focusin` for a programmatic focus in an
            // inactive window, so the session hears of this focus here; an active
            // window's listener follows and repeats nothing (the request is the same).
            if !window.is_window_active() {
                let caret = self.input.read(cx).cursor();
                effects.extend(self.combobox.focus_in(caret));
            }
        }
        let (vocabulary, draft, kind, language) = {
            let model = self.runtime.read(cx).model();
            (
                model.library().known_tags().clone(),
                model.search().filter_input().to_owned(),
                model.search().media_kind(),
                model.language(),
            )
        };
        effects.extend(self.combobox.set_vocabulary(&vocabulary));
        for event in events {
            effects.extend(self.combobox.receive(event));
        }
        if draft != self.input.read(cx).text() {
            effects.extend(self.combobox.set_value(&draft, draft.len()));
            self.input
                .update(cx, |input, cx| input.set_text(&draft, cx));
        }
        self.kind.update(cx, |control, cx| {
            control.set_value(Some(media_kind_value(kind).into()), cx)
        });
        if language != self.language {
            self.language = language;
            let i18n = I18n::new(language);
            self.input.update(cx, |input, cx| {
                input.set_placeholder(i18n.t("search.placeholder"), cx);
                input.set_label(i18n.t("search.placeholder"), cx);
            });
            self.kind.update(cx, |control, cx| {
                control.set_label(i18n.t("search.mediaKind"), cx);
                control.set_options(kind_options(&i18n), cx);
            });
        }
        self.run(effects, window, cx);
    }

    fn key(&mut self, key: ComboboxKey, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let (handled, effects) = self.combobox.key(key);
        self.run(effects, window, cx);
        handled
    }

    /// The media kind and favourites of the tools row, the divider and the tag list's
    /// launcher.
    pub fn tools(&self, i18n: &I18n<'_>, theme: ThemeName, cx: &App) -> Vec<AnyElement> {
        let favorites = self.runtime.read(cx).model().search().favorites_only();
        let runtime = self.runtime.downgrade();
        // The accessible name says what a click does; the title of the Solid button
        // (`search.favoritesOnly.enabled`/`label`) waits for tooltips.
        let label = if favorites {
            "search.favoritesOnly.disable"
        } else {
            "search.favoritesOnly.enable"
        };
        vec![
            self.kind.clone().into_any_element(),
            Button::icon(FAVORITES_ID, IconName::Heart, i18n.t(label))
                .size(tools::FAVORITES_SIZE)
                .active(favorites)
                .pressed(favorites)
                .debug_selector(FAVORITES_ID)
                .on_click(move |_, _, cx| {
                    crate::runtime::send(
                        &runtime,
                        RuntimeInput::Search(SearchCommand::ApplyFavorites(!favorites)),
                        cx,
                    )
                })
                .into_any_element(),
            div()
                .debug_selector(|| DIVIDER_SELECTOR.to_owned())
                .flex_none()
                .mx(px(tools::DIVIDER_MARGIN_X))
                .w(px(tools::DIVIDER_WIDTH))
                .h(px(tools::DIVIDER_HEIGHT))
                .bg(color(theme.tokens().colors.border_strong))
                .into_any_element(),
            self.tag_list.read(cx).launcher(i18n).into_any_element(),
        ]
    }

    /// The validation message under the header, while a submit was refused.
    pub fn validation(&self, i18n: &I18n<'_>, cx: &App) -> Option<AnyElement> {
        let error = self.runtime.read(cx).model().search().validation_error()?;
        Some(
            div()
                .debug_selector(|| VALIDATION_SELECTOR.to_owned())
                .px(px(layout::alert::PADDING_X))
                .pb(px(layout::alert::PADDING_BOTTOM))
                .child(
                    Alert::new(VALIDATION_SELECTOR, i18n.t(error.i18n_key()))
                        .tone(AlertTone::Error),
                )
                .into_any_element(),
        )
    }

    fn suggestion_list(&self, theme: ThemeName, window: &Window, cx: &Context<Self>) -> AnyElement {
        let this = cx.entity().downgrade();
        let max_height = list::max_height(f32::from(window.viewport_size().height));
        let body = suggestion_list_body(
            SuggestionListIds {
                list: LIST_SELECTOR,
                option: "search-suggestion",
                option_selector,
            },
            self.combobox.items(),
            self.combobox.active(),
            theme,
            Rc::new(move |index, window, cx| {
                this.update(cx, |this, cx| {
                    let effects = this.combobox.pick_at(index);
                    this.run(effects, window, cx);
                })
                .ok();
            }),
        );
        deferred(
            body.absolute()
                .left_0()
                .right_0()
                .top(relative(1.))
                .mt(px(list::GAP))
                .max_h(px(max_height)),
        )
        .with_priority(1)
        .into_any_element()
    }
}

/// What a press on the option at an index does.
pub(crate) type PickHandler = Rc<dyn Fn(usize, &mut Window, &mut App)>;

/// The ids of a suggestion list and its options.
pub(crate) struct SuggestionListIds {
    /// The list's element id and debug selector.
    pub list: &'static str,
    /// The element id of the options, with their index.
    pub option: &'static str,
    /// The debug selector of the option at an index.
    pub option_selector: fn(usize) -> String,
}

/// A suggestion list (`SearchSuggestionsList` with its options) without its place: the
/// caller positions it and sets its height. A press on an option calls `on_pick` with
/// its index and keeps the focus in the field (`mousedown` with `preventDefault`).
pub(crate) fn suggestion_list_body(
    ids: SuggestionListIds,
    items: &Arc<[TagSuggestion]>,
    active: Option<usize>,
    theme: ThemeName,
    on_pick: PickHandler,
) -> Stateful<Div> {
    let colors = &theme.tokens().colors;
    let highlight = HighlightStyle {
        color: Some(color(colors.primary_text)),
        font_weight: Some(FontWeight::BOLD),
        ..HighlightStyle::default()
    };
    let option_selector = ids.option_selector;
    let options = items.iter().enumerate().map(|(index, item)| {
        let mut highlights = Vec::new();
        let mut at = 0;
        for (text, bold) in highlight_runs(&item.value, &item.ranges) {
            if bold {
                highlights.push((at..at + text.len(), highlight));
            }
            at += text.len();
        }
        let is_active = active == Some(index);
        let on_pick = on_pick.clone();
        div()
            .id((ids.option, index))
            .debug_selector(move || option_selector(index))
            .w_full()
            .px(px(list::OPTION_PADDING_X))
            .py(px(list::OPTION_PADDING_Y))
            .rounded(px(RADII.control))
            .text_color(color(colors.base_content))
            .cursor_pointer()
            .map(|option| {
                if is_active {
                    option
                        .bg(color(colors.tagator_suggestion_active_bg))
                        .shadow(vec![BoxShadow {
                            color: color(colors.tagator_suggestion_active_ring),
                            offset: point(px(0.), px(0.)),
                            blur_radius: px(0.),
                            spread_radius: px(list::ACTIVE_RING_WIDTH),
                            inset: true,
                        }])
                } else {
                    option.hover(|style| {
                        style.bg(color(
                            colors
                                .base_content
                                .with_alpha_percent(list::HOVER_FILL_PERCENT),
                        ))
                    })
                }
            })
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
                on_pick(index, window, cx);
            })
            .child(StyledText::new(item.value.clone()).with_highlights(highlights))
    });
    let list_selector = ids.list;
    div()
        .id(ids.list)
        .debug_selector(move || list_selector.to_owned())
        .occlude()
        // The options use the application's font; the mono font is the input's.
        .font_family(fonts::SANS)
        .text_size(px(TYPOGRAPHY.body_size))
        .overflow_y_scroll()
        .flex()
        .flex_col()
        .gap(px(list::ITEM_GAP))
        .p(px(list::PADDING))
        .rounded(px(RADII.control))
        .border_1()
        .border_color(color(colors.border_soft))
        .bg(color(colors.surface_raised))
        .shadow(theme::box_shadows(theme.tokens().shadows.popover))
        .children(options)
}

impl Render for SearchBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = theme::active(cx);
        let colors = &theme.tokens().colors;
        let i18n = self.runtime.read(cx).i18n();
        let focused = self.frame_highlighted(window, cx);
        let runtime = self.runtime.downgrade();
        let this = cx.entity().downgrade();

        // The list hangs below the input's own box (`SearchTagator`'s `relative`
        // wrapper), not below the whole field.
        let list = self
            .combobox
            .popup()
            .then(|| self.suggestion_list(theme, window, cx));
        let input = div()
            .debug_selector(|| INPUT_SELECTOR.to_owned())
            .relative()
            .flex_1()
            .min_w_0()
            .font_family(fonts::MONO)
            .text_size(px(TYPOGRAPHY.base.size))
            .on_action(cx.listener(|this, _: &field_actions::Confirm, window, cx| {
                if !this.key(ComboboxKey::Enter, window, cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &field_actions::Cancel, window, cx| {
                if !this.key(ComboboxKey::Escape, window, cx) {
                    cx.propagate();
                }
            }))
            .capture_action(cx.listener(|this, _: &field_actions::Up, window, cx| {
                if this.key(ComboboxKey::Up, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .capture_action(cx.listener(|this, _: &field_actions::Down, window, cx| {
                if this.key(ComboboxKey::Down, window, cx) {
                    cx.stop_propagation();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    let caret = this.input.read(cx).cursor();
                    let effects = this.combobox.click(caret);
                    this.run(effects, window, cx);
                }),
            )
            .child(self.input.clone())
            .children(list);

        let hint = div()
            .debug_selector(|| HINT_SELECTOR.to_owned())
            .flex_none()
            .rounded(px(shortcut_hint::RADIUS))
            .px(px(shortcut_hint::PADDING_X))
            .py(px(shortcut_hint::PADDING_Y))
            .bg(color(colors.surface_raised))
            .text_color(color(colors.text_muted))
            .font_family(fonts::MONO)
            .text_size(px(shortcut_hint::FONT_SIZE))
            .line_height(px(shortcut_hint::LINE_HEIGHT))
            .child(shortcut_hint::TEXT);

        let clear = {
            let runtime = runtime.clone();
            let this = this.clone();
            Button::icon(CLEAR_ID, IconName::Reset, i18n.t("search.clearFilters"))
                .size(field::CLEAR_SIZE)
                .transparent(true)
                .debug_selector(CLEAR_ID)
                .on_click(move |_, window, cx| {
                    crate::runtime::send(&runtime, RuntimeInput::Search(SearchCommand::Clear), cx);
                    // The focus returns to the field (`requestAnimationFrame` in Solid).
                    if let Some(this) = this.upgrade() {
                        let handle = this.read(cx).input.focus_handle(cx);
                        window.focus(&handle, cx);
                    }
                })
        };

        let submit = Button::new(SUBMIT_ID, i18n.t("workspace.search"))
            .variant(ButtonVariant::Primary)
            .height(field::SUBMIT_HEIGHT)
            .text_size(field::SUBMIT_FONT_SIZE)
            .debug_selector(SUBMIT_ID)
            .on_click(move |_, _, cx| {
                crate::runtime::send(&runtime, RuntimeInput::Search(SearchCommand::Submit), cx)
            });

        let retry = self.combobox.failed().then(|| {
            let this = this.clone();
            Button::new(RETRY_ID, i18n.t("search.suggestionsRetry"))
                .variant(ButtonVariant::Ghost)
                .debug_selector(RETRY_ID)
                .on_click(move |_, window, cx| {
                    this.update(cx, |this, cx| {
                        let effects = this.combobox.retry();
                        this.run(effects, window, cx);
                    })
                    .ok();
                })
        });

        let frame = div()
            .track_focus(&self.frame_focus)
            .flex()
            .items_center()
            .gap(px(field::GAP))
            .px(px(field::PADDING_X))
            .rounded(px(RADII.control))
            .border_1()
            .border_color(color(if focused {
                colors.primary
            } else {
                colors.border_strong
            }))
            .bg(color(colors.surface_muted))
            .child(
                Icon::new(IconName::Search)
                    .size(field::ICON_SIZE)
                    .color(color(colors.text_muted)),
            )
            .child(input)
            .children(retry)
            .child(hint)
            .child(clear)
            .child(submit);

        div()
            .id("search-field-slot")
            .debug_selector(|| FIELD_SELECTOR.to_owned())
            .relative()
            .flex_1()
            .min_w_0()
            .child(frame)
            // Draws itself over the window while open; nothing otherwise.
            .child(self.tag_list.clone())
    }
}
