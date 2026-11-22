//! The bulk model (`createBulk.ts` of the Solid frontend): what the dock edits for the
//! current selection.
//!
//! [`Bulk`] is a state machine like every model of this crate, with one difference in
//! form: it reads the library and the selection directly, through the [`BulkEnv`] its
//! owner lends it with every input, where the Solid model reads reactive getters. It
//! never changes them: everything it wants done is an effect, and the runtime hands
//! the library and the selection those effects in the same turn
//! ([`BulkEffect::Library`], [`BulkEffect::Selection`]) and runs the reads of selected
//! summaries on the executor ([`BulkEffect::LoadSummaries`]). After every input any
//! model took, the runtime sends [`BulkInput::Sync`], which is where the Solid model's
//! effects run: drafts are reset for a new selection or identity, the summaries of the
//! selection are brought up to date, the group draft and order are derived once per
//! selection, the single selected asset's tag record is pinned and its complete tag
//! list is read.
//!
//! # Writes
//!
//! A write goes through the library's mutation queue ([`LibraryInput::Mutate`]) with a
//! ticket of the bulk model's own range ([`BULK_TICKET_BASE`]) and is answered by
//! [`LibraryEvent::MutationSettled`]. Nothing is published before that answer: the
//! library publishes the canonical values of the assets the backend acknowledged, and
//! this model only keeps feedback (what was added, how many assets took it, what
//! failed). Feedback of a write whose selection or identity is gone is dropped
//! (`context.current()`).
//!
//! - One selected asset: tags replace (`set_asset_tags` over the canonical list at
//!   the head of the asset's queue). Several: tags are added only
//!   (`merge_asset_tags_bulk`). A range selection: one all-or-nothing write on the
//!   selection spec (`apply_selection_mutation`), `Stale` counts as failed.
//! - A partial answer publishes only the acknowledged IDs; [`Bulk::partial`] says how
//!   many of the requested assets took the write.
//! - A favorite toggle unselects the IDs the backend did not acknowledge.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use media_tagger_core::models::{AssetSummary, SelectionMutationResult};

use super::{
    group_order::{derive_bulk_group_selection_state, normalize_group_key},
    selected_summaries::{SelectedSummaries, SummaryLoad},
    tag_merge::{merge_tag_lists, normalize_bulk_tag},
};
use crate::{
    backend::{BackendResult, MediaGroupUpdate},
    gallery::selection::{
        Selection, SelectionInput, SelectionNotice, SelectionWrite, SELECTION_TICKET_BASE,
    },
    library::{
        Library, LibraryEvent, LibraryInput, MetadataCommit, Mutation, MutationError,
        MutationOutcome, TagEdit, TagToken, Ticket,
    },
};

/// The first [`Ticket`] value of the bulk model's library commands; its range ends
/// where the selection's begins ([`SELECTION_TICKET_BASE`]). Every other caller of
/// ticketed library commands chooses its tickets below it.
pub const BULK_TICKET_BASE: u64 = 1 << 61;

/// Reads of the complete tag list that a write or a refresh discarded are repeated
/// this often (`readTags`).
const TAG_READ_ATTEMPTS: u8 = 3;

/// What the model reads of its neighbours while it handles one input.
#[derive(Debug, Clone, Copy)]
pub struct BulkEnv<'a> {
    pub library: &'a Library,
    pub selection: &'a Selection,
    /// The settings page is shown: no tag list is read for the dock behind it.
    pub settings_open: bool,
}

/// How many of the assets a write asked for took it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartialResult {
    pub processed: usize,
    pub requested: usize,
}

impl PartialResult {
    /// Fewer assets took the write than were asked for.
    pub fn is_partial(&self) -> bool {
        self.processed < self.requested
    }
}

/// How a group write ended (`BulkGroupSaveResult`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupSaveResult {
    Saved,
    Failed,
    /// Nothing was sent, or the selection changed before the answer.
    Ignored,
    Partial(PartialResult),
}

/// `tagMode`: what a tag edit does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagMode {
    None,
    /// One asset held by ID: its tags are replaced.
    Single,
    /// Several assets, or a range: tags are added only.
    Multiple,
}

/// What the dock asks of the model (the commands of `BulkController`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BulkCommand {
    /// `onGroupKeyDraftChange`.
    SetGroupKeyDraft(String),
    /// `onAddTag`; answered with [`BulkEvent::TagSettled`].
    AddTag { request: u64, tag: String },
    /// `onRemoveTag`; answered with [`BulkEvent::TagSettled`].
    RemoveTag { request: u64, tag: String },
    /// `onToggleFavorite`.
    ToggleFavorite,
    /// `onApplyGroup`, with the explicit order of an order board when it differs
    /// from the model's; answered with [`BulkEvent::GroupSettled`].
    ApplyGroup {
        request: u64,
        order: Option<Vec<i64>>,
    },
    /// `onRetryMetadata`: reads the missing summaries again.
    RetryMetadata,
    /// `onRetryTagDetails`: reads the complete tag list again.
    RetryTagDetails,
}

