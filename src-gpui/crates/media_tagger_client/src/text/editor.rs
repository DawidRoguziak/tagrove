//! The buffer of one single-line field: text, selection, composition and history.

use std::ops::Range;

use super::{
    boundary::{
        floor_char_boundary, nearest_grapheme, next_grapheme, next_word_end, prev_grapheme,
        prev_word_start, word_range_at,
    },
    history::{EditKind, History, Snapshot},
    sanitize::single_line,
    utf16,
};

/// A selection as the user made it: it starts at `anchor` and the caret is at `head`.
/// Both are UTF-8 byte offsets on character boundaries; an empty selection is a caret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Selection {
    pub anchor: usize,
    pub head: usize,
}

impl Selection {
    pub const fn caret(offset: usize) -> Self {
        Self {
            anchor: offset,
            head: offset,
        }
    }

    /// The selected bytes, start before end whichever way the selection was made.
    pub fn range(self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }

    pub const fn is_empty(self) -> bool {
        self.anchor == self.head
    }

    /// The caret is at the start of the selection (it was made right to left).
    pub const fn is_reversed(self) -> bool {
        self.head < self.anchor
    }
}

/// What an operation changed, so the owner knows what to republish.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Change {
    /// Nothing: Backspace at the start, a rejected edit, a caret already in place.
    None,
    /// The selection or the caret only.
    Selection,
    /// The text, and with it possibly the selection.
    Text,
}

/// An editing command that needs nothing but the buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Edit {
    MoveLeft,
    MoveRight,
    MoveWordLeft,
    MoveWordRight,
    MoveToStart,
    MoveToEnd,
    SelectLeft,
    SelectRight,
    SelectWordLeft,
    SelectWordRight,
    SelectToStart,
    SelectToEnd,
    SelectAll,
    DeleteBackward,
    DeleteForward,
    DeleteWordBackward,
    DeleteWordForward,
    Undo,
    Redo,
}

impl Edit {
    /// The command moves the caret or the selection and leaves the text alone.
    pub const fn is_motion(self) -> bool {
        matches!(
            self,
            Self::MoveLeft
                | Self::MoveRight
                | Self::MoveWordLeft
                | Self::MoveWordRight
                | Self::MoveToStart
                | Self::MoveToEnd
                | Self::SelectLeft
                | Self::SelectRight
                | Self::SelectWordLeft
                | Self::SelectWordRight
                | Self::SelectToStart
                | Self::SelectToEnd
                | Self::SelectAll
        )
    }
}

/// The editing model of a single-line text field, without any GUI.
///
/// Offsets are UTF-8 byte offsets. Every method accepts any offset and moves it to a
/// character boundary inside the text first, so no caller can make the buffer slice
/// inside a character. The caret moves by extended grapheme clusters and by words
/// (see [`super::boundary`]); inserted text is reduced to one line
/// ([`super::sanitize::single_line`]).
///
/// The `ime_*` methods are the buffer's side of GPUI's input handler and take UTF-16
/// offsets, as the platform sends them.
#[derive(Debug, Clone, Default)]
pub struct LineEditor {
    text: String,
    selection: Selection,
    /// The text an input method is still composing; it is part of `text`.
    marked: Option<Range<usize>>,
    /// The state before the running composition, which one undo step returns to.
    composition_base: Option<Snapshot>,
    history: History,
    /// Whether the last typed text was whitespace: a word typed after it starts a new
    /// undo step.
    typed_whitespace: bool,
    accept: Option<fn(&str) -> bool>,
}

impl LineEditor {
    pub fn new() -> Self {
        Self::default()
    }

    /// A buffer holding `text` (reduced to one line) with the caret at its end.
    pub fn with_text(text: &str) -> Self {
        let mut editor = Self::new();
        editor.set_text(text);
        editor
    }

