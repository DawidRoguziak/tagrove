//! Application events: the Tauri implementation of the core's progress port.

use tauri::Emitter;

use crate::{error::AppResult, models::ScanProgress, services::progress::ProgressSink};

impl<R: tauri::Runtime> ProgressSink for tauri::AppHandle<R> {
    fn progress(&self, progress: ScanProgress) -> AppResult<()> {
        self.emit("process-progress", progress)
            .map_err(|e| e.to_string().into())
    }
}
