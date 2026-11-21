//! Behaviour of the single-line editing model, ported from the Tauri interface's tests
//! and fields (epic E04) and from the epic's acceptance criteria. It runs against the
//! public API of `media_tagger_client::text` only.
//!
//! - `src/features/search/suggestions/__tests__/inputService.test.ts`: a picked
//!   suggestion replaces the active token and the caret lands behind it.
//! - `LightboxDeleteConfirmDialog.tsx`: the field keeps what was typed; folding
//!   is the owner's.
//! - `MediaGroupSetter.tsx`: a paste whose result the filter refuses does not
//!   happen.
//! - Acceptance criteria: no caret position, selection or deletion splits an emoji, a
//!   ZWJ sequence, a flag, a combining sequence or a Hangul syllable; selection works
//!   in both directions; undo and redo; pasted line breaks.

use media_tagger_client::text::{Change, Edit, LineEditor, Selection};
use unicode_segmentation::UnicodeSegmentation;

/// Texts whose grapheme clusters span several characters.
const CLUSTERED: &[&str] = &[
    "a👨\u{200d}👩\u{200d}👧b",
    "🇵🇱🇩🇪x",
    // Three regional indicators: one flag and a lone indicator.
    "x🇵🇱🇩",
    "👍🏽 ok",
    "tag 👩🏾\u{200d}💻 dev",
    "e\u{301}e\u{301}\u{323}z",
    // A Hangul syllable written as conjoining jamo, then a precomposed one.
    "\u{1100}\u{1161}\u{11a8}한",
    "漢字 かな",
    "a👨\u{200d}👩\u{200d}👧🇵🇱e\u{301}漢",
];

const MOTIONS: &[Edit] = &[
    Edit::MoveLeft,
    Edit::MoveRight,
    Edit::MoveWordLeft,
    Edit::MoveWordRight,
    Edit::MoveToStart,
    Edit::MoveToEnd,
    Edit::SelectLeft,
    Edit::SelectRight,
    Edit::SelectWordLeft,
    Edit::SelectWordRight,
    Edit::SelectToStart,
    Edit::SelectToEnd,
    Edit::SelectAll,
];

const DELETIONS: &[Edit] = &[
    Edit::DeleteBackward,
    Edit::DeleteForward,
    Edit::DeleteWordBackward,
    Edit::DeleteWordForward,
];

/// The grapheme boundaries of `text`, its end included.
fn boundaries(text: &str) -> Vec<usize> {
    text.grapheme_indices(true)
        .map(|(start, _)| start)
        .chain([text.len()])
        .collect()
}

fn at(text: &str, caret: usize) -> LineEditor {
    let mut editor = LineEditor::with_text(text);
    editor.move_to(caret);
    assert_eq!(editor.selection(), Selection::caret(caret));
    editor
}

// inputService.test.ts

#[test]
fn a_suggestion_replaces_a_negative_token_and_the_caret_lands_behind_it() {
    // "replaces active token and keeps negative prefix": the token "-ca" at 4..7 of
    // "dog -ca bird" becomes "-cat" (the owner serializes the tag with its prefix).
    let mut editor = at("dog -ca bird", 6);

    assert_eq!(editor.replace_range(4..7, "-cat"), Change::Text);

    assert_eq!(editor.text(), "dog -cat bird");
    assert_eq!(editor.selection(), Selection::caret(8));
}

#[test]
fn a_suggestion_replaces_the_first_token_and_the_caret_lands_behind_it() {
    // "replaces active token without negative prefix".
    let mut editor = at("ca dog", 2);

    assert_eq!(editor.replace_range(0..2, "car"), Change::Text);

    assert_eq!(editor.text(), "car dog");
    assert_eq!(editor.selection(), Selection::caret(3));
}

