//! The editing model as a user drives it: typing, moving, selecting, deleting,
//! clipboard, undo, and an input method composing text.

use super::{Change, Edit, LineEditor, Selection, MAX_UNDO_STEPS};

/// A family joined with zero-width joiners: one grapheme cluster of 18 bytes.
const FAMILY: &str = "👨\u{200d}👩\u{200d}👧";

fn editor(text: &str) -> LineEditor {
    LineEditor::with_text(text)
}

/// Types `text` one character at a time, as key presses arrive.
fn type_chars(editor: &mut LineEditor, text: &str) {
    for c in text.chars() {
        editor.insert(c.encode_utf8(&mut [0; 4]));
    }
}

fn apply_all(editor: &mut LineEditor, edits: &[Edit]) {
    for edit in edits {
        editor.apply(*edit);
    }
}

fn selection(anchor: usize, head: usize) -> Selection {
    Selection { anchor, head }
}

#[test]
fn typing_inserts_at_the_caret_and_moves_it() {
    let mut editor = LineEditor::new();

    assert_eq!(editor.insert("a"), Change::Text);
    editor.insert("c");
    editor.apply(Edit::MoveLeft);
    editor.insert("b");

    assert_eq!(editor.text(), "abc");
    assert_eq!(editor.selection(), Selection::caret(2));
    assert_eq!(editor.cursor(), 2);
}

#[test]
fn a_new_buffer_has_the_caret_at_the_end_of_its_text() {
    let editor = editor("tag");

    assert_eq!(editor.selection(), Selection::caret(3));
    assert!(!editor.can_undo());
    assert!(LineEditor::new().is_empty());
}

#[test]
fn the_caret_and_backspace_treat_a_joined_emoji_as_one_character() {
    let mut editor = editor(&format!("a{FAMILY}b"));

    editor.apply(Edit::MoveLeft);
    assert_eq!(editor.cursor(), 1 + FAMILY.len());
    editor.apply(Edit::MoveLeft);
    assert_eq!(editor.cursor(), 1);
    editor.apply(Edit::MoveRight);
    assert_eq!(editor.cursor(), 1 + FAMILY.len());

    assert_eq!(editor.apply(Edit::DeleteBackward), Change::Text);
    assert_eq!(editor.text(), "ab");
    assert_eq!(editor.cursor(), 1);
}

#[test]
fn delete_forward_removes_a_whole_flag_and_a_skin_tone_sequence() {
    let mut editor = editor("🇵🇱👍🏽x");
    editor.apply(Edit::MoveToStart);

    editor.apply(Edit::DeleteForward);
    assert_eq!(editor.text(), "👍🏽x");
    editor.apply(Edit::DeleteForward);
    assert_eq!(editor.text(), "x");
}

#[test]
fn selecting_over_an_emoji_takes_all_of_it() {
    let mut editor = editor(&format!("{FAMILY}!"));
    editor.apply(Edit::MoveToStart);

    editor.apply(Edit::SelectRight);

    assert_eq!(editor.selected_text(), FAMILY);
    assert_eq!(editor.selection(), selection(0, FAMILY.len()));
}

#[test]
fn a_combining_mark_moves_and_is_deleted_with_its_base() {
    // "é" as "e" + U+0301, and "ñ" as "n" + U+0303.
    let mut editor = editor("e\u{301}n\u{303}");

    editor.apply(Edit::MoveLeft);
    assert_eq!(editor.cursor(), 3);
    editor.apply(Edit::DeleteBackward);
    assert_eq!(editor.text(), "n\u{303}");
    assert_eq!(editor.cursor(), 0);
    editor.apply(Edit::DeleteForward);
    assert_eq!(editor.text(), "");
}

#[test]
fn cjk_text_moves_one_character_at_a_time() {
    let mut editor = editor("日本語タグ");

    apply_all(&mut editor, &[Edit::MoveLeft, Edit::MoveLeft]);
    assert_eq!(editor.cursor(), 9);
    editor.insert("の");
    assert_eq!(editor.text(), "日本語のタグ");
    apply_all(&mut editor, &[Edit::SelectLeft, Edit::SelectLeft]);
    assert_eq!(editor.selected_text(), "語の");
    editor.apply(Edit::DeleteBackward);
    assert_eq!(editor.text(), "日本タグ");
}

