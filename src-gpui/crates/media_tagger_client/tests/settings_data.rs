//! Import/export, restore and duplicate resolution of the settings model (epic E18)
//! inside the application runtime: the file dialogs, the mutation barrier around CSV
//! import and restore, the overwrite and delete confirmations, and the refreshes each
//! operation ends with. The scripted backend runs on the deterministic executor; the
//! destructive flows also run against the real core on a fresh temporary profile and
//! fresh temporary media, never anything else.

use std::sync::Arc;

use media_tagger_client::{
    backend::{
        core_backend::test_support::TempCore, fake::Call, Backend, BackendError, FakeBackend,
    },
    exec::{Executor, ManualExecutor},
    i18n::{I18n, Language},
    library::{LibraryInput, Mutation, Ticket},
    model::Driver,
    prefs::MemoryPrefsStore,
    runtime::{AppRuntime, RuntimeConfig, RuntimeInput, RuntimeRunner},
    settings::{
        DialogTitle, DuplicateChange, OperationSection, PathDialogKind, PathDialogRequest,
        SettingsCommand, SettingsInput,
    },
    theme::ThemeName,
};
use media_tagger_core::models::{
    CsvExportSummary, CsvImportSummary, DbBundleExportSummary, DbBundleImportSummary,
    DbBundleInspection, DbRootMapping, DuplicateAsset, DuplicateGroup,
    DuplicateResolutionBatchInput, DuplicateResolutionBatchStatus, DuplicateResolutionBatchSummary,
    DuplicateResolutionChangeInput, DuplicateResolutionItemResult, DuplicateResolutionItemStatus,
    DuplicateScanSummary, ScanProgress, ScanRoot, StartAssetQueryResult, TagListPage,
    VideoToolStatus,
};

struct Fixture {
    backend: Arc<FakeBackend>,
    executor: Arc<ManualExecutor>,
    driver: Driver<AppRuntime, RuntimeRunner>,
}

fn runtime_over(
    backend: Arc<dyn Backend>,
) -> (Arc<ManualExecutor>, Driver<AppRuntime, RuntimeRunner>) {
    let executor = Arc::new(ManualExecutor::new());
    let driver = Driver::new(
        AppRuntime::new(RuntimeConfig {
            theme: ThemeName::Dark,
            language: Language::En,
        }),
        RuntimeRunner::new(backend, Arc::new(MemoryPrefsStore::default())),
        executor.clone() as Arc<dyn Executor>,
    );
    (executor, driver)
}

fn settle(executor: &ManualExecutor, driver: &mut Driver<AppRuntime, RuntimeRunner>) {
    while executor.run_all() > 0 || driver.pump() > 0 {}
}

fn line_of(driver: &Driver<AppRuntime, RuntimeRunner>, section: OperationSection) -> String {
    driver
        .model()
        .settings()
        .status(section)
        .message
        .resolve(&I18n::new(Language::En))
}

/// Takes the file dialog the running operation asked for and answers it.
fn answer_dialog(
    driver: &mut Driver<AppRuntime, RuntimeRunner>,
    result: Result<Option<&str>, &str>,
) -> PathDialogRequest {
    let request = driver
        .model_mut()
        .take_path_dialog_request()
        .expect("an operation waits for a file dialog");
    driver.handle(RuntimeInput::Settings(SettingsInput::PathPicked {
        op: request.op,
        result: result
            .map(|path| path.map(str::to_owned))
            .map_err(str::to_owned),
    }));
    request
}

fn asset(id: i64, path: &str) -> DuplicateAsset {
    DuplicateAsset {
        id,
        path: path.to_owned(),
        record_version: id + 2,
        size_bytes: 10 * id,
        thumb_version: format!("{id:016x}"),
    }
}

fn one_group(revision: i64) -> DuplicateScanSummary {
    DuplicateScanSummary {
        groups: vec![DuplicateGroup {
            file_name: "same.jpg".into(),
            assets: vec![asset(1, "/media/a/same.jpg"), asset(2, "/media/b/same.jpg")],
        }],
        duplicate_groups: 1,
        duplicate_assets: 2,
        revision,
    }
}

fn no_groups(revision: i64) -> DuplicateScanSummary {
    DuplicateScanSummary {
        groups: Vec::new(),
        duplicate_groups: 0,
        duplicate_assets: 0,
        revision,
    }
}

fn item(
    asset_id: i64,
    status: DuplicateResolutionItemStatus,
    recovery_path: Option<&str>,
) -> DuplicateResolutionItemResult {
    DuplicateResolutionItemResult {
        asset_id,
        status,
        old_path: format!("/media/{asset_id}/same.jpg"),
        new_path: None,
        recovery_path: recovery_path.map(str::to_owned),
    }
}

fn batch_result(
    status: DuplicateResolutionBatchStatus,
    results: Vec<DuplicateResolutionItemResult>,
) -> DuplicateResolutionBatchSummary {
    DuplicateResolutionBatchSummary {
        status,
        revision: 9,
        results,
        removed_thumbnails: 0,
    }
}

impl Fixture {
    /// A started runtime over the scripted backend: one root, a launch without
    /// startup work, an empty library.
    fn started(script: impl FnOnce(&FakeBackend)) -> Self {
        let backend = Arc::new(FakeBackend::new());
        backend
            .on_start_asset_query(|_| {
                Ok(StartAssetQueryResult::Ready {
                    session_id: 1,
                    revision: 1,
                    total: 0,
                    offset: 0,
                    items: Vec::new(),
                })
            })
            .on_list_tags(|_, _, _| {
                Ok(TagListPage {
                    items: Vec::new(),
                    total: 0,
                })
            })
            .on_scan_startup_roots(|| Ok(None))
            .on_list_scan_roots(|| {
                Ok(vec![ScanRoot {
                    path: "/synthetic/media".into(),
                    auto_scan_on_startup: false,
                }])
            })
            .on_get_video_tool_status(|| {
                Ok(VideoToolStatus {
                    ffmpeg_available: true,
                    ffprobe_available: true,
                })
            });
        script(&backend);
        let (executor, driver) = runtime_over(backend.clone());
        let mut fixture = Self {
            backend,
            executor,
            driver,
        };
        fixture.driver.handle(RuntimeInput::Start);
        fixture.settle();
        fixture
    }

    fn runtime(&self) -> &AppRuntime {
        self.driver.model()
    }

    fn command(&mut self, command: SettingsCommand) {
        self.driver.handle(RuntimeInput::settings(command));
    }

    fn settle(&mut self) {
        settle(&self.executor, &mut self.driver);
    }

    fn line(&self, section: OperationSection) -> String {
        line_of(&self.driver, section)
    }

    fn calls(&self, method: &str) -> usize {
        self.backend
            .called_methods()
            .into_iter()
            .filter(|called| *called == method)
            .count()
    }

    fn dialog(&mut self, result: Result<Option<&str>, &str>) -> PathDialogRequest {
        answer_dialog(&mut self.driver, result)
    }

    fn locked(&self) -> bool {
        self.runtime().settings().is_locked()
    }
}

const IMPORT_EXPORT: OperationSection = OperationSection::ImportExport;
const DUPLICATES: OperationSection = OperationSection::Duplicates;

// createSettingsActions.test.tsx, "handles cancelled export/import dialog selections".
#[test]
fn a_cancelled_dialog_ends_each_action_without_a_backend_call() {
    let mut fx = Fixture::started(|_| {});
    let before = fx.backend.calls().len();
    for (command, kind, cancelled) in [
        (
            SettingsCommand::ExportCsv,
            PathDialogKind::SaveFile {
                prefix: "tags-export",
                extension: "csv",
            },
            "CSV export cancelled.",
        ),
        (
            SettingsCommand::ImportCsv,
            PathDialogKind::OpenFile { extension: "csv" },
            "CSV import cancelled.",
        ),
        (
            SettingsCommand::ExportDbBundle,
            PathDialogKind::SaveFile {
                prefix: "media-backup",
                extension: "zip",
            },
            "Database backup export cancelled.",
        ),
        (
            SettingsCommand::ImportDbBundle,
            PathDialogKind::OpenFile { extension: "zip" },
            "Database backup import cancelled.",
        ),
    ] {
        fx.command(command.clone());
        assert!(
            fx.locked(),
            "{command:?} holds the lock while its dialog is open"
        );
        assert!(fx.runtime().settings().status(IMPORT_EXPORT).loading);
        assert!(fx.runtime().maintenance_loading(), "{command:?}");
        // A second action is refused in the same turn.
        fx.command(SettingsCommand::ExportCsv);
        let request = fx.dialog(Ok(None));
        assert_eq!(request.kind, kind, "{command:?}");
        fx.settle();
        assert_eq!(fx.line(IMPORT_EXPORT), cancelled);
        assert!(!fx.locked() && !fx.runtime().settings().status(IMPORT_EXPORT).loading);
        assert!(fx.runtime().settings().path_dialog_request().is_none());
        assert!(fx.runtime().settings().pending_import_db_source().is_none());
    }
    assert_eq!(fx.backend.calls().len(), before, "no backend call was made");
    assert!(!fx.runtime().library().maintenance_active());
}

