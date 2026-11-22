//! The behaviour tests of the bulk dock's client side (epic E14), ported from the
//! Solid suite: `src/features/bulk/__tests__/createBulk.test.ts` (the bulk model in
//! the application runtime, against the scripted backend on the deterministic
//! executor), `bulkGroupOrderService.test.ts` (the group services),
//! `GroupOrderModal.test.tsx` (the order board's rows and its scroll after a key
//! move) and the state cases of `BulkDock.test.tsx` (the dock's own state over the
//! real runtime). Each test names the TS case it ports in its doc comment.
//!
//! A pending promise of the Solid suite is a backend call that waits on the
//! `ManualExecutor`: the test decides which call answers first (`run_nth`), and a
//! handler scripted after the call was made decides what it answers.

use std::sync::Arc;

use media_tagger_client::{
    backend::{
        fake::Call, AssetIdRange, BackendError, FakeBackend, MediaGroupUpdate, SelectionMutation,
        SelectionSpec,
    },
    bulk::{
        build_bulk_media_group_assignments, derive_bulk_group_selection_state,
        layout::group::MODAL_MIN_TILE_WIDTH, normalize_group_key, reorder_asset_ids_by_drop,
        unique_ordered_asset_ids, BoardKey, BoardLayout, BulkCommand, BulkEvent,
        BulkGroupSelectionState, BulkInput, Dock, DockEffect, DockTab, GroupSaveResult,
        PartialResult, SelectedSummaries, SummaryLoad, TabKey, TagMode, SUMMARY_BATCH,
    },
    exec::{Executor, ManualExecutor},
    gallery::{Interaction, SelectionInput, SelectionNotice},
    i18n::Language,
    library::{
        LibraryEvent, LibraryFilters, LibraryInput, LibraryReset, MetadataCommit, Mutation,
        QueryInput, Ticket,
    },
    model::Driver,
    prefs::MemoryPrefsStore,
    runtime::{AppRuntime, RuntimeConfig, RuntimeInput, RuntimeRunner},
    theme::ThemeName,
};
use media_tagger_core::models::{
    AssetDetails, AssetQueryPageResult, AssetSummary, AssetTagResult, BulkFavoriteSummary,
    BulkMediaGroupSummary, BulkTagMergeSummary, DeleteAssetSummary, DeleteSourceStatus,
    SelectionMutationResult, SetAssetTagsSummary, StartAssetQueryResult, TagListPage,
    TagQueryImpact,
};

/// Rows of the first page the scripted query answers with.
const FIRST_PAGE: usize = 6;

fn asset(id: i64) -> AssetSummary {
    AssetSummary {
        id,
        file_name: format!("{id}.png"),
        preview_path: None,
        kind: "image".into(),
        modified_at: 1,
        width: Some(10),
        height: Some(10),
        duration_ms: None,
        thumb_version: "0123456789abcdef".into(),
        is_favorite: false,
        media_group_key: None,
        media_group_order: None,
    }
}

fn grouped(id: i64, key: Option<&str>, order: Option<f64>) -> AssetSummary {
    AssetSummary {
        media_group_key: key.map(str::to_owned),
        media_group_order: order,
        ..asset(id)
    }
}

fn list(tags: &[&str]) -> Vec<String> {
    tags.iter().map(|tag| (*tag).to_owned()).collect()
}

fn details(id: i64, tags: &[&str]) -> AssetDetails {
    AssetDetails {
        summary: asset(id),
        path: format!("/synthetic/{id}.png"),
        size_bytes: 10,
        thumb_path: None,
        tags: list(tags),
    }
}

fn saved(asset_id: i64, tags: Vec<String>) -> SetAssetTagsSummary {
    SetAssetTagsSummary {
        vocabulary_changed: false,
        query_impact: TagQueryImpact::None,
        asset_id,
        changed: true,
        tags,
        revision: 2,
    }
}

/// Every asset took the merge; its canonical list is `cat` and the new tags.
fn merged(asset_ids: Vec<i64>, tags: Vec<String>) -> BulkTagMergeSummary {
    BulkTagMergeSummary {
        vocabulary_changed: false,
        query_impact: TagQueryImpact::None,
        processed_assets: asset_ids.len(),
        updated_assets: asset_ids.len(),
        processed_asset_ids: asset_ids.clone(),
        updated_asset_ids: asset_ids.clone(),
        results: asset_ids
            .into_iter()
            .map(|asset_id| AssetTagResult {
                asset_id,
                changed: true,
                tags: [list(&["cat"]), tags.clone()].concat(),
            })
            .collect(),
        revision: 2,
    }
}

fn grouped_all(updates: Vec<MediaGroupUpdate>, key: Option<String>) -> BulkMediaGroupSummary {
    BulkMediaGroupSummary {
        processed_asset_ids: updates.iter().map(|update| update.asset_id).collect(),
        processed_assets: updates.len(),
        updated_assets: updates.len(),
        media_group_key: key,
    }
}

fn ready_selection(processed: usize, favorite: Option<bool>) -> SelectionMutationResult {
    SelectionMutationResult::Ready {
        processed_assets: processed,
        updated_assets: processed,
        revision: 5,
        query_impact: TagQueryImpact::Tags {
            changed_tags: list(&["sunset"]),
            tag_count_changed: true,
        },
        vocabulary_changed: true,
        is_favorite: favorite,
        media_group_key: None,
    }
}

fn update(asset_id: i64, order: Option<f64>) -> MediaGroupUpdate {
    MediaGroupUpdate {
        asset_id,
        media_group_order: order,
    }
}

/// The first page of a query of `total` results whose rows are `rows`.
fn first_page(total: usize, rows: &[AssetSummary]) -> StartAssetQueryResult {
    StartAssetQueryResult::Ready {
        session_id: 1,
        revision: 1,
        total,
        offset: 0,
        items: rows.to_vec(),
    }
}

struct Fx {
    backend: Arc<FakeBackend>,
    executor: Arc<ManualExecutor>,
    driver: Driver<AppRuntime, RuntimeRunner>,
    next_request: u64,
}

impl Fx {
    /// The scripted answers of the Solid suite's `beforeEach` for a query of `total`
    /// results (asset ID = index + 1, each tagged `cat`), the library loaded and
    /// selection mode on.
    fn new(total: usize) -> Self {
        Self::with_rows(
            total,
            (1..=total.min(FIRST_PAGE) as i64).map(asset).collect(),
        )
    }

    /// As [`Fx::new`], with `rows` as the first page.
    fn with_rows(total: usize, rows: Vec<AssetSummary>) -> Self {
        let backend = Arc::new(FakeBackend::new());
        let start_rows = rows.clone();
        backend
            .on_start_asset_query(move |_| Ok(first_page(total, &start_rows)))
            .on_get_asset_query_page(move |session_id, offset, limit| {
                let end = (offset + limit).min(total);
                Ok(AssetQueryPageResult::Ready {
                    session_id,
                    revision: 1,
                    total,
                    offset,
                    items: (offset..end).map(|index| asset(index as i64 + 1)).collect(),
                })
            })
            .on_get_asset_query_id_range(|_, start, end| {
                Ok(AssetIdRange::Ready(
                    (start..=end).map(|index| index as i64 + 1).collect(),
                ))
            })
            .on_get_asset_summaries_by_ids(|ids| Ok(ids.into_iter().map(asset).collect()))
            .on_get_asset_details(|id| Ok(Some(details(id, &["cat"]))))
            .on_list_tags(|_, _, _| {
                Ok(TagListPage {
                    items: list(&["cat", "dog"]),
                    total: 2,
                })
            })
            .on_set_asset_tags(|id, tags| Ok(saved(id, tags)))
            .on_merge_asset_tags_bulk(|ids, tags| Ok(merged(ids, tags)))
            .on_toggle_assets_favorite_bulk(|ids| {
                Ok(BulkFavoriteSummary {
                    processed_asset_ids: ids,
                    is_favorite: true,
                    revision: 2,
                })
            })
            .on_set_assets_media_group_bulk(|updates, key| Ok(grouped_all(updates, key)));
        let executor = Arc::new(ManualExecutor::new());
        let driver = Driver::new(
            AppRuntime::new(RuntimeConfig {
                theme: ThemeName::Dark,
                language: Language::En,
            }),
            RuntimeRunner::new(backend.clone(), Arc::new(MemoryPrefsStore::default())),
            executor.clone() as Arc<dyn Executor>,
        );
        let mut fx = Self {
            backend,
            executor,
            driver,
            next_request: 0,
        };
        fx.send(RuntimeInput::RefreshLibrary);
        fx.settle();
        fx.send(RuntimeInput::Selection(SelectionInput::ToggleMode));
        fx
    }

    fn runtime(&self) -> &AppRuntime {
        self.driver.model()
    }

    fn send(&mut self, input: RuntimeInput) {
        self.driver.handle(input);
    }

    fn bulk(&mut self, command: BulkCommand) {
        self.send(RuntimeInput::bulk(command));
    }

    fn request(&mut self) -> u64 {
        self.next_request += 1;
        self.next_request
    }

    /// `onAddTag`; returns the request to find its answer by.
    fn add(&mut self, tag: &str) -> u64 {
        let request = self.request();
        self.bulk(BulkCommand::AddTag {
            request,
            tag: tag.to_owned(),
        });
        request
    }

    fn remove(&mut self, tag: &str) -> u64 {
        let request = self.request();
        self.bulk(BulkCommand::RemoveTag {
            request,
            tag: tag.to_owned(),
        });
        request
    }

    fn apply(&mut self, order: Option<Vec<i64>>) -> u64 {
        let request = self.request();
        self.bulk(BulkCommand::ApplyGroup { request, order });
        request
    }

    fn draft(&mut self, key: &str) {
        self.bulk(BulkCommand::SetGroupKeyDraft(key.to_owned()));
    }

    /// Runs every waiting call and every reply, including the ones they start.
    fn settle(&mut self) {
        while self.executor.run_all() > 0 || self.driver.pump() > 0 {}
    }

    /// Runs the waiting call at `index` and hands its reply to the runtime; whatever
    /// that starts waits.
    fn answer(&mut self, index: usize) {
        assert!(self.executor.run_nth(index), "a call waits at {index}");
        while self.driver.pump() > 0 {}
    }

    /// Answers the call that was started last.
    fn answer_last(&mut self) {
        let last = self
            .executor
            .pending()
            .checked_sub(1)
            .expect("a call waits");
        self.answer(last);
    }

    fn interact(&mut self, interaction: Interaction) {
        self.send(RuntimeInput::Selection(SelectionInput::Interact(
            interaction,
        )));
    }

    fn click(&mut self, id: i64) {
        self.interact(Interaction::Click {
            asset_id: id,
            asset_index: (id - 1) as usize,
            ctrl_like: true,
            shift: false,
        });
    }

    /// Ctrl-clicks the tiles of `ids` (ID = index + 1) and lets every read end.
    fn select(&mut self, ids: &[i64]) {
        for id in ids {
            self.click(*id);
        }
        self.settle();
    }

