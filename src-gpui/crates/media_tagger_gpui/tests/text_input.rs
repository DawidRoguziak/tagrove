//! The text field in GPUI's test platform (no display, no GPU): focus, typing, the
//! clipboard, Enter and Escape reaching the owner, application shortcuts staying
//! silent while the field handles the key, the pointer, scrolling and the input
//! handler an input method talks to.

use std::{ops::Range, path::PathBuf, sync::Arc};

use gpui::{
    actions, div, point, prelude::*, px, rgba, size, ClipboardItem, Context, Entity,
    EntityInputHandler, FocusHandle, Focusable, HeadlessAppContext, KeyBinding, Modifiers,
    MouseButton, MouseDownEvent, Pixels, Point, Subscription, TestAppContext, VisualTestContext,
    Window,
};
use media_tagger_client::{
    text::{Selection, KEYMAP},
    theme::{Rgba, ThemeName, DARK},
};
use media_tagger_gpui::ui::{
    self,
    text_input::{self, actions as field, outside_text_input, TextInput, TextInputEvent},
};

actions!(
    test_shell,
    [
        FocusSearch,
        SelectAllTiles,
        ToggleFavorite,
        PreviousImage,
        CloseDialog,
        OpenTile
    ]
);

/// The key context of the stand-in application around the field.
const SHELL: &str = "Shell";
/// The width of the field: a few characters wide, so a long text overflows.
const FIELD_WIDTH: f32 = 160.0;

/// A stand-in for a screen that owns a field: it handles Enter and Escape, can take
/// the arrow keys for a list, and has application shortcuts bound around it.
struct Owner {
    input: Entity<TextInput>,
    elsewhere: FocusHandle,
    /// Texts the field reported with `Changed`.
    changes: Vec<String>,
    /// Carets the field reported with `SelectionChanged`.
    selections: Vec<usize>,
    confirmed: usize,
    cancelled: usize,
    /// Escape closes something of the owner's; otherwise the owner passes it on.
    closes_on_cancel: bool,
    /// The owner walks a list with the arrow keys.
    takes_arrows: bool,
    arrows: Vec<&'static str>,
    /// Application shortcuts that fired.
    shortcuts: Vec<&'static str>,
    _subscription: Subscription,
}

impl Owner {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| TextInput::new(ThemeName::Dark, window, cx));
        let subscription = cx.subscribe(&input, |owner: &mut Self, input, event, cx| {
            let input = input.read(cx);
            match event {
                TextInputEvent::Changed => owner.changes.push(input.text().to_owned()),
                TextInputEvent::SelectionChanged => owner.selections.push(input.cursor()),
            }
        });
        Self {
            input,
            elsewhere: cx.focus_handle(),
            changes: Vec::new(),
            selections: Vec::new(),
            confirmed: 0,
            cancelled: 0,
            closes_on_cancel: true,
            takes_arrows: false,
            arrows: Vec::new(),
            shortcuts: Vec::new(),
            _subscription: subscription,
        }
    }

    fn shortcut(&mut self, name: &'static str, cx: &mut Context<Self>) {
        self.shortcuts.push(name);
        cx.notify();
    }
}

impl Render for Owner {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context(SHELL)
            .size_full()
            .on_action(
                cx.listener(|owner, _: &FocusSearch, _, cx| owner.shortcut("focus-search", cx)),
            )
            .on_action(
                cx.listener(|owner, _: &SelectAllTiles, _, cx| owner.shortcut("select-all", cx)),
            )
            .on_action(
                cx.listener(|owner, _: &ToggleFavorite, _, cx| owner.shortcut("favorite", cx)),
            )
            .on_action(
                cx.listener(|owner, _: &PreviousImage, _, cx| owner.shortcut("previous", cx)),
            )
            .on_action(cx.listener(|owner, _: &CloseDialog, _, cx| owner.shortcut("close", cx)))
            .on_action(cx.listener(|owner, _: &OpenTile, _, cx| owner.shortcut("open", cx)))
            .child(
                div()
                    .track_focus(&self.elsewhere)
                    .h(px(40.))
                    .child("elsewhere"),
            )
            .child(
                div()
                    .w(px(FIELD_WIDTH))
                    .on_action(cx.listener(|owner, _: &field::Confirm, _, _| owner.confirmed += 1))
                    .on_action(cx.listener(|owner, _: &field::Cancel, _, cx| {
                        if owner.closes_on_cancel {
                            owner.cancelled += 1;
                        } else {
                            cx.propagate();
                        }
                    }))
                    .capture_action(cx.listener(|owner, _: &field::Up, _, cx| {
                        if owner.takes_arrows {
                            owner.arrows.push("up");
                            cx.stop_propagation();
                        }
                    }))
                    .capture_action(cx.listener(|owner, _: &field::Down, _, cx| {
                        if owner.takes_arrows {
                            owner.arrows.push("down");
                            cx.stop_propagation();
                        }
                    }))
                    .child(self.input.clone()),
            )
    }
}

/// The application shortcuts of the stand-in, bound as the module documentation of
/// `text_input` asks: always in a context, and with `outside_text_input` when the key
/// types a character.
fn shell_bindings() -> Vec<KeyBinding> {
    let typing_safe = outside_text_input(SHELL);
    vec![
        KeyBinding::new("ctrl-k", FocusSearch, Some(SHELL)),
        KeyBinding::new("ctrl-a", SelectAllTiles, Some(SHELL)),
        KeyBinding::new("left", PreviousImage, Some(SHELL)),
        KeyBinding::new("escape", CloseDialog, Some(SHELL)),
        KeyBinding::new("enter", OpenTile, Some(SHELL)),
        KeyBinding::new("f", ToggleFavorite, Some(&typing_safe)),
        KeyBinding::new("shift-f", ToggleFavorite, Some(&typing_safe)),
        KeyBinding::new("space", ToggleFavorite, Some(&typing_safe)),
    ]
}

