//! Bulk selection (`src/features/gallery/createSelection.ts`) as a state machine.
//!
//! The selection is a spec instead of a set of every ID (ADR 0008): inclusive index
//! ranges of one query session plus two small ID maps, `excluded` (unselected IDs
//! inside the ranges) and `included` (selected IDs outside them). Selecting a
//! million results costs one range, [`Selection::is_selected`] is a binary search and
//! a map lookup, and the count never needs an ID. Ranges covering at most
//! [`MATERIALIZE_LIMIT`] positions are resolved to IDs with one read of their
//! envelope, so small selections keep their IDs across queries; larger ones stay
//! ranges, which only the backend resolves (`apply_selection_mutation`).
//!
//! [`Selection`] follows the convention of [`crate::model`]. It never calls the query
//! model itself. What the Solid model reads through reactive getters it is told
//! with [`SelectionInput::Sync`] after every input the query model took (the query
//! epoch, the session, the total, the failure epoch) and the identity epoch of the
//! tag state. Its reads of the query model are effects with a [`Ticket`] of its own
//! range ([`SELECTION_TICKET_BASE`]), answered with the query's events. The owner
//! ([`AppRuntime`](crate::runtime::AppRuntime)) passes all of them on in the same
//! turn.
//!
//! # Epochs
//!
//! - **Query epoch.** Every change cancels a pending range read, its failed retry
//!   and the Shift anchor. When the query has a session again and it is another one
//!   than the ranges', the positions mean other assets: the whole selection is
//!   dropped with [`SelectionNotice::RangeCleared`]. The same session (a cache hit)
//!   keeps the ranges. IDs held directly stay selected; only their positions are
//!   forgotten, and nothing is published, so whoever keys drafts by
//!   [`Selection::key`] keeps them.
//! - **Failure epoch.** A failed refresh leaves the query without a session, so the
//!   keepalive below could no longer reach the ranges' session: ranges are dropped
//!   with [`SelectionNotice::RangeExpired`]. A failed page read keeps the session and
//!   the ranges. IDs held directly stay.
//! - **Identity epoch.** The library was replaced: the selection is cleared.
//!
//! # Keepalive (ADR 0029)
//!
//! Exactly while ranges are held, their session is read once at once and then every
//! [`SESSION_KEEPALIVE`], one read in flight at most, so the backend's idle expiry
//! never drops it. The interval is a timer effect. When the read made as the ranges
//! appeared is answered `Stale`, their session was gone before the selection: they
//! are dropped with [`SelectionNotice::RangeExpired`]. A later stale read is a changed
//! result, which the query's own refresh reports when its new session lands.

use std::{
    collections::{HashMap, HashSet},
    fmt::{Display, Formatter},
    sync::{Arc, OnceLock},
    time::Duration,
};

use crate::{
    backend::SelectionSpec,
    library::{KeepAlive, Mutation, QueryError, Ticket},
    model::{Model, RequestToken, Tokens},
};

/// Ranges covering at most this many positions are resolved to IDs right away, so
/// small selections keep their IDs across queries. Larger ones stay index ranges of
/// their query session and are resolved by the backend (ADR 0008).
pub const MATERIALIZE_LIMIT: usize = 10_000;
/// While ranges are held, their session is read this often so the backend does not
/// expire it after five idle minutes and a later bulk write is not refused as stale
/// (ADR 0029). Must stay well below the backend's session TTL.
pub const SESSION_KEEPALIVE: Duration = Duration::from_secs(60);
/// The first [`Ticket`] value of the selection's reads of the query model. Every
/// other caller of ticketed query reads chooses its tickets below it.
pub const SELECTION_TICKET_BASE: u64 = 1 << 62;

/// Inclusive global indexes `[start, end]`.
pub type IndexRange = [usize; 2];

