//! The bounded cache of decoded thumbnails.
//!
//! A decoded thumbnail is 390 x 390 pixels of four bytes, about 0.6 MiB, and the
//! renderer keeps a texture of the same size for each one it has drawn. The cache
//! therefore has a byte budget and gives back what it drops, so the adapter can
//! release the texture as well.
//!
//! Rules:
//!
//! - Pictures of tiles in range are pinned ([`PictureCache::pin`]) and never
//!   dropped: evicting one would make its tile ask for it again at once.
//! - Everything else is kept in order of last use and dropped from the least
//!   recently used end as soon as the cache is over its budget. So the cache never
//!   holds more than its budget, or than the pinned pictures when those alone exceed
//!   it: `bytes() <= max(budget(), pinned_bytes())`.
//! - A picture stored with `keep: false` (bytes of a newer source version than the
//!   one asked for, `ThumbnailEvent::Ready { current: false }`) lives only while it
//!   is pinned: the next showing must ask again.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::thumbs::ThumbnailKey;

/// The default budget of decoded pictures: about 440 thumbnails.
pub const PICTURE_CACHE_BUDGET: usize = 256 * 1024 * 1024;

#[derive(Debug)]
struct Entry<V> {
    value: V,
    bytes: usize,
    /// Position in the order of last use.
    tick: u64,
    keep: bool,
}

#[derive(Debug)]
pub struct PictureCache<V> {
    budget: usize,
    bytes: usize,
    tick: u64,
    entries: HashMap<ThumbnailKey, Entry<V>>,
    /// Keys by last use, least recent first.
    order: BTreeMap<u64, ThumbnailKey>,
    pinned: HashSet<ThumbnailKey>,
}

impl<V> PictureCache<V> {
    /// A cache that keeps at most `budget` bytes of pictures that are not pinned.
    pub fn new(budget: usize) -> Self {
        Self {
            budget,
            bytes: 0,
            tick: 0,
            entries: HashMap::new(),
            order: BTreeMap::new(),
            pinned: HashSet::new(),
        }
    }

    pub fn budget(&self) -> usize {
        self.budget
    }

    /// Bytes of every picture held.
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// Bytes of the pinned pictures held.
    pub fn pinned_bytes(&self) -> usize {
        self.pinned
            .iter()
            .filter_map(|key| self.entries.get(key))
            .map(|entry| entry.bytes)
            .sum()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn contains(&self, key: &ThumbnailKey) -> bool {
        self.entries.contains_key(key)
    }

    /// The picture of `key`, without counting as a use.
    pub fn peek(&self, key: &ThumbnailKey) -> Option<&V> {
        self.entries.get(key).map(|entry| &entry.value)
    }

    fn touch(&mut self, key: &ThumbnailKey) {
        if let Some(entry) = self.entries.get_mut(key) {
            self.order.remove(&entry.tick);
            self.tick += 1;
            entry.tick = self.tick;
            self.order.insert(self.tick, key.clone());
        }
    }

    /// The picture of `key`, now the most recently used.
    pub fn get(&mut self, key: &ThumbnailKey) -> Option<&V> {
        self.touch(key);
        self.peek(key)
    }

    fn remove(&mut self, key: &ThumbnailKey) -> Option<V> {
        let entry = self.entries.remove(key)?;
        self.order.remove(&entry.tick);
        self.bytes -= entry.bytes;
        Some(entry.value)
    }

    /// Drops least recently used pictures that are not pinned until the cache is
    /// within its budget.
    fn trim(&mut self, dropped: &mut Vec<V>) {
        if self.bytes <= self.budget {
            return;
        }
        let victims: Vec<ThumbnailKey> = self
            .order
            .values()
            .filter(|key| !self.pinned.contains(*key))
            .cloned()
            .collect();
        for key in victims {
            if self.bytes <= self.budget {
                break;
            }
            dropped.extend(self.remove(&key));
        }
    }

    /// Stores the picture of `key`, which takes `bytes`, as the most recently
    /// used. `keep: false` keeps it only while the key is pinned. Returns the
    /// pictures the cache no longer holds: one replaced under the same key, and
    /// those dropped to stay within the budget (the new one itself when nothing
    /// else can go).
    pub fn insert(&mut self, key: ThumbnailKey, value: V, bytes: usize, keep: bool) -> Vec<V> {
        let mut dropped: Vec<V> = self.remove(&key).into_iter().collect();
        if !keep && !self.pinned.contains(&key) {
            dropped.push(value);
            return dropped;
        }
        self.tick += 1;
        self.order.insert(self.tick, key.clone());
        self.entries.insert(
            key,
            Entry {
                value,
                bytes,
                tick: self.tick,
                keep,
            },
        );
        self.bytes += bytes;
        self.trim(&mut dropped);
        dropped
    }

    /// Replaces the pinned set with `keys` (the tiles in range) and marks their
    /// pictures as used. Returns the pictures dropped: the ones stored with
    /// `keep: false` that are no longer pinned, and whatever is over the budget now.
    pub fn pin(&mut self, keys: impl IntoIterator<Item = ThumbnailKey>) -> Vec<V> {
        let pinned: HashSet<ThumbnailKey> = keys.into_iter().collect();
        let released: Vec<ThumbnailKey> = self
            .pinned
            .iter()
            .filter(|key| !pinned.contains(*key))
            .cloned()
            .collect();
        self.pinned = pinned;
        let mut dropped = Vec::new();
        for key in released {
            if self.entries.get(&key).is_some_and(|entry| !entry.keep) {
                dropped.extend(self.remove(&key));
            }
        }
        let used: Vec<ThumbnailKey> = self
            .pinned
            .iter()
            .filter(|key| self.entries.contains_key(*key))
            .cloned()
            .collect();
        for key in used {
            self.touch(&key);
        }
        self.trim(&mut dropped);
        dropped
    }

    /// Drops every picture and unpins every key.
    pub fn clear(&mut self) -> Vec<V> {
        self.order.clear();
        self.pinned.clear();
        self.bytes = 0;
        self.entries.drain().map(|(_, entry)| entry.value).collect()
    }
}

impl<V> Default for PictureCache<V> {
    fn default() -> Self {
        Self::new(PICTURE_CACHE_BUDGET)
    }
}

#[cfg(test)]
mod tests {
    use super::{PictureCache, PICTURE_CACHE_BUDGET};
    use crate::thumbs::ThumbnailKey;