    /// `replace` of the Solid suite: the selection becomes `id` alone.
    fn replace(&mut self, id: i64) {
        self.interact(Interaction::Clear);
        self.click(id);
    }

    /// A write of another owner of the library (the lightbox, the tag editor).
    fn external(&mut self, ticket: u64, mutation: Mutation) {
        self.send(RuntimeInput::library(LibraryInput::Mutate {
            ticket: Ticket(ticket),
            mutation,
        }));
    }

    fn events(&mut self) -> Vec<BulkEvent> {
        self.driver.model_mut().take_bulk_events()
    }

    fn tag_saved(&mut self, request: u64) -> Option<bool> {
        self.events().into_iter().find_map(|event| match event {
            BulkEvent::TagSettled { request: r, saved } if r == request => Some(saved),
            _ => None,
        })
    }

    fn group_result(&mut self, request: u64) -> Option<GroupSaveResult> {
        self.events().into_iter().find_map(|event| match event {
            BulkEvent::GroupSettled { request: r, result } if r == request => Some(result),
            _ => None,
        })
    }

    fn tags_of(&self, asset_id: i64) -> Option<Vec<String>> {
        self.runtime()
            .library()
            .asset_tags(asset_id)
            .map(|known| known.tags.to_vec())
    }

    fn single_tags(&self) -> Vec<String> {
        self.runtime().bulk().single_asset_tags().to_vec()
    }

    fn selected_ids(&self) -> Vec<i64> {
        self.runtime().selection().ids().to_vec()
    }

    fn selected_assets(&self) -> Vec<AssetSummary> {
        self.runtime()
            .bulk()
            .selected_assets()
            .into_iter()
            .cloned()
            .collect()
    }

    fn count(&self, method: &str) -> usize {
        self.backend
            .calls()
            .iter()
            .filter(|call| call.method() == method)
            .count()
    }

    fn set_tags_calls(&self) -> Vec<(i64, Vec<String>)> {
        self.backend
            .calls()
            .into_iter()
            .filter_map(|call| match call {
                Call::SetAssetTags { asset_id, tags } => Some((asset_id, tags)),
                _ => None,
            })
            .collect()
    }

    fn merge_calls(&self) -> Vec<(Vec<i64>, Vec<String>)> {
        self.backend
            .calls()
            .into_iter()
            .filter_map(|call| match call {
                Call::MergeAssetTagsBulk { asset_ids, tags } => Some((asset_ids, tags)),
                _ => None,
            })
            .collect()
    }

    fn group_calls(&self) -> Vec<(Vec<MediaGroupUpdate>, Option<String>)> {
        self.backend
            .calls()
            .into_iter()
            .filter_map(|call| match call {
                Call::SetAssetsMediaGroupBulk {
                    updates,
                    media_group_key,
                } => Some((updates, media_group_key)),
                _ => None,
            })
            .collect()
    }

    fn spec_calls(&self) -> Vec<(SelectionSpec, SelectionMutation)> {
        self.backend
            .calls()
            .into_iter()
            .filter_map(|call| match call {
                Call::ApplySelectionMutation {
                    selection,
                    mutation,
                } => Some((selection, mutation)),
                _ => None,
            })
            .collect()
    }

    fn apply_filters(&mut self, filters: LibraryFilters) {
        self.send(RuntimeInput::Query(QueryInput::ApplyFilters(filters)));
        self.settle();
    }

    /// The dock's own state brought in line with the model and its answers.
    fn sync_dock(&mut self, dock: &mut Dock) -> Vec<DockEffect> {
        let events = self.events();
        dock.sync(&self.runtime().bulk(), &events)
    }

    /// Hands the dock's commands to the runtime; the other effects are returned.
    fn run_dock(&mut self, effects: Vec<DockEffect>) -> Vec<DockEffect> {
        let mut rest = Vec::new();
        for effect in effects {
            match effect {
                DockEffect::Bulk(command) => self.bulk(command),
                DockEffect::Selection(input) => self.send(RuntimeInput::Selection(input)),
                other => rest.push(other),
            }
        }
        rest
    }
}

fn rejected<T>() -> Result<T, BackendError> {
    Err(BackendError::rejected("disk"))
}

// ---------------------------------------------------------------------------
// createBulk.test.ts: "bulk canonical editing"
// ---------------------------------------------------------------------------

/// "loads full tags, normalizes additions, removes and shares canonical data"
#[test]
fn loads_full_tags_normalizes_additions_removes_and_shares_canonical_data() {
    let mut fx = Fx::new(2);
    fx.select(&[1]);
    assert_eq!(fx.single_tags(), ["cat"]);

    let request = fx.add(" DOG ");
    fx.settle();
    assert_eq!(fx.tag_saved(request), Some(true));
    assert_eq!(fx.set_tags_calls(), [(1, list(&["cat", "dog"]))]);
    fx.remove("cat");
    fx.settle();
    assert_eq!(fx.tags_of(1), Some(list(&["dog"])));
    assert_eq!(fx.single_tags(), ["dog"]);
    assert_eq!(fx.count("list_tags"), 1, "the known tags are read once");
}

/// "accepts unchanged additions without a write"
#[test]
fn accepts_unchanged_additions_without_a_write() {
    let mut fx = Fx::new(2);
    fx.select(&[1]);
    let request = fx.add("cat");
    assert_eq!(fx.tag_saved(request), Some(true), "answered at once");
    fx.settle();
    assert!(fx.set_tags_calls().is_empty());
}

/// "never replaces unknown tags and retries missing details separately"
#[test]
fn never_replaces_unknown_tags_and_retries_missing_details_separately() {
    let mut fx = Fx::new(2);
    let mut reads = 0;
    fx.backend.on_get_asset_details(move |id| {
        reads += 1;
        Ok((reads > 1).then(|| details(id, &["cat"])))
    });
    fx.select(&[1]);
    assert!(fx.runtime().bulk().tag_details_failed());

    let request = fx.add("dog");
    fx.settle();
    assert_eq!(fx.tag_saved(request), Some(false));
    assert!(fx.set_tags_calls().is_empty());

    fx.bulk(BulkCommand::RetryTagDetails);
    fx.settle();
    assert_eq!(fx.single_tags(), ["cat"]);
    assert!(!fx.runtime().bulk().tag_details_failed());
}

/// "retains canonical tags after rejection and allows retry"
#[test]
fn retains_canonical_tags_after_rejection_and_allows_retry() {
    let mut fx = Fx::new(2);
    fx.select(&[1]);
    let mut writes = 0;
    fx.backend.on_set_asset_tags(move |id, tags| {
        writes += 1;
        if writes == 1 {
            rejected()
        } else {
            Ok(saved(id, tags))
        }
    });
    let request = fx.add("dog");
    fx.settle();
    assert_eq!(fx.tag_saved(request), Some(false));
    assert!(fx.runtime().bulk().tag_save_failed());
    assert_eq!(fx.single_tags(), ["cat"]);

    let request = fx.add("dog");
    fx.settle();
    assert_eq!(fx.tag_saved(request), Some(true));
    assert!(!fx.runtime().bulk().tag_save_failed());
}

/// "queues single-asset saves behind other writers and keeps every quick addition"
#[test]
fn queues_single_asset_saves_behind_other_writers_and_keeps_every_quick_addition() {
    let mut fx = Fx::new(2);
    fx.select(&[1]);
    // Another owner's write on the asset waits for its backend call.
    fx.external(
        1,
        Mutation::MergeTags {
            asset_ids: vec![1],
            tags: list(&["external"]),
        },
    );
    assert_eq!(fx.executor.pending(), 1);
    let first = fx.add("dog");
    assert!(fx.runtime().bulk().tag_applying());
    assert_eq!(
        fx.executor.pending(),
        1,
        "the save waits behind the other writer"
    );
    assert!(fx.set_tags_calls().is_empty());
    fx.settle();
    assert_eq!(fx.tag_saved(first), Some(true));
    // The replacement is computed from the canonical tags the earlier writer left.
    assert_eq!(
        fx.set_tags_calls().last().cloned(),
        Some((1, list(&["cat", "external", "dog"])))
    );

    // Two quick additions: the second waits for the first and keeps it.
    let second = fx.add("bird");
    let third = fx.add("fish");
    fx.settle();
    let answers: Vec<_> = fx.events();
    assert!(answers.contains(&BulkEvent::TagSettled {
        request: second,
        saved: true
    }));
    assert!(answers.contains(&BulkEvent::TagSettled {
        request: third,
        saved: true
    }));
    let writes = fx.set_tags_calls();
    assert_eq!(writes.len(), 3);
    assert_eq!(
        writes[2],
        (1, list(&["cat", "external", "dog", "bird", "fish"]))
    );
    assert_eq!(fx.single_tags(), ["cat", "external", "dog", "bird", "fish"]);
    assert!(!fx.runtime().bulk().tag_applying());
}

/// "does not publish old detail reads into a replacement selection"
#[test]
fn does_not_publish_old_detail_reads_into_a_replacement_selection() {
    let mut fx = Fx::new(2);
    fx.backend.on_get_asset_details(|id| {
        Ok(Some(details(id, if id == 1 { &["old"] } else { &["cat"] })))
    });
    fx.replace(1);
    fx.replace(2);
    // The read for the replacement answers first, the old one after it.
    let waiting = fx.executor.pending();
    assert!(waiting >= 2);
    fx.answer(waiting - 1);
    assert_eq!(fx.single_tags(), ["cat"]);
    fx.settle();
    assert_eq!(fx.selected_ids(), [2]);
    assert_eq!(fx.single_tags(), ["cat"]);
}

/// "merges all IDs including offscreen ones and shows add-only feedback"
#[test]
fn merges_all_ids_including_offscreen_ones_and_shows_add_only_feedback() {
    let mut fx = Fx::new(3000);
    fx.select(&[1, 2000]);
    let request = fx.add("dog");
    fx.settle();
    assert_eq!(fx.tag_saved(request), Some(true));
    assert_eq!(fx.merge_calls(), [(vec![1, 2000], list(&["dog"]))]);
    assert_eq!(fx.runtime().bulk().applied_bulk_tags(), ["dog"]);
    assert_eq!(fx.tags_of(2000), Some(list(&["cat", "dog"])));
    assert_eq!(
        fx.selected_assets()
            .iter()
            .map(|a| a.id)
            .collect::<Vec<_>>(),
        [1, 2000]
    );
    assert!(
        fx.count("get_asset_details") <= 1,
        "no read per selected asset"
    );
}

/// "queues a merge behind a writer on one target and sends the complete ID set"
#[test]
fn queues_a_merge_behind_a_writer_on_one_target_and_sends_the_complete_id_set() {
    let mut fx = Fx::new(2);
    fx.select(&[1, 2]);
    fx.external(
        1,
        Mutation::MergeTags {
            asset_ids: vec![2],
            tags: list(&["other"]),
        },
    );
    let pending = fx.executor.pending();
    let request = fx.add("dog");
    assert_eq!(fx.executor.pending(), pending, "the merge waits");
    assert_eq!(fx.merge_calls(), []);
    fx.settle();
    assert_eq!(fx.tag_saved(request), Some(true));
    let merges = fx.merge_calls();
    assert_eq!(merges.len(), 2);
    assert_eq!(merges[0], (vec![2], list(&["other"])));
    assert_eq!(merges[1], (vec![1, 2], list(&["dog"])));
}