#[test]
fn a_suggestion_replaces_its_token_wherever_the_selection_is_and_typing_goes_on_behind_it() {
    let mut editor = LineEditor::with_text("dog -ca bird");
    // The user selected "bird" before picking the suggestion for "-ca".
    editor.set_selection(8..12);

    editor.replace_range(4..7, "-cat");
    assert_eq!(editor.text(), "dog -cat bird");
    assert_eq!(editor.selection(), Selection::caret(8));

    // Typing goes on behind the completed tag, as its own undo step.
    assert_eq!(editor.insert("s"), Change::Text);
    assert_eq!(editor.text(), "dog -cats bird");
    editor.undo();
    assert_eq!(editor.text(), "dog -cat bird");
    editor.undo();
    assert_eq!(editor.text(), "dog -ca bird");
    editor.redo();
    assert_eq!(editor.text(), "dog -cat bird");
}

#[test]
fn a_suggestion_with_a_multibyte_tag_puts_the_caret_on_a_character_boundary() {
    // "ę" and "ż" are two bytes each: the token "ża" is 9..12.
    let mut editor = at("zdjęcia ża", 12);

    editor.replace_range(9..12, "żaba");

    assert_eq!(editor.text(), "zdjęcia żaba");
    assert_eq!(editor.cursor(), "zdjęcia żaba".len());
    assert_eq!(editor.ime_selection(), (12..12, false));
}

// LightboxDeleteConfirmDialog.tsx

#[test]
fn the_confirmation_field_keeps_case_and_surrounding_spaces_for_its_owner() {
    // The dialog trims and folds the typed word itself; the field must hand it over
    // as typed.
    let mut editor = LineEditor::new();
    for c in ["  ", "Y", "e", "S", " "] {
        editor.insert(c);
    }
    assert_eq!(editor.text(), "  YeS ");

    // A copied word usually ends in a line break: the field drops it.
    let mut pasted = LineEditor::new();
    pasted.paste("yes\n");
    assert_eq!(pasted.text(), "yes");
}

// MediaGroupSetter.tsx

fn digits_only(text: &str) -> bool {
    text.chars().all(|c| c.is_ascii_digit())
}

#[test]
fn a_refused_paste_into_the_order_field_leaves_text_and_selection_alone() {
    // `onPaste`: the pasted text replaces the selection, and the paste is prevented
    // when the result is not a valid order.
    let mut editor = LineEditor::with_text("12");
    editor.set_accept(Some(digits_only));
    editor.set_selection(0..1);

    assert_eq!(editor.paste("9a"), Change::None);
    assert_eq!(editor.text(), "12");
    assert_eq!(editor.selection(), Selection { anchor: 0, head: 1 });

    assert_eq!(editor.paste("9"), Change::Text);
    assert_eq!(editor.text(), "92");
    assert_eq!(editor.selection(), Selection::caret(1));
}

// Acceptance criteria: clusters

#[test]
fn no_motion_leaves_the_caret_or_the_anchor_inside_a_cluster() {
    for text in CLUSTERED {
        let stops = boundaries(text);
        for &start in &stops {
            for &first in MOTIONS {
                for &second in MOTIONS {
                    let mut editor = at(text, start);
                    editor.apply(first);
                    editor.apply(second);
                    let selection = editor.selection();
                    assert!(
                        stops.contains(&selection.anchor) && stops.contains(&selection.head),
                        "{text:?} from {start}: {first:?}, {second:?} -> {selection:?}"
                    );
                    assert_eq!(editor.text(), *text, "a motion changed the text");
                }
            }
        }
    }
}

#[test]
fn no_deletion_removes_part_of_a_cluster_and_undo_restores_it() {
    for text in CLUSTERED {
        let stops = boundaries(text);
        for &start in &stops {
            for &deletion in DELETIONS {
                let mut editor = at(text, start);
                let change = editor.apply(deletion);
                let after = editor.text().to_owned();
                let removed = text.len() - after.len();
                let caret = editor.cursor();
                if change == Change::None {
                    assert_eq!(after, *text);
                    continue;
                }
                assert!(removed > 0, "{text:?} from {start}: {deletion:?}");
                let end = caret + removed;
                assert!(
                    stops.contains(&caret) && stops.contains(&end),
                    "{text:?} from {start}: {deletion:?} removed {caret}..{end}"
                );
                assert_eq!(format!("{}{}", &text[..caret], &text[end..]), after);

                editor.undo();
                assert_eq!(editor.text(), *text);
                assert_eq!(editor.selection(), Selection::caret(start));
                editor.redo();
                assert_eq!(editor.text(), after);
            }
        }
    }
}

