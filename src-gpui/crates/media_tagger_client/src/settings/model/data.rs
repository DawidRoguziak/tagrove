//! The operations of the import/export and duplicate sections
//! (`createImportExportSettings.ts`, `createDuplicateSettings.ts`) on the runner of
//! [`Settings`].
//!
//! # Import and export
//!
//! Each of the four actions takes the runner's lock and asks for a file dialog
//! ([`Settings::take_path_dialog_request`]). A cancelled dialog ends the operation
//! with a cancellation line and without a backend call.
//!
//! - **CSV export, backup export**: the chosen path goes to the backend; the summary
//!   is shown. Nothing is refreshed.
//! - **CSV import**: the chosen file is imported behind the mutation barrier of the
//!   library. Whatever the import answers, tag state and cached details are thrown
//!   away before the barrier comes down ([`LibraryReset::Identity`]) and the library
//!   is read again; the summary is shown only after a refresh that succeeded.
//! - **Restore**: two operations with a confirmation between them. The first chooses
//!   the archive, has the backend inspect it and, when the archive's roots need
//!   folders on this machine, asks for one folder per root (the section's line names
//!   the root each dialog asks about); it ends by storing the
//!   archive and the mappings ([`Settings::pending_import_db_source`]), which opens
//!   the overwrite confirmation, and releases the lock. The stored restore is given up
//!   by Cancel (Escape and a press beside the dialog are the same command), by the
//!   start of another choice of an archive and when the page is left
//!   ([`Settings::page_left`]). Confirming takes the stored
//!   values, so a second confirmation finds nothing, and runs the restore behind the
//!   mutation barrier; whatever it answers, identity and query state are thrown away
//!   before the barrier comes down ([`LibraryReset::Restored`]: a restore can fail
//!   after it swapped the live files), and the roots and the library are read again.
//!
//! # Duplicates
//!
//! "Start" opens the resolver and scans; "Rescan" scans. The scan's result replaces
//! the groups and advances [`Settings::duplicate_scan_generation`], on which the
//! dialog starts its form over. Saving sends one batch with the scan's revision and
//! the path and record version the scan saw of every changed asset. A batch with a
//! delete waits for its confirmation first. After the batch answered, the library is
//! read again and the duplicates are scanned again, both best effort, and then the
//! outcome is shown: committed (with the files that were already missing), rolled
//! back, or recovery required with the reported paths.

use std::collections::VecDeque;

use media_tagger_core::models::DbRootMapping;

use super::{
    Active, Kind, PendingBundleImport, Settings, SettingsCall, SettingsEffect, Settling, Stage,
    Start,
};
use crate::{
    backend::BackendResult,
    i18n::Params,
    library::{LibraryInput, LibraryReset},
    settings::{
        duplicates::{
            build_batch, AppliedDuplicates, AppliedStatus, DuplicateChange, DuplicateScan,
            PendingDuplicateDelete,
        },
        progress::PhaseFilter,
        status::{OperationSection, StatusMessage},
        transfer::{
            BundleCounts, BundleInspection, CsvImported, DialogTitle, PathDialogKind,
            PathDialogRequest, BUNDLE_EXPORT_PREFIX, CSV_EXPORT_PREFIX,
        },
        OperationId,
    },
};

const CSV: &str = "csv";
const ZIP: &str = "zip";
const DB_IMPORT_PENDING: &str = "settings.actions.pending.prepareDbImport";
const DB_IMPORT_ERROR: &str = "settings.actions.errorPrefix.importDb";
const DB_IMPORT_CANCELLED: &str = "settings.actions.summary.dbImportCancelled";

fn counts(pairs: &[(&str, usize)]) -> Params {
    pairs.iter().fold(Params::new(), |params, (name, value)| {
        params.with(*name, value)
    })
}

impl Settings {
    /// Begins an import/export operation that starts with a file dialog.
    fn begin_with_dialog(
        &mut self,
        kind: Kind,
        pending: &'static str,
        error_prefix: &'static str,
        dialog: PathDialogKind,
        title: &'static str,
    ) -> Vec<SettingsEffect> {
        let Some(op) = self.begin(Start {
            kind,
            section: OperationSection::ImportExport,
            pending: StatusMessage::key(pending),
            error_prefix,
            filter: None,
            global: true,
        }) else {
            return Vec::new();
        };
        self.ask_for_path(op, dialog, DialogTitle::Key(title));
        Vec::new()
    }

    fn ask_for_path(&mut self, op: OperationId, kind: PathDialogKind, title: DialogTitle) {
        self.stage(Stage::PathDialog);
        self.path_dialog_request = Some(PathDialogRequest { op, kind, title });
    }