/// "runs a tag and a favorite on the same selection in click order"
#[test]
fn runs_a_tag_and_a_favorite_on_the_same_selection_in_click_order() {
    let mut fx = Fx::new(2);
    fx.select(&[1, 2]);
    fx.backend.take_calls();
    let tag = fx.add("dog");
    fx.bulk(BulkCommand::ToggleFavorite);
    assert_eq!(fx.executor.pending(), 1, "the favorite waits for the tag");
    fx.settle();
    assert_eq!(fx.tag_saved(tag), Some(true));
    let order: Vec<_> = fx
        .backend
        .called_methods()
        .into_iter()
        .filter(|method| {
            *method == "merge_asset_tags_bulk" || *method == "toggle_assets_favorite_bulk"
        })
        .collect();
    assert_eq!(
        order,
        ["merge_asset_tags_bulk", "toggle_assets_favorite_bulk"]
    );
    assert!(fx
        .backend
        .calls()
        .contains(&Call::ToggleAssetsFavoriteBulk {
            asset_ids: vec![1, 2]
        }));
    let view = fx.runtime().bulk();
    assert!(!view.tag_save_failed() && !view.favorite_failed());
    assert!(view.all_selected_favorites());
}

/// "ignores obsolete selection errors and bulk feedback"
#[test]
fn ignores_obsolete_selection_errors_and_bulk_feedback() {
    let mut fx = Fx::new(3);
    fx.select(&[1, 2]);
    fx.backend.on_merge_asset_tags_bulk(|_, _| rejected());
    fx.add("dog");
    fx.replace(3);
    fx.settle();
    let view = fx.runtime().bulk();
    assert!(!view.tag_save_failed());
    assert!(view.applied_bulk_tags().is_empty());
}

/// "reports a zero-result merge, keeps it retryable and refreshes"
#[test]
fn reports_a_zero_result_merge_keeps_it_retryable_and_refreshes() {
    let mut fx = Fx::new(2);
    fx.select(&[1, 2]);
    fx.backend.on_merge_asset_tags_bulk(|_, _| {
        Ok(BulkTagMergeSummary {
            processed_assets: 0,
            updated_assets: 0,
            processed_asset_ids: Vec::new(),
            updated_asset_ids: Vec::new(),
            ..merged(Vec::new(), Vec::new())
        })
    });
    let request = fx.add("dog");
    fx.settle();
    assert_eq!(fx.tag_saved(request), Some(false));
    let view = fx.runtime().bulk();
    assert!(view.tag_save_failed());
    assert_eq!(
        view.partial_result(),
        Some(PartialResult {
            processed: 0,
            requested: 2
        })
    );
    assert_eq!(fx.count("start_asset_query"), 2, "the query is refreshed");
}

/// "blocks every operation while rectangle IDs are resolving"
#[test]
fn blocks_every_operation_while_rectangle_ids_are_resolving() {
    let mut fx = Fx::new(2);
    fx.select(&[1]);
    fx.backend.take_calls();
    fx.interact(Interaction::RectangleStart);
    let tag = fx.add("dog");
    fx.bulk(BulkCommand::ToggleFavorite);
    let group = fx.apply(None);
    fx.settle();
    let events = fx.events();
    assert!(events.contains(&BulkEvent::TagSettled {
        request: tag,
        saved: false
    }));
    assert!(events.contains(&BulkEvent::GroupSettled {
        request: group,
        result: GroupSaveResult::Ignored
    }));
    assert!(fx.set_tags_calls().is_empty());
    assert_eq!(fx.count("toggle_assets_favorite_bulk"), 0);
    assert!(fx.group_calls().is_empty());
}

// ---------------------------------------------------------------------------
// createBulk.test.ts: "bulk favorites and groups"
// ---------------------------------------------------------------------------

/// "uses backend favorite results for mixed and all-favorite selections"
#[test]
fn uses_backend_favorite_results_for_mixed_and_all_favorite_selections() {
    let mut fx = Fx::new(2);
    fx.select(&[1, 2]);
    fx.bulk(BulkCommand::ToggleFavorite);
    fx.settle();
    assert!(fx.runtime().bulk().all_selected_favorites());
    fx.backend.on_toggle_assets_favorite_bulk(|ids| {
        Ok(BulkFavoriteSummary {
            processed_asset_ids: ids,
            is_favorite: false,
            revision: 3,
        })
    });
    fx.bulk(BulkCommand::ToggleFavorite);
    fx.settle();
    assert!(!fx.runtime().bulk().all_selected_favorites());
}

/// "prunes only missing IDs, including an entirely missing selection"
#[test]
fn prunes_only_missing_ids_including_an_entirely_missing_selection() {
    let mut fx = Fx::new(3000);
    fx.select(&[1, 2000]);
    fx.backend.on_toggle_assets_favorite_bulk(|_| {
        Ok(BulkFavoriteSummary {
            processed_asset_ids: vec![2000],
            is_favorite: true,
            revision: 2,
        })
    });
    fx.bulk(BulkCommand::ToggleFavorite);
    fx.settle();
    assert_eq!(fx.selected_ids(), [2000]);
    assert!(fx.selected_assets()[0].is_favorite);

    fx.backend.on_toggle_assets_favorite_bulk(|_| {
        Ok(BulkFavoriteSummary {
            processed_asset_ids: Vec::new(),
            is_favorite: false,
            revision: 2,
        })
    });
    fx.bulk(BulkCommand::ToggleFavorite);
    fx.settle();
    assert_eq!(fx.runtime().selection().count(), 0);
}

/// "refreshes favorites-only filters"
#[test]
fn refreshes_favorites_only_filters() {
    let mut fx = Fx::new(3000);
    fx.select(&[1, 2000]);
    fx.apply_filters(LibraryFilters {
        favorites_only: true,
        ..LibraryFilters::default()
    });
    fx.bulk(BulkCommand::ToggleFavorite);
    fx.settle();
    assert_eq!(fx.count("start_asset_query"), 3);
    assert!(fx
        .backend
        .calls()
        .contains(&Call::ToggleAssetsFavoriteBulk {
            asset_ids: vec![1, 2000]
        }));
}

/// "keeps favorite failures in their card and retries"
#[test]
fn keeps_favorite_failures_in_their_card_and_retries() {
    let mut fx = Fx::new(2);
    fx.select(&[1]);
    let mut toggles = 0;
    fx.backend.on_toggle_assets_favorite_bulk(move |ids| {
        toggles += 1;
        if toggles == 1 {
            return rejected();
        }
        Ok(BulkFavoriteSummary {
            processed_asset_ids: ids,
            is_favorite: true,
            revision: 2,
        })
    });
    fx.bulk(BulkCommand::ToggleFavorite);
    fx.settle();
    let view = fx.runtime().bulk();
    assert!(view.favorite_failed());
    assert!(!view.tag_save_failed());
    assert!(!view.all_selected_favorites());
    fx.bulk(BulkCommand::ToggleFavorite);
    fx.settle();
    assert!(!fx.runtime().bulk().favorite_failed());
}

/// "queues repeated favorites and drains them before maintenance"
#[test]
fn queues_repeated_favorites_and_drains_them_before_maintenance() {
    let mut fx = Fx::new(2);
    fx.select(&[1]);
    fx.backend.take_calls();
    fx.bulk(BulkCommand::ToggleFavorite);
    fx.bulk(BulkCommand::ToggleFavorite);
    fx.send(RuntimeInput::library(LibraryInput::BeginMaintenance {
        ticket: Ticket(9),
    }));
    assert_eq!(fx.executor.pending(), 1, "one toggle at a time");
    let ready = |events: &[LibraryEvent]| {
        events
            .iter()
            .any(|event| matches!(event, LibraryEvent::MaintenanceReady { ticket: Ticket(9) }))
    };
    assert!(!ready(&fx.driver.model_mut().take_library_events()));
    fx.answer(0);
    assert_eq!(fx.count("toggle_assets_favorite_bulk"), 1);
    assert!(
        !ready(&fx.driver.model_mut().take_library_events()),
        "the second toggle was admitted before the barrier"
    );
    fx.settle();
    assert_eq!(fx.count("toggle_assets_favorite_bulk"), 2);
    assert!(ready(&fx.driver.model_mut().take_library_events()));
    assert!(!fx.runtime().bulk().favorite_applying());
    fx.send(RuntimeInput::library(LibraryInput::EndMaintenance {
        ticket: Ticket(9),
        reset: None,
    }));
}

/// "ignores stale identity completions"
#[test]
fn ignores_stale_identity_completions() {
    let mut fx = Fx::new(2);
    fx.select(&[1]);
    fx.bulk(BulkCommand::ToggleFavorite);
    fx.send(RuntimeInput::library(LibraryInput::Reset(
        LibraryReset::Identity,
    )));
    fx.replace(1);
    fx.settle();
    assert_eq!(fx.selected_ids(), [1]);
    assert!(!fx.runtime().bulk().all_selected_favorites());
}

/// "preserves existing single group order and normalizes the key"
#[test]
fn preserves_existing_single_group_order_and_normalizes_the_key() {
    let mut fx = Fx::with_rows(1, vec![grouped(1, Some("trip"), Some(7.0))]);
    fx.select(&[1]);
    fx.draft(" TRIP ");
    let request = fx.apply(None);
    fx.settle();
    assert_eq!(fx.group_result(request), Some(GroupSaveResult::Saved));
    assert_eq!(
        fx.group_calls(),
        [(vec![update(1, Some(7.0))], Some("TRIP".to_owned()))]
    );
}

/// "applies the exact order, rejects incomplete orders and removes groups"
#[test]
fn applies_the_exact_order_rejects_incomplete_orders_and_removes_groups() {
    let mut fx = Fx::new(2);
    fx.select(&[1, 2]);
    fx.draft(" trip ");
    let duplicate = fx.apply(Some(vec![1, 1]));
    assert_eq!(fx.group_result(duplicate), Some(GroupSaveResult::Ignored));
    let short = fx.apply(Some(vec![2]));
    assert_eq!(fx.group_result(short), Some(GroupSaveResult::Ignored));
    let exact = fx.apply(Some(vec![2, 1]));
    fx.settle();
    assert_eq!(fx.group_result(exact), Some(GroupSaveResult::Saved));
    assert_eq!(
        fx.group_calls(),
        [(
            vec![update(2, Some(1.0)), update(1, Some(2.0))],
            Some("trip".to_owned())
        )]
    );

    fx.draft("");
    fx.apply(None);
    fx.settle();
    assert_eq!(
        fx.group_calls().last().cloned(),
        Some((vec![update(2, None), update(1, None)], None))
    );
}