#[test]
fn movement_at_the_ends_changes_nothing() {
    let mut editor = editor("ab");

    assert_eq!(editor.apply(Edit::MoveRight), Change::None);
    assert_eq!(editor.apply(Edit::DeleteForward), Change::None);
    assert_eq!(editor.apply(Edit::MoveToEnd), Change::None);
    editor.apply(Edit::MoveToStart);
    assert_eq!(editor.apply(Edit::MoveLeft), Change::None);
    assert_eq!(editor.apply(Edit::DeleteBackward), Change::None);
    assert_eq!(editor.apply(Edit::DeleteWordBackward), Change::None);
    assert_eq!(editor.text(), "ab");
    assert!(!editor.can_undo());
}

#[test]
fn word_movement_stops_at_word_starts_going_left_and_word_ends_going_right() {
    let mut editor = editor("cat  -dog, bird");
    //                      012345678901234

    let mut stops = vec![];
    for _ in 0..4 {
        editor.apply(Edit::MoveWordLeft);
        stops.push(editor.cursor());
    }
    assert_eq!(stops, vec![11, 6, 0, 0]);

    let mut stops = vec![];
    for _ in 0..4 {
        editor.apply(Edit::MoveWordRight);
        stops.push(editor.cursor());
    }
    assert_eq!(stops, vec![3, 9, 15, 15]);
}

#[test]
fn word_selection_and_word_deletion_use_the_same_stops() {
    let mut editor = editor("one two three");

    editor.apply(Edit::SelectWordLeft);
    assert_eq!(editor.selected_text(), "three");
    editor.apply(Edit::SelectWordLeft);
    assert_eq!(editor.selected_text(), "two three");
    editor.apply(Edit::SelectWordRight);
    assert_eq!(editor.selected_text(), " three");

    editor.apply(Edit::MoveToEnd);
    editor.apply(Edit::DeleteWordBackward);
    assert_eq!(editor.text(), "one two ");
    editor.apply(Edit::MoveToStart);
    editor.apply(Edit::DeleteWordForward);
    assert_eq!(editor.text(), " two ");
    assert_eq!(editor.cursor(), 0);
}

#[test]
fn a_selection_made_to_the_left_is_reversed_and_grows_at_its_head() {
    let mut editor = editor("abcdef");
    editor.move_to(4);

    apply_all(&mut editor, &[Edit::SelectLeft, Edit::SelectLeft]);

    assert_eq!(editor.selection(), selection(4, 2));
    assert!(editor.selection().is_reversed());
    assert_eq!(editor.selected_range(), 2..4);
    assert_eq!(editor.selected_text(), "cd");
    assert_eq!(editor.cursor(), 2);
}

#[test]
fn a_selection_can_cross_its_anchor_in_both_directions() {
    let mut editor = editor("abcdef");
    editor.move_to(3);

    apply_all(
        &mut editor,
        &[Edit::SelectLeft, Edit::SelectRight, Edit::SelectRight],
    );
    assert_eq!(editor.selection(), selection(3, 4));
    assert!(!editor.selection().is_reversed());

    apply_all(
        &mut editor,
        &[Edit::SelectLeft, Edit::SelectLeft, Edit::SelectLeft],
    );
    assert_eq!(editor.selection(), selection(3, 1));
    assert_eq!(editor.selected_text(), "bc");

    editor.apply(Edit::SelectToEnd);
    assert_eq!(editor.selection(), selection(3, 6));
    editor.apply(Edit::SelectToStart);
    assert_eq!(editor.selection(), selection(3, 0));
}

#[test]
fn left_and_right_collapse_a_selection_to_its_ends() {
    for (edit, caret) in [(Edit::MoveLeft, 1), (Edit::MoveRight, 4)] {
        // Made left to right, then right to left: the direction does not matter.
        for (anchor, head) in [(1, 4), (4, 1)] {
            let mut editor = editor("abcdef");
            editor.move_to(anchor);
            editor.select_to(head);

            assert_eq!(editor.apply(edit), Change::Selection);
            assert_eq!(editor.selection(), Selection::caret(caret));
        }
    }
}

