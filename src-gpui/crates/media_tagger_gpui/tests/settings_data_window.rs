//! The import/export and duplicate sections of the settings page (epic E18) on GPUI's
//! test platform with the scripted backend and the deterministic executor: the four
//! import/export actions through the platform's file dialogs, the overwrite
//! confirmation of a restore, the duplicate resolver with its cards, previews and
//! staged changes, and the confirmation of a batch that deletes files.

use std::{path::PathBuf, sync::Arc};

use gpui::{Focusable as _, Modifiers, TestAppContext, VisualTestContext, WindowHandle};
use media_tagger_client::{
    backend::{fake::Call, BackendError, FakeBackend},
    exec::{Executor, ManualExecutor},
    i18n::{I18n, Language},
    prefs::MemoryPrefsStore,
    runtime::{RuntimeConfig, RuntimeInput},
    scenario::{
        ScenarioDuplicates, DUPLICATES_RESOLVER, SETTINGS_DUPLICATES, SETTINGS_IMPORT_EXPORT,
    },
    settings::{
        DuplicateAction, DuplicateChange, OperationSection, SettingsCommand, SettingsSection,
    },
    theme::ThemeName,
};
use media_tagger_core::models::{
    CsvExportSummary, DbBundleImportSummary, DbBundleInspection, DuplicateAsset, DuplicateGroup,
    DuplicateResolutionBatchInput, DuplicateResolutionBatchStatus, DuplicateResolutionBatchSummary,
    DuplicateResolutionChangeInput, DuplicateScanSummary, ScanRoot, StartAssetQueryResult,
    TagListPage, VideoToolStatus,
};
use media_tagger_gpui::{
    profile,
    runtime::RuntimeParts,
    views::{
        settings::{self, AssetControl},
        shell::ShellView,
    },
    window::{init_application, open_main_window},
};

struct Fixture {
    backend: Arc<FakeBackend>,
    executor: Arc<ManualExecutor>,
}

fn asset(id: i64, path: &str) -> DuplicateAsset {
    DuplicateAsset {
        id,
        path: path.to_owned(),
        record_version: id + 2,
        size_bytes: 10,
        thumb_version: format!("{id:016x}"),
    }
}

fn groups(count: usize) -> DuplicateScanSummary {
    let groups: Vec<DuplicateGroup> = (0..count)
        .map(|index| {
            let first = i64::try_from(index).expect("a small index") * 2 + 1;
            DuplicateGroup {
                file_name: format!("same-{index}.jpg"),
                assets: vec![
                    asset(first, &format!("/media/a/same-{index}.jpg")),
                    asset(first + 1, &format!("/media/b/same-{index}.jpg")),
                ],
            }
        })
        .collect();
    DuplicateScanSummary {
        duplicate_groups: groups.len(),
        duplicate_assets: groups.len() * 2,
        groups,
        revision: 7,
    }
}

impl Fixture {
    fn new() -> Self {
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
                    path: "/media/synthetic".into(),
                    auto_scan_on_startup: false,
                }])
            })
            .on_get_video_tool_status(|| {
                Ok(VideoToolStatus {
                    ffmpeg_available: true,
                    ffprobe_available: true,
                })
            });
        Self {
            backend,
            executor: Arc::new(ManualExecutor::new()),
        }
    }

    fn parts(&self) -> RuntimeParts {
        RuntimeParts {
            config: RuntimeConfig {
                theme: ThemeName::Dark,
                language: Language::En,
            },
            backend: self.backend.clone(),
            prefs: Arc::new(MemoryPrefsStore::default()),
            executor: self.executor.clone() as Arc<dyn Executor>,
        }
    }

    fn calls(&self, method: &str) -> usize {
        self.backend
            .called_methods()
            .into_iter()
            .filter(|called| *called == method)
            .count()
    }

    /// The assets whose previews were asked for.
    fn previews(&self) -> Vec<i64> {
        let mut ids: Vec<i64> = self
            .backend
            .calls()
            .into_iter()
            .filter_map(|call| match call {
                Call::RequestThumbnail { request } => Some(request.asset_id),
                _ => None,
            })
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    }
}

fn settle(fixture: &Fixture, cx: &mut VisualTestContext) {
    loop {
        cx.run_until_parked();
        if fixture.executor.run_all() == 0 {
            break;
        }
    }
    cx.run_until_parked();
}

fn open_settings(
    fixture: &Fixture,
    cx: &mut TestAppContext,
) -> (WindowHandle<ShellView>, VisualTestContext) {
    let window = cx
        .update(|cx| {
            init_application(ThemeName::Dark, cx)?;
            open_main_window(profile::E2E, fixture.parts(), cx)
        })
        .expect("open the main window");
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    settle(fixture, &mut cx);
    send(&window, RuntimeInput::OpenSettings, fixture, &mut cx);
    (window, cx)
}

fn send(
    window: &WindowHandle<ShellView>,
    input: RuntimeInput,
    fixture: &Fixture,
    cx: &mut VisualTestContext,
) {
    let runtime = window
        .read_with(cx, |shell, _| shell.runtime().clone())
        .expect("the window is open");
    runtime.update(cx, |runtime, cx| runtime.send(input, cx));
    settle(fixture, cx);
}

fn drawn(selector: &str, cx: &mut VisualTestContext) -> bool {
    let selector: &'static str = Box::leak(selector.to_owned().into_boxed_str());
    cx.debug_bounds(selector).is_some()
}

fn click(selector: &str, fixture: &Fixture, cx: &mut VisualTestContext) {
    let leaked: &'static str = Box::leak(selector.to_owned().into_boxed_str());
    let at = cx
        .debug_bounds(leaked)
        .unwrap_or_else(|| panic!("{selector} is drawn"))
        .center();
    cx.simulate_click(at, Modifiers::none());
    settle(fixture, cx);
}

fn view(
    window: &WindowHandle<ShellView>,
    cx: &VisualTestContext,
) -> gpui::Entity<settings::SettingsView> {
    window
        .read_with(cx, |shell, _| shell.settings().cloned())
        .unwrap()
        .expect("the settings page is shown")
}

/// Scrolls `section` to the top of the page, as its navigation link does: the two
/// sections of this epic lie below the first screen of a 900 px window, and a click
/// lands only on what is in the window.
fn show(window: &WindowHandle<ShellView>, section: SettingsSection, cx: &mut VisualTestContext) {
    let page = view(window, cx);
    page.update(cx, |page, cx| page.navigate(section, cx));
    cx.run_until_parked();
}

/// Lets a dialog that was just closed finish fading out. Until then it is still drawn
/// and takes every click, as the exiting dialog of the Tauri page does.
fn fade_out(cx: &mut VisualTestContext) {
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(500));
    cx.run_until_parked();
}

/// A value of the settings model.
fn model<T>(
    window: &WindowHandle<ShellView>,
    cx: &VisualTestContext,
    read: impl FnOnce(&media_tagger_client::settings::Settings) -> T,
) -> T {
    window
        .read_with(cx, |shell, cx| {
            read(shell.runtime().read(cx).model().settings())
        })
        .expect("the window is open")
}