/// Sorts and merges overlapping or adjacent inclusive ranges; a range whose start is
/// after its end is dropped.
pub fn merge_ranges(ranges: &[IndexRange]) -> Vec<IndexRange> {
    let mut sorted: Vec<IndexRange> = ranges
        .iter()
        .copied()
        .filter(|[start, end]| start <= end)
        .collect();
    sorted.sort_by_key(|range| range[0]);
    let mut merged: Vec<IndexRange> = Vec::with_capacity(sorted.len());
    for [start, end] in sorted {
        match merged.last_mut() {
            Some(last) if start <= last[1].saturating_add(1) => last[1] = last[1].max(end),
            _ => merged.push([start, end]),
        }
    }
    merged
}

/// Binary search: whether `index` lies in one of the sorted ranges. O(log r).
pub fn ranges_contain(ranges: &[IndexRange], index: usize) -> bool {
    super::selection_geometry::contains_index(ranges, index)
}

/// How many positions the sorted, non-overlapping ranges cover.
fn coverage(ranges: &[IndexRange]) -> usize {
    ranges.iter().map(|[start, end]| end - start + 1).sum()
}

/// Why ranges were dropped without a user action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionNotice {
    /// A new session (another result) replaced the ranges' one.
    RangeCleared,
    /// The ranges' session was gone before they were used, or the query lost its
    /// session in a failed refresh.
    RangeExpired,
}

/// Why a range could not be added. The selection from before stays, and
/// [`SelectionInput::Retry`] repeats the same ranges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionError {
    /// The query has no session, or the ranges held belong to another one.
    Cancelled,
    /// The read answered with another number of IDs than the range has positions.
    Incomplete,
    /// The read of the range's IDs failed.
    Read(QueryError),
}

impl Display for SelectionError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => f.write_str("Selection range was cancelled"),
            Self::Incomplete => f.write_str("Selection range is incomplete"),
            Self::Read(error) => Display::fmt(error, f),
        }
    }
}

impl std::error::Error for SelectionError {}

/// What the user did in the gallery (`BulkSelectionInteraction`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Interaction {
    Click {
        asset_id: i64,
        asset_index: usize,
        /// Ctrl or the platform key was held.
        ctrl_like: bool,
        shift: bool,
    },
    Clear,
    /// Every result of the current query, as one range (Ctrl+A in selection mode).
    SelectAll,
    /// A press that may become a rectangle: writes wait until it ends.
    RectangleStart,
    RectangleCancel,
    RectangleCommit {
        ranges: Vec<IndexRange>,
        /// Adds to the selection instead of replacing it.
        additive: bool,
    },
}

/// What the selection knows of the query model and the tag state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct QuerySnapshot {
    /// `AssetQuery::query_epoch`.
    pub query_epoch: u64,
    /// `AssetTagState::epoch`: changes when the library was replaced.
    pub identity_epoch: u64,
    /// `AssetQuery::page_failure_epoch`.
    pub failure_epoch: u64,
    /// The backend session; `None` while a query is pending and after one failed.
    pub session_id: Option<u64>,
    /// The result count of the current query (select all).
    pub total: usize,
}

/// Commands, the query's state and answers, and the keepalive timer.
#[derive(Debug, PartialEq)]
pub enum SelectionInput {
    /// Turns selection mode on or off; turning it off clears the selection.
    ToggleMode,
    /// A gesture of the gallery; ignored while selection mode is off.
    Interact(Interaction),
    /// Clears the selection and keeps selection mode on.
    Clear,
    /// Drops a pending range read, its error and its retry.
    Cancel,
    /// Repeats the ranges whose read failed, in their original mode.
    Retry,
    /// Unselects IDs held directly (assets that were deleted).
    Remove(Vec<i64>),
    /// The query model and the tag state as they are now.
    Sync(QuerySnapshot),
    /// The answer to [`SelectionEffect::ReadIdRange`].
    IdRangeRead {
        ticket: Ticket,
        result: Result<Vec<i64>, QueryError>,
    },
    /// The answer to [`SelectionEffect::KeepSessionAlive`].
    SessionKeptAlive {
        ticket: Ticket,
        result: Result<KeepAlive, QueryError>,
    },
    /// The keepalive timer started with this token fired.
    KeepAliveDue(RequestToken),
    /// The owner is going away: nothing is read or published again.
    Dispose,
}