#[test]
fn typing_and_deleting_replace_the_selection() {
    let mut typed = editor("abcdef");
    typed.set_selection(1..5);
    typed.insert("X");
    assert_eq!(typed.text(), "aXf");
    assert_eq!(typed.cursor(), 2);

    for edit in [
        Edit::DeleteBackward,
        Edit::DeleteForward,
        Edit::DeleteWordBackward,
        Edit::DeleteWordForward,
    ] {
        let mut editor = editor("abc def");
        editor.set_selection(2..5);
        editor.apply(edit);
        assert_eq!(editor.text(), "abef", "{edit:?}");
        assert_eq!(editor.cursor(), 2);
    }
}

#[test]
fn select_all_selects_everything_once() {
    let mut editor = editor("a👍🏽");

    assert_eq!(editor.select_all(), Change::Selection);
    assert_eq!(editor.selection(), selection(0, 9));
    assert_eq!(editor.select_all(), Change::None);
    assert_eq!(LineEditor::new().select_all(), Change::None);
}

#[test]
fn offsets_from_outside_are_moved_to_a_character_boundary() {
    let mut editor = editor("a👍🏽b");

    // Inside the emoji, and far past the end.
    editor.move_to(3);
    assert_eq!(editor.cursor(), 1);
    editor.select_to(99);
    assert_eq!(editor.selection(), selection(1, 10));
    // A range that runs backwards is empty at its start.
    editor.set_selection(std::ops::Range { start: 7, end: 3 });
    assert_eq!(editor.selection(), Selection::caret(5));
    assert_eq!(editor.nearest_boundary(7), 9);
    assert_eq!(editor.nearest_boundary(3), 1);
    assert_eq!(editor.replace_range(2..99, "!"), Change::Text);
    assert_eq!(editor.text(), "a!");
}

#[test]
fn a_double_click_selects_a_word_and_a_drag_extends_by_words() {
    let mut editor = editor("alpha beta gamma");
    //                      0123456789012345

    let origin = editor.select_word_at(7);
    assert_eq!(origin, 6..10);
    assert_eq!(editor.selection(), selection(6, 10));

    editor.extend_word_selection(origin.clone(), 13);
    assert_eq!(editor.selection(), selection(6, 16));
    editor.extend_word_selection(origin.clone(), 2);
    assert_eq!(editor.selection(), selection(10, 0));
    editor.extend_word_selection(origin.clone(), 8);
    assert_eq!(editor.selection(), selection(6, 10));
    // On the space before the word: the space joins the selection.
    editor.extend_word_selection(origin, 5);
    assert_eq!(editor.selection(), selection(10, 5));
}

#[test]
fn undo_takes_back_a_typed_word_at_a_time_and_redo_brings_it_back() {
    let mut editor = LineEditor::new();
    type_chars(&mut editor, "red green  blue");

    let mut undone = vec![];
    while editor.can_undo() {
        assert_eq!(editor.undo(), Change::Text);
        undone.push(editor.text().to_owned());
    }
    assert_eq!(undone, vec!["red green  ", "red ", ""]);
    assert_eq!(editor.undo(), Change::None);

    let mut redone = vec![];
    while editor.can_redo() {
        assert_eq!(editor.apply(Edit::Redo), Change::Text);
        redone.push((editor.text().to_owned(), editor.cursor()));
    }
    assert_eq!(
        redone,
        vec![
            ("red ".to_owned(), 4),
            ("red green  ".to_owned(), 11),
            ("red green  blue".to_owned(), 15)
        ]
    );
    assert_eq!(editor.redo(), Change::None);
}

#[test]
fn undo_restores_the_selection_an_edit_replaced() {
    let mut editor = editor("abcdef");
    editor.move_to(5);
    editor.select_to(2);

    editor.insert("X");
    assert_eq!(editor.text(), "abXf");
    editor.undo();

    assert_eq!(editor.text(), "abcdef");
    assert_eq!(editor.selection(), selection(5, 2));
    editor.redo();
    assert_eq!(editor.text(), "abXf");
    assert_eq!(editor.selection(), Selection::caret(3));
}

