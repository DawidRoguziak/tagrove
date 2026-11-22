//! The behaviour tests of the lightbox model (epic E15), ported from the Solid suite:
//! \`src/features/lightbox/__tests__/createLightbox.test.ts\` (the model in the
//! application runtime against the scripted backend on the deterministic executor),
//! \`presentation.test.ts\` (the side panel, the control fade and the group-name copy),
//! and the model halves of \`LightboxModal.test.tsx\` (the delete confirmation) and
//! \`MediaGroupSetter.test.tsx\` (the order draft). Each test names the TS case it
//! ports in its doc comment.
//!
//! A pending promise of the Solid suite is a backend call that waits on the
//! \`ManualExecutor\`: the test decides which call answers first (\`run_nth\`), and a
//! handler scripted after the call was made decides what it answers. Nothing here
//! touches the file system: deletions are answered by the fake.

use std::{sync::Arc, time::Duration};

use media_tagger_client::{
    backend::{fake::Call, BackendError, FakeBackend, SelectionSpec},
    exec::{Executor, ManualExecutor},
    i18n::{I18n, Language},
    library::{
        AssetPatch, LibraryEvent, LibraryFilters, LibraryInput, LibraryReset, Mutation,
        MutationError, QueryInput, SummaryPatch, TagEdit, Ticket,
    },
    lightbox::{
        Activity, DeleteConfirm, DeletedSource, GroupClipboard, LightboxCommand, LightboxEvent,
        LightboxView, NavigationStatus, Sidebar,
    },
    model::Driver,
    prefs::MemoryPrefsStore,
    runtime::{AppRuntime, RuntimeConfig, RuntimeInput, RuntimeRunner},
    theme::ThemeName,
};
use media_tagger_core::models::{
    AssetDetails, AssetQueryPageResult, AssetQueryPositionResult, AssetSummary, AssetTagResult,
    BulkTagMergeSummary, DeleteAssetSummary, DeleteSourceStatus, SelectionMutationResult,
    SetAssetTagsSummary, StartAssetQueryResult, TagListPage, TagQueryImpact,
};

/// Rows of the first page the scripted query answers with (the query's page size).
const PAGE: usize = 128;

fn summary(id: i64) -> AssetSummary {
    AssetSummary {
        id,
        file_name: format!("{id}.png"),
        preview_path: None,
        kind: "image".into(),
        modified_at: 1,
        width: Some(100),
        height: Some(80),
        duration_ms: None,
        thumb_version: "0123456789abcdef".into(),
        is_favorite: false,
        media_group_key: None,
        media_group_order: None,
    }
}

fn list(tags: &[&str]) -> Vec<String> {
    tags.iter().map(|tag| (*tag).to_owned()).collect()
}

/// \`asset(id)\` of the Solid fixtures: tagged \`cat\`, path \`/synthetic/<id>.png\`.
fn details(id: i64, tags: &[&str]) -> AssetDetails {
    AssetDetails {
        summary: summary(id),
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

/// A merge whose canonical result for every asset is \`tags\`.
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
                tags: tags.clone(),
            })
            .collect(),
        revision: 2,
    }
}

fn started(ids: &[i64], total: usize) -> StartAssetQueryResult {
    StartAssetQueryResult::Ready {
        session_id: 1,
        revision: 1,
        total,
        offset: 0,
        items: ids.iter().copied().map(summary).collect(),
    }
}

fn page_of(
    total: usize,
) -> impl FnMut(u64, usize, usize) -> Result<AssetQueryPageResult, BackendError> {
    move |session_id, offset, limit| {
        Ok(AssetQueryPageResult::Ready {
            session_id,
            revision: 1,
            total,
            offset,
            items: (offset..(offset + limit).min(total))
                .map(|index| summary(index as i64 + 1))
                .collect(),
        })
    }
}

fn deleted() -> DeleteAssetSummary {
    DeleteAssetSummary {
        removed_assets: 1,
        removed_thumbnails: 0,
        source_status: DeleteSourceStatus::Deleted,
        revision: 2,
        recovery_path: None,
    }
}

fn range_write_result(is_favorite: Option<bool>) -> SelectionMutationResult {
    SelectionMutationResult::Ready {
        processed_assets: 3,
        updated_assets: 3,
        revision: 2,
        query_impact: TagQueryImpact::None,
        vocabulary_changed: false,
        is_favorite,
        media_group_key: None,
    }
}

fn first_three() -> SelectionSpec {
    SelectionSpec {
        session_id: 1,
        ranges: vec![[0, 2]],
        excluded: Vec::new(),
        included: Vec::new(),
    }
}

struct Fx {
    backend: Arc<FakeBackend>,
    executor: Arc<ManualExecutor>,
    driver: Driver<AppRuntime, RuntimeRunner>,
}

impl Fx {
    /// The \`beforeEach\` of the Solid suite for a query of \`total\` results (asset ID =
    /// index + 1, each tagged \`cat\`), the library loaded, the lightbox closed.
    fn new(total: usize) -> Self {
        let backend = Arc::new(FakeBackend::new());
        backend
            .on_start_asset_query(move |_| {
                let ids: Vec<i64> = (1..=total.min(PAGE) as i64).collect();
                Ok(started(&ids, total))
            })
            .on_get_asset_query_page(page_of(total))
            .on_get_asset_query_position(|_, asset_id| {
                Ok(AssetQueryPositionResult::Resolved {
                    index: (asset_id - 1) as usize,
                })
            })
            .on_get_asset_details(|id| Ok(Some(details(id, &["cat"]))))
            .on_list_tags(|_, _, _| {
                Ok(TagListPage {
                    items: list(&["cat", "dog"]),
                    total: 2,
                })
            })
            .on_set_asset_tags(|id, tags| Ok(saved(id, tags)))
            .on_merge_asset_tags_bulk(|ids, tags| Ok(merged(ids, tags)))
            .on_set_asset_favorite(|_, _| Ok(()))
            .on_set_asset_media_group(|_, _, _| Ok(()))
            .on_apply_selection_mutation(|_, _| Ok(range_write_result(None)))
            .on_delete_asset(|_| Ok(deleted()));
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
        };
        fx.send(RuntimeInput::RefreshLibrary);
        fx.settle();
        fx
    }

    /// \`setup()\` of the Solid suite: the library loaded and asset 1 opened with its
    /// details and neighbours read.
    fn opened(total: usize) -> Self {
        let mut fx = Self::new(total);
        fx.open(1);
        fx
    }

    fn runtime(&self) -> &AppRuntime {
        self.driver.model()
    }

    fn send(&mut self, input: RuntimeInput) {
        self.driver.handle(input);
    }

    fn lightbox(&mut self, command: LightboxCommand) {
        self.send(RuntimeInput::lightbox(command));
    }

    fn select(&mut self, id: i64) {
        self.lightbox(LightboxCommand::Select {
            asset: Box::new(summary(id)),
            index: None,
        });
    }

    fn select_at(&mut self, id: i64, index: usize) {
        self.lightbox(LightboxCommand::Select {
            asset: Box::new(summary(id)),
            index: Some(index),
        });
    }

    fn open(&mut self, id: i64) {
        self.select(id);
        self.settle();
    }

    fn close(&mut self) {
        self.lightbox(LightboxCommand::Close);
    }

    fn next(&mut self) {
        self.lightbox(LightboxCommand::Next);
    }

    fn previous(&mut self) {
        self.lightbox(LightboxCommand::Previous);
    }

    fn save(&mut self, tags: &[&str]) {
        self.lightbox(LightboxCommand::SaveTags(list(tags)));
    }

    /// A write of another owner of the library (the bulk dock, the tag editor).
    fn external(&mut self, ticket: u64, mutation: Mutation) {
        self.send(RuntimeInput::library(LibraryInput::Mutate {
            ticket: Ticket(ticket),
            mutation,
        }));
    }

    fn refresh_query(&mut self) {
        self.send(RuntimeInput::Query(QueryInput::Refresh));
    }

    /// Runs every waiting call and every reply, including the ones they start.
    fn settle(&mut self) {
        while self.executor.run_all() > 0 || self.driver.pump() > 0 {}
    }

    /// As [\`settle\`](Self::settle), but the first \`keep\` waiting calls keep waiting.
    fn settle_holding(&mut self, keep: usize) {
        loop {
            while self.driver.pump() > 0 {}
            if self.executor.pending() <= keep {
                break;
            }
            assert!(self.executor.run_nth(keep));
        }
    }

    /// Runs the waiting call at \`index\` and hands its reply to the runtime; whatever
    /// that starts waits.
    fn answer(&mut self, index: usize) {
        assert!(self.executor.run_nth(index), "a call waits at {index}");
        while self.driver.pump() > 0 {}
    }

    fn view(&self) -> LightboxView<'_> {
        self.runtime().lightbox()
    }

    fn selected_id(&self) -> Option<i64> {
        self.view().selected().map(|selected| selected.id())
    }

    fn path(&self) -> Option<String> {
        self.view()
            .selected()
            .and_then(|selected| selected.path.clone())
    }

    fn favorite(&self) -> bool {
        self.view()
            .selected()
            .is_some_and(|selected| selected.summary.is_favorite)
    }

    fn editor(&self) -> Vec<String> {
        self.view().tag_editor().to_vec()
    }

    fn count(&self, method: &str) -> usize {
        self.backend
            .calls()
            .iter()
            .filter(|call| call.method() == method)
            .count()
    }

    fn detail_reads(&self, id: i64) -> usize {
        self.backend
            .calls()
            .iter()
            .filter(|call| matches!(call, Call::GetAssetDetails { asset_id } if *asset_id == id))
            .count()
    }

    fn detail_ids(&self) -> Vec<i64> {
        let mut ids: Vec<i64> = self
            .backend
            .calls()
            .into_iter()
            .filter_map(|call| match call {
                Call::GetAssetDetails { asset_id } => Some(asset_id),
                _ => None,
            })
            .collect();
        ids.sort_unstable();
        ids
    }

    fn tag_saves(&self) -> Vec<(i64, Vec<String>)> {
        self.backend
            .calls()
            .into_iter()
            .filter_map(|call| match call {
                Call::SetAssetTags { asset_id, tags } => Some((asset_id, tags)),
                _ => None,
            })
            .collect()
    }

    fn favorite_saves(&self) -> Vec<(i64, bool)> {
        self.backend
            .calls()
            .into_iter()
            .filter_map(|call| match call {
                Call::SetAssetFavorite {
                    asset_id,
                    is_favorite,
                } => Some((asset_id, is_favorite)),
                _ => None,
            })
            .collect()
    }

    fn group_saves(&self) -> Vec<(i64, Option<String>, Option<f64>)> {
        self.backend
            .calls()
            .into_iter()
            .filter_map(|call| match call {
                Call::SetAssetMediaGroup {
                    asset_id,
                    media_group_key,
                    media_group_order,
                } => Some((asset_id, media_group_key, media_group_order)),
                _ => None,
            })
            .collect()
    }

    fn events(&mut self) -> Vec<LightboxEvent> {
        self.driver.model_mut().take_lightbox_events()
    }

    /// \`failedBoundary\`: 260 results, every page after the first fails; asset 128 at
    /// index 127 is open and Next has failed on the poisoned page.
    fn failed_boundary() -> Self {
        let mut fx = Self::new(260);
        fx.backend
            .on_get_asset_query_page(|_, _, _| Err(BackendError::rejected("page unavailable")));
        fx.select_at(128, 127);
        fx.settle();
        assert!(fx.runtime().query().load_error().is_some());
        fx.next();
        fx.settle();
        assert_eq!(fx.view().navigation_status(), NavigationStatus::Failed);
        assert_eq!(fx.selected_id(), Some(128));
        assert_eq!(fx.count("get_asset_query_page"), 1);
        fx
    }
}

