//! The query model against the scripted backend on the deterministic executor, and
//! once against the real core.

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use media_tagger_core::models::{
    AssetQueryPageResult, AssetQueryPositionResult, AssetQuerySection, AssetQuerySectionsResult,
    AssetSummary, StartAssetQueryResult,
};

use super::{
    AssetQuery, KeepAlive, QueryConfig, QueryError, QueryEvent, QueryInput, QueryRunner, Ticket,
    Viewport, ID_RANGE_CHUNK, MAX_QUERY_PAGE_SIZE, STALE_REFRESH_DELAY, STALE_REFRESH_MIN_INTERVAL,
};
use crate::{
    backend::{
        core_backend::test_support::TempCore, fake::Call, AssetIdRange, Backend, BackendError,
        FakeBackend, MediaKind,
    },
    exec::{Executor, ManualExecutor, ThreadExecutor},
    library::{AssetPatch, LibraryFilters, SummaryPatch, MAX_CACHED_PAGES},
    model::Driver,
};

const PAGE: usize = 2;

fn row(id: i64) -> AssetSummary {
    AssetSummary {
        id,
        file_name: format!("photo-{id}.jpg"),
        preview_path: None,
        kind: "image".into(),
        modified_at: 100 - id,
        width: Some(10),
        height: Some(10),
        duration_ms: None,
        thumb_version: "0123456789abcdef".into(),
        is_favorite: false,
        media_group_key: None,
        media_group_order: None,
    }
}

fn rows(ids: &[i64]) -> Vec<AssetSummary> {
    ids.iter().copied().map(row).collect()
}

fn start(session_id: u64, total: usize, offset: usize, ids: &[i64]) -> StartAssetQueryResult {
    StartAssetQueryResult::Ready {
        session_id,
        revision: 3,
        total,
        offset,
        items: rows(ids),
    }
}

fn page(session_id: u64, total: usize, offset: usize, ids: &[i64]) -> AssetQueryPageResult {
    AssetQueryPageResult::Ready {
        session_id,
        revision: 3,
        total,
        offset,
        items: rows(ids),
    }
}

fn viewport(start_index: usize, end_index: usize) -> Viewport {
    Viewport {
        start_index,
        end_index,
        visible_start_index: start_index,
        visible_end_index: end_index,
        paused: false,
    }
}

struct Fixture {
    fake: Arc<FakeBackend>,
    executor: Arc<ManualExecutor>,
    driver: Driver<AssetQuery, QueryRunner>,
    next_ticket: u64,
}

impl Fixture {
    fn new(page_size: usize) -> Self {
        Self::with_filters(page_size, LibraryFilters::default())
    }

    fn with_filters(page_size: usize, filters: LibraryFilters) -> Self {
        let fake = Arc::new(FakeBackend::new());
        // Sections are optional; a test that cares scripts them.
        fake.on_get_asset_query_sections(|session_id| {
            Ok(AssetQuerySectionsResult::Ready {
                session_id,
                total: 0,
                sections: Vec::new(),
            })
        });
        let executor = Arc::new(ManualExecutor::new());
        let driver = Driver::new(
            AssetQuery::with_filters(QueryConfig { page_size }, filters),
            QueryRunner::new(fake.clone()),
            executor.clone() as Arc<dyn Executor>,
        );
        Self {
            fake,
            executor,
            driver,
            next_ticket: 0,
        }
    }

    /// A result of `total` rows in session `session_id` whose row at index `i` has ID
    /// `i + 1`, served page by page.
    fn linear(&self, session_id: u64, total: usize) {
        let ids = move |offset: usize, limit: usize| -> Vec<i64> {
            (offset..total.min(offset + limit))
                .map(|index| index as i64 + 1)
                .collect()
        };
        self.fake.on_start_asset_query(move |request| {
            let offset = request.anchor_offset.unwrap_or(0) / request.page_size * request.page_size;
            Ok(start(
                session_id,
                total,
                offset,
                &ids(offset, request.page_size),
            ))
        });
        self.fake
            .on_get_asset_query_page(move |session, offset, limit| {
                Ok(page(session, total, offset, &ids(offset, limit)))
            });
    }

    fn query(&self) -> &AssetQuery {
        self.driver.model()
    }

    fn send(&mut self, input: QueryInput) {
        self.driver.handle(input);
    }

    /// Runs every waiting backend call and applies the replies; timers stay.
    fn settle(&mut self) {
        while self.executor.run_all() > 0 || self.driver.pump() > 0 {}
    }

    fn refresh(&mut self) {
        self.send(QueryInput::Refresh);
        self.settle();
    }

    fn advance(&mut self, by: Duration) {
        self.executor.advance(by);
        self.settle();
    }

    fn ticket(&mut self) -> Ticket {
        self.next_ticket += 1;
        Ticket(self.next_ticket)
    }

    fn events(&mut self) -> Vec<QueryEvent> {
        self.driver.model_mut().take_events()
    }

    fn ask(&mut self, index: usize) -> Ticket {
        let ticket = self.ticket();
        self.send(QueryInput::ReadAsset { ticket, index });
        ticket
    }

    /// Reads the row at `index` to the end and returns its ID.
    fn read(&mut self, index: usize) -> Result<Option<i64>, QueryError> {
        let ticket = self.ask(index);
        self.settle();
        self.asset_answer(ticket)
    }

    fn asset_answer(&mut self, ticket: Ticket) -> Result<Option<i64>, QueryError> {
        let mut answers: Vec<_> = self
            .events()
            .into_iter()
            .filter_map(|event| match event {
                QueryEvent::AssetRead { ticket: t, result } if t == ticket => {
                    Some(result.map(|row| row.map(|row| row.id)))
                }
                _ => None,
            })
            .collect();
        assert_eq!(answers.len(), 1, "one answer for {ticket:?}");
        answers.remove(0)
    }

    fn read_ids(&mut self, start: usize, end: usize) -> Result<Vec<i64>, QueryError> {
        let ticket = self.ticket();
        self.send(QueryInput::ReadIdRange { ticket, start, end });
        self.settle();
        match &self.events()[..] {
            [QueryEvent::IdRangeRead { ticket: t, result }] if *t == ticket => result.clone(),
            other => panic!("unexpected events {other:?}"),
        }
    }

    fn keep_alive(&mut self, session_id: u64) -> Result<KeepAlive, QueryError> {
        let ticket = self.ticket();
        self.send(QueryInput::KeepSessionAlive { ticket, session_id });
        self.settle();
        match &self.events()[..] {
            [QueryEvent::SessionKeptAlive { ticket: t, result }] if *t == ticket => result.clone(),
            other => panic!("unexpected events {other:?}"),
        }
    }

    fn position(&mut self, asset_id: i64) -> Result<AssetQueryPositionResult, QueryError> {
        let ticket = self.ticket();
        self.send(QueryInput::ResolvePosition { ticket, asset_id });
        self.settle();
        match &self.events()[..] {
            [QueryEvent::PositionResolved { ticket: t, result }] if *t == ticket => result.clone(),
            other => panic!("unexpected events {other:?}"),
        }
    }

    fn calls(&self, method: &str) -> usize {
        self.fake
            .called_methods()
            .into_iter()
            .filter(|called| *called == method)
            .count()
    }

    fn page_offsets(&self) -> Vec<usize> {
        self.fake
            .calls()
            .into_iter()
            .filter_map(|call| match call {
                Call::GetAssetQueryPage { offset, .. } => Some(offset),
                _ => None,
            })
            .collect()
    }

    fn cached_ids(&self) -> Vec<i64> {
        self.query().assets().map(|asset| asset.id).collect()
    }

    fn id_at(&self, index: usize) -> Option<i64> {
        self.query().asset_at(index).map(|asset| asset.id)
    }
}

#[test]
fn a_ready_first_page_of_the_current_generation_replaces_the_cache() {
    let mut fx = Fixture::new(PAGE);
    let mut answers = vec![start(8, 1, 0, &[9]), start(7, 2, 0, &[1, 2])];
    fx.fake
        .on_start_asset_query(move |_| Ok(answers.pop().unwrap()));
    assert_eq!(fx.query().query_epoch(), 0);

    fx.send(QueryInput::Refresh);
    assert!(fx.query().query_pending() && fx.query().loading());
    assert_eq!(
        fx.query().total(),
        0,
        "nothing is published before the reply"
    );
    fx.settle();

    assert_eq!(fx.query().total(), 2);
    assert_eq!(fx.query().load_error(), None);
    assert_eq!(fx.id_at(0), Some(1));
    assert_eq!(fx.query().session_id(), Some(7));
    assert!(!fx.query().query_pending() && !fx.query().loading());
    assert_eq!(fx.query().query_epoch(), 2);
    match &fx.fake.calls()[0] {
        Call::StartAssetQuery { request } => {
            assert_eq!((request.generation, request.page_size), (1, PAGE));
            assert_eq!(request.owner_id, fx.query().owner_id());
            assert_eq!(request.anchor_offset, None);
        }
        other => panic!("unexpected call {other:?}"),
    }

    fx.refresh();
    assert_eq!(fx.cached_ids(), [9]);
    assert_eq!(fx.query().total(), 1);
}

#[test]
fn summaries_are_stored_as_they_came() {
    let mut fx = Fixture::new(PAGE);
    let video = AssetSummary {
        file_name: "clip.mp4".into(),
        preview_path: Some("/synthetic/library/clip.mp4".into()),
        kind: "video".into(),
        duration_ms: Some(10_000),
        ..row(7)
    };
    let sent = video.clone();
    fx.fake.on_start_asset_query(move |_| {
        Ok(StartAssetQueryResult::Ready {
            session_id: 1,
            revision: 1,
            total: 1,
            offset: 0,
            items: vec![sent.clone()],
        })
    });

    fx.refresh();

    assert_eq!(fx.query().asset_at(0), Some(&video));
}

