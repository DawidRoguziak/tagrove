//! The query model: `createAssetQuery` of the Solid frontend, with the viewport and
//! filter handling of `createLibrary`, as a state machine.
//!
//! [`AssetQuery`] owns one backend query session at a time and a sparse cache of its
//! pages ([`PageCache`]). It follows the convention of [`crate::model`]: commands and
//! backend replies are inputs, backend calls and timers are effects, and
//! [`QueryRunner`] (or [`run_query_effect`] inside a parent model's runner) runs them
//! through the `Backend` port. Contract:
//! `docs/subsystems/library-query-and-gallery.md`.
//!
//! # Generations and the owner
//!
//! Every query model has its own owner ID for its lifetime, and every refresh, reset
//! and disposal starts a new generation. `start_asset_query` carries both; the backend
//! compares generations within one owner. Here, a reply is applied only when it
//! belongs to the current generation: the start by its generation, every other read by
//! a request token that the generation change forgets. Only a `Ready` result of the
//! current generation replaces the cache. A `Superseded` start that is still current
//! is a load error with Retry; the rows shown before stay.
//!
//! # Reads
//!
//! - **Viewport** ([`QueryInput::SetViewport`]): the gallery's range. Its pages are
//!   loaded when it changes and when the query epoch changes, unless it is paused
//!   (fast scrolling). Its first visible index is the `anchor_offset` of the next
//!   refresh, so the `Ready` result already carries the page on screen. A page that
//!   arrives never starts page demand again.
//! - **Tickets**: a read on behalf of another model ([`QueryInput::ReadAsset`],
//!   [`QueryInput::ResolvePosition`], [`QueryInput::ReadIdRange`],
//!   [`QueryInput::KeepSessionAlive`]) carries a [`Ticket`] chosen by the caller and is
//!   answered exactly once with a [`QueryEvent`] ([`AssetQuery::take_events`]). A read
//!   that a refresh, a reset or the disposal overtakes is answered then, without
//!   waiting for the backend: a page read with no row, a position with `Stale`, an ID
//!   range with [`QueryError::Cancelled`], a keepalive with [`KeepAlive::Skipped`].
//! - **One request per page offset**: readers of a page that is on its way share its
//!   request. A page that failed is not requested again until an explicit retry
//!   ([`QueryInput::RetryLoad`], [`QueryInput::RetryAsset`]).
//! - **ID ranges** come from the backend session vector in chunks of
//!   [`ID_RANGE_CHUNK`], never from pages, and are answered whole or not at all.
//!
//! # Stale sessions
//!
//! A `Stale` page, position, section or range read schedules one refresh
//! [`STALE_REFRESH_DELAY`] later; further stale reads join it. Two stale-driven
//! refreshes are at least [`STALE_REFRESH_MIN_INTERVAL`] apart. An explicit refresh
//! runs at once and cancels the scheduled one. The model has no clock: the delay is
//! one timer effect and the minimum interval another that starts when a stale-driven
//! refresh runs, and the scheduled refresh runs once both have fired.
//!
//! # Patches
//!
//! [`QueryInput::PatchAssets`] changes cached rows after the backend accepted a write.
//! A start or page reply that was requested before a patch may hold the rows from
//! before the write, so the model reads that page again from the returned session
//! before it merges it, and again while patches keep crossing.

use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    fmt::{Display, Formatter},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use media_tagger_core::models::{
    AssetQueryPageResult, AssetQueryPositionResult, AssetQuerySection, AssetQuerySectionsResult,
    AssetSummary, StartAssetQueryResult,
};

use super::{
    filters::LibraryFilters,
    page_cache::{AssetPatch, Merge, PageCache},
};
use crate::{
    backend::{
        AssetIdRange, AssetQueryRequest, Backend, BackendError, BackendResult,
        DEFAULT_QUERY_PAGE_SIZE,
    },
    model::{Dispatch, Generation, Model, RequestSlot, RequestToken, Runner, Tokens},
};

#[cfg(test)]
mod tests;

/// A `Stale` read schedules one refresh after this delay; later stale reads join it.
pub const STALE_REFRESH_DELAY: Duration = Duration::from_millis(250);
/// Minimum spacing between two stale-driven refreshes.
pub const STALE_REFRESH_MIN_INTERVAL: Duration = Duration::from_millis(1000);
/// Most IDs requested in one `get_asset_query_id_range` call.
pub const ID_RANGE_CHUNK: usize = 1_000_000;
/// Largest page the backend serves; it clamps a larger page size to this, so the
/// model must not lay its pages out wider.
pub const MAX_QUERY_PAGE_SIZE: usize = 256;

static NEXT_OWNER: AtomicU64 = AtomicU64::new(1);

/// An owner ID no other query model of this process has. The backend that compares
/// the generations of an owner lives in the same process, so a counter is enough.
fn next_owner_id() -> String {
    format!(
        "gpui-asset-query-{}",
        NEXT_OWNER.fetch_add(1, Ordering::Relaxed)
    )
}

/// Identifies one read made on behalf of another model. The caller chooses it and
/// finds it again in the [`QueryEvent`] that answers the read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Ticket(pub u64);