#[test]
fn moving_the_caret_or_changing_the_kind_of_edit_starts_a_new_undo_step() {
    let mut editor = LineEditor::new();
    type_chars(&mut editor, "ab");
    editor.apply(Edit::MoveLeft);
    editor.apply(Edit::MoveRight);
    type_chars(&mut editor, "cd");
    apply_all(
        &mut editor,
        &[
            Edit::DeleteBackward,
            Edit::DeleteBackward,
            Edit::DeleteBackward,
        ],
    );
    apply_all(&mut editor, &[Edit::MoveToStart, Edit::DeleteForward]);
    assert_eq!(editor.text(), "");

    let mut undone = vec![];
    while editor.can_undo() {
        editor.undo();
        undone.push(editor.text().to_owned());
    }

    // Forward delete, the three backspaces as one, "cd", "ab".
    assert_eq!(undone, vec!["a", "abcd", "ab", ""]);
}

#[test]
fn a_new_edit_after_undo_drops_the_redo_steps() {
    let mut editor = LineEditor::new();
    type_chars(&mut editor, "one");
    editor.undo();
    assert!(editor.can_redo());

    editor.insert("x");

    assert!(!editor.can_redo());
    assert_eq!(editor.redo(), Change::None);
    assert_eq!(editor.text(), "x");
}

#[test]
fn paste_cut_and_word_deletion_are_undo_steps_of_their_own() {
    let mut editor = LineEditor::new();
    type_chars(&mut editor, "ab");
    editor.paste("cd");
    editor.paste("ef");
    type_chars(&mut editor, "gh");
    editor.set_selection(0..2);
    assert_eq!(editor.cut().0.as_deref(), Some("ab"));
    editor.apply(Edit::MoveToEnd);
    editor.apply(Edit::DeleteWordBackward);

    let mut undone = vec![];
    while editor.can_undo() {
        editor.undo();
        undone.push(editor.text().to_owned());
    }

    assert_eq!(
        undone,
        vec!["cdefgh", "abcdefgh", "abcdef", "abcd", "ab", ""]
    );
}

#[test]
fn the_history_keeps_a_bounded_number_of_steps() {
    let mut editor = LineEditor::new();
    for _ in 0..MAX_UNDO_STEPS + 20 {
        editor.paste("x");
    }

    let mut steps = 0;
    while editor.can_undo() {
        editor.undo();
        steps += 1;
    }

    assert_eq!(steps, MAX_UNDO_STEPS);
    assert_eq!(editor.text().len(), 20);
}

#[test]
fn pasted_line_breaks_become_spaces_and_trailing_ones_are_dropped() {
    let mut editor = editor("tags: ");

    assert_eq!(editor.paste("one\r\ntwo\nthree\n"), Change::Text);

    assert_eq!(editor.text(), "tags: one two three");
    assert_eq!(editor.cursor(), editor.text().len());
    editor.undo();
    assert_eq!(editor.text(), "tags: ");
}

#[test]
fn a_paste_of_nothing_but_a_line_break_changes_nothing() {
    let mut editor = editor("tag");

    assert_eq!(editor.paste("\n"), Change::None);
    assert_eq!(editor.insert("\r\n"), Change::None);
    assert_eq!(editor.text(), "tag");
    assert!(!editor.can_undo());
}

#[test]
fn a_typed_line_break_or_tab_never_reaches_the_buffer() {
    let mut editor = LineEditor::new();

    editor.insert("a\nb");
    assert_eq!(editor.insert("\t"), Change::None);
    assert_eq!(editor.ime_commit(None, "\n"), Change::None);
    editor.insert("c\u{7}");

    assert_eq!(editor.text(), "a bc");
    editor.paste("\t");
    assert_eq!(editor.text(), "a bc ");
    assert_eq!(editor.set_text("x\ny\n"), Change::Text);
    assert_eq!(editor.text(), "x y");
}

#[test]
fn paste_replaces_the_selection() {
    let mut editor = editor("hello world");
    editor.set_selection(6..11);

    editor.paste("there");

    assert_eq!(editor.text(), "hello there");
    assert_eq!(editor.selection(), Selection::caret(11));
}

#[test]
fn cut_returns_the_selection_and_nothing_for_a_caret() {
    let mut editor = editor("a👍🏽b");

    assert_eq!(editor.cut(), (None, Change::None));
    editor.set_selection(1..9);
    assert_eq!(editor.cut(), (Some("👍🏽".to_owned()), Change::Text));
    assert_eq!(editor.text(), "ab");
    assert_eq!(editor.selection(), Selection::caret(1));
}