fn line(
    window: &WindowHandle<ShellView>,
    section: OperationSection,
    cx: &VisualTestContext,
) -> String {
    model(window, cx, |settings| {
        settings
            .status(section)
            .message
            .resolve(&I18n::new(Language::En))
    })
}

/// The duplicate groups of the scan the page shows.
fn scan_groups(cx: &gpui::App) -> Vec<DuplicateGroup> {
    cx.windows()
        .first()
        .and_then(|window| window.downcast::<ShellView>())
        .and_then(|window| window.read(cx).ok())
        .map(|shell| {
            shell
                .runtime()
                .read(cx)
                .model()
                .settings()
                .duplicate_groups()
                .to_vec()
        })
        .unwrap_or_default()
}

fn control(control: AssetControl, asset_id: i64) -> String {
    settings::asset_control_id(control, asset_id).to_string()
}

/// The thumbnails that are asked for and pinned now. The gallery is not shown and the
/// library is empty, so these are the previews of the resolver.
fn previews_in_demand(window: &WindowHandle<ShellView>, cx: &VisualTestContext) -> usize {
    window
        .read_with(cx, |shell, cx| {
            shell
                .runtime()
                .read(cx)
                .thumbnails()
                .read(cx)
                .tiles_in_range()
        })
        .expect("the window is open")
}

fn page_shown(window: &WindowHandle<ShellView>, cx: &VisualTestContext) -> bool {
    window
        .read_with(cx, |shell, _| shell.settings().is_some())
        .expect("the window is open")
}

/// Sends a command to the settings model without running what it starts, so the
/// operation it began is still running afterwards.
fn begin(window: &WindowHandle<ShellView>, command: SettingsCommand, cx: &mut VisualTestContext) {
    let runtime = window
        .read_with(cx, |shell, _| shell.runtime().clone())
        .expect("the window is open");
    runtime.update(cx, |runtime, cx| {
        runtime.send(RuntimeInput::settings(command), cx);
    });
    cx.run_until_parked();
}

// ImportExportSection.test.tsx: "triggers import-export actions" and "disables all
// actions when operation is locked".
#[gpui::test]
fn the_import_export_section_asks_the_platform_for_a_file_and_exports_to_it(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new();
    fixture
        .backend
        .on_export_tags_csv(|_| Ok(CsvExportSummary { rows: 4 }));
    let (window, mut cx) = open_settings(&fixture, cx);
    for selector in [
        settings::IMPORT_EXPORT_SECTION_SELECTOR,
        settings::EXPORT_CSV_ID,
        settings::IMPORT_CSV_ID,
        settings::EXPORT_DB_BUNDLE_ID,
        settings::IMPORT_DB_BUNDLE_ID,
        settings::DUPLICATES_SECTION_SELECTOR,
        settings::START_DUPLICATES_ID,
    ] {
        assert!(drawn(selector, &mut cx), "{selector} is drawn");
    }
    assert!(!drawn(settings::DUPLICATES_LAST_RESULT_SELECTOR, &mut cx));

    // The navigation brings the section to the page's scroll margin.
    show(&window, SettingsSection::ImportExport, &mut cx);
    let top = cx
        .debug_bounds(settings::IMPORT_EXPORT_SECTION_SELECTOR)
        .expect("the section is drawn")
        .top();
    assert_eq!(
        top,
        gpui::px(96.),
        "the section starts at the scroll margin"
    );

    click(settings::EXPORT_CSV_ID, &fixture, &mut cx);
    assert!(cx.did_prompt_for_new_path(), "a save dialog is open");
    assert!(model(&window, &cx, |settings| settings.is_locked()));
    assert!(drawn(settings::IMPORT_EXPORT_LOADER_SELECTOR, &mut cx));
    // Locked: a click on another action starts nothing.
    click(settings::IMPORT_DB_BUNDLE_ID, &fixture, &mut cx);
    assert!(!cx.did_prompt_for_paths());

    cx.simulate_new_path_selection(|_| Some(PathBuf::from("/synthetic/out/tags.csv")));
    settle(&fixture, &mut cx);
    assert_eq!(
        line(&window, OperationSection::ImportExport, &cx),
        "CSV exported. Rows: 4"
    );
    assert!(fixture.backend.calls().contains(&Call::ExportTagsCsv {
        path: "/synthetic/out/tags.csv".into()
    }));
    assert!(!model(&window, &cx, |settings| settings.is_locked()));
    assert!(!drawn(settings::IMPORT_EXPORT_LOADER_SELECTOR, &mut cx));

    // A dialog closed without a file is a cancellation.
    click(settings::EXPORT_DB_BUNDLE_ID, &fixture, &mut cx);
    cx.simulate_new_path_selection(|_| None);
    settle(&fixture, &mut cx);
    assert_eq!(
        line(&window, OperationSection::ImportExport, &cx),
        "Database backup export cancelled."
    );
    assert_eq!(fixture.calls("export_db_bundle"), 0);
}

// ImportDbOverwriteConfirmDialog and createSettingsActionsConfirmFlow.test.tsx: the
// restore's dialogs, driven by pointer.
#[gpui::test]
fn a_restore_maps_its_roots_and_runs_only_after_the_overwrite_confirmation(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new();
    fixture
        .backend
        .on_inspect_db_bundle(|_| {
            Ok(DbBundleInspection {
                format_version: Some(1),
                source_platform: Some("linux".into()),
                source_thumbs_dir: None,
                roots: vec!["/old/photos".into()],
                requires_mapping: true,
            })
        })
        .on_import_db_bundle(|_, _| {
            Ok(DbBundleImportSummary {
                restored_files: 2,
                restored_thumbnails: 5,
            })
        });
    let (window, mut cx) = open_settings(&fixture, cx);
    let page = view(&window, &cx);
    let overwrite_open = |cx: &mut VisualTestContext| {
        page.read_with(cx, |page, cx| {
            page.import_overwrite_modal().read(cx).is_open()
        })
    };

    show(&window, SettingsSection::ImportExport, &mut cx);
    click(settings::IMPORT_DB_BUNDLE_ID, &fixture, &mut cx);
    assert!(cx.did_prompt_for_paths(), "the archive is asked for");
    cx.simulate_path_prompt_response(|options| {
        assert!(options.files && !options.directories && !options.multiple);
        Some(vec![PathBuf::from("/synthetic/backup.zip")])
    });
    settle(&fixture, &mut cx);
    // The root's folder is asked for, and the section says which root it is.
    assert!(cx.did_prompt_for_paths(), "the root's folder is asked for");
    assert_eq!(
        line(&window, OperationSection::ImportExport, &cx),
        "Choose folder: /old/photos"
    );
    assert!(!overwrite_open(&mut cx));
    cx.simulate_path_prompt_response(|options| {
        assert!(options.directories && !options.files && !options.multiple);
        Some(vec![PathBuf::from("/new/photos")])
    });
    settle(&fixture, &mut cx);

    assert!(overwrite_open(&mut cx), "the confirmation is open");
    assert!(drawn(settings::IMPORT_OVERWRITE_CONFIRM_ID, &mut cx));
    assert_eq!(fixture.calls("import_db_bundle"), 0);

    // Cancel imports nothing and closes the question.
    click(settings::IMPORT_OVERWRITE_CANCEL_ID, &fixture, &mut cx);
    assert!(!overwrite_open(&mut cx));
    assert_eq!(fixture.calls("import_db_bundle"), 0);
    assert_eq!(
        line(&window, OperationSection::ImportExport, &cx),
        "Database backup import cancelled."
    );

    // While the closed question fades out it is still drawn and takes the click.
    assert!(drawn(settings::IMPORT_OVERWRITE_CONFIRM_ID, &mut cx));
    fade_out(&mut cx);
    assert!(!drawn(settings::IMPORT_OVERWRITE_CONFIRM_ID, &mut cx));

    // Again, and confirmed.
    click(settings::IMPORT_DB_BUNDLE_ID, &fixture, &mut cx);
    cx.simulate_path_prompt_response(|_| Some(vec![PathBuf::from("/synthetic/backup.zip")]));
    settle(&fixture, &mut cx);
    cx.simulate_path_prompt_response(|_| Some(vec![PathBuf::from("/new/photos")]));
    settle(&fixture, &mut cx);
    click(settings::IMPORT_OVERWRITE_CONFIRM_ID, &fixture, &mut cx);

    assert!(!overwrite_open(&mut cx));
    assert_eq!(fixture.calls("import_db_bundle"), 1);
    assert_eq!(
        line(&window, OperationSection::ImportExport, &cx),
        "Database backup archive imported. Files: 2, thumbnails: 5"
    );
    let imported = fixture
        .backend
        .calls()
        .into_iter()
        .find_map(|call| match call {
            Call::ImportDbBundle {
                path,
                root_mappings,
            } => Some((path, root_mappings)),
            _ => None,
        })
        .expect("the restore was asked for");
    assert_eq!(imported.0, "/synthetic/backup.zip");
    assert_eq!(imported.1.len(), 1);
    assert_eq!(
        (
            imported.1[0].source_root.as_str(),
            imported.1[0].target_root.as_str()
        ),
        ("/old/photos", "/new/photos")
    );
}

