//! The runtime against the scripted backend and the deterministic executor.

use std::sync::Arc;

use media_tagger_core::models::{ScanCompletion, ScanRoot, ScanSummary, StartAssetQueryResult};

use super::{
    clamp_tile_size, next_tile_size_by_wheel, AppRuntime, RuntimeConfig, RuntimeInput,
    RuntimeRunner, StartupScanOutcome, StartupScanPhase, View, DEFAULT_TILE_SIZE, LIBRARY_OWNER_ID,
    TILE_SIZE_MAX, TILE_SIZE_MIN,
};
use crate::{
    backend::{fake::sequence, BackendError, FakeBackend},
    exec::{Executor, ManualExecutor},
    i18n::Language,
    model::Driver,
    prefs::{MemoryPrefsStore, Prefs, PrefsStore},
    theme::ThemeName,
};

struct Fixture {
    backend: Arc<FakeBackend>,
    prefs: Arc<MemoryPrefsStore>,
    executor: Arc<ManualExecutor>,
    driver: Driver<AppRuntime, RuntimeRunner>,
}

const CONFIG: RuntimeConfig = RuntimeConfig {
    theme: ThemeName::Dark,
    language: Language::En,
};

fn ready(total: usize) -> StartAssetQueryResult {
    StartAssetQueryResult::Ready {
        session_id: 1,
        revision: 1,
        total,
        offset: 0,
        items: Vec::new(),
    }
}

fn summary(indexed: usize) -> ScanSummary {
    ScanSummary {
        completion: ScanCompletion::Complete,
        indexed,
        removed: 0,
        failed: 0,
    }
}

fn root(path: &str) -> ScanRoot {
    ScanRoot {
        path: path.to_owned(),
        auto_scan_on_startup: true,
    }
}

impl Fixture {
    fn new() -> Self {
        let backend = Arc::new(FakeBackend::new());
        let prefs = Arc::new(MemoryPrefsStore::default());
        let executor = Arc::new(ManualExecutor::new());
        let driver = Driver::new(
            AppRuntime::new(CONFIG),
            RuntimeRunner::new(backend.clone(), prefs.clone()),
            executor.clone() as Arc<dyn Executor>,
        );
        Self {
            backend,
            prefs,
            executor,
            driver,
        }
    }

    fn runtime(&self) -> &AppRuntime {
        self.driver.model()
    }

    fn send(&mut self, input: RuntimeInput) {
        self.driver.handle(input);
    }

    /// Runs every waiting backend call and applies the replies.
    fn settle(&mut self) {
        while self.executor.run_all() > 0 || self.driver.pump() > 0 {}
    }

    fn calls(&self, method: &str) -> usize {
        self.backend
            .called_methods()
            .into_iter()
            .filter(|called| *called == method)
            .count()
    }
}

#[test]
fn starts_in_the_gallery_with_the_configured_theme_and_language() {
    let runtime = AppRuntime::new(RuntimeConfig {
        theme: ThemeName::Light,
        language: Language::Pl,
    });

    assert_eq!(runtime.view(), View::Gallery);
    assert_eq!(runtime.theme(), ThemeName::Light);
    assert_eq!(runtime.language(), Language::Pl);
    assert_eq!(runtime.tile_size(), DEFAULT_TILE_SIZE);
    assert!(!runtime.is_started() && !runtime.maintenance_loading());
    assert!(!runtime.has_scan_roots());
    assert!(matches!(
        runtime.startup_scan(),
        StartupScanPhase::NotStarted
    ));
}

#[test]
fn the_start_sequence_runs_once_and_stores_the_start_preferences() {
    let mut fx = Fixture::new();
    fx.backend
        .on_start_asset_query(|request| {
            assert_eq!(request.owner_id, LIBRARY_OWNER_ID);
            Ok(ready(7))
        })
        .on_scan_startup_roots(|| Ok(None))
        .on_list_scan_roots(|| Ok(vec![root("/media")]));

    fx.send(RuntimeInput::Start);
    fx.send(RuntimeInput::Start);
    assert!(fx.runtime().maintenance_loading());
    fx.settle();

    assert_eq!(fx.calls("start_asset_query"), 1);
    assert_eq!(fx.calls("scan_startup_roots"), 1);
    assert_eq!(fx.calls("list_scan_roots"), 1);
    assert_eq!(fx.runtime().library().total, 7);
    assert!(fx.runtime().library().loaded);
    assert!(fx.runtime().has_scan_roots());
    assert!(matches!(
        fx.runtime().startup_scan().outcome(),
        Some(StartupScanOutcome::NoWork)
    ));
    assert!(!fx.runtime().maintenance_loading());
    assert_eq!(
        fx.prefs.load(),
        Prefs::default()
            .with_theme(ThemeName::Dark)
            .with_language(Language::En)
    );
}

