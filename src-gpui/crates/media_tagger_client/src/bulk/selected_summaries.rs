//! `createSelectedSummaries.ts`: the summaries of the selected assets.
//!
//! Selected records survive the eviction of their gallery page, and never outlive
//! their selection. A record comes from the query's cache when the selection changes,
//! else from `get_asset_summaries_by_ids`, asked for in batches of [`SUMMARY_BATCH`]
//! with at most [`SUMMARY_READS`] reads in flight. A write that was published while a
//! read was on its way is laid over the read's older answer; an asset the backend did
//! not return, or a failed read, marks the selection as failed until
//! [`SelectedSummaries::restart`] runs again (Retry).
//!
//! The structure does no I/O: [`SelectedSummaries::restart`] and
//! [`SelectedSummaries::loaded`] return the reads to start.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
};

use media_tagger_core::models::AssetSummary;

use crate::{
    backend::{BackendResult, MAX_SUMMARY_BATCH},
    library::{MetadataCommit, SummaryPatch},
};

/// IDs per read: the most one `get_asset_summaries_by_ids` call accepts.
pub const SUMMARY_BATCH: usize = MAX_SUMMARY_BATCH;
/// Reads in flight at most.
pub const SUMMARY_READS: usize = 4;

/// One `get_asset_summaries_by_ids` call to make. The answer comes back with the same
/// `generation`, `epoch` and `ids`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryLoad {
    /// The selection the read belongs to; a later selection drops its answer.
    pub generation: u64,
    /// The identity epoch when the read started.
    pub epoch: u64,
    pub ids: Vec<i64>,
}

fn merge_patch(into: &mut SummaryPatch, patch: &SummaryPatch) {
    if patch.is_favorite.is_some() {
        into.is_favorite = patch.is_favorite;
    }
    if patch.media_group_key.is_some() {
        into.media_group_key = patch.media_group_key.clone();
    }
    if patch.media_group_order.is_some() {
        into.media_group_order = patch.media_group_order;
    }
}

fn is_empty_patch(patch: &SummaryPatch) -> bool {
    *patch == SummaryPatch::default()
}

#[derive(Debug, Default)]
pub struct SelectedSummaries {
    /// The selected IDs, in the order they were selected.
    ids: Arc<[i64]>,
    members: HashSet<i64>,
    records: HashMap<i64, AssetSummary>,
    /// How many of `records` are no favorite, kept in step with every change of
    /// `records` so that the dock's heart reads it without walking the selection.
    not_favorite: usize,
    /// What writes published for selected assets since they were selected.
    patches: HashMap<i64, SummaryPatch>,
    deleted: HashSet<i64>,
    failures: HashSet<i64>,
    generation: u64,
    /// The identity epoch the records belong to.
    identity: u64,
    running: usize,
    queue: VecDeque<Vec<i64>>,
    disposed: bool,
}

impl SelectedSummaries {
    /// Nothing selected, in identity epoch `identity`.
    pub fn new(identity: u64) -> Self {
        Self {
            identity,
            ..Self::default()
        }
    }

