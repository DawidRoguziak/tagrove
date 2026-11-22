//! The single-line text field. GPUI ships none, and the port takes no component
//! library from a third party (plan decision D1), so this is the field every screen
//! uses: the search box, the tag editor, the group key, the confirmation word.
//!
//! Derived from `crates/gpui/examples/input.rs` of the Zed repository
//! (<https://github.com/zed-industries/zed>, commit
//! `d01b134cddf5e4f71964deb2d6f81cdb38fcf1e7`, the pinned GPUI revision; Copyright
//! Zed Industries, Inc., licensed under the Apache License, Version 2.0,
//! <https://www.apache.org/licenses/LICENSE-2.0>). The element that shapes and paints
//! the line, the input-handler glue and the mouse handling started as that example.
//! Changed here: the buffer, its history and every offset conversion moved into
//! `media_tagger_client::text`; added are horizontal scrolling, word and line
//! selection with the mouse, undo and redo, the disabled state, the theme tokens, and
//! keys bound in the field's own key context.
//!
//! # Keys and the owner of a field
//!
//! [`init`] binds the keys of `media_tagger_client::text::KEYMAP` in the key context
//! [`KEY_CONTEXT`]. GPUI resolves a keystroke to the binding with the deepest matching
//! context, so with the focus in a field these bindings win over a shortcut bound
//! further out, and the shortcut does not fire. Two rules keep that true for every
//! shortcut of the application:
//!
//! - **A shortcut is never bound without a context.** A binding without one counts as
//!   deep as the focused element and, registered later, would take Enter or Ctrl+A
//!   away from the field.
//! - **A shortcut on a key that types a character** (a letter, a digit, Space, with or
//!   without Shift) **uses [`outside_text_input`]** for its context. Key bindings run
//!   before text input, so the field cannot claim such a key itself.
//!
//! A shortcut on a key the field does not use (Ctrl+K, F-keys) fires with the focus in
//! a field, as it does in the Tauri interface.
//!
//! Enter and Escape are dispatched as [`actions::Confirm`] and [`actions::Cancel`]. The
//! field does not handle them, so they bubble to its owner: an element around the
//! field handles them with `on_action`, and calls `cx.propagate()` to pass one on (an
//! Escape that closes nothing). Unhandled, the keystroke falls through to the next
//! binding further out, such as the Escape of a dialog. Arrow up and down are
//! [`actions::Up`] and [`actions::Down`]: an owner that walks a list with them takes them
//! in the capture phase (`capture_action`) and stops propagation; otherwise the caret
//! goes to the start or the end of the line.
//!
//! The owner reads the text with [`TextInput::text`] when the field emits
//! [`TextInputEvent::Changed`]; the field is the only holder of the text being edited.
//!
//! A field that is disabled while it has the focus gives the focus up, which leaves
//! the window without a focused element and so without a key context in the dispatch
//! path. The application shell takes the focus back in `on_focus_lost`.

use std::ops::Range;

use gpui::{
    div, fill, point, prelude::*, px, relative, rgba, size, App, Bounds, BoxShadow, ClipboardItem,
    ContentMask, Context, CursorStyle, DispatchPhase, ElementId, ElementInputHandler, Entity,
    EntityInputHandler, EventEmitter, FocusHandle, Focusable, GlobalElementId, Hsla,
    InspectorElementId, KeyBinding, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, PaintQuad, Pixels, Point, Role, ShapedLine, SharedString, Style, Subscription,
    TextAlign, TextRun, UTF16Selection, UnderlineStyle, Window,
};
use media_tagger_client::{
    text::{
        boundary::common_prefix_len, field_visuals, scroll_to_reveal, style_runs, Change, Command,
        Edit, FieldChrome, FieldState, FieldVisuals, LineEditor, Selection, CARET_WIDTH, KEYMAP,
    },
    theme::{Rgba, Shadow, ThemeName},
};

/// The key context of a focused text field.
pub const KEY_CONTEXT: &str = "TextInput";

/// The actions the keys of a field dispatch.
pub mod actions {
    use gpui::{actions, Action};
    use media_tagger_client::text::Edit;

