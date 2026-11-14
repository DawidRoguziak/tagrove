//! Use cases: the workflows behind the IPC commands, without Tauri.
//!
//! Each `commands/X.rs` handler parses its arguments, runs the matching
//! `usecases::X::<command>_service` on the blocking pool and converts the
//! result for the wire. Validation, the workflow locks of `app::locks` (always
//! taken inside the blocking task, in the order maintenance gate → `scan_lock`
//! → `thumb_lock`), SQL through `db` and progress through `ProgressSink` live
//! here. Rule 4 of `tests/architecture_boundaries.rs` keeps them out of
//! `commands/` (ADR 0034).

pub mod assets;
pub mod import_export;
pub mod scan;
pub mod thumbs;
