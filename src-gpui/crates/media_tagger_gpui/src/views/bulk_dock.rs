//! The bulk dock (`BulkDock.tsx`, `GroupOrderBoard.tsx`, `GroupOrderModal.tsx`): the
//! floating panel at the bottom centre of the gallery in selection mode (ADR 0035).
//!
//! With nothing selected it is a small chip. With a selection it shows the tag field,
//! the favorite heart and the counter that clears the selection in one row, the
//! feedback of those controls under it, and two tabs: Tags (the tag-mode description)
//! and Group & order (the group key, Apply and the inline order board).
//!
//! The view only draws and forwards. What is written, and what a write published, is
//! decided by the runtime's bulk model
//! ([`Bulk`](media_tagger_client::bulk::Bulk)); the tab, the drafts and the larger
//! order view are the [`Dock`] state of the client crate, which this view owns and
//! loses with the gallery view, as the Solid component loses its signals. The order
//! boards (`board.rs`) draw only the rows near their visible part and keep the focus
//! on a grip while its tile moves.

mod board;

use std::{cell::Cell, rc::Rc};

use gpui::{
    actions, canvas, div, prelude::*, px, AnyElement, App, Context, Entity, FocusHandle, Focusable,
    KeyBinding, MouseButton, Pixels, Role, SharedString, Size, Subscription, Window,
};
use media_tagger_client::{
    bulk::{
        layout::{dock as dock_layout, group, header, order_modal, tabs},
        BulkCommand, BulkView, Dock, DockEffect, DockTab, GroupSaveResult, TabKey, TagMode,
    },
    controls::{AlertTone, ButtonVariant, FocusRing},
    gallery::TileRequest,
    i18n::{I18n, Language, Params},
    icons::IconName,
    overlay::ModalSize,
    runtime::RuntimeInput,
    scenario::ScenarioDock,
    tagging::{SuggestionPlacement, TagEditorVariant},
    text::FieldChrome,
    theme::{ThemeName, RADII, TYPOGRAPHY},
};

use self::board::{BoardKind, BoardState};
use super::{
    gallery::{LineDown, LineUp, PageDown, PageUp, ScrollEnd, ScrollStart},
    tag_editor::{TagEditor, TagEditorEvent, TagEditorProps},
};
use crate::{
    runtime::Runtime,
    thumbs::Thumbnails,
    ui::{
        alert::Alert,
        button::{keyed_focus_handle, Button},
        fonts,
        icon::Icon,
        modal::{Modal, ModalEvent, ModalState},
        text_input::{TextInput, TextInputEvent},
        theme::{self, box_shadows, color, focus_ring, weight},
    },
};

actions!(
    bulk_dock,
    [
        /// Arrow right on a tab.
        TabNext,
        /// Arrow left on a tab.
        TabPrevious,
        /// Home on a tab.
        TabFirst,
        /// End on a tab.
        TabLast,
        /// The arrow keys on the grip of an order tile.
        MoveLeft,
        MoveRight,
        MoveUp,
        MoveDown,
    ]
);

/// The key context of a tab button and of the grip of an order tile.
pub const TAB_KEY_CONTEXT: &str = "BulkDockTab";
pub const HANDLE_KEY_CONTEXT: &str = "GroupOrderHandle";

/// Debug selectors and element ids (the `data-testid`s and ids of the Solid dock).
pub const PANEL_SELECTOR: &str = "bulk-action-panel";
pub const CHIP_SELECTOR: &str = "bulk-dock-chip";
pub const HEADER_SELECTOR: &str = "bulk-header-panel";
pub const NOTICE_SELECTOR: &str = "bulk-selection-cleared";
pub const TAG_INVALID_SELECTOR: &str = "bulk-tag-invalid";
pub const PARTIAL_SELECTOR: &str = "bulk-partial-result";
pub const FAVORITE_ID: &str = "bulk-favorite";
pub const CLEAR_ID: &str = "bulk-clear-selection";
pub const RETRY_TAGS_ID: &str = "bulk-retry-tag-details";
pub const TAGS_PANEL_SELECTOR: &str = "bulk-tags-panel";
pub const GROUP_PANEL_SELECTOR: &str = "bulk-group-panel";
pub const GROUP_KEY_SELECTOR: &str = "bulk-group-key-input";
pub const GENERATE_KEY_ID: &str = "bulk-generate-group-key";
pub const APPLY_GROUP_ID: &str = "bulk-apply-group";
pub const RETRY_METADATA_ID: &str = "bulk-retry-metadata";
pub const ORDER_PANEL_SELECTOR: &str = "bulk-group-order-panel";
pub const OPEN_ORDER_MODAL_ID: &str = "bulk-open-order-modal";
pub const ORDER_MODAL_ID: &str = "bulk-order-modal";
pub const ORDER_MODAL_CANCEL_ID: &str = "bulk-order-cancel";
pub const ORDER_MODAL_SAVE_ID: &str = "bulk-order-save";
pub const ORDER_MODAL_RESULT_SELECTOR: &str = "bulk-order-result";
pub const FAVORITE_FAILED_SELECTOR: &str = "bulk-favorite-failed";
pub const TAG_LOAD_FAILED_SELECTOR: &str = "bulk-tag-load-failed";
pub const TAG_SAVE_FAILED_SELECTOR: &str = "bulk-tag-save-failed";
pub const GROUP_CONFLICT_SELECTOR: &str = "bulk-group-conflict";
pub const GROUP_FAILED_SELECTOR: &str = "bulk-group-failed";

pub use board::{grid_selector, handle_selector, insertion_line_selector, tile_selector};

