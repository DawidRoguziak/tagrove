//! The status line of a settings section (`statusMessage.ts`,
//! `operationStateService.ts`, `SectionOperationStatus.tsx`).
//!
//! A message is stored as data and translated each time it is shown, so a pending
//! line, a progress line, a summary or an error follows a language switch.

use media_tagger_core::models::{ScanCompletion, ScanProgress, ScanSummary};

use super::{
    progress::{progress_message, thumbnail_summary, ThumbnailCounts},
    scan::folder_add_message,
};
use crate::i18n::{I18n, Params};

/// The sections with an operation status (`SettingsOperationSection`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationSection {
    Scan,
    ImportExport,
    Danger,
    Duplicates,
}

impl OperationSection {
    pub const ALL: [Self; 4] = [
        Self::Scan,
        Self::ImportExport,
        Self::Danger,
        Self::Duplicates,
    ];

    /// `SECTION_READY_MESSAGE_KEYS`: the line of a section with nothing running.
    pub const fn ready_key(self) -> &'static str {
        match self {
            Self::Scan => "settings.status.ready.scan",
            Self::ImportExport => "settings.status.ready.importExport",
            Self::Danger => "settings.status.ready.danger",
            Self::Duplicates => "settings.status.ready.duplicates",
        }
    }

    pub(crate) const fn index(self) -> usize {
        match self {
            Self::Scan => 0,
            Self::ImportExport => 1,
            Self::Danger => 2,
            Self::Duplicates => 3,
        }
    }
}

/// A progress event as a section keeps it: `ScanProgress` with equality.
#[derive(Debug, Clone)]
pub struct ProgressEvent(pub ScanProgress);

impl PartialEq for ProgressEvent {
    fn eq(&self, other: &Self) -> bool {
        let (a, b) = (&self.0, &other.0);
        a.phase == b.phase
            && a.processed == b.processed
            && a.total == b.total
            && a.message == b.message
    }
}

impl std::ops::Deref for ProgressEvent {
    type Target = ScanProgress;

    fn deref(&self) -> &ScanProgress {
        &self.0
    }
}

/// A status line, untranslated.
#[derive(Debug, Clone, PartialEq)]
pub enum StatusMessage {
    /// The section's ready line.
    Ready(OperationSection),
    /// Literal text (the startup scan without work leaves the line empty).
    Text(String),
    /// A locale key with its parameters.
    Key { key: &'static str, params: Params },
    /// The live line of a progress event.
    Progress(ProgressEvent),
    /// `formatScanSummary`.
    ScanSummary {
        partial: bool,
        indexed: usize,
        removed: usize,
        failed: usize,
    },
    /// `formatThumbnailSummary` with the locale key of its label.
    ThumbnailSummary {
        label_key: &'static str,
        counts: ThumbnailCounts,
    },
    /// `formatFolderAddMessage`.
    FoldersAdded {
        selected: Vec<String>,
        added: Vec<String>,
        skipped: usize,
    },
    /// `<prefix>: <detail>`. The detail is the backend's text, translated when it
    /// names a locale key (`translateBackendText`), unless `raw`.
    Failed {
        prefix: &'static str,
        detail: String,
        raw: bool,
    },
}

impl StatusMessage {
    /// A locale key without parameters.
    pub fn key(key: &'static str) -> Self {
        Self::Key {
            key,
            params: Params::new(),
        }
    }

    pub fn key_with(key: &'static str, params: Params) -> Self {
        Self::Key { key, params }
    }

    pub fn scan_summary(summary: &ScanSummary) -> Self {
        Self::ScanSummary {
            partial: matches!(summary.completion, ScanCompletion::Partial),
            indexed: summary.indexed,
            removed: summary.removed,
            failed: summary.failed,
        }
    }

    /// An error line whose detail is backend text.
    pub fn failed(prefix: &'static str, detail: impl Into<String>) -> Self {
        Self::Failed {
            prefix,
            detail: detail.into(),
            raw: false,
        }
    }