// DuplicatesSection.test.tsx and DuplicateResolverDialog.test.tsx: "starts duplicate
// scan and shows last summary", "does not render when closed", "queues delete action
// and allows clearing it", "blocks save when queued changes still keep duplicate names".
#[gpui::test]
fn start_opens_the_resolver_with_its_groups_and_stages_changes(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    fixture.backend.on_find_duplicate_assets(|| Ok(groups(1)));
    let (window, mut cx) = open_settings(&fixture, cx);
    let page = view(&window, &cx);
    let resolver_open = |cx: &mut VisualTestContext| {
        page.read_with(cx, |page, cx| page.resolver_modal().read(cx).is_open())
    };
    assert!(!resolver_open(&mut cx));
    assert!(
        !drawn(settings::RESOLVER_SAVE_ID, &mut cx),
        "closed: not drawn"
    );
    assert!(fixture.previews().is_empty());

    show(&window, SettingsSection::Duplicates, &mut cx);
    click(settings::START_DUPLICATES_ID, &fixture, &mut cx);
    assert!(resolver_open(&mut cx));
    assert_eq!(fixture.calls("find_duplicate_assets"), 1);
    assert_eq!(
        line(&window, OperationSection::Duplicates, &cx),
        "Duplicate scan complete. Groups: 1, assets: 2"
    );
    for selector in [
        settings::RESOLVER_SAVE_ID.to_owned(),
        settings::RESOLVER_SUMMARY_SELECTOR.to_owned(),
        settings::RESOLVER_HINT_SELECTOR.to_owned(),
        settings::group_selector(0),
        control(AssetControl::Card, 1),
        control(AssetControl::Card, 2),
        control(AssetControl::Name, 1),
        control(AssetControl::Delete, 2),
    ] {
        assert!(drawn(&selector, &mut cx), "{selector} is drawn");
    }
    assert!(!drawn(settings::RESOLVER_EMPTY_SELECTOR, &mut cx));
    assert!(!drawn(&control(AssetControl::ClearAction, 1), &mut cx));
    assert!(!drawn(&settings::group_ready_selector(0), &mut cx));
    // The cards that are drawn ask for their previews, and only they.
    assert_eq!(fixture.previews(), [1, 2]);
    // The rename fields start with the files' names.
    let name = |asset_id: i64, cx: &mut VisualTestContext| {
        page.read_with(cx, |page, cx| {
            page.duplicate_name_input(asset_id)
                .map(|input| input.read(cx).text().to_owned())
        })
    };
    assert_eq!(name(1, &mut cx).as_deref(), Some("same-0.jpg"));

    // Nothing staged: Save all does nothing.
    click(settings::RESOLVER_SAVE_ID, &fixture, &mut cx);
    assert_eq!(fixture.calls("apply_duplicate_resolution_batch"), 0);
    assert!(model(&window, &cx, |settings| settings
        .pending_duplicate_delete()
        .is_none()));

    // "Queue delete" stages a delete: the group is resolved and can be cleared.
    click(&control(AssetControl::Delete, 2), &fixture, &mut cx);
    page.read_with(&cx, |page, _| {
        assert_eq!(
            page.duplicate_form().staged_changes(),
            [DuplicateChange::delete(2)]
        );
    });
    assert!(drawn(&settings::group_ready_selector(0), &mut cx));
    assert!(drawn(&control(AssetControl::ClearAction, 2), &mut cx));
    click(&control(AssetControl::ClearAction, 2), &fixture, &mut cx);
    page.read_with(&cx, |page, _| assert!(!page.duplicate_form().has_changes()));
    assert!(!drawn(&settings::group_ready_selector(0), &mut cx));

    // "Generate UUID" writes a new draft with the extension and stages nothing;
    // "Queue rename" stages it.
    click(&control(AssetControl::GenerateUuid, 1), &fixture, &mut cx);
    let generated = name(1, &mut cx).expect("the field is drawn");
    assert!(
        generated.ends_with(".jpg") && generated.len() == 36 + 4,
        "{generated}"
    );
    page.read_with(&cx, |page, _| assert!(!page.duplicate_form().has_changes()));
    click(&control(AssetControl::QueueRename, 1), &fixture, &mut cx);
    page.read_with(&cx, |page, _| {
        assert_eq!(
            page.duplicate_form().staged(1),
            Some(&DuplicateAction::Rename {
                next_file_name: generated.clone()
            })
        );
    });
    assert!(drawn(&settings::group_ready_selector(0), &mut cx));

    // "Discard changes" starts the form over.
    click(settings::RESOLVER_DISCARD_ID, &fixture, &mut cx);
    page.read_with(&cx, |page, _| assert!(!page.duplicate_form().has_changes()));
    assert_eq!(name(1, &mut cx).as_deref(), Some("same-0.jpg"));

    // A rename that still leaves two files of one name cannot be saved: the field
    // holds the file's own name again, and queueing it resolves nothing.
    click(&control(AssetControl::QueueRename, 1), &fixture, &mut cx);
    let unresolved = page.read_with(&cx, |page, cx| {
        let groups = scan_groups(cx);
        let form = page.duplicate_form();
        assert_eq!(
            form.staged(1),
            Some(&DuplicateAction::Rename {
                next_file_name: "same-0.jpg".into()
            })
        );
        assert!(!form.can_save(&groups, false));
        form.validation(&groups)
            .first_error()
            .map(|error| error.resolve(&I18n::new(Language::En)))
    });
    assert_eq!(
        unresolved.as_deref(),
        Some(
            "Group \"same-0.jpg\": queued changes still leave duplicate file names. Rename or \
             delete more items in this block."
        )
    );
    assert!(!drawn(&settings::group_ready_selector(0), &mut cx));
    click(settings::RESOLVER_SAVE_ID, &fixture, &mut cx);
    assert_eq!(
        fixture.calls("apply_duplicate_resolution_batch"),
        0,
        "an unresolved batch is never sent"
    );
    assert!(model(&window, &cx, |settings| !settings.is_locked()));
    click(settings::RESOLVER_DISCARD_ID, &fixture, &mut cx);

    // "Close" closes the resolver, which then asks for no preview.
    click(settings::RESOLVER_CLOSE_ID, &fixture, &mut cx);
    assert!(!resolver_open(&mut cx));
    assert!(!model(&window, &cx, |settings| settings.duplicate_dialog_open()));
    // The section keeps the result of the scan.
    assert!(drawn(settings::DUPLICATES_LAST_RESULT_SELECTOR, &mut cx));
}

