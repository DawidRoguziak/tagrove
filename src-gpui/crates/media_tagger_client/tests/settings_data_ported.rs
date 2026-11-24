//! Ported behaviour of the import/export and duplicate settings (epic E18), case by
//! case from the TypeScript tests in `src/features/settings/__tests__`:
//! the batch rules of `duplicateValidationService.test.ts`, the resolver form of
//! `createDuplicateResolver.test.tsx`, the plural texts of `pluralCopy.test.tsx` and
//! the dialog contract of the exports from `createSettingsActions.test.tsx`. The
//! cases that need the settings model run in the application runtime over the scripted
//! backend on the deterministic executor; nothing here touches a file.

use std::{collections::BTreeMap, sync::Arc};

use media_tagger_client::{
    backend::{fake::Call, Backend, FakeBackend},
    exec::{Executor, ManualExecutor},
    i18n::{format_count, I18n, Language, Params},
    model::Driver,
    prefs::MemoryPrefsStore,
    runtime::{AppRuntime, RuntimeConfig, RuntimeInput, RuntimeRunner},
    settings::{
        duplicates::{
            file_name_from_path, is_group_resolved, random_uuid, validate_changes, Validation,
        },
        transfer::{export_file_name, is_export_timestamp, utc_export_timestamp},
        DuplicateAction, DuplicateChange, DuplicateResolver, OperationSection, PathDialogKind,
        SettingsCommand, SettingsInput,
    },
    theme::ThemeName,
};
use media_tagger_core::models::{
    DuplicateAsset, DuplicateGroup, DuplicateResolutionBatchInput, DuplicateResolutionBatchStatus,
    DuplicateResolutionBatchSummary, DuplicateResolutionChangeInput, DuplicateScanSummary,
    ScanRoot, StartAssetQueryResult, TagListPage, VideoToolStatus,
};

fn en() -> I18n<'static> {
    I18n::new(Language::En)
}

fn first_error(validation: &Validation) -> Option<String> {
    validation.first_error().map(|error| error.resolve(&en()))
}

/// `makeGroup` of duplicateValidationService.test.ts.
fn make_group(file_name: &str, ids: &[i64]) -> DuplicateGroup {
    DuplicateGroup {
        file_name: file_name.to_owned(),
        assets: ids
            .iter()
            .enumerate()
            .map(|(index, id)| DuplicateAsset {
                id: *id,
                path: format!("C:/media/folder-{}/{file_name}", index + 1),
                record_version: 1,
                size_bytes: 10,
                thumb_version: "0123456789abcdef".into(),
            })
            .collect(),
    }
}

// duplicateValidationService.test.ts, "accepts three changes for a 4-item duplicate
// block".
#[test]
fn three_renames_resolve_a_block_of_four() {
    let groups = [make_group("same.jpg", &[1, 2, 3, 4])];
    let result = validate_changes(
        &groups,
        &[
            DuplicateChange::rename(1, "same-1.jpg"),
            DuplicateChange::rename(2, "same-2.jpg"),
            DuplicateChange::rename(3, "same-3.jpg"),
        ],
    );
    assert!(result.valid());
    assert_eq!(first_error(&result), None);
}

// "rejects block when queued changes still leave duplicate names".
#[test]
fn two_renames_leave_a_block_of_four_unresolved() {
    let groups = [make_group("same.jpg", &[1, 2, 3, 4])];
    let result = validate_changes(
        &groups,
        &[
            DuplicateChange::rename(1, "same-1.jpg"),
            DuplicateChange::rename(2, "same-2.jpg"),
        ],
    );
    assert!(!result.valid());
    let error = first_error(&result).expect("an error");
    assert!(
        error.contains("still leave duplicate file names"),
        "{error}"
    );
}

// "accepts delete operations when they leave one item in block".
#[test]
fn deletes_that_leave_one_file_resolve_the_block() {
    let groups = [make_group("same.jpg", &[1, 2, 3, 4])];
    let result = validate_changes(
        &groups,
        &[
            DuplicateChange::delete(1),
            DuplicateChange::delete(2),
            DuplicateChange::delete(3),
        ],
    );
    assert!(result.valid(), "{:?}", first_error(&result));
}

