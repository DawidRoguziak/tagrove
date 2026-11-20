//! Tagrove presentation logic without a GUI (ADR 0036).
//!
//! `media_tagger_gpui` renders what this crate decides; this crate never depends on
//! GPUI, wgpu or any other GUI runtime, and it reaches `media_tagger_core` only through
//! its `Backend` port (`backend/core_backend.rs`, added by epic E02). Both rules are
//! enforced by `src-tauri/tests/architecture_boundaries.rs`.

pub mod hello;
pub mod i18n;
pub mod theme;
