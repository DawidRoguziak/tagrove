//! The thumbnail pipeline: what the gallery's `<img src="thumb://...">` tiles and
//! `ThumbnailImage` do in the Solid frontend, as a model.
//!
//! A view asks for the thumbnail of an asset version in a [`ThumbnailMode`] and
//! says when it no longer wants it. The pipeline requests it from the backend
//! ([`Backend::request_thumbnail`]), retries a failed load with the backoff of
//! `src/ui/controls/ThumbnailImage.tsx`, and reports the JPEG bytes with their
//! version, or the failure, as [`ThumbnailEvent`]s. Rules it keeps
//! (`docs/subsystems/thumbnails.md`, "Frontend tiles"):
//!
//! - A failed load (`NotFound` or `Unavailable`, the 404 and 503 of `thumb://`)
//!   is retried after 500 ms and 2 s. After these quick retries the tile shows its
//!   placeholder and `Failed` is reported once; retries go on after 4 s, 8 s, 16 s
//!   and then every 30 s while the view still wants the picture. `Unavailable` is
//!   also the answer while the backend is not ready, so a tile asked for during
//!   startup shows its picture once the core has started.
//! - A cached-only request (fast-scroll pause) never renders and is not retried:
//!   its miss is final until the view asks again in a stronger mode.
//! - Modes only get stronger for an entry: cached only → prefetch → visible. A
//!   stronger request starts over with a new backend request (which moves a queued
//!   render to the front); an equal or weaker one changes nothing.
//! - Cancelling drops the entry. Its request is not sent when it has not started
//!   yet, and a reply or timer that still arrives is ignored by its token. The core
//!   cannot take a queued render back; a render that already started is published
//!   and its bytes are discarded here.
//!
//! [`ThumbnailFetcher`] runs the effects. It sends at most
//! [`MAX_CONCURRENT_FETCHES`] requests at once and keeps the rest in its own
//! queue, so requests that hang on an unreachable mount cannot take the threads
//! the other backend calls need.
//!
//! One pipeline serves one surface with at most one tile per asset version (the
//! gallery grid); decoded pictures and their cache belong to the adapter.

use std::{
    collections::{HashMap, VecDeque},
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::{
    backend::{Backend, ThumbnailImage, ThumbnailReply, ThumbnailRequest, ThumbnailResponder},
    model::{Dispatch, Model, RequestToken, Runner, Sender, Tokens},
};

/// Delays before the quick retries of a failed load.
pub const RETRY_DELAYS: [Duration; 2] = [Duration::from_millis(500), Duration::from_millis(2000)];
/// First delay of the slower retries that follow the quick ones; it doubles each
/// time.
pub const RECOVERY_DELAY: Duration = Duration::from_millis(4000);
/// Cap of the slower retries' delay.
pub const RECOVERY_MAX_DELAY: Duration = Duration::from_millis(30_000);
/// Largest retry number of a request; later retries cycle back to 1.
pub const MAX_RETRY_ATTEMPT: u32 = 999;

/// Delay before the retry that follows failed attempt `count` (0 is the first
/// load).
pub fn retry_delay(count: u32) -> Duration {
    if let Some(quick) = RETRY_DELAYS.get(count as usize) {
        return *quick;
    }
    let doublings = (count - RETRY_DELAYS.len() as u32).min(16);
    (RECOVERY_DELAY * 2_u32.pow(doublings)).min(RECOVERY_MAX_DELAY)
}

/// The retry number request `attempt` carries (0 for the first load, then 1 to
/// [`MAX_RETRY_ATTEMPT`], cycling).
pub fn retry_number(attempt: u32) -> u32 {
    if attempt == 0 {
        0
    } else {
        (attempt - 1) % MAX_RETRY_ATTEMPT + 1
    }
}

/// The picture a tile shows: an asset and the source version of its summary.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ThumbnailKey {
    pub asset_id: i64,
    /// `AssetSummary::thumb_version`.
    pub version: String,
}

impl ThumbnailKey {
    pub fn new(asset_id: i64, version: impl Into<String>) -> Self {
        Self {
            asset_id,
            version: version.into(),
        }
    }
}

/// How a thumbnail may be produced, weakest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ThumbnailMode {
    /// Serve an existing file only (`c=1`): a tile first shown during a
    /// fast-scroll pause.
    CachedOnly,
    /// Render behind every visible tile (`p=1`): an overscan tile.
    Prefetch,
    /// Render ahead of everything else: a tile in view.
    Visible,
}

#[derive(Debug)]
pub enum ThumbnailInput {
    /// The view wants this picture. Repeating it is harmless.
    Request {
        key: ThumbnailKey,
        mode: ThumbnailMode,
    },
    /// The view no longer wants this picture (its tile is gone).
    Cancel { key: ThumbnailKey },
    /// The backend answered the request started with `token`.
    Reply {
        token: RequestToken,
        reply: ThumbnailReply,
    },
    /// The retry timer started with `token` fired.
    RetryDue { token: RequestToken },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThumbnailEffect {
    /// Ask the backend; the answer comes back as `Reply` with this token.
    Fetch {
        token: RequestToken,
        request: ThumbnailRequest,
    },
    /// The answer to this request is no longer wanted.
    Cancel { token: RequestToken },
    /// Send `RetryDue` with this token after `delay`.
    Retry {
        token: RequestToken,
        delay: Duration,
    },
}

/// What the pipeline reports to its view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThumbnailEvent {
    /// The picture arrived. `current` is `false` when the bytes belong to a
    /// newer source version than the one asked for: they are shown, but must not
    /// be cached under the requested version.
    Ready {
        key: ThumbnailKey,
        image: ThumbnailImage,
        current: bool,
    },
    /// The load and its quick retries failed, or it failed for good. Reported
    /// once per showing; slower retries may still deliver `Ready` later.
    Failed { key: ThumbnailKey },
}

