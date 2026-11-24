//! Import and export (`createImportExportSettings.ts`): the file dialogs an operation
//! asks the adapter for, the default names of exported files, and what the operations
//! report.
//!
//! The model never opens a dialog: it leaves a [`PathDialogRequest`] for the adapter
//! (`Settings::take_path_dialog_request`) and waits for
//! `SettingsInput::PathPicked`. A dialog that is cancelled ends its operation with a
//! cancellation line and without a backend call.

use media_tagger_core::models::{
    CsvImportSummary, DbBundleExportSummary, DbBundleImportSummary, DbBundleInspection,
};

use super::model::OperationId;
use crate::i18n::I18n;

/// Which dialog an operation waits for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathDialogKind {
    /// A save dialog. The suggested name is `<prefix>-<timestamp>.<extension>`
    /// ([`export_file_name`]); `extension` is also the dialog's file filter.
    SaveFile {
        prefix: &'static str,
        extension: &'static str,
    },
    /// One existing file with this extension.
    OpenFile { extension: &'static str },
    /// One existing folder.
    OpenFolder,
}

/// The title of a dialog, translated when the dialog opens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogTitle {
    Key(&'static str),
    /// The target folder of one root of a backup: "Choose folder: <root>".
    MapRoot(String),
}

impl DialogTitle {
    pub fn resolve(&self, i18n: &I18n<'_>) -> String {
        match self {
            Self::Key(key) => i18n.t(key),
            Self::MapRoot(root) => format!("{}: {root}", i18n.t("settings.scan.chooseFolder")),
        }
    }
}

/// A file or folder dialog an operation waits for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathDialogRequest {
    pub op: OperationId,
    pub kind: PathDialogKind,
    pub title: DialogTitle,
}

/// The default name of a CSV export and of a backup archive.
pub const CSV_EXPORT_PREFIX: &str = "tags-export";
pub const BUNDLE_EXPORT_PREFIX: &str = "media-backup";

/// `formatExportTimestamp`: `YYYY-MM-DD_HH-mm`.
pub fn export_timestamp(year: i64, month: u32, day: u32, hours: u32, minutes: u32) -> String {
    format!("{year:04}-{month:02}-{day:02}_{hours:02}-{minutes:02}")
}

/// Whether `text` has the form of [`export_timestamp`]: the adapter checks what
/// its clock gave before it puts it into a file name.
pub fn is_export_timestamp(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 16
        && bytes.iter().enumerate().all(|(index, byte)| match index {
            4 | 7 | 13 => *byte == b'-',
            10 => *byte == b'_',
            _ => byte.is_ascii_digit(),
        })
}

/// The timestamp of `unix_seconds` in UTC, for an adapter that cannot read the local
/// time.
pub fn utc_export_timestamp(unix_seconds: u64) -> String {
    let days = i64::try_from(unix_seconds / 86_400).unwrap_or(0);
    let second_of_day = unix_seconds % 86_400;
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    export_timestamp(
        year,
        month as u32,
        day as u32,
        (second_of_day / 3_600) as u32,
        (second_of_day % 3_600 / 60) as u32,
    )
}

/// `buildExportFileName`.
pub fn export_file_name(prefix: &str, timestamp: &str, extension: &str) -> String {
    format!("{prefix}-{timestamp}.{extension}")
}

/// What a CSV import did (`CsvImportSummary`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CsvImported {
    pub rows_read: usize,
    pub rows_applied: usize,
    pub assets_matched: usize,
    pub assets_updated: usize,
}

impl From<&CsvImportSummary> for CsvImported {
    fn from(summary: &CsvImportSummary) -> Self {
        Self {
            rows_read: summary.rows_read,
            rows_applied: summary.rows_applied,
            assets_matched: summary.assets_matched,
            assets_updated: summary.assets_updated,
        }
    }
}

/// The files and thumbnails a backup export copied or a restore brought back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BundleCounts {
    pub files: usize,
    pub thumbnails: usize,
}

impl From<&DbBundleExportSummary> for BundleCounts {
    fn from(summary: &DbBundleExportSummary) -> Self {
        Self {
            files: summary.copied_files,
            thumbnails: summary.copied_thumbnails,
        }
    }
}

impl From<&DbBundleImportSummary> for BundleCounts {
    fn from(summary: &DbBundleImportSummary) -> Self {
        Self {
            files: summary.restored_files,
            thumbnails: summary.restored_thumbnails,
        }
    }
}

/// What a restore needs to know of an archive before it asks for confirmation
/// (`DbBundleInspection`): the scan roots it holds, and whether each needs a folder
/// on this machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleInspection {
    pub roots: Vec<String>,
    pub requires_mapping: bool,
}

impl From<DbBundleInspection> for BundleInspection {
    fn from(inspection: DbBundleInspection) -> Self {
        Self {
            roots: inspection.roots,
            requires_mapping: inspection.requires_mapping,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Language;

    #[test]
    fn an_export_is_named_after_its_minute() {
        let timestamp = export_timestamp(2026, 3, 7, 9, 5);
        assert_eq!(timestamp, "2026-03-07_09-05");
        assert!(is_export_timestamp(&timestamp));
        assert_eq!(
            export_file_name(CSV_EXPORT_PREFIX, &timestamp, "csv"),
            "tags-export-2026-03-07_09-05.csv"
        );
        assert_eq!(
            export_file_name(BUNDLE_EXPORT_PREFIX, &timestamp, "zip"),
            "media-backup-2026-03-07_09-05.zip"
        );
        for other in ["", "2026-03-07 09:05", "2026-03-07_09-05\n", "2026-3-7_9-5"] {
            assert!(!is_export_timestamp(other), "{other:?}");
        }
    }

    #[test]
    fn utc_timestamps_follow_the_calendar() {
        assert_eq!(utc_export_timestamp(0), "1970-01-01_00-00");
        // 2020-02-29T23:59:59Z and the second after it.
        assert_eq!(utc_export_timestamp(1_583_020_799), "2020-02-29_23-59");
        assert_eq!(utc_export_timestamp(1_583_020_800), "2020-03-01_00-00");
        // 2026-11-24T12:34:00Z.
        assert_eq!(utc_export_timestamp(1_795_523_640), "2026-11-24_12-34");
    }

    #[test]
    fn a_mapping_dialog_names_the_root_it_asks_for() {
        let i18n = I18n::new(Language::En);
        assert_eq!(
            DialogTitle::Key("settings.actions.dialogs.selectDbArchive").resolve(&i18n),
            "Select DB backup archive"
        );
        assert_eq!(
            DialogTitle::MapRoot("/media/photos".into()).resolve(&i18n),
            "Choose folder: /media/photos"
        );
    }
}