#[test]
fn replacing_a_range_completes_a_token_and_puts_the_caret_behind_it() {
    // `applySuggestionToValue`: the active token "ca" becomes "cat".
    let mut editor = editor("dog ca bird");

    assert_eq!(editor.replace_range(4..6, "cat"), Change::Text);

    assert_eq!(editor.text(), "dog cat bird");
    assert_eq!(editor.selection(), Selection::caret(7));
    editor.undo();
    assert_eq!(editor.text(), "dog ca bird");
}

#[test]
fn setting_the_text_from_outside_moves_the_caret_to_the_end_and_drops_the_history() {
    let mut editor = LineEditor::new();
    type_chars(&mut editor, "draft");
    editor.move_to(2);

    assert_eq!(editor.set_text("draft"), Change::None);
    assert_eq!(editor.cursor(), 2);
    assert!(editor.can_undo());

    assert_eq!(editor.set_text(""), Change::Text);
    assert_eq!(editor.selection(), Selection::caret(0));
    assert!(!editor.can_undo());
    assert_eq!(editor.set_text("next"), Change::Text);
    assert_eq!(editor.selection(), Selection::caret(4));
}

fn digits_only(text: &str) -> bool {
    text.chars().all(|c| c.is_ascii_digit())
}

#[test]
fn an_edit_whose_result_is_not_accepted_does_not_happen() {
    let mut editor = editor("12");
    editor.set_accept(Some(digits_only));

    assert_eq!(editor.insert("x"), Change::None);
    assert_eq!(editor.paste("3 4"), Change::None);
    assert_eq!(editor.replace_range(0..1, "-"), Change::None);
    assert_eq!(editor.text(), "12");
    assert_eq!(editor.cursor(), 2);
    assert!(!editor.can_undo());

    assert_eq!(editor.insert("3"), Change::Text);
    assert_eq!(editor.paste("45"), Change::Text);
    assert_eq!(editor.text(), "12345");
}

#[test]
fn text_set_from_outside_is_not_filtered() {
    let mut editor = LineEditor::new();
    editor.set_accept(Some(digits_only));

    editor.set_text("n/a");

    assert_eq!(editor.text(), "n/a");
    // Removing the offending text is accepted once the rest passes.
    editor.select_all();
    assert_eq!(editor.cut().0.as_deref(), Some("n/a"));
}

// Input method

#[test]
fn a_composition_is_marked_until_it_is_committed() {
    let mut editor = editor("ab");
    editor.move_to(1);

    assert_eq!(editor.ime_compose(None, "k", None), Change::Text);
    assert_eq!(editor.text(), "akb");
    assert_eq!(editor.marked_range(), Some(1..2));
    assert!(editor.is_composing());
    editor.ime_compose(None, "か", None);
    assert_eq!(editor.text(), "aかb");
    assert_eq!(editor.marked_range(), Some(1..4));
    assert_eq!(editor.ime_marked_range(), Some(1..2));
    assert_eq!(editor.cursor(), 4);

    assert_eq!(editor.ime_commit(None, "蚊"), Change::Text);

    assert_eq!(editor.text(), "a蚊b");
    assert_eq!(editor.marked_range(), None);
    assert_eq!(editor.selection(), Selection::caret(4));
}

#[test]
fn a_whole_composition_is_one_undo_step() {
    let mut editor = LineEditor::new();
    type_chars(&mut editor, "x");
    for preedit in ["n", "に", "にh", "にほ", "にほn", "にほん"] {
        editor.ime_compose(None, preedit, None);
    }
    editor.ime_commit(None, "日本");

    assert_eq!(editor.text(), "x日本");
    editor.undo();
    assert_eq!(editor.text(), "x");
    editor.redo();
    assert_eq!(editor.text(), "x日本");
    editor.undo();
    editor.undo();
    assert_eq!(editor.text(), "");
    assert!(!editor.can_undo());
}

#[test]
fn a_composition_replaces_the_selection_it_started_on() {
    let mut editor = editor("hello");
    editor.set_selection(1..4);

    editor.ime_compose(None, "é", None);
    assert_eq!(editor.text(), "héo");
    editor.ime_commit(None, "é");

    assert_eq!(editor.text(), "héo");
    editor.undo();
    assert_eq!(editor.text(), "hello");
    assert_eq!(editor.selection(), selection(1, 4));
}