/// "retains group drafts after failure and reports partial saves"
#[test]
fn retains_group_drafts_after_failure_and_reports_partial_saves() {
    let mut fx = Fx::new(2);
    fx.select(&[1, 2]);
    fx.draft("trip");
    fx.backend.on_set_assets_media_group_bulk(|_, _| rejected());
    let failed = fx.apply(None);
    fx.settle();
    assert_eq!(fx.group_result(failed), Some(GroupSaveResult::Failed));
    assert_eq!(fx.runtime().bulk().group_key_draft(), "trip");
    assert!(fx.runtime().bulk().group_failed());

    fx.backend.on_set_assets_media_group_bulk(|_, key| {
        Ok(BulkMediaGroupSummary {
            processed_asset_ids: vec![1],
            processed_assets: 1,
            updated_assets: 1,
            media_group_key: key,
        })
    });
    let partial = fx.apply(None);
    fx.settle();
    assert_eq!(
        fx.group_result(partial),
        Some(GroupSaveResult::Partial(PartialResult {
            processed: 1,
            requested: 2
        }))
    );
}

/// Acceptance "Częściowy wynik publikuje tylko potwierdzone ID" for a group write: the
/// group reaches the records of the acknowledged assets only.
#[test]
fn a_partial_group_write_publishes_the_group_of_the_acknowledged_assets_only() {
    let mut fx = Fx::new(3);
    fx.select(&[1, 2, 3]);
    fx.draft("trip");
    fx.backend.on_set_assets_media_group_bulk(|updates, key| {
        let acknowledged: Vec<i64> = updates
            .iter()
            .map(|update| update.asset_id)
            .filter(|id| *id != 2)
            .collect();
        Ok(BulkMediaGroupSummary {
            processed_assets: acknowledged.len(),
            updated_assets: acknowledged.len(),
            processed_asset_ids: acknowledged,
            media_group_key: key,
        })
    });
    let request = fx.apply(None);
    fx.settle();
    assert_eq!(
        fx.group_result(request),
        Some(GroupSaveResult::Partial(PartialResult {
            processed: 2,
            requested: 3
        }))
    );
    let keys: Vec<(i64, Option<String>)> = fx
        .selected_assets()
        .into_iter()
        .map(|asset| (asset.id, asset.media_group_key))
        .collect();
    assert_eq!(
        keys,
        [
            (1, Some("trip".to_owned())),
            (2, None),
            (3, Some("trip".to_owned()))
        ]
    );
}

// ---------------------------------------------------------------------------
// createBulk.test.ts: "selected summary hydration"
// ---------------------------------------------------------------------------

fn ids(range: impl IntoIterator<Item = i64>) -> Arc<[i64]> {
    range.into_iter().collect()
}

fn answer_of(load: &SummaryLoad) -> Vec<AssetSummary> {
    load.ids.iter().copied().map(asset).collect()
}

/// "batches at 256 with four requests and retains records across eviction"
#[test]
fn batches_at_256_with_four_requests_and_retains_records_across_eviction() {
    assert_eq!(SUMMARY_BATCH, 256);
    let mut summaries = SelectedSummaries::new(0);
    let first = summaries.restart(ids(1..=1025), 0, []);
    assert_eq!(first.len(), 4);
    assert!(first.iter().all(|load| load.ids.len() == 256));

    let fifth = summaries.loaded(first[0].clone(), Ok(answer_of(&first[0])), 0);
    assert_eq!(fifth.len(), 1, "a fifth read starts once one ended");
    assert_eq!(fifth[0].ids, [1025]);
    for load in first[1..].iter().chain(&fifth) {
        assert!(summaries
            .loaded(load.clone(), Ok(answer_of(load)), 0)
            .is_empty());
    }
    assert!(summaries.ready());

    // The gallery's page of these rows is long gone: the records stay.
    let again = summaries.restart(ids([1, 1025]), 0, []);
    assert!(again.is_empty(), "no read for records it holds");
    assert_eq!(
        summaries
            .selected()
            .iter()
            .map(|a| a.id)
            .collect::<Vec<_>>(),
        [1, 1025]
    );
}

/// "marks omitted records retryable without dropping IDs"
#[test]
fn marks_omitted_records_retryable_without_dropping_ids() {
    let mut fx = Fx::new(3000);
    let mut reads = 0;
    fx.backend.on_get_asset_summaries_by_ids(move |ids| {
        reads += 1;
        Ok(if reads == 1 {
            Vec::new()
        } else {
            ids.into_iter().map(asset).collect()
        })
    });
    fx.select(&[2000]);
    assert!(fx.runtime().bulk().metadata_failed());
    assert_eq!(fx.selected_ids(), [2000]);
    let request = fx.apply(None);
    assert_eq!(fx.group_result(request), Some(GroupSaveResult::Ignored));

    fx.bulk(BulkCommand::RetryMetadata);
    fx.settle();
    assert_eq!(fx.selected_assets()[0].id, 2000);
    assert!(!fx.runtime().bulk().metadata_failed());
}

// ---------------------------------------------------------------------------
// createBulk.test.ts: "bulk read and completion races"
// ---------------------------------------------------------------------------

/// "overlays durable favorites on an older offscreen summary response"
#[test]
fn overlays_durable_favorites_on_an_older_offscreen_summary_response() {
    let mut fx = Fx::new(3000);
    fx.replace(2000);
    let reads = fx.executor.pending();
    assert!(reads >= 1, "the summary of the offscreen asset is read");
    fx.bulk(BulkCommand::ToggleFavorite);
    // The favorite is answered and published before the older summary read.
    fx.answer_last();
    fx.settle();
    assert_eq!(fx.selected_assets().len(), 1);
    assert!(fx.runtime().bulk().all_selected_favorites());
}

/// "preserves a typed group key while offscreen metadata arrives"
#[test]
fn preserves_a_typed_group_key_while_offscreen_metadata_arrives() {
    let mut fx = Fx::new(3000);
    fx.backend.on_get_asset_summaries_by_ids(|ids| {
        Ok(ids
            .into_iter()
            .map(|id| grouped(id, Some("loaded"), None))
            .collect())
    });
    fx.replace(2000);
    fx.draft("typed");
    fx.settle();
    assert_eq!(fx.runtime().bulk().group_key_draft(), "typed");
    assert_eq!(fx.runtime().bulk().ordered_asset_ids(), [2000]);
}

/// "does not reset current group drafts or failures when a retired save settles"
#[test]
fn does_not_reset_current_group_drafts_or_failures_when_a_retired_save_settles() {
    let mut fx = Fx::new(3);
    fx.select(&[1, 2]);
    fx.draft("old");
    let request = fx.apply(None);
    fx.replace(3);
    fx.draft("new");
    fx.settle();
    assert_eq!(fx.group_result(request), Some(GroupSaveResult::Ignored));
    assert_eq!(fx.runtime().bulk().group_key_draft(), "new");
    assert!(!fx.runtime().bulk().group_failed());
}

/// "does not update a disposed controller when a request rejects"
#[test]
fn does_not_update_a_disposed_controller_when_a_request_rejects() {
    let mut fx = Fx::new(2);
    fx.select(&[1]);
    fx.backend.on_set_asset_tags(|_, _| rejected());
    let request = fx.add("dog");
    fx.send(RuntimeInput::Bulk(BulkInput::Dispose));
    fx.settle();
    assert_eq!(
        fx.events(),
        [BulkEvent::TagSettled {
            request,
            saved: false
        }]
    );
    assert!(!fx.runtime().bulk().tag_save_failed());
}

/// "ignores obsolete hydration and keeps requests bounded after selection replacement"
#[test]
fn ignores_obsolete_hydration_and_keeps_requests_bounded_after_selection_replacement() {
    let mut summaries = SelectedSummaries::new(0);
    let old = summaries.restart(ids([2000]), 0, []);
    let new = summaries.restart(ids([3000]), 0, []);
    assert_eq!((old.len(), new.len()), (1, 1));
    assert!(summaries.running() <= 4);
    summaries.loaded(new[0].clone(), Ok(answer_of(&new[0])), 0);
    assert_eq!(summaries.selected()[0].id, 3000);
    summaries.loaded(old[0].clone(), Ok(answer_of(&old[0])), 0);
    assert_eq!(
        summaries
            .selected()
            .iter()
            .map(|a| a.id)
            .collect::<Vec<_>>(),
        [3000]
    );
    assert_eq!(summaries.running(), 0);
}

// ---------------------------------------------------------------------------
// createBulk.test.ts: "bulk partial publication and query changes"
// ---------------------------------------------------------------------------

/// "publishes only returned tag lists and keeps durable feedback after refresh
/// failure"
#[test]
fn publishes_only_returned_tag_lists_and_keeps_durable_feedback_after_refresh_failure() {
    let mut fx = Fx::new(2);
    fx.select(&[1, 2]);
    fx.backend
        .on_start_asset_query(|_| Err(BackendError::rejected("refresh")));
    let mut merges = 0;
    fx.backend.on_merge_asset_tags_bulk(move |ids, tags| {
        merges += 1;
        if merges > 1 {
            return Ok(merged(ids, tags));
        }
        Ok(BulkTagMergeSummary {
            query_impact: TagQueryImpact::All,
            processed_assets: 1,
            updated_assets: 1,
            processed_asset_ids: vec![1],
            updated_asset_ids: vec![1],
            results: vec![AssetTagResult {
                asset_id: 1,
                changed: true,
                tags: list(&["canonical"]),
            }],
            ..merged(Vec::new(), Vec::new())
        })
    });
    let request = fx.add("dog");
    fx.settle();
    assert_eq!(fx.tag_saved(request), Some(true));
    let view = fx.runtime().bulk();
    assert_eq!(view.applied_bulk_tags(), ["dog"]);
    assert_eq!(
        view.partial_result(),
        Some(PartialResult {
            processed: 1,
            requested: 2
        })
    );
    assert_eq!(fx.tags_of(1), Some(list(&["canonical"])));
    assert_eq!(fx.tags_of(2), None);
    // The unacknowledged asset's queue is free: the next writer on it is sent.
    fx.external(
        1,
        Mutation::MergeTags {
            asset_ids: vec![2],
            tags: list(&["next"]),
        },
    );
    fx.settle();
    assert_eq!(
        fx.merge_calls().last().cloned(),
        Some((vec![2], list(&["next"])))
    );
}

/// "keeps drafts and all selected IDs across filtered query replacement"
#[test]
fn keeps_drafts_and_all_selected_ids_across_filtered_query_replacement() {
    let mut fx = Fx::new(3000);
    fx.select(&[1, 2000]);
    fx.draft("kept");
    fx.add("dog");
    fx.settle();
    // The filtered query is a new backend session.
    fx.backend.on_start_asset_query(|_| {
        Ok(StartAssetQueryResult::Ready {
            session_id: 2,
            revision: 2,
            total: 1,
            offset: 0,
            items: vec![asset(1)],
        })
    });
    fx.apply_filters(LibraryFilters {
        include: list(&["dog"]),
        ..LibraryFilters::default()
    });
    let view = fx.runtime().bulk();
    assert_eq!(view.group_key_draft(), "kept");
    assert_eq!(view.applied_bulk_tags(), ["dog"]);
    fx.add("bird");
    fx.settle();
    assert_eq!(
        fx.merge_calls().last().cloned(),
        Some((vec![1, 2000], list(&["bird"])))
    );
}