#[test]
fn a_dialog_that_cannot_open_is_the_operations_error_and_a_late_answer_is_dropped() {
    let mut fx = Fixture::started(|_| {});
    fx.command(SettingsCommand::ExportCsv);
    let request = fx.dialog(Err("the portal is missing"));
    assert_eq!(
        fx.line(IMPORT_EXPORT),
        "CSV export error: the portal is missing"
    );
    assert!(!fx.locked());

    // The answer of that dialog once more, and an answer for the next operation's
    // dialog under the old operation's id: neither is taken.
    fx.command(SettingsCommand::ImportCsv);
    fx.driver
        .handle(RuntimeInput::Settings(SettingsInput::PathPicked {
            op: request.op,
            result: Ok(Some("/synthetic/late.csv".into())),
        }));
    fx.settle();
    assert!(fx.locked(), "the import still waits for its own dialog");
    assert_eq!(fx.calls("import_tags_csv"), 0);
    assert_eq!(fx.line(IMPORT_EXPORT), "Preparing CSV import...");
}

// createSettingsActions.test.tsx, "exports database bundle and reports copied files
// summary", and the CSV export next to it.
#[test]
fn exports_go_to_the_chosen_path_and_refresh_nothing() {
    let mut fx = Fixture::started(|backend| {
        backend
            .on_export_tags_csv(|_| Ok(CsvExportSummary { rows: 12 }))
            .on_export_db_bundle(|_| {
                Ok(DbBundleExportSummary {
                    copied_files: 3,
                    copied_thumbnails: 7,
                })
            });
    });
    let queries = fx.calls("start_asset_query");
    let listings = fx.calls("list_scan_roots");

    fx.command(SettingsCommand::ExportCsv);
    assert_eq!(fx.line(IMPORT_EXPORT), "Preparing CSV export...");
    let request = fx.dialog(Ok(Some("/synthetic/out/tags.csv")));
    assert_eq!(
        request.title,
        DialogTitle::Key("settings.actions.dialogs.exportTagsCsv")
    );
    assert_eq!(fx.line(IMPORT_EXPORT), "Exporting tags CSV...");
    assert!(fx.locked());
    fx.settle();
    assert_eq!(fx.line(IMPORT_EXPORT), "CSV exported. Rows: 12");
    assert!(!fx.locked());

    fx.command(SettingsCommand::ExportDbBundle);
    fx.dialog(Ok(Some("/synthetic/out/backup.zip")));
    assert_eq!(
        fx.line(IMPORT_EXPORT),
        "Exporting database and thumbnails..."
    );
    fx.settle();
    assert_eq!(
        fx.line(IMPORT_EXPORT),
        "Database backup archive exported. Files: 3, thumbnails: 7"
    );

    let calls = fx.backend.calls();
    assert!(calls.contains(&Call::ExportTagsCsv {
        path: "/synthetic/out/tags.csv".into()
    }));
    assert!(calls.contains(&Call::ExportDbBundle {
        path: "/synthetic/out/backup.zip".into()
    }));
    assert_eq!(fx.calls("start_asset_query"), queries);
    assert_eq!(fx.calls("list_scan_roots"), listings);
    assert!(!fx.runtime().library().maintenance_active());
}

#[test]
fn a_failed_export_shows_the_backends_error() {
    let mut fx = Fixture::started(|backend| {
        backend.on_export_tags_csv(|_| Err(BackendError::rejected("i18n:common.libraryBusy")));
    });
    fx.command(SettingsCommand::ExportCsv);
    fx.dialog(Ok(Some("/synthetic/out/tags.csv")));
    fx.settle();
    assert_eq!(
        fx.line(IMPORT_EXPORT),
        "CSV export error: A scan or another library operation is running. Try again when it \
         finishes."
    );
    assert!(!fx.locked());
}

// createSettingsActions.test.tsx, "imports CSV and refreshes library state on success".
#[test]
fn a_csv_import_runs_behind_the_mutation_barrier_and_refreshes_the_library() {
    let mut fx = Fixture::started(|backend| {
        backend.on_import_tags_csv(|_| {
            Ok(CsvImportSummary {
                rows_read: 5,
                rows_applied: 4,
                assets_matched: 6,
                assets_updated: 3,
            })
        });
    });
    let queries = fx.calls("start_asset_query");
    let identity = fx.runtime().library().tags().epoch();

    fx.command(SettingsCommand::ImportCsv);
    fx.dialog(Ok(Some("/synthetic/in/tags.csv")));
    // The barrier is up before the import is asked for, and the import is the only
    // call that waits.
    assert!(fx.runtime().library().maintenance_active());
    assert_eq!(fx.line(IMPORT_EXPORT), "Importing tags CSV...");
    assert_eq!(fx.executor.pending(), 1);
    assert_eq!(fx.calls("import_tags_csv"), 0);

    assert!(fx.executor.run_next());
    while fx.driver.pump() > 0 {}
    // The import answered: tag state was thrown away and the barrier is down, the
    // refresh is on its way and the operation still holds the lock.
    assert!(!fx.runtime().library().maintenance_active());
    assert_ne!(fx.runtime().library().tags().epoch(), identity);
    assert!(fx.locked());
    assert_eq!(fx.line(IMPORT_EXPORT), "Importing tags CSV...");

    fx.settle();
    assert_eq!(
        fx.line(IMPORT_EXPORT),
        "CSV imported. Rows read: 5, applied: 4, assets matched by file name: 6, assets updated: \
         3. One row can match files with the same name in multiple folders."
    );
    assert!(!fx.locked());
    assert_eq!(fx.calls("start_asset_query"), queries + 1);
    assert!(fx.backend.calls().contains(&Call::ImportTagsCsv {
        path: "/synthetic/in/tags.csv".into()
    }));
}

// createSettingsActions.test.tsx, "invalidates tag caches and refreshes after a
// rejected CSV import".
#[test]
fn a_rejected_csv_import_still_resets_tag_state_and_refreshes_before_its_error() {
    let mut fx = Fixture::started(|backend| {
        backend.on_import_tags_csv(|_| Err(BackendError::rejected("csv row 3 is malformed")));
    });
    let queries = fx.calls("start_asset_query");
    let identity = fx.runtime().library().tags().epoch();

    fx.command(SettingsCommand::ImportCsv);
    fx.dialog(Ok(Some("/synthetic/in/tags.csv")));
    fx.settle();

    assert_eq!(
        fx.line(IMPORT_EXPORT),
        "CSV import error: csv row 3 is malformed"
    );
    assert_ne!(fx.runtime().library().tags().epoch(), identity);
    assert_eq!(fx.calls("start_asset_query"), queries + 1);
    assert!(!fx.locked() && !fx.runtime().library().maintenance_active());
}

#[test]
fn a_refresh_that_fails_after_a_csv_import_is_the_imports_error() {
    let mut fx = Fixture::started(|backend| {
        backend.on_import_tags_csv(|_| {
            Ok(CsvImportSummary {
                rows_read: 1,
                rows_applied: 1,
                assets_matched: 1,
                assets_updated: 1,
            })
        });
    });
    fx.command(SettingsCommand::ImportCsv);
    fx.dialog(Ok(Some("/synthetic/in/tags.csv")));
    assert!(fx.executor.run_next());
    while fx.driver.pump() > 0 {}
    fx.backend
        .on_start_asset_query(|_| Err(BackendError::rejected("the query failed")));
    fx.settle();

    let line = fx.line(IMPORT_EXPORT);
    assert!(line.starts_with("CSV import error: "), "{line}");
    assert!(!line.contains("CSV imported"), "{line}");
    assert!(!fx.locked());
}

/// A tag write on one asset, as the bulk dock sends it.
fn merge(ticket: u64, asset_id: i64) -> RuntimeInput {
    RuntimeInput::library(LibraryInput::Mutate {
        ticket: Ticket(ticket),
        mutation: Mutation::MergeTags {
            asset_ids: vec![asset_id],
            tags: vec!["synthetic".into()],
        },
    })
}