/// Reads of the query model and the keepalive timer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionEffect {
    /// `QueryInput::ReadIdRange`; answered with [`SelectionInput::IdRangeRead`].
    ReadIdRange {
        ticket: Ticket,
        start: usize,
        end: usize,
    },
    /// `QueryInput::KeepSessionAlive`; answered with
    /// [`SelectionInput::SessionKeptAlive`].
    KeepSessionAlive { ticket: Ticket, session_id: u64 },
    /// Send [`SelectionInput::KeepAliveDue`] with this token after `delay`.
    KeepAliveTimer {
        token: RequestToken,
        delay: Duration,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Slot {
    /// The position in the state's session; `None` when it is not known (an ID kept
    /// from an earlier query).
    position: Option<usize>,
    /// Insertion order.
    order: u64,
}

/// IDs with their position, in insertion order (a JavaScript `Map`): setting an ID
/// that is present keeps its place, removing and adding it moves it to the end.
#[derive(Debug, Clone, Default)]
struct IdMap {
    slots: HashMap<i64, Slot>,
    next: u64,
}

impl IdMap {
    fn len(&self) -> usize {
        self.slots.len()
    }

    fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    fn contains(&self, id: i64) -> bool {
        self.slots.contains_key(&id)
    }

    fn set(&mut self, id: i64, position: Option<usize>) {
        let order = self.next;
        let slot = self.slots.entry(id).or_insert_with(|| {
            self.next += 1;
            Slot { position, order }
        });
        slot.position = position;
    }

    fn remove(&mut self, id: i64) -> bool {
        self.slots.remove(&id).is_some()
    }

    fn retain(&mut self, mut keep: impl FnMut(i64, Option<usize>) -> bool) {
        self.slots.retain(|id, slot| keep(*id, slot.position));
    }

    /// Whether the position of some ID is unknown.
    fn has_unknown_position(&self) -> bool {
        self.slots.values().any(|slot| slot.position.is_none())
    }

    fn forget_positions(&mut self) {
        for slot in self.slots.values_mut() {
            slot.position = None;
        }
    }

    /// The IDs in insertion order.
    fn ids(&self) -> Vec<i64> {
        let mut ids: Vec<(u64, i64)> = self
            .slots
            .iter()
            .map(|(id, slot)| (slot.order, *id))
            .collect();
        ids.sort_unstable();
        ids.into_iter().map(|(_, id)| id).collect()
    }
}

#[derive(Debug, Clone, Default)]
struct State {
    /// Inclusive, ascending, non-overlapping positions in `session`.
    ranges: Vec<IndexRange>,
    session: Option<u64>,
    /// Unselected IDs inside `ranges`, with their position.
    excluded: IdMap,
    /// Selected IDs outside `ranges`, with their position in `session` when known.
    included: IdMap,
}

/// Ranges to add, kept for the read that resolves them and for its retry.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RangeIntent {
    ranges: Vec<IndexRange>,
    additive: bool,
    clear_anchor: bool,
}

/// The one range read on its way.
#[derive(Debug)]
struct PendingRead {
    ticket: Ticket,
    intent: RangeIntent,
    /// The selection the ranges are added to, as it was when the read started.
    base: Arc<State>,
    /// The new ranges, merged.
    fresh: Vec<IndexRange>,
    /// `base`'s ranges and `fresh`, merged.
    merged: Vec<IndexRange>,
    session: u64,
    /// The result is IDs (a small selection) instead of ranges.
    materialize: bool,
    first: usize,
    last: usize,
}

