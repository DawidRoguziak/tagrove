//! The tag editor (`TagEditor.tsx`): the assigned tags as chips and a borderless field
//! in one frame, with the suggestions of the search field. The lightbox (epic E16) and
//! the bulk dock (epic E14) own one each.
//!
//! The owner gives the editor its [`TagEditorProps`] (tags, texts, disabled, loading)
//! and listens to [`TagEditorEvent`]: a typed or picked tag to add, a chip's tag to
//! remove, a changed draft. The editor never writes anything itself. Its field's session
//! is a [`Combobox`] with [`TAG_EDITOR_COMBOBOX`]; suggestions are searched by the
//! runtime's suggestion client on the executor and never include the assigned tags.
//! The list is drawn over the window next to the field (the `viewport` strategy), so
//! the frame's own scrolling never clips it.

use std::{cell::Cell, rc::Rc};

use gpui::{
    canvas, deferred, div, prelude::*, px, AnyElement, App, Bounds, Context, Entity, EventEmitter,
    FocusHandle, Focusable, MouseButton, Pixels, ScrollHandle, SharedString, Subscription, Window,
};
use media_tagger_client::{
    icons::IconName,
    runtime::RuntimeInput,
    search::{Combobox, ComboboxEffect, ComboboxKey, EditorId},
    tagging::{
        editing_disabled, editor_content, editor_visuals, layout, list_placement, EditorContent,
        Rect, SuggestionPlacement, TagEditorVariant, TAG_EDITOR_COMBOBOX,
    },
    text::FieldChrome,
    theme::{RADII, TYPOGRAPHY},
};

use super::search::{suggestion_list_body, SuggestionListIds};
use crate::{
    runtime::Runtime,
    ui::{
        button::Button,
        fonts,
        text_input::{actions as field_actions, TextInput, TextInputEvent},
        theme::{self, box_shadows, color, weight},
    },
};

/// The accessible name of a chip's remove button, from its tag.
pub type RemoveLabel = Rc<dyn Fn(&str) -> SharedString>;

/// What the owner decides.
#[derive(Clone, Default)]
pub struct TagEditorProps {
    pub variant: TagEditorVariant,
    /// The assigned tags, shown as chips and never suggested.
    pub tags: Vec<String>,
    /// Shown without tags (`emptyText`).
    pub empty_text: Option<SharedString>,
    /// Shown instead of the chips while the tags load; editing is disabled then
    /// (`loadingText`).
    pub loading_text: Option<SharedString>,
    pub disabled: bool,
    /// The field's accessible name (`inputAriaLabel`).
    pub input_label: SharedString,
    /// The suggestion list's accessible name (`listboxAriaLabel`).
    pub list_label: SharedString,
    pub placeholder: SharedString,
    /// Chips get a remove button with this name; `None`: add-only chips.
    pub remove_label: Option<RemoveLabel>,
    pub placement: SuggestionPlacement,
}

/// What the editor asks of its owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagEditorEvent {
    /// The field's text changed by typing (`onDraftChange`).
    DraftChanged(String),
    /// Enter without an active suggestion (the draft), or a picked suggestion
    /// (`onAddTag`).
    AddTag(String),
    /// A chip's remove button (`onRemoveTag`).
    RemoveTag(String),
}

/// The debug selector of the field's box and of the suggestion list.
pub const INPUT_SELECTOR: &str = "tag-editor-input";
pub const LIST_SELECTOR: &str = "tag-editor-suggestions";

/// The debug selector of the suggestion at `index`.
pub fn option_selector(index: usize) -> String {
    format!("tag-editor-suggestion-{index}")
}

/// The debug selector of the chip of the tag at `index` and of its remove button.
pub fn chip_selector(index: usize) -> String {
    format!("tag-editor-chip-{index}")
}

pub fn remove_selector(index: usize) -> String {
    format!("tag-editor-remove-{index}")
}

pub struct TagEditor {
    runtime: Entity<Runtime>,
    input: Entity<TextInput>,
    combobox: Combobox,
    props: TagEditorProps,
    scroll: ScrollHandle,
    /// Where the field's box was last laid out, for the suggestion list.
    anchor: Rc<Cell<Option<Bounds<Pixels>>>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<TagEditorEvent> for TagEditor {}

impl Focusable for TagEditor {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.focus_handle(cx)
    }
}

