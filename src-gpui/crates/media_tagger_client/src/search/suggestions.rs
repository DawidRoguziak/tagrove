//! The suggestion client of the application (`SuggestionClient.ts` with its worker,
//! plan decision D13).
//!
//! One client serves every suggesting field (search, and later the tag editors). It
//! runs one search at a time on the executor; each field ([`EditorId`]) keeps only its
//! latest request, so a burst of keystrokes costs one search per field, not one per
//! key. A search that a newer request of its field replaces is stopped between tags
//! ([`Cancel`]) and its answer is dropped. The vocabulary is identified by its
//! `Arc`: a new `Arc` is a new version, older queued requests are dropped, and the
//! runner builds the search index again only then
//! ([`SuggestionIndexCache`]), as the Solid client sends the worker a vocabulary
//! message only for a new array.
//!
//! A failed search (the task panicked or was lost) fails the client: that request and
//! every queued one answer with [`SuggestionFailure`], and so does every later one
//! until [`SuggestionInput::Retry`]. The fields show a Retry button then and plain
//! text search keeps working.

use std::{
    collections::{BTreeSet, HashSet, VecDeque},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use super::{
    active_token::ActiveToken,
    matcher::{build_tag_suggestions, Cancel, Cancelled, TagIndex, TagSuggestion},
};
use crate::model::{Dispatch, Model, Runner};

/// One suggesting field. Each field takes its own with [`EditorId::next`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EditorId(u64);

impl EditorId {
    /// An id no earlier call returned in this process.
    pub fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }

    pub fn value(self) -> u64 {
        self.0
    }
}

/// What a request's answer belongs to: the active token, the tags already used and
/// the hard exclusions (`context` of `createSuggestions`). A failure belongs to it too.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct SuggestionContext {
    pub token: Option<ActiveToken>,
    /// Tags of the draft and hard exclusions, lowercased.
    pub used: BTreeSet<String>,
    /// Tags never to suggest (an editor's assigned tags), as given.
    pub excluded: Vec<String>,
}

/// One request of a field: its context and how often the field asked to retry.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SuggestionRequest {
    pub context: SuggestionContext,
    pub retry: u64,
}

/// Why suggestions are unavailable ("Tag suggestions unavailable").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SuggestionFailure;

/// Commands and the answers of searches.
#[derive(Debug)]
pub enum SuggestionInput {
    /// The latest request of `editor`; replaces its queued one and stops its running
    /// one.
    Search {
        editor: EditorId,
        vocabulary: Arc<Vec<String>>,
        request: SuggestionRequest,
    },
    /// `editor` wants nothing any more (blur, a changed draft, unmount).
    Cancel { editor: EditorId },
    /// Clears the failure, so the next request searches again.
    Retry,
    /// The owner is going away: nothing is answered or searched any more.
    Dispose,
    /// A search ended. `Err` when the task failed.
    Answered {
        job: u64,
        result: Result<Result<Vec<TagSuggestion>, Cancelled>, SuggestionFailure>,
    },
}

/// Work for the runner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SuggestionEffect {
    /// Search `vocabulary` for the request's token; answered with
    /// [`SuggestionInput::Answered`].
    Search {
        job: u64,
        vocabulary: Arc<Vec<String>>,
        context: SuggestionContext,
        cancel: Cancel,
    },
}

/// The answer to a field's request.
#[derive(Debug, Clone)]
pub enum SuggestionEvent {
    Settled {
        editor: EditorId,
        request: SuggestionRequest,
        vocabulary: Arc<Vec<String>>,
        result: Result<Arc<[TagSuggestion]>, SuggestionFailure>,
    },
}

#[derive(Debug)]
struct Job {
    id: u64,
    editor: EditorId,
    version: u64,
    request: SuggestionRequest,
    vocabulary: Arc<Vec<String>>,
    cancel: Cancel,
}

#[derive(Debug, Default)]
pub struct SuggestionClient {
    vocabulary: Option<Arc<Vec<String>>>,
    version: u64,
    /// Indexes built: one per new vocabulary that reached a search.
    indexed_version: u64,
    next_job: u64,
    pending: VecDeque<Job>,
    active: Option<Job>,
    failed: bool,
    disposed: bool,
    events: Vec<SuggestionEvent>,
}

impl SuggestionClient {
    pub fn new() -> Self {
        Self::default()
    }

    /// Counts the vocabularies the client has seen; a request with the same `Arc`
    /// does not count.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// How many vocabularies were handed to a search (the worker's vocabulary
    /// messages of the Solid client).
    pub fn indexed_versions(&self) -> u64 {
        self.indexed_version
    }

    /// Whether a search runs.
    pub fn is_searching(&self) -> bool {
        self.active.is_some()
    }

    /// How many requests wait for the running search.
    pub fn queued(&self) -> usize {
        self.pending.len()
    }

    pub fn is_failed(&self) -> bool {
        self.failed
    }

    /// The answers since the last call, oldest first.
    pub fn take_events(&mut self) -> Vec<SuggestionEvent> {
        std::mem::take(&mut self.events)
    }

    /// The answers for `editor` since the last call; the others stay.
    pub fn take_events_for(&mut self, editor: EditorId) -> Vec<SuggestionEvent> {
        let (mine, others) = std::mem::take(&mut self.events)
            .into_iter()
            .partition(|SuggestionEvent::Settled { editor: owner, .. }| *owner == editor);
        self.events = others;
        mine
    }

    fn cancel(&mut self, editor: EditorId) {
        self.pending.retain(|job| job.editor != editor);
        if let Some(active) = &self.active {
            if active.editor == editor {
                active.cancel.cancel();
            }
        }
    }