// "validates rename file name with zod schema".
#[test]
fn a_rename_with_a_slash_has_invalid_characters() {
    let groups = [make_group("same.jpg", &[1, 2])];
    let result = validate_changes(&groups, &[DuplicateChange::rename(1, "bad/name.jpg")]);
    assert!(!result.valid());
    let error = first_error(&result).expect("an error");
    assert!(error.contains("invalid characters"), "{error}");
}

// "rejects staged change for unknown asset id".
#[test]
fn a_change_of_an_unknown_asset_is_refused() {
    let groups = [make_group("same.jpg", &[1, 2])];
    let result = validate_changes(&groups, &[DuplicateChange::rename(99, "other.jpg")]);
    assert!(!result.valid());
    let error = first_error(&result).expect("an error");
    assert!(
        error.contains("Unknown asset in queued changes: 99"),
        "{error}"
    );
}

// "treats file name collisions as case-insensitive".
#[test]
fn names_that_differ_only_in_case_still_collide() {
    let group = make_group("same.jpg", &[1, 2]);
    let staged = BTreeMap::from([(
        1,
        DuplicateAction::Rename {
            next_file_name: "SAME.JPG".into(),
        },
    )]);
    assert!(!is_group_resolved(&group, &staged));
}

// "does not fail untouched duplicate groups".
#[test]
fn an_untouched_group_does_not_fail_the_batch() {
    let groups = [
        make_group("same.jpg", &[1, 2]),
        make_group("other.jpg", &[3, 4]),
    ];
    let result = validate_changes(&groups, &[DuplicateChange::rename(1, "same-a.jpg")]);
    assert!(result.valid(), "{:?}", first_error(&result));
}

// "rejects special dot names as invalid file names".
#[test]
fn a_lone_dot_is_an_invalid_name() {
    let groups = [make_group("same.jpg", &[1, 2])];
    let result = validate_changes(&groups, &[DuplicateChange::rename(1, ".")]);
    assert!(!result.valid());
    let error = first_error(&result).expect("an error");
    assert!(error.contains("invalid"), "{error}");
}

// "extracts file name from unix and windows path".
#[test]
fn the_file_name_comes_from_slashes_and_backslashes() {
    assert_eq!(file_name_from_path("C:/media/folder/name.jpg"), "name.jpg");
    assert_eq!(
        file_name_from_path("C:\\media\\folder\\name.jpg"),
        "name.jpg"
    );
}

/// The groups of createDuplicateResolver.test.tsx: assets 1 and 7.
fn resolver_groups() -> Vec<DuplicateGroup> {
    let asset = |id: i64, folder: &str| DuplicateAsset {
        id,
        path: format!("/synthetic/{folder}/same.jpg"),
        record_version: 1,
        size_bytes: 10,
        thumb_version: "0123456789abcdef".into(),
    };
    vec![DuplicateGroup {
        file_name: "same.jpg".into(),
        assets: vec![asset(1, "a"), asset(7, "b")],
    }]
}

fn opened_form(groups: &[DuplicateGroup]) -> DuplicateResolver {
    let mut form = DuplicateResolver::new();
    form.reset(groups);
    form
}

// createDuplicateResolver.test.tsx, "updates a staged rename, preserving the input
// draft and trimming the command" (the form's part; what Save sends is below).
#[test]
fn a_staged_rename_follows_the_draft_and_is_trimmed() {
    let groups = resolver_groups();
    let mut form = opened_form(&groups);
    form.rename(1, "first.jpg");
    form.queue_rename(1);
    form.rename(1, " next-name.png ");
    assert_eq!(
        form.draft(1),
        Some(" next-name.png "),
        "the draft is kept as typed"
    );
    assert_eq!(
        form.staged_changes(),
        [DuplicateChange::rename(1, "next-name.png")]
    );
}

// "queues by ID, replaces a rename with delete, unmarks and clears without stale reads".
#[test]
fn a_delete_replaces_a_rename_and_toggles_and_clear_take_it_back() {
    let groups = resolver_groups();
    let mut form = opened_form(&groups);
    form.rename(7, " queued.jpg ");
    form.queue_rename(7);
    assert_eq!(
        form.staged_changes(),
        [DuplicateChange::rename(7, "queued.jpg")]
    );
    form.toggle_delete(7);
    assert_eq!(form.staged_changes(), [DuplicateChange::delete(7)]);
    form.toggle_delete(7);
    form.toggle_delete(7);
    form.toggle_delete(7);
    assert_eq!(form.staged_changes(), []);
    form.queue_rename(7);
    form.clear(7);
    assert_eq!(form.staged_changes(), []);
}

