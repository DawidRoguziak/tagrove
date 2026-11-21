//! Building blocks of the views: what GPUI does not ship and every screen needs.

pub mod text_input;

use gpui::App;

/// Registers the key bindings of the building blocks. Call once at startup, before a
/// window opens.
pub fn init(cx: &mut App) {
    text_input::init(cx);
}
