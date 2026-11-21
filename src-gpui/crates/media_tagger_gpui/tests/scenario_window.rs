//! The window a comparison scenario starts in (`media_tagger_gpui --scenario <id>`,
//! epics E01 and E06): the start state the E2E profile accepts becomes the shell over an
//! empty library, in the scenario's theme and in English, as the Tauri E2E specs start;
//! and the same scenario always lays out the same, so a rerun of `compare:ui` gives the
//! same capture. GPUI's test platform needs no display; pixels are checked by the
//! headless case of `shell_window`.

use std::sync::Arc;

use gpui::{Bounds, Pixels, Size, TestAppContext, VisualTestContext};
use media_tagger_client::{
    backend::FakeBackend,
    exec::{Executor, ManualExecutor},
    i18n::Language,
    prefs::MemoryPrefsStore,
    runtime::RuntimeConfig,
    scenario::SCENARIOS,
    theme::ThemeName,
};
use media_tagger_core::models::StartAssetQueryResult;
use media_tagger_gpui::{
    profile,
    runtime::RuntimeParts,
    scenario::start_state,
    views::gallery,
    window::{init_application, open_main_window, WINDOW_SIZE},
};

/// What `run` would start for `--scenario <id>` in the E2E build, over an empty
/// library.
fn scenario_parts(id: &str) -> (RuntimeParts, Arc<ManualExecutor>) {
    let start = start_state(profile::E2E, Some(id)).expect("the E2E profile accepts it");
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
        .on_scan_startup_roots(|| Ok(None))
        .on_list_scan_roots(|| Ok(Vec::new()));
    let executor = Arc::new(ManualExecutor::new());
    let parts = RuntimeParts {
        config: RuntimeConfig {
            theme: start.theme,
            language: Language::En,
        },
        backend,
        prefs: Arc::new(MemoryPrefsStore::default()),
        executor: executor.clone() as Arc<dyn Executor>,
    };
    (parts, executor)
}

type Layout = (Size<Pixels>, Vec<Option<Bounds<Pixels>>>);

/// Opens the scenario's main window and returns its viewport and where the shell's
/// parts are drawn.
fn open(cx: &mut TestAppContext, id: &str) -> Layout {
    let (parts, executor) = scenario_parts(id);
    let theme = parts.config.theme;
    let window = cx
        .update(|cx| {
            init_application(theme, cx)?;
            open_main_window(profile::E2E, parts, cx)
        })
        .expect("open the main window");
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    loop {
        cx.run_until_parked();
        if executor.run_all() == 0 {
            break;
        }
    }
    cx.run_until_parked();
    let viewport = cx.update(|window, _| window.viewport_size());
    let bounds = [
        gallery::HEADER_SELECTOR,
        gallery::COUNT_SELECTOR,
        gallery::EMPTY_STATE_SELECTOR,
    ]
    .into_iter()
    .map(|selector| cx.debug_bounds(selector))
    .collect();
    (viewport, bounds)
}

#[test]
fn every_scenario_starts_in_english_in_its_theme() {
    for scenario in SCENARIOS {
        let (parts, _) = scenario_parts(scenario.id);
        assert_eq!(parts.config.language, Language::En, "{}", scenario.id);
        assert_eq!(parts.config.theme, scenario.theme, "{}", scenario.id);
    }
    assert_eq!(
        scenario_parts("shell-empty").0.config.theme,
        ThemeName::Dark,
        "shell-empty is the default dark start"
    );
    assert_eq!(
        scenario_parts("shell-empty-light").0.config.theme,
        ThemeName::Light
    );
}

#[gpui::test]
fn reopening_a_scenario_gives_the_same_layout(cx: &mut TestAppContext) {
    for scenario in SCENARIOS {
        let first = open(cx, scenario.id);
        let second = open(cx, scenario.id);

        assert_eq!(first, second, "{}", scenario.id);
        assert!(
            first.1.iter().all(Option::is_some),
            "{}: header, count and empty state are drawn",
            scenario.id
        );
        assert_eq!(
            (f32::from(first.0.width), f32::from(first.0.height)),
            WINDOW_SIZE,
            "{}: the capture size of compare:ui",
            scenario.id
        );
    }
}

#[gpui::test]
fn the_light_scenario_lays_out_like_the_dark_one(cx: &mut TestAppContext) {
    let dark = open(cx, "shell-empty");
    let light = open(cx, "shell-empty-light");

    assert_eq!(dark, light);
}