#[test]
fn a_startup_scan_with_work_refreshes_the_library_and_the_roots_before_it_finishes() {
    let mut fx = Fixture::new();
    fx.backend
        .on_start_asset_query(|_| Ok(ready(0)))
        .on_scan_startup_roots(|| Ok(Some(summary(4))))
        .on_list_scan_roots(sequence([Ok(Vec::new()), Ok(vec![root("/media")])]));

    fx.send(RuntimeInput::Start);
    fx.settle();

    assert_eq!(fx.calls("start_asset_query"), 2);
    assert_eq!(fx.calls("list_scan_roots"), 2);
    assert!(fx.runtime().has_scan_roots());
    match fx.runtime().startup_scan().outcome() {
        Some(StartupScanOutcome::Scanned(summary)) => assert_eq!(summary.indexed, 4),
        other => panic!("unexpected outcome {other:?}"),
    }
}

#[test]
fn the_scan_stays_running_until_both_refreshes_answered() {
    let mut fx = Fixture::new();
    fx.backend
        .on_start_asset_query(|_| Ok(ready(1)))
        .on_scan_startup_roots(|| Ok(Some(summary(1))))
        .on_list_scan_roots(|| Ok(Vec::new()));
    fx.send(RuntimeInput::Start);
    // The three start calls and the preference save.
    assert_eq!(fx.executor.run_all(), 4);
    fx.driver.pump();
    assert!(matches!(
        fx.runtime().startup_scan(),
        StartupScanPhase::Refreshing(_)
    ));

    // The library answers first; the roots are still out.
    assert!(fx.executor.run_next());
    fx.driver.pump();
    assert!(fx.runtime().maintenance_loading());

    assert!(fx.executor.run_next());
    fx.driver.pump();
    assert!(!fx.runtime().maintenance_loading());
    assert!(fx.runtime().startup_scan().outcome().is_some());
}

#[test]
fn a_failed_startup_scan_does_not_stop_the_runtime_and_is_kept() {
    let mut fx = Fixture::new();
    fx.backend
        .on_start_asset_query(|_| Ok(ready(2)))
        .on_scan_startup_roots(|| Err(BackendError::rejected("synthetic scan failure")))
        .on_list_scan_roots(|| Ok(Vec::new()));

    fx.send(RuntimeInput::Start);
    fx.settle();

    match fx.runtime().startup_scan().outcome() {
        Some(StartupScanOutcome::Failed(error)) => {
            assert_eq!(error.text(), "synthetic scan failure");
        }
        other => panic!("unexpected outcome {other:?}"),
    }
    // Library and roots were refreshed again after the failure, as Solid does.
    assert_eq!(fx.calls("start_asset_query"), 2);
    assert_eq!(fx.calls("list_scan_roots"), 2);
    assert_eq!(fx.runtime().library().total, 2);

    fx.send(RuntimeInput::OpenSettings);
    assert_eq!(fx.runtime().view(), View::Settings);
    fx.send(RuntimeInput::Start);
    assert_eq!(fx.executor.pending(), 0, "a failed scan is not retried");
}

#[test]
fn a_late_library_reply_is_dropped() {
    let mut fx = Fixture::new();
    fx.backend
        .on_start_asset_query(|request| Ok(ready(if request.generation == 1 { 100 } else { 5 })));

    fx.send(RuntimeInput::RefreshLibrary);
    fx.send(RuntimeInput::RefreshLibrary);
    // The newer refresh answers first, then the older one.
    assert!(fx.executor.run_nth(1));
    assert!(fx.executor.run_next());
    fx.driver.pump();

    assert_eq!(fx.runtime().library().total, 5);
    assert!(!fx.runtime().library_loading());
}

#[test]
fn a_failed_refresh_keeps_what_was_published() {
    let mut fx = Fixture::new();
    fx.backend.on_start_asset_query(sequence_query());
    fx.send(RuntimeInput::RefreshLibrary);
    fx.settle();
    fx.send(RuntimeInput::RefreshLibrary);
    fx.settle();

    assert_eq!(fx.runtime().library().total, 3);
    assert_eq!(
        fx.runtime()
            .library()
            .error
            .as_ref()
            .map(|error| error.text()),
        Some("synthetic query failure")
    );
}

