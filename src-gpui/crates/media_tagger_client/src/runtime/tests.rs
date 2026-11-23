//! The runtime against the scripted backend and the deterministic executor.

use std::sync::Arc;

use media_tagger_core::models::{
    ScanCompletion, ScanRoot, ScanSummary, StartAssetQueryResult, TagListPage,
};

use super::{
    clamp_tile_size, next_tile_size_by_wheel, AppRuntime, RuntimeConfig, RuntimeInput,
    RuntimeRunner, StartupScanOutcome, StartupScanPhase, View, DEFAULT_TILE_SIZE, TILE_SIZE_MAX,
    TILE_SIZE_MIN,
};
use crate::{
    backend::{fake::sequence, BackendError, FakeBackend},
    exec::{Executor, ManualExecutor},
    i18n::Language,
    library::QueryError,
    model::Driver,
    prefs::{MemoryPrefsStore, Prefs, PrefsStore},
    settings::SettingsCommand,
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
            assert_eq!(request.generation, 1);
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
    assert_eq!(fx.runtime().query().total(), 7);
    assert_eq!(fx.runtime().query().session_id(), Some(1));
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
    fx.backend.on_list_tags(|_, _, _| {
        Ok(TagListPage {
            items: vec!["cat".into()],
            total: 1,
        })
    });
    fx.send(RuntimeInput::Start);
    // The query, the known tags, the scan, the roots, the preference save and the
    // video tools.
    assert_eq!(fx.executor.run_all(), 6);
    fx.driver.pump();
    assert!(matches!(
        fx.runtime().startup_scan(),
        StartupScanPhase::Refreshing(_)
    ));

    // The first refresh also asked for the timeline sections of its session; the
    // refresh after the scan made that read obsolete.
    assert_eq!(fx.executor.pending(), 4);
    assert!(fx.executor.run_next());
    fx.driver.pump();
    assert!(fx.runtime().maintenance_loading());

    // The query answers first; the known tags and the roots are still out.
    assert!(fx.executor.run_next());
    fx.driver.pump();
    assert!(fx.runtime().maintenance_loading());
    assert!(!fx.runtime().library_loading());

    // The known tags answer; the roots are still out.
    assert!(fx.executor.run_next());
    fx.driver.pump();
    assert!(fx.runtime().maintenance_loading());
    assert!(!fx.runtime().library().refreshing());

    assert!(fx.executor.run_next());
    fx.driver.pump();
    assert!(!fx.runtime().maintenance_loading());
    assert!(fx.runtime().startup_scan().outcome().is_some());
    assert_eq!(**fx.runtime().library().known_tags(), ["cat"]);
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
    assert_eq!(fx.runtime().query().total(), 2);

    fx.send(RuntimeInput::OpenSettings);
    assert_eq!(fx.runtime().view(), View::Settings);
    fx.send(RuntimeInput::Start);
    assert_eq!(fx.executor.pending(), 0, "a failed scan is not retried");
}

/// An operation that held the settings runner when the runtime started (the folder
/// dialog) keeps the runner and its line. The startup scan is then no operation of the
/// runner: the runtime refreshes the library and the roots itself after a scan that found
/// work, and the scan is finished once both answered.
#[test]
fn a_startup_scan_that_cannot_take_the_runner_is_refreshed_by_the_runtime() {
    let mut fx = Fixture::new();
    fx.backend
        .on_start_asset_query(|_| Ok(ready(1)))
        .on_scan_startup_roots(|| Ok(Some(summary(3))))
        .on_list_scan_roots(sequence([Ok(Vec::new()), Ok(vec![root("/media")])]));

    fx.send(RuntimeInput::settings(SettingsCommand::PickFolder));
    assert!(fx.runtime().settings().is_locked());
    fx.send(RuntimeInput::Start);
    assert!(!fx.runtime().settings().startup_running());
    assert!(fx.runtime().maintenance_loading());
    fx.settle();

    assert_eq!(fx.calls("start_asset_query"), 2);
    assert_eq!(fx.calls("list_scan_roots"), 2);
    assert!(fx.runtime().has_scan_roots());
    match fx.runtime().startup_scan().outcome() {
        Some(StartupScanOutcome::Scanned(summary)) => assert_eq!(summary.indexed, 3),
        other => panic!("unexpected outcome {other:?}"),
    }
    assert!(!fx.runtime().maintenance_loading());
    assert!(
        fx.runtime().settings().is_locked(),
        "the folder dialog still holds the runner"
    );
}

