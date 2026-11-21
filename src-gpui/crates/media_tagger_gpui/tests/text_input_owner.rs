//! The text field with an owner around it, as the tag editor, the search box and the
//! dialogs of the Tauri interface use theirs (epic E04). Ported from
//! `src/features/tagging/__tests__/TagEditor.test.tsx` where a case is about the
//! field: Enter submits the draft to the owner, the arrow keys reach the owner's list,
//! Escape keeps the draft and the focus, Tab moves between the field and its
//! neighbours, and a disabled field takes nothing and recovers. The other cases add
//! what the acceptance criteria ask for: shortcuts on keys the field uses stay silent,
//! a long line scrolls with the caret visible (also for the pointer and the input
//! method), a composition does not outlive the focus, and the clipboard keeps clusters
//! whole.

use std::ops::Range;

use gpui::{
    actions, div, point, prelude::*, px, ClipboardItem, Context, Entity, EntityInputHandler,
    FocusHandle, Focusable, KeyBinding, Modifiers, Pixels, Subscription, TestAppContext,
    VisualTestContext, Window,
};
use media_tagger_client::{text::Selection, theme::ThemeName};
use media_tagger_gpui::ui::{
    self,
    text_input::{actions as field, TextInput, TextInputEvent},
};

actions!(
    owner_shell,
    [
        FocusNextStop,
        FocusPreviousStop,
        AppCopy,
        AppCut,
        AppPaste,
        AppUndo,
        AppRedo,
        FirstTile,
        LastTile,
        RowUp,
        RowDown,
        NextImage,
        PreviousImage,
        ExtendLeft,
        WordLeft,
        RemoveTag,
        DeleteAsset,
        SelectAllTiles,
        OpenTile,
        CloseDialog
    ]
);

const SHELL: &str = "OwnerShell";
const FIELD_WIDTH: f32 = 160.0;

/// Application shortcuts on keys a text field uses itself, all in the shell's context
/// as the rules of `text_input` ask; Tab and Shift+Tab move the focus.
fn shell_bindings() -> Vec<KeyBinding> {
    let context = Some(SHELL);
    vec![
        KeyBinding::new("tab", FocusNextStop, context),
        KeyBinding::new("shift-tab", FocusPreviousStop, context),
        KeyBinding::new("ctrl-c", AppCopy, context),
        KeyBinding::new("ctrl-x", AppCut, context),
        KeyBinding::new("ctrl-v", AppPaste, context),
        KeyBinding::new("ctrl-z", AppUndo, context),
        KeyBinding::new("ctrl-y", AppRedo, context),
        KeyBinding::new("home", FirstTile, context),
        KeyBinding::new("end", LastTile, context),
        KeyBinding::new("up", RowUp, context),
        KeyBinding::new("down", RowDown, context),
        KeyBinding::new("right", NextImage, context),
        KeyBinding::new("left", PreviousImage, context),
        KeyBinding::new("shift-left", ExtendLeft, context),
        KeyBinding::new("ctrl-left", WordLeft, context),
        KeyBinding::new("backspace", RemoveTag, context),
        KeyBinding::new("delete", DeleteAsset, context),
        KeyBinding::new("ctrl-a", SelectAllTiles, context),
        KeyBinding::new("enter", OpenTile, context),
        KeyBinding::new("escape", CloseDialog, context),
    ]
}

/// A stand-in for the tag editor: a chip's remove button before the field, a button
/// after it, a suggestion list walked with the arrow keys while it is open, Enter
/// submitting the draft, Escape closing the list.
struct Owner {
    input: Entity<TextInput>,
    chip: FocusHandle,
    after: FocusHandle,
    /// The drafts Enter submitted, read from the field when the key arrived.
    submitted: Vec<String>,
    cancelled: usize,
    /// The suggestion list is open: it takes the arrow keys.
    list_open: bool,
    arrows: Vec<&'static str>,
    changes: Vec<String>,
    shortcuts: Vec<&'static str>,
    _subscription: Subscription,
}