#[test]
fn filters_reach_the_request_and_applying_them_always_refreshes() {
    let filters = LibraryFilters {
        include: vec!["cat".into()],
        exclude: vec!["dog".into()],
        media_kind: Some(MediaKind::Image),
        favorites_only: true,
        meta_filter: None,
    };
    let mut fx = Fixture::with_filters(PAGE, filters.clone());
    fx.linear(1, 5);

    fx.refresh();
    fx.send(QueryInput::ApplyFilters(filters.clone()));
    fx.settle();

    let requests: Vec<_> = fx
        .fake
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            Call::StartAssetQuery { request } => Some(request),
            _ => None,
        })
        .collect();
    assert_eq!(requests.len(), 2, "an equal descriptor refreshes too");
    assert_eq!(requests[0].tags_and, ["cat"]);
    assert_eq!(requests[0].tags_not, ["dog"]);
    assert_eq!(requests[0].kind, Some(MediaKind::Image));
    assert!(requests[0].favorites_only);
    assert!(requests[0].generation < requests[1].generation);
    assert_eq!(fx.query().filters(), &filters);
    assert_eq!(fx.cached_ids(), [1, 2]);
    assert_eq!(fx.query().total(), 5);
}

#[test]
fn the_newest_refresh_wins_whichever_reply_comes_first() {
    for newest_first in [true, false] {
        let mut fx = Fixture::new(PAGE);
        fx.fake.on_start_asset_query(|request| {
            Ok(if request.tags_and == ["new"] {
                start(2, 1, 0, &[9])
            } else {
                StartAssetQueryResult::Superseded
            })
        });
        fx.send(QueryInput::ApplyFilters(LibraryFilters {
            include: vec!["old".into()],
            ..LibraryFilters::default()
        }));
        fx.send(QueryInput::ApplyFilters(LibraryFilters {
            include: vec!["new".into()],
            ..LibraryFilters::default()
        }));
        assert_eq!(fx.executor.pending(), 2);

        // Task 0 is the old start, task 1 the new one.
        assert!(fx.executor.run_nth(usize::from(newest_first)));
        fx.driver.pump();
        assert!(fx.executor.run_nth(0));
        fx.settle();

        assert_eq!(fx.cached_ids(), [9], "newest first: {newest_first}");
        assert_eq!(fx.query().load_error(), None);
        assert_eq!(fx.query().asset_index(9), Some(0));
        assert!(!fx.query().query_pending());
    }
}

#[test]
fn the_page_size_stays_inside_what_the_backend_serves() {
    assert_eq!(AssetQuery::default().page_size(), 128);
    assert_eq!(AssetQuery::new(QueryConfig { page_size: 0 }).page_size(), 1);
    assert_eq!(
        AssetQuery::new(QueryConfig { page_size: 4096 }).page_size(),
        MAX_QUERY_PAGE_SIZE
    );
}

#[test]
fn every_query_model_has_its_own_owner_and_its_own_generations() {
    let first = AssetQuery::default();
    let second = AssetQuery::default();
    assert_ne!(first.owner_id(), second.owner_id());
    assert!((1..=128).contains(&first.owner_id().len()));

    let mut fx = Fixture::new(PAGE);
    fx.linear(1, 0);
    fx.refresh();
    fx.refresh();
    let mut replacement = Fixture::new(PAGE);
    replacement.linear(1, 0);
    replacement.refresh();
    match &replacement.fake.calls()[0] {
        Call::StartAssetQuery { request } => {
            assert_eq!(request.generation, 1);
            assert_ne!(request.owner_id, fx.query().owner_id());
        }
        other => panic!("unexpected call {other:?}"),
    }
}

#[test]
fn a_failed_start_is_a_load_error_without_a_session_and_retry_recovers() {
    let mut fx = Fixture::new(PAGE);
    let mut answers = vec![
        Ok(start(9, 3, 0, &[1, 2])),
        Err(BackendError::rejected("database is locked")),
    ];
    fx.fake
        .on_start_asset_query(move |_| answers.pop().unwrap());

    fx.refresh();

    assert_eq!(
        fx.query().load_error().map(ToString::to_string).as_deref(),
        Some("database is locked")
    );
    assert_eq!(fx.query().session_id(), None);
    assert_eq!(fx.query().page_failure_epoch(), 1);
    assert!(!fx.query().query_pending() && !fx.query().loading());
    // Nothing retries on its own, and a position lookup has no session to ask.
    assert_eq!(fx.position(201), Err(QueryError::Unavailable));
    assert_eq!(fx.calls("start_asset_query"), 1);
    assert_eq!(fx.calls("get_asset_query_position"), 0);

    fx.send(QueryInput::RetryLoad);
    fx.settle();

    assert_eq!(fx.query().load_error(), None);
    assert_eq!(fx.query().total(), 3);
    assert_eq!(fx.id_at(0), Some(1));
}

#[test]
fn a_current_supersession_is_a_retryable_error_and_keeps_the_rows_on_screen() {
    let mut fx = Fixture::new(PAGE);
    let mut answers = vec![
        start(8, 1, 0, &[9]),
        StartAssetQueryResult::Superseded,
        start(7, 1, 0, &[1]),
    ];
    fx.fake
        .on_start_asset_query(move |_| Ok(answers.pop().unwrap()));
    fx.refresh();
    let epoch = fx.query().query_epoch();

    fx.refresh();

    assert_eq!(fx.query().total(), 1);
    assert_eq!(fx.id_at(0), Some(1), "the previous cache stays visible");
    assert!(!fx.query().loading() && !fx.query().query_pending());
    assert_eq!(fx.query().load_error(), Some(&QueryError::Superseded));
    // The epoch moved when the refresh started: positions are no longer trusted.
    assert_eq!(fx.query().query_epoch(), epoch + 1);
    assert_eq!(fx.query().asset_index(1), None);

    fx.send(QueryInput::RetryLoad);
    fx.settle();
    assert_eq!(fx.query().load_error(), None);
    assert_eq!(fx.id_at(0), Some(9));
}

#[test]
fn a_failed_page_stays_failed_for_demand_until_retry() {
    let mut fx = Fixture::new(PAGE);
    fx.linear(5, 6);
    fx.refresh();
    fx.fake
        .on_get_asset_query_page(|_, _, _| Err(BackendError::rejected("offline")));

    assert_eq!(
        fx.read(2),
        Err(QueryError::Backend(BackendError::rejected("offline")))
    );
    assert_eq!(
        fx.query().load_error().map(ToString::to_string).as_deref(),
        Some("offline")
    );
    assert_eq!(fx.query().page_failure_epoch(), 1);
    assert_eq!(
        fx.query().session_id(),
        Some(5),
        "a page failure keeps the session"
    );
    assert_eq!(fx.id_at(2), None);

    // Demand and direct reads do not ask again.
    for _ in 0..6 {
        fx.send(QueryInput::SetViewport(Some(viewport(2, 3))));
        fx.settle();
    }
    assert!(fx.read(2).is_err());
    assert_eq!(fx.calls("get_asset_query_page"), 1);

    // Retry asks for the failed page of the demanded range, in the same session.
    fx.send(QueryInput::RetryLoad);
    assert_eq!(fx.query().load_error(), None);
    fx.settle();
    assert_eq!(fx.calls("get_asset_query_page"), 2);
    assert_eq!(fx.calls("start_asset_query"), 1);
    assert!(fx.query().load_error().is_some(), "it failed again");

    fx.linear(5, 6);
    fx.send(QueryInput::RetryLoad);
    fx.settle();
    assert_eq!(fx.query().load_error(), None);
    assert_eq!(fx.id_at(2), Some(3));
}

#[test]
fn retrying_one_asset_unblocks_its_page_only() {
    let mut fx = Fixture::new(PAGE);
    fx.linear(7, 6);
    fx.refresh();
    fx.fake
        .on_get_asset_query_page(|_, _, _| Err(BackendError::rejected("offline")));
    assert!(fx.read(2).is_err());
    assert!(fx.read(4).is_err());

    fx.linear(7, 6);
    let ticket = fx.ticket();
    fx.send(QueryInput::RetryAsset { ticket, index: 4 });
    fx.settle();

    assert_eq!(fx.asset_answer(ticket), Ok(Some(5)));
    assert!(
        fx.query().load_error().is_some(),
        "the other page is still failed"
    );
    assert!(fx.read(2).is_err());
    assert_eq!(fx.page_offsets(), [2, 4, 4]);
    assert_eq!(fx.calls("start_asset_query"), 1);
}

#[test]
fn readers_of_one_missing_page_share_one_request() {
    let mut fx = Fixture::new(PAGE);
    fx.linear(1, 4);
    fx.refresh();

    let first = fx.ask(2);
    let second = fx.ask(3);
    fx.send(QueryInput::SetViewport(Some(viewport(2, 3))));
    fx.send(QueryInput::SetViewport(Some(viewport(2, 3))));
    assert_eq!(fx.executor.pending(), 1);
    assert!(fx.query().loading());
    fx.settle();

    assert_eq!(fx.calls("get_asset_query_page"), 1);
    assert_eq!(
        fx.events(),
        [
            QueryEvent::AssetRead {
                ticket: first,
                result: Ok(Some(row(3)))
            },
            QueryEvent::AssetRead {
                ticket: second,
                result: Ok(Some(row(4)))
            },
        ]
    );
    assert_eq!(fx.cached_ids(), [1, 2, 3, 4]);
    assert!(!fx.query().loading());

    // The page is cached: demand asks for nothing more.
    fx.send(QueryInput::SetViewport(Some(viewport(2, 3))));
    assert_eq!(fx.executor.pending(), 0);
}