/// The gallery's range (`LibraryViewport` of `createLibrary.ts`), in global indexes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Viewport {
    /// First and last index whose pages are wanted (the visible rows and overscan).
    pub start_index: usize,
    pub end_index: usize,
    /// First and last index in view.
    pub visible_start_index: usize,
    pub visible_end_index: usize,
    /// Fast scrolling: the range is remembered and no page is requested for it.
    pub paused: bool,
}

/// Why a query, or a read against it, failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryError {
    /// The backend admitted a newer query of this owner first. Retry starts over.
    Superseded,
    /// A position lookup without a session: the query is pending or failed.
    Unavailable,
    /// An ID range whose session went away while it was read, or was gone before.
    Cancelled,
    /// An ID range chunk with fewer IDs than asked for.
    IncompleteRange,
    Backend(BackendError),
}

impl Display for QueryError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Superseded => f.write_str("Query was superseded. Retry your search."),
            Self::Unavailable => f.write_str("Asset query is unavailable"),
            Self::Cancelled => f.write_str("Asset ID range was cancelled"),
            Self::IncompleteRange => f.write_str("Asset ID range is incomplete"),
            Self::Backend(error) => Display::fmt(error, f),
        }
    }
}

impl std::error::Error for QueryError {}

/// How a keepalive read ended (`keepSessionAlive`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeepAlive {
    /// The session was read; its idle expiry starts over.
    Ready,
    /// The session is gone. A refresh is scheduled.
    Stale,
    /// Nothing was read: the session is not the current one, or the result is empty.
    Skipped,
}

/// The answer to a read that carried a [`Ticket`].
#[derive(Debug, Clone, PartialEq)]
pub enum QueryEvent {
    /// [`QueryInput::ReadAsset`] and [`QueryInput::RetryAsset`]. `Ok(None)`: no row at
    /// that index (outside the result, a hole, a stale or replaced session).
    AssetRead {
        ticket: Ticket,
        result: Result<Option<AssetSummary>, QueryError>,
    },
    /// [`QueryInput::ResolvePosition`].
    PositionResolved {
        ticket: Ticket,
        result: Result<AssetQueryPositionResult, QueryError>,
    },
    /// [`QueryInput::ReadIdRange`]: every ID of the range in session order, or an
    /// error; never a part of the range.
    IdRangeRead {
        ticket: Ticket,
        result: Result<Vec<i64>, QueryError>,
    },
    /// [`QueryInput::KeepSessionAlive`].
    SessionKeptAlive {
        ticket: Ticket,
        result: Result<KeepAlive, QueryError>,
    },
}

/// What a command needs at once, in the same turn as a refresh (`navigationSnapshot`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NavigationSnapshot {
    pub epoch: u64,
    pub pending: bool,
    pub total: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueryConfig {
    /// Rows per page, used within 1 to [`MAX_QUERY_PAGE_SIZE`] as the backend clamps
    /// it.
    pub page_size: usize,
}

impl Default for QueryConfig {
    fn default() -> Self {
        Self {
            page_size: DEFAULT_QUERY_PAGE_SIZE,
        }
    }
}

/// Commands, backend replies and timer firings.
#[derive(Debug)]
pub enum QueryInput {
    /// Starts the query again with the applied filters, at once.
    Refresh,
    /// Applies new filters and refreshes, also when they equal the old ones.
    ApplyFilters(LibraryFilters),
    /// The gallery's range changed; `None` when the gallery is gone.
    SetViewport(Option<Viewport>),
    /// Explicit Retry: restarts a query without a session, otherwise unblocks every
    /// failed page and requests the failed pages of the demanded range again.
    RetryLoad,
    /// The library was cleared or replaced: no rows, no session, total 0.
    Reset,
    /// The owner is going away: pending reads are answered and nothing is requested
    /// again.
    Dispose,
    /// A local change to cached rows after an accepted write.
    PatchAssets(AssetPatch),
    /// The row at a global index, from the cache or by loading its page.
    ReadAsset { ticket: Ticket, index: usize },
    /// [`ReadAsset`](Self::ReadAsset) after unblocking the failed page of that index
    /// only.
    RetryAsset { ticket: Ticket, index: usize },
    /// The session position of an asset, cached or not.
    ResolvePosition { ticket: Ticket, asset_id: i64 },
    /// The IDs of the inclusive index range, clamped to the result; the bounds may be
    /// given in either order.
    ReadIdRange {
        ticket: Ticket,
        start: usize,
        end: usize,
    },
    /// Reads one ID of `session_id` so the backend keeps it, if it is still current.
    KeepSessionAlive { ticket: Ticket, session_id: u64 },

    Started {
        generation: u64,
        result: BackendResult<StartAssetQueryResult>,
    },
    PageLoaded {
        token: RequestToken,
        result: BackendResult<AssetQueryPageResult>,
    },
    SectionsLoaded {
        token: RequestToken,
        result: BackendResult<AssetQuerySectionsResult>,
    },
    PositionLoaded {
        token: RequestToken,
        result: BackendResult<AssetQueryPositionResult>,
    },
    IdRangeLoaded {
        token: RequestToken,
        result: BackendResult<AssetIdRange>,
    },

    /// The debounce timer of a scheduled stale refresh fired.
    StaleRefreshDue(RequestToken),
    /// The minimum interval after a stale-driven refresh has passed.
    StaleCooldownEnded(RequestToken),
}

