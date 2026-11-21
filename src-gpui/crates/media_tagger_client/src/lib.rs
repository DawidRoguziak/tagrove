//! Tagrove presentation logic without a GUI (ADR 0036).
//!
//! `media_tagger_gpui` renders what this crate decides; this crate never depends on
//! GPUI, wgpu or any other GUI runtime, and it reaches `media_tagger_core` only through
//! its `Backend` port (`backend/core_backend.rs` is the one file that names the core's
//! use cases). Both rules are enforced by `src-tauri/tests/architecture_boundaries.rs`.
//!
//! How the pieces fit (plan decisions D3 and D4):
//!
//! - [`backend`]: the `Backend` port, its adapter over the real core and a scriptable
//!   fake.
//! - [`model`]: the convention every model follows: input → state and effects, replies
//!   and timers identified by tokens, a `Driver` that runs effects on an executor.
//! - [`exec`]: the executor port with a thread pool for the application and a
//!   deterministic executor for tests.
//! - [`progress`], [`startup_scan`], [`thumbs`]: the progress stream, the single-flight
//!   startup scan and the thumbnail pipeline, which the Tauri adapter implements with
//!   its event, its async runtime and the `thumb://` scheme.
//! - [`i18n`], [`theme`], [`prefs`]: texts of the shared locale files, the theme
//!   tokens of `src/styles.css` as constants, and the stored theme and language.
//! - [`text`]: the editing model of the single-line text field, which GPUI does not
//!   ship: buffer, selection, input-method composition, undo, keys.
//! - [`controls`], [`icons`], [`overlay`]: the shared controls of `src/ui/controls`
//!   (their looks from the theme tokens, the slider and segmented-control keys), the
//!   icon catalogue, and the overlay layer: the layer stack behind Escape and focus
//!   restore, and the fading presence of a modal.

pub mod backend;
pub mod controls;
pub mod exec;
pub mod hello;
pub mod i18n;
pub mod icons;
pub mod kit;
pub mod model;
pub mod overlay;
pub mod prefs;
pub mod progress;
pub mod scenario;
pub mod startup_scan;
pub mod text;
pub mod theme;
pub mod thumbs;
