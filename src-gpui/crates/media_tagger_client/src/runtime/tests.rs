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
    // The query, the known tags, the scan, the roots and the preference save.
    assert_eq!(fx.executor.run_all(), 5);
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
