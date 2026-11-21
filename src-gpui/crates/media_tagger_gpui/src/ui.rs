//! Building blocks of the views: what GPUI does not ship and every screen needs.
//!
//! - [`theme`], [`fonts`], [`assets`]: the active theme as a global, the bundled fonts
//!   and the icon files.
//! - Controls of `src/ui/controls`: [`button`] (with icon buttons), [`chip`],
//!   [`alert`], [`progress`], [`slider`], [`segmented`], [`switch`], [`spinner`],
//!   [`icon`], and the text field of epic E04, [`text_input`]; the Tagrove [`logo`].
//! - The overlay layer of `src/ui/overlays`: [`layers`] (Escape, Tab, focus restore)
//!   and [`modal`].

pub mod alert;
pub mod assets;
pub mod button;
pub mod chip;
pub mod fonts;
pub mod icon;
pub mod layers;
pub mod logo;
pub mod modal;
pub mod progress;
pub mod segmented;
pub mod slider;
pub mod spinner;
pub mod switch;
pub mod text_input;
pub mod theme;

use gpui::App;

/// Sets up the building blocks: the theme global, the layer stack and the key bindings
/// of every control. Call once at startup, before a window opens. Every binding has a
/// key context (ADR 0038); the layer stack's bindings sit at the root of the window,
/// every control's deeper.
pub fn init(cx: &mut App) {
    theme::init(cx);
    layers::init(cx);
    text_input::init(cx);
    slider::init(cx);
    segmented::init(cx);
}
