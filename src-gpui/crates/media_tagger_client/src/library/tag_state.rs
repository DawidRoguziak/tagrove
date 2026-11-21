//! Canonical tags per asset, the write queue and the maintenance barrier
//! (`createAssetTagState` of the Solid frontend; ADR 0001).
//!
//! [`AssetTagState`] answers two questions for every tag editor and every metadata
//! write: *is the complete tag list of this asset known*, and *who may write now*.
//! It does no I/O and has no effects: the library model
//! ([`Library`](super::Library)) drives it and turns its answers into backend calls
//! and events.
//!
//! # Clocks and tokens
//!
//! Every asset has a clock: a generation that advances whenever what was read
//! before must not be published any more. A detail read captures the generation
//! ([`capture_generation`](AssetTagState::capture_generation)) and may publish its
//! tags only while the token is still current
//! ([`publish_details`](AssetTagState::publish_details)): not after a write on the
//! asset started or ended, not after the asset was deleted, not after an identity
//! reset. A write holds the asset's clock exclusively from
//! [`begin`](AssetTagState::begin) until
//! [`settle_mutation`](AssetTagState::settle_mutation), which publishes the
//! canonical tags of the backend's answer.
//!
//! # The write queue
//!
//! [`enqueue_mutation`](AssetTagState::enqueue_mutation) admits a write behind every
//! write admitted earlier on any of its assets, all assets linked at once, so the
//! order of admission orders the queue and overlapping bulk writes cannot deadlock.
//! A write on idle assets is ready at once. A turn that is not ready becomes ready
//! when the last write it waits for is released
//! ([`release`](AssetTagState::release) returns the turns that did).
//! [`enqueue_library_mutation`](AssetTagState::enqueue_library_mutation) admits a
//! write whose assets only the backend knows (a selection spec): it waits for every
//! earlier write and every later write waits for it.
//!
//! A write is refused only at admission during maintenance or after disposal, and
//! at its start when its asset was deleted or the identity epoch changed while it
//! waited. A failed write releases its turn like any other.
//!
//! # The maintenance barrier
//!
//! [`request_barrier`](AssetTagState::request_barrier) refuses new writes from that
//! moment on. The barrier is granted ([`poll_barrier`](AssetTagState::poll_barrier))
//! once the barriers requested before it have ended and every admitted write,
//! queued or running, has been released; it ends with
//! [`release_barrier`](AssetTagState::release_barrier).
//!
//! # Bounds
//!
//! Records of assets that nobody pins and no write holds are kept for the
//! [`MAX_INACTIVE_RECORDS`] most recently used assets. A token of an evicted record
//! is no longer current.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
    fmt::{Display, Formatter},
};

use super::tags::normalize_tags;

/// Most records of unpinned, idle assets that are kept.
pub const MAX_INACTIVE_RECORDS: usize = 512;

/// The clock of one asset as a read or a write saw it (`AssetTagGenerationToken`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TagToken {
    asset_id: i64,
    epoch: u64,
    generation: u64,
}

impl TagToken {
    pub fn asset_id(&self) -> i64 {
        self.asset_id
    }

    /// The identity epoch the token was captured in.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
}

/// One admitted write (`AssetTagMutationTurn`, `LibraryMutationTurn`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TurnId(u64);

/// One requested maintenance barrier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BarrierId(u64);

/// Why a write was not admitted or could not start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagStateError {
    /// The coordinator was disposed.
    Unavailable,
    /// A maintenance barrier is requested or running.
    Maintenance,
    /// The identity epoch changed while the write was queued.
    LibraryChanged,
    /// An asset of the write was deleted while it was queued.
    AssetDeleted,
    /// The turn is not at the head of its queues, or an asset of it is still held.
    OutOfOrder,
    /// The turn was started before, or was released.
    TurnUsed,
}