#[test]
fn a_page_reply_of_the_previous_generation_never_merges() {
    let mut fx = Fixture::new(1);
    fx.linear(1, 30);
    fx.refresh();

    // The old page request waits; a refresh starts a new generation and its own
    // request for the same offset.
    let old = fx.ask(1);
    assert_eq!(fx.executor.pending(), 1);
    fx.fake
        .on_start_asset_query(|_| Ok(start(2, 30, 0, &[100])));
    fx.send(QueryInput::Refresh);
    assert_eq!(
        fx.asset_answer(old),
        Ok(None),
        "answered when it became obsolete"
    );
    assert!(fx.executor.run_nth(1), "the new start");
    fx.driver.pump();
    assert_eq!(fx.query().session_id(), Some(2));
    fx.executor.run_nth(1); // the sections read of session 2
    fx.driver.pump();
    let current = fx.ask(1);
    assert_eq!(fx.executor.pending(), 2, "the old page and the new one");

    // The old reply arrives first: it is dropped and the new request stays.
    assert!(fx.executor.run_nth(0));
    fx.driver.pump();
    assert_eq!(fx.id_at(1), None);
    let joined = fx.ask(1);
    assert_eq!(
        fx.executor.pending(),
        1,
        "the replacement request is still out"
    );
    assert!(fx.query().loading());

    fx.fake
        .on_get_asset_query_page(|session, offset, _| Ok(page(session, 30, offset, &[200])));
    fx.settle();
    let events = fx.events();
    assert_eq!(events.len(), 2);
    for (event, ticket) in events.iter().zip([current, joined]) {
        match event {
            QueryEvent::AssetRead { ticket: t, result } => {
                assert_eq!(*t, ticket);
                assert_eq!(
                    result.as_ref().unwrap().as_ref().map(|row| row.id),
                    Some(200)
                );
            }
            other => panic!("unexpected event {other:?}"),
        }
    }
    assert_eq!(fx.id_at(1), Some(200));
    assert_eq!(
        fx.page_offsets(),
        [1, 1],
        "one request per generation for the offset"
    );
}

#[test]
fn replies_that_arrive_in_another_order_land_at_their_own_offsets() {
    let mut fx = Fixture::new(PAGE);
    fx.linear(1, 10);
    fx.refresh();
    let a = fx.ask(2);
    let b = fx.ask(4);
    let c = fx.ask(6);
    assert_eq!(fx.executor.pending(), 3);

    assert!(fx.executor.run_nth(2));
    assert!(fx.executor.run_nth(0));
    assert!(fx.executor.run_nth(0));
    fx.driver.pump();

    let answered: Vec<_> = fx
        .events()
        .into_iter()
        .map(|event| match event {
            QueryEvent::AssetRead { ticket, result } => (ticket, result.unwrap().map(|row| row.id)),
            other => panic!("unexpected event {other:?}"),
        })
        .collect();
    assert_eq!(answered, [(c, Some(7)), (a, Some(3)), (b, Some(5))]);
    assert_eq!(fx.id_at(2), Some(3));
    assert_eq!(fx.id_at(4), Some(5));
    assert_eq!(fx.id_at(6), Some(7));
    assert_eq!(fx.query().asset_index(5), Some(4));
    assert_eq!(fx.cached_ids(), [1, 2, 3, 4, 5, 6, 7, 8]);
}

#[test]
fn a_stale_page_during_other_page_loads_schedules_one_refresh_and_the_rest_still_merge() {
    let mut fx = Fixture::new(PAGE);
    fx.linear(5, 10);
    fx.refresh();
    fx.fake.on_get_asset_query_page(|session, offset, _| {
        Ok(if offset == 2 {
            AssetQueryPageResult::Stale
        } else {
            page(session, 10, offset, &[offset as i64 + 1, offset as i64 + 2])
        })
    });
    let stale = fx.ask(2);
    let fine = fx.ask(4);
    assert_eq!(fx.executor.pending(), 2);

    // The stale reply comes while the other page is still out.
    assert!(fx.executor.run_nth(0));
    fx.driver.pump();
    assert_eq!(fx.asset_answer(stale), Ok(None), "a stale page is a hole");
    assert_eq!(fx.executor.pending_timers(), 1);
    assert!(fx.query().loading());
    assert_eq!(
        fx.query().session_id(),
        Some(5),
        "the session stays until the refresh"
    );

    // The other page of the same generation still merges.
    fx.settle();
    assert_eq!(fx.asset_answer(fine), Ok(Some(5)));
    assert_eq!(fx.id_at(4), Some(5));
    assert_eq!(fx.calls("start_asset_query"), 1);

    // A page requested before the refresh and answered after it is dropped.
    fx.fake
        .on_get_asset_query_page(|session, offset, _| Ok(page(session, 10, offset, &[77, 78])));
    let late = fx.ask(6);
    fx.fake.on_start_asset_query(|_| Ok(start(6, 1, 0, &[9])));
    fx.executor.advance(STALE_REFRESH_DELAY);
    assert_eq!(fx.executor.pending(), 2, "the page and the timer");
    assert!(fx.executor.run_nth(1));
    fx.driver.pump();
    assert_eq!(fx.asset_answer(late), Ok(None));
    fx.settle();

    assert_eq!(fx.calls("start_asset_query"), 2);
    assert_eq!(fx.query().session_id(), Some(6));
    assert_eq!(fx.query().total(), 1);
    assert_eq!(fx.cached_ids(), [9]);
    assert_eq!(fx.id_at(6), None);
}

#[test]
fn stale_refreshes_are_debounced_spaced_and_cancelled_by_an_explicit_refresh() {
    let mut fx = Fixture::new(PAGE);
    fx.fake
        .on_start_asset_query(|_| Ok(start(5, 8, 0, &[1])))
        .on_get_asset_query_page(|_, _, _| Ok(AssetQueryPageResult::Stale));
    fx.refresh();

    // A burst of stale reads joins one refresh.
    for index in [2, 4, 6] {
        assert_eq!(fx.read(index), Ok(None));
    }
    assert_eq!(fx.calls("start_asset_query"), 1);
    assert_eq!(fx.executor.pending_timers(), 1);
    fx.advance(STALE_REFRESH_DELAY - Duration::from_millis(1));
    assert_eq!(fx.calls("start_asset_query"), 1);
    fx.advance(Duration::from_millis(1));
    assert_eq!(fx.calls("start_asset_query"), 2);

    // A second burst right after a stale refresh waits for the minimum interval.
    assert_eq!(fx.read(2), Ok(None));
    fx.advance(STALE_REFRESH_DELAY);
    assert_eq!(fx.calls("start_asset_query"), 2);
    fx.advance(STALE_REFRESH_MIN_INTERVAL - STALE_REFRESH_DELAY - Duration::from_millis(1));
    assert_eq!(fx.calls("start_asset_query"), 2);
    fx.advance(Duration::from_millis(1));
    assert_eq!(fx.calls("start_asset_query"), 3);

    // An explicit refresh runs at once and cancels a scheduled stale refresh.
    fx.advance(STALE_REFRESH_MIN_INTERVAL);
    assert_eq!(fx.read(2), Ok(None));
    fx.refresh();
    assert_eq!(fx.calls("start_asset_query"), 4);
    fx.advance(STALE_REFRESH_MIN_INTERVAL * 2);
    assert_eq!(fx.calls("start_asset_query"), 4);

    // Long after the last stale refresh only the debounce delay applies again.
    assert_eq!(fx.read(2), Ok(None));
    fx.advance(STALE_REFRESH_DELAY);
    assert_eq!(fx.calls("start_asset_query"), 5);
}

#[test]
fn a_stale_refresh_cancelled_inside_the_interval_is_rescheduled_from_the_new_stale_read() {
    let mut fx = Fixture::new(PAGE);
    fx.fake
        .on_start_asset_query(|_| Ok(start(5, 8, 0, &[1])))
        .on_get_asset_query_page(|_, _, _| Ok(AssetQueryPageResult::Stale));
    fx.refresh();
    assert_eq!(fx.read(2), Ok(None));
    fx.advance(STALE_REFRESH_DELAY); // stale refresh at 250 ms; interval until 1250 ms
    assert_eq!(fx.calls("start_asset_query"), 2);

    assert_eq!(fx.read(2), Ok(None)); // scheduled
    fx.advance(Duration::from_millis(100));
    fx.refresh(); // explicit, at 350 ms: cancels it
    assert_eq!(fx.calls("start_asset_query"), 3);
    assert_eq!(fx.read(2), Ok(None)); // scheduled again at 350 ms
    fx.advance(Duration::from_millis(250)); // 600 ms: both debounce timers have fired
    assert_eq!(fx.calls("start_asset_query"), 3);
    fx.advance(Duration::from_millis(649)); // 1249 ms
    assert_eq!(fx.calls("start_asset_query"), 3);
    fx.advance(Duration::from_millis(1)); // 1250 ms: the interval has passed
    assert_eq!(fx.calls("start_asset_query"), 4);
}

#[test]
fn the_cache_keeps_at_most_24_pages_and_an_oversized_range_does_not_reload() {
    let mut fx = Fixture::new(1);
    fx.linear(1, 30);
    fx.refresh();

    fx.send(QueryInput::SetViewport(Some(viewport(0, 29))));
    fx.settle();

    assert_eq!(fx.query().cached_len(), MAX_CACHED_PAGES);
    assert_eq!(fx.query().cached_pages(), MAX_CACHED_PAGES);
    assert_eq!(fx.calls("get_asset_query_page"), 29);
    assert!(!fx.query().loading());
    // Page arrivals do not start demand again.
    fx.settle();
    assert_eq!(fx.calls("get_asset_query_page"), 29);

    // An evicted row is read again on request, and the bound holds.
    let missing = (0..30).find(|index| fx.id_at(*index).is_none()).unwrap();
    assert_eq!(fx.read(missing), Ok(Some(missing as i64 + 1)));
    assert_eq!(fx.query().cached_len(), MAX_CACHED_PAGES);
    assert_eq!(fx.query().total(), 30, "total is the result, not the cache");
}