/// The arrow, Home and End keys of the tablist and the arrow keys of an order grip.
/// Both contexts lie inside the gallery's, whose own bindings of these keys scroll the
/// grid; the deeper context wins.
pub fn key_bindings() -> Vec<KeyBinding> {
    let tab = Some(TAB_KEY_CONTEXT);
    let handle = Some(HANDLE_KEY_CONTEXT);
    vec![
        KeyBinding::new("right", TabNext, tab),
        KeyBinding::new("left", TabPrevious, tab),
        KeyBinding::new("home", TabFirst, tab),
        KeyBinding::new("end", TabLast, tab),
        KeyBinding::new("left", MoveLeft, handle),
        KeyBinding::new("right", MoveRight, handle),
        KeyBinding::new("up", MoveUp, handle),
        KeyBinding::new("down", MoveDown, handle),
    ]
}

/// Binds the dock's keys. Part of `views::init`.
pub fn init(cx: &mut App) {
    cx.bind_keys(key_bindings());
}

/// What the tag editor was last given: its props are rebuilt only when one of these
/// changed.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct EditorStamp {
    tags: Vec<String>,
    loading: bool,
    empty: bool,
    disabled: bool,
    removable: bool,
    language: Option<Language>,
}

pub struct BulkDock {
    runtime: Entity<Runtime>,
    thumbnails: Entity<Thumbnails>,
    dock: Dock,
    editor: Entity<TagEditor>,
    editor_stamp: EditorStamp,
    /// The tag draft the editor's field was last known to show.
    seen_draft: String,
    group_key: Entity<TextInput>,
    modal: Entity<ModalState>,
    /// The key and the order the larger view showed while it was open: its closing
    /// frame keeps them.
    modal_frame: (String, Vec<i64>),
    tab_focus: [FocusHandle; 2],
    /// "Sort in larger view": the focus goes back to it when the larger view closes.
    open_modal_focus: FocusHandle,
    inline: BoardState,
    larger: BoardState,
    /// The order tiles the frame being built draws: their thumbnails are asked for.
    board_tiles: Vec<TileRequest>,
    /// The size of the gallery's viewport, which the dock's own size follows.
    viewport: Option<Size<Pixels>>,
    language: Language,
    /// The dock steps of a comparison scenario that are still to be made.
    scripted: Option<ScenarioDock>,
    _subscriptions: Vec<Subscription>,
}

impl BulkDock {
    pub fn new(runtime: Entity<Runtime>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (i18n, language, thumbnails) = {
            let runtime = runtime.read(cx);
            (
                runtime.i18n(),
                runtime.model().language(),
                runtime.thumbnails().clone(),
            )
        };
        let editor = cx.new(|cx| {
            TagEditor::new(
                runtime.clone(),
                TagEditorProps {
                    variant: TagEditorVariant::Bulk,
                    placement: SuggestionPlacement::Above,
                    disabled: true,
                    ..TagEditorProps::default()
                },
                window,
                cx,
            )
        });
        let group_key = cx.new(|cx| {
            let mut input = TextInput::themed(window, cx);
            input.set_chrome(FieldChrome::Boxed, cx);
            input.set_height(Some(group::INPUT_HEIGHT), cx);
            input.set_placeholder(i18n.t("bulk.groupModal.groupKeyPlaceholder"), cx);
            input.set_label(i18n.t("bulk.groupModal.groupKeyAria"), cx);
            input
        });
        let open_modal_focus = cx.focus_handle().tab_stop(true);
        let modal = cx.new(|cx| {
            let mut modal = ModalState::new(cx);
            modal.set_restore_focus(Some(open_modal_focus.clone()));
            modal
        });
        let subscriptions = vec![
            cx.subscribe_in(&editor, window, |this, _, event, window, cx| {
                this.editor_event(event, window, cx)
            }),
            cx.subscribe_in(&group_key, window, |this, input, event, window, cx| {
                if *event != TextInputEvent::Changed {
                    return;
                }
                let text = input.read(cx).text().to_owned();
                let effects = this.with_view(cx, |dock, view| {
                    (text != view.group_key_draft()).then(|| dock.set_group_key(text))
                });
                this.run(effects.unwrap_or_default(), window, cx);
            }),
            cx.subscribe_in(&modal, window, |this, _, event, window, cx| {
                if *event == ModalEvent::Dismiss {
                    this.with_view(cx, |dock, view| dock.close_order_modal(view));
                    this.sync(window, cx);
                }
            }),
            cx.observe_in(&runtime, window, |this, _, window, cx| {
                this.sync(window, cx)
            }),
            // A picture of an order tile arrived.
            cx.observe(&thumbnails, |_, _, cx| cx.notify()),
            cx.observe_window_activation(window, |this, window, cx| {
                if !window.is_window_active() {
                    this.cancel_drags(cx);
                }
            }),
        ];
        // The boards go with the dock: their thumbnails are no longer asked for.
        cx.on_release(|this: &mut Self, cx| {
            this.thumbnails
                .update(cx, |thumbnails, _| thumbnails.sync_board(Vec::new()));
        })
        .detach();
        let mut dock = Self {
            runtime,
            thumbnails,
            dock: Dock::new(),
            editor,
            editor_stamp: EditorStamp::default(),
            seen_draft: String::new(),
            group_key,
            modal,
            modal_frame: (String::new(), Vec::new()),
            tab_focus: [
                cx.focus_handle().tab_stop(true),
                cx.focus_handle().tab_stop(false),
            ],
            open_modal_focus,
            inline: BoardState::new(),
            larger: BoardState::new(),
            board_tiles: Vec::new(),
            viewport: None,
            language,
            scripted: None,
            _subscriptions: subscriptions,
        };
        dock.sync(window, cx);
        dock
    }

    /// The dock's own state: the tab, the drafts, the larger view.
    pub fn dock(&self) -> &Dock {
        &self.dock
    }

    /// The tag editor of the header row.
    pub fn editor(&self) -> &Entity<TagEditor> {
        &self.editor
    }

    /// The group key field of the group tab.
    pub fn group_key_input(&self) -> &Entity<TextInput> {
        &self.group_key
    }