#[derive(Debug)]
struct SessionKeepAlive {
    session: u64,
    timer: RequestToken,
    /// The read on its way.
    pending: Option<Ticket>,
    /// The read made when the ranges appeared, until it is answered.
    initial: Option<Ticket>,
}

/// What a bulk command needs of the selection at the moment it starts (`capture`).
#[derive(Debug, Clone, PartialEq)]
pub struct SelectionCapture {
    /// The IDs held directly, in the order they were selected.
    pub ids: Arc<[i64]>,
    /// The spec to write through, while ranges are held.
    pub spec: Option<SelectionSpec>,
    pub count: usize,
    pub key: u64,
    pub enabled: bool,
    /// A gesture or a range read is pending: writes wait.
    pub busy: bool,
}

/// A write on everything a range selection holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionWrite {
    /// Adds tags; never removes any.
    MergeTags(Vec<String>),
    /// Favorites all of it unless all already is, then clears all.
    ToggleFavorites,
    /// Groups it in gallery order; a blank key clears its groups.
    SetMediaGroup(Option<String>),
}

impl SelectionCapture {
    /// The mutation that applies `write` to a range selection: the backend resolves
    /// the spec on its session and writes all of it or, when the session is stale,
    /// nothing (`apply_selection_mutation`). `None` while no ranges are held (the IDs
    /// are written through the ID commands) and while a gesture or a range read is
    /// pending.
    pub fn spec_mutation(&self, write: SelectionWrite) -> Option<Mutation> {
        if self.busy {
            return None;
        }
        let selection = self.spec.clone()?;
        Some(match write {
            SelectionWrite::MergeTags(tags) => Mutation::MergeTagsInSelection { selection, tags },
            SelectionWrite::ToggleFavorites => Mutation::ToggleFavoritesInSelection { selection },
            SelectionWrite::SetMediaGroup(key) => {
                Mutation::SetMediaGroupInSelection { selection, key }
            }
        })
    }
}

/// The bulk selection of the gallery.
#[derive(Debug)]
pub struct Selection {
    enabled: bool,
    /// A rectangle gesture is held or a range read is on its way.
    busy: bool,
    state: Arc<State>,
    count: usize,
    /// Counts the published changes.
    revision: u64,
    ids: OnceLock<Arc<[i64]>>,
    /// The global index Shift ranges start from.
    anchor: Option<usize>,
    error: Option<SelectionError>,
    notice: Option<SelectionNotice>,
    retry: Option<RangeIntent>,
    pending: Option<PendingRead>,
    keepalive: Option<SessionKeepAlive>,
    query: QuerySnapshot,
    tokens: Tokens,
    last_ticket: u64,
    disposed: bool,
}

impl Selection {
    /// An empty selection with selection mode off, for a query as in `query`.
    pub fn new(query: QuerySnapshot) -> Self {
        Self {
            enabled: false,
            busy: false,
            state: Arc::default(),
            count: 0,
            revision: 0,
            ids: OnceLock::new(),
            anchor: None,
            error: None,
            notice: None,
            retry: None,
            pending: None,
            keepalive: None,
            query,
            tokens: Tokens::default(),
            last_ticket: 0,
            disposed: false,
        }
    }

    /// Whether a ticket of a query event is one of the selection's.
    pub fn owns(ticket: Ticket) -> bool {
        ticket.0 >= SELECTION_TICKET_BASE
    }

    /// Whether selection mode is on.
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// The selected IDs held directly, in the order they were selected; with ranges
    /// these are only the IDs outside them.
    pub fn ids(&self) -> Arc<[i64]> {
        self.ids
            .get_or_init(|| self.state.included.ids().into())
            .clone()
    }

    /// Whether the selection holds index ranges that only the backend resolves.
    pub fn ranged(&self) -> bool {
        !self.state.ranges.is_empty()
    }

    /// How many assets are selected: covered positions − excluded + included.
    pub fn count(&self) -> usize {
        self.count
    }

    /// Changes with every published change of the selection.
    pub fn key(&self) -> u64 {
        self.revision
    }

