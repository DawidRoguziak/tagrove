use crate::{error::AppResult, models::ScanProgress};

pub trait ProgressSink {
    fn progress(&self, progress: ScanProgress) -> AppResult<()>;
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