impl Display for TagStateError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Unavailable => "Tag mutation coordinator is unavailable",
            Self::Maintenance => "Metadata changes are paused during maintenance.",
            Self::LibraryChanged => "The library changed before this metadata change started.",
            Self::AssetDeleted => "The asset was deleted before this metadata change started.",
            Self::OutOfOrder => "Metadata write started out of queue order",
            Self::TurnUsed => "Metadata write turn was already used",
        })
    }
}

impl std::error::Error for TagStateError {}

/// The complete, canonical tag list of an asset (`AuthoritativeAssetTags`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssetTags<'a> {
    pub tags: &'a [String],
    /// The clock generation the list was published under.
    pub generation: u64,
}

/// A write that was admitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Admission {
    pub turn: TurnId,
    /// Nothing is queued ahead of it: it may [`begin`](AssetTagState::begin) now.
    pub ready: bool,
}

#[derive(Debug)]
struct StoredTags {
    tags: Vec<String>,
    generation: u64,
}

#[derive(Debug)]
struct Clock {
    generation: u64,
    /// The generation of the write that holds the asset.
    active_mutation: Option<u64>,
    deleted: bool,
}

/// Assets nobody pins and no write holds, least recently used first.
#[derive(Debug, Default)]
struct InactiveLru {
    order: BTreeMap<u64, i64>,
    stamps: HashMap<i64, u64>,
    next_stamp: u64,
}

impl InactiveLru {
    fn contains(&self, asset_id: i64) -> bool {
        self.stamps.contains_key(&asset_id)
    }

    fn remove(&mut self, asset_id: i64) {
        if let Some(stamp) = self.stamps.remove(&asset_id) {
            self.order.remove(&stamp);
        }
    }

    /// Adds the asset as the most recently used one; an asset that is already
    /// listed keeps its place.
    fn insert(&mut self, asset_id: i64) {
        if self.contains(asset_id) {
            return;
        }
        self.next_stamp += 1;
        self.stamps.insert(asset_id, self.next_stamp);
        self.order.insert(self.next_stamp, asset_id);
    }

    /// Makes a listed asset the most recently used one.
    fn touch(&mut self, asset_id: i64) {
        if self.contains(asset_id) {
            self.remove(asset_id);
            self.insert(asset_id);
        }
    }

    fn len(&self) -> usize {
        self.stamps.len()
    }

    fn pop_oldest(&mut self) -> Option<i64> {
        let (_, asset_id) = self.order.pop_first()?;
        self.stamps.remove(&asset_id);
        Some(asset_id)
    }

    fn clear(&mut self) {
        self.order.clear();
        self.stamps.clear();
    }
}

#[derive(Debug)]
enum TurnKind {
    Assets(Vec<i64>),
    Library,
}

#[derive(Debug)]
struct Turn {
    kind: TurnKind,
    /// The identity epoch at admission.
    epoch: u64,
    /// Admitted writes this one waits for, as linked at admission.
    waiting_on: BTreeSet<TurnId>,
    used: bool,
}

#[derive(Debug)]
struct Barrier {
    id: BarrierId,
    granted: bool,
}

/// Source of complete, canonical tag lists shared by every tag editor, and the
/// coordinator of metadata writes.
#[derive(Debug, Default)]
pub struct AssetTagState {
    entries: HashMap<i64, StoredTags>,
    clocks: HashMap<i64, Clock>,
    pins: HashMap<i64, usize>,
    inactive: InactiveLru,
    epoch: u64,
    revision: u64,
    last_generation: u64,
    last_turn: u64,
    last_barrier: u64,
    /// Admitted writes that were not released yet, queued or running.
    turns: BTreeMap<TurnId, Turn>,
    /// The latest admitted write per asset of the current identity.
    tails: HashMap<i64, TurnId>,
    /// The latest admitted selection-wide write; every later admission waits for it.
    library_tail: Option<TurnId>,
    /// Requested barriers in request order; only the first can be granted.
    barriers: VecDeque<Barrier>,
    disposed: bool,
}