#[test]
fn the_selection_inside_composed_text_is_relative_to_it_and_in_utf16() {
    // The case of GPUI's example: the marked text "é🙂" replaces "🙂".
    let mut editor = editor("A🙂Z");

    editor.ime_compose(Some(1..3), "é🙂", Some(1..3));
    assert_eq!(editor.text(), "Aé🙂Z");
    assert_eq!(editor.marked_range(), Some(1..7));
    assert_eq!(editor.selected_range(), 3..7);

    editor.ime_compose(None, "漢", Some(1..1));
    assert_eq!(editor.text(), "A漢Z");
    assert_eq!(editor.marked_range(), Some(1..4));
    assert_eq!(editor.selection(), Selection::caret(4));

    // A selection past the composed text is cut to it.
    editor.ime_compose(None, "ab", Some(1..9));
    assert_eq!(editor.selected_range(), 2..3);
}

#[test]
fn an_emptied_composition_ends_without_an_undo_step() {
    let mut editor = editor("AZ");
    editor.move_to(1);

    editor.ime_compose(None, "漢", None);
    assert_eq!(editor.ime_compose(None, "", Some(0..0)), Change::Text);

    assert_eq!(editor.text(), "AZ");
    assert_eq!(editor.marked_range(), None);
    assert_eq!(editor.selection(), Selection::caret(1));
    assert!(!editor.is_composing());
    assert!(!editor.can_undo());
}

#[test]
fn unmarking_keeps_the_composed_text_as_final() {
    let mut editor = LineEditor::new();
    editor.ime_compose(None, "´", None);

    assert_eq!(editor.ime_unmark(), Change::Selection);

    assert_eq!(editor.text(), "´");
    assert_eq!(editor.marked_range(), None);
    assert_eq!(editor.ime_unmark(), Change::None);
    editor.undo();
    assert_eq!(editor.text(), "");
}

#[test]
fn an_edit_during_a_composition_finishes_it_first() {
    let mut editor = editor("ab");
    editor.ime_compose(None, "ｋ", None);

    assert_eq!(editor.apply(Edit::DeleteBackward), Change::Text);

    assert_eq!(editor.text(), "ab");
    assert_eq!(editor.marked_range(), None);
    assert!(!editor.is_composing());
    // The composition and the deletion are two undo steps.
    editor.undo();
    assert_eq!(editor.text(), "abｋ");
    editor.undo();
    assert_eq!(editor.text(), "ab");
}

#[test]
fn moving_the_caret_during_a_composition_leaves_it_to_the_input_method() {
    // A click into the field while composing: the input method does not know, and
    // its next update still replaces the text it marked.
    let mut editor = editor("ab");
    editor.ime_compose(None, "に", None);

    assert_eq!(editor.move_to(0), Change::Selection);
    editor.apply(Edit::MoveRight);
    editor.select_word_at(0);
    assert_eq!(editor.marked_range(), Some(2..5));
    assert!(editor.is_composing());

    editor.ime_compose(None, "にほ", None);
    assert_eq!(editor.text(), "abにほ");
    assert_eq!(editor.marked_range(), Some(2..8));
    editor.ime_commit(None, "日本");

    assert_eq!(editor.text(), "ab日本");
    assert_eq!(editor.selection(), Selection::caret(8));
    editor.undo();
    assert_eq!(editor.text(), "ab");
}

#[test]
fn a_dead_key_sequence_composes_and_commits_through_the_same_calls() {
    // X11 without an input method: the dead key is shown as preedit, the composed
    // character then arrives as ordinary typed text.
    let mut editor = editor("caf");

    editor.ime_compose(None, "´", None);
    assert_eq!(editor.text(), "caf´");
    editor.ime_commit(None, "é");

    assert_eq!(editor.text(), "café");
    assert_eq!(editor.cursor(), 5);
    editor.undo();
    assert_eq!(editor.text(), "caf");
}

#[test]
fn typed_text_arrives_as_a_commit_without_a_composition_and_merges_like_typing() {
    let mut editor = LineEditor::new();
    for c in ["t", "a", "g"] {
        assert_eq!(editor.ime_commit(None, c), Change::Text);
    }

    assert_eq!(editor.text(), "tag");
    editor.undo();
    assert_eq!(editor.text(), "");
}