// operationSafety.test.tsx, "settings and actual metadata maintenance barrier": a tag
// write admitted before a CSV import ends before the import runs, writes are refused
// while the barrier stands, and they are admitted again afterwards.
#[test]
fn a_csv_import_waits_for_a_tag_write_in_flight_and_refuses_new_ones() {
    let mut fx = Fixture::started(|backend| {
        backend
            .on_merge_asset_tags_bulk(|_, _| Err(BackendError::rejected("disk")))
            .on_import_tags_csv(|_| Err(BackendError::rejected("uncertain IPC failure")));
    });

    fx.driver.handle(merge(1, 1));
    assert_eq!(fx.executor.pending(), 1, "the write is in flight");
    fx.command(SettingsCommand::ImportCsv);
    fx.dialog(Ok(Some("/synthetic/in/tags.csv")));
    assert!(fx.locked() && fx.runtime().library().maintenance_active());
    assert_eq!(fx.executor.pending(), 1, "the import waits for the write");
    assert_eq!(fx.calls("import_tags_csv"), 0);

    fx.driver.handle(merge(2, 2));
    assert_eq!(
        fx.executor.pending(),
        1,
        "a write under the barrier is refused"
    );

    assert!(fx.executor.run_next());
    while fx.driver.pump() > 0 {}
    assert_eq!(fx.calls("merge_asset_tags_bulk"), 1);
    assert_eq!(fx.calls("import_tags_csv"), 0, "not run yet");
    assert_eq!(fx.executor.pending(), 1, "the import is queued now");
    assert!(fx.executor.run_next());
    while fx.driver.pump() > 0 {}
    assert_eq!(fx.calls("import_tags_csv"), 1);
    assert!(!fx.runtime().library().maintenance_active(), "barrier down");

    fx.settle();
    assert!(fx.line(IMPORT_EXPORT).contains("uncertain IPC failure"));
    assert!(!fx.locked());
    fx.driver.handle(merge(3, 3));
    assert_eq!(
        fx.executor.pending(),
        1,
        "a later write reaches the backend"
    );
    fx.settle();
    assert_eq!(fx.calls("merge_asset_tags_bulk"), 2);
}

#[test]
fn a_restore_waits_for_a_tag_write_in_flight_and_refuses_new_ones() {
    let mut fx = Fixture::started(|backend| {
        backend
            .on_merge_asset_tags_bulk(|_, _| Err(BackendError::rejected("disk")))
            .on_inspect_db_bundle(|_| Ok(inspection(&[], false)))
            .on_import_db_bundle(|_, _| Err(BackendError::rejected("restore failed after swap")));
    });
    fx.command(SettingsCommand::ImportDbBundle);
    fx.dialog(Ok(Some("/synthetic/backup.zip")));
    fx.settle();

    // Choosing the archive put up no barrier: a write is still admitted.
    assert!(!fx.runtime().library().maintenance_active());
    fx.driver.handle(merge(1, 1));
    assert_eq!(fx.executor.pending(), 1, "the write is in flight");

    fx.command(SettingsCommand::ConfirmImportDbOverwrite);
    assert!(fx.locked() && fx.runtime().library().maintenance_active());
    assert_eq!(fx.executor.pending(), 1, "the restore waits for the write");
    assert_eq!(fx.calls("import_db_bundle"), 0);
    fx.driver.handle(merge(2, 2));
    assert_eq!(
        fx.executor.pending(),
        1,
        "a write under the barrier is refused"
    );

    assert!(fx.executor.run_next());
    while fx.driver.pump() > 0 {}
    assert_eq!(fx.calls("merge_asset_tags_bulk"), 1);
    assert_eq!(fx.calls("import_db_bundle"), 0, "not run yet");
    assert_eq!(fx.executor.pending(), 1, "the restore is queued now");
    assert!(fx.executor.run_next());
    while fx.driver.pump() > 0 {}
    assert_eq!(fx.calls("import_db_bundle"), 1);
    assert!(!fx.runtime().library().maintenance_active(), "barrier down");

    fx.settle();
    assert!(fx.line(IMPORT_EXPORT).contains("restore failed after swap"));
    assert!(!fx.locked());
    fx.driver.handle(merge(3, 3));
    assert_eq!(
        fx.executor.pending(),
        1,
        "a later write reaches the backend"
    );
    fx.settle();
    assert_eq!(fx.calls("merge_asset_tags_bulk"), 2);
}

fn inspection(roots: &[&str], requires_mapping: bool) -> DbBundleInspection {
    DbBundleInspection {
        format_version: Some(1),
        source_platform: Some("linux".into()),
        source_thumbs_dir: None,
        roots: roots.iter().map(|root| (*root).to_owned()).collect(),
        requires_mapping,
    }
}

// createSettingsActionsConfirmFlow.test.tsx, "stores DB import confirmation and cancels
// it without importing".
#[test]
fn a_restore_waits_for_its_confirmation_and_cancel_imports_nothing() {
    let mut fx = Fixture::started(|backend| {
        backend.on_inspect_db_bundle(|_| Ok(inspection(&[], false)));
    });
    fx.command(SettingsCommand::ImportDbBundle);
    fx.dialog(Ok(Some("/synthetic/backup.zip")));
    fx.settle();

    let settings = fx.runtime().settings();
    assert_eq!(
        settings.pending_import_db_source(),
        Some("/synthetic/backup.zip")
    );
    assert!(settings.pending_import_db_mappings().is_empty());
    // The choosing has ended: the lock is free while the confirmation is read.
    assert!(!fx.locked() && !settings.status(IMPORT_EXPORT).loading);
    assert_eq!(
        fx.line(IMPORT_EXPORT),
        "Preparing database backup import..."
    );
    assert_eq!(fx.calls("import_db_bundle"), 0);

    fx.command(SettingsCommand::CancelImportDbOverwrite);
    fx.settle();
    assert!(fx.runtime().settings().pending_import_db_source().is_none());
    assert_eq!(fx.line(IMPORT_EXPORT), "Database backup import cancelled.");
    assert_eq!(fx.calls("import_db_bundle"), 0);
    assert!(!fx.runtime().library().maintenance_active());

    // Nothing is left to confirm.
    fx.command(SettingsCommand::ConfirmImportDbOverwrite);
    fx.settle();
    assert_eq!(fx.calls("import_db_bundle"), 0);
}

// createSettingsActionsConfirmFlow.test.tsx, "imports DB backup archive only after
// confirmation and inside the tag mutation barrier".
#[test]
fn a_confirmed_restore_runs_behind_the_barrier_and_reads_roots_and_library_again() {
    let mut fx = Fixture::started(|backend| {
        backend
            .on_inspect_db_bundle(|_| Ok(inspection(&[], false)))
            .on_import_db_bundle(|_, _| {
                Ok(DbBundleImportSummary {
                    restored_files: 2,
                    restored_thumbnails: 5,
                })
            });
    });
    fx.command(SettingsCommand::ImportDbBundle);
    fx.dialog(Ok(Some("/synthetic/backup.zip")));
    fx.settle();
    let queries = fx.calls("start_asset_query");
    let listings = fx.calls("list_scan_roots");
    let identity = fx.runtime().library().tags().epoch();

    fx.command(SettingsCommand::ConfirmImportDbOverwrite);
    // The stored restore was taken before anything ran, and the barrier is up
    // before the restore is asked for.
    assert!(fx.runtime().settings().pending_import_db_source().is_none());
    assert!(fx.locked() && fx.runtime().library().maintenance_active());
    assert_eq!(
        fx.line(IMPORT_EXPORT),
        "Importing database and thumbnails..."
    );
    assert_eq!(fx.executor.pending(), 1);
    // A second confirmation in the same turn finds nothing.
    fx.command(SettingsCommand::ConfirmImportDbOverwrite);
    assert_eq!(fx.executor.pending(), 1);

    assert!(fx.executor.run_next());
    while fx.driver.pump() > 0 {}
    assert!(!fx.runtime().library().maintenance_active());
    assert_ne!(fx.runtime().library().tags().epoch(), identity);
    assert!(fx.locked(), "the refreshes are still out");

    fx.settle();
    assert_eq!(
        fx.line(IMPORT_EXPORT),
        "Database backup archive imported. Files: 2, thumbnails: 5"
    );
    assert!(!fx.locked());
    assert_eq!(fx.calls("import_db_bundle"), 1);
    assert!(fx.backend.calls().contains(&Call::ImportDbBundle {
        path: "/synthetic/backup.zip".into(),
        root_mappings: Vec::new(),
    }));
    assert_eq!(fx.calls("start_asset_query"), queries + 1);
    assert_eq!(fx.calls("list_scan_roots"), listings + 1);
}