// ---------------------------------------------------------------------------
// createLightbox.test.ts: "Solid lightbox selection and metadata"
// ---------------------------------------------------------------------------

/// "opens a summary immediately and blocks replacements until full tags arrive"
#[test]
fn opens_a_summary_immediately_and_blocks_replacements_until_full_tags_arrive() {
    let mut fx = Fx::new(3);
    fx.select(1);
    fx.save(&["unsafe"]);
    assert_eq!(fx.selected_id(), Some(1));
    assert_eq!(fx.path(), None);
    assert!(fx.view().tag_details_loading());
    assert!(fx.tag_saves().is_empty());

    fx.settle();
    assert_eq!(fx.editor(), ["cat"]);
    assert!(!fx.view().tag_editing_disabled());
    assert!(
        fx.tag_saves().is_empty(),
        "the early edit was refused whole"
    );
}

fn sends_no_mutation_without_a_selection(opened: bool) {
    let mut fx = if opened { Fx::opened(3) } else { Fx::new(3) };
    fx.close();
    fx.save(&["ignored"]);
    fx.lightbox(LightboxCommand::ToggleFavorite);
    fx.lightbox(LightboxCommand::SaveMediaGroup);
    fx.lightbox(LightboxCommand::DeleteSelected { request: 1 });
    fx.settle();
    assert_eq!(
        fx.events(),
        [LightboxEvent::DeleteSettled {
            request: 1,
            result: Ok(None),
        }]
    );
    for method in [
        "set_asset_tags",
        "set_asset_favorite",
        "set_asset_media_group",
        "delete_asset",
    ] {
        assert_eq!(fx.count(method), 0, "{method}");
    }
}

/// "sends no mutation without a selection, previously opened=false"
#[test]
fn sends_no_mutation_without_a_selection_never_opened() {
    sends_no_mutation_without_a_selection(false);
}

/// "sends no mutation without a selection, previously opened=true"
#[test]
fn sends_no_mutation_without_a_selection_previously_opened() {
    sends_no_mutation_without_a_selection(true);
}

/// "ignores details after selection changes and close" (the select -> select and
/// select -> close races)
#[test]
fn ignores_details_after_selection_changes_and_close() {
    let mut fx = Fx::new(3);
    fx.select(1);
    fx.select(2);
    assert_eq!(fx.executor.pending(), 2);
    fx.answer(0);
    assert_eq!(fx.selected_id(), Some(2));
    assert_eq!(fx.path(), None, "the details of asset 1 were not published");

    fx.close();
    fx.settle();
    assert_eq!(fx.selected_id(), None);
    assert!(!fx.view().is_open());
}

/// "prefetches only adjacent details and reuses complete cached records"
#[test]
fn prefetches_only_adjacent_details_and_reuses_complete_cached_records() {
    let mut fx = Fx::opened(3);
    assert_eq!(fx.detail_ids(), [1, 2, 3]);
    fx.close();
    fx.select(2);
    assert_eq!(fx.path().as_deref(), Some("/synthetic/2.png"));
    assert_eq!(fx.count("get_asset_details"), 3);
}

/// "does not prefetch in a single-result query"
#[test]
fn does_not_prefetch_in_a_single_result_query() {
    let fx = Fx::opened(1);
    assert_eq!(fx.count("get_asset_details"), 1);
}

/// "keeps prefetching neighbors when navigation selects cached details"
#[test]
fn keeps_prefetching_neighbors_when_navigation_selects_cached_details() {
    let mut fx = Fx::opened(5);
    assert_eq!(fx.detail_ids(), [1, 2, 5]);

    fx.next();
    fx.settle();
    assert_eq!(fx.selected_id(), Some(2));
    assert!(fx.runtime().library().cached_details(3).is_some());
    assert_eq!(fx.detail_ids(), [1, 2, 3, 5]);

    fx.next();
    fx.settle();
    assert_eq!(fx.selected_id(), Some(3));
    assert!(fx.runtime().library().cached_details(4).is_some());
    assert_eq!(fx.detail_ids(), [1, 2, 3, 4, 5]);
}

/// "keeps failed details and failed saves independent". The Rust library refuses a
/// tag replacement while the tag list is unknown, so the other writer that makes the
/// list known is a merge whose canonical result is \`bulk\`.
#[test]
fn keeps_failed_details_and_failed_saves_independent() {
    let mut fx = Fx::new(3);
    fx.backend
        .on_get_asset_details(|_| Err(BackendError::rejected("read failed")));
    fx.open(1);
    assert!(fx.view().asset_details_failed());
    assert!(fx.view().tag_details_failed());

    fx.backend
        .on_merge_asset_tags_bulk(|ids, _| Ok(merged(ids, list(&["bulk"]))));
    fx.external(
        5,
        Mutation::MergeTags {
            asset_ids: vec![1],
            tags: list(&["bulk"]),
        },
    );
    fx.settle();
    assert!(fx.view().asset_details_failed());
    assert!(!fx.view().tag_details_failed());
    assert!(!fx.view().tag_editing_disabled());

    fx.backend
        .on_set_asset_tags(|_, _| Err(BackendError::rejected("write failed")));
    fx.save(&["bulk", "draft"]);
    fx.settle();
    assert!(fx.view().tag_failed());
    assert_eq!(fx.editor(), ["bulk", "draft"]);

    fx.backend.on_set_asset_tags(|id, tags| Ok(saved(id, tags)));
    fx.backend
        .on_get_asset_details(|id| Ok(Some(details(id, &["bulk"]))));
    fx.lightbox(LightboxCommand::RetryDetails);
    fx.settle();
    assert!(!fx.view().asset_details_failed());
    assert!(fx.view().tag_failed(), "a details retry is no tag retry");

    fx.lightbox(LightboxCommand::RetryTags);
    fx.settle();
    assert!(!fx.view().tag_failed());
    assert_eq!(fx.editor(), ["bulk", "draft"]);
}

/// "coalesces same-turn saves and serializes across close and reopen"
#[test]
fn coalesces_same_turn_saves_and_serializes_across_close_and_reopen() {
    let mut fx = Fx::opened(3);
    fx.save(&["cat", "a"]);
    fx.save(&["cat", "a", "b"]);
    fx.save(&["cat", "final"]);
    fx.close();
    fx.select(1);
    // Only the first write was sent; it still waits (index 0). The reopened asset may
    // read its details meanwhile.
    fx.settle_holding(1);
    assert!(fx.tag_saves().is_empty());
    assert!(fx.view().tag_saving());

    fx.answer(0);
    fx.settle();
    assert_eq!(
        fx.tag_saves(),
        [(1, list(&["cat", "a"])), (1, list(&["cat", "final"]))]
    );
    assert_eq!(fx.editor(), ["cat", "final"]);
}

