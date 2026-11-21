//! The known tags of the library (`createKnownTags` of the Solid frontend).
//!
//! [`KnownTags`] is the shared in-memory vocabulary for search suggestions, the tag
//! editors and the tag list's fallback. It is a model in the convention of
//! [`crate::model`]: a refresh reads `list_tags` in pages of
//! [`VOCABULARY_PAGE_SIZE`] until the reported total is loaded, and only a complete
//! read is published, so a failed later page keeps the previous vocabulary.
//!
//! # Passes and generations
//!
//! At most one pass runs. A refresh that arrives while one runs joins it and asks
//! for one more read with the state of then: the running read stops between two
//! pages and starts over, and what it had read is thrown away. A failed page of a
//! read that was already superseded does not fail the pass; the newer read still
//! runs.
//!
//! # After a write
//!
//! [`VocabularyInput::RefreshAfterMutation`] carries `vocabulary_changed` of the
//! backend's answer. A write that changed the vocabulary refreshes it. One that did
//! not joins a running pass without asking for another read, reads nothing when the
//! snapshot is current, and refreshes when the last refresh failed (the snapshot is
//! *dirty* until a pass completed).
//!
//! # Readers
//!
//! A refresh may carry a [`Ticket`]; it is answered exactly once with a
//! [`VocabularyEvent`] when its pass ends: the published vocabulary, the error of
//! the page that failed, or an empty list when a reset or the disposal overtook it
//! (answered at that moment). The published list is replaced only when its length
//! or one of its values in order differs, so a reader can compare the `Arc`.

use std::{collections::VecDeque, sync::Arc};

use media_tagger_core::models::TagListPage;

use super::query::Ticket;
use crate::{
    backend::{Backend, BackendError, BackendResult},
    model::{Dispatch, Model, RequestToken, Runner, Tokens},
};

/// Tags per `list_tags` call: the most the backend serves in one page.
pub const VOCABULARY_PAGE_SIZE: i64 = 200;

/// How a ticketed refresh ended: the published vocabulary, or the error of the page
/// that failed.
pub type VocabularyResult = Result<Arc<Vec<String>>, BackendError>;

/// Commands and backend replies.
#[derive(Debug)]
pub enum VocabularyInput {
    /// `refreshKnownTags`: read the vocabulary again, with the state of now.
    Refresh { ticket: Option<Ticket> },
    /// `refreshKnownTagsAfterMutation`: what an accepted write asks for.
    RefreshAfterMutation {
        vocabulary_changed: bool,
        ticket: Option<Ticket>,
    },
    /// The library was cleared: no tags, and a pass on its way publishes nothing.
    Reset,
    /// The owner is going away: [`Reset`](Self::Reset), and nothing is read again.
    Dispose,
    PageLoaded {
        token: RequestToken,
        result: BackendResult<TagListPage>,
    },
}

/// Work for the runner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VocabularyEffect {
    /// `list_tags` with an empty query; answered with
    /// [`VocabularyInput::PageLoaded`].
    LoadPage {
        token: RequestToken,
        offset: i64,
        limit: i64,
    },
}

/// The answer to a refresh that carried a [`Ticket`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VocabularyEvent {
    Settled {
        ticket: Ticket,
        result: VocabularyResult,
    },
}

/// The read that is on its way.
#[derive(Debug)]
struct Pass {
    /// The generation this read serves; a newer one makes it start over.
    requested: u64,
    items: Vec<String>,
    request: RequestToken,
    waiters: Vec<Ticket>,
}

#[derive(Debug)]
pub struct KnownTags {
    snapshot: Arc<Vec<String>>,
    version: u64,
    /// No pass completed since the last refresh request, failure or reset.
    dirty: bool,
    generation: u64,
    mounted: bool,
    tokens: Tokens,
    pass: Option<Pass>,
    events: VecDeque<VocabularyEvent>,
}

impl Default for KnownTags {
    fn default() -> Self {
        Self::new()
    }
}

impl KnownTags {
    /// An empty vocabulary that has not been read yet.
    pub fn new() -> Self {
        Self {
            snapshot: Arc::new(Vec::new()),
            version: 0,
            dirty: true,
            generation: 0,
            mounted: true,
            tokens: Tokens::default(),
            pass: None,
            events: VecDeque::new(),
        }
    }

    /// The vocabulary as the backend last listed it completely, in the backend's
    /// order. The same `Arc` as before when a refresh found it unchanged.
    pub fn known_tags(&self) -> &Arc<Vec<String>> {
        &self.snapshot
    }

    /// Counts the replacements of [`known_tags`](Self::known_tags).
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Whether a pass is on its way.
    pub fn refreshing(&self) -> bool {
        self.pass.is_some()
    }

    /// Whether the snapshot may be outdated: never read, a refresh on its way, or
    /// the last one failed.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// The answers to ticketed refreshes since the last call, oldest first.
    pub fn take_events(&mut self) -> Vec<VocabularyEvent> {
        self.events.drain(..).collect()
    }

    fn answer(&mut self, waiters: impl IntoIterator<Item = Ticket>, result: &VocabularyResult) {
        for ticket in waiters {
            self.events.push_back(VocabularyEvent::Settled {
                ticket,
                result: result.clone(),
            });
        }
    }

    fn load_page(&mut self, offset: usize) -> (RequestToken, VocabularyEffect) {
        let token = self.tokens.issue();
        (
            token,
            VocabularyEffect::LoadPage {
                token,
                offset: i64::try_from(offset).unwrap_or(i64::MAX),
                limit: VOCABULARY_PAGE_SIZE,
            },
        )
    }

