//! Ported view behaviour of the import/export and duplicate sections (epic E18) on
//! GPUI's test platform with the scripted backend and the deterministic executor, case
//! by case from `ImportExportSection.test.tsx`, `DuplicatesSection.test.tsx`,
//! `DuplicateResolverDialog.test.tsx`, `createDuplicateResolver.test.tsx` and
//! `confirmationPresence.test.tsx`: what each button asks for and when it is
//! disabled, typing a rename, the backdrop and Escape of the resolver and of the
//! confirmation that lies over it, the counts that stay while that confirmation fades
//! out, the form that starts over when the resolver opens again, and a rename field that
//! keeps its draft and focus while the list scrolls.

use std::sync::Arc;

use gpui::{Focusable as _, Modifiers, TestAppContext, VisualTestContext, WindowHandle};
use media_tagger_client::{
    backend::{fake::Call, BackendError, FakeBackend},
    exec::{Executor, ManualExecutor},
    i18n::{I18n, Language},
    prefs::MemoryPrefsStore,
    runtime::{RuntimeConfig, RuntimeInput},
    settings::{
        DuplicateAction, DuplicateChange, OperationSection, SettingsCommand, SettingsSection,
    },
    theme::ThemeName,
};
use media_tagger_core::models::{
    DuplicateAsset, DuplicateGroup, DuplicateResolutionBatchInput, DuplicateResolutionBatchStatus,
    DuplicateResolutionBatchSummary, DuplicateResolutionChangeInput, DuplicateScanSummary,
    ScanRoot, StartAssetQueryResult, TagListPage, VideoToolStatus,
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

fn committed() -> DuplicateResolutionBatchSummary {
    DuplicateResolutionBatchSummary {
        status: DuplicateResolutionBatchStatus::Committed,
        revision: 8,
        results: Vec::new(),
        removed_thumbnails: 0,
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

/// A press on the dimmed backdrop, in the window's corner, far from every dialog.
fn click_backdrop(cx: &mut VisualTestContext) {
    cx.simulate_click(gpui::point(gpui::px(3.), gpui::px(3.)), Modifiers::none());
    cx.run_until_parked();
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

/// Scrolls `section` to the top of the page, as its navigation link does.
fn show(window: &WindowHandle<ShellView>, section: SettingsSection, cx: &mut VisualTestContext) {
    let page = view(window, cx);
    page.update(cx, |page, cx| page.navigate(section, cx));
    cx.run_until_parked();
}

/// Lets a dialog that was just closed finish fading out.
fn fade_out(cx: &mut VisualTestContext) {
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(500));
    cx.run_until_parked();
}

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

fn control(control: AssetControl, asset_id: i64) -> String {
    settings::asset_control_id(control, asset_id).to_string()
}

fn name_text(
    page: &gpui::Entity<settings::SettingsView>,
    asset_id: i64,
    cx: &VisualTestContext,
) -> Option<String> {
    page.read_with(cx, |page, cx| {
        page.duplicate_name_input(asset_id)
            .map(|input| input.read(cx).text().to_owned())
    })
}

/// Opens the resolver by "Start" and lets the scan answer.
fn start_duplicates(
    window: &WindowHandle<ShellView>,
    fixture: &Fixture,
    cx: &mut VisualTestContext,
) {
    show(window, SettingsSection::Duplicates, cx);
    click(settings::START_DUPLICATES_ID, fixture, cx);
}

// ImportExportSection.test.tsx, "triggers import-export actions": each of the four
// buttons asks the platform for its own dialog (a save dialog for the exports, one file
// for the imports); closed without a file, each ends as cancelled.
#[gpui::test]
fn each_import_export_button_asks_for_its_own_dialog(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    let (window, mut cx) = open_settings(&fixture, cx);
    show(&window, SettingsSection::ImportExport, &mut cx);

    for (button, save, cancelled) in [
        (settings::EXPORT_CSV_ID, true, "CSV export cancelled."),
        (settings::IMPORT_CSV_ID, false, "CSV import cancelled."),
        (
            settings::EXPORT_DB_BUNDLE_ID,
            true,
            "Database backup export cancelled.",
        ),
        (
            settings::IMPORT_DB_BUNDLE_ID,
            false,
            "Database backup import cancelled.",
        ),
    ] {
        click(button, &fixture, &mut cx);
        assert_eq!(cx.did_prompt_for_new_path(), save, "{button}");
        assert_eq!(cx.did_prompt_for_paths(), !save, "{button}");
        if save {
            cx.simulate_new_path_selection(|_| None);
        } else {
            cx.simulate_path_prompt_response(|options| {
                assert!(options.files && !options.directories && !options.multiple);
                None
            });
        }
        settle(&fixture, &mut cx);
        assert_eq!(
            line(&window, OperationSection::ImportExport, &cx),
            cancelled,
            "{button}"
        );
        assert!(!model(&window, &cx, |settings| settings.is_locked()));
    }
    assert!(fixture.backend.calls().iter().all(|call| !matches!(
        call,
        Call::ExportTagsCsv { .. }
            | Call::ImportTagsCsv { .. }
            | Call::ExportDbBundle { .. }
            | Call::ImportDbBundle { .. }
            | Call::InspectDbBundle { .. }
    )));
}

// ImportExportSection.test.tsx, "disables all actions when operation is locked", and
// DuplicatesSection.test.tsx, "disables start while locked": while an operation runs,
// none of the four buttons and not "Start" does anything.
#[gpui::test]
fn every_data_button_is_inert_while_an_operation_runs(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    fixture.backend.on_find_duplicate_assets(|| Ok(groups(1)));
    let (window, mut cx) = open_settings(&fixture, cx);
    let page = view(&window, &cx);
    show(&window, SettingsSection::ImportExport, &mut cx);

    click(settings::EXPORT_CSV_ID, &fixture, &mut cx);
    assert!(cx.did_prompt_for_new_path());
    assert!(model(&window, &cx, |settings| settings.is_locked()));
    for button in [
        settings::EXPORT_CSV_ID,
        settings::IMPORT_CSV_ID,
        settings::EXPORT_DB_BUNDLE_ID,
        settings::IMPORT_DB_BUNDLE_ID,
    ] {
        click(button, &fixture, &mut cx);
        assert!(!cx.did_prompt_for_paths(), "{button} opened a dialog");
    }
    show(&window, SettingsSection::Duplicates, &mut cx);
    click(settings::START_DUPLICATES_ID, &fixture, &mut cx);
    assert!(!page.read_with(&cx, |page, cx| page.resolver_modal().read(cx).is_open()));
    assert!(!model(&window, &cx, |settings| settings.duplicate_dialog_open()));
    assert_eq!(fixture.calls("find_duplicate_assets"), 0);

    // The one dialog that is open is the export's; once it is answered, nothing else
    // waits.
    cx.simulate_new_path_selection(|_| None);
    settle(&fixture, &mut cx);
    assert!(!cx.did_prompt_for_new_path(), "no second save dialog");
    assert_eq!(
        line(&window, OperationSection::ImportExport, &cx),
        "CSV export cancelled."
    );

    // Unlocked, Start works.
    click(settings::START_DUPLICATES_ID, &fixture, &mut cx);
    assert!(page.read_with(&cx, |page, cx| page.resolver_modal().read(cx).is_open()));
    assert_eq!(fixture.calls("find_duplicate_assets"), 1);
}

// DuplicateResolverDialog.test.tsx, "queues rename and submits all staged changes": a
// name typed into the field, queued, and "Save all" sends it as a rename batch.
#[gpui::test]
fn a_typed_rename_is_queued_and_saved(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    fixture
        .backend
        .on_find_duplicate_assets(|| Ok(groups(1)))
        .on_apply_duplicate_resolution_batch(|_| Ok(committed()));
    let (window, mut cx) = open_settings(&fixture, cx);
    let page = view(&window, &cx);
    start_duplicates(&window, &fixture, &mut cx);

    click(&control(AssetControl::Name, 1), &fixture, &mut cx);
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("renamed.jpg");
    settle(&fixture, &mut cx);
    assert_eq!(name_text(&page, 1, &cx).as_deref(), Some("renamed.jpg"));
    page.read_with(&cx, |page, _| {
        assert_eq!(page.duplicate_form().draft(1), Some("renamed.jpg"));
        assert!(
            !page.duplicate_form().has_changes(),
            "typing stages nothing"
        );
    });

    click(&control(AssetControl::QueueRename, 1), &fixture, &mut cx);
    let scanned = model(&window, &cx, |settings| {
        settings.duplicate_groups().to_vec()
    });
    page.read_with(&cx, |page, _| {
        assert!(
            page.duplicate_form().can_save(&scanned, false),
            "Save all is enabled"
        );
    });
    assert!(drawn(&settings::group_ready_selector(0), &mut cx));

    click(settings::RESOLVER_SAVE_ID, &fixture, &mut cx);
    assert_eq!(fixture.calls("apply_duplicate_resolution_batch"), 1);
    assert!(fixture
        .backend
        .calls()
        .contains(&Call::ApplyDuplicateResolutionBatch {
            input: DuplicateResolutionBatchInput {
                scan_revision: 7,
                changes: vec![DuplicateResolutionChangeInput::Rename {
                    asset_id: 1,
                    expected_path: "/media/a/same-0.jpg".into(),
                    expected_record_version: 3,
                    new_file_name: "renamed.jpg".into(),
                }],
            },
        }));
    assert!(
        model(&window, &cx, |settings| settings
            .pending_duplicate_delete()
            .is_none()),
        "a batch without a delete asks nothing"
    );
}

// DuplicateResolverDialog.test.tsx, "closes on backdrop click only when unlocked".
#[gpui::test]
fn the_backdrop_closes_the_resolver_only_while_no_operation_runs(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    fixture.backend.on_find_duplicate_assets(|| Ok(groups(1)));
    let (window, mut cx) = open_settings(&fixture, cx);
    let page = view(&window, &cx);
    let resolver_open = |cx: &VisualTestContext| {
        page.read_with(cx, |page, cx| page.resolver_modal().read(cx).is_open())
    };

    // The scan is asked for and not answered: the resolver is open and locked.
    show(&window, SettingsSection::Duplicates, &mut cx);
    let runtime = window
        .read_with(&cx, |shell, _| shell.runtime().clone())
        .expect("the window is open");
    runtime.update(&mut cx, |runtime, cx| {
        runtime.send(RuntimeInput::settings(SettingsCommand::StartDuplicates), cx);
    });
    cx.run_until_parked();
    assert!(resolver_open(&cx));
    assert!(model(&window, &cx, |settings| settings.is_locked()));
    click_backdrop(&mut cx);
    assert!(resolver_open(&cx), "locked: the backdrop does not close it");
    assert!(model(&window, &cx, |settings| settings.duplicate_dialog_open()));

    settle(&fixture, &mut cx);
    assert!(!model(&window, &cx, |settings| settings.is_locked()));
    click_backdrop(&mut cx);
    settle(&fixture, &mut cx);
    assert!(!resolver_open(&cx), "unlocked: the backdrop closes it");
    assert!(!model(&window, &cx, |settings| settings.duplicate_dialog_open()));
}

// Layer order: the delete confirmation lies over the resolver, so Escape and the
// backdrop answer it alone, as a cancellation, and leave the resolver open.
#[gpui::test]
fn escape_and_the_backdrop_answer_only_the_delete_confirmation_over_the_resolver(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new();
    fixture
        .backend
        .on_find_duplicate_assets(|| Ok(groups(1)))
        .on_apply_duplicate_resolution_batch(|_| Ok(committed()));
    let (window, mut cx) = open_settings(&fixture, cx);
    let page = view(&window, &cx);
    let open = |cx: &VisualTestContext| {
        page.read_with(cx, |page, cx| {
            (
                page.resolver_modal().read(cx).is_open(),
                page.duplicate_delete_modal().read(cx).is_open(),
            )
        })
    };
    start_duplicates(&window, &fixture, &mut cx);
    click(&control(AssetControl::Delete, 2), &fixture, &mut cx);

    for dismiss in ["escape", "backdrop"] {
        click(settings::RESOLVER_SAVE_ID, &fixture, &mut cx);
        assert_eq!(
            open(&cx),
            (true, true),
            "{dismiss}: the confirmation is open"
        );
        if dismiss == "escape" {
            cx.simulate_keystrokes("escape");
        } else {
            click_backdrop(&mut cx);
        }
        settle(&fixture, &mut cx);
        assert_eq!(
            open(&cx),
            (true, false),
            "{dismiss} closed only the confirmation"
        );
        assert!(model(&window, &cx, |settings| settings
            .pending_duplicate_delete()
            .is_none()));
        assert_eq!(
            line(&window, OperationSection::Duplicates, &cx),
            "Applying duplicate changes cancelled."
        );
        page.read_with(&cx, |page, _| {
            assert_eq!(
                page.duplicate_form().staged_changes(),
                [DuplicateChange::delete(2)],
                "{dismiss}: what was staged stays"
            );
        });
        fade_out(&mut cx);
    }
    assert_eq!(fixture.calls("apply_duplicate_resolution_batch"), 0);

    // With nothing over it, Escape closes the resolver.
    cx.simulate_keystrokes("escape");
    settle(&fixture, &mut cx);
    assert_eq!(open(&cx), (false, false));
}

// confirmationPresence.test.tsx, "retains the duplicate counts in the exiting dialog
// after confirmation clears them".
#[gpui::test]
fn the_delete_confirmation_keeps_its_counts_while_it_fades_out(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    fixture
        .backend
        .on_find_duplicate_assets(|| Ok(groups(1)))
        .on_apply_duplicate_resolution_batch(|_| Ok(committed()));
    let (window, mut cx) = open_settings(&fixture, cx);
    let page = view(&window, &cx);
    let presence = |cx: &VisualTestContext| {
        page.read_with(cx, |page, cx| {
            let modal = page.duplicate_delete_modal().read(cx);
            (modal.is_open(), modal.is_present())
        })
    };
    let shown =
        |cx: &VisualTestContext| page.read_with(cx, |page, _| page.duplicate_delete_counts());
    start_duplicates(&window, &fixture, &mut cx);

    // A rename and a delete: two changes, one file deleted.
    click(&control(AssetControl::GenerateUuid, 1), &fixture, &mut cx);
    click(&control(AssetControl::QueueRename, 1), &fixture, &mut cx);
    click(&control(AssetControl::Delete, 2), &fixture, &mut cx);
    click(settings::RESOLVER_SAVE_ID, &fixture, &mut cx);
    assert_eq!(presence(&cx), (true, true));
    assert_eq!(shown(&cx), (2, 1));

    click(settings::DUPLICATE_DELETE_CONFIRM_ID, &fixture, &mut cx);
    assert_eq!(fixture.calls("apply_duplicate_resolution_batch"), 1);
    assert!(model(&window, &cx, |settings| settings
        .pending_duplicate_delete()
        .is_none()));
    assert_eq!(presence(&cx), (false, true), "closed and fading out");
    assert!(drawn(settings::DUPLICATE_DELETE_CONFIRM_ID, &mut cx));
    assert_eq!(shown(&cx), (2, 1), "with the counts it asked about");

    fade_out(&mut cx);
    assert_eq!(presence(&cx), (false, false));
    assert!(!drawn(settings::DUPLICATE_DELETE_CONFIRM_ID, &mut cx));
}

// createDuplicateResolver.test.tsx, "... resets on reopen": a resolver closed with
// staged changes and drafts opens again with neither, even when the scan of the new
// Start fails and the groups stay the same.
#[gpui::test]
fn the_resolver_opens_again_with_a_fresh_form(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    fixture.backend.on_find_duplicate_assets(|| Ok(groups(1)));
    let (window, mut cx) = open_settings(&fixture, cx);
    let page = view(&window, &cx);
    start_duplicates(&window, &fixture, &mut cx);

    click(&control(AssetControl::GenerateUuid, 1), &fixture, &mut cx);
    click(&control(AssetControl::Delete, 2), &fixture, &mut cx);
    assert_ne!(name_text(&page, 1, &cx).as_deref(), Some("same-0.jpg"));
    page.read_with(&cx, |page, _| assert!(page.duplicate_form().has_changes()));
    click(settings::RESOLVER_CLOSE_ID, &fixture, &mut cx);
    fade_out(&mut cx);

    fixture
        .backend
        .on_find_duplicate_assets(|| Err(BackendError::rejected("the scan failed")));
    click(settings::START_DUPLICATES_ID, &fixture, &mut cx);
    assert!(page.read_with(&cx, |page, cx| page.resolver_modal().read(cx).is_open()));
    assert_eq!(fixture.calls("find_duplicate_assets"), 2);
    assert_eq!(
        model(&window, &cx, |settings| settings.duplicate_groups().len()),
        1,
        "the failed scan kept the groups"
    );
    page.read_with(&cx, |page, _| {
        let form = page.duplicate_form();
        assert!(!form.has_changes(), "nothing is staged any more");
        assert_eq!(form.draft(1), Some("same-0.jpg"));
        assert_eq!(form.staged(2), None);
    });
    assert_eq!(name_text(&page, 1, &cx).as_deref(), Some("same-0.jpg"));
}

// DuplicateResolverDialog.test.tsx, "preserves a visible rename input, focus and
// thumbnail images across a small scroll", and "Thumbnails are requested by asset ID and
// version".
#[gpui::test]
fn a_focused_rename_field_keeps_its_draft_and_focus_across_a_small_scroll(cx: &mut TestAppContext) {
    let fixture = Fixture::new();
    fixture.backend.on_find_duplicate_assets(|| Ok(groups(20)));
    let (window, mut cx) = open_settings(&fixture, cx);
    let page = view(&window, &cx);
    start_duplicates(&window, &fixture, &mut cx);

    // The previews of the drawn cards are asked for by asset ID and the version the
    // scan reported.
    let requests: Vec<(i64, String)> = fixture
        .backend
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            Call::RequestThumbnail { request } => Some((request.asset_id, request.version)),
            _ => None,
        })
        .collect();
    assert!(requests.iter().any(|(id, _)| *id == 1), "{requests:?}");
    for (id, version) in &requests {
        assert_eq!(version, &format!("{id:016x}"), "asset {id}");
    }

    click(&control(AssetControl::Name, 1), &fixture, &mut cx);
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("draft.png");
    settle(&fixture, &mut cx);
    let input = page
        .read_with(&cx, |page, _| page.duplicate_name_input(1).cloned())
        .expect("the field of asset 1 is drawn");
    let focused = |cx: &mut VisualTestContext| {
        let handle = input.read_with(cx, |input, cx| input.focus_handle(cx));
        cx.update(|window, _| handle.is_focused(window))
    };
    assert!(focused(&mut cx));
    assert_eq!(name_text(&page, 1, &cx).as_deref(), Some("draft.png"));
    let before = fixture.calls("request_thumbnail");

    let list = page.read_with(&cx, |page, _| page.duplicate_list().clone());
    list.scroll_by(gpui::px(1.));
    page.update(&mut cx, |_, cx| cx.notify());
    settle(&fixture, &mut cx);

    let after = page
        .read_with(&cx, |page, _| page.duplicate_name_input(1).cloned())
        .expect("the field of asset 1 is still drawn");
    assert_eq!(after.entity_id(), input.entity_id(), "the same field");
    assert!(focused(&mut cx), "it keeps the focus");
    assert_eq!(name_text(&page, 1, &cx).as_deref(), Some("draft.png"));
    page.read_with(&cx, |page, _| {
        assert_eq!(page.duplicate_form().draft(1), Some("draft.png"));
    });
    assert!(drawn(&control(AssetControl::Preview, 1), &mut cx));
    assert_eq!(
        fixture.calls("request_thumbnail"),
        before,
        "a card that stays drawn asks for its preview no second time"
    );
    page.read_with(&cx, |page, _| {
        assert_eq!(
            page.duplicate_form().staged(1),
            None::<&DuplicateAction>,
            "typing stages nothing"
        );
    });
}