/// "retains the newest queued replacement after failure"
#[test]
fn retains_the_newest_queued_replacement_after_failure() {
    let mut fx = Fx::opened(3);
    fx.backend
        .on_set_asset_tags(|_, _| Err(BackendError::rejected("failed")));
    fx.save(&["a"]);
    fx.save(&["b"]);
    fx.settle();
    assert!(fx.view().tag_failed());
    assert_eq!(fx.editor(), ["b"]);
    assert_eq!(fx.tag_saves().len(), 1);

    fx.backend.on_set_asset_tags(|id, tags| Ok(saved(id, tags)));
    fx.close();
    fx.select(1);
    fx.lightbox(LightboxCommand::RetryTags);
    fx.settle();
    assert_eq!(fx.tag_saves().last(), Some(&(1, list(&["b"]))));
}

/// "queues behind an external writer and rebases its intent onto the winner without
/// Retry"
#[test]
fn queues_behind_an_external_writer_and_rebases_its_intent_onto_the_winner_without_retry() {
    let mut fx = Fx::opened(3);
    fx.external(
        7,
        Mutation::SaveTags {
            asset_id: 1,
            edit: TagEdit::Replace(list(&["cat", "external"])),
        },
    );
    fx.save(&["mine"]);
    assert!(!fx.view().tag_failed());
    assert!(fx.view().tag_saving());
    assert_eq!(fx.executor.pending(), 1, "only the external write was sent");

    fx.settle();
    assert_eq!(
        fx.tag_saves(),
        [
            (1, list(&["cat", "external"])),
            (1, list(&["external", "mine"]))
        ]
    );
    assert!(!fx.view().tag_failed());
    assert_eq!(fx.editor(), ["external", "mine"]);
}

/// "queues a favorite behind a pending tag save and publishes both in click order"
#[test]
fn queues_a_favorite_behind_a_pending_tag_save_and_publishes_both_in_click_order() {
    let mut fx = Fx::opened(3);
    fx.save(&["cat", "dog"]);
    fx.lightbox(LightboxCommand::ToggleFavorite);
    assert!(fx.view().favorite_pending());
    assert_eq!(
        fx.executor.pending(),
        1,
        "the favorite waits for the tag save"
    );
    assert!(fx.favorite_saves().is_empty());

    fx.settle();
    let methods: Vec<&str> = fx
        .backend
        .calls()
        .iter()
        .map(Call::method)
        .filter(|method| *method == "set_asset_tags" || *method == "set_asset_favorite")
        .collect();
    assert_eq!(methods, ["set_asset_tags", "set_asset_favorite"]);
    assert_eq!(fx.favorite_saves(), [(1, true)]);
    assert!(!fx.view().favorite_failed());
    assert!(!fx.view().favorite_pending());
    assert!(!fx.view().tag_failed());
    assert!(fx.favorite());
    assert_eq!(fx.editor(), ["cat", "dog"]);
}

/// "retains retry intent while maintenance denies writes"
#[test]
fn retains_retry_intent_while_maintenance_denies_writes() {
    let mut fx = Fx::opened(3);
    fx.send(RuntimeInput::library(LibraryInput::BeginMaintenance {
        ticket: Ticket(50),
    }));
    fx.settle();
    let events = fx.driver.model_mut().take_library_events();
    assert!(events
        .iter()
        .any(|event| matches!(event, LibraryEvent::MaintenanceReady { ticket: Ticket(50) })));

    fx.save(&["draft"]);
    fx.settle();
    assert!(fx.view().tag_failed());
    assert!(fx.tag_saves().is_empty());

    fx.send(RuntimeInput::library(LibraryInput::EndMaintenance {
        ticket: Ticket(50),
        reset: None,
    }));
    fx.lightbox(LightboxCommand::RetryTags);
    fx.settle();
    assert_eq!(fx.editor(), ["draft"]);
    assert_eq!(fx.tag_saves(), [(1, list(&["draft"]))]);
    assert!(!fx.view().tag_failed());
}

/// "re-reads complete details that predate a favorite mutation"
#[test]
fn re_reads_complete_details_that_predate_a_favorite_mutation() {
    let mut fx = Fx::new(3);
    fx.select(1);
    assert_eq!(fx.executor.pending(), 1);
    fx.external(
        2,
        Mutation::SetFavorite {
            asset_id: 1,
            favorite: true,
        },
    );
    // The write answers first, then the stale details.
    fx.answer(1);
    fx.backend.on_get_asset_details(|id| {
        let mut details = details(id, &["cat"]);
        details.summary.is_favorite = true;
        Ok(Some(details))
    });
    fx.answer(0);
    fx.settle();
    assert!(fx.favorite());
    assert_eq!(fx.path().as_deref(), Some("/synthetic/1.png"));
    assert_eq!(fx.detail_reads(1), 2);
}

/// "waits for a pending write before refetching details"
#[test]
fn waits_for_a_pending_write_before_refetching_details() {
    let mut fx = Fx::new(3);
    fx.select(1);
    fx.external(
        2,
        Mutation::SetFavorite {
            asset_id: 1,
            favorite: true,
        },
    );
    assert_eq!(fx.executor.pending(), 2);
    // The details answer while the write still runs.
    fx.answer(0);
    assert_eq!(fx.detail_reads(1), 1, "no read again while the write runs");

    fx.backend.on_get_asset_details(|id| {
        let mut details = details(id, &["cat"]);
        details.summary.is_favorite = true;
        Ok(Some(details))
    });
    fx.settle();
    assert!(fx.favorite());
}

/// "preserves empty canonical tags and cached favorite changes on reopen"
#[test]
fn preserves_empty_canonical_tags_and_cached_favorite_changes_on_reopen() {
    let mut fx = Fx::opened(3);
    fx.external(
        2,
        Mutation::SetFavorite {
            asset_id: 1,
            favorite: true,
        },
    );
    fx.settle();
    fx.save(&[]);
    fx.settle();
    fx.close();
    fx.select(1);
    assert!(fx.editor().is_empty());
    assert!(fx.favorite());
}

/// "closes on identity reset and never publishes an obsolete save"
#[test]
fn closes_on_identity_reset_and_never_publishes_an_obsolete_save() {
    let mut fx = Fx::opened(3);
    fx.save(&["old"]);
    assert_eq!(fx.executor.pending(), 1);
    fx.send(RuntimeInput::library(LibraryInput::Reset(
        LibraryReset::Identity,
    )));
    assert_eq!(fx.selected_id(), None);

    fx.backend
        .on_get_asset_details(|id| Ok(Some(details(id, &["new"]))));
    fx.select(1);
    fx.settle_holding(1);
    // The old write answers last.
    fx.settle();
    assert_eq!(fx.editor(), ["new"]);
    assert!(!fx.view().tag_saving());
    assert!(!fx.view().tag_failed());
}

/// "preserves independently dirty group fields through hydration and tag changes"
#[test]
fn preserves_independently_dirty_group_fields_through_hydration_and_tag_changes() {
    let mut fx = Fx::new(3);
    fx.backend.on_get_asset_details(|id| {
        let mut details = details(id, &["cat"]);
        details.summary.media_group_key = Some("canonical".into());
        details.summary.media_group_order = Some(4.0);
        Ok(Some(details))
    });
    fx.select(1);
    fx.lightbox(LightboxCommand::SetMediaGroupKeyDraft("draft".into()));
    fx.settle();
    assert_eq!(fx.view().media_group_key_editor(), "draft");
    assert_eq!(fx.view().media_group_order_editor(), "4");

    fx.lightbox(LightboxCommand::SetMediaGroupOrderDraft("07".into()));
    fx.save(&["new"]);
    fx.settle();
    assert_eq!(fx.view().media_group_key_editor(), "draft");
    assert_eq!(fx.view().media_group_order_editor(), "07");

    fx.lightbox(LightboxCommand::SaveMediaGroup);
    fx.settle();
    assert_eq!(
        fx.group_saves().last(),
        Some(&(1, Some("draft".to_owned()), Some(7.0)))
    );
}

/// "queues repeated metadata commands, stays pending until the last settles and
/// reports failures"
#[test]
fn queues_repeated_metadata_commands_stays_pending_until_the_last_settles_and_reports_failures() {
    let mut fx = Fx::opened(3);
    fx.lightbox(LightboxCommand::ToggleFavorite);
    fx.lightbox(LightboxCommand::ToggleFavorite);
    assert_eq!(fx.executor.pending(), 1);
    fx.answer(0);
    assert_eq!(fx.favorite_saves(), [(1, true)]);
    assert!(fx.view().favorite_pending());

    fx.settle();
    // The second toggle queued behind the first and reversed it.
    assert_eq!(fx.favorite_saves(), [(1, true), (1, false)]);
    assert!(!fx.favorite());
    assert!(!fx.view().favorite_pending());
    assert!(!fx.view().favorite_failed());

    fx.external(
        8,
        Mutation::MergeTags {
            asset_ids: vec![1],
            tags: list(&["cat"]),
        },
    );
    fx.backend
        .on_set_asset_media_group(|_, _, _| Err(BackendError::rejected("locked")));
    fx.lightbox(LightboxCommand::SaveMediaGroup);
    assert!(fx.view().group_pending());
    assert!(fx.group_saves().is_empty());
    assert_eq!(
        fx.executor.pending(),
        1,
        "the group write waits for the merge"
    );

    fx.settle();
    assert_eq!(fx.group_saves().len(), 1);
    assert!(fx.view().group_failed());
    assert!(!fx.view().group_pending());
}