impl Owner {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| TextInput::new(ThemeName::Dark, window, cx));
        let subscription = cx.subscribe(&input, |owner: &mut Self, input, event, cx| {
            if *event == TextInputEvent::Changed {
                owner.changes.push(input.read(cx).text().to_owned());
            }
        });
        Self {
            input,
            chip: cx.focus_handle().tab_stop(true),
            after: cx.focus_handle().tab_stop(true),
            submitted: Vec::new(),
            cancelled: 0,
            list_open: false,
            arrows: Vec::new(),
            changes: Vec::new(),
            shortcuts: Vec::new(),
            _subscription: subscription,
        }
    }

    fn shortcut(&mut self, name: &'static str, cx: &mut Context<Self>) {
        self.shortcuts.push(name);
        cx.notify();
    }
}

macro_rules! shortcut {
    ($cx:expr, $action:ty, $name:literal) => {
        $cx.listener(|owner: &mut Owner, _: &$action, _, cx| owner.shortcut($name, cx))
    };
}

impl Render for Owner {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context(SHELL)
            .size_full()
            .on_action(cx.listener(|_, _: &FocusNextStop, window, cx| window.focus_next(cx)))
            .on_action(cx.listener(|_, _: &FocusPreviousStop, window, cx| window.focus_prev(cx)))
            .on_action(shortcut!(cx, AppCopy, "copy"))
            .on_action(shortcut!(cx, AppCut, "cut"))
            .on_action(shortcut!(cx, AppPaste, "paste"))
            .on_action(shortcut!(cx, AppUndo, "undo"))
            .on_action(shortcut!(cx, AppRedo, "redo"))
            .on_action(shortcut!(cx, FirstTile, "first"))
            .on_action(shortcut!(cx, LastTile, "last"))
            .on_action(shortcut!(cx, RowUp, "row-up"))
            .on_action(shortcut!(cx, RowDown, "row-down"))
            .on_action(shortcut!(cx, NextImage, "next"))
            .on_action(shortcut!(cx, PreviousImage, "previous"))
            .on_action(shortcut!(cx, ExtendLeft, "extend-left"))
            .on_action(shortcut!(cx, WordLeft, "word-left"))
            .on_action(shortcut!(cx, RemoveTag, "remove-tag"))
            .on_action(shortcut!(cx, DeleteAsset, "delete-asset"))
            .on_action(shortcut!(cx, SelectAllTiles, "select-all"))
            .on_action(shortcut!(cx, OpenTile, "open"))
            .on_action(shortcut!(cx, CloseDialog, "close"))
            .child(div().track_focus(&self.chip).h(px(20.)).child("remove cat"))
            .child(
                div()
                    .w(px(FIELD_WIDTH))
                    .on_action(cx.listener(|owner, _: &field::Confirm, _, cx| {
                        let draft = owner.input.read(cx).text().to_owned();
                        owner.submitted.push(draft);
                    }))
                    .on_action(cx.listener(|owner, _: &field::Cancel, _, _| {
                        owner.cancelled += 1;
                        owner.list_open = false;
                    }))
                    .capture_action(cx.listener(|owner, _: &field::Up, _, cx| {
                        if owner.list_open {
                            owner.arrows.push("up");
                            cx.stop_propagation();
                        }
                    }))
                    .capture_action(cx.listener(|owner, _: &field::Down, _, cx| {
                        if owner.list_open {
                            owner.arrows.push("down");
                            cx.stop_propagation();
                        }
                    }))
                    .child(self.input.clone()),
            )
            .child(div().track_focus(&self.after).h(px(20.)).child("after"))
    }
}

struct Fixture<'a> {
    owner: Entity<Owner>,
    input: Entity<TextInput>,
    cx: &'a mut VisualTestContext,
}

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

/// Which of the three focus targets has the focus.
#[derive(Debug, PartialEq, Eq)]
enum Focus {
    Chip,
    Field,
    After,
    Other,
}