/// "refreshes a matching tag filter after single removal"
#[test]
fn refreshes_a_matching_tag_filter_after_single_removal() {
    let mut fx = Fx::new(2);
    fx.select(&[1]);
    fx.apply_filters(LibraryFilters {
        include: list(&["cat"]),
        ..LibraryFilters::default()
    });
    fx.backend.on_set_asset_tags(|id, tags| {
        Ok(SetAssetTagsSummary {
            query_impact: TagQueryImpact::Tags {
                changed_tags: list(&["cat"]),
                tag_count_changed: true,
            },
            ..saved(id, tags)
        })
    });
    fx.remove("cat");
    fx.settle();
    assert_eq!(fx.count("start_asset_query"), 3);
}

/// "ignores details completed after deletion and clears selection on identity reset"
/// (the identity half; see the report for the deletion half)
#[test]
fn clears_the_selection_and_the_single_tags_on_identity_reset() {
    let mut fx = Fx::new(2);
    fx.select(&[1]);
    assert_eq!(fx.single_tags(), ["cat"]);
    fx.send(RuntimeInput::library(LibraryInput::Reset(
        LibraryReset::Identity,
    )));
    fx.settle();
    assert_eq!(fx.runtime().selection().count(), 0);
    assert!(fx.single_tags().is_empty());
}

/// "ignores details completed after deletion and clears selection on identity reset"
/// (the deletion half, epic E15): the single selected asset is deleted while the
/// retry of its details read still waits; the late answer publishes nothing.
#[test]
fn ignores_details_completed_after_deletion() {
    let mut fx = Fx::new(2);
    fx.backend
        .on_get_asset_details(|_| Err(BackendError::rejected("read")))
        .on_delete_asset(|_| {
            Ok(DeleteAssetSummary {
                removed_assets: 1,
                removed_thumbnails: 0,
                source_status: DeleteSourceStatus::Deleted,
                revision: 2,
                recovery_path: None,
            })
        });
    fx.select(&[1]);
    assert!(fx.runtime().bulk().tag_details_failed());

    fx.backend
        .on_get_asset_details(|id| Ok(Some(details(id, &["cat"]))));
    fx.bulk(BulkCommand::RetryTagDetails);
    assert!(fx.runtime().bulk().tag_details_loading());
    let waiting = fx.executor.pending();
    assert!(waiting >= 1, "the details read waits");

    fx.external(9, Mutation::DeleteAsset { asset_id: 1 });
    assert_eq!(fx.executor.pending(), waiting + 1);
    // The deletion answers first, then the details read.
    fx.answer(waiting);
    assert_eq!(fx.count("delete_asset"), 1);
    fx.settle();
    let view = fx.runtime().bulk();
    assert!(view.single_asset_tags().is_empty());
    assert!(view.tag_details_failed());
}

/// "queues group saves behind another writer and repeated saves behind each other"
#[test]
fn queues_group_saves_behind_another_writer_and_repeated_saves_behind_each_other() {
    let mut fx = Fx::new(2);
    fx.select(&[1, 2]);
    fx.external(
        1,
        Mutation::MergeTags {
            asset_ids: vec![2],
            tags: list(&["other"]),
        },
    );
    let first = fx.apply(None);
    let second = fx.apply(None);
    assert!(fx.runtime().bulk().group_applying());
    assert_eq!(
        fx.executor.pending(),
        1,
        "both saves wait for the other writer"
    );
    assert!(fx.group_calls().is_empty());
    fx.answer(0);
    assert_eq!(fx.executor.pending(), 1, "one save at a time");
    fx.settle();
    let events = fx.events();
    for request in [first, second] {
        assert!(events.contains(&BulkEvent::GroupSettled {
            request,
            result: GroupSaveResult::Saved
        }));
    }
    assert_eq!(fx.group_calls().len(), 2);
    assert!(!fx.runtime().bulk().group_applying());
}

/// "re-reads complete tags when a query refresh discards a pending detail read"
#[test]
fn re_reads_complete_tags_when_a_query_refresh_discards_a_pending_detail_read() {
    let mut fx = Fx::new(2);
    fx.backend.take_calls();
    fx.click(1);
    assert_eq!(fx.count("get_asset_details"), 0);
    assert_eq!(fx.executor.pending(), 1, "the detail read waits");
    fx.send(RuntimeInput::RefreshLibrary);
    // The refresh ends before the read. Solid reads twice: the discarded read and one
    // read once the refresh has ended.
    while fx.executor.pending() > 1 {
        fx.answer(1);
    }
    fx.settle();
    let view = fx.runtime().bulk();
    assert_eq!(view.single_asset_tags(), ["cat"]);
    assert!(!view.tag_details_failed() && !view.tag_details_loading());
    assert_eq!(fx.count("get_asset_details"), 2);
}

/// "enables the editor when another owner publishes canonical tags before an old read
/// completes"
#[test]
fn enables_the_editor_when_another_owner_publishes_canonical_tags_before_an_old_read_completes() {
    let mut fx = Fx::new(2);
    fx.backend
        .on_get_asset_details(|_| Err(BackendError::rejected("read")));
    fx.select(&[1]);
    assert!(fx.runtime().bulk().tag_details_failed());
    fx.backend
        .on_get_asset_details(|id| Ok(Some(details(id, &["cat"]))));
    fx.bulk(BulkCommand::RetryTagDetails);
    assert!(fx.runtime().bulk().tag_details_loading());
    // Another owner publishes the canonical list before the read answers.
    fx.backend.on_merge_asset_tags_bulk(|ids, _| {
        Ok(BulkTagMergeSummary {
            results: ids
                .iter()
                .map(|asset_id| AssetTagResult {
                    asset_id: *asset_id,
                    changed: true,
                    tags: list(&["shared"]),
                })
                .collect(),
            ..merged(ids.clone(), Vec::new())
        })
    });
    fx.external(
        1,
        Mutation::MergeTags {
            asset_ids: vec![1],
            tags: list(&["shared"]),
        },
    );
    fx.answer_last();
    let view = fx.runtime().bulk();
    assert_eq!(view.single_asset_tags(), ["shared"]);
    assert!(!view.tag_details_loading() && !view.tag_details_failed());
    fx.settle();
    assert_eq!(fx.single_tags(), ["shared"]);
}

/// "ignores deletion notifications outside the selected IDs"
#[test]
fn ignores_deletion_notifications_outside_the_selected_ids() {
    let mut summaries = SelectedSummaries::new(0);
    summaries.restart(ids([1]), 0, &[asset(1)]);
    summaries.commit(&MetadataCommit::Deleted(vec![2000]));
    assert!(summaries.ready());
    assert!(!summaries.failed());
    assert_eq!(summaries.selected()[0].id, 1);
}

// ---------------------------------------------------------------------------
// createBulk.test.ts: "ranged (spec) selections"
// ---------------------------------------------------------------------------

const RANGED_TOTAL: usize = 50_000;

/// Select all of 50,000 results, then a Ctrl-click takes the second one out.
fn ranged() -> Fx {
    let mut fx = Fx::new(RANGED_TOTAL);
    fx.interact(Interaction::SelectAll);
    fx.settle();
    fx.click(2);
    fx.settle();
    fx
}

fn ranged_spec() -> SelectionSpec {
    SelectionSpec {
        session_id: 1,
        ranges: vec![[0, RANGED_TOTAL - 1]],
        excluded: vec![2],
        included: Vec::new(),
    }
}

/// "adds a tag and toggles favorites through one spec write each, without loading
/// summaries"
#[test]
fn adds_a_tag_and_toggles_favorites_through_one_spec_write_each_without_loading_summaries() {
    let mut fx = ranged();
    let view = fx.runtime().bulk();
    assert_eq!(view.selected_count(), RANGED_TOTAL - 1);
    assert!(view.selection_ranged());
    assert_eq!(view.tag_mode(), TagMode::Multiple);
    assert!(!view.metadata_loading());
    assert!(!view.all_selected_favorites());
    assert_eq!(fx.count("get_asset_summaries_by_ids"), 0);

    fx.backend
        .on_apply_selection_mutation(|_, _| Ok(ready_selection(RANGED_TOTAL - 1, None)));
    let request = fx.add("Sunset");
    fx.settle();
    assert_eq!(fx.tag_saved(request), Some(true));
    assert_eq!(
        fx.spec_calls(),
        [(
            ranged_spec(),
            SelectionMutation::MergeTags {
                tags: list(&["sunset"])
            }
        )]
    );
    assert!(fx.merge_calls().is_empty());
    assert_eq!(fx.runtime().bulk().applied_bulk_tags(), ["sunset"]);
    assert!(!fx.runtime().bulk().tag_save_failed());

    fx.backend
        .on_apply_selection_mutation(|_, _| Ok(ready_selection(RANGED_TOTAL - 1, Some(true))));
    fx.bulk(BulkCommand::ToggleFavorite);
    fx.settle();
    assert_eq!(
        fx.spec_calls().last().cloned(),
        Some((ranged_spec(), SelectionMutation::ToggleFavorite))
    );
    assert_eq!(fx.count("toggle_assets_favorite_bulk"), 0);
    assert!(!fx.runtime().bulk().favorite_failed());
}

/// "reports a stale spec as failed, writes nothing locally and refreshes the query"
#[test]
fn reports_a_stale_spec_as_failed_writes_nothing_locally_and_refreshes_the_query() {
    let mut fx = ranged();
    let starts = fx.count("start_asset_query");
    fx.backend
        .on_apply_selection_mutation(|_, _| Ok(SelectionMutationResult::Stale));
    let request = fx.add("late");
    fx.settle();
    assert_eq!(fx.tag_saved(request), Some(false));
    assert!(fx.runtime().bulk().tag_save_failed());
    assert!(fx.runtime().bulk().applied_bulk_tags().is_empty());
    assert!(
        fx.count("start_asset_query") > starts,
        "the query is refreshed"
    );
    assert_eq!(fx.runtime().query().total(), RANGED_TOTAL);
}

/// "reports a stale spec as failed ..." (the favorite half: a stale toggle fails its
/// own card)
#[test]
fn a_stale_spec_fails_the_favorite_toggle() {
    let mut fx = ranged();
    fx.backend
        .on_apply_selection_mutation(|_, _| Ok(SelectionMutationResult::Stale));
    fx.bulk(BulkCommand::ToggleFavorite);
    fx.settle();
    assert!(fx.runtime().bulk().favorite_failed());
}