// createSettingsActionsConfirmFlow.test.tsx, "invalidates and refreshes library state
// when confirmed DB restore rejects".
#[test]
fn a_rejected_restore_still_resets_the_library_and_refreshes_before_its_error() {
    let mut fx = Fixture::started(|backend| {
        backend
            .on_inspect_db_bundle(|_| Ok(inspection(&[], false)))
            .on_import_db_bundle(|_, _| Err(BackendError::rejected("restore failed after swap")));
    });
    fx.command(SettingsCommand::ImportDbBundle);
    fx.dialog(Ok(Some("/synthetic/broken.zip")));
    fx.settle();
    let queries = fx.calls("start_asset_query");
    let listings = fx.calls("list_scan_roots");
    let identity = fx.runtime().library().tags().epoch();

    fx.command(SettingsCommand::ConfirmImportDbOverwrite);
    fx.settle();

    assert_eq!(
        fx.line(IMPORT_EXPORT),
        "Database backup import error: restore failed after swap"
    );
    assert_ne!(fx.runtime().library().tags().epoch(), identity);
    assert_eq!(fx.calls("start_asset_query"), queries + 1);
    assert_eq!(fx.calls("list_scan_roots"), listings + 1);
    assert!(!fx.locked() && !fx.runtime().library().maintenance_active());
    assert!(fx.runtime().settings().pending_import_db_source().is_none());
}

// operationSafety.test.tsx, "collects all root mappings before overwrite confirmation
// and consumes it only once".
#[test]
fn every_root_of_an_archive_gets_its_folder_before_the_confirmation() {
    let mut fx = Fixture::started(|backend| {
        backend
            .on_inspect_db_bundle(|_| Ok(inspection(&["/old/photos", "/old/videos"], true)))
            .on_import_db_bundle(|_, _| {
                Ok(DbBundleImportSummary {
                    restored_files: 1,
                    restored_thumbnails: 0,
                })
            });
    });
    fx.command(SettingsCommand::ImportDbBundle);
    fx.dialog(Ok(Some("/synthetic/backup.zip")));
    fx.settle();

    // One folder dialog per root, in the archive's order; the line names the root.
    assert!(fx.locked());
    assert_eq!(fx.line(IMPORT_EXPORT), "Choose folder: /old/photos");
    let first = fx.dialog(Ok(Some("/new/photos")));
    assert_eq!(first.kind, PathDialogKind::OpenFolder);
    assert_eq!(first.title, DialogTitle::MapRoot("/old/photos".into()));
    assert!(fx.runtime().settings().pending_import_db_source().is_none());
    assert_eq!(fx.line(IMPORT_EXPORT), "Choose folder: /old/videos");
    let second = fx.dialog(Ok(Some("/new/videos")));
    assert_eq!(second.title, DialogTitle::MapRoot("/old/videos".into()));

    let mappings = vec![
        DbRootMapping {
            source_root: "/old/photos".into(),
            target_root: "/new/photos".into(),
        },
        DbRootMapping {
            source_root: "/old/videos".into(),
            target_root: "/new/videos".into(),
        },
    ];
    let settings = fx.runtime().settings();
    assert_eq!(
        settings.pending_import_db_source(),
        Some("/synthetic/backup.zip")
    );
    assert_eq!(settings.pending_import_db_mappings(), mappings.as_slice());
    assert!(!fx.locked());
    assert_eq!(
        fx.line(IMPORT_EXPORT),
        "Preparing database backup import..."
    );
    assert_eq!(fx.calls("import_db_bundle"), 0);

    fx.command(SettingsCommand::ConfirmImportDbOverwrite);
    fx.command(SettingsCommand::ConfirmImportDbOverwrite);
    fx.settle();
    assert_eq!(fx.calls("import_db_bundle"), 1, "consumed once");
    assert!(fx.backend.calls().contains(&Call::ImportDbBundle {
        path: "/synthetic/backup.zip".into(),
        root_mappings: mappings,
    }));
}

// operationSafety.test.tsx, "cancels root mapping without opening overwrite
// confirmation".
#[test]
fn a_cancelled_root_mapping_opens_no_confirmation() {
    let mut fx = Fixture::started(|backend| {
        backend.on_inspect_db_bundle(|_| Ok(inspection(&["/old/photos", "/old/videos"], true)));
    });
    fx.command(SettingsCommand::ImportDbBundle);
    fx.dialog(Ok(Some("/synthetic/backup.zip")));
    fx.settle();
    fx.dialog(Ok(Some("/new/photos")));
    fx.dialog(Ok(None));
    fx.settle();

    assert!(fx.runtime().settings().pending_import_db_source().is_none());
    assert!(fx.runtime().settings().path_dialog_request().is_none());
    assert_eq!(fx.line(IMPORT_EXPORT), "Database backup import cancelled.");
    assert!(!fx.locked());
    assert_eq!(fx.calls("import_db_bundle"), 0);
}

#[test]
fn an_archive_that_cannot_be_inspected_opens_no_confirmation() {
    let mut fx = Fixture::started(|backend| {
        backend.on_inspect_db_bundle(|_| Err(BackendError::rejected("not a backup archive")));
    });
    fx.command(SettingsCommand::ImportDbBundle);
    fx.dialog(Ok(Some("/synthetic/other.zip")));
    fx.settle();
    assert_eq!(
        fx.line(IMPORT_EXPORT),
        "Database backup import error: not a backup archive"
    );
    assert!(fx.runtime().settings().pending_import_db_source().is_none());
    assert!(!fx.locked());
}

// Orchestrator audit G3: the restore that waits for its confirmation is the archive
// chosen last. The page's buttons are covered by the confirmation, so a second choice
// can only start past the view; it still must not leave the first archive confirmable.
#[test]
fn a_second_choice_of_an_archive_can_never_confirm_the_first() {
    let mut fx = Fixture::started(|backend| {
        backend
            .on_inspect_db_bundle(|path| {
                if path == "/synthetic/broken.zip" {
                    Err(BackendError::rejected("not a backup archive"))
                } else {
                    Ok(inspection(&[], false))
                }
            })
            .on_import_db_bundle(|_, _| {
                Ok(DbBundleImportSummary {
                    restored_files: 1,
                    restored_thumbnails: 0,
                })
            });
    });
    let pick_first = |fx: &mut Fixture| {
        fx.command(SettingsCommand::ImportDbBundle);
        fx.dialog(Ok(Some("/synthetic/first.zip")));
        fx.settle();
        assert_eq!(
            fx.runtime().settings().pending_import_db_source(),
            Some("/synthetic/first.zip")
        );
    };
    let nothing_to_confirm = |fx: &mut Fixture, why: &str| {
        assert!(
            fx.runtime().settings().pending_import_db_source().is_none(),
            "{why}"
        );
        fx.command(SettingsCommand::ConfirmImportDbOverwrite);
        fx.settle();
        assert_eq!(fx.calls("import_db_bundle"), 0, "{why}");
        assert!(!fx.locked() && !fx.runtime().library().maintenance_active());
    };

    // The second choice starts: the first archive is given up before its dialog
    // answers, and a confirmation sent meanwhile restores nothing.
    pick_first(&mut fx);
    fx.command(SettingsCommand::ImportDbBundle);
    assert!(fx.locked());
    assert!(fx.runtime().settings().pending_import_db_source().is_none());
    fx.command(SettingsCommand::ConfirmImportDbOverwrite);
    assert_eq!(fx.executor.pending(), 0, "refused while the choice runs");
    // Its dialog is cancelled.
    fx.dialog(Ok(None));
    fx.settle();
    assert_eq!(fx.line(IMPORT_EXPORT), "Database backup import cancelled.");
    nothing_to_confirm(&mut fx, "after a cancelled second choice");

    // The dialog of the second choice cannot open.
    pick_first(&mut fx);
    fx.command(SettingsCommand::ImportDbBundle);
    fx.dialog(Err("the portal is not available"));
    fx.settle();
    nothing_to_confirm(&mut fx, "after a second choice whose dialog failed");

    // The second archive cannot be inspected.
    pick_first(&mut fx);
    fx.command(SettingsCommand::ImportDbBundle);
    fx.dialog(Ok(Some("/synthetic/broken.zip")));
    fx.settle();
    assert_eq!(
        fx.line(IMPORT_EXPORT),
        "Database backup import error: not a backup archive"
    );
    nothing_to_confirm(&mut fx, "after a second archive that is no backup");

    // The second archive is chosen: the confirmation restores it and nothing else.
    pick_first(&mut fx);
    fx.command(SettingsCommand::ImportDbBundle);
    fx.dialog(Ok(Some("/synthetic/second.zip")));
    fx.settle();
    assert_eq!(
        fx.runtime().settings().pending_import_db_source(),
        Some("/synthetic/second.zip")
    );
    fx.command(SettingsCommand::ConfirmImportDbOverwrite);
    fx.settle();
    let restored: Vec<String> = fx
        .backend
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            Call::ImportDbBundle { path, .. } => Some(path),
            _ => None,
        })
        .collect();
    assert_eq!(restored, ["/synthetic/second.zip"]);
    assert!(fx.runtime().settings().pending_import_db_source().is_none());
}