    pub(super) fn export_csv(&mut self) -> Vec<SettingsEffect> {
        self.begin_with_dialog(
            Kind::ExportCsv,
            "settings.actions.pending.prepareCsvExport",
            "settings.actions.errorPrefix.exportCsv",
            PathDialogKind::SaveFile {
                prefix: CSV_EXPORT_PREFIX,
                extension: CSV,
            },
            "settings.actions.dialogs.exportTagsCsv",
        )
    }

    pub(super) fn import_csv(&mut self) -> Vec<SettingsEffect> {
        self.begin_with_dialog(
            Kind::ImportCsv { barrier: None },
            "settings.actions.pending.prepareCsvImport",
            "settings.actions.errorPrefix.importCsv",
            PathDialogKind::OpenFile { extension: CSV },
            "settings.actions.dialogs.importTagsCsv",
        )
    }

    pub(super) fn export_bundle(&mut self) -> Vec<SettingsEffect> {
        self.begin_with_dialog(
            Kind::ExportBundle,
            "settings.actions.pending.prepareDbExport",
            "settings.actions.errorPrefix.exportDb",
            PathDialogKind::SaveFile {
                prefix: BUNDLE_EXPORT_PREFIX,
                extension: ZIP,
            },
            "settings.actions.dialogs.exportDbArchive",
        )
    }

    /// "Import DB + thumbnails": the first of the restore's two operations. A choice
    /// that starts takes the place of a restore that still waited for its confirmation,
    /// at once and however the new choice ends, so the confirmation can only ever
    /// restore the archive chosen last.
    pub(super) fn pick_bundle(&mut self) -> Vec<SettingsEffect> {
        if self.is_locked() {
            return Vec::new();
        }
        self.pending_import = None;
        self.begin_with_dialog(
            Kind::PickBundle {
                source: None,
                roots: VecDeque::new(),
                mapping: None,
                mappings: Vec::new(),
            },
            DB_IMPORT_PENDING,
            DB_IMPORT_ERROR,
            PathDialogKind::OpenFile { extension: ZIP },
            "settings.actions.dialogs.selectDbArchive",
        )
    }

    pub(super) fn cancel_import_db_overwrite(&mut self) -> Vec<SettingsEffect> {
        if self.is_locked() {
            return Vec::new();
        }
        self.pending_import = None;
        self.set_message(
            OperationSection::ImportExport,
            StatusMessage::key(DB_IMPORT_CANCELLED),
        );
        Vec::new()
    }

    /// The settings page was left. A restore that waited for its confirmation is given
    /// up, as by Cancel: the question belongs to the page that asked it, and must not
    /// be there again, about an archive chosen some time ago, when the page is opened
    /// the next time. The open confirmation covers the page and takes Escape, so no
    /// input of the user leaves the page with it; this is for every other way out.
    pub fn page_left(&mut self) {
        if self.pending_import.take().is_none() {
            return;
        }
        // The line of an operation that runs in the section stays that operation's.
        let busy = self
            .active
            .as_ref()
            .is_some_and(|active| active.section == OperationSection::ImportExport);
        if !busy {
            self.set_message(
                OperationSection::ImportExport,
                StatusMessage::key(DB_IMPORT_CANCELLED),
            );
        }
    }

    /// "Confirm" of the overwrite confirmation: takes the stored archive and mappings
    /// and restores behind the mutation barrier.
    pub(super) fn confirm_import_db_overwrite(&mut self) -> Vec<SettingsEffect> {
        if self.is_locked() {
            return Vec::new();
        }
        let Some(pending) = self.pending_import.take() else {
            return Vec::new();
        };
        let ticket = self.next_ticket();
        let started = self.begin(Start {
            kind: Kind::RestoreBundle {
                ticket,
                source: pending.source,
                mappings: pending.mappings,
            },
            section: OperationSection::ImportExport,
            pending: StatusMessage::key("settings.actions.summary.dbImporting"),
            error_prefix: DB_IMPORT_ERROR,
            filter: None,
            global: true,
        });
        if started.is_none() {
            return Vec::new();
        }
        self.stage(Stage::Barrier);
        vec![SettingsEffect::Library(Box::new(
            LibraryInput::BeginMaintenance { ticket },
        ))]
    }