/// "groups a ranged selection in gallery order through the spec"
#[test]
fn groups_a_ranged_selection_in_gallery_order_through_the_spec() {
    let mut fx = ranged();
    fx.backend.on_apply_selection_mutation(|_, _| {
        Ok(SelectionMutationResult::Ready {
            processed_assets: RANGED_TOTAL - 1,
            updated_assets: RANGED_TOTAL - 1,
            revision: 5,
            query_impact: TagQueryImpact::None,
            vocabulary_changed: false,
            is_favorite: None,
            media_group_key: Some("trip".into()),
        })
    });
    fx.draft(" trip ");
    let request = fx.apply(None);
    fx.settle();
    assert_eq!(fx.group_result(request), Some(GroupSaveResult::Saved));
    assert_eq!(
        fx.spec_calls(),
        [(
            ranged_spec(),
            SelectionMutation::SetMediaGroup {
                media_group_key: Some("trip".into())
            }
        )]
    );
    assert!(fx.group_calls().is_empty());
    // An explicit order needs materialized IDs, which a range does not have.
    let explicit = fx.apply(Some(vec![1, 3]));
    assert_eq!(fx.group_result(explicit), Some(GroupSaveResult::Ignored));
}

// ---------------------------------------------------------------------------
// bulkGroupOrderService.test.ts
// ---------------------------------------------------------------------------

/// "trims group key"
#[test]
fn trims_group_key() {
    assert_eq!(normalize_group_key("  trip-2026  "), "trip-2026");
}

/// "keeps positive unique asset ids in order"
#[test]
fn keeps_positive_unique_asset_ids_in_order() {
    assert_eq!(unique_ordered_asset_ids(&[3, -1, 3, 2, 0, 1, 2]), [3, 2, 1]);
}

/// "builds 1-based media group order assignments"
#[test]
fn builds_1_based_media_group_order_assignments() {
    assert_eq!(
        build_bulk_media_group_assignments(&[7, 9, 4], true),
        [
            update(7, Some(1.0)),
            update(9, Some(2.0)),
            update(4, Some(3.0))
        ]
    );
}

/// "reorders ids by drag and drop target"
#[test]
fn reorders_ids_by_drag_and_drop_target() {
    assert_eq!(
        reorder_asset_ids_by_drop(&[11, 12, 13, 14], 11, 13),
        [12, 13, 11, 14]
    );
}

/// "derives a shared group and sorts its assets by saved order"
#[test]
fn derives_a_shared_group_and_sorts_its_assets_by_saved_order() {
    let assets = [
        grouped(1, Some(" Trip "), Some(3.0)),
        grouped(2, Some("trip"), Some(1.0)),
        grouped(3, Some("TRIP"), None),
    ];
    assert_eq!(
        derive_bulk_group_selection_state(&assets.iter().collect::<Vec<_>>()),
        BulkGroupSelectionState {
            group_key: "Trip".into(),
            has_conflicting_groups: false,
            ordered_asset_ids: vec![2, 1, 3],
        }
    );
}

/// "reports mixed grouped and ungrouped selections as conflicting"
#[test]
fn reports_mixed_grouped_and_ungrouped_selections_as_conflicting() {
    let assets = [grouped(1, Some("trip"), Some(1.0)), grouped(2, None, None)];
    assert_eq!(
        derive_bulk_group_selection_state(&assets.iter().collect::<Vec<_>>()),
        BulkGroupSelectionState {
            group_key: String::new(),
            has_conflicting_groups: true,
            ordered_asset_ids: vec![1, 2],
        }
    );
}

/// "builds nullable assignments for clearing"
#[test]
fn builds_nullable_assignments_for_clearing() {
    assert_eq!(
        build_bulk_media_group_assignments(&[4, 2], false),
        [update(4, None), update(2, None)]
    );
}

// ---------------------------------------------------------------------------
// GroupOrderModal.test.tsx: the board's rows (the drawn tiles and their thumbnail
// requests are checked in the GPUI window tests)
// ---------------------------------------------------------------------------

/// "windows fixed-height rows and requests thumbnails only for them": a 960 pixel wide
/// larger view lists three 312 pixel columns in rows of 262 pixels; at the top three
/// visible rows and two overscan rows are drawn, scrolled to row 20 rows 18 to 24.
#[test]
fn windows_fixed_height_rows() {
    let layout = BoardLayout::new(960.0, MODAL_MIN_TILE_WIDTH, 96);
    assert_eq!(layout.columns, 3);
    assert_eq!(layout.tile_width, 312.0);
    assert_eq!(layout.row_height, 262.0);
    assert_eq!(layout.total_height(), 32.0 * 262.0);
    assert_eq!(layout.row_top(1), 262.0);
    let tiles = |rows: std::ops::Range<usize>| {
        rows.flat_map(|row| layout.row_indexes(row))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        tiles(layout.visible_rows(0.0, 560.0)),
        (0..15).collect::<Vec<_>>()
    );
    assert_eq!(
        tiles(layout.visible_rows(262.0 * 20.0, 560.0)),
        (18 * 3..25 * 3).collect::<Vec<_>>()
    );
}

/// "keeps a keyboard-moved tile in view": arrow down on the seventh tile moves it to
/// index 9 (row 3), which ends at 4 * 262 = 1048 pixels.
#[test]
fn keeps_a_keyboard_moved_tile_in_view() {
    let layout = BoardLayout::new(960.0, MODAL_MIN_TILE_WIDTH, 96);
    let order: Vec<i64> = (1..=96).collect();
    let (next, row) = layout
        .key_move(&order, 7, BoardKey::Down)
        .expect("the tile moves a row down");
    assert_eq!(next.iter().position(|id| *id == 7), Some(9));
    assert_eq!(row, 3);
    let scroll = layout.reveal_row(row, 0.0, 560.0);
    assert_eq!(scroll, 1048.0 - 560.0);
    let drawn = layout.visible_rows(scroll, 560.0);
    assert!(drawn.contains(&row));
}

// ---------------------------------------------------------------------------
// BulkDock.test.tsx: the dock's own state over the real runtime
// ---------------------------------------------------------------------------

/// "explains an invalid tag, keeps the draft and does not submit it"
#[test]
fn explains_an_invalid_tag_keeps_the_draft_and_does_not_submit_it() {
    let mut fx = Fx::new(2);
    fx.select(&[1]);
    let mut dock = Dock::new();
    fx.sync_dock(&mut dock);
    dock.set_tag_draft("two;tags".into());
    let effects = dock.add_tag("two;tags", &fx.runtime().bulk());
    assert!(effects.is_empty(), "nothing is submitted");
    assert!(dock.tag_invalid());
    assert_eq!(dock.tag_draft(), "two;tags");
    // Typing again takes the message away.
    dock.set_tag_draft("two;tag".into());
    assert!(!dock.tag_invalid());
}

/// "cancels deferred tag focus when selection changes before re-enabling"
#[test]
fn cancels_deferred_tag_focus_when_selection_changes_before_re_enabling() {
    let mut fx = Fx::new(2);
    fx.select(&[1]);
    let mut dock = Dock::new();
    fx.sync_dock(&mut dock);
    let effects = dock.add_tag("bird", &fx.runtime().bulk());
    fx.run_dock(effects);
    // The save answers, but the selection changed before the field is enabled again.
    fx.replace(2);
    fx.settle();
    let effects = fx.sync_dock(&mut dock);
    assert!(!effects.contains(&DockEffect::FocusTagInput));
    assert_eq!(dock.tag_draft(), "");
}

/// "ignores a retired selection's saved / failed / rejected tag save without clearing
/// the new draft or stealing focus" (a rejection reaches the dock as a failed save)
#[test]
fn ignores_a_retired_selections_tag_save_without_clearing_the_new_draft_or_stealing_focus() {
    for succeeds in [true, false] {
        let mut fx = Fx::new(2);
        fx.select(&[1]);
        if !succeeds {
            fx.backend.on_set_asset_tags(|_, _| rejected());
        }
        let mut dock = Dock::new();
        fx.sync_dock(&mut dock);
        let effects = dock.add_tag("old-tag", &fx.runtime().bulk());
        fx.run_dock(effects);
        assert_eq!(fx.executor.pending(), 1, "the save waits");
        fx.replace(2);
        fx.sync_dock(&mut dock);
        assert_eq!(dock.tag_draft(), "", "a new selection starts empty");
        dock.set_tag_draft("new-draft".into());
        fx.settle();
        let effects = fx.sync_dock(&mut dock);
        assert_eq!(dock.tag_draft(), "new-draft", "saved: {succeeds}");
        assert!(
            !effects.contains(&DockEffect::FocusTagInput),
            "saved: {succeeds}"
        );
        assert_eq!(fx.set_tags_calls().len(), 1);
    }
}

/// "restores focus after a completed save's disabled state is committed away" and
/// "restores tag input focus after an Enter submission so another tag can be added":
/// the focus is asked for once the save ended and the field is enabled again.
#[test]
fn asks_for_the_tag_field_focus_once_a_save_ended_and_the_field_is_enabled() {
    let mut fx = Fx::new(2);
    fx.select(&[1]);
    let mut dock = Dock::new();
    fx.sync_dock(&mut dock);
    let effects = dock.add_tag("travel", &fx.runtime().bulk());
    fx.run_dock(effects);
    assert!(Dock::tag_controls_disabled(&fx.runtime().bulk()));
    assert_eq!(dock.tag_draft(), "travel");
    assert!(!fx.sync_dock(&mut dock).contains(&DockEffect::FocusTagInput));
    fx.settle();
    let effects = fx.sync_dock(&mut dock);
    assert!(effects.contains(&DockEffect::FocusTagInput));
    assert_eq!(dock.tag_draft(), "");
    // The next addition is sent as well.
    let effects = dock.add_tag("summer", &fx.runtime().bulk());
    fx.run_dock(effects);
    fx.settle();
    assert!(fx.sync_dock(&mut dock).contains(&DockEffect::FocusTagInput));
    assert_eq!(
        fx.set_tags_calls().last().cloned(),
        Some((1, list(&["cat", "travel", "summer"])))
    );
}

/// "retains the selected suggestion, not the search prefix, when saving fails"
#[test]
fn retains_the_added_tag_not_the_typed_prefix_when_saving_fails() {
    let mut fx = Fx::new(2);
    fx.select(&[1]);
    fx.backend.on_set_asset_tags(|_, _| rejected());
    let mut dock = Dock::new();
    fx.sync_dock(&mut dock);
    dock.set_tag_draft("tra".into());
    // A picked suggestion adds its own text.
    let effects = dock.add_tag("travel", &fx.runtime().bulk());
    fx.run_dock(effects);
    fx.settle();
    fx.sync_dock(&mut dock);
    assert_eq!(dock.tag_draft(), "travel");
    // Enter adds it again.
    let effects = dock.add_tag(dock.tag_draft().to_owned().as_str(), &fx.runtime().bulk());
    assert!(matches!(
        effects.as_slice(),
        [DockEffect::Bulk(BulkCommand::AddTag { tag, .. })] if tag == "travel"
    ));
}