struct Fixture<'a> {
    owner: Entity<Owner>,
    input: Entity<TextInput>,
    cx: &'a mut VisualTestContext,
}

/// A window with the owner and its field, application shortcuts registered after the
/// field's own (the order in which a later binding would win a tie).
fn open(cx: &mut TestAppContext) -> Fixture<'_> {
    cx.update(|cx| {
        ui::init(cx);
        cx.bind_keys(shell_bindings());
    });
    let (owner, cx) = cx.add_window_view(Owner::new);
    let input = owner.read_with(cx, |owner, _| owner.input.clone());
    cx.run_until_parked();
    Fixture { owner, input, cx }
}

impl Fixture<'_> {
    fn focus_field(&mut self) {
        self.input.update_in(self.cx, |input, window, cx| {
            input.focus_handle(cx).focus(window, cx)
        });
        self.cx.run_until_parked();
    }

    fn focus_elsewhere(&mut self) {
        self.owner.update_in(self.cx, |owner, window, cx| {
            owner.elsewhere.focus(window, cx)
        });
        self.cx.run_until_parked();
    }

    fn field_is_focused(&mut self) -> bool {
        self.input.update_in(self.cx, |input, window, cx| {
            input.focus_handle(cx).is_focused(window)
        })
    }

    fn text(&self) -> String {
        self.input
            .read_with(self.cx, |input, _| input.text().to_owned())
    }

    fn selection(&self) -> Selection {
        self.input.read_with(self.cx, |input, _| input.selection())
    }

    fn selected(&self) -> Range<usize> {
        self.selection().range()
    }

    fn set_text(&mut self, text: &str) {
        self.input
            .update(self.cx, |input, cx| input.set_text(text, cx));
        self.cx.run_until_parked();
    }

    fn shortcuts(&self) -> Vec<&'static str> {
        self.owner
            .read_with(self.cx, |owner, _| owner.shortcuts.clone())
    }

    fn clipboard(&self) -> Option<String> {
        self.cx.read_from_clipboard().and_then(|item| item.text())
    }

    fn last_change(&self) -> Option<String> {
        self.owner
            .read_with(self.cx, |owner, _| owner.changes.last().cloned())
    }

    /// Runs what was scheduled for the next frame and returns how many callbacks that
    /// were. Nothing but the field schedules one in this window: it does so to have
    /// GPUI hand the caret's bounds to the platform.
    fn caret_reports(&mut self) -> usize {
        self.cx.update(|window, cx| window.simulate_next_frame(cx))
    }

    /// Where the caret would be drawn at `offset`, in the middle of the line's height.
    fn point_at(&mut self, offset: usize) -> Point<Pixels> {
        let before = self.selection();
        self.input
            .update(self.cx, |input, cx| input.set_selection(offset..offset, cx));
        self.cx.run_until_parked();
        let position = self.input.read_with(self.cx, |input, _| {
            let bounds = input.text_bounds().expect("the field was drawn");
            point(
                input.caret_x().expect("the field was drawn"),
                bounds.center().y,
            )
        });
        self.input.update(self.cx, |input, cx| {
            input.set_selection(
                before.anchor.min(before.head)..before.anchor.max(before.head),
                cx,
            )
        });
        self.cx.run_until_parked();
        position
    }

    fn mouse_down(&mut self, position: Point<Pixels>, click_count: usize, modifiers: Modifiers) {
        self.cx.simulate_event(MouseDownEvent {
            button: MouseButton::Left,
            position,
            modifiers,
            click_count,
            first_mouse: false,
        });
    }

    fn mouse_up(&mut self, position: Point<Pixels>) {
        self.cx
            .simulate_mouse_up(position, MouseButton::Left, Modifiers::none());
    }

    fn drag_to(&mut self, position: Point<Pixels>) {
        self.cx
            .simulate_mouse_move(position, MouseButton::Left, Modifiers::none());
    }
}

#[gpui::test]
fn every_key_of_the_model_becomes_a_binding_in_the_field_context(cx: &mut TestAppContext) {
    let bindings = text_input::key_bindings();

    assert_eq!(bindings.len(), KEYMAP.len());
    cx.update(|cx| {
        ui::init(cx);
        // ui::init also binds the layer stack and the controls of epic E05.
        let others = ui::layers::key_bindings().len()
            + ui::slider::key_bindings().len()
            + ui::segmented::key_bindings().len();
        assert_eq!(
            cx.key_bindings().borrow().bindings().count(),
            KEYMAP.len() + others
        );
    });
    assert_eq!(text_input::KEY_CONTEXT, "TextInput");
    assert_eq!(outside_text_input("Lightbox"), "(Lightbox) && !TextInput");
}

#[gpui::test]
fn a_click_focuses_the_field_and_typing_goes_into_it(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    assert!(!fixture.field_is_focused());

    // Without the focus, typed text goes nowhere.
    fixture.cx.simulate_input("x");
    assert_eq!(fixture.text(), "");

    let inside = fixture
        .input
        .read_with(fixture.cx, |input, _| input.text_bounds())
        .expect("the field was drawn")
        .center();
    fixture.cx.simulate_click(inside, Modifiers::none());
    assert!(fixture.field_is_focused());

    fixture.cx.simulate_input("tag one");
    assert_eq!(fixture.text(), "tag one");
    assert_eq!(fixture.selection(), Selection::caret(7));
    // The owner heard every change with the text after it.
    fixture.owner.read_with(fixture.cx, |owner, _| {
        assert_eq!(owner.changes.len(), 7);
        assert_eq!(owner.changes.first().map(String::as_str), Some("t"));
        assert_eq!(owner.changes.last().map(String::as_str), Some("tag one"));
    });

    fixture.focus_elsewhere();
    assert!(!fixture.field_is_focused());
    fixture.cx.simulate_input("z");
    assert_eq!(fixture.text(), "tag one");
}