// "generates a draft with its extension without staging, and resets on reopen or scan
// replacement". The reset is `DuplicateResolver::reset`, which the page calls when the
// dialog opens and when a scan replaces the groups (settings_data_ported.rs of the
// adapter proves that it does).
#[test]
fn a_generated_draft_keeps_the_extension_and_a_reset_forgets_everything() {
    let groups = resolver_groups();
    let mut form = opened_form(&groups);
    let draft = form.generate_name(1, "same.jpg", &random_uuid());
    assert_eq!(form.draft(1), Some(draft.as_str()));
    let stem = draft.strip_suffix(".jpg").expect("the extension is kept");
    assert!(
        !stem.is_empty()
            && stem
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
        "{draft}"
    );
    assert_eq!(form.staged_changes(), []);
    form.queue_rename(1);
    assert!(form.validation(&groups).valid());

    // Closed and opened again.
    form.reset(&groups);
    assert_eq!(form.draft(1), Some("same.jpg"));
    assert_eq!(form.staged_changes(), []);

    form.rename(1, "renamed.jpg");
    form.queue_rename(1);
    // A scan that found nothing replaces the groups.
    form.reset(&[]);
    assert_eq!(form.staged_changes(), []);
    assert_eq!(form.draft(1), None);
    assert_eq!(form.draft(7), None);
}

// pluralCopy.test.tsx, "duplicate delete confirmation plurals": the paragraph of the
// confirmation, two sentences with an empty line between them.
#[test]
fn the_delete_confirmation_counts_changes_and_files_in_each_language() {
    for (language, count, deletes, expected) in [
        (
            Language::En,
            1,
            1,
            "Apply 1 queued change?\n\nThis will permanently delete 1 file from disk.",
        ),
        (
            Language::En,
            2,
            2,
            "Apply 2 queued changes?\n\nThis will permanently delete 2 files from disk.",
        ),
        (
            Language::Pl,
            1,
            1,
            "Zastosować 1 zmianę z kolejki?\n\nTo trwale usunie 1 plik z dysku.",
        ),
        (
            Language::Pl,
            2,
            2,
            "Zastosować 2 zmiany z kolejki?\n\nTo trwale usunie 2 pliki z dysku.",
        ),
        (
            Language::Pl,
            5,
            5,
            "Zastosować 5 zmian z kolejki?\n\nTo trwale usunie 5 plików z dysku.",
        ),
        (
            Language::Pl,
            5,
            1,
            "Zastosować 5 zmian z kolejki?\n\nTo trwale usunie 1 plik z dysku.",
        ),
    ] {
        let i18n = I18n::new(language);
        let text = format!(
            "{}\n\n{}",
            i18n.t_with(
                "settings.actions.dialogs.confirmDuplicateChanges",
                &Params::new().with_count(count)
            ),
            i18n.t_with(
                "settings.actions.dialogs.confirmDuplicateDeletes",
                &Params::new().with_count(deletes)
            ),
        );
        assert_eq!(text, expected, "{language:?} {count} {deletes}");
    }
}

// pluralCopy.test.tsx, "duplicates last result plurals": the line of the section, as the
// section builds it.
#[test]
fn the_last_result_line_counts_groups_and_assets_in_each_language() {
    for (language, groups, assets, expected) in [
        (Language::En, 1, 1, "Last result: 1 group / 1 asset"),
        (Language::En, 2, 5, "Last result: 2 groups / 5 assets"),
        (Language::Pl, 1, 1, "Ostatni wynik: 1 grupa / 1 zasób"),
        (Language::Pl, 2, 2, "Ostatni wynik: 2 grupy / 2 zasoby"),
        (Language::Pl, 5, 5, "Ostatni wynik: 5 grup / 5 zasobów"),
    ] {
        let i18n = I18n::new(language);
        let counted = |key: &str, value: i64| {
            i18n.t_with(
                key,
                &Params::new()
                    .with_count(value)
                    .with("formattedCount", format_count(language, value)),
            )
        };
        let line = i18n.t_with(
            "settings.duplicates.lastResult",
            &Params::new()
                .with(
                    "groups",
                    counted("settings.duplicates.lastResultGroups", groups),
                )
                .with(
                    "assets",
                    counted("settings.duplicates.lastResultAssets", assets),
                ),
        );
        assert_eq!(line, expected, "{language:?} {groups} {assets}");
    }
}