/// "explains why a range selection was dropped: changed results or an expired
/// session" and "keeps the dock open to show why an empty selection was cleared"
#[test]
fn a_dropped_range_selection_keeps_the_dock_open_with_its_reason() {
    let fx = Fx::new(2);
    assert_eq!(fx.runtime().bulk().selection_notice(), None);
    assert!(!Dock::expanded(&fx.runtime().bulk()));

    let mut fx = ranged();
    // A new result: the ranges' positions mean other assets now.
    fx.backend.on_start_asset_query(|_| {
        Ok(StartAssetQueryResult::Ready {
            session_id: 2,
            revision: 2,
            total: RANGED_TOTAL,
            offset: 0,
            items: (1..=FIRST_PAGE as i64).map(asset).collect(),
        })
    });
    fx.send(RuntimeInput::RefreshLibrary);
    fx.settle();
    let view = fx.runtime().bulk();
    assert_eq!(view.selected_count(), 0);
    assert_eq!(view.selection_notice(), Some(SelectionNotice::RangeCleared));
    assert!(Dock::expanded(&view));
    assert_eq!(
        Dock::notice_description_key(SelectionNotice::RangeCleared),
        "bulk.panel.selectionClearedDescription"
    );
    assert!(Dock::tag_controls_disabled(&view));
    assert!(Dock::controls_disabled(&view), "the counter takes no click");

    // A failed refresh leaves the ranges without their session: expired.
    let mut fx = ranged();
    fx.backend
        .on_start_asset_query(|_| Err(BackendError::rejected("refresh")));
    fx.send(RuntimeInput::RefreshLibrary);
    fx.settle();
    let view = fx.runtime().bulk();
    assert_eq!(view.selection_notice(), Some(SelectionNotice::RangeExpired));
    assert!(Dock::expanded(&view));
    assert_eq!(
        Dock::notice_description_key(SelectionNotice::RangeExpired),
        "bulk.panel.selectionExpiredDescription"
    );
}

/// "renders the lightbox-style heart and exposes pending and failure states"
#[test]
fn the_heart_is_enabled_for_unloaded_rows_and_shows_pending_and_failure() {
    let mut fx = Fx::new(3000);
    fx.replace(2000);
    // Selected IDs need not have loaded gallery rows.
    assert!(!Dock::favorite_disabled(&fx.runtime().bulk()));
    assert_eq!(
        Dock::favorite_label_key(&fx.runtime().bulk()),
        "bulk.favorite.toggle"
    );
    fx.settle();
    fx.bulk(BulkCommand::ToggleFavorite);
    assert!(Dock::favorite_disabled(&fx.runtime().bulk()), "pending");
    fx.settle();
    let view = fx.runtime().bulk();
    assert!(view.all_selected_favorites());
    assert_eq!(Dock::favorite_label_key(&view), "bulk.favorite.remove");
    fx.backend.on_toggle_assets_favorite_bulk(|_| rejected());
    fx.bulk(BulkCommand::ToggleFavorite);
    fx.settle();
    assert!(fx.runtime().bulk().favorite_failed());
}

/// "disables every write control while rectangle IDs are resolving"
#[test]
fn disables_every_write_control_while_rectangle_ids_are_resolving() {
    let mut fx = Fx::new(2);
    fx.select(&[1]);
    fx.draft("trip");
    fx.interact(Interaction::RectangleStart);
    let view = fx.runtime().bulk();
    assert!(Dock::favorite_disabled(&view));
    assert!(
        Dock::tag_controls_disabled(&view),
        "the field and the chips"
    );
    assert!(Dock::group_controls_disabled(&view));
    assert!(Dock::apply_group_disabled(&view));
}

/// "collapses to a chip for an empty selection and expands once items are selected"
/// and "switches tabs ... makes the group tab tall"
#[test]
fn collapses_for_an_empty_selection_and_is_tall_only_on_the_group_tab() {
    let mut fx = Fx::new(2);
    let mut dock = Dock::new();
    fx.sync_dock(&mut dock);
    assert!(!Dock::expanded(&fx.runtime().bulk()));
    fx.select(&[1, 2]);
    fx.sync_dock(&mut dock);
    let view = fx.runtime().bulk();
    assert!(Dock::expanded(&view));
    assert!(!dock.tall(&view));
    assert_eq!(dock.tab(), DockTab::Tags);
    assert_eq!(
        Dock::tags_description_key(&view),
        "bulk.panel.tagsMultipleDescription"
    );
    dock.select_tab(DockTab::Group);
    assert!(dock.tall(&fx.runtime().bulk()));
}

/// "clears the selection from the counter"
#[test]
fn clears_the_selection_from_the_counter() {
    let mut fx = Fx::new(3);
    fx.select(&[1, 2, 3]);
    let dock = Dock::new();
    let effects = dock.clear_selection();
    assert_eq!(
        effects,
        [DockEffect::Selection(SelectionInput::Interact(
            Interaction::Clear
        ))]
    );
    fx.run_dock(effects);
    assert_eq!(fx.runtime().selection().count(), 0);
    assert!(
        fx.runtime().selection().enabled(),
        "selection mode stays on"
    );
}

/// "returns to the tags tab when the selection is cleared"
#[test]
fn returns_to_the_tags_tab_when_the_selection_is_cleared() {
    let mut fx = Fx::new(2);
    fx.select(&[1]);
    let mut dock = Dock::new();
    fx.sync_dock(&mut dock);
    dock.select_tab(DockTab::Group);
    fx.interact(Interaction::Clear);
    fx.sync_dock(&mut dock);
    fx.select(&[1]);
    fx.sync_dock(&mut dock);
    assert_eq!(dock.tab(), DockTab::Tags);
}

/// `BulkDock.tsx`: the effect on `selectedCount === 0` sets the Tags tab when the
/// selection becomes empty, not while it stays empty. A notice keeps the dock open
/// with nothing selected ("keeps the dock open to show why an empty selection was
/// cleared"), and a tab picked then stays, also once something is selected again.
#[test]
fn a_tab_picked_while_a_notice_shows_an_empty_selection_stays() {
    let mut fx = ranged();
    let mut dock = Dock::new();
    fx.sync_dock(&mut dock);
    dock.select_tab(DockTab::Group);
    fx.sync_dock(&mut dock);
    assert_eq!(dock.tab(), DockTab::Group);

    // The results change under the ranges: the selection is dropped with a notice,
    // and the tab is Tags again.
    fx.backend.on_start_asset_query(|_| {
        Ok(StartAssetQueryResult::Ready {
            session_id: 2,
            revision: 2,
            total: RANGED_TOTAL,
            offset: 0,
            items: (1..=FIRST_PAGE as i64).map(asset).collect(),
        })
    });
    fx.send(RuntimeInput::RefreshLibrary);
    fx.settle();
    fx.sync_dock(&mut dock);
    let view = fx.runtime().bulk();
    assert_eq!(view.selected_count(), 0);
    assert_eq!(view.selection_notice(), Some(SelectionNotice::RangeCleared));
    assert!(Dock::expanded(&view));
    assert_eq!(dock.tab(), DockTab::Tags);

    // A click on Group & order, and the arrow keys of the tablist, are not undone
    // by the syncs that follow while nothing is selected.
    dock.select_tab(DockTab::Group);
    fx.sync_dock(&mut dock);
    fx.sync_dock(&mut dock);
    assert_eq!(dock.tab(), DockTab::Group);
    let view = fx.runtime().bulk();
    assert!(dock.tall(&view));
    assert!(Dock::group_controls_disabled(&view) && Dock::apply_group_disabled(&view));
    dock.tab_key(DockTab::Group, TabKey::Right);
    fx.sync_dock(&mut dock);
    assert_eq!(dock.tab(), DockTab::Tags);
    dock.tab_key(DockTab::Tags, TabKey::End);
    fx.sync_dock(&mut dock);
    assert_eq!(dock.tab(), DockTab::Group);

    // Something is selected again: the notice goes and the tab stays where it was.
    fx.select(&[1]);
    fx.sync_dock(&mut dock);
    let view = fx.runtime().bulk();
    assert_eq!(view.selected_count(), 1);
    assert_eq!(view.selection_notice(), None);
    assert_eq!(dock.tab(), DockTab::Group);

    // Emptied again, the tab returns to Tags once.
    fx.interact(Interaction::Clear);
    fx.sync_dock(&mut dock);
    assert_eq!(dock.tab(), DockTab::Tags);
}

/// Three assets selected, the key `trip` typed and the dock in line.
fn sorting() -> (Fx, Dock) {
    let mut fx = Fx::new(3);
    fx.select(&[1, 2, 3]);
    fx.draft("trip");
    let mut dock = Dock::new();
    fx.sync_dock(&mut dock);
    (fx, dock)
}

/// "sorts inline and applies the changed order with the group"
#[test]
fn sorts_inline_and_applies_the_changed_order_with_the_group() {
    let (mut fx, mut dock) = sorting();
    assert!(Dock::has_order_panel(&fx.runtime().bulk()));
    assert_eq!(dock.inline_order(), [1, 2, 3]);
    dock.set_inline_order(vec![2, 1, 3]);
    let effects = dock.apply_group(&fx.runtime().bulk());
    assert!(matches!(
        effects.as_slice(),
        [DockEffect::Bulk(BulkCommand::ApplyGroup { order: Some(order), .. })] if order == &[2, 1, 3]
    ));
    fx.run_dock(effects);
    fx.settle();
    assert_eq!(
        fx.group_calls(),
        [(
            vec![
                update(2, Some(1.0)),
                update(1, Some(2.0)),
                update(3, Some(3.0))
            ],
            Some("trip".to_owned())
        )]
    );
}

/// "applies the controller's order when the inline order is unchanged"
#[test]
fn applies_the_models_order_when_the_inline_order_is_unchanged() {
    let (fx, mut dock) = sorting();
    let effects = dock.apply_group(&fx.runtime().bulk());
    assert!(matches!(
        effects.as_slice(),
        [DockEffect::Bulk(BulkCommand::ApplyGroup {
            order: None,
            ..
        })]
    ));
}

/// "opens the larger view from the inline draft and hides the inline board meanwhile"
#[test]
fn opens_the_larger_view_from_the_inline_draft() {
    let (mut fx, mut dock) = sorting();
    dock.set_inline_order(vec![1, 3, 2]);
    dock.open_order_modal(&fx.runtime().bulk());
    fx.sync_dock(&mut dock);
    assert!(dock.order_modal_open(&fx.runtime().bulk()));
    assert_eq!(dock.modal_order(), [1, 3, 2]);
}

/// "resets the inline draft when the group key changes"
#[test]
fn resets_the_inline_draft_when_the_group_key_changes() {
    let (mut fx, mut dock) = sorting();
    dock.set_inline_order(vec![2, 1, 3]);
    fx.sync_dock(&mut dock);
    assert_eq!(dock.inline_order(), [2, 1, 3]);
    fx.draft("other");
    fx.sync_dock(&mut dock);
    assert_eq!(dock.inline_order(), [1, 2, 3]);
}

