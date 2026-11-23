//! Progress of settings operations (`progressService.ts`): which phases of the shared
//! progress stream belong to which operation, and how a phase reads.
//!
//! The stream is shared and uncorrelated (`docs/architecture/ipc-contract.md`): an
//! operation accepts only its own phases, and a translated phase name takes precedence
//! over the event's `message`.

use media_tagger_core::models::ScanProgress;

use crate::i18n::{I18n, Params};

/// The phases of a scan.
const SCAN_PHASES: [&str; 5] = ["counting", "scanning", "cleanup", "scan-empty", "scan-skip"];

/// Which phases an operation accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhaseFilter {
    /// `isScanProgressPhase`.
    Scan,
    /// `isThumbnailProgressPhase`: every phase starting with `thumbs`.
    Thumbnails,
    /// `isLibraryClearProgressPhase`: every phase starting with `library-clear`.
    LibraryClear,
    /// `isDuplicateScanProgressPhase`: every phase starting with `duplicates-scan`.
    DuplicateScan,
}

impl PhaseFilter {
    pub fn accepts(self, phase: &str) -> bool {
        match self {
            Self::Scan => is_scan_progress_phase(phase),
            Self::Thumbnails => is_thumbnail_progress_phase(phase),
            Self::LibraryClear => is_library_clear_progress_phase(phase),
            Self::DuplicateScan => is_duplicate_scan_progress_phase(phase),
        }
    }
}

pub fn is_scan_progress_phase(phase: &str) -> bool {
    SCAN_PHASES.contains(&phase)
}

pub fn is_thumbnail_progress_phase(phase: &str) -> bool {
    phase.starts_with("thumbs")
}

pub fn is_library_clear_progress_phase(phase: &str) -> bool {
    phase.starts_with("library-clear")
}

pub fn is_duplicate_scan_progress_phase(phase: &str) -> bool {
    phase.starts_with("duplicates-scan")
}

/// `formatProgressLabel`: `processed/total`, the label of the progress bar.
pub fn progress_label(progress: &ScanProgress) -> String {
    format!("{}/{}", progress.processed, progress.total)
}

/// The locale key of a known phase.
pub fn phase_key(phase: &str) -> Option<&'static str> {
    Some(match phase {
        "counting" => "settings.progress.counting",
        "scanning" => "settings.progress.scanning",
        "cleanup" => "settings.progress.cleanup",
        "scan-empty" => "settings.progress.scanEmpty",
        "scan-skip" => "settings.progress.scanSkip",
        "thumbs-start" => "settings.progress.thumbsStart",
        "thumbs" => "settings.progress.thumbs",
        "thumbs-done" => "settings.progress.thumbsDone",
        "thumbs-failed-start" => "settings.progress.thumbsFailedStart",
        "thumbs-failed" => "settings.progress.thumbsFailed",
        "thumbs-failed-done" => "settings.progress.thumbsFailedDone",
        "library-clear" => "settings.progress.libraryClear",
        "library-clear-done" => "settings.progress.libraryClearDone",
        "duplicates-scan" => "settings.progress.duplicatesScan",
        "duplicates-scan-done" => "settings.progress.duplicatesScanDone",
        _ => return None,
    })
}

/// `translateProgressMessage`: the text of a known phase with `processed` and
/// `total`, else the event's own message as backend text.
pub fn progress_message(progress: &ScanProgress, i18n: &I18n<'_>) -> String {
    match phase_key(&progress.phase) {
        Some(key) => i18n.t_with(
            key,
            &Params::new()
                .with("processed", progress.processed)
                .with("total", progress.total),
        ),
        None => i18n.backend_text(&progress.message),
    }
}

/// The counts of a bulk thumbnail render (`ThumbnailRenderSummary`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ThumbnailCounts {
    pub generated: usize,
    pub failed: usize,
    pub skipped_failed: usize,
    pub processed: usize,
    pub total: usize,
    pub cancelled: bool,
}