    /// Restricts what the user can enter: an edit whose resulting text `accept`
    /// refuses does not happen (the Tauri interface does this for the group order,
    /// which takes digits only). Text set with [`Self::set_text`] is not checked, and a
    /// composition is checked when it ends.
    pub fn set_accept(&mut self, accept: Option<fn(&str) -> bool>) {
        self.accept = accept;
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn selection(&self) -> Selection {
        self.selection
    }

    /// The caret: the moving end of the selection.
    pub fn cursor(&self) -> usize {
        self.selection.head
    }

    pub fn selected_range(&self) -> Range<usize> {
        self.selection.range()
    }

    /// The selected text; empty for a caret.
    pub fn selected_text(&self) -> &str {
        &self.text[self.selection.range()]
    }

    /// The text an input method is composing, if any.
    pub fn marked_range(&self) -> Option<Range<usize>> {
        self.marked.clone()
    }

    pub fn is_composing(&self) -> bool {
        self.marked.is_some()
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// Replaces the whole text from outside (the owner cleared the field, applied a
    /// stored value). The caret goes to the end and the history is dropped, as a
    /// browser does for an assigned `value`. Setting the text the buffer already holds
    /// changes nothing, so an owner may echo every change back.
    pub fn set_text(&mut self, text: &str) -> Change {
        let text = single_line(text);
        if text == self.text {
            return Change::None;
        }
        self.text = text.into_owned();
        self.selection = Selection::caret(self.text.len());
        self.marked = None;
        self.composition_base = None;
        self.history.clear();
        Change::Text
    }

    // Selection
    //
    // Moving the caret does not end a composition: the input method still owns the
    // marked text and replaces it in place with its next update, wherever the caret
    // went (a click into the field while composing).

    /// Puts the caret at `offset`.
    pub fn move_to(&mut self, offset: usize) -> Change {
        let offset = floor_char_boundary(&self.text, offset);
        self.select(Selection::caret(offset))
    }

    /// Moves the caret to `offset` and keeps the other end of the selection (Shift
    /// with a click or a movement key, a drag).
    pub fn select_to(&mut self, offset: usize) -> Change {
        let head = floor_char_boundary(&self.text, offset);
        self.select(Selection {
            anchor: self.selection.anchor,
            head,
        })
    }

    /// Selects `range` with the caret at its end.
    pub fn set_selection(&mut self, range: Range<usize>) -> Change {
        let anchor = floor_char_boundary(&self.text, range.start);
        let head = floor_char_boundary(&self.text, range.end).max(anchor);
        self.select(Selection { anchor, head })
    }

    /// Selects the word segment at `offset` (a double click) and returns its range,
    /// which [`Self::extend_word_selection`] grows from.
    pub fn select_word_at(&mut self, offset: usize) -> Range<usize> {
        let word = word_range_at(&self.text, floor_char_boundary(&self.text, offset));
        self.select(Selection {
            anchor: word.start,
            head: word.end,
        });
        word
    }

    /// Extends a word selection that started at `origin` to the word at `offset` (a
    /// drag after a double click). The caret is on the side the pointer went to.
    pub fn extend_word_selection(&mut self, origin: Range<usize>, offset: usize) -> Change {
        let word = word_range_at(&self.text, floor_char_boundary(&self.text, offset));
        let origin_start = floor_char_boundary(&self.text, origin.start);
        let origin_end = floor_char_boundary(&self.text, origin.end).max(origin_start);
        let selection = if word.start < origin_start {
            Selection {
                anchor: origin_end,
                head: word.start,
            }
        } else {
            Selection {
                anchor: origin_start,
                head: word.end.max(origin_end),
            }
        };
        self.select(selection)
    }

    pub fn select_all(&mut self) -> Change {
        self.apply(Edit::SelectAll)
    }

    /// The grapheme boundary nearest to `offset`: where a click between two glyphs of
    /// one cluster puts the caret.
    pub fn nearest_boundary(&self, offset: usize) -> usize {
        nearest_grapheme(&self.text, offset)
    }

    // Editing

    /// Runs one command. A command that changes the text ends a running composition
    /// first, keeping what was composed.
    pub fn apply(&mut self, edit: Edit) -> Change {
        let finished = if edit.is_motion() {
            Change::None
        } else {
            self.finish_composition()
        };
        let head = self.selection.head;
        let range = self.selection.range();
        let text = &self.text;
        let change = match edit {
            Edit::MoveLeft if range.is_empty() => self.caret(prev_grapheme(text, head)),
            Edit::MoveLeft => self.caret(range.start),
            Edit::MoveRight if range.is_empty() => self.caret(next_grapheme(text, head)),
            Edit::MoveRight => self.caret(range.end),
            Edit::MoveWordLeft => self.caret(prev_word_start(text, head)),
            Edit::MoveWordRight => self.caret(next_word_end(text, head)),
            Edit::MoveToStart => self.caret(0),
            Edit::MoveToEnd => self.caret(text.len()),
            Edit::SelectLeft => self.head(prev_grapheme(text, head)),
            Edit::SelectRight => self.head(next_grapheme(text, head)),
            Edit::SelectWordLeft => self.head(prev_word_start(text, head)),
            Edit::SelectWordRight => self.head(next_word_end(text, head)),
            Edit::SelectToStart => self.head(0),
            Edit::SelectToEnd => self.head(text.len()),
            Edit::SelectAll => self.select(Selection {
                anchor: 0,
                head: text.len(),
            }),
            Edit::DeleteBackward if range.is_empty() => {
                let start = prev_grapheme(text, head);
                self.replace(start..head, "", EditKind::DeleteBackward, true)
            }
            Edit::DeleteForward if range.is_empty() => {
                let end = next_grapheme(text, head);
                self.replace(head..end, "", EditKind::DeleteForward, true)
            }
            Edit::DeleteWordBackward if range.is_empty() => {
                let start = prev_word_start(text, head);
                self.replace(start..head, "", EditKind::Other, false)
            }
            Edit::DeleteWordForward if range.is_empty() => {
                let end = next_word_end(text, head);
                self.replace(head..end, "", EditKind::Other, false)
            }
            Edit::DeleteBackward
            | Edit::DeleteForward
            | Edit::DeleteWordBackward
            | Edit::DeleteWordForward => self.replace(range, "", EditKind::Other, false),
            Edit::Undo => self.undo(),
            Edit::Redo => self.redo(),
        };
        finished.max(change)
    }

    /// Typed text: replaces the selection. Consecutive typing is one undo step, and a
    /// word typed after whitespace starts the next one.
    pub fn insert(&mut self, text: &str) -> Change {
        let finished = self.finish_composition();
        finished.max(self.type_text(self.selection.range(), text))
    }

    /// Pasted text: replaces the selection as one undo step of its own. Line breaks
    /// in it become spaces ([`single_line`]).
    pub fn paste(&mut self, text: &str) -> Change {
        let finished = self.finish_composition();
        let text = single_line(text);
        finished.max(self.replace(self.selection.range(), &text, EditKind::Other, false))
    }

    /// Removes the selected text and returns it for the clipboard, with what the call
    /// changed. Nothing is cut for a caret, and when the remaining text is not
    /// accepted; the text can have changed all the same, when the call ended a
    /// composition whose result is not accepted.
    pub fn cut(&mut self) -> (Option<String>, Change) {
        let finished = self.finish_composition();
        let range = self.selection.range();
        let cut = self.text[range.clone()].to_owned();
        let change = self.replace(range, "", EditKind::Other, false);
        (
            (change == Change::Text).then_some(cut),
            finished.max(change),
        )
    }

    /// Replaces `range` with `text` as one undo step and puts the caret behind it (the
    /// owner completes a token from a suggestion).
    pub fn replace_range(&mut self, range: Range<usize>, text: &str) -> Change {
        let finished = self.finish_composition();
        let start = floor_char_boundary(&self.text, range.start);
        let end = floor_char_boundary(&self.text, range.end).max(start);
        let text = single_line(text);
        finished.max(self.replace(start..end, &text, EditKind::Other, false))
    }

    pub fn undo(&mut self) -> Change {
        let finished = self.finish_composition();
        let current = self.snapshot();
        finished.max(match self.history.undo(current) {
            Some(previous) => self.restore(previous),
            None => Change::None,
        })
    }

    pub fn redo(&mut self) -> Change {
        let finished = self.finish_composition();
        let current = self.snapshot();
        finished.max(match self.history.redo(current) {
            Some(next) => self.restore(next),
            None => Change::None,
        })
    }

    // Input method (UTF-16 offsets)

    /// The length of the text in UTF-16 code units.
    pub fn ime_len(&self) -> usize {
        utf16::utf16_len(&self.text)
    }

    /// The selection in UTF-16 code units, and whether its caret is at the start.
    pub fn ime_selection(&self) -> (Range<usize>, bool) {
        (
            utf16::utf16_range(&self.text, &self.selection.range()),
            self.selection.is_reversed(),
        )
    }

    pub fn ime_marked_range(&self) -> Option<Range<usize>> {
        self.marked
            .as_ref()
            .map(|marked| utf16::utf16_range(&self.text, marked))
    }

    /// The text of a UTF-16 range and the range it really covers, which differs when
    /// the requested one ends past the text or inside a character.
    pub fn ime_text_for_range(&self, range_utf16: Range<usize>) -> (&str, Range<usize>) {
        let range = utf16::byte_range(&self.text, &range_utf16);
        let actual = utf16::utf16_range(&self.text, &range);
        (&self.text[range], actual)
    }

    /// The byte range of a UTF-16 range, for the bounds the platform asks for.
    pub fn ime_byte_range(&self, range_utf16: &Range<usize>) -> Range<usize> {
        utf16::byte_range(&self.text, range_utf16)
    }

    /// The UTF-16 offset of a byte offset, for the character index the platform asks
    /// for.
    pub fn ime_offset(&self, offset: usize) -> usize {
        utf16::utf16_offset(&self.text, offset)
    }

    /// The platform moved the selection. A running composition stays marked.
    pub fn ime_set_selection(&mut self, range_utf16: Range<usize>) -> Change {
        let range = utf16::byte_range(&self.text, &range_utf16);
        self.select(Selection {
            anchor: range.start,
            head: range.end,
        })
    }

    /// Final text from the platform: a typed character, or the result of a
    /// composition. It replaces `range_utf16`, or else the marked text, or else the
    /// selection.
    ///
    /// A composition ends here as one undo step back to the text before it began. When
    /// the result is not accepted ([`Self::set_accept`]) the field returns to that
    /// text.
    pub fn ime_commit(&mut self, range_utf16: Option<Range<usize>>, text: &str) -> Change {
        let explicit = range_utf16.map(|range| utf16::byte_range(&self.text, &range));
        let Some(base) = self.composition_base.take() else {
            return match explicit {
                Some(range) => {
                    let text = single_line(text);
                    self.replace(range, &text, EditKind::Other, false)
                }
                None => self.type_text(self.selection.range(), text),
            };
        };
        let range = explicit
            .or(self.marked.take())
            .unwrap_or(self.selection.range());
        self.marked = None;
        let text = single_line(text);
        let committed = splice(&self.text, &range, &text);
        if !self.accepts(&committed) {
            return self.restore(base);
        }
        self.text = committed;
        self.selection = Selection::caret(range.start + text.len());
        if self.text != base.text {
            self.history.record(EditKind::Other, false, base);
        }
        Change::Text
    }

    /// Text the platform is still composing: it replaces `range_utf16`, or else the
    /// marked text, or else the selection, and becomes the marked text.
    /// `selected_utf16` is the selection inside the new text, in UTF-16 code units
    /// from its start; without it the caret goes behind the text.
    ///
    /// Composed text is not checked by [`Self::set_accept`] and makes no undo step
    /// until the composition ends.
    pub fn ime_compose(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        selected_utf16: Option<Range<usize>>,
    ) -> Change {
        let range = range_utf16
            .map(|range| utf16::byte_range(&self.text, &range))
            .or(self.marked.clone())
            .unwrap_or(self.selection.range());
        if self.composition_base.is_none() {
            self.composition_base = Some(self.snapshot());
            self.history.close_step();
        }
        let text = single_line(text);
        self.text = splice(&self.text, &range, &text);
        let end = range.start + text.len();
        self.selection = match selected_utf16 {
            Some(selected) => {
                let selected = utf16::byte_range(&text, &selected);
                Selection {
                    anchor: range.start + selected.start,
                    head: range.start + selected.end,
                }
            }
            None => Selection::caret(end),
        };
        if text.is_empty() {
            // The composition was emptied: it is over.
            self.marked = None;
            self.finish_composition();
        } else {
            self.marked = Some(range.start..end);
        }
        Change::Text
    }

    /// The platform ended the composition: what was composed stays as final text.
    pub fn ime_unmark(&mut self) -> Change {
        if self.composition_base.is_none() && self.marked.is_none() {
            return Change::None;
        }
        // The marked range disappears even when the text stays, and its underline
        // with it.
        self.finish_composition().max(Change::Selection)
    }

    // Internals

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            text: self.text.clone(),
            selection: self.selection,
        }
    }