/// Work for the runner.
#[derive(Debug, Clone, PartialEq)]
pub enum QueryEffect {
    /// `start_asset_query`; answered with [`QueryInput::Started`] and the request's
    /// generation.
    Start(AssetQueryRequest),
    LoadPage {
        token: RequestToken,
        session_id: u64,
        offset: usize,
        limit: usize,
    },
    LoadSections {
        token: RequestToken,
        session_id: u64,
    },
    ResolvePosition {
        token: RequestToken,
        session_id: u64,
        asset_id: i64,
    },
    /// `get_asset_query_id_range` for the inclusive positions `start..=end`.
    LoadIdRange {
        token: RequestToken,
        session_id: u64,
        start: usize,
        end: usize,
    },
    /// Send [`QueryInput::StaleRefreshDue`] after `delay`.
    StaleRefreshTimer {
        token: RequestToken,
        delay: Duration,
    },
    /// Send [`QueryInput::StaleCooldownEnded`] after `delay`.
    StaleCooldownTimer {
        token: RequestToken,
        delay: Duration,
    },
}

/// The `start_asset_query` of the current generation.
#[derive(Debug)]
struct StartFlight {
    /// The patch counter when the request, or its latest reread, was sent.
    patch_counter: u64,
    /// The reread of the returned page after a patch crossed the reply.
    reread: Option<RequestToken>,
}

/// The one request of a page offset and everyone waiting for it.
#[derive(Debug)]
struct PageFlight {
    patch_counter: u64,
    /// Tickets with the global index each asked for.
    waiters: Vec<(Ticket, usize)>,
}

#[derive(Debug)]
enum IdRead {
    Range(RangeRead),
    KeepAlive { ticket: Ticket },
}

/// An ID range that is being read chunk by chunk.
#[derive(Debug)]
struct RangeRead {
    ticket: Ticket,
    session_id: u64,
    /// Last position of the range.
    to: usize,
    /// The chunk that is on its way.
    chunk_start: usize,
    chunk_end: usize,
    ids: Vec<i64>,
}

/// Timeline sections and the session they belong to; no session while they load.
#[derive(Debug, Default)]
struct Sections {
    session_id: Option<u64>,
    items: Vec<AssetQuerySection>,
}

/// The scheduled stale refresh and the interval that keeps two of them apart.
#[derive(Debug, Default)]
struct StaleRefresh {
    pending: Option<PendingStale>,
    /// Set from the moment a stale-driven refresh runs until its interval timer fires.
    cooldown: Option<RequestToken>,
}

#[derive(Debug)]
struct PendingStale {
    timer: RequestToken,
    /// The debounce timer fired; the refresh waits for the cooldown only.
    debounced: bool,
}

#[derive(Debug)]
pub struct AssetQuery {
    owner_id: String,
    page_size: usize,
    filters: LibraryFilters,
    viewport: Option<Viewport>,
    tokens: Tokens,
    generation: Generation,
    session_id: Option<u64>,
    total: usize,
    load_error: Option<QueryError>,
    page_failure_epoch: u64,
    query_epoch: u64,
    patch_counter: u64,
    disposed: bool,
    cache: PageCache,
    sections: Sections,
    sections_request: RequestSlot,
    start: Option<StartFlight>,
    /// Page requests of the current generation, by page offset.
    in_flight: BTreeMap<usize, PageFlight>,
    /// The page offset each awaited page token belongs to.
    page_tokens: HashMap<RequestToken, usize>,
    /// Pages that failed in the current generation. Demand does not retry them.
    failed_pages: HashMap<usize, BackendError>,
    position_reads: BTreeMap<RequestToken, Ticket>,
    id_reads: BTreeMap<RequestToken, IdRead>,
    stale: StaleRefresh,
    events: VecDeque<QueryEvent>,
}

impl Default for AssetQuery {
    fn default() -> Self {
        Self::new(QueryConfig::default())
    }
}

impl AssetQuery {
    /// A query model with an owner ID of its own, unfiltered, with nothing loaded.
    pub fn new(config: QueryConfig) -> Self {
        Self::with_filters(config, LibraryFilters::default())
    }

    pub fn with_filters(config: QueryConfig, filters: LibraryFilters) -> Self {
        let cache = PageCache::new(config.page_size.clamp(1, MAX_QUERY_PAGE_SIZE));
        Self {
            owner_id: next_owner_id(),
            page_size: cache.page_size(),
            filters,
            viewport: None,
            tokens: Tokens::default(),
            generation: Generation::default(),
            session_id: None,
            total: 0,
            load_error: None,
            page_failure_epoch: 0,
            query_epoch: 0,
            patch_counter: 0,
            disposed: false,
            cache,
            sections: Sections::default(),
            sections_request: RequestSlot::default(),
            start: None,
            in_flight: BTreeMap::new(),
            page_tokens: HashMap::new(),
            failed_pages: HashMap::new(),
            position_reads: BTreeMap::new(),
            id_reads: BTreeMap::new(),
            stale: StaleRefresh::default(),
            events: VecDeque::new(),
        }
    }

    /// The owner of this model's queries towards the backend.
    pub fn owner_id(&self) -> &str {
        &self.owner_id
    }

    pub fn page_size(&self) -> usize {
        self.page_size
    }

    /// The applied filters: what the next refresh asks for.
    pub fn filters(&self) -> &LibraryFilters {
        &self.filters
    }