// createSettingsActionsConfirmFlow.test.tsx, by pointer: a batch with a delete is
// applied only after the confirmation that lies over the resolver.
#[gpui::test]
fn a_staged_delete_is_applied_only_after_the_confirmation_over_the_resolver(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new();
    fixture
        .backend
        .on_find_duplicate_assets(|| Ok(groups(1)))
        .on_apply_duplicate_resolution_batch(|_| {
            Ok(DuplicateResolutionBatchSummary {
                status: DuplicateResolutionBatchStatus::Committed,
                revision: 8,
                results: Vec::new(),
                removed_thumbnails: 0,
            })
        });
    let (window, mut cx) = open_settings(&fixture, cx);
    let page = view(&window, &cx);
    let delete_open = |cx: &mut VisualTestContext| {
        page.read_with(cx, |page, cx| {
            page.duplicate_delete_modal().read(cx).is_open()
        })
    };

    show(&window, SettingsSection::Duplicates, &mut cx);
    click(settings::START_DUPLICATES_ID, &fixture, &mut cx);
    click(&control(AssetControl::Delete, 2), &fixture, &mut cx);
    click(settings::RESOLVER_SAVE_ID, &fixture, &mut cx);
    assert!(delete_open(&mut cx), "the confirmation is open");
    assert!(page.read_with(&cx, |page, cx| page.resolver_modal().read(cx).is_open()));
    assert_eq!(fixture.calls("apply_duplicate_resolution_batch"), 0);

    // Cancel applies nothing and keeps what was staged.
    click(settings::DUPLICATE_DELETE_CANCEL_ID, &fixture, &mut cx);
    assert!(!delete_open(&mut cx));
    assert_eq!(fixture.calls("apply_duplicate_resolution_batch"), 0);
    assert_eq!(
        line(&window, OperationSection::Duplicates, &cx),
        "Applying duplicate changes cancelled."
    );
    page.read_with(&cx, |page, _| {
        assert_eq!(
            page.duplicate_form().staged_changes(),
            [DuplicateChange::delete(2)]
        );
    });

    fade_out(&mut cx);
    click(settings::RESOLVER_SAVE_ID, &fixture, &mut cx);
    assert!(delete_open(&mut cx), "asked again");
    click(settings::DUPLICATE_DELETE_CONFIRM_ID, &fixture, &mut cx);
    assert!(!delete_open(&mut cx));
    assert!(fixture
        .backend
        .calls()
        .contains(&Call::ApplyDuplicateResolutionBatch {
            input: DuplicateResolutionBatchInput {
                scan_revision: 7,
                changes: vec![DuplicateResolutionChangeInput::Delete {
                    asset_id: 2,
                    expected_path: "/media/b/same-0.jpg".into(),
                    expected_record_version: 4,
                }],
            },
        }));
    assert_eq!(fixture.calls("apply_duplicate_resolution_batch"), 1);
    // The scan that follows replaced the groups: the form started over.
    assert_eq!(fixture.calls("find_duplicate_assets"), 2);
    page.read_with(&cx, |page, _| assert!(!page.duplicate_form().has_changes()));
    assert_eq!(
        line(&window, OperationSection::Duplicates, &cx),
        "Applied 1 change. Remaining groups: 1"
    );
}

// DuplicateResolverDialog.test.tsx, "closes on backdrop click only when unlocked", and
// the virtual list: a scan with many groups draws and asks previews for a few only.
#[gpui::test]
fn a_running_scan_locks_the_resolver_and_many_groups_are_drawn_a_few_at_a_time(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new();
    fixture.backend.on_find_duplicate_assets(|| Ok(groups(200)));
    let (window, mut cx) = open_settings(&fixture, cx);
    let page = view(&window, &cx);
    let resolver_open = |cx: &mut VisualTestContext| {
        page.read_with(cx, |page, cx| page.resolver_modal().read(cx).is_open())
    };

    // The scan is asked for and not answered yet: the resolver is open and locked.
    let runtime = window
        .read_with(&cx, |shell, _| shell.runtime().clone())
        .expect("the window is open");
    runtime.update(&mut cx, |runtime, cx| {
        runtime.send(RuntimeInput::settings(SettingsCommand::StartDuplicates), cx);
    });
    cx.run_until_parked();
    assert!(resolver_open(&mut cx));
    assert!(model(&window, &cx, |settings| settings.is_locked()));
    assert!(drawn(settings::RESOLVER_LOADER_SELECTOR, &mut cx));
    assert!(
        !drawn(settings::RESOLVER_EMPTY_SELECTOR, &mut cx),
        "no groups yet, and no claim that there are none"
    );
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(
        resolver_open(&mut cx),
        "Escape does not close a locked resolver"
    );
    let close = cx
        .debug_bounds(settings::RESOLVER_CLOSE_ID)
        .expect("Close is drawn")
        .center();
    cx.simulate_click(close, Modifiers::none());
    cx.run_until_parked();
    assert!(
        resolver_open(&mut cx),
        "Close is disabled while the scan runs"
    );

    settle(&fixture, &mut cx);
    assert_eq!(
        model(&window, &cx, |settings| settings.duplicate_groups().len()),
        200
    );
    assert_eq!(
        page.read_with(&cx, |page, _| page.duplicate_list().item_count()),
        200
    );
    assert!(drawn(&settings::group_selector(0), &mut cx));
    assert!(!drawn(&settings::group_selector(199), &mut cx));
    let previews = fixture.previews();
    assert!(
        !previews.is_empty() && previews.len() < 40,
        "previews of the drawn groups only: {}",
        previews.len()
    );
    assert!(previews.contains(&1) && !previews.contains(&400));

    // Scrolled to its end, the list draws the last groups instead, and they ask for
    // their previews.
    let list = page.read_with(&cx, |page, _| page.duplicate_list().clone());
    list.scroll_to_reveal_item(199);
    page.update(&mut cx, |_, cx| cx.notify());
    settle(&fixture, &mut cx);
    assert!(drawn(&settings::group_selector(199), &mut cx));
    assert!(!drawn(&settings::group_selector(0), &mut cx));
    assert!(drawn(&control(AssetControl::Name, 400), &mut cx));
    let previews = fixture.previews();
    assert!(
        previews.contains(&399) && previews.contains(&400),
        "{previews:?}"
    );
    assert!(
        previews.len() < 80,
        "still only what was drawn: {}",
        previews.len()
    );

    // Unlocked: Escape closes it.
    cx.simulate_keystrokes("escape");
    settle(&fixture, &mut cx);
    assert!(!resolver_open(&mut cx));
}

