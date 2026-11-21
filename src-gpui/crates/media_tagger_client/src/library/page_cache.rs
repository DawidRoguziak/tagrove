//! The sparse page cache of a query session (the three coordinated structures of
//! `createAssetQuery`: `pages`, `assetsById` and `lru`, plus the refresh fallback).
//!
//! Rows are addressed by their global index in the session. A page is the run of
//! `page_size` indexes that starts at a multiple of `page_size` (its offset). The cache
//! keeps at most [`MAX_CACHED_PAGES`] pages whatever the size of the result, and stores
//! every `AssetSummary` exactly as the backend returned it.
//!
//! - **Positions.** A page remembers the IDs at its positions; the summaries live in one
//!   map by ID. A patch that removes an asset leaves a hole at its position instead of
//!   shifting the rows after it, until the next query replaces the snapshot.
//! - **Eviction.** A merge beyond the bound evicts the least recently accessed page that
//!   the latest demanded range does not protect. The range protects its first
//!   [`MAX_CACHED_PAGES`] pages; when everything left is protected the bound still wins.
//! - **Fallback.** When a refresh replaces the cache, the other cached pages of the
//!   latest demanded range stay readable through [`PageCache::asset_at`] only, until
//!   their offset loads again, leaves the range, or lies beyond the new total.
//! - **Version.** Every change of what a reader can see bumps [`PageCache::version`].
//!
//! The cache is not the result: [`PageCache::assets`] lists cached rows only and must
//! never be indexed as the global result list.

use std::collections::{BTreeMap, HashMap};

use media_tagger_core::models::AssetSummary;

/// Most pages the cache keeps (`MAX_CACHED_PAGES`).
pub const MAX_CACHED_PAGES: usize = 24;

/// The aligned page offsets `first..=last`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageSpan {
    pub first: usize,
    pub last: usize,
}

impl PageSpan {
    pub fn contains(self, offset: usize) -> bool {
        (self.first..=self.last).contains(&offset)
    }

    /// The page offsets of the span, ascending.
    pub fn offsets(self, page_size: usize) -> impl Iterator<Item = usize> {
        (self.first..=self.last).step_by(page_size.max(1))
    }
}

/// How a page enters the cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Merge {
    /// The first page of a new query: everything cached before it goes, and the other
    /// pages of the demanded range become fallback rows.
    Replace,
    /// One more page of the current query.
    Page,
}

/// A change to the summary fields a metadata write can touch. Unset fields stay.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SummaryPatch {
    pub is_favorite: Option<bool>,
    pub media_group_key: Option<Option<String>>,
    pub media_group_order: Option<Option<f64>>,
}

impl SummaryPatch {
    pub fn apply(&self, row: &mut AssetSummary) {
        if let Some(is_favorite) = self.is_favorite {
            row.is_favorite = is_favorite;
        }
        if let Some(key) = &self.media_group_key {
            row.media_group_key = key.clone();
        }
        if let Some(order) = self.media_group_order {
            row.media_group_order = order;
        }
    }
}

/// A local change to cached rows after the backend accepted a write (`patchAssets`).
#[derive(Debug, Clone, PartialEq)]
pub enum AssetPatch {
    /// Canonical field values for cached assets, by ID. IDs that are not cached are
    /// skipped.
    Update(Vec<(i64, SummaryPatch)>),
    /// Assets that are gone. Their positions become holes.
    Remove(Vec<i64>),
}

#[derive(Debug)]
pub struct PageCache {
    page_size: usize,
    /// The IDs at the positions of each loaded page, by page offset.
    pages: BTreeMap<usize, Vec<i64>>,
    assets_by_id: HashMap<i64, AssetSummary>,
    /// Eviction order as of the last merge, most recently accessed first.
    lru: Vec<usize>,
    /// Pages read since then, most recent first; at most [`MAX_CACHED_PAGES`].
    access_order: Vec<usize>,
    /// Rows of the previous query's pages inside the demanded range. Render only.
    fallback: HashMap<usize, Vec<AssetSummary>>,
    /// The pages eviction spares: the first [`MAX_CACHED_PAGES`] of the demanded range.
    active: Option<PageSpan>,
    /// The pages of the latest demanded range. Unlike `active` it survives a refresh, so
    /// the new query knows which old rows may stay on screen.
    viewport: Option<PageSpan>,
    version: u64,
}