    /// The gallery's latest range, also while it is paused.
    pub fn viewport(&self) -> Option<Viewport> {
        self.viewport
    }

    /// The number of results of the last `Ready` query: the count a virtual grid lays
    /// out. It says nothing about how many rows are cached.
    pub fn total(&self) -> usize {
        self.total
    }

    /// Whether a query start or a page request of the current generation is out.
    pub fn loading(&self) -> bool {
        self.start.is_some() || !self.in_flight.is_empty()
    }

    /// The error of the last start or page request that failed; cleared by the next
    /// `Ready` start, by a page that loads while no page is failed, and by Retry.
    pub fn load_error(&self) -> Option<&QueryError> {
        self.load_error.as_ref()
    }

    /// Counts failed starts and failed pages. With [`session_id`](Self::session_id)
    /// `None` a change means the session is lost; with a session, a page failed.
    pub fn page_failure_epoch(&self) -> u64 {
        self.page_failure_epoch
    }

    /// Changes when a refresh starts, when its `Ready` result is applied and on reset.
    /// Positions and details read under another epoch are obsolete.
    pub fn query_epoch(&self) -> u64 {
        self.query_epoch
    }

    /// Whether a refresh waits for the backend.
    pub fn query_pending(&self) -> bool {
        self.start.is_some()
    }

    pub fn navigation_snapshot(&self) -> NavigationSnapshot {
        NavigationSnapshot {
            epoch: self.query_epoch,
            pending: self.query_pending(),
            total: self.total,
        }
    }

    /// Timeline sections of the current session in session order; empty until they
    /// loaded, and when the backend has none for this snapshot.
    pub fn sections(&self) -> &[AssetQuerySection] {
        &self.sections.items
    }

    /// The backend session, or `None` while a query is pending and after one failed.
    pub fn session_id(&self) -> Option<u64> {
        self.session_id
    }

    /// Counts the changes of the cache: a view that reads rows redraws when it moves.
    pub fn version(&self) -> u64 {
        self.cache.version()
    }

    /// The cached rows of the current query in index order. Cached rows only, at most
    /// 24 pages of them: never index this as the result list.
    pub fn assets(&self) -> impl Iterator<Item = &AssetSummary> {
        self.cache.assets()
    }

    /// How many rows are cached.
    pub fn cached_len(&self) -> usize {
        self.cache.len()
    }

    /// How many pages are cached.
    pub fn cached_pages(&self) -> usize {
        self.cache.page_count()
    }

    /// What a tile at global index `index` shows (`getAssetAt`): the cached row, or
    /// while its page has not loaded since the last refresh the previous query's row
    /// there, or nothing. Reading requests and promotes nothing.
    pub fn asset_at(&self, index: usize) -> Option<&AssetSummary> {
        self.cache.asset_at(index)
    }

    /// The global index of an asset whose page is cached, while a session exists
    /// (`getAssetIndex`). Fallback rows have no index.
    pub fn asset_index(&self, asset_id: i64) -> Option<usize> {
        self.session_id?;
        self.cache.index_of(asset_id)
    }

    /// The answers to ticketed reads since the last call, oldest first.
    pub fn take_events(&mut self) -> Vec<QueryEvent> {
        self.events.drain(..).collect()
    }

    fn emit(&mut self, event: QueryEvent) {
        self.events.push_back(event);
    }

    fn answer_asset(&mut self, ticket: Ticket, result: Result<Option<AssetSummary>, QueryError>) {
        self.emit(QueryEvent::AssetRead { ticket, result });
    }

    /// Makes every request of the current generation obsolete and answers the readers
    /// that wait for one (`invalidateReads`).
    fn invalidate_reads(&mut self) {
        self.generation.advance();
        self.session_id = None;
        self.start = None;
        self.sections_request.cancel();
        self.page_tokens.clear();
        self.failed_pages.clear();
        for flight in std::mem::take(&mut self.in_flight).into_values() {
            for (ticket, _) in flight.waiters {
                self.answer_asset(ticket, Ok(None));
            }
        }
        for ticket in std::mem::take(&mut self.position_reads).into_values() {
            self.emit(QueryEvent::PositionResolved {
                ticket,
                result: Ok(AssetQueryPositionResult::Stale),
            });
        }
        for read in std::mem::take(&mut self.id_reads).into_values() {
            self.emit(match read {
                IdRead::Range(read) => QueryEvent::IdRangeRead {
                    ticket: read.ticket,
                    result: Err(QueryError::Cancelled),
                },
                IdRead::KeepAlive { ticket } => QueryEvent::SessionKeptAlive {
                    ticket,
                    result: Ok(KeepAlive::Skipped),
                },
            });
        }
    }

    fn refresh(&mut self, effects: &mut Vec<QueryEffect>) {
        if self.disposed {
            return;
        }
        self.stale.pending = None;
        self.invalidate_reads();
        self.query_epoch += 1;
        self.cache.release_active();
        self.start = Some(StartFlight {
            patch_counter: self.patch_counter,
            reread: None,
        });
        effects.push(QueryEffect::Start(self.filters.request(
            &self.owner_id,
            self.generation.current(),
            self.page_size,
            self.viewport.map(|viewport| viewport.visible_start_index),
        )));
        // The epoch changed: the range is demanded again. Without a session this
        // requests nothing; it restores which pages eviction spares.
        self.demand_viewport(effects);
    }

