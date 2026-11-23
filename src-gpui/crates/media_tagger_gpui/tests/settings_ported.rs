//! The settings page's view behaviour from the TypeScript specification
//! (`ScanSettingsSection.test.tsx`, `SettingsPanel.test.tsx`, `DangerZoneSection.test.tsx`,
//! `ClearLibraryConfirmDialog.test.tsx`, `RemoveScanRootConfirmDialog.test.tsx`,
//! `SectionOperationStatus.test.tsx`, `AboutSection.test.tsx`; epic E17) on GPUI's test
//! platform with the scripted backend and the deterministic executor: what each control
//! asks for, what is disabled while an operation runs, the root list's "Show more", the
//! confirmations, the folder dialog and the About links.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

use gpui::{Modifiers, TestAppContext, VisualTestContext, WindowHandle};
use media_tagger_client::{
    backend::{fake::Call, FakeBackend},
    exec::{Executor, ManualExecutor},
    i18n::{I18n, Language},
    prefs::MemoryPrefsStore,
    runtime::{AppRuntime, RuntimeConfig, RuntimeInput},
    scenario::{
        ScenarioSettings, SETTINGS_APPEARANCE_LANGUAGE, SETTINGS_DANGER_ABOUT, SETTINGS_SCAN,
    },
    settings::{
        about::{info, AboutLink},
        layout, navigation, OperationSection, SettingsCommand, SettingsSection,
    },
    theme::ThemeName,
};
use media_tagger_core::models::{
    ClearLibrarySummary, RemoveRootSummary, ScanCompletion, ScanRoot, ScanSummary,
    StartAssetQueryResult, ThumbnailRenderSummary, VideoToolStatus,
};
use media_tagger_gpui::{
    profile,
    runtime::RuntimeParts,
    views::{settings, shell::ShellView},
    window::{init_application, open_main_window},
};

fn summary() -> ScanSummary {
    ScanSummary {
        completion: ScanCompletion::Complete,
        indexed: 1,
        removed: 0,
        failed: 0,
    }
}

fn rendered() -> ThumbnailRenderSummary {
    ThumbnailRenderSummary {
        stale: 0,
        generated: 1,
        failed: 0,
        skipped_failed: 0,
        processed: 1,
        total: 1,
        cancelled: false,
    }
}

fn paths(count: usize) -> Vec<String> {
    ["zebra", "alpha", "media", "video", "photos"]
        .iter()
        .take(count)
        .map(|name| format!("/synthetic/{name}"))
        .collect()
}

struct Fixture {
    backend: Arc<FakeBackend>,
    executor: Arc<ManualExecutor>,
    roots: Arc<Mutex<Vec<String>>>,
}

impl Fixture {
    /// The page over `count` synthetic roots; every settings command answers at once
    /// when the executor runs it.
    fn with_roots(count: usize) -> Self {
        let backend = Arc::new(FakeBackend::new());
        let roots = Arc::new(Mutex::new(paths(count)));
        let listed = roots.clone();
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
            .on_scan_startup_roots(|| Ok(None))
            .on_list_scan_roots(move || {
                Ok(listed
                    .lock()
                    .unwrap()
                    .iter()
                    .map(|path| ScanRoot {
                        path: path.clone(),
                        auto_scan_on_startup: false,
                    })
                    .collect())
            })
            .on_get_video_tool_status(|| {
                Ok(VideoToolStatus {
                    ffmpeg_available: true,
                    ffprobe_available: false,
                })
            })
            .on_set_scan_root_auto_scan(|_, _| Ok(()))
            .on_rescan_all_roots(|| Ok(summary()))
            .on_scan_folder(|_| Ok(summary()))
            .on_add_scan_root(|_| Ok(()))
            .on_render_all_thumbnails(|| Ok(rendered()))
            .on_render_failed_thumbnails(|| Ok(rendered()))
            .on_cancel_render_all_thumbnails(|| Ok(true))
            .on_remove_scan_root(|_| {
                Ok(RemoveRootSummary {
                    removed_assets: 0,
                    removed_thumbnails: 0,
                })
            })
            .on_clear_library_data(|| {
                Ok(ClearLibrarySummary {
                    removed_assets: 0,
                    removed_roots: 0,
                    removed_thumbnails: 0,
                })
            });
        Self {
            backend,
            executor: Arc::new(ManualExecutor::new()),
            roots,
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

    fn called(&self, call: &Call) -> bool {
        self.backend.calls().contains(call)
    }

    fn set_roots(&self, count: usize) {
        *self.roots.lock().unwrap() = paths(count);
    }
}

/// Runs every waiting backend call and what follows from it.
fn settle(fixture: &Fixture, cx: &mut VisualTestContext) {
    loop {
        cx.run_until_parked();
        if fixture.executor.run_all() == 0 {
            break;
        }
    }
    cx.run_until_parked();
}

fn leak(selector: impl Into<String>) -> &'static str {
    Box::leak(selector.into().into_boxed_str())
}

fn drawn(selector: impl Into<String>, cx: &mut VisualTestContext) -> bool {
    cx.debug_bounds(leak(selector)).is_some()
}

/// Clicks the element; its backend calls stay waiting.
fn press(selector: impl Into<String>, cx: &mut VisualTestContext) {
    let selector = leak(selector);
    let at = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} is drawn"))
        .center();
    cx.simulate_click(at, Modifiers::none());
    cx.run_until_parked();
}