impl PageCache {
    /// A cache of pages of `page_size` rows (at least one).
    pub fn new(page_size: usize) -> Self {
        Self {
            page_size: page_size.max(1),
            pages: BTreeMap::new(),
            assets_by_id: HashMap::new(),
            lru: Vec::new(),
            access_order: Vec::new(),
            fallback: HashMap::new(),
            active: None,
            viewport: None,
            version: 0,
        }
    }

    pub fn page_size(&self) -> usize {
        self.page_size
    }

    /// The offset of the page that holds global index `index`.
    pub fn page_offset(&self, index: usize) -> usize {
        index / self.page_size * self.page_size
    }

    /// Counts every change a reader can see.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// Cached pages of the current query.
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    /// Cached rows of the current query.
    pub fn len(&self) -> usize {
        self.assets_by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.assets_by_id.is_empty()
    }

    pub fn has_page(&self, offset: usize) -> bool {
        self.pages.contains_key(&offset)
    }

    /// Pages that hold fallback rows of the previous query.
    pub fn fallback_page_count(&self) -> usize {
        self.fallback.len()
    }

    /// The pages of the latest demanded range.
    pub fn viewport(&self) -> Option<PageSpan> {
        self.viewport
    }

    /// Whether eviction spares the page at `offset`.
    pub fn is_active(&self, offset: usize) -> bool {
        self.active.is_some_and(|span| span.contains(offset))
    }

    /// The row of the current query at `index`; never a fallback row.
    pub fn cached_at(&self, index: usize) -> Option<&AssetSummary> {
        let offset = self.page_offset(index);
        let id = self.pages.get(&offset)?.get(index - offset)?;
        self.assets_by_id.get(id)
    }

    /// What a tile at `index` shows: the row of the current query, or, while its page
    /// has not loaded since the last refresh, the previous query's row at that
    /// position. Reading promotes nothing.
    pub fn asset_at(&self, index: usize) -> Option<&AssetSummary> {
        let offset = self.page_offset(index);
        if self.pages.contains_key(&offset) {
            return self.cached_at(index);
        }
        self.fallback.get(&offset)?.get(index - offset)
    }

    /// The global index of a cached asset of the current query.
    pub fn index_of(&self, asset_id: i64) -> Option<usize> {
        if !self.assets_by_id.contains_key(&asset_id) {
            return None;
        }
        self.pages.iter().find_map(|(offset, ids)| {
            ids.iter()
                .position(|id| *id == asset_id)
                .map(|local| offset + local)
        })
    }

    /// The cached rows of the current query in index order. Cached rows only: this is
    /// not the result list and has no relation to `total`.
    pub fn assets(&self) -> impl Iterator<Item = &AssetSummary> {
        self.pages
            .values()
            .flat_map(|ids| ids.iter().filter_map(|id| self.assets_by_id.get(id)))
    }

    /// Records a read of the page at `offset` for the next eviction (`touchPage`).
    pub fn touch(&mut self, offset: usize) {
        self.access_order.retain(|page| *page != offset);
        self.access_order.insert(0, offset);
        self.access_order.truncate(MAX_CACHED_PAGES);
    }