#[test]
fn read_pages_and_the_demanded_range_survive_eviction() {
    let cached_rows = MAX_CACHED_PAGES * PAGE;
    let total = cached_rows + 8 * PAGE;
    let mut fx = Fixture::new(PAGE);
    fx.linear(7, total);
    fx.refresh();
    for index in (2..cached_rows).step_by(2) {
        fx.read(index).unwrap();
    }
    assert_eq!(fx.query().cached_len(), cached_rows);

    // An explicit read promotes the first page; a render-time lookup does not.
    fx.read(0).unwrap();
    fx.read(cached_rows).unwrap();
    assert_eq!(fx.id_at(0), Some(1));
    assert_eq!(fx.id_at(2), None);

    fx.send(QueryInput::SetViewport(Some(viewport(0, 1))));
    for index in (4..total).step_by(2) {
        fx.read(index).unwrap();
        assert!(fx.query().cached_len() <= cached_rows);
    }
    assert_eq!(fx.id_at(0), Some(1), "the demanded page is protected");
    assert_eq!(fx.read(2), Ok(Some(3)));
    assert_eq!(fx.query().cached_len(), cached_rows);
}

#[test]
fn old_rows_of_the_viewport_stay_as_render_only_fallback_until_their_page_reloads() {
    let mut fx = Fixture::new(PAGE);
    fx.linear(5, 6);
    fx.refresh();
    fx.send(QueryInput::SetViewport(Some(viewport(2, 3))));
    fx.settle();
    assert_eq!(fx.id_at(2), Some(3));

    fx.fake
        .on_start_asset_query(|_| Ok(start(6, 6, 0, &[11, 12])));
    fx.send(QueryInput::Refresh);
    assert!(fx.executor.run_next());
    fx.driver.pump();

    // The new first page replaced page 0; the viewport's page is still on screen
    // and its request under the new session is out.
    assert_eq!(fx.id_at(0), Some(11));
    assert_eq!(fx.id_at(2), Some(3));
    assert_eq!(fx.query().asset_index(3), None);
    assert_eq!(fx.cached_ids(), [11, 12]);
    let ticket = fx.ask(2);
    fx.fake
        .on_get_asset_query_page(|session, offset, _| Ok(page(session, 6, offset, &[13, 14])));
    fx.settle();

    assert_eq!(
        fx.asset_answer(ticket),
        Ok(Some(13)),
        "reads never see fallback rows"
    );
    assert_eq!(fx.id_at(2), Some(13));
    assert_eq!(
        fx.fake.calls().last(),
        Some(&Call::GetAssetQueryPage {
            session_id: 6,
            offset: 2,
            limit: PAGE
        })
    );
}

#[test]
fn fallback_rows_go_when_the_viewport_leaves_them() {
    let mut fx = Fixture::new(PAGE);
    fx.linear(5, 6);
    fx.refresh();
    fx.send(QueryInput::SetViewport(Some(viewport(2, 3))));
    fx.settle();
    fx.fake
        .on_start_asset_query(|_| Ok(start(6, 6, 0, &[11, 12])));
    fx.send(QueryInput::Refresh);
    assert!(fx.executor.run_next());
    fx.driver.pump();
    assert_eq!(fx.id_at(2), Some(3));
    let version = fx.query().version();

    fx.send(QueryInput::SetViewport(Some(viewport(0, 1))));

    assert_eq!(fx.id_at(2), None);
    assert!(fx.query().version() > version);
}

#[test]
fn a_refresh_is_anchored_at_the_first_visible_index() {
    let mut fx = Fixture::new(PAGE);
    fx.linear(5, 40);
    fx.refresh();
    fx.send(QueryInput::SetViewport(Some(Viewport {
        start_index: 30,
        end_index: 31,
        visible_start_index: 31,
        visible_end_index: 31,
        paused: false,
    })));
    fx.settle();
    fx.fake.take_calls();

    fx.refresh();

    match &fx.fake.calls()[0] {
        Call::StartAssetQuery { request } => assert_eq!(request.anchor_offset, Some(31)),
        other => panic!("unexpected call {other:?}"),
    }
    assert_eq!(fx.id_at(30), Some(31));
    assert_eq!(fx.id_at(0), None);
    assert_eq!(fx.query().asset_index(32), Some(31));
    assert_eq!(
        fx.calls("get_asset_query_page"),
        0,
        "one call carried the page"
    );
}

#[test]
fn a_paused_viewport_holds_demand_and_resume_loads_the_latest_range_only() {
    let mut fx = Fixture::new(PAGE);
    fx.linear(1, 100);
    fx.refresh();
    fx.send(QueryInput::SetViewport(Some(viewport(2, 3))));
    assert_eq!(fx.executor.pending(), 1);
    let paused = |start: usize| Viewport {
        paused: true,
        ..viewport(start, start + 1)
    };
    fx.send(QueryInput::SetViewport(Some(paused(20))));
    fx.send(QueryInput::SetViewport(Some(paused(40))));
    fx.settle();
    assert_eq!(fx.id_at(2), Some(3), "a request already out still lands");
    assert_eq!(fx.page_offsets(), [2]);

    // A refresh during the pause asks for the page of the latest range first and
    // demands nothing else.
    fx.send(QueryInput::ApplyFilters(LibraryFilters {
        include: vec!["new".into()],
        ..LibraryFilters::default()
    }));
    fx.settle();
    match fx.fake.calls().iter().rev().find_map(|call| match call {
        Call::StartAssetQuery { request } => Some(request.anchor_offset),
        _ => None,
    }) {
        Some(anchor) => assert_eq!(anchor, Some(40)),
        None => panic!("no start"),
    }
    assert_eq!(fx.page_offsets(), [2]);

    // Explicit reads still work during the pause.
    assert_eq!(fx.read(60), Ok(Some(61)));
    fx.fake.on_get_asset_query_id_range(|_, start, end| {
        Ok(AssetIdRange::Ready(
            (start..=end).map(|index| index as i64 + 1).collect(),
        ))
    });
    assert_eq!(fx.read_ids(62, 63), Ok(vec![63, 64]));
    assert_eq!(fx.page_offsets(), [2, 60]);

    fx.send(QueryInput::SetViewport(Some(viewport(42, 43))));
    fx.settle();
    assert_eq!(fx.id_at(42), Some(43));
    assert_eq!(fx.page_offsets(), [2, 60, 42]);

    // The gallery goes away: the cache stays.
    fx.send(QueryInput::SetViewport(None));
    assert_eq!(fx.id_at(42), Some(43));
    assert_eq!(fx.executor.pending(), 0);
}

#[test]
fn an_id_range_comes_from_the_session_vector_in_one_call() {
    let mut fx = Fixture::new(PAGE);
    fx.linear(5, 4);
    fx.refresh();
    fx.fake
        .on_get_asset_query_id_range(|_, _, _| Ok(AssetIdRange::Ready(vec![2, 3, 4])));

    // Reversed and over-long bounds are normalized to the snapshot.
    assert_eq!(fx.read_ids(9, 1), Ok(vec![2, 3, 4]));

    assert_eq!(
        fx.fake.calls().last(),
        Some(&Call::GetAssetQueryIdRange {
            session_id: 5,
            start: 1,
            end: 3
        })
    );
    assert_eq!(fx.calls("get_asset_query_id_range"), 1);
    assert_eq!(fx.calls("get_asset_query_page"), 0);
    assert_eq!(fx.id_at(2), None);
    assert_eq!(fx.cached_ids(), [1, 2]);

    // Bounds outside the snapshot need no call.
    assert_eq!(fx.read_ids(4, 8), Ok(Vec::new()));
    assert_eq!(fx.calls("get_asset_query_id_range"), 1);
}

#[test]
fn an_id_range_is_whole_or_an_error() {
    let mut fx = Fixture::new(PAGE);
    fx.linear(5, 4);
    fx.refresh();
    let mut answers = vec![
        Ok(AssetIdRange::Stale),
        Ok(AssetIdRange::Ready(vec![2])),
        Err(BackendError::rejected("range worker failed")),
    ];
    fx.fake
        .on_get_asset_query_id_range(move |_, _, _| answers.pop().unwrap());

    assert_eq!(
        fx.read_ids(1, 2),
        Err(QueryError::Backend(BackendError::rejected(
            "range worker failed"
        )))
    );
    assert_eq!(fx.read_ids(1, 2), Err(QueryError::IncompleteRange));
    // A range failure belongs to its caller, not to the gallery's page state.
    assert_eq!(fx.query().load_error(), None);
    assert_eq!(fx.executor.pending_timers(), 0);

    // A stale session cancels the range and schedules the refresh.
    assert_eq!(fx.read_ids(1, 2), Err(QueryError::Cancelled));
    fx.advance(STALE_REFRESH_DELAY);
    assert_eq!(fx.calls("start_asset_query"), 2);
}

#[test]
fn a_huge_id_range_is_read_in_chunks_and_assembled_in_order() {
    let total = ID_RANGE_CHUNK + 5;
    let mut fx = Fixture::new(PAGE);
    fx.fake
        .on_start_asset_query(move |_| Ok(start(5, total, 0, &[1])))
        .on_get_asset_query_id_range(|_, start, end| {
            Ok(AssetIdRange::Ready(
                (start..=end).map(|index| index as i64 + 1).collect(),
            ))
        });
    fx.refresh();

    let ids = fx.read_ids(0, total - 1).unwrap();

    let chunks: Vec<_> = fx
        .fake
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            Call::GetAssetQueryIdRange { start, end, .. } => Some((start, end)),
            _ => None,
        })
        .collect();
    assert_eq!(
        chunks,
        [(0, ID_RANGE_CHUNK - 1), (ID_RANGE_CHUNK, total - 1)]
    );
    assert_eq!(ids.len(), total);
    assert_eq!(
        [ids[0], ids[ID_RANGE_CHUNK], ids[total - 1]],
        [1, ID_RANGE_CHUNK as i64 + 1, total as i64]
    );
    assert_eq!(fx.query().cached_len(), 1, "a range read caches no rows");
}

