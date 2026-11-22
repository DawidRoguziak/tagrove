//! The tag list (`TagListSearchLauncher.tsx`, `TagListModal.tsx`, `TagListChipList.tsx`):
//! the launcher in the tools row and the dialog it opens.
//!
//! The view only draws and forwards. Everything the list decides (its pages, the
//! filter, the choices, the click timer, Apply) is the runtime's
//! [`TagList`](media_tagger_client::search::tag_list::TagList); pages are loaded on the
//! executor. The view follows the model's open state with its [`ModalState`], gives
//! the filter field the focus when the dialog opens and the launcher back when it
//! closes, reports the list's scroll metrics after layout, and scrolls the list back
//! to the top when the model starts over. A tag button that gets the keyboard focus is
//! scrolled into view, and the Arrow, Page, Home and End keys on a tag scroll the list,
//! as the browser does with the Solid list's overflow box.
//!
//! The buttons are packed into rows from their measured widths
//! ([`ChipRows`](media_tagger_client::search::tag_list::rows::ChipRows)), and a frame
//! draws only the rows near the visible part of the list, plus the row of the focused
//! button and its neighbours; the rows above and below take their height as margin. A
//! frame's work then follows the list's height, not the number of loaded tags.

use std::{
    collections::{HashMap, HashSet},
    ops::Range,
};

use gpui::{
    actions, canvas, div, point, prelude::*, px, AnyElement, App, BoxShadow, ClickEvent, Context,
    Entity, FocusHandle, Focusable, KeyBinding, Pixels, Role, ScrollHandle, SharedString,
    Subscription, TextRun, TextStyle, Toggled, Window,
};
use media_tagger_client::{
    i18n::{I18n, Language, Params},
    icons::IconName,
    overlay::ModalSize,
    runtime::RuntimeInput,
    search::{
        layout::tools,
        tag_list::{
            layout::{
                chip, chip_visuals, content_max_height, dialog, key_scroll_top, reveal_scroll_top,
                state_key, ListScrollKey,
            },
            rows::{ChipRows, ChipSize},
            ListMetrics, TagListInput, TagSelectionMode, TagSelections,
        },
    },
    text::FieldChrome,
    theme::{RADII, TYPOGRAPHY},
};

use crate::{
    runtime::Runtime,
    ui::{
        button::Button,
        fonts,
        modal::{Modal, ModalEvent, ModalState},
        text_input::{outside_text_input, TextInput, TextInputEvent},
        theme::{self, color, weight},
    },
    views::gallery::{LineDown, LineUp, PageDown, PageUp, ScrollEnd, ScrollStart},
};

actions!(
    tag_list,
    [
        /// Shift+Enter or Shift+Space on a tag: what a double click does.
        ExcludeTag,
    ]
);

/// The key context of a tag button.
pub const CHIP_KEY_CONTEXT: &str = "TagListChip";

/// Element ids and debug selectors.
pub const LAUNCHER_ID: &str = "tag-list-open";
pub const MODAL_ID: &str = "tag-list-modal";
pub const FILTER_SELECTOR: &str = "tag-list-filter";
pub const SUMMARY_SELECTOR: &str = "tag-list-summary";
pub const LIST_SELECTOR: &str = "tag-list-results";
pub const MESSAGE_SELECTOR: &str = "tag-list-message";
pub const LOADING_MORE_SELECTOR: &str = "tag-list-loading-more";
pub const CLOSE_ID: &str = "tag-list-close";
pub const CANCEL_ID: &str = "tag-list-cancel";
pub const APPLY_ID: &str = "tag-list-apply";

/// The debug selector of the tag button at `index`.
pub fn chip_selector(index: usize) -> String {
    format!("tag-list-chip-{index}")
}

/// Shift+Enter and Shift+Space on a tag button. A plain Enter or Space activates the
/// button (a click); with Shift GPUI makes no click of it. Space types a character, so
/// its binding stays out of text fields (`outside_text_input`, as `text_input` asks).
pub fn key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("shift-enter", ExcludeTag, Some(CHIP_KEY_CONTEXT)),
        KeyBinding::new(
            "shift-space",
            ExcludeTag,
            Some(&outside_text_input(CHIP_KEY_CONTEXT)),
        ),
    ]
}