    fn settle(&mut self, job: Job, result: Result<Arc<[TagSuggestion]>, SuggestionFailure>) {
        self.events.push(SuggestionEvent::Settled {
            editor: job.editor,
            request: job.request,
            vocabulary: job.vocabulary,
            result,
        });
    }

    fn fail(&mut self) {
        self.failed = true;
        let jobs: Vec<Job> = self
            .active
            .take()
            .into_iter()
            .chain(self.pending.drain(..))
            .collect();
        for job in jobs {
            if !job.cancel.is_cancelled() {
                self.settle(job, Err(SuggestionFailure));
            }
        }
    }

    fn pump(&mut self, effects: &mut Vec<SuggestionEffect>) {
        if self.active.is_some() || self.pending.is_empty() {
            return;
        }
        if self.failed {
            self.fail();
            return;
        }
        while let Some(job) = self.pending.pop_front() {
            if job.version != self.version {
                continue;
            }
            if job.version > self.indexed_version {
                self.indexed_version = job.version;
            }
            effects.push(SuggestionEffect::Search {
                job: job.id,
                vocabulary: Arc::clone(&job.vocabulary),
                context: job.request.context.clone(),
                cancel: job.cancel.clone(),
            });
            self.active = Some(job);
            return;
        }
    }
}

impl Model for SuggestionClient {
    type Input = SuggestionInput;
    type Effect = SuggestionEffect;

    fn handle(&mut self, input: SuggestionInput) -> Vec<SuggestionEffect> {
        let mut effects = Vec::new();
        match input {
            SuggestionInput::Search {
                editor,
                vocabulary,
                request,
            } => {
                if self.disposed {
                    return effects;
                }
                if !self
                    .vocabulary
                    .as_ref()
                    .is_some_and(|current| Arc::ptr_eq(current, &vocabulary))
                {
                    self.vocabulary = Some(Arc::clone(&vocabulary));
                    self.version += 1;
                }
                self.cancel(editor);
                self.next_job += 1;
                self.pending.push_back(Job {
                    id: self.next_job,
                    editor,
                    version: self.version,
                    request,
                    vocabulary,
                    cancel: Cancel::new(),
                });
                self.pump(&mut effects);
            }
            SuggestionInput::Cancel { editor } => self.cancel(editor),
            SuggestionInput::Retry => self.failed = false,
            SuggestionInput::Dispose => {
                self.disposed = true;
                if let Some(active) = self.active.take() {
                    active.cancel.cancel();
                }
                self.pending.clear();
            }
            SuggestionInput::Answered { job, result } => {
                let Some(active) = self.active.take_if(|active| active.id == job) else {
                    return effects;
                };
                match result {
                    Ok(Ok(items)) => {
                        if !active.cancel.is_cancelled() && active.version == self.version {
                            self.settle(active, Ok(items.into()));
                        }
                    }
                    Ok(Err(Cancelled)) => {}
                    Err(SuggestionFailure) => {
                        self.active = Some(active);
                        self.fail();
                        return effects;
                    }
                }
                self.pump(&mut effects);
            }
        }
        effects
    }
}

/// The search index of the latest vocabulary, kept between searches; a new vocabulary
/// replaces it.
#[derive(Debug, Default)]
pub struct SuggestionIndexCache {
    index: Mutex<Option<Arc<TagIndex>>>,
}

impl SuggestionIndexCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// The index of `vocabulary`, built now unless it is the cached one's.
    pub fn index(&self, vocabulary: &Arc<Vec<String>>) -> Arc<TagIndex> {
        let mut cached = self
            .index
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(index) = cached.as_ref() {
            if Arc::ptr_eq(index.tags(), vocabulary) {
                return Arc::clone(index);
            }
        }
        let index = Arc::new(TagIndex::new(Arc::clone(vocabulary)));
        *cached = Some(Arc::clone(&index));
        index
    }
}

/// Runs one effect of a [`SuggestionClient`] on the executor. `wrap` turns its inputs
/// into the inputs of the model that owns the inbox.
pub fn run_suggestion_effect<I, W>(
    cache: &Arc<SuggestionIndexCache>,
    effect: SuggestionEffect,
    dispatch: &Dispatch<I>,
    wrap: W,
) where
    I: Send + 'static,
    W: Fn(SuggestionInput) -> I + Send + 'static,
{
    match effect {
        SuggestionEffect::Search {
            job,
            vocabulary,
            context,
            cancel,
        } => {
            let cache = Arc::clone(cache);
            dispatch.call(
                "tag suggestions",
                move || {
                    if cancel.is_cancelled() {
                        return Ok(Err(Cancelled));
                    }
                    let index = cache.index(&vocabulary);
                    // Hard exclusions are stored names, compared as given.
                    let excluded: HashSet<String> = context.excluded.iter().cloned().collect();
                    Ok(build_tag_suggestions(
                        context.token.as_ref(),
                        &context.used,
                        &excluded,
                        &index,
                        &cancel,
                    ))
                },
                move |result| {
                    wrap(SuggestionInput::Answered {
                        job,
                        result: result.map_err(|_| SuggestionFailure),
                    })
                },
            );
        }
    }
}

/// Runs the effects of a client that is driven on its own.
pub struct SuggestionRunner {
    cache: Arc<SuggestionIndexCache>,
}

impl SuggestionRunner {
    pub fn new() -> Self {
        Self {
            cache: Arc::new(SuggestionIndexCache::new()),
        }
    }
}

impl Default for SuggestionRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl Runner<SuggestionClient> for SuggestionRunner {
    fn run(&mut self, effect: SuggestionEffect, dispatch: &Dispatch<SuggestionInput>) {
        run_suggestion_effect(&self.cache, effect, dispatch, std::convert::identity);
    }
}

#[cfg(test)]
mod tests;