/// "keeps a saved favorite removed when query refresh fails and retries only the
/// query"
#[test]
fn keeps_a_saved_favorite_removed_when_query_refresh_fails_and_retries_only_the_query() {
    let mut fx = Fx::opened(3);
    fx.external(
        2,
        Mutation::SetFavorite {
            asset_id: 1,
            favorite: true,
        },
    );
    fx.settle();
    fx.backend.on_start_asset_query(|_| {
        let mut row = summary(1);
        row.is_favorite = true;
        Ok(StartAssetQueryResult::Ready {
            session_id: 2,
            revision: 2,
            total: 1,
            offset: 0,
            items: vec![row],
        })
    });
    fx.send(RuntimeInput::Query(QueryInput::ApplyFilters(
        LibraryFilters {
            favorites_only: true,
            ..LibraryFilters::default()
        },
    )));
    fx.settle();
    assert_eq!(fx.count("start_asset_query"), 2);
    assert!(fx.favorite());

    fx.backend
        .on_start_asset_query(|_| Err(BackendError::rejected("query failed")));
    fx.lightbox(LightboxCommand::ToggleFavorite);
    fx.settle();
    assert_eq!(
        fx.count("start_asset_query"),
        3,
        "unfavoriting refreshed the query"
    );
    assert_eq!(fx.view().navigation_status(), NavigationStatus::Failed);
    assert!(fx.runtime().query().load_error().is_some());
    assert!(!fx.favorite());
    assert!(!fx.view().favorite_pending());
    assert!(!fx.view().favorite_failed());

    fx.backend
        .on_start_asset_query(|_| Ok(started(&[], 0)))
        .on_get_asset_query_position(|_, _| Ok(AssetQueryPositionResult::Missing));
    fx.lightbox(LightboxCommand::RetryNavigation);
    fx.settle();
    assert_eq!(fx.view().navigation_status(), NavigationStatus::Missing);
    assert!(fx.runtime().query().load_error().is_none());
    assert!(!fx.favorite());
    assert_eq!(fx.favorite_saves(), [(1, true), (1, false)]);
}

/// "reports a genuine favorite write failure without changing saved state"
#[test]
fn reports_a_genuine_favorite_write_failure_without_changing_saved_state() {
    let mut fx = Fx::opened(3);
    fx.backend
        .on_set_asset_favorite(|_, _| Err(BackendError::rejected("write failed")));
    fx.lightbox(LightboxCommand::ToggleFavorite);
    fx.settle();
    assert!(!fx.view().favorite_pending());
    assert!(fx.view().favorite_failed());
    assert!(!fx.favorite());
    assert_eq!(fx.count("start_asset_query"), 1);
}

fn scopes_pending_favorites_to_identity(rejected: bool) {
    let mut fx = Fx::opened(3);
    fx.lightbox(LightboxCommand::ToggleFavorite);
    assert_eq!(fx.executor.pending(), 1, "the old write waits");
    fx.close();
    // Direct identity invalidation also has to tolerate a still-dispatched reply.
    fx.send(RuntimeInput::library(LibraryInput::Reset(
        LibraryReset::Restored,
    )));
    fx.refresh_query();
    fx.settle_holding(1);
    fx.select(1);
    fx.settle_holding(1);
    assert_eq!(fx.path().as_deref(), Some("/synthetic/1.png"));
    assert!(!fx.view().favorite_pending());

    fx.lightbox(LightboxCommand::ToggleFavorite);
    assert!(
        fx.favorite_saves().is_empty(),
        "the old write has not answered"
    );
    assert_eq!(fx.executor.pending(), 2, "the new write was sent at once");

    if rejected {
        fx.backend
            .on_set_asset_favorite(|_, _| Err(BackendError::rejected("obsolete failure")));
    }
    fx.answer(0);
    fx.backend.on_set_asset_favorite(|_, _| Ok(()));
    assert!(!fx.favorite());
    assert!(fx.view().favorite_pending());
    assert!(!fx.view().favorite_failed());

    fx.close();
    fx.select(1);
    assert!(fx.view().favorite_pending());
    // A repeated toggle queues behind the pending one in this identity and reverses it.
    fx.lightbox(LightboxCommand::ToggleFavorite);
    // Everything but the pending write (index 0) runs: the repeated toggle is not
    // among it, it waits for that write.
    fx.settle_holding(1);
    assert_eq!(
        fx.favorite_saves(),
        [(1, true)],
        "the repeated toggle waits"
    );
    fx.settle();
    assert_eq!(fx.favorite_saves(), [(1, true), (1, true), (1, false)]);
    assert!(!fx.view().favorite_pending());
    assert!(!fx.favorite());
}

/// "scopes pending favorites and late completion to identity, rejected=false"
#[test]
fn scopes_pending_favorites_and_late_completion_to_identity_resolved() {
    scopes_pending_favorites_to_identity(false);
}

/// "scopes pending favorites and late completion to identity, rejected=true"
#[test]
fn scopes_pending_favorites_and_late_completion_to_identity_rejected() {
    scopes_pending_favorites_to_identity(true);
}

/// "releases favorite presentation ownership across identity maintenance": the write
/// ends inside an identity maintenance barrier that resets the library.
#[test]
fn releases_favorite_presentation_ownership_across_identity_maintenance() {
    let mut fx = Fx::opened(3);
    fx.lightbox(LightboxCommand::ToggleFavorite);
    fx.close();
    fx.send(RuntimeInput::library(LibraryInput::BeginMaintenance {
        ticket: Ticket(60),
    }));
    fx.settle();
    fx.send(RuntimeInput::library(LibraryInput::EndMaintenance {
        ticket: Ticket(60),
        reset: Some(LibraryReset::Identity),
    }));
    fx.refresh_query();
    fx.settle();
    fx.select(1);
    fx.settle();
    assert_eq!(fx.path().as_deref(), Some("/synthetic/1.png"));
    assert!(!fx.view().favorite_pending());

    fx.lightbox(LightboxCommand::ToggleFavorite);
    fx.settle();
    assert_eq!(fx.favorite_saves().len(), 2);
    assert!(fx.favorite());
    assert!(!fx.view().favorite_pending());
}

/// "closes only the deleted selection and preserves a committed delete on refresh
/// failure"
#[test]
fn closes_only_the_deleted_selection_and_preserves_a_committed_delete_on_refresh_failure() {
    let mut fx = Fx::opened(3);
    fx.backend
        .on_start_asset_query(|_| Err(BackendError::rejected("refresh")));
    fx.lightbox(LightboxCommand::DeleteSelected { request: 3 });
    fx.settle();
    let events = fx.events();
    let [LightboxEvent::DeleteSettled {
        request: 3,
        result: Ok(Some(deleted)),
    }] = events.as_slice()
    else {
        panic!("{events:?}");
    };
    assert_eq!(deleted.source, DeletedSource::Deleted);
    assert_eq!(fx.selected_id(), None);
}

/// "keeps failed deletion selected"
#[test]
fn keeps_failed_deletion_selected() {
    let mut fx = Fx::opened(3);
    fx.backend
        .on_delete_asset(|_| Err(BackendError::rejected("preserved")));
    fx.lightbox(LightboxCommand::DeleteSelected { request: 4 });
    fx.settle();
    assert_eq!(
        fx.events(),
        [LightboxEvent::DeleteSettled {
            request: 4,
            result: Err(MutationError::Backend(BackendError::rejected("preserved"))),
        }]
    );
    assert_eq!(fx.selected_id(), Some(1));
}

/// "accumulates rapid navigation and rejects reverse-order responses": the two rows
/// lie on two pages that are not cached, and the later page answers first.
#[test]
fn accumulates_rapid_navigation_and_rejects_reverse_order_responses() {
    let mut fx = Fx::new(600);
    fx.select_at(383, 382);
    assert_eq!(fx.executor.pending(), 1, "the details read");
    fx.next();
    fx.next();
    assert_eq!(fx.executor.pending(), 3, "a page read for each press");
    assert_eq!(fx.selected_id(), Some(383));

    fx.answer(2);
    assert_eq!(fx.selected_id(), Some(385));
    fx.answer(1);
    fx.settle();
    assert_eq!(fx.selected_id(), Some(385));
    assert_eq!(fx.view().selected_index(), Some(384));
}

/// "rapid Right then Left returns to the original target": Right waits for a page,
/// Left finds its row cached; the late page selects nothing.
#[test]
fn rapid_right_then_left_returns_to_the_original_target() {
    let mut fx = Fx::new(600);
    fx.select_at(128, 127);
    fx.next();
    fx.previous();
    assert_eq!(fx.selected_id(), Some(128));
    fx.settle();
    assert_eq!(fx.selected_id(), Some(128));
    assert_eq!(fx.view().selected_index(), Some(127));
}