// Orchestrator audit G3: a choice that is refused, because another operation runs,
// leaves the waiting restore as it was (`runExclusiveOperation` returns before the
// operation's body), and so does a Cancel that is refused.
#[test]
fn a_refused_choice_and_a_refused_cancel_keep_the_waiting_restore() {
    let mut fx = Fixture::started(|backend| {
        backend
            .on_inspect_db_bundle(|_| Ok(inspection(&[], false)))
            .on_find_duplicate_assets(|| Ok(no_groups(3)));
    });
    fx.command(SettingsCommand::ImportDbBundle);
    fx.dialog(Ok(Some("/synthetic/backup.zip")));
    fx.settle();

    fx.command(SettingsCommand::RescanDuplicates);
    assert!(fx.locked());
    fx.command(SettingsCommand::ImportDbBundle);
    fx.command(SettingsCommand::CancelImportDbOverwrite);
    assert!(fx.runtime().settings().path_dialog_request().is_none());
    assert_eq!(
        fx.runtime().settings().pending_import_db_source(),
        Some("/synthetic/backup.zip")
    );
    fx.settle();

    // The scan has ended: Cancel gives the restore up.
    fx.command(SettingsCommand::CancelImportDbOverwrite);
    assert!(fx.runtime().settings().pending_import_db_source().is_none());
    assert_eq!(fx.calls("import_db_bundle"), 0);
}

// Orchestrator audit G3: the confirmation does not outlive the page. Nothing the user
// does leaves the page while the confirmation is open (it covers the page and takes
// Escape); a page left some other way gives the waiting restore up.
#[test]
fn leaving_the_page_gives_up_the_restore_that_waited_for_confirmation() {
    let mut fx = Fixture::started(|backend| {
        backend.on_inspect_db_bundle(|_| Ok(inspection(&[], false)));
    });
    fx.driver.handle(RuntimeInput::OpenSettings);
    fx.command(SettingsCommand::ImportDbBundle);
    fx.dialog(Ok(Some("/synthetic/backup.zip")));
    fx.settle();
    assert!(fx.runtime().settings().pending_import_db_source().is_some());

    fx.driver.handle(RuntimeInput::CloseSettings);
    assert!(fx.runtime().settings().pending_import_db_source().is_none());
    assert_eq!(fx.line(IMPORT_EXPORT), "Database backup import cancelled.");

    fx.driver.handle(RuntimeInput::OpenSettings);
    assert!(fx.runtime().settings().pending_import_db_source().is_none());
    fx.command(SettingsCommand::ConfirmImportDbOverwrite);
    fx.settle();
    assert_eq!(fx.calls("import_db_bundle"), 0);
    assert!(!fx.locked() && !fx.runtime().library().maintenance_active());

    // Leaving a page that asks nothing changes no line.
    fx.driver.handle(RuntimeInput::CloseSettings);
    assert_eq!(fx.line(IMPORT_EXPORT), "Database backup import cancelled.");
}

// Orchestrator audit G2. `Dispatch::call` turns a panic of the backend call into that
// call's `WorkerFailed` reply, so a settings operation whose backend call panics ends
// on its normal failure path, like a Tauri command whose promise rejects: the error
// line, the lock released, and the next operation can start.
#[test]
fn a_backend_call_that_panics_is_the_operations_error_and_releases_the_lock() {
    let mut fx = Fixture::started(|backend| {
        backend
            .on_export_tags_csv(|_| panic!("synthetic backend panic"))
            .on_find_duplicate_assets(|| panic!("synthetic backend panic"))
            .on_export_db_bundle(|_| {
                Ok(DbBundleExportSummary {
                    copied_files: 2,
                    copied_thumbnails: 3,
                })
            });
    });
    let queries = fx.calls("start_asset_query");

    fx.command(SettingsCommand::ExportCsv);
    fx.dialog(Ok(Some("/synthetic/out/tags.csv")));
    assert!(fx.locked());
    fx.settle();
    assert_eq!(
        fx.line(IMPORT_EXPORT),
        "CSV export error: export_tags_csv worker failed: synthetic backend panic"
    );
    assert!(!fx.locked());
    assert!(!fx.runtime().settings().status(IMPORT_EXPORT).loading);
    assert!(!fx.runtime().library().maintenance_active());
    assert_eq!(fx.calls("start_asset_query"), queries, "nothing to refresh");

    // The duplicate scan reports progress: its subscription ends with the panic too.
    fx.command(SettingsCommand::StartDuplicates);
    fx.settle();
    assert_eq!(
        fx.line(DUPLICATES),
        "Duplicate scan error: find_duplicate_assets worker failed: synthetic backend panic"
    );
    assert!(!fx.locked());
    assert!(!fx.runtime().settings().status(DUPLICATES).loading);

    // The runner is free: the next operation runs to its end.
    fx.command(SettingsCommand::ExportDbBundle);
    fx.dialog(Ok(Some("/synthetic/out/backup.zip")));
    fx.settle();
    assert_eq!(
        fx.line(IMPORT_EXPORT),
        "Database backup archive exported. Files: 2, thumbnails: 3"
    );
    assert!(!fx.locked());
}

// Orchestrator audit G2, the operations behind the mutation barrier: a panic of the
// import, the restore or the clear brings the barrier down with the reset each of them
// makes on any failure, reads the library again and releases the lock, so tag writes are
// admitted again for the rest of the session.
#[test]
fn a_panic_behind_the_mutation_barrier_brings_the_barrier_down() {
    let mut fx = Fixture::started(|backend| {
        backend
            .on_import_tags_csv(|_| panic!("synthetic backend panic"))
            .on_inspect_db_bundle(|_| Ok(inspection(&[], false)))
            .on_import_db_bundle(|_, _| panic!("synthetic backend panic"))
            .on_clear_library_data(|| panic!("synthetic backend panic"))
            .on_merge_asset_tags_bulk(|_, _| Err(BackendError::rejected("disk")));
    });
    let mut ticket = 0;
    let mut check = |fx: &mut Fixture, section: OperationSection, method: &str, reset: bool| {
        let queries = fx.calls("start_asset_query");
        let identity = fx.runtime().library().tags().epoch();
        assert!(fx.locked() && fx.runtime().library().maintenance_active());
        fx.settle();

        let line = fx.line(section);
        assert!(
            line.ends_with(&format!("{method} worker failed: synthetic backend panic")),
            "{line}"
        );
        assert_eq!(fx.calls(method), 1);
        assert!(!fx.locked(), "{method}: the lock is released");
        assert!(!fx.runtime().settings().status(section).loading);
        assert!(
            !fx.runtime().library().maintenance_active(),
            "{method}: the barrier is down"
        );
        if reset {
            assert_ne!(
                fx.runtime().library().tags().epoch(),
                identity,
                "{method}: tag state was thrown away"
            );
        }
        assert!(
            fx.calls("start_asset_query") > queries,
            "{method}: the library was read again"
        );
        // A tag write is admitted again.
        ticket += 1;
        let writes = fx.calls("merge_asset_tags_bulk");
        fx.driver.handle(merge(ticket, 1));
        fx.settle();
        assert_eq!(fx.calls("merge_asset_tags_bulk"), writes + 1, "{method}");
    };

    fx.command(SettingsCommand::ImportCsv);
    fx.dialog(Ok(Some("/synthetic/in/tags.csv")));
    check(&mut fx, IMPORT_EXPORT, "import_tags_csv", true);

    fx.command(SettingsCommand::ImportDbBundle);
    fx.dialog(Ok(Some("/synthetic/backup.zip")));
    fx.settle();
    fx.command(SettingsCommand::ConfirmImportDbOverwrite);
    assert!(fx.runtime().settings().pending_import_db_source().is_none());
    check(&mut fx, IMPORT_EXPORT, "import_db_bundle", true);

    fx.command(SettingsCommand::ConfirmClear);
    check(
        &mut fx,
        OperationSection::Danger,
        "clear_library_data",
        false,
    );
}