impl Fixture<'_> {
    fn focus_field(&mut self) {
        self.input.update_in(self.cx, |input, window, cx| {
            input.focus_handle(cx).focus(window, cx)
        });
        self.cx.run_until_parked();
    }

    fn focus_chip(&mut self) {
        self.owner
            .update_in(self.cx, |owner, window, cx| owner.chip.focus(window, cx));
        self.cx.run_until_parked();
    }

    fn focus(&mut self) -> Focus {
        let input = self.input.clone();
        self.owner.update_in(self.cx, |owner, window, cx| {
            if input.focus_handle(cx).is_focused(window) {
                Focus::Field
            } else if owner.chip.is_focused(window) {
                Focus::Chip
            } else if owner.after.is_focused(window) {
                Focus::After
            } else {
                Focus::Other
            }
        })
    }

    fn text(&self) -> String {
        self.input
            .read_with(self.cx, |input, _| input.text().to_owned())
    }

    fn selection(&self) -> Selection {
        self.input.read_with(self.cx, |input, _| input.selection())
    }

    fn set_disabled(&mut self, disabled: bool) {
        self.input.update_in(self.cx, |input, window, cx| {
            input.set_disabled(disabled, window, cx)
        });
        self.cx.run_until_parked();
    }

    fn owner<R>(&self, read: impl FnOnce(&Owner) -> R) -> R {
        self.owner.read_with(self.cx, |owner, _| read(owner))
    }

    fn field_center(&self) -> gpui::Point<Pixels> {
        self.input
            .read_with(self.cx, |input, _| input.text_bounds())
            .expect("the field was drawn")
            .center()
    }

    fn clipboard(&self) -> Option<String> {
        self.cx.read_from_clipboard().and_then(|item| item.text())
    }

    fn assert_caret_visible(&self) {
        self.input.read_with(self.cx, |input, _| {
            let bounds = input.text_bounds().expect("the field was drawn");
            let caret = input.caret_x().expect("the field was drawn");
            assert!(
                caret >= bounds.left() && caret + px(1.) <= bounds.right(),
                "caret at {caret:?} outside {bounds:?} (scroll {:?})",
                input.scroll_offset()
            );
        });
    }
}

// TagEditor.test.tsx

/// "supports add-only chips, excludes assigned tags and submits typed or selected
/// tags": Enter submits the typed draft; with the list open, ArrowDown walks the list
/// and the next Enter is the owner's to turn into the picked tag.
#[gpui::test]
fn enter_submits_the_typed_draft_and_the_arrows_walk_the_owners_list(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    fixture.focus_field();
    fixture.cx.simulate_input("ca");

    fixture.cx.simulate_keystrokes("enter");
    assert_eq!(fixture.owner(|owner| owner.submitted.clone()), vec!["ca"]);

    fixture
        .owner
        .update(fixture.cx, |owner, _| owner.list_open = true);
    fixture.cx.simulate_keystrokes("down down up enter");
    assert_eq!(
        fixture.owner(|owner| owner.arrows.clone()),
        vec!["down", "down", "up"]
    );
    assert_eq!(
        fixture.owner(|owner| owner.submitted.clone()),
        vec!["ca", "ca"]
    );
    // The keys moved neither the caret nor the text, and fired no shortcut.
    assert_eq!(fixture.text(), "ca");
    assert_eq!(fixture.selection(), Selection::caret(2));
    assert_eq!(fixture.focus(), Focus::Field);
    assert_eq!(
        fixture.owner(|owner| owner.shortcuts.clone()),
        Vec::<&str>::new()
    );
}

/// "tabs between chip removal and input and dismisses suggestions without losing the
/// draft".
#[gpui::test]
fn tab_moves_between_chip_and_field_and_escape_keeps_the_draft(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    fixture.focus_chip();

    fixture.cx.simulate_keystrokes("tab");
    assert_eq!(fixture.focus(), Focus::Field);

    fixture.cx.simulate_input("ca");
    fixture
        .owner
        .update(fixture.cx, |owner, _| owner.list_open = true);
    fixture.cx.simulate_keystrokes("down escape");
    fixture.owner(|owner| {
        assert_eq!(owner.arrows, vec!["down"]);
        assert_eq!(owner.cancelled, 1);
        assert!(!owner.list_open);
        assert_eq!(owner.shortcuts, Vec::<&str>::new());
    });
    assert_eq!(fixture.focus(), Focus::Field);
    assert_eq!(fixture.text(), "ca");
    assert_eq!(fixture.selection(), Selection::caret(2));

    fixture.cx.simulate_keystrokes("shift-tab");
    assert_eq!(fixture.focus(), Focus::Chip);
    fixture.cx.simulate_keystrokes("tab tab");
    assert_eq!(fixture.focus(), Focus::After);
    // Tab typed nothing on its way through the field.
    assert_eq!(fixture.text(), "ca");
}