    const VERSION: &str = "0123456789abcdef";
    /// One decoded thumbnail: 390 x 390 pixels of four bytes.
    const PICTURE: usize = 390 * 390 * 4;

    fn key(asset_id: i64) -> ThumbnailKey {
        ThumbnailKey::new(asset_id, VERSION)
    }

    #[test]
    fn the_cache_never_holds_more_than_its_budget() {
        let budget = 10 * PICTURE;
        let mut cache = PictureCache::new(budget);
        let mut dropped = Vec::new();

        for id in 1..=1_000 {
            dropped.extend(cache.insert(key(id), id, PICTURE, true));
            assert!(cache.bytes() <= budget, "after {id} pictures");
        }

        assert_eq!(cache.len(), 10);
        assert_eq!(cache.bytes(), budget);
        // The least recently used went first, each exactly once.
        assert_eq!(dropped, (1..=990).collect::<Vec<_>>());
        assert!(cache.contains(&key(1_000)) && !cache.contains(&key(990)));
    }

    #[test]
    fn the_default_budget_bounds_a_long_scroll_through_a_large_library() {
        let mut cache = PictureCache::default();
        assert_eq!(cache.budget(), PICTURE_CACHE_BUDGET);

        for id in 1..=5_000 {
            // The 126 tiles of a screen of small tiles are in range.
            cache.pin((id.max(126) - 125..=id).map(key));
            cache.insert(key(id), (), PICTURE, true);
            assert!(cache.bytes() <= PICTURE_CACHE_BUDGET);
        }
        assert_eq!(cache.len(), PICTURE_CACHE_BUDGET / PICTURE);
    }

    #[test]
    fn a_used_picture_outlives_an_unused_one() {
        let mut cache = PictureCache::new(2 * PICTURE);
        cache.insert(key(1), "one", PICTURE, true);
        cache.insert(key(2), "two", PICTURE, true);

        assert_eq!(cache.get(&key(1)), Some(&"one"));
        assert_eq!(cache.insert(key(3), "three", PICTURE, true), ["two"]);
        // Peeking is not a use.
        assert_eq!(cache.peek(&key(1)), Some(&"one"));
        assert_eq!(cache.insert(key(4), "four", PICTURE, true), ["one"]);
    }

    #[test]
    fn pinned_pictures_are_never_dropped_even_over_the_budget() {
        let mut cache = PictureCache::new(2 * PICTURE);
        cache.pin((1..=4).map(key));

        for id in 1..=4 {
            assert!(cache.insert(key(id), id, PICTURE, true).is_empty());
        }
        assert_eq!(cache.bytes(), 4 * PICTURE);
        assert_eq!(cache.pinned_bytes(), 4 * PICTURE);
        // Nothing unpinned fits beside them.
        assert_eq!(cache.insert(key(9), 9, PICTURE, true), [9]);
        assert!(cache.bytes() <= cache.budget().max(cache.pinned_bytes()));

        // Out of range, the oldest go until the budget holds again.
        let mut dropped = cache.pin((3..=4).map(key));
        dropped.sort_unstable();
        assert_eq!(dropped, [1, 2]);
        assert_eq!(cache.bytes(), 2 * PICTURE);
    }

    #[test]
    fn a_picture_of_another_version_lives_only_while_its_tile_is_in_range() {
        let mut cache = PictureCache::new(10 * PICTURE);

        // Not in range: it is not stored at all.
        assert_eq!(cache.insert(key(1), "stale", PICTURE, false), ["stale"]);
        assert!(cache.is_empty());

        cache.pin([key(1)]);
        assert!(cache.insert(key(1), "stale", PICTURE, false).is_empty());
        assert_eq!(cache.get(&key(1)), Some(&"stale"));
        assert_eq!(cache.pin([key(2)]), ["stale"]);
        assert_eq!(cache.bytes(), 0);
    }

    #[test]
    fn replacing_and_clearing_give_every_picture_back() {
        let mut cache = PictureCache::new(10 * PICTURE);
        cache.insert(key(1), "old", PICTURE, true);

        assert_eq!(cache.insert(key(1), "new", PICTURE, true), ["old"]);
        assert_eq!((cache.len(), cache.bytes()), (1, PICTURE));
        cache.insert(key(2), "other", PICTURE, true);
        let mut cleared = cache.clear();
        cleared.sort_unstable();
        assert_eq!(cleared, ["new", "other"]);
        assert_eq!((cache.len(), cache.bytes()), (0, 0));
    }
}