#[test]
fn a_range_interrupted_by_refresh_reset_or_disposal_is_cancelled_whole() {
    for interruption in ["refresh", "reset", "dispose"] {
        let mut fx = Fixture::new(PAGE);
        fx.linear(1, 6);
        fx.refresh();
        fx.fake.on_get_asset_query_id_range(|_, start, end| {
            Ok(AssetIdRange::Ready(
                (start..=end).map(|index| index as i64 + 1).collect(),
            ))
        });
        let ticket = fx.ticket();
        fx.send(QueryInput::ReadIdRange {
            ticket,
            start: 0,
            end: 5,
        });
        assert_eq!(fx.executor.pending(), 1);

        fx.send(match interruption {
            "refresh" => QueryInput::Refresh,
            "reset" => QueryInput::Reset,
            _ => QueryInput::Dispose,
        });
        fx.settle();

        assert_eq!(
            fx.events(),
            [QueryEvent::IdRangeRead {
                ticket,
                result: Err(QueryError::Cancelled)
            }],
            "{interruption}"
        );
        assert_eq!(fx.calls("get_asset_query_id_range"), 1, "{interruption}");
    }
}

#[test]
fn keepalive_reads_one_id_of_the_current_session_and_a_stale_one_refreshes() {
    let mut fx = Fixture::new(PAGE);
    let mut starts = vec![start(6, 1, 0, &[9]), start(5, 4, 0, &[1, 2])];
    fx.fake
        .on_start_asset_query(move |_| Ok(starts.pop().unwrap()));
    let mut ranges = vec![AssetIdRange::Stale, AssetIdRange::Ready(vec![1])];
    fx.fake
        .on_get_asset_query_id_range(move |_, _, _| Ok(ranges.pop().unwrap()));
    fx.refresh();

    assert_eq!(fx.keep_alive(5), Ok(KeepAlive::Ready));
    assert_eq!(
        fx.fake.calls().last(),
        Some(&Call::GetAssetQueryIdRange {
            session_id: 5,
            start: 0,
            end: 0
        })
    );
    // A session that is not the current one is left to expire.
    assert_eq!(fx.keep_alive(4), Ok(KeepAlive::Skipped));
    assert_eq!(fx.calls("get_asset_query_id_range"), 1);
    fx.advance(STALE_REFRESH_DELAY * 2);
    assert_eq!(fx.calls("start_asset_query"), 1);
    assert_eq!(fx.query().load_error(), None);

    assert_eq!(fx.keep_alive(5), Ok(KeepAlive::Stale));
    fx.advance(STALE_REFRESH_DELAY);
    assert_eq!(fx.calls("start_asset_query"), 2);
    assert_eq!(fx.query().session_id(), Some(6));
}

#[test]
fn keepalive_skips_an_empty_result_and_reports_its_failure_to_the_caller_only() {
    let mut fx = Fixture::new(PAGE);
    fx.linear(3, 0);
    fx.refresh();
    assert_eq!(fx.keep_alive(3), Ok(KeepAlive::Skipped));
    assert_eq!(fx.calls("get_asset_query_id_range"), 0);

    fx.linear(4, 2);
    fx.refresh();
    fx.fake
        .on_get_asset_query_id_range(|_, _, _| Err(BackendError::rejected("busy")));
    assert_eq!(
        fx.keep_alive(4),
        Err(QueryError::Backend(BackendError::rejected("busy")))
    );
    assert_eq!(fx.query().load_error(), None);
}

#[test]
fn sections_are_read_once_per_session_and_dropped_for_a_new_one() {
    let sections = vec![
        AssetQuerySection {
            t: 202403,
            start: 0,
            count: 1,
        },
        AssetQuerySection {
            t: 0,
            start: 1,
            count: 1,
        },
    ];
    let mut fx = Fixture::new(PAGE);
    let mut starts = vec![
        start(8, 1, 0, &[9]),
        start(7, 2, 0, &[1, 2]),
        start(7, 2, 0, &[1, 2]),
    ];
    fx.fake
        .on_start_asset_query(move |_| Ok(starts.pop().unwrap()));
    let sent = sections.clone();
    fx.fake.on_get_asset_query_sections(move |session_id| {
        Ok(if session_id == 7 {
            AssetQuerySectionsResult::Ready {
                session_id,
                total: 2,
                sections: sent.clone(),
            }
        } else {
            AssetQuerySectionsResult::Stale
        })
    });

    fx.refresh();
    assert_eq!(fx.query().sections(), sections);
    // The same session (a cache hit in the backend) keeps its sections.
    fx.refresh();
    assert_eq!(fx.calls("get_asset_query_sections"), 1);
    assert_eq!(fx.query().sections(), sections);
    assert_eq!(fx.query().session_id(), Some(7));

    // A new session drops them; a stale read leaves the timeline empty.
    fx.refresh();
    assert_eq!(fx.calls("get_asset_query_sections"), 2);
    assert_eq!(fx.query().sections(), []);
    assert_eq!(fx.query().session_id(), Some(8));
    assert_eq!(fx.executor.pending_timers(), 1, "and schedules the refresh");
}

#[test]
fn ready_sections_without_content_keep_the_session_and_hide_the_timeline() {
    // A scan batch committed without a revision bump (ADR 0027): the backend keeps
    // the session and answers `Ready` with no sections.
    let mut fx = Fixture::new(PAGE);
    fx.linear(7, 4);
    fx.fake.on_get_asset_query_sections(|session_id| {
        Ok(AssetQuerySectionsResult::Ready {
            session_id,
            total: 4,
            sections: Vec::new(),
        })
    });

    fx.refresh();
    fx.advance(STALE_REFRESH_DELAY + STALE_REFRESH_MIN_INTERVAL);

    assert_eq!(fx.calls("get_asset_query_sections"), 1);
    assert_eq!(fx.calls("start_asset_query"), 1);
    assert_eq!(fx.query().sections(), []);
    assert_eq!(fx.query().session_id(), Some(7));
    assert_eq!(fx.read(3), Ok(Some(4)));
}

#[test]
fn sections_of_another_total_or_a_failed_read_leave_the_timeline_hidden() {
    let section = AssetQuerySection {
        t: 202401,
        start: 0,
        count: 3,
    };
    let mut fx = Fixture::new(PAGE);
    fx.linear(7, 4);
    fx.fake.on_get_asset_query_sections(move |session_id| {
        Ok(AssetQuerySectionsResult::Ready {
            session_id,
            total: 3,
            sections: vec![section],
        })
    });
    fx.refresh();
    assert_eq!(fx.query().sections(), []);

    let mut fx = Fixture::new(PAGE);
    fx.linear(7, 4);
    fx.fake
        .on_get_asset_query_sections(|_| Err(BackendError::rejected("sections failed")));
    fx.refresh();
    assert_eq!(fx.query().sections(), []);
    assert_eq!(fx.query().load_error(), None);
    assert_eq!(fx.query().session_id(), Some(7));
}

#[test]
fn positions_come_from_the_session_and_a_stale_one_refreshes() {
    let mut fx = Fixture::new(128);
    fx.fake.on_start_asset_query(|_| Ok(start(1, 400, 0, &[1])));
    let mut answers = vec![
        AssetQueryPositionResult::Stale,
        AssetQueryPositionResult::Missing,
        AssetQueryPositionResult::Resolved { index: 200 },
    ];
    fx.fake
        .on_get_asset_query_position(move |_, _| Ok(answers.pop().unwrap()));
    fx.refresh();

    assert_eq!(
        fx.position(201),
        Ok(AssetQueryPositionResult::Resolved { index: 200 })
    );
    assert_eq!(
        fx.fake.calls().last(),
        Some(&Call::GetAssetQueryPosition {
            session_id: 1,
            asset_id: 201
        })
    );
    assert_eq!(fx.position(401), Ok(AssetQueryPositionResult::Missing));
    assert_eq!(fx.position(201), Ok(AssetQueryPositionResult::Stale));
    fx.advance(STALE_REFRESH_DELAY);
    assert_eq!(fx.calls("start_asset_query"), 2);
    assert_eq!(fx.calls("get_asset_query_page"), 0);
    assert_eq!(fx.query().cached_len(), 1);
}

#[test]
fn a_position_overtaken_by_a_refresh_is_stale() {
    let mut fx = Fixture::new(128);
    fx.fake
        .on_start_asset_query(|_| Ok(start(1, 400, 0, &[1])))
        .on_get_asset_query_position(|_, _| Ok(AssetQueryPositionResult::Resolved { index: 200 }));
    fx.refresh();
    let ticket = fx.ticket();
    fx.send(QueryInput::ResolvePosition {
        ticket,
        asset_id: 201,
    });

    fx.refresh();

    assert_eq!(
        fx.events(),
        [QueryEvent::PositionResolved {
            ticket,
            result: Ok(AssetQueryPositionResult::Stale)
        }]
    );
    assert_eq!(
        fx.executor.pending_timers(),
        0,
        "the late reply schedules nothing"
    );
}