/// "wraps and rolls back failed navigation so the next key retries"
#[test]
fn wraps_and_rolls_back_failed_navigation_so_the_next_key_retries() {
    let mut fx = Fx::opened(3);
    fx.previous();
    fx.settle();
    assert_eq!(fx.selected_id(), Some(3));

    // A failed read on a wrap: 260 results, the last page cannot be read.
    let mut fx = Fx::new(260);
    fx.backend
        .on_get_asset_query_page(|_, _, _| Err(BackendError::rejected("page")));
    fx.select_at(1, 0);
    fx.settle();
    fx.previous();
    fx.settle();
    assert_eq!(fx.selected_id(), Some(1));
    assert_eq!(fx.view().navigation_status(), NavigationStatus::Failed);
    // The target rolled back: Next steps from the shown asset, not from 259.
    fx.next();
    fx.settle();
    assert_eq!(fx.selected_id(), Some(2));
    assert_eq!(fx.view().selected_index(), Some(1));
    assert_eq!(fx.view().navigation_status(), NavigationStatus::Ready);
}

/// "resolves a selected item beyond page one after query reorder"
#[test]
fn resolves_a_selected_item_beyond_page_one_after_query_reorder() {
    let mut fx = Fx::opened(3);
    fx.backend
        .on_start_asset_query(|_| Ok(started(&[9], 500)))
        .on_get_asset_query_page(page_of(500))
        .on_get_asset_query_position(|_, _| Ok(AssetQueryPositionResult::Resolved { index: 300 }));
    fx.refresh_query();
    fx.settle();
    assert_eq!(fx.view().navigation_status(), NavigationStatus::Ready);
    assert_eq!(fx.view().selected_index(), Some(300));

    fx.next();
    fx.settle();
    // The page answers row 302 at index 301.
    assert_eq!(fx.view().selected_index(), Some(301));
    assert_eq!(fx.selected_id(), Some(302));
}

/// "keeps missing selection open and retries failed position lookup"
#[test]
fn keeps_missing_selection_open_and_retries_failed_position_lookup() {
    let mut fx = Fx::opened(3);
    fx.backend
        .on_start_asset_query(|_| Ok(started(&[], 0)))
        .on_get_asset_query_position(|_, _| Ok(AssetQueryPositionResult::Missing));
    fx.refresh_query();
    fx.settle();
    assert_eq!(fx.view().navigation_status(), NavigationStatus::Missing);
    assert_eq!(fx.selected_id(), Some(1));

    fx.backend
        .on_get_asset_query_position(|_, _| Err(BackendError::rejected("position")));
    fx.lightbox(LightboxCommand::RetryNavigation);
    fx.settle();
    assert_eq!(fx.view().navigation_status(), NavigationStatus::Failed);

    fx.backend
        .on_start_asset_query(|_| Ok(started(&[1, 2, 3], 3)));
    fx.lightbox(LightboxCommand::RetryNavigation);
    fx.settle();
    assert_eq!(fx.view().navigation_status(), NavigationStatus::Ready);
}

/// "rejects retired position and navigation reads after close"
#[test]
fn rejects_retired_position_and_navigation_reads_after_close() {
    let mut fx = Fx::opened(3);
    fx.backend
        .on_start_asset_query(|_| Ok(started(&[9], 100)))
        .on_get_asset_query_position(|_, _| Ok(AssetQueryPositionResult::Resolved { index: 99 }));
    fx.refresh_query();
    fx.answer(0);
    // The position lookup was sent and has not answered yet.
    assert_eq!(fx.count("get_asset_query_position"), 0);
    assert!(fx.executor.pending() >= 1);
    fx.close();
    fx.settle();
    fx.next();
    fx.settle();
    assert_eq!(fx.selected_id(), None);
    assert_eq!(fx.view().selected_index(), None);
}

/// "drops query-scoped cached details and refetches on reselect"
#[test]
fn drops_query_scoped_cached_details_and_refetches_on_reselect() {
    let mut fx = Fx::opened(3);
    let count = fx.detail_reads(1);
    fx.refresh_query();
    fx.settle();
    fx.select(1);
    fx.settle();
    assert_eq!(fx.detail_reads(1), count + 1);
}

/// "does not publish details after owner disposal"
#[test]
fn does_not_publish_details_after_owner_disposal() {
    let mut fx = Fx::new(3);
    fx.select(1);
    fx.send(RuntimeInput::library(LibraryInput::Dispose));
    fx.settle();
    assert_eq!(fx.path(), None);
}

/// "crosses a real page boundary and wraps using the query total"
#[test]
fn crosses_a_real_page_boundary_and_wraps_using_the_query_total() {
    let mut fx = Fx::new(260);
    fx.select_at(128, 127);
    fx.settle();
    fx.next();
    fx.settle();
    assert_eq!(fx.selected_id(), Some(129));

    fx.select_at(1, 0);
    fx.previous();
    fx.settle();
    assert_eq!(fx.selected_id(), Some(260));
    assert!(fx.runtime().query().assets().count() <= 260);
}

/// "rolls rapid failed navigation back to the shown asset, not an uncommitted target"
#[test]
fn rolls_rapid_failed_navigation_back_to_the_shown_asset_not_an_uncommitted_target() {
    let mut fx = Fx::new(260);
    fx.backend
        .on_get_asset_query_page(|_, _, _| Err(BackendError::rejected("page unavailable")));
    fx.select_at(128, 127);
    fx.settle();
    assert!(fx.runtime().query().load_error().is_some());

    fx.next();
    fx.next();
    fx.settle();
    assert_eq!(fx.view().navigation_status(), NavigationStatus::Failed);
    assert_eq!(fx.selected_id(), Some(128));

    fx.next();
    fx.settle();
    assert_eq!(fx.view().navigation_status(), NavigationStatus::Failed);
    // From the shown index 127, not from the uncommitted 129: the cached index 126.
    fx.previous();
    fx.settle();
    assert_eq!(fx.selected_id(), Some(127));
    assert_eq!(fx.view().navigation_status(), NavigationStatus::Ready);
}