/// Commands, the answers to the model's library commands and reads.
#[derive(Debug)]
pub enum BulkInput {
    /// The library, the selection or the view changed: runs what the Solid model's
    /// effects run. The runtime sends it after every input.
    Sync,
    Command(BulkCommand),
    /// An answer to one of the model's own tickets ([`Bulk::owns`]).
    Library(LibraryEvent),
    /// A write of any owner published canonical values or deleted assets.
    Committed(MetadataCommit),
    /// The answer to [`BulkEffect::LoadSummaries`].
    SummariesLoaded {
        load: SummaryLoad,
        result: BackendResult<Vec<AssetSummary>>,
    },
    /// The owner is going away: nothing is asked for or published again.
    Dispose,
}

/// What the model asks of its owner.
#[derive(Debug)]
pub enum BulkEffect {
    /// A command for the library model, to be applied in this turn. Boxed: a write
    /// is several times the size of the other effects.
    Library(Box<LibraryInput>),
    /// A command for the selection, to be applied in this turn.
    Selection(SelectionInput),
    /// `get_asset_summaries_by_ids`; answered with [`BulkInput::SummariesLoaded`].
    LoadSummaries(SummaryLoad),
}

impl BulkEffect {
    /// `input` for the library model.
    pub fn library(input: LibraryInput) -> Self {
        Self::Library(Box::new(input))
    }
}