#[test]
fn selecting_cluster_by_cluster_to_the_left_takes_the_whole_text_reversed() {
    let text = "a👨\u{200d}👩\u{200d}👧🇵🇱e\u{301}漢";
    let clusters = text.graphemes(true).count();
    assert_eq!(clusters, 5);
    let mut editor = LineEditor::with_text(text);

    for _ in 0..clusters {
        assert_eq!(editor.apply(Edit::SelectLeft), Change::Selection);
    }
    assert_eq!(editor.apply(Edit::SelectLeft), Change::None);

    assert_eq!(editor.selected_text(), text);
    assert!(editor.selection().is_reversed());
    assert_eq!(
        editor.selection(),
        Selection {
            anchor: text.len(),
            head: 0
        }
    );
    // The platform sees the same selection in UTF-16, caret at its start.
    assert_eq!(editor.ime_selection(), (0..editor.ime_len(), true));
}

#[test]
fn a_selection_grown_one_way_shrinks_back_the_other_way() {
    for text in CLUSTERED {
        for &start in &boundaries(text) {
            // At an end of the text the first step has nowhere to go.
            if start == text.len() || start == 0 {
                continue;
            }
            let mut right = at(text, start);
            right.apply(Edit::SelectRight);
            right.apply(Edit::SelectLeft);
            assert_eq!(
                right.selection(),
                Selection::caret(start),
                "{text:?} {start}"
            );

            let mut left = at(text, start);
            left.apply(Edit::SelectLeft);
            left.apply(Edit::SelectRight);
            assert_eq!(
                left.selection(),
                Selection::caret(start),
                "{text:?} {start}"
            );
        }
    }
}

#[test]
fn every_cluster_boundary_survives_a_round_trip_through_utf16() {
    for text in CLUSTERED {
        let mut editor = LineEditor::with_text(text);
        for &stop in &boundaries(text) {
            let units = editor.ime_offset(stop);
            editor.ime_set_selection(units..units);
            assert_eq!(
                editor.selection(),
                Selection::caret(stop),
                "{text:?} {stop}"
            );
        }
    }
}

// Acceptance criteria: pasted line breaks, undo and redo

#[test]
fn a_multi_line_paste_into_the_middle_is_one_line_and_one_undo_step() {
    let mut editor = at("abcd", 2);

    assert_eq!(editor.paste("x\r\ny\nz\n\n"), Change::Text);

    assert_eq!(editor.text(), "abx y zcd");
    assert_eq!(editor.selection(), Selection::caret(7));
    editor.undo();
    assert_eq!(editor.text(), "abcd");
    assert_eq!(editor.selection(), Selection::caret(2));
    editor.redo();
    assert_eq!(editor.text(), "abx y zcd");
    assert_eq!(editor.selection(), Selection::caret(7));
}

#[test]
fn undo_brings_back_a_reversed_selection_that_typing_replaced() {
    let mut editor = LineEditor::with_text("one two");
    editor.apply(Edit::SelectWordLeft);
    let reversed = editor.selection();
    assert_eq!(reversed, Selection { anchor: 7, head: 4 });

    editor.insert("2");
    assert_eq!(editor.text(), "one 2");
    editor.undo();

    assert_eq!(editor.text(), "one two");
    assert_eq!(editor.selection(), reversed);
    editor.redo();
    assert_eq!(editor.text(), "one 2");
    assert!(!editor.can_redo());
}
