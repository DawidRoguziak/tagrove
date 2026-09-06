use crate::{
    error::AppResult,
    models::{ScanProgress, ThumbnailBatchItem},
};
use tauri::Emitter;

pub trait ProgressSink {
    fn progress(&self, progress: ScanProgress) -> AppResult<()>;
    fn thumbnail_ready(&self, _item: &ThumbnailBatchItem) -> AppResult<()> {
        Ok(())
    }
}
impl<R: tauri::Runtime> ProgressSink for tauri::AppHandle<R> {
    fn progress(&self, progress: ScanProgress) -> AppResult<()> {
        self.emit("process-progress", progress)
            .map_err(|e| e.to_string().into())
    }
    fn thumbnail_ready(&self, item: &ThumbnailBatchItem) -> AppResult<()> {
        self.emit("thumbnail-ready", item)
            .map_err(|e| e.to_string().into())
    }
}
impl<F: Fn(ScanProgress) -> AppResult<()>> ProgressSink for F {
    fn progress(&self, progress: ScanProgress) -> AppResult<()> {
        self(progress)
    }
}

pub fn emit_progress(
    sink: &impl ProgressSink,
    phase: &str,
    processed: usize,
    total: usize,
    message: String,
) -> AppResult<()> {
    sink.progress(ScanProgress {
        phase: phase.to_string(),
        processed,
        total,
        message,
    })
}