#[test]
fn a_late_library_reply_is_dropped() {
    let mut fx = Fixture::new();
    fx.backend
        .on_start_asset_query(|request| Ok(ready(if request.generation == 1 { 100 } else { 5 })));

    fx.send(RuntimeInput::RefreshLibrary);
    fx.send(RuntimeInput::RefreshLibrary);
    // The first query start, the one pass over the known tags that both refreshes
    // share, the second query start.
    assert_eq!(fx.executor.pending(), 3);
    // The newer refresh answers first, then the older one.
    assert!(fx.executor.run_nth(2));
    assert!(fx.executor.run_next());
    fx.driver.pump();

    assert_eq!(fx.runtime().query().total(), 5);
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

    // The rows published before stay; the failed refresh leaves no session.
    assert_eq!(fx.runtime().query().total(), 3);
    assert_eq!(fx.runtime().query().session_id(), None);
    assert_eq!(
        fx.runtime().query().load_error(),
        Some(&QueryError::Backend(BackendError::rejected(
            "synthetic query failure"
        )))
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

/// E07 through the runtime: the gallery's viewport, a ticketed read and the stale
/// debounce timer of the embedded query model travel as `RuntimeInput::Query` and
/// reach the backend through the runtime's runner.
#[test]
fn the_embedded_query_model_loads_pages_answers_reads_and_runs_its_stale_timer() {
    use media_tagger_core::models::{AssetQueryPageResult, AssetSummary};

    use crate::library::{QueryEvent, QueryInput, Ticket, Viewport, STALE_REFRESH_DELAY};

    fn row(id: i64) -> AssetSummary {
        AssetSummary {
            id,
            file_name: format!("{id}.jpg"),
            preview_path: None,
            kind: "image".into(),
            modified_at: 0,
            width: Some(10),
            height: Some(10),
            duration_ms: None,
            thumb_version: "0123456789abcdef".into(),
            is_favorite: false,
            media_group_key: None,
            media_group_order: None,
        }
    }
    let mut fx = Fixture::new();
    fx.backend
        .on_start_asset_query(|request| {
            let first = request.page_size as i64;
            Ok(StartAssetQueryResult::Ready {
                session_id: 1,
                revision: 1,
                total: 300,
                offset: 0,
                items: (1..=first).map(row).collect(),
            })
        })
        .on_get_asset_query_page(|session_id, offset, limit| {
            Ok(if offset >= 256 {
                AssetQueryPageResult::Stale
            } else {
                AssetQueryPageResult::Ready {
                    session_id,
                    revision: 1,
                    total: 300,
                    offset,
                    items: (offset..offset + limit)
                        .map(|i| row(i as i64 + 1))
                        .collect(),
                }
            })
        });
    fx.send(RuntimeInput::RefreshLibrary);
    fx.settle();
    assert_eq!(fx.runtime().query().total(), 300);
    assert_eq!(fx.runtime().query().page_size(), 128);

    fx.send(RuntimeInput::Query(QueryInput::SetViewport(Some(
        Viewport {
            start_index: 130,
            end_index: 140,
            visible_start_index: 130,
            visible_end_index: 140,
            paused: false,
        },
    ))));
    fx.settle();
    assert_eq!(fx.calls("get_asset_query_page"), 1);
    assert_eq!(fx.runtime().query().asset_at(130).map(|a| a.id), Some(131));

    fx.send(RuntimeInput::Query(QueryInput::ReadAsset {
        ticket: Ticket(1),
        index: 290,
    }));
    fx.settle();
    let events = fx.driver.model_mut().take_query_events();
    assert_eq!(
        events,
        [QueryEvent::AssetRead {
            ticket: Ticket(1),
            result: Ok(None)
        }],
        "a stale page is a hole"
    );
    assert_eq!(fx.calls("start_asset_query"), 1);

    fx.executor.advance(STALE_REFRESH_DELAY);
    fx.settle();
    assert_eq!(fx.calls("start_asset_query"), 2, "the stale refresh ran");
    assert!(!fx.runtime().library_loading());
}

/// E08 through the runtime: details, a tag pin and metadata writes of the embedded
/// library model travel as `RuntimeInput::Library`, reach the backend through the
/// runtime's runner, and a refresh of the query that passes through the runtime
/// empties the library's details cache.
#[test]
fn the_embedded_library_model_loads_details_and_queues_writes_through_the_runtime() {
    use media_tagger_core::models::{
        AssetDetails, AssetSummary, SetAssetTagsSummary, TagQueryImpact,
    };

    use crate::library::{
        LibraryEvent, LibraryInput, Mutation, MutationOutcome, QueryInput, TagEdit, Ticket,
    };

    fn details(id: i64) -> AssetDetails {
        AssetDetails {
            summary: AssetSummary {
                id,
                file_name: format!("{id}.jpg"),
                preview_path: None,
                kind: "image".into(),
                modified_at: 0,
                width: Some(10),
                height: Some(10),
                duration_ms: None,
                thumb_version: "0123456789abcdef".into(),
                is_favorite: false,
                media_group_key: None,
                media_group_order: None,
            },
            path: format!("/synthetic/{id}.jpg"),
            size_bytes: 10,
            thumb_path: None,
            tags: vec!["cat".into()],
        }
    }
    let mut fx = Fixture::new();
    fx.backend
        .on_start_asset_query(|_| Ok(ready(1)))
        .on_list_tags(|_, _, _| {
            Ok(TagListPage {
                items: vec!["cat".into()],
                total: 1,
            })
        })
        .on_get_asset_details(|id| Ok(Some(details(id))))
        .on_set_asset_favorite(|_, _| Ok(()))
        .on_set_asset_tags(|asset_id, tags| {
            Ok(SetAssetTagsSummary {
                vocabulary_changed: true,
                query_impact: TagQueryImpact::None,
                asset_id,
                changed: true,
                tags,
                revision: 2,
            })
        });
    fx.send(RuntimeInput::RefreshLibrary);
    fx.settle();
    assert_eq!(**fx.runtime().library().known_tags(), ["cat"]);
    assert!(!fx.runtime().library().can_edit_tags(7));

    fx.send(RuntimeInput::library(LibraryInput::PinTags(7)));
    fx.send(RuntimeInput::library(LibraryInput::LoadDetails {
        ticket: Ticket(1),
        asset_id: 7,
    }));
    fx.settle();
    assert!(fx.runtime().library().can_edit_tags(7));
    assert!(fx.runtime().library().cached_details(7).is_some());

    // A tag edit and a favorite right behind it: one write at a time, in order.
    fx.send(RuntimeInput::library(LibraryInput::Mutate {
        ticket: Ticket(2),
        mutation: Mutation::SaveTags {
            asset_id: 7,
            edit: TagEdit::Change {
                add: vec!["dog".into()],
                remove: Vec::new(),
            },
        },
    }));
    fx.send(RuntimeInput::library(LibraryInput::Mutate {
        ticket: Ticket(3),
        mutation: Mutation::SetFavorite {
            asset_id: 7,
            favorite: true,
        },
    }));
    assert_eq!(fx.executor.pending(), 1);
    assert_eq!(fx.runtime().library().pending_writes(), 2);
    fx.settle();

    let library = fx.runtime().library();
    assert_eq!(
        library.asset_tags(7).map(|known| known.tags.to_vec()),
        Some(vec!["cat".to_owned(), "dog".to_owned()])
    );
    assert!(library.cached_details(7).unwrap().summary.is_favorite);
    assert_eq!(library.pending_writes(), 0);
    // The save changed the vocabulary: the known tags were read again.
    assert_eq!(fx.calls("list_tags"), 2);
    let events = fx.driver.model_mut().take_library_events();
    let answered: Vec<u64> = events
        .iter()
        .filter_map(|event| match event {
            LibraryEvent::DetailsLoaded { ticket, result } => {
                assert!(matches!(result, Ok(Some(_))));
                Some(ticket.0)
            }
            LibraryEvent::MutationSettled { ticket, result } => {
                assert!(matches!(
                    result,
                    Ok(MutationOutcome::TagsSaved { .. } | MutationOutcome::FavoriteSet { .. })
                ));
                Some(ticket.0)
            }
            _ => None,
        })
        .collect();
    assert_eq!(answered, [1, 2, 3]);

    // Details belong to one query epoch, whichever input changed it.
    fx.send(RuntimeInput::Query(QueryInput::Refresh));
    assert!(fx.runtime().library().cached_details(7).is_none());
    assert!(fx.runtime().library().can_edit_tags(7), "the tags stay");
}

/// mutations.test.ts "keeps one library across Settings navigation and starts
/// queries with 128-item pages": the library lives in the runtime, so leaving the
/// gallery for the settings page and coming back keeps its rows, details, tags and
/// known tags, and asks the backend for nothing.
#[test]
fn the_library_survives_settings_navigation_and_queries_in_pages_of_128() {
    use media_tagger_core::models::{AssetDetails, AssetSummary};

    use crate::{
        backend::fake::Call,
        library::{LibraryInput, Ticket},
    };

    let mut fx = Fixture::new();
    fx.backend
        .on_start_asset_query(|_| Ok(ready(1)))
        .on_list_tags(|_, _, _| {
            Ok(TagListPage {
                items: vec!["cat".into()],
                total: 1,
            })
        })
        .on_get_asset_details(|id| {
            Ok(Some(AssetDetails {
                summary: AssetSummary {
                    id,
                    file_name: format!("{id}.jpg"),
                    preview_path: None,
                    kind: "image".into(),
                    modified_at: 0,
                    width: Some(10),
                    height: Some(10),
                    duration_ms: None,
                    thumb_version: "0123456789abcdef".into(),
                    is_favorite: false,
                    media_group_key: None,
                    media_group_order: None,
                },
                path: format!("/synthetic/{id}.jpg"),
                size_bytes: 10,
                thumb_path: None,
                tags: vec!["cat".into()],
            }))
        });
    fx.send(RuntimeInput::RefreshLibrary);
    fx.settle();
    fx.send(RuntimeInput::library(LibraryInput::LoadDetails {
        ticket: Ticket(1),
        asset_id: 1,
    }));
    fx.settle();
    let calls = fx.backend.calls().len();

    fx.send(RuntimeInput::OpenSettings);
    assert_eq!(fx.runtime().view(), View::Settings);
    fx.send(RuntimeInput::CloseSettings);
    assert_eq!(fx.runtime().view(), View::Gallery);

    let library = fx.runtime().library();
    assert_eq!(library.query().total(), 1);
    assert!(library.cached_details(1).is_some());
    assert!(library.can_edit_tags(1));
    assert_eq!(**library.known_tags(), ["cat"]);
    assert_eq!(fx.executor.pending(), 0);
    assert_eq!(
        fx.backend.calls().len(),
        calls,
        "navigation asks for nothing"
    );
    let page_sizes: Vec<usize> = fx
        .backend
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            Call::StartAssetQuery { request } => Some(request.page_size),
            _ => None,
        })
        .collect();
    assert_eq!(page_sizes, [128]);
}