// pluralCopy.test.tsx, "count strings use plural keys", the rows of this epic.
#[test]
fn the_batch_summaries_use_plural_keys() {
    for (language, key, count, expected) in [
        (
            Language::En,
            "settings.actions.summary.duplicateApplied",
            1,
            "Applied 1 change. Remaining groups: 3",
        ),
        (
            Language::En,
            "settings.actions.summary.duplicateApplied",
            5,
            "Applied 5 changes. Remaining groups: 3",
        ),
        (
            Language::Pl,
            "settings.actions.summary.duplicateApplied",
            1,
            "Zastosowano 1 zmianę. Pozostałe grupy: 3",
        ),
        (
            Language::Pl,
            "settings.actions.summary.duplicateApplied",
            2,
            "Zastosowano 2 zmiany. Pozostałe grupy: 3",
        ),
        (
            Language::Pl,
            "settings.actions.summary.duplicateApplied",
            5,
            "Zastosowano 5 zmian. Pozostałe grupy: 3",
        ),
        (
            Language::En,
            "settings.actions.summary.duplicateRecoveryRequired",
            1,
            "The batch could not be fully recovered. 1 item has a reported recovery path; \
             rescan before retrying.",
        ),
        (
            Language::En,
            "settings.actions.summary.duplicateRecoveryRequired",
            2,
            "The batch could not be fully recovered. 2 items have a reported recovery path; \
             rescan before retrying.",
        ),
    ] {
        let i18n = I18n::new(language);
        let text = i18n.t_with(
            key,
            &Params::new()
                .with_count(count)
                .with("formattedCount", count.to_string())
                .with("groups", 3),
        );
        assert_eq!(text, expected, "{language:?} {key} {count}");
    }
}

// ---------------------------------------------------------------------------------
// The settings model in the runtime.

struct Fixture {
    backend: Arc<FakeBackend>,
    executor: Arc<ManualExecutor>,
    driver: Driver<AppRuntime, RuntimeRunner>,
}

impl Fixture {
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
        let executor = Arc::new(ManualExecutor::new());
        let driver = Driver::new(
            AppRuntime::new(RuntimeConfig {
                theme: ThemeName::Dark,
                language: Language::En,
            }),
            RuntimeRunner::new(
                backend.clone() as Arc<dyn Backend>,
                Arc::new(MemoryPrefsStore::default()),
            ),
            executor.clone() as Arc<dyn Executor>,
        );
        let mut fixture = Self {
            backend,
            executor,
            driver,
        };
        fixture.driver.handle(RuntimeInput::Start);
        fixture.settle();
        fixture
    }

    fn command(&mut self, command: SettingsCommand) {
        self.driver.handle(RuntimeInput::settings(command));
    }

    fn settle(&mut self) {
        while self.executor.run_all() > 0 || self.driver.pump() > 0 {}
    }

    fn line(&self, section: OperationSection) -> String {
        self.driver
            .model()
            .settings()
            .status(section)
            .message
            .resolve(&en())
    }

    fn calls(&self, method: &str) -> usize {
        self.backend
            .called_methods()
            .into_iter()
            .filter(|called| *called == method)
            .count()
    }
}

/// Whether `name` is `<prefix>-YYYY-MM-DD_HH-mm.<extension>`.
fn is_timestamped(name: &str, prefix: &str, extension: &str) -> bool {
    name.strip_prefix(prefix)
        .and_then(|rest| rest.strip_prefix('-'))
        .and_then(|rest| rest.strip_suffix(extension))
        .and_then(|rest| rest.strip_suffix('.'))
        .is_some_and(is_export_timestamp)
}