#[test]
fn a_patch_that_crosses_a_reply_makes_the_model_read_that_page_again() {
    for kind in ["initial", "page"] {
        let offset = usize::from(kind == "page");
        let mut fx = Fixture::new(1);
        let favorite = AssetSummary {
            is_favorite: true,
            ..row(2)
        };
        let grouped = AssetSummary {
            media_group_key: Some("saved".into()),
            ..favorite.clone()
        };
        // The first read of the page predates the writes; each reread sees more.
        let versions = Arc::new(Mutex::new(vec![grouped.clone(), favorite, row(2)]));
        let next = move |session_id: u64, offset: usize| {
            let item = versions.lock().unwrap().pop().unwrap();
            (session_id, offset, item)
        };
        let from_start = next.clone();
        if kind == "initial" {
            fx.fake.on_start_asset_query(move |_| {
                let (session_id, offset, item) = from_start(1, 0);
                Ok(StartAssetQueryResult::Ready {
                    session_id,
                    revision: 1,
                    total: 2,
                    offset,
                    items: vec![item],
                })
            });
        } else {
            fx.fake.on_start_asset_query(|_| Ok(start(1, 2, 0, &[1])));
        }
        fx.fake.on_get_asset_query_page(move |session, offset, _| {
            let (session_id, offset, item) = next(session, offset);
            Ok(AssetQueryPageResult::Ready {
                session_id,
                revision: 1,
                total: 2,
                offset,
                items: vec![item],
            })
        });
        let patch =
            |change: SummaryPatch| QueryInput::PatchAssets(AssetPatch::Update(vec![(2, change)]));

        let ticket = if kind == "initial" {
            fx.send(QueryInput::Refresh);
            None
        } else {
            fx.refresh();
            Some(fx.ask(1))
        };
        // A write is accepted while the read is out.
        fx.send(patch(SummaryPatch {
            is_favorite: Some(true),
            ..SummaryPatch::default()
        }));
        assert!(fx.executor.run_next());
        fx.driver.pump();
        assert_eq!(
            fx.id_at(offset),
            None,
            "{kind}: the crossed reply is not merged"
        );
        assert_eq!(fx.executor.pending(), 1, "{kind}: the reread");
        // Another write crosses the reread.
        fx.send(patch(SummaryPatch {
            media_group_key: Some(Some("saved".into())),
            ..SummaryPatch::default()
        }));
        fx.settle();

        assert_eq!(fx.query().asset_at(offset), Some(&grouped), "{kind}");
        assert_eq!(fx.calls("start_asset_query"), 1, "{kind}: never a restart");
        // Each reread asks for the page the crossed reply carried.
        let rereads = if kind == "initial" { 2 } else { 3 };
        assert_eq!(fx.page_offsets(), vec![offset; rereads], "{kind}");
        assert!(!fx.query().loading() && !fx.query().query_pending());
        if let Some(ticket) = ticket {
            assert_eq!(fx.asset_answer(ticket), Ok(Some(2)));
        }
    }
}

#[test]
fn a_removed_row_leaves_a_hole_and_positions_do_not_shift() {
    let mut fx = Fixture::new(1);
    fx.linear(1, 30);
    fx.refresh();
    fx.read(1).unwrap();
    let version = fx.query().version();

    fx.send(QueryInput::PatchAssets(AssetPatch::Remove(vec![1])));

    assert_eq!(fx.id_at(0), None);
    assert_eq!(fx.query().asset_index(1), None);
    assert_eq!(fx.id_at(1), Some(2));
    assert_eq!(fx.query().asset_index(2), Some(1));
    assert!(fx.query().version() > version);
    assert_eq!(fx.read(0), Ok(None), "the hole is not requested again");
    assert_eq!(fx.calls("get_asset_query_page"), 1);
}

#[test]
fn reset_clears_rows_total_session_and_error() {
    let mut fx = Fixture::new(PAGE);
    fx.linear(1, 6);
    fx.refresh();
    fx.send(QueryInput::SetViewport(Some(viewport(0, 5))));
    fx.settle();
    let epoch = fx.query().query_epoch();
    assert_eq!(fx.query().cached_len(), 6);

    fx.send(QueryInput::Reset);

    assert_eq!(fx.query().total(), 0);
    assert_eq!(fx.cached_ids(), [] as [i64; 0]);
    assert_eq!(fx.query().session_id(), None);
    assert_eq!(fx.query().sections(), []);
    assert_eq!(fx.query().load_error(), None);
    assert!(!fx.query().loading() && !fx.query().query_pending());
    assert_eq!(fx.query().query_epoch(), epoch + 1);
    assert_eq!(fx.id_at(0), None);
    // The next refresh starts from the empty state.
    fx.linear(2, 1);
    fx.refresh();
    assert_eq!(fx.query().total(), 1);
}

#[test]
fn disposal_answers_pending_reads_and_requests_nothing_again() {
    let mut fx = Fixture::new(1);
    fx.linear(1, 30);
    fx.fake
        .on_get_asset_query_id_range(|_, _, _| Ok(AssetIdRange::Ready(vec![1, 2])));
    fx.refresh();
    let page_read = fx.ask(1);
    let range = fx.ticket();
    fx.send(QueryInput::ReadIdRange {
        ticket: range,
        start: 0,
        end: 1,
    });

    fx.send(QueryInput::Dispose);
    fx.settle();

    assert_eq!(
        fx.events(),
        [
            QueryEvent::AssetRead {
                ticket: page_read,
                result: Ok(None)
            },
            QueryEvent::IdRangeRead {
                ticket: range,
                result: Err(QueryError::Cancelled)
            },
        ]
    );
    assert_eq!(fx.id_at(1), None, "the late page did not merge");
    fx.refresh();
    fx.send(QueryInput::RetryLoad);
    fx.settle();
    assert_eq!(fx.calls("start_asset_query"), 1);
    assert_eq!(fx.keep_alive(1), Ok(KeepAlive::Skipped));
}

#[test]
fn a_million_results_cost_one_page_of_memory_and_one_request_per_page() {
    let total = 1_000_000;
    let mut fx = Fixture::new(128);
    fx.linear(1, total);
    fx.refresh();
    assert_eq!(fx.query().total(), total);
    assert_eq!(fx.query().cached_len(), 128);

    // Jump to the end, then through the middle: only the pages of the range load.
    let mut requested = 0;
    for start in [total - 300, 500_000, 128 * 4000, 0] {
        let before = fx.calls("get_asset_query_page");
        fx.send(QueryInput::SetViewport(Some(viewport(start, start + 299))));
        fx.settle();
        let pages = fx.calls("get_asset_query_page") - before;
        assert!(pages <= 4, "{start}: {pages} pages for a 300-row range");
        requested += pages;
        assert_eq!(fx.id_at(start), Some(start as i64 + 1));
        assert!(fx.query().cached_pages() <= MAX_CACHED_PAGES);
    }
    assert!(requested <= 16);
    assert_eq!(fx.query().total(), total);
    assert!(fx.query().cached_len() <= MAX_CACHED_PAGES * 128);
}

#[test]
fn the_model_reads_the_real_core_on_background_threads() {
    let core = TempCore::start();
    for index in 0..5 {
        core.write_image(&format!("image-{index:02}.png"));
    }
    core.backend()
        .scan_folder(TempCore::text(&core.media_dir()))
        .unwrap();
    let executor = Arc::new(ThreadExecutor::new());
    let mut driver = Driver::new(
        AssetQuery::new(QueryConfig { page_size: 2 }),
        QueryRunner::new(core.shared()),
        executor.clone() as Arc<dyn Executor>,
    );
    let wait = |driver: &mut Driver<AssetQuery, QueryRunner>,
                done: &dyn Fn(&AssetQuery) -> bool| {
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        while !done(driver.model()) {
            assert!(
                std::time::Instant::now() < deadline,
                "the query did not settle"
            );
            driver.pump_one(Duration::from_millis(50));
        }
    };

    driver.handle(QueryInput::Refresh);
    wait(&mut driver, &|query| !query.query_pending());
    assert_eq!(driver.model().total(), 5);
    assert_eq!(driver.model().cached_len(), 2);
    let session_id = driver.model().session_id().expect("a session");

    driver.handle(QueryInput::SetViewport(Some(viewport(0, 4))));
    wait(&mut driver, &|query| {
        !query.loading() && query.cached_len() == 5
    });
    let ids: Vec<i64> = driver.model().assets().map(|asset| asset.id).collect();
    assert_eq!(ids.len(), 5);
    for (index, id) in ids.iter().enumerate() {
        assert_eq!(driver.model().asset_index(*id), Some(index));
    }

    driver.handle(QueryInput::ReadIdRange {
        ticket: Ticket(1),
        start: 0,
        end: 99,
    });
    driver.handle(QueryInput::ResolvePosition {
        ticket: Ticket(2),
        asset_id: ids[3],
    });
    driver.handle(QueryInput::KeepSessionAlive {
        ticket: Ticket(3),
        session_id,
    });
    let mut events = Vec::new();
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    while events.len() < 3 {
        assert!(
            std::time::Instant::now() < deadline,
            "the reads did not answer"
        );
        driver.pump_one(Duration::from_millis(50));
        events.extend(driver.model_mut().take_events());
    }
    events.sort_by_key(|event| match event {
        QueryEvent::IdRangeRead { ticket, .. }
        | QueryEvent::PositionResolved { ticket, .. }
        | QueryEvent::SessionKeptAlive { ticket, .. }
        | QueryEvent::AssetRead { ticket, .. } => *ticket,
    });
    assert_eq!(
        events,
        [
            QueryEvent::IdRangeRead {
                ticket: Ticket(1),
                result: Ok(ids.clone())
            },
            QueryEvent::PositionResolved {
                ticket: Ticket(2),
                result: Ok(AssetQueryPositionResult::Resolved { index: 3 })
            },
            QueryEvent::SessionKeptAlive {
                ticket: Ticket(3),
                result: Ok(KeepAlive::Ready)
            },
        ]
    );
    wait(&mut driver, &|query| !query.sections().is_empty());
    assert_eq!(
        driver
            .model()
            .sections()
            .iter()
            .map(|section| section.count)
            .sum::<usize>(),
        5
    );
    executor.shutdown();
}

// ---------------------------------------------------------------------------
// Behaviour tests written against the specification (the Solid tests of
// `src/library/query/__tests__` and `src/library/__tests__`, and the acceptance
// criteria of epic E07), next to the implementer's own cases above.
// ---------------------------------------------------------------------------

fn start_requests(fx: &Fixture) -> Vec<crate::backend::AssetQueryRequest> {
    fx.fake
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            Call::StartAssetQuery { request } => Some(request),
            _ => None,
        })
        .collect()
}