    actions!(
        text_input,
        [
            /// Copies the selection to the clipboard.
            Copy,
            /// Moves the selection to the clipboard.
            Cut,
            /// Inserts the clipboard's text.
            Paste,
            /// Arrow up: for the owner's list, otherwise the caret goes to the start.
            Up,
            /// Arrow down: for the owner's list, otherwise the caret goes to the end.
            Down,
            /// Enter: for the owner of the field.
            Confirm,
            /// Escape: for the owner of the field.
            Cancel,
        ]
    );

    /// A command that changes the buffer only.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Action)]
    #[action(namespace = text_input, no_json)]
    pub struct EditText(pub Edit);
}

/// The key bindings of a text field: every entry of `KEYMAP`, in [`KEY_CONTEXT`].
pub fn key_bindings() -> Vec<KeyBinding> {
    let context = Some(KEY_CONTEXT);
    KEYMAP
        .iter()
        .map(|&(keystroke, command)| match command {
            Command::Edit(edit) => KeyBinding::new(keystroke, actions::EditText(edit), context),
            Command::Copy => KeyBinding::new(keystroke, actions::Copy, context),
            Command::Cut => KeyBinding::new(keystroke, actions::Cut, context),
            Command::Paste => KeyBinding::new(keystroke, actions::Paste, context),
            Command::Up => KeyBinding::new(keystroke, actions::Up, context),
            Command::Down => KeyBinding::new(keystroke, actions::Down, context),
            Command::Confirm => KeyBinding::new(keystroke, actions::Confirm, context),
            Command::Cancel => KeyBinding::new(keystroke, actions::Cancel, context),
        })
        .collect()
}

/// Registers the key bindings of text fields. Call once at startup.
pub fn init(cx: &mut App) {
    cx.bind_keys(key_bindings());
}

/// The key context of a shortcut that is bound in `context` and must not fire while a
/// text field has the focus: every shortcut on a key that types a character. See the
/// module documentation.
pub fn outside_text_input(context: &str) -> String {
    format!("({context}) && !{KEY_CONTEXT}")
}

/// What a field tells its owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextInputEvent {
    /// The text changed, and with it possibly the caret. Text being composed by an
    /// input method counts, as it does for the `input` event of a browser.
    Changed,
    /// The caret or the selection moved and the text is the same.
    SelectionChanged,
}

/// What the platform's text services have not been told yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum PlatformNotice {
    Selection,
    Content,
}

/// A selection the pointer is dragging.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Drag {
    Characters,
    /// Started with a double click on the word at this range.
    Words(Range<usize>),
}

/// The line as the last frame drew it: what pointer positions and the platform's
/// questions about bounds are answered from.
struct PaintedLine {
    line: ShapedLine,
    bounds: Bounds<Pixels>,
    scroll: Pixels,
    /// The line shows the placeholder, not the text.
    placeholder: bool,
    /// The field had the focus.
    focused: bool,
}

pub struct TextInput {
    editor: LineEditor,
    focus_handle: FocusHandle,
    placeholder: SharedString,
    label: Option<SharedString>,
    disabled: bool,
    theme: ThemeName,
    chrome: FieldChrome,
    height: Option<f32>,
    /// How far the line is shifted to the left, in pixels.
    scroll: f32,
    painted: Option<PaintedLine>,
    drag: Option<Drag>,
    platform_notice: Option<PlatformNotice>,
    _focus_subscriptions: [Subscription; 2],
    /// Set for a field that follows the active theme ([`TextInput::themed`]).
    _theme_subscription: Option<Subscription>,
}

impl EventEmitter<TextInputEvent> for TextInput {}

impl Focusable for TextInput {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl TextInput {
    /// An empty field drawn with the tokens of `theme`.
    pub fn new(theme: ThemeName, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle().tab_stop(true);
        let focus_subscriptions = [
            cx.on_focus(&focus_handle, window, |_, _, cx| cx.notify()),
            cx.on_blur(&focus_handle, window, |input, _, cx| input.blurred(cx)),
        ];
        Self {
            editor: LineEditor::new(),
            focus_handle,
            placeholder: SharedString::default(),
            label: None,
            disabled: false,
            theme,
            chrome: FieldChrome::default(),
            height: None,
            scroll: 0.0,
            painted: None,
            drag: None,
            platform_notice: None,
            _focus_subscriptions: focus_subscriptions,
            _theme_subscription: None,
        }
    }