#[gpui::test]
fn movement_selection_and_deletion_keys_edit_the_text(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    fixture.focus_field();
    fixture.cx.simulate_input("alpha beta");

    fixture.cx.simulate_keystrokes("home right right");
    assert_eq!(fixture.selection(), Selection::caret(2));
    fixture.cx.simulate_keystrokes("shift-right shift-right");
    assert_eq!(fixture.selected(), 2..4);
    fixture.cx.simulate_keystrokes("backspace");
    assert_eq!(fixture.text(), "ala beta");
    fixture.cx.simulate_keystrokes("end ctrl-shift-left");
    assert_eq!(fixture.selected(), 4..8);
    fixture.cx.simulate_keystrokes("delete");
    assert_eq!(fixture.text(), "ala ");
    fixture.cx.simulate_keystrokes("ctrl-left ctrl-right");
    assert_eq!(fixture.selection(), Selection::caret(3));
    fixture.cx.simulate_keystrokes("shift-home");
    assert_eq!(fixture.selection(), Selection { anchor: 3, head: 0 });
    fixture.cx.simulate_keystrokes("ctrl-backspace");
    assert_eq!(fixture.text(), " ");

    // Caret movement alone is reported as a selection change, not as a text change.
    fixture.owner.update(fixture.cx, |owner, _| {
        owner.changes.clear();
        owner.selections.clear();
    });
    fixture.cx.simulate_keystrokes("right left");
    fixture.owner.read_with(fixture.cx, |owner, _| {
        assert_eq!(owner.changes, Vec::<String>::new());
        assert_eq!(owner.selections, vec![1, 0]);
    });
}

#[gpui::test]
fn undo_and_redo_keys_walk_the_history(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    fixture.focus_field();
    fixture.cx.simulate_input("one two");

    fixture.cx.simulate_keystrokes("ctrl-z");
    assert_eq!(fixture.text(), "one ");
    fixture.cx.simulate_keystrokes("ctrl-z");
    assert_eq!(fixture.text(), "");
    fixture.cx.simulate_keystrokes("ctrl-shift-z");
    assert_eq!(fixture.text(), "one ");
    fixture.cx.simulate_keystrokes("ctrl-y");
    assert_eq!(fixture.text(), "one two");
}

#[gpui::test]
fn copy_cut_and_paste_use_the_clipboard(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    fixture.focus_field();
    fixture.cx.simulate_input("red green");

    // Nothing selected: the clipboard is left alone.
    fixture
        .cx
        .write_to_clipboard(ClipboardItem::new_string("before".to_owned()));
    fixture.cx.simulate_keystrokes("ctrl-c ctrl-x");
    assert_eq!(fixture.clipboard().as_deref(), Some("before"));
    assert_eq!(fixture.text(), "red green");

    fixture.cx.simulate_keystrokes("ctrl-shift-left ctrl-c");
    assert_eq!(fixture.clipboard().as_deref(), Some("green"));
    assert_eq!(fixture.text(), "red green");

    fixture
        .cx
        .simulate_keystrokes("home ctrl-shift-right ctrl-x");
    assert_eq!(fixture.clipboard().as_deref(), Some("red"));
    assert_eq!(fixture.text(), " green");

    fixture.cx.simulate_keystrokes("end ctrl-v ctrl-v");
    assert_eq!(fixture.text(), " greenredred");

    // Paste replaces the selection, and is one undo step.
    fixture.cx.simulate_keystrokes("ctrl-a ctrl-v");
    assert_eq!(fixture.text(), "red");
    fixture.cx.simulate_keystrokes("ctrl-z");
    assert_eq!(fixture.text(), " greenredred");
}

#[gpui::test]
fn pasted_line_breaks_become_spaces(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    fixture.focus_field();
    fixture
        .cx
        .write_to_clipboard(ClipboardItem::new_string("one\r\ntwo\nthree\n".to_owned()));

    fixture.cx.simulate_keystrokes("ctrl-v");

    assert_eq!(fixture.text(), "one two three");
    // The platform's own paste request goes the same way.
    fixture.input.update_in(fixture.cx, |input, window, cx| {
        input.paste(ClipboardItem::new_string("\nfour\n".to_owned()), window, cx)
    });
    assert_eq!(fixture.text(), "one two three four");
}

#[gpui::test]
fn enter_and_escape_reach_the_owner_and_not_the_application(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    fixture.focus_field();
    fixture.cx.simulate_input("tag");

    fixture.cx.simulate_keystrokes("enter");
    fixture.cx.simulate_keystrokes("escape");

    fixture.owner.read_with(fixture.cx, |owner, _| {
        assert_eq!((owner.confirmed, owner.cancelled), (1, 1));
        assert_eq!(owner.shortcuts, Vec::<&str>::new());
    });
    // Neither key changed the text.
    assert_eq!(fixture.text(), "tag");
    assert!(fixture.field_is_focused());
}