    fn reset(&mut self) {
        self.stale.pending = None;
        self.invalidate_reads();
        self.cache.reset();
        self.sections = Sections::default();
        self.total = 0;
        self.load_error = None;
        self.query_epoch += 1;
    }

    fn dispose(&mut self) {
        self.disposed = true;
        self.stale.pending = None;
        self.invalidate_reads();
    }

    /// Requests the pages of the viewport, unless it is paused or gone.
    fn demand_viewport(&mut self, effects: &mut Vec<QueryEffect>) {
        let Some(viewport) = self.viewport.filter(|viewport| !viewport.paused) else {
            return;
        };
        let Some(span) =
            self.cache
                .set_demand(viewport.start_index, viewport.end_index, self.total)
        else {
            return;
        };
        if self.session_id.is_none() {
            return;
        }
        for offset in span.offsets(self.page_size) {
            self.load_page(offset, None, effects);
        }
    }

    /// `loadPage`: joins or starts the request of the page at `offset`. A `waiter` is
    /// answered now when nothing has to be requested, otherwise with the page.
    fn load_page(
        &mut self,
        offset: usize,
        waiter: Option<(Ticket, usize)>,
        effects: &mut Vec<QueryEffect>,
    ) {
        let session_id = match self.session_id {
            Some(session_id) if offset < self.total => session_id,
            _ => {
                if let Some((ticket, _)) = waiter {
                    self.answer_asset(ticket, Ok(None));
                }
                return;
            }
        };
        self.cache.touch(offset);
        if self.cache.has_page(offset) {
            // The caller found no row at its index: a hole or the end of a short page.
            if let Some((ticket, _)) = waiter {
                self.answer_asset(ticket, Ok(None));
            }
            return;
        }
        if let Some(error) = self.failed_pages.get(&offset) {
            // Demand must not retry this hole until the user explicitly retries.
            if let Some((ticket, _)) = waiter {
                let error = QueryError::Backend(error.clone());
                self.answer_asset(ticket, Err(error));
            }
            return;
        }
        if let Some(flight) = self.in_flight.get_mut(&offset) {
            flight.waiters.extend(waiter);
            return;
        }
        let token = self.tokens.issue();
        self.page_tokens.insert(token, offset);
        self.in_flight.insert(
            offset,
            PageFlight {
                patch_counter: self.patch_counter,
                waiters: waiter.into_iter().collect(),
            },
        );
        effects.push(QueryEffect::LoadPage {
            token,
            session_id,
            offset,
            limit: self.page_size,
        });
    }

    fn read_asset(&mut self, ticket: Ticket, index: usize, effects: &mut Vec<QueryEffect>) {
        let offset = self.cache.page_offset(index);
        if let Some(row) = self.cache.cached_at(index) {
            let row = row.clone();
            self.cache.touch(offset);
            self.answer_asset(ticket, Ok(Some(row)));
            return;
        }
        self.load_page(offset, Some((ticket, index)), effects);
    }

    fn retry_load(&mut self, effects: &mut Vec<QueryEffect>) {
        if self.session_id.is_none() {
            self.refresh(effects);
            return;
        }
        // Explicit Retry also unblocks failed offscreen pages (lightbox reads). Only
        // viewport demand is fetched here.
        let mut offsets: Vec<usize> = self
            .failed_pages
            .keys()
            .copied()
            .filter(|offset| self.cache.is_active(*offset))
            .collect();
        offsets.sort_unstable();
        self.failed_pages.clear();
        self.load_error = None;
        for offset in offsets {
            self.load_page(offset, None, effects);
        }
    }

    fn on_started(
        &mut self,
        generation: u64,
        result: BackendResult<StartAssetQueryResult>,
        effects: &mut Vec<QueryEffect>,
    ) {
        if !self.generation.is_current(generation) {
            return;
        }
        let Some(start) = &self.start else {
            return;
        };
        if start.reread.is_some() {
            // The start was answered before; this is a repeated reply.
            return;
        }
        match result {
            Ok(StartAssetQueryResult::Ready {
                session_id,
                total,
                offset,
                items,
                ..
            }) => self.on_start_page(session_id, total, offset, items, effects),
            Ok(StartAssetQueryResult::Superseded) => self.fail_start(QueryError::Superseded),
            Err(error) => self.fail_start(QueryError::Backend(error)),
        }
    }

    /// A `Ready` first page of the current generation, from the start or from its
    /// reread: read again when a patch crossed it, otherwise the new result.
    fn on_start_page(
        &mut self,
        session_id: u64,
        total: usize,
        offset: usize,
        items: Vec<AssetSummary>,
        effects: &mut Vec<QueryEffect>,
    ) {
        let Some(start) = &mut self.start else {
            return;
        };
        if start.patch_counter != self.patch_counter {
            let token = self.tokens.issue();
            start.patch_counter = self.patch_counter;
            start.reread = Some(token);
            effects.push(QueryEffect::LoadPage {
                token,
                session_id,
                offset,
                limit: self.page_size,
            });
            return;
        }
        self.start = None;
        self.session_id = Some(session_id);
        if self.sections.session_id != Some(session_id) {
            self.sections = Sections::default();
            let token = self.sections_request.begin(&mut self.tokens);
            effects.push(QueryEffect::LoadSections { token, session_id });
        }
        self.query_epoch += 1;
        self.total = total;
        self.load_error = None;
        self.cache.merge(offset, items, Merge::Replace, total);
        // The epoch changed: the other pages of the range are requested now.
        self.demand_viewport(effects);
    }