    /// An empty field drawn with the active theme (`ui::theme`), which it follows when
    /// the theme changes. [`Self::new`] keeps the theme it is given.
    pub fn themed(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut input = Self::new(crate::ui::theme::active(cx), window, cx);
        input._theme_subscription = Some(cx.observe_global_in::<crate::ui::theme::ActiveTheme>(
            window,
            |input, _, cx| {
                let theme = crate::ui::theme::active(cx);
                input.set_theme(theme, cx);
            },
        ));
        input
    }

    pub fn text(&self) -> &str {
        self.editor.text()
    }

    /// The caret as a UTF-8 byte offset into [`Self::text`].
    pub fn cursor(&self) -> usize {
        self.editor.cursor()
    }

    pub fn selection(&self) -> Selection {
        self.editor.selection()
    }

    /// An input method is composing text in the field.
    pub fn is_composing(&self) -> bool {
        self.editor.is_composing()
    }

    /// The theme the field draws with.
    pub fn theme(&self) -> ThemeName {
        self.theme
    }

    pub fn is_disabled(&self) -> bool {
        self.disabled
    }

    /// How far the line is shifted to the left to keep the caret visible.
    pub fn scroll_offset(&self) -> Pixels {
        px(self.scroll)
    }

    /// The caret's horizontal position in the window as the last frame drew it;
    /// `None` before the first frame.
    pub fn caret_x(&self) -> Option<Pixels> {
        let painted = self.painted.as_ref()?;
        let in_line = if painted.placeholder {
            px(0.)
        } else {
            painted.line.x_for_index(self.editor.cursor())
        };
        Some(painted.bounds.left() - painted.scroll + in_line)
    }

    /// The area of the text inside the field as the last frame drew it.
    pub fn text_bounds(&self) -> Option<Bounds<Pixels>> {
        self.painted.as_ref().map(|painted| painted.bounds)
    }

    /// Replaces the text from outside; the caret goes to its end and the undo history
    /// is dropped. Setting the text the field already has changes nothing.
    pub fn set_text(&mut self, text: &str, cx: &mut Context<Self>) {
        let change = self.editor.set_text(text);
        self.publish(change, cx);
    }

    /// Selects the bytes `range` of the text, the caret at its end.
    pub fn set_selection(&mut self, range: Range<usize>, cx: &mut Context<Self>) {
        let change = self.editor.set_selection(range);
        self.publish(change, cx);
    }

    pub fn select_all(&mut self, cx: &mut Context<Self>) {
        let change = self.editor.select_all();
        self.publish(change, cx);
    }

    /// Replaces the bytes `range` with `text` as one undo step and puts the caret
    /// behind it: how an owner completes a token from a suggestion.
    pub fn replace_range(&mut self, range: Range<usize>, text: &str, cx: &mut Context<Self>) {
        let change = self.editor.replace_range(range, text);
        self.publish(change, cx);
    }

    pub fn set_placeholder(
        &mut self,
        placeholder: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) {
        self.placeholder = placeholder.into();
        cx.notify();
    }

    /// The name assistive technology announces for the field (`aria-label`).
    pub fn set_label(&mut self, label: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.label = Some(label.into());
        cx.notify();
    }

    pub fn set_theme(&mut self, theme: ThemeName, cx: &mut Context<Self>) {
        self.theme = theme;
        cx.notify();
    }

    pub fn set_chrome(&mut self, chrome: FieldChrome, cx: &mut Context<Self>) {
        self.chrome = chrome;
        cx.notify();
    }

    /// A fixed height in pixels (`h-8`, `h-10` of the Tauri interface), the line
    /// centred in it. Without one the field is as high as its line and padding.
    pub fn set_height(&mut self, height: Option<f32>, cx: &mut Context<Self>) {
        self.height = height;
        cx.notify();
    }

    /// Restricts what can be entered; see `LineEditor::set_accept`.
    pub fn set_accept(&mut self, accept: Option<fn(&str) -> bool>) {
        self.editor.set_accept(accept);
    }