/// Binds the keys of the tag buttons. Part of `views::init`.
pub fn init(cx: &mut App) {
    cx.bind_keys(key_bindings());
}

/// What a tag button shows: worked out once per page and kept until its choice or the
/// language changes (the memos of `TagListChipList`), so that a render, which every
/// scroll frame causes, does no work per tag beyond drawing it.
#[derive(Clone)]
struct ChipRow {
    tag: SharedString,
    mode: Option<TagSelectionMode>,
    label: SharedString,
    /// The width of the tag's text on one line, as the text element measures it.
    text_width: f32,
}

/// The text run of a tag button's label: the root's family (`theme::app_root`) at
/// `text-xs font-medium`.
fn chip_text_run(len: usize) -> TextRun {
    TextStyle {
        font_family: fonts::SANS.into(),
        font_weight: weight(TYPOGRAPHY.weight_medium),
        ..TextStyle::default()
    }
    .to_run(len)
}

/// The width of `tag` on one line, rounded up as the text element rounds it.
fn chip_text_width(tag: &SharedString, window: &Window) -> f32 {
    let line = window.text_system().shape_line(
        tag.clone(),
        px(TYPOGRAPHY.xs.size),
        &[chip_text_run(tag.len())],
        None,
    );
    f32::from(line.width()).ceil()
}

/// The height of `tag` wrapped at `wrap_width`, as the text element measures it.
fn chip_text_height(
    tag: &SharedString,
    wrap_width: f32,
    line_height: Pixels,
    window: &Window,
) -> f32 {
    window
        .text_system()
        .shape_text(
            tag.clone(),
            px(TYPOGRAPHY.xs.size),
            &[chip_text_run(tag.len())],
            Some(px(wrap_width)),
            None,
        )
        .map(|lines| {
            lines
                .iter()
                .map(|line| f32::from(line.size(line_height).height))
                .sum()
        })
        .unwrap_or(f32::from(line_height))
}

impl ChipRow {
    fn label(i18n: &I18n<'_>, tag: &str, mode: Option<TagSelectionMode>) -> SharedString {
        let state = i18n.t(state_key(mode));
        let params = Params::new().with("tag", tag).with("state", &state);
        i18n.t_with("tagList.tagButtonAria", &params).into()
    }
}

pub struct TagListModal {
    runtime: Entity<Runtime>,
    modal: Entity<ModalState>,
    input: Entity<TextInput>,
    /// The launcher's focus: the focus goes back to it when the dialog closes.
    launcher_focus: FocusHandle,
    scroll: ScrollHandle,
    /// The model's starts that were applied to the scroll position.
    seen_resets: u64,
    /// The focus handles of the tag buttons, by tag.
    chip_focus: HashMap<String, FocusHandle>,
    /// The tag whose button had the focus at the last render: a button that gets the
    /// keyboard focus is scrolled into view once, not on every frame.
    focused_chip: Option<SharedString>,
    /// The buttons of the shown page, and what they were worked out for.
    rows: Vec<ChipRow>,
    rows_page: Option<u64>,
    rows_language: Language,
    rows_selections: TagSelections,
    /// Counts the times `rows` was built again: the packing below is kept until then.
    rows_version: u64,
    /// The buttons packed into rows, and the rows version and content width they were
    /// packed for.
    packed: ChipRows,
    packed_for: Option<(u64, u32)>,
    /// The buttons drawn by the last render, and how many.
    drawn: Vec<Range<usize>>,
    drawn_chips: usize,
    /// How often a button's state and label were worked out.
    label_builds: u64,
    /// The tag whose button takes the focus once it is drawn (a comparison scenario
    /// whose Tauri steps end with a click on it).
    scripted_focus: Option<String>,
    language: Language,
    _subscriptions: Vec<Subscription>,
}

