//! The shortcut registry of the application: its key contexts and the shortcuts bound
//! in them (ADR 0038, "GPUI port: keys and text fields").
//!
//! GPUI matches a key binding against the key contexts on the path from the focused
//! element to the window's root. The application nests them like this, outermost
//! first:
//!
//! - `UiLayers` (`ui::layers`): Escape, Tab and Shift+Tab of the overlay layer stack;
//! - [`KeyContext::Shell`]: the main window's content, whatever view it shows;
//! - [`KeyContext::Gallery`] or [`KeyContext::Settings`]: the view shown;
//! - the contexts of controls (`TextInput`, the slider, the segmented control).
//!
//! Rules every entry of [`SHORTCUTS`] keeps (tested): it has a context, never none,
//! and a key that types a character (a letter, a digit or Space, also with Shift) is
//! bound only outside a text field, which the adapter expresses with
//! `text_input::outside_text_input(context)`. There are no context menus: the Tauri
//! application suppresses its webview's, and the GPUI one draws none.
//!
//! The shell itself binds nothing: the Tauri shell has no key of its own besides
//! Escape, which belongs to the layer stack. The gallery binds the keys that scroll
//! its grid, which a webview's scroller handles natively (Home, End, Page Up, Page
//! Down, Up, Down), and Ctrl+A (or the platform key with A), which selects every
//! result while selection mode is on and no layer lies above the gallery; a text
//! field keeps its own Ctrl+A, since its context is deeper. The epics that bring the
//! search (Ctrl+K) and the lightbox add their shortcuts here.

/// A key context of the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyContext {
    /// The content of the main window, below the layer stack's root.
    Shell,
    /// The gallery view: header and grid.
    Gallery,
    /// The settings page.
    Settings,
}

impl KeyContext {
    pub const ALL: [Self; 3] = [Self::Shell, Self::Gallery, Self::Settings];

    /// The identifier GPUI's key dispatch matches.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Shell => "Shell",
            Self::Gallery => "Gallery",
            Self::Settings => "Settings",
        }
    }

    /// The context this one is nested in; `None` for the outermost of the
    /// application (which sits inside the layer stack's root).
    pub const fn parent(self) -> Option<Self> {
        match self {
            Self::Shell => None,
            Self::Gallery | Self::Settings => Some(Self::Shell),
        }
    }
}

/// What a shortcut asks for. Every command is handled in the context it is bound in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShortcutCommand {
    /// Scrolls the gallery grid to its start or end.
    GalleryScrollStart,
    GalleryScrollEnd,
    /// Scrolls the gallery grid by most of a viewport.
    GalleryPageUp,
    GalleryPageDown,
    /// Scrolls the gallery grid by a short step.
    GalleryLineUp,
    GalleryLineDown,
    /// Selects every result of the query, in selection mode only.
    GallerySelectAll,
}

/// One key binding of the application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shortcut {
    /// GPUI keystroke notation (`ctrl-k`, `shift-f`).
    pub keystroke: &'static str,
    pub command: ShortcutCommand,
    pub context: KeyContext,
}

/// Every application shortcut.
pub const SHORTCUTS: &[Shortcut] = &[
    Shortcut {
        keystroke: "home",
        command: ShortcutCommand::GalleryScrollStart,
        context: KeyContext::Gallery,
    },
    Shortcut {
        keystroke: "end",
        command: ShortcutCommand::GalleryScrollEnd,
        context: KeyContext::Gallery,
    },
    Shortcut {
        keystroke: "pageup",
        command: ShortcutCommand::GalleryPageUp,
        context: KeyContext::Gallery,
    },
    Shortcut {
        keystroke: "pagedown",
        command: ShortcutCommand::GalleryPageDown,
        context: KeyContext::Gallery,
    },
    Shortcut {
        keystroke: "up",
        command: ShortcutCommand::GalleryLineUp,
        context: KeyContext::Gallery,
    },
    Shortcut {
        keystroke: "down",
        command: ShortcutCommand::GalleryLineDown,
        context: KeyContext::Gallery,
    },
    Shortcut {
        keystroke: "ctrl-a",
        command: ShortcutCommand::GallerySelectAll,
        context: KeyContext::Gallery,
    },
    // `event.ctrlKey || event.metaKey` of the Solid gallery.
    Shortcut {
        keystroke: "cmd-a",
        command: ShortcutCommand::GallerySelectAll,
        context: KeyContext::Gallery,
    },
];

/// Whether `keystroke` types a character in a text field: one printable key, or
/// Space, with no modifier other than Shift.
pub fn types_a_character(keystroke: &str) -> bool {
    let mut parts: Vec<&str> = keystroke.split('-').collect();
    // A trailing "-" is the minus key itself: "ctrl--" splits into ["ctrl", "", ""].
    if keystroke.ends_with("--") || keystroke == "-" {
        parts.truncate(parts.len().saturating_sub(2));
        parts.push("-");
    }
    let Some((key, modifiers)) = parts.split_last() else {
        return false;
    };
    let only_shift = modifiers.iter().all(|modifier| *modifier == "shift");
    let printable = *key == "space" || key.chars().count() == 1;
    only_shift && printable
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{types_a_character, KeyContext, SHORTCUTS};

    #[test]
    fn contexts_nest_inside_the_shell_and_have_distinct_names() {
        let names: HashSet<_> = KeyContext::ALL.iter().map(|c| c.name()).collect();
        assert_eq!(names.len(), KeyContext::ALL.len());
        assert!(!names.contains("UiLayers") && !names.contains("TextInput"));
        for context in KeyContext::ALL {
            let mut depth = 0;
            let mut current = context;
            while let Some(parent) = current.parent() {
                current = parent;
                depth += 1;
                assert!(depth < 8, "the nesting has no cycle");
            }
            assert_eq!(current, KeyContext::Shell);
        }
    }

    #[test]
    fn every_shortcut_is_unique_in_its_context() {
        let mut seen = HashSet::new();
        for shortcut in SHORTCUTS {
            assert!(
                seen.insert((shortcut.keystroke, shortcut.context)),
                "{} is bound twice in {:?}",
                shortcut.keystroke,
                shortcut.context
            );
        }
    }

    #[test]
    fn typing_keys_are_recognised() {
        for key in ["f", "shift-f", "space", "shift-space", "1", "-", "shift--"] {
            assert!(types_a_character(key), "{key}");
        }
        for key in [
            "ctrl-k", "ctrl-a", "escape", "enter", "left", "alt-f", "ctrl--", "f5", "tab",
        ] {
            assert!(!types_a_character(key), "{key}");
        }
    }
}