#[gpui::test]
fn an_escape_the_owner_passes_on_falls_through_to_the_application(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    fixture.focus_field();
    fixture
        .owner
        .update(fixture.cx, |owner, _| owner.closes_on_cancel = false);

    fixture.cx.simulate_keystrokes("escape");

    fixture.owner.read_with(fixture.cx, |owner, _| {
        assert_eq!(owner.cancelled, 0);
        assert_eq!(owner.shortcuts, vec!["close"]);
    });
}

#[gpui::test]
fn application_shortcuts_stay_silent_while_the_field_handles_the_key(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    fixture.focus_field();

    // Keys that type a character: the text gets them, the shortcut does not fire.
    fixture.cx.simulate_input("f F");
    fixture.cx.simulate_keystrokes("space");
    assert_eq!(fixture.text(), "f F ");
    // Keys the field binds itself.
    fixture.cx.simulate_keystrokes("ctrl-a");
    assert_eq!(fixture.selected(), 0..4);
    fixture.cx.simulate_keystrokes("left");
    assert_eq!(fixture.selection(), Selection::caret(0));
    assert_eq!(fixture.shortcuts(), Vec::<&str>::new());

    // A key the field does not use still reaches the application.
    fixture.cx.simulate_keystrokes("ctrl-k");
    assert_eq!(fixture.shortcuts(), vec!["focus-search"]);
    assert_eq!(fixture.text(), "f F ");

    // With the focus elsewhere every shortcut fires and the text stays.
    fixture.focus_elsewhere();
    fixture
        .cx
        .simulate_keystrokes("f shift-f space ctrl-a left escape enter");
    assert_eq!(
        fixture.shortcuts(),
        vec![
            "focus-search",
            "favorite",
            "favorite",
            "favorite",
            "select-all",
            "previous",
            "close",
            "open"
        ]
    );
    assert_eq!(fixture.text(), "f F ");
}

#[gpui::test]
fn arrow_up_and_down_move_the_caret_unless_the_owner_takes_them(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    fixture.focus_field();
    fixture.cx.simulate_input("abc");

    fixture.cx.simulate_keystrokes("up");
    assert_eq!(fixture.selection(), Selection::caret(0));
    fixture.cx.simulate_keystrokes("down");
    assert_eq!(fixture.selection(), Selection::caret(3));
    fixture.cx.simulate_keystrokes("shift-up");
    assert_eq!(fixture.selection(), Selection { anchor: 3, head: 0 });

    fixture.cx.simulate_keystrokes("end");
    fixture
        .owner
        .update(fixture.cx, |owner, _| owner.takes_arrows = true);
    fixture.cx.simulate_keystrokes("up down up");
    assert_eq!(fixture.selection(), Selection::caret(3));
    fixture.owner.read_with(fixture.cx, |owner, _| {
        assert_eq!(owner.arrows, vec!["up", "down", "up"]);
    });
}

#[gpui::test]
fn a_long_text_scrolls_and_the_caret_stays_inside_the_field(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    fixture.focus_field();

    let caret_is_visible = |fixture: &Fixture| {
        fixture.input.read_with(fixture.cx, |input, _| {
            let bounds = input.text_bounds().expect("the field was drawn");
            let caret = input.caret_x().expect("the field was drawn");
            assert!(
                caret >= bounds.left() && caret + px(1.) <= bounds.right(),
                "caret at {caret:?} outside {bounds:?} (scroll {:?})",
                input.scroll_offset()
            );
        });
    };
    let scroll = |fixture: &Fixture| {
        f32::from(
            fixture
                .input
                .read_with(fixture.cx, |input, _| input.scroll_offset()),
        )
    };

    fixture.cx.simulate_input("short");
    assert_eq!(scroll(&fixture), 0.0);
    caret_is_visible(&fixture);

    fixture
        .cx
        .simulate_input("a text that is much wider than the field it is typed into");
    let at_end = scroll(&fixture);
    assert!(at_end > 0.0, "the line scrolled");
    caret_is_visible(&fixture);

    // Walking left moves the view only when the caret reaches its left edge.
    fixture.cx.simulate_keystrokes("left left");
    assert_eq!(scroll(&fixture), at_end);
    caret_is_visible(&fixture);
    for _ in 0..40 {
        fixture.cx.simulate_keystrokes("left");
        caret_is_visible(&fixture);
    }
    assert!(scroll(&fixture) < at_end);

    fixture.cx.simulate_keystrokes("home");
    assert_eq!(scroll(&fixture), 0.0);
    caret_is_visible(&fixture);
    fixture.cx.simulate_keystrokes("end");
    assert_eq!(scroll(&fixture), at_end);
    caret_is_visible(&fixture);

    // A shorter text does not leave the view scrolled past its end.
    fixture.cx.simulate_keystrokes("ctrl-a backspace");
    assert_eq!(scroll(&fixture), 0.0);
    caret_is_visible(&fixture);
}

#[gpui::test]
fn the_pointer_places_the_caret_and_drags_a_selection(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    fixture.set_text("alpha beta gamma");
    let at = |fixture: &mut Fixture, offset| fixture.point_at(offset);
    let (two, seven, twelve) = (
        at(&mut fixture, 2),
        at(&mut fixture, 7),
        at(&mut fixture, 12),
    );

    fixture.cx.simulate_click(two, Modifiers::none());
    assert!(fixture.field_is_focused());
    assert_eq!(fixture.selection(), Selection::caret(2));

    // Shift and a click extend the selection from the caret.
    fixture.mouse_down(seven, 1, Modifiers::shift());
    fixture.mouse_up(seven);
    assert_eq!(fixture.selection(), Selection { anchor: 2, head: 7 });

    // A drag selects, also backwards, and goes on outside the field.
    fixture.mouse_down(seven, 1, Modifiers::none());
    fixture.drag_to(twelve);
    assert_eq!(
        fixture.selection(),
        Selection {
            anchor: 7,
            head: 12
        }
    );
    fixture.drag_to(two);
    assert_eq!(fixture.selection(), Selection { anchor: 7, head: 2 });
    fixture.drag_to(point(px(-50.), px(300.)));
    assert_eq!(fixture.selection(), Selection { anchor: 7, head: 0 });
    fixture.mouse_up(two);

    // After the button is released the pointer no longer selects.
    fixture
        .cx
        .simulate_mouse_move(twelve, None, Modifiers::none());
    assert_eq!(fixture.selection(), Selection { anchor: 7, head: 0 });
}