impl TagListModal {
    pub fn new(runtime: Entity<Runtime>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (i18n, language, resets) = {
            let runtime = runtime.read(cx);
            (
                runtime.i18n(),
                runtime.model().language(),
                runtime.model().tag_list().scroll_resets(),
            )
        };
        let input = cx.new(|cx| {
            let mut input = TextInput::themed(window, cx);
            input.set_chrome(FieldChrome::Outlined, cx);
            input.set_height(Some(dialog::INPUT_HEIGHT), cx);
            input.set_placeholder(i18n.t("tagList.filterPlaceholder"), cx);
            input.set_label(i18n.t("tagList.filterLabel"), cx);
            input
        });
        let launcher_focus = cx.focus_handle().tab_stop(true);
        let modal = cx.new(|cx| {
            let mut modal = ModalState::new(cx);
            // `data-initial-focus` on the filter, and the launcher afterwards.
            modal.set_initial_focus(Some(input.focus_handle(cx)));
            modal.set_restore_focus(Some(launcher_focus.clone()));
            modal
        });
        let subscriptions = vec![
            cx.subscribe(&input, |this, input, event, cx| {
                if *event != TextInputEvent::Changed {
                    return;
                }
                let text = input.read(cx).text().to_owned();
                if text != this.runtime.read(cx).model().tag_list().query() {
                    this.send(TagListInput::SetQuery(text), cx);
                }
            }),
            cx.subscribe(&modal, |this, _, event, cx| {
                if *event == ModalEvent::Dismiss {
                    this.send(TagListInput::Close, cx);
                }
            }),
            cx.observe_in(&runtime, window, |this, _, window, cx| {
                this.sync(window, cx)
            }),
        ];
        // The launcher's state goes with the view (`TagListSearchLauncher` keeps `open`
        // locally): a list left open is closed.
        cx.on_release(|this: &mut Self, cx| {
            if this.runtime.read(cx).model().tag_list().is_open() {
                this.runtime.update(cx, |runtime, cx| {
                    runtime.send(RuntimeInput::TagList(TagListInput::Close), cx)
                });
            }
        })
        .detach();
        let mut view = Self {
            runtime,
            modal,
            input,
            launcher_focus,
            scroll: ScrollHandle::new(),
            seen_resets: resets,
            chip_focus: HashMap::new(),
            focused_chip: None,
            rows: Vec::new(),
            rows_page: None,
            rows_language: language,
            rows_selections: TagSelections::new(),
            rows_version: 0,
            packed: ChipRows::default(),
            packed_for: None,
            drawn: Vec::new(),
            drawn_chips: 0,
            label_builds: 0,
            scripted_focus: None,
            language,
            _subscriptions: subscriptions,
        };
        // A list the runtime opened before the view existed (a comparison scenario).
        view.sync(window, cx);
        view
    }

    pub fn modal(&self) -> &Entity<ModalState> {
        &self.modal
    }

    pub fn input(&self) -> &Entity<TextInput> {
        &self.input
    }

    pub fn launcher_focus(&self) -> &FocusHandle {
        &self.launcher_focus
    }

    /// The scroll state of the list of tags.
    pub fn scroll_handle(&self) -> &ScrollHandle {
        &self.scroll
    }

    /// The focus handle of the button of `tag`, once it has been drawn.
    pub fn chip_focus(&self, tag: &str) -> Option<&FocusHandle> {
        self.chip_focus.get(tag)
    }

    /// How often a tag button's state and label were worked out: once per tag of a new
    /// page, then only for the tags whose choice changed, or all of them for a new
    /// language. Scrolling works out none.
    pub fn label_builds(&self) -> u64 {
        self.label_builds
    }

    /// How many tag buttons the last render built: the rows near the visible part of
    /// the list, whatever the number of loaded tags.
    pub fn drawn_chips(&self) -> usize {
        self.drawn_chips
    }

    fn send(&self, input: TagListInput, cx: &mut Context<Self>) {
        self.runtime.update(cx, |runtime, cx| {
            runtime.send(RuntimeInput::TagList(input), cx)
        });
    }

    /// The launcher of the tools row (`TagListSearchLauncher`): `h-8 w-8`.
    pub fn launcher(&self, i18n: &I18n<'_>) -> Button {
        let runtime = self.runtime.downgrade();
        Button::icon(LAUNCHER_ID, IconName::Tag, i18n.t("tagList.open"))
            .size(tools::FAVORITES_SIZE)
            .track_focus(&self.launcher_focus)
            .debug_selector(LAUNCHER_ID)
            .on_click(move |_, _, cx| {
                crate::runtime::send(&runtime, RuntimeInput::TagList(TagListInput::Open), cx)
            })
    }