    /// The state of the larger order view's dialog.
    pub fn order_modal(&self) -> &Entity<ModalState> {
        &self.modal
    }

    /// The focus handle of a tab button.
    pub fn tab_focus(&self, tab: DockTab) -> &FocusHandle {
        &self.tab_focus[tab.index()]
    }

    /// The focus handle of "Sort in larger view", which gets the focus back when the
    /// larger view closes.
    pub fn open_order_modal_focus(&self) -> &FocusHandle {
        &self.open_modal_focus
    }

    /// The focus handle of the grip of `asset_id` in the inline board (`larger`: in the
    /// larger view), once its tile was drawn.
    pub fn handle_focus(&self, larger: bool, asset_id: i64) -> Option<&FocusHandle> {
        let state = if larger { &self.larger } else { &self.inline };
        state.handles.get(&asset_id)
    }

    /// The gallery's viewport was laid out at `size`.
    pub fn set_viewport(&mut self, size: Size<Pixels>, cx: &mut Context<Self>) {
        if self.viewport != Some(size) {
            self.viewport = Some(size);
            cx.notify();
        }
    }

    fn send(&self, input: RuntimeInput, cx: &mut Context<Self>) {
        self.runtime
            .update(cx, |runtime, cx| runtime.send(input, cx));
    }

    /// Runs `f` with the dock's state and what the bulk model shows now.
    fn with_view<R>(&mut self, cx: &App, f: impl FnOnce(&mut Dock, &BulkView<'_>) -> R) -> R {
        let runtime = self.runtime.read(cx);
        let view = runtime.model().bulk();
        f(&mut self.dock, &view)
    }

    fn run(&mut self, effects: Vec<DockEffect>, window: &mut Window, cx: &mut Context<Self>) {
        for effect in effects {
            match effect {
                DockEffect::Bulk(command) => self.send(RuntimeInput::bulk(command), cx),
                DockEffect::Selection(input) => self.send(RuntimeInput::Selection(input), cx),
                // A comparison scenario shows what a capture of the Tauri window shows,
                // and that has no focus ring and no caret.
                DockEffect::FocusTagInput if self.scripted.is_some() => {}
                DockEffect::FocusTagInput => {
                    // One frame later, when the field is drawn enabled again; not at
                    // all when the selection changed or the field is disabled by then.
                    let key = self.runtime.read(cx).model().selection().key();
                    cx.on_next_frame(window, move |this, window, cx| {
                        let due = this.with_view(cx, |_, view| {
                            view.selection_key() == key && !Dock::tag_controls_disabled(view)
                        });
                        if due {
                            let focus = this.editor.focus_handle(cx);
                            window.focus(&focus, cx);
                        }
                    });
                }
                DockEffect::FocusTab(tab) => window.focus(&self.tab_focus[tab.index()], cx),
            }
        }
        cx.notify();
    }

    fn editor_event(
        &mut self,
        event: &TagEditorEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let effects = match event {
            TagEditorEvent::DraftChanged(text) => {
                self.seen_draft.clone_from(text);
                self.dock.set_tag_draft(text.clone());
                Vec::new()
            }
            TagEditorEvent::AddTag(tag) => self.with_view(cx, |dock, view| dock.add_tag(tag, view)),
            TagEditorEvent::RemoveTag(tag) => self.dock.remove_tag(tag),
        };
        self.run(effects, window, cx);
        self.apply(window, cx);
    }

    /// Brings the dock in line with the runtime: the model's answers, then what the
    /// child entities show.
    fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let events = self
            .runtime
            .update(cx, |runtime, _| runtime.take_bulk_events());
        let mut effects = self.with_view(cx, |dock, view| dock.sync(view, &events));
        effects.extend(self.scripted_steps(cx));
        self.apply(window, cx);
        self.run(effects, window, cx);
    }

    /// The dock steps of a comparison scenario, made as its Tauri steps make them: the
    /// tag, the tab and the group key at once, the larger view once the inline board
    /// is there.
    fn scripted_steps(&mut self, cx: &mut Context<Self>) -> Vec<DockEffect> {
        let mut effects = Vec::new();
        if self.scripted.is_none() {
            let Some(script) = self
                .runtime
                .update(cx, |runtime, _| runtime.take_scripted_dock())
            else {
                return effects;
            };
            self.scripted = Some(script);
            if let Some(tag) = script.tag {
                effects.extend(self.with_view(cx, |dock, view| dock.add_tag(tag, view)));
            }
            self.dock.select_tab(script.tab);
            if let Some(key) = script.group_key {
                effects.extend(self.dock.set_group_key(key.to_owned()));
            }
        }
        match self.scripted {
            Some(script) if script.order_modal => {
                let opened = self.with_view(cx, |dock, view| {
                    let ready =
                        Dock::has_order_panel(view) && !Dock::open_order_modal_disabled(view);
                    if ready {
                        dock.open_order_modal(view);
                    }
                    ready
                });
                if opened {
                    self.scripted = Some(ScenarioDock {
                        order_modal: false,
                        ..script
                    });
                    // Opening changed what the dock derives: look again.
                    effects.extend(self.with_view(cx, |dock, view| dock.sync(view, &[])));
                }
            }
            _ => {}
        }
        effects
    }

