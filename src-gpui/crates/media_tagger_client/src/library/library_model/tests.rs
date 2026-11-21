//! The library model against the scripted backend on the deterministic executor
//! (cases of `mutations.test.ts` and `createLibrary.test.ts`), and once against the
//! real core.

use std::{sync::Arc, time::Duration};

use media_tagger_core::models::{
    AssetDetails, AssetQuerySectionsResult, AssetSummary, AssetTagResult, BulkFavoriteSummary,
    BulkMediaGroupSummary, BulkTagMergeSummary, DeleteAssetSummary, DeleteSourceStatus,
    SelectionMutationResult, SetAssetTagsSummary, StartAssetQueryResult, TagListPage,
    TagQueryImpact,
};

use super::{
    Library, LibraryEvent, LibraryInput, LibraryReset, LibraryRunner, MetadataCommit, Mutation,
    MutationError, MutationOutcome, RefreshError,
};
use crate::{
    backend::{
        core_backend::test_support::TempCore, fake::Call, Backend, BackendError, FakeBackend,
        MediaGroupUpdate, SelectionMutation, SelectionSpec,
    },
    exec::{Executor, ManualExecutor, ThreadExecutor},
    library::{
        LibraryFilters, QueryConfig, QueryError, QueryInput, TagEdit, TagStateError, Ticket,
        MAX_CACHED_DETAILS,
    },
    model::Driver,
};

fn list(tags: &[&str]) -> Vec<String> {
    tags.iter().map(|tag| (*tag).to_owned()).collect()
}

