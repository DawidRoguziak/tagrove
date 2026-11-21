//! The editing model of the single-line text field (epic E04). GPUI ships no text
//! field, so the port has its own: this module is everything about it that needs no
//! GUI, and `media_tagger_gpui::ui::text_input` draws it and feeds it input.
//!
//! - [`LineEditor`]: the buffer with its selection, the composition of an input
//!   method, the undo history and an optional filter for what may be entered.
//! - [`boundary`]: where the caret may stand (extended grapheme clusters) and where
//!   words start and end.
//! - [`utf16`]: UTF-8 byte offsets of the buffer ↔ UTF-16 offsets of the platform's
//!   input protocol. Every conversion ends on a character boundary.
//! - [`single_line`]: what the field keeps of inserted text (no line breaks).
//! - [`KEYMAP`], [`Command`]: which key does what.
//! - [`scroll_to_reveal`]: the horizontal shift that keeps the caret visible.
//! - [`style_runs`], [`field_visuals`]: how the line and its frame are drawn.

pub mod boundary;
pub mod utf16;

mod editor;
mod history;
mod keymap;
mod runs;
mod sanitize;
mod scroll;
mod visuals;

#[cfg(test)]
mod tests;

pub use editor::{Change, Edit, LineEditor, Selection};
pub use history::MAX_UNDO_STEPS;
pub use keymap::{command_for, Command, KEYMAP};
pub use runs::{style_runs, RunSpec};
pub use sanitize::single_line;
pub use scroll::scroll_to_reveal;
pub use visuals::{field_visuals, FieldChrome, FieldState, FieldVisuals, CARET_WIDTH};