    /// Gives the tag editor, the group key field and the larger view's dialog what the
    /// model and the dock say now.
    fn apply(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (i18n, language) = {
            let runtime = self.runtime.read(cx);
            (runtime.i18n(), runtime.model().language())
        };
        let language_changed = std::mem::replace(&mut self.language, language) != language;

        let (stamp, group_draft, group_disabled, modal_open, modal_blocked) =
            self.with_view(cx, |dock, view| {
                let mode = view.tag_mode();
                (
                    EditorStamp {
                        tags: Dock::displayed_tags(view).to_vec(),
                        loading: view.tag_details_loading(),
                        empty: mode == TagMode::None,
                        disabled: Dock::tag_controls_disabled(view),
                        removable: mode == TagMode::Single,
                        language: Some(language),
                    },
                    view.group_key_draft().to_owned(),
                    Dock::group_controls_disabled(view),
                    dock.order_modal_open(view),
                    dock.modal_locked(view),
                )
            });

        if stamp != self.editor_stamp {
            let remove_i18n = i18n;
            let props = TagEditorProps {
                variant: TagEditorVariant::Bulk,
                tags: stamp.tags.clone(),
                empty_text: stamp
                    .empty
                    .then(|| i18n.t("bulk.panel.selectItemsPrompt").into()),
                loading_text: stamp
                    .loading
                    .then(|| i18n.t("bulk.panel.loadingTags").into()),
                disabled: stamp.disabled,
                input_label: i18n.t("bulk.tagModal.addTag").into(),
                placeholder: i18n.t("bulk.tagModal.tagInputPlaceholder").into(),
                remove_label: stamp.removable.then(|| {
                    Rc::new(move |tag: &str| {
                        SharedString::from(remove_i18n.t_with(
                            "bulk.tagModal.removeTagAria",
                            &Params::new().with("tag", tag),
                        ))
                    }) as super::tag_editor::RemoveLabel
                }),
                placement: SuggestionPlacement::Above,
            };
            self.editor_stamp = stamp;
            self.editor
                .update(cx, |editor, cx| editor.set_props(props, window, cx));
        }
        if self.dock.tag_draft() != self.seen_draft {
            self.seen_draft = self.dock.tag_draft().to_owned();
            let draft = self.seen_draft.clone();
            self.editor
                .update(cx, |editor, cx| editor.set_draft(&draft, window, cx));
        }

        self.group_key.update(cx, |input, cx| {
            if input.text() != group_draft {
                input.set_text(&group_draft, cx);
            }
            input.set_disabled(group_disabled, window, cx);
            if language_changed {
                input.set_placeholder(i18n.t("bulk.groupModal.groupKeyPlaceholder"), cx);
                input.set_label(i18n.t("bulk.groupModal.groupKeyAria"), cx);
            }
        });

        if modal_open {
            self.modal_frame = (group_draft, self.dock.modal_order().to_vec());
        }
        self.modal.update(cx, |modal, cx| {
            modal.set_close_on_escape(!modal_blocked, cx);
            modal.set_close_on_overlay_click(!modal_blocked);
            if modal.is_open() != modal_open {
                modal.set_open(modal_open, window, cx);
            }
        });
        if !modal_open {
            self.larger.drag.cancel();
        }
        cx.notify();
    }

    fn command(&mut self, command: BulkCommand, cx: &mut Context<Self>) {
        self.send(RuntimeInput::bulk(command), cx);
    }

    fn select_tab(&mut self, tab: DockTab, window: &mut Window, cx: &mut Context<Self>) {
        self.dock.select_tab(tab);
        self.sync(window, cx);
    }

    fn tab_key(&mut self, from: DockTab, key: TabKey, window: &mut Window, cx: &mut Context<Self>) {
        let effects = self.dock.tab_key(from, key);
        self.run(effects, window, cx);
        self.sync(window, cx);
    }

    fn alert(
        id: &'static str,
        tone: AlertTone,
        title: Option<String>,
        message: String,
        theme: ThemeName,
    ) -> Alert {
        let alert = Alert::new(id, message).tone(tone).theme(theme);
        match title {
            Some(title) => alert.title(title),
            None => alert,
        }
    }

    /// The collapsed dock: a chip that says what to do.
    fn chip(&self, i18n: &I18n<'_>, theme: ThemeName) -> gpui::Div {
        let colors = &theme.tokens().colors;
        div()
            .debug_selector(|| CHIP_SELECTOR.to_owned())
            .flex()
            .items_center()
            .justify_center()
            .gap(px(dock_layout::CHIP_GAP))
            .min_h(px(dock_layout::CHIP_MIN_HEIGHT))
            .text_size(px(dock_layout::CHIP_FONT_SIZE))
            .text_color(color(colors.text_muted))
            .child(
                Icon::new(IconName::BulkActions)
                    .size(dock_layout::CHIP_ICON_SIZE)
                    .color(color(colors.text_muted)),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .child(i18n.t("bulk.dock.collapsedHint")),
            )
    }

    /// The counter that clears the selection (`.bulk-dock-count`).
    fn counter(
        &self,
        count: usize,
        i18n: &I18n<'_>,
        theme: ThemeName,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = &theme.tokens().colors;
        let label = SharedString::from(i18n.t_with(
            "bulk.dock.clearSelectionAria",
            &Params::new().with_count(count as i64),
        ));
        let id = gpui::ElementId::from(CLEAR_ID);
        let focus = keyed_focus_handle(&id, window, cx);
        let focus_visible = focus.is_focused(window) && window.last_input_was_keyboard();
        let disabled = count == 0;
        let (error, error_content) = (colors.error, colors.error_content);
        let button = div()
            .id(id)
            .debug_selector(|| CLEAR_ID.to_owned())
            .role(Role::Button)
            .aria_label(label)
            .relative()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .gap(px(header::COUNT_GAP))
            .h(px(header::COUNT_HEIGHT))
            .min_w(px(header::COUNT_MIN_WIDTH))
            .px(px(header::COUNT_PADDING_X))
            .rounded(px(RADII.control))
            .border_1()
            .border_color(color(colors.border_soft))
            .bg(color(colors.surface_raised))
            .text_color(color(colors.primary_text))
            .font_family(fonts::MONO)
            .text_size(px(header::COUNT_FONT_SIZE))
            .font_weight(weight(TYPOGRAPHY.weight_semibold))
            .child(count.to_string())
            .child(
                Icon::new(IconName::Close)
                    .size(header::COUNT_ICON_SIZE)
                    .color(color(colors.primary_text)),
            );
        if disabled {
            return button
                .opacity(header::COUNT_DISABLED_OPACITY)
                .cursor_not_allowed()
                .into_any_element();
        }
        button
            .track_focus(&focus)
            .cursor_pointer()
            .hover(move |style| {
                style
                    .bg(color(error))
                    .text_color(color(error_content))
                    .border_color(gpui::transparent_black())
            })
            .when(focus_visible, |button| {
                button.child(focus_ring(FocusRing::base(theme.tokens()), RADII.control))
            })
            .on_click(cx.listener(|this, _, window, cx| {
                let effects = this.dock.clear_selection();
                this.run(effects, window, cx);
            }))
            .into_any_element()
    }