fn sequence_query() -> impl FnMut(
    crate::backend::AssetQueryRequest,
) -> crate::backend::BackendResult<StartAssetQueryResult>
       + Send
       + 'static {
    let mut answers = sequence([
        Ok(ready(3)),
        Err(BackendError::rejected("synthetic query failure")),
    ]);
    move |_| answers()
}

#[test]
fn views_switch_and_the_scan_highlight_ends_with_back() {
    let mut fx = Fixture::new();

    fx.send(RuntimeInput::OpenFirstFolderSettings);
    assert_eq!(fx.runtime().view(), View::Settings);
    assert!(fx.runtime().highlight_scan());

    fx.send(RuntimeInput::CloseSettings);
    assert_eq!(fx.runtime().view(), View::Gallery);
    assert!(!fx.runtime().highlight_scan());

    fx.send(RuntimeInput::OpenSettings);
    assert!(!fx.runtime().highlight_scan());
    assert_eq!(fx.executor.pending(), 0, "switching views calls no backend");
}

#[test]
fn theme_and_language_change_at_once_and_are_saved_in_order() {
    let mut fx = Fixture::new();

    fx.send(RuntimeInput::SetTheme(ThemeName::Light));
    fx.send(RuntimeInput::SetLanguage(Language::De));
    fx.send(RuntimeInput::SetLanguage(Language::Fr));
    assert_eq!(fx.runtime().theme(), ThemeName::Light);
    assert_eq!(fx.runtime().language(), Language::Fr);
    // One save runs at a time; the changes after it are saved together afterwards.
    assert_eq!(fx.executor.pending(), 1);
    fx.settle();

    assert_eq!(fx.prefs.saves(), 2);
    assert_eq!(
        fx.prefs.load(),
        Prefs::default()
            .with_theme(ThemeName::Light)
            .with_language(Language::Fr)
    );
    assert_eq!(fx.runtime().prefs_error(), None);

    fx.send(RuntimeInput::SetTheme(ThemeName::Light));
    assert_eq!(fx.executor.pending(), 0, "an unchanged theme saves nothing");
}

#[test]
fn a_failed_save_keeps_the_change_of_the_session() {
    let mut fx = Fixture::new();
    fx.prefs.fail_saves(true);

    fx.send(RuntimeInput::SetTheme(ThemeName::Light));
    fx.settle();

    assert_eq!(fx.runtime().theme(), ThemeName::Light);
    assert!(fx.runtime().prefs_error().is_some());
    assert_eq!(fx.prefs.load(), Prefs::default());
}

#[test]
fn tile_size_follows_the_slider_and_the_wheel_inside_its_range() {
    let mut fx = Fixture::new();

    fx.send(RuntimeInput::SetTileSize(1000.0));
    assert_eq!(fx.runtime().tile_size(), TILE_SIZE_MAX);
    fx.send(RuntimeInput::SetTileSize(f64::NAN));
    assert_eq!(fx.runtime().tile_size(), TILE_SIZE_MAX);
    fx.send(RuntimeInput::Zoom(-120.0));
    assert_eq!(fx.runtime().tile_size(), TILE_SIZE_MAX);
    fx.send(RuntimeInput::Zoom(120.0));
    assert_eq!(fx.runtime().tile_size(), TILE_SIZE_MAX - 8);
    fx.send(RuntimeInput::SetTileSize(0.0));
    assert_eq!(fx.runtime().tile_size(), TILE_SIZE_MIN);

    assert_eq!(clamp_tile_size(200.4, 188), 200);
    assert_eq!(next_tile_size_by_wheel(188, 0.0), 188);
    assert_eq!(next_tile_size_by_wheel(188, -1.0), 196);
    assert_eq!(next_tile_size_by_wheel(144, 3.0), TILE_SIZE_MIN);
}

#[test]
fn a_late_scan_root_listing_is_dropped() {
    let mut fx = Fixture::new();
    fx.backend
        .on_list_scan_roots(sequence([Ok(vec![root("/old")]), Ok(vec![root("/new")])]));

    fx.send(RuntimeInput::RefreshScanRoots);
    fx.send(RuntimeInput::RefreshScanRoots);
    assert!(fx.executor.run_nth(1));
    assert!(fx.executor.run_next());
    fx.driver.pump();

    // The newer listing ran first and got the first scripted answer; the older one,
    // answering last, is late.
    assert_eq!(fx.runtime().scan_roots(), [root("/old")]);
}