fn row(id: i64) -> AssetSummary {
    AssetSummary {
        id,
        file_name: format!("{id}.jpg"),
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

fn detail(id: i64) -> AssetDetails {
    AssetDetails {
        summary: row(id),
        path: format!("/synthetic/{id}.jpg"),
        size_bytes: 10,
        thumb_path: None,
        tags: list(&["cat"]),
    }
}

fn ready(ids: &[i64]) -> StartAssetQueryResult {
    StartAssetQueryResult::Ready {
        session_id: 1,
        revision: 1,
        total: ids.len(),
        offset: 0,
        items: ids.iter().copied().map(row).collect(),
    }
}

fn saved(asset_id: i64, tags: &[&str]) -> SetAssetTagsSummary {
    SetAssetTagsSummary {
        vocabulary_changed: false,
        query_impact: TagQueryImpact::None,
        asset_id,
        changed: true,
        tags: list(tags),
        revision: 2,
    }
}

fn merged(results: &[(i64, &[&str])]) -> BulkTagMergeSummary {
    BulkTagMergeSummary {
        vocabulary_changed: false,
        query_impact: TagQueryImpact::None,
        processed_assets: results.len(),
        updated_assets: results.len(),
        processed_asset_ids: results.iter().map(|(id, _)| *id).collect(),
        updated_asset_ids: results.iter().map(|(id, _)| *id).collect(),
        results: results
            .iter()
            .map(|(asset_id, tags)| AssetTagResult {
                asset_id: *asset_id,
                changed: true,
                tags: list(tags),
            })
            .collect(),
        revision: 2,
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

fn add(tag: &str) -> TagEdit {
    TagEdit::Change {
        add: list(&[tag]),
        remove: Vec::new(),
    }
}

fn save(asset_id: i64, edit: TagEdit) -> Mutation {
    Mutation::SaveTags { asset_id, edit }
}

fn favorite(asset_id: i64, favorite: bool) -> Mutation {
    Mutation::SetFavorite { asset_id, favorite }
}

struct Fixture {
    fake: Arc<FakeBackend>,
    executor: Arc<ManualExecutor>,
    driver: Driver<Library, LibraryRunner>,
    next_ticket: u64,
}

impl Fixture {
    /// A library of one asset (ID 1, tags `cat`) with a scripted backend: details of
    /// any ID, the known tags `cat` and `dog`, tag saves that store what they are
    /// given.
    fn new() -> Self {
        Self::with_filters(LibraryFilters::default())
    }

    fn with_filters(filters: LibraryFilters) -> Self {
        let fake = Arc::new(FakeBackend::new());
        fake.on_start_asset_query(|_| Ok(ready(&[1])))
            .on_get_asset_query_sections(|session_id| {
                Ok(AssetQuerySectionsResult::Ready {
                    session_id,
                    total: 0,
                    sections: Vec::new(),
                })
            })
            .on_get_asset_details(|id| Ok(Some(detail(id))))
            .on_list_tags(|_, _, _| {
                Ok(TagListPage {
                    items: list(&["cat", "dog"]),
                    total: 2,
                })
            })
            .on_set_asset_tags(|asset_id, tags| {
                let tags: Vec<&str> = tags.iter().map(String::as_str).collect();
                Ok(saved(asset_id, &tags))
            });
        let executor = Arc::new(ManualExecutor::new());
        let driver = Driver::new(
            Library::with_filters(QueryConfig::default(), filters),
            LibraryRunner::new(fake.clone()),
            executor.clone() as Arc<dyn Executor>,
        );
        Self {
            fake,
            executor,
            driver,
            next_ticket: 100,
        }
    }

    /// [`new`](Self::new), refreshed, with the details of asset 1 loaded (`setup` of
    /// the Solid tests).
    fn loaded() -> Self {
        let mut fx = Self::new();
        fx.send(LibraryInput::Refresh { ticket: None });
        fx.settle();
        fx.load(1);
        fx.settle();
        fx.events();
        fx
    }

    fn library(&self) -> &Library {
        self.driver.model()
    }

    fn send(&mut self, input: LibraryInput) {
        self.driver.handle(input);
    }

    /// Runs every waiting backend call and applies the replies.
    fn settle(&mut self) {
        while self.executor.run_all() > 0 || self.driver.pump() > 0 {}
    }

    /// Runs the oldest waiting backend call and applies its reply.
    fn answer_next(&mut self) {
        assert!(self.executor.run_next(), "a backend call is waiting");
        self.driver.pump();
    }

    fn ticket(&mut self) -> Ticket {
        self.next_ticket += 1;
        Ticket(self.next_ticket)
    }

    fn mutate(&mut self, mutation: Mutation) -> Ticket {
        let ticket = self.ticket();
        self.send(LibraryInput::Mutate { ticket, mutation });
        ticket
    }

    fn load(&mut self, asset_id: i64) -> Ticket {
        let ticket = self.ticket();
        self.send(LibraryInput::LoadDetails { ticket, asset_id });
        ticket
    }

    fn events(&mut self) -> Vec<LibraryEvent> {
        self.driver.model_mut().take_events()
    }

    fn calls(&self, method: &str) -> usize {
        self.fake
            .called_methods()
            .into_iter()
            .filter(|called| *called == method)
            .count()
    }

    fn tags(&self, asset_id: i64) -> Option<Vec<String>> {
        self.library()
            .asset_tags(asset_id)
            .map(|known| known.tags.to_vec())
    }

    fn details(&self, asset_id: i64) -> Option<Arc<AssetDetails>> {
        self.library().cached_details(asset_id).cloned()
    }
}

/// Takes the answer to the write of `ticket` out of `events`.
fn settled(
    events: &mut Vec<LibraryEvent>,
    ticket: Ticket,
) -> Option<Result<MutationOutcome, MutationError>> {
    let index = events.iter().position(|event| {
        matches!(event, LibraryEvent::MutationSettled { ticket: answered, .. } if *answered == ticket)
    })?;
    match events.remove(index) {
        LibraryEvent::MutationSettled { result, .. } => Some(result),
        _ => None,
    }
}

fn refused(events: &mut Vec<LibraryEvent>, ticket: Ticket) -> Option<MutationError> {
    settled(events, ticket)?.err()
}

/// Takes the answer to the details read of `ticket` out of `events`.
fn loaded(events: &mut Vec<LibraryEvent>, ticket: Ticket) -> Option<Option<Arc<AssetDetails>>> {
    let index = events.iter().position(|event| {
        matches!(event, LibraryEvent::DetailsLoaded { ticket: answered, .. } if *answered == ticket)
    })?;
    match events.remove(index) {
        LibraryEvent::DetailsLoaded { result, .. } => Some(result.expect("no backend error")),
        _ => None,
    }
}

fn commits(events: &[LibraryEvent]) -> Vec<&MetadataCommit> {
    events
        .iter()
        .filter_map(|event| match event {
            LibraryEvent::Committed(commit) => Some(commit),
            _ => None,
        })
        .collect()
}

#[test]
fn a_refresh_reads_the_query_and_the_known_tags_and_details_come_with_canonical_tags() {
    let mut fx = Fixture::new();
    fx.fake.on_get_asset_details(|id| {
        Ok(Some(AssetDetails {
            tags: list(&[" Cat ", "ca\u{FEFF}t", "dog", "cat"]),
            ..detail(id)
        }))
    });
    assert!(!fx.library().can_edit_tags(1));

    let refresh = fx.ticket();
    fx.send(LibraryInput::Refresh {
        ticket: Some(refresh),
    });
    assert!(fx.library().refreshing());
    fx.settle();
    assert!(matches!(
        fx.events()[..],
        [LibraryEvent::Refreshed { ticket, result: Ok(()) }] if ticket == refresh
    ));
    assert_eq!(fx.library().query().total(), 1);
    assert_eq!(**fx.library().known_tags(), ["cat", "dog"]);
    assert!(!fx.library().refreshing());

    let read = fx.load(1);
    assert!(fx.events().is_empty(), "the backend has not answered");
    fx.settle();
    let mut events = fx.events();
    let details = loaded(&mut events, read).flatten().expect("details");
    // The legacy tag with an inner U+FEFF never reaches an editor (ADR 0033).
    assert_eq!(details.tags, ["cat", "dog"]);
    assert_eq!(details.path, "/synthetic/1.jpg");
    assert_eq!(fx.tags(1), Some(list(&["cat", "dog"])));
    assert!(fx.library().can_edit_tags(1));

    // The second read is served from the cache.
    let again = fx.load(1);
    let mut events = fx.events();
    assert!(loaded(&mut events, again).flatten().is_some());
    assert_eq!(fx.calls("get_asset_details"), 1);
}

#[test]
fn a_failed_refresh_reports_which_half_failed() {
    let mut fx = Fixture::new();
    fx.fake
        .on_list_tags(|_, _, _| Err(BackendError::rejected("tags offline")));
    let ticket = fx.ticket();
    fx.send(LibraryInput::Refresh {
        ticket: Some(ticket),
    });
    fx.settle();
    assert!(matches!(
        &fx.events()[..],
        [LibraryEvent::Refreshed { result: Err(RefreshError::Vocabulary(error)), .. }]
            if error.text() == "tags offline"
    ));

    fx.fake
        .on_start_asset_query(|_| Err(BackendError::rejected("query offline")));
    let ticket = fx.ticket();
    fx.send(LibraryInput::Refresh {
        ticket: Some(ticket),
    });
    fx.settle();
    assert!(matches!(
        &fx.events()[..],
        [LibraryEvent::Refreshed { result: Err(RefreshError::Query(QueryError::Backend(error))), .. }]
            if error.text() == "query offline"
    ));
}

/// Acceptance: two quick writes of one asset. One is in flight, the second is
/// computed from the canonical tags of the first, and the state comes from the
/// backend's answers only.
#[test]
fn two_quick_tag_edits_of_one_asset_are_sent_one_after_the_other_and_merge() {
    let mut fx = Fixture::loaded();
    // The backend's spelling wins over what was asked for.
    fx.fake.on_set_asset_tags(|asset_id, tags| {
        let mut canonical: Vec<String> = tags.iter().map(|tag| tag.to_uppercase()).collect();
        canonical.dedup();
        Ok(SetAssetTagsSummary {
            tags: canonical,
            ..saved(asset_id, &[])
        })
    });

    let first = fx.mutate(save(1, add("dog")));
    let second = fx.mutate(save(1, add("bird")));
    assert_eq!(fx.executor.pending(), 1, "one write in flight");
    assert_eq!(fx.library().pending_writes(), 2);
    assert_eq!(
        fx.tags(1),
        Some(list(&["cat"])),
        "nothing is published early"
    );
    assert!(fx.details(1).is_none(), "the cached details are held back");
    assert!(fx.events().is_empty());

    fx.answer_next();
    assert_eq!(
        fx.tags(1),
        Some(list(&["cat", "dog"])),
        "normalized canonical"
    );
    assert_eq!(fx.executor.pending(), 1, "the second write was sent");
    let mut events = fx.events();
    match settled(&mut events, first) {
        Some(Ok(MutationOutcome::TagsSaved { requested, summary })) => {
            assert_eq!(requested, ["cat", "dog"]);
            assert_eq!(summary.tags, ["CAT", "DOG"]);
        }
        other => panic!("unexpected answer {other:?}"),
    }
    assert!(settled(&mut events, second).is_none());

    fx.answer_next();
    assert_eq!(
        fx.fake
            .calls()
            .into_iter()
            .rev()
            .find_map(|call| match call {
                Call::SetAssetTags { tags, .. } => Some(tags),
                _ => None,
            }),
        Some(list(&["cat", "dog", "bird"])),
        "the second edit was applied to the first one's canonical result"
    );
    assert_eq!(fx.tags(1), Some(list(&["cat", "dog", "bird"])));
    assert_eq!(
        fx.details(1).map(|details| details.tags.clone()),
        Some(list(&["cat", "dog", "bird"]))
    );
    let mut events = fx.events();
    assert!(matches!(settled(&mut events, second), Some(Ok(_))));
    assert_eq!(fx.library().pending_writes(), 0);
    assert_eq!(fx.calls("set_asset_tags"), 2);
    // Neither write touched the query or read the known tags again.
    assert_eq!(fx.calls("start_asset_query"), 1);
    assert_eq!(fx.calls("list_tags"), 1);
}

/// mutations.test.ts "queues a favorite behind a pending tag save and publishes both
/// canonical results in order".
#[test]
fn a_favorite_behind_a_pending_tag_save_is_sent_after_it_and_both_are_published() {
    let mut fx = Fixture::loaded();
    fx.fake.on_set_asset_favorite(|_, _| Ok(()));
    let old_read = fx.driver.model_mut().tags.capture_generation(1);

    let write = fx.mutate(save(1, TagEdit::Replace(list(&["DOG"]))));
    let favorite_write = fx.mutate(favorite(1, true));
    assert_eq!(fx.executor.pending(), 1);
    assert_eq!(fx.calls("set_asset_favorite"), 0);
    assert_eq!(fx.tags(1), Some(list(&["cat"])));
    assert!(fx.details(1).is_none());
    let during = fx.driver.model_mut().tags.capture_generation(1);

    fx.settle();
    assert_eq!(
        fx.fake.called_methods()[fx.fake.calls().len() - 2..],
        ["set_asset_tags", "set_asset_favorite"]
    );
    assert_eq!(fx.tags(1), Some(list(&["dog"])));
    // The favorite overlays the details its predecessor published.
    let details = fx.details(1).expect("details");
    assert_eq!(details.tags, ["dog"]);
    assert!(details.summary.is_favorite);
    assert!(fx.library().query().asset_at(0).unwrap().is_favorite);
    let tags = &mut fx.driver.model_mut().tags;
    assert!(!tags.publish_details(1, list(&["old"]), &old_read));
    assert!(!tags.publish_details(1, list(&["during"]), &during));

    let mut events = fx.events();
    assert!(matches!(settled(&mut events, write), Some(Ok(_))));
    assert!(matches!(
        settled(&mut events, favorite_write),
        Some(Ok(MutationOutcome::FavoriteSet {
            asset_id: 1,
            favorite: true
        }))
    ));
    assert_eq!(commits(&events).len(), 2);
    assert_eq!(fx.calls("start_asset_query"), 1);
    assert_eq!(fx.calls("list_tags"), 1);
}

/// mutations.test.ts "runs the next queued write after a failure ..." and "keeps
/// canonical state on rejection, releases ownership and allows retry".
#[test]
fn a_failed_write_publishes_nothing_and_the_next_one_starts_from_canonical_tags() {
    let mut fx = Fixture::loaded();
    let mut first_call = true;
    fx.fake.on_set_asset_tags(move |asset_id, tags| {
        if std::mem::take(&mut first_call) {
            return Err(BackendError::rejected("locked"));
        }
        let tags: Vec<&str> = tags.iter().map(String::as_str).collect();
        Ok(saved(asset_id, &tags))
    });

    let first = fx.mutate(save(1, TagEdit::Replace(list(&["lost"]))));
    let second = fx.mutate(save(1, add("dog")));
    fx.answer_next();
    let mut events = fx.events();
    assert_eq!(
        refused(&mut events, first),
        Some(MutationError::Backend(BackendError::rejected("locked")))
    );
    assert!(commits(&events).is_empty());
    assert_eq!(fx.tags(1), Some(list(&["cat"])), "the canonical tags stay");
    assert!(fx.details(1).is_none());

    fx.settle();
    assert_eq!(fx.tags(1), Some(list(&["cat", "dog"])));
    let mut events = fx.events();
    assert!(matches!(settled(&mut events, second), Some(Ok(_))));

    let third = fx.mutate(save(1, add("bird")));
    fx.settle();
    assert_eq!(fx.tags(1), Some(list(&["cat", "dog", "bird"])));
    assert!(matches!(settled(&mut fx.events(), third), Some(Ok(_))));
}

/// Acceptance: tag editing is blocked until the complete tag list of the asset is
/// known.
#[test]
fn a_tag_edit_is_refused_until_the_complete_tag_list_is_known() {
    let mut fx = Fixture::new();
    fx.send(LibraryInput::Refresh { ticket: None });
    fx.settle();
    assert!(!fx.library().can_edit_tags(1));

    for edit in [TagEdit::Replace(list(&["dog"])), add("dog")] {
        let ticket = fx.mutate(save(1, edit));
        assert_eq!(
            refused(&mut fx.events(), ticket),
            Some(MutationError::TagsNotLoaded)
        );
    }
    assert_eq!(fx.calls("set_asset_tags"), 0);
    assert_eq!(fx.library().pending_writes(), 0);
    assert!(fx.library().tags().is_idle());

    // A summary row is not enough; the details are.
    fx.load(1);
    fx.settle();
    assert!(fx.library().can_edit_tags(1));
    let ticket = fx.mutate(save(1, add("dog")));
    fx.settle();
    assert!(matches!(settled(&mut fx.events(), ticket), Some(Ok(_))));
    assert_eq!(fx.tags(1), Some(list(&["cat", "dog"])));
}

/// Acceptance: a partial bulk result publishes only the acknowledged IDs.
/// mutations.test.ts "waits for every bulk target, never sends a reduced subset and
/// settles a partial canonical result".
#[test]
fn a_bulk_write_waits_for_every_asset_and_publishes_only_acknowledged_ids() {
    let mut fx = Fixture::loaded();
    fx.fake.on_set_asset_favorite(|_, _| Ok(()));
    fx.fake
        .on_merge_asset_tags_bulk(|_, _| Ok(merged(&[(1, &["cat", "dog"])])));
    // Asset 2 is held by another write.
    let held = fx.mutate(favorite(2, true));

    let merge = fx.mutate(Mutation::MergeTags {
        asset_ids: vec![1, 2, 1],
        tags: list(&["dog"]),
    });
    assert_eq!(fx.calls("merge_asset_tags_bulk"), 0);
    assert_eq!(fx.executor.pending(), 1, "only the favorite was sent");

    fx.answer_next();
    assert!(matches!(settled(&mut fx.events(), held), Some(Ok(_))));
    fx.settle();
    assert_eq!(
        fx.fake.calls().into_iter().find_map(|call| match call {
            Call::MergeAssetTagsBulk { asset_ids, tags } => Some((asset_ids, tags)),
            _ => None,
        }),
        Some((vec![1, 2], list(&["dog"]))),
        "the whole target, once"
    );
    assert_eq!(fx.tags(1), Some(list(&["cat", "dog"])));
    assert_eq!(fx.tags(2), None, "asset 2 was not acknowledged");
    let mut events = fx.events();
    assert_eq!(
        commits(&events),
        [&MetadataCommit::Updated(vec![(
            1,
            crate::library::MetadataPatch {
                tags: Some(list(&["cat", "dog"])),
                ..Default::default()
            }
        )])]
    );
    match settled(&mut events, merge) {
        Some(Ok(MutationOutcome::TagsMerged(summary))) => assert_eq!(summary.processed_assets, 1),
        other => panic!("unexpected answer {other:?}"),
    }
    // An asset went missing: the query is read again.
    assert_eq!(fx.calls("start_asset_query"), 2);
    assert!(fx.library().tags().is_idle());
}

/// mutations.test.ts "patches favorites in query and details and publishes only
/// acknowledged bulk IDs".
#[test]
fn bulk_favorites_patch_only_the_processed_ids() {
    let mut fx = Fixture::loaded();
    fx.fake.on_start_asset_query(|_| Ok(ready(&[1, 2])));
    fx.send(LibraryInput::Refresh { ticket: None });
    fx.settle();
    fx.fake.on_toggle_assets_favorite_bulk(|_| {
        Ok(BulkFavoriteSummary {
            processed_asset_ids: vec![1],
            is_favorite: true,
            revision: 3,
        })
    });
    let before = fx.calls("start_asset_query");

    let toggle = fx.mutate(Mutation::ToggleFavorites {
        asset_ids: vec![1, 2],
    });
    fx.settle();

    let mut events = fx.events();
    assert!(matches!(
        settled(&mut events, toggle),
        Some(Ok(MutationOutcome::FavoritesToggled(_)))
    ));
    match commits(&events)[..] {
        [MetadataCommit::Updated(patches)] => {
            assert_eq!(patches.len(), 1);
            assert_eq!(patches[0].0, 1);
            assert_eq!(patches[0].1.summary.is_favorite, Some(true));
        }
        ref other => panic!("unexpected commits {other:?}"),
    }
    // Fewer assets than asked for: the visible rows are read again.
    assert_eq!(fx.calls("start_asset_query"), before + 1);
}

#[test]
fn favorites_restart_a_favorites_only_query_only_when_one_is_removed() {
    let mut fx = Fixture::with_filters(LibraryFilters {
        favorites_only: true,
        ..LibraryFilters::default()
    });
    fx.fake.on_set_asset_favorite(|_, _| Ok(()));
    fx.send(LibraryInput::Refresh { ticket: None });
    fx.settle();

    fx.mutate(favorite(1, true));
    fx.settle();
    assert_eq!(fx.calls("start_asset_query"), 1);
    assert!(fx.library().query().asset_at(0).unwrap().is_favorite);

    fx.mutate(favorite(1, false));
    fx.settle();
    assert_eq!(fx.calls("start_asset_query"), 2);
}

/// mutations.test.ts "uses the current applied filter when a save settles".
#[test]
fn a_save_is_judged_by_the_filters_applied_when_it_settles() {
    let mut fx = Fixture::loaded();
    fx.fake.on_set_asset_tags(|asset_id, _| {
        Ok(SetAssetTagsSummary {
            query_impact: TagQueryImpact::Tags {
                changed_tags: list(&["dog"]),
                tag_count_changed: false,
            },
            vocabulary_changed: true,
            ..saved(asset_id, &["dog"])
        })
    });

    fx.mutate(save(1, TagEdit::Replace(list(&["dog"]))));
    fx.send(LibraryInput::Query(QueryInput::ApplyFilters(
        LibraryFilters {
            include: list(&["dog"]),
            ..LibraryFilters::default()
        },
    )));
    assert_eq!(fx.executor.pending(), 2, "the save and the new query");
    fx.settle();

    assert_eq!(fx.calls("start_asset_query"), 3);
    // The save changed the vocabulary: the known tags were read again.
    assert_eq!(fx.calls("list_tags"), 2);
}

/// mutations.test.ts "publishes normalized groups and uses canonical bulk
/// acknowledgement".
#[test]
fn media_groups_are_published_with_the_trimmed_key_and_the_acknowledged_ids() {
    let mut fx = Fixture::loaded();
    fx.fake.on_set_asset_media_group(|_, _, _| Ok(()));
    fx.fake.on_set_assets_media_group_bulk(|_, _| {
        Ok(BulkMediaGroupSummary {
            processed_asset_ids: vec![1],
            processed_assets: 1,
            updated_assets: 1,
            media_group_key: Some("Trip".into()),
        })
    });

    fx.mutate(Mutation::SetMediaGroup {
        asset_id: 1,
        key: Some(" Trip ".into()),
        order: Some(2.0),
    });
    fx.settle();
    assert!(fx.fake.calls().contains(&Call::SetAssetMediaGroup {
        asset_id: 1,
        media_group_key: Some("Trip".into()),
        media_group_order: Some(2.0),
    }));
    fx.mutate(Mutation::SetMediaGroups {
        updates: vec![
            MediaGroupUpdate {
                asset_id: 1,
                media_group_order: Some(3.0),
            },
            MediaGroupUpdate {
                asset_id: 2,
                media_group_order: Some(4.0),
            },
        ],
        key: Some(" Trip ".into()),
    });
    fx.settle();

    let events = fx.events();
    let orders: Vec<_> = commits(&events)
        .into_iter()
        .map(|commit| match commit {
            MetadataCommit::Updated(patches) => {
                assert_eq!(patches.len(), 1);
                let (id, patch) = &patches[0];
                assert_eq!(patch.summary.media_group_key, Some(Some("Trip".into())));
                (*id, patch.summary.media_group_order)
            }
            other => panic!("unexpected commit {other:?}"),
        })
        .collect();
    assert_eq!(orders, [(1, Some(Some(2.0))), (1, Some(Some(3.0)))]);
    // The setup refresh and one after each group write.
    assert_eq!(fx.calls("start_asset_query"), 3);
}

/// mutations.test.ts "drains active and queued writes before maintenance, blocks new
/// admissions synchronously, resets even on failure".
#[test]
fn maintenance_waits_for_admitted_writes_refuses_new_ones_and_resets_inside_the_barrier() {
    let mut fx = Fixture::loaded();
    fx.fake.on_set_asset_favorite(|_, _| Ok(()));
    fx.fake.on_set_asset_media_group(|_, _, _| Ok(()));
    let write = fx.mutate(favorite(1, true));
    let queued = fx.mutate(Mutation::SetMediaGroup {
        asset_id: 1,
        key: Some("trip".into()),
        order: Some(1.0),
    });

    let maintenance = fx.ticket();
    fx.send(LibraryInput::BeginMaintenance {
        ticket: maintenance,
    });
    let denied = fx.mutate(favorite(2, true));
    let mut events = fx.events();
    assert_eq!(
        refused(&mut events, denied),
        Some(MutationError::Refused(TagStateError::Maintenance))
    );
    assert!(events.is_empty(), "the barrier is not granted yet");
    assert!(fx.library().maintenance_active());

    // Both admitted writes run before the operation may.
    fx.answer_next();
    let mut events = fx.events();
    assert!(matches!(settled(&mut events, write), Some(Ok(_))));
    assert!(!events
        .iter()
        .any(|event| matches!(event, LibraryEvent::MaintenanceReady { .. })));
    fx.settle();
    let mut events = fx.events();
    assert!(matches!(settled(&mut events, queued), Some(Ok(_))));
    assert!(events.iter().any(
        |event| matches!(event, LibraryEvent::MaintenanceReady { ticket } if *ticket == maintenance)
    ));
    assert_eq!(fx.calls("set_asset_media_group"), 1);

    // The operation failed; the reset happens all the same, before writes return.
    fx.send(LibraryInput::EndMaintenance {
        ticket: maintenance,
        reset: Some(LibraryReset::Cleared),
    });
    assert!(!fx.library().maintenance_active());
    assert_eq!(fx.library().query().total(), 0);
    assert_eq!(fx.library().query().cached_len(), 0);
    assert_eq!(fx.tags(1), None);
    assert!(fx.details(1).is_none());
    assert!(fx.library().known_tags().is_empty());

    let again = fx.mutate(favorite(1, true));
    fx.settle();
    assert!(matches!(settled(&mut fx.events(), again), Some(Ok(_))));
    assert_eq!(fx.calls("set_asset_favorite"), 2);
}

/// mutations.test.ts "does not publish a stale identity or a disposed owner's write
/// and releases waiting maintenance".
#[test]
fn a_write_of_another_identity_publishes_nothing_and_disposal_rejects_waiting_maintenance() {
    let mut fx = Fixture::loaded();
    let write = fx.mutate(save(1, TagEdit::Replace(list(&["dog"]))));
    fx.send(LibraryInput::Reset(LibraryReset::Restored));
    // The reused ID is read again under the new identity.
    let read = fx.load(1);
    assert!(fx.executor.run_nth(1), "the details answer before the save");
    fx.driver.pump();
    assert!(loaded(&mut fx.events(), read).flatten().is_some());

    let maintenance = fx.ticket();
    fx.send(LibraryInput::BeginMaintenance {
        ticket: maintenance,
    });
    fx.send(LibraryInput::Dispose);
    assert!(fx.events().iter().any(|event| matches!(
        event,
        LibraryEvent::MaintenanceRejected { ticket, error: TagStateError::Unavailable }
            if *ticket == maintenance
    )));

    fx.settle();
    let mut events = fx.events();
    // The backend stored the write; the caller learns so, and nothing is published.
    assert!(matches!(settled(&mut events, write), Some(Ok(_))));
    assert!(commits(&events).is_empty());
    assert_eq!(fx.tags(1), Some(list(&["cat"])));
    assert_eq!(fx.calls("list_tags"), 1);

    let late = fx.mutate(favorite(1, true));
    assert_eq!(
        refused(&mut fx.events(), late),
        Some(MutationError::Refused(TagStateError::Unavailable))
    );
}

/// mutations.test.ts "does not restore captured details or tags into a reused
/// identity".
#[test]
fn a_late_answer_does_not_restore_details_or_tags_into_a_reused_identity() {
    let mut fx = Fixture::loaded();
    fx.mutate(save(1, TagEdit::Replace(list(&["obsolete"]))));
    fx.send(LibraryInput::Reset(LibraryReset::Restored));
    fx.fake.on_get_asset_details(|id| {
        Ok(Some(AssetDetails {
            path: "/synthetic/reused.jpg".into(),
            size_bytes: 99,
            tags: list(&["current"]),
            ..detail(id)
        }))
    });
    fx.load(1);
    assert!(fx.executor.run_nth(1));
    fx.driver.pump();

    fx.settle();
    let details = fx.details(1).expect("the details of the new identity");
    assert_eq!(details.path, "/synthetic/reused.jpg");
    assert_eq!(details.size_bytes, 99);
    assert_eq!(fx.tags(1), Some(list(&["current"])));
}

/// mutations.test.ts "rejects a write queued behind its asset's deletion without
/// sending IPC" and "keeps deletion tombstones ...".
#[test]
fn a_deletion_leaves_a_tombstone_and_refuses_the_writes_queued_behind_it() {
    let mut fx = Fixture::loaded();
    fx.fake.on_delete_asset(|_| Ok(deleted()));
    fx.fake.on_set_asset_favorite(|_, _| Ok(()));
    let old_read = fx.driver.model_mut().tags.capture_generation(1);

    let removing = fx.mutate(Mutation::DeleteAsset { asset_id: 1 });
    let queued = fx.mutate(favorite(1, true));
    fx.fake.on_start_asset_query(|_| Ok(ready(&[])));
    fx.answer_next();

    // The row is gone before the refresh answered.
    assert!(fx.library().query().asset_at(0).is_none());
    assert!(fx.details(1).is_none());
    assert!(!fx
        .driver
        .model_mut()
        .tags
        .publish_details(1, list(&["old"]), &old_read));
    let mut events = fx.events();
    assert_eq!(
        refused(&mut events, queued),
        Some(MutationError::Refused(TagStateError::AssetDeleted))
    );
    assert_eq!(commits(&events), [&MetadataCommit::Deleted(vec![1])]);
    assert!(
        settled(&mut events, removing).is_none(),
        "the deletion answers after its refreshes"
    );

    fx.settle();
    assert!(matches!(
        settled(&mut fx.events(), removing),
        Some(Ok(MutationOutcome::Deleted(_)))
    ));
    assert_eq!(fx.calls("set_asset_favorite"), 0);
    assert_eq!(fx.library().query().total(), 0);
    assert_eq!(fx.calls("list_tags"), 2);
}

/// mutations.test.ts "deduplicates reads, retains exactly 256 details and reloads
/// evicted records" and "publishes saved tags without restoring an evicted complete
/// record".
#[test]
fn details_reads_are_shared_and_the_cache_keeps_256_records() {
    let mut fx = Fixture::loaded();
    let first = fx.load(2);
    let second = fx.load(2);
    assert_eq!(fx.executor.pending(), 1, "one read for both");
    fx.settle();
    let mut events = fx.events();
    assert!(loaded(&mut events, first).flatten().is_some());
    assert!(loaded(&mut events, second).flatten().is_some());

    for id in 3..=257 {
        fx.load(id);
    }
    fx.settle();
    fx.events();
    assert!(fx.details(1).is_none(), "the oldest record went");
    assert!(fx.details(2).is_some());
    assert_eq!(fx.library().details().len(), MAX_CACHED_DETAILS);
    assert_eq!(fx.calls("get_asset_details"), 257);

    // The tag state still knows asset 1; a save publishes tags and restores no
    // details.
    fx.mutate(save(1, TagEdit::Replace(list(&["dog"]))));
    fx.settle();
    assert_eq!(fx.tags(1), Some(list(&["dog"])));
    assert!(fx.details(1).is_none());
    fx.load(1);
    fx.settle();
    assert_eq!(fx.calls("get_asset_details"), 258);
}

/// mutations.test.ts "rejects a pending detail after %s invalidation".
#[test]
fn a_details_read_overtaken_by_a_new_query_a_reset_or_the_disposal_answers_nothing() {
    let overtake: [(&str, LibraryInput); 4] = [
        ("query", LibraryInput::Query(QueryInput::Refresh)),
        ("identity", LibraryInput::Reset(LibraryReset::Restored)),
        ("clear", LibraryInput::Reset(LibraryReset::Cleared)),
        ("dispose", LibraryInput::Dispose),
    ];
    for (mode, input) in overtake {
        let mut fx = Fixture::loaded();
        let read = fx.load(2);
        fx.send(input);
        assert!(
            matches!(loaded(&mut fx.events(), read), Some(None)),
            "{mode}: answered at that moment"
        );
        fx.settle();
        assert!(fx.details(2).is_none(), "{mode}");
        assert_eq!(fx.tags(2), None, "{mode}");
        assert_eq!(fx.library().details().in_flight(), 0, "{mode}");
    }
}

/// mutations.test.ts "rejects details read before or during writes and allows a
/// fresh read after settlement".
#[test]
fn a_details_read_crossed_by_a_write_is_dropped_and_a_fresh_one_is_accepted() {
    let mut fx = Fixture::loaded();
    fx.fake.on_set_asset_favorite(|_, _| Ok(()));
    let read = fx.load(2);
    fx.mutate(favorite(2, true));
    fx.settle();
    assert!(matches!(loaded(&mut fx.events(), read), Some(None)));
    assert!(fx.details(2).is_none());

    fx.fake.on_get_asset_details(|id| {
        let mut details = detail(id);
        details.summary.is_favorite = true;
        Ok(Some(details))
    });
    let fresh = fx.load(2);
    fx.settle();
    let details = loaded(&mut fx.events(), fresh).flatten().expect("details");
    assert!(details.summary.is_favorite);
}

/// mutations.test.ts "keeps vocabulary until restore refresh but clears it
/// immediately on library clear".
#[test]
fn a_restore_keeps_the_known_tags_and_a_clear_drops_them() {
    let mut fx = Fixture::loaded();
    let epoch = fx.library().tags().epoch();
    assert_eq!(**fx.library().known_tags(), ["cat", "dog"]);

    fx.send(LibraryInput::Reset(LibraryReset::Restored));
    assert_eq!(fx.library().tags().epoch(), epoch + 1);
    assert_eq!(fx.tags(1), None);
    assert!(fx.details(1).is_none());
    assert_eq!(fx.library().query().total(), 0);
    assert_eq!(**fx.library().known_tags(), ["cat", "dog"]);

    fx.send(LibraryInput::Reset(LibraryReset::Cleared));
    assert!(fx.library().known_tags().is_empty());
    assert_eq!(fx.library().query().viewport(), None);
}

#[test]
fn an_identity_reset_keeps_the_query_and_the_known_tags() {
    let mut fx = Fixture::loaded();
    fx.send(LibraryInput::Reset(LibraryReset::Identity));
    assert_eq!(fx.tags(1), None);
    assert!(fx.details(1).is_none());
    assert_eq!(fx.library().query().total(), 1);
    assert_eq!(**fx.library().known_tags(), ["cat", "dog"]);
}

#[test]
fn a_selection_write_takes_the_library_turn_and_drops_every_cached_tag_list() {
    let mut fx = Fixture::loaded();
    fx.fake.on_set_asset_favorite(|_, _| Ok(()));
    fx.fake.on_apply_selection_mutation(|_, _| {
        Ok(SelectionMutationResult::Ready {
            processed_assets: 3,
            updated_assets: 3,
            revision: 4,
            query_impact: TagQueryImpact::Tags {
                changed_tags: list(&["dog"]),
                tag_count_changed: true,
            },
            vocabulary_changed: true,
            is_favorite: None,
            media_group_key: None,
        })
    });
    let selection = SelectionSpec {
        session_id: 1,
        ranges: vec![[0, 2]],
        ..SelectionSpec::default()
    };

    let earlier = fx.mutate(favorite(1, true));
    let merge = fx.mutate(Mutation::MergeTagsInSelection {
        selection: selection.clone(),
        tags: list(&["dog"]),
    });
    // A later write on an unrelated asset waits for the selection write.
    let later = fx.mutate(favorite(9, true));
    assert_eq!(fx.executor.pending(), 1);
    assert_eq!(fx.calls("apply_selection_mutation"), 0);

    fx.answer_next();
    assert!(matches!(settled(&mut fx.events(), earlier), Some(Ok(_))));
    assert_eq!(fx.executor.pending(), 1, "the selection write alone");
    assert_eq!(
        fx.tags(1),
        None,
        "every cached tag list is dropped at its start"
    );
    assert!(fx.details(1).is_none());
    // A read during the write is answered when the write ends, with nothing.
    let during = fx.load(1);

    assert!(fx.executor.run_next(), "the selection write");
    fx.driver.pump();
    let mut events = fx.events();
    assert!(matches!(loaded(&mut events, during), Some(None)));
    assert!(matches!(
        settled(&mut events, merge),
        Some(Ok(MutationOutcome::Selection(
            SelectionMutationResult::Ready { .. }
        )))
    ));
    assert!(fx.fake.calls().contains(&Call::ApplySelectionMutation {
        selection,
        mutation: SelectionMutation::MergeTags {
            tags: list(&["dog"])
        },
    }));

    fx.settle();
    assert!(matches!(settled(&mut fx.events(), later), Some(Ok(_))));
    assert_eq!(fx.tags(1), None);
    // The vocabulary changed; the unfiltered query is not restarted by a tag merge.
    assert_eq!(fx.calls("list_tags"), 2);
    assert_eq!(fx.calls("start_asset_query"), 1);
}

#[test]
fn a_stale_selection_write_refreshes_the_query_and_a_favorite_toggle_rereads_the_rows() {
    let mut fx = Fixture::loaded();
    fx.fake
        .on_apply_selection_mutation(|_, _| Ok(SelectionMutationResult::Stale));
    let stale = fx.mutate(Mutation::SetMediaGroupInSelection {
        selection: SelectionSpec::default(),
        key: Some("  ".into()),
    });
    fx.settle();
    assert!(matches!(
        settled(&mut fx.events(), stale),
        Some(Ok(MutationOutcome::Selection(
            SelectionMutationResult::Stale
        )))
    ));
    assert!(fx.fake.calls().contains(&Call::ApplySelectionMutation {
        selection: SelectionSpec::default(),
        mutation: SelectionMutation::SetMediaGroup {
            media_group_key: None
        },
    }));
    assert_eq!(fx.calls("start_asset_query"), 2);

    fx.fake.on_apply_selection_mutation(|_, _| {
        Ok(SelectionMutationResult::Ready {
            processed_assets: 1,
            updated_assets: 1,
            revision: 5,
            query_impact: TagQueryImpact::None,
            vocabulary_changed: false,
            is_favorite: Some(true),
            media_group_key: None,
        })
    });
    fx.mutate(Mutation::ToggleFavoritesInSelection {
        selection: SelectionSpec::default(),
    });
    fx.settle();
    assert_eq!(fx.calls("start_asset_query"), 3);

    fx.fake
        .on_apply_selection_mutation(|_, _| Err(BackendError::rejected("i18n:common.libraryBusy")));
    let failed = fx.mutate(Mutation::ToggleFavoritesInSelection {
        selection: SelectionSpec::default(),
    });
    fx.settle();
    assert!(matches!(
        refused(&mut fx.events(), failed),
        Some(MutationError::Backend(_))
    ));
    assert_eq!(
        fx.calls("start_asset_query"),
        3,
        "a failed write refreshes nothing"
    );
    assert!(fx.library().tags().is_idle());
}

#[test]
fn idle_waiters_are_answered_when_the_queue_drained() {
    let mut fx = Fixture::loaded();
    fx.fake.on_set_asset_favorite(|_, _| Ok(()));
    let at_rest = fx.ticket();
    fx.send(LibraryInput::WaitForIdle { ticket: at_rest });
    assert!(matches!(
        fx.events()[..],
        [LibraryEvent::Idle { ticket }] if ticket == at_rest
    ));

    fx.mutate(favorite(1, true));
    fx.mutate(favorite(1, false));
    let busy = fx.ticket();
    fx.send(LibraryInput::WaitForIdle { ticket: busy });
    fx.answer_next();
    assert!(!fx
        .events()
        .iter()
        .any(|event| matches!(event, LibraryEvent::Idle { .. })));
    fx.settle();
    assert!(fx
        .events()
        .iter()
        .any(|event| matches!(event, LibraryEvent::Idle { ticket } if *ticket == busy)));
}

#[test]
fn pins_keep_a_tag_record_through_eviction() {
    let mut fx = Fixture::loaded();
    fx.send(LibraryInput::PinTags(1));
    for id in 2..=600 {
        fx.load(id);
    }
    fx.settle();
    assert_eq!(fx.tags(1), Some(list(&["cat"])), "the pinned record stays");
    assert_eq!(fx.tags(2), None, "an old unpinned record went");
    fx.send(LibraryInput::UnpinTags(1));
    assert_eq!(fx.tags(1), Some(list(&["cat"])));
}

/// The same model on background threads against the real core: canonical tags come
/// from the core's answers, a partial bulk result publishes only what the core
/// processed, and a deletion (in a temporary library) removes the row.
#[test]
fn the_library_writes_through_the_real_core() {
    let core = TempCore::start();
    for index in 0..3 {
        core.write_image(&format!("image-{index:02}.png"));
    }
    core.backend()
        .scan_folder(TempCore::text(&core.media_dir()))
        .unwrap();
    let executor = Arc::new(ThreadExecutor::new());
    let mut driver = Driver::new(
        Library::new(QueryConfig::default()),
        LibraryRunner::new(core.shared()),
        executor as Arc<dyn Executor>,
    );
    let mut events: Vec<LibraryEvent> = Vec::new();
    let wait = |driver: &mut Driver<Library, LibraryRunner>,
                events: &mut Vec<LibraryEvent>,
                done: &dyn Fn(&Library, &[LibraryEvent]) -> bool| {
        let deadline = std::time::Instant::now() + Duration::from_secs(60);
        loop {
            events.extend(driver.model_mut().take_events());
            if done(driver.model(), events) {
                return;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the library did not settle: {events:?}"
            );
            driver.pump_one(Duration::from_millis(50));
        }
    };
    let answered = |events: &[LibraryEvent], wanted: Ticket| {
        events.iter().any(|event| match event {
            LibraryEvent::Refreshed { ticket, .. }
            | LibraryEvent::DetailsLoaded { ticket, .. }
            | LibraryEvent::MutationSettled { ticket, .. } => *ticket == wanted,
            _ => false,
        })
    };

    driver.handle(LibraryInput::Refresh {
        ticket: Some(Ticket(1)),
    });
    wait(&mut driver, &mut events, &|_, events| {
        answered(events, Ticket(1))
    });
    assert_eq!(driver.model().query().total(), 3);
    assert!(driver.model().known_tags().is_empty());
    let ids: Vec<i64> = driver.model().query().assets().map(|row| row.id).collect();
    assert_eq!(ids.len(), 3);

    // Editing needs the complete tag list first.
    driver.handle(LibraryInput::LoadDetails {
        ticket: Ticket(2),
        asset_id: ids[0],
    });
    wait(&mut driver, &mut events, &|_, events| {
        answered(events, Ticket(2))
    });
    assert!(driver.model().can_edit_tags(ids[0]));

    // Two quick edits of one asset.
    driver.handle(LibraryInput::Mutate {
        ticket: Ticket(3),
        mutation: save(ids[0], TagEdit::Replace(list(&[" Dog ", "cat", "dog"]))),
    });
    driver.handle(LibraryInput::Mutate {
        ticket: Ticket(4),
        mutation: save(ids[0], add("bird")),
    });
    wait(&mut driver, &mut events, &|library, events| {
        answered(events, Ticket(4)) && !library.refreshing()
    });
    // The order is the core's (by name), not the order the edits were made in.
    assert_eq!(
        driver
            .model()
            .asset_tags(ids[0])
            .map(|tags| tags.tags.to_vec()),
        Some(list(&["bird", "cat", "dog"]))
    );
    assert_eq!(**driver.model().known_tags(), ["bird", "cat", "dog"]);

    // One of three targets does not exist: only the two processed assets get tags.
    driver.handle(LibraryInput::Mutate {
        ticket: Ticket(5),
        mutation: Mutation::MergeTags {
            asset_ids: vec![ids[1], 9_999, ids[2]],
            tags: list(&["trip"]),
        },
    });
    wait(&mut driver, &mut events, &|library, events| {
        answered(events, Ticket(5)) && !library.refreshing()
    });
    assert_eq!(
        driver
            .model()
            .asset_tags(ids[1])
            .map(|tags| tags.tags.to_vec()),
        Some(list(&["trip"]))
    );
    assert!(driver.model().asset_tags(9_999).is_none());
    assert!(driver.model().known_tags().contains(&"trip".to_owned()));

    driver.handle(LibraryInput::Mutate {
        ticket: Ticket(6),
        mutation: Mutation::DeleteAsset { asset_id: ids[2] },
    });
    wait(&mut driver, &mut events, &|_, events| {
        answered(events, Ticket(6))
    });
    assert_eq!(driver.model().query().total(), 2);
    assert!(events.iter().any(|event| matches!(
        event,
        LibraryEvent::Committed(MetadataCommit::Deleted(deleted)) if *deleted == [ids[2]]
    )));
    for event in &events {
        if let LibraryEvent::MutationSettled { ticket, result } = event {
            assert!(result.is_ok(), "{ticket:?}: {result:?}");
        }
    }
}

// ---- Ported behaviour cases (E08 test step) ----

/// mutations.test.ts "settles a saved favorite independently of its failed query
/// refresh": the write answers as soon as the backend stored it; the refresh it
/// started may fail later without touching the write or the queue.
#[test]
fn a_saved_favorite_settles_independently_of_its_failed_query_refresh() {
    let mut fx = Fixture::with_filters(LibraryFilters {
        favorites_only: true,
        ..LibraryFilters::default()
    });
    fx.fake.on_set_asset_favorite(|_, _| Ok(()));
    fx.send(LibraryInput::Refresh { ticket: None });
    fx.settle();
    fx.events();
    let starts = fx.calls("start_asset_query");
    fx.fake
        .on_start_asset_query(|_| Err(BackendError::rejected("refresh failed")));

    let write = fx.mutate(favorite(1, false));
    fx.answer_next();
    let mut events = fx.events();
    assert!(
        matches!(
            settled(&mut events, write),
            Some(Ok(MutationOutcome::FavoriteSet {
                asset_id: 1,
                favorite: false
            }))
        ),
        "the write answers before its refresh"
    );
    assert!(
        fx.library().query().query_pending(),
        "the favorites-only query is refreshed"
    );
    assert!(!fx.library().query().asset_at(0).unwrap().is_favorite);
    assert_eq!(fx.library().pending_writes(), 0);
    assert!(fx.library().tags().is_idle());

    fx.settle();
    assert_eq!(fx.calls("start_asset_query"), starts + 1);
    assert!(matches!(
        fx.library().query().load_error(),
        Some(QueryError::Backend(error)) if error.text() == "refresh failed"
    ));
    // The failed refresh answers nothing twice and leaves the queue free.
    assert!(settled(&mut fx.events(), write).is_none());
    let next = fx.mutate(favorite(1, true));
    fx.settle();
    assert!(matches!(settled(&mut fx.events(), next), Some(Ok(_))));
}

/// mutations.test.ts "refreshes favorites-only queries after removal and bulk
/// toggles": in a favorites-only result every bulk toggle restarts the query, even
/// when every asset was processed; outside it, only a partial toggle does.
#[test]
fn bulk_favorite_toggles_restart_a_favorites_only_query_even_when_complete() {
    let toggled = |fx: &mut Fixture| {
        fx.fake.on_toggle_assets_favorite_bulk(|ids| {
            Ok(BulkFavoriteSummary {
                processed_asset_ids: ids,
                is_favorite: true,
                revision: 3,
            })
        });
    };
    let mut fx = Fixture::with_filters(LibraryFilters {
        favorites_only: true,
        ..LibraryFilters::default()
    });
    toggled(&mut fx);
    fx.send(LibraryInput::Refresh { ticket: None });
    fx.settle();
    let before = fx.calls("start_asset_query");
    let toggle = fx.mutate(Mutation::ToggleFavorites { asset_ids: vec![1] });
    fx.settle();
    assert!(matches!(settled(&mut fx.events(), toggle), Some(Ok(_))));
    assert_eq!(fx.calls("start_asset_query"), before + 1);

    let mut fx = Fixture::loaded();
    toggled(&mut fx);
    let before = fx.calls("start_asset_query");
    fx.mutate(Mutation::ToggleFavorites { asset_ids: vec![1] });
    fx.settle();
    assert_eq!(
        fx.calls("start_asset_query"),
        before,
        "an unfiltered, complete toggle keeps the query"
    );
    assert!(fx.library().query().asset_at(0).unwrap().is_favorite);
}

/// Acceptance: a partial bulk result publishes only acknowledged IDs. An asset whose
/// tags were known and that the backend did not acknowledge keeps its canonical
/// list; nothing of the requested merge reaches it.
#[test]
fn a_partial_bulk_merge_leaves_the_unacknowledged_asset_on_its_canonical_tags() {
    let mut fx = Fixture::loaded();
    fx.load(2);
    fx.settle();
    fx.events();
    assert_eq!(fx.tags(2), Some(list(&["cat"])));
    fx.fake
        .on_merge_asset_tags_bulk(|_, _| Ok(merged(&[(1, &["cat", "dog"])])));

    let merge = fx.mutate(Mutation::MergeTags {
        asset_ids: vec![1, 2],
        tags: list(&["dog"]),
    });
    fx.settle();

    assert_eq!(fx.tags(1), Some(list(&["cat", "dog"])));
    assert_eq!(
        fx.tags(2),
        Some(list(&["cat"])),
        "not acknowledged: the list known before stays"
    );
    assert!(fx.library().can_edit_tags(2));
    assert!(
        fx.details(2).is_none_or(|details| details.tags == ["cat"]),
        "no cached details carry the unacknowledged merge"
    );
    let mut events = fx.events();
    match commits(&events)[..] {
        [MetadataCommit::Updated(patches)] => {
            assert_eq!(patches.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [1]);
        }
        ref other => panic!("unexpected commits {other:?}"),
    }
    assert!(matches!(settled(&mut events, merge), Some(Ok(_))));
}

/// Acceptance: a partial bulk favorite toggle patches only the acknowledged rows of
/// the query, before the refresh it starts has answered.
#[test]
fn a_partial_bulk_favorite_toggle_patches_only_the_acknowledged_rows() {
    let mut fx = Fixture::new();
    fx.fake.on_start_asset_query(|_| Ok(ready(&[1, 2])));
    fx.send(LibraryInput::Refresh { ticket: None });
    fx.settle();
    fx.fake.on_toggle_assets_favorite_bulk(|_| {
        Ok(BulkFavoriteSummary {
            processed_asset_ids: vec![1],
            is_favorite: true,
            revision: 3,
        })
    });

    fx.mutate(Mutation::ToggleFavorites {
        asset_ids: vec![1, 2],
    });
    fx.answer_next();
    let query = fx.library().query();
    assert!(query.asset_at(0).unwrap().is_favorite);
    assert!(
        !query.asset_at(1).unwrap().is_favorite,
        "asset 2 was not acknowledged"
    );
}

/// Acceptance: tag editing is blocked until the complete tag list is known, also
/// for an edit admitted while the list was known: when a selection-wide write
/// ahead of it dropped every cached list, the edit is refused at its start and
/// nothing is sent.
#[test]
fn a_queued_tag_edit_is_refused_when_its_list_was_dropped_before_it_started() {
    let mut fx = Fixture::loaded();
    fx.fake.on_set_asset_favorite(|_, _| Ok(()));
    fx.fake.on_apply_selection_mutation(|_, _| {
        Ok(SelectionMutationResult::Ready {
            processed_assets: 1,
            updated_assets: 1,
            revision: 4,
            query_impact: TagQueryImpact::None,
            vocabulary_changed: false,
            is_favorite: None,
            media_group_key: None,
        })
    });

    let earlier = fx.mutate(favorite(1, true));
    fx.mutate(Mutation::MergeTagsInSelection {
        selection: SelectionSpec::default(),
        tags: list(&["bird"]),
    });
    assert!(
        fx.library().can_edit_tags(1),
        "the list is known at admission"
    );
    let edit = fx.mutate(save(1, add("dog")));
    assert!(
        settled(&mut fx.events(), edit).is_none(),
        "admitted, waiting"
    );

    fx.settle();
    let mut events = fx.events();
    assert!(matches!(settled(&mut events, earlier), Some(Ok(_))));
    assert_eq!(
        refused(&mut events, edit),
        Some(MutationError::TagsNotLoaded)
    );
    assert_eq!(fx.calls("set_asset_tags"), 0);
    assert!(!fx.library().can_edit_tags(1));
    assert!(fx.library().tags().is_idle());
    assert_eq!(fx.library().pending_writes(), 0);
}

/// Acceptance: an identity reset (CSV import, restore) makes every complete tag
/// list unknown again; tag edits are refused until the details are read again.
#[test]
fn a_tag_edit_after_an_identity_reset_waits_for_fresh_details() {
    for reset in [
        LibraryReset::Identity,
        LibraryReset::Restored,
        LibraryReset::Cleared,
    ] {
        let mut fx = Fixture::loaded();
        assert!(fx.library().can_edit_tags(1));
        fx.send(LibraryInput::Reset(reset));
        assert!(!fx.library().can_edit_tags(1), "{reset:?}");
        let ticket = fx.mutate(save(1, add("dog")));
        assert_eq!(
            refused(&mut fx.events(), ticket),
            Some(MutationError::TagsNotLoaded),
            "{reset:?}"
        );
        assert_eq!(fx.calls("set_asset_tags"), 0, "{reset:?}");

        fx.load(1);
        fx.settle();
        assert!(fx.library().can_edit_tags(1), "{reset:?}");
    }
}

/// createAssetTagState.test.ts "serializes maintenance requests and releases the
/// barrier after thrown errors", through the library: the second request is granted
/// only after the first ended, a failed operation still lifts its barrier, and the
/// `metadata` kind (`LibraryReset::Identity`) keeps the query and the known tags.
#[test]
fn maintenance_requests_run_one_after_the_other_and_a_failed_one_lifts_its_barrier() {
    let mut fx = Fixture::loaded();
    fx.fake.on_set_asset_favorite(|_, _| Ok(()));
    let first = fx.ticket();
    let second = fx.ticket();
    fx.send(LibraryInput::BeginMaintenance { ticket: first });
    fx.send(LibraryInput::BeginMaintenance { ticket: second });
    let events = fx.events();
    let ready: Vec<Ticket> = events
        .iter()
        .filter_map(|event| match event {
            LibraryEvent::MaintenanceReady { ticket } => Some(*ticket),
            _ => None,
        })
        .collect();
    assert_eq!(ready, [first], "one operation at a time");

    let denied = fx.mutate(favorite(8, true));
    assert_eq!(
        refused(&mut fx.events(), denied),
        Some(MutationError::Refused(TagStateError::Maintenance))
    );

    // The first operation failed: its reset still applies and its barrier comes down.
    fx.send(LibraryInput::EndMaintenance {
        ticket: first,
        reset: Some(LibraryReset::Identity),
    });
    assert!(fx.events().iter().any(
        |event| matches!(event, LibraryEvent::MaintenanceReady { ticket } if *ticket == second)
    ));
    assert_eq!(fx.tags(1), None);
    assert_eq!(
        fx.library().query().total(),
        1,
        "the metadata kind keeps the query"
    );
    assert_eq!(**fx.library().known_tags(), ["cat", "dog"]);
    let still = fx.mutate(favorite(8, true));
    assert_eq!(
        refused(&mut fx.events(), still),
        Some(MutationError::Refused(TagStateError::Maintenance)),
        "the second barrier is up"
    );

    fx.send(LibraryInput::EndMaintenance {
        ticket: second,
        reset: None,
    });
    assert!(!fx.library().maintenance_active());
    let admitted = fx.mutate(favorite(8, true));
    fx.settle();
    assert!(matches!(settled(&mut fx.events(), admitted), Some(Ok(_))));
}

/// Scope "LRU cache of 256 entries, cleared on a new query": applying other filters
/// (a new query epoch) drops every cached detail, while the canonical tag lists stay.
#[test]
fn new_filters_empty_the_details_cache_but_keep_the_tag_lists() {
    let mut fx = Fixture::loaded();
    assert!(fx.details(1).is_some());
    let generation = fx.library().details().generation();

    fx.send(LibraryInput::Query(QueryInput::ApplyFilters(
        LibraryFilters {
            include: list(&["cat"]),
            ..LibraryFilters::default()
        },
    )));
    assert!(fx.details(1).is_none());
    assert!(fx.library().details().is_empty());
    assert!(fx.library().details().generation() > generation);
    assert_eq!(fx.tags(1), Some(list(&["cat"])));

    // The query's answer is a new epoch as well; a read after it is kept.
    fx.settle();
    assert!(fx.details(1).is_none());
    assert_eq!(fx.tags(1), Some(list(&["cat"])));
    let read = fx.load(1);
    fx.settle();
    assert!(loaded(&mut fx.events(), read).flatten().is_some());
    assert!(fx.details(1).is_some());
    assert_eq!(
        fx.calls("get_asset_details"),
        2,
        "read again for the new query"
    );
}
