use tauri::Emitter;

use crate::{error::AppResult, models::ScanProgress};

pub fn emit_progress(
    app: &tauri::AppHandle<impl tauri::Runtime>,
    phase: &str,
    processed: usize,
    total: usize,
    message: String,
) -> AppResult<()> {
    app.emit(
        "process-progress",
        ScanProgress {
            phase: phase.to_string(),
            processed,
            total,
            message,
        },
    )
    .map_err(|e| e.to_string().into())
}
