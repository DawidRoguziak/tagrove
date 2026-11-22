//! The lightbox model (`createLightbox.ts` of the Solid frontend): the selected asset,
//! its place in the query result, and every edit made in the lightbox.
//!
//! [`Lightbox`] has the form of the bulk model (ADR 0044): it reads the library through
//! the [`LightboxEnv`] its owner lends it with every input, changes nothing itself, and
//! returns what it wants done as effects, all of them inputs of the library model
//! that the runtime applies in the same turn. After every input of any model the
//! runtime sends [`LightboxInput::Sync`], which is where the Solid model's effects run.
//! Contract: `docs/subsystems/lightbox.md` ("GPUI port: the lightbox model").
//!
//! # Requests
//!
//! Every select, also the close, starts a new selection request; so does every read
//! of the details. Navigation and the position lookup have request counters of their
//! own. An answer is applied only when the request it was started for is still the
//! current one, the selected asset is the one it was read for, and the identity and
//! query epochs are the ones it was read under. A slow answer for A therefore never
//! replaces B and never reopens a closed lightbox.
//!
//! - **Details** ([`LibraryInput::LoadDetails`]). An answer without details is a
//!   failure only when nothing changed since the read started; a read that a write
//!   or a query refresh discarded waits for the refresh, then for the write queue
//!   ([`LibraryInput::WaitForIdle`]), and reads again.
//! - **Navigation** ([`QueryInput::ReadAsset`]). Each key press moves a target index
//!   at once, so rapid presses add up; only the answer of the latest press selects. A
//!   row that cannot be read rolls the target back to the shown asset and keeps the
//!   failed index for Retry ([`QueryInput::RetryAsset`]).
//! - **Position** ([`QueryInput::ResolvePosition`]). After the query changed, the
//!   selected ID is looked up in the cached rows, then in the backend's session.
//!
//! # Writes
//!
//! Every write goes through the library's queue ([`LibraryInput::Mutate`]) and
//! publishes nothing itself: the library publishes the backend's canonical values
//! and this model follows them ([`LightboxInput::Committed`], the tag revision, the
//! cached row of the selected asset).
//!
//! - **Tags.** This model is the only owner of the lightbox's tag saves: one write
//!   per asset at a time, and edits made while it runs are saved by one following
//!   write. The write is a [`TagEdit::Change`] of what the editor added and removed,
//!   so a writer queued ahead is not undone. A failed or refused write keeps the
//!   wanted tags and offers Retry.
//! - **Favorite, media group.** Pending state is counted per asset and identity
//!   epoch; a repeated favorite toggle reverses the newest pending one.
//! - **Delete** ([`Mutation::DeleteAsset`]): the core's use case decides, and a
//!   refusal (a scan holds the scan lock, ADR 0022) comes back as the answer of the
//!   command with nothing changed. Every delete command is answered exactly once
//!   ([`LightboxEvent::DeleteSettled`]), also when the model is disposed first.

use std::collections::HashMap;

use media_tagger_core::models::{
    AssetDetails, AssetQueryPositionResult, AssetSummary, DeleteAssetSummary, DeleteSourceStatus,
};

use super::intents::{PendingWrites, TagIntent, WriteState};
use crate::{
    bulk::{parse_media_group_order, GroupOrderDraft, BULK_TICKET_BASE},
    library::{
        normalize_tags, DetailsResult, Library, LibraryEvent, LibraryInput, MetadataCommit,
        Mutation, MutationError, MutationOutcome, QueryError, QueryEvent, QueryInput, TagEdit,
        TagStateError, TagToken, Ticket,
    },
};

#[cfg(test)]
mod tests;

/// The first [`Ticket`] value of the lightbox model's reads and writes; its range ends
/// where the bulk model's begins ([`BULK_TICKET_BASE`]). The same range is used for
/// query reads and for library commands, which are answered on separate channels.
pub const LIGHTBOX_TICKET_BASE: u64 = 1 << 60;

/// A details read that writes or refreshes keep discarding gives up after this many
/// discarded answers and shows the load failure with Retry. The Solid model retries
/// without a bound; a selected asset whose reads can never be published (a tombstone
/// left by a deletion this model was not told of) would be read forever.
const MAX_DISCARDED_READS: u8 = 8;

/// What the model reads of the library while it handles one input.
#[derive(Debug, Clone, Copy)]
pub struct LightboxEnv<'a> {
    pub library: &'a Library,
}

/// The asset the lightbox shows (`SelectedAsset`): a query row at first, completed by
/// the details when they arrive.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectedAsset {
    pub summary: AssetSummary,
    /// The verified source: the preview path of a GIF or video row, the path of the
    /// details once they are read, `None` for an image until then.
    pub path: Option<String>,
    /// Known from the details only.
    pub size_bytes: Option<i64>,
    /// The tags as last published; never authoritative before the details arrived
    /// ([`LightboxView::tag_editing_disabled`]).
    pub tags: Vec<String>,
}

impl SelectedAsset {
    pub fn id(&self) -> i64 {
        self.summary.id
    }

    fn from_details(details: &AssetDetails) -> Self {
        Self {
            summary: details.summary.clone(),
            path: Some(details.path.clone()),
            size_bytes: Some(details.size_bytes),
            tags: details.tags.clone(),
        }
    }
}

/// `navigationStatus`: whether Previous and Next know where the selected asset is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NavigationStatus {
    #[default]
    Ready,
    /// The query is being refreshed or the position is being looked up.
    Resolving,
    /// The asset is not in the result any more; the lightbox stays open.
    Missing,
    /// A page, the position or the refresh failed; Retry is offered.
    Failed,
}

impl NavigationStatus {
    /// The locale key of the notice (`lightbox.navigation.*`); none while ready.
    pub const fn notice_key(self) -> Option<&'static str> {
        match self {
            Self::Ready => None,
            Self::Resolving => Some("lightbox.navigation.resolving"),
            Self::Missing => Some("lightbox.navigation.missing"),
            Self::Failed => Some("lightbox.navigation.failed"),
        }
    }
}

/// What became of the source file of a deleted asset (`source_status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeletedSource {
    Deleted,
    /// The file was gone already; the stale library entry was removed.
    Missing,
    /// The entry is removed; the final cleanup of the staged file failed.
    CleanupPending,
}

/// The backend's answer to an accepted deletion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletedAsset {
    pub source: DeletedSource,
    /// Where the staged file was left when the cleanup failed.
    pub recovery_path: Option<String>,
    pub removed_assets: usize,
    pub removed_thumbnails: usize,
}