#[test]
fn a_commit_can_name_the_range_it_replaces_in_utf16() {
    let mut editor = editor("e🙂漢");

    editor.ime_commit(Some(1..3), "!");

    assert_eq!(editor.text(), "e!漢");
    assert_eq!(editor.selection(), Selection::caret(2));
}

#[test]
fn a_composition_whose_result_is_not_accepted_leaves_the_field_as_it_was() {
    let mut editor = editor("12");
    editor.set_accept(Some(digits_only));
    editor.move_to(1);

    // Composed text is not checked while it is being composed.
    editor.ime_compose(None, "さん", None);
    assert_eq!(editor.text(), "1さん2");
    assert_eq!(editor.ime_commit(None, "三"), Change::Text);

    assert_eq!(editor.text(), "12");
    assert_eq!(editor.selection(), Selection::caret(1));
    assert!(!editor.can_undo());

    // The same when the composition is ended by another command.
    editor.ime_compose(None, "x", None);
    assert_eq!(editor.ime_unmark(), Change::Text);
    assert_eq!(editor.text(), "12");
    editor.ime_compose(None, "x", None);
    editor.insert("7");
    assert_eq!(editor.text(), "172");
    editor.undo();

    // A composition with an accepted result goes through.
    editor.move_to(1);
    editor.ime_compose(None, "３", None);
    editor.ime_commit(None, "3");
    assert_eq!(editor.text(), "132");
}

#[test]
fn a_cut_that_ends_a_refused_composition_reports_the_text_it_brought_back() {
    let mut editor = editor("12");
    editor.set_accept(Some(digits_only));
    editor.move_to(1);
    editor.ime_compose(None, "１", None);
    assert_eq!(editor.text(), "1１2");

    // Nothing is selected, so nothing is cut, but the composed text is gone.
    assert_eq!(editor.cut(), (None, Change::Text));

    assert_eq!(editor.text(), "12");
    assert_eq!(editor.selection(), Selection::caret(1));
    assert!(!editor.is_composing());
}

#[test]
fn the_platform_reads_and_sets_the_selection_in_utf16() {
    let mut editor = editor("e🙂漢");

    assert_eq!(editor.ime_len(), 4);
    assert_eq!(editor.ime_selection(), (4..4, false));
    assert_eq!(editor.ime_set_selection(1..3), Change::Selection);
    assert_eq!(editor.selected_range(), 1..5);
    assert_eq!(editor.ime_selection(), (1..3, false));
    editor.ime_set_selection(3..99);
    assert_eq!(editor.selected_range(), 5..8);

    editor.move_to(5);
    editor.apply(Edit::SelectLeft);
    assert_eq!(editor.ime_selection(), (1..3, true));
    assert_eq!(editor.ime_offset(5), 3);
    assert_eq!(editor.ime_byte_range(&(1..3)), 1..5);
}

#[test]
fn text_for_a_utf16_range_reports_the_range_it_covers() {
    let editor = editor("e🙂漢");

    assert_eq!(editor.ime_text_for_range(1..3), ("🙂", 1..3));
    // Ends inside the surrogate pair: the whole character is returned.
    assert_eq!(editor.ime_text_for_range(0..2), ("e🙂", 0..3));
    assert_eq!(editor.ime_text_for_range(3..99), ("漢", 3..4));
    assert_eq!(editor.ime_text_for_range(50..60), ("", 4..4));
}

#[test]
fn no_utf16_offset_can_make_the_buffer_slice_inside_a_character() {
    let text = format!("a\u{301}{FAMILY}漢🇵🇱");
    let units = LineEditor::with_text(&text).ime_len();

    for start in 0..=units + 2 {
        for end in 0..=units + 2 {
            let mut editor = LineEditor::with_text(&text);
            editor.ime_text_for_range(start..end);
            editor.ime_set_selection(start..end);
            editor.ime_compose(Some(start..end), "🙂é", Some(end..start));
            editor.ime_commit(None, "ok");

            let mut editor = LineEditor::with_text(&text);
            editor.ime_commit(Some(start..end), "🙂");
            assert!(editor.text().contains('🙂'));
        }
    }
}