/// What a tile without its picture shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbnailStatus {
    /// A request is on its way or a quick retry is due: the spinner.
    Loading,
    /// The quick retries failed and a slower retry is due: the placeholder.
    Placeholder,
    /// Failed for good in this mode: the placeholder, and nothing is pending.
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Waiting for the backend's answer to `token`.
    Loading,
    /// Waiting for the retry timer of `token`.
    Waiting,
    /// Nothing is pending.
    Failed,
}

#[derive(Debug)]
struct Entry {
    mode: ThumbnailMode,
    /// Index of the current attempt: 0 for the first load.
    attempt: u32,
    phase: Phase,
    /// The request or timer this entry waits for.
    token: RequestToken,
}

#[derive(Debug, Default)]
pub struct ThumbnailPipeline {
    tokens: Tokens,
    entries: HashMap<ThumbnailKey, Entry>,
    /// The key each awaited token belongs to.
    awaited: HashMap<RequestToken, ThumbnailKey>,
    events: VecDeque<ThumbnailEvent>,
}

impl ThumbnailPipeline {
    pub fn new() -> Self {
        Self::default()
    }

    /// What the tile of `key` shows while it has no picture; `None` when the
    /// pipeline holds nothing for it (never asked, delivered or cancelled).
    pub fn status(&self, key: &ThumbnailKey) -> Option<ThumbnailStatus> {
        let entry = self.entries.get(key)?;
        Some(match entry.phase {
            Phase::Failed => ThumbnailStatus::Failed,
            Phase::Waiting if entry.attempt as usize >= RETRY_DELAYS.len() => {
                ThumbnailStatus::Placeholder
            }
            Phase::Loading | Phase::Waiting => ThumbnailStatus::Loading,
        })
    }

    /// The mode the entry of `key` was last started in.
    pub fn mode(&self, key: &ThumbnailKey) -> Option<ThumbnailMode> {
        self.entries.get(key).map(|entry| entry.mode)
    }

    /// Pictures the pipeline is still trying to get or has given up on.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Requests whose answer the pipeline is waiting for.
    pub fn in_flight(&self) -> usize {
        self.entries
            .values()
            .filter(|entry| entry.phase == Phase::Loading)
            .count()
    }

    /// The events since the last call, oldest first.
    pub fn take_events(&mut self) -> Vec<ThumbnailEvent> {
        self.events.drain(..).collect()
    }

    fn request(key: &ThumbnailKey, mode: ThumbnailMode, attempt: u32) -> ThumbnailRequest {
        ThumbnailRequest {
            asset_id: key.asset_id,
            version: key.version.clone(),
            cached_only: mode == ThumbnailMode::CachedOnly,
            prefetch: mode == ThumbnailMode::Prefetch,
            retry: retry_number(attempt),
        }
    }

    /// Starts over for `key` in `mode`: a new showing with its own retry budget.
    fn start(&mut self, key: ThumbnailKey, mode: ThumbnailMode) -> ThumbnailEffect {
        let token = self.tokens.issue();
        let request = Self::request(&key, mode, 0);
        self.awaited.insert(token, key.clone());
        self.entries.insert(
            key,
            Entry {
                mode,
                attempt: 0,
                phase: Phase::Loading,
                token,
            },
        );
        ThumbnailEffect::Fetch { token, request }
    }

    /// Forgets what the entry of `key` waits for; returns the effect that tells
    /// the runner when a request was in flight.
    fn abandon(&mut self, key: &ThumbnailKey) -> Option<ThumbnailEffect> {
        let entry = self.entries.remove(key)?;
        self.awaited.remove(&entry.token);
        (entry.phase == Phase::Loading).then_some(ThumbnailEffect::Cancel { token: entry.token })
    }

    fn on_request(&mut self, key: ThumbnailKey, mode: ThumbnailMode) -> Vec<ThumbnailEffect> {
        match self.entries.get(&key) {
            Some(entry) if mode <= entry.mode => Vec::new(),
            Some(_) => {
                let mut effects: Vec<_> = self.abandon(&key).into_iter().collect();
                effects.push(self.start(key, mode));
                effects
            }
            None => vec![self.start(key, mode)],
        }
    }

    /// The key that waits for `token` in `phase`, no longer awaiting it. `None`
    /// for the token of a request or timer that was cancelled, superseded or
    /// already handled: its input is dropped without touching the state.
    fn accept(&mut self, token: RequestToken, phase: Phase) -> Option<ThumbnailKey> {
        let key = self.awaited.get(&token)?;
        let entry = self.entries.get(key)?;
        if entry.token != token || entry.phase != phase {
            return None;
        }
        self.awaited.remove(&token)
    }

    fn on_reply(&mut self, token: RequestToken, reply: ThumbnailReply) -> Vec<ThumbnailEffect> {
        let Some(key) = self.accept(token, Phase::Loading) else {
            return Vec::new();
        };
        let Some(entry) = self.entries.get_mut(&key) else {
            return Vec::new();
        };
        let retry = match reply {
            ThumbnailReply::Ready(image) => {
                self.entries.remove(&key);
                let current = image.version == key.version;
                self.events.push_back(ThumbnailEvent::Ready {
                    key,
                    image,
                    current,
                });
                return Vec::new();
            }
            ThumbnailReply::BadRequest => false,
            ThumbnailReply::NotFound | ThumbnailReply::Unavailable => {
                entry.mode != ThumbnailMode::CachedOnly
            }
        };
        if !retry {
            entry.phase = Phase::Failed;
            self.events.push_back(ThumbnailEvent::Failed { key });
            return Vec::new();
        }
        let count = entry.attempt;
        let token = self.tokens.issue();
        entry.phase = Phase::Waiting;
        entry.token = token;
        self.awaited.insert(token, key.clone());
        if count as usize == RETRY_DELAYS.len() {
            self.events.push_back(ThumbnailEvent::Failed { key });
        }
        vec![ThumbnailEffect::Retry {
            token,
            delay: retry_delay(count),
        }]
    }

