//! The library model (`createLibrary` of the Solid frontend): the query, the
//! canonical tags, the details cache, the known tags and every metadata write, as
//! one state machine.
//!
//! [`Library`] follows the convention of [`crate::model`]. It embeds the query model
//! ([`AssetQuery`]) and the vocabulary ([`KnownTags`]), whose inputs and effects it
//! wraps, and owns [`AssetTagState`] and [`DetailsCache`], which have no effects of
//! their own. [`run_library_effect`] runs its effects through the `Backend` port.
//! Contract: `docs/subsystems/search-tags-and-media-groups.md` ("Metadata
//! coordination", "GPUI port: metadata, mutations and the vocabulary").
//!
//! # Writes
//!
//! [`LibraryInput::Mutate`] admits a [`Mutation`] to the write queue (ADR 0001). A
//! write on idle assets is sent in the same turn; one behind another write is sent
//! when that one ended, whether it succeeded or failed, and a write on several
//! assets waits for all of them and is never sent for fewer. Nothing is published
//! when a write is sent. When the backend answered:
//!
//! 1. every asset the answer acknowledges gets its canonical values: the tag list in
//!    the tag state, the cached details (the copy read when the write *started*,
//!    never one captured while it was queued), the cached query rows (never tags);
//! 2. [`LibraryEvent::Committed`] tells other models what was published;
//! 3. the queue turn is released;
//! 4. the query is refreshed when the applied filters can have changed their result,
//!    and the vocabulary when the write changed it;
//! 5. [`LibraryEvent::MutationSettled`] answers the caller, and the writes that were
//!    waiting start.
//!
//! A failed write publishes nothing and releases its turn. A write is refused
//! without a backend call during maintenance, after disposal, when its asset was
//! deleted or the identity changed while it waited, and, for a tag edit, while the
//! asset's complete tag list is not known ([`MutationError`]).
//!
//! A write on a selection spec takes the queue's library-wide turn: every earlier
//! write ends first, later writes wait, and every cached tag list and detail is
//! dropped when it starts and again when it ends.
//!
//! # Reads
//!
//! [`LibraryInput::LoadDetails`] answers from the cache or reads the asset once for
//! all its readers ([`LibraryEvent::DetailsLoaded`]). The details cache is emptied
//! whenever the query epoch changes, and a read overtaken by that, by a reset or by
//! the disposal is answered with nothing at that moment.
//!
//! # Maintenance
//!
//! [`LibraryInput::BeginMaintenance`] puts up the mutation barrier for an operation
//! that rewrites metadata behind the models' back (CSV import, restore, clear): new
//! writes are refused at once, and [`LibraryEvent::MaintenanceReady`] follows when
//! every admitted write has ended. The caller runs its operation and ends with
//! [`LibraryInput::EndMaintenance`] and the reset the operation calls for, which is
//! applied before the barrier comes down, whether the operation succeeded or not.

use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

use media_tagger_core::models::{AssetDetails, SelectionMutationResult, TagQueryImpact};

use super::{
    details::{DetailsCache, DetailsResult, LoadStart},
    filters::LibraryFilters,
    mutations::{
        outcome_of, patches, request_of, selection_request_of, MetadataCommit, MetadataPatch,
        Mutation, MutationError, MutationOutcome, Sent, WriteReply, WriteRequest,
    },
    page_cache::AssetPatch,
    query::{
        run_query_effect, AssetQuery, QueryConfig, QueryEffect, QueryError, QueryEvent, QueryInput,
        Ticket,
    },
    query_impact::{favorite_mutation_requires_refresh, tag_mutation_requires_refresh},
    tag_state::{AssetTagState, AssetTags, BarrierId, TagStateError, TagToken, TurnId},
    vocabulary::{
        run_vocabulary_effect, KnownTags, VocabularyEffect, VocabularyEvent, VocabularyInput,
        VocabularyResult,
    },
};
use crate::{
    backend::{Backend, BackendError, BackendResult},
    model::{Dispatch, Model, RequestToken, Runner, Tokens},
};

#[cfg(test)]
mod tests;

/// What is thrown away when the library changed behind the models' back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryReset {
    /// `resetIdentity`: every tag list, tombstone and cached detail; a new identity
    /// epoch. For an operation that rewrote tags (CSV import).
    Identity,
    /// `handleImportDbRestored`: [`Identity`](Self::Identity) and the query's rows and
    /// session. The known tags stay until the next refresh replaces them.
    Restored,
    /// `handleLibraryCleared`: [`Restored`](Self::Restored), the known tags and the
    /// gallery's range.
    Cleared,
}

/// Why a library refresh did not end with a fresh result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RefreshError {
    Query(QueryError),
    Vocabulary(BackendError),
}