    /// A gesture or a range read is pending: bulk writes wait.
    pub fn busy(&self) -> bool {
        self.busy
    }

    /// Why the last range could not be added; the gallery shows its load error
    /// banner, whose Retry sends [`SelectionInput::Retry`].
    pub fn error(&self) -> Option<&SelectionError> {
        self.error.as_ref()
    }

    /// Why the selection was cleared without a user action, if it was.
    pub fn notice(&self) -> Option<SelectionNotice> {
        self.notice
    }

    /// The global index a Shift range starts from.
    pub fn anchor(&self) -> Option<usize> {
        self.anchor
    }

    /// The session the keepalive reads, while ranges are held.
    pub fn keepalive_session(&self) -> Option<u64> {
        self.keepalive.as_ref().map(|keepalive| keepalive.session)
    }

    /// O(log r). Without an `id` (a slot whose asset is not loaded) only the ranges
    /// are consulted.
    pub fn is_selected(&self, index: usize, id: Option<i64>) -> bool {
        let state = &self.state;
        if !state.ranges.is_empty() && ranges_contain(&state.ranges, index) {
            return id.is_none_or(|id| !state.excluded.contains(id));
        }
        id.is_some_and(|id| state.included.contains(id))
    }

    /// The spec a bulk write goes through, while ranges are held.
    pub fn spec(&self) -> Option<SelectionSpec> {
        let state = &self.state;
        let session_id = state.session.filter(|_| !state.ranges.is_empty())?;
        Some(SelectionSpec {
            session_id,
            ranges: state.ranges.clone(),
            excluded: state.excluded.ids(),
            included: state.included.ids(),
        })
    }

    pub fn capture(&self) -> SelectionCapture {
        SelectionCapture {
            ids: self.ids(),
            spec: self.spec(),
            count: self.count,
            key: self.revision,
            enabled: self.enabled,
            busy: self.busy,
        }
    }

    fn ticket(&mut self) -> Ticket {
        self.last_ticket += 1;
        Ticket(SELECTION_TICKET_BASE + self.last_ticket)
    }

    fn publish(&mut self, next: State, effects: &mut Vec<SelectionEffect>) {
        self.count = coverage(&next.ranges) - next.excluded.len() + next.included.len();
        self.state = Arc::new(next);
        self.revision += 1;
        self.ids = OnceLock::new();
        self.sync_keepalive(effects);
    }

    /// The keepalive runs exactly while ranges are held, for their session. Its first
    /// read is immediate: selecting a large range reads nothing, and the session may
    /// already have been idle for most of its lifetime.
    fn sync_keepalive(&mut self, effects: &mut Vec<SelectionEffect>) {
        let target = self
            .state
            .session
            .filter(|_| !self.disposed && !self.state.ranges.is_empty());
        if target == self.keepalive_session() {
            return;
        }
        self.keepalive = None;
        let Some(session) = target else {
            return;
        };
        let timer = self.tokens.issue();
        let ticket = self.ticket();
        self.keepalive = Some(SessionKeepAlive {
            session,
            timer,
            pending: Some(ticket),
            initial: Some(ticket),
        });
        effects.push(SelectionEffect::KeepAliveTimer {
            token: timer,
            delay: SESSION_KEEPALIVE,
        });
        effects.push(SelectionEffect::KeepSessionAlive {
            ticket,
            session_id: session,
        });
    }

    fn on_keepalive_due(&mut self, token: RequestToken, effects: &mut Vec<SelectionEffect>) {
        let read = match &self.keepalive {
            Some(keepalive) if keepalive.timer == token => keepalive.pending.is_none(),
            // The timer of ranges that are gone.
            _ => return,
        };
        effects.push(SelectionEffect::KeepAliveTimer {
            token,
            delay: SESSION_KEEPALIVE,
        });
        if !read {
            return;
        }
        let ticket = self.ticket();
        if let Some(keepalive) = &mut self.keepalive {
            keepalive.pending = Some(ticket);
            effects.push(SelectionEffect::KeepSessionAlive {
                ticket,
                session_id: keepalive.session,
            });
        }
    }