/// E11 through the runtime: the selection reads the embedded query model, follows its
/// epochs, keeps its session alive with the runner's timer and writes through the
/// library's selection mutations.
mod selection {
    use std::sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    };

    use media_tagger_core::models::{SelectionMutationResult, StartAssetQueryResult};

    use super::{Fixture, RuntimeInput};
    use crate::{
        backend::{fake::Call, AssetIdRange, BackendError, SelectionMutation, SelectionSpec},
        gallery::{
            Interaction, SelectionError, SelectionInput, SelectionNotice, SelectionWrite,
            SESSION_KEEPALIVE,
        },
        library::{
            LibraryEvent, LibraryInput, MutationOutcome, QueryError, QueryEvent, QueryInput, Ticket,
        },
    };

    const TOTAL: usize = 50_000;

    struct Script {
        session: Arc<AtomicU64>,
        fail_start: Arc<AtomicBool>,
    }

    /// A library of `TOTAL` results whose asset ID is index + 1, in the session
    /// `Script::session` names at the time of each query start.
    fn library(fx: &Fixture) -> Script {
        let script = Script {
            session: Arc::new(AtomicU64::new(1)),
            fail_start: Arc::new(AtomicBool::new(false)),
        };
        let (session, fail_start) = (script.session.clone(), script.fail_start.clone());
        fx.backend
            .on_start_asset_query(move |_| {
                if fail_start.load(Ordering::SeqCst) {
                    return Err(BackendError::rejected("i18n:common.libraryBusy"));
                }
                Ok(StartAssetQueryResult::Ready {
                    session_id: session.load(Ordering::SeqCst),
                    revision: 1,
                    total: TOTAL,
                    offset: 0,
                    items: Vec::new(),
                })
            })
            .on_get_asset_query_id_range(|_, start, end| {
                Ok(AssetIdRange::Ready(
                    (start..=end).map(|index| index as i64 + 1).collect(),
                ))
            });
        script
    }

    fn started(fx: &mut Fixture) -> Script {
        let script = library(fx);
        fx.send(RuntimeInput::RefreshLibrary);
        fx.settle();
        select(fx, SelectionInput::ToggleMode);
        script
    }

    fn select(fx: &mut Fixture, input: SelectionInput) {
        fx.send(RuntimeInput::Selection(input));
    }

    fn click(fx: &mut Fixture, index: usize, ctrl_like: bool, shift: bool) {
        select(
            fx,
            SelectionInput::Interact(Interaction::Click {
                asset_id: index as i64 + 1,
                asset_index: index,
                ctrl_like,
                shift,
            }),
        );
    }

    fn id_range_reads(fx: &Fixture) -> Vec<(u64, usize, usize)> {
        fx.backend
            .calls()
            .into_iter()
            .filter_map(|call| match call {
                Call::GetAssetQueryIdRange {
                    session_id,
                    start,
                    end,
                } => Some((session_id, start, end)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_shift_range_is_read_through_the_query_model_and_published_from_its_answer() {
        let mut fx = Fixture::new();
        started(&mut fx);

        click(&mut fx, 0, false, false);
        click(&mut fx, 2, false, true);
        assert!(fx.runtime().selection().busy());
        assert_eq!(
            &*fx.runtime().selection().ids(),
            [1],
            "nothing before the answer"
        );
        fx.settle();

        let selection = fx.runtime().selection();
        assert_eq!(&*selection.ids(), [1, 2, 3]);
        assert!(!selection.busy() && !selection.ranged());
        assert_eq!(id_range_reads(&fx), [(1, 0, 2)]);
        assert!(!fx.runtime().gallery_load_failed());
        // The answer went to the selection, not to the other readers of the query.
        assert!(fx.driver.model_mut().take_query_events().is_empty());

        // Another caller's read is still answered through the query events.
        fx.send(RuntimeInput::Query(QueryInput::ReadIdRange {
            ticket: Ticket(7),
            start: 0,
            end: 1,
        }));
        fx.settle();
        assert_eq!(
            fx.driver.model_mut().take_query_events(),
            [QueryEvent::IdRangeRead {
                ticket: Ticket(7),
                result: Ok(vec![1, 2])
            }]
        );
        assert_eq!(&*fx.runtime().selection().ids(), [1, 2, 3]);
    }

    #[test]
    fn a_failed_range_read_shows_the_gallery_error_and_retry_reads_again() {
        let mut fx = Fixture::new();
        started(&mut fx);
        let fail = Arc::new(AtomicBool::new(true));
        let failing = fail.clone();
        fx.backend
            .on_get_asset_query_id_range(move |_, start, end| {
                if failing.load(Ordering::SeqCst) {
                    return Err(BackendError::rejected("i18n:common.libraryBusy"));
                }
                Ok(AssetIdRange::Ready(
                    (start..=end).map(|index| index as i64 + 1).collect(),
                ))
            });

        click(&mut fx, 9, false, false);
        select(
            &mut fx,
            SelectionInput::Interact(Interaction::RectangleCommit {
                ranges: vec![[0, 2]],
                additive: false,
            }),
        );
        fx.settle();
        assert!(fx.runtime().gallery_load_failed());
        assert!(matches!(
            fx.runtime().selection().error(),
            Some(SelectionError::Read(QueryError::Backend(_)))
        ));
        assert_eq!(
            &*fx.runtime().selection().ids(),
            [10],
            "the selection from before"
        );

        fail.store(false, Ordering::SeqCst);
        fx.send(RuntimeInput::Query(QueryInput::RetryLoad));
        select(&mut fx, SelectionInput::Retry);
        fx.settle();
        assert!(!fx.runtime().gallery_load_failed());
        assert_eq!(&*fx.runtime().selection().ids(), [1, 2, 3]);
    }

    #[test]
    fn a_refresh_that_overtakes_a_range_read_leaves_neither_a_selection_nor_an_error() {
        let mut fx = Fixture::new();
        started(&mut fx);

        select(
            &mut fx,
            SelectionInput::Interact(Interaction::RectangleCommit {
                ranges: vec![[0, 2]],
                additive: false,
            }),
        );
        assert!(fx.runtime().selection().busy());
        fx.send(RuntimeInput::RefreshLibrary);
        assert!(!fx.runtime().selection().busy());
        assert_eq!(fx.runtime().selection().error(), None);
        fx.settle();

        let selection = fx.runtime().selection();
        assert_eq!(selection.count(), 0);
        assert_eq!(selection.error(), None);
        assert!(fx.driver.model_mut().take_query_events().is_empty());
    }

    #[test]
    fn the_selection_survives_a_refresh_by_the_query_epochs_and_a_failed_query_drops_its_ranges() {
        let mut fx = Fixture::new();
        let script = started(&mut fx);

        // IDs held directly survive a new session and keep their identity.
        click(&mut fx, 4, false, false);
        let key = fx.runtime().selection().key();
        script.session.store(2, Ordering::SeqCst);
        fx.send(RuntimeInput::RefreshLibrary);
        fx.settle();
        assert_eq!(&*fx.runtime().selection().ids(), [5]);
        assert_eq!(fx.runtime().selection().key(), key);
        assert_eq!(fx.runtime().selection().notice(), None);

        // Ranges survive a refresh that returns the same session (a cache hit).
        select(&mut fx, SelectionInput::Interact(Interaction::SelectAll));
        fx.settle();
        assert!(fx.runtime().selection().ranged());
        fx.send(RuntimeInput::RefreshLibrary);
        assert!(
            fx.runtime().selection().ranged(),
            "a pending query changes nothing"
        );
        fx.settle();
        assert_eq!(fx.runtime().selection().count(), TOTAL);
        assert_eq!(fx.runtime().selection().notice(), None);

        // Another session: the positions mean other assets.
        script.session.store(3, Ordering::SeqCst);
        fx.send(RuntimeInput::RefreshLibrary);
        fx.settle();
        let selection = fx.runtime().selection();
        assert_eq!((selection.count(), selection.ranged()), (0, false));
        assert_eq!(selection.notice(), Some(SelectionNotice::RangeCleared));
        assert!(selection.enabled());

        // A refresh that fails leaves the query without a session: ranges go with the
        // expired notice, and nothing reads the old session any more.
        select(&mut fx, SelectionInput::Interact(Interaction::SelectAll));
        fx.settle();
        assert!(fx.runtime().selection().ranged());
        script.fail_start.store(true, Ordering::SeqCst);
        fx.send(RuntimeInput::RefreshLibrary);
        fx.settle();
        let selection = fx.runtime().selection();
        assert_eq!(selection.count(), 0);
        assert_eq!(selection.notice(), Some(SelectionNotice::RangeExpired));
        assert_eq!(selection.keepalive_session(), None);
        assert_eq!(fx.runtime().selection().capture().spec, None);

        // IDs held directly stay through a failed refresh.
        click(&mut fx, 7, false, false);
        fx.send(RuntimeInput::RefreshLibrary);
        fx.settle();
        assert_eq!(&*fx.runtime().selection().ids(), [8]);
        assert_eq!(fx.runtime().selection().notice(), None);
    }

    #[test]
    fn a_held_range_reads_one_id_of_its_session_at_once_and_then_every_minute() {
        let mut fx = Fixture::new();
        started(&mut fx);
        fx.backend.take_calls();

        select(&mut fx, SelectionInput::Interact(Interaction::SelectAll));
        fx.settle();
        assert_eq!(id_range_reads(&fx), [(1, 0, 0)]);
        assert_eq!(fx.executor.pending_timers(), 1);

        // Longer than the backend's five-minute session lifetime.
        for _ in 0..6 {
            fx.executor.advance_and_run(SESSION_KEEPALIVE);
            fx.settle();
        }
        assert_eq!(id_range_reads(&fx), [(1, 0, 0); 7]);
        assert!(fx.runtime().selection().ranged());
        assert_eq!(fx.executor.pending_timers(), 1);

        select(&mut fx, SelectionInput::Clear);
        fx.executor.advance_and_run(SESSION_KEEPALIVE);
        fx.settle();
        fx.executor.advance_and_run(SESSION_KEEPALIVE);
        fx.settle();
        assert_eq!(id_range_reads(&fx).len(), 7);
        assert_eq!(fx.executor.pending_timers(), 0);
    }

    #[test]
    fn a_range_whose_session_is_already_gone_is_dropped_with_the_expired_notice() {
        let mut fx = Fixture::new();
        let script = started(&mut fx);
        fx.backend
            .on_get_asset_query_id_range(|_, _, _| Ok(AssetIdRange::Stale));

        select(&mut fx, SelectionInput::Interact(Interaction::SelectAll));
        assert!(fx.runtime().selection().ranged());
        // The stale keepalive read schedules the query's refresh; its new session
        // finds nothing left to clear.
        script.session.store(2, Ordering::SeqCst);
        fx.executor.run_all();
        fx.driver.pump();
        let selection = fx.runtime().selection();
        assert_eq!(selection.count(), 0);
        assert_eq!(selection.notice(), Some(SelectionNotice::RangeExpired));

        fx.executor
            .advance_and_run(crate::library::STALE_REFRESH_DELAY);
        fx.settle();
        assert_eq!(fx.runtime().query().session_id(), Some(2));
        assert_eq!(
            fx.runtime().selection().notice(),
            Some(SelectionNotice::RangeExpired)
        );
    }

    #[test]
    fn a_range_selection_is_written_through_apply_selection_mutation_and_a_stale_write_clears_it() {
        let mut fx = Fixture::new();
        let script = started(&mut fx);
        select(&mut fx, SelectionInput::Interact(Interaction::SelectAll));
        click(&mut fx, 2, true, false);
        fx.settle();
        let spec = SelectionSpec {
            session_id: 1,
            ranges: vec![[0, TOTAL - 1]],
            excluded: vec![3],
            included: Vec::new(),
        };
        let capture = fx.runtime().selection().capture();
        assert_eq!(capture.spec.as_ref(), Some(&spec));
        assert_eq!(capture.count, TOTAL - 1);

        // The backend finds the session gone: nothing is written.
        fx.backend
            .on_apply_selection_mutation(|_, _| Ok(SelectionMutationResult::Stale));
        script.session.store(2, Ordering::SeqCst);
        let mutation = capture
            .spec_mutation(SelectionWrite::ToggleFavorites)
            .expect("a range selection writes through its spec");
        fx.backend.take_calls();
        fx.send(RuntimeInput::library(LibraryInput::Mutate {
            ticket: Ticket(1),
            mutation,
        }));
        fx.settle();

        let writes: Vec<_> = fx
            .backend
            .calls()
            .into_iter()
            .filter_map(|call| match call {
                Call::ApplySelectionMutation {
                    selection,
                    mutation,
                } => Some((selection, mutation)),
                _ => None,
            })
            .collect();
        assert_eq!(writes, [(spec, SelectionMutation::ToggleFavorite)]);
        let settled = fx
            .driver
            .model_mut()
            .take_library_events()
            .into_iter()
            .find_map(|event| match event {
                LibraryEvent::MutationSettled { ticket, result } => Some((ticket, result)),
                _ => None,
            })
            .expect("the write was answered");
        assert_eq!(settled.0, Ticket(1));
        assert!(matches!(
            settled.1,
            Ok(MutationOutcome::Selection(SelectionMutationResult::Stale))
        ));
        // The refresh the stale write asked for brought another session.
        let selection = fx.runtime().selection();
        assert_eq!(selection.count(), 0);
        assert_eq!(selection.notice(), Some(SelectionNotice::RangeCleared));
        assert_eq!(selection.keepalive_session(), None);
    }

    #[test]
    fn an_identity_reset_clears_the_selection_and_keeps_selection_mode() {
        use crate::library::LibraryReset;

        let mut fx = Fixture::new();
        started(&mut fx);
        click(&mut fx, 0, false, false);
        click(&mut fx, 1, false, false);
        assert_eq!(fx.runtime().selection().count(), 2);

        fx.send(RuntimeInput::library(LibraryInput::Reset(
            LibraryReset::Identity,
        )));
        let selection = fx.runtime().selection();
        assert_eq!(selection.count(), 0);
        assert!(selection.enabled());
        assert_eq!(selection.notice(), None);
    }
}

#[test]
fn a_valid_search_submit_starts_the_query_with_its_filters_and_an_invalid_one_does_not() {
    use crate::{
        backend::{MediaKind, MetaFilter},
        search::{SearchCommand, SearchFilterValidationError},
    };
    let mut fx = Fixture::new();
    let requests = Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = Arc::clone(&requests);
    fx.backend.on_start_asset_query(move |request| {
        seen.lock().unwrap().push(request.clone());
        Ok(ready(2))
    });

    fx.send(RuntimeInput::Search(SearchCommand::Change(
        "Cat -dog".into(),
    )));
    fx.settle();
    assert_eq!(fx.calls("start_asset_query"), 0, "typing applies nothing");
    fx.send(RuntimeInput::Search(SearchCommand::ApplyKind(Some(
        MediaKind::Image,
    ))));
    fx.settle();
    {
        let requests = requests.lock().unwrap();
        let request = requests.last().expect("a query");
        assert_eq!(
            (request.tags_and.clone(), request.tags_not.clone()),
            (vec!["cat".to_owned()], vec!["dog".to_owned()])
        );
        assert_eq!(request.kind, Some(MediaKind::Image));
    }
    assert_eq!(fx.runtime().query().total(), 2);
    assert_eq!(fx.runtime().query().filters().include, ["cat"]);

    fx.send(RuntimeInput::Search(SearchCommand::ApplyInput(
        "gN:".into(),
    )));
    fx.settle();
    assert_eq!(fx.calls("start_asset_query"), 1);
    assert_eq!(
        fx.runtime().search().validation_error(),
        Some(SearchFilterValidationError::GroupNameMissingValue)
    );
    fx.send(RuntimeInput::Search(SearchCommand::ApplyInput(
        "tags:2".into(),
    )));
    fx.settle();
    assert_eq!(fx.calls("start_asset_query"), 2);
    assert_eq!(
        requests.lock().unwrap().last().unwrap().meta_filter,
        Some(MetaFilter::HasNoTags { tag_count: 2 })
    );
}

#[test]
fn suggestion_searches_run_on_the_executor_against_the_known_tags() {
    use crate::search::{
        read_active_token, EditorId, SuggestionContext, SuggestionEvent, SuggestionInput,
        SuggestionRequest,
    };
    let mut fx = Fixture::new();
    let editor = EditorId::next();
    let vocabulary = Arc::new(vec!["cat".to_owned(), "car".to_owned(), "dog".to_owned()]);
    fx.send(RuntimeInput::Suggestions(SuggestionInput::Search {
        editor,
        vocabulary,
        request: SuggestionRequest {
            context: SuggestionContext {
                token: read_active_token("ca", 2),
                ..SuggestionContext::default()
            },
            retry: 0,
        },
    }));
    assert!(fx
        .driver
        .model_mut()
        .take_suggestion_events_for(editor)
        .is_empty());
    assert_eq!(
        fx.executor.pending(),
        1,
        "the search waits for the executor"
    );
    fx.settle();
    let events = fx.driver.model_mut().take_suggestion_events_for(editor);
    match &events[..] {
        [SuggestionEvent::Settled {
            result: Ok(items), ..
        }] => {
            let values: Vec<_> = items.iter().map(|item| item.value.as_str()).collect();
            assert_eq!(values, ["cat", "car"]);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_tag_list_loads_its_pages_through_the_backend_and_applies_after_the_query_answered() {
    use crate::search::tag_list::{TagListInput, SINGLE_CLICK_DELAY};
    let mut fx = Fixture::new();
    fx.backend
        .on_start_asset_query(|_| Ok(ready(3)))
        .on_list_tags(|query, offset, limit| {
            assert_eq!((query.as_str(), offset, limit), ("", 0, 100));
            Ok(TagListPage {
                items: vec!["cat".into(), "dog".into()],
                total: 2,
            })
        });

    fx.send(RuntimeInput::TagList(TagListInput::Open));
    assert!(fx.runtime().tag_list().is_open());
    fx.settle();
    assert_eq!(fx.runtime().tag_list().page().items, ["cat", "dog"]);

    // A click waits for its timer on the executor.
    fx.send(RuntimeInput::TagList(TagListInput::Click {
        tag: "cat".into(),
        count: 1,
    }));
    fx.settle();
    assert!(fx.runtime().tag_list().selections().is_empty());
    fx.executor.advance_and_run(SINGLE_CLICK_DELAY);
    fx.settle();
    assert_eq!(fx.runtime().tag_list().selections().filter_input(), "cat");
    fx.send(RuntimeInput::TagList(TagListInput::DoubleClick {
        tag: "dog".into(),
    }));

    let queries = fx.calls("start_asset_query");
    fx.send(RuntimeInput::TagList(TagListInput::Apply));
    assert_eq!(fx.runtime().search().filter_input(), "cat -dog");
    assert!(fx.runtime().tag_list().applying());
    assert!(
        fx.runtime().tag_list().is_open(),
        "open until the query answered"
    );
    fx.settle();
    assert_eq!(fx.calls("start_asset_query"), queries + 1);
    assert!(!fx.runtime().tag_list().is_open());
    assert!(!fx.runtime().tag_list().applying());
    assert_eq!(fx.runtime().search().applied().include, ["cat"]);
}

#[test]
fn a_failed_query_keeps_the_tag_list_open_with_its_choices() {
    use crate::search::tag_list::TagListInput;
    let mut fx = Fixture::new();
    fx.backend
        .on_start_asset_query(|_| Err(BackendError::rejected("offline")))
        .on_list_tags(|_, _, _| {
            Ok(TagListPage {
                items: vec!["cat".into()],
                total: 1,
            })
        });
    fx.send(RuntimeInput::TagList(TagListInput::Open));
    fx.settle();
    fx.send(RuntimeInput::TagList(TagListInput::DoubleClick {
        tag: "cat".into(),
    }));
    fx.send(RuntimeInput::TagList(TagListInput::Apply));
    fx.settle();

    let tag_list = fx.runtime().tag_list();
    assert!(tag_list.is_open() && !tag_list.applying());
    assert_eq!(tag_list.selections().filter_input(), "-cat");
    assert_eq!(fx.runtime().search().filter_input(), "-cat");
}

#[test]
fn the_tag_list_falls_back_to_the_known_tags_of_the_library() {
    use crate::search::tag_list::TagListInput;
    let mut fx = Fixture::new();
    fx.backend
        .on_start_asset_query(|_| Ok(ready(0)))
        .on_scan_startup_roots(|| Ok(None))
        .on_list_scan_roots(|| Ok(Vec::new()))
        .on_list_tags({
            let mut next = sequence(vec![
                Ok(TagListPage {
                    items: vec!["beta".into(), "alpha".into()],
                    total: 2,
                }),
                Err(BackendError::rejected("offline")),
            ]);
            move |_, _, _| next()
        });
    fx.send(RuntimeInput::Start);
    fx.settle();
    assert_eq!(fx.runtime().library().known_tags().len(), 2);

    fx.send(RuntimeInput::TagList(TagListInput::Open));
    assert_eq!(fx.runtime().tag_list().page().items, ["alpha", "beta"]);
    fx.settle();
    assert_eq!(fx.runtime().tag_list().page().items, ["alpha", "beta"]);
}