    fn on_retry_due(&mut self, token: RequestToken) -> Vec<ThumbnailEffect> {
        let Some(key) = self.accept(token, Phase::Waiting) else {
            return Vec::new();
        };
        let Some(entry) = self.entries.get_mut(&key) else {
            return Vec::new();
        };
        let token = self.tokens.issue();
        entry.attempt += 1;
        entry.phase = Phase::Loading;
        entry.token = token;
        let request = Self::request(&key, entry.mode, entry.attempt);
        self.awaited.insert(token, key);
        vec![ThumbnailEffect::Fetch { token, request }]
    }
}

impl Model for ThumbnailPipeline {
    type Input = ThumbnailInput;
    type Effect = ThumbnailEffect;

    fn handle(&mut self, input: ThumbnailInput) -> Vec<ThumbnailEffect> {
        match input {
            ThumbnailInput::Request { key, mode } => self.on_request(key, mode),
            ThumbnailInput::Cancel { key } => self.abandon(&key).into_iter().collect(),
            ThumbnailInput::Reply { token, reply } => self.on_reply(token, reply),
            ThumbnailInput::RetryDue { token } => self.on_retry_due(token),
        }
    }
}

/// Most thumbnail requests that are inside the backend at once.
///
/// A request normally returns as soon as it has served a file or enqueued a
/// render, so this is no brake on a gallery screen. It is the bound of what a
/// source check that hangs can take: such a request keeps its thread until the mount
/// answers, and without a bound a screen of tiles from that mount would take
/// every worker of the executor, which queries, writes and timers share.
pub const MAX_CONCURRENT_FETCHES: usize = 16;

// Stuck fetches must leave most of the application's pool to everything else.
const _: () = assert!(MAX_CONCURRENT_FETCHES * 2 <= crate::exec::thread::MAX_THREADS);

/// Runs the pipeline's effects against a backend.
///
/// Requests wait in the fetcher's own queue and at most
/// [`MAX_CONCURRENT_FETCHES`] executor tasks (lanes) send them, oldest first, so
/// thumbnail requests never occupy more threads than that however many tiles
/// ask. A request cancelled before a lane took it is never sent, which is what
/// saves work while tiles scroll past; the answer to one that was sent is
/// dropped here instead of being queued for the model.
pub struct ThumbnailFetcher {
    backend: Arc<dyn Backend>,
    flights: Arc<Mutex<Flights>>,
    max_lanes: usize,
}

#[derive(Default)]
struct Flights {
    /// Requests without an answer yet, and whether each was cancelled.
    open: HashMap<RequestToken, bool>,
    /// Open requests no lane has taken yet, oldest first.
    waiting: VecDeque<(RequestToken, ThumbnailRequest)>,
    /// Lanes handed to the executor that have not ended.
    lanes: usize,
}

impl ThumbnailFetcher {
    pub fn new(backend: Arc<dyn Backend>) -> Self {
        Self::with_limit(backend, MAX_CONCURRENT_FETCHES)
    }

    /// A fetcher that sends at most `max_concurrent` requests (at least one) at
    /// once.
    pub fn with_limit(backend: Arc<dyn Backend>, max_concurrent: usize) -> Self {
        Self {
            backend,
            flights: Arc::new(Mutex::new(Flights::default())),
            max_lanes: max_concurrent.max(1),
        }
    }

    /// Requests without an answer yet: waiting for a lane, inside the backend,
    /// or waiting for their render.
    pub fn open_requests(&self) -> usize {
        flights(&self.flights).open.len()
    }
}

fn flights(flights: &Mutex<Flights>) -> std::sync::MutexGuard<'_, Flights> {
    flights
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// One executor task that sends waiting requests until none is left.
struct Lane {
    flights: Arc<Mutex<Flights>>,
    /// Whether the lane already gave its place back.
    ended: bool,
}

impl Lane {
    /// The next request to send. `None` ends the lane under the same lock that
    /// found the queue empty, so a request queued afterwards starts a new one.
    fn next(&mut self) -> Option<(RequestToken, ThumbnailRequest)> {
        let mut flights = flights(&self.flights);
        let next = flights.waiting.pop_front();
        if next.is_none() {
            flights.lanes -= 1;
            self.ended = true;
        }
        next
    }

    fn run(mut self, backend: &dyn Backend, sender: &Sender<ThumbnailInput>) {
        while let Some((token, request)) = self.next() {
            let answered = Arc::clone(&self.flights);
            let sender = sender.clone();
            let responder = ThumbnailResponder::new(move |reply| {
                let cancelled = flights(&answered).open.remove(&token).unwrap_or(true);
                if !cancelled {
                    sender.send(ThumbnailInput::Reply { token, reply });
                }
            });
            // A panicking request must not end the lane with requests still
            // waiting; its dropped responder answers `Unavailable`.
            let _ = catch_unwind(AssertUnwindSafe(|| {
                backend.request_thumbnail(request, responder);
            }));
        }
    }
}

impl Drop for Lane {
    /// A lane the executor dropped before it ran gives its place back; what
    /// still waits is sent by the lane the next request starts.
    fn drop(&mut self) {
        if !self.ended {
            flights(&self.flights).lanes -= 1;
        }
    }
}