    /// Brings the dialog in line with the model.
    fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (open, applying, query, resets, language) = {
            let model = self.runtime.read(cx).model();
            let list = model.tag_list();
            (
                list.is_open(),
                list.applying(),
                list.query().to_owned(),
                list.scroll_resets(),
                model.language(),
            )
        };
        self.modal.update(cx, |modal, cx| {
            modal.set_close_on_escape(!applying, cx);
            modal.set_close_on_overlay_click(!applying);
            if modal.is_open() != open {
                modal.set_open(open, window, cx);
            }
        });
        self.input.update(cx, |input, cx| {
            if input.text() != query {
                input.set_text(&query, cx);
            }
            input.set_disabled(applying, window, cx);
        });
        if resets != self.seen_resets {
            self.seen_resets = resets;
            self.scroll.set_offset(point(px(0.), px(0.)));
        }
        if language != self.language {
            self.language = language;
            let i18n = I18n::new(language);
            self.input.update(cx, |input, cx| {
                input.set_placeholder(i18n.t("tagList.filterPlaceholder"), cx);
                input.set_label(i18n.t("tagList.filterLabel"), cx);
            });
        }
        if open {
            if let Some(tag) = self
                .runtime
                .update(cx, |runtime, _| runtime.take_tag_list_focus())
            {
                self.scripted_focus = Some(tag.to_owned());
            }
        } else {
            self.chip_focus.clear();
            self.focused_chip = None;
            self.scripted_focus = None;
        }
        cx.notify();
    }

    /// Brings the buttons in line with the page, the choices and the language: a new
    /// page builds them again (and forgets the focus of tags that left), a new choice
    /// relabels only the tags whose mode changed.
    fn refresh_rows(&mut self, window: &Window, cx: &App) {
        let runtime = self.runtime.read(cx);
        let i18n = runtime.i18n();
        let model = runtime.model();
        let list = model.tag_list();
        let language = model.language();
        let selections = list.selections();
        if self.rows_page != Some(list.page_version()) {
            self.rows_page = Some(list.page_version());
            self.rows_version += 1;
            // A next page appends to the shown tags, and the server's first page often
            // repeats the fallback's: the rows that stay where they were are kept.
            let same_language = language == self.rows_language;
            let same_choices = *selections == self.rows_selections;
            let mut old = std::mem::take(&mut self.rows).into_iter();
            let old_len = old.len();
            let mut kept = 0;
            let mut builds = 0;
            self.rows = list
                .page()
                .items
                .iter()
                .map(|tag| {
                    if let Some(mut row) =
                        old.next().filter(|row| same_language && *row.tag == **tag)
                    {
                        kept += 1;
                        if !same_choices {
                            let mode = selections.mode(tag);
                            if mode != row.mode {
                                row.mode = mode;
                                row.label = ChipRow::label(&i18n, tag, mode);
                                builds += 1;
                            }
                        }
                        return row;
                    }
                    let mode = selections.mode(tag);
                    builds += 1;
                    let tag = SharedString::from(tag.clone());
                    ChipRow {
                        text_width: chip_text_width(&tag, window),
                        label: ChipRow::label(&i18n, &tag, mode),
                        tag,
                        mode,
                    }
                })
                .collect();
            self.label_builds += builds;
            if kept < old_len {
                let live: HashSet<&str> = list.page().items.iter().map(String::as_str).collect();
                self.chip_focus.retain(|tag, _| live.contains(tag.as_str()));
            }
        } else {
            let relabel = language != self.rows_language;
            if !relabel && *selections == self.rows_selections {
                return;
            }
            for row in &mut self.rows {
                let mode = selections.mode(&row.tag);
                if relabel || mode != row.mode {
                    row.mode = mode;
                    row.label = ChipRow::label(&i18n, &row.tag, mode);
                    self.label_builds += 1;
                }
            }
        }
        self.rows_language = language;
        if *selections != self.rows_selections {
            self.rows_selections = selections.clone();
        }
    }

    fn chip(
        &mut self,
        index: usize,
        row: &ChipRow,
        disabled: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = theme::active(cx);
        let tag: &str = &row.tag;
        let mode = row.mode;
        let visuals = chip_visuals(theme.tokens(), mode);
        let focus = match self.chip_focus.get(tag) {
            Some(focus) => focus.clone(),
            None => {
                let focus = cx.focus_handle().tab_stop(true);
                self.chip_focus.insert(tag.to_owned(), focus.clone());
                focus
            }
        };
        let focused = !disabled && focus.is_focused(window);
        let focus_visible = focused && window.last_input_was_keyboard();
        if !disabled && self.scripted_focus.as_deref() == Some(tag) {
            self.scripted_focus = None;
            // After the dialog's own initial focus, which waits for the frame after
            // it opened and was asked for first.
            let focus = focus.clone();
            window.on_next_frame(move |window, cx| window.focus(&focus, cx));
        }
        let owned = row.tag.clone();
        let on_click = cx.listener(move |this, event: &ClickEvent, _, cx| {
            let input = if event.click_count() == 2 {
                TagListInput::DoubleClick {
                    tag: owned.to_string(),
                }
            } else {
                TagListInput::Click {
                    tag: owned.to_string(),
                    count: event.click_count(),
                }
            };
            this.send(input, cx);
        });
        let owned = row.tag.clone();
        let on_exclude = cx.listener(move |this, _: &ExcludeTag, _, cx| {
            this.send(
                TagListInput::DoubleClick {
                    tag: owned.to_string(),
                },
                cx,
            );
        });
        let chip = div()
            .id(("tag-list-chip", index))
            .debug_selector(move || chip_selector(index))
            .key_context(CHIP_KEY_CONTEXT)
            .role(Role::Button)
            .aria_label(row.label.clone())
            .aria_toggled(if mode.is_some() {
                Toggled::True
            } else {
                Toggled::False
            })
            .flex()
            .flex_none()
            .items_center()
            .max_w_full()
            .px(px(chip::PADDING_X))
            .py(px(chip::PADDING_Y))
            .rounded(px(RADII.control))
            .border_1()
            .border_color(color(visuals.border))
            .bg(color(visuals.background))
            .text_color(color(visuals.text))
            .text_size(px(TYPOGRAPHY.xs.size))
            .line_height(px(TYPOGRAPHY.xs.line_height_px()))
            .font_weight(weight(TYPOGRAPHY.weight_medium))
            .when(focus_visible, |chip| {
                chip.shadow(vec![BoxShadow {
                    color: color(visuals.focus_ring),
                    offset: point(px(0.), px(0.)),
                    blur_radius: px(0.),
                    spread_radius: px(chip::FOCUS_RING_WIDTH),
                    inset: false,
                }])
            })
            .child(row.tag.clone());
        if disabled {
            return chip.opacity(0.5).cursor_not_allowed().into_any_element();
        }
        chip.track_focus(&focus)
            .cursor_pointer()
            .hover(move |style| {
                style
                    .border_color(color(visuals.hover_border))
                    .bg(color(visuals.hover_background))
            })
            .on_click(on_click)
            .on_action(on_exclude)
            .into_any_element()
    }

    /// Packs the buttons into rows `width` wide, once per rows version and width.
    fn pack_rows(&mut self, width: f32, window: &Window) {
        let key = (self.rows_version, width.to_bits());
        if self.packed_for == Some(key) {
            return;
        }
        self.packed_for = Some(key);
        let line_height = window.pixel_snap(px(TYPOGRAPHY.xs.line_height_px()));
        let frame_x = 2.0 * (chip::PADDING_X + chip::BORDER);
        let frame_y = 2.0 * (chip::PADDING_Y + chip::BORDER);
        let one_line = f32::from(line_height) + frame_y;
        let sizes: Vec<ChipSize> = self
            .rows
            .iter()
            .map(|row| {
                let chip_width = row.text_width + frame_x;
                // A tag wider than the row wraps inside its button (`max-w-full`).
                let height = if chip_width <= width {
                    one_line
                } else {
                    chip_text_height(&row.tag, width - frame_x, line_height, window) + frame_y
                };
                ChipSize {
                    width: chip_width,
                    height,
                }
            })
            .collect();
        self.packed = ChipRows::pack(sizes, width, chip::GAP);
    }

    /// The button with the focus: one of those the last render drew (the focus moves
    /// only to a drawn button), else the one that had it, wherever it is now.
    fn focused_index(&self, rows: &[ChipRow], window: &Window) -> Option<usize> {
        let has_focus = |index: usize| {
            rows.get(index).is_some_and(|row| {
                self.chip_focus
                    .get(&*row.tag)
                    .is_some_and(|focus| focus.is_focused(window))
            })
        };
        if let Some(index) = self
            .drawn
            .iter()
            .flat_map(Clone::clone)
            .find(|index| has_focus(*index))
        {
            return Some(index);
        }
        let tag = self.focused_chip.as_ref()?;
        if !self
            .chip_focus
            .get(&**tag)
            .is_some_and(|focus| focus.is_focused(window))
        {
            return None;
        }
        rows.iter().position(|row| row.tag == *tag)
    }

    /// Scrolls the list so that its content's `top` is at the list's top edge.
    fn scroll_to(&mut self, top: f32, cx: &mut Context<Self>) {
        self.scroll.set_offset(point(px(0.), px(-top)));
        cx.notify();
    }

    /// A key that scrolls the list (Arrow, Page, Home, End on a tag button). It stops
    /// there: the gallery under the dialog does nothing with it.
    fn key_scroll(&mut self, key: ListScrollKey, cx: &mut Context<Self>) {
        let Some(metrics) = list_metrics(&self.scroll) else {
            return;
        };
        let top = key_scroll_top(metrics, key);
        if top != metrics.scroll_top {
            self.scroll_to(top, cx);
        }
    }

    /// The scroll container's metrics after layout, reported to the model when they are
    /// worth it ([`ListMetrics::worth_reporting`]): a scroll in the middle of the list
    /// goes no further than this view. `packed_width` is the content width the drawn
    /// buttons were packed for (`Some(None)`: drawn before the list had a width, `None`:
    /// no buttons); a list whose width is new is drawn again.
    fn measure(&self, packed_width: Option<Option<f32>>, cx: &Context<Self>) -> impl IntoElement {
        let view = cx.entity().downgrade();
        let scroll = self.scroll.clone();
        let known = self.runtime.read(cx).model().tag_list().metrics();
        canvas(
            move |_, window, cx| {
                let width = content_width(&scroll);
                if packed_width.is_some_and(|packed| width > 0.0 && packed != Some(width)) {
                    let view = view.clone();
                    window.defer(cx, move |_, cx| {
                        view.update(cx, |_, cx| cx.notify()).ok();
                    });
                }
                let Some(metrics) = list_metrics(&scroll) else {
                    return;
                };
                if metrics.worth_reporting(known) {
                    window.defer(cx, move |_, cx| {
                        view.update(cx, |view, cx| {
                            view.send(TagListInput::ListMetrics(metrics), cx)
                        })
                        .ok();
                    });
                }
            },
            |_, _, _, _| {},
        )
        .w_full()
        .h(px(0.))
    }
}