    /// Stores the page at `offset` and evicts down to the bound. `total` is the result
    /// count of the query the page belongs to.
    pub fn merge(&mut self, offset: usize, items: Vec<AssetSummary>, mode: Merge, total: usize) {
        self.touch(offset);
        match mode {
            Merge::Replace => {
                self.fallback.clear();
                if let Some(viewport) = self.viewport {
                    for (&page, ids) in &self.pages {
                        // Keep the old viewport visible until each of its pages
                        // reloads under the new query.
                        if page == offset
                            || page >= total
                            || !viewport.contains(page)
                            || !ids.iter().all(|id| self.assets_by_id.contains_key(id))
                        {
                            continue;
                        }
                        let rows = ids
                            .iter()
                            .filter_map(|id| self.assets_by_id.get(id).cloned())
                            .collect();
                        self.fallback.insert(page, rows);
                    }
                }
                self.pages.clear();
                self.assets_by_id.clear();
                self.lru.clear();
            }
            Merge::Page => {
                self.fallback.remove(&offset);
                if let Some(previous) = self.pages.remove(&offset) {
                    for id in previous {
                        self.assets_by_id.remove(&id);
                    }
                }
            }
        }
        self.pages
            .insert(offset, items.iter().map(|item| item.id).collect());
        for item in items {
            self.assets_by_id.insert(item.id, item);
        }

        let previous = std::mem::take(&mut self.lru);
        for page in self
            .access_order
            .iter()
            .copied()
            .chain([offset])
            .chain(previous)
        {
            if self.pages.contains_key(&page) && !self.lru.contains(&page) {
                self.lru.push(page);
            }
        }
        while self.lru.len() > MAX_CACHED_PAGES {
            // The least recently accessed page the demanded range does not protect;
            // when all are protected the exact bound still wins.
            let candidate = self
                .lru
                .iter()
                .rposition(|page| !self.is_active(*page))
                .unwrap_or(self.lru.len() - 1);
            let evicted = self.lru.remove(candidate);
            if let Some(ids) = self.pages.remove(&evicted) {
                for id in ids {
                    self.assets_by_id.remove(&id);
                }
            }
        }
        self.version += 1;
    }

    /// Makes the inclusive index range `start..=end` the demanded range of a result of
    /// `total` rows (the bookkeeping of `ensureRange`): its first
    /// [`MAX_CACHED_PAGES`] pages are protected from eviction, and fallback rows
    /// outside it are dropped. Returns the pages of the range, for the caller to load.
    pub fn set_demand(&mut self, start: usize, end: usize, total: usize) -> Option<PageSpan> {
        self.active = None;
        self.viewport = None;
        let span = (total > 0 && start <= end)
            .then(|| PageSpan {
                first: self.page_offset(start),
                last: self.page_offset(end.min(total - 1)),
            })
            .filter(|span| span.first <= span.last);
        self.viewport = span;
        self.active = span.map(|span| PageSpan {
            first: span.first,
            last: span.last.min(
                span.first
                    .saturating_add((MAX_CACHED_PAGES - 1).saturating_mul(self.page_size)),
            ),
        });
        let before = self.fallback.len();
        self.fallback
            .retain(|page, _| span.is_some_and(|span| span.contains(*page)));
        if self.fallback.len() != before {
            self.version += 1;
        }
        span
    }

    /// Forgets which pages are protected and which were read, as a starting refresh
    /// does. The demanded range itself stays for the fallback of the next replace.
    pub fn release_active(&mut self) {
        self.active = None;
        self.access_order.clear();
    }

    /// Applies a local change to cached rows. Positions never shift.
    pub fn patch(&mut self, patch: &AssetPatch) {
        match patch {
            AssetPatch::Update(changes) => {
                for (id, change) in changes {
                    if let Some(row) = self.assets_by_id.get_mut(id) {
                        change.apply(row);
                    }
                }
            }
            AssetPatch::Remove(ids) => {
                for id in ids {
                    self.assets_by_id.remove(id);
                }
            }
        }
        self.version += 1;
    }

    /// Drops everything: rows, fallback, demand and access history.
    pub fn reset(&mut self) {
        self.pages.clear();
        self.assets_by_id.clear();
        self.lru.clear();
        self.access_order.clear();
        self.fallback.clear();
        self.active = None;
        self.viewport = None;
        self.version += 1;
    }
}

#[cfg(test)]
mod tests {
    use media_tagger_core::models::AssetSummary;

    use super::{AssetPatch, Merge, PageCache, PageSpan, SummaryPatch, MAX_CACHED_PAGES};

    fn row(id: i64) -> AssetSummary {
        AssetSummary {
            id,
            file_name: format!("photo-{id}.jpg"),
            preview_path: None,
            kind: "image".into(),
            modified_at: 100 - id,
            width: Some(10),
            height: Some(10),
            duration_ms: None,
            thumb_version: "0123456789abcdef".into(),
            is_favorite: false,
            media_group_key: None,
            media_group_order: None,
        }
    }

    /// The page at `offset` of a result whose row at index `i` has ID `i + 1`.
    fn page(offset: usize, size: usize) -> Vec<AssetSummary> {
        (offset..offset + size)
            .map(|index| row(index as i64 + 1))
            .collect()
    }

