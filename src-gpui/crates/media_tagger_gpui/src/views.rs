//! Views draw what the models of `media_tagger_client` decide and hold no logic of
//! their own.
//!
//! - [`shell`]: the main window's root: layer stack, key context, current view, resize
//!   grips.
//! - [`gallery`], [`settings`]: the two views the runtime switches between.
//! - [`chrome`]: the window's own decorations (buttons, grips, drag surface).
//! - [`kit`]: the board of every shared control (`--scenario kit`).

pub mod chrome;
pub mod gallery;
pub mod kit;
pub mod settings;
pub mod shell;

use gpui::App;

/// Binds the keys of the views. Call once at startup, after `ui::init`.
pub fn init(cx: &mut App) {
    gallery::init(cx);
}