#[gpui::test]
fn a_double_click_selects_a_word_and_a_triple_click_everything(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    fixture.set_text("alpha beta gamma");
    // Inside the glyphs, not on a boundary between two of them.
    let nudge = point(px(2.), px(0.));
    let in_beta = fixture.point_at(7) + nudge;
    let in_gamma = fixture.point_at(13) + nudge;
    let in_alpha = fixture.point_at(1) + nudge;

    fixture.mouse_down(in_beta, 2, Modifiers::none());
    assert_eq!(
        fixture.selection(),
        Selection {
            anchor: 6,
            head: 10
        }
    );

    // Dragging after the double click extends word by word, in both directions.
    fixture.drag_to(in_gamma);
    assert_eq!(
        fixture.selection(),
        Selection {
            anchor: 6,
            head: 16
        }
    );
    fixture.drag_to(in_alpha);
    assert_eq!(
        fixture.selection(),
        Selection {
            anchor: 10,
            head: 0
        }
    );
    fixture.mouse_up(in_alpha);

    fixture.mouse_down(in_beta, 3, Modifiers::none());
    fixture.mouse_up(in_beta);
    assert_eq!(
        fixture.selection(),
        Selection {
            anchor: 0,
            head: 16
        }
    );
}

#[gpui::test]
fn an_input_method_composes_through_the_input_handler(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    fixture.focus_field();
    fixture.cx.simulate_input("a🙂");

    fixture.input.update_in(fixture.cx, |input, window, cx| {
        // The platform addresses the text in UTF-16: "a" is 1 unit, the emoji 2.
        assert_eq!(input.text_length_utf16(window, cx), Some(3));
        let selection = input.selected_text_range(false, window, cx).unwrap();
        assert_eq!((selection.range, selection.reversed), (3..3, false));
        let mut adjusted = None;
        assert_eq!(
            input
                .text_for_range(0..2, &mut adjusted, window, cx)
                .as_deref(),
            Some("a🙂")
        );
        assert_eq!(adjusted, Some(0..3));

        input.replace_and_mark_text_in_range(None, "に", None, window, cx);
        input.replace_and_mark_text_in_range(None, "にほ", Some(1..2), window, cx);
        assert_eq!(input.text(), "a🙂にほ");
        assert!(input.is_composing());
        assert_eq!(input.marked_text_range(window, cx), Some(3..5));
        assert_eq!(
            input.selected_text_range(false, window, cx).unwrap().range,
            4..5
        );
    });
    fixture.cx.run_until_parked();

    // Enter and Escape during a composition do not reach the owner.
    fixture.cx.simulate_keystrokes("enter");
    fixture.owner.read_with(fixture.cx, |owner, _| {
        assert_eq!((owner.confirmed, owner.cancelled), (0, 0));
        assert_eq!(owner.shortcuts, Vec::<&str>::new());
    });
    assert_eq!(fixture.text(), "a🙂にほ");
    assert!(!fixture
        .input
        .read_with(fixture.cx, |input, _| input.is_composing()));

    fixture.cx.simulate_keystrokes("end");
    fixture.input.update_in(fixture.cx, |input, window, cx| {
        input.replace_and_mark_text_in_range(None, "ん", None, window, cx);
        input.replace_text_in_range(None, "本", window, cx);
        assert_eq!(input.text(), "a🙂にほ本");
        assert_eq!(input.marked_text_range(window, cx), None);
        // Offsets past the end or inside the emoji's surrogate pair are harmless.
        input.set_selected_text_range(2..99, window, cx);
        assert_eq!(input.selection().range(), 5..14);
        input.replace_text_in_range(Some(2..40), "!", window, cx);
        assert_eq!(input.text(), "a🙂!");
    });
    fixture.cx.run_until_parked();

    // The composition was one undo step.
    fixture.cx.simulate_keystrokes("ctrl-z ctrl-z");
    assert_eq!(fixture.text(), "a🙂にほ");
    fixture.cx.simulate_keystrokes("enter");
    fixture
        .owner
        .read_with(fixture.cx, |owner, _| assert_eq!(owner.confirmed, 1));
}