fn progress(phase: &str, processed: usize, total: usize) -> ScanProgress {
    ScanProgress {
        phase: phase.into(),
        processed,
        total,
        message: String::new(),
    }
}

// createSettingsActions.test.tsx, "opens duplicate dialog and scans duplicate groups".
#[test]
fn start_opens_the_resolver_and_scans_with_its_own_progress() {
    let mut fx = Fixture::started(|backend| {
        backend.on_find_duplicate_assets(|| Ok(one_group(7)));
    });
    let generation = fx.runtime().settings().duplicate_scan_generation();

    fx.command(SettingsCommand::StartDuplicates);
    let settings = fx.runtime().settings();
    assert!(settings.duplicate_dialog_open());
    assert!(fx.locked() && settings.status(DUPLICATES).loading);
    assert!(
        !fx.runtime().maintenance_loading(),
        "a duplicate scan is not maintenance of the gallery"
    );
    assert_eq!(fx.line(DUPLICATES), "Searching duplicate file names...");
    assert_eq!(
        fx.backend.progress_listeners(),
        1,
        "subscribed before the call"
    );

    fx.backend.emit_progress(progress("scanning", 1, 2));
    fx.backend.emit_progress(progress("duplicates-scan", 3, 9));
    while fx.driver.pump() > 0 {}
    assert_eq!(fx.line(DUPLICATES), "Scanning duplicates 3/9");

    fx.settle();
    let settings = fx.runtime().settings();
    assert_eq!(
        fx.line(DUPLICATES),
        "Duplicate scan complete. Groups: 1, assets: 2"
    );
    assert_eq!(settings.duplicate_groups().len(), 1);
    assert_eq!(settings.duplicate_asset_count(), 2);
    assert_eq!(settings.duplicate_scan_revision(), 7);
    assert_eq!(settings.duplicate_scan_generation(), generation + 1);
    assert!(settings.duplicate_dialog_open() && !fx.locked());
    assert_eq!(fx.backend.progress_listeners(), 0);

    fx.command(SettingsCommand::CloseDuplicateDialog);
    assert!(!fx.runtime().settings().duplicate_dialog_open());
    assert_eq!(
        fx.runtime().settings().duplicate_groups().len(),
        1,
        "the result stays"
    );
}

#[test]
fn a_failed_duplicate_scan_keeps_the_groups_it_had() {
    let mut fx = Fixture::started(|backend| {
        backend.on_find_duplicate_assets(|| Ok(one_group(7)));
    });
    fx.command(SettingsCommand::StartDuplicates);
    fx.settle();
    let generation = fx.runtime().settings().duplicate_scan_generation();
    fx.backend
        .on_find_duplicate_assets(|| Err(BackendError::rejected("i18n:common.libraryBusy")));
    fx.command(SettingsCommand::RescanDuplicates);
    fx.settle();
    let line = fx.line(DUPLICATES);
    assert!(
        line.starts_with("Duplicate scan error: A scan or another"),
        "{line}"
    );
    assert_eq!(fx.runtime().settings().duplicate_groups().len(), 1);
    assert_eq!(
        fx.runtime().settings().duplicate_scan_generation(),
        generation
    );
}

// createSettingsActions.test.tsx, "reports when duplicate-change save receives no
// changes" and "rejects duplicate rename changes with a blank file name".
#[test]
fn an_empty_batch_and_a_blank_rename_never_reach_the_backend() {
    let mut fx = Fixture::started(|backend| {
        backend.on_find_duplicate_assets(|| Ok(one_group(7)));
    });
    fx.command(SettingsCommand::StartDuplicates);
    fx.settle();

    fx.command(SettingsCommand::SaveDuplicateChanges(Vec::new()));
    assert_eq!(fx.line(DUPLICATES), "No pending duplicate changes to apply");
    assert!(!fx.locked());

    fx.command(SettingsCommand::SaveDuplicateChanges(vec![
        DuplicateChange::rename(1, "   "),
    ]));
    assert_eq!(
        fx.line(DUPLICATES),
        "Save duplicate changes error: File name cannot be empty"
    );
    fx.command(SettingsCommand::SaveDuplicateChanges(vec![
        DuplicateChange::rename(99, "other.jpg"),
    ]));
    assert_eq!(
        fx.line(DUPLICATES),
        "Save duplicate changes error: Unknown asset in queued changes: 99"
    );
    assert!(!fx.locked());
    fx.settle();
    assert_eq!(fx.calls("apply_duplicate_resolution_batch"), 0);
}

// createSettingsActionsConfirmFlow.test.tsx, "stores duplicate delete confirmation and
// cancels it without applying changes".
#[test]
fn a_batch_with_a_delete_waits_for_its_confirmation_and_cancel_applies_nothing() {
    let mut fx = Fixture::started(|backend| {
        backend.on_find_duplicate_assets(|| Ok(one_group(7)));
    });
    fx.command(SettingsCommand::StartDuplicates);
    fx.settle();

    let changes = vec![DuplicateChange::delete(1)];
    fx.command(SettingsCommand::SaveDuplicateChanges(changes.clone()));
    let pending = fx
        .runtime()
        .settings()
        .pending_duplicate_delete()
        .expect("the confirmation is open");
    assert_eq!(
        (pending.changes.clone(), pending.delete_count),
        (changes, 1)
    );
    assert!(!fx.locked());
    assert_eq!(fx.calls("apply_duplicate_resolution_batch"), 0);

    fx.command(SettingsCommand::CancelDuplicateDelete);
    fx.settle();
    assert!(fx.runtime().settings().pending_duplicate_delete().is_none());
    assert_eq!(fx.line(DUPLICATES), "Applying duplicate changes cancelled.");
    assert_eq!(fx.calls("apply_duplicate_resolution_batch"), 0);
    assert_eq!(fx.calls("delete_asset"), 0);
}

// createSettingsActionsConfirmFlow.test.tsx, "waits for confirmation before applying
// mixed duplicate changes".
#[test]
fn a_confirmed_mixed_batch_is_one_request_with_what_the_scan_saw() {
    let mut fx = Fixture::started(|backend| {
        let mut scans = vec![Ok(no_groups(8)), Ok(one_group(7))];
        backend
            .on_find_duplicate_assets(move || scans.pop().expect("two scans"))
            .on_apply_duplicate_resolution_batch(|_| {
                Ok(batch_result(
                    DuplicateResolutionBatchStatus::Committed,
                    vec![
                        item(1, DuplicateResolutionItemStatus::Renamed, None),
                        item(2, DuplicateResolutionItemStatus::Deleted, None),
                    ],
                ))
            });
    });
    fx.command(SettingsCommand::StartDuplicates);
    fx.settle();
    let queries = fx.calls("start_asset_query");
    let generation = fx.runtime().settings().duplicate_scan_generation();

    fx.command(SettingsCommand::SaveDuplicateChanges(vec![
        DuplicateChange::rename(1, " renamed.jpg "),
        DuplicateChange::delete(2),
    ]));
    assert_eq!(
        fx.runtime()
            .settings()
            .pending_duplicate_delete()
            .map(|pending| (pending.changes.len(), pending.delete_count)),
        Some((2, 1))
    );
    fx.settle();
    assert_eq!(fx.calls("apply_duplicate_resolution_batch"), 0);

    fx.command(SettingsCommand::ConfirmDuplicateDelete);
    assert!(fx.runtime().settings().pending_duplicate_delete().is_none());
    assert!(fx.locked());
    assert!(!fx.runtime().maintenance_loading());
    assert_eq!(fx.line(DUPLICATES), "Applying duplicate changes...");
    // A second confirmation finds nothing.
    fx.command(SettingsCommand::ConfirmDuplicateDelete);
    fx.settle();

    assert_eq!(fx.calls("apply_duplicate_resolution_batch"), 1);
    assert!(fx
        .backend
        .calls()
        .contains(&Call::ApplyDuplicateResolutionBatch {
            input: DuplicateResolutionBatchInput {
                scan_revision: 7,
                changes: vec![
                    DuplicateResolutionChangeInput::Rename {
                        asset_id: 1,
                        expected_path: "/media/a/same.jpg".into(),
                        expected_record_version: 3,
                        new_file_name: "renamed.jpg".into(),
                    },
                    DuplicateResolutionChangeInput::Delete {
                        asset_id: 2,
                        expected_path: "/media/b/same.jpg".into(),
                        expected_record_version: 4,
                    },
                ],
            },
        }));
    // The library was read again and the duplicates were scanned again.
    assert_eq!(fx.calls("start_asset_query"), queries + 1);
    assert_eq!(fx.calls("find_duplicate_assets"), 2);
    let settings = fx.runtime().settings();
    assert!(settings.duplicate_groups().is_empty());
    assert_eq!(settings.duplicate_scan_revision(), 8);
    assert_eq!(settings.duplicate_scan_generation(), generation + 1);
    assert_eq!(
        fx.line(DUPLICATES),
        "Applied 2 changes. Remaining groups: 0"
    );
    assert!(!fx.locked());
}