    fn on_session_kept_alive(
        &mut self,
        ticket: Ticket,
        result: Result<KeepAlive, QueryError>,
        effects: &mut Vec<SelectionEffect>,
    ) {
        let Some(keepalive) = self
            .keepalive
            .as_mut()
            .filter(|keepalive| keepalive.pending == Some(ticket))
        else {
            // The read of ranges that were replaced since.
            return;
        };
        keepalive.pending = None;
        let initial = keepalive.initial.take() == Some(ticket);
        // A failed read is retried at the next tick; a stale one refreshes the query.
        // The read made when the ranges appeared found their session already gone, so
        // they were never usable: drop them now and say so.
        if initial && result == Ok(KeepAlive::Stale) {
            self.drop_ranges(SelectionNotice::RangeExpired, effects);
        }
    }

    fn cancel(&mut self) {
        self.pending = None;
        self.busy = false;
        self.retry = None;
        self.error = None;
    }

    fn clear(&mut self, effects: &mut Vec<SelectionEffect>) {
        self.cancel();
        self.anchor = None;
        self.notice = None;
        self.publish(State::default(), effects);
    }

    /// Drops the whole selection because its ranges can no longer be written.
    fn drop_ranges(&mut self, reason: SelectionNotice, effects: &mut Vec<SelectionEffect>) {
        self.cancel();
        self.anchor = None;
        self.publish(State::default(), effects);
        self.notice = Some(reason);
    }

    fn on_query_epoch(&mut self, effects: &mut Vec<SelectionEffect>) {
        self.cancel();
        self.anchor = None;
        let Some(current) = self.query.session_id else {
            return;
        };
        if self.state.session == Some(current) {
            return;
        }
        if !self.state.ranges.is_empty() {
            // Ranges are positions in the session the user saw; a new result (filter,
            // scan, reorder) gives them another meaning. Small ranges were already
            // resolved to IDs.
            self.drop_ranges(SelectionNotice::RangeCleared, effects);
        } else if !self.state.included.is_empty() {
            // IDs stay selected; only their positions belonged to the old session. The
            // selected set, count and identity are unchanged, so nothing is published.
            let state = Arc::make_mut(&mut self.state);
            state.session = Some(current);
            state.included.forget_positions();
        }
    }

    fn sync(&mut self, query: QuerySnapshot, effects: &mut Vec<SelectionEffect>) {
        let previous = std::mem::replace(&mut self.query, query);
        if query.query_epoch != previous.query_epoch {
            self.on_query_epoch(effects);
        }
        // A failed refresh leaves the query without a session. The keepalive only
        // reaches the query's current session, so the ranges' session would expire
        // under the selection.
        if query.failure_epoch != previous.failure_epoch
            && !self.state.ranges.is_empty()
            && query.session_id.is_none()
        {
            self.drop_ranges(SelectionNotice::RangeExpired, effects);
        }
        if query.identity_epoch != previous.identity_epoch {
            self.clear(effects);
        }
    }

    fn finish(&mut self, next: State, clear_anchor: bool, effects: &mut Vec<SelectionEffect>) {
        self.publish(next, effects);
        if clear_anchor {
            self.anchor = None;
        }
    }

    fn fail(&mut self, error: SelectionError, intent: RangeIntent) {
        self.error = Some(error);
        self.retry = Some(intent);
    }