/// One file name in `files` folders, a single group with the asset IDs `1..=files`, and
/// after it a group of two files, `files + 1` and `files + 2`.
fn a_large_group(files: i64) -> DuplicateScanSummary {
    let groups = vec![
        DuplicateGroup {
            file_name: "cover.jpg".into(),
            assets: (1..=files)
                .map(|id| asset(id, &format!("/media/album-{id}/cover.jpg")))
                .collect(),
        },
        DuplicateGroup {
            file_name: "same.jpg".into(),
            assets: vec![
                asset(files + 1, "/media/a/same.jpg"),
                asset(files + 2, "/media/b/same.jpg"),
            ],
        },
    ];
    DuplicateScanSummary {
        duplicate_groups: groups.len(),
        duplicate_assets: usize::try_from(files).expect("a count") + 2,
        groups,
        revision: 7,
    }
}

// One file name can be in a library hundreds or thousands of times, and the scan lists
// the largest group first. Its card is drawn a few rows of files at a time, so the
// previews asked for and kept, and the rename fields made, follow what is near the
// visible part and not the size of the group.
#[gpui::test]
fn a_group_of_many_files_is_drawn_a_few_rows_at_a_time(cx: &mut TestAppContext) {
    const FILES: i64 = 298;
    // Eight files to a row: 37 full rows of the large group and one with its last two
    // files, then the row of the small group.
    const ROWS: usize = 39;
    // Four rows: more than the rows near the visible part hold, far fewer than the
    // group.
    const NEAR: usize = 32;

    let fixture = Fixture::new();
    fixture
        .backend
        .on_find_duplicate_assets(|| Ok(a_large_group(FILES)));
    let (window, mut cx) = open_settings(&fixture, cx);
    let page = view(&window, &cx);
    let thumbnails = window
        .read_with(&cx, |shell, cx| {
            shell.runtime().read(cx).thumbnails().clone()
        })
        .expect("the window is open");
    let in_range = |cx: &VisualTestContext| {
        thumbnails.read_with(cx, |thumbnails, _| thumbnails.tiles_in_range())
    };

    show(&window, SettingsSection::Duplicates, &mut cx);
    click(settings::START_DUPLICATES_ID, &fixture, &mut cx);
    assert_eq!(
        page.read_with(&cx, |page, _| page.duplicate_list().item_count()),
        ROWS
    );
    assert!(drawn(&settings::group_selector(0), &mut cx));
    assert!(drawn(&control(AssetControl::Card, 1), &mut cx));
    assert!(!drawn(&control(AssetControl::Card, FILES), &mut cx));
    let previews = fixture.previews();
    assert!(
        previews.contains(&1) && previews.len() < NEAR,
        "previews of the rows near the visible part only: {}",
        previews.len()
    );
    assert!(
        (1..NEAR).contains(&in_range(&cx)),
        "previews kept: {}",
        in_range(&cx)
    );
    page.read_with(&cx, |page, _| {
        assert!(page.duplicate_name_input(1).is_some());
        let fields = (1..=FILES)
            .filter(|id| page.duplicate_name_input(*id).is_some())
            .count();
        assert!(fields < NEAR, "rename fields made: {fields}");
    });

    // The last row of the group at the top of the list: its last files are drawn and
    // ask for their previews, the first ones have given theirs up.
    let list = page.read_with(&cx, |page, _| page.duplicate_list().clone());
    list.scroll_to(gpui::ListOffset {
        item_ix: ROWS - 2,
        offset_in_item: gpui::px(0.),
    });
    page.update(&mut cx, |_, cx| cx.notify());
    settle(&fixture, &mut cx);
    assert!(drawn(&control(AssetControl::Card, FILES), &mut cx));
    assert!(!drawn(&control(AssetControl::Card, 1), &mut cx));
    assert!(
        !drawn(&settings::group_selector(0), &mut cx),
        "the row with the group's name is far above"
    );
    let previews = fixture.previews();
    assert!(
        previews.contains(&FILES) && previews.len() < 2 * NEAR,
        "{}",
        previews.len()
    );
    assert!(
        (1..NEAR).contains(&in_range(&cx)),
        "previews kept: {}",
        in_range(&cx)
    );
    // The rename fields of the first rows went with them: the fields kept are those of
    // the rows near the visible part, not of every file scrolled past.
    page.read_with(&cx, |page, _| {
        assert!(page.duplicate_name_input(FILES).is_some());
        assert!(page.duplicate_name_input(1).is_none());
        let fields = (1..=FILES + 2)
            .filter(|id| page.duplicate_name_input(*id).is_some())
            .count();
        assert!(fields < NEAR, "rename fields kept: {fields}");
    });

    // A change staged in a later row counts for the whole group: one file less still
    // leaves the name hundreds of times, so nothing can be saved.
    click(&control(AssetControl::Delete, FILES), &fixture, &mut cx);
    page.read_with(&cx, |page, _| {
        assert_eq!(
            page.duplicate_form().staged(FILES),
            Some(&DuplicateAction::Delete)
        );
    });
    assert!(drawn(&control(AssetControl::ClearAction, FILES), &mut cx));
    click(settings::RESOLVER_SAVE_ID, &fixture, &mut cx);
    assert_eq!(fixture.calls("apply_duplicate_resolution_batch"), 0);
    assert!(model(&window, &cx, |settings| settings
        .pending_duplicate_delete()
        .is_none()));

    // The next group is a card of its own below the last row.
    assert!(drawn(&settings::group_selector(1), &mut cx));
    assert!(drawn(&control(AssetControl::Card, FILES + 2), &mut cx));

    // The rows of a group join into one card. With the end of the row before in view,
    // the last file of that row and the first of the next lie as far apart as two files
    // of one row, in one column.
    list.scroll_by(gpui::px(-200.));
    page.update(&mut cx, |_, cx| cx.notify());
    settle(&fixture, &mut cx);
    let card = |id: i64, cx: &mut VisualTestContext| {
        let selector: &'static str = Box::leak(control(AssetControl::Card, id).into_boxed_str());
        cx.debug_bounds(selector)
            .unwrap_or_else(|| panic!("the card of asset {id} is drawn"))
    };
    let (above, first, second) = (
        card(FILES - 2, &mut cx),
        card(FILES - 1, &mut cx),
        card(FILES, &mut cx),
    );
    assert_eq!(first.top() - above.bottom(), second.top() - first.bottom());
    assert!(first.top() > above.bottom());
    assert_eq!((above.left(), above.right()), (first.left(), first.right()));
}