#[test]
fn a_pure_rename_batch_runs_at_once_and_its_refreshes_are_best_effort() {
    let mut fx = Fixture::started(|backend| {
        let mut scans = vec![
            Err(BackendError::rejected("the rescan failed")),
            Ok(one_group(7)),
        ];
        backend
            .on_find_duplicate_assets(move || scans.pop().expect("two scans"))
            .on_apply_duplicate_resolution_batch(|_| {
                Ok(batch_result(
                    DuplicateResolutionBatchStatus::Committed,
                    vec![item(1, DuplicateResolutionItemStatus::SourceMissing, None)],
                ))
            });
    });
    fx.command(SettingsCommand::StartDuplicates);
    fx.settle();
    let generation = fx.runtime().settings().duplicate_scan_generation();
    fx.backend
        .on_start_asset_query(|_| Err(BackendError::rejected("the query failed")));

    fx.command(SettingsCommand::SaveDuplicateChanges(vec![
        DuplicateChange::rename(1, "renamed.jpg"),
    ]));
    assert!(fx.runtime().settings().pending_duplicate_delete().is_none());
    assert!(fx.locked());
    fx.settle();

    // Neither the failed refresh nor the failed rescan hides the committed batch;
    // the groups of the scan it was staged on stay.
    assert_eq!(
        fx.line(DUPLICATES),
        "Applied 1 change. Remaining groups: 1 The source file was already missing. Its stale \
         library entry was removed. (1)"
    );
    assert_eq!(fx.runtime().settings().duplicate_groups().len(), 1);
    assert_eq!(
        fx.runtime().settings().duplicate_scan_generation(),
        generation
    );
    assert!(!fx.locked());
}

#[test]
fn a_rolled_back_batch_and_one_that_needs_recovery_say_so() {
    for (status, results, expected) in [
        (
            DuplicateResolutionBatchStatus::RolledBack,
            vec![item(1, DuplicateResolutionItemStatus::RolledBack, None)],
            "The duplicate batch failed and all file changes were rolled back. No library \
             metadata was changed."
                .to_owned(),
        ),
        (
            DuplicateResolutionBatchStatus::RecoveryRequired,
            vec![
                item(
                    1,
                    DuplicateResolutionItemStatus::RollbackFailed,
                    Some("/media/a/.same.jpg.pending"),
                ),
                item(2, DuplicateResolutionItemStatus::RolledBack, None),
            ],
            "The batch could not be fully recovered. 1 item has a reported recovery path; rescan \
             before retrying. #1: /media/a/.same.jpg.pending"
                .to_owned(),
        ),
    ] {
        let results = std::sync::Mutex::new(Some((status, results)));
        let mut fx = Fixture::started(|backend| {
            backend
                .on_find_duplicate_assets(|| Ok(one_group(7)))
                .on_apply_duplicate_resolution_batch(move |_| {
                    let (status, results) = results.lock().unwrap().take().expect("one batch");
                    Ok(batch_result(status, results))
                });
        });
        fx.command(SettingsCommand::StartDuplicates);
        fx.settle();
        let queries = fx.calls("start_asset_query");
        fx.command(SettingsCommand::SaveDuplicateChanges(vec![
            DuplicateChange::rename(1, "renamed.jpg"),
        ]));
        fx.settle();
        assert_eq!(fx.line(DUPLICATES), expected);
        // Every structured result is followed by a refresh and a rescan.
        assert_eq!(fx.calls("start_asset_query"), queries + 1);
        assert_eq!(fx.calls("find_duplicate_assets"), 2);
        assert!(!fx.locked());
    }
}

#[test]
fn a_rejected_batch_is_an_error_without_refresh_or_rescan() {
    let mut fx = Fixture::started(|backend| {
        backend
            .on_find_duplicate_assets(|| Ok(one_group(7)))
            .on_apply_duplicate_resolution_batch(|_| {
                Err(BackendError::rejected("the scan is out of date"))
            });
    });
    fx.command(SettingsCommand::StartDuplicates);
    fx.settle();
    let queries = fx.calls("start_asset_query");
    fx.command(SettingsCommand::SaveDuplicateChanges(vec![
        DuplicateChange::rename(1, "renamed.jpg"),
    ]));
    fx.settle();
    assert_eq!(
        fx.line(DUPLICATES),
        "Save duplicate changes error: the scan is out of date"
    );
    assert_eq!(fx.calls("start_asset_query"), queries);
    assert_eq!(fx.calls("find_duplicate_assets"), 1);
    assert!(!fx.locked());
}

// createSettingsActions.test.tsx, "blocks other operations while exclusive operation is
// running", for the operations of this epic.
#[test]
fn the_operations_of_both_sections_exclude_each_other_and_every_confirmation() {
    let mut fx = Fixture::started(|backend| {
        backend
            .on_find_duplicate_assets(|| Ok(one_group(7)))
            .on_inspect_db_bundle(|_| Ok(inspection(&[], false)));
    });
    // A restore waits for its confirmation, a batch with a delete for its own.
    fx.command(SettingsCommand::ImportDbBundle);
    fx.dialog(Ok(Some("/synthetic/backup.zip")));
    fx.settle();
    fx.command(SettingsCommand::StartDuplicates);
    fx.settle();
    fx.command(SettingsCommand::SaveDuplicateChanges(vec![
        DuplicateChange::delete(1),
    ]));

    // While a scan runs, nothing else starts and no confirmation is answered.
    fx.command(SettingsCommand::RescanDuplicates);
    assert!(fx.locked());
    let waiting = fx.executor.pending();
    for refused in [
        SettingsCommand::ExportCsv,
        SettingsCommand::ImportCsv,
        SettingsCommand::ExportDbBundle,
        SettingsCommand::ImportDbBundle,
        SettingsCommand::ConfirmImportDbOverwrite,
        SettingsCommand::CancelImportDbOverwrite,
        SettingsCommand::ConfirmDuplicateDelete,
        SettingsCommand::CancelDuplicateDelete,
        SettingsCommand::SaveDuplicateChanges(vec![DuplicateChange::rename(1, "x.jpg")]),
        SettingsCommand::RescanDuplicates,
        SettingsCommand::RescanAll,
        SettingsCommand::ConfirmClear,
    ] {
        fx.command(refused.clone());
        assert_eq!(fx.executor.pending(), waiting, "{refused:?} started a call");
        assert!(
            fx.runtime().settings().path_dialog_request().is_none(),
            "{refused:?}"
        );
    }
    let settings = fx.runtime().settings();
    assert_eq!(
        settings.pending_import_db_source(),
        Some("/synthetic/backup.zip")
    );
    assert!(settings.pending_duplicate_delete().is_some());
    assert_eq!(
        fx.line(IMPORT_EXPORT),
        "Preparing database backup import..."
    );
    assert!(!fx.runtime().library().maintenance_active());
    fx.settle();
    assert_eq!(fx.calls("import_db_bundle"), 0);
    assert_eq!(fx.calls("apply_duplicate_resolution_batch"), 0);
}

// statusLanguage.test.tsx, "re-translates the applied-changes summary".
#[test]
fn the_lines_of_both_sections_follow_the_language() {
    let mut fx = Fixture::started(|backend| {
        backend
            .on_find_duplicate_assets(|| Ok(one_group(7)))
            .on_apply_duplicate_resolution_batch(|_| {
                Ok(batch_result(
                    DuplicateResolutionBatchStatus::Committed,
                    vec![item(1, DuplicateResolutionItemStatus::Renamed, None)],
                ))
            });
    });
    fx.command(SettingsCommand::StartDuplicates);
    fx.settle();
    fx.command(SettingsCommand::SaveDuplicateChanges(vec![
        DuplicateChange::rename(1, "renamed.jpg"),
    ]));
    fx.settle();
    let message = &fx.runtime().settings().status(DUPLICATES).message;
    let english = message.resolve(&I18n::new(Language::En));
    let polish = message.resolve(&I18n::new(Language::Pl));
    assert_eq!(english, "Applied 1 change. Remaining groups: 1");
    assert_ne!(english, polish);
    assert!(polish.contains('1'), "{polish}");
}