    /// The answer of a file dialog.
    pub(super) fn path_picked(
        &mut self,
        op: OperationId,
        result: Result<Option<String>, String>,
    ) -> Vec<SettingsEffect> {
        let waiting = matches!(
            &self.active,
            Some(Active { op: active, stage: Stage::PathDialog, .. }) if *active == op
        );
        if !waiting {
            return Vec::new();
        }
        if self
            .path_dialog_request
            .as_ref()
            .is_some_and(|request| request.op == op)
        {
            self.path_dialog_request = None;
        }
        let picked = match result {
            Err(error) => {
                self.fail(error);
                return Vec::new();
            }
            Ok(picked) => picked.filter(|path| !path.is_empty()),
        };
        let Some(active) = self.active.as_mut() else {
            return Vec::new();
        };
        let section = active.section;
        let cancelled = match &active.kind {
            Kind::ExportCsv => "settings.actions.summary.csvExportCancelled",
            Kind::ImportCsv { .. } => "settings.actions.summary.csvImportCancelled",
            Kind::ExportBundle => "settings.actions.summary.dbExportCancelled",
            _ => DB_IMPORT_CANCELLED,
        };
        let Some(path) = picked else {
            self.finish(StatusMessage::key(cancelled));
            return Vec::new();
        };
        match &mut active.kind {
            Kind::ExportCsv => {
                active.stage = Stage::Call;
                self.set_message(
                    section,
                    StatusMessage::key("settings.actions.summary.csvExporting"),
                );
                vec![SettingsEffect::Call(SettingsCall::ExportCsv { op, path })]
            }
            Kind::ExportBundle => {
                active.stage = Stage::Call;
                self.set_message(
                    section,
                    StatusMessage::key("settings.actions.summary.dbExporting"),
                );
                vec![SettingsEffect::Call(SettingsCall::ExportBundle {
                    op,
                    path,
                })]
            }
            Kind::ImportCsv { .. } => {
                let ticket = self.next_ticket();
                if let Some(active) = self.active.as_mut() {
                    active.kind = Kind::ImportCsv {
                        barrier: Some((ticket, path)),
                    };
                    active.stage = Stage::Barrier;
                }
                self.set_message(
                    section,
                    StatusMessage::key("settings.actions.summary.csvImporting"),
                );
                vec![SettingsEffect::Library(Box::new(
                    LibraryInput::BeginMaintenance { ticket },
                ))]
            }
            Kind::PickBundle {
                source: source @ None,
                ..
            } => {
                *source = Some(path.clone());
                active.stage = Stage::Call;
                vec![SettingsEffect::Call(SettingsCall::InspectBundle {
                    op,
                    path,
                })]
            }
            Kind::PickBundle {
                mapping, mappings, ..
            } => {
                if let Some(source_root) = mapping.take() {
                    mappings.push(DbRootMapping {
                        source_root,
                        target_root: path,
                    });
                }
                self.next_root_mapping(op);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// Asks for the folder of the next root of the archive, or, when every root has
    /// one, stores the restore for its confirmation and ends the choosing.
    fn next_root_mapping(&mut self, op: OperationId) {
        let Some(Active {
            kind:
                Kind::PickBundle {
                    source,
                    roots,
                    mapping,
                    mappings,
                },
            ..
        }) = self.active.as_mut()
        else {
            return;
        };
        if let Some(root) = roots.pop_front() {
            *mapping = Some(root.clone());
            // The platform's folder dialog has a title of its own, so the section's
            // line says which root the dialog asks about.
            let title = DialogTitle::MapRoot(root);
            self.set_message(
                OperationSection::ImportExport,
                StatusMessage::Dialog(title.clone()),
            );
            self.ask_for_path(op, PathDialogKind::OpenFolder, title);
            return;
        }
        let Some(source) = source.take() else {
            return;
        };
        self.pending_import = Some(PendingBundleImport {
            source,
            mappings: std::mem::take(mappings),
        });
        // The pending line stays: the confirmation is the next thing to read.
        self.set_message(
            OperationSection::ImportExport,
            StatusMessage::key(DB_IMPORT_PENDING),
        );
        self.finish_keeping_line();
    }

    pub(super) fn csv_exported(&mut self, result: BackendResult<usize>) -> Vec<SettingsEffect> {
        match result {
            Ok(rows) => self.finish(StatusMessage::key_with(
                "settings.actions.summary.csvExported",
                counts(&[("rows", rows)]),
            )),
            Err(error) => self.fail(error.text()),
        }
        Vec::new()
    }

    pub(super) fn bundle_exported(
        &mut self,
        result: BackendResult<BundleCounts>,
    ) -> Vec<SettingsEffect> {
        match result {
            Ok(exported) => self.finish(StatusMessage::key_with(
                "settings.actions.summary.dbExported",
                counts(&[
                    ("files", exported.files),
                    ("thumbnails", exported.thumbnails),
                ]),
            )),
            Err(error) => self.fail(error.text()),
        }
        Vec::new()
    }

    pub(super) fn csv_imported(
        &mut self,
        result: BackendResult<CsvImported>,
    ) -> Vec<SettingsEffect> {
        let Some(Kind::ImportCsv {
            barrier: Some((ticket, _)),
        }) = self.active.as_ref().map(|active| &active.kind)
        else {
            return Vec::new();
        };
        // Tag state is thrown away before the barrier comes down, whatever the result:
        // a rejection alone does not say what the import wrote.
        let mut effects = vec![SettingsEffect::Library(Box::new(
            LibraryInput::EndMaintenance {
                ticket: *ticket,
                reset: Some(LibraryReset::Identity),
            },
        ))];
        effects.extend(match result {
            Ok(imported) => self.settle(
                true,
                false,
                true,
                StatusMessage::key_with(
                    "settings.actions.summary.csvImported",
                    counts(&[
                        ("rowsRead", imported.rows_read),
                        ("rowsApplied", imported.rows_applied),
                        ("assetsMatched", imported.assets_matched),
                        ("assetsUpdated", imported.assets_updated),
                    ]),
                ),
            ),
            Err(error) => {
                let message = self.failure(error.text());
                self.settle(true, false, false, message)
            }
        });
        effects
    }

    pub(super) fn bundle_inspected(
        &mut self,
        op: OperationId,
        result: BackendResult<BundleInspection>,
    ) -> Vec<SettingsEffect> {
        let inspection = match result {
            Ok(inspection) => inspection,
            Err(error) => {
                self.fail(error.text());
                return Vec::new();
            }
        };
        let Some(Active {
            kind: Kind::PickBundle { roots, .. },
            ..
        }) = self.active.as_mut()
        else {
            return Vec::new();
        };
        if inspection.requires_mapping {
            *roots = inspection.roots.into();
        }
        self.next_root_mapping(op);
        Vec::new()
    }

    pub(super) fn bundle_imported(
        &mut self,
        result: BackendResult<BundleCounts>,
    ) -> Vec<SettingsEffect> {
        let Some(Kind::RestoreBundle { ticket, .. }) =
            self.active.as_ref().map(|active| &active.kind)
        else {
            return Vec::new();
        };
        // A restore can fail after it swapped the live files: identity and query state
        // are thrown away before the barrier comes down, whatever the result.
        let mut effects = vec![SettingsEffect::Library(Box::new(
            LibraryInput::EndMaintenance {
                ticket: *ticket,
                reset: Some(LibraryReset::Restored),
            },
        ))];
        effects.extend(match result {
            Ok(restored) => self.settle(
                true,
                true,
                true,
                StatusMessage::key_with(
                    "settings.actions.summary.dbImported",
                    counts(&[
                        ("files", restored.files),
                        ("thumbnails", restored.thumbnails),
                    ]),
                ),
            ),
            Err(error) => {
                let message = self.failure(error.text());
                self.settle(true, true, false, message)
            }
        });
        effects
    }

    /// "Start" and "Rescan" of the duplicate section.
    pub(super) fn scan_duplicates(&mut self) -> Vec<SettingsEffect> {
        let Some(op) = self.begin(Start {
            kind: Kind::DuplicateScan,
            section: OperationSection::Duplicates,
            pending: StatusMessage::key("settings.actions.pending.findDuplicates"),
            error_prefix: "settings.actions.errorPrefix.duplicateScan",
            filter: Some(PhaseFilter::DuplicateScan),
            global: false,
        }) else {
            return Vec::new();
        };
        vec![SettingsEffect::Call(SettingsCall::FindDuplicates { op })]
    }

    fn publish_duplicates(&mut self, scan: DuplicateScan) {
        self.duplicates = scan;
        self.duplicate_scan_generation += 1;
    }

    pub(super) fn duplicates_found(
        &mut self,
        result: BackendResult<DuplicateScan>,
    ) -> Vec<SettingsEffect> {
        match result {
            Ok(scan) => {
                let message = StatusMessage::key_with(
                    "settings.actions.summary.duplicateScanComplete",
                    counts(&[
                        ("groups", scan.duplicate_groups),
                        ("assets", scan.duplicate_assets),
                    ]),
                );
                self.publish_duplicates(scan);
                self.finish(message);
            }
            Err(error) => self.fail(error.text()),
        }
        Vec::new()
    }

    /// "Save all" of the resolver (`onSaveAll`).
    pub(super) fn save_duplicate_changes(
        &mut self,
        changes: Vec<DuplicateChange>,
    ) -> Vec<SettingsEffect> {
        if self.is_locked() {
            return Vec::new();
        }
        if changes.is_empty() {
            self.set_message(
                OperationSection::Duplicates,
                StatusMessage::key("settings.actions.summary.noDuplicateChanges"),
            );
            return Vec::new();
        }
        let delete_count = changes.iter().filter(|change| change.is_delete()).count();
        if delete_count > 0 {
            self.pending_duplicate_delete = Some(PendingDuplicateDelete {
                changes,
                delete_count,
            });
            return Vec::new();
        }
        self.apply_duplicate_changes(&changes)
    }

    pub(super) fn cancel_duplicate_delete(&mut self) -> Vec<SettingsEffect> {
        if self.is_locked() {
            return Vec::new();
        }
        self.pending_duplicate_delete = None;
        self.set_message(
            OperationSection::Duplicates,
            StatusMessage::key("settings.actions.summary.duplicateApplyCancelled"),
        );
        Vec::new()
    }

    pub(super) fn confirm_duplicate_delete(&mut self) -> Vec<SettingsEffect> {
        if self.is_locked() {
            return Vec::new();
        }
        let Some(pending) = self.pending_duplicate_delete.take() else {
            return Vec::new();
        };
        self.apply_duplicate_changes(&pending.changes)
    }

    /// `runSaveDuplicateChanges`: one batch against the scan it was staged on.
    fn apply_duplicate_changes(&mut self, changes: &[DuplicateChange]) -> Vec<SettingsEffect> {
        let error_prefix = "settings.actions.errorPrefix.saveDuplicateChanges";
        let Some(op) = self.begin(Start {
            kind: Kind::ApplyDuplicates {
                changes: changes.len(),
            },
            section: OperationSection::Duplicates,
            pending: StatusMessage::key("settings.actions.pending.applyDuplicateChanges"),
            error_prefix,
            filter: None,
            global: false,
        }) else {
            return Vec::new();
        };
        match build_batch(self.duplicates.revision, &self.duplicates.groups, changes) {
            Ok(input) => vec![SettingsEffect::Call(SettingsCall::ApplyDuplicates {
                op,
                input,
            })],
            Err(error) => {
                self.finish(StatusMessage::FailedKey {
                    prefix: error_prefix,
                    key: error.key,
                    params: error.params,
                });
                Vec::new()
            }
        }
    }

    /// The batch answered. A structured result, whatever its status, is followed by a
    /// refresh of the library and a new duplicate scan, both best effort, and then by
    /// its line.
    pub(super) fn duplicates_applied(
        &mut self,
        op: OperationId,
        result: BackendResult<AppliedDuplicates>,
    ) -> Vec<SettingsEffect> {
        let applied = match result {
            Ok(applied) => applied,
            Err(error) => {
                self.fail(error.text());
                return Vec::new();
            }
        };
        let changes = match self.active.as_ref().map(|active| &active.kind) {
            Some(Kind::ApplyDuplicates { changes }) => *changes,
            _ => return Vec::new(),
        };
        let outcome = match applied.status {
            AppliedStatus::RolledBack => {
                StatusMessage::key("settings.actions.summary.duplicateRolledBack")
            }
            AppliedStatus::RecoveryRequired => StatusMessage::DuplicateRecovery {
                items: applied.recovery,
            },
            AppliedStatus::Committed => StatusMessage::DuplicatesApplied {
                changes,
                // The groups of the scan the batch was staged on, until the new scan
                // answers.
                groups: self.duplicates.groups.len(),
                source_missing: applied.source_missing,
            },
        };
        let ticket = self.next_ticket();
        self.stage(Stage::Settling(Settling {
            library: Some(ticket),
            roots: false,
            duplicates: true,
            strict: false,
            refresh_error: None,
            outcome,
        }));
        vec![
            SettingsEffect::Library(Box::new(LibraryInput::Refresh {
                ticket: Some(ticket),
            })),
            SettingsEffect::Call(SettingsCall::RescanDuplicates { op }),
        ]
    }

    /// The scan after a batch answered. A scan that failed leaves the groups as they
    /// were.
    pub(super) fn duplicates_rescanned(&mut self, result: BackendResult<DuplicateScan>) {
        let Some(settling) = self.settling().filter(|settling| settling.duplicates) else {
            return;
        };
        settling.duplicates = false;
        if let Ok(scan) = result {
            if let StatusMessage::DuplicatesApplied { groups, .. } = &mut settling.outcome {
                *groups = scan.duplicate_groups;
            }
            self.publish_duplicates(scan);
        }
        self.try_settle();
    }
}