/// The width of the list's content (inside its padding) at the last layout; zero or
/// less before the first.
fn content_width(scroll: &ScrollHandle) -> f32 {
    f32::from(scroll.bounds().size.width) - 2.0 * dialog::LIST_PADDING
}

/// The list's metrics from its scroll state after layout; `None` before the first.
fn list_metrics(scroll: &ScrollHandle) -> Option<ListMetrics> {
    let height = f32::from(scroll.bounds().size.height);
    (height > 0.0).then(|| ListMetrics {
        content_height: height + f32::from(scroll.max_offset().y),
        viewport_height: height,
        scroll_top: -f32::from(scroll.offset().y),
    })
}

impl Render for TagListModal {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = theme::active(cx);
        let colors = &theme.tokens().colors;
        let i18n = self.runtime.read(cx).i18n();
        let (applying, placeholder, loading_more, included, excluded) = {
            let list = self.runtime.read(cx).model().tag_list();
            (
                list.applying(),
                list.placeholder_key(),
                list.loading_more(),
                list.selections().included_count(),
                list.selections().excluded_count(),
            )
        };
        if !self.modal.read(cx).is_present() {
            return div().into_any_element();
        }
        let viewport = window.viewport_size();
        let max_height = content_max_height(f32::from(viewport.width), f32::from(viewport.height));
        let muted = color(colors.text_muted);
        let close = {
            let this = cx.entity().downgrade();
            move |_: &ClickEvent, _: &mut Window, cx: &mut App| {
                this.update(cx, |this, cx| this.send(TagListInput::Close, cx))
                    .ok();
            }
        };