impl AssetTagState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Counts every change of a published tag list, deletion and reset. A view that
    /// shows tags redraws when it moves.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The identity epoch: it advances with every [`reset`](Self::reset). Asset IDs
    /// of another epoch may name other assets.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Whether no admitted write is left, queued or running (`waitForIdle`).
    pub fn is_idle(&self) -> bool {
        self.turns.is_empty()
    }

    pub fn is_disposed(&self) -> bool {
        self.disposed
    }

    /// Whether a maintenance barrier is requested or running: new writes are refused.
    pub fn barrier_active(&self) -> bool {
        !self.barriers.is_empty()
    }

    fn next_generation(&mut self) -> u64 {
        self.last_generation += 1;
        self.last_generation
    }

    fn prune(&mut self) {
        while self.inactive.len() > MAX_INACTIVE_RECORDS {
            let Some(asset_id) = self.inactive.pop_oldest() else {
                break;
            };
            self.clocks.remove(&asset_id);
            self.entries.remove(&asset_id);
        }
    }

    /// `getClock`: the asset's clock, created when missing, with the asset moved to
    /// the recently used end of the inactive list when nothing holds it.
    fn clock(&mut self, asset_id: i64) -> &mut Clock {
        if !self.clocks.contains_key(&asset_id) {
            let generation = self.next_generation();
            self.clocks.insert(
                asset_id,
                Clock {
                    generation,
                    active_mutation: None,
                    deleted: false,
                },
            );
        }
        self.inactive.remove(asset_id);
        let held = self
            .clocks
            .get(&asset_id)
            .is_some_and(|clock| clock.active_mutation.is_some());
        if !self.pins.contains_key(&asset_id) && !held {
            self.inactive.insert(asset_id);
        }
        // The asset is now the newest inactive record or none at all, so pruning
        // cannot take its clock.
        self.prune();
        let generation = self.last_generation;
        self.clocks.entry(asset_id).or_insert(Clock {
            generation,
            active_mutation: None,
            deleted: false,
        })
    }

    /// Keeps the asset's record out of the inactive list while an editor shows it.
    /// Every pin needs one [`unpin`](Self::unpin).
    pub fn pin(&mut self, asset_id: i64) {
        self.inactive.remove(asset_id);
        *self.pins.entry(asset_id).or_default() += 1;
    }

    /// Ends one [`pin`](Self::pin). Without a pin of the asset this does nothing.
    pub fn unpin(&mut self, asset_id: i64) {
        let Some(count) = self.pins.get_mut(&asset_id) else {
            return;
        };
        if *count > 1 {
            *count -= 1;
        } else {
            self.pins.remove(&asset_id);
            if self
                .clocks
                .get(&asset_id)
                .is_some_and(|clock| clock.active_mutation.is_none())
            {
                self.inactive.insert(asset_id);
            }
        }
        self.prune();
    }

    /// Whether what was read under `token` may still be used: same identity epoch,
    /// same generation, no write holding the asset, not deleted.
    pub fn is_current(&self, token: &TagToken) -> bool {
        !self.disposed
            && token.epoch == self.epoch
            && self.clocks.get(&token.asset_id).is_some_and(|clock| {
                clock.generation == token.generation
                    && clock.active_mutation.is_none()
                    && !clock.deleted
            })
    }

    /// The complete tag list of the asset when it is known, and marks the record as
    /// recently used.
    pub fn get(&mut self, asset_id: i64) -> Option<AssetTags<'_>> {
        if self.entries.contains_key(&asset_id) {
            self.inactive.touch(asset_id);
        }
        self.peek(asset_id)
    }

    /// [`get`](Self::get) for a reader that only looks (a view): the record's place in
    /// the inactive list stays.
    pub fn peek(&self, asset_id: i64) -> Option<AssetTags<'_>> {
        self.entries.get(&asset_id).map(|entry| AssetTags {
            tags: &entry.tags,
            generation: entry.generation,
        })
    }

    /// Whether the complete tag list of the asset is known. Tag editing stays
    /// disabled until it is.
    pub fn is_known(&self, asset_id: i64) -> bool {
        self.entries.contains_key(&asset_id)
    }

    /// The asset's clock as a read that starts now sees it.
    pub fn capture_generation(&mut self, asset_id: i64) -> TagToken {
        let epoch = self.epoch;
        let generation = self.clock(asset_id).generation;
        TagToken {
            asset_id,
            epoch,
            generation,
        }
    }

    /// Publishes the tags of a detail read that started under `token`. `false`: the
    /// read is obsolete (another identity, a write since, a deletion) and nothing
    /// was published.
    pub fn publish_details(&mut self, asset_id: i64, tags: Vec<String>, token: &TagToken) -> bool {
        let epoch = self.epoch;
        let disposed = self.disposed;
        let clock = self.clock(asset_id);
        if disposed
            || token.asset_id != asset_id
            || token.epoch != epoch
            || token.generation != clock.generation
            || clock.active_mutation.is_some()
            || clock.deleted
        {
            return false;
        }
        let generation = clock.generation;
        let tags = normalize_tags(tags);
        if self
            .entries
            .get(&asset_id)
            .is_some_and(|previous| previous.generation == generation && previous.tags == tags)
        {
            return true;
        }
        self.entries
            .insert(asset_id, StoredTags { tags, generation });
        self.revision += 1;
        true
    }

    fn check_admission(&self) -> Result<(), TagStateError> {
        // Admission is decided at once: maintenance denies new writes before any
        // queueing.
        if self.disposed {
            Err(TagStateError::Unavailable)
        } else if self.barrier_active() {
            Err(TagStateError::Maintenance)
        } else {
            Ok(())
        }
    }

    fn admit(&mut self, kind: TurnKind, waiting_on: BTreeSet<TurnId>) -> Admission {
        self.last_turn += 1;
        let turn = TurnId(self.last_turn);
        let ready = waiting_on.is_empty();
        self.turns.insert(
            turn,
            Turn {
                kind,
                epoch: self.epoch,
                waiting_on,
                used: false,
            },
        );
        Admission { turn, ready }
    }

    /// Admits a write on `asset_ids` (duplicates count once) behind earlier writes
    /// on any of them.
    pub fn enqueue_mutation(&mut self, asset_ids: &[i64]) -> Result<Admission, TagStateError> {
        self.check_admission()?;
        let mut ids: Vec<i64> = Vec::with_capacity(asset_ids.len());
        let mut seen = BTreeSet::new();
        for id in asset_ids {
            if seen.insert(*id) {
                ids.push(*id);
            }
        }
        let turn = TurnId(self.last_turn + 1);
        let mut waiting_on = BTreeSet::new();
        waiting_on.extend(self.library_tail);
        for id in &ids {
            waiting_on.extend(self.tails.insert(*id, turn));
        }
        Ok(self.admit(TurnKind::Assets(ids), waiting_on))
    }

    /// Admits a write whose asset IDs are unknown here (a selection spec). It waits
    /// for every earlier write, and every later write waits for it.
    pub fn enqueue_library_mutation(&mut self) -> Result<Admission, TagStateError> {
        self.check_admission()?;
        let turn = TurnId(self.last_turn + 1);
        let mut waiting_on: BTreeSet<TurnId> = self.tails.values().copied().collect();
        waiting_on.extend(self.library_tail.replace(turn));
        Ok(self.admit(TurnKind::Library, waiting_on))
    }

    /// Whether the turn is admitted, not started and at the head of its queues.
    pub fn is_ready(&self, turn: TurnId) -> bool {
        self.turns
            .get(&turn)
            .is_some_and(|turn| !turn.used && turn.waiting_on.is_empty())
    }

    /// Every cached complete tag list may be outdated; detail reads in flight are
    /// rejected.
    fn invalidate_all(&mut self) {
        for clock in self.clocks.values_mut() {
            self.last_generation += 1;
            clock.generation = self.last_generation;
        }
        self.entries.clear();
        self.revision += 1;
    }

    /// Starts the write of a ready turn. For a write on assets it returns one
    /// exclusive token per asset, each to be settled with
    /// [`settle_mutation`](Self::settle_mutation); for a selection-wide write it
    /// drops every cached tag list and returns no token. A turn that failed to
    /// start still has to be [released](Self::release).
    pub fn begin(&mut self, turn: TurnId) -> Result<Vec<TagToken>, TagStateError> {
        let disposed = self.disposed;
        let current_epoch = self.epoch;
        let Some(entry) = self.turns.get_mut(&turn) else {
            return Err(TagStateError::TurnUsed);
        };
        if entry.used {
            return Err(TagStateError::TurnUsed);
        }
        if !entry.waiting_on.is_empty() {
            return Err(TagStateError::OutOfOrder);
        }
        entry.used = true;
        if disposed {
            return Err(TagStateError::Unavailable);
        }
        if entry.epoch != current_epoch {
            return Err(TagStateError::LibraryChanged);
        }
        let ids = match &entry.kind {
            TurnKind::Library => None,
            TurnKind::Assets(ids) => Some(ids.clone()),
        };
        let Some(ids) = ids else {
            self.invalidate_all();
            return Ok(Vec::new());
        };
        let clock_of = |id: &i64| self.clocks.get(id);
        if ids
            .iter()
            .any(|id| clock_of(id).is_some_and(|clock| clock.deleted))
        {
            return Err(TagStateError::AssetDeleted);
        }
        if ids
            .iter()
            .any(|id| clock_of(id).is_some_and(|clock| clock.active_mutation.is_some()))
        {
            return Err(TagStateError::OutOfOrder);
        }
        // One pass per ID: a held clock leaves the inactive list before the next
        // prune.
        Ok(ids
            .into_iter()
            .map(|asset_id| {
                self.clock(asset_id);
                let generation = self.next_generation();
                if let Some(clock) = self.clocks.get_mut(&asset_id) {
                    clock.generation = generation;
                    clock.active_mutation = Some(generation);
                }
                self.inactive.remove(asset_id);
                TagToken {
                    asset_id,
                    epoch: current_epoch,
                    generation,
                }
            })
            .collect())
    }

    /// Ends an admitted write, started or not, and frees its place in the queues.
    /// Returns the turns that became ready by it, in admission order. Releasing a
    /// turn twice does nothing.
    pub fn release(&mut self, turn: TurnId) -> Vec<TurnId> {
        let Some(released) = self.turns.remove(&turn) else {
            return Vec::new();
        };
        match released.kind {
            TurnKind::Assets(ids) => {
                for id in ids {
                    if self.tails.get(&id) == Some(&turn) {
                        self.tails.remove(&id);
                    }
                }
            }
            TurnKind::Library => {
                if self.library_tail == Some(turn) {
                    self.library_tail = None;
                }
                // Reads that started while the write ran saw the old tags.
                if released.used && released.epoch == self.epoch && !self.disposed {
                    self.invalidate_all();
                }
            }
        }
        self.turns
            .iter_mut()
            .filter_map(|(id, waiting)| {
                (waiting.waiting_on.remove(&turn) && waiting.waiting_on.is_empty()).then_some(*id)
            })
            .collect()
    }

    /// Ends the hold of a write on one asset. With `tags` (the canonical list of the
    /// backend's answer) the list is published; without, a list known before stays.
    /// `false`: the token is not the asset's running write (another identity, a
    /// deletion, settled before) and nothing was published.
    pub fn settle_mutation(&mut self, token: &TagToken, tags: Option<Vec<String>>) -> bool {
        if self.disposed || token.epoch != self.epoch {
            return false;
        }
        let asset_id = token.asset_id;
        let clock = self.clock(asset_id);
        if clock.active_mutation != Some(token.generation) || clock.generation != token.generation {
            return false;
        }
        let deleted = clock.deleted;
        // Advancing again invalidates detail reads that started while the write was
        // pending.
        let generation = self.next_generation();
        if let Some(clock) = self.clocks.get_mut(&asset_id) {
            clock.generation = generation;
            clock.active_mutation = None;
        }
        if deleted {
            return false;
        }
        match tags {
            Some(tags) => {
                let tags = normalize_tags(tags);
                self.entries
                    .insert(asset_id, StoredTags { tags, generation });
            }
            None => {
                if let Some(previous) = self.entries.get_mut(&asset_id) {
                    previous.generation = generation;
                }
            }
        }
        self.revision += 1;
        if !self.pins.contains_key(&asset_id) {
            self.inactive.insert(asset_id);
        }
        self.prune();
        true
    }

    /// Requests a maintenance barrier (`runWithMutationBarrier`): from now on new
    /// writes are refused. The operation may run once
    /// [`poll_barrier`](Self::poll_barrier) hands the barrier out.
    pub fn request_barrier(&mut self) -> Result<BarrierId, TagStateError> {
        if self.disposed {
            return Err(TagStateError::Unavailable);
        }
        self.last_barrier += 1;
        let id = BarrierId(self.last_barrier);
        self.barriers.push_back(Barrier { id, granted: false });
        Ok(id)
    }

    /// The barrier whose operation may start now, handed out once: the first
    /// requested barrier, as soon as no admitted write is left.
    pub fn poll_barrier(&mut self) -> Option<BarrierId> {
        if self.disposed || !self.turns.is_empty() {
            return None;
        }
        let barrier = self
            .barriers
            .front_mut()
            .filter(|barrier| !barrier.granted)?;
        barrier.granted = true;
        Some(barrier.id)
    }

    /// Ends a barrier, granted or still waiting. Writes are admitted again when no
    /// barrier is left. `false`: no such barrier.
    pub fn release_barrier(&mut self, id: BarrierId) -> bool {
        let before = self.barriers.len();
        self.barriers.retain(|barrier| barrier.id != id);
        self.barriers.len() != before
    }

    /// Marks the asset as deleted: its tags go, reads and queued writes on it are
    /// rejected, and the tombstone stays until the record is evicted or the identity
    /// is reset. With `identity`, nothing happens when that token is of another
    /// epoch.
    pub fn remove(&mut self, asset_id: i64, identity: Option<&TagToken>) -> bool {
        if self.disposed || identity.is_some_and(|token| token.epoch != self.epoch) {
            return false;
        }
        self.clock(asset_id);
        let generation = self.next_generation();
        if let Some(clock) = self.clocks.get_mut(&asset_id) {
            clock.generation = generation;
            clock.active_mutation = None;
            clock.deleted = true;
        }
        self.entries.remove(&asset_id);
        if !self.pins.contains_key(&asset_id) {
            self.inactive.insert(asset_id);
        }
        self.prune();
        // Consumers are always told: a deletion invalidates their own caches even
        // when no tags were loaded.
        self.revision += 1;
        true
    }

    /// Starts a new identity epoch (the library was cleared or replaced): every tag
    /// list, clock and tombstone goes and the queues start empty. Writes admitted
    /// before still count for [`is_idle`](Self::is_idle) and are refused when they
    /// try to start.
    pub fn reset(&mut self) {
        self.epoch += 1;
        self.tails.clear();
        self.library_tail = None;
        self.entries.clear();
        self.clocks.clear();
        self.inactive.clear();
        self.revision += 1;
    }

    /// The owner is going away: nothing is admitted, started or published any
    /// more. Returns the barriers that were still waiting; their operations must
    /// not run.
    pub fn dispose(&mut self) -> Vec<BarrierId> {
        self.disposed = true;
        let (granted, waiting): (Vec<Barrier>, Vec<Barrier>) = std::mem::take(&mut self.barriers)
            .into_iter()
            .partition(|barrier| barrier.granted);
        self.barriers = granted.into();
        waiting.into_iter().map(|barrier| barrier.id).collect()
    }
}

#[cfg(test)]
mod tests;