/// createLibrary.test.ts "loads first page with replace mode": the whole request of
/// the first refresh, field by field.
#[test]
fn spec_the_first_refresh_sends_every_filter_field_with_generation_one_and_no_anchor() {
    let filters = LibraryFilters {
        include: vec!["cat".into()],
        exclude: vec!["dog".into()],
        media_kind: Some(MediaKind::Image),
        favorites_only: true,
        meta_filter: None,
    };
    let mut fx = Fixture::with_filters(PAGE, filters);
    fx.fake
        .on_start_asset_query(|_| Ok(start(1, 5, 0, &[1, 2])));

    fx.refresh();

    let requests = start_requests(&fx);
    assert_eq!(
        requests,
        [crate::backend::AssetQueryRequest {
            tags_and: vec!["cat".into()],
            tags_not: vec!["dog".into()],
            kind: Some(MediaKind::Image),
            favorites_only: true,
            meta_filter: None,
            owner_id: fx.query().owner_id().to_owned(),
            generation: 1,
            page_size: PAGE,
            anchor_offset: None,
        }]
    );
    assert_eq!(fx.cached_ids(), [1, 2]);
    assert_eq!(fx.query().total(), 5);
    assert!(!fx.query().loading());
}

/// createLibrary.test.ts "forwards meta filters to asset loading", through the model.
#[test]
fn spec_a_meta_filter_reaches_the_start_request() {
    let group = crate::backend::MetaFilter::GroupName {
        group_name: "Trip-2026".into(),
    };
    let mut fx = Fixture::new(10);
    fx.fake.on_start_asset_query(|_| Ok(start(1, 1, 0, &[3])));

    fx.send(QueryInput::ApplyFilters(LibraryFilters {
        meta_filter: Some(group.clone()),
        ..LibraryFilters::default()
    }));
    fx.settle();

    let requests = start_requests(&fx);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].meta_filter, Some(group));
    assert!(requests[0].tags_and.is_empty() && requests[0].tags_not.is_empty());
    assert_eq!(requests[0].kind, None);
    assert!(!requests[0].favorites_only);
    assert_eq!((requests[0].generation, requests[0].page_size), (1, 10));
    assert_eq!(fx.cached_ids(), [3]);
}

/// createAssetQuery.test.ts "keeps the newest filter with reversed admission": both
/// starts carry the same owner, ascending generations, and the second the new filter.
#[test]
fn spec_reversed_admission_keeps_one_owner_and_ascending_generations() {
    for newest_first in [true, false] {
        let mut fx = Fixture::new(PAGE);
        fx.fake.on_start_asset_query(|request| {
            Ok(if request.tags_and == ["new"] {
                start(2, 1, 0, &[9])
            } else {
                StartAssetQueryResult::Superseded
            })
        });
        for tag in ["old", "new"] {
            fx.send(QueryInput::ApplyFilters(LibraryFilters {
                include: vec![tag.into()],
                ..LibraryFilters::default()
            }));
        }
        assert!(fx.executor.run_nth(usize::from(newest_first)));
        fx.driver.pump();
        assert!(fx.executor.run_nth(0));
        fx.settle();

        let requests = start_requests(&fx);
        let (old, new) = if newest_first {
            (&requests[1], &requests[0])
        } else {
            (&requests[0], &requests[1])
        };
        assert_eq!(old.owner_id, new.owner_id, "newest first: {newest_first}");
        assert!(
            old.generation < new.generation,
            "newest first: {newest_first}"
        );
        assert_eq!(new.tags_and, ["new"]);
        assert_eq!(old.tags_and, ["old"]);
        assert_eq!(fx.cached_ids(), [9]);
        assert_eq!(
            fx.query().load_error(),
            None,
            "the old supersession is not an error"
        );
        assert_eq!(fx.query().page_failure_epoch(), 0);
    }
}

/// createAssetQuery.test.ts "notifies render-time readers through version when a page
/// merges in place" and queryRaces.test.ts "reactive slots update after page arrival
/// while loading alone preserves the assets array": a request going out changes
/// nothing a reader sees; its arrival bumps `version` and shows the row.
#[test]
fn spec_version_moves_when_a_page_merges_and_not_when_it_is_only_requested() {
    let mut fx = Fixture::new(PAGE);
    fx.linear(1, 6);
    fx.refresh();
    assert_eq!(fx.id_at(PAGE), None);
    let before = fx.query().version();

    let ticket = fx.ask(PAGE);
    assert!(fx.query().loading());
    assert_eq!(
        fx.query().version(),
        before,
        "loading alone changes nothing a reader sees"
    );
    assert_eq!(fx.id_at(PAGE), None);

    fx.settle();

    assert_eq!(fx.asset_answer(ticket), Ok(Some(PAGE as i64 + 1)));
    assert!(fx.query().version() > before, "the merge bumps the version");
    assert_eq!(fx.id_at(PAGE), Some(PAGE as i64 + 1));
    assert!(!fx.query().loading());
}

/// queryRaces.test.ts "publishes an unchanged viewport's missing page when it arrives".
#[test]
fn spec_an_unchanged_viewport_shows_its_missing_page_when_it_arrives() {
    let mut fx = Fixture::new(1);
    fx.linear(1, 30);
    fx.refresh();

    fx.send(QueryInput::SetViewport(Some(viewport(1, 1))));
    assert_eq!(fx.id_at(1), None);
    assert_eq!(fx.executor.pending(), 1);
    let version = fx.query().version();

    // The page arrives while the viewport stays the same.
    fx.settle();

    assert_eq!(fx.id_at(1), Some(2));
    assert!(fx.query().version() > version);
    assert_eq!(fx.calls("get_asset_query_page"), 1);
}

/// Acceptance: pages keep `AssetSummary` exactly as it came, also for a later page
/// (not only the first one) and with every optional field set.
#[test]
fn spec_a_later_page_stores_its_summaries_exactly_as_they_came() {
    let mut fx = Fixture::new(PAGE);
    fx.fake
        .on_start_asset_query(|_| Ok(start(4, 4, 0, &[1, 2])));
    let unusual = AssetSummary {
        id: 3,
        file_name: "clip.webm".into(),
        preview_path: Some("/synthetic/library/clip.webm".into()),
        kind: "video".into(),
        modified_at: -5,
        width: None,
        height: None,
        duration_ms: Some(1234),
        thumb_version: "fedcba9876543210".into(),
        is_favorite: true,
        media_group_key: Some("g-1".into()),
        media_group_order: Some(2.5),
    };
    let sent = vec![unusual.clone(), row(4)];
    fx.fake
        .on_get_asset_query_page(move |session_id, offset, _| {
            Ok(AssetQueryPageResult::Ready {
                session_id,
                revision: 3,
                total: 4,
                offset,
                items: sent.clone(),
            })
        });
    fx.refresh();

    let ticket = fx.ask(2);
    fx.settle();

    let answered = fx
        .events()
        .into_iter()
        .find_map(|event| match event {
            QueryEvent::AssetRead { ticket: t, result } if t == ticket => Some(result),
            _ => None,
        })
        .expect("an answer");
    assert_eq!(answered, Ok(Some(unusual.clone())));
    assert_eq!(fx.query().asset_at(2), Some(&unusual));
    assert_eq!(fx.query().asset_at(3), Some(&row(4)));
    assert_eq!(fx.query().asset_index(3), Some(2));
}

/// createAssetQuery.test.ts "keeps the session and hides the timeline when ready
/// sections are unavailable": the later page is read from the kept session.
#[test]
fn spec_a_kept_session_without_sections_still_serves_its_later_pages() {
    let mut fx = Fixture::new(PAGE);
    fx.linear(7, 4);
    fx.fake.on_get_asset_query_sections(|session_id| {
        Ok(AssetQuerySectionsResult::Ready {
            session_id,
            total: 4,
            sections: Vec::new(),
        })
    });
    fx.refresh();
    fx.advance(STALE_REFRESH_DELAY + STALE_REFRESH_MIN_INTERVAL);

    assert_eq!(fx.read(3), Ok(Some(4)));
    assert_eq!(
        fx.fake.calls().last(),
        Some(&Call::GetAssetQueryPage {
            session_id: 7,
            offset: 2,
            limit: PAGE
        })
    );
    assert_eq!(fx.calls("start_asset_query"), 1);
}

/// Deterministic executor: a sections reply of a session that a refresh replaced
/// arrives late and must not become the timeline of the new session.
#[test]
fn spec_a_late_sections_reply_of_the_previous_session_is_dropped() {
    let old_sections = vec![AssetQuerySection {
        t: 202001,
        start: 0,
        count: 2,
    }];
    let new_sections = vec![AssetQuerySection {
        t: 202402,
        start: 0,
        count: 2,
    }];
    let mut fx = Fixture::new(PAGE);
    let mut starts = vec![start(8, 2, 0, &[3, 4]), start(7, 2, 0, &[1, 2])];
    fx.fake
        .on_start_asset_query(move |_| Ok(starts.pop().unwrap()));
    let (old, new) = (old_sections.clone(), new_sections.clone());
    fx.fake.on_get_asset_query_sections(move |session_id| {
        Ok(AssetQuerySectionsResult::Ready {
            session_id,
            total: 2,
            sections: if session_id == 7 {
                old.clone()
            } else {
                new.clone()
            },
        })
    });

    fx.send(QueryInput::Refresh);
    assert!(fx.executor.run_next(), "the start of session 7");
    fx.driver.pump();
    assert_eq!(fx.query().session_id(), Some(7));
    assert_eq!(fx.executor.pending(), 1, "the sections read of session 7");

    fx.send(QueryInput::Refresh);
    assert!(fx.executor.run_nth(1), "the start of session 8");
    fx.driver.pump();
    assert_eq!(fx.query().session_id(), Some(8));
    // The sections of session 7 arrive only now.
    assert!(fx.executor.run_nth(0));
    fx.driver.pump();
    assert_eq!(fx.query().sections(), [], "the late reply is not applied");

    fx.settle();
    assert_eq!(fx.query().sections(), new_sections);
    assert_eq!(fx.executor.pending_timers(), 0);
}