    /// A disabled field shows its text dimmed, takes no input and cannot be focused;
    /// one that has the focus gives it up.
    pub fn set_disabled(&mut self, disabled: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.disabled == disabled {
            return;
        }
        self.disabled = disabled;
        self.drag = None;
        if disabled && self.focus_handle.is_focused(window) {
            window.blur(cx);
        }
        cx.notify();
    }

    fn visuals(&self, focused: bool) -> FieldVisuals {
        field_visuals(
            self.theme.tokens(),
            self.chrome,
            FieldState {
                focused,
                disabled: self.disabled,
            },
        )
    }

    /// Tells the owner, the platform and the renderer what an operation changed.
    fn publish(&mut self, change: Change, cx: &mut Context<Self>) {
        let (event, notice) = match change {
            Change::None => return,
            Change::Selection => (TextInputEvent::SelectionChanged, PlatformNotice::Selection),
            Change::Text => (TextInputEvent::Changed, PlatformNotice::Content),
        };
        // The platform is told during the next paint, where the window is at hand.
        self.platform_notice = self.platform_notice.max(Some(notice));
        cx.emit(event);
        cx.notify();
    }

    fn blurred(&mut self, cx: &mut Context<Self>) {
        self.drag = None;
        // A composition does not outlive the focus: what was composed stays.
        let change = self.editor.ime_unmark();
        self.publish(change, cx);
        cx.notify();
    }

    // Keys

    fn on_edit(&mut self, action: &actions::EditText, _: &mut Window, cx: &mut Context<Self>) {
        self.edit(action.0, cx);
    }

    fn on_up(&mut self, _: &actions::Up, _: &mut Window, cx: &mut Context<Self>) {
        self.fallback(Command::Up, cx);
    }

    fn on_down(&mut self, _: &actions::Down, _: &mut Window, cx: &mut Context<Self>) {
        self.fallback(Command::Down, cx);
    }

    fn fallback(&mut self, command: Command, cx: &mut Context<Self>) {
        if let Some(edit) = command.fallback_edit() {
            self.edit(edit, cx);
        }
    }

    fn edit(&mut self, edit: Edit, cx: &mut Context<Self>) {
        let change = self.editor.apply(edit);
        self.publish(change, cx);
    }

    fn on_copy(&mut self, _: &actions::Copy, _: &mut Window, cx: &mut Context<Self>) {
        let selected = self.editor.selected_text();
        if !selected.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(selected.to_owned()));
        }
    }

    fn on_cut(&mut self, _: &actions::Cut, _: &mut Window, cx: &mut Context<Self>) {
        // Without a selection nothing is cut, but ending a composition the filter
        // refuses still changed the text.
        let (cut, change) = self.editor.cut();
        if let Some(cut) = cut {
            cx.write_to_clipboard(ClipboardItem::new_string(cut));
        }
        self.publish(change, cx);
    }

    fn on_paste(&mut self, _: &actions::Paste, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            let change = self.editor.paste(&text);
            self.publish(change, cx);
        }
    }

    /// Enter and Escape during a composition end it and go no further, as a browser
    /// keeps them from the page (`isComposing`). Otherwise they are the owner's.
    fn on_confirm(&mut self, _: &actions::Confirm, _: &mut Window, cx: &mut Context<Self>) {
        self.end_composition_or_propagate(cx);
    }

    fn on_cancel(&mut self, _: &actions::Cancel, _: &mut Window, cx: &mut Context<Self>) {
        self.end_composition_or_propagate(cx);
    }

    fn end_composition_or_propagate(&mut self, cx: &mut Context<Self>) {
        if self.editor.is_composing() {
            let change = self.editor.ime_unmark();
            self.publish(change, cx);
        } else {
            cx.propagate();
        }
    }

    // Pointer

    /// The line's own x coordinate of a window position.
    fn x_in_line(painted: &PaintedLine, position: Point<Pixels>) -> Pixels {
        position.x - painted.bounds.left() + painted.scroll
    }

    /// The caret position nearest to a window position.
    fn offset_at(&self, position: Point<Pixels>) -> usize {
        match &self.painted {
            Some(painted) if !painted.placeholder => self.editor.nearest_boundary(
                painted
                    .line
                    .closest_index_for_x(Self::x_in_line(painted, position)),
            ),
            _ => 0,
        }
    }

    /// The offset of the character under a window position, for selecting its word.
    fn character_at(&self, position: Point<Pixels>) -> usize {
        match &self.painted {
            Some(painted) if !painted.placeholder => {
                let x = Self::x_in_line(painted, position);
                painted.line.index_for_x(x).unwrap_or(if x < px(0.) {
                    0
                } else {
                    self.editor.text().len()
                })
            }
            _ => 0,
        }
    }

    fn on_mouse_down(&mut self, event: &MouseDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let change = match event.click_count {
            0 | 1 => {
                self.drag = Some(Drag::Characters);
                let offset = self.offset_at(event.position);
                if event.modifiers.shift {
                    self.editor.select_to(offset)
                } else {
                    self.editor.move_to(offset)
                }
            }
            2 => {
                let before = self.editor.selection();
                let word = self
                    .editor
                    .select_word_at(self.character_at(event.position));
                self.drag = Some(Drag::Words(word));
                if self.editor.selection() == before {
                    Change::None
                } else {
                    Change::Selection
                }
            }
            _ => {
                self.drag = None;
                self.editor.select_all()
            }
        };
        self.publish(change, cx);
    }

    fn drag_to(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if event.pressed_button != Some(MouseButton::Left) {
            // The button was released where the window did not see it.
            self.drag = None;
            return;
        }
        let change = match self.drag.clone() {
            Some(Drag::Characters) => self.editor.select_to(self.offset_at(event.position)),
            Some(Drag::Words(origin)) => self
                .editor
                .extend_word_selection(origin, self.character_at(event.position)),
            None => return,
        };
        self.publish(change, cx);
    }
}