/// "guards disabled or loading editors" and "closes active suggestions and blocks
/// writes when disabled, then recovers".
#[gpui::test]
fn a_disabled_field_takes_nothing_is_skipped_by_tab_and_recovers(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    fixture.focus_field();
    fixture.cx.simulate_input("ca");
    fixture
        .owner
        .update(fixture.cx, |owner, _| owner.list_open = true);
    let center = fixture.field_center();

    fixture.set_disabled(true);
    assert_ne!(fixture.focus(), Focus::Field);

    // Typing, the arrows and Enter reach neither the field nor its owner.
    fixture.cx.simulate_input("dog");
    fixture.cx.simulate_keystrokes("down enter");
    fixture.cx.simulate_click(center, Modifiers::none());
    assert_ne!(fixture.focus(), Focus::Field);
    fixture.owner(|owner| {
        assert_eq!(owner.submitted, Vec::<String>::new());
        assert_eq!(owner.arrows, Vec::<&str>::new());
    });
    assert_eq!(fixture.text(), "ca");

    // Tab goes past a disabled field.
    fixture.focus_chip();
    fixture.cx.simulate_keystrokes("tab");
    assert_eq!(fixture.focus(), Focus::After);

    // Enabled again, the draft is still there and Enter submits it once.
    fixture.set_disabled(false);
    fixture.cx.simulate_click(center, Modifiers::none());
    assert_eq!(fixture.focus(), Focus::Field);
    fixture
        .owner
        .update(fixture.cx, |owner, _| owner.list_open = false);
    fixture.cx.simulate_keystrokes("enter");
    assert_eq!(fixture.owner(|owner| owner.submitted.clone()), vec!["ca"]);
    assert_eq!(fixture.text(), "ca");
}

// Acceptance criteria

/// Every key the field acts on is the field's while it has the focus, even where the
/// key changes nothing (Delete at the end, Backspace in an empty field, Ctrl+C without
/// a selection, Ctrl+Z without history); with the focus elsewhere the application
/// gets each of them.
#[gpui::test]
fn shortcuts_on_keys_the_field_uses_stay_silent_while_it_has_the_focus(cx: &mut TestAppContext) {
    let keys = "backspace delete ctrl-c ctrl-x ctrl-v ctrl-z ctrl-y home end up down right left \
                shift-left ctrl-left ctrl-a";
    let mut fixture = open(cx);
    fixture.focus_field();

    // An empty field: nothing to delete, copy or undo.
    fixture.cx.simulate_keystrokes(keys);
    assert_eq!(
        fixture.owner(|owner| owner.shortcuts.clone()),
        Vec::<&str>::new()
    );
    assert_eq!(fixture.text(), "");

    // With text, and the caret at the end where Delete has nothing to remove.
    fixture.cx.simulate_input("tag");
    fixture.cx.simulate_keystrokes("end delete");
    assert_eq!(fixture.text(), "tag");
    fixture.cx.simulate_keystrokes(keys);
    assert_eq!(
        fixture.owner(|owner| owner.shortcuts.clone()),
        Vec::<&str>::new()
    );

    fixture.focus_chip();
    let text = fixture.text();
    fixture.cx.simulate_keystrokes(keys);
    assert_eq!(
        fixture.owner(|owner| owner.shortcuts.clone()),
        vec![
            "remove-tag",
            "delete-asset",
            "copy",
            "cut",
            "paste",
            "undo",
            "redo",
            "first",
            "last",
            "row-up",
            "row-down",
            "next",
            "previous",
            "extend-left",
            "word-left",
            "select-all"
        ]
    );
    assert_eq!(fixture.text(), text);
}

