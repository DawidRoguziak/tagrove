//! Which key does what in a text field. The GPUI adapter turns each entry into a key
//! binding in the field's key context; the table lives here so the keys are part of
//! the tested model and not of a view.

use super::editor::Edit;

/// What a key of the field asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Command {
    /// Changes the buffer only.
    Edit(Edit),
    /// Needs the clipboard, which the adapter owns.
    Copy,
    Cut,
    Paste,
    /// Arrow up and down. The owner of the field may take them first (to walk a
    /// suggestion list); otherwise they move the caret to the start and to the end, as
    /// in a single-line field of WebKit.
    Up,
    Down,
    /// Enter and Escape belong to the owner of the field: the field never acts on
    /// them, apart from ending a running composition.
    Confirm,
    Cancel,
}

impl Command {
    /// What the field does with the command when its owner did not take it.
    pub const fn fallback_edit(self) -> Option<Edit> {
        match self {
            Self::Edit(edit) => Some(edit),
            Self::Up => Some(Edit::MoveToStart),
            Self::Down => Some(Edit::MoveToEnd),
            Self::Copy | Self::Cut | Self::Paste | Self::Confirm | Self::Cancel => None,
        }
    }
}

/// The keys of a text field on Linux, as `modifier-key` strings in GPUI's keystroke
/// notation. Plain characters are not listed: they arrive as text input.
pub const KEYMAP: &[(&str, Command)] = &[
    ("left", Command::Edit(Edit::MoveLeft)),
    ("right", Command::Edit(Edit::MoveRight)),
    ("ctrl-left", Command::Edit(Edit::MoveWordLeft)),
    ("ctrl-right", Command::Edit(Edit::MoveWordRight)),
    ("home", Command::Edit(Edit::MoveToStart)),
    ("end", Command::Edit(Edit::MoveToEnd)),
    ("ctrl-home", Command::Edit(Edit::MoveToStart)),
    ("ctrl-end", Command::Edit(Edit::MoveToEnd)),
    ("shift-left", Command::Edit(Edit::SelectLeft)),
    ("shift-right", Command::Edit(Edit::SelectRight)),
    ("ctrl-shift-left", Command::Edit(Edit::SelectWordLeft)),
    ("ctrl-shift-right", Command::Edit(Edit::SelectWordRight)),
    ("shift-home", Command::Edit(Edit::SelectToStart)),
    ("shift-end", Command::Edit(Edit::SelectToEnd)),
    ("ctrl-shift-home", Command::Edit(Edit::SelectToStart)),
    ("ctrl-shift-end", Command::Edit(Edit::SelectToEnd)),
    ("shift-up", Command::Edit(Edit::SelectToStart)),
    ("shift-down", Command::Edit(Edit::SelectToEnd)),
    ("ctrl-a", Command::Edit(Edit::SelectAll)),
    ("backspace", Command::Edit(Edit::DeleteBackward)),
    ("shift-backspace", Command::Edit(Edit::DeleteBackward)),
    ("delete", Command::Edit(Edit::DeleteForward)),
    ("ctrl-backspace", Command::Edit(Edit::DeleteWordBackward)),
    ("ctrl-delete", Command::Edit(Edit::DeleteWordForward)),
    ("ctrl-z", Command::Edit(Edit::Undo)),
    ("ctrl-shift-z", Command::Edit(Edit::Redo)),
    ("ctrl-y", Command::Edit(Edit::Redo)),
    ("ctrl-c", Command::Copy),
    ("ctrl-insert", Command::Copy),
    ("ctrl-x", Command::Cut),
    ("shift-delete", Command::Cut),
    ("ctrl-v", Command::Paste),
    ("shift-insert", Command::Paste),
    ("up", Command::Up),
    ("down", Command::Down),
    ("enter", Command::Confirm),
    ("escape", Command::Cancel),
];

/// The command bound to `keystroke`, written as in [`KEYMAP`].
pub fn command_for(keystroke: &str) -> Option<Command> {
    KEYMAP
        .iter()
        .find_map(|(bound, command)| (*bound == keystroke).then_some(*command))
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{command_for, Command, KEYMAP};
    use crate::text::Edit;

    #[test]
    fn every_keystroke_is_bound_once() {
        let mut seen = HashSet::new();
        for (keystroke, _) in KEYMAP {
            assert!(seen.insert(keystroke), "{keystroke} is bound twice");
        }
    }

    #[test]
    fn keystrokes_are_lower_case_with_modifiers_in_gpui_order() {
        for (keystroke, _) in KEYMAP {
            let mut parts: Vec<&str> = keystroke.split('-').collect();
            let key = parts.pop().expect("a key");
            assert!(!key.is_empty() && key == key.to_lowercase(), "{keystroke}");
            assert!(
                matches!(
                    parts.as_slice(),
                    [] | ["ctrl"] | ["shift"] | ["ctrl", "shift"]
                ),
                "{keystroke}"
            );
        }
    }

    #[test]
    fn the_keys_of_the_acceptance_list_are_bound() {
        for (keystroke, command) in [
            ("left", Command::Edit(Edit::MoveLeft)),
            ("shift-right", Command::Edit(Edit::SelectRight)),
            ("ctrl-left", Command::Edit(Edit::MoveWordLeft)),
            ("ctrl-shift-right", Command::Edit(Edit::SelectWordRight)),
            ("home", Command::Edit(Edit::MoveToStart)),
            ("shift-end", Command::Edit(Edit::SelectToEnd)),
            ("ctrl-a", Command::Edit(Edit::SelectAll)),
            ("ctrl-z", Command::Edit(Edit::Undo)),
            ("ctrl-shift-z", Command::Edit(Edit::Redo)),
            ("ctrl-c", Command::Copy),
            ("ctrl-x", Command::Cut),
            ("ctrl-v", Command::Paste),
            ("enter", Command::Confirm),
            ("escape", Command::Cancel),
        ] {
            assert_eq!(command_for(keystroke), Some(command), "{keystroke}");
        }
        assert_eq!(command_for("ctrl-k"), None);
        assert_eq!(command_for("tab"), None);
    }

    #[test]
    fn only_owner_and_clipboard_commands_have_no_edit() {
        assert_eq!(Command::Up.fallback_edit(), Some(Edit::MoveToStart));
        assert_eq!(Command::Down.fallback_edit(), Some(Edit::MoveToEnd));
        assert_eq!(Command::Edit(Edit::Undo).fallback_edit(), Some(Edit::Undo));
        for command in [
            Command::Copy,
            Command::Cut,
            Command::Paste,
            Command::Confirm,
            Command::Cancel,
        ] {
            assert_eq!(command.fallback_edit(), None, "{command:?}");
        }
    }
}
