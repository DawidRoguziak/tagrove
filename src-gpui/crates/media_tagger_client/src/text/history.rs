//! The undo and redo stacks of one field.

use super::editor::Selection;

/// How many undo steps a field keeps; the oldest is dropped first.
pub const MAX_UNDO_STEPS: usize = 100;

/// Text and selection at one moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Snapshot {
    pub text: String,
    pub selection: Selection,
}

/// What kind of change an edit is. Consecutive edits of the same kind can share one
/// undo step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EditKind {
    Typing,
    DeleteBackward,
    DeleteForward,
    /// Never merged: paste, cut, word deletion, replacement of a selection, a
    /// finished composition.
    Other,
}

#[derive(Debug, Clone, Default)]
pub(super) struct History {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// The kind of the last edit while later edits may still join its undo step.
    open: Option<EditKind>,
}

impl History {
    /// Records the state before an edit. With `merge`, an edit of the same kind as
    /// the previous one joins its undo step instead of starting a new one.
    pub fn record(&mut self, kind: EditKind, merge: bool, before: Snapshot) {
        let joins = merge && kind != EditKind::Other && self.open == Some(kind);
        if !joins {
            if self.undo.len() == MAX_UNDO_STEPS {
                self.undo.remove(0);
            }
            self.undo.push(before);
        }
        self.redo.clear();
        self.open = Some(kind);
    }

    /// The next edit starts a new undo step (the caret moved, for example).
    pub fn close_step(&mut self) {
        self.open = None;
    }

    pub fn undo(&mut self, current: Snapshot) -> Option<Snapshot> {
        let previous = self.undo.pop()?;
        self.redo.push(current);
        self.open = None;
        Some(previous)
    }

    pub fn redo(&mut self, current: Snapshot) -> Option<Snapshot> {
        let next = self.redo.pop()?;
        self.undo.push(current);
        self.open = None;
        Some(next)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }
}
