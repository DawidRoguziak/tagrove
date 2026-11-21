//! The shared details cache (`createDetailsCache` of the Solid frontend).
//!
//! Query pages carry summaries only; the path, the size and the complete tag list of
//! an asset come from `get_asset_details`. [`DetailsCache`] keeps the
//! [`MAX_CACHED_DETAILS`] most recently used answers and knows which reads are on
//! their way, one per asset. It does no I/O: the library model
//! ([`Library`](super::Library)) asks it what to do for a read and hands it the
//! backend's reply.
//!
//! A cached record is only as good as the tag state says: it is stored with the
//! asset's [`TagToken`] and is dropped on the first read after that token stopped
//! being current (a write on the asset, its deletion, an identity reset).
//! [`clear`](DetailsCache::clear) starts a new generation, which the library model
//! does for every new query; a reply that was requested under an older generation is
//! not stored.

use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use media_tagger_core::models::AssetDetails;

use super::{
    query::Ticket,
    tag_state::{AssetTagState, TagToken},
};
use crate::{
    backend::{BackendError, BackendResult},
    model::{RequestToken, Tokens},
};

/// Most details the cache keeps.
pub const MAX_CACHED_DETAILS: usize = 256;

/// What a ticketed details read ended with: the details with the canonical tags,
/// `None` when the asset is gone or the read became obsolete, or the backend's error.
pub type DetailsResult = Result<Option<Arc<AssetDetails>>, BackendError>;

#[derive(Debug)]
struct Entry {
    token: TagToken,
    details: Arc<AssetDetails>,
    stamp: u64,
}

/// A `get_asset_details` call whose reply is still out.
#[derive(Debug)]
struct Flight {
    asset_id: i64,
    /// The cache generation the read started under.
    generation: u64,
    tag_token: TagToken,
    waiters: Vec<Ticket>,
}

/// What [`DetailsCache::begin_load`] decided for a read.
#[derive(Debug)]
pub enum LoadStart {
    /// The details are cached: answer the reader now.
    Cached(Arc<AssetDetails>),
    /// A read of the asset is on its way; the reader waits for it.
    Joined,
    /// Call `get_asset_details` and answer with [`DetailsCache::finish_load`].
    Request(RequestToken),
    /// The cache was disposed: answer the reader with nothing.
    Disposed,
}

#[derive(Debug, Default)]
pub struct DetailsCache {
    entries: HashMap<i64, Entry>,
    /// Use order of the entries by stamp, least recently used first.
    order: BTreeMap<u64, i64>,
    last_stamp: u64,
    generation: u64,
    disposed: bool,
    /// The read each asset's readers share.
    pending: HashMap<i64, RequestToken>,
    /// Every read whose reply is still out, also the ones a clear made obsolete.
    flights: HashMap<RequestToken, Flight>,
}