    fn ids(cache: &PageCache) -> Vec<i64> {
        cache.assets().map(|asset| asset.id).collect()
    }

    #[test]
    fn rows_are_read_by_global_index_and_listed_in_index_order() {
        let mut cache = PageCache::new(2);
        cache.merge(4, page(4, 2), Merge::Replace, 10);
        cache.merge(0, page(0, 2), Merge::Page, 10);

        assert_eq!(cache.asset_at(0).map(|row| row.id), Some(1));
        assert_eq!(cache.asset_at(5).map(|row| row.id), Some(6));
        assert_eq!(cache.asset_at(2), None, "a hole");
        assert_eq!(cache.index_of(6), Some(5));
        assert_eq!(cache.index_of(3), None);
        assert_eq!(ids(&cache), [1, 2, 5, 6]);
        assert_eq!((cache.len(), cache.page_count()), (4, 2));
    }

    #[test]
    fn summaries_are_stored_exactly_as_they_came() {
        let mut cache = PageCache::new(2);
        let video = AssetSummary {
            kind: "video".into(),
            preview_path: Some("/synthetic/clip.mp4".into()),
            duration_ms: Some(10_000),
            ..row(7)
        };
        cache.merge(0, vec![video.clone()], Merge::Replace, 1);

        assert_eq!(cache.asset_at(0), Some(&video));
    }

    #[test]
    fn the_page_bound_holds_and_the_least_recently_read_page_goes_first() {
        let mut cache = PageCache::new(1);
        for offset in 0..MAX_CACHED_PAGES {
            cache.merge(offset, page(offset, 1), Merge::Page, 100);
        }
        assert_eq!(cache.page_count(), MAX_CACHED_PAGES);

        // Page 0 is the oldest; reading it promotes it, so page 1 goes instead.
        cache.touch(0);
        cache.merge(50, page(50, 1), Merge::Page, 100);

        assert_eq!(cache.page_count(), MAX_CACHED_PAGES);
        assert!(cache.has_page(0) && cache.has_page(50));
        assert!(!cache.has_page(1));
        assert_eq!(cache.len(), MAX_CACHED_PAGES);
    }

    #[test]
    fn the_demanded_range_protects_its_pages_but_never_more_than_the_bound() {
        let mut cache = PageCache::new(1);
        cache.merge(0, page(0, 1), Merge::Replace, 100);
        assert_eq!(
            cache.set_demand(0, 0, 100),
            Some(PageSpan { first: 0, last: 0 })
        );
        for offset in 1..60 {
            cache.merge(offset, page(offset, 1), Merge::Page, 100);
            assert!(cache.page_count() <= MAX_CACHED_PAGES);
        }
        assert!(cache.has_page(0), "the demanded page survives 59 merges");

        // A range of 30 pages protects its first 24; the bound still holds.
        let span = cache.set_demand(0, 29, 100).unwrap();
        assert_eq!((span.first, span.last), (0, 29));
        assert!(cache.is_active(23) && !cache.is_active(24));
        for offset in span.offsets(1) {
            cache.merge(offset, page(offset, 1), Merge::Page, 100);
        }
        assert_eq!(cache.page_count(), MAX_CACHED_PAGES);
    }

    #[test]
    fn a_replace_keeps_the_other_pages_of_the_demanded_range_as_fallback_only() {
        let mut cache = PageCache::new(2);
        cache.merge(0, page(0, 2), Merge::Replace, 6);
        cache.set_demand(2, 3, 6);
        cache.merge(2, page(2, 2), Merge::Page, 6);
        cache.merge(4, page(4, 2), Merge::Page, 6);

        cache.merge(0, vec![row(11), row(12)], Merge::Replace, 6);

        assert_eq!(cache.asset_at(0).map(|row| row.id), Some(11));
        assert_eq!(cache.asset_at(2).map(|row| row.id), Some(3), "fallback");
        assert_eq!(cache.asset_at(4), None, "outside the demanded range");
        assert_eq!(cache.cached_at(2), None);
        assert_eq!(cache.index_of(3), None);
        assert_eq!(ids(&cache), [11, 12]);

        // The page's own load replaces its fallback.
        cache.merge(2, vec![row(13), row(14)], Merge::Page, 6);
        assert_eq!(cache.asset_at(2).map(|row| row.id), Some(13));
        assert_eq!(cache.fallback_page_count(), 0);
    }