#[gpui::test]
fn the_platform_gets_bounds_for_the_candidate_window(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    fixture.focus_field();

    fixture.input.update_in(fixture.cx, |input, window, cx| {
        let bounds = input.text_bounds().expect("the field was drawn");
        // An empty field answers with its left edge.
        let caret = input.bounds_for_range(0..0, bounds, window, cx).unwrap();
        assert_eq!((caret.left(), caret.size.width), (bounds.left(), px(0.)));
    });
    fixture.cx.simulate_input("e🙂漢");
    fixture.input.update_in(fixture.cx, |input, window, cx| {
        let bounds = input.text_bounds().expect("the field was drawn");
        let emoji = input.bounds_for_range(1..3, bounds, window, cx).unwrap();
        assert!(emoji.left() > bounds.left() && emoji.size.width > px(0.));
        assert_eq!(
            (emoji.top(), emoji.bottom()),
            (bounds.top(), bounds.bottom())
        );
        let rectangles = input.selection_bounds_for_range(0..99, bounds, window, cx);
        assert_eq!(rectangles.len(), 1);
        // A point inside the emoji maps to a UTF-16 offset at one of its edges.
        let index = input
            .character_index_for_point(emoji.center(), window, cx)
            .unwrap();
        assert!(index == 1 || index == 3, "{index}");
        assert_eq!(
            input.character_index_for_point(point(px(-5.), px(-5.)), window, cx),
            None
        );
    });

    // The Linux backends ask right after a composition changed the text, before the
    // frame with that text is drawn: X11 for the selection, Wayland for the start of
    // the marked text. Both get the place where the composition starts, taken from
    // the line that is on the screen.
    fixture.cx.simulate_keystrokes("left");
    let start = fixture.input.update_in(fixture.cx, |input, window, cx| {
        let bounds = input.text_bounds().expect("the field was drawn");
        let start = input.bounds_for_range(3..3, bounds, window, cx).unwrap();
        assert!(start.left() > bounds.left());

        input.replace_and_mark_text_in_range(None, "にほ", None, window, cx);

        assert_eq!(input.text(), "e🙂にほ漢");
        assert_eq!(input.marked_text_range(window, cx), Some(3..5));
        let selection = input.selected_text_range(true, window, cx).unwrap();
        assert_eq!(selection.range, 5..5);
        assert_eq!(
            input.bounds_for_range(selection.range, bounds, window, cx),
            Some(start)
        );
        assert_eq!(
            input.bounds_for_range(3..3, bounds, window, cx),
            Some(start)
        );
        // Text in front of the composition is where it was.
        let emoji = input.bounds_for_range(1..3, bounds, window, cx).unwrap();
        assert!(emoji.left() > bounds.left() && emoji.right() == start.left());
        start
    });
    // The frame with the composed text agrees, and has the composed text behind it.
    fixture.cx.run_until_parked();
    fixture.input.update_in(fixture.cx, |input, window, cx| {
        let bounds = input.text_bounds().expect("the field was drawn");
        assert_eq!(
            input.bounds_for_range(3..3, bounds, window, cx),
            Some(start)
        );
        let composed = input.bounds_for_range(3..5, bounds, window, cx).unwrap();
        assert_eq!(composed.left(), start.left());
        assert!(composed.size.width > px(0.));
    });
}

/// On Linux the candidate window of an input method goes where the platform was last
/// handed the caret's bounds, and the backends take them only between compositions.
/// The field has GPUI hand them over after every frame in which the caret can have
/// moved: it took the focus, or the text or the selection changed.
#[gpui::test]
fn the_platform_is_handed_the_caret_whenever_it_can_have_moved(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    assert_eq!(fixture.caret_reports(), 0, "a field without the focus");

    // The focus alone: a second field takes over from the first without an edit.
    fixture.focus_field();
    assert!(fixture.caret_reports() > 0, "the field took the focus");

    fixture.cx.simulate_input("tag");
    assert!(fixture.caret_reports() > 0, "typing");
    fixture.cx.simulate_keystrokes("left");
    assert!(fixture.caret_reports() > 0, "moving the caret");
    fixture.set_text("another");
    assert!(fixture.caret_reports() > 0, "the owner set the text");

    // A frame in which the field is drawn as it was reports nothing.
    fixture.owner.update(fixture.cx, |_, cx| cx.notify());
    fixture.cx.run_until_parked();
    assert_eq!(fixture.caret_reports(), 0, "nothing changed");
    fixture.cx.simulate_keystrokes("home home");
    assert!(fixture.caret_reports() > 0, "Home moved the caret");
    assert_eq!(fixture.caret_reports(), 0, "the second Home moved nothing");

    fixture.focus_elsewhere();
    assert_eq!(fixture.caret_reports(), 0, "the focus left");
    fixture.set_text("unfocused");
    assert_eq!(fixture.caret_reports(), 0, "a field without the focus");
}

#[gpui::test]
fn a_disabled_field_takes_no_focus_and_no_input(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    fixture.focus_field();
    fixture.cx.simulate_input("kept");

    fixture.input.update_in(fixture.cx, |input, window, cx| {
        input.set_disabled(true, window, cx)
    });
    fixture.cx.run_until_parked();

    // Disabling gave up the focus, and neither keys nor the pointer reach the field.
    assert!(!fixture.field_is_focused());
    fixture.cx.simulate_input("x");
    fixture.cx.simulate_keystrokes("backspace");
    fixture.focus_elsewhere();
    fixture.cx.simulate_keystrokes("ctrl-a");
    let inside = fixture
        .input
        .read_with(fixture.cx, |input, _| input.text_bounds())
        .expect("the field was drawn")
        .center();
    fixture.cx.simulate_click(inside, Modifiers::none());
    assert!(!fixture.field_is_focused());
    assert_eq!(fixture.text(), "kept");
    assert!(fixture
        .input
        .read_with(fixture.cx, |input, _| input.is_disabled()));
    fixture.input.update_in(fixture.cx, |input, window, cx| {
        assert!(!input.accepts_text_input(window, cx));
        input.replace_text_in_range(None, "y", window, cx);
        assert_eq!(input.text(), "kept");
    });
    // With the focus outside the field, the application has its shortcut back.
    assert_eq!(fixture.shortcuts(), vec!["select-all"]);

    fixture.input.update_in(fixture.cx, |input, window, cx| {
        input.set_disabled(false, window, cx)
    });
    fixture.cx.run_until_parked();
    fixture.cx.simulate_click(inside, Modifiers::none());
    assert!(fixture.field_is_focused());
    fixture.cx.simulate_keystrokes("end");
    fixture.cx.simulate_input("!");
    assert_eq!(fixture.text(), "kept!");
}