impl TagEditor {
    pub fn new(
        runtime: Entity<Runtime>,
        props: TagEditorProps,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let vocabulary = runtime.read(cx).model().library().known_tags().clone();
        let input = cx.new(|cx| {
            let mut input = TextInput::themed(window, cx);
            input.set_chrome(FieldChrome::Bare, cx);
            input.set_height(Some(layout::INPUT_HEIGHT), cx);
            input
        });
        let combobox = Combobox::new(EditorId::next(), TAG_EDITOR_COMBOBOX, vocabulary);
        let input_focus = input.focus_handle(cx);
        let subscriptions = vec![
            cx.subscribe_in(&input, window, |this, input, event, window, cx| {
                let (text, caret) = {
                    let input = input.read(cx);
                    (input.text().to_owned(), input.cursor())
                };
                let effects = match event {
                    TextInputEvent::Changed if text != this.combobox.value() => {
                        cx.emit(TagEditorEvent::DraftChanged(text.clone()));
                        this.combobox.input(&text, caret)
                    }
                    _ => this.combobox.caret_moved(caret),
                };
                this.run(effects, window, cx);
            }),
            // As in the search header: the session reacts right after the frame in
            // which the focus moved.
            cx.on_focus_in(&input_focus, window, |_, window, cx| {
                cx.defer_in(window, |this, window, cx| {
                    let caret = this.input.read(cx).cursor();
                    let effects = this.combobox.focus_in(caret);
                    this.run(effects, window, cx);
                    this.reveal_input(window, cx);
                });
            }),
            cx.on_focus_out(&input_focus, window, |_, _, window, cx| {
                cx.defer_in(window, |this, window, cx| {
                    let effects = this.combobox.focus_out();
                    this.run(effects, window, cx);
                });
            }),
            cx.observe_in(&runtime, window, |this, _, window, cx| {
                this.sync(window, cx)
            }),
        ];
        cx.on_release(|this: &mut Self, cx| {
            for effect in this.combobox.dispose() {
                if let ComboboxEffect::Suggest(input) = effect {
                    this.runtime.update(cx, |runtime, cx| {
                        runtime.send(RuntimeInput::Suggestions(input), cx)
                    });
                }
            }
        })
        .detach();
        let mut editor = Self {
            runtime,
            input,
            combobox,
            props: TagEditorProps::default(),
            scroll: ScrollHandle::new(),
            anchor: Rc::default(),
            _subscriptions: subscriptions,
        };
        editor.set_props(props, window, cx);
        editor
    }

    pub fn props(&self) -> &TagEditorProps {
        &self.props
    }

    /// The field's session.
    pub fn combobox(&self) -> &Combobox {
        &self.combobox
    }

    pub fn input(&self) -> &Entity<TextInput> {
        &self.input
    }