    /// The selection, the identity epoch or the retry counter changed: keeps the
    /// records of IDs that stay selected (none after an identity change), takes what
    /// `cached` (the query's rows) holds of the others and asks for the rest.
    pub fn restart<'a>(
        &mut self,
        ids: Arc<[i64]>,
        epoch: u64,
        cached: impl IntoIterator<Item = &'a AssetSummary>,
    ) -> Vec<SummaryLoad> {
        if self.disposed {
            return Vec::new();
        }
        self.generation += 1;
        let members: HashSet<i64> = ids.iter().copied().collect();
        if self.identity == epoch {
            self.records.retain(|id, _| members.contains(id));
            self.patches.retain(|id, _| members.contains(id));
            self.deleted.retain(|id| members.contains(id));
        } else {
            self.records.clear();
            self.patches.clear();
            self.deleted.clear();
        }
        self.identity = epoch;
        self.ids = ids;
        self.members = members;
        if self.records.len() < self.members.len() {
            for item in cached {
                if self.members.contains(&item.id)
                    && !self.records.contains_key(&item.id)
                    && !self.deleted.contains(&item.id)
                {
                    let record = self.patched(item.clone());
                    self.records.insert(item.id, record);
                }
            }
        }
        // The one walk over the records per selection change.
        self.not_favorite = self
            .records
            .values()
            .filter(|record| !record.is_favorite)
            .count();
        self.failures.clear();
        let missing: Vec<i64> = self
            .ids
            .iter()
            .copied()
            .filter(|id| !self.records.contains_key(id))
            .collect();
        self.queue = missing.chunks(SUMMARY_BATCH).map(<[i64]>::to_vec).collect();
        self.pump(epoch)
    }

    fn patched(&self, mut item: AssetSummary) -> AssetSummary {
        if let Some(patch) = self.patches.get(&item.id) {
            patch.apply(&mut item);
        }
        item
    }

    /// Keeps `record`, in place of an older one of the same asset.
    fn store(&mut self, record: AssetSummary) {
        self.not_favorite += usize::from(!record.is_favorite);
        if let Some(replaced) = self.records.insert(record.id, record) {
            self.not_favorite -= usize::from(!replaced.is_favorite);
        }
    }

    /// Starts queued reads while fewer than [`SUMMARY_READS`] are out. A read whose
    /// selection was replaced still holds its place until it answers.
    fn pump(&mut self, epoch: u64) -> Vec<SummaryLoad> {
        let mut loads = Vec::new();
        while !self.disposed && self.running < SUMMARY_READS {
            let Some(ids) = self.queue.pop_front() else {
                break;
            };
            self.running += 1;
            loads.push(SummaryLoad {
                generation: self.generation,
                epoch,
                ids,
            });
        }
        loads
    }

    /// The answer to `load`. `epoch` is the identity epoch now: an answer of another
    /// selection or another identity is dropped. Returns the reads to start next.
    pub fn loaded(
        &mut self,
        load: SummaryLoad,
        result: BackendResult<Vec<AssetSummary>>,
        epoch: u64,
    ) -> Vec<SummaryLoad> {
        self.running = self.running.saturating_sub(1);
        if !self.disposed && load.generation == self.generation && load.epoch == epoch {
            match result {
                Ok(items) => {
                    let found: HashSet<i64> = items.iter().map(|item| item.id).collect();
                    for item in items {
                        if self.members.contains(&item.id) && !self.deleted.contains(&item.id) {
                            let record = self.patched(item);
                            self.store(record);
                        }
                    }
                    self.failures
                        .extend(load.ids.iter().filter(|id| !found.contains(id)));
                }
                Err(_) => self.failures.extend(load.ids.iter()),
            }
        }
        self.pump(epoch)
    }

    /// A write published canonical values, or deleted assets (`subscribe` of
    /// `createMutations`). Only selected assets are touched; tags are no summary field.
    pub fn commit(&mut self, commit: &MetadataCommit) {
        if self.disposed {
            return;
        }
        match commit {
            MetadataCommit::Deleted(ids) => {
                for id in ids.iter().filter(|id| self.members.contains(id)) {
                    if let Some(removed) = self.records.remove(id) {
                        self.not_favorite -= usize::from(!removed.is_favorite);
                    }
                    self.deleted.insert(*id);
                    self.failures.insert(*id);
                }
            }
            MetadataCommit::Updated(patches) => {
                for (id, patch) in patches {
                    if !self.members.contains(id) || is_empty_patch(&patch.summary) {
                        continue;
                    }
                    merge_patch(self.patches.entry(*id).or_default(), &patch.summary);
                    if let Some(record) = self.records.get_mut(id) {
                        let was_favorite = record.is_favorite;
                        patch.summary.apply(record);
                        match (was_favorite, record.is_favorite) {
                            (true, false) => self.not_favorite += 1,
                            (false, true) => self.not_favorite -= 1,
                            _ => {}
                        }
                    }
                }
            }
        }
    }

    /// The owner is going away: nothing is asked for or stored again.
    pub fn dispose(&mut self) {
        self.disposed = true;
        self.generation += 1;
        self.queue.clear();
    }

    /// The selected IDs the records are kept for.
    pub fn ids(&self) -> &Arc<[i64]> {
        &self.ids
    }

    /// The records that are loaded, in selection order.
    pub fn selected(&self) -> Vec<&AssetSummary> {
        self.ids
            .iter()
            .filter_map(|id| self.records.get(id))
            .collect()
    }

    /// The record of one selected asset, when it is loaded.
    pub fn record(&self, asset_id: i64) -> Option<&AssetSummary> {
        self.records.get(&asset_id)
    }

    /// How many records are loaded.
    pub fn loaded_count(&self) -> usize {
        self.records.len()
    }

    /// Every loaded record is a favorite (also when none is loaded). Constant time:
    /// the dock asks on every frame, for selections of thousands of assets.
    pub fn all_favorites(&self) -> bool {
        self.not_favorite == 0
    }

    /// Every selected asset has its record.
    pub fn ready(&self) -> bool {
        self.records.len() == self.members.len()
    }

    /// A read failed, an asset was not returned, or one was deleted.
    pub fn failed(&self) -> bool {
        !self.failures.is_empty()
    }

    /// Reads without an answer, of this selection and of earlier ones.
    pub fn running(&self) -> usize {
        self.running
    }

    /// Batches waiting for a free read.
    pub fn queued(&self) -> usize {
        self.queue.len()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use media_tagger_core::models::AssetSummary;

    use super::{SelectedSummaries, SUMMARY_BATCH, SUMMARY_READS};
    use crate::{
        backend::BackendError,
        library::{MetadataCommit, MetadataPatch, SummaryPatch},
    };

    fn asset(id: i64) -> AssetSummary {
        AssetSummary {
            id,
            file_name: format!("{id}.png"),
            preview_path: None,
            kind: "image".into(),
            modified_at: 0,
            width: None,
            height: None,
            duration_ms: None,
            thumb_version: format!("{id:016x}"),
            is_favorite: false,
            media_group_key: None,
            media_group_order: None,
        }
    }

    fn ids(ids: impl IntoIterator<Item = i64>) -> Arc<[i64]> {
        ids.into_iter().collect()
    }

    fn favorite(id: i64) -> MetadataCommit {
        MetadataCommit::Updated(vec![(
            id,
            MetadataPatch {
                tags: None,
                summary: SummaryPatch {
                    is_favorite: Some(true),
                    ..SummaryPatch::default()
                },
            },
        )])
    }

    #[test]
    fn cached_rows_need_no_read_and_the_rest_is_read_in_bounded_batches() {
        let mut summaries = SelectedSummaries::new(0);
        let cached = [asset(1), asset(2)];
        let selected = ids(1..=(2 + 5 * SUMMARY_BATCH as i64 + 1));
        let loads = summaries.restart(selected, 0, &cached);

        assert_eq!(loads.len(), SUMMARY_READS);
        assert!(loads.iter().all(|load| load.ids.len() == SUMMARY_BATCH));
        assert_eq!(loads[0].ids[0], 3);
        assert_eq!(summaries.queued(), 2);
        assert_eq!(summaries.loaded_count(), 2);
        assert!(!summaries.ready() && !summaries.failed());

        let first = loads[0].clone();
        let answer = first.ids.iter().copied().map(asset).collect();
        let next = summaries.loaded(first, Ok(answer), 0);
        assert_eq!(next.len(), 1);
        assert_eq!(summaries.running(), SUMMARY_READS);
        assert_eq!(summaries.loaded_count(), 2 + SUMMARY_BATCH);
    }

    #[test]
    fn omitted_and_failed_records_are_failures_until_the_next_restart() {
        let mut summaries = SelectedSummaries::new(0);
        let loads = summaries.restart(ids([1, 2, 3]), 0, []);
        assert_eq!(loads.len(), 1);
        summaries.loaded(loads[0].clone(), Ok(vec![asset(1), asset(3)]), 0);
        assert!(summaries.failed() && !summaries.ready());
        assert_eq!(
            summaries
                .selected()
                .iter()
                .map(|a| a.id)
                .collect::<Vec<_>>(),
            [1, 3]
        );

        let retry = summaries.restart(ids([1, 2, 3]), 0, []);
        assert!(!summaries.failed());
        assert_eq!(retry[0].ids, [2]);
        summaries.loaded(
            retry[0].clone(),
            Err(BackendError::worker_failed(
                "get_asset_summaries_by_ids",
                "x",
            )),
            0,
        );
        assert!(summaries.failed());
    }

    #[test]
    fn an_answer_of_a_replaced_selection_or_identity_is_dropped() {
        let mut summaries = SelectedSummaries::new(0);
        let old = summaries.restart(ids([1]), 0, []);
        let new = summaries.restart(ids([2]), 0, []);
        summaries.loaded(old[0].clone(), Ok(vec![asset(1)]), 0);
        assert_eq!(summaries.loaded_count(), 0);
        // Another identity epoch by the time the answer comes.
        summaries.loaded(new[0].clone(), Ok(vec![asset(2)]), 1);
        assert_eq!(summaries.loaded_count(), 0);
        assert_eq!(summaries.running(), 0);
    }

    #[test]
    fn a_published_write_is_laid_over_an_older_answer() {
        let mut summaries = SelectedSummaries::new(0);
        let loads = summaries.restart(ids([1, 2]), 0, []);
        summaries.commit(&favorite(1));
        // Not selected: ignored.
        summaries.commit(&favorite(9));
        summaries.loaded(loads[0].clone(), Ok(vec![asset(1), asset(2)]), 0);

        let selected = summaries.selected();
        assert!(selected[0].is_favorite && !selected[1].is_favorite);

        summaries.commit(&favorite(2));
        assert!(summaries.selected()[1].is_favorite);
    }

    /// What `all_favorites` must say: the walk over the records it replaces.
    fn walked(summaries: &SelectedSummaries) -> bool {
        summaries.selected().iter().all(|asset| asset.is_favorite)
    }

    fn set_favorite(ids: impl IntoIterator<Item = i64>, value: bool) -> MetadataCommit {
        MetadataCommit::Updated(
            ids.into_iter()
                .map(|id| {
                    (
                        id,
                        MetadataPatch {
                            tags: None,
                            summary: SummaryPatch {
                                is_favorite: Some(value),
                                ..SummaryPatch::default()
                            },
                        },
                    )
                })
                .collect(),
        )
    }

    #[test]
    fn all_favorites_follows_every_change_of_the_records_without_walking_them() {
        let mut summaries = SelectedSummaries::new(0);
        assert!(summaries.all_favorites(), "nothing loaded");

        // Cached rows: one favorite, one not.
        let mut liked = asset(1);
        liked.is_favorite = true;
        let loads = summaries.restart(ids([1, 2, 3, 4]), 0, &[liked.clone(), asset(2)]);
        assert!(!summaries.all_favorites() && !walked(&summaries));

        // A write for a loaded record, and one laid over a record that is still read.
        summaries.commit(&set_favorite([2, 3], true));
        assert!(summaries.all_favorites() && walked(&summaries));
        summaries.loaded(loads[0].clone(), Ok(vec![asset(3), asset(4)]), 0);
        assert!(!summaries.all_favorites() && !walked(&summaries));
        assert!(summaries.record(3).is_some_and(|asset| asset.is_favorite));

        // The same write twice counts once, in both directions.
        summaries.commit(&set_favorite([4], true));
        summaries.commit(&set_favorite([4], true));
        assert!(summaries.all_favorites() && walked(&summaries));
        summaries.commit(&set_favorite([1, 4], false));
        summaries.commit(&set_favorite([1], false));
        assert!(!summaries.all_favorites() && !walked(&summaries));
        summaries.commit(&set_favorite([1], true));
        assert!(!summaries.all_favorites(), "4 is still no favorite");

        // The record that was no favorite is deleted, or leaves the selection.
        summaries.commit(&MetadataCommit::Deleted(vec![4]));
        assert!(summaries.all_favorites() && walked(&summaries));
        summaries.commit(&set_favorite([2], false));
        assert!(!summaries.all_favorites());
        summaries.restart(ids([1, 3]), 0, []);
        assert!(summaries.all_favorites() && walked(&summaries));
        assert_eq!(summaries.loaded_count(), 2);

        // Another identity starts from the rows it is given.
        summaries.restart(ids([1, 3]), 1, &[liked, asset(3)]);
        assert!(!summaries.all_favorites() && !walked(&summaries));
        summaries.commit(&set_favorite([3], true));
        assert!(summaries.all_favorites() && walked(&summaries));
    }

    #[test]
    fn an_answer_that_repeats_a_record_replaces_it_and_counts_it_once() {
        let mut summaries = SelectedSummaries::new(0);
        let loads = summaries.restart(ids([1, 2]), 0, []);
        let mut liked = asset(1);
        liked.is_favorite = true;
        summaries.loaded(
            loads[0].clone(),
            Ok(vec![asset(1), liked, asset(2), asset(2)]),
            0,
        );
        assert_eq!(summaries.loaded_count(), 2);
        assert!(!summaries.all_favorites() && !walked(&summaries));
        summaries.commit(&set_favorite([2], true));
        assert!(summaries.all_favorites() && walked(&summaries));
    }

    #[test]
    fn a_deleted_asset_loses_its_record_and_fails_the_selection() {
        let mut summaries = SelectedSummaries::new(0);
        summaries.restart(ids([1, 2]), 0, &[asset(1), asset(2)]);
        assert!(summaries.ready());
        summaries.commit(&MetadataCommit::Deleted(vec![2, 7]));
        assert!(summaries.failed() && !summaries.ready());
        // The record does not come back from the cache while the asset stays selected.
        summaries.restart(ids([1, 2]), 0, &[asset(1), asset(2)]);
        assert_eq!(summaries.loaded_count(), 1);
        // Another identity forgets the deletion.
        summaries.restart(ids([1, 2]), 1, &[asset(1), asset(2)]);
        assert!(summaries.ready());
    }
}