/// Commands and backend replies.
#[derive(Debug)]
pub enum LibraryInput {
    /// A command, a backend reply or a timer of the query model.
    Query(QueryInput),
    /// A command or a backend reply of the known tags.
    Vocabulary(VocabularyInput),
    /// `refreshLibrary`: starts the query again and reads the known tags again. With a
    /// ticket, [`LibraryEvent::Refreshed`] follows when both have ended.
    Refresh {
        ticket: Option<Ticket>,
    },
    /// The details of an asset with its complete, canonical tag list.
    LoadDetails {
        ticket: Ticket,
        asset_id: i64,
    },
    /// An editor shows the asset: its tag record is kept whatever else is evicted.
    /// Every pin needs one [`UnpinTags`](Self::UnpinTags).
    PinTags(i64),
    UnpinTags(i64),
    /// A metadata write; answered with [`LibraryEvent::MutationSettled`].
    Mutate {
        ticket: Ticket,
        mutation: Mutation,
    },
    /// `waitForIdle`: answered with [`LibraryEvent::Idle`] once no admitted write is
    /// left.
    WaitForIdle {
        ticket: Ticket,
    },
    /// Puts up the mutation barrier; answered with [`LibraryEvent::MaintenanceReady`]
    /// or [`LibraryEvent::MaintenanceRejected`].
    BeginMaintenance {
        ticket: Ticket,
    },
    /// Takes the barrier of `ticket` down, after `reset` when the barrier had been
    /// granted.
    EndMaintenance {
        ticket: Ticket,
        reset: Option<LibraryReset>,
    },
    /// The library changed outside a maintenance barrier.
    Reset(LibraryReset),
    /// The owner is going away: pending reads and waiting maintenance are answered,
    /// and nothing is admitted, requested or published again.
    Dispose,

    DetailsLoaded {
        token: RequestToken,
        result: BackendResult<Option<AssetDetails>>,
    },
    WriteFinished {
        token: RequestToken,
        result: BackendResult<WriteReply>,
    },
}

/// Work for the runner.
#[derive(Debug, Clone, PartialEq)]
pub enum LibraryEffect {
    Query(QueryEffect),
    Vocabulary(VocabularyEffect),
    /// `get_asset_details`; answered with [`LibraryInput::DetailsLoaded`].
    LoadDetails {
        token: RequestToken,
        asset_id: i64,
    },
    /// One backend call of a write; answered with [`LibraryInput::WriteFinished`].
    Write {
        token: RequestToken,
        request: WriteRequest,
    },
}

/// Answers to ticketed commands, and what writes published.
#[derive(Debug)]
pub enum LibraryEvent {
    DetailsLoaded {
        ticket: Ticket,
        result: DetailsResult,
    },
    MutationSettled {
        ticket: Ticket,
        result: Result<MutationOutcome, MutationError>,
    },
    /// A write published canonical values, or an asset was deleted.
    Committed(MetadataCommit),
    Refreshed {
        ticket: Ticket,
        result: Result<(), RefreshError>,
    },
    /// A ticketed [`VocabularyInput`] refresh ended.
    VocabularySettled {
        ticket: Ticket,
        result: VocabularyResult,
    },
    Idle {
        ticket: Ticket,
    },
    /// Every write admitted before the barrier has ended: the operation may run.
    MaintenanceReady {
        ticket: Ticket,
    },
    /// The barrier was never granted; the operation must not run.
    MaintenanceRejected {
        ticket: Ticket,
        error: TagStateError,
    },
}

/// An admitted write that waits for its turn.
#[derive(Debug)]
struct Queued {
    ticket: Ticket,
    mutation: Mutation,
}

/// A write that was sent to the backend.
#[derive(Debug)]
enum Flight {
    Assets(AssetFlight),
    Selection {
        turn: TurnId,
        ticket: Ticket,
        merges_tags: bool,
    },
}

#[derive(Debug)]
struct AssetFlight {
    turn: TurnId,
    ticket: Ticket,
    sent: Sent,
    /// One exclusive token per asset of the write.
    tokens: Vec<TagToken>,
    /// The cached details of its assets when the write started.
    cached: Vec<(i64, Arc<AssetDetails>)>,
    details_generation: u64,
}

#[derive(Debug)]
struct Maintenance {
    barrier: BarrierId,
    ticket: Ticket,
    granted: bool,
}

/// Who waits for a vocabulary pass, by the ticket the model gave the pass.
#[derive(Debug)]
enum VocabularyWaiter {
    /// A refresh asked for from outside, with the caller's ticket.
    Reader(Ticket),
    Completion(u64),
}

/// An answer that waits for the query refresh and the vocabulary pass it started.
#[derive(Debug)]
struct Completion {
    id: u64,
    vocabulary: Option<VocabularyResult>,
    kind: CompletionKind,
}