impl EntityInputHandler for TextInput {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let (text, actual) = self.editor.ime_text_for_range(range_utf16);
        adjusted_range.replace(actual);
        Some(text.to_owned())
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        let (range, reversed) = self.editor.ime_selection();
        Some(UTF16Selection { range, reversed })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.editor.ime_marked_range()
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let change = self.editor.ime_unmark();
        self.publish(change, cx);
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.disabled {
            return;
        }
        let change = self.editor.ime_commit(range_utf16, text);
        self.publish(change, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.disabled {
            return;
        }
        let change = self
            .editor
            .ime_compose(range_utf16, new_text, new_selected_range_utf16);
        self.publish(change, cx);
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let painted = self.painted.as_ref()?;
        let (start, end) = if painted.placeholder {
            // An empty field: the caret is at its left edge.
            (px(0.), px(0.))
        } else {
            // The Linux backends ask right after an input method changed the text,
            // before the frame with that text is drawn. The line of the last frame still
            // places what the two texts start with; an offset behind that is answered
            // with its end, which during a composition is where the composed text
            // starts.
            let drawn = common_prefix_len(painted.line.text.as_ref(), self.editor.text());
            let range = self.editor.ime_byte_range(&range_utf16);
            (
                painted.line.x_for_index(range.start.min(drawn)),
                painted.line.x_for_index(range.end.min(drawn)),
            )
        };
        let left = element_bounds.left() - painted.scroll;
        Some(Bounds::from_corners(
            point(left + start, element_bounds.top()),
            point(left + end, element_bounds.bottom()),
        ))
    }

    fn selection_bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        element_bounds: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Bounds<Pixels>> {
        self.bounds_for_range(range_utf16, element_bounds, window, cx)
            .into_iter()
            .collect()
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let painted = self.painted.as_ref()?;
        if !painted.bounds.contains(&point) {
            return None;
        }
        Some(self.editor.ime_offset(self.offset_at(point)))
    }

    fn set_selected_text_range(
        &mut self,
        range_utf16: Range<usize>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let change = self.editor.ime_set_selection(range_utf16);
        self.publish(change, cx);
    }

    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        Some(self.editor.ime_len())
    }

    fn accepts_text_input(&self, _: &mut Window, _: &mut Context<Self>) -> bool {
        !self.disabled
    }
}

fn color(color: Rgba) -> Hsla {
    rgba(color.to_rgba_u32()).into()
}