    /// The tablist: the selected tab is its one tab stop.
    fn tablist(
        &mut self,
        i18n: &I18n<'_>,
        theme: ThemeName,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = &theme.tokens().colors;
        let selected = self.dock.tab();
        for tab in DockTab::ALL {
            let handle = &mut self.tab_focus[tab.index()];
            *handle = handle.clone().tab_stop(tab == selected);
        }
        let keyboard = window.last_input_was_keyboard();
        let buttons = DockTab::ALL.map(|tab| {
            let is_selected = tab == selected;
            let focus = &self.tab_focus[tab.index()];
            let focus_visible = keyboard && focus.is_focused(window);
            let base_content = colors.base_content;
            div()
                .id(tab.test_id())
                .debug_selector(move || tab.test_id().to_owned())
                .role(Role::Tab)
                .aria_selected(is_selected)
                .key_context(TAB_KEY_CONTEXT)
                .track_focus(focus)
                .relative()
                .flex_none()
                .px(px(tabs::PADDING_X))
                .py(px(tabs::PADDING_Y))
                .border_b_2()
                .border_color(if is_selected {
                    color(colors.primary)
                } else {
                    gpui::transparent_black()
                })
                .text_color(color(if is_selected {
                    colors.primary_text
                } else {
                    colors.text_muted
                }))
                .text_size(px(tabs::FONT_SIZE))
                .cursor_pointer()
                .when(!is_selected, |button| {
                    button.hover(move |style| style.text_color(color(base_content)))
                })
                .when(focus_visible, |button| {
                    button.child(focus_ring(FocusRing::base(theme.tokens()), RADII.field))
                })
                .on_click(cx.listener(move |this, _, window, cx| this.select_tab(tab, window, cx)))
                .on_action(cx.listener(move |this, _: &TabNext, window, cx| {
                    this.tab_key(tab, TabKey::Right, window, cx)
                }))
                .on_action(cx.listener(move |this, _: &TabPrevious, window, cx| {
                    this.tab_key(tab, TabKey::Left, window, cx)
                }))
                .on_action(cx.listener(move |this, _: &TabFirst, window, cx| {
                    this.tab_key(tab, TabKey::Home, window, cx)
                }))
                .on_action(cx.listener(move |this, _: &TabLast, window, cx| {
                    this.tab_key(tab, TabKey::End, window, cx)
                }))
                .child(i18n.t(tab.label_key()))
        });
        div()
            .id("bulk-dock-tabs")
            .role(Role::TabList)
            .aria_label(SharedString::from(i18n.t("bulk.panel.ariaLabel")))
            .relative()
            .flex()
            .flex_none()
            .gap(px(tabs::GAP))
            // The tablist's own line; the selected tab's line lies on it.
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .h(px(tabs::RULE))
                    .bg(color(colors.border_soft)),
            )
            .children(buttons)
            .into_any_element()
    }

    /// The feedback of the header's controls, shown on either tab.
    fn feedback(
        &self,
        i18n: &I18n<'_>,
        theme: ThemeName,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let colors = &theme.tokens().colors;
        let (favorite_failed, details_failed, tag_failed, partial) = {
            let runtime = self.runtime.read(cx);
            let view = runtime.model().bulk();
            (
                view.favorite_failed(),
                view.tag_details_failed(),
                view.tag_save_failed(),
                view.partial_result().filter(|partial| partial.is_partial()),
            )
        };
        let save_failed = || Some(i18n.t("bulk.panel.saveFailedTitle"));
        let mut feedback: Vec<AnyElement> = Vec::new();
        if favorite_failed {
            feedback.push(
                Self::alert(
                    FAVORITE_FAILED_SELECTOR,
                    AlertTone::Error,
                    save_failed(),
                    i18n.t("bulk.favorite.failed"),
                    theme,
                )
                .debug_selector(FAVORITE_FAILED_SELECTOR)
                .into_any_element(),
            );
        }
        if self.dock.tag_invalid() {
            feedback.push(
                Self::alert(
                    TAG_INVALID_SELECTOR,
                    AlertTone::Error,
                    None,
                    i18n.t("search.errors.tagInvalidCharacters"),
                    theme,
                )
                .debug_selector(TAG_INVALID_SELECTOR)
                .into_any_element(),
            );
        }
        if details_failed {
            feedback.push(
                Self::alert(
                    TAG_LOAD_FAILED_SELECTOR,
                    AlertTone::Error,
                    Some(i18n.t("bulk.panel.tagLoadFailedTitle")),
                    i18n.t("bulk.panel.tagLoadFailed"),
                    theme,
                )
                .debug_selector(TAG_LOAD_FAILED_SELECTOR)
                .child(
                    Button::new(RETRY_TAGS_ID, i18n.t("bulk.panel.retryTagDetails"))
                        .debug_selector(RETRY_TAGS_ID)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.command(BulkCommand::RetryTagDetails, cx)
                        })),
                )
                .into_any_element(),
            );
        }
        if tag_failed {
            feedback.push(
                Self::alert(
                    TAG_SAVE_FAILED_SELECTOR,
                    AlertTone::Error,
                    save_failed(),
                    i18n.t("bulk.panel.tagSaveFailed"),
                    theme,
                )
                .debug_selector(TAG_SAVE_FAILED_SELECTOR)
                .into_any_element(),
            );
        }
        if let Some(partial) = partial {
            feedback.push(
                div()
                    .id(PARTIAL_SELECTOR)
                    .debug_selector(|| PARTIAL_SELECTOR.to_owned())
                    .role(Role::Status)
                    .text_color(color(colors.base_content))
                    .child(
                        i18n.t_with(
                            "bulk.panel.partialResult",
                            &Params::new()
                                .with("processed", partial.processed)
                                .with("requested", partial.requested),
                        ),
                    )
                    .into_any_element(),
            );
        }
        feedback
    }

    /// The group tab: the key row, its feedback and the inline order board.
    fn group_panel(
        &mut self,
        i18n: &I18n<'_>,
        theme: ThemeName,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = &theme.tokens().colors;
        struct Shown {
            controls_disabled: bool,
            apply_disabled: bool,
            apply_label: &'static str,
            apply_variant: ButtonVariant,
            conflicting: bool,
            group_failed: bool,
            metadata_loading: bool,
            metadata_failed: bool,
            order_panel: bool,
            modal_open: bool,
            open_disabled: bool,
            order_locked: bool,
        }
        let shown = self.with_view(cx, |dock, view| Shown {
            controls_disabled: Dock::group_controls_disabled(view),
            apply_disabled: Dock::apply_group_disabled(view),
            apply_label: Dock::apply_group_label_key(view),
            apply_variant: Dock::apply_group_variant(view),
            conflicting: view.has_conflicting_groups(),
            group_failed: view.group_failed(),
            metadata_loading: view.metadata_loading(),
            metadata_failed: view.metadata_failed(),
            order_panel: Dock::has_order_panel(view),
            modal_open: dock.order_modal_open(view),
            open_disabled: Dock::open_order_modal_disabled(view),
            order_locked: Dock::order_locked(view),
        });

        let key_row = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(group::ROW_GAP))
            .child(
                div()
                    .debug_selector(|| GROUP_KEY_SELECTOR.to_owned())
                    .flex_1()
                    .min_w_0()
                    .child(self.group_key.clone()),
            )
            // The button's fixed track of the Solid grid; the button sits at its start.
            .child(
                div()
                    .flex()
                    .flex_none()
                    .w(px(group::KEY_BUTTON_TRACK))
                    .child(
                        Button::icon(
                            GENERATE_KEY_ID,
                            IconName::Reset,
                            i18n.t("bulk.groupModal.generateGroupAria"),
                        )
                        .debug_selector(GENERATE_KEY_ID)
                        .disabled(shown.controls_disabled)
                        .on_click(cx.listener(|this, _, window, cx| {
                            let effects = this.dock.generate_group_key();
                            this.run(effects, window, cx);
                        })),
                    ),
            )
            .child(
                Button::new(APPLY_GROUP_ID, i18n.t(shown.apply_label))
                    .variant(shown.apply_variant)
                    .debug_selector(APPLY_GROUP_ID)
                    .disabled(shown.apply_disabled)
                    .on_click(cx.listener(|this, _, window, cx| {
                        let effects = this.with_view(cx, |dock, view| dock.apply_group(view));
                        this.run(effects, window, cx);
                    })),
            );

        let mut panel = div()
            .id(GROUP_PANEL_SELECTOR)
            .debug_selector(|| GROUP_PANEL_SELECTOR.to_owned())
            .role(Role::TabPanel)
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap(px(group::GAP))
            .child(key_row);
        if shown.conflicting {
            panel = panel.child(
                Self::alert(
                    GROUP_CONFLICT_SELECTOR,
                    AlertTone::Warning,
                    Some(i18n.t("bulk.panel.groupConflictTitle")),
                    i18n.t("bulk.panel.groupConflictDescription"),
                    theme,
                )
                .debug_selector(GROUP_CONFLICT_SELECTOR),
            );
        }
        if shown.group_failed {
            panel = panel.child(
                Self::alert(
                    GROUP_FAILED_SELECTOR,
                    AlertTone::Error,
                    Some(i18n.t("bulk.panel.saveFailedTitle")),
                    i18n.t("bulk.panel.groupSaveFailed"),
                    theme,
                )
                .debug_selector(GROUP_FAILED_SELECTOR),
            );
        }
        if shown.metadata_loading {
            panel = panel.child(
                div()
                    .id("bulk-metadata-loading")
                    .role(Role::Status)
                    .flex_none()
                    .child(i18n.t("common.loading")),
            );
        }
        if shown.metadata_failed {
            panel = panel.child(
                Self::alert(
                    "bulk-metadata-failed",
                    AlertTone::Error,
                    Some(i18n.t("bulk.panel.metadataFailed")),
                    String::new(),
                    theme,
                )
                .child(
                    Button::new(RETRY_METADATA_ID, i18n.t("gallery.retry"))
                        .debug_selector(RETRY_METADATA_ID)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.command(BulkCommand::RetryMetadata, cx)
                        })),
                ),
            );
        }
        if !shown.order_panel {
            self.inline.drag.cancel();
            return panel.into_any_element();
        }

        let heading = div()
            .flex()
            .flex_none()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap(px(group::ROW_GAP))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_size(px(TYPOGRAPHY.sm.size))
                            .line_height(px(TYPOGRAPHY.sm.line_height_px()))
                            .font_weight(weight(TYPOGRAPHY.heading_weight))
                            .child(i18n.t("bulk.panel.orderHeading")),
                    )
                    .child(
                        div()
                            .text_size(px(TYPOGRAPHY.xs.size))
                            .line_height(px(TYPOGRAPHY.xs.line_height_px()))
                            .text_color(color(colors.text_muted))
                            .child(i18n.t("bulk.panel.orderDescription")),
                    ),
            )
            .child(
                Button::new(OPEN_ORDER_MODAL_ID, i18n.t("bulk.orderModal.open"))
                    .text_size(TYPOGRAPHY.xs.size)
                    .track_focus(&self.open_modal_focus)
                    .debug_selector(OPEN_ORDER_MODAL_ID)
                    .disabled(shown.open_disabled)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.with_view(cx, |dock, view| dock.open_order_modal(view));
                        this.sync(window, cx);
                    })),
            );
        // The larger view and the inline board are never shown together.
        let board = if shown.modal_open {
            self.inline.drag.cancel();
            div().flex_1().min_h_0().into_any_element()
        } else {
            let order = self.dock.inline_order().to_vec();
            self.board(
                BoardKind::Inline,
                &order,
                shown.order_locked,
                i18n,
                theme,
                window,
                cx,
            )
        };
        panel
            .child(
                div()
                    .debug_selector(|| ORDER_PANEL_SELECTOR.to_owned())
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h(px(group::ORDER_PANEL_MIN_HEIGHT))
                    .gap(px(group::ROW_GAP))
                    .child(heading)
                    .child(board),
            )
            .into_any_element()
    }

    /// The content of the larger order view (`GroupOrderModal`).
    fn order_modal_content(
        &mut self,
        i18n: &I18n<'_>,
        theme: ThemeName,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = &theme.tokens().colors;
        let open = self.modal.read(cx).is_open();
        let (blocked, save_disabled) = self.with_view(cx, |dock, view| {
            (dock.modal_locked(view), dock.modal_save_disabled(view))
        });
        let saving = self.dock.modal_saving();
        let result = self.dock.modal_result();
        // The closing frame keeps the key and the order it had while open.
        let (key, order) = self.modal_frame.clone();
        let summary = format!(
            "{key} · {}",
            i18n.t_with(
                "bulk.panel.selectedItems",
                &Params::new().with_count(order.len() as i64)
            )
        );
        let heading = div()
            .flex()
            .flex_col()
            .flex_none()
            .child(
                div()
                    .text_size(px(TYPOGRAPHY.lg.size))
                    .line_height(px(TYPOGRAPHY.lg.line_height_px()))
                    .font_weight(weight(TYPOGRAPHY.weight_semibold))
                    .child(i18n.t("bulk.orderModal.title")),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(px(TYPOGRAPHY.sm.size))
                    .line_height(px(TYPOGRAPHY.sm.line_height_px()))
                    .text_color(color(colors.text_muted))
                    .child(summary),
            )
            .child(
                div()
                    .mt(px(4.0))
                    .text_size(px(TYPOGRAPHY.xs.size))
                    .line_height(px(TYPOGRAPHY.xs.line_height_px()))
                    .text_color(color(colors.text_muted))
                    .child(i18n.t("bulk.orderModal.instructions")),
            );
        let board = self.board(
            BoardKind::Larger,
            &order,
            blocked || !open,
            i18n,
            theme,
            window,
            cx,
        );
        let outcome = match result {
            Some(GroupSaveResult::Failed | GroupSaveResult::Ignored) => Some(
                Self::alert(
                    ORDER_MODAL_RESULT_SELECTOR,
                    AlertTone::Error,
                    Some(i18n.t("bulk.panel.saveFailedTitle")),
                    i18n.t("bulk.panel.groupSaveFailed"),
                    theme,
                )
                .debug_selector(ORDER_MODAL_RESULT_SELECTOR),
            ),
            Some(GroupSaveResult::Partial(partial)) => Some(
                Self::alert(
                    ORDER_MODAL_RESULT_SELECTOR,
                    AlertTone::Warning,
                    Some(
                        i18n.t_with(
                            "bulk.panel.partialResult",
                            &Params::new()
                                .with("processed", partial.processed)
                                .with("requested", partial.requested),
                        ),
                    ),
                    i18n.t("bulk.orderModal.partial"),
                    theme,
                )
                .debug_selector(ORDER_MODAL_RESULT_SELECTOR),
            ),
            Some(GroupSaveResult::Saved) | None => None,
        };
        let footer = div()
            .flex()
            .flex_col()
            .flex_none()
            .gap(px(order_modal::FOOTER_GAP))
            .pt(px(order_modal::FOOTER_PADDING_TOP))
            .border_t_1()
            .border_color(color(colors.border_soft))
            .children(outcome)
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(order_modal::BUTTON_GAP))
                    .child(
                        Button::new(ORDER_MODAL_CANCEL_ID, i18n.t("common.cancel"))
                            .debug_selector(ORDER_MODAL_CANCEL_ID)
                            .disabled(blocked)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.with_view(cx, |dock, view| dock.close_order_modal(view));
                                this.sync(window, cx);
                            })),
                    )
                    .child(
                        Button::new(
                            ORDER_MODAL_SAVE_ID,
                            i18n.t(if saving {
                                "bulk.groupModal.applyInProgress"
                            } else {
                                "bulk.orderModal.save"
                            }),
                        )
                        .variant(ButtonVariant::Primary)
                        .debug_selector(ORDER_MODAL_SAVE_ID)
                        .disabled(save_disabled)
                        .on_click(cx.listener(|this, _, window, cx| {
                            let effects =
                                this.with_view(cx, |dock, view| dock.save_order_modal(view));
                            this.run(effects, window, cx);
                        })),
                    ),
            );
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(order_modal::GAP))
            .overflow_hidden()
            .child(heading)
            .child(board)
            .child(footer)
            .into_any_element()
    }
}