    fn fail_start(&mut self, error: QueryError) {
        self.start = None;
        self.load_error = Some(error);
        self.page_failure_epoch += 1;
    }

    fn on_page_loaded(
        &mut self,
        token: RequestToken,
        result: BackendResult<AssetQueryPageResult>,
        effects: &mut Vec<QueryEffect>,
    ) {
        if self
            .start
            .as_ref()
            .is_some_and(|start| start.reread == Some(token))
        {
            match result {
                Ok(AssetQueryPageResult::Ready {
                    session_id,
                    total,
                    offset,
                    items,
                    ..
                }) => self.on_start_page(session_id, total, offset, items, effects),
                Ok(AssetQueryPageResult::Stale) => {
                    self.start = None;
                    self.schedule_stale_refresh(effects);
                }
                Err(error) => self.fail_start(QueryError::Backend(error)),
            }
            return;
        }
        // A token the generation change forgot: the reply is obsolete.
        let Some(offset) = self.page_tokens.remove(&token) else {
            return;
        };
        match result {
            Ok(AssetQueryPageResult::Ready {
                session_id,
                offset: page_offset,
                items,
                ..
            }) => {
                let patch_counter = self.patch_counter;
                if let Some(flight) = self
                    .in_flight
                    .get_mut(&offset)
                    .filter(|flight| flight.patch_counter != patch_counter)
                {
                    flight.patch_counter = patch_counter;
                    let token = self.tokens.issue();
                    self.page_tokens.insert(token, offset);
                    effects.push(QueryEffect::LoadPage {
                        token,
                        session_id,
                        offset: page_offset,
                        limit: self.page_size,
                    });
                    return;
                }
                let waiters = self.take_waiters(offset);
                if self.failed_pages.is_empty() {
                    self.load_error = None;
                }
                for (ticket, index) in waiters {
                    let row = index
                        .checked_sub(offset)
                        .and_then(|local| items.get(local))
                        .cloned();
                    self.answer_asset(ticket, Ok(row));
                }
                self.cache
                    .merge(page_offset, items, Merge::Page, self.total);
            }
            Ok(AssetQueryPageResult::Stale) => {
                // A stale page is a hole while a fresh query replaces the view.
                for (ticket, _) in self.take_waiters(offset) {
                    self.answer_asset(ticket, Ok(None));
                }
                self.schedule_stale_refresh(effects);
            }
            Err(error) => {
                for (ticket, _) in self.take_waiters(offset) {
                    self.answer_asset(ticket, Err(QueryError::Backend(error.clone())));
                }
                self.failed_pages.insert(offset, error.clone());
                self.load_error = Some(QueryError::Backend(error));
                self.page_failure_epoch += 1;
            }
        }
    }

    fn take_waiters(&mut self, offset: usize) -> Vec<(Ticket, usize)> {
        self.in_flight
            .remove(&offset)
            .map(|flight| flight.waiters)
            .unwrap_or_default()
    }

    /// Sections are optional: a failure leaves the timeline hidden.
    fn on_sections_loaded(
        &mut self,
        token: RequestToken,
        result: BackendResult<AssetQuerySectionsResult>,
        effects: &mut Vec<QueryEffect>,
    ) {
        if !self.sections_request.finish(token) {
            return;
        }
        match result {
            Ok(AssetQuerySectionsResult::Ready {
                session_id,
                total,
                sections,
            }) => {
                self.sections = Sections {
                    session_id: Some(session_id),
                    items: if total == self.total {
                        sections
                    } else {
                        Vec::new()
                    },
                };
            }
            Ok(AssetQuerySectionsResult::Stale) => self.schedule_stale_refresh(effects),
            Err(_) => {}
        }
    }

    fn resolve_position(&mut self, ticket: Ticket, asset_id: i64, effects: &mut Vec<QueryEffect>) {
        let Some(session_id) = self.session_id else {
            self.emit(QueryEvent::PositionResolved {
                ticket,
                result: Err(QueryError::Unavailable),
            });
            return;
        };
        let token = self.tokens.issue();
        self.position_reads.insert(token, ticket);
        effects.push(QueryEffect::ResolvePosition {
            token,
            session_id,
            asset_id,
        });
    }

    fn on_position_loaded(
        &mut self,
        token: RequestToken,
        result: BackendResult<AssetQueryPositionResult>,
        effects: &mut Vec<QueryEffect>,
    ) {
        let Some(ticket) = self.position_reads.remove(&token) else {
            return;
        };
        if matches!(result, Ok(AssetQueryPositionResult::Stale)) {
            self.schedule_stale_refresh(effects);
        }
        self.emit(QueryEvent::PositionResolved {
            ticket,
            result: result.map_err(QueryError::Backend),
        });
    }