        let header = div()
            .flex()
            .items_start()
            .justify_between()
            .gap(px(dialog::HEADER_GAP))
            .child(
                div()
                    .text_size(px(TYPOGRAPHY.lg.size))
                    .line_height(px(TYPOGRAPHY.lg.line_height_px()))
                    .font_weight(weight(TYPOGRAPHY.heading_weight))
                    .text_color(color(colors.base_content))
                    .child(i18n.t("tagList.modalTitle")),
            )
            .child(
                Button::icon(CLOSE_ID, IconName::Close, i18n.t("tagList.closeModal"))
                    .disabled(applying)
                    .debug_selector(CLOSE_ID)
                    .on_click(close.clone()),
            );

        let field = div()
            .debug_selector(|| FILTER_SELECTOR.to_owned())
            .flex()
            .flex_col()
            .gap(px(dialog::FIELD_GAP))
            .child(
                div()
                    .text_size(px(TYPOGRAPHY.xs.size))
                    .line_height(px(TYPOGRAPHY.xs.line_height_px()))
                    .text_color(muted)
                    .child(i18n.t("tagList.filterLabel")),
            )
            .child(
                // `text-sm` of the filter.
                div()
                    .text_size(px(TYPOGRAPHY.sm.size))
                    .line_height(px(TYPOGRAPHY.sm.line_height_px()))
                    .child(self.input.clone()),
            );

