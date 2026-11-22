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
//! - [`library`]: the library model. The query: one backend query session read in
//!   pages into a bounded sparse cache, with generations, stale recovery, ID ranges,
//!   timeline sections and the rule for when a metadata write restarts the query. The
//!   metadata: the complete tag list per asset, the per-asset write queue and the
//!   maintenance barrier, the details cache, the writes that publish only what the
//!   backend acknowledged, and the known tags.
//! - [`gallery`]: the gallery grid: the square-tile geometry, the scroll state with
//!   its logical `f64` offset and fast-scroll pause, which thumbnail each tile asks
//!   for, the bounded cache of decoded pictures, and the measures of tile and footer;
//!   and the bulk selection: ranges of the query session plus included and excluded
//!   IDs, resolved by the backend, with the pointer gestures of selection mode.
//! - [`i18n`], [`theme`], [`prefs`]: texts of the shared locale files, the theme
//!   tokens of `src/styles.css` as constants, and the stored theme and language.
//! - [`text`]: the editing model of the single-line text field, which GPUI does not
//!   ship: buffer, selection, input-method composition, undo, keys.
//! - [`controls`], [`icons`], [`overlay`]: the shared controls of `src/ui/controls`
//!   (their looks from the theme tokens, the slider and segmented-control keys), the
//!   icon catalogue, and the overlay layer: the layer stack behind Escape and focus
//!   restore, and the fading presence of a modal.
//! - [`search`]: the search header: the grammar of the search field, the draft and the
//!   applied filters, the tag suggestions matched in the background, and the tag list.
//! - [`tagging`]: the shared tag editor of the lightbox and the bulk dock.
//! - [`runtime`], [`shell`], [`window_chrome`], [`shortcuts`]: the application runtime
//!   (models for the application's lifetime, the view, the start sequence), what the
//!   shell draws around the views, the window's own decorations and the registry of
//!   key contexts and shortcuts.

pub mod backend;
pub mod controls;
pub mod exec;
pub mod gallery;
pub mod i18n;
pub mod icons;
pub mod kit;
pub mod library;
pub mod model;
pub mod overlay;
pub mod prefs;
pub mod progress;
pub mod runtime;
pub mod scenario;
pub mod search;
pub mod shell;
pub mod shortcuts;
pub mod startup_scan;
pub mod tagging;
pub mod text;
pub mod theme;
pub mod thumbs;
pub mod window_chrome;