/// "shows conflicts in the group tab and opens the larger sorting view"
#[test]
fn shows_conflicts_and_opens_the_larger_sorting_view() {
    let mut fx = Fx::with_rows(
        3,
        vec![
            grouped(1, Some("a"), Some(1.0)),
            grouped(2, Some("b"), Some(1.0)),
            grouped(3, None, None),
        ],
    );
    fx.select(&[1, 2, 3]);
    let mut dock = Dock::new();
    fx.sync_dock(&mut dock);
    assert!(fx.runtime().bulk().has_conflicting_groups());
    assert_eq!(fx.runtime().bulk().group_key_draft(), "");
    fx.draft("trip");
    fx.sync_dock(&mut dock);
    assert!(Dock::has_order_panel(&fx.runtime().bulk()));
    assert!(!Dock::open_order_modal_disabled(&fx.runtime().bulk()));
    dock.open_order_modal(&fx.runtime().bulk());
    fx.sync_dock(&mut dock);
    assert!(dock.order_modal_open(&fx.runtime().bulk()));
    assert_eq!(dock.modal_order(), [1, 2, 3]);
}

/// "does not expose group ordering until a group key is set"
#[test]
fn does_not_expose_group_ordering_until_a_group_key_is_set() {
    let mut fx = Fx::new(2);
    fx.select(&[1, 2]);
    assert!(!Dock::has_order_panel(&fx.runtime().bulk()));
    fx.draft("  ");
    assert!(!Dock::has_order_panel(&fx.runtime().bulk()));
    fx.draft("trip");
    assert!(Dock::has_order_panel(&fx.runtime().bulk()));
}

/// "shows applied tags without removal controls in multi-selection mode" (the chips
/// the dock shows; their remove buttons are checked in the window tests)
#[test]
fn shows_applied_tags_in_multi_selection_mode() {
    let mut fx = Fx::new(2);
    fx.select(&[1, 2]);
    fx.add("travel");
    fx.settle();
    let view = fx.runtime().bulk();
    assert_eq!(view.tag_mode(), TagMode::Multiple);
    assert_eq!(Dock::displayed_tags(&view), ["travel"]);
}

/// "keeps tag editing disabled after details failure and retries loading separately"
#[test]
fn keeps_tag_editing_disabled_after_details_failure_and_retries_loading_separately() {
    let mut fx = Fx::new(2);
    fx.backend
        .on_get_asset_details(|_| Err(BackendError::rejected("read")));
    fx.select(&[1]);
    assert!(Dock::tag_controls_disabled(&fx.runtime().bulk()));
    assert!(fx.runtime().bulk().tag_details_failed());
    let reads = fx.count("get_asset_details");
    fx.bulk(BulkCommand::RetryTagDetails);
    fx.settle();
    assert_eq!(fx.count("get_asset_details"), reads + 1);
}

/// "shows the assigned-tag loading state"
#[test]
fn shows_the_assigned_tag_loading_state() {
    let mut fx = Fx::new(2);
    fx.click(1);
    let view = fx.runtime().bulk();
    assert!(view.tag_details_loading());
    assert!(Dock::displayed_tags(&view).is_empty());
    assert!(Dock::tag_controls_disabled(&view));
}

// ---------------------------------------------------------------------------
// BulkDock.test.tsx: "bulk group sorting modal" (the dock's state; focus, Escape and
// the closing frame are checked in the window tests)
// ---------------------------------------------------------------------------

fn open_sorter() -> (Fx, Dock) {
    let (mut fx, mut dock) = sorting();
    dock.open_order_modal(&fx.runtime().bulk());
    fx.sync_dock(&mut dock);
    assert!(dock.order_modal_open(&fx.runtime().bulk()));
    (fx, dock)
}

/// "keeps keyboard changes local, discards on Cancel and restores trigger focus"
/// (the state half)
#[test]
fn keeps_modal_changes_local_and_discards_them_on_cancel() {
    let (mut fx, mut dock) = open_sorter();
    dock.set_modal_order(vec![2, 1, 3]);
    assert_eq!(dock.inline_order(), [1, 2, 3]);
    assert!(fx.group_calls().is_empty());
    dock.close_order_modal(&fx.runtime().bulk());
    fx.sync_dock(&mut dock);
    assert!(!dock.order_modal_open(&fx.runtime().bulk()));
    dock.open_order_modal(&fx.runtime().bulk());
    fx.sync_dock(&mut dock);
    assert_eq!(dock.modal_order(), [1, 2, 3], "a fresh draft");
}

/// "saves the exact modal draft, blocks duplicate submissions and dismissal while
/// saving"
#[test]
fn saves_the_exact_modal_draft_and_blocks_duplicates_and_dismissal_while_saving() {
    let (mut fx, mut dock) = open_sorter();
    dock.set_modal_order(vec![2, 1, 3]);
    let effects = dock.save_order_modal(&fx.runtime().bulk());
    assert!(matches!(
        effects.as_slice(),
        [DockEffect::Bulk(BulkCommand::ApplyGroup { order: Some(order), .. })] if order == &[2, 1, 3]
    ));
    fx.run_dock(effects);
    fx.sync_dock(&mut dock);
    assert!(dock.modal_saving());
    assert!(dock.modal_locked(&fx.runtime().bulk()));
    assert!(dock.save_order_modal(&fx.runtime().bulk()).is_empty());
    dock.close_order_modal(&fx.runtime().bulk());
    fx.sync_dock(&mut dock);
    assert!(dock.order_modal_open(&fx.runtime().bulk()), "no dismissal");
    fx.settle();
    fx.sync_dock(&mut dock);
    assert!(
        !dock.order_modal_open(&fx.runtime().bulk()),
        "saved: closed"
    );
    assert_eq!(fx.group_calls().len(), 1);
}

/// "retains the draft after failure and partial saves, then retries"
#[test]
fn retains_the_modal_draft_after_failure_and_partial_saves_then_retries() {
    let (mut fx, mut dock) = open_sorter();
    let mut saves = 0;
    fx.backend
        .on_set_assets_media_group_bulk(move |updates, key| {
            saves += 1;
            match saves {
                1 => rejected(),
                2 => Ok(BulkMediaGroupSummary {
                    processed_asset_ids: vec![updates[0].asset_id, updates[1].asset_id],
                    processed_assets: 2,
                    updated_assets: 2,
                    media_group_key: key,
                }),
                _ => Ok(grouped_all(updates, key)),
            }
        });
    dock.set_modal_order(vec![2, 1, 3]);
    let save = |fx: &mut Fx, dock: &mut Dock| {
        let effects = dock.save_order_modal(&fx.runtime().bulk());
        fx.run_dock(effects);
        fx.settle();
        fx.sync_dock(dock);
    };
    save(&mut fx, &mut dock);
    assert_eq!(dock.modal_result(), Some(GroupSaveResult::Failed));
    assert!(dock.order_modal_open(&fx.runtime().bulk()));
    assert_eq!(dock.modal_order(), [2, 1, 3]);
    save(&mut fx, &mut dock);
    assert_eq!(
        dock.modal_result(),
        Some(GroupSaveResult::Partial(PartialResult {
            processed: 2,
            requested: 3
        }))
    );
    assert!(dock.order_modal_open(&fx.runtime().bulk()));
    save(&mut fx, &mut dock);
    assert_eq!(
        fx.group_calls().last().map(|(updates, _)| updates
            .iter()
            .map(|update| update.asset_id)
            .collect::<Vec<_>>()),
        Some(vec![2, 1, 3])
    );
    assert!(!dock.order_modal_open(&fx.runtime().bulk()));
}

/// "ignores a retired group's save completion after another group opens"
#[test]
fn ignores_a_retired_groups_save_completion_after_another_group_opens() {
    let (mut fx, mut dock) = open_sorter();
    // The retired save fails in the backend: a failure keeps the model's draft, so the
    // view opened for the new key stays.
    fx.backend.on_set_assets_media_group_bulk(|_, _| rejected());
    let effects = dock.save_order_modal(&fx.runtime().bulk());
    let request = match effects.as_slice() {
        [DockEffect::Bulk(BulkCommand::ApplyGroup { request, .. })] => *request,
        other => panic!("one group write, not {other:?}"),
    };
    fx.run_dock(effects);
    fx.sync_dock(&mut dock);
    // The group changes while the save is on its way, and the view opens for it.
    fx.draft("other");
    fx.sync_dock(&mut dock);
    dock.open_order_modal(&fx.runtime().bulk());
    fx.sync_dock(&mut dock);
    assert!(dock.order_modal_open(&fx.runtime().bulk()));
    // The retired save's answer reaches the dock (the Solid test's controller keeps
    // the new key; the real model's answer is checked by the createBulk ports).
    dock.sync(
        &fx.runtime().bulk(),
        &[BulkEvent::GroupSettled {
            request,
            result: GroupSaveResult::Saved,
        }],
    );
    assert!(dock.order_modal_open(&fx.runtime().bulk()));
    assert!(!dock.modal_saving());
    fx.settle();
    fx.sync_dock(&mut dock);
    let view = fx.runtime().bulk();
    assert!(dock.order_modal_open(&view));
    assert!(!dock.modal_saving());
    assert_eq!(dock.modal_result(), None, "not this view's save");
    assert!(!dock.modal_save_disabled(&view));
}

/// "invalidates the modal when its group context changes"
#[test]
fn invalidates_the_modal_when_its_group_context_changes() {
    let (mut fx, mut dock) = open_sorter();
    fx.draft("other");
    fx.sync_dock(&mut dock);
    assert!(!dock.order_modal_open(&fx.runtime().bulk()));
    assert!(fx.group_calls().is_empty());
}

/// "disables opening until selected metadata is ready"
#[test]
fn disables_opening_until_selected_metadata_is_ready() {
    let mut fx = Fx::new(3000);
    fx.click(1);
    fx.click(2000);
    fx.draft("trip");
    assert!(fx.runtime().bulk().metadata_loading());
    assert!(Dock::open_order_modal_disabled(&fx.runtime().bulk()));
    fx.settle();
    assert!(!Dock::open_order_modal_disabled(&fx.runtime().bulk()));
}

/// "keeps the active modal save when shared metadata changes during acknowledgement"
#[test]
fn keeps_the_active_modal_save_when_shared_metadata_changes_during_acknowledgement() {
    let (mut fx, mut dock) = open_sorter();
    dock.set_modal_order(vec![2, 1, 3]);
    let effects = dock.save_order_modal(&fx.runtime().bulk());
    fx.run_dock(effects);
    // The write publishes the group on the selected assets before its answer reaches
    // the dock.
    fx.settle();
    assert!(fx
        .runtime()
        .bulk()
        .selected_assets()
        .iter()
        .all(|asset| asset.media_group_key.as_deref() == Some("trip")));
    assert_eq!(dock.modal_order(), [2, 1, 3]);
    fx.sync_dock(&mut dock);
    assert!(!dock.order_modal_open(&fx.runtime().bulk()));
}