/// "explicitly retries a page poisoned by adjacent prefetch and keeps cached Previous
/// usable"
#[test]
fn explicitly_retries_a_page_poisoned_by_adjacent_prefetch_and_keeps_cached_previous_usable() {
    let mut fx = Fx::failed_boundary();
    fx.previous();
    fx.settle();
    assert_eq!(fx.selected_id(), Some(127));
    fx.next();
    fx.settle();
    assert_eq!(fx.selected_id(), Some(128));
    fx.next();
    fx.settle();
    assert_eq!(fx.view().navigation_status(), NavigationStatus::Failed);

    fx.lightbox(LightboxCommand::RetryNavigation);
    fx.settle();
    assert_eq!(fx.view().navigation_status(), NavigationStatus::Failed);
    assert_eq!(fx.count("get_asset_query_page"), 2);

    fx.backend.on_get_asset_query_page(page_of(260));
    fx.lightbox(LightboxCommand::RetryNavigation);
    fx.settle();
    assert_eq!(fx.selected_id(), Some(129));
    assert_eq!(fx.view().navigation_status(), NavigationStatus::Ready);
    assert!(fx.runtime().query().load_error().is_none());
    assert_eq!(fx.count("get_asset_query_page"), 3);
    assert_eq!(fx.count("start_asset_query"), 1);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RetryChange {
    Selection,
    Close,
    Refresh,
    Dispose,
}

fn rejects_an_obsolete_page_retry(change: RetryChange) {
    let mut fx = Fx::failed_boundary();
    fx.backend.on_get_asset_query_page(page_of(260));
    fx.lightbox(LightboxCommand::RetryNavigation);
    assert_eq!(fx.executor.pending(), 1, "the page retry waits");
    match change {
        RetryChange::Selection => fx.select_at(127, 126),
        RetryChange::Close => fx.close(),
        RetryChange::Refresh => {
            fx.refresh_query();
            fx.settle_holding(1);
        }
        RetryChange::Dispose => fx.send(RuntimeInput::library(LibraryInput::Dispose)),
    }
    fx.answer(0);
    fx.settle();
    let expected = match change {
        RetryChange::Selection => Some(127),
        RetryChange::Close => None,
        RetryChange::Refresh | RetryChange::Dispose => Some(128),
    };
    assert_eq!(fx.selected_id(), expected);
}

/// "rejects an obsolete page Retry after selection"
#[test]
fn rejects_an_obsolete_page_retry_after_selection() {
    rejects_an_obsolete_page_retry(RetryChange::Selection);
}

/// "rejects an obsolete page Retry after close"
#[test]
fn rejects_an_obsolete_page_retry_after_close() {
    rejects_an_obsolete_page_retry(RetryChange::Close);
}

/// "rejects an obsolete page Retry after refresh"
#[test]
fn rejects_an_obsolete_page_retry_after_refresh() {
    rejects_an_obsolete_page_retry(RetryChange::Refresh);
}

/// "rejects an obsolete page Retry after dispose"
#[test]
fn rejects_an_obsolete_page_retry_after_dispose() {
    rejects_an_obsolete_page_retry(RetryChange::Dispose);
}

/// "invalidates a navigation target immediately when refresh starts in the same turn"
#[test]
fn invalidates_a_navigation_target_immediately_when_refresh_starts_in_the_same_turn() {
    let mut fx = Fx::opened(3);
    fx.backend
        .on_start_asset_query(|_| Ok(started(&[3, 2, 1], 3)));
    fx.refresh_query();
    let waiting = fx.executor.pending();
    fx.next();
    assert_eq!(fx.executor.pending(), waiting, "nothing was read");
    assert_eq!(fx.selected_id(), Some(1));
    fx.settle();
    assert_eq!(fx.selected_id(), Some(1));

    fx.next();
    fx.settle();
    assert_eq!(fx.selected_id(), Some(3));
}

/// "re-reads details read during a range write so the tag editor and Retry work
/// afterwards"
#[test]
fn re_reads_details_read_during_a_range_write_so_the_tag_editor_and_retry_work_afterwards() {
    let mut fx = Fx::new(3);
    fx.external(
        3,
        Mutation::MergeTagsInSelection {
            selection: first_three(),
            tags: list(&["dog"]),
        },
    );
    assert_eq!(fx.count("apply_selection_mutation"), 0);
    assert_eq!(fx.executor.pending(), 1);
    // The asset opens and its details resolve while the write still runs.
    fx.select_at(2, 1);
    fx.settle_holding(1);
    assert_eq!(fx.count("apply_selection_mutation"), 0);
    assert!(!fx.view().tag_editing_disabled());
    assert_eq!(fx.detail_reads(2), 1);

    // An edit made now queues behind the write and fails once the write drops the
    // tag list.
    fx.save(&["cat", "fox"]);
    fx.backend
        .on_get_asset_details(|id| Ok(Some(details(id, &["cat", "dog"]))));
    fx.settle();
    assert_eq!(fx.detail_reads(2), 2);
    assert!(!fx.view().tag_editing_disabled());
    assert!(!fx.view().tag_details_loading());
    assert!(fx.view().tag_failed());
    assert_eq!(fx.editor(), ["cat", "dog", "fox"]);

    fx.lightbox(LightboxCommand::RetryTags);
    fx.settle();
    assert!(fx.tag_saves().contains(&(2, list(&["cat", "dog", "fox"]))));
    assert!(!fx.view().tag_failed());
    assert_eq!(fx.editor(), ["cat", "dog", "fox"]);
}

/// "shows tag loading during a range write on the open asset and re-reads once it
/// ends"
#[test]
fn shows_tag_loading_during_a_range_write_on_the_open_asset_and_re_reads_once_it_ends() {
    let mut fx = Fx::opened(3);
    let before = fx.detail_reads(1);
    fx.backend
        .on_apply_selection_mutation(|_, _| Ok(range_write_result(Some(true))));
    fx.external(
        3,
        Mutation::ToggleFavoritesInSelection {
            selection: first_three(),
        },
    );
    fx.settle_holding(1);
    assert!(fx.view().tag_editing_disabled());
    assert!(fx.view().tag_details_loading());
    assert_eq!(fx.detail_reads(1), before);

    fx.backend.on_get_asset_details(|id| {
        let mut details = details(id, &["cat", "dog"]);
        details.summary.is_favorite = true;
        Ok(Some(details))
    });
    fx.settle();
    assert_eq!(fx.detail_reads(1), before + 1);
    assert!(!fx.view().tag_editing_disabled());
    assert!(!fx.view().tag_details_loading());
    assert_eq!(fx.editor(), ["cat", "dog"]);
    assert!(fx.favorite());
}

/// \`raceRangeWriteBehindSave\`: a range write admitted behind save A drops the tag
/// lists before A's answer is applied, so the edit queued behind A has no base when
/// it would be sent.
fn race_range_write_behind_save(close_before_save: bool) -> Fx {
    let mut fx = Fx::opened(3);
    fx.save(&["cat", "a"]);
    fx.save(&["cat", "a", "b"]);
    assert_eq!(fx.executor.pending(), 1);
    if close_before_save {
        fx.close();
    }
    fx.external(
        3,
        Mutation::MergeTagsInSelection {
            selection: first_three(),
            tags: list(&["dog"]),
        },
    );
    assert_eq!(
        fx.executor.pending(),
        1,
        "the range write waits for the save"
    );
    fx.answer(0);
    assert_eq!(fx.tag_saves(), [(1, list(&["cat", "a"]))]);
    // The range write started before the queued edit could be sent.
    assert_eq!(fx.executor.pending(), 1);
    if !close_before_save {
        assert!(fx.view().tag_failed());
        assert!(!fx.view().tag_saving());
        assert_eq!(fx.editor(), ["cat", "a", "b"]);
    }
    fx.backend
        .on_get_asset_details(|id| Ok(Some(details(id, &["a", "cat", "dog"]))));
    fx.settle();
    assert_eq!(fx.count("apply_selection_mutation"), 1);
    fx
}

/// \`expectRetrySendsRebasedEdit\`
fn expect_retry_sends_rebased_edit(fx: &mut Fx) {
    assert_eq!(fx.tag_saves().len(), 1);
    assert!(fx.view().tag_failed());
    assert!(!fx.view().tag_saving());
    assert!(!fx.view().tag_editing_disabled());
    assert_eq!(fx.editor(), ["a", "cat", "dog", "b"]);

    fx.backend.on_set_asset_tags(|id, mut tags| {
        tags.sort();
        Ok(saved(id, tags))
    });
    fx.lightbox(LightboxCommand::RetryTags);
    assert_eq!(fx.executor.pending(), 1);
    // Not reported as saved until the backend confirms.
    assert!(fx.view().tag_saving());
    assert!(!fx.view().tag_failed());
    fx.settle();
    assert_eq!(
        fx.tag_saves(),
        [
            (1, list(&["cat", "a"])),
            (1, list(&["a", "cat", "dog", "b"]))
        ]
    );
    assert!(!fx.view().tag_saving());
    assert!(!fx.view().tag_failed());
    assert_eq!(fx.editor(), ["a", "b", "cat", "dog"]);
}

/// "fails a queued edit whose base a range write dropped before the previous save
/// settled"
#[test]
fn fails_a_queued_edit_whose_base_a_range_write_dropped_before_the_previous_save_settled() {
    let mut fx = race_range_write_behind_save(false);
    expect_retry_sends_rebased_edit(&mut fx);
}

/// "shows a queued edit dropped by a range write as failed after the lightbox
/// reopens"
#[test]
fn shows_a_queued_edit_dropped_by_a_range_write_as_failed_after_the_lightbox_reopens() {
    let mut fx = race_range_write_behind_save(true);
    fx.select_at(1, 0);
    fx.settle();
    expect_retry_sends_rebased_edit(&mut fx);
}

/// "synchronizes loaded summary changes without losing complete path or tags". The
/// rows carry no file name patch; a refresh brings the renamed row.
#[test]
fn synchronizes_loaded_summary_changes_without_losing_complete_path_or_tags() {
    let mut fx = Fx::opened(3);
    fx.send(RuntimeInput::Query(QueryInput::PatchAssets(
        AssetPatch::Update(vec![(
            1,
            SummaryPatch {
                media_group_key: Some(Some("new-group".into())),
                ..SummaryPatch::default()
            },
        )]),
    )));
    let selected = fx.view().selected().expect("open").clone();
    assert_eq!(
        selected.summary.media_group_key.as_deref(),
        Some("new-group")
    );
    assert_eq!(selected.path.as_deref(), Some("/synthetic/1.png"));
    assert_eq!(fx.editor(), ["cat"]);

    fx.backend.on_start_asset_query(|_| {
        let mut row = summary(1);
        row.file_name = "renamed.png".into();
        row.media_group_key = Some("new-group".into());
        Ok(StartAssetQueryResult::Ready {
            session_id: 1,
            revision: 2,
            total: 3,
            offset: 0,
            items: vec![row, summary(2), summary(3)],
        })
    });
    fx.refresh_query();
    fx.settle();
    let selected = fx.view().selected().expect("open").clone();
    assert_eq!(selected.summary.file_name, "renamed.png");
    assert_eq!(selected.path.as_deref(), Some("/synthetic/1.png"));
    assert_eq!(fx.editor(), ["cat"]);
}

// ---------------------------------------------------------------------------
// Acceptance: a save racing navigation
// ---------------------------------------------------------------------------

/// The save -> navigation race of the acceptance criteria: Next waits for a page that
/// is not cached while a save of the shown asset is on its way, and the save answers
/// first. The save is published for its own asset only, and the row Next reads opens
/// with its own tags.
#[test]
fn a_save_that_answers_while_navigation_waits_stays_with_its_asset() {
    let mut fx = Fx::new(600);
    fx.select_at(256, 255);
    assert_eq!(fx.executor.pending(), 1, "the details read");
    fx.answer(0);
    assert_eq!(fx.editor(), ["cat"]);
    // The neighbours' pages are on their way; the save is sent after them.
    let write = fx.executor.pending();
    fx.save(&["cat", "mine"]);
    assert_eq!(fx.executor.pending(), write + 1);
    fx.next();
    assert_eq!(fx.selected_id(), Some(256), "the page has not answered");

    fx.answer(write);
    assert_eq!(fx.tag_saves(), [(256, list(&["cat", "mine"]))]);
    assert_eq!(fx.selected_id(), Some(256));
    assert_eq!(fx.editor(), ["cat", "mine"]);
    assert!(!fx.view().tag_saving() && !fx.view().tag_failed());

    fx.settle();
    assert_eq!(fx.selected_id(), Some(257));
    assert_eq!(fx.view().selected_index(), Some(256));
    assert_eq!(fx.editor(), ["cat"]);
    assert!(!fx.view().tag_saving() && !fx.view().tag_failed());
    assert_eq!(fx.tag_saves().len(), 1);

    fx.previous();
    fx.settle();
    assert_eq!(fx.selected_id(), Some(256));
    assert_eq!(fx.editor(), ["cat", "mine"]);
}

// ---------------------------------------------------------------------------
// MediaGroupSetter.test.tsx: the order draft of the model
// ---------------------------------------------------------------------------

const INVALID_ORDERS: [&str; 13] = [
    "0",
    "00",
    "-1",
    "1.5",
    "+2",
    "1e3",
    "9007199254740992",
    "12x3",
    " 2",
    "2 ",
    "2\n",
    "NaN",
    "Infinity",
];

fn order_draft(fx: &mut Fx, value: &str) {
    fx.lightbox(LightboxCommand::SetMediaGroupOrderDraft(value.to_owned()));
}

/// "saves typed positive integers and clears to null"
#[test]
fn saves_typed_positive_integers_and_clears_to_null() {
    let mut fx = Fx::opened(3);
    fx.lightbox(LightboxCommand::SetMediaGroupKeyDraft("group".into()));
    for typed in ["1", "12", "123"] {
        order_draft(&mut fx, typed);
    }
    fx.lightbox(LightboxCommand::SaveMediaGroup);
    fx.settle();
    assert_eq!(
        fx.group_saves().last(),
        Some(&(1, Some("group".to_owned()), Some(123.0)))
    );
    // The scripted rows do not carry the saved group, so the drafts follow the
    // canonical row; the setter's key stays "group" as in the Solid case.
    fx.lightbox(LightboxCommand::SetMediaGroupKeyDraft("group".into()));
    order_draft(&mut fx, "");
    assert_eq!(fx.view().media_group_order_editor(), "");
    fx.lightbox(LightboxCommand::SaveMediaGroup);
    fx.settle();
    assert_eq!(
        fx.group_saves().last(),
        Some(&(1, Some("group".to_owned()), None))
    );
}

/// "accepts pasted 00012 and saves its numeric value" and "accepts pasted
/// 9007199254740991 and saves its numeric value"
#[test]
fn accepts_pasted_orders_and_saves_their_numeric_value() {
    for (pasted, value) in [
        ("00012", 12.0),
        ("9007199254740991", 9_007_199_254_740_991.0),
    ] {
        let mut fx = Fx::opened(3);
        fx.lightbox(LightboxCommand::SetMediaGroupKeyDraft("group".into()));
        order_draft(&mut fx, pasted);
        assert_eq!(fx.view().media_group_order_editor(), pasted);
        fx.lightbox(LightboxCommand::SaveMediaGroup);
        fx.settle();
        assert_eq!(
            fx.group_saves().last(),
            Some(&(1, Some("group".to_owned()), Some(value))),
            "{pasted}"
        );
    }
}

/// "rejects the entire pasted change %j" (every invalid order)
#[test]
fn rejects_the_entire_pasted_change() {
    let mut fx = Fx::opened(3);
    order_draft(&mut fx, "7");
    for pasted in INVALID_ORDERS {
        order_draft(&mut fx, pasted);
        assert_eq!(fx.view().media_group_order_editor(), "7", "{pasted:?}");
    }
}

/// "rejects invalid typed characters and zero"
#[test]
fn rejects_invalid_typed_characters_and_zero() {
    let mut fx = Fx::opened(3);
    for typed in ["0", "-", "+", ".", "e", " ", "x"] {
        order_draft(&mut fx, typed);
        assert_eq!(fx.view().media_group_order_editor(), "", "{typed:?}");
    }
    order_draft(&mut fx, "2");
    for typed in ["2.", "2e", "2+", "2-", "2x"] {
        order_draft(&mut fx, typed);
        assert_eq!(fx.view().media_group_order_editor(), "2", "{typed:?}");
    }
}

/// "preserves invalid preloaded %j and disables Apply until edited", for the invalid
/// orders a stored number can be.
#[test]
fn preserves_invalid_preloaded_orders_and_disables_apply_until_edited() {
    for (stored, shown) in [
        (0.0, "0"),
        (-1.0, "-1"),
        (1.5, "1.5"),
        (9_007_199_254_740_992.0, "9007199254740992"),
    ] {
        let mut fx = Fx::new(3);
        fx.backend.on_get_asset_details(move |id| {
            let mut details = details(id, &["cat"]);
            details.summary.media_group_key = Some("group".into());
            details.summary.media_group_order = Some(stored);
            Ok(Some(details))
        });
        fx.open(1);
        assert_eq!(fx.view().media_group_order_editor(), shown);
        assert!(!fx.view().media_group_valid(), "{shown}");
        fx.lightbox(LightboxCommand::SaveMediaGroup);
        fx.settle();
        assert!(fx.group_saves().is_empty(), "{shown}");

        order_draft(&mut fx, "3");
        assert!(fx.view().media_group_valid());
        fx.lightbox(LightboxCommand::SaveMediaGroup);
        fx.settle();
        assert_eq!(
            fx.group_saves(),
            [(1, Some("group".to_owned()), Some(3.0))],
            "{shown}"
        );
    }
}

// ---------------------------------------------------------------------------
// LightboxModal.test.tsx: the delete confirmation and the group copy
// ---------------------------------------------------------------------------

fn confirm_word(language: Language) -> String {
    I18n::new(language).t("lightbox.deleteConfirm.confirmWord")
}

/// "confirms deletion case-insensitively and prevents duplicate submission"
#[test]
fn confirms_deletion_case_insensitively_and_prevents_duplicate_submission() {
    let mut fx = Fx::opened(3);
    let word = confirm_word(Language::En);
    let mut confirm = DeleteConfirm::new();
    confirm.sync(fx.selected_id());
    confirm.open();
    assert!(!confirm.can_confirm(&word));
    confirm.set_text(" yEs ");
    assert!(confirm.can_confirm(&word));

    let request = confirm.submit(&word).expect("a deletion to send");
    assert_eq!(confirm.submit(&word), None, "a second click sends nothing");
    fx.lightbox(LightboxCommand::DeleteSelected { request });
    assert!(confirm.deleting(), "Close is disabled while it runs");
    assert!(!confirm.close());

    fx.settle();
    for event in fx.events() {
        let LightboxEvent::DeleteSettled { request, result } = event;
        confirm.settled(request, &result);
    }
    assert_eq!(fx.count("delete_asset"), 1);
    assert_eq!(fx.selected_id(), None);
    assert!(!confirm.deleting());
    assert!(!confirm.is_open());
}

/// "matches the confirm word case-insensitively in every locale without
/// localeCompare"
#[test]
fn matches_the_confirm_word_case_insensitively_in_every_locale() {
    let cases: [(Language, &[&str], &str); 5] = [
        (Language::En, &["YES", "yes", " Yes "], "yess"),
        (Language::Pl, &["TAK", "tak", "Tak"], "Yes"),
        (Language::Ru, &["ДА", "да"], "Tak"),
        (Language::Es, &["SÍ", "sí", "Si\u{301}"], "si"),
        (Language::Cs, &["ANO", "ano"], "ne"),
    ];
    for (language, accepted, rejected) in cases {
        let word = confirm_word(language);
        let mut confirm = DeleteConfirm::new();
        confirm.sync(Some(1));
        confirm.open();
        for value in accepted {
            confirm.set_text(*value);
            assert!(confirm.can_confirm(&word), "{language:?}: {value}");
        }
        confirm.set_text(rejected);
        assert!(!confirm.can_confirm(&word), "{language:?}: {rejected}");
    }
}

/// "dismisses the confirmation first on Escape with focus outside it" (the model
/// half: Escape closes the confirmation, not the lightbox)
#[test]
fn dismisses_the_confirmation_first_on_escape() {
    let fx = Fx::opened(3);
    let mut confirm = DeleteConfirm::new();
    confirm.sync(fx.selected_id());
    confirm.open();
    confirm.set_text("Yes");
    assert!(confirm.close());
    assert!(!confirm.is_open());
    assert_eq!(confirm.text(), "");
    assert_eq!(fx.selected_id(), Some(1));
}

/// "ignores Escape outside the confirmation while a delete is pending"
#[test]
fn ignores_escape_outside_the_confirmation_while_a_delete_is_pending() {
    let mut fx = Fx::opened(3);
    let word = confirm_word(Language::En);
    let mut confirm = DeleteConfirm::new();
    confirm.sync(fx.selected_id());
    confirm.open();
    confirm.set_text("Yes");
    let request = confirm.submit(&word).expect("a deletion to send");
    fx.lightbox(LightboxCommand::DeleteSelected { request });
    assert_eq!(fx.executor.pending(), 1, "the deletion waits");
    assert!(!confirm.close(), "Escape changes nothing while it runs");
    assert!(confirm.is_open());
    assert_eq!(fx.selected_id(), Some(1));

    fx.settle();
    for event in fx.events() {
        let LightboxEvent::DeleteSettled { request, result } = event;
        confirm.settled(request, &result);
    }
    assert_eq!(fx.selected_id(), None);
    assert!(!confirm.is_open());
}

/// "keeps the confirmation, typed word and Information open across tag and favorite
/// writes" (the model half: a republished selection of the same asset keeps the
/// confirmation and its word)
#[test]
fn keeps_the_confirmation_and_typed_word_across_tag_and_favorite_writes() {
    let mut fx = Fx::opened(3);
    let word = confirm_word(Language::En);
    let mut confirm = DeleteConfirm::new();
    confirm.sync(fx.selected_id());
    confirm.open();
    confirm.set_text("Yes");

    fx.save(&["cat", "dog"]);
    fx.settle();
    confirm.sync(fx.selected_id());
    assert_eq!(fx.editor(), ["cat", "dog"]);
    assert!(confirm.is_open());
    assert_eq!(confirm.text(), "Yes");
    assert!(confirm.can_confirm(&word));

    fx.lightbox(LightboxCommand::ToggleFavorite);
    fx.settle();
    confirm.sync(fx.selected_id());
    assert_eq!(fx.favorite_saves(), [(1, true)]);
    assert!(fx.favorite());
    assert!(confirm.is_open());
    assert_eq!(confirm.text(), "Yes");

    let request = confirm.submit(&word).expect("a deletion to send");
    fx.lightbox(LightboxCommand::DeleteSelected { request });
    fx.settle();
    assert_eq!(fx.count("delete_asset"), 1);
    assert_eq!(fx.selected_id(), None);
}

/// "still resets the confirmation and Information when the selected ID changes" (the
/// confirmation half)
#[test]
fn still_resets_the_confirmation_when_the_selected_id_changes() {
    let mut fx = Fx::opened(3);
    let mut confirm = DeleteConfirm::new();
    confirm.sync(fx.selected_id());
    confirm.open();
    confirm.set_text("Yes");
    fx.select(2);
    confirm.sync(fx.selected_id());
    assert_eq!(fx.selected_id(), Some(2));
    assert!(!confirm.is_open());
    assert_eq!(confirm.text(), "");
}

/// "keeps failed deletion in its confirmation and restores focus after Escape" (the
/// model half)
#[test]
fn keeps_failed_deletion_in_its_confirmation() {
    let mut fx = Fx::opened(3);
    fx.backend
        .on_delete_asset(|_| Err(BackendError::rejected("source retained")));
    let word = confirm_word(Language::En);
    let mut confirm = DeleteConfirm::new();
    confirm.sync(fx.selected_id());
    confirm.open();
    confirm.set_text("Yes");
    let request = confirm.submit(&word).expect("a deletion to send");
    fx.lightbox(LightboxCommand::DeleteSelected { request });
    fx.settle();
    for event in fx.events() {
        let LightboxEvent::DeleteSettled { request, result } = event;
        confirm.settled(request, &result);
    }
    assert!(confirm.is_open());
    let alert = confirm
        .error_text(&I18n::new(Language::En))
        .expect("an alert");
    assert!(alert.contains("source retained"), "{alert}");
    assert_eq!(fx.selected_id(), Some(1));
    assert!(confirm.close());
    assert!(!confirm.is_open());
}

/// "shows a busy-library delete rejection localized and keeps every exit usable"
/// (ADR 0022: the backend fails fast while a scan holds the scan lock)
#[test]
fn shows_a_busy_library_delete_rejection_localized_and_keeps_every_exit_usable() {
    let mut fx = Fx::opened(3);
    fx.backend
        .on_delete_asset(|_| Err(BackendError::rejected("i18n:common.libraryBusy")));
    let word = confirm_word(Language::En);
    let mut confirm = DeleteConfirm::new();
    confirm.sync(fx.selected_id());
    confirm.open();
    confirm.set_text("Yes");
    let request = confirm.submit(&word).expect("a deletion to send");
    fx.lightbox(LightboxCommand::DeleteSelected { request });
    fx.settle();
    for event in fx.events() {
        let LightboxEvent::DeleteSettled { request, result } = event;
        confirm.settled(request, &result);
    }
    let alert = confirm
        .error_text(&I18n::new(Language::En))
        .expect("an alert");
    assert_eq!(
        alert,
        "The file could not be deleted. Its library metadata was preserved. A scan or another \
         library operation is running. Try again when it finishes."
    );
    assert!(!alert.contains("i18n:"));
    assert_eq!(fx.selected_id(), Some(1));
    assert!(!confirm.deleting());
    confirm.set_text("Yes");
    assert!(
        confirm.can_confirm(&word),
        "the field and Confirm are usable"
    );
    assert!(confirm.close(), "Cancel is usable");

    fx.next();
    fx.settle();
    assert_eq!(fx.selected_id(), Some(2));
    fx.close();
    assert_eq!(fx.selected_id(), None);
}

/// "copies trimmed group names and rejects late clipboard results"
#[test]
fn copies_trimmed_group_names_and_rejects_late_clipboard_results() {
    let mut fx = Fx::opened(3);
    fx.lightbox(LightboxCommand::SetMediaGroupKeyDraft(
        "  group-one  ".into(),
    ));
    let mut clipboard = GroupClipboard::new(
        fx.view().media_group_key_editor(),
        fx.selected_id().unwrap(),
    );
    let request = clipboard.begin_copy().expect("a group name to copy");
    assert_eq!(request.text, "group-one");

    fx.next();
    fx.settle();
    clipboard.sync(
        fx.view().media_group_key_editor(),
        fx.selected_id().unwrap(),
    );
    assert!(!clipboard.finish_copy(&request, true, Duration::ZERO));
    assert!(!clipboard.confirmed(Duration::ZERO));
    assert_ne!(
        clipboard.title_key(Duration::ZERO),
        "lightbox.copyMediaGroup.copied"
    );
}

// ---------------------------------------------------------------------------
// presentation.test.ts: "lightbox presentation owners"
// ---------------------------------------------------------------------------

const fn ms(value: u64) -> Duration {
    Duration::from_millis(value)
}

/// "hides after three seconds and only movement restarts the timer"
#[test]
fn hides_after_three_seconds_and_only_movement_restarts_the_timer() {
    let mut activity = Activity::new();
    activity.set_active(true, false, ms(0));
    assert!(activity.visible(ms(2999)));
    activity.pointer_moved(10.0, 20.0, ms(2999));
    assert!(activity.visible(ms(5998)));
    activity.pointer_moved(10.0, 20.0, ms(5998));
    assert!(!activity.visible(ms(5999)));
    activity.pointer_moved(11.0, 20.0, ms(5999));
    assert!(activity.visible(ms(5999)));
}

/// "reveals without consuming %s and releases timers on close" for pointerdown,
/// wheel, scroll and keydown (the event is not consumed: the methods take nothing
/// from the view's event)
#[test]
fn reveals_on_input_and_releases_timers_on_close() {
    type Input = fn(&mut Activity, Duration);
    let inputs: [(&str, Input); 4] = [
        ("pointerdown", |activity, now| {
            activity.pointer_down(false, now)
        }),
        ("wheel", |activity, now| activity.input(now)),
        ("scroll", |activity, now| activity.input(now)),
        ("keydown", |activity, now| activity.input(now)),
    ];
    for (name, input) in inputs {
        let mut activity = Activity::new();
        activity.set_active(true, false, ms(0));
        assert!(!activity.visible(ms(3000)), "{name}");
        input(&mut activity, ms(3000));
        assert!(activity.visible(ms(3000)), "{name}");
        activity.set_active(false, false, ms(3000));
        assert_eq!(activity.deadline(ms(3000)), None, "{name}");
        activity.reveal(ms(3100));
        assert_eq!(activity.deadline(ms(3100)), None, "{name}");
    }
}

/// "does not hold controls visible for focus alone": focus is no input of the model.
#[test]
fn does_not_hold_controls_visible_for_focus_alone() {
    let mut activity = Activity::new();
    activity.set_active(true, false, ms(0));
    assert!(!activity.visible(ms(3000)));
}

/// "keeps touch controls visible"
#[test]
fn keeps_touch_controls_visible() {
    let mut activity = Activity::new();
    activity.set_active(true, true, ms(0));
    assert!(activity.visible(ms(10_000)));
}

/// "follows breakpoints until explicit choice, retains occupied space and resets on
/// close"
#[test]
fn follows_breakpoints_until_explicit_choice_retains_occupied_space_and_resets_on_close() {
    let mut sidebar = Sidebar::new(false, false);
    assert!(sidebar.open());
    sidebar.set_narrow(true, ms(0));
    assert!(!sidebar.open());
    assert!(sidebar.occupied(ms(0)));
    assert!(!sidebar.occupied(ms(200)));

    sidebar.show(ms(200));
    sidebar.set_narrow(false, ms(200));
    sidebar.set_narrow(true, ms(200));
    assert!(sidebar.open());
    sidebar.set_locked(true, ms(200));
    sidebar.hide(ms(200));
    assert!(sidebar.open());

    sidebar.set_locked(false, ms(200));
    sidebar.set_active(false, ms(200));
    sidebar.set_active(true, ms(200));
    assert!(!sidebar.open());
}

/// "copies trimmed names, resets after 1600ms and ignores disposed replies"
#[test]
fn copies_trimmed_names_resets_after_1600ms_and_ignores_disposed_replies() {
    let mut clipboard = GroupClipboard::new(" group ", 1);
    let request = clipboard.begin_copy().expect("a group name to copy");
    assert_eq!(request.text, "group");
    assert!(clipboard.finish_copy(&request, true, ms(0)));
    assert!(clipboard.confirmed(ms(0)));
    assert!(!clipboard.confirmed(ms(1600)));

    let pending = clipboard.begin_copy().expect("a group name to copy");
    clipboard.dispose();
    assert!(!clipboard.finish_copy(&pending, true, ms(2000)));
    assert_eq!(clipboard.deadline(ms(2000)), None, "no timer is left");
}