#[derive(Debug)]
enum CompletionKind {
    Refresh {
        ticket: Ticket,
    },
    Mutation {
        ticket: Ticket,
        outcome: MutationOutcome,
    },
}

#[derive(Debug)]
pub struct Library {
    query: AssetQuery,
    tags: AssetTagState,
    details: DetailsCache,
    vocabulary: KnownTags,
    tokens: Tokens,
    /// The query epoch the details cache belongs to.
    details_epoch: u64,
    queued: HashMap<TurnId, Queued>,
    flights: HashMap<RequestToken, Flight>,
    maintenance: Vec<Maintenance>,
    idle_waiters: Vec<Ticket>,
    vocabulary_waiters: HashMap<Ticket, VocabularyWaiter>,
    completions: Vec<Completion>,
    last_internal: u64,
    events: VecDeque<LibraryEvent>,
    disposed: bool,
}

impl Default for Library {
    fn default() -> Self {
        Self::new(QueryConfig::default())
    }
}

impl Library {
    /// An unfiltered library with nothing loaded.
    pub fn new(config: QueryConfig) -> Self {
        Self::with_filters(config, LibraryFilters::default())
    }

    pub fn with_filters(config: QueryConfig, filters: LibraryFilters) -> Self {
        let query = AssetQuery::with_filters(config, filters);
        Self {
            details_epoch: query.query_epoch(),
            query,
            tags: AssetTagState::new(),
            details: DetailsCache::new(),
            vocabulary: KnownTags::new(),
            tokens: Tokens::default(),
            queued: HashMap::new(),
            flights: HashMap::new(),
            maintenance: Vec::new(),
            idle_waiters: Vec::new(),
            vocabulary_waiters: HashMap::new(),
            completions: Vec::new(),
            last_internal: 0,
            events: VecDeque::new(),
            disposed: false,
        }
    }

    /// The query model: the result's total, the cached rows, the session.
    pub fn query(&self) -> &AssetQuery {
        &self.query
    }

    /// The tag state: identity epoch, revision, whether a token is still current.
    pub fn tags(&self) -> &AssetTagState {
        &self.tags
    }

    /// The details cache: its generation and size.
    pub fn details(&self) -> &DetailsCache {
        &self.details
    }

    /// The vocabulary model.
    pub fn vocabulary(&self) -> &KnownTags {
        &self.vocabulary
    }

    /// The known tags of the library as last listed completely.
    pub fn known_tags(&self) -> &Arc<Vec<String>> {
        self.vocabulary.known_tags()
    }