    #[test]
    fn fallback_rows_go_when_they_leave_the_range_or_the_new_total() {
        let mut cache = PageCache::new(2);
        cache.merge(0, page(0, 2), Merge::Replace, 6);
        cache.set_demand(2, 5, 6);
        cache.merge(2, page(2, 2), Merge::Page, 6);
        cache.merge(4, page(4, 2), Merge::Page, 6);

        // The new result has four rows: the page at 4 is beyond it.
        cache.merge(0, vec![row(11), row(12)], Merge::Replace, 4);
        assert_eq!(cache.asset_at(2).map(|row| row.id), Some(3));
        assert_eq!(cache.asset_at(4), None);

        let version = cache.version();
        cache.set_demand(0, 1, 4);
        assert_eq!(cache.asset_at(2), None);
        assert!(cache.version() > version);
    }

    #[test]
    fn a_page_with_a_removed_row_is_not_fallback() {
        let mut cache = PageCache::new(2);
        cache.merge(0, page(0, 2), Merge::Replace, 4);
        cache.set_demand(2, 3, 4);
        cache.merge(2, page(2, 2), Merge::Page, 4);
        cache.patch(&AssetPatch::Remove(vec![3]));

        cache.merge(0, vec![row(11)], Merge::Replace, 4);

        assert_eq!(cache.asset_at(3), None);
    }

    #[test]
    fn a_patch_changes_rows_in_place_and_a_removal_leaves_a_hole() {
        let mut cache = PageCache::new(2);
        cache.merge(0, page(0, 2), Merge::Replace, 2);
        let version = cache.version();

        cache.patch(&AssetPatch::Update(vec![
            (
                2,
                SummaryPatch {
                    is_favorite: Some(true),
                    media_group_key: Some(Some("trip".into())),
                    media_group_order: Some(Some(2.0)),
                },
            ),
            (99, SummaryPatch::default()),
        ]));
        cache.patch(&AssetPatch::Remove(vec![1]));

        assert_eq!(cache.asset_at(0), None);
        assert_eq!(cache.index_of(1), None);
        let second = cache.asset_at(1).unwrap();
        assert!(second.is_favorite);
        assert_eq!(second.media_group_key.as_deref(), Some("trip"));
        assert_eq!(second.media_group_order, Some(2.0));
        assert_eq!(cache.index_of(2), Some(1), "positions do not shift");
        assert_eq!(cache.version(), version + 2);
    }

    #[test]
    fn a_range_outside_the_result_demands_nothing() {
        let mut cache = PageCache::new(2);
        assert_eq!(cache.set_demand(0, 5, 0), None);
        assert_eq!(cache.set_demand(8, 9, 4), None);
        assert_eq!(cache.set_demand(3, 2, 4), None);
        assert_eq!(
            cache.set_demand(1, 99, 4),
            Some(PageSpan { first: 0, last: 2 })
        );
    }

    #[test]
    fn nothing_grows_with_the_size_of_the_result() {
        let total = 1_000_000;
        let mut cache = PageCache::new(128);
        cache.merge(0, page(0, 128), Merge::Replace, total);
        let mut offset = 0;
        while offset < total {
            cache.set_demand(offset, offset + 127, total);
            cache.merge(offset, page(offset, 128), Merge::Page, total);
            // Stride through the result instead of visiting all 7,813 pages.
            offset += 128 * 97;
        }

        assert_eq!(cache.page_count(), MAX_CACHED_PAGES);
        assert_eq!(cache.len(), MAX_CACHED_PAGES * 128);
        assert!(cache.lru.len() <= MAX_CACHED_PAGES);
        assert!(cache.access_order.len() <= MAX_CACHED_PAGES);
        assert_eq!(cache.fallback_page_count(), 0);
    }

    #[test]
    fn reset_drops_rows_fallback_and_demand() {
        let mut cache = PageCache::new(2);
        cache.merge(0, page(0, 2), Merge::Replace, 4);
        cache.set_demand(0, 3, 4);
        let version = cache.version();

        cache.reset();

        assert!(cache.is_empty());
        assert_eq!(cache.viewport(), None);
        assert!(!cache.is_active(0));
        assert!(cache.version() > version);
    }
}