/// Clicks the element and settles what it started.
fn click(selector: impl Into<String>, fixture: &Fixture, cx: &mut VisualTestContext) {
    press(selector, cx);
    settle(fixture, cx);
}

/// Lets a closing dialog finish its fade, so its backdrop no longer takes clicks.
fn let_dialogs_fade(cx: &mut VisualTestContext) {
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(500));
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
    send(&window, RuntimeInput::OpenSettings, &mut cx);
    settle(fixture, &mut cx);
    (window, cx)
}

fn send(window: &WindowHandle<ShellView>, input: RuntimeInput, cx: &mut VisualTestContext) {
    let runtime = window
        .read_with(cx, |shell, _| shell.runtime().clone())
        .expect("the window is open");
    runtime.update(cx, |runtime, cx| runtime.send(input, cx));
    cx.run_until_parked();
}

fn command(window: &WindowHandle<ShellView>, command: SettingsCommand, cx: &mut VisualTestContext) {
    send(window, RuntimeInput::settings(command), cx);
}

fn model<R>(
    window: &WindowHandle<ShellView>,
    cx: &VisualTestContext,
    read: impl FnOnce(&AppRuntime) -> R,
) -> R {
    window
        .read_with(cx, |shell, cx| read(shell.runtime().read(cx).model()))
        .expect("the window is open")
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

fn remove_open(window: &WindowHandle<ShellView>, cx: &VisualTestContext) -> bool {
    view(window, cx).read_with(cx, |view, cx| view.remove_modal().read(cx).is_open())
}

fn clear_open(window: &WindowHandle<ShellView>, cx: &VisualTestContext) -> bool {
    view(window, cx).read_with(cx, |view, cx| view.clear_modal().read(cx).is_open())
}

fn scan_line(window: &WindowHandle<ShellView>, cx: &VisualTestContext) -> String {
    model(window, cx, |model| {
        model
            .settings()
            .status(OperationSection::Scan)
            .message
            .resolve(&I18n::new(Language::En))
    })
}

/// ScanSettingsSection.test.tsx "renders roots and triggers all scan actions" and
/// SettingsPanel.test.tsx "wires section actions to their application models": each
/// control of the scan section asks for its own operation with the row's root.
#[gpui::test]
fn every_scan_control_asks_for_its_operation(cx: &mut TestAppContext) {
    let fixture = Fixture::with_roots(2);
    let (window, mut cx) = open_settings(&fixture, cx);
    let first = paths(1)[0].clone();

    click(settings::root_switch_id(0), &fixture, &mut cx);
    assert!(fixture.called(&Call::SetScanRootAutoScan {
        path: first.clone(),
        enabled: true,
    }));
    assert_eq!(fixture.calls("scan_folder"), 0, "the switch does not scan");

    click(settings::RESCAN_ALL_ID, &fixture, &mut cx);
    assert_eq!(fixture.calls("rescan_all_roots"), 1);

    click(settings::root_rescan_id(0), &fixture, &mut cx);
    assert!(fixture.called(&Call::ScanFolder {
        path: first.clone()
    }));

    // "Stop thumbnail render" is there while a render runs.
    assert!(!drawn(settings::STOP_RENDER_ID, &mut cx));
    press(settings::RENDER_ALL_ID, &mut cx);
    assert!(drawn(settings::STOP_RENDER_ID, &mut cx));
    press(settings::STOP_RENDER_ID, &mut cx);
    settle(&fixture, &mut cx);
    assert_eq!(fixture.calls("render_all_thumbnails"), 1);
    assert_eq!(fixture.calls("cancel_render_all_thumbnails"), 1);
    assert!(
        !drawn(settings::STOP_RENDER_ID, &mut cx),
        "gone with the render"
    );

    click(settings::RENDER_FAILED_ID, &fixture, &mut cx);
    assert_eq!(fixture.calls("render_failed_thumbnails"), 1);

    click(settings::root_remove_id(0), &fixture, &mut cx);
    assert_eq!(
        model(&window, &cx, |model| model
            .settings()
            .remove_root_confirm()
            .map(str::to_owned)),
        Some(first)
    );
    assert!(remove_open(&window, &cx));
    assert_eq!(
        fixture.calls("remove_scan_root"),
        0,
        "only after confirmation"
    );
}

/// SettingsPanel.test.tsx "wires ... Choose folder" with `src/platform/tauri/dialogs.ts`:
/// "Choose folder" opens the system folder dialog (several folders, directories only),
/// adds what was chosen, and a cancelled dialog says so.
#[gpui::test]
fn choose_folder_opens_the_system_dialog_and_adds_the_choice(cx: &mut TestAppContext) {
    let fixture = Fixture::with_roots(1);
    let (window, mut cx) = open_settings(&fixture, cx);

    press(settings::CHOOSE_FOLDER_ID, &mut cx);
    assert!(cx.did_prompt_for_paths(), "the folder dialog is open");
    cx.simulate_path_prompt_response(|options| {
        assert!(options.directories && !options.files && options.multiple);
        Some(vec![PathBuf::from("/synthetic/added")])
    });
    settle(&fixture, &mut cx);
    assert!(fixture.called(&Call::AddScanRoot {
        path: "/synthetic/added".into()
    }));

    press(settings::CHOOSE_FOLDER_ID, &mut cx);
    cx.simulate_path_prompt_response(|_| None);
    settle(&fixture, &mut cx);
    assert_eq!(fixture.calls("add_scan_root"), 1);
    assert_eq!(scan_line(&window, &cx), "Folder selection cancelled.");
}

/// ScanSettingsSection.test.tsx "shows all %i roots without a toggle" (0 to 3), with
/// "Rescan all" disabled without roots.
#[gpui::test]
fn up_to_three_roots_show_without_a_toggle(cx: &mut TestAppContext) {
    let fixture = Fixture::with_roots(0);
    let (window, mut cx) = open_settings(&fixture, cx);
    for count in 0..=3 {
        fixture.set_roots(count);
        send(&window, RuntimeInput::RefreshScanRoots, &mut cx);
        settle(&fixture, &mut cx);
        for index in 0..count {
            assert!(
                drawn(settings::root_rescan_id(index), &mut cx),
                "{count}: {index}"
            );
        }
        assert!(!drawn(settings::root_rescan_id(count), &mut cx), "{count}");
        assert!(!drawn(settings::EXPAND_ROOTS_ID, &mut cx), "{count}");
        let before = fixture.calls("rescan_all_roots");
        click(settings::RESCAN_ALL_ID, &fixture, &mut cx);
        assert_eq!(
            fixture.calls("rescan_all_roots"),
            before + usize::from(count > 0),
            "{count}: Rescan all is disabled without roots"
        );
    }
}

/// ScanSettingsSection.test.tsx "expands and collapses in controller order with
/// accessible list controls": three rows, "Show more" reveals the rest in order and
/// their actions use their own root, "Show less" hides them again.
#[gpui::test]
fn the_root_list_expands_and_collapses(cx: &mut TestAppContext) {
    let fixture = Fixture::with_roots(5);
    let (window, mut cx) = open_settings(&fixture, cx);
    let all = paths(5);
    let settings_view = view(&window, &cx);

    for index in 0..3 {
        assert!(drawn(settings::root_rescan_id(index), &mut cx));
    }
    assert!(!drawn(settings::root_rescan_id(3), &mut cx));
    assert!(!settings_view.read_with(&cx, |view, _| view.roots_expanded()));

    click(settings::RESCAN_ALL_ID, &fixture, &mut cx);
    assert_eq!(fixture.calls("rescan_all_roots"), 1);

    click(settings::EXPAND_ROOTS_ID, &fixture, &mut cx);
    assert!(settings_view.read_with(&cx, |view, _| view.roots_expanded()));
    for index in 0..5 {
        assert!(drawn(settings::root_rescan_id(index), &mut cx), "{index}");
    }

    click(settings::root_switch_id(4), &fixture, &mut cx);
    click(settings::root_rescan_id(4), &fixture, &mut cx);
    click(settings::root_remove_id(4), &fixture, &mut cx);
    assert!(fixture.called(&Call::SetScanRootAutoScan {
        path: all[4].clone(),
        enabled: true,
    }));
    assert!(fixture.called(&Call::ScanFolder {
        path: all[4].clone()
    }));
    assert_eq!(
        model(&window, &cx, |model| model
            .settings()
            .remove_root_confirm()
            .map(str::to_owned)),
        Some(all[4].clone())
    );
    click(settings::REMOVE_CANCEL_ID, &fixture, &mut cx);
    let_dialogs_fade(&mut cx);

    click(settings::EXPAND_ROOTS_ID, &fixture, &mut cx);
    assert!(!settings_view.read_with(&cx, |view, _| view.roots_expanded()));
    assert!(!drawn(settings::root_rescan_id(3), &mut cx));
}

/// ScanSettingsSection.test.tsx "preserves expansion through live root refreshes and
/// threshold changes" and "resets expansion after remount".
#[gpui::test]
fn expansion_survives_root_refreshes_and_resets_when_the_page_is_made_again(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::with_roots(5);
    let (window, mut cx) = open_settings(&fixture, cx);
    let refresh = |count: usize, cx: &mut VisualTestContext| {
        fixture.set_roots(count);
        send(&window, RuntimeInput::RefreshScanRoots, cx);
        settle(&fixture, cx);
    };

    click(settings::EXPAND_ROOTS_ID, &fixture, &mut cx);
    refresh(4, &mut cx);
    assert!(
        drawn(settings::root_rescan_id(3), &mut cx),
        "still expanded"
    );
    assert!(view(&window, &cx).read_with(&cx, |view, _| view.roots_expanded()));
    for count in [2, 0] {
        refresh(count, &mut cx);
        assert!(!drawn(settings::EXPAND_ROOTS_ID, &mut cx), "{count}");
        assert!(!drawn(settings::root_rescan_id(count), &mut cx), "{count}");
    }
    refresh(5, &mut cx);
    assert!(
        drawn(settings::root_rescan_id(4), &mut cx),
        "expanded again"
    );

    click(settings::EXPAND_ROOTS_ID, &fixture, &mut cx);
    assert!(!drawn(settings::root_rescan_id(3), &mut cx));
    click(settings::EXPAND_ROOTS_ID, &fixture, &mut cx);

    send(&window, RuntimeInput::CloseSettings, &mut cx);
    settle(&fixture, &mut cx);
    send(&window, RuntimeInput::OpenSettings, &mut cx);
    settle(&fixture, &mut cx);
    assert!(!view(&window, &cx).read_with(&cx, |view, _| view.roots_expanded()));
    assert!(
        !drawn(settings::root_rescan_id(3), &mut cx),
        "collapsed after remount"
    );
}

/// ScanSettingsSection.test.tsx "keeps the display toggle usable while operations and
/// row actions are locked".
#[gpui::test]
fn the_list_toggle_works_while_the_row_actions_are_locked(cx: &mut TestAppContext) {
    let fixture = Fixture::with_roots(5);
    let (window, mut cx) = open_settings(&fixture, cx);

    press(settings::RESCAN_ALL_ID, &mut cx);
    assert!(model(&window, &cx, |model| model.settings().is_locked()));
    press(settings::EXPAND_ROOTS_ID, &mut cx);
    assert!(
        drawn(settings::root_rescan_id(4), &mut cx),
        "the toggle works"
    );
    for index in 0..5 {
        press(settings::root_switch_id(index), &mut cx);
        press(settings::root_rescan_id(index), &mut cx);
        press(settings::root_remove_id(index), &mut cx);
    }
    press(settings::RESCAN_ALL_ID, &mut cx);
    press(settings::EXPAND_ROOTS_ID, &mut cx);
    assert!(!drawn(settings::root_rescan_id(3), &mut cx));
    settle(&fixture, &mut cx);

    assert_eq!(fixture.calls("rescan_all_roots"), 1);
    assert_eq!(fixture.calls("scan_folder"), 0);
    assert_eq!(fixture.calls("set_scan_root_auto_scan"), 0);
    assert!(model(&window, &cx, |model| model
        .settings()
        .remove_root_confirm()
        .is_none()));
}

/// ScanSettingsSection.test.tsx "disables controls when locked and without roots", and
/// DangerZoneSection.test.tsx "disables dangerous action while locked".
#[gpui::test]
fn a_running_operation_disables_the_sections_actions(cx: &mut TestAppContext) {
    let fixture = Fixture::with_roots(0);
    let (window, mut cx) = open_settings(&fixture, cx);
    command(&window, SettingsCommand::RenderAllThumbnails, &mut cx);
    assert!(model(&window, &cx, |model| model.settings().is_locked()));

    press(settings::CHOOSE_FOLDER_ID, &mut cx);
    assert!(!cx.did_prompt_for_paths(), "Choose folder is disabled");
    press(settings::RESCAN_ALL_ID, &mut cx);
    press(settings::RENDER_ALL_ID, &mut cx);
    press(settings::RENDER_FAILED_ID, &mut cx);
    view(&window, &cx).update(&mut cx, |view, cx| {
        view.navigate(SettingsSection::Danger, cx)
    });
    cx.run_until_parked();
    press(settings::CLEAR_LIBRARY_ID, &mut cx);
    assert!(!clear_open(&window, &cx), "the danger action is disabled");
    settle(&fixture, &mut cx);

    assert_eq!(fixture.calls("rescan_all_roots"), 0);
    assert_eq!(
        fixture.calls("render_all_thumbnails"),
        1,
        "only the running one"
    );
    assert_eq!(fixture.calls("render_failed_thumbnails"), 0);
    assert_eq!(fixture.calls("clear_library_data"), 0);

    // DangerZoneSection.test.tsx "opens clear confirmation callback" once unlocked.
    click(settings::CLEAR_LIBRARY_ID, &fixture, &mut cx);
    assert!(clear_open(&window, &cx));
}

/// SectionOperationStatus.test.tsx "renders spinner status while section is loading".
#[gpui::test]
fn the_status_shows_a_spinner_while_its_operation_runs(cx: &mut TestAppContext) {
    let fixture = Fixture::with_roots(1);
    let (window, mut cx) = open_settings(&fixture, cx);
    assert!(!drawn(settings::SCAN_LOADER_SELECTOR, &mut cx));
    press(settings::RESCAN_ALL_ID, &mut cx);
    assert!(drawn(settings::SCAN_LOADER_SELECTOR, &mut cx));
    assert_eq!(scan_line(&window, &cx), "Rescanning all folders...");
    settle(&fixture, &mut cx);
    assert!(!drawn(settings::SCAN_LOADER_SELECTOR, &mut cx));
}

/// RemoveScanRootConfirmDialog.test.tsx and ClearLibraryConfirmDialog.test.tsx "disables
/// actions when operation is locked", with the page's Escape: while an operation runs an
/// open confirmation ignores Escape and its buttons; afterwards Escape closes it.
#[gpui::test]
fn the_confirmations_hold_still_while_an_operation_runs(cx: &mut TestAppContext) {
    let fixture = Fixture::with_roots(1);
    let (window, mut cx) = open_settings(&fixture, cx);

    click(settings::root_remove_id(0), &fixture, &mut cx);
    assert!(remove_open(&window, &cx));
    command(&window, SettingsCommand::RescanAll, &mut cx);
    cx.simulate_keystrokes("escape");
    assert!(remove_open(&window, &cx), "Escape is ignored while locked");
    press(settings::REMOVE_CANCEL_ID, &mut cx);
    press(settings::REMOVE_CONFIRM_ID, &mut cx);
    assert!(remove_open(&window, &cx), "the buttons are disabled");
    settle(&fixture, &mut cx);
    assert_eq!(fixture.calls("remove_scan_root"), 0);
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(!remove_open(&window, &cx), "Escape closes it once unlocked");
    assert!(model(&window, &cx, |model| model
        .settings()
        .remove_root_confirm()
        .is_none()));

    command(&window, SettingsCommand::OpenClearConfirm, &mut cx);
    assert!(clear_open(&window, &cx));
    command(&window, SettingsCommand::RescanAll, &mut cx);
    cx.simulate_keystrokes("escape");
    assert!(clear_open(&window, &cx), "Escape is ignored while locked");
    press(settings::CLEAR_CONFIRM_ID, &mut cx);
    press(settings::CLEAR_CANCEL_ID, &mut cx);
    assert!(clear_open(&window, &cx), "the buttons are disabled");
    settle(&fixture, &mut cx);
    assert_eq!(fixture.calls("clear_library_data"), 0);
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(!clear_open(&window, &cx), "Escape closes it once unlocked");
}

/// ClearLibraryConfirmDialog.test.tsx / RemoveScanRootConfirmDialog.test.tsx "runs
/// confirm and ... callbacks" and SettingsPanel.test.tsx "wires clear and root
/// confirmation callbacks": the dialogs are absent until asked for, "YES" clears and
/// "Remove" removes the asked root, each closing its dialog.
#[gpui::test]
fn the_confirmations_run_their_operations(cx: &mut TestAppContext) {
    let fixture = Fixture::with_roots(1);
    let (window, mut cx) = open_settings(&fixture, cx);
    assert!(!remove_open(&window, &cx) && !clear_open(&window, &cx));
    assert!(!drawn(settings::REMOVE_CONFIRM_ID, &mut cx));
    assert!(!drawn(settings::CLEAR_CONFIRM_ID, &mut cx));

    click(settings::root_remove_id(0), &fixture, &mut cx);
    click(settings::REMOVE_CONFIRM_ID, &fixture, &mut cx);
    assert!(fixture.called(&Call::RemoveScanRoot {
        path: paths(1)[0].clone()
    }));
    assert!(!remove_open(&window, &cx));
    let_dialogs_fade(&mut cx);

    view(&window, &cx).update(&mut cx, |view, cx| {
        view.navigate(SettingsSection::Danger, cx)
    });
    cx.run_until_parked();
    click(settings::CLEAR_LIBRARY_ID, &fixture, &mut cx);
    assert!(clear_open(&window, &cx));
    click(settings::CLEAR_CONFIRM_ID, &fixture, &mut cx);
    assert_eq!(fixture.calls("clear_library_data"), 1);
    assert!(!clear_open(&window, &cx));
}

/// AboutSection.test.tsx "shows build metadata ... without opening the browser" and
/// "uses the system opener only after a click": nothing is opened until a link is
/// clicked, then the repository and its issue tracker.
#[gpui::test]
fn the_about_links_open_the_browser_only_when_clicked(cx: &mut TestAppContext) {
    let fixture = Fixture::with_roots(1);
    let (window, mut cx) = open_settings(&fixture, cx);
    view(&window, &cx).update(&mut cx, |view, cx| {
        view.navigate(SettingsSection::About, cx)
    });
    cx.run_until_parked();
    assert_eq!(cx.opened_url(), None);

    click(
        settings::about_link_id(AboutLink::Source),
        &fixture,
        &mut cx,
    );
    assert_eq!(cx.opened_url(), Some(info().repository.clone()));
    click(
        settings::about_link_id(AboutLink::Contact),
        &fixture,
        &mut cx,
    );
    assert_eq!(
        cx.opened_url(),
        Some(format!("{}/issues", info().repository))
    );
}

/// AboutSection.test.tsx "places About last and moves navigation focus to its section":
/// About is the last link and the last section, its link marks it, and scrolling back
/// to the top marks the first section again.
#[gpui::test]
fn about_is_last_and_the_mark_follows_the_scroll_position(cx: &mut TestAppContext) {
    let fixture = Fixture::with_roots(1);
    let (window, mut cx) = open_settings(&fixture, cx);
    let link_tops: Vec<_> = SettingsSection::ALL
        .iter()
        .map(|section| {
            cx.debug_bounds(leak(settings::nav_link_id(*section).to_string()))
                .expect("the link is drawn")
                .top()
        })
        .collect();
    let section_tops: Vec<_> = SettingsSection::ALL
        .iter()
        .map(|section| {
            cx.debug_bounds(leak(settings::section_id(*section).to_string()))
                .expect("the section is drawn")
                .top()
        })
        .collect();
    assert_eq!(SettingsSection::ALL.last(), Some(&SettingsSection::About));
    assert!(
        link_tops.windows(2).all(|pair| pair[0] < pair[1]),
        "About's link is last"
    );
    assert!(
        section_tops.windows(2).all(|pair| pair[0] < pair[1]),
        "About's section is last"
    );

    click(
        settings::nav_link_id(SettingsSection::About),
        &fixture,
        &mut cx,
    );
    let page = view(&window, &cx);
    assert_eq!(
        page.read_with(&cx, |view, _| view.active_section()),
        SettingsSection::About
    );
    page.update(&mut cx, |view, cx| view.navigate(SettingsSection::Scan, cx));
    cx.run_until_parked();
    assert_eq!(
        page.read_with(&cx, |view, _| view.active_section()),
        SettingsSection::Scan
    );
}

/// Acceptance "Parytet `settings-scan`, `settings-appearance-language`,
/// `settings-danger-about`", the part a test can observe without a display: each
/// scenario's start state opens the page, scrolls the section its Tauri steps click to
/// and marks it, and `settings-scan` turns the one root's startup scan on through the
/// backend once the startup scan has ended.
#[gpui::test]
fn the_settings_scenarios_reach_their_start_states(cx: &mut TestAppContext) {
    for (scenario, section) in [
        (SETTINGS_SCAN, SettingsSection::Scan),
        (SETTINGS_APPEARANCE_LANGUAGE, SettingsSection::Appearance),
        (SETTINGS_DANGER_ABOUT, SettingsSection::About),
    ] {
        let fixture = Fixture::with_roots(1);
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
        let script: ScenarioSettings = scenario;
        runtime.update(&mut vcx, |runtime, cx| runtime.script_settings(script, cx));
        for _ in 0..4 {
            settle(&fixture, &mut vcx);
            vcx.update(|window, _| window.refresh());
        }
        settle(&fixture, &mut vcx);

        let page = view(&window, &vcx);
        assert_eq!(
            page.read_with(&vcx, |view, _| view.active_section()),
            section,
            "{scenario:?}"
        );
        // The click is made on the page the launch's work left, so the section lands
        // where a click on the settled Tauri page puts it.
        let section_bounds = |section: SettingsSection, cx: &mut VisualTestContext| {
            cx.debug_bounds(leak(settings::section_id(section).to_string()))
                .expect("the section is drawn")
        };
        match section {
            SettingsSection::Appearance => assert_eq!(
                section_bounds(section, &mut vcx).top(),
                gpui::px(navigation::SCROLL_MARGIN_TOP),
                "{scenario:?}"
            ),
            SettingsSection::About => {
                let viewport = vcx.update(|window, _| window.viewport_size());
                assert_eq!(
                    viewport.height - section_bounds(section, &mut vcx).bottom(),
                    gpui::px(layout::PAGE_PADDING_BOTTOM + layout::VIEW_PADDING_BOTTOM),
                    "{scenario:?}"
                );
            }
            _ => {}
        }
        // A pointer click: the focus is on the clicked link's section, without a ring.
        if let Some(target) = scenario.section {
            let handle = page.read_with(&vcx, |view, _| view.section_focus(target).clone());
            assert!(
                vcx.update(|window, _| handle.is_focused(window)),
                "{scenario:?}"
            );
        }
        assert!(
            !drawn(settings::SECTION_FOCUS_RING_SELECTOR, &mut vcx),
            "{scenario:?}"
        );
        let auto_scan = fixture.calls("set_scan_root_auto_scan");
        if scenario.first_root_auto_scan == Some(true) {
            assert!(fixture.called(&Call::SetScanRootAutoScan {
                path: paths(1)[0].clone(),
                enabled: true,
            }));
            assert!(model(&window, &vcx, |model| model.scan_roots()[0]
                .auto_scan_on_startup));
            assert_eq!(
                scan_line(&window, &vcx),
                "Saved. Applies on the next app launch."
            );
        } else {
            assert_eq!(auto_scan, 0, "{scenario:?}");
        }
        vcx.update(|window, _| window.remove_window());
        vcx.run_until_parked();
    }
}