impl From<DeleteAssetSummary> for DeletedAsset {
    fn from(summary: DeleteAssetSummary) -> Self {
        Self {
            source: match summary.source_status {
                DeleteSourceStatus::Deleted => DeletedSource::Deleted,
                DeleteSourceStatus::Missing => DeletedSource::Missing,
                DeleteSourceStatus::CleanupPending => DeletedSource::CleanupPending,
            },
            recovery_path: summary.recovery_path,
            removed_assets: summary.removed_assets,
            removed_thumbnails: summary.removed_thumbnails,
        }
    }
}

/// What the lightbox asks of the model (the commands of `LightboxModel`).
#[derive(Debug, Clone, PartialEq)]
pub enum LightboxCommand {
    /// `select`: opens the asset of a query row, with its global index when the
    /// caller knows it (a gallery tile does).
    Select {
        asset: Box<AssetSummary>,
        index: Option<usize>,
    },
    /// `close`.
    Close,
    Previous,
    Next,
    /// `saveTags`: the complete list the editor shows now.
    SaveTags(Vec<String>),
    RetryTags,
    RetryDetails,
    RetryNavigation,
    ToggleFavorite,
    /// `setMediaGroupKeyEditor`.
    SetMediaGroupKeyDraft(String),
    /// `setMediaGroupOrderEditor`: a draft that is no valid order is refused whole.
    SetMediaGroupOrderDraft(String),
    SaveMediaGroup,
    /// `deleteSelected`; answered with exactly one [`LightboxEvent::DeleteSettled`]
    /// of the same `request`, whatever happens to the model meanwhile.
    DeleteSelected {
        request: u64,
    },
}

/// Commands and the answers to the model's reads and writes.
#[derive(Debug)]
pub enum LightboxInput {
    /// The library changed: runs what the Solid model's effects run. The runtime
    /// sends it after every input.
    Sync,
    Command(LightboxCommand),
    /// An answer to one of the model's own library tickets ([`Lightbox::owns`]).
    Library(LibraryEvent),
    /// An answer to one of the model's own query tickets.
    Query(QueryEvent),
    /// A write of any owner published canonical values or deleted assets.
    Committed(MetadataCommit),
    /// The owner is going away: nothing is asked for or published again. Deletions
    /// that wait for their answer get it now ([`LightboxEvent::DeleteSettled`]).
    Dispose,
}

/// What the model asks of its owner.
#[derive(Debug)]
pub enum LightboxEffect {
    /// A command for the library model (a read, a pin, a write, a query read), to be
    /// applied in this turn. Boxed: a write is several times the size of a read.
    Library(Box<LibraryInput>),
}

impl LightboxEffect {
    /// `input` for the library model.
    pub fn library(input: LibraryInput) -> Self {
        Self::Library(Box::new(input))
    }

    fn query(input: QueryInput) -> Self {
        Self::library(LibraryInput::Query(input))
    }
}