        let summary = div()
            .debug_selector(|| SUMMARY_SELECTOR.to_owned())
            .text_size(px(TYPOGRAPHY.xs.size))
            .line_height(px(TYPOGRAPHY.xs.line_height_px()))
            .text_color(muted)
            .child(
                i18n.t_with(
                    "tagList.selectionSummary",
                    &Params::new()
                        .with("included", included)
                        .with("excluded", excluded),
                ),
            );

        let message = |text: String, size: f32, line_height: f32, selector: &'static str| {
            div()
                .debug_selector(move || selector.to_owned())
                .px(px(dialog::MESSAGE_PADDING_X))
                .py(px(dialog::MESSAGE_PADDING_Y))
                .text_size(px(size))
                .line_height(px(line_height))
                .text_color(muted)
                .child(text)
        };
        let mut packed_width = None;
        let body = match placeholder {
            Some(key) => {
                self.focused_chip = None;
                self.drawn.clear();
                self.drawn_chips = 0;
                message(
                    i18n.t(key),
                    TYPOGRAPHY.sm.size,
                    TYPOGRAPHY.sm.line_height_px(),
                    MESSAGE_SELECTOR,
                )
                .into_any_element()
            }
            None => {
                self.refresh_rows(window, cx);
                let width = content_width(&self.scroll);
                let packed = width > 0.0;
                if packed {
                    self.pack_rows(width, window);
                }
                packed_width = Some(packed.then_some(width));
                let rows = std::mem::take(&mut self.rows);
                let focused = self.focused_index(&rows, window);
                let focused_tag = focused.map(|index| rows[index].tag.clone());
                let reveal = focused.filter(|_| {
                    focused_tag != self.focused_chip && window.last_input_was_keyboard()
                });
                self.focused_chip = focused_tag;
                let body = if packed {
                    // A button that just got the keyboard focus is scrolled into view
                    // (the browser's focus scrolling); the list then reports its
                    // metrics as for any scroll, so a button near the end loads the
                    // next page.
                    if let (Some(row), Some(metrics)) = (
                        reveal.and_then(|index| self.packed.row_of(index)),
                        list_metrics(&self.scroll),
                    ) {
                        let item_top = dialog::LIST_PADDING + self.packed.top(row);
                        let top = reveal_scroll_top(metrics, item_top, self.packed.height(row));
                        if top != metrics.scroll_top {
                            self.scroll.set_offset(point(px(0.), px(-top)));
                        }
                    }
                    let scroll_top = -f32::from(self.scroll.offset().y) - dialog::LIST_PADDING;
                    let viewport = f32::from(self.scroll.bounds().size.height);
                    let segments = self.packed.rows_to_draw(scroll_top, viewport, focused);
                    let packed = std::mem::take(&mut self.packed);
                    let mut drawn = Vec::with_capacity(segments.len());
                    let mut lines = Vec::new();
                    let mut bottom = 0.0;
                    for segment in segments {
                        drawn.push(packed.chips_of_rows(segment.clone()));
                        for row in segment {
                            let chips: Vec<AnyElement> = packed
                                .chips(row)
                                .map(|index| self.chip(index, &rows[index], applying, window, cx))
                                .collect();
                            let top = packed.top(row);
                            let height = packed.height(row);
                            // The rows not drawn above this one take their place as
                            // its margin.
                            lines.push(
                                div()
                                    .flex()
                                    .flex_none()
                                    .w_full()
                                    .h(px(height))
                                    .mt(px(top - bottom))
                                    .gap(px(chip::GAP))
                                    .children(chips)
                                    .into_any_element(),
                            );
                            bottom = top + height;
                        }
                    }
                    let content_height = packed.content_height();
                    self.packed = packed;
                    self.drawn = drawn;
                    div()
                        .flex()
                        .flex_col()
                        .h(px(content_height))
                        .children(lines)
                        .into_any_element()
                } else {
                    // Before the list's first layout its width is unknown: the buttons
                    // are drawn in one flex-wrap, and the frame after packs them.
                    let chips: Vec<AnyElement> = rows
                        .iter()
                        .enumerate()
                        .map(|(index, row)| self.chip(index, row, applying, window, cx))
                        .collect();
                    self.drawn.clear();
                    self.drawn.push(0..rows.len());
                    div()
                        .flex()
                        .flex_wrap()
                        .gap(px(chip::GAP))
                        .children(chips)
                        .into_any_element()
                };
                self.drawn_chips = self.drawn.iter().map(ExactSizeIterator::len).sum();
                self.rows = rows;
                body
            }
        };
        let list = div()
            .id(LIST_SELECTOR)
            .debug_selector(|| LIST_SELECTOR.to_owned())
            .aria_label(SharedString::from(i18n.t("tagList.listLabel")))
            .track_scroll(&self.scroll)
            .overflow_y_scroll()
            .on_action(
                cx.listener(|this, _: &LineUp, _, cx| this.key_scroll(ListScrollKey::LineUp, cx)),
            )
            .on_action(
                cx.listener(|this, _: &LineDown, _, cx| {
                    this.key_scroll(ListScrollKey::LineDown, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &PageUp, _, cx| this.key_scroll(ListScrollKey::PageUp, cx)),
            )
            .on_action(
                cx.listener(|this, _: &PageDown, _, cx| {
                    this.key_scroll(ListScrollKey::PageDown, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &ScrollStart, _, cx| {
                    this.key_scroll(ListScrollKey::Start, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &ScrollEnd, _, cx| this.key_scroll(ListScrollKey::End, cx)),
            )
            .min_h_0()
            .flex_shrink_1()
            .p(px(dialog::LIST_PADDING))
            .rounded(px(RADII.control))
            .child(body)
            .when(loading_more, |list| {
                list.child(message(
                    i18n.t("tagList.loadingMore"),
                    TYPOGRAPHY.xs.size,
                    TYPOGRAPHY.xs.line_height_px(),
                    LOADING_MORE_SELECTOR,
                ))
            })
            .child(self.measure(packed_width, cx));

        let apply = {
            let this = cx.entity().downgrade();
            Button::new(
                APPLY_ID,
                i18n.t(if applying {
                    "tagList.applying"
                } else {
                    "common.apply"
                }),
            )
            .disabled(applying)
            .debug_selector(APPLY_ID)
            .on_click(move |_, _, cx| {
                this.update(cx, |this, cx| this.send(TagListInput::Apply, cx))
                    .ok();
            })
        };
        let footer = div()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(dialog::FOOTER_GAP))
            .child(
                Button::new(CANCEL_ID, i18n.t("common.cancel"))
                    .disabled(applying)
                    .debug_selector(CANCEL_ID)
                    .on_click(close),
            )
            .child(apply);

        Modal::new(MODAL_ID, &self.modal)
            .label(i18n.t("tagList.modalTitle"))
            .size(ModalSize::Medium)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(dialog::GAP))
                    .max_h(px(max_height))
                    .overflow_hidden()
                    .child(header)
                    .child(field)
                    .child(summary)
                    .child(list)
                    .child(footer),
            )
            .into_any_element()
    }
}