/// Answers to the dock's commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BulkEvent {
    /// The promise of `onAddTag` and `onRemoveTag`: whether the tag is saved and the
    /// library is still the one it was saved to.
    TagSettled { request: u64, saved: bool },
    /// The promise of `onApplyGroup`.
    GroupSettled {
        request: u64,
        result: GroupSaveResult,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum ReadState {
    #[default]
    Idle,
    Loading,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReadPhase {
    /// Waiting for `LibraryEvent::DetailsLoaded`.
    Loading,
    /// The read was discarded and a query refresh is on its way: waiting for its end
    /// before asking for `LibraryEvent::Idle`.
    WaitingQuery,
    /// Waiting for `LibraryEvent::Idle` before the next attempt.
    WaitingIdle,
}

/// The read of the single selected asset's complete tag list.
#[derive(Debug)]
struct TagRead {
    asset_id: i64,
    attempt: u8,
    ticket: Ticket,
    phase: ReadPhase,
    /// The asset's tag clock when the attempt started; taken on the first sync after
    /// the read was handed to the library when the asset had no clock before.
    token: Option<TagToken>,
    details_generation: u64,
}

/// The selection and identity a write was started for (`capture`).
#[derive(Debug, Clone)]
struct Context {
    key: u64,
    identity: u64,
    ids: Arc<[i64]>,
    count: usize,
}

#[derive(Debug)]
enum Write {
    /// A tag edit of the single selected asset.
    SingleTag {
        request: u64,
    },
    MergeTags {
        request: u64,
        tag: String,
        requested: usize,
    },
    SpecTags {
        request: u64,
        tag: String,
    },
    Favorites,
    SpecFavorites,
    Groups {
        request: u64,
        ids: Vec<i64>,
        key: Option<String>,
    },
    SpecGroup {
        request: u64,
        key: Option<String>,
    },
}

#[derive(Debug)]
struct PendingWrite {
    context: Context,
    write: Write,
}

/// What the details effect depends on: the single asset, selection mode, the
/// settings page, the retry counter and the identity epoch.
type DetailsDeps = (Option<i64>, bool, bool, u64, u64);

#[derive(Debug)]
pub struct Bulk {
    summaries: SelectedSummaries,
    /// The selection key, identity epoch and retry counter the summaries were
    /// brought up to date for.
    summaries_for: Option<(u64, u64, u64)>,
    metadata_retry: u64,
    group_draft: String,
    /// The user typed the group key of this selection: derived keys leave it alone.
    dirty: bool,
    order: Vec<i64>,
    /// Counts every assignment of [`order`](Self::order), also of an equal one: the
    /// dock restarts its inline draft from it.
    order_version: u64,
    conflicting: bool,
    applied_tags: Vec<String>,
    partial: Option<PartialResult>,
    tag_writes: usize,
    group_writes: usize,
    favorite_writes: usize,
    tag_failed: bool,
    group_failed: bool,
    favorite_failed: bool,
    /// The selection key the group draft and order were derived for.
    initialized: Option<u64>,
    observed_key: u64,
    observed_epoch: u64,
    /// The single selected asset whose tag record this model pins.
    pinned: Option<i64>,
    details_state: ReadState,
    details_retry: u64,
    details_for: Option<DetailsDeps>,
    read: Option<TagRead>,
    writes: HashMap<Ticket, PendingWrite>,
    last_ticket: u64,
    events: Vec<BulkEvent>,
    disposed: bool,
}

impl Bulk {
    /// A model for the selection and the library as they are in `env`.
    pub fn new(env: BulkEnv<'_>) -> Self {
        let epoch = env.library.tags().epoch();
        Self {
            summaries: SelectedSummaries::new(epoch),
            summaries_for: None,
            metadata_retry: 0,
            group_draft: String::new(),
            dirty: false,
            order: Vec::new(),
            order_version: 0,
            conflicting: false,
            applied_tags: Vec::new(),
            partial: None,
            tag_writes: 0,
            group_writes: 0,
            favorite_writes: 0,
            tag_failed: false,
            group_failed: false,
            favorite_failed: false,
            initialized: None,
            observed_key: env.selection.key(),
            observed_epoch: epoch,
            pinned: None,
            details_state: ReadState::Idle,
            details_retry: 0,
            details_for: None,
            read: None,
            writes: HashMap::new(),
            last_ticket: 0,
            events: Vec::new(),
            disposed: false,
        }
    }

    /// Whether a ticket of a library event is one of the bulk model's.
    pub fn owns(ticket: Ticket) -> bool {
        (BULK_TICKET_BASE..SELECTION_TICKET_BASE).contains(&ticket.0)
    }

    /// The answers to the dock's commands since the last call.
    pub fn take_events(&mut self) -> Vec<BulkEvent> {
        std::mem::take(&mut self.events)
    }

    /// The records of the selected assets.
    pub fn summaries(&self) -> &SelectedSummaries {
        &self.summaries
    }

    /// `groupKeyDraft`.
    pub fn group_key_draft(&self) -> &str {
        &self.group_draft
    }

    /// `orderedAssetIds`: the order a group write uses unless a board gives its own.
    pub fn order(&self) -> &[i64] {
        &self.order
    }

    /// Changes with every assignment of [`order`](Self::order).
    pub fn order_version(&self) -> u64 {
        self.order_version
    }

    /// `hasConflictingGroups`.
    pub fn conflicting(&self) -> bool {
        self.conflicting
    }

    /// `appliedBulkTags`: the tags added to this multi-selection so far.
    pub fn applied_tags(&self) -> &[String] {
        &self.applied_tags
    }

    /// `partialResult`: how many assets the last write reached.
    pub fn partial(&self) -> Option<PartialResult> {
        self.partial
    }

    pub fn tag_applying(&self) -> bool {
        self.tag_writes > 0
    }

    pub fn tag_save_failed(&self) -> bool {
        self.tag_failed
    }

    pub fn group_applying(&self) -> bool {
        self.group_writes > 0
    }

    pub fn group_failed(&self) -> bool {
        self.group_failed
    }

    pub fn favorite_applying(&self) -> bool {
        self.favorite_writes > 0
    }

    pub fn favorite_failed(&self) -> bool {
        self.favorite_failed
    }

    /// Writes that were sent and are not answered yet.
    pub fn pending_writes(&self) -> usize {
        self.writes.len()
    }

    fn ticket(&mut self) -> Ticket {
        self.last_ticket += 1;
        Ticket(BULK_TICKET_BASE + self.last_ticket)
    }

    fn set_order(&mut self, order: Vec<i64>) {
        self.order = order;
        self.order_version += 1;
    }

    /// `synchronize`: a new selection or identity starts with empty drafts and no
    /// feedback.
    fn synchronize(&mut self, env: &BulkEnv<'_>) {
        let key = env.selection.key();
        let epoch = env.library.tags().epoch();
        if self.observed_key == key && self.observed_epoch == epoch {
            return;
        }
        self.observed_key = key;
        self.observed_epoch = epoch;
        self.initialized = None;
        self.dirty = false;
        self.group_draft.clear();
        let order = if env.selection.ranged() {
            Vec::new()
        } else {
            env.selection.ids().to_vec()
        };
        self.set_order(order);
        self.conflicting = false;
        self.applied_tags.clear();
        self.partial = None;
        self.tag_failed = false;
        self.group_failed = false;
        self.favorite_failed = false;
    }

    /// The single asset held by ID, whose tags the dock replaces (`singleId`).
    fn single_id(env: &BulkEnv<'_>) -> Option<i64> {
        if env.selection.ranged() {
            return None;
        }
        match *env.selection.ids() {
            [id] => Some(id),
            _ => None,
        }
    }

    /// Derives the group draft and the order once per selection, when every record is
    /// there. A key the user typed meanwhile stays.
    fn initialize(&mut self, env: &BulkEnv<'_>) {
        let key = env.selection.key();
        if env.selection.ranged() || !self.summaries.ready() || self.initialized == Some(key) {
            return;
        }
        self.initialized = Some(key);
        let derived = derive_bulk_group_selection_state(&self.summaries.selected());
        if !self.dirty {
            self.group_draft = derived.group_key;
        }
        self.set_order(derived.ordered_asset_ids);
        self.conflicting = derived.has_conflicting_groups;
    }

    fn sync(&mut self, env: &BulkEnv<'_>, effects: &mut Vec<BulkEffect>) {
        if self.disposed {
            return;
        }
        self.synchronize(env);
        let epoch = env.library.tags().epoch();

        // A range selection is never materialized here: its summaries stay unloaded
        // and its writes go to the backend as a spec.
        let summaries_for = (env.selection.key(), epoch, self.metadata_retry);
        if self.summaries_for != Some(summaries_for) {
            self.summaries_for = Some(summaries_for);
            let ids: Arc<[i64]> = if env.selection.ranged() {
                Arc::from([])
            } else {
                env.selection.ids()
            };
            let loads = self
                .summaries
                .restart(ids, epoch, env.library.query().assets());
            effects.extend(loads.into_iter().map(BulkEffect::LoadSummaries));
        }
        self.initialize(env);

        let single = Self::single_id(env);
        if single != self.pinned {
            if let Some(previous) = self.pinned.take() {
                effects.push(BulkEffect::library(LibraryInput::UnpinTags(previous)));
            }
            if let Some(id) = single {
                effects.push(BulkEffect::library(LibraryInput::PinTags(id)));
            }
            self.pinned = single;
        }

        let details_for = (
            single,
            env.selection.enabled(),
            env.settings_open,
            self.details_retry,
            epoch,
        );
        if self.details_for != Some(details_for) {
            self.details_for = Some(details_for);
            // Whatever read was on its way belongs to the state before.
            self.read = None;
            match single {
                Some(id)
                    if env.selection.enabled()
                        && !env.settings_open
                        && !env.library.can_edit_tags(id) =>
                {
                    self.details_state = ReadState::Loading;
                    self.start_read(id, 0, env, effects);
                }
                _ => self.details_state = ReadState::Idle,
            }
        }
        // The library took the read since: its clock is the one the read started
        // under.
        if let Some(read) = &mut self.read {
            if read.phase == ReadPhase::Loading && read.token.is_none() {
                read.token = env.library.tags().peek_generation(read.asset_id);
            }
        }
        // The refresh a discarded read waited for has ended.
        if self
            .read
            .as_ref()
            .is_some_and(|read| read.phase == ReadPhase::WaitingQuery)
        {
            self.wait_to_retry(env, effects);
        }
    }

    /// One attempt of `readTags`: the details of the asset, whose read publishes its
    /// complete tag list.
    fn start_read(
        &mut self,
        asset_id: i64,
        attempt: u8,
        env: &BulkEnv<'_>,
        effects: &mut Vec<BulkEffect>,
    ) {
        let ticket = self.ticket();
        self.read = Some(TagRead {
            asset_id,
            attempt,
            ticket,
            phase: ReadPhase::Loading,
            token: env.library.tags().peek_generation(asset_id),
            details_generation: env.library.details().generation(),
        });
        effects.push(BulkEffect::library(LibraryInput::LoadDetails {
            ticket,
            asset_id,
        }));
    }

    fn fail_read(&mut self) {
        self.read = None;
        self.details_state = ReadState::Failed;
    }

    fn on_details_loaded(
        &mut self,
        ticket: Ticket,
        failed: bool,
        env: &BulkEnv<'_>,
        effects: &mut Vec<BulkEffect>,
    ) {
        let Some(read) = self
            .read
            .as_ref()
            .filter(|read| read.ticket == ticket && read.phase == ReadPhase::Loading)
        else {
            return;
        };
        if failed {
            self.fail_read();
            return;
        }
        if env.library.can_edit_tags(read.asset_id) {
            self.read = None;
            self.details_state = ReadState::Idle;
            return;
        }
        // A current, unchanged read that published nothing is a genuine failure.
        let unchanged = read
            .token
            .is_some_and(|token| env.library.tags().is_current(&token))
            && read.details_generation == env.library.details().generation();
        if unchanged {
            self.fail_read();
            return;
        }
        // A write or a query refresh discarded the read: it is made again.
        self.wait_to_retry(env, effects);
    }

    /// What a discarded read waits for before its next attempt: the end of the query
    /// refresh that is on its way, then of the writes (`waitForIdle`).
    ///
    /// The Solid reader learns that its read was discarded when the backend answers.
    /// Here the library says so the moment the query epoch changes, which a refresh
    /// does when it starts and again when its result is applied: a read made in
    /// between would be discarded by the same refresh and use up an attempt.
    fn wait_to_retry(&mut self, env: &BulkEnv<'_>, effects: &mut Vec<BulkEffect>) {
        let ticket = self.ticket();
        let Some(read) = &mut self.read else {
            return;
        };
        // An answer to the ticket before is not this wait's.
        read.ticket = ticket;
        if env.library.query().query_pending() {
            read.phase = ReadPhase::WaitingQuery;
            return;
        }
        read.phase = ReadPhase::WaitingIdle;
        effects.push(BulkEffect::library(LibraryInput::WaitForIdle { ticket }));
    }

    fn on_idle(&mut self, ticket: Ticket, env: &BulkEnv<'_>, effects: &mut Vec<BulkEffect>) {
        let Some(read) = self
            .read
            .as_ref()
            .filter(|read| read.ticket == ticket && read.phase == ReadPhase::WaitingIdle)
        else {
            return;
        };
        let (asset_id, attempt) = (read.asset_id, read.attempt + 1);
        if attempt >= TAG_READ_ATTEMPTS {
            self.fail_read();
        } else if env.library.query().query_pending() {
            // A refresh started while the writes ended.
            self.wait_to_retry(env, effects);
        } else {
            self.start_read(asset_id, attempt, env, effects);
        }
    }

    /// `capture`: the selection and identity a command starts for.
    fn context(&mut self, env: &BulkEnv<'_>) -> Context {
        self.synchronize(env);
        Context {
            key: env.selection.key(),
            identity: env.library.tags().epoch(),
            ids: env.selection.ids(),
            count: env.selection.count(),
        }
    }

    /// The write's selection and identity are still the current ones.
    fn current(&self, context: &Context, env: &BulkEnv<'_>) -> bool {
        !self.disposed
            && context.key == env.selection.key()
            && context.identity == env.library.tags().epoch()
    }

    /// The library is still the one the write went to.
    fn valid(&self, context: &Context, env: &BulkEnv<'_>) -> bool {
        !self.disposed && context.identity == env.library.tags().epoch()
    }

    fn send(
        &mut self,
        context: Context,
        write: Write,
        mutation: Mutation,
        effects: &mut Vec<BulkEffect>,
    ) {
        let ticket = self.ticket();
        self.writes.insert(ticket, PendingWrite { context, write });
        effects.push(BulkEffect::library(LibraryInput::Mutate {
            ticket,
            mutation,
        }));
    }

    fn tag_settled(&mut self, request: u64, saved: bool) {
        self.events.push(BulkEvent::TagSettled { request, saved });
    }

    fn group_settled(&mut self, request: u64, result: GroupSaveResult) {
        self.events
            .push(BulkEvent::GroupSettled { request, result });
    }

    /// `saveTag`.
    fn save_tag(
        &mut self,
        request: u64,
        raw: &str,
        remove: bool,
        env: &BulkEnv<'_>,
        effects: &mut Vec<BulkEffect>,
    ) {
        let tag = if remove {
            raw.to_owned()
        } else {
            normalize_bulk_tag(raw)
        };
        let context = self.context(env);
        if !env.selection.enabled() || env.selection.busy() || context.count == 0 || tag.is_empty()
        {
            return self.tag_settled(request, false);
        }
        let capture = env.selection.capture();
        if capture.spec.is_some() {
            // A range is written through its spec: all or nothing, decided by the
            // backend, and add-only.
            let mutation = capture
                .spec_mutation(SelectionWrite::MergeTags(vec![tag.clone()]))
                .filter(|_| !remove);
            let Some(mutation) = mutation else {
                return self.tag_settled(request, false);
            };
            self.tag_writes += 1;
            self.tag_failed = false;
            return self.send(context, Write::SpecTags { request, tag }, mutation, effects);
        }
        let ids = context.ids.clone();
        if ids.is_empty() || (remove && ids.len() != 1) {
            return self.tag_settled(request, false);
        }
        if let [asset_id] = *ids {
            // Never replaces a tag list that is not known completely.
            let Some(base) = env.library.asset_tags(asset_id) else {
                return self.tag_settled(request, false);
            };
            if !remove && base.tags.contains(&tag) {
                return self.tag_settled(request, true);
            }
            self.tag_writes += 1;
            self.tag_failed = false;
            // Applied to the canonical tags when the write leaves the asset's queue.
            let edit = if remove {
                TagEdit::Change {
                    add: Vec::new(),
                    remove: vec![tag],
                }
            } else {
                TagEdit::Change {
                    add: vec![tag],
                    remove: Vec::new(),
                }
            };
            return self.send(
                context,
                Write::SingleTag { request },
                Mutation::SaveTags { asset_id, edit },
                effects,
            );
        }
        self.tag_writes += 1;
        self.tag_failed = false;
        let mutation = Mutation::MergeTags {
            asset_ids: ids.to_vec(),
            tags: vec![tag.clone()],
        };
        self.send(
            context,
            Write::MergeTags {
                request,
                tag,
                requested: ids.len(),
            },
            mutation,
            effects,
        );
    }

    /// `toggleFavorite`.
    fn toggle_favorite(&mut self, env: &BulkEnv<'_>, effects: &mut Vec<BulkEffect>) {
        let context = self.context(env);
        if !env.selection.enabled() || env.selection.busy() || context.count == 0 {
            return;
        }
        self.favorite_writes += 1;
        self.favorite_failed = false;
        match env
            .selection
            .capture()
            .spec_mutation(SelectionWrite::ToggleFavorites)
        {
            Some(mutation) => self.send(context, Write::SpecFavorites, mutation, effects),
            None => {
                let mutation = Mutation::ToggleFavorites {
                    asset_ids: context.ids.to_vec(),
                };
                self.send(context, Write::Favorites, mutation, effects);
            }
        }
    }

    /// `applyGroup`.
    fn apply_group(
        &mut self,
        request: u64,
        explicit: Option<Vec<i64>>,
        env: &BulkEnv<'_>,
        effects: &mut Vec<BulkEffect>,
    ) {
        let context = self.context(env);
        let key =
            Some(normalize_group_key(&self.group_draft).to_owned()).filter(|key| !key.is_empty());
        let blocked = !env.selection.enabled() || env.selection.busy();
        let capture = env.selection.capture();
        if capture.spec.is_some() {
            // A range is grouped in gallery order: it has no order editor.
            let mutation = capture
                .spec_mutation(SelectionWrite::SetMediaGroup(key.clone()))
                .filter(|_| !blocked && explicit.is_none());
            let Some(mutation) = mutation else {
                return self.group_settled(request, GroupSaveResult::Ignored);
            };
            self.group_writes += 1;
            self.group_failed = false;
            return self.send(
                context,
                Write::SpecGroup { request, key },
                mutation,
                effects,
            );
        }
        let ids = explicit.unwrap_or_else(|| self.order.clone());
        let selected: HashSet<i64> = context.ids.iter().copied().collect();
        let unique: HashSet<i64> = ids.iter().copied().collect();
        if blocked
            || selected.is_empty()
            || !self.summaries.ready()
            || ids.len() != selected.len()
            || unique.len() != ids.len()
            || unique != selected
        {
            return self.group_settled(request, GroupSaveResult::Ignored);
        }
        // One asset that stays in its group keeps its place in it.
        let kept_order = match (self.summaries.selected().as_slice(), &key) {
            ([single], Some(key))
                if normalize_group_key(single.media_group_key.as_deref().unwrap_or(""))
                    .to_lowercase()
                    == key.to_lowercase() =>
            {
                Some(single.media_group_order.unwrap_or(1.0))
            }
            _ => None,
        };
        let updates = ids
            .iter()
            .enumerate()
            .map(|(index, asset_id)| MediaGroupUpdate {
                asset_id: *asset_id,
                media_group_order: key
                    .as_ref()
                    .map(|_| kept_order.unwrap_or((index + 1) as f64)),
            })
            .collect();
        self.group_writes += 1;
        self.group_failed = false;
        let mutation = Mutation::SetMediaGroups {
            updates,
            key: key.clone(),
        };
        self.send(
            context,
            Write::Groups { request, ids, key },
            mutation,
            effects,
        );
    }

    fn on_mutation_settled(
        &mut self,
        ticket: Ticket,
        result: Result<MutationOutcome, MutationError>,
        env: &BulkEnv<'_>,
        effects: &mut Vec<BulkEffect>,
    ) {
        let Some(PendingWrite { context, write }) = self.writes.remove(&ticket) else {
            return;
        };
        let current = self.current(&context, env);
        let valid = self.valid(&context, env);
        match write {
            Write::SingleTag { request } => {
                self.tag_writes = self.tag_writes.saturating_sub(1);
                let saved = match result {
                    Ok(_) => valid,
                    Err(_) => {
                        self.tag_failed |= current;
                        false
                    }
                };
                self.tag_settled(request, saved);
            }
            Write::MergeTags {
                request,
                tag,
                requested,
            } => {
                self.tag_writes = self.tag_writes.saturating_sub(1);
                let saved = match result {
                    Ok(MutationOutcome::TagsMerged(summary)) => {
                        let processed = summary.processed_assets;
                        if current {
                            self.partial = Some(PartialResult {
                                processed,
                                requested,
                            });
                            if processed > 0 {
                                self.applied_tags = merge_tag_lists(&self.applied_tags, &[tag]);
                            } else {
                                self.tag_failed = true;
                            }
                        }
                        processed > 0 && valid
                    }
                    _ => {
                        self.tag_failed |= current;
                        false
                    }
                };
                self.tag_settled(request, saved);
            }
            Write::SpecTags { request, tag } => {
                self.tag_writes = self.tag_writes.saturating_sub(1);
                let saved = match result {
                    Ok(MutationOutcome::Selection(SelectionMutationResult::Ready {
                        processed_assets,
                        ..
                    })) => {
                        if current {
                            self.partial = Some(PartialResult {
                                processed: processed_assets,
                                requested: context.count,
                            });
                            self.applied_tags = merge_tag_lists(&self.applied_tags, &[tag]);
                        }
                        valid
                    }
                    // Stale: nothing was written.
                    _ => {
                        self.tag_failed |= current;
                        false
                    }
                };
                self.tag_settled(request, saved);
            }
            Write::Favorites => {
                self.favorite_writes = self.favorite_writes.saturating_sub(1);
                match result {
                    Ok(MutationOutcome::FavoritesToggled(summary)) => {
                        if !valid {
                            return;
                        }
                        if current {
                            self.partial = Some(PartialResult {
                                processed: summary.processed_asset_ids.len(),
                                requested: context.ids.len(),
                            });
                        }
                        // IDs the backend did not acknowledge are gone.
                        let processed: HashSet<i64> =
                            summary.processed_asset_ids.iter().copied().collect();
                        let missing: Vec<i64> = context
                            .ids
                            .iter()
                            .copied()
                            .filter(|id| !processed.contains(id))
                            .collect();
                        if !missing.is_empty() {
                            effects.push(BulkEffect::Selection(SelectionInput::Remove(missing)));
                        }
                    }
                    _ => self.favorite_failed |= current,
                }
            }
            Write::SpecFavorites => {
                self.favorite_writes = self.favorite_writes.saturating_sub(1);
                match result {
                    Ok(MutationOutcome::Selection(SelectionMutationResult::Ready {
                        processed_assets,
                        ..
                    })) => {
                        if current {
                            self.partial = Some(PartialResult {
                                processed: processed_assets,
                                requested: context.count,
                            });
                        }
                    }
                    _ => self.favorite_failed |= current,
                }
            }
            Write::Groups { request, ids, key } => {
                self.group_writes = self.group_writes.saturating_sub(1);
                let outcome = match result {
                    Ok(MutationOutcome::MediaGroupsSet(summary)) => {
                        if current {
                            let partial = PartialResult {
                                processed: summary.processed_assets,
                                requested: ids.len(),
                            };
                            self.partial = Some(partial);
                            self.set_order(ids);
                            self.group_draft = key.unwrap_or_default();
                            self.conflicting = false;
                            if partial.is_partial() {
                                GroupSaveResult::Partial(partial)
                            } else {
                                GroupSaveResult::Saved
                            }
                        } else {
                            GroupSaveResult::Ignored
                        }
                    }
                    _ => {
                        self.group_failed |= current;
                        GroupSaveResult::Failed
                    }
                };
                self.group_settled(request, outcome);
            }
            Write::SpecGroup { request, key } => {
                self.group_writes = self.group_writes.saturating_sub(1);
                let outcome = match result {
                    Ok(MutationOutcome::Selection(SelectionMutationResult::Ready {
                        processed_assets,
                        ..
                    })) => {
                        if current {
                            let partial = PartialResult {
                                processed: processed_assets,
                                requested: context.count,
                            };
                            self.partial = Some(partial);
                            self.group_draft = key.unwrap_or_default();
                            if partial.is_partial() {
                                GroupSaveResult::Partial(partial)
                            } else {
                                GroupSaveResult::Saved
                            }
                        } else {
                            GroupSaveResult::Ignored
                        }
                    }
                    _ => {
                        self.group_failed |= current;
                        GroupSaveResult::Failed
                    }
                };
                self.group_settled(request, outcome);
            }
        }
    }

    fn dispose(&mut self) {
        if self.disposed {
            return;
        }
        self.disposed = true;
        self.summaries.dispose();
        self.read = None;
        // The answers the dock still waits for.
        for (_, pending) in std::mem::take(&mut self.writes) {
            match pending.write {
                Write::SingleTag { request }
                | Write::MergeTags { request, .. }
                | Write::SpecTags { request, .. } => self.tag_settled(request, false),
                Write::Groups { request, .. } | Write::SpecGroup { request, .. } => {
                    self.group_settled(request, GroupSaveResult::Ignored);
                }
                Write::Favorites | Write::SpecFavorites => {}
            }
        }
        self.tag_writes = 0;
        self.group_writes = 0;
        self.favorite_writes = 0;
    }

    /// Applies one input in the state of `env` and returns what to do for it.
    pub fn handle(&mut self, input: BulkInput, env: BulkEnv<'_>) -> Vec<BulkEffect> {
        let mut effects = Vec::new();
        if self.disposed {
            return effects;
        }
        let env = &env;
        match input {
            BulkInput::Sync => {}
            BulkInput::Command(command) => match command {
                BulkCommand::SetGroupKeyDraft(value) => {
                    self.synchronize(env);
                    self.dirty = true;
                    self.group_draft = value;
                }
                BulkCommand::AddTag { request, tag } => {
                    self.save_tag(request, &tag, false, env, &mut effects);
                }
                BulkCommand::RemoveTag { request, tag } => {
                    self.save_tag(request, &tag, true, env, &mut effects);
                }
                BulkCommand::ToggleFavorite => self.toggle_favorite(env, &mut effects),
                BulkCommand::ApplyGroup { request, order } => {
                    self.apply_group(request, order, env, &mut effects);
                }
                BulkCommand::RetryMetadata => self.metadata_retry += 1,
                BulkCommand::RetryTagDetails => self.details_retry += 1,
            },
            BulkInput::Library(event) => match event {
                LibraryEvent::MutationSettled { ticket, result } => {
                    self.on_mutation_settled(ticket, result, env, &mut effects);
                }
                LibraryEvent::DetailsLoaded { ticket, result } => {
                    self.on_details_loaded(ticket, result.is_err(), env, &mut effects);
                }
                LibraryEvent::Idle { ticket } => self.on_idle(ticket, env, &mut effects),
                _ => {}
            },
            BulkInput::Committed(commit) => self.summaries.commit(&commit),
            BulkInput::SummariesLoaded { load, result } => {
                let epoch = env.library.tags().epoch();
                let loads = self.summaries.loaded(load, result, epoch);
                effects.extend(loads.into_iter().map(BulkEffect::LoadSummaries));
            }
            BulkInput::Dispose => {
                self.dispose();
                return effects;
            }
        }
        self.sync(env, &mut effects);
        effects
    }
}

/// The bulk model together with the library and the selection it reads: everything
/// the dock shows (the getters of `BulkController`).
#[derive(Debug, Clone, Copy)]
pub struct BulkView<'a> {
    pub bulk: &'a Bulk,
    pub library: &'a Library,
    pub selection: &'a Selection,
}

impl<'a> BulkView<'a> {
    /// `selectedAssetIds`: the IDs held directly, in the order they were selected.
    pub fn selected_asset_ids(&self) -> Arc<[i64]> {
        self.selection.ids()
    }

    pub fn selected_count(&self) -> usize {
        self.selection.count()
    }

    /// Changes whenever the selection changes.
    pub fn selection_key(&self) -> u64 {
        self.selection.key()
    }

    /// The selection holds ranges that are written as a spec.
    pub fn selection_ranged(&self) -> bool {
        self.selection.ranged()
    }

    pub fn selection_notice(&self) -> Option<SelectionNotice> {
        self.selection.notice()
    }

    pub fn selection_busy(&self) -> bool {
        self.selection.busy()
    }

    /// `selectedAssets`: the loaded records, in selection order.
    pub fn selected_assets(&self) -> Vec<&'a AssetSummary> {
        self.bulk.summaries.selected()
    }

    pub fn metadata_loading(&self) -> bool {
        !self.selection.ranged() && !self.bulk.summaries.ready() && !self.bulk.summaries.failed()
    }

    pub fn metadata_failed(&self) -> bool {
        self.bulk.summaries.failed()
    }

    pub fn group_key_draft(&self) -> &'a str {
        &self.bulk.group_draft
    }

    pub fn ordered_asset_ids(&self) -> &'a [i64] {
        &self.bulk.order
    }

    pub fn order_version(&self) -> u64 {
        self.bulk.order_version
    }

    pub fn has_conflicting_groups(&self) -> bool {
        self.bulk.conflicting
    }

    pub fn group_applying(&self) -> bool {
        self.bulk.group_applying()
    }

    pub fn group_failed(&self) -> bool {
        self.bulk.group_failed
    }

    pub fn favorite_applying(&self) -> bool {
        self.bulk.favorite_applying()
    }

    pub fn favorite_failed(&self) -> bool {
        self.bulk.favorite_failed
    }

    /// Every selected asset is a favorite: the heart is filled and the toggle clears.
    pub fn all_selected_favorites(&self) -> bool {
        !self.selection.ranged()
            && !self.selection.ids().is_empty()
            && self.bulk.summaries.ready()
            && self.bulk.summaries.all_favorites()
    }

    pub fn partial_result(&self) -> Option<PartialResult> {
        self.bulk.partial
    }

    pub fn tag_mode(&self) -> TagMode {
        match self.selection.count() {
            0 => TagMode::None,
            1 if !self.selection.ranged() => TagMode::Single,
            _ => TagMode::Multiple,
        }
    }

    fn single_id(&self) -> Option<i64> {
        if self.selection.ranged() {
            return None;
        }
        match *self.selection.ids() {
            [id] => Some(id),
            _ => None,
        }
    }

    fn single_tags_known(&self) -> bool {
        self.single_id()
            .is_some_and(|id| self.library.can_edit_tags(id))
    }

    /// `singleAssetTags`: the complete tag list of the single selected asset, empty
    /// until it is known.
    pub fn single_asset_tags(&self) -> &'a [String] {
        self.single_id()
            .and_then(|id| self.library.asset_tags(id))
            .map(|known| known.tags)
            .unwrap_or(&[])
    }

    pub fn applied_bulk_tags(&self) -> &'a [String] {
        &self.bulk.applied_tags
    }

    pub fn tag_details_loading(&self) -> bool {
        self.bulk.details_state == ReadState::Loading && !self.single_tags_known()
    }

    pub fn tag_details_failed(&self) -> bool {
        self.bulk.details_state == ReadState::Failed && !self.single_tags_known()
    }

    pub fn tag_applying(&self) -> bool {
        self.bulk.tag_applying()
    }

    pub fn tag_save_failed(&self) -> bool {
        self.bulk.tag_failed
    }

    pub fn known_tags(&self) -> &'a Arc<Vec<String>> {
        self.library.known_tags()
    }
}