    /// The line in the language of `i18n`.
    pub fn resolve(&self, i18n: &I18n<'_>) -> String {
        match self {
            Self::Ready(section) => i18n.t(section.ready_key()),
            Self::Text(text) => text.clone(),
            Self::Key { key, params } => i18n.t_with(key, params),
            Self::Progress(progress) => progress_message(progress, i18n),
            Self::ScanSummary {
                partial,
                indexed,
                removed,
                failed,
            } => i18n.t_with(
                if *partial {
                    "settings.actions.summary.doneIndexedPartial"
                } else {
                    "settings.actions.summary.doneIndexed"
                },
                &Params::new()
                    .with("indexed", indexed)
                    .with("removed", removed)
                    .with("failed", failed),
            ),
            Self::ThumbnailSummary { label_key, counts } => {
                thumbnail_summary(&i18n.t(label_key), counts, i18n)
            }
            Self::FoldersAdded {
                selected,
                added,
                skipped,
            } => folder_add_message(i18n, selected, added, *skipped),
            Self::Failed {
                prefix,
                detail,
                raw,
            } => {
                let detail = if *raw {
                    detail.clone()
                } else {
                    i18n.backend_text(detail)
                };
                format!("{}: {detail}", i18n.t(prefix))
            }
        }
    }
}

/// What a section shows below its actions: a spinner while its operation runs, the
/// line, and a progress bar while progress is known (`SectionStatusState`).
#[derive(Debug, Clone, PartialEq)]
pub struct SectionStatus {
    pub loading: bool,
    pub message: StatusMessage,
    pub progress: Option<ProgressEvent>,
}

impl SectionStatus {
    /// `createSectionStatusState`: idle, with the ready line.
    pub fn ready(section: OperationSection) -> Self {
        Self {
            loading: false,
            message: StatusMessage::Ready(section),
            progress: None,
        }
    }
}

/// The progress bar of a status: `value` is `None` (indeterminate) while the total
/// is unknown, and `max` is at least 1 (`SectionOperationStatus.tsx`).
pub fn progress_bar(progress: &ScanProgress) -> (Option<usize>, usize) {
    let value = (progress.total > 0).then_some(progress.processed);
    (value, progress.total.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Language;

    #[test]
    fn every_section_starts_ready_and_its_line_follows_the_language() {
        for section in OperationSection::ALL {
            let status = SectionStatus::ready(section);
            assert!(!status.loading && status.progress.is_none());
            let english = status.message.resolve(&I18n::new(Language::En));
            let polish = status.message.resolve(&I18n::new(Language::Pl));
            assert!(!english.is_empty());
            assert_ne!(english, polish, "{section:?}");
        }
    }

    #[test]
    fn an_error_line_translates_a_backend_key_unless_it_is_raw() {
        let i18n = I18n::new(Language::En);
        assert_eq!(
            StatusMessage::failed(
                "settings.actions.errorPrefix.rescan",
                "i18n:common.libraryBusy"
            )
            .resolve(&i18n),
            "Rescan error: A scan or another library operation is running. Try again when it finishes."
        );
        assert_eq!(
            StatusMessage::Failed {
                prefix: "settings.actions.errorPrefix.cancelThumbRender",
                detail: "i18n:common.loading".into(),
                raw: true,
            }
            .resolve(&i18n),
            "Cancel thumbnail render error: i18n:common.loading"
        );
    }

    #[test]
    fn a_progress_bar_without_a_total_is_indeterminate() {
        let progress = |processed, total| ScanProgress {
            phase: "scanning".into(),
            processed,
            total,
            message: String::new(),
        };
        assert_eq!(progress_bar(&progress(3, 9)), (Some(3), 9));
        assert_eq!(progress_bar(&progress(0, 0)), (None, 1));
    }
}