    fn restore(&mut self, snapshot: Snapshot) -> Change {
        let change = if self.text != snapshot.text {
            Change::Text
        } else if self.selection != snapshot.selection {
            Change::Selection
        } else {
            Change::None
        };
        self.text = snapshot.text;
        self.selection = snapshot.selection;
        self.marked = None;
        change
    }

    fn accepts(&self, text: &str) -> bool {
        self.accept.is_none_or(|accept| accept(text))
    }

    /// Ends a composition, keeping its text: every change to the text that is not the
    /// input method's own does this first. Composed text that is not accepted is
    /// removed.
    fn finish_composition(&mut self) -> Change {
        self.marked = None;
        let Some(base) = self.composition_base.take() else {
            return Change::None;
        };
        if self.text == base.text {
            return Change::None;
        }
        if !self.accepts(&self.text) {
            return self.restore(base);
        }
        self.history.record(EditKind::Other, false, base);
        Change::None
    }

    fn select(&mut self, selection: Selection) -> Change {
        self.history.close_step();
        if selection == self.selection {
            return Change::None;
        }
        self.selection = selection;
        Change::Selection
    }

    fn caret(&mut self, offset: usize) -> Change {
        self.select(Selection::caret(offset))
    }

    fn head(&mut self, head: usize) -> Change {
        self.select(Selection {
            anchor: self.selection.anchor,
            head,
        })
    }