/// Deterministic executor: a start that a reset overtook publishes nothing.
#[test]
fn spec_a_start_reply_that_a_reset_overtook_publishes_nothing() {
    let mut fx = Fixture::new(PAGE);
    fx.linear(1, 6);
    fx.send(QueryInput::Refresh);
    assert!(fx.query().query_pending());

    fx.send(QueryInput::Reset);
    assert!(!fx.query().query_pending());
    fx.settle();

    assert_eq!(fx.query().total(), 0);
    assert_eq!(fx.cached_ids(), [] as [i64; 0]);
    assert_eq!(fx.query().session_id(), None);
    assert_eq!(fx.query().load_error(), None);
    assert_eq!(fx.calls("get_asset_query_sections"), 0);
}

/// Deterministic executor: a keepalive overtaken by a refresh is answered `Skipped`
/// once; its late `Stale` reply neither answers again nor schedules a refresh.
#[test]
fn spec_a_keepalive_overtaken_by_a_refresh_is_skipped_once_and_its_late_reply_ignored() {
    let mut fx = Fixture::new(PAGE);
    fx.linear(5, 4);
    fx.fake
        .on_get_asset_query_id_range(|_, _, _| Ok(AssetIdRange::Stale));
    fx.refresh();
    let ticket = fx.ticket();
    fx.send(QueryInput::KeepSessionAlive {
        ticket,
        session_id: 5,
    });
    assert_eq!(fx.executor.pending(), 1);

    fx.send(QueryInput::Refresh);
    assert_eq!(
        fx.events(),
        [QueryEvent::SessionKeptAlive {
            ticket,
            result: Ok(KeepAlive::Skipped)
        }]
    );
    fx.settle();

    assert_eq!(fx.events(), []);
    assert_eq!(
        fx.executor.pending_timers(),
        0,
        "the late stale reply schedules nothing"
    );
    assert_eq!(fx.calls("start_asset_query"), 2);
}

/// createAssetQuery's reset cancels a scheduled stale refresh; disposal does too.
#[test]
fn spec_reset_and_disposal_cancel_a_scheduled_stale_refresh() {
    for action in ["reset", "dispose"] {
        let mut fx = Fixture::new(PAGE);
        fx.fake
            .on_start_asset_query(|_| Ok(start(5, 8, 0, &[1, 2])))
            .on_get_asset_query_page(|_, _, _| Ok(AssetQueryPageResult::Stale));
        fx.refresh();
        assert_eq!(fx.read(2), Ok(None));
        assert_eq!(fx.executor.pending_timers(), 1, "{action}");

        fx.send(if action == "reset" {
            QueryInput::Reset
        } else {
            QueryInput::Dispose
        });
        fx.advance(STALE_REFRESH_DELAY + STALE_REFRESH_MIN_INTERVAL);

        assert_eq!(fx.calls("start_asset_query"), 1, "{action}");
        assert_eq!(fx.query().total(), if action == "reset" { 0 } else { 8 });
    }
}

/// createAssetQuery.test.ts "keeps a failed page retryable and clears loadError once
/// it loads": the retry reads the same page of the same session.
#[test]
fn spec_retry_reads_the_failed_page_of_the_demanded_range_from_the_same_session() {
    let mut fx = Fixture::new(PAGE);
    fx.linear(5, 4);
    fx.refresh();
    fx.fake
        .on_get_asset_query_page(|_, _, _| Err(BackendError::rejected("page worker failed")));
    assert!(fx.read(PAGE).is_err());
    assert!(fx.query().page_failure_epoch() > 0);

    fx.linear(5, 4);
    fx.send(QueryInput::SetViewport(Some(viewport(PAGE, PAGE + 1))));
    fx.send(QueryInput::RetryLoad);
    fx.settle();

    assert_eq!(fx.query().load_error(), None);
    assert_eq!(fx.id_at(PAGE), Some(3));
    let pages: Vec<_> = fx
        .fake
        .calls()
        .into_iter()
        .filter(|call| matches!(call, Call::GetAssetQueryPage { .. }))
        .collect();
    assert_eq!(
        pages,
        vec![
            Call::GetAssetQueryPage {
                session_id: 5,
                offset: PAGE,
                limit: PAGE
            };
            2
        ]
    );
}

/// Acceptance: `total` is the number of results and nothing grows with it. A viewport
/// that walks across a result of a million rows keeps the cache at the page bound
/// and every row it reads at its global index.
#[test]
fn spec_walking_a_million_results_keeps_the_cache_bounded_and_rows_at_their_indexes() {
    let total = 1_000_000;
    let mut fx = Fixture::new(128);
    fx.linear(1, total);
    fx.refresh();

    for step in 0..60 {
        let start = (step * 16_661) % (total - 200);
        fx.send(QueryInput::SetViewport(Some(viewport(start, start + 199))));
        fx.settle();
        assert!(fx.query().cached_pages() <= MAX_CACHED_PAGES, "step {step}");
        assert!(
            fx.query().cached_len() <= MAX_CACHED_PAGES * 128,
            "step {step}"
        );
        assert_eq!(fx.id_at(start), Some(start as i64 + 1), "step {step}");
        assert_eq!(
            fx.id_at(start + 199),
            Some(start as i64 + 200),
            "step {step}"
        );
    }
    assert_eq!(fx.query().total(), total);
    // A read far away loads exactly one page.
    let before = fx.calls("get_asset_query_page");
    assert_eq!(fx.read(total - 1), Ok(Some(total as i64)));
    assert_eq!(fx.calls("get_asset_query_page"), before + 1);
    assert_eq!(fx.query().asset_index(total as i64), Some(total - 1));
}

/// Real SQLite: a scan that adds files bumps the library revision, so the next page
/// read of the old session is `Stale`; the model refreshes after the debounce and
/// publishes the new total from the backend.
#[test]
fn spec_on_the_real_core_a_stale_session_after_a_scan_is_replaced_after_the_debounce() {
    let core = TempCore::start();
    for index in 0..5 {
        core.write_image(&format!("image-{index:02}.png"));
    }
    core.backend()
        .scan_folder(TempCore::text(&core.media_dir()))
        .unwrap();
    let executor = Arc::new(ThreadExecutor::new());
    let mut driver = Driver::new(
        AssetQuery::new(QueryConfig { page_size: 2 }),
        QueryRunner::new(core.shared()),
        executor.clone() as Arc<dyn Executor>,
    );
    let wait = |driver: &mut Driver<AssetQuery, QueryRunner>,
                done: &dyn Fn(&mut AssetQuery) -> bool| {
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        while !done(driver.model_mut()) {
            assert!(
                std::time::Instant::now() < deadline,
                "the query did not settle"
            );
            driver.pump_one(Duration::from_millis(50));
        }
    };

    driver.handle(QueryInput::Refresh);
    wait(&mut driver, &|query| !query.query_pending());
    assert_eq!(driver.model().total(), 5);
    let first_session = driver.model().session_id().expect("a session");

    for index in 5..8 {
        core.write_image(&format!("image-{index:02}.png"));
    }
    core.backend()
        .scan_folder(TempCore::text(&core.media_dir()))
        .unwrap();

    // Page 2 is not cached: reading it asks the old session, which is stale now.
    driver.handle(QueryInput::ReadAsset {
        ticket: Ticket(1),
        index: 4,
    });
    let answer = Arc::new(Mutex::new(None));
    let seen = answer.clone();
    wait(&mut driver, &move |query| {
        for event in query.take_events() {
            if let QueryEvent::AssetRead { result, .. } = event {
                *seen.lock().unwrap() = Some(result.map(|row| row.map(|row| row.id)));
            }
        }
        seen.lock().unwrap().is_some()
    });
    assert_eq!(
        answer.lock().unwrap().clone(),
        Some(Ok(None)),
        "a stale page is a hole"
    );
    wait(&mut driver, &|query| {
        query.session_id().is_some_and(|id| id != first_session) && !query.query_pending()
    });
    assert_eq!(driver.model().total(), 8);
    assert_eq!(driver.model().load_error(), None);
    executor.shutdown();
}

/// Real SQLite: a refresh anchored at the first visible index carries that page, and
/// its rows are the rows an unanchored read finds at the same indexes.
#[test]
fn spec_on_the_real_core_an_anchored_refresh_carries_the_visible_page() {
    let core = TempCore::start();
    for index in 0..7 {
        core.write_image(&format!("image-{index:02}.png"));
    }
    core.backend()
        .scan_folder(TempCore::text(&core.media_dir()))
        .unwrap();
    let executor = Arc::new(ThreadExecutor::new());
    let mut driver = Driver::new(
        AssetQuery::new(QueryConfig { page_size: 2 }),
        QueryRunner::new(core.shared()),
        executor.clone() as Arc<dyn Executor>,
    );
    let wait = |driver: &mut Driver<AssetQuery, QueryRunner>,
                done: &dyn Fn(&AssetQuery) -> bool| {
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        while !done(driver.model()) {
            assert!(
                std::time::Instant::now() < deadline,
                "the query did not settle"
            );
            driver.pump_one(Duration::from_millis(50));
        }
    };
    driver.handle(QueryInput::Refresh);
    wait(&mut driver, &|query| !query.query_pending());
    driver.handle(QueryInput::SetViewport(Some(viewport(0, 6))));
    wait(&mut driver, &|query| {
        !query.loading() && query.cached_len() == 7
    });
    let expected: Vec<i64> = driver.model().assets().map(|asset| asset.id).collect();

    driver.handle(QueryInput::SetViewport(Some(Viewport {
        start_index: 4,
        end_index: 5,
        visible_start_index: 5,
        visible_end_index: 5,
        paused: true,
    })));
    driver.handle(QueryInput::Refresh);
    wait(&mut driver, &|query| !query.query_pending());

    assert_eq!(driver.model().total(), 7);
    assert_eq!(driver.model().cached_len(), 2, "one page: the anchored one");
    assert_eq!(
        driver.model().asset_at(4).map(|asset| asset.id),
        Some(expected[4])
    );
    assert_eq!(
        driver.model().asset_at(5).map(|asset| asset.id),
        Some(expected[5])
    );
    assert_eq!(driver.model().asset_index(expected[5]), Some(5));
    executor.shutdown();
}