#[gpui::test]
fn the_owner_sets_the_text_and_completes_a_token(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    fixture.focus_field();
    fixture.cx.simulate_input("dog ca");

    // `applySuggestionToValue`: the token "ca" at 4..6 becomes "cat ".
    fixture.input.update(fixture.cx, |input, cx| {
        input.replace_range(4..6, "cat ", cx)
    });
    fixture.cx.run_until_parked();
    assert_eq!(fixture.text(), "dog cat ");
    assert_eq!(fixture.selection(), Selection::caret(8));
    fixture.cx.simulate_input("x");
    assert_eq!(fixture.text(), "dog cat x");

    // Echoing the same text back changes nothing; a new text resets caret and history.
    let changes = fixture
        .owner
        .read_with(fixture.cx, |owner, _| owner.changes.len());
    fixture.cx.simulate_keystrokes("home");
    fixture.set_text("dog cat x");
    assert_eq!(fixture.selection(), Selection::caret(0));
    fixture.set_text("");
    assert_eq!(fixture.text(), "");
    fixture.cx.simulate_keystrokes("ctrl-z");
    assert_eq!(fixture.text(), "");
    fixture.owner.read_with(fixture.cx, |owner, _| {
        assert_eq!(owner.changes.len(), changes + 1);
        assert_eq!(owner.changes.last().map(String::as_str), Some(""));
    });
}

#[gpui::test]
fn a_filter_keeps_what_is_not_accepted_out_of_the_field(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    fixture.focus_field();
    fixture.input.update(fixture.cx, |input, _| {
        input.set_accept(Some(|text| text.chars().all(|c| c.is_ascii_digit())))
    });
    fixture
        .cx
        .write_to_clipboard(ClipboardItem::new_string("4x".to_owned()));

    fixture.cx.simulate_input("1a2");
    fixture.cx.simulate_keystrokes("ctrl-v");
    fixture.cx.simulate_input("3");

    assert_eq!(fixture.text(), "123");
}

/// Ctrl+X without a selection cuts nothing, but it ends a composition. When the filter
/// refuses what was composed, the field goes back to the text before it, and the owner
/// hears of that text as it heard of the composed one.
#[gpui::test]
fn a_cut_that_ends_a_refused_composition_tells_the_owner(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    fixture.focus_field();
    fixture.input.update(fixture.cx, |input, _| {
        input.set_accept(Some(|text| text.chars().all(|c| c.is_ascii_digit())))
    });
    fixture.cx.simulate_input("12");
    fixture.input.update_in(fixture.cx, |input, window, cx| {
        input.replace_and_mark_text_in_range(None, "３", None, window, cx);
    });
    fixture.cx.run_until_parked();
    assert_eq!(fixture.last_change().as_deref(), Some("12３"));

    fixture.cx.simulate_keystrokes("ctrl-x");

    assert_eq!(fixture.text(), "12");
    assert_eq!(fixture.last_change().as_deref(), Some("12"));
    assert_eq!(fixture.clipboard(), None);
    assert!(!fixture
        .input
        .read_with(fixture.cx, |input, _| input.is_composing()));
}

/// Three fields on the application background, for the renderer test: an empty one
/// with its placeholder, one with text, and a disabled one.
struct Fields {
    empty: Entity<TextInput>,
    edited: Entity<TextInput>,
    disabled: Entity<TextInput>,
}

impl Fields {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut field = |text: &str, placeholder: &'static str| {
            cx.new(|cx| {
                let mut input = TextInput::new(ThemeName::Dark, window, cx);
                input.set_placeholder(placeholder, cx);
                input.set_text(text, cx);
                input
            })
        };
        let empty = field("", "placeholder");
        let edited = field("alpha beta gamma", "");
        let disabled = field("disabled", "");
        disabled.update(cx, |input, cx| input.set_disabled(true, window, cx));
        Self {
            empty,
            edited,
            disabled,
        }
    }
}

impl Render for Fields {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap(px(16.))
            .p(px(24.))
            .bg(rgba(DARK.colors.bg_1.to_rgba_u32()))
            .text_size(px(13.))
            .children(
                [&self.empty, &self.edited, &self.disabled]
                    .map(|input| div().w(px(FIELD_WIDTH)).child(input.clone())),
            )
    }
}

/// Where the renderer test leaves its PNGs when `MEDIATAGGER_GPUI_SCREENSHOT_DIR` is
/// set.
fn screenshot_target(name: &str) -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("MEDIATAGGER_GPUI_SCREENSHOT_DIR")?);
    Some(dir.join(name))
}