    fn type_text(&mut self, range: Range<usize>, text: &str) -> Change {
        // A key that produces only a control character (Tab, Enter where the platform
        // reports them as text) types nothing; in pasted text a tab is a space.
        if !text.is_empty() && text.chars().all(char::is_control) {
            return Change::None;
        }
        let text = single_line(text);
        if text.is_empty() && range.is_empty() {
            return Change::None;
        }
        let whitespace = text.chars().all(char::is_whitespace);
        // Typing over a selection, and a word after whitespace, start a new undo step.
        let merge = range.is_empty() && (whitespace || !self.typed_whitespace);
        let change = self.replace(range, &text, EditKind::Typing, merge);
        if change == Change::Text {
            self.typed_whitespace = whitespace;
        }
        change
    }

    /// The one place that changes the text outside a composition. `range` is on
    /// character boundaries.
    fn replace(&mut self, range: Range<usize>, text: &str, kind: EditKind, merge: bool) -> Change {
        if range.is_empty() && text.is_empty() {
            return Change::None;
        }
        let replaced = splice(&self.text, &range, text);
        if !self.accepts(&replaced) {
            return Change::None;
        }
        let before = self.snapshot();
        self.history.record(kind, merge, before);
        self.text = replaced;
        self.selection = Selection::caret(range.start + text.len());
        Change::Text
    }
}

fn splice(text: &str, range: &Range<usize>, replacement: &str) -> String {
    let mut spliced = String::with_capacity(text.len() - range.len() + replacement.len());
    spliced.push_str(&text[..range.start]);
    spliced.push_str(replacement);
    spliced.push_str(&text[range.end..]);
    spliced
}