impl Render for BulkDock {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = theme::active(cx);
        let tokens = theme.tokens();
        let colors = &tokens.colors;
        let i18n = self.runtime.read(cx).i18n();
        struct Shown {
            expanded: bool,
            tall: bool,
            count: usize,
            notice: Option<&'static str>,
            favorites: bool,
            favorite_label: &'static str,
            favorite_disabled: bool,
            tags_description: &'static str,
        }
        let shown = self.with_view(cx, |dock, view| Shown {
            expanded: Dock::expanded(view),
            tall: dock.tall(view),
            count: view.selected_count(),
            notice: view.selection_notice().map(Dock::notice_description_key),
            favorites: view.all_selected_favorites(),
            favorite_label: Dock::favorite_label_key(view),
            favorite_disabled: Dock::favorite_disabled(view),
            tags_description: Dock::tags_description_key(view),
        });
        let (viewport_width, viewport_height) = self
            .viewport
            .map(|size| (f32::from(size.width), f32::from(size.height)))
            .unwrap_or_else(|| {
                let size = window.viewport_size();
                (f32::from(size.width), f32::from(size.height))
            });

        self.board_tiles.clear();
        // The larger view: drawn over the window while it is open or fading out,
        // whatever the dock shows.
        let window_size = window.viewport_size();
        let modal_content = if self.modal.read(cx).is_present() {
            Some(self.order_modal_content(&i18n, theme, window, cx))
        } else {
            self.larger.drag.cancel();
            None
        };
        // In a box of no size: the dialog draws itself over the window, and the dock's
        // column must not keep a gap for it.
        let modal = Modal::new(ORDER_MODAL_ID, &self.modal)
            .label(i18n.t("bulk.orderModal.title"))
            .size(ModalSize::XLarge)
            .height(order_modal::height(
                f32::from(window_size.height),
                ModalSize::overlay_padding(f32::from(window_size.width)),
            ))
            .theme(theme)
            .children(modal_content);