/// Answers to the lightbox's commands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LightboxEvent {
    /// The promise of `deleteSelected`; every [`LightboxCommand::DeleteSelected`] gets
    /// exactly one. `Ok(None)`: nothing was selected, nothing was sent. `Err`: the
    /// file and its metadata are untouched, with one exception: a deletion that was
    /// on its way when the model was disposed is answered at the disposal with
    /// [`MutationError::Refused`] of [`TagStateError::Unavailable`], as the library
    /// answers a write it no longer takes, and what the backend made of it is not
    /// reported any more. A command that comes after the disposal gets the same
    /// answer and sends nothing.
    DeleteSettled {
        request: u64,
        result: Result<Option<DeletedAsset>, MutationError>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum DetailState {
    Loading,
    #[default]
    Ready,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReadPhase {
    /// Waiting for `LibraryEvent::DetailsLoaded`.
    Loading,
    /// A query refresh is on its way: waiting for its end before asking for
    /// `LibraryEvent::Idle`.
    WaitingQuery,
    /// Waiting for `LibraryEvent::Idle` before the read.
    WaitingIdle,
}

/// The read of the selected asset's details (`readDetails`).
#[derive(Debug)]
struct DetailsRead {
    /// The selection request the read was started as.
    request: u64,
    identity: u64,
    asset_id: i64,
    ticket: Ticket,
    phase: ReadPhase,
    /// The asset's tag clock when the attempt started; taken on the first sync after
    /// the read was handed to the library when the asset had no clock before.
    token: Option<TagToken>,
    query_epoch: u64,
    details_generation: u64,
    discarded: u8,
}

/// The page read of the latest navigation (`loadTarget`).
#[derive(Debug, Clone, Copy)]
struct NavigationRead {
    ticket: Ticket,
    request: u64,
    query_epoch: u64,
    target: usize,
}

/// The position lookup of the selected asset (`resolvePosition`).
#[derive(Debug, Clone, Copy)]
struct PositionRead {
    ticket: Ticket,
    request: u64,
    asset_id: i64,
    query_epoch: u64,
}

/// A write that was handed to the library and is not answered yet.
#[derive(Debug)]
enum PendingWrite {
    Tags {
        asset_id: i64,
        intent: u64,
    },
    Favorite {
        asset_id: i64,
        identity: u64,
    },
    Group {
        asset_id: i64,
        identity: u64,
        /// The drafts the write was made from.
        key: String,
        order: GroupOrderDraft,
    },
    Delete {
        request: u64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    Previous,
    Next,
}

#[derive(Debug)]
pub struct Lightbox {
    current: Option<SelectedAsset>,
    detail_state: DetailState,
    navigation: NavigationStatus,
    editor: Vec<String>,
    tag_state: WriteState,
    favorite_state: WriteState,
    group_state: WriteState,
    key_draft: String,
    order_draft: String,
    key_dirty: bool,
    order_dirty: bool,
    selection_request: u64,
    navigation_request: u64,
    position_request: u64,
    /// The global index of the shown asset, and the index navigation is heading for.
    selected_index: Option<usize>,
    target_index: Option<usize>,
    /// The index whose row could not be read; Retry reads it again.
    failed_target: Option<usize>,
    /// What the last sync saw of the library.
    query_epoch: u64,
    query_pending: bool,
    identity: u64,
    tag_revision: u64,
    /// The cached row of the selected asset as the last sync saw it, with the query
    /// epoch of the result it was last seen in.
    seen_row: Option<(u64, AssetSummary)>,
    /// The asset whose tag record this model pins.
    pinned: Option<i64>,
    intents: HashMap<i64, TagIntent>,
    last_intent: u64,
    favorite_writes: PendingWrites<bool>,
    group_writes: PendingWrites<()>,
    details_read: Option<DetailsRead>,
    navigation_read: Option<NavigationRead>,
    position_read: Option<PositionRead>,
    /// Row reads for the neighbours of the shown asset, with the query epoch they
    /// were started under.
    prefetch: HashMap<Ticket, u64>,
    writes: HashMap<Ticket, PendingWrite>,
    last_ticket: u64,
    events: Vec<LightboxEvent>,
    disposed: bool,
}

impl Lightbox {
    /// A closed lightbox over the library as it is in `env`.
    pub fn new(env: LightboxEnv<'_>) -> Self {
        let snapshot = env.library.query().navigation_snapshot();
        Self {
            current: None,
            detail_state: DetailState::Ready,
            navigation: NavigationStatus::Ready,
            editor: Vec::new(),
            tag_state: WriteState::Idle,
            favorite_state: WriteState::Idle,
            group_state: WriteState::Idle,
            key_draft: String::new(),
            order_draft: String::new(),
            key_dirty: false,
            order_dirty: false,
            selection_request: 0,
            navigation_request: 0,
            position_request: 0,
            selected_index: None,
            target_index: None,
            failed_target: None,
            query_epoch: snapshot.epoch,
            query_pending: snapshot.pending,
            identity: env.library.tags().epoch(),
            tag_revision: env.library.tags().revision(),
            seen_row: None,
            pinned: None,
            intents: HashMap::new(),
            last_intent: 0,
            favorite_writes: PendingWrites::default(),
            group_writes: PendingWrites::default(),
            details_read: None,
            navigation_read: None,
            position_read: None,
            prefetch: HashMap::new(),
            writes: HashMap::new(),
            last_ticket: 0,
            events: Vec::new(),
            disposed: false,
        }
    }

    /// Whether a ticket of a library or query event is one of the lightbox model's.
    pub fn owns(ticket: Ticket) -> bool {
        (LIGHTBOX_TICKET_BASE..BULK_TICKET_BASE).contains(&ticket.0)
    }

    /// The answers to the lightbox's commands since the last call.
    pub fn take_events(&mut self) -> Vec<LightboxEvent> {
        std::mem::take(&mut self.events)
    }

    /// `selected`: the shown asset; `None` while the lightbox is closed.
    pub fn selected(&self) -> Option<&SelectedAsset> {
        self.current.as_ref()
    }

    /// The global index of the shown asset in the query result, when it is known.
    pub fn selected_index(&self) -> Option<usize> {
        self.selected_index
    }

    pub fn navigation_status(&self) -> NavigationStatus {
        self.navigation
    }

    /// Counts every select and every details read, the close included.
    pub fn selection_request(&self) -> u64 {
        self.selection_request
    }

    /// Writes that were handed to the library and are not answered yet.
    pub fn pending_writes(&self) -> usize {
        self.writes.len()
    }

    /// Assets whose tag edit is being saved, waits for Retry, or is shown.
    pub fn retained_intents(&self) -> usize {
        self.intents.len()
    }

    pub fn is_disposed(&self) -> bool {
        self.disposed
    }

    fn ticket(&mut self) -> Ticket {
        self.last_ticket += 1;
        Ticket(LIGHTBOX_TICKET_BASE + self.last_ticket)
    }

    fn current_id(&self) -> Option<i64> {
        self.current.as_ref().map(SelectedAsset::id)
    }

    /// `shown`: the write's asset is the selected one in the identity it was made in.
    fn shown(&self, asset_id: i64, identity: u64, env: &LightboxEnv<'_>) -> bool {
        !self.disposed
            && self.current_id() == Some(asset_id)
            && identity == env.library.tags().epoch()
    }

    /// `publish`: makes `value` the shown asset. A new asset moves the tag pin, drops
    /// the group drafts of the one before and takes over the pending state of its own
    /// writes; the same asset keeps drafts the user touched.
    fn publish(
        &mut self,
        value: Option<SelectedAsset>,
        env: &LightboxEnv<'_>,
        effects: &mut Vec<LightboxEffect>,
    ) {
        let previous = self.current_id();
        let next = value.as_ref().map(SelectedAsset::id);
        let changed = previous != next;
        if changed {
            if let Some(previous) = previous {
                // A saved intent is kept only while its asset is shown.
                if self.intents.get(&previous).is_some_and(|intent| {
                    intent.state == WriteState::Idle && intent.desired.is_none()
                }) {
                    self.intents.remove(&previous);
                }
            }
        }
        self.current = value;
        if changed {
            if let Some(pinned) = self.pinned.take() {
                effects.push(LightboxEffect::library(LibraryInput::UnpinTags(pinned)));
            }
            if let Some(id) = next {
                effects.push(LightboxEffect::library(LibraryInput::PinTags(id)));
                self.pinned = Some(id);
            }
            self.key_dirty = false;
            self.order_dirty = false;
            let identity = env.library.tags().epoch();
            let pending = |has: bool| {
                if has {
                    WriteState::Pending
                } else {
                    WriteState::Idle
                }
            };
            self.favorite_state =
                pending(next.is_some_and(|id| self.favorite_writes.has(id, identity)));
            self.group_state = pending(next.is_some_and(|id| self.group_writes.has(id, identity)));
            self.seen_row = None;
        }
        let summary = self.current.as_ref().map(|current| &current.summary);
        if !self.key_dirty {
            self.key_draft = summary
                .and_then(|summary| summary.media_group_key.clone())
                .unwrap_or_default();
        }
        if !self.order_dirty {
            self.order_draft = summary
                .and_then(|summary| summary.media_group_order)
                .map(|order| order.to_string())
                .unwrap_or_default();
        }
        self.sync_tags(env);
    }

    /// `syncTags`: the editor shows the unsaved intent of the asset, else its
    /// canonical tags, else what the selection carries.
    fn sync_tags(&mut self, env: &LightboxEnv<'_>) {
        let asset_id = self.current_id();
        let intent = asset_id.and_then(|asset_id| self.intents.get(&asset_id));
        self.tag_state = intent.map(|intent| intent.state).unwrap_or_default();
        self.editor = match (
            intent.and_then(|intent| intent.desired.clone()),
            &self.current,
        ) {
            (Some(desired), _) => desired,
            (None, Some(current)) => match env.library.asset_tags(current.id()) {
                Some(canonical) => canonical.tags.to_vec(),
                None => current.tags.clone(),
            },
            (None, None) => Vec::new(),
        };
    }

    /// `valid`: the read is still the current selection request, for the asset that
    /// is shown, in the identity it started in.
    fn read_is_current(&self, read: &DetailsRead, env: &LightboxEnv<'_>) -> bool {
        !self.disposed
            && read.request == self.selection_request
            && self.current_id() == Some(read.asset_id)
            && read.identity == env.library.tags().epoch()
    }

    /// `prefetch`: the rows before and after `index`, wrapping, and then their
    /// details. Nothing for a result of one row.
    fn prefetch(&mut self, index: usize, env: &LightboxEnv<'_>, effects: &mut Vec<LightboxEffect>) {
        let snapshot = env.library.query().navigation_snapshot();
        let count = snapshot.total;
        if count <= 1 {
            return;
        }
        let before = (index % count + count - 1) % count;
        let after = (index % count + 1) % count;
        for adjacent in [Some(before), Some(after).filter(|after| *after != before)]
            .into_iter()
            .flatten()
        {
            let ticket = self.ticket();
            self.prefetch.insert(ticket, snapshot.epoch);
            effects.push(LightboxEffect::query(QueryInput::ReadAsset {
                ticket,
                index: adjacent,
            }));
        }
    }

    /// A neighbour's row arrived: its details are read into the shared cache. Nobody
    /// waits for that read.
    fn on_prefetched(
        &mut self,
        started_under: u64,
        row: Option<AssetSummary>,
        env: &LightboxEnv<'_>,
        effects: &mut Vec<LightboxEffect>,
    ) {
        let Some(row) = row else {
            return;
        };
        if env.library.query().query_epoch() != started_under
            || env.library.cached_details(row.id).is_some()
        {
            return;
        }
        let ticket = self.ticket();
        effects.push(LightboxEffect::library(LibraryInput::LoadDetails {
            ticket,
            asset_id: row.id,
        }));
    }

    /// `readDetails`: reads the details of the selected asset as a new selection
    /// request; after a write that dropped its tag list, once the write queue is
    /// empty.
    fn read_details(
        &mut self,
        asset_id: i64,
        after_writes: bool,
        env: &LightboxEnv<'_>,
        effects: &mut Vec<LightboxEffect>,
    ) {
        self.selection_request += 1;
        self.detail_state = DetailState::Loading;
        let ticket = self.ticket();
        self.details_read = Some(DetailsRead {
            request: self.selection_request,
            identity: env.library.tags().epoch(),
            asset_id,
            ticket,
            phase: ReadPhase::WaitingIdle,
            token: None,
            query_epoch: env.library.query().query_epoch(),
            details_generation: env.library.details().generation(),
            discarded: 0,
        });
        if after_writes {
            self.wait_to_read(env, effects);
        } else {
            self.begin_read(env, effects);
        }
    }

    /// One attempt: the details of the asset, whose read publishes its complete tag
    /// list.
    fn begin_read(&mut self, env: &LightboxEnv<'_>, effects: &mut Vec<LightboxEffect>) {
        let ticket = self.ticket();
        let Some(read) = &mut self.details_read else {
            return;
        };
        read.ticket = ticket;
        read.phase = ReadPhase::Loading;
        read.token = env.library.tags().peek_generation(read.asset_id);
        read.query_epoch = env.library.query().query_epoch();
        read.details_generation = env.library.details().generation();
        effects.push(LightboxEffect::library(LibraryInput::LoadDetails {
            ticket,
            asset_id: read.asset_id,
        }));
    }

    /// What a read waits for before its next attempt: the end of the query refresh
    /// that is on its way, then of the writes (`waitForIdle`). A read made while the
    /// refresh runs would be discarded by the same refresh.
    fn wait_to_read(&mut self, env: &LightboxEnv<'_>, effects: &mut Vec<LightboxEffect>) {
        let ticket = self.ticket();
        let Some(read) = &mut self.details_read else {
            return;
        };
        // An answer to the ticket before is not this wait's.
        read.ticket = ticket;
        if env.library.query().query_pending() {
            read.phase = ReadPhase::WaitingQuery;
            return;
        }
        read.phase = ReadPhase::WaitingIdle;
        effects.push(LightboxEffect::library(LibraryInput::WaitForIdle {
            ticket,
        }));
    }

    fn fail_read(&mut self) {
        self.details_read = None;
        self.detail_state = DetailState::Failed;
    }

    fn on_details_loaded(
        &mut self,
        ticket: Ticket,
        result: DetailsResult,
        env: &LightboxEnv<'_>,
        effects: &mut Vec<LightboxEffect>,
    ) {
        let Some(read) = self
            .details_read
            .as_ref()
            .filter(|read| read.ticket == ticket && read.phase == ReadPhase::Loading)
        else {
            return;
        };
        if !self.read_is_current(read, env) {
            self.details_read = None;
            return;
        }
        match result {
            Err(_) => self.fail_read(),
            Ok(Some(details)) => {
                self.details_read = None;
                self.publish(Some(SelectedAsset::from_details(&details)), env, effects);
                self.detail_state = DetailState::Ready;
                if let Some(index) = self.selected_index {
                    self.prefetch(index, env, effects);
                }
            }
            Ok(None) => {
                // A current, unchanged read that published nothing is a genuine
                // failure: the asset is gone.
                let unchanged = read
                    .token
                    .is_some_and(|token| env.library.tags().is_current(&token))
                    && read.details_generation == env.library.details().generation()
                    && read.query_epoch == env.library.query().query_epoch();
                let discarded = read.discarded + 1;
                if unchanged || discarded >= MAX_DISCARDED_READS {
                    return self.fail_read();
                }
                if let Some(read) = &mut self.details_read {
                    read.discarded = discarded;
                }
                // A write or a query refresh discarded the read: it is made again.
                self.wait_to_read(env, effects);
            }
        }
    }

    fn on_idle(
        &mut self,
        ticket: Ticket,
        env: &LightboxEnv<'_>,
        effects: &mut Vec<LightboxEffect>,
    ) {
        let Some(read) = self
            .details_read
            .as_ref()
            .filter(|read| read.ticket == ticket && read.phase == ReadPhase::WaitingIdle)
        else {
            return;
        };
        if !self.read_is_current(read, env) {
            self.details_read = None;
        } else if env.library.query().query_pending() {
            // A refresh started while the writes ended.
            self.wait_to_read(env, effects);
        } else {
            self.begin_read(env, effects);
        }
    }

    /// `resolvePosition`: where the selected asset is in the result. `changed_query`:
    /// the index known before belongs to another result.
    fn resolve_position(
        &mut self,
        changed_query: bool,
        env: &LightboxEnv<'_>,
        effects: &mut Vec<LightboxEffect>,
    ) {
        self.failed_target = None;
        self.position_request += 1;
        self.navigation_request += 1;
        self.position_read = None;
        self.navigation_read = None;
        let Some(asset_id) = self.current_id() else {
            return;
        };
        let query = env.library.query();
        let pending = query.query_pending();
        let index = if pending {
            None
        } else if changed_query {
            query.asset_index(asset_id)
        } else {
            self.selected_index.or_else(|| query.asset_index(asset_id))
        };
        self.selected_index = index;
        self.target_index = index;
        if index.is_some() {
            self.navigation = NavigationStatus::Ready;
            return;
        }
        self.navigation = NavigationStatus::Resolving;
        if pending {
            return;
        }
        let ticket = self.ticket();
        self.position_read = Some(PositionRead {
            ticket,
            request: self.position_request,
            asset_id,
            query_epoch: query.query_epoch(),
        });
        effects.push(LightboxEffect::query(QueryInput::ResolvePosition {
            ticket,
            asset_id,
        }));
    }

    fn on_position_resolved(
        &mut self,
        ticket: Ticket,
        result: Result<AssetQueryPositionResult, QueryError>,
        env: &LightboxEnv<'_>,
    ) {
        let Some(read) = self.position_read.filter(|read| read.ticket == ticket) else {
            return;
        };
        self.position_read = None;
        if read.request != self.position_request {
            return;
        }
        let Ok(result) = result else {
            self.navigation = NavigationStatus::Failed;
            return;
        };
        if self.current_id() != Some(read.asset_id)
            || read.query_epoch != env.library.query().query_epoch()
        {
            return;
        }
        match result {
            AssetQueryPositionResult::Resolved { index } => {
                self.selected_index = Some(index);
                self.target_index = Some(index);
                self.navigation = NavigationStatus::Ready;
            }
            AssetQueryPositionResult::Missing => self.navigation = NavigationStatus::Missing,
            AssetQueryPositionResult::Stale => self.navigation = NavigationStatus::Failed,
        }
    }

    /// `select`. `None` closes.
    fn select(
        &mut self,
        summary: Option<AssetSummary>,
        index: Option<usize>,
        env: &LightboxEnv<'_>,
        effects: &mut Vec<LightboxEffect>,
    ) {
        self.failed_target = None;
        self.selection_request += 1;
        self.navigation_request += 1;
        self.position_request += 1;
        self.details_read = None;
        self.navigation_read = None;
        self.position_read = None;
        let Some(summary) = summary else {
            self.selected_index = None;
            self.target_index = None;
            self.publish(None, env, effects);
            self.detail_state = DetailState::Ready;
            return;
        };
        let asset_id = summary.id;
        let index = index.or_else(|| env.library.query().asset_index(asset_id));
        self.selected_index = index;
        self.target_index = index;
        let cached = env.library.cached_details(asset_id);
        let complete = cached.is_some();
        let selected = match cached {
            Some(details) => SelectedAsset::from_details(details),
            None => SelectedAsset {
                path: summary.preview_path.clone(),
                size_bytes: None,
                tags: env
                    .library
                    .asset_tags(asset_id)
                    .map(|canonical| canonical.tags.to_vec())
                    .unwrap_or_default(),
                summary,
            },
        };
        self.publish(Some(selected), env, effects);
        self.resolve_position(false, env, effects);
        if complete {
            self.detail_state = DetailState::Ready;
            if let Some(index) = self.selected_index {
                self.prefetch(index, env, effects);
            }
        } else {
            self.read_details(asset_id, false, env, effects);
        }
    }

    /// `navigate`: one step from the index the last press was heading for, wrapping
    /// at the ends of the result.
    fn navigate(&mut self, step: Step, env: &LightboxEnv<'_>, effects: &mut Vec<LightboxEffect>) {
        let snapshot = env.library.query().navigation_snapshot();
        // A refresh changes the query at once, before this model saw it.
        if snapshot.epoch != self.query_epoch || snapshot.pending {
            return;
        }
        let Some(previous) = self.target_index.or(self.selected_index) else {
            return;
        };
        if snapshot.total == 0 || self.current.is_none() {
            return;
        }
        let previous = previous % snapshot.total;
        let next = match step {
            Step::Next => (previous + 1) % snapshot.total,
            Step::Previous => (previous + snapshot.total - 1) % snapshot.total,
        };
        self.load_target(next, false, env, effects);
    }

    /// `loadTarget`: reads the row at `next`; only the answer of the latest call
    /// selects.
    fn load_target(
        &mut self,
        next: usize,
        retry: bool,
        env: &LightboxEnv<'_>,
        effects: &mut Vec<LightboxEffect>,
    ) {
        if !retry {
            self.failed_target = None;
        }
        self.navigation = NavigationStatus::Ready;
        self.target_index = Some(next);
        self.navigation_request += 1;
        let ticket = self.ticket();
        self.navigation_read = Some(NavigationRead {
            ticket,
            request: self.navigation_request,
            query_epoch: env.library.query().query_epoch(),
            target: next,
        });
        effects.push(LightboxEffect::query(if retry {
            QueryInput::RetryAsset {
                ticket,
                index: next,
            }
        } else {
            QueryInput::ReadAsset {
                ticket,
                index: next,
            }
        }));
    }

    fn on_asset_read(
        &mut self,
        ticket: Ticket,
        result: Result<Option<AssetSummary>, QueryError>,
        env: &LightboxEnv<'_>,
        effects: &mut Vec<LightboxEffect>,
    ) {
        if let Some(started_under) = self.prefetch.remove(&ticket) {
            return self.on_prefetched(started_under, result.ok().flatten(), env, effects);
        }
        let Some(read) = self.navigation_read.filter(|read| read.ticket == ticket) else {
            return;
        };
        self.navigation_read = None;
        if read.request != self.navigation_request
            || read.query_epoch != env.library.query().query_epoch()
        {
            return;
        }
        match result {
            Ok(Some(row)) => self.select(Some(row), Some(read.target), env, effects),
            _ => {
                // Earlier rapid presses never selected, so the next key steps from
                // the shown asset.
                self.target_index = self.selected_index;
                self.failed_target = Some(read.target);
                self.navigation = NavigationStatus::Failed;
            }
        }
    }

    /// `retryNavigation`: the failed row again, or the query again when the position
    /// was lost.
    fn retry_navigation(&mut self, env: &LightboxEnv<'_>, effects: &mut Vec<LightboxEffect>) {
        let snapshot = env.library.query().navigation_snapshot();
        if self.current.is_none() || snapshot.pending {
            return;
        }
        if let Some(target) = self.failed_target {
            if snapshot.epoch == self.query_epoch && self.selected_index.is_some() {
                self.load_target(target, true, env, effects);
            }
            return;
        }
        // The sync that follows the refresh resolves the position in the new result,
        // or reports that the refresh failed.
        effects.push(LightboxEffect::query(QueryInput::Refresh));
    }

    fn send(&mut self, write: PendingWrite, mutation: Mutation, effects: &mut Vec<LightboxEffect>) {
        let ticket = self.ticket();
        self.writes.insert(ticket, write);
        effects.push(LightboxEffect::library(LibraryInput::Mutate {
            ticket,
            mutation,
        }));
    }

    /// `drain`: sends the wanted tags of the asset unless a save of them is running.
    fn drain(&mut self, asset_id: i64, env: &LightboxEnv<'_>, effects: &mut Vec<LightboxEffect>) {
        let Some(intent) = self.intents.get_mut(&asset_id) else {
            return;
        };
        if intent.desired.is_none() || intent.state != WriteState::Idle {
            return;
        }
        if !env.library.can_edit_tags(asset_id) {
            // A range write dropped the base before this edit could be sent. It must
            // not look saved: it fails like one queued behind that write.
            intent.failed();
            self.sync_tags(env);
            return;
        }
        let Some((add, remove)) = intent.start() else {
            return;
        };
        let write = PendingWrite::Tags {
            asset_id,
            intent: intent.serial,
        };
        self.sync_tags(env);
        self.send(
            write,
            Mutation::SaveTags {
                asset_id,
                edit: TagEdit::Change { add, remove },
            },
            effects,
        );
    }

    /// `saveTags`.
    fn save_tags(
        &mut self,
        next: Vec<String>,
        env: &LightboxEnv<'_>,
        effects: &mut Vec<LightboxEffect>,
    ) {
        let Some(asset_id) = self.current_id() else {
            return;
        };
        // Never replaces a tag list that is not known completely.
        let Some(canonical) = env.library.asset_tags(asset_id) else {
            return;
        };
        if !self.intents.contains_key(&asset_id) {
            self.last_intent += 1;
            self.intents
                .insert(asset_id, TagIntent::new(self.last_intent, canonical.tags));
        }
        if let Some(intent) = self.intents.get_mut(&asset_id) {
            intent.desired = Some(normalize_tags(next));
            if intent.state != WriteState::Pending {
                intent.state = WriteState::Idle;
            }
        }
        self.sync_tags(env);
        self.drain(asset_id, env, effects);
    }

    /// `retryTags`.
    fn retry_tags(&mut self, env: &LightboxEnv<'_>, effects: &mut Vec<LightboxEffect>) {
        let Some(asset_id) = self.current_id() else {
            return;
        };
        let (Some(intent), Some(canonical)) = (
            self.intents.get_mut(&asset_id),
            env.library.asset_tags(asset_id),
        ) else {
            return;
        };
        if intent.state == WriteState::Pending {
            return;
        }
        intent.rebase(canonical.tags);
        intent.state = WriteState::Idle;
        self.sync_tags(env);
        self.drain(asset_id, env, effects);
    }

    /// `toggleFavorite`: queued behind the pending writes of the asset; a repeated
    /// toggle reverses the newest one.
    fn toggle_favorite(&mut self, env: &LightboxEnv<'_>, effects: &mut Vec<LightboxEffect>) {
        let Some(current) = &self.current else {
            return;
        };
        let asset_id = current.id();
        let identity = env.library.tags().epoch();
        let favorite = !self
            .favorite_writes
            .latest(asset_id, identity)
            .copied()
            .unwrap_or(current.summary.is_favorite);
        self.favorite_writes
            .start(asset_id, identity, Some(favorite));
        self.favorite_state = WriteState::Pending;
        self.send(
            PendingWrite::Favorite { asset_id, identity },
            Mutation::SetFavorite { asset_id, favorite },
            effects,
        );
    }

    /// `saveMediaGroup`: the drafts as they are; nothing for an order that is not
    /// valid.
    fn save_media_group(&mut self, env: &LightboxEnv<'_>, effects: &mut Vec<LightboxEffect>) {
        let Some(asset_id) = self.current_id() else {
            return;
        };
        let order = parse_media_group_order(&self.order_draft);
        let order_value = match order {
            GroupOrderDraft::Invalid => return,
            GroupOrderDraft::Clear => None,
            GroupOrderDraft::Order(order) => Some(order as f64),
        };
        let identity = env.library.tags().epoch();
        self.group_writes.start(asset_id, identity, None);
        self.group_state = WriteState::Pending;
        let key = self.key_draft.clone();
        self.send(
            PendingWrite::Group {
                asset_id,
                identity,
                key: key.clone(),
                order,
            },
            // The library trims the key; a blank key clears the group.
            Mutation::SetMediaGroup {
                asset_id,
                key: Some(key),
                order: order_value,
            },
            effects,
        );
    }

    /// `deleteSelected`.
    fn delete_selected(&mut self, request: u64, effects: &mut Vec<LightboxEffect>) {
        match self.current_id() {
            Some(asset_id) => self.send(
                PendingWrite::Delete { request },
                Mutation::DeleteAsset { asset_id },
                effects,
            ),
            None => self.events.push(LightboxEvent::DeleteSettled {
                request,
                result: Ok(None),
            }),
        }
    }

    fn on_mutation_settled(
        &mut self,
        ticket: Ticket,
        result: Result<MutationOutcome, MutationError>,
        env: &LightboxEnv<'_>,
        effects: &mut Vec<LightboxEffect>,
    ) {
        let Some(write) = self.writes.remove(&ticket) else {
            return;
        };
        match write {
            PendingWrite::Tags { asset_id, intent } => {
                // An intent of another identity, or a later one, is not this write's.
                let Some(intent) = self
                    .intents
                    .get_mut(&asset_id)
                    .filter(|candidate| candidate.serial == intent)
                else {
                    return;
                };
                match result {
                    Ok(MutationOutcome::TagsSaved { summary, .. }) => {
                        intent.saved(normalize_tags(summary.tags));
                    }
                    _ => intent.failed(),
                }
                let idle = intent.state == WriteState::Idle;
                let wanted = intent.desired.is_some();
                self.sync_tags(env);
                if idle && wanted {
                    self.drain(asset_id, env, effects);
                } else if idle && self.current_id() != Some(asset_id) {
                    self.intents.remove(&asset_id);
                }
            }
            PendingWrite::Favorite { asset_id, identity } => {
                let state = self
                    .favorite_writes
                    .finish(asset_id, identity, result.is_err());
                if let Some(state) = state.filter(|_| self.shown(asset_id, identity, env)) {
                    self.favorite_state = state;
                }
            }
            PendingWrite::Group {
                asset_id,
                identity,
                key,
                order,
            } => {
                let failed = result.is_err();
                if !failed && self.shown(asset_id, identity, env) {
                    // Drafts the user did not touch since follow the saved values.
                    if self.key_draft == key {
                        self.key_dirty = false;
                    }
                    if parse_media_group_order(&self.order_draft) == order {
                        self.order_dirty = false;
                    }
                    let current = self.current.clone();
                    self.publish(current, env, effects);
                }
                let state = self.group_writes.finish(asset_id, identity, failed);
                if let Some(state) = state.filter(|_| self.shown(asset_id, identity, env)) {
                    self.group_state = state;
                }
            }
            PendingWrite::Delete { request } => {
                let result = result.map(|outcome| match outcome {
                    MutationOutcome::Deleted(summary) => Some(DeletedAsset::from(summary)),
                    _ => None,
                });
                self.events
                    .push(LightboxEvent::DeleteSettled { request, result });
            }
        }
    }

    /// `mutations.subscribe`: a deletion closes the selection it names; published
    /// values are laid over the shown asset.
    fn on_commit(
        &mut self,
        commit: MetadataCommit,
        env: &LightboxEnv<'_>,
        effects: &mut Vec<LightboxEffect>,
    ) {
        let Some(current) = &self.current else {
            return;
        };
        let asset_id = current.id();
        match commit {
            MetadataCommit::Deleted(ids) => {
                if ids.contains(&asset_id) {
                    self.select(None, None, env, effects);
                }
            }
            MetadataCommit::Updated(patches) => {
                let Some((_, patch)) = patches.into_iter().find(|(id, _)| *id == asset_id) else {
                    return;
                };
                let mut next = current.clone();
                patch.summary.apply(&mut next.summary);
                if let Some(tags) = patch.tags {
                    next.tags = tags;
                }
                self.publish(Some(next), env, effects);
            }
        }
    }

    /// The effects of the Solid model, in their order: the identity epoch, the query,
    /// the tag revision, the cached row of the selected asset.
    fn sync(&mut self, env: &LightboxEnv<'_>, effects: &mut Vec<LightboxEffect>) {
        if self.disposed {
            return;
        }
        let tags = env.library.tags();
        let query = env.library.query();

        // The library was cleared or replaced: IDs may name other assets now.
        if tags.epoch() != self.identity {
            self.identity = tags.epoch();
            self.intents.clear();
            self.select(None, None, env, effects);
        }

        let snapshot = query.navigation_snapshot();
        if (snapshot.epoch, snapshot.pending) != (self.query_epoch, self.query_pending) {
            let changed = snapshot.epoch != self.query_epoch;
            self.query_epoch = snapshot.epoch;
            self.query_pending = snapshot.pending;
            self.resolve_position(changed, env, effects);
        }

        if tags.revision() != self.tag_revision {
            self.tag_revision = tags.revision();
            self.on_tags_changed(env, effects);
        }

        self.follow_cached_row(env, effects);

        // The library took the read since: its clock is the one the read started
        // under.
        if let Some(read) = &mut self.details_read {
            if read.phase == ReadPhase::Loading && read.token.is_none() {
                read.token = tags.peek_generation(read.asset_id);
            }
        }
        // The refresh a read waited for has ended.
        if self
            .details_read
            .as_ref()
            .is_some_and(|read| read.phase == ReadPhase::WaitingQuery)
            && !query.query_pending()
        {
            self.wait_to_read(env, effects);
        }
    }

    /// A tag list was published, dropped or deleted somewhere.
    fn on_tags_changed(&mut self, env: &LightboxEnv<'_>, effects: &mut Vec<LightboxEffect>) {
        // A save that waits behind another writer is merged onto that writer's
        // result, as the library will do when it sends it.
        for (asset_id, intent) in &mut self.intents {
            if intent.state != WriteState::Pending {
                continue;
            }
            if let Some(canonical) = env.library.asset_tags(*asset_id) {
                if canonical.tags != intent.confirmed.as_slice() {
                    intent.rebase_pending(canonical.tags);
                }
            }
        }
        let Some(current) = &self.current else {
            return;
        };
        let asset_id = current.id();
        let Some(canonical) = env.library.asset_tags(asset_id) else {
            // A selection-wide write drops every tag list, even one read while it
            // ran; a failed or running read already shows its own notice.
            if self.detail_state == DetailState::Ready {
                self.read_details(asset_id, true, env, effects);
            }
            return;
        };
        if let Some(intent) = self
            .intents
            .get_mut(&asset_id)
            .filter(|intent| intent.state != WriteState::Pending)
        {
            intent.rebase(canonical.tags);
        }
        if current.tags.as_slice() != canonical.tags {
            let mut next = current.clone();
            next.tags = canonical.tags.to_vec();
            self.publish(Some(next), env, effects);
        } else {
            self.sync_tags(env);
        }
    }

    /// The cached row of the selected asset is another one than the last sync saw (a
    /// write of another owner patched it, a refresh read it again): its fields replace
    /// the shown ones; the complete path, the size and the tags stay. A row that is
    /// not cached changes nothing, and neither does a refresh that has not answered or
    /// has failed, whose rows are still the old ones.
    fn follow_cached_row(&mut self, env: &LightboxEnv<'_>, effects: &mut Vec<LightboxEffect>) {
        let Some(current) = &self.current else {
            self.seen_row = None;
            return;
        };
        let query = env.library.query();
        let Some(row) = query.cached_asset(current.id()) else {
            self.seen_row = None;
            return;
        };
        // The epoch of rows that a query session stands behind.
        let settled =
            (!query.query_pending() && query.session_id().is_some()).then(|| query.query_epoch());
        let seen_epoch = self.seen_row.as_ref().map(|(epoch, _)| *epoch);
        if self.seen_row.as_ref().is_some_and(|(seen_epoch, seen)| {
            seen == row && settled.is_none_or(|epoch| epoch == *seen_epoch)
        }) {
            return;
        }
        let epoch = settled
            .or(seen_epoch)
            .unwrap_or_else(|| query.query_epoch());
        self.seen_row = Some((epoch, row.clone()));
        if *row == current.summary {
            return;
        }
        let next = SelectedAsset {
            summary: row.clone(),
            path: current.path.clone().or_else(|| row.preview_path.clone()),
            size_bytes: current.size_bytes,
            tags: current.tags.clone(),
        };
        self.publish(Some(next), env, effects);
    }

    fn command(
        &mut self,
        command: LightboxCommand,
        env: &LightboxEnv<'_>,
        effects: &mut Vec<LightboxEffect>,
    ) {
        match command {
            LightboxCommand::Select { asset, index } => {
                self.select(Some(*asset), index, env, effects);
            }
            LightboxCommand::Close => self.select(None, None, env, effects),
            LightboxCommand::Previous => self.navigate(Step::Previous, env, effects),
            LightboxCommand::Next => self.navigate(Step::Next, env, effects),
            LightboxCommand::SaveTags(tags) => self.save_tags(tags, env, effects),
            LightboxCommand::RetryTags => self.retry_tags(env, effects),
            LightboxCommand::RetryDetails => {
                if let Some(asset_id) = self.current_id() {
                    self.read_details(asset_id, false, env, effects);
                }
            }
            LightboxCommand::RetryNavigation => self.retry_navigation(env, effects),
            LightboxCommand::ToggleFavorite => self.toggle_favorite(env, effects),
            LightboxCommand::SetMediaGroupKeyDraft(value) => {
                self.key_dirty = true;
                self.key_draft = value;
            }
            LightboxCommand::SetMediaGroupOrderDraft(value) => {
                if parse_media_group_order(&value) != GroupOrderDraft::Invalid {
                    self.order_dirty = true;
                    self.order_draft = value;
                }
            }
            LightboxCommand::SaveMediaGroup => self.save_media_group(env, effects),
            LightboxCommand::DeleteSelected { request } => self.delete_selected(request, effects),
        }
    }

    fn dispose(&mut self) {
        if self.disposed {
            return;
        }
        self.disposed = true;
        self.selection_request += 1;
        self.navigation_request += 1;
        self.position_request += 1;
        self.details_read = None;
        self.navigation_read = None;
        self.position_read = None;
        self.prefetch.clear();
        self.pinned = None;
        self.intents.clear();
        // A write that is on its way still ends in the library and publishes nothing
        // here. A deletion is a command with an answer, and the confirmation that
        // sent it keeps its exits locked until the answer: it is given now, in the
        // order the deletions were sent, and the library's own answer finds no
        // write to settle.
        let mut deletions: Vec<(Ticket, u64)> = std::mem::take(&mut self.writes)
            .into_iter()
            .filter_map(|(ticket, write)| match write {
                PendingWrite::Delete { request } => Some((ticket, request)),
                _ => None,
            })
            .collect();
        deletions.sort_unstable();
        for (_, request) in deletions {
            self.refuse_deletion(request);
        }
    }

    /// The answer to a deletion the disposed model cannot see through.
    fn refuse_deletion(&mut self, request: u64) {
        self.events.push(LightboxEvent::DeleteSettled {
            request,
            result: Err(MutationError::Refused(TagStateError::Unavailable)),
        });
    }

    /// Applies one input in the state of `env` and returns what to do for it.
    ///
    /// An answer or a commit is applied as it is; what the library changed besides is
    /// looked at by the [`LightboxInput::Sync`] the owner sends after the answers of
    /// one turn. The order matters for a tag save: the library publishes the saved
    /// tags and answers the write in one turn, and the answer has to be applied
    /// before the new tags are compared with what the editor wants.
    pub fn handle(&mut self, input: LightboxInput, env: LightboxEnv<'_>) -> Vec<LightboxEffect> {
        let mut effects = Vec::new();
        if self.disposed {
            // Nothing is asked for or published again; a deletion is still answered.
            if let LightboxInput::Command(LightboxCommand::DeleteSelected { request }) = input {
                self.refuse_deletion(request);
            }
            return effects;
        }
        let env = &env;
        match input {
            LightboxInput::Sync => self.sync(env, &mut effects),
            LightboxInput::Command(command) => {
                self.command(command, env, &mut effects);
                self.sync(env, &mut effects);
            }
            LightboxInput::Library(event) => match event {
                LibraryEvent::DetailsLoaded { ticket, result } => {
                    self.on_details_loaded(ticket, result, env, &mut effects);
                }
                LibraryEvent::MutationSettled { ticket, result } => {
                    self.on_mutation_settled(ticket, result, env, &mut effects);
                }
                LibraryEvent::Idle { ticket } => self.on_idle(ticket, env, &mut effects),
                _ => {}
            },
            LightboxInput::Query(event) => match event {
                QueryEvent::AssetRead { ticket, result } => {
                    self.on_asset_read(ticket, result, env, &mut effects);
                }
                QueryEvent::PositionResolved { ticket, result } => {
                    self.on_position_resolved(ticket, result, env);
                }
                _ => {}
            },
            LightboxInput::Committed(commit) => self.on_commit(commit, env, &mut effects),
            LightboxInput::Dispose => self.dispose(),
        }
        effects
    }
}

/// The lightbox model together with the library it reads: everything the lightbox
/// shows (the getters of `LightboxModel`).
#[derive(Debug, Clone, Copy)]
pub struct LightboxView<'a> {
    pub lightbox: &'a Lightbox,
    pub library: &'a Library,
}

impl<'a> LightboxView<'a> {
    /// `selected`.
    pub fn selected(&self) -> Option<&'a SelectedAsset> {
        self.lightbox.selected()
    }

    /// Whether an asset is selected: the lightbox is open.
    pub fn is_open(&self) -> bool {
        self.lightbox.current.is_some()
    }

    pub fn selected_index(&self) -> Option<usize> {
        self.lightbox.selected_index
    }

    /// `navigationStatus`.
    pub fn navigation_status(&self) -> NavigationStatus {
        self.lightbox.navigation
    }

    /// `assetDetailsFailed`: the details could not be read; Retry is offered.
    pub fn asset_details_failed(&self) -> bool {
        self.lightbox.detail_state == DetailState::Failed
    }

    fn tags_unknown(&self) -> bool {
        self.lightbox
            .current_id()
            .is_some_and(|asset_id| !self.library.can_edit_tags(asset_id))
    }

    /// `tagDetailsLoading`: the complete tag list is being read.
    pub fn tag_details_loading(&self) -> bool {
        self.tags_unknown() && self.lightbox.detail_state == DetailState::Loading
    }

    /// `tagDetailsFailed`.
    pub fn tag_details_failed(&self) -> bool {
        self.tags_unknown() && self.lightbox.detail_state == DetailState::Failed
    }

    /// `tagEditingDisabled`: nothing is selected, or the complete tag list of the
    /// selected asset is not known.
    pub fn tag_editing_disabled(&self) -> bool {
        self.lightbox.current.is_none() || self.tags_unknown()
    }

    /// `tagEditor`: the chips of the tag editor.
    pub fn tag_editor(&self) -> &'a [String] {
        &self.lightbox.editor
    }

    pub fn tag_saving(&self) -> bool {
        self.lightbox.tag_state == WriteState::Pending
    }

    pub fn tag_failed(&self) -> bool {
        self.lightbox.tag_state == WriteState::Failed
    }

    pub fn favorite_pending(&self) -> bool {
        self.lightbox.favorite_state == WriteState::Pending
    }

    pub fn favorite_failed(&self) -> bool {
        self.lightbox.favorite_state == WriteState::Failed
    }

    pub fn group_pending(&self) -> bool {
        self.lightbox.group_state == WriteState::Pending
    }

    pub fn group_failed(&self) -> bool {
        self.lightbox.group_state == WriteState::Failed
    }

    /// `mediaGroupKeyEditor`.
    pub fn media_group_key_editor(&self) -> &'a str {
        &self.lightbox.key_draft
    }

    /// `mediaGroupOrderEditor`.
    pub fn media_group_order_editor(&self) -> &'a str {
        &self.lightbox.order_draft
    }

    /// Whether Apply can save the group drafts: the order draft is empty or a valid
    /// order (a stored order that is not one stays shown and cannot be submitted).
    pub fn media_group_valid(&self) -> bool {
        parse_media_group_order(&self.lightbox.order_draft) != GroupOrderDraft::Invalid
    }
}