    fn refresh(&mut self, ticket: Option<Ticket>, effects: &mut Vec<VocabularyEffect>) {
        self.dirty = true;
        self.generation += 1;
        if let Some(pass) = &mut self.pass {
            pass.waiters.extend(ticket);
            return;
        }
        if !self.mounted {
            self.answer(ticket, &Ok(Arc::new(Vec::new())));
            return;
        }
        let (request, effect) = self.load_page(0);
        self.pass = Some(Pass {
            requested: self.generation,
            items: Vec::new(),
            request,
            waiters: ticket.into_iter().collect(),
        });
        effects.push(effect);
    }

    fn refresh_after_mutation(
        &mut self,
        vocabulary_changed: bool,
        ticket: Option<Ticket>,
        effects: &mut Vec<VocabularyEffect>,
    ) {
        if vocabulary_changed {
            self.refresh(ticket, effects);
        } else if let Some(pass) = &mut self.pass {
            pass.waiters.extend(ticket);
        } else if self.dirty {
            self.refresh(ticket, effects);
        } else {
            let snapshot = Ok(Arc::clone(&self.snapshot));
            self.answer(ticket, &snapshot);
        }
    }

    fn reset(&mut self) {
        self.generation += 1;
        // The pass is forgotten with its token; its reply changes nothing.
        if let Some(pass) = self.pass.take() {
            self.answer(pass.waiters, &Ok(Arc::new(Vec::new())));
        }
        self.dirty = true;
        self.snapshot = Arc::new(Vec::new());
        self.version += 1;
    }

    /// Starts the running pass over for the current generation.
    fn restart(&mut self, mut pass: Pass, effects: &mut Vec<VocabularyEffect>) {
        let (request, effect) = self.load_page(0);
        pass.requested = self.generation;
        pass.items.clear();
        pass.request = request;
        self.pass = Some(pass);
        effects.push(effect);
    }

    fn on_page_loaded(
        &mut self,
        token: RequestToken,
        result: BackendResult<TagListPage>,
        effects: &mut Vec<VocabularyEffect>,
    ) {
        let Some(mut pass) = self.pass.take_if(|pass| pass.request == token) else {
            return;
        };
        if pass.requested != self.generation {
            // A newer refresh still needs its read, also when the superseded one
            // failed.
            self.restart(pass, effects);
            return;
        }
        let page = match result {
            Ok(page) => page,
            Err(error) => {
                self.answer(pass.waiters, &Err(error));
                return;
            }
        };
        let empty = page.items.is_empty();
        pass.items.extend(page.items);
        let total = usize::try_from(page.total).unwrap_or(0);
        if !empty && pass.items.len() < total {
            let (request, effect) = self.load_page(pass.items.len());
            pass.request = request;
            self.pass = Some(pass);
            effects.push(effect);
            return;
        }
        if *self.snapshot != pass.items {
            self.snapshot = Arc::new(pass.items);
            self.version += 1;
        }
        self.dirty = false;
        let snapshot = Ok(Arc::clone(&self.snapshot));
        self.answer(pass.waiters, &snapshot);
    }
}

impl Model for KnownTags {
    type Input = VocabularyInput;
    type Effect = VocabularyEffect;

    fn handle(&mut self, input: VocabularyInput) -> Vec<VocabularyEffect> {
        let mut effects = Vec::new();
        match input {
            VocabularyInput::Refresh { ticket } => self.refresh(ticket, &mut effects),
            VocabularyInput::RefreshAfterMutation {
                vocabulary_changed,
                ticket,
            } => self.refresh_after_mutation(vocabulary_changed, ticket, &mut effects),
            VocabularyInput::Reset => self.reset(),
            VocabularyInput::Dispose => {
                self.mounted = false;
                self.reset();
            }
            VocabularyInput::PageLoaded { token, result } => {
                self.on_page_loaded(token, result, &mut effects);
            }
        }
        effects
    }
}

/// Runs one effect of a [`KnownTags`] through the backend port. `wrap` turns its
/// inputs into the inputs of the model that owns the inbox.
pub fn run_vocabulary_effect<I, W>(
    backend: &Arc<dyn Backend>,
    effect: VocabularyEffect,
    dispatch: &Dispatch<I>,
    wrap: W,
) where
    I: Send + 'static,
    W: Fn(VocabularyInput) -> I + Send + 'static,
{
    let backend = Arc::clone(backend);
    match effect {
        VocabularyEffect::LoadPage {
            token,
            offset,
            limit,
        } => dispatch.call(
            "list_tags",
            move || backend.list_tags(String::new(), offset, limit),
            move |result| wrap(VocabularyInput::PageLoaded { token, result }),
        ),
    }
}

/// Runs the effects of a vocabulary that is driven on its own.
pub struct VocabularyRunner {
    backend: Arc<dyn Backend>,
}

impl VocabularyRunner {
    pub fn new(backend: Arc<dyn Backend>) -> Self {
        Self { backend }
    }
}

impl Runner<KnownTags> for VocabularyRunner {
    fn run(&mut self, effect: VocabularyEffect, dispatch: &Dispatch<VocabularyInput>) {
        run_vocabulary_effect(&self.backend, effect, dispatch, std::convert::identity);
    }
}

#[cfg(test)]
mod tests;
