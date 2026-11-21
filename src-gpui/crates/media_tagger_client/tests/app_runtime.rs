//! The application runtime (epic E06) ported from the Solid specification:
//! `src/app/__tests__/startupScanRemount.test.ts`, `shell.test.tsx`,
//! `preferenceStorage.test.tsx` and `gallery.test.tsx`, at the lowest layer that proves
//! each case: the scripted backend on the deterministic executor, and the real core on a
//! temporary profile where the startup scan's single flight and the preference file
//! matter. Every path is a fresh temporary directory.

use std::{
    ffi::OsString,
    fs,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

use media_tagger_client::{
    backend::{
        core_backend::test_support::TempCore, AssetQueryRequest, Backend, BackendError, FakeBackend,
    },
    exec::{Executor, ManualExecutor, ThreadExecutor},
    i18n::{I18n, Language, LocaleEnvironment},
    model::Driver,
    prefs::{FilePrefsStore, MemoryPrefsStore, Prefs, PrefsStore, PREFS_FILE_NAME},
    runtime::{
        AppRuntime, RuntimeConfig, RuntimeInput, RuntimeRunner, StartupScanOutcome,
        StartupScanPhase, View,
    },
    shell::{empty_state, item_count, EmptyState},
    theme::ThemeName,
};
use media_tagger_core::models::{ScanCompletion, ScanRoot, ScanSummary, StartAssetQueryResult};

const DARK_EN: RuntimeConfig = RuntimeConfig {
    theme: ThemeName::Dark,
    language: Language::En,
};

/// The jobs `Start` queues, in order: the library's query, its known tags, the startup
/// scan, the scan roots, the preference save.
const START_JOBS: usize = 5;

struct Fixture {
    fake: Option<Arc<FakeBackend>>,
    executor: Arc<ManualExecutor>,
    driver: Driver<AppRuntime, RuntimeRunner>,
}

impl Fixture {
    fn with(
        backend: Arc<dyn Backend>,
        fake: Option<Arc<FakeBackend>>,
        prefs: Arc<dyn PrefsStore>,
        config: RuntimeConfig,
    ) -> Self {
        let executor = Arc::new(ManualExecutor::new());
        let driver = Driver::new(
            AppRuntime::new(config),
            RuntimeRunner::new(backend, prefs),
            executor.clone() as Arc<dyn Executor>,
        );
        Self {
            fake,
            executor,
            driver,
        }
    }

    fn fake(fake: &Arc<FakeBackend>) -> Self {
        Self::with(
            fake.clone(),
            Some(fake.clone()),
            Arc::new(MemoryPrefsStore::default()),
            DARK_EN,
        )
    }

    fn runtime(&self) -> &AppRuntime {
        self.driver.model()
    }

    fn send(&mut self, input: RuntimeInput) {
        self.driver.handle(input);
    }

    fn settle(&mut self) {
        while self.executor.run_all() > 0 || self.driver.pump() > 0 {}
    }

    fn calls(&self, method: &str) -> usize {
        self.fake
            .as_ref()
            .expect("a scripted backend")
            .called_methods()
            .into_iter()
            .filter(|called| *called == method)
            .count()
    }

    fn total(&self) -> usize {
        self.runtime().query().total()
    }
}

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

/// A scripted library whose size the test changes, as `rows` in the Solid spec.
fn library_of(rows: &Arc<AtomicUsize>) -> Arc<FakeBackend> {
    let backend = Arc::new(FakeBackend::new());
    let rows_now = rows.clone();
    backend
        .on_start_asset_query(move |_| Ok(ready(rows_now.load(Ordering::SeqCst))))
        .on_list_scan_roots(|| Ok(Vec::new()));
    backend
}

/// startupScanRemount.test.ts "refreshes the replacement with %i initial rows after
/// shared scan completion" for 0, 3 and 160 rows: the window shows the library while the
/// startup scan runs; when the scan finishes with work, library and roots are refreshed
/// exactly once more and the new row is published, and the scan is never asked again.
#[test]
fn a_startup_scan_with_work_publishes_the_rows_it_added_for_any_initial_library() {
    for count in [0, 3, 160] {
        let rows = Arc::new(AtomicUsize::new(count));
        let backend = library_of(&rows);
        backend.on_scan_startup_roots(|| Ok(Some(summary(4))));
        let mut fx = Fixture::fake(&backend);

        fx.send(RuntimeInput::Start);
        fx.send(RuntimeInput::Start);
        assert_eq!(
            fx.executor.pending(),
            START_JOBS,
            "{count}: one start sequence"
        );
        // Everything but the scan answers first: the library's query and its known
        // tags (both ahead of the scan), then the roots and the preference save (each
        // now right behind the scan, which is the oldest job left).
        assert!(fx.executor.run_nth(0));
        assert!(fx.executor.run_nth(0));
        assert!(fx.executor.run_nth(1));
        assert!(fx.executor.run_nth(1));
        assert_eq!(fx.executor.pending(), 1);
        fx.driver.pump();
        assert_eq!(
            fx.calls("scan_startup_roots"),
            0,
            "{count}: the scan still waits"
        );
        assert_eq!(fx.total(), count, "{count}");
        assert!(fx.runtime().query().session_id().is_some());
        assert!(!fx.runtime().library_loading());
        assert!(fx.runtime().maintenance_loading(), "{count}");
        assert!(matches!(
            fx.runtime().startup_scan(),
            StartupScanPhase::Scanning
        ));

        // The scan commits one more row and reports its summary.
        rows.store(count + 1, Ordering::SeqCst);
        fx.settle();

        assert_eq!(fx.total(), count + 1, "{count}");
        assert_eq!(fx.calls("start_asset_query"), 2, "{count}");
        assert_eq!(fx.calls("list_scan_roots"), 2, "{count}");
        assert_eq!(fx.calls("scan_startup_roots"), 1, "{count}");
        assert!(!fx.runtime().maintenance_loading(), "{count}");
        match fx.runtime().startup_scan().outcome() {
            Some(StartupScanOutcome::Scanned(summary)) => assert_eq!(summary.indexed, 4),
            other => panic!("{count}: unexpected outcome {other:?}"),
        }
        fx.send(RuntimeInput::Start);
        fx.settle();
        assert_eq!(
            fx.calls("scan_startup_roots"),
            1,
            "{count}: never asked again"
        );
    }
}

/// startupScanRemount.test.ts "replays completion on a later mount and skips refresh
/// when the launch had no work", the second half: a launch without work refreshes
/// nothing after the scan.
#[test]
fn a_launch_without_startup_work_refreshes_nothing_more() {
    let rows = Arc::new(AtomicUsize::new(2));
    let backend = library_of(&rows);
    backend.on_scan_startup_roots(|| Ok(None));
    let mut fx = Fixture::fake(&backend);

    fx.send(RuntimeInput::Start);
    fx.settle();

    assert_eq!(fx.calls("start_asset_query"), 1);
    assert_eq!(fx.calls("list_scan_roots"), 1);
    assert_eq!(fx.calls("scan_startup_roots"), 1);
    assert!(matches!(
        fx.runtime().startup_scan().outcome(),
        Some(StartupScanOutcome::NoWork)
    ));
    assert!(!fx.runtime().maintenance_loading());
    assert_eq!(fx.total(), 2);
}

/// startupScanRemount.test.ts "refreshes committed rows and exposes a shared failure
/// only in the replacement": a scan that fails after committing rows still refreshes
/// the library and the roots, the failure is kept, and nothing retries the scan.
#[test]
fn a_failed_startup_scan_publishes_the_rows_it_committed_and_is_not_retried() {
    let rows = Arc::new(AtomicUsize::new(0));
    let backend = library_of(&rows);
    let committed = rows.clone();
    backend.on_scan_startup_roots(move || {
        committed.store(1, Ordering::SeqCst);
        Err(BackendError::rejected("synthetic scan failure"))
    });
    let mut fx = Fixture::fake(&backend);

    fx.send(RuntimeInput::Start);
    fx.settle();

    assert_eq!(fx.total(), 1);
    assert_eq!(fx.calls("start_asset_query"), 2);
    assert_eq!(fx.calls("list_scan_roots"), 2);
    match fx.runtime().startup_scan().outcome() {
        Some(StartupScanOutcome::Failed(error)) => {
            assert!(error.text().contains("synthetic scan failure"))
        }
        other => panic!("unexpected outcome {other:?}"),
    }
    assert!(!fx.runtime().maintenance_loading());
    fx.send(RuntimeInput::Start);
    fx.settle();
    assert_eq!(fx.calls("scan_startup_roots"), 1);
}

/// Acceptance "the startup scan starts once (single flight)" on the real core, and the
/// replay half of startupScanRemount.test.ts "replays completion on a later mount": a
/// second runtime over the same launch (the Solid frontend's replacement mount) gets the
/// first scan's result and does not scan again, so a file added after that scan stays
/// out of the library.
#[test]
fn a_second_runtime_of_the_same_launch_replays_the_startup_scan_on_the_real_core() {
    let core = TempCore::start();
    let root = TempCore::text(&core.media_dir());
    core.backend().add_scan_root(root.clone()).unwrap();
    core.backend().set_scan_root_auto_scan(root, true).unwrap();
    for index in 0..3 {
        core.write_image(&format!("image-{index:02}.png"));
    }
    let core = core.relaunch();
    let prefs: Arc<dyn PrefsStore> = Arc::new(MemoryPrefsStore::default());

    let mut first = Fixture::with(core.shared(), None, prefs.clone(), DARK_EN);
    first.send(RuntimeInput::Start);
    first.settle();
    match first.runtime().startup_scan().outcome() {
        Some(StartupScanOutcome::Scanned(summary)) => assert_eq!(summary.indexed, 3),
        other => panic!("unexpected outcome {other:?}"),
    }
    assert_eq!(first.total(), 3);
    assert_eq!(first.runtime().scan_roots().len(), 1);
    assert_eq!(empty_state(first.runtime()), None);

    core.write_image("added-later.png");
    let mut second = Fixture::with(core.shared(), None, prefs, DARK_EN);
    second.send(RuntimeInput::Start);
    second.settle();
    match second.runtime().startup_scan().outcome() {
        Some(StartupScanOutcome::Scanned(summary)) => assert_eq!(summary.indexed, 3),
        other => panic!("unexpected outcome {other:?}"),
    }
    assert!(!second.runtime().maintenance_loading());
    // Read by an owner of its own, apart from both runtimes' query models.
    match core
        .backend()
        .start_asset_query(AssetQueryRequest::new("replay-check", 1))
        .unwrap()
    {
        StartAssetQueryResult::Ready { total, .. } => {
            assert_eq!(total, 3, "the scan was replayed, not run again")
        }
        StartAssetQueryResult::Superseded => panic!("a fresh owner is never superseded"),
    }
}

/// The start sequence on an empty real profile: no roots, no startup work, the "Add
/// first folder" empty state and the start preferences stored.
#[test]
fn an_empty_real_profile_starts_into_the_first_folder_state() {
    let core = TempCore::start();
    let profile = tempfile::tempdir().unwrap();
    let prefs: Arc<dyn PrefsStore> = Arc::new(FilePrefsStore::new(profile.path()));
    let mut fx = Fixture::with(core.shared(), None, prefs.clone(), DARK_EN);

    fx.send(RuntimeInput::Start);
    fx.settle();

    assert!(fx.runtime().query().session_id().is_some());
    assert_eq!(fx.total(), 0);
    assert!(!fx.runtime().has_scan_roots());
    assert!(matches!(
        fx.runtime().startup_scan().outcome(),
        Some(StartupScanOutcome::NoWork)
    ));
    assert_eq!(empty_state(fx.runtime()), Some(EmptyState::NoFolders));
    assert_eq!(
        prefs.load(),
        Prefs::default()
            .with_theme(ThemeName::Dark)
            .with_language(Language::En)
    );
}

/// shell.test.tsx "highlights scanning only for the first-folder route and clears it
/// on Back", with the empty state that offers the route.
#[test]
fn the_first_folder_route_highlights_scanning_until_back() {
    let backend = library_of(&Arc::new(AtomicUsize::new(0)));
    backend.on_scan_startup_roots(|| Ok(None));
    let mut fx = Fixture::fake(&backend);
    fx.send(RuntimeInput::Start);
    fx.settle();
    assert_eq!(
        empty_state(fx.runtime()).and_then(EmptyState::action_key),
        Some("gallery.addFirstFolder")
    );

    fx.send(RuntimeInput::OpenFirstFolderSettings);
    assert_eq!(fx.runtime().view(), View::Settings);
    assert!(fx.runtime().highlight_scan());
    fx.send(RuntimeInput::CloseSettings);
    fx.send(RuntimeInput::OpenSettings);
    assert_eq!(fx.runtime().view(), View::Settings);
    assert!(!fx.runtime().highlight_scan());
}

/// shell.test.tsx "switches views, translates live, persists appearance and returns
/// with Escape" and preferenceStorage.test.tsx ("none" fault): the change is live at
/// once, survives the round trip through the gallery and is stored for the next launch.
#[test]
fn appearance_changes_survive_the_view_round_trip_and_the_next_launch() {
    let profile = tempfile::tempdir().unwrap();
    let store = Arc::new(FilePrefsStore::new(profile.path()));
    store
        .save(
            &Prefs::default()
                .with_theme(ThemeName::Light)
                .with_language(Language::De),
        )
        .unwrap();
    let stored = store.load();
    let config = RuntimeConfig {
        theme: stored.theme_or_default(),
        language: stored.language_or_system(&LocaleEnvironment::default()),
    };
    assert_eq!(config.theme, ThemeName::Light);
    assert_eq!(config.language, Language::De);
    let backend = Arc::new(FakeBackend::new());
    let mut fx = Fixture::with(backend.clone(), Some(backend), store.clone(), config);

    fx.send(RuntimeInput::OpenSettings);
    fx.send(RuntimeInput::SetTheme(ThemeName::Dark));
    fx.send(RuntimeInput::SetLanguage(Language::Fr));
    assert_eq!(fx.runtime().theme(), ThemeName::Dark);
    assert_eq!(fx.runtime().language(), Language::Fr);
    fx.send(RuntimeInput::CloseSettings);
    fx.send(RuntimeInput::OpenSettings);
    fx.settle();

    assert_eq!(fx.runtime().theme(), ThemeName::Dark);
    assert_eq!(fx.runtime().language(), Language::Fr);
    assert_eq!(fx.runtime().prefs_error(), None);
    let relaunched = FilePrefsStore::new(profile.path()).load();
    assert_eq!(relaunched.theme_or_default(), ThemeName::Dark);
    assert_eq!(
        relaunched.language_or_system(&LocaleEnvironment::default()),
        Language::Fr
    );
}

/// preferenceStorage.test.tsx "mounts and changes preferences with read/write failure":
/// storage that can be neither read nor written starts in the default theme and the
/// system's language, and theme and language still change for the session.
#[test]
fn unusable_preference_storage_starts_on_defaults_and_still_changes_for_the_session() {
    let profile = tempfile::tempdir().unwrap();
    // A directory where the file belongs: reading and replacing it both fail.
    fs::create_dir(profile.path().join(PREFS_FILE_NAME)).unwrap();
    let store = Arc::new(FilePrefsStore::new(profile.path()));
    let stored = store.load();
    for (system, language) in [("pl_PL.UTF-8", Language::Pl), ("sv_SE.UTF-8", Language::En)] {
        let environment = LocaleEnvironment {
            lang: Some(OsString::from(system)),
            ..LocaleEnvironment::default()
        };
        assert_eq!(
            stored.language_or_system(&environment),
            language,
            "{system}"
        );
    }
    let config = RuntimeConfig {
        theme: stored.theme_or_default(),
        language: Language::Pl,
    };
    assert_eq!(config.theme, ThemeName::Dark);
    let backend = Arc::new(FakeBackend::new());
    backend
        .on_start_asset_query(|_| Ok(ready(0)))
        .on_scan_startup_roots(|| Ok(None))
        .on_list_scan_roots(|| Ok(Vec::new()));
    let mut fx = Fixture::with(backend.clone(), Some(backend), store.clone(), config);

    fx.send(RuntimeInput::Start);
    fx.settle();
    assert!(
        fx.runtime().query().session_id().is_some(),
        "the window starts all the same"
    );
    assert!(fx.runtime().prefs_error().is_some());

    fx.send(RuntimeInput::OpenSettings);
    fx.send(RuntimeInput::SetTheme(ThemeName::Light));
    fx.send(RuntimeInput::SetLanguage(Language::Fr));
    fx.settle();
    fx.send(RuntimeInput::CloseSettings);
    fx.send(RuntimeInput::OpenSettings);

    assert_eq!(fx.runtime().theme(), ThemeName::Light);
    assert_eq!(fx.runtime().language(), Language::Fr);
    assert!(fx.runtime().prefs_error().is_some());
    assert!(profile.path().join(PREFS_FILE_NAME).is_dir());
}

/// shell.test.tsx "keeps all eleven locales reactive with normalized document/storage
/// values": every language becomes the runtime's at once and is the one stored;
/// regional and unknown codes normalise as the stored language does.
#[test]
fn every_language_is_live_and_stored() {
    let backend = Arc::new(FakeBackend::new());
    let prefs = Arc::new(MemoryPrefsStore::default());
    let mut fx = Fixture::with(backend.clone(), Some(backend), prefs.clone(), DARK_EN);

    for language in Language::ALL.into_iter().rev() {
        fx.send(RuntimeInput::SetLanguage(language));
        fx.settle();
        assert_eq!(fx.runtime().language(), language);
        assert_eq!(prefs.load().language, Some(language), "{language:?}");
        assert!(!item_count(&I18n::new(language), 2).is_empty());
    }
    assert_eq!(Language::normalize("pl-PL"), Some(Language::Pl));
    assert_eq!(Language::normalize("unsupported"), None);
    assert_eq!(fx.runtime().prefs_error(), None);
}

/// shell.test.tsx "preserves theme fallback": a stored theme other than light or dark
/// starts dark.
#[test]
fn an_unknown_stored_theme_starts_dark() {
    let prefs = Prefs::parse(r#"{"theme": "unknown"}"#);
    let runtime = AppRuntime::new(RuntimeConfig {
        theme: prefs.theme_or_default(),
        language: Language::En,
    });
    assert_eq!(runtime.theme(), ThemeName::Dark);
}

/// gallery.test.tsx "starts queries after mount ... and retains drafts across
/// Settings", the shell's part: the header count shows the library's total, and a
/// round trip through the settings page queries nothing.
#[test]
fn the_count_shows_the_total_and_settings_round_trips_query_nothing() {
    let backend = library_of(&Arc::new(AtomicUsize::new(7)));
    backend.on_scan_startup_roots(|| Ok(None));
    let mut fx = Fixture::fake(&backend);
    fx.send(RuntimeInput::Start);
    fx.settle();
    assert_eq!(fx.calls("start_asset_query"), 1);
    assert_eq!(item_count(&I18n::new(Language::En), fx.total()), "7 items");
    assert_eq!(empty_state(fx.runtime()), None);

    fx.send(RuntimeInput::OpenSettings);
    fx.send(RuntimeInput::CloseSettings);
    assert_eq!(fx.executor.pending(), 0);
    assert_eq!(fx.calls("start_asset_query"), 1);
    assert_eq!(fx.total(), 7);
}

/// Acceptance "models survive a view switch": everything the runtime holds stays as it
/// was through any number of switches.
#[test]
fn runtime_state_survives_view_switches() {
    let backend = library_of(&Arc::new(AtomicUsize::new(5)));
    backend
        .on_scan_startup_roots(|| Ok(Some(summary(2))))
        .on_list_scan_roots(|| {
            Ok(vec![ScanRoot {
                path: "/synthetic/media".into(),
                auto_scan_on_startup: true,
            }])
        });
    let mut fx = Fixture::fake(&backend);
    fx.send(RuntimeInput::Start);
    fx.settle();
    fx.send(RuntimeInput::SetTileSize(260.0));
    fx.send(RuntimeInput::SetTheme(ThemeName::Light));
    fx.send(RuntimeInput::SetLanguage(Language::Ja));
    fx.settle();
    let calls = backend.called_methods().len();

    for _ in 0..3 {
        fx.send(RuntimeInput::OpenSettings);
        fx.send(RuntimeInput::CloseSettings);
        fx.send(RuntimeInput::OpenFirstFolderSettings);
        fx.send(RuntimeInput::CloseSettings);
    }
    fx.settle();

    assert_eq!(fx.runtime().view(), View::Gallery);
    assert_eq!(fx.runtime().tile_size(), 260);
    assert_eq!(fx.runtime().theme(), ThemeName::Light);
    assert_eq!(fx.runtime().language(), Language::Ja);
    assert_eq!(fx.total(), 5);
    assert_eq!(fx.runtime().scan_roots().len(), 1);
    assert!(matches!(
        fx.runtime().startup_scan().outcome(),
        Some(StartupScanOutcome::Scanned(_))
    ));
    assert_eq!(
        backend.called_methods().len(),
        calls,
        "switching calls nothing"
    );
}

/// Scope "the production executor of E02 wired in": the runtime starts on the
/// application's `ThreadExecutor` against the real core, with the replies coming back
/// from its threads, and the startup scan of the launch runs once.
#[test]
fn the_runtime_starts_on_the_production_executor_and_the_real_core() {
    let core = TempCore::start();
    let root = TempCore::text(&core.media_dir());
    core.backend().add_scan_root(root.clone()).unwrap();
    core.backend().set_scan_root_auto_scan(root, true).unwrap();
    core.write_image("one.png");
    core.write_image("two.png");
    let core = core.relaunch();
    let profile = tempfile::tempdir().unwrap();
    let prefs: Arc<dyn PrefsStore> = Arc::new(FilePrefsStore::new(profile.path()));
    let executor = Arc::new(ThreadExecutor::new());
    let mut driver = Driver::new(
        AppRuntime::new(DARK_EN),
        RuntimeRunner::new(core.shared(), prefs.clone()),
        executor.clone() as Arc<dyn Executor>,
    );

    driver.handle(RuntimeInput::Start);
    driver.handle(RuntimeInput::Start);
    let deadline = Instant::now() + Duration::from_secs(60);
    while driver.model().startup_scan().outcome().is_none() || driver.model().library_loading() {
        assert!(
            Instant::now() < deadline,
            "the start sequence did not finish"
        );
        driver.pump_one(Duration::from_millis(100));
    }
    assert!(executor.wait_until_quiet(Duration::from_secs(60)));
    driver.pump();

    match driver.model().startup_scan().outcome() {
        Some(StartupScanOutcome::Scanned(summary)) => assert_eq!(summary.indexed, 2),
        other => panic!("unexpected outcome {other:?}"),
    }
    assert_eq!(driver.model().query().total(), 2);
    assert_eq!(driver.model().scan_roots().len(), 1);
    assert!(!driver.model().maintenance_loading());
    assert_eq!(driver.model().prefs_error(), None);
    assert_eq!(
        prefs.load(),
        Prefs::default()
            .with_theme(ThemeName::Dark)
            .with_language(Language::En)
    );
    executor.shutdown();
}