    /// Adds ranges (or replaces the selection with them), resolving IDs only when
    /// needed.
    fn add_ranges(&mut self, intent: RangeIntent, effects: &mut Vec<SelectionEffect>) {
        self.cancel();
        self.notice = None;
        let base = if intent.additive {
            Arc::clone(&self.state)
        } else {
            Arc::default()
        };
        let fresh = merge_ranges(&intent.ranges);
        let merged = merge_ranges(&[base.ranges.as_slice(), fresh.as_slice()].concat());
        let session = self
            .query
            .session_id
            .filter(|session| base.ranges.is_empty() || base.session == Some(*session));
        let Some(session) = session else {
            self.fail(SelectionError::Cancelled, intent);
            return;
        };
        // Nothing to add: also a range that would only be read to place IDs kept from
        // an earlier query.
        if merged.is_empty() || fresh.is_empty() {
            let base = Arc::unwrap_or_clone(base);
            self.finish(base, intent.clear_anchor, effects);
            return;
        }
        let materialize = coverage(&merged) <= MATERIALIZE_LIMIT;
        if !materialize && !base.included.has_unknown_position() {
            // A large range is selected without reading a single ID.
            let mut next = Arc::unwrap_or_clone(base);
            next.excluded
                .retain(|_, position| !position.is_some_and(|at| ranges_contain(&fresh, at)));
            next.included
                .retain(|_, position| !position.is_some_and(|at| ranges_contain(&merged, at)));
            next.ranges = merged;
            next.session = Some(session);
            self.finish(next, intent.clear_anchor, effects);
            return;
        }
        // One read of the envelope: of all ranges when they are small, else only of
        // the new ones (to find included IDs whose position in this session is
        // unknown).
        let scope = if materialize { &merged } else { &fresh };
        let (first, last) = (scope[0][0], scope[scope.len() - 1][1]);
        let ticket = self.ticket();
        self.busy = true;
        self.pending = Some(PendingRead {
            ticket,
            intent,
            base,
            fresh,
            merged,
            session,
            materialize,
            first,
            last,
        });
        effects.push(SelectionEffect::ReadIdRange {
            ticket,
            start: first,
            end: last,
        });
    }

    fn on_id_range_read(
        &mut self,
        ticket: Ticket,
        result: Result<Vec<i64>, QueryError>,
        effects: &mut Vec<SelectionEffect>,
    ) {
        if self.pending.as_ref().map(|pending| pending.ticket) != Some(ticket) {
            // The answer to a read that was cancelled or replaced.
            return;
        }
        let Some(read) = self.pending.take() else {
            return;
        };
        self.busy = false;
        let ids = match result {
            Ok(ids) if ids.len() == read.last - read.first + 1 => ids,
            Ok(_) => return self.fail(SelectionError::Incomplete, read.intent),
            Err(error) => return self.fail(SelectionError::Read(error), read.intent),
        };
        let PendingRead {
            intent,
            base,
            fresh,
            merged,
            session,
            materialize,
            first,
            ..
        } = read;
        let mut next = Arc::unwrap_or_clone(base);
        next.session = Some(session);
        if materialize {
            for [start, end] in &merged {
                for index in *start..=*end {
                    let id = ids[index - first];
                    if !next.excluded.contains(id) {
                        next.included.set(id, Some(index));
                    }
                }
            }
            next.ranges = Vec::new();
            next.excluded = IdMap::default();
        } else {
            // The IDs kept from an earlier query that the new ranges cover get their
            // position, so the filter below drops them from `included`.
            let unknown: HashSet<i64> = next
                .included
                .slots
                .iter()
                .filter(|(_, slot)| slot.position.is_none())
                .map(|(id, _)| *id)
                .collect();
            for [start, end] in &fresh {
                for index in *start..=*end {
                    let id = ids[index - first];
                    if unknown.contains(&id) {
                        next.included.set(id, Some(index));
                    }
                }
            }
            next.excluded
                .retain(|_, position| !position.is_some_and(|at| ranges_contain(&fresh, at)));
            next.included
                .retain(|_, position| !position.is_some_and(|at| ranges_contain(&merged, at)));
            next.ranges = merged;
        }
        self.finish(next, intent.clear_anchor, effects);
    }