// A rename field lives as long as its row is near the visible part. Scrolled away, it is
// dropped with its window listeners, also when it had the focus, which the window takes
// back as a page does for an unmounted input. Its draft stays in the form: the field
// made when the row comes back shows it.
#[gpui::test]
fn a_rename_field_leaves_with_its_row_and_comes_back_with_its_draft(cx: &mut TestAppContext) {
    const FILES: i64 = 298;
    const ROWS: usize = 39;

    let fixture = Fixture::new();
    fixture
        .backend
        .on_find_duplicate_assets(|| Ok(a_large_group(FILES)));
    let (window, mut cx) = open_settings(&fixture, cx);
    let page = view(&window, &cx);
    show(&window, SettingsSection::Duplicates, &mut cx);
    click(settings::START_DUPLICATES_ID, &fixture, &mut cx);

    let field = |id: i64, cx: &VisualTestContext| {
        page.read_with(cx, |page, _| page.duplicate_name_input(id).cloned())
    };
    let text = |id: i64, cx: &VisualTestContext| {
        field(id, cx).map(|input| input.read_with(cx, |input, _| input.text().to_owned()))
    };
    let scroll_to_row = |row: usize, cx: &mut VisualTestContext| {
        let list = page.read_with(cx, |page, _| page.duplicate_list().clone());
        list.scroll_to(gpui::ListOffset {
            item_ix: row,
            offset_in_item: gpui::px(0.),
        });
        page.update(cx, |_, cx| cx.notify());
        settle(&fixture, cx);
    };

    // A draft typed into the field of the first file, which has the focus.
    click(&control(AssetControl::Name, 1), &fixture, &mut cx);
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("draft.png");
    settle(&fixture, &mut cx);
    let first = field(1, &cx).expect("the field of asset 1 is drawn");
    let handle = first.read_with(&cx, |input, cx| input.focus_handle(cx));
    assert!(cx.update(|window, _| handle.is_focused(window)));
    assert!(field(2, &cx).is_some());
    let first = {
        let weak = first.downgrade();
        drop(first);
        weak
    };

    // At the end of the group the fields of the first row are gone, the focused one
    // too: the page was the only one that held it, and the window has its focus back.
    scroll_to_row(ROWS - 2, &mut cx);
    assert!(!drawn(&control(AssetControl::Card, 1), &mut cx));
    assert!(field(2, &cx).is_none(), "the field of a row out of view");
    assert!(field(1, &cx).is_none(), "the field that had the focus");
    assert!(!cx.update(|window, _| handle.is_focused(window)));
    drop(handle);
    assert!(first.upgrade().is_none(), "the field is released");
    assert!(field(FILES, &cx).is_some());
    page.read_with(&cx, |page, _| {
        assert_eq!(page.duplicate_form().draft(1), Some("draft.png"));
    });

    // Back at the top the field is made again and shows the draft; the file next to it
    // shows its name.
    scroll_to_row(0, &mut cx);
    assert!(drawn(&control(AssetControl::Card, 1), &mut cx));
    assert_eq!(text(1, &cx).as_deref(), Some("draft.png"));
    assert_eq!(text(2, &cx).as_deref(), Some("cover.jpg"));
    assert!(field(FILES, &cx).is_none());
    page.read_with(&cx, |page, _| {
        assert_eq!(page.duplicate_form().draft(1), Some("draft.png"));
        assert_eq!(
            page.duplicate_form().staged(1),
            None::<&DuplicateAction>,
            "a field made again stages nothing"
        );
        assert!(!page.duplicate_form().has_changes());
    });
}

// "updates the mounted virtual list when a rescan removes every group".
#[gpui::test]
fn a_rescan_that_finds_nothing_empties_the_open_resolver(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let mut scans = vec![Ok(groups(0)), Ok(groups(3))];
    fixture.backend.on_find_duplicate_assets(move || {
        scans
            .pop()
            .unwrap_or_else(|| Err(BackendError::rejected("used up")))
    });
    let (window, mut cx) = open_settings(&fixture, cx);
    let page = view(&window, &cx);

    show(&window, SettingsSection::Duplicates, &mut cx);
    click(settings::START_DUPLICATES_ID, &fixture, &mut cx);
    assert_eq!(
        page.read_with(&cx, |page, _| page.duplicate_list().item_count()),
        3
    );
    assert!(drawn(&settings::group_selector(0), &mut cx));
    assert!(!drawn(settings::RESOLVER_EMPTY_SELECTOR, &mut cx));
    assert_eq!(fixture.previews(), [1, 2, 3, 4, 5, 6]);
    assert_eq!(
        previews_in_demand(&window, &cx),
        6,
        "the previews of the drawn cards are asked for"
    );
    click(&control(AssetControl::Delete, 2), &fixture, &mut cx);
    page.read_with(&cx, |page, _| assert!(page.duplicate_form().has_changes()));

    click(settings::RESOLVER_RESCAN_ID, &fixture, &mut cx);
    assert_eq!(fixture.calls("find_duplicate_assets"), 2);
    assert!(model(&window, &cx, |settings| settings
        .duplicate_groups()
        .is_empty()));
    assert_eq!(
        page.read_with(&cx, |page, _| page.duplicate_list().item_count()),
        0
    );
    assert!(!drawn(&settings::group_selector(0), &mut cx));
    assert!(drawn(settings::RESOLVER_EMPTY_SELECTOR, &mut cx));
    page.read_with(&cx, |page, _| {
        assert!(
            !page.duplicate_form().has_changes(),
            "the form started over"
        );
        assert!(page.duplicate_name_input(2).is_none());
    });
    assert!(page.read_with(&cx, |page, cx| page.resolver_modal().read(cx).is_open()));
    assert!(!drawn(settings::DUPLICATES_LAST_RESULT_SELECTOR, &mut cx));
    // The open resolver draws no card any more: it asks for no preview and pins none,
    // although it has not been closed.
    assert_eq!(previews_in_demand(&window, &cx), 0);
}