    fn read_id_range(
        &mut self,
        ticket: Ticket,
        start: usize,
        end: usize,
        effects: &mut Vec<QueryEffect>,
    ) {
        let from = start.min(end);
        if from >= self.total {
            self.emit(QueryEvent::IdRangeRead {
                ticket,
                result: Ok(Vec::new()),
            });
            return;
        }
        let to = start.max(end).min(self.total - 1);
        let Some(session_id) = self.session_id else {
            self.emit(QueryEvent::IdRangeRead {
                ticket,
                result: Err(QueryError::Cancelled),
            });
            return;
        };
        self.request_range_chunk(
            RangeRead {
                ticket,
                session_id,
                to,
                chunk_start: from,
                chunk_end: from,
                ids: Vec::with_capacity((to - from + 1).min(ID_RANGE_CHUNK)),
            },
            from,
            effects,
        );
    }

    fn request_range_chunk(
        &mut self,
        mut read: RangeRead,
        chunk_start: usize,
        effects: &mut Vec<QueryEffect>,
    ) {
        read.chunk_start = chunk_start;
        read.chunk_end = read.to.min(chunk_start.saturating_add(ID_RANGE_CHUNK - 1));
        let token = self.tokens.issue();
        effects.push(QueryEffect::LoadIdRange {
            token,
            session_id: read.session_id,
            start: read.chunk_start,
            end: read.chunk_end,
        });
        self.id_reads.insert(token, IdRead::Range(read));
    }

    fn keep_session_alive(
        &mut self,
        ticket: Ticket,
        session_id: u64,
        effects: &mut Vec<QueryEffect>,
    ) {
        if self.disposed || self.session_id != Some(session_id) || self.total == 0 {
            self.emit(QueryEvent::SessionKeptAlive {
                ticket,
                result: Ok(KeepAlive::Skipped),
            });
            return;
        }
        let token = self.tokens.issue();
        self.id_reads.insert(token, IdRead::KeepAlive { ticket });
        effects.push(QueryEffect::LoadIdRange {
            token,
            session_id,
            start: 0,
            end: 0,
        });
    }

    fn on_id_range_loaded(
        &mut self,
        token: RequestToken,
        result: BackendResult<AssetIdRange>,
        effects: &mut Vec<QueryEffect>,
    ) {
        let Some(read) = self.id_reads.remove(&token) else {
            return;
        };
        if matches!(result, Ok(AssetIdRange::Stale)) {
            self.schedule_stale_refresh(effects);
        }
        match read {
            IdRead::KeepAlive { ticket } => {
                let result = match result {
                    Ok(AssetIdRange::Ready(_)) => Ok(KeepAlive::Ready),
                    Ok(AssetIdRange::Stale) => Ok(KeepAlive::Stale),
                    Err(error) => Err(QueryError::Backend(error)),
                };
                self.emit(QueryEvent::SessionKeptAlive { ticket, result });
            }
            IdRead::Range(mut read) => {
                let failure = match result {
                    Ok(AssetIdRange::Ready(ids)) => {
                        if ids.len() != read.chunk_end - read.chunk_start + 1 {
                            QueryError::IncompleteRange
                        } else {
                            read.ids.extend(ids);
                            if read.chunk_end < read.to {
                                let next = read.chunk_end + 1;
                                self.request_range_chunk(read, next, effects);
                            } else {
                                self.emit(QueryEvent::IdRangeRead {
                                    ticket: read.ticket,
                                    result: Ok(read.ids),
                                });
                            }
                            return;
                        }
                    }
                    Ok(AssetIdRange::Stale) => QueryError::Cancelled,
                    Err(error) => QueryError::Backend(error),
                };
                self.emit(QueryEvent::IdRangeRead {
                    ticket: read.ticket,
                    result: Err(failure),
                });
            }
        }
    }

    /// Stale reads join one pending refresh.
    fn schedule_stale_refresh(&mut self, effects: &mut Vec<QueryEffect>) {
        if self.disposed || self.stale.pending.is_some() {
            return;
        }
        let token = self.tokens.issue();
        self.stale.pending = Some(PendingStale {
            timer: token,
            debounced: false,
        });
        effects.push(QueryEffect::StaleRefreshTimer {
            token,
            delay: STALE_REFRESH_DELAY,
        });
    }

    fn on_stale_refresh_due(&mut self, token: RequestToken, effects: &mut Vec<QueryEffect>) {
        match &mut self.stale.pending {
            Some(pending) if pending.timer == token => pending.debounced = true,
            // The timer of a refresh that an explicit one cancelled.
            _ => return,
        }
        if self.stale.cooldown.is_none() {
            self.run_stale_refresh(effects);
        }
    }

    fn on_stale_cooldown_ended(&mut self, token: RequestToken, effects: &mut Vec<QueryEffect>) {
        if self.stale.cooldown != Some(token) {
            return;
        }
        self.stale.cooldown = None;
        if self
            .stale
            .pending
            .as_ref()
            .is_some_and(|pending| pending.debounced)
        {
            self.run_stale_refresh(effects);
        }
    }

    fn run_stale_refresh(&mut self, effects: &mut Vec<QueryEffect>) {
        self.stale.pending = None;
        if self.disposed {
            return;
        }
        let token = self.tokens.issue();
        self.stale.cooldown = Some(token);
        effects.push(QueryEffect::StaleCooldownTimer {
            token,
            delay: STALE_REFRESH_MIN_INTERVAL,
        });
        self.refresh(effects);
    }
}