    /// The complete, canonical tag list of an asset when it is known.
    pub fn asset_tags(&self, asset_id: i64) -> Option<AssetTags<'_>> {
        self.tags.peek(asset_id)
    }

    /// Whether the tags of the asset may be edited: its complete tag list is known.
    /// Until then an editor stays disabled, and a tag edit is refused.
    pub fn can_edit_tags(&self, asset_id: i64) -> bool {
        self.tags.is_known(asset_id)
    }

    /// The cached details of an asset, while they are still current.
    pub fn cached_details(&self, asset_id: i64) -> Option<&Arc<AssetDetails>> {
        self.details.peek(&self.tags, asset_id)
    }

    /// Whether a library refresh waits for the backend: the query's start or a pass
    /// over the known tags.
    pub fn refreshing(&self) -> bool {
        self.query.query_pending() || self.vocabulary.refreshing()
    }

    /// Admitted writes that have not ended, queued or sent.
    pub fn pending_writes(&self) -> usize {
        self.queued.len() + self.flights.len()
    }

    /// Whether a maintenance barrier is up: writes are refused.
    pub fn maintenance_active(&self) -> bool {
        self.tags.barrier_active()
    }

    pub fn is_disposed(&self) -> bool {
        self.disposed
    }

    /// The answers and commits since the last call, oldest first.
    pub fn take_events(&mut self) -> Vec<LibraryEvent> {
        self.events.drain(..).collect()
    }

    /// The answers to ticketed reads of the query model since the last call.
    pub fn take_query_events(&mut self) -> Vec<QueryEvent> {
        self.query.take_events()
    }

    fn emit(&mut self, event: LibraryEvent) {
        self.events.push_back(event);
    }

    fn settle_mutation(&mut self, ticket: Ticket, result: Result<MutationOutcome, MutationError>) {
        self.emit(LibraryEvent::MutationSettled { ticket, result });
    }

    /// A ticket of this model's own, for a vocabulary pass it waits for.
    fn internal_ticket(&mut self, waiter: VocabularyWaiter) -> Ticket {
        self.last_internal += 1;
        let ticket = Ticket(self.last_internal);
        self.vocabulary_waiters.insert(ticket, waiter);
        ticket
    }

    /// Empties the details cache and answers the reads that were on their way.
    fn clear_details(&mut self) {
        for ticket in self.details.clear() {
            self.emit(LibraryEvent::DetailsLoaded {
                ticket,
                result: Ok(None),
            });
        }
    }

    /// Hands `input` to the query model. Details belong to one query epoch.
    fn query_input(&mut self, input: QueryInput, effects: &mut Vec<LibraryEffect>) {
        effects.extend(
            self.query
                .handle(input)
                .into_iter()
                .map(LibraryEffect::Query),
        );
        if self.query.query_epoch() != self.details_epoch {
            self.details_epoch = self.query.query_epoch();
            self.clear_details();
        }
    }

    fn refresh_query(&mut self, effects: &mut Vec<LibraryEffect>) {
        self.query_input(QueryInput::Refresh, effects);
    }

    /// Hands `input` to the vocabulary and passes its answers on.
    fn vocabulary_input(&mut self, input: VocabularyInput, effects: &mut Vec<LibraryEffect>) {
        effects.extend(
            self.vocabulary
                .handle(input)
                .into_iter()
                .map(LibraryEffect::Vocabulary),
        );
        for VocabularyEvent::Settled { ticket, result } in self.vocabulary.take_events() {
            match self.vocabulary_waiters.remove(&ticket) {
                Some(VocabularyWaiter::Reader(ticket)) => {
                    self.emit(LibraryEvent::VocabularySettled { ticket, result });
                }
                Some(VocabularyWaiter::Completion(id)) => {
                    if let Some(completion) = self
                        .completions
                        .iter_mut()
                        .find(|completion| completion.id == id)
                    {
                        completion.vocabulary = Some(result);
                    }
                }
                None => {}
            }
        }
    }

    /// A vocabulary input from outside: a caller's ticket is answered as a
    /// [`LibraryEvent::VocabularySettled`].
    fn vocabulary_command(&mut self, input: VocabularyInput, effects: &mut Vec<LibraryEffect>) {
        let input = match input {
            VocabularyInput::Refresh { ticket } => VocabularyInput::Refresh {
                ticket: ticket.map(|ticket| self.internal_ticket(VocabularyWaiter::Reader(ticket))),
            },
            VocabularyInput::RefreshAfterMutation {
                vocabulary_changed,
                ticket,
            } => VocabularyInput::RefreshAfterMutation {
                vocabulary_changed,
                ticket: ticket.map(|ticket| self.internal_ticket(VocabularyWaiter::Reader(ticket))),
            },
            other => other,
        };
        self.vocabulary_input(input, effects);
    }

    /// Reads the known tags again and answers `kind` once that pass and the query
    /// refresh that was just started have ended.
    fn complete_after_refresh(&mut self, kind: CompletionKind, effects: &mut Vec<LibraryEffect>) {
        self.last_internal += 1;
        let id = self.last_internal;
        self.completions.push(Completion {
            id,
            vocabulary: None,
            kind,
        });
        let ticket = self.internal_ticket(VocabularyWaiter::Completion(id));
        self.vocabulary_input(
            VocabularyInput::Refresh {
                ticket: Some(ticket),
            },
            effects,
        );
    }

    fn refresh_library(&mut self, ticket: Option<Ticket>, effects: &mut Vec<LibraryEffect>) {
        self.refresh_query(effects);
        match ticket {
            Some(ticket) => {
                self.complete_after_refresh(CompletionKind::Refresh { ticket }, effects);
            }
            None => self.vocabulary_input(VocabularyInput::Refresh { ticket: None }, effects),
        }
    }

    fn load_details(&mut self, ticket: Ticket, asset_id: i64, effects: &mut Vec<LibraryEffect>) {
        let start = self
            .details
            .begin_load(&mut self.tags, &mut self.tokens, ticket, asset_id);
        match start {
            LoadStart::Cached(details) => self.emit(LibraryEvent::DetailsLoaded {
                ticket,
                result: Ok(Some(details)),
            }),
            LoadStart::Joined => {}
            LoadStart::Request(token) => {
                effects.push(LibraryEffect::LoadDetails { token, asset_id });
            }
            LoadStart::Disposed => self.emit(LibraryEvent::DetailsLoaded {
                ticket,
                result: Ok(None),
            }),
        }
    }

    fn on_details_loaded(
        &mut self,
        token: RequestToken,
        result: BackendResult<Option<AssetDetails>>,
    ) {
        let Some((waiters, answer)) = self.details.finish_load(&mut self.tags, token, result)
        else {
            return;
        };
        for ticket in waiters {
            self.emit(LibraryEvent::DetailsLoaded {
                ticket,
                result: answer.clone(),
            });
        }
    }

    fn mutate(&mut self, ticket: Ticket, mutation: Mutation, effects: &mut Vec<LibraryEffect>) {
        let mutation = mutation.normalized();
        let admission = match mutation.asset_ids() {
            Some(ids) => self.tags.enqueue_mutation(&ids),
            None => self.tags.enqueue_library_mutation(),
        };
        match admission {
            Err(error) => self.settle_mutation(ticket, Err(MutationError::Refused(error))),
            Ok(admission) => {
                self.queued
                    .insert(admission.turn, Queued { ticket, mutation });
                if admission.ready {
                    self.start_turns(vec![admission.turn], effects);
                }
            }
        }
    }

    /// Starts the writes whose turn came, and the ones that become ready because a
    /// write before them could not start.
    fn start_turns(&mut self, ready: Vec<TurnId>, effects: &mut Vec<LibraryEffect>) {
        let mut ready: VecDeque<TurnId> = ready.into();
        while let Some(turn) = ready.pop_front() {
            ready.extend(self.start_write(turn, effects));
        }
    }

    /// Sends the write of `turn`. A write that cannot start releases its turn; the
    /// turns that became ready by that are returned.
    fn start_write(&mut self, turn: TurnId, effects: &mut Vec<LibraryEffect>) -> Vec<TurnId> {
        let Some(Queued { ticket, mutation }) = self.queued.remove(&turn) else {
            return Vec::new();
        };
        let Some(ids) = mutation.asset_ids() else {
            return self.start_selection_write(turn, ticket, mutation, effects);
        };
        // Read when the write starts: the writes queued ahead have published by now.
        // Starting the tokens below invalidates these cache entries.
        let cached: Vec<(i64, Arc<AssetDetails>)> = ids
            .iter()
            .filter_map(|id| Some((*id, self.details.get(&self.tags, *id)?)))
            .collect();
        // Canonical metadata may survive a refresh; a captured path or size must not.
        let details_generation = self.details.generation();
        let started = match self.tags.begin(turn) {
            Err(error) => Err(MutationError::Refused(error)),
            Ok(tokens) => {
                let canonical = match &mutation {
                    Mutation::SaveTags { asset_id, .. } => {
                        self.tags.get(*asset_id).map(|known| known.tags.to_vec())
                    }
                    _ => None,
                };
                match request_of(mutation, canonical.as_deref()) {
                    Ok((request, sent)) => Ok((tokens, request, sent)),
                    Err(error) => {
                        for token in &tokens {
                            self.tags.settle_mutation(token, None);
                        }
                        Err(error)
                    }
                }
            }
        };
        match started {
            Ok((tokens, request, sent)) => {
                let token = self.tokens.issue();
                self.flights.insert(
                    token,
                    Flight::Assets(AssetFlight {
                        turn,
                        ticket,
                        sent,
                        tokens,
                        cached,
                        details_generation,
                    }),
                );
                effects.push(LibraryEffect::Write { token, request });
                Vec::new()
            }
            Err(error) => {
                let ready = self.tags.release(turn);
                self.settle_mutation(ticket, Err(error));
                ready
            }
        }
    }

    fn start_selection_write(
        &mut self,
        turn: TurnId,
        ticket: Ticket,
        mutation: Mutation,
        effects: &mut Vec<LibraryEffect>,
    ) -> Vec<TurnId> {
        let merges_tags = matches!(mutation, Mutation::MergeTagsInSelection { .. });
        let started = self
            .tags
            .begin(turn)
            .map_err(MutationError::Refused)
            .and_then(|_| selection_request_of(mutation).ok_or_else(MutationError::unexpected));
        match started {
            Ok(request) => {
                // The affected assets are unknown here: no cached detail can be trusted.
                self.clear_details();
                let token = self.tokens.issue();
                self.flights.insert(
                    token,
                    Flight::Selection {
                        turn,
                        ticket,
                        merges_tags,
                    },
                );
                effects.push(LibraryEffect::Write { token, request });
                Vec::new()
            }
            Err(error) => {
                let ready = self.tags.release(turn);
                self.settle_mutation(ticket, Err(error));
                ready
            }
        }
    }

    fn on_write_finished(
        &mut self,
        token: RequestToken,
        result: BackendResult<WriteReply>,
        effects: &mut Vec<LibraryEffect>,
    ) {
        let ready = match self.flights.remove(&token) {
            Some(Flight::Assets(flight)) => self.finish_asset_write(flight, result, effects),
            Some(Flight::Selection {
                turn,
                ticket,
                merges_tags,
            }) => self.finish_selection_write(turn, ticket, merges_tags, result, effects),
            None => return,
        };
        // After this write's own refreshes and tombstones, so a write queued behind a
        // deletion sees the asset gone.
        self.start_turns(ready, effects);
    }

    fn finish_asset_write(
        &mut self,
        flight: AssetFlight,
        result: BackendResult<WriteReply>,
        effects: &mut Vec<LibraryEffect>,
    ) -> Vec<TurnId> {
        let AssetFlight {
            turn,
            ticket,
            sent,
            tokens,
            cached,
            details_generation,
        } = flight;
        let result =
            result
                .map_err(MutationError::Backend)
                .and_then(|reply| match patches(&sent, &reply) {
                    Some(patches) => Ok((reply, patches)),
                    None => Err(MutationError::unexpected()),
                });
        // Whether at least one asset of the write took the backend's answer.
        let mut accepted = false;
        if let Ok((_, patches)) = &result {
            let patches: HashMap<i64, &MetadataPatch> =
                patches.iter().map(|(id, patch)| (*id, patch)).collect();
            let mut published = Vec::new();
            for token in &tokens {
                let id = token.asset_id();
                let patch = patches.get(&id).copied();
                let tags = patch.and_then(|patch| patch.tags.clone());
                // A token of another identity, or of a deleted asset, publishes nothing.
                if !self.tags.settle_mutation(token, tags) {
                    continue;
                }
                accepted = true;
                let Some(patch) = patch else {
                    continue;
                };
                let mut patch = patch.clone();
                if patch.tags.is_some() {
                    // The list as the tag state holds it now.
                    patch.tags = self.tags.get(id).map(|known| known.tags.to_vec());
                }
                if let Some((_, previous)) = cached.iter().find(|(cached, _)| *cached == id) {
                    self.details.put(
                        &mut self.tags,
                        Arc::new(patch.apply(previous)),
                        details_generation,
                    );
                }
                published.push((id, patch));
            }
            if !published.is_empty() {
                // Summary rows never carry tags.
                let rows = published
                    .iter()
                    .map(|(id, patch)| (*id, patch.summary.clone()))
                    .collect();
                self.query_input(QueryInput::PatchAssets(AssetPatch::Update(rows)), effects);
                self.emit(LibraryEvent::Committed(MetadataCommit::Updated(published)));
            }
        }
        // Settling also advances the generations after a failure, a reset or the
        // disposal.
        for token in &tokens {
            self.tags.settle_mutation(token, None);
        }
        let ready = self.tags.release(turn);
        match result {
            Err(error) => self.settle_mutation(ticket, Err(error)),
            Ok((reply, _)) => {
                if accepted {
                    self.after_write(&sent, &reply, effects);
                }
                match outcome_of(sent, reply) {
                    Ok(outcome @ MutationOutcome::Deleted(_)) if accepted => {
                        // A deletion answers once the result without the asset and
                        // the known tags have been read again.
                        self.complete_after_refresh(
                            CompletionKind::Mutation { ticket, outcome },
                            effects,
                        );
                    }
                    outcome => self.settle_mutation(ticket, outcome),
                }
            }
        }
        ready
    }

    fn finish_selection_write(
        &mut self,
        turn: TurnId,
        ticket: Ticket,
        merges_tags: bool,
        result: BackendResult<WriteReply>,
        effects: &mut Vec<LibraryEffect>,
    ) -> Vec<TurnId> {
        // Detail reads that ran during the write saw old metadata.
        self.clear_details();
        let ready = self.tags.release(turn);
        let outcome = match result {
            Ok(WriteReply::Selection(result)) => {
                match &result {
                    // Nothing was written; the refresh tells the selection that its
                    // session is gone.
                    SelectionMutationResult::Stale => self.refresh_query(effects),
                    SelectionMutationResult::Ready {
                        query_impact,
                        vocabulary_changed,
                        ..
                    } if merges_tags => {
                        self.tag_refresh(query_impact, *vocabulary_changed, false, effects);
                    }
                    // Summaries carry the favorite and the group: visible rows are
                    // read again.
                    SelectionMutationResult::Ready { .. } => self.refresh_query(effects),
                }
                Ok(MutationOutcome::Selection(result))
            }
            Ok(_) => Err(MutationError::unexpected()),
            Err(error) => Err(MutationError::Backend(error)),
        };
        self.settle_mutation(ticket, outcome);
        ready
    }

    /// `tagRefresh`: the query when the applied filters name a changed tag (or an
    /// asset went missing), the vocabulary as the write asks for.
    fn tag_refresh(
        &mut self,
        impact: &TagQueryImpact,
        vocabulary_changed: bool,
        missing: bool,
        effects: &mut Vec<LibraryEffect>,
    ) {
        if missing || tag_mutation_requires_refresh(impact, self.query.filters()) {
            self.refresh_query(effects);
        }
        self.vocabulary_input(
            VocabularyInput::RefreshAfterMutation {
                vocabulary_changed,
                ticket: None,
            },
            effects,
        );
    }

    /// What follows a write the backend accepted and at least one asset took.
    fn after_write(&mut self, sent: &Sent, reply: &WriteReply, effects: &mut Vec<LibraryEffect>) {
        match (sent, reply) {
            (Sent::SaveTags { .. }, WriteReply::TagsSet(summary)) => self.tag_refresh(
                &summary.query_impact,
                summary.vocabulary_changed,
                false,
                effects,
            ),
            (Sent::MergeTags { requested }, WriteReply::TagsMerged(summary)) => self.tag_refresh(
                &summary.query_impact,
                summary.vocabulary_changed,
                summary.processed_assets < *requested,
                effects,
            ),
            (Sent::SetFavorite { favorite, .. }, _) => {
                if favorite_mutation_requires_refresh(
                    self.query.filters().favorites_only,
                    *favorite,
                ) {
                    self.refresh_query(effects);
                }
            }
            (Sent::ToggleFavorites { requested }, WriteReply::FavoritesToggled(summary)) => {
                if self.query.filters().favorites_only
                    || summary.processed_asset_ids.len() < *requested
                {
                    self.refresh_query(effects);
                }
            }
            (Sent::SetMediaGroup { .. } | Sent::SetMediaGroups { .. }, _) => {
                self.refresh_query(effects);
            }
            (Sent::DeleteAsset { asset_id }, _) => {
                let asset_id = *asset_id;
                // The tombstone comes before the next queued write starts.
                self.tags.remove(asset_id, None);
                self.details.remove(asset_id);
                self.query_input(
                    QueryInput::PatchAssets(AssetPatch::Remove(vec![asset_id])),
                    effects,
                );
                self.emit(LibraryEvent::Committed(MetadataCommit::Deleted(vec![
                    asset_id,
                ])));
                self.refresh_query(effects);
            }
            _ => {}
        }
    }

    fn begin_maintenance(&mut self, ticket: Ticket) {
        match self.tags.request_barrier() {
            Ok(barrier) => self.maintenance.push(Maintenance {
                barrier,
                ticket,
                granted: false,
            }),
            Err(error) => self.emit(LibraryEvent::MaintenanceRejected { ticket, error }),
        }
    }

    fn end_maintenance(
        &mut self,
        ticket: Ticket,
        reset: Option<LibraryReset>,
        effects: &mut Vec<LibraryEffect>,
    ) {
        let Some(index) = self
            .maintenance
            .iter()
            .position(|maintenance| maintenance.ticket == ticket)
        else {
            return;
        };
        let maintenance = self.maintenance.remove(index);
        // Inside the barrier: no write can start between the operation and the reset.
        if let Some(reset) = reset.filter(|_| maintenance.granted && !self.disposed) {
            self.reset(reset, effects);
        }
        self.tags.release_barrier(maintenance.barrier);
    }

    fn reset(&mut self, reset: LibraryReset, effects: &mut Vec<LibraryEffect>) {
        self.tags.reset();
        self.clear_details();
        if reset == LibraryReset::Identity {
            return;
        }
        self.query_input(QueryInput::Reset, effects);
        if reset == LibraryReset::Cleared {
            self.vocabulary_input(VocabularyInput::Reset, effects);
            self.query_input(QueryInput::SetViewport(None), effects);
        }
    }

    fn dispose(&mut self, effects: &mut Vec<LibraryEffect>) {
        if self.disposed {
            return;
        }
        self.disposed = true;
        for barrier in self.tags.dispose() {
            if let Some(index) = self
                .maintenance
                .iter()
                .position(|maintenance| maintenance.barrier == barrier)
            {
                let ticket = self.maintenance.remove(index).ticket;
                self.emit(LibraryEvent::MaintenanceRejected {
                    ticket,
                    error: TagStateError::Unavailable,
                });
            }
        }
        for ticket in self.details.dispose() {
            self.emit(LibraryEvent::DetailsLoaded {
                ticket,
                result: Ok(None),
            });
        }
        self.vocabulary_input(VocabularyInput::Dispose, effects);
        self.query_input(QueryInput::Dispose, effects);
    }

    /// What became true by the input that was just applied: a barrier may be
    /// granted, the queue may have drained, a refresh may have ended.
    fn settle(&mut self) {
        if let Some(barrier) = self.tags.poll_barrier() {
            if let Some(maintenance) = self
                .maintenance
                .iter_mut()
                .find(|maintenance| maintenance.barrier == barrier)
            {
                maintenance.granted = true;
                let ticket = maintenance.ticket;
                self.emit(LibraryEvent::MaintenanceReady { ticket });
            }
        }
        if self.tags.is_idle() || self.disposed {
            for ticket in std::mem::take(&mut self.idle_waiters) {
                self.emit(LibraryEvent::Idle { ticket });
            }
        }
        if self.completions.is_empty() || self.query.query_pending() {
            return;
        }
        let (ended, waiting): (Vec<Completion>, Vec<Completion>) =
            std::mem::take(&mut self.completions)
                .into_iter()
                .partition(|completion| completion.vocabulary.is_some());
        self.completions = waiting;
        for completion in ended {
            match completion.kind {
                CompletionKind::Mutation { ticket, outcome } => {
                    // The write is durable whatever its secondary refreshes did.
                    self.settle_mutation(ticket, Ok(outcome));
                }
                CompletionKind::Refresh { ticket } => {
                    let query_error = match self.query.session_id() {
                        Some(_) => None,
                        None => self.query.load_error().cloned(),
                    };
                    let result = match (query_error, completion.vocabulary) {
                        (Some(error), _) => Err(RefreshError::Query(error)),
                        (None, Some(Err(error))) => Err(RefreshError::Vocabulary(error)),
                        (None, _) => Ok(()),
                    };
                    self.emit(LibraryEvent::Refreshed { ticket, result });
                }
            }
        }
    }
}