    fn click(
        &mut self,
        asset_id: i64,
        asset_index: usize,
        ctrl_like: bool,
        shift: bool,
        effects: &mut Vec<SelectionEffect>,
    ) {
        if let Some(anchor) = self.anchor.filter(|_| shift) {
            // Shift replaces the selection with the range from the anchor, Ctrl+Shift
            // adds it. Shift does not move the anchor.
            self.add_ranges(
                RangeIntent {
                    ranges: vec![[anchor.min(asset_index), anchor.max(asset_index)]],
                    additive: ctrl_like,
                    clear_anchor: false,
                },
                effects,
            );
            return;
        }
        self.anchor = Some(asset_index);
        self.notice = None;
        let mut next = (*self.state).clone();
        if !next.ranges.is_empty() && ranges_contain(&next.ranges, asset_index) {
            // Inside a range a click toggles the exclusion.
            if !shift && !next.excluded.contains(asset_id) {
                next.excluded.set(asset_id, Some(asset_index));
            } else {
                next.excluded.remove(asset_id);
            }
        } else {
            if !shift && next.included.contains(asset_id) {
                next.included.remove(asset_id);
            } else {
                // Without a session the position belongs to no result.
                let position = self.query.session_id.map(|_| asset_index);
                next.included.set(asset_id, position);
            }
            next.session = next.session.or(self.query.session_id);
        }
        self.publish(next, effects);
    }

    fn interact(&mut self, interaction: Interaction, effects: &mut Vec<SelectionEffect>) {
        if !self.enabled {
            return;
        }
        self.cancel();
        match interaction {
            Interaction::Clear => self.clear(effects),
            Interaction::SelectAll => {
                let total = self.query.total;
                if total > 0 {
                    self.add_ranges(
                        RangeIntent {
                            ranges: vec![[0, total - 1]],
                            additive: false,
                            clear_anchor: true,
                        },
                        effects,
                    );
                }
            }
            Interaction::RectangleStart => self.busy = true,
            Interaction::RectangleCancel => {}
            Interaction::RectangleCommit { ranges, additive } => self.add_ranges(
                RangeIntent {
                    ranges,
                    additive,
                    clear_anchor: true,
                },
                effects,
            ),
            Interaction::Click {
                asset_id,
                asset_index,
                ctrl_like,
                shift,
            } => self.click(asset_id, asset_index, ctrl_like, shift, effects),
        }
    }
}

impl Model for Selection {
    type Input = SelectionInput;
    type Effect = SelectionEffect;

    fn handle(&mut self, input: SelectionInput) -> Vec<SelectionEffect> {
        let mut effects = Vec::new();
        if self.disposed {
            return effects;
        }
        match input {
            SelectionInput::ToggleMode => {
                self.cancel();
                self.enabled = !self.enabled;
                if !self.enabled {
                    self.clear(&mut effects);
                }
            }
            SelectionInput::Interact(interaction) => self.interact(interaction, &mut effects),
            SelectionInput::Clear => self.clear(&mut effects),
            SelectionInput::Cancel => self.cancel(),
            SelectionInput::Retry => {
                if let Some(intent) = self.retry.take() {
                    self.add_ranges(intent, &mut effects);
                }
            }
            SelectionInput::Remove(ids) => {
                let mut next = (*self.state).clone();
                for id in ids {
                    next.included.remove(id);
                }
                self.publish(next, &mut effects);
            }
            SelectionInput::Sync(query) => self.sync(query, &mut effects),
            SelectionInput::IdRangeRead { ticket, result } => {
                self.on_id_range_read(ticket, result, &mut effects);
            }
            SelectionInput::SessionKeptAlive { ticket, result } => {
                self.on_session_kept_alive(ticket, result, &mut effects);
            }
            SelectionInput::KeepAliveDue(token) => self.on_keepalive_due(token, &mut effects),
            SelectionInput::Dispose => {
                self.pending = None;
                self.disposed = true;
                self.keepalive = None;
            }
        }
        effects
    }
}

#[cfg(test)]
mod tests;