        let surface = div()
            .id("bulk-dock")
            .debug_selector(|| PANEL_SELECTOR.to_owned())
            .aria_label(SharedString::from(i18n.t("bulk.panel.ariaLabel")))
            .key_context("BulkDock")
            .occlude()
            .flex()
            .flex_col()
            .flex_none()
            .gap(px(dock_layout::GAP))
            .max_h(px(dock_layout::max_height(viewport_height)))
            .border_1()
            .border_color(color(colors.border_strong))
            .bg(color(colors.surface_solid))
            .shadow(box_shadows(tokens.shadows.popover))
            // A press or the wheel on the dock is none on the grid beneath it.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_scroll(|_, _, cx| cx.stop_propagation())
            // With the focus in the dock, the scroll keys are not the grid's.
            .on_action(|_: &ScrollStart, _, _| {})
            .on_action(|_: &ScrollEnd, _, _| {})
            .on_action(|_: &PageUp, _, _| {})
            .on_action(|_: &PageDown, _, _| {})
            .on_action(|_: &LineUp, _, _| {})
            .on_action(|_: &LineDown, _, _| {})
            .child(self.drag_listeners(cx));

        if !shown.expanded {
            self.flush_board_tiles(cx);
            return surface
                .w(px(dock_layout::CHIP_WIDTH))
                .p(px(dock_layout::CHIP_PADDING))
                .rounded(px(dock_layout::CHIP_RADIUS))
                .child(self.chip(&i18n, theme))
                .child(div().absolute().size_0().child(modal));
        }