impl Runner<ThumbnailPipeline> for ThumbnailFetcher {
    fn run(&mut self, effect: ThumbnailEffect, dispatch: &Dispatch<ThumbnailInput>) {
        match effect {
            ThumbnailEffect::Fetch { token, request } => {
                {
                    let mut flights = flights(&self.flights);
                    flights.open.insert(token, false);
                    flights.waiting.push_back((token, request));
                    if flights.lanes >= self.max_lanes {
                        // Every lane is busy; one of them takes it when it is
                        // back from the backend.
                        return;
                    }
                    flights.lanes += 1;
                }
                // Spawned outside the lock: an executor that shut down drops
                // the lane at once, and its drop takes the lock.
                let lane = Lane {
                    flights: Arc::clone(&self.flights),
                    ended: false,
                };
                let backend = Arc::clone(&self.backend);
                dispatch.spawn(move |sender| lane.run(backend.as_ref(), &sender));
            }
            ThumbnailEffect::Cancel { token } => {
                let mut flights = flights(&self.flights);
                let waiting = flights
                    .waiting
                    .iter()
                    .position(|(waiting, _)| *waiting == token);
                if let Some(index) = waiting {
                    // Not sent yet: it never reaches the backend.
                    flights.waiting.remove(index);
                    flights.open.remove(&token);
                } else if let Some(cancelled) = flights.open.get_mut(&token) {
                    *cancelled = true;
                }
            }
            ThumbnailEffect::Retry { token, delay } => {
                dispatch.after(delay, ThumbnailInput::RetryDue { token });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{mpsc, Arc},
        time::Duration,
    };

    use super::{
        retry_delay, retry_number, ThumbnailEffect, ThumbnailEvent, ThumbnailFetcher,
        ThumbnailInput, ThumbnailKey, ThumbnailMode, ThumbnailPipeline, ThumbnailStatus,
        MAX_CONCURRENT_FETCHES,
    };
    use crate::{
        backend::{Backend, FakeBackend, ThumbnailImage, ThumbnailReply, ThumbnailRequest},
        exec::{Executor, ManualExecutor, ThreadExecutor},
        model::{Driver, Model, RequestToken},
    };

    const VERSION: &str = "0123456789abcdef";

    fn key(asset_id: i64) -> ThumbnailKey {
        ThumbnailKey::new(asset_id, VERSION)
    }

    fn image(version: &str) -> ThumbnailImage {
        ThumbnailImage {
            bytes: vec![0xff, 0xd8, 0xff],
            version: version.to_owned(),
        }
    }

    fn request(mode: ThumbnailMode) -> ThumbnailInput {
        ThumbnailInput::Request { key: key(1), mode }
    }

    fn fetch_token(effects: &[ThumbnailEffect]) -> RequestToken {
        effects
            .iter()
            .find_map(|effect| match effect {
                ThumbnailEffect::Fetch { token, .. } => Some(*token),
                _ => None,
            })
            .expect("a fetch effect")
    }

    fn retry_token(effects: &[ThumbnailEffect]) -> (RequestToken, Duration) {
        match effects {
            [ThumbnailEffect::Retry { token, delay }] => (*token, *delay),
            other => panic!("expected one retry effect, got {other:?}"),
        }
    }

    #[test]
    fn retry_delays_follow_the_quick_budget_then_a_capped_backoff() {
        let delays: Vec<u64> = (0..8)
            .map(|count| retry_delay(count).as_millis() as u64)
            .collect();

        assert_eq!(
            delays,
            [500, 2000, 4000, 8000, 16_000, 30_000, 30_000, 30_000]
        );
        assert_eq!(retry_delay(u32::MAX), Duration::from_secs(30));
    }

    #[test]
    fn retry_numbers_cycle_through_three_digits() {
        assert_eq!(retry_number(0), 0);
        assert_eq!(retry_number(1), 1);
        assert_eq!(retry_number(999), 999);
        assert_eq!(retry_number(1000), 1);
        assert_eq!(retry_number(1998), 999);
    }

    #[test]
    fn a_request_maps_its_mode_to_the_flags_of_the_thumb_url() {
        for (mode, cached_only, prefetch) in [
            (ThumbnailMode::Visible, false, false),
            (ThumbnailMode::Prefetch, false, true),
            (ThumbnailMode::CachedOnly, true, false),
        ] {
            let mut pipeline = ThumbnailPipeline::new();

            let effects = pipeline.handle(request(mode));

            assert_eq!(
                effects,
                [ThumbnailEffect::Fetch {
                    token: fetch_token(&effects),
                    request: ThumbnailRequest {
                        asset_id: 1,
                        version: VERSION.into(),
                        cached_only,
                        prefetch,
                        retry: 0,
                    },
                }]
            );
            assert_eq!(pipeline.status(&key(1)), Some(ThumbnailStatus::Loading));
            assert_eq!(pipeline.in_flight(), 1);
        }
    }

    #[test]
    fn a_ready_reply_is_reported_with_its_bytes_and_ends_the_entry() {
        let mut pipeline = ThumbnailPipeline::new();
        let token = fetch_token(&pipeline.handle(request(ThumbnailMode::Visible)));

        let effects = pipeline.handle(ThumbnailInput::Reply {
            token,
            reply: ThumbnailReply::Ready(image(VERSION)),
        });

        assert!(effects.is_empty());
        assert_eq!(
            pipeline.take_events(),
            [ThumbnailEvent::Ready {
                key: key(1),
                image: image(VERSION),
                current: true,
            }]
        );
        assert_eq!(pipeline.status(&key(1)), None);
        assert!(pipeline.is_empty());
        assert!(pipeline.take_events().is_empty());
    }

    #[test]
    fn bytes_of_a_newer_source_version_are_reported_as_not_current() {
        let mut pipeline = ThumbnailPipeline::new();
        let token = fetch_token(&pipeline.handle(request(ThumbnailMode::Visible)));

        pipeline.handle(ThumbnailInput::Reply {
            token,
            reply: ThumbnailReply::Ready(image("fedcba9876543210")),
        });

        assert!(matches!(
            pipeline.take_events()[..],
            [ThumbnailEvent::Ready { current: false, .. }]
        ));
    }

    #[test]
    fn a_failed_load_is_retried_with_backoff_and_reports_failure_once() {
        let mut pipeline = ThumbnailPipeline::new();
        let mut token = fetch_token(&pipeline.handle(request(ThumbnailMode::Visible)));
        let mut delays = Vec::new();
        let mut retries = Vec::new();
        let mut statuses = Vec::new();
        let mut failures = 0;

        for round in 0..6 {
            // 404 and 503 take the same path.
            let reply = if round % 2 == 0 {
                ThumbnailReply::NotFound
            } else {
                ThumbnailReply::Unavailable
            };
            let (timer, delay) =
                retry_token(&pipeline.handle(ThumbnailInput::Reply { token, reply }));
            delays.push(delay.as_millis() as u64);
            statuses.push(pipeline.status(&key(1)).unwrap());
            failures += pipeline.take_events().len();
            let effects = pipeline.handle(ThumbnailInput::RetryDue { token: timer });
            let [ThumbnailEffect::Fetch {
                token: next,
                request,
            }] = &effects[..]
            else {
                panic!("expected one fetch effect, got {effects:?}");
            };
            retries.push(request.retry);
            assert_eq!(pipeline.status(&key(1)), Some(ThumbnailStatus::Loading));
            token = *next;
        }

        assert_eq!(delays, [500, 2000, 4000, 8000, 16_000, 30_000]);
        assert_eq!(retries, [1, 2, 3, 4, 5, 6]);
        assert_eq!(
            statuses,
            [
                ThumbnailStatus::Loading,
                ThumbnailStatus::Loading,
                ThumbnailStatus::Placeholder,
                ThumbnailStatus::Placeholder,
                ThumbnailStatus::Placeholder,
                ThumbnailStatus::Placeholder,
            ]
        );
        assert_eq!(
            failures, 1,
            "Failed is reported once, after the quick retries"
        );

        // A later retry still delivers the picture.
        pipeline.handle(ThumbnailInput::Reply {
            token,
            reply: ThumbnailReply::Ready(image(VERSION)),
        });
        assert!(matches!(
            pipeline.take_events()[..],
            [ThumbnailEvent::Ready { current: true, .. }]
        ));
    }

    #[test]
    fn a_cached_only_miss_is_final_until_a_stronger_request() {
        let mut pipeline = ThumbnailPipeline::new();
        let token = fetch_token(&pipeline.handle(request(ThumbnailMode::CachedOnly)));

        let effects = pipeline.handle(ThumbnailInput::Reply {
            token,
            reply: ThumbnailReply::NotFound,
        });

        assert!(effects.is_empty(), "a cached-only miss starts no retry");
        assert_eq!(
            pipeline.take_events(),
            [ThumbnailEvent::Failed { key: key(1) }]
        );
        assert_eq!(pipeline.status(&key(1)), Some(ThumbnailStatus::Failed));
        assert!(pipeline
            .handle(request(ThumbnailMode::CachedOnly))
            .is_empty());

        // The pause ended: the tile asks for a render.
        let effects = pipeline.handle(request(ThumbnailMode::Prefetch));
        assert!(matches!(
            &effects[..],
            [ThumbnailEffect::Fetch { request, .. }] if request.prefetch && !request.cached_only && request.retry == 0
        ));
        assert_eq!(pipeline.status(&key(1)), Some(ThumbnailStatus::Loading));
    }

    #[test]
    fn a_bad_request_is_final() {
        let mut pipeline = ThumbnailPipeline::new();
        let token = fetch_token(&pipeline.handle(request(ThumbnailMode::Visible)));

        let effects = pipeline.handle(ThumbnailInput::Reply {
            token,
            reply: ThumbnailReply::BadRequest,
        });

        assert!(effects.is_empty());
        assert_eq!(pipeline.status(&key(1)), Some(ThumbnailStatus::Failed));
        assert_eq!(
            pipeline.take_events(),
            [ThumbnailEvent::Failed { key: key(1) }]
        );
    }

    #[test]
    fn repeating_a_request_or_asking_in_a_weaker_mode_changes_nothing() {
        let mut pipeline = ThumbnailPipeline::new();
        pipeline.handle(request(ThumbnailMode::Visible));

        assert!(pipeline.handle(request(ThumbnailMode::Visible)).is_empty());
        assert!(pipeline.handle(request(ThumbnailMode::Prefetch)).is_empty());
        assert!(pipeline
            .handle(request(ThumbnailMode::CachedOnly))
            .is_empty());
        assert_eq!(pipeline.mode(&key(1)), Some(ThumbnailMode::Visible));
        assert_eq!(pipeline.in_flight(), 1);
    }

    #[test]
    fn a_visible_tile_drops_its_pending_prefetch_and_the_old_reply_is_ignored() {
        let mut pipeline = ThumbnailPipeline::new();
        let prefetch = fetch_token(&pipeline.handle(request(ThumbnailMode::Prefetch)));

        let effects = pipeline.handle(request(ThumbnailMode::Visible));

        let visible = fetch_token(&effects);
        assert_eq!(effects[0], ThumbnailEffect::Cancel { token: prefetch });
        assert!(matches!(
            &effects[1],
            ThumbnailEffect::Fetch { request, .. } if !request.prefetch && request.retry == 0
        ));
        assert_ne!(visible, prefetch);

        // The prefetch answers late with a failure: nothing changes.
        assert!(pipeline
            .handle(ThumbnailInput::Reply {
                token: prefetch,
                reply: ThumbnailReply::NotFound,
            })
            .is_empty());
        assert_eq!(pipeline.status(&key(1)), Some(ThumbnailStatus::Loading));
        assert!(pipeline.take_events().is_empty());

        pipeline.handle(ThumbnailInput::Reply {
            token: visible,
            reply: ThumbnailReply::Ready(image(VERSION)),
        });
        assert_eq!(pipeline.take_events().len(), 1);
    }

    #[test]
    fn a_stronger_request_during_a_retry_wait_starts_over_and_the_old_timer_is_ignored() {
        let mut pipeline = ThumbnailPipeline::new();
        let token = fetch_token(&pipeline.handle(request(ThumbnailMode::Prefetch)));
        let (timer, _) = retry_token(&pipeline.handle(ThumbnailInput::Reply {
            token,
            reply: ThumbnailReply::Unavailable,
        }));

        let effects = pipeline.handle(request(ThumbnailMode::Visible));

        assert!(
            matches!(&effects[..], [ThumbnailEffect::Fetch { request, .. }] if request.retry == 0),
            "no request was in flight, so nothing is cancelled: {effects:?}"
        );
        assert!(pipeline
            .handle(ThumbnailInput::RetryDue { token: timer })
            .is_empty());
        assert_eq!(pipeline.in_flight(), 1);
    }

    #[test]
    fn a_cancelled_request_is_forgotten_and_its_late_reply_changes_nothing() {
        let mut pipeline = ThumbnailPipeline::new();
        let token = fetch_token(&pipeline.handle(request(ThumbnailMode::Visible)));

        let effects = pipeline.handle(ThumbnailInput::Cancel { key: key(1) });

        assert_eq!(effects, [ThumbnailEffect::Cancel { token }]);
        assert!(pipeline.is_empty());
        assert!(pipeline
            .handle(ThumbnailInput::Reply {
                token,
                reply: ThumbnailReply::Ready(image(VERSION)),
            })
            .is_empty());
        assert!(pipeline.take_events().is_empty());
        assert!(pipeline.is_empty());
        assert!(pipeline
            .handle(ThumbnailInput::Cancel { key: key(1) })
            .is_empty());
    }

    #[test]
    fn a_picture_that_comes_back_after_a_cancel_is_requested_again() {
        let mut pipeline = ThumbnailPipeline::new();
        let first = fetch_token(&pipeline.handle(request(ThumbnailMode::Visible)));
        let (timer, _) = retry_token(&pipeline.handle(ThumbnailInput::Reply {
            token: first,
            reply: ThumbnailReply::NotFound,
        }));
        pipeline.handle(ThumbnailInput::Cancel { key: key(1) });

        let effects = pipeline.handle(request(ThumbnailMode::Visible));

        let second = fetch_token(&effects);
        assert!(matches!(
            &effects[..],
            [ThumbnailEffect::Fetch { request, .. }] if request.retry == 0
        ));
        // The timer of the first showing fires into the second: ignored.
        assert!(pipeline
            .handle(ThumbnailInput::RetryDue { token: timer })
            .is_empty());
        assert_eq!(pipeline.in_flight(), 1);
        assert!(second > timer);
    }

    #[test]
    fn a_reply_is_accepted_once() {
        let mut pipeline = ThumbnailPipeline::new();
        let token = fetch_token(&pipeline.handle(request(ThumbnailMode::Visible)));
        pipeline.handle(ThumbnailInput::Reply {
            token,
            reply: ThumbnailReply::NotFound,
        });
        pipeline.take_events();

        assert!(pipeline
            .handle(ThumbnailInput::Reply {
                token,
                reply: ThumbnailReply::Ready(image(VERSION)),
            })
            .is_empty());
        assert!(pipeline.take_events().is_empty());
        assert_eq!(pipeline.status(&key(1)), Some(ThumbnailStatus::Loading));
    }

    #[test]
    fn versions_of_one_asset_are_separate_pictures() {
        let mut pipeline = ThumbnailPipeline::new();
        let old = fetch_token(&pipeline.handle(request(ThumbnailMode::Visible)));
        let newer = ThumbnailKey::new(1, "fedcba9876543210");
        let new = fetch_token(&pipeline.handle(ThumbnailInput::Request {
            key: newer.clone(),
            mode: ThumbnailMode::Visible,
        }));

        pipeline.handle(ThumbnailInput::Cancel { key: key(1) });
        pipeline.handle(ThumbnailInput::Reply {
            token: old,
            reply: ThumbnailReply::Ready(image(VERSION)),
        });
        pipeline.handle(ThumbnailInput::Reply {
            token: new,
            reply: ThumbnailReply::Ready(image("fedcba9876543210")),
        });

        assert_eq!(
            pipeline.take_events(),
            [ThumbnailEvent::Ready {
                key: newer,
                image: image("fedcba9876543210"),
                current: true,
            }]
        );
    }

    fn driver(
        fake: &Arc<FakeBackend>,
        executor: &Arc<ManualExecutor>,
    ) -> Driver<ThumbnailPipeline, ThumbnailFetcher> {
        Driver::new(
            ThumbnailPipeline::new(),
            ThumbnailFetcher::new(Arc::clone(fake) as Arc<dyn Backend>),
            Arc::clone(executor) as Arc<dyn Executor>,
        )
    }

    fn want(asset_id: i64, mode: ThumbnailMode) -> ThumbnailInput {
        ThumbnailInput::Request {
            key: key(asset_id),
            mode,
        }
    }

    #[test]
    fn replies_that_arrive_out_of_order_reach_their_own_tiles() {
        let fake = Arc::new(FakeBackend::new());
        let executor = Arc::new(ManualExecutor::new());
        let mut driver = driver(&fake, &executor);
        driver.handle(want(1, ThumbnailMode::Visible));
        driver.handle(want(2, ThumbnailMode::Visible));
        executor.run_all();
        assert_eq!(fake.held_thumbnails().len(), 2);

        fake.answer_thumbnail(1, ThumbnailReply::Ready(image(VERSION)));
        fake.answer_thumbnail(0, ThumbnailReply::NotFound);
        assert_eq!(driver.pump(), 2);

        assert_eq!(
            driver.model_mut().take_events(),
            [ThumbnailEvent::Ready {
                key: key(2),
                image: image(VERSION),
                current: true,
            }]
        );
        assert_eq!(
            driver.model().status(&key(1)),
            Some(ThumbnailStatus::Loading)
        );
        assert_eq!(driver.model().status(&key(2)), None);
    }

    #[test]
    fn a_request_cancelled_before_it_started_never_reaches_the_backend() {
        let fake = Arc::new(FakeBackend::new());
        let executor = Arc::new(ManualExecutor::new());
        let mut driver = driver(&fake, &executor);
        driver.handle(want(1, ThumbnailMode::Visible));
        driver.handle(want(2, ThumbnailMode::Visible));

        driver.handle(ThumbnailInput::Cancel { key: key(1) });
        executor.run_all();

        assert_eq!(
            fake.held_thumbnails()
                .iter()
                .map(|request| request.asset_id)
                .collect::<Vec<_>>(),
            [2]
        );
        assert_eq!(driver.pump(), 0);
    }

    #[test]
    fn the_answer_to_a_request_cancelled_in_flight_is_dropped_before_the_inbox() {
        let fake = Arc::new(FakeBackend::new());
        let executor = Arc::new(ManualExecutor::new());
        let mut driver = driver(&fake, &executor);
        driver.handle(want(1, ThumbnailMode::Visible));
        executor.run_all();

        driver.handle(ThumbnailInput::Cancel { key: key(1) });
        fake.answer_thumbnail(0, ThumbnailReply::Ready(image(VERSION)));

        assert_eq!(driver.pump(), 0);
        assert!(driver.model_mut().take_events().is_empty());
        assert!(driver.model().is_empty());
    }

    #[test]
    fn a_tile_asked_for_before_the_backend_is_ready_shows_its_picture_after_the_retry() {
        let fake = Arc::new(FakeBackend::new());
        let executor = Arc::new(ManualExecutor::new());
        let mut driver = driver(&fake, &executor);
        let ready = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let started = Arc::clone(&ready);
        fake.on_request_thumbnail(move |_, responder| {
            responder.respond(if started.load(std::sync::atomic::Ordering::SeqCst) {
                ThumbnailReply::Ready(image(VERSION))
            } else {
                ThumbnailReply::Unavailable
            });
        });

        driver.handle(want(1, ThumbnailMode::Visible));
        executor.run_all();
        driver.pump();
        assert_eq!(
            driver.model().status(&key(1)),
            Some(ThumbnailStatus::Loading)
        );
        assert_eq!(executor.next_timer_in(), Some(Duration::from_millis(500)));

        ready.store(true, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(executor.advance_and_run(Duration::from_millis(499)), 0);
        executor.advance_and_run(Duration::from_millis(1));
        driver.pump();
        executor.run_all();
        driver.pump();

        assert!(matches!(
            driver.model_mut().take_events()[..],
            [ThumbnailEvent::Ready { current: true, .. }]
        ));
        let retries: Vec<u32> = fake
            .calls()
            .iter()
            .filter_map(|call| match call {
                crate::backend::fake::Call::RequestThumbnail { request } => Some(request.retry),
                _ => None,
            })
            .collect();
        assert_eq!(retries, [0, 1]);
    }

    #[test]
    fn every_open_request_is_closed_by_its_answer_or_its_cancellation() {
        let fake = Arc::new(FakeBackend::new());
        let executor = Arc::new(ManualExecutor::new());
        let fetcher = ThumbnailFetcher::new(Arc::clone(&fake) as Arc<dyn Backend>);
        let open = Arc::clone(&fetcher.flights);
        let mut driver = Driver::new(
            ThumbnailPipeline::new(),
            fetcher,
            Arc::clone(&executor) as Arc<dyn Executor>,
        );
        for asset_id in 1..=4 {
            driver.handle(want(asset_id, ThumbnailMode::Visible));
        }
        // 1: cancelled before it starts. 2: cancelled in flight. 3: answered.
        // 4: answered, then cancelled (no entry is left to cancel).
        driver.handle(ThumbnailInput::Cancel { key: key(1) });
        executor.run_all();
        driver.handle(ThumbnailInput::Cancel { key: key(2) });
        for _ in 0..3 {
            fake.answer_thumbnail(0, ThumbnailReply::Ready(image(VERSION)));
        }
        driver.pump();
        driver.handle(ThumbnailInput::Cancel { key: key(4) });

        {
            let flights = open.lock().unwrap();
            assert!(flights.open.is_empty());
            assert!(flights.waiting.is_empty());
            assert_eq!(flights.lanes, 0);
        }
        assert_eq!(driver.model_mut().take_events().len(), 2);
    }
    fn limited_driver(
        fake: &Arc<FakeBackend>,
        executor: Arc<dyn Executor>,
        limit: usize,
    ) -> Driver<ThumbnailPipeline, ThumbnailFetcher> {
        Driver::new(
            ThumbnailPipeline::new(),
            ThumbnailFetcher::with_limit(Arc::clone(fake) as Arc<dyn Backend>, limit),
            executor,
        )
    }

    fn requested(fake: &FakeBackend) -> Vec<i64> {
        fake.calls()
            .iter()
            .filter_map(|call| match call {
                crate::backend::fake::Call::RequestThumbnail { request } => Some(request.asset_id),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn requests_beyond_the_limit_wait_in_the_fetcher_and_take_no_executor_task() {
        let fake = Arc::new(FakeBackend::new());
        let executor = Arc::new(ManualExecutor::new());
        let fetcher = ThumbnailFetcher::with_limit(Arc::clone(&fake) as Arc<dyn Backend>, 2);
        let open = Arc::clone(&fetcher.flights);
        let mut driver = Driver::new(
            ThumbnailPipeline::new(),
            fetcher,
            Arc::clone(&executor) as Arc<dyn Executor>,
        );

        for asset_id in 1..=5 {
            driver.handle(want(asset_id, ThumbnailMode::Visible));
        }

        // Two lanes, however many tiles ask; the rest waits in the fetcher.
        assert_eq!(executor.pending(), 2);
        assert_eq!(open.lock().unwrap().open.len(), 5);

        // A request cancelled while it waits is never sent.
        driver.handle(ThumbnailInput::Cancel { key: key(4) });
        assert_eq!(open.lock().unwrap().open.len(), 4);

        // One lane sends everything that waits, oldest first; the other finds
        // nothing left.
        assert!(executor.run_next());
        assert_eq!(requested(&fake), [1, 2, 3, 5]);
        assert_eq!(executor.run_all(), 1);
        assert_eq!(requested(&fake), [1, 2, 3, 5]);
        assert_eq!(open.lock().unwrap().lanes, 0);

        // The next request starts a lane again.
        driver.handle(want(6, ThumbnailMode::Prefetch));
        assert_eq!(executor.pending(), 1);
        executor.run_all();
        assert_eq!(requested(&fake), [1, 2, 3, 5, 6]);
    }

    #[test]
    fn the_default_limit_bounds_the_executor_tasks_of_a_gallery_screen() {
        let fake = Arc::new(FakeBackend::new());
        let executor = Arc::new(ManualExecutor::new());
        let mut driver = driver(&fake, &executor);

        for asset_id in 1..=200 {
            driver.handle(want(asset_id, ThumbnailMode::Prefetch));
        }

        assert_eq!(executor.pending(), MAX_CONCURRENT_FETCHES);
        executor.run_all();
        assert_eq!(fake.held_thumbnails().len(), 200);
    }

    #[test]
    fn a_lane_the_executor_dropped_gives_its_place_to_the_next_request() {
        let fake = Arc::new(FakeBackend::new());
        let executor = Arc::new(ManualExecutor::new());
        let mut driver = limited_driver(&fake, Arc::clone(&executor) as Arc<dyn Executor>, 1);
        driver.handle(want(1, ThumbnailMode::Visible));
        driver.handle(want(2, ThumbnailMode::Visible));
        assert_eq!(executor.pending(), 1);

        assert!(executor.drop_nth(0));
        driver.handle(want(3, ThumbnailMode::Visible));

        assert_eq!(executor.pending(), 1);
        executor.run_all();
        assert_eq!(requested(&fake), [1, 2, 3]);
    }

    #[test]
    fn a_request_that_panics_in_the_backend_does_not_strand_the_requests_behind_it() {
        let fake = Arc::new(FakeBackend::new());
        let executor = Arc::new(ManualExecutor::new());
        let mut driver = limited_driver(&fake, Arc::clone(&executor) as Arc<dyn Executor>, 1);
        fake.on_request_thumbnail(|request, responder| {
            assert_ne!(request.asset_id, 1, "synthetic backend panic");
            responder.respond(ThumbnailReply::Ready(image(VERSION)));
        });
        driver.handle(want(1, ThumbnailMode::Visible));
        driver.handle(want(2, ThumbnailMode::Visible));

        executor.run_all();
        driver.pump();

        // The panicking request was answered `Unavailable` by its dropped
        // responder and waits for its retry; the one behind it was served.
        assert_eq!(
            driver.model().status(&key(1)),
            Some(ThumbnailStatus::Loading)
        );
        assert_eq!(executor.pending_timers(), 1);
        assert!(matches!(
            &driver.model_mut().take_events()[..],
            [ThumbnailEvent::Ready { key: served, .. }] if *served == key(2)
        ));
    }

    #[test]
    fn fetches_stuck_in_the_backend_leave_executor_threads_to_other_work() {
        const WAIT: Duration = Duration::from_secs(10);
        let fake = Arc::new(FakeBackend::new());
        let executor = Arc::new(ThreadExecutor::with_limits(3, Duration::from_secs(5)));
        let mut driver = limited_driver(&fake, Arc::clone(&executor) as Arc<dyn Executor>, 2);
        // Every request blocks inside the backend until the test lets go, the
        // way a source check on a hung mount does.
        let (release, gate) = mpsc::channel::<()>();
        let (entered_tx, entered) = mpsc::channel();
        fake.on_request_thumbnail(move |_, responder| {
            let _ = entered_tx.send(());
            let _ = gate.recv();
            responder.respond(ThumbnailReply::NotFound);
        });

        for asset_id in 1..=6 {
            driver.handle(want(asset_id, ThumbnailMode::Visible));
        }
        entered.recv_timeout(WAIT).unwrap();

        // More tiles ask than the pool has threads, yet a query, a write or a
        // timer firing still gets one.
        let (done_tx, done) = mpsc::channel();
        executor.spawn(Box::new(move || done_tx.send(()).unwrap()));
        done.recv_timeout(WAIT).unwrap();
        assert!(requested(&fake).len() <= 2);

        drop(release);
        assert!(executor.wait_until_quiet(WAIT));
        let mut sent = requested(&fake);
        sent.sort_unstable();
        assert_eq!(sent, [1, 2, 3, 4, 5, 6]);
        assert_eq!(driver.pump(), 6);
    }
}