// Orchestrator audit G6: the rename field is drawn with the frame of the card's
// `input input-sm h-10 ... border border-base-content/20 bg-base-100/70`: 40 px high,
// the 12 px text on a 16 px line 12 px from the left edge and centred, and the outline
// of daisyUI's `.input:focus` (2 px wide, 2 px off the border) while it has the focus.
#[gpui::test]
fn a_rename_field_has_the_frame_and_the_focus_outline_of_the_cards_input(cx: &mut TestAppContext) {
    use media_tagger_client::settings::layout::resolver;

    let fixture = Fixture::new();
    fixture.backend.on_find_duplicate_assets(|| Ok(groups(1)));
    let (window, mut cx) = open_settings(&fixture, cx);
    let page = view(&window, &cx);
    show(&window, SettingsSection::Duplicates, &mut cx);
    click(settings::START_DUPLICATES_ID, &fixture, &mut cx);

    let bounds_of = |selector: &str, cx: &mut VisualTestContext| {
        let selector: &'static str = Box::leak(selector.to_owned().into_boxed_str());
        cx.debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} is drawn"))
    };
    let field = |cx: &mut VisualTestContext| {
        page.read_with(cx, |page, _| {
            page.duplicate_name_input(1)
                .expect("the card is drawn")
                .clone()
        })
    };
    let frame = bounds_of(&control(AssetControl::Name, 1), &mut cx);
    assert_eq!(frame.size.height, gpui::px(resolver::INPUT_HEIGHT));
    let line = field(&mut cx)
        .read_with(&cx, |input, _| input.text_bounds())
        .expect("the line is drawn");
    assert_eq!(
        line.left() - frame.left(),
        gpui::px(resolver::INPUT_PADDING_X),
        "the text starts at the frame's padding"
    );
    assert_eq!(line.size.height, gpui::px(16.0), "a text-xs line");
    assert_eq!(
        line.top() - frame.top(),
        frame.bottom() - line.bottom(),
        "centred in the frame"
    );
    let outline = settings::rename_outline_selector(1);
    assert!(!drawn(&outline, &mut cx), "no outline at rest");

    // A press on the frame's padding, beside the line, focuses the field.
    cx.simulate_click(
        gpui::point(frame.left() + gpui::px(4.0), frame.center().y),
        Modifiers::none(),
    );
    cx.run_until_parked();
    let handle = field(&mut cx).read_with(&cx, |input, cx| input.focus_handle(cx));
    assert!(cx.update(|window, _| handle.is_focused(window)));
    let ring = bounds_of(&outline, &mut cx);
    let outset = gpui::px(resolver::INPUT_OUTLINE_WIDTH + resolver::INPUT_OUTLINE_OFFSET);
    assert_eq!(
        (ring.left(), ring.top(), ring.right(), ring.bottom()),
        (
            frame.left() - outset,
            frame.top() - outset,
            frame.right() + outset,
            frame.bottom() + outset
        )
    );
    // What is typed goes to the form's draft.
    cx.simulate_input("x");
    cx.run_until_parked();
    page.read_with(&cx, |page, _| {
        assert!(page
            .duplicate_form()
            .draft(1)
            .is_some_and(|draft| draft.contains('x')));
    });

    // A file staged for deletion has a disabled field: no focus, no outline.
    click(&control(AssetControl::Delete, 1), &fixture, &mut cx);
    assert!(field(&mut cx).read_with(&cx, |input, _| input.is_disabled()));
    assert!(!drawn(&outline, &mut cx));
    cx.simulate_click(
        gpui::point(frame.left() + gpui::px(4.0), frame.center().y),
        Modifiers::none(),
    );
    cx.run_until_parked();
    assert!(!cx.update(|window, _| handle.is_focused(window)));
    assert!(!drawn(&outline, &mut cx));
}

fn empty_archive() -> DbBundleInspection {
    DbBundleInspection {
        format_version: Some(1),
        source_platform: Some("linux".into()),
        source_thumbs_dir: None,
        roots: Vec::new(),
        requires_mapping: false,
    }
}

// Orchestrator audit G5 (`.claude/rules/settings.md`: the dialogs register with the
// layer manager, the page owns page-level Escape). Escape closes the dialog on top and
// nothing under it; while an operation runs it closes neither a dialog nor the page;
// with no dialog open it closes the page.
#[gpui::test]
fn escape_closes_the_dialog_on_top_and_the_page_only_when_no_dialog_is_open(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new();
    fixture
        .backend
        .on_find_duplicate_assets(|| Ok(groups(1)))
        .on_inspect_db_bundle(|_| Ok(empty_archive()));
    let (window, mut cx) = open_settings(&fixture, cx);
    let page = view(&window, &cx);
    let resolver_open = |cx: &mut VisualTestContext| {
        page.read_with(cx, |page, cx| page.resolver_modal().read(cx).is_open())
    };
    let delete_open = |cx: &mut VisualTestContext| {
        page.read_with(cx, |page, cx| {
            page.duplicate_delete_modal().read(cx).is_open()
        })
    };
    let overwrite_open = |cx: &mut VisualTestContext| {
        page.read_with(cx, |page, cx| {
            page.import_overwrite_modal().read(cx).is_open()
        })
    };
    let escape = |cx: &mut VisualTestContext| {
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
    };

    // The resolver while its scan runs: Escape closes neither it nor the page.
    begin(&window, SettingsCommand::StartDuplicates, &mut cx);
    assert!(model(&window, &cx, |settings| settings.is_locked()));
    assert!(resolver_open(&mut cx));
    escape(&mut cx);
    assert!(resolver_open(&mut cx), "a locked resolver stays open");
    assert!(page_shown(&window, &cx), "and the page under it stays");
    settle(&fixture, &mut cx);
    assert!(!model(&window, &cx, |settings| settings.is_locked()));

    // The delete confirmation over the resolver: Escape closes the confirmation only.
    click(&control(AssetControl::Delete, 2), &fixture, &mut cx);
    click(settings::RESOLVER_SAVE_ID, &fixture, &mut cx);
    assert!(delete_open(&mut cx) && resolver_open(&mut cx));
    escape(&mut cx);
    assert!(!delete_open(&mut cx), "the confirmation on top closes");
    assert!(resolver_open(&mut cx), "the resolver under it stays");
    assert!(page_shown(&window, &cx));
    assert_eq!(fixture.calls("apply_duplicate_resolution_batch"), 0);
    fade_out(&mut cx);

    // The unlocked resolver: Escape closes it, and only it.
    escape(&mut cx);
    assert!(!resolver_open(&mut cx));
    assert!(!model(&window, &cx, |settings| settings.duplicate_dialog_open()));
    assert!(page_shown(&window, &cx), "the page stays");
    fade_out(&mut cx);
    assert!(page_shown(&window, &cx));

    // The overwrite confirmation of a restore.
    show(&window, SettingsSection::ImportExport, &mut cx);
    click(settings::IMPORT_DB_BUNDLE_ID, &fixture, &mut cx);
    cx.simulate_path_prompt_response(|_| Some(vec![PathBuf::from("/synthetic/backup.zip")]));
    settle(&fixture, &mut cx);
    assert!(overwrite_open(&mut cx));
    // While an operation runs (one started past the page the dialog covers), Escape
    // closes nothing and Cancel is disabled.
    begin(&window, SettingsCommand::RescanDuplicates, &mut cx);
    assert!(model(&window, &cx, |settings| settings.is_locked()));
    escape(&mut cx);
    assert!(overwrite_open(&mut cx), "a locked confirmation stays open");
    assert!(page_shown(&window, &cx));
    let cancel = cx
        .debug_bounds(settings::IMPORT_OVERWRITE_CANCEL_ID)
        .expect("Cancel is drawn")
        .center();
    cx.simulate_click(cancel, Modifiers::none());
    cx.run_until_parked();
    assert!(overwrite_open(&mut cx), "Cancel is disabled meanwhile");
    assert_eq!(
        model(&window, &cx, |settings| settings
            .pending_import_db_source()
            .map(str::to_owned)),
        Some("/synthetic/backup.zip".to_owned())
    );
    settle(&fixture, &mut cx);
    assert!(!model(&window, &cx, |settings| settings.is_locked()));

    // Unlocked: Escape gives the restore up and leaves the page.
    escape(&mut cx);
    assert!(!overwrite_open(&mut cx));
    assert!(model(&window, &cx, |settings| settings
        .pending_import_db_source()
        .is_none()));
    assert_eq!(
        line(&window, OperationSection::ImportExport, &cx),
        "Database backup import cancelled."
    );
    assert!(page_shown(&window, &cx), "the page stays");
    assert_eq!(fixture.calls("import_db_bundle"), 0);
    fade_out(&mut cx);

    // A press beside the confirmation, where the page's Back button is, is taken by
    // the dialog: it gives the restore up and the page stays.
    click(settings::IMPORT_DB_BUNDLE_ID, &fixture, &mut cx);
    cx.simulate_path_prompt_response(|_| Some(vec![PathBuf::from("/synthetic/backup.zip")]));
    settle(&fixture, &mut cx);
    assert!(overwrite_open(&mut cx));
    click(settings::BACK_ID, &fixture, &mut cx);
    assert!(!overwrite_open(&mut cx));
    assert!(model(&window, &cx, |settings| settings
        .pending_import_db_source()
        .is_none()));
    assert!(page_shown(&window, &cx), "the press did not reach Back");
    assert_eq!(fixture.calls("import_db_bundle"), 0);
    fade_out(&mut cx);

    // No dialog is open: Escape is the page's, also while an operation runs.
    begin(&window, SettingsCommand::RescanDuplicates, &mut cx);
    assert!(model(&window, &cx, |settings| settings.is_locked()));
    escape(&mut cx);
    assert!(!page_shown(&window, &cx), "Escape closes the page");
    settle(&fixture, &mut cx);
}