        let header = div()
            .debug_selector(|| HEADER_SELECTOR.to_owned())
            .flex()
            .flex_none()
            .items_start()
            .gap(px(header::GAP))
            .child(div().flex_1().min_w_0().child(self.editor.clone()))
            .child(
                Button::icon(FAVORITE_ID, IconName::Heart, i18n.t(shown.favorite_label))
                    .size(header::HEART_WIDTH)
                    .height(header::HEART_HEIGHT)
                    .active(shown.favorites)
                    .pressed(shown.favorites)
                    .debug_selector(FAVORITE_ID)
                    .disabled(shown.favorite_disabled)
                    .on_click(
                        cx.listener(|this, _, _, cx| this.command(BulkCommand::ToggleFavorite, cx)),
                    ),
            )
            .child(self.counter(shown.count, &i18n, theme, window, cx));

        let panel = match self.dock.tab() {
            DockTab::Tags => {
                self.inline.drag.cancel();
                div()
                    .id(TAGS_PANEL_SELECTOR)
                    .debug_selector(|| TAGS_PANEL_SELECTOR.to_owned())
                    .role(Role::TabPanel)
                    .flex_none()
                    .text_size(px(TYPOGRAPHY.xs.size))
                    .line_height(gpui::relative(1.625))
                    .text_color(color(colors.text_muted))
                    .child(i18n.t(shown.tags_description))
                    .into_any_element()
            }
            DockTab::Group => self.group_panel(&i18n, theme, window, cx),
        };
        self.flush_board_tiles(cx);

        surface
            .overflow_y_scroll()
            .w(px(dock_layout::expanded_width(viewport_width)))
            .when(shown.tall, |surface| {
                surface.h(px(dock_layout::tall_height(viewport_height)))
            })
            .p(px(dock_layout::PADDING))
            .rounded(px(RADII.panel))
            .children(shown.notice.map(|description| {
                Self::alert(
                    NOTICE_SELECTOR,
                    AlertTone::Warning,
                    Some(i18n.t("bulk.panel.selectionClearedTitle")),
                    i18n.t(description),
                    theme,
                )
                .debug_selector(NOTICE_SELECTOR)
            }))
            .child(header)
            .children(self.feedback(&i18n, theme, cx))
            .child(self.tablist(&i18n, theme, window, cx))
            .child(panel)
            .child(div().absolute().size_0().child(modal))
    }
}

/// A canvas that reports where it was laid out; used to learn a board's box.
pub(super) fn measure(
    bounds: Rc<Cell<Option<gpui::Bounds<Pixels>>>>,
    view: gpui::WeakEntity<BulkDock>,
) -> impl IntoElement {
    let known = bounds.get();
    canvas(
        move |laid_out, window, cx| {
            bounds.set(Some(laid_out));
            // The first layout, or a new size: the board is drawn again for it.
            if known.map(|known| known.size) != Some(laid_out.size) {
                window.defer(cx, move |_, cx| {
                    view.update(cx, |_, cx| cx.notify()).ok();
                });
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}