/// The real field through GPUI's renderer: frame, focus ring, selection, caret,
/// placeholder, the dimmed disabled state and a scrolled line, read back as pixels.
#[test]
fn the_field_draws_its_states_with_the_theme_tokens() {
    if let Err(error) = gpui_platform::current_headless_renderer() {
        if std::env::var_os("MEDIATAGGER_GPUI_REQUIRE_RENDERER").is_some() {
            panic!("no headless renderer: {error:#}");
        }
        eprintln!("skipped: no GPU adapter for the headless renderer ({error:#})");
        return;
    }
    let text_system = gpui_platform::current_platform(true).text_system();
    let mut cx = HeadlessAppContext::with_platform(text_system, Arc::new(()), || {
        gpui_platform::current_headless_renderer()
    });
    let colors = DARK.colors;
    let rgb = |color: Rgba| [color.r, color.g, color.b, 0xff];

    let window = cx
        .open_window(size(px(320.), px(220.)), |window, cx| {
            cx.new(|cx| Fields::new(window, cx))
        })
        .expect("open the window");
    let (empty, edited, disabled) = cx
        .update_window(window.into(), |_, window, cx| {
            let fields = window.root::<Fields>().flatten().expect("the root view");
            let fields = fields.read(cx);
            (
                fields.empty.clone(),
                fields.edited.clone(),
                fields.disabled.clone(),
            )
        })
        .expect("the window is open");
    cx.update_window(window.into(), |_, window, cx| {
        edited.update(cx, |input, cx| {
            input.focus_handle(cx).focus(window, cx);
            // "beta"
            input.set_selection(6..10, cx);
        });
    })
    .expect("the window is open");
    cx.run_until_parked();

    let scale = cx
        .update_window(window.into(), |_, window, _| window.scale_factor())
        .expect("the window is open");
    let capture = cx
        .capture_screenshot(window.into())
        .expect("capture the window");
    if let Some(target) = screenshot_target("text-input-states.png") {
        capture.save(&target).expect("write the screenshot");
        eprintln!("screenshot: {}", target.display());
    }
    let text_area = |cx: &mut HeadlessAppContext, input: &Entity<TextInput>| {
        cx.update(|cx| input.read(cx).text_bounds())
            .expect("the field was drawn")
    };
    let device = |value: Pixels| (f32::from(value) * scale).round() as u32;
    let count = |area: gpui::Bounds<Pixels>, color: [u8; 4]| {
        let mut pixels = 0;
        for y in device(area.top())..device(area.bottom()) {
            for x in device(area.left())..device(area.right()) {
                if capture.get_pixel(x, y).0 == color {
                    pixels += 1;
                }
            }
        }
        pixels
    };
    // The frame around a text area: the padding of `input` (8px 10px) on each side.
    let frame = |area: gpui::Bounds<Pixels>| {
        gpui::Bounds::from_corners(
            point(area.left() - px(10.), area.top() - px(8.)),
            point(area.right() + px(10.), area.bottom() + px(8.)),
        )
    };
    let pixel = |x: Pixels, y: Pixels| capture.get_pixel(device(x), device(y)).0;

    // The empty field: field background, a one-pixel border, the placeholder drawn,
    // no caret and no selection.
    let empty_area = text_area(&mut cx, &empty);
    let empty_frame = frame(empty_area);
    assert_eq!(
        pixel(empty_frame.left() + px(4.), empty_frame.center().y),
        rgb(colors.surface_raised)
    );
    assert_eq!(
        pixel(empty_frame.center().x, empty_frame.top()),
        rgb(colors.border_strong),
        "the border of an unfocused field"
    );
    assert!(
        count(empty_area, rgb(colors.surface_raised))
            < count(empty_frame, rgb(colors.surface_raised)),
        "the placeholder is drawn"
    );
    assert_eq!(count(empty_frame, rgb(colors.primary)), 0);

    // The focused field: primary border, the selection in primary with text on it.
    let edited_area = text_area(&mut cx, &edited);
    let edited_frame = frame(edited_area);
    assert_eq!(
        pixel(edited_frame.center().x, edited_frame.top()),
        rgb(colors.primary),
        "the border of the focused field"
    );
    let selected = count(edited_area, rgb(colors.primary));
    assert!(
        selected > 100,
        "the selection is filled ({selected} pixels)"
    );
    assert_eq!(
        pixel(edited_area.left() + px(1.), edited_area.top()),
        rgb(colors.surface_raised),
        "nothing is selected at the start of the line"
    );

    // The disabled field is dimmed: its background is between the page and the field.
    let disabled_frame = frame(text_area(&mut cx, &disabled));
    let dimmed = pixel(disabled_frame.left() + px(4.), disabled_frame.center().y);
    assert_ne!(dimmed, rgb(colors.surface_raised));
    assert_ne!(dimmed, rgb(colors.bg_1));

    // A long line with the caret at its end: the caret is drawn at the right edge of
    // the text area and nothing is drawn outside the frame.
    cx.update_window(window.into(), |_, _, cx| {
        edited.update(cx, |input, cx| {
            input.set_text("a line that is much wider than the field it is in", cx)
        });
    })
    .expect("the window is open");
    cx.run_until_parked();
    let capture = cx
        .capture_screenshot(window.into())
        .expect("capture the window");
    if let Some(target) = screenshot_target("text-input-scrolled.png") {
        capture.save(&target).expect("write the screenshot");
        eprintln!("screenshot: {}", target.display());
    }
    let (caret_x, scroll) = cx.update(|cx| {
        let input = edited.read(cx);
        (
            input.caret_x().expect("the field was drawn"),
            input.scroll_offset(),
        )
    });
    assert!(scroll > px(0.), "the line scrolled");
    assert!(caret_x >= edited_area.left() && caret_x + px(1.) <= edited_area.right());
    let pixel = |x: Pixels, y: Pixels| capture.get_pixel(device(x), device(y)).0;
    assert_eq!(
        pixel(caret_x, edited_area.center().y),
        rgb(colors.base_content),
        "the caret"
    );
    for x in [edited_frame.left() - px(3.), edited_frame.right() + px(3.)] {
        for y in device(edited_frame.top())..device(edited_frame.bottom()) {
            assert_eq!(
                capture.get_pixel(device(x), y).0,
                rgb(colors.bg_1),
                "the text is clipped to the field"
            );
        }
    }
}