// Orchestrator audit G3: a page left with a restore waiting for its confirmation does
// not ask again when it is opened the next time.
#[gpui::test]
fn a_page_opened_again_does_not_ask_about_a_restore_chosen_before(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    fixture
        .backend
        .on_inspect_db_bundle(|_| Ok(empty_archive()));
    let (window, mut cx) = open_settings(&fixture, cx);
    show(&window, SettingsSection::ImportExport, &mut cx);
    click(settings::IMPORT_DB_BUNDLE_ID, &fixture, &mut cx);
    cx.simulate_path_prompt_response(|_| Some(vec![PathBuf::from("/synthetic/backup.zip")]));
    settle(&fixture, &mut cx);
    let page = view(&window, &cx);
    assert!(page.read_with(&cx, |page, cx| page
        .import_overwrite_modal()
        .read(cx)
        .is_open()));

    // Left past the dialog, which takes every key and press of the user.
    send(&window, RuntimeInput::CloseSettings, &fixture, &mut cx);
    assert!(!page_shown(&window, &cx));
    assert!(model(&window, &cx, |settings| settings
        .pending_import_db_source()
        .is_none()));
    fade_out(&mut cx);

    send(&window, RuntimeInput::OpenSettings, &fixture, &mut cx);
    let page = view(&window, &cx);
    assert!(!page.read_with(&cx, |page, cx| page
        .import_overwrite_modal()
        .read(cx)
        .is_open()));
    assert!(!drawn(settings::IMPORT_OVERWRITE_CONFIRM_ID, &mut cx));
    assert_eq!(fixture.calls("import_db_bundle"), 0);
}

/// Acceptance "Parytet `settings-import-export`, `settings-duplicates`, `duplicates-resolver`",
/// the part a test can observe without a display: each scenario's start state opens the
/// page and scrolls to the section its Tauri steps click; the two duplicate scenarios
/// then make "Start", and `settings-duplicates` closes the resolver once the scan has
/// answered, as its Tauri steps do.
#[gpui::test]
fn the_data_scenarios_reach_their_start_states(cx: &mut TestAppContext) {
    for (scenario, section) in [
        (SETTINGS_IMPORT_EXPORT, SettingsSection::ImportExport),
        (SETTINGS_DUPLICATES, SettingsSection::Duplicates),
        (DUPLICATES_RESOLVER, SettingsSection::Duplicates),
    ] {
        let fixture = Fixture::new();
        fixture.backend.on_find_duplicate_assets(|| Ok(groups(1)));
        let window = cx
            .update(|cx| {
                init_application(ThemeName::Dark, cx)?;
                open_main_window(profile::E2E, fixture.parts(), cx)
            })
            .expect("open the main window");
        let mut vcx = VisualTestContext::from_window(window.into(), cx);
        let runtime = window
            .read_with(&vcx, |shell, _| shell.runtime().clone())
            .expect("the window is open");
        runtime.update(&mut vcx, |runtime, cx| {
            runtime.script_settings(scenario, cx)
        });
        // The navigation click waits for a laid-out page, the steps after it for the
        // click: a few frames.
        for _ in 0..6 {
            settle(&fixture, &mut vcx);
            vcx.update(|window, _| window.refresh());
        }
        settle(&fixture, &mut vcx);

        let page = view(&window, &vcx);
        // The section is at the page's scroll margin and marked.
        let id: &'static str =
            Box::leak(settings::section_id(section).to_string().into_boxed_str());
        assert_eq!(
            vcx.debug_bounds(id).expect("the section is drawn").top(),
            gpui::px(96.),
            "{scenario:?}"
        );
        assert_eq!(
            page.read_with(&vcx, |page, _| page.active_section()),
            section,
            "{scenario:?}"
        );
        let resolver_open =
            page.read_with(&vcx, |page, cx| page.resolver_modal().read(cx).is_open());
        let dialog_open = model(&window, &vcx, |settings| settings.duplicate_dialog_open());
        assert!(
            !model(&window, &vcx, |settings| settings.is_locked()),
            "{scenario:?}: nothing runs in a start state"
        );
        match scenario.duplicates {
            None => {
                assert_eq!(fixture.calls("find_duplicate_assets"), 0, "{scenario:?}");
                assert!(!resolver_open && !dialog_open, "{scenario:?}");
                assert!(drawn(settings::EXPORT_CSV_ID, &mut vcx));
                assert_eq!(
                    line(&window, OperationSection::ImportExport, &vcx),
                    "Ready for import/export"
                );
            }
            Some(ScenarioDuplicates::Scanned) => {
                assert_eq!(fixture.calls("find_duplicate_assets"), 1, "one scan");
                assert!(
                    !resolver_open && !dialog_open,
                    "the resolver was closed again"
                );
                assert!(drawn(settings::DUPLICATES_LAST_RESULT_SELECTOR, &mut vcx));
                assert_eq!(
                    line(&window, OperationSection::Duplicates, &vcx),
                    "Duplicate scan complete. Groups: 1, assets: 2"
                );
            }
            Some(ScenarioDuplicates::Resolver) => {
                assert_eq!(fixture.calls("find_duplicate_assets"), 1, "one scan");
                assert!(resolver_open && dialog_open, "the resolver stays open");
                assert!(drawn(&settings::group_selector(0), &mut vcx));
                assert!(drawn(&control(AssetControl::Card, 2), &mut vcx));
                page.read_with(&vcx, |page, _| {
                    assert!(!page.duplicate_form().has_changes(), "nothing is staged");
                });
                assert_eq!(fixture.previews(), [1, 2]);
            }
        }
        assert_eq!(fixture.calls("apply_duplicate_resolution_batch"), 0);
        vcx.update(|window, _| window.remove_window());
        vcx.run_until_parked();
    }
}