fn box_shadow(shadow: &Shadow) -> BoxShadow {
    BoxShadow {
        color: color(shadow.color),
        offset: point(px(shadow.offset_x), px(shadow.offset_y)),
        blur_radius: px(shadow.blur),
        spread_radius: px(shadow.spread),
        inset: shadow.inset,
    }
}

/// The shadows of `shadows` that are inset, or those that are not.
fn shadows(shadows: &[Shadow], inset: bool) -> Vec<BoxShadow> {
    shadows
        .iter()
        .filter(|shadow| shadow.inset == inset)
        .map(box_shadow)
        .collect()
}

impl Render for TextInput {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let visuals = self.visuals(self.focus_handle.is_focused(window));
        let line = TextLine { input: cx.entity() };
        let field = div()
            .id("text-input")
            .role(Role::TextInput)
            .key_context(KEY_CONTEXT)
            .flex()
            .items_center()
            .w_full()
            .min_w_0()
            .overflow_hidden()
            .rounded(px(visuals.radius))
            .bg(color(visuals.background))
            .text_color(color(visuals.text))
            .opacity(visuals.opacity)
            .map(|field| match self.height {
                Some(height) => field.h(px(height)),
                None => field,
            })
            .when_some(self.label.clone(), |field, label| field.aria_label(label));
        let field = match visuals.border {
            // GPUI draws inset shadows from the element's bounds and its border over
            // them; CSS draws them inside the border. The padding box is therefore an
            // element of its own that carries the inset shadows.
            Some(border) => field
                .border_1()
                .border_color(color(border))
                .shadow(shadows(visuals.shadows, false))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .size_full()
                        .min_w_0()
                        .px(px(visuals.padding_x))
                        .when(self.height.is_none(), |inner| {
                            inner.py(px(visuals.padding_y))
                        })
                        .rounded(px((visuals.radius - 1.0).max(0.0)))
                        .shadow(shadows(visuals.shadows, true))
                        .child(line),
                ),
            None => field
                .px(px(visuals.padding_x))
                .when(self.height.is_none(), |field| {
                    field.py(px(visuals.padding_y))
                })
                .shadow(visuals.shadows.iter().map(box_shadow).collect())
                .child(line),
        };
        if self.disabled {
            return field.cursor(CursorStyle::OperationNotAllowed);
        }
        field
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::on_edit))
            .on_action(cx.listener(Self::on_up))
            .on_action(cx.listener(Self::on_down))
            .on_action(cx.listener(Self::on_copy))
            .on_action(cx.listener(Self::on_cut))
            .on_action(cx.listener(Self::on_paste))
            .on_action(cx.listener(Self::on_confirm))
            .on_action(cx.listener(Self::on_cancel))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
    }
}

/// The element that shapes and paints the line of a [`TextInput`] and connects it to
/// the platform's text input.
struct TextLine {
    input: Entity<TextInput>,
}

struct PrepaintState {
    line: ShapedLine,
    /// Where the line starts: left of the element when it is scrolled.
    origin: Point<Pixels>,
    scroll: Pixels,
    placeholder: bool,
    selection: Option<PaintQuad>,
    caret: Option<PaintQuad>,
}

impl Element for TextLine {
    type RequestLayoutState = ();
    type PrepaintState = PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = window.line_height().into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let input = self.input.read(cx);
        let focused = !input.disabled && input.focus_handle.is_focused(window);
        let visuals = input.visuals(focused);
        let text_style = window.text_style();
        let font_size = text_style.font_size.to_pixels(window.rem_size());

        let text = input.editor.text();
        let placeholder = text.is_empty();
        let selected = input.editor.selected_range();
        let shown_selection = (focused && !placeholder && !selected.is_empty()).then_some(selected);
        let (display, specs) = if placeholder {
            (
                input.placeholder.clone(),
                style_runs(input.placeholder.len(), None, None),
            )
        } else {
            (
                SharedString::from(text.to_owned()),
                style_runs(
                    text.len(),
                    shown_selection.as_ref(),
                    input.editor.marked_range().as_ref(),
                ),
            )
        };
        let runs: Vec<TextRun> = specs
            .iter()
            .map(|spec| {
                let run_color = color(if placeholder {
                    visuals.placeholder
                } else if spec.selected {
                    visuals.selection_text
                } else {
                    visuals.text
                });
                TextRun {
                    len: spec.len,
                    font: text_style.font(),
                    color: run_color,
                    background_color: None,
                    underline: spec.marked.then_some(UnderlineStyle {
                        color: Some(run_color),
                        thickness: px(1.),
                        wavy: false,
                    }),
                    strikethrough: None,
                }
            })
            .collect();
        let line = window
            .text_system()
            .shape_line(display, font_size, &runs, None);