/// A stored value longer than the field, set by its owner (the group key, a mapped
/// root path): the caret at its end is visible, and a click inside the scrolled line
/// puts the caret under the pointer.
#[gpui::test]
fn a_long_text_set_by_the_owner_scrolls_and_the_pointer_lands_where_it_points(
    cx: &mut TestAppContext,
) {
    let mut fixture = open(cx);
    fixture.focus_field();
    let long = "a stored value that is much wider than the field it is shown in";

    fixture
        .input
        .update(fixture.cx, |input, cx| input.set_text(long, cx));
    fixture.cx.run_until_parked();
    let scrolled = fixture
        .input
        .read_with(fixture.cx, |input, _| input.scroll_offset());
    assert!(scrolled > px(0.), "the line scrolled");
    fixture.assert_caret_visible();

    // A click in the middle of the visible part of the line.
    let target = fixture.field_center();
    fixture.cx.simulate_click(target, Modifiers::none());
    let caret = fixture.selection();
    assert!(caret.is_empty());
    assert!(
        caret.head > 0 && caret.head < long.len(),
        "the caret is inside the line: {caret:?}"
    );
    let caret_x = fixture
        .input
        .read_with(fixture.cx, |input, _| input.caret_x())
        .expect("the field was drawn");
    assert!(
        (caret_x - target.x).abs() < px(12.),
        "the caret at {caret_x:?} is not under the pointer at {:?}",
        target.x
    );
    fixture.assert_caret_visible();

    // Typing in the middle of the scrolled line keeps the caret in view.
    fixture.cx.simulate_input("xyz");
    fixture.assert_caret_visible();
    assert_eq!(fixture.text().len(), long.len() + 3);
}

/// The candidate window of an input method is placed by the bounds the field answers
/// with. In a scrolled line they lie inside the field: where the composition starts
/// when the platform asks right after composing, as the Linux backends do, and around
/// the composed text, ending at the caret, once the frame with it is drawn.
#[gpui::test]
fn the_candidate_window_of_a_scrolled_line_is_at_its_caret(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    fixture.focus_field();
    fixture
        .cx
        .simulate_input("a text that is much wider than the field it is typed into");

    fixture.input.update_in(fixture.cx, |input, window, cx| {
        assert!(input.scroll_offset() > px(0.), "the line scrolled");
        let bounds = input.text_bounds().expect("the field was drawn");
        let caret_before = input.caret_x().expect("the field was drawn");
        let end = input.text_length_utf16(window, cx).expect("a length");
        input.replace_and_mark_text_in_range(None, "か", None, window, cx);
        assert_eq!(
            input.marked_text_range(window, cx),
            Some(end..end + 1),
            "the composition is marked"
        );
        // Asked before the next frame, the field answers from the line on the screen.
        let selection = input
            .selected_text_range(true, window, cx)
            .expect("a selection");
        let anchor = input
            .bounds_for_range(selection.range, bounds, window, cx)
            .expect("bounds before the frame with the composed text");
        assert!(anchor.left() >= bounds.left() && anchor.right() <= bounds.right());
        assert!(
            (anchor.left() - caret_before).abs() < px(1.) && anchor.size.width == px(0.),
            "the composition starts where the caret was: {anchor:?}, {caret_before:?}"
        );
    });
    fixture.cx.run_until_parked();
    let caret_x = fixture
        .input
        .read_with(fixture.cx, |input, _| input.caret_x())
        .expect("the field was drawn");
    fixture.input.update_in(fixture.cx, |input, window, cx| {
        let bounds = input.text_bounds().expect("the field was drawn");
        let end = input.text_length_utf16(window, cx).expect("a length") - 1;
        let marked: Range<usize> = input.marked_text_range(window, cx).expect("marked");
        assert_eq!(marked, end..end + 1);
        let candidate = input
            .bounds_for_range(marked, bounds, window, cx)
            .expect("bounds for the marked text");
        assert!(
            candidate.size.width > px(0.),
            "the composed text has a width"
        );
        assert!(candidate.left() >= bounds.left() && candidate.right() <= bounds.right());
        assert!(
            (candidate.right() - caret_x).abs() < px(1.),
            "the composed text ends at the caret"
        );
    });
    fixture.cx.run_until_parked();
    fixture.assert_caret_visible();
}