impl Model for AssetQuery {
    type Input = QueryInput;
    type Effect = QueryEffect;

    fn handle(&mut self, input: QueryInput) -> Vec<QueryEffect> {
        let mut effects = Vec::new();
        match input {
            QueryInput::Refresh => self.refresh(&mut effects),
            QueryInput::ApplyFilters(filters) => {
                self.filters = filters;
                self.refresh(&mut effects);
            }
            QueryInput::SetViewport(viewport) => {
                self.viewport = viewport;
                self.demand_viewport(&mut effects);
            }
            QueryInput::RetryLoad => self.retry_load(&mut effects),
            QueryInput::Reset => self.reset(),
            QueryInput::Dispose => self.dispose(),
            QueryInput::PatchAssets(patch) => {
                self.patch_counter += 1;
                // Positions stay until the next query replaces the snapshot.
                self.cache.patch(&patch);
            }
            QueryInput::ReadAsset { ticket, index } => {
                self.read_asset(ticket, index, &mut effects);
            }
            QueryInput::RetryAsset { ticket, index } => {
                // Explicit recovery unblocks only its target, also an offscreen page.
                self.failed_pages.remove(&self.cache.page_offset(index));
                if self.failed_pages.is_empty() {
                    self.load_error = None;
                }
                self.read_asset(ticket, index, &mut effects);
            }
            QueryInput::ResolvePosition { ticket, asset_id } => {
                self.resolve_position(ticket, asset_id, &mut effects);
            }
            QueryInput::ReadIdRange { ticket, start, end } => {
                self.read_id_range(ticket, start, end, &mut effects);
            }
            QueryInput::KeepSessionAlive { ticket, session_id } => {
                self.keep_session_alive(ticket, session_id, &mut effects);
            }
            QueryInput::Started { generation, result } => {
                self.on_started(generation, result, &mut effects);
            }
            QueryInput::PageLoaded { token, result } => {
                self.on_page_loaded(token, result, &mut effects);
            }
            QueryInput::SectionsLoaded { token, result } => {
                self.on_sections_loaded(token, result, &mut effects);
            }
            QueryInput::PositionLoaded { token, result } => {
                self.on_position_loaded(token, result, &mut effects);
            }
            QueryInput::IdRangeLoaded { token, result } => {
                self.on_id_range_loaded(token, result, &mut effects);
            }
            QueryInput::StaleRefreshDue(token) => self.on_stale_refresh_due(token, &mut effects),
            QueryInput::StaleCooldownEnded(token) => {
                self.on_stale_cooldown_ended(token, &mut effects);
            }
        }
        effects
    }
}

/// Runs one effect of an [`AssetQuery`] through the backend port. `wrap` turns the
/// query's inputs into the inputs of the model that owns the inbox: the identity for a
/// query driven on its own, the wrapping variant for a parent model that embeds it.
pub fn run_query_effect<I: Send + 'static>(
    backend: &Arc<dyn Backend>,
    effect: QueryEffect,
    dispatch: &Dispatch<I>,
    wrap: fn(QueryInput) -> I,
) {
    let backend = Arc::clone(backend);
    match effect {
        QueryEffect::Start(request) => {
            let generation = request.generation;
            dispatch.call(
                "start_asset_query",
                move || backend.start_asset_query(request),
                move |result| wrap(QueryInput::Started { generation, result }),
            );
        }
        QueryEffect::LoadPage {
            token,
            session_id,
            offset,
            limit,
        } => dispatch.call(
            "get_asset_query_page",
            move || backend.get_asset_query_page(session_id, offset, limit),
            move |result| wrap(QueryInput::PageLoaded { token, result }),
        ),
        QueryEffect::LoadSections { token, session_id } => dispatch.call(
            "get_asset_query_sections",
            move || backend.get_asset_query_sections(session_id),
            move |result| wrap(QueryInput::SectionsLoaded { token, result }),
        ),
        QueryEffect::ResolvePosition {
            token,
            session_id,
            asset_id,
        } => dispatch.call(
            "get_asset_query_position",
            move || backend.get_asset_query_position(session_id, asset_id),
            move |result| wrap(QueryInput::PositionLoaded { token, result }),
        ),
        QueryEffect::LoadIdRange {
            token,
            session_id,
            start,
            end,
        } => dispatch.call(
            "get_asset_query_id_range",
            move || backend.get_asset_query_id_range(session_id, start, end),
            move |result| wrap(QueryInput::IdRangeLoaded { token, result }),
        ),
        QueryEffect::StaleRefreshTimer { token, delay } => {
            dispatch.after(delay, wrap(QueryInput::StaleRefreshDue(token)));
        }
        QueryEffect::StaleCooldownTimer { token, delay } => {
            dispatch.after(delay, wrap(QueryInput::StaleCooldownEnded(token)));
        }
    }
}

/// Runs the effects of a query model that is driven on its own.
pub struct QueryRunner {
    backend: Arc<dyn Backend>,
}

impl QueryRunner {
    pub fn new(backend: Arc<dyn Backend>) -> Self {
        Self { backend }
    }
}

impl Runner<AssetQuery> for QueryRunner {
    fn run(&mut self, effect: QueryEffect, dispatch: &Dispatch<QueryInput>) {
        run_query_effect(&self.backend, effect, dispatch, std::convert::identity);
    }
}