        let (caret_x, line_width) = if placeholder {
            (px(0.), px(0.))
        } else {
            (line.x_for_index(input.editor.cursor()), line.width())
        };
        let scroll = px(scroll_to_reveal(
            input.scroll,
            caret_x.into(),
            line_width.into(),
            bounds.size.width.into(),
            CARET_WIDTH,
        ));
        let origin = point(bounds.left() - scroll, bounds.top());
        let selection = shown_selection.map(|selected| {
            fill(
                Bounds::from_corners(
                    point(origin.x + line.x_for_index(selected.start), bounds.top()),
                    point(origin.x + line.x_for_index(selected.end), bounds.bottom()),
                ),
                color(visuals.selection_background),
            )
        });
        let caret = (focused && selection.is_none()).then(|| {
            fill(
                Bounds::new(
                    point(origin.x + caret_x, bounds.top()),
                    size(px(CARET_WIDTH), bounds.size.height),
                ),
                color(visuals.caret),
            )
        });
        PrepaintState {
            line,
            origin,
            scroll,
            placeholder,
            selection,
            caret,
        }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let (focus_handle, disabled) = {
            let input = self.input.read(cx);
            (input.focus_handle.clone(), input.disabled)
        };
        let focused = !disabled && focus_handle.is_focused(window);
        if !disabled {
            window.handle_input(
                &focus_handle,
                ElementInputHandler::new(bounds, self.input.clone()),
                cx,
            );
        }

        let line_height = window.line_height();
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            if let Some(selection) = prepaint.selection.take() {
                window.paint_quad(selection);
            }
            // A glyph that cannot be rasterised leaves a gap; the frame goes on.
            prepaint
                .line
                .paint(
                    prepaint.origin,
                    line_height,
                    TextAlign::Left,
                    None,
                    window,
                    cx,
                )
                .ok();
            if let Some(caret) = prepaint.caret.take() {
                window.paint_quad(caret);
            }
        });

        let painted = PaintedLine {
            line: prepaint.line.clone(),
            bounds,
            scroll: prepaint.scroll,
            placeholder: prepaint.placeholder,
            focused,
        };
        self.input.update(cx, |input, _| {
            // The field took the focus, or the line is drawn somewhere else.
            let moved = input.painted.as_ref().is_none_or(|previous| {
                !previous.focused
                    || previous.bounds != painted.bounds
                    || previous.scroll != painted.scroll
            });
            input.scroll = painted.scroll.into();
            input.painted = Some(painted);
            let notice = input.platform_notice.take();
            match notice {
                Some(PlatformNotice::Content) => window.notify_text_input_changed(&focus_handle),
                Some(PlatformNotice::Selection) => {
                    window.notify_text_selection_changed(&focus_handle)
                }
                None => {}
            }
            if focused && (moved || notice.is_some()) {
                // The notices above do nothing on Linux, where the candidate window
                // of an input method is placed only when the platform is handed the
                // caret's bounds: GPUI reads them once this frame is drawn. The
                // backends drop them during a composition, so the caret is reported
                // whenever it can have moved, before one starts.
                window.invalidate_character_coordinates();
            }
        });

        // A drag goes on outside the field, so these listen on the window.
        let input = self.input.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
            if phase == DispatchPhase::Bubble && input.read(cx).drag.is_some() {
                input.update(cx, |input, cx| input.drag_to(event, cx));
            }
        });
        let input = self.input.clone();
        window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
            if phase == DispatchPhase::Bubble
                && event.button == MouseButton::Left
                && input.read(cx).drag.is_some()
            {
                input.update(cx, |input, _| input.drag = None);
            }
        });
    }
}
