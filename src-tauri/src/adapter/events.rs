//! Application events: the Tauri implementation of the core's progress port.

use tauri::{AppHandle, Emitter, Runtime};

use media_tagger_core::{error::AppResult, models::ScanProgress, services::progress::ProgressSink};

/// Emits core progress as the `process-progress` event. A newtype, because the
/// orphan rule forbids implementing the core's `ProgressSink` for `AppHandle`.
pub struct TauriProgress<'a, R: Runtime>(pub &'a AppHandle<R>);

impl<R: Runtime> ProgressSink for TauriProgress<'_, R> {
    fn progress(&self, progress: ScanProgress) -> AppResult<()> {
        self.0
            .emit("process-progress", progress)
            .map_err(|e| e.to_string().into())
    }
}