    /// The field's text.
    pub fn draft<'a>(&self, cx: &'a App) -> &'a str {
        self.input.read(cx).text()
    }

    /// Whether the field and the remove buttons take input.
    pub fn editing_disabled(&self) -> bool {
        editing_disabled(self.props.disabled, self.props.loading_text.is_some())
    }

    /// The owner's new props: the assigned tags are no longer suggested, a disabled
    /// editor closes its list and gives up the focus.
    pub fn set_props(
        &mut self,
        props: TagEditorProps,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.props = props;
        let disabled = self.editing_disabled();
        let (label, placeholder) = (
            self.props.input_label.clone(),
            self.props.placeholder.clone(),
        );
        self.input.update(cx, |input, cx| {
            input.set_label(label, cx);
            input.set_placeholder(placeholder, cx);
            input.set_disabled(disabled, window, cx);
        });
        let mut effects = self.combobox.set_excluded(self.props.tags.clone());
        effects.extend(self.combobox.set_disabled(disabled));
        self.run(effects, window, cx);
        self.reveal_input(window, cx);
    }

    /// The owner changed the draft (a tag was added and the field cleared); the list is
    /// not opened by it.
    pub fn set_draft(&mut self, draft: &str, window: &mut Window, cx: &mut Context<Self>) {
        if draft == self.input.read(cx).text() {
            return;
        }
        let effects = self.combobox.set_value(draft, draft.len());
        self.input.update(cx, |input, cx| input.set_text(draft, cx));
        self.run(effects, window, cx);
        self.reveal_input(window, cx);
    }

    /// Gives the field the focus (`inputRef` of the owner).
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.input.focus_handle(cx), cx);
    }

    /// Keeps the field in view inside a frame that scrolls (`revealInput`): the field
    /// is the frame's last child. Only a focused field is revealed; an editor without the
    /// focus keeps the frame where the user scrolled it.
    fn reveal_input(&self, window: &Window, cx: &App) {
        if !self.input.focus_handle(cx).is_focused(window) {
            return;
        }
        let before = match editor_content(
            self.props.loading_text.is_some(),
            self.props.tags.len(),
            self.props.empty_text.is_some(),
        ) {
            EditorContent::Tags => self.props.tags.len(),
            EditorContent::Loading | EditorContent::Empty => 1,
            EditorContent::Nothing => 0,
        };
        self.scroll.scroll_to_item(before);
    }

    fn send(&self, input: RuntimeInput, cx: &mut Context<Self>) {
        self.runtime
            .update(cx, |runtime, cx| runtime.send(input, cx));
    }

    fn run(&mut self, effects: Vec<ComboboxEffect>, window: &mut Window, cx: &mut Context<Self>) {
        for effect in effects {
            match effect {
                ComboboxEffect::Suggest(input) => self.send(RuntimeInput::Suggestions(input), cx),
                ComboboxEffect::Pick(tag) => {
                    if !self.editing_disabled() {
                        cx.emit(TagEditorEvent::AddTag(tag));
                    }
                }
                ComboboxEffect::Submit => {
                    if !self.editing_disabled() {
                        let draft = self.input.read(cx).text().to_owned();
                        cx.emit(TagEditorEvent::AddTag(draft));
                    }
                }
                ComboboxEffect::FocusInput => self.focus(window, cx),
                // Picks go to the owner (`pick_to_owner`); nothing replaces a token.
                ComboboxEffect::ReplaceToken { .. } => {}
            }
        }
        cx.notify();
    }

    /// The known tags and the answers to this field.
    fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let editor = self.combobox.editor();
        let events = self
            .runtime
            .update(cx, |runtime, _| runtime.take_suggestion_events_for(editor));
        let vocabulary = self.runtime.read(cx).model().library().known_tags().clone();
        let mut effects = self.combobox.set_vocabulary(&vocabulary);
        for event in events {
            effects.extend(self.combobox.receive(event));
        }
        // The texts are the owner's; it gives new props when the language changes.
        self.run(effects, window, cx);
    }

    fn key(&mut self, key: ComboboxKey, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let (handled, effects) = self.combobox.key(key);
        self.run(effects, window, cx);
        handled
    }

    fn chip(&self, index: usize, tag: &str, disabled: bool, cx: &Context<Self>) -> AnyElement {
        let theme = theme::active(cx);
        let colors = &theme.tokens().colors;
        let remove = self.props.remove_label.as_ref().map(|label| {
            let owned = tag.to_owned();
            let this = cx.entity().downgrade();
            div().debug_selector(move || remove_selector(index)).child(
                Button::icon(("tag-editor-remove", index), IconName::Close, label(tag))
                    .size(layout::REMOVE_SIZE)
                    // `[stroke-width:2.4]` on the chip's cross (`TagEditor.tsx`).
                    .icon_heavy(true)
                    .transparent(true)
                    .disabled(disabled)
                    .on_click(move |_, _, cx| {
                        this.update(cx, |this, cx| {
                            if !this.editing_disabled() {
                                cx.emit(TagEditorEvent::RemoveTag(owned.clone()));
                            }
                        })
                        .ok();
                    }),
            )
        });
        div()
            .id(("tag-editor-chip", index))
            .debug_selector(move || chip_selector(index))
            .flex()
            .flex_none()
            .items_center()
            .max_w_full()
            .gap(px(layout::CHIP_GAP))
            .px(px(layout::CHIP_PADDING_X))
            .py(px(layout::CHIP_PADDING_Y))
            .rounded(px(RADII.control))
            .border_1()
            .border_color(gpui::transparent_black())
            .bg(color(colors.surface_raised))
            .shadow(box_shadows(theme.tokens().shadows.chip))
            .font_family(fonts::MONO)
            .text_size(px(TYPOGRAPHY.xs.size))
            .line_height(px(layout::CHIP_LINE_HEIGHT))
            .font_weight(weight(TYPOGRAPHY.weight_medium))
            .text_color(color(colors.primary_text))
            // A press on a chip is no press on the frame's blank space.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(div().min_w_0().child(tag.to_owned()))
            .children(remove)
            .into_any_element()
    }

    fn suggestion_list(&self, window: &Window, cx: &Context<Self>) -> Option<AnyElement> {
        if !self.combobox.popup() {
            return None;
        }
        let anchor = self.anchor.get()?;
        let viewport = window.viewport_size();
        let placement = list_placement(
            Rect {
                left: f32::from(anchor.origin.x),
                top: f32::from(anchor.origin.y),
                width: f32::from(anchor.size.width),
                height: f32::from(anchor.size.height),
            },
            f32::from(viewport.width),
            f32::from(viewport.height),
            self.props.placement,
        );
        let this = cx.entity().downgrade();
        let body = suggestion_list_body(
            SuggestionListIds {
                label: self.props.list_label.clone(),
                list: LIST_SELECTOR,
                option: "tag-editor-suggestion",
                option_selector,
            },
            self.combobox.items(),
            self.combobox.active(),
            theme::active(cx),
            Rc::new(move |index, window, cx| {
                this.update(cx, |this, cx| {
                    let effects = this.combobox.pick_at(index);
                    this.run(effects, window, cx);
                })
                .ok();
            }),
        )
        .w(px(placement.width))
        .max_h(px(placement.max_height));
        // The top edge of a list below the field, the bottom edge of one above it.
        let position = gpui::point(px(placement.left), px(placement.edge));
        let anchor_corner = match placement.placement {
            SuggestionPlacement::Below => gpui::Anchor::TopLeft,
            SuggestionPlacement::Above => gpui::Anchor::BottomLeft,
        };
        Some(
            deferred(
                gpui::anchored()
                    .position(position)
                    .anchor(anchor_corner)
                    .snap_to_window()
                    .child(body),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }
}

impl Render for TagEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = theme::active(cx);
        let tokens = theme.tokens();
        let visuals = editor_visuals(tokens, self.props.variant);
        let disabled = self.editing_disabled();
        let focused = self.input.focus_handle(cx).is_focused(window);
        let content = editor_content(
            self.props.loading_text.is_some(),
            self.props.tags.len(),
            self.props.empty_text.is_some(),
        );
        let status = |text: SharedString| {
            div()
                .px(px(visuals.status_padding_x))
                .text_size(px(TYPOGRAPHY.xs.size))
                .line_height(px(TYPOGRAPHY.xs.line_height_px()))
                .text_color(color(tokens.colors.text_muted))
                .child(text)
                .into_any_element()
        };
        let mut children: Vec<AnyElement> = match content {
            EditorContent::Loading => self
                .props
                .loading_text
                .clone()
                .map(status)
                .into_iter()
                .collect(),
            EditorContent::Tags => self
                .props
                .tags
                .iter()
                .enumerate()
                .map(|(index, tag)| self.chip(index, tag, disabled, cx))
                .collect(),
            EditorContent::Empty => self
                .props
                .empty_text
                .clone()
                .map(status)
                .into_iter()
                .collect(),
            EditorContent::Nothing => Vec::new(),
        };
        let anchor = self.anchor.clone();
        let measure = canvas(
            move |bounds, _, _| anchor.set(Some(bounds)),
            |_, _, _, _| {},
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full();
        let field = div()
            .debug_selector(|| INPUT_SELECTOR.to_owned())
            .relative()
            .flex_grow_1()
            .flex_shrink_1()
            .flex_basis(px(layout::INPUT_BASIS))
            .max_w_full()
            // The input inherits the font of its surroundings (`font: inherit`); only
            // the chips are monospaced.
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
            .child(measure)
            .children(self.suggestion_list(window, cx))
            .into_any_element();
        children.push(field);
        let variant = self.props.variant;
        div()
            .id("tag-editor")
            .debug_selector(move || variant.test_id().to_owned())
            .track_scroll(&self.scroll)
            .overflow_y_scroll()
            .flex()
            .flex_wrap()
            .items_start()
            .gap(px(layout::GAP))
            .p(px(layout::PADDING))
            .min_h(px(visuals.min_height))
            .max_h(px(visuals.max_height))
            .rounded(px(RADII.control))
            .border_1()
            .border_color(color(if focused {
                tokens.colors.primary
            } else {
                visuals.border
            }))
            .bg(color(visuals.background))
            .when(focused, |frame| {
                frame.shadow(box_shadows(tokens.shadows.field_focus))
            })
            // A press on the frame's blank space focuses the field; one on the field
            // or a button focused that already (`prevent_default`).
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    if window.default_prevented() || this.editing_disabled() {
                        return;
                    }
                    this.focus(window, cx);
                    // The window's root would take the focus back otherwise.
                    window.prevent_default();
                }),
            )
            .children(children)
    }
}