impl Model for Library {
    type Input = LibraryInput;
    type Effect = LibraryEffect;

    fn handle(&mut self, input: LibraryInput) -> Vec<LibraryEffect> {
        let mut effects = Vec::new();
        match input {
            LibraryInput::Query(input) => self.query_input(input, &mut effects),
            LibraryInput::Vocabulary(input) => self.vocabulary_command(input, &mut effects),
            LibraryInput::Refresh { ticket } => self.refresh_library(ticket, &mut effects),
            LibraryInput::LoadDetails { ticket, asset_id } => {
                self.load_details(ticket, asset_id, &mut effects);
            }
            LibraryInput::PinTags(asset_id) => self.tags.pin(asset_id),
            LibraryInput::UnpinTags(asset_id) => self.tags.unpin(asset_id),
            LibraryInput::Mutate { ticket, mutation } => {
                self.mutate(ticket, mutation, &mut effects);
            }
            LibraryInput::WaitForIdle { ticket } => self.idle_waiters.push(ticket),
            LibraryInput::BeginMaintenance { ticket } => self.begin_maintenance(ticket),
            LibraryInput::EndMaintenance { ticket, reset } => {
                self.end_maintenance(ticket, reset, &mut effects);
            }
            LibraryInput::Reset(reset) => self.reset(reset, &mut effects),
            LibraryInput::Dispose => self.dispose(&mut effects),
            LibraryInput::DetailsLoaded { token, result } => {
                self.on_details_loaded(token, result);
            }
            LibraryInput::WriteFinished { token, result } => {
                self.on_write_finished(token, result, &mut effects);
            }
        }
        self.settle();
        effects
    }
}