// createSettingsActions.test.tsx, "preserves the timestamped filename and dialog
// contract for %s": the title, the suggested name and the filter of both exports.
#[test]
fn both_exports_ask_for_a_timestamped_file_under_their_own_title() {
    let mut fx = Fixture::started(|_| {});
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("after 1970")
        .as_secs();
    for (command, title, prefix, extension) in [
        (
            SettingsCommand::ExportCsv,
            "Export tags CSV",
            "tags-export",
            "csv",
        ),
        (
            SettingsCommand::ExportDbBundle,
            "Export DB backup archive",
            "media-backup",
            "zip",
        ),
    ] {
        fx.command(command.clone());
        let request = fx
            .driver
            .model_mut()
            .take_path_dialog_request()
            .expect("a save dialog is asked for");
        assert_eq!(request.title.resolve(&en()), title, "{command:?}");
        let PathDialogKind::SaveFile {
            prefix: asked_prefix,
            extension: asked_extension,
        } = request.kind
        else {
            panic!("{command:?} asks for a save dialog, not {:?}", request.kind);
        };
        assert_eq!((asked_prefix, asked_extension), (prefix, extension));
        let name = export_file_name(asked_prefix, &utc_export_timestamp(now), asked_extension);
        assert!(is_timestamped(&name, prefix, extension), "{name}");

        // Closed without a file.
        fx.driver
            .handle(RuntimeInput::Settings(SettingsInput::PathPicked {
                op: request.op,
                result: Ok(None),
            }));
        fx.settle();
    }
    assert_eq!(
        fx.calls("export_tags_csv") + fx.calls("export_db_bundle"),
        0
    );
}

// createDuplicateResolver.test.tsx, "updates a staged rename ...": `save()` hands over the
// staged changes, and the batch carries the trimmed name with what the scan saw.
#[test]
fn what_the_form_staged_is_the_batch_that_is_sent() {
    let groups = resolver_groups();
    let scanned = groups.clone();
    let mut fx = Fixture::started(move |backend| {
        backend
            .on_find_duplicate_assets(move || {
                Ok(DuplicateScanSummary {
                    groups: scanned.clone(),
                    duplicate_groups: 1,
                    duplicate_assets: 2,
                    revision: 4,
                })
            })
            .on_apply_duplicate_resolution_batch(|_| {
                Ok(DuplicateResolutionBatchSummary {
                    status: DuplicateResolutionBatchStatus::Committed,
                    revision: 5,
                    results: Vec::new(),
                    removed_thumbnails: 0,
                })
            });
    });
    fx.command(SettingsCommand::StartDuplicates);
    fx.settle();

    let mut form = opened_form(fx.driver.model().settings().duplicate_groups());
    form.rename(1, "first.jpg");
    form.queue_rename(1);
    form.rename(1, " next-name.png ");
    fx.command(SettingsCommand::SaveDuplicateChanges(form.staged_changes()));
    fx.settle();

    assert!(fx
        .backend
        .calls()
        .contains(&Call::ApplyDuplicateResolutionBatch {
            input: DuplicateResolutionBatchInput {
                scan_revision: 4,
                changes: vec![DuplicateResolutionChangeInput::Rename {
                    asset_id: 1,
                    expected_path: "/synthetic/a/same.jpg".into(),
                    expected_record_version: 1,
                    new_file_name: "next-name.png".into(),
                }],
            },
        }));
    assert_eq!(fx.calls("apply_duplicate_resolution_batch"), 1);
}

// DuplicatesSection.test.tsx, "disables start while locked", for the model: a Start
// while another operation holds the lock scans nothing.
#[test]
fn a_start_while_another_operation_runs_scans_nothing() {
    let mut fx = Fixture::started(|backend| {
        backend.on_find_duplicate_assets(|| {
            Ok(DuplicateScanSummary {
                groups: resolver_groups(),
                duplicate_groups: 1,
                duplicate_assets: 2,
                revision: 4,
            })
        });
    });
    fx.command(SettingsCommand::ExportCsv);
    assert!(fx.driver.model().settings().is_locked());
    let generation = fx.driver.model().settings().duplicate_scan_generation();
    fx.command(SettingsCommand::StartDuplicates);
    fx.settle();
    assert_eq!(fx.calls("find_duplicate_assets"), 0);
    assert!(fx.driver.model().settings().duplicate_groups().is_empty());
    assert_eq!(
        fx.driver.model().settings().duplicate_scan_generation(),
        generation
    );
    assert_eq!(
        fx.line(OperationSection::ImportExport),
        "Preparing CSV export...",
        "the export keeps its line"
    );
}