impl From<&media_tagger_core::models::ThumbnailRenderSummary> for ThumbnailCounts {
    fn from(summary: &media_tagger_core::models::ThumbnailRenderSummary) -> Self {
        Self {
            generated: summary.generated,
            failed: summary.failed,
            skipped_failed: summary.skipped_failed,
            processed: summary.processed,
            total: summary.total,
            cancelled: summary.cancelled,
        }
    }
}

fn count(value: usize) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

/// `formatThumbnailSummary`: `label. Ready: n, failed: n, skipped failed: n.` and
/// how far the render got.
pub fn thumbnail_summary(label: &str, counts: &ThumbnailCounts, i18n: &I18n<'_>) -> String {
    let base = format!(
        "{label}. {}, {}, {}",
        i18n.t_with(
            "settings.progress.thumbnailSummary.ready",
            &Params::new().with_count(count(counts.generated))
        ),
        i18n.t_with(
            "settings.progress.thumbnailSummary.failed",
            &Params::new().with_count(count(counts.failed))
        ),
        i18n.t_with(
            "settings.progress.thumbnailSummary.skippedFailed",
            &Params::new().with_count(count(counts.skipped_failed))
        ),
    );
    let reach = Params::new()
        .with("processed", counts.processed)
        .with("total", counts.total);
    let key = if counts.cancelled {
        "settings.progress.thumbnailSummary.cancelledAt"
    } else {
        "settings.progress.thumbnailSummary.processed"
    };
    format!("{base}. {}", i18n.t_with(key, &reach))
}

#[cfg(test)]
mod tests {
    use media_tagger_core::models::ScanProgress;

    use super::*;
    use crate::i18n::Language;

    fn progress(phase: &str, processed: usize, total: usize, message: &str) -> ScanProgress {
        ScanProgress {
            phase: phase.to_owned(),
            processed,
            total,
            message: message.to_owned(),
        }
    }

    #[test]
    fn phases_belong_to_their_operation() {
        for phase in SCAN_PHASES {
            assert!(PhaseFilter::Scan.accepts(phase), "{phase}");
            assert!(!PhaseFilter::Thumbnails.accepts(phase), "{phase}");
        }
        assert!(PhaseFilter::Thumbnails.accepts("thumbs-failed-done"));
        assert!(PhaseFilter::LibraryClear.accepts("library-clear-done"));
        assert!(PhaseFilter::DuplicateScan.accepts("duplicates-scan"));
        assert!(!PhaseFilter::Scan.accepts("thumbs"));
        assert!(!PhaseFilter::Scan.accepts("library-clear"));
    }

    #[test]
    fn a_known_phase_wins_over_the_message_and_others_read_as_backend_text() {
        let i18n = I18n::new(Language::En);
        assert_eq!(
            progress_message(&progress("scanning", 3, 9, "ignored"), &i18n),
            "Scanning 3/9"
        );
        assert_eq!(
            progress_message(&progress("other", 0, 0, "i18n:common.loading"), &i18n),
            "Loading..."
        );
        assert_eq!(progress_label(&progress("thumbs", 2, 5, "")), "2/5");
    }

    #[test]
    fn a_thumbnail_summary_says_how_far_the_render_got() {
        let i18n = I18n::new(Language::En);
        let mut counts = ThumbnailCounts {
            generated: 3,
            failed: 1,
            skipped_failed: 2,
            processed: 6,
            total: 6,
            cancelled: false,
        };
        assert_eq!(
            thumbnail_summary("All thumbnails", &counts, &i18n),
            "All thumbnails. Ready: 3, failed: 1, skipped failed: 2. Processed: 6/6."
        );
        counts.cancelled = true;
        counts.processed = 4;
        assert_eq!(
            thumbnail_summary("All thumbnails", &counts, &i18n),
            "All thumbnails. Ready: 3, failed: 1, skipped failed: 2. Cancelled at 4/6."
        );
    }
}