/// Runs one effect of a [`Library`] through the backend port. `wrap` turns the
/// library's inputs into the inputs of the model that owns the inbox: the identity
/// for a library driven on its own, the wrapping variant for a parent that embeds it.
pub fn run_library_effect<I, W>(
    backend: &Arc<dyn Backend>,
    effect: LibraryEffect,
    dispatch: &Dispatch<I>,
    wrap: W,
) where
    I: Send + 'static,
    W: Fn(LibraryInput) -> I + Send + 'static,
{
    match effect {
        LibraryEffect::Query(effect) => {
            run_query_effect(backend, effect, dispatch, move |input| {
                wrap(LibraryInput::Query(input))
            });
        }
        LibraryEffect::Vocabulary(effect) => {
            run_vocabulary_effect(backend, effect, dispatch, move |input| {
                wrap(LibraryInput::Vocabulary(input))
            });
        }
        LibraryEffect::LoadDetails { token, asset_id } => {
            let backend = Arc::clone(backend);
            dispatch.call(
                "get_asset_details",
                move || backend.get_asset_details(asset_id),
                move |result| wrap(LibraryInput::DetailsLoaded { token, result }),
            );
        }
        LibraryEffect::Write { token, request } => {
            let backend = Arc::clone(backend);
            dispatch.call(
                request.operation(),
                move || request.run(backend.as_ref()),
                move |result| wrap(LibraryInput::WriteFinished { token, result }),
            );
        }
    }
}

/// Runs the effects of a library that is driven on its own.
pub struct LibraryRunner {
    backend: Arc<dyn Backend>,
}

impl LibraryRunner {
    pub fn new(backend: Arc<dyn Backend>) -> Self {
        Self { backend }
    }
}

impl Runner<Library> for LibraryRunner {
    fn run(&mut self, effect: LibraryEffect, dispatch: &Dispatch<LibraryInput>) {
        run_library_effect(&self.backend, effect, dispatch, std::convert::identity);
    }
}