/// The focus leaving the field ends a composition: what was composed stays as text
/// the owner heard of, and it is one undo step.
#[gpui::test]
fn a_composition_ends_with_the_focus_and_keeps_its_text(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    // Focus events reach listeners only in an active window, as on a desktop.
    fixture.cx.update(|window, _| window.activate_window());
    fixture.cx.run_until_parked();
    assert!(fixture.cx.update(|window, _| window.is_window_active()));
    fixture.focus_field();
    fixture.cx.simulate_input("tag ");
    fixture.input.update_in(fixture.cx, |input, window, cx| {
        input.replace_and_mark_text_in_range(None, "に", None, window, cx);
        input.replace_and_mark_text_in_range(None, "にほん", None, window, cx);
    });
    fixture.cx.run_until_parked();
    assert!(fixture
        .input
        .read_with(fixture.cx, |input, _| input.is_composing()));

    fixture.focus_chip();

    assert!(!fixture
        .input
        .read_with(fixture.cx, |input, _| input.is_composing()));
    assert_eq!(fixture.text(), "tag にほん");
    assert_eq!(
        fixture.owner(|owner| owner.changes.last().cloned()),
        Some("tag にほん".to_owned())
    );
    fixture.input.update_in(fixture.cx, |input, window, cx| {
        assert_eq!(input.marked_text_range(window, cx), None);
    });

    fixture.focus_field();
    fixture.cx.simulate_keystrokes("ctrl-z");
    assert_eq!(fixture.text(), "tag ");
}

/// Copy and cut take a ZWJ sequence whole, undo brings back what was cut, and a paste
/// with nothing on the clipboard changes nothing.
#[gpui::test]
fn the_clipboard_keeps_clusters_whole(cx: &mut TestAppContext) {
    let mut fixture = open(cx);
    fixture.focus_field();
    let family = "👨\u{200d}👩\u{200d}👧";

    fixture.cx.simulate_keystrokes("ctrl-v");
    assert_eq!(fixture.text(), "");
    assert_eq!(fixture.owner(|owner| owner.changes.len()), 0);

    fixture.cx.simulate_input(&format!("tag {family}"));
    fixture.cx.simulate_keystrokes("shift-left ctrl-c");
    assert_eq!(fixture.clipboard().as_deref(), Some(family));
    assert_eq!(fixture.selection().range(), 4..4 + family.len());

    fixture.cx.simulate_keystrokes("ctrl-x");
    assert_eq!(fixture.text(), "tag ");
    assert_eq!(fixture.clipboard().as_deref(), Some(family));

    fixture.cx.simulate_keystrokes("ctrl-z");
    assert_eq!(fixture.text(), format!("tag {family}"));

    fixture
        .cx
        .write_to_clipboard(ClipboardItem::new_string(format!("{family}\n")));
    fixture.cx.simulate_keystrokes("home ctrl-v");
    assert_eq!(fixture.text(), format!("{family}tag {family}"));
    assert_eq!(fixture.selection(), Selection::caret(family.len()));
}

/// The pointer cannot leave the caret inside a cluster: a click on any part of an
/// emoji puts it at one of the emoji's edges.
#[gpui::test]
fn a_click_on_an_emoji_puts_the_caret_at_one_of_its_edges(cx: &mut TestAppContext) {
    let fixture = open(cx);
    let family = "👨\u{200d}👩\u{200d}👧";
    fixture.input.update(fixture.cx, |input, cx| {
        input.set_text(&format!("a{family}b"), cx)
    });
    fixture.cx.run_until_parked();
    let (start_x, end_x) = {
        let mut x_at = |offset: usize| {
            fixture.input.update(fixture.cx, |input, cx| {
                input.set_selection(offset..offset, cx)
            });
            fixture.cx.run_until_parked();
            fixture
                .input
                .read_with(fixture.cx, |input, _| input.caret_x())
                .expect("the field was drawn")
        };
        (x_at(1), x_at(1 + family.len()))
    };
    let y = fixture.field_center().y;
    assert!(end_x > start_x, "the emoji has a width");

    for step in 1..8 {
        let x = start_x + (end_x - start_x) * (step as f32 / 8.0);
        fixture.cx.simulate_click(point(x, y), Modifiers::none());
        let caret = fixture.selection();
        assert!(
            caret == Selection::caret(1) || caret == Selection::caret(1 + family.len()),
            "a click at {x:?} put the caret at {caret:?}"
        );
    }
}