/// A runtime over the real core on a fresh temporary profile, started and settled.
fn core_runtime(core: &TempCore) -> (Arc<ManualExecutor>, Driver<AppRuntime, RuntimeRunner>) {
    let (executor, mut driver) = runtime_over(core.shared());
    driver.handle(RuntimeInput::Start);
    settle(&executor, &mut driver);
    (executor, driver)
}

// Acceptance criterion: a destructive test runs on a fresh temporary directory and a
// temporary database. Duplicate resolution on real files.
#[test]
fn duplicates_are_renamed_and_deleted_on_disk_only_after_the_confirmation() {
    let core = TempCore::start();
    let root = TempCore::text(&core.media_dir());
    let first = core.write_image("a/same.png");
    let second = core.write_image("b/same.png");
    let third = core.write_image("c/same.png");
    let other = core.write_image("a/other.png");
    core.backend().add_scan_root(root.clone()).unwrap();
    assert_eq!(core.backend().scan_folder(root).unwrap().indexed, 4);

    let (executor, mut driver) = core_runtime(&core);
    assert_eq!(driver.model().query().total(), 4);
    driver.handle(RuntimeInput::settings(SettingsCommand::StartDuplicates));
    settle(&executor, &mut driver);
    assert_eq!(
        line_of(&driver, DUPLICATES),
        "Duplicate scan complete. Groups: 1, assets: 3"
    );
    let group = driver.model().settings().duplicate_groups()[0].clone();
    assert_eq!(group.file_name, "same.png");
    let id_of = |path: &std::path::Path| {
        group
            .assets
            .iter()
            .find(|asset| std::path::Path::new(&asset.path) == path)
            .unwrap_or_else(|| panic!("{} is in the group", path.display()))
            .id
    };

    // A rename and a delete: nothing happens before the confirmation.
    driver.handle(RuntimeInput::settings(
        SettingsCommand::SaveDuplicateChanges(vec![
            DuplicateChange::rename(id_of(&first), "renamed.png"),
            DuplicateChange::delete(id_of(&second)),
        ]),
    ));
    settle(&executor, &mut driver);
    assert!(driver
        .model()
        .settings()
        .pending_duplicate_delete()
        .is_some());
    assert!(first.exists() && second.exists() && third.exists());

    driver.handle(RuntimeInput::settings(
        SettingsCommand::ConfirmDuplicateDelete,
    ));
    settle(&executor, &mut driver);

    assert_eq!(
        line_of(&driver, DUPLICATES),
        "Applied 2 changes. Remaining groups: 0"
    );
    let renamed = first.with_file_name("renamed.png");
    assert!(renamed.exists() && !first.exists(), "the rename is on disk");
    assert!(!second.exists(), "the delete is on disk");
    assert!(third.exists() && other.exists(), "untouched files stay");
    assert!(driver.model().settings().duplicate_groups().is_empty());
    assert_eq!(
        driver.model().query().total(),
        3,
        "the gallery was read again"
    );
    assert!(!driver.model().settings().is_locked());
    // Nothing outside the temporary media directory was written.
    assert!(renamed.starts_with(core.media_dir()));
}

#[test]
fn a_batch_staged_on_a_stale_scan_changes_no_file() {
    let core = TempCore::start();
    let root = TempCore::text(&core.media_dir());
    let first = core.write_image("a/same.png");
    let second = core.write_image("b/same.png");
    core.backend().add_scan_root(root.clone()).unwrap();
    core.backend().scan_folder(root.clone()).unwrap();

    let (executor, mut driver) = core_runtime(&core);
    driver.handle(RuntimeInput::settings(SettingsCommand::StartDuplicates));
    settle(&executor, &mut driver);
    let group = driver.model().settings().duplicate_groups()[0].clone();

    // The library changes behind the scan's back.
    core.write_image("c/new.png");
    core.backend().scan_folder(root).unwrap();

    driver.handle(RuntimeInput::settings(
        SettingsCommand::SaveDuplicateChanges(vec![DuplicateChange::rename(
            group.assets[0].id,
            "renamed.png",
        )]),
    ));
    settle(&executor, &mut driver);

    let line = line_of(&driver, DUPLICATES);
    assert!(line.starts_with("Save duplicate changes error: "), "{line}");
    assert!(first.exists() && second.exists(), "no file was touched");
    assert!(!first.with_file_name("renamed.png").exists());
    assert!(!second.with_file_name("renamed.png").exists());
}

// Acceptance criterion: restore and CSV import pass the mutation barrier; both on a
// fresh temporary profile.
#[test]
fn a_backup_restores_the_library_it_was_made_of() {
    let core = TempCore::start();
    let root = TempCore::text(&core.media_dir());
    core.write_image("one.png");
    core.write_image("two.png");
    core.backend().add_scan_root(root.clone()).unwrap();
    core.backend().scan_folder(root).unwrap();
    let archive = TempCore::text(&core.root().join("backup.zip"));

    let (executor, mut driver) = core_runtime(&core);
    assert_eq!(driver.model().query().total(), 2);

    driver.handle(RuntimeInput::settings(SettingsCommand::ExportDbBundle));
    answer_dialog(&mut driver, Ok(Some(&archive)));
    settle(&executor, &mut driver);
    let exported = line_of(&driver, IMPORT_EXPORT);
    assert!(
        exported.starts_with("Database backup archive exported. Files: "),
        "{exported}"
    );
    assert!(std::path::Path::new(&archive).is_file());

    // The library is emptied, then brought back from the archive.
    driver.handle(RuntimeInput::settings(SettingsCommand::OpenClearConfirm));
    driver.handle(RuntimeInput::settings(SettingsCommand::ConfirmClear));
    settle(&executor, &mut driver);
    assert_eq!(driver.model().query().total(), 0);
    assert!(!driver.model().has_scan_roots());

    driver.handle(RuntimeInput::settings(SettingsCommand::ImportDbBundle));
    answer_dialog(&mut driver, Ok(Some(&archive)));
    settle(&executor, &mut driver);
    assert_eq!(
        driver.model().settings().pending_import_db_source(),
        Some(archive.as_str()),
        "{}",
        line_of(&driver, IMPORT_EXPORT)
    );
    assert_eq!(driver.model().query().total(), 0, "nothing is restored yet");

    driver.handle(RuntimeInput::settings(
        SettingsCommand::ConfirmImportDbOverwrite,
    ));
    settle(&executor, &mut driver);
    let restored = line_of(&driver, IMPORT_EXPORT);
    assert!(
        restored.starts_with("Database backup archive imported. Files: "),
        "{restored}"
    );
    assert_eq!(driver.model().query().total(), 2);
    assert!(driver.model().has_scan_roots());
    assert!(!driver.model().settings().is_locked());
    assert!(!driver.model().library().maintenance_active());
}

#[test]
fn a_csv_export_imports_back_into_the_library_it_came_from() {
    let core = TempCore::start();
    let root = TempCore::text(&core.media_dir());
    core.write_image("one.png");
    core.backend().add_scan_root(root.clone()).unwrap();
    core.backend().scan_folder(root).unwrap();
    let csv = TempCore::text(&core.root().join("tags.csv"));

    let (executor, mut driver) = core_runtime(&core);
    driver.handle(RuntimeInput::settings(SettingsCommand::ExportCsv));
    answer_dialog(&mut driver, Ok(Some(&csv)));
    settle(&executor, &mut driver);
    assert_eq!(line_of(&driver, IMPORT_EXPORT), "CSV exported. Rows: 1");

    driver.handle(RuntimeInput::settings(SettingsCommand::ImportCsv));
    answer_dialog(&mut driver, Ok(Some(&csv)));
    settle(&executor, &mut driver);
    let line = line_of(&driver, IMPORT_EXPORT);
    assert!(line.starts_with("CSV imported. Rows read: 1, "), "{line}");
    assert_eq!(driver.model().query().total(), 1);
    assert!(!driver.model().library().maintenance_active());
    assert!(!driver.model().settings().is_locked());
}
