# 0038: The GPUI port has its own text field over a GUI-free editing model, and its keys live in a key context

- Status: Accepted
- Date: 2026-11-21
- Areas: GPUI port, frontend conventions, keyboard, dependencies
- Related: [frontend conventions](../frontend/architecture-and-ui-conventions.md#gpui-port-the-text-field), [testing](../development/testing.md#gpui-workspace), [GPUI port plan](../plans/gpui-port/README.md) (decision D1, epic E04), [ADR 0036](0036-gpui-second-adapter-in-its-own-workspace.md), [ADR 0037](0037-gpui-client-backend-port-and-model-convention.md)

## Context

GPUI ships no text field. The Tauri interface gets its fields from WebKit (`<input>`): the search box, the tag editor, the group key and order, the confirmation word of a delete. The port needs the same in five later epics, and [ADR 0036](0036-gpui-second-adapter-in-its-own-workspace.md) rules out the component library that would provide one (`gpui-component`, published by a third party).

Three properties of GPUI shape the solution:

- The input handler of a window (`EntityInputHandler`) speaks UTF-16 offsets, as the platforms' input-method protocols do. A Rust `String` slices by UTF-8 byte offsets and panics inside a character.
- Key bindings run before text input. A keystroke that matches a binding never reaches the focused field as a character.
- A binding is chosen by the depth of its key context in the focused element's ancestry. A binding without a context counts as deep as the focused element, and among equals the one registered last wins.

## Options considered

1. **Take `gpui-component`.** Rejected with [ADR 0036](0036-gpui-second-adapter-in-its-own-workspace.md): one third-party publisher, and it needs its `gpui-pre` snapshots instead of the pinned Zed revision.
2. **Copy Zed's example `crates/gpui/examples/input.rs` as it is.** It has the element and the input handler, but the buffer, the selection and the offset conversions sit in the view, there is no word movement, no undo, no scrolling and no disabled state, and its bindings have no context.
3. **Start from that example and move everything that needs no GUI into the client crate.** The view keeps shaping, painting and event glue; the buffer becomes a tested model.
4. **Protect typing with a keystroke interceptor.** An `intercept_keystrokes` hook could claim every printable key while a field has the focus and insert the character itself, so no shortcut could shadow typing. It would re-implement what the platform does after an unhandled key (`replace_text_in_range`), would run for every keystroke of the application, and would diverge from how GPUI and Zed scope bindings.
5. **Report Enter and Escape as events of the field.** Simple, but an event cannot be declined: an Escape that closes nothing in the owner could not fall through to the dialog around it.

## Decision

Option 3, with keys scoped by a key context.

- **Model.** `media_tagger_client::text` holds everything that needs no GUI: `LineEditor` (buffer, selection as anchor and head, composition of an input method, undo and redo, an optional filter for what may be entered), grapheme and word boundaries, UTF-8 ↔ UTF-16 conversion, the single-line rule, the key table `KEYMAP`, horizontal scrolling and the field's visuals from the theme tokens. Every entry point accepts any offset and moves it to a character boundary first.
- **View.** `media_tagger_gpui::ui::text_input` (`TextInput`) is derived from Zed's example (Apache-2.0; the file header keeps the origin). It shapes one line, paints selection, text and caret under a content mask, and forwards keys, pointer and the input handler to the model.
- **Keys.** `text_input::init` binds every entry of `KEYMAP` in the key context `TextInput`. With the focus in a field these bindings are the deepest match, so a shortcut bound further out does not fire for a key the field handles. Two rules follow for every shortcut of the port: it is **never bound without a context**, and a shortcut on a key that types a character **uses `text_input::outside_text_input(context)`** (`(context) && !TextInput`).
- **Owner.** Enter and Escape are the actions `text_input::actions::Confirm` and `Cancel`. The field does not handle them (it only ends a running composition), so they bubble to the element around the field; an owner that declines calls `cx.propagate()` and the keystroke falls through to the next binding further out. Arrow up and down are `Up` and `Down`: an owner takes them in the capture phase, otherwise the caret goes to the start or the end. Text and caret changes are the events `TextInputEvent::Changed` and `SelectionChanged`.
- **One line.** Inserted text is reduced by `single_line`: line breaks at the end are dropped, every other line break becomes one space (`\r\n` counts once), a tab becomes a space, other control characters are dropped. WebKit does the same with pasted line breaks.
- **Dependency.** `unicode-segmentation` (UAX #29 grapheme clusters and word segments) becomes a direct dependency of the client crate. It was already in both lock files through other crates, in the same version.

## Consequences

- The caret never stands inside a grapheme cluster, and no offset from the platform can slice inside a character; `media_tagger_client` tests cover emoji sequences, combining marks, CJK, UTF-16 offsets inside surrogate pairs and past the end.
- The model runs in tests without GPUI; the view is tested on GPUI's test platform (`tests/text_input.rs`), and once through the headless renderer.
- The two shortcut rules are a convention the compiler does not check. `tests/text_input.rs` shows them working with stand-in shortcuts; each epic that adds shortcuts has to bind them this way and test it with a focused field.
- A field that is disabled while it has the focus gives the focus up, and the window then has no focused element. The application shell has to put the focus back on itself (`on_focus_lost`), or its key context is not in the dispatch path.
- Known differences from a WebKit field: a pasted tab becomes a space; Backspace removes a whole grapheme cluster; the caret does not blink; there is no middle-click paste of the primary selection, no auto-scroll while a drag is held outside the field, and no select-all on Tab focus.
- Composition was tested through the input handler, not with a real input method on X11 or Wayland. The candidate window is placed by the bounds the field reports after a frame in which the caret can have moved (`Window::invalidate_character_coordinates`) and, while composing, by the start of the composition in the line of the last frame, because GPUI's Linux backends ask before the next frame is drawn.
- No effect on the Tauri application, its wire contract, the core or `src-tauri/Cargo.lock`.