impl DetailsCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Advances with every [`clear`](Self::clear). A reader that saw another
    /// generation knows the cache was emptied meanwhile.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Reads that are on their way.
    pub fn in_flight(&self) -> usize {
        self.flights.len()
    }

    fn forget(&mut self, asset_id: i64) -> Option<Entry> {
        let entry = self.entries.remove(&asset_id)?;
        self.order.remove(&entry.stamp);
        Some(entry)
    }

    fn store(&mut self, token: TagToken, details: Arc<AssetDetails>) {
        let asset_id = details.summary.id;
        self.forget(asset_id);
        self.last_stamp += 1;
        self.order.insert(self.last_stamp, asset_id);
        self.entries.insert(
            asset_id,
            Entry {
                token,
                details,
                stamp: self.last_stamp,
            },
        );
    }

    /// The cached details of the asset, marked as recently used. A record whose
    /// tag token is no longer current is dropped and not returned.
    pub fn get(&mut self, tags: &AssetTagState, asset_id: i64) -> Option<Arc<AssetDetails>> {
        let entry = self.forget(asset_id)?;
        if !tags.is_current(&entry.token) {
            return None;
        }
        let details = Arc::clone(&entry.details);
        self.store(entry.token, entry.details);
        Some(details)
    }

    /// [`get`](Self::get) for a reader that only looks (a view): nothing is dropped
    /// and the use order stays.
    pub fn peek(&self, tags: &AssetTagState, asset_id: i64) -> Option<&Arc<AssetDetails>> {
        self.entries
            .get(&asset_id)
            .filter(|entry| tags.is_current(&entry.token))
            .map(|entry| &entry.details)
    }

    /// Stores details that belong to cache generation `expected_generation`, under
    /// the asset's current tag token. Nothing is stored after a clear since then,
    /// while a write holds the asset, or for a deleted asset.
    pub fn put(
        &mut self,
        tags: &mut AssetTagState,
        details: Arc<AssetDetails>,
        expected_generation: u64,
    ) {
        if self.disposed || expected_generation != self.generation {
            return;
        }
        let token = tags.capture_generation(details.summary.id);
        if !tags.is_current(&token) {
            return;
        }
        self.store(token, details);
        while self.entries.len() > MAX_CACHED_DETAILS {
            let Some((_, oldest)) = self.order.pop_first() else {
                break;
            };
            self.entries.remove(&oldest);
        }
    }

    pub fn remove(&mut self, asset_id: i64) -> bool {
        self.forget(asset_id).is_some()
    }

    /// Empties the cache and starts a new generation. Returns the readers of every
    /// read that is on its way: they are answered with nothing now, and the late
    /// replies store nothing.
    pub fn clear(&mut self) -> Vec<Ticket> {
        self.generation += 1;
        self.entries.clear();
        self.order.clear();
        self.pending.clear();
        self.flights
            .values_mut()
            .flat_map(|flight| std::mem::take(&mut flight.waiters))
            .collect()
    }

    /// The owner is going away: [`clear`](Self::clear), and nothing is stored or
    /// requested any more.
    pub fn dispose(&mut self) -> Vec<Ticket> {
        self.disposed = true;
        self.clear()
    }

    /// Decides how the read of `ticket` for `asset_id` is served. A new backend
    /// read pins the asset's tag record until its reply came.
    pub fn begin_load(
        &mut self,
        tags: &mut AssetTagState,
        tokens: &mut Tokens,
        ticket: Ticket,
        asset_id: i64,
    ) -> LoadStart {
        if self.disposed {
            return LoadStart::Disposed;
        }
        if let Some(cached) = self.get(tags, asset_id) {
            return LoadStart::Cached(cached);
        }
        if let Some(flight) = self
            .pending
            .get(&asset_id)
            .and_then(|token| self.flights.get_mut(token))
        {
            flight.waiters.push(ticket);
            return LoadStart::Joined;
        }
        tags.pin(asset_id);
        let tag_token = tags.capture_generation(asset_id);
        let token = tokens.issue();
        self.pending.insert(asset_id, token);
        self.flights.insert(
            token,
            Flight {
                asset_id,
                generation: self.generation,
                tag_token,
                waiters: vec![ticket],
            },
        );
        LoadStart::Request(token)
    }

    /// Applies the backend's reply to the read `token`. Returns the readers still
    /// waiting for it and their answer; `None` for a reply nobody asked for. The
    /// details are published and stored only when the read is still current: same
    /// cache generation, and the tag state accepts its tags.
    pub fn finish_load(
        &mut self,
        tags: &mut AssetTagState,
        token: RequestToken,
        result: BackendResult<Option<AssetDetails>>,
    ) -> Option<(Vec<Ticket>, DetailsResult)> {
        let flight = self.flights.remove(&token)?;
        let asset_id = flight.asset_id;
        let answer = match result {
            Err(error) => Err(error),
            Ok(None) => Ok(None),
            Ok(Some(mut details)) => {
                if self.disposed
                    || flight.generation != self.generation
                    || !tags.publish_details(asset_id, details.tags.clone(), &flight.tag_token)
                {
                    Ok(None)
                } else {
                    if let Some(canonical) = tags.get(asset_id) {
                        details.tags = canonical.tags.to_vec();
                    }
                    let details = Arc::new(details);
                    self.put(tags, Arc::clone(&details), flight.generation);
                    Ok(Some(details))
                }
            }
        };
        tags.unpin(asset_id);
        if self.pending.get(&asset_id) == Some(&token) {
            self.pending.remove(&asset_id);
        }
        Some((flight.waiters, answer))
    }
}
