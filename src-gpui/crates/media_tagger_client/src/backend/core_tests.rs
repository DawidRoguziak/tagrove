//! Every method of the `Backend` port on the real core: a temporary profile, a
//! temporary media directory and synthetic images (`TempCore`). The tests go through
//! the port only, as a model does. Destructive calls (delete, clear, restore,
//! duplicate resolution) therefore never see anything but their own temporary files.

use std::{
    fs,
    path::Path,
    sync::{mpsc, Arc, Mutex},
    time::Duration,
};

use media_tagger_core::models::{
    AssetQueryPageResult, AssetQueryPositionResult, AssetQuerySectionsResult, AssetSummary,
    DbRootMapping, DeleteSourceStatus, DuplicateResolutionBatchInput,
    DuplicateResolutionBatchStatus, DuplicateResolutionChangeInput, DuplicateResolutionItemStatus,
    ScanProgress, SelectionMutationResult, StartAssetQueryResult, TagQueryImpact,
};

use super::{
    core_backend::{test_support::TempCore, AttachError},
    AssetIdRange, AssetQueryRequest, Backend, BackendErrorKind, CoreBackend, MediaGroupUpdate,
    MediaKind, MetaFilter, SelectionMutation, SelectionSpec, ThumbnailReply, ThumbnailRequest,
    ThumbnailResponder,
};
use crate::{
    exec::{Executor, ThreadExecutor},
    model::Driver,
    thumbs::{
        ThumbnailEvent, ThumbnailFetcher, ThumbnailInput, ThumbnailKey, ThumbnailMode,
        ThumbnailPipeline,
    },
};

const WAIT: Duration = Duration::from_secs(60);

/// A core with `count` indexed images `image-00.png`, `image-01.png`, ...
fn library(count: usize) -> TempCore {
    let core = TempCore::start();
    for index in 0..count {
        core.write_image(&format!("image-{index:02}.png"));
    }
    let summary = core
        .backend()
        .scan_folder(TempCore::text(&core.media_dir()))
        .expect("scan the media directory");
    assert_eq!(summary.indexed, count);
    core
}

struct Session {
    id: u64,
    total: usize,
    items: Vec<AssetSummary>,
}

fn query(backend: &CoreBackend, request: AssetQueryRequest) -> Session {
    match backend.start_asset_query(request).expect("start the query") {
        StartAssetQueryResult::Ready {
            session_id,
            total,
            items,
            ..
        } => Session {
            id: session_id,
            total,
            items,
        },
        StartAssetQueryResult::Superseded => panic!("the query was superseded"),
    }
}

/// The session of an unfiltered query by a fresh owner.
fn everything(backend: &CoreBackend, owner: &str) -> Session {
    query(backend, AssetQueryRequest::new(owner, 1))
}

fn ids(items: &[AssetSummary]) -> Vec<i64> {
    items.iter().map(|item| item.id).collect()
}

fn names(items: &[AssetSummary]) -> Vec<&str> {
    let mut names: Vec<&str> = items.iter().map(|item| item.file_name.as_str()).collect();
    names.sort_unstable();
    names
}

fn tags_of(backend: &CoreBackend, asset_id: i64) -> Vec<String> {
    backend
        .get_asset_details(asset_id)
        .expect("read the details")
        .expect("the asset exists")
        .tags
}

/// Collects the progress phases published while the returned guard lives.
fn phases(
    backend: &CoreBackend,
) -> (
    Arc<Mutex<Vec<String>>>,
    crate::progress::ProgressSubscription,
) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let subscription = backend.subscribe_progress(Arc::new(move |progress: &ScanProgress| {
        sink.lock().unwrap().push(progress.phase.clone());
    }));
    (seen, subscription)
}

fn thumbnail(backend: &CoreBackend, request: ThumbnailRequest) -> ThumbnailReply {
    let (sender, receiver) = mpsc::channel();
    backend.request_thumbnail(
        request,
        ThumbnailResponder::new(move |reply| {
            let _ = sender.send(reply);
        }),
    );
    receiver
        .recv_timeout(WAIT)
        .expect("the request is answered")
}

fn visible(asset: &AssetSummary) -> ThumbnailRequest {
    ThumbnailRequest {
        asset_id: asset.id,
        version: asset.thumb_version.clone(),
        cached_only: false,
        prefetch: false,
        retry: 0,
    }
}

fn is_jpeg(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xff, 0xd8, 0xff])
}

fn out_file(core: &TempCore, name: &str) -> String {
    let directory = core.root().join("out");
    fs::create_dir_all(&directory).expect("create the output directory");
    TempCore::text(&directory.join(name))
}

// Query sessions and reads.

#[test]
fn start_asset_query_returns_the_first_page_of_a_session() {
    let core = library(5);
    let backend = core.backend();

    let mut request = AssetQueryRequest::new("gallery", 1);
    request.page_size = 3;
    let session = query(backend, request);

    assert_eq!(session.total, 5);
    assert_eq!(session.items.len(), 3);
    assert!(session.items.iter().all(|item| item.kind == "image"));
    assert!(session
        .items
        .iter()
        .all(|item| item.thumb_version.len() == super::THUMB_VERSION_LEN));
    assert_eq!(
        (session.items[0].width, session.items[0].height),
        (Some(8), Some(6))
    );
}

#[test]
fn start_asset_query_applies_filters_and_the_anchor_offset() {
    let core = library(4);
    let backend = core.backend();
    let all = everything(backend, "seed");
    backend
        .set_asset_tags(all.items[0].id, vec!["cat".into()])
        .unwrap();
    backend.set_asset_favorite(all.items[1].id, true).unwrap();

    let mut tagged = AssetQueryRequest::new("tagged", 1);
    tagged.tags_and = vec!["cat".into()];
    assert_eq!(ids(&query(backend, tagged).items), [all.items[0].id]);

    let mut without = AssetQueryRequest::new("without", 1);
    without.tags_not = vec!["cat".into()];
    assert_eq!(query(backend, without).total, 3);

    let mut favorites = AssetQueryRequest::new("favorites", 1);
    favorites.favorites_only = true;
    assert_eq!(ids(&query(backend, favorites).items), [all.items[1].id]);

    let mut untagged = AssetQueryRequest::new("untagged", 1);
    untagged.meta_filter = Some(MetaFilter::HasNoTags { tag_count: 0 });
    assert_eq!(query(backend, untagged).total, 3);

    let mut videos = AssetQueryRequest::new("videos", 1);
    videos.kind = Some(MediaKind::Video);
    assert_eq!(query(backend, videos).total, 0);

    let mut anchored = AssetQueryRequest::new("anchored", 1);
    anchored.page_size = 2;
    anchored.anchor_offset = Some(3);
    let StartAssetQueryResult::Ready { offset, items, .. } =
        backend.start_asset_query(anchored).unwrap()
    else {
        panic!("the anchored query is ready");
    };
    assert_eq!(offset, 2);
    assert_eq!(ids(&items), ids(&all.items[2..4]));
}

#[test]
fn start_asset_query_supersedes_an_older_generation_of_the_same_owner() {
    let core = library(2);
    let backend = core.backend();

    // The newer generation was admitted first, as when two calls of one owner
    // run out of order on the executor.
    assert!(matches!(
        backend.start_asset_query(AssetQueryRequest::new("owner", 2)),
        Ok(StartAssetQueryResult::Ready { .. })
    ));
    assert_eq!(
        backend.start_asset_query(AssetQueryRequest::new("owner", 1)),
        Ok(StartAssetQueryResult::Superseded)
    );
    // Another owner has its own generations.
    assert!(matches!(
        backend.start_asset_query(AssetQueryRequest::new("other", 1)),
        Ok(StartAssetQueryResult::Ready { .. })
    ));
}

#[test]
fn start_asset_query_passes_validation_errors_on_with_the_core_text() {
    let core = library(1);
    let backend = core.backend();

    let error = backend
        .start_asset_query(AssetQueryRequest::new("", 1))
        .unwrap_err();
    assert_eq!(error.kind(), BackendErrorKind::Rejected);
    assert_eq!(error.text(), "Invalid query owner ID");

    let mut negative = AssetQueryRequest::new("owner", 1);
    negative.meta_filter = Some(MetaFilter::HasNoTags { tag_count: -1 });
    assert_eq!(
        backend.start_asset_query(negative).unwrap_err().text(),
        "Invalid hasNoTags meta filter"
    );

    let mut blank = AssetQueryRequest::new("owner", 2);
    blank.meta_filter = Some(MetaFilter::GroupName {
        group_name: "  ".into(),
    });
    assert_eq!(
        backend.start_asset_query(blank).unwrap_err().text(),
        "Invalid groupName meta filter"
    );
}

#[test]
fn get_asset_query_page_reads_later_pages_and_reports_a_stale_session() {
    let core = library(5);
    let backend = core.backend();
    let session = everything(backend, "gallery");

    let AssetQueryPageResult::Ready {
        session_id,
        total,
        offset,
        items,
        ..
    } = backend.get_asset_query_page(session.id, 3, 10).unwrap()
    else {
        panic!("the page is ready");
    };

    assert_eq!((session_id, total, offset), (session.id, 5, 3));
    assert_eq!(ids(&items), ids(&session.items[3..]));
    assert_eq!(
        backend.get_asset_query_page(session.id + 1000, 0, 10),
        Ok(AssetQueryPageResult::Stale)
    );
}

#[test]
fn get_asset_query_position_resolves_an_asset_in_its_session() {
    let core = library(3);
    let backend = core.backend();
    let session = everything(backend, "gallery");

    assert_eq!(
        backend.get_asset_query_position(session.id, session.items[2].id),
        Ok(AssetQueryPositionResult::Resolved { index: 2 })
    );
    assert_eq!(
        backend.get_asset_query_position(session.id, 9_999_999),
        Ok(AssetQueryPositionResult::Missing)
    );
    assert_eq!(
        backend.get_asset_query_position(session.id + 1000, session.items[0].id),
        Ok(AssetQueryPositionResult::Stale)
    );
}

#[test]
fn get_asset_query_id_range_returns_typed_ids_in_session_order() {
    let core = library(5);
    let backend = core.backend();
    let session = everything(backend, "gallery");
    let all = ids(&session.items);

    assert_eq!(
        backend.get_asset_query_id_range(session.id, 1, 3),
        Ok(AssetIdRange::Ready(all[1..=3].to_vec()))
    );
    // The backend clamps the end and answers an empty range past the snapshot.
    assert_eq!(
        backend.get_asset_query_id_range(session.id, 3, 99),
        Ok(AssetIdRange::Ready(all[3..].to_vec()))
    );
    assert_eq!(
        backend.get_asset_query_id_range(session.id, 7, 9),
        Ok(AssetIdRange::Ready(Vec::new()))
    );
    assert_eq!(
        backend.get_asset_query_id_range(session.id + 1000, 0, 4),
        Ok(AssetIdRange::Stale)
    );
}

#[test]
fn get_asset_query_sections_covers_the_whole_session() {
    let core = library(4);
    let backend = core.backend();
    let session = everything(backend, "gallery");

    let AssetQuerySectionsResult::Ready {
        session_id,
        total,
        sections,
    } = backend.get_asset_query_sections(session.id).unwrap()
    else {
        panic!("the sections are ready");
    };

    assert_eq!((session_id, total), (session.id, 4));
    assert_eq!(
        sections.iter().map(|section| section.count).sum::<usize>(),
        4
    );
    assert_eq!(sections[0].start, 0);
    assert_eq!(
        backend.get_asset_query_sections(session.id + 1000),
        Ok(AssetQuerySectionsResult::Stale)
    );
}

#[test]
fn get_asset_summaries_by_ids_skips_unknown_ids_and_limits_the_batch() {
    let core = library(3);
    let backend = core.backend();
    let session = everything(backend, "gallery");
    let wanted = vec![session.items[2].id, session.items[0].id, 9_999_999];

    let summaries = backend.get_asset_summaries_by_ids(wanted).unwrap();

    let mut found = ids(&summaries);
    found.sort_unstable();
    let mut expected = vec![session.items[0].id, session.items[2].id];
    expected.sort_unstable();
    assert_eq!(found, expected);
    assert_eq!(
        backend
            .get_asset_summaries_by_ids(vec![1; super::MAX_SUMMARY_BATCH + 1])
            .unwrap_err()
            .text(),
        "At most 256 asset IDs are allowed"
    );
    assert!(backend
        .get_asset_summaries_by_ids(vec![session.items[0].id; super::MAX_SUMMARY_BATCH])
        .is_ok());
}

#[test]
fn get_asset_details_reads_the_path_and_tags_of_one_asset() {
    let core = library(1);
    let backend = core.backend();
    let asset = &everything(backend, "gallery").items[0];
    backend
        .set_asset_tags(asset.id, vec!["cat".into()])
        .unwrap();

    let details = backend.get_asset_details(asset.id).unwrap().unwrap();

    assert_eq!(details.summary.id, asset.id);
    assert_eq!(details.summary.file_name, "image-00.png");
    assert_eq!(
        Path::new(&details.path),
        core.media_dir().join("image-00.png")
    );
    assert_eq!(
        details.size_bytes,
        super::core_backend::test_support::PNG_FIXTURE.len() as i64
    );
    assert_eq!(details.tags, ["cat"]);
    assert!(backend.get_asset_details(9_999_999).unwrap().is_none());
}

#[test]
fn list_tags_pages_the_vocabulary_and_filters_by_text() {
    let core = library(1);
    let backend = core.backend();
    let asset = &everything(backend, "gallery").items[0];
    backend
        .set_asset_tags(asset.id, vec!["cat".into(), "castle".into(), "dog".into()])
        .unwrap();

    let all = backend.list_tags(String::new(), 0, 10).unwrap();
    assert_eq!(all.total, 3);
    assert_eq!(all.items.len(), 3);

    let matching = backend.list_tags("ca".into(), 0, 10).unwrap();
    assert_eq!(matching.total, 2);
    let mut found = matching.items;
    found.sort_unstable();
    assert_eq!(found, ["castle", "cat"]);

    let second = backend.list_tags(String::new(), 2, 10).unwrap();
    assert_eq!((second.total, second.items.len()), (3, 1));
}

// Tags, favorites and media groups.

#[test]
fn set_asset_tags_stores_the_normalized_tags_and_returns_the_canonical_result() {
    let core = library(1);
    let backend = core.backend();
    let asset = &everything(backend, "gallery").items[0];

    let summary = backend
        .set_asset_tags(asset.id, vec!["  Cat ".into(), "dog".into(), "cat".into()])
        .unwrap();

    assert_eq!(summary.asset_id, asset.id);
    assert!(summary.changed && summary.vocabulary_changed);
    let mut tags = summary.tags.clone();
    tags.sort_unstable();
    assert_eq!(tags, ["cat", "dog"]);
    assert!(matches!(summary.query_impact, TagQueryImpact::Tags { .. }));
    assert!(summary.revision > 0);
    let mut stored = tags_of(backend, asset.id);
    stored.sort_unstable();
    assert_eq!(stored, ["cat", "dog"]);

    let again = backend
        .set_asset_tags(asset.id, vec!["dog".into(), "cat".into()])
        .unwrap();
    assert!(!again.changed);
    assert!(backend
        .set_asset_tags(asset.id, vec!["two words".into()])
        .is_err());
    let mut unchanged = tags_of(backend, asset.id);
    unchanged.sort_unstable();
    assert_eq!(unchanged, ["cat", "dog"]);
}

#[test]
fn merge_asset_tags_bulk_adds_tags_without_removing_any() {
    let core = library(3);
    let backend = core.backend();
    let session = everything(backend, "gallery");
    let (first, second) = (session.items[0].id, session.items[1].id);
    backend.set_asset_tags(first, vec!["cat".into()]).unwrap();

    let summary = backend
        .merge_asset_tags_bulk(vec![first, second, first], vec!["Travel".into()])
        .unwrap();

    assert_eq!((summary.processed_assets, summary.updated_assets), (2, 2));
    assert_eq!(summary.processed_asset_ids, [first, second]);
    assert_eq!(summary.results.len(), 2);
    let mut merged = tags_of(backend, first);
    merged.sort_unstable();
    assert_eq!(merged, ["cat", "travel"]);
    assert_eq!(tags_of(backend, second), ["travel"]);
    assert!(tags_of(backend, session.items[2].id).is_empty());
}

#[test]
fn set_asset_favorite_marks_and_unmarks_one_asset() {
    let core = library(2);
    let backend = core.backend();
    let asset = everything(backend, "gallery").items[0].id;
    let favorite = |backend: &CoreBackend| {
        backend
            .get_asset_details(asset)
            .unwrap()
            .unwrap()
            .summary
            .is_favorite
    };

    backend.set_asset_favorite(asset, true).unwrap();
    assert!(favorite(backend));
    backend.set_asset_favorite(asset, false).unwrap();
    assert!(!favorite(backend));
}

#[test]
fn toggle_assets_favorite_bulk_favorites_all_unless_all_already_are() {
    let core = library(3);
    let backend = core.backend();
    let session = everything(backend, "gallery");
    let selected = vec![session.items[0].id, session.items[1].id];
    backend.set_asset_favorite(selected[0], true).unwrap();

    let mixed = backend
        .toggle_assets_favorite_bulk(selected.clone())
        .unwrap();
    assert!(mixed.is_favorite);
    assert_eq!(mixed.processed_asset_ids, selected);

    let all = backend
        .toggle_assets_favorite_bulk(selected.clone())
        .unwrap();
    assert!(!all.is_favorite);
    assert!(all.revision > mixed.revision);
    let summaries = backend.get_asset_summaries_by_ids(selected).unwrap();
    assert!(summaries.iter().all(|summary| !summary.is_favorite));
}

#[test]
fn set_asset_media_group_assigns_and_clears_the_group_of_one_asset() {
    let core = library(2);
    let backend = core.backend();
    let asset = everything(backend, "gallery").items[0].id;
    let group = |backend: &CoreBackend| {
        let summary = backend.get_asset_details(asset).unwrap().unwrap().summary;
        (summary.media_group_key, summary.media_group_order)
    };

    backend
        .set_asset_media_group(asset, Some("  trip ".into()), Some(2.0))
        .unwrap();
    assert_eq!(group(backend), (Some("trip".into()), Some(2.0)));

    let mut grouped = AssetQueryRequest::new("grouped", 1);
    grouped.meta_filter = Some(MetaFilter::GroupName {
        group_name: "trip".into(),
    });
    assert_eq!(ids(&query(backend, grouped).items), [asset]);

    backend.set_asset_media_group(asset, None, None).unwrap();
    assert_eq!(group(backend), (None, None));
}

#[test]
fn set_assets_media_group_bulk_groups_assets_in_the_given_order() {
    let core = library(3);
    let backend = core.backend();
    let session = everything(backend, "gallery");
    let (first, second) = (session.items[0].id, session.items[1].id);

    let summary = backend
        .set_assets_media_group_bulk(
            vec![
                MediaGroupUpdate {
                    asset_id: second,
                    media_group_order: Some(1.0),
                },
                MediaGroupUpdate {
                    asset_id: first,
                    media_group_order: Some(2.0),
                },
            ],
            Some("trip".into()),
        )
        .unwrap();

    assert_eq!((summary.processed_assets, summary.updated_assets), (2, 2));
    assert_eq!(summary.media_group_key.as_deref(), Some("trip"));
    let mut processed = summary.processed_asset_ids;
    processed.sort_unstable();
    let mut expected = vec![first, second];
    expected.sort_unstable();
    assert_eq!(processed, expected);
    let details = backend.get_asset_details(second).unwrap().unwrap();
    assert_eq!(details.summary.media_group_key.as_deref(), Some("trip"));
    assert_eq!(details.summary.media_group_order, Some(1.0));
    let untouched = backend
        .get_asset_details(session.items[2].id)
        .unwrap()
        .unwrap();
    assert_eq!(untouched.summary.media_group_key, None);
}

// Selection.

#[test]
fn apply_selection_mutation_writes_to_the_assets_the_spec_denotes() {
    let core = library(5);
    let backend = core.backend();
    let session = everything(backend, "gallery");
    let all = ids(&session.items);
    let spec = SelectionSpec {
        session_id: session.id,
        ranges: vec![[0, 2]],
        excluded: vec![all[1]],
        included: vec![all[4]],
    };

    let merged = backend
        .apply_selection_mutation(
            spec.clone(),
            SelectionMutation::MergeTags {
                tags: vec!["Trip".into()],
            },
        )
        .unwrap();

    let SelectionMutationResult::Ready {
        processed_assets,
        updated_assets,
        vocabulary_changed,
        ..
    } = merged
    else {
        panic!("the selection is current");
    };
    assert_eq!((processed_assets, updated_assets), (3, 3));
    assert!(vocabulary_changed);
    for (index, id) in all.iter().enumerate() {
        let expected: &[&str] = if [0, 2, 4].contains(&index) {
            &["trip"]
        } else {
            &[]
        };
        assert_eq!(tags_of(backend, *id), expected, "asset at {index}");
    }
}

#[test]
fn apply_selection_mutation_toggles_favorites_and_groups_a_selection() {
    let core = library(3);
    let backend = core.backend();
    let spec = |backend: &CoreBackend, owner: &str| SelectionSpec {
        session_id: everything(backend, owner).id,
        ranges: vec![[0, 1]],
        ..SelectionSpec::default()
    };

    let favorite = backend
        .apply_selection_mutation(spec(backend, "first"), SelectionMutation::ToggleFavorite)
        .unwrap();
    assert!(matches!(
        favorite,
        SelectionMutationResult::Ready {
            processed_assets: 2,
            is_favorite: Some(true),
            ..
        }
    ));

    let grouped = backend
        .apply_selection_mutation(
            spec(backend, "second"),
            SelectionMutation::SetMediaGroup {
                media_group_key: Some(" trip ".into()),
            },
        )
        .unwrap();
    let SelectionMutationResult::Ready {
        processed_assets,
        media_group_key,
        ..
    } = grouped
    else {
        panic!("the selection is current");
    };
    assert_eq!(processed_assets, 2);
    assert_eq!(media_group_key.as_deref(), Some("trip"));
}

#[test]
fn apply_selection_mutation_refuses_a_stale_session_and_an_empty_tag_list() {
    let core = library(2);
    let backend = core.backend();
    let session = everything(backend, "gallery");
    let spec = |session_id| SelectionSpec {
        session_id,
        ranges: vec![[0, 1]],
        ..SelectionSpec::default()
    };

    assert_eq!(
        backend
            .apply_selection_mutation(spec(session.id + 1000), SelectionMutation::ToggleFavorite),
        Ok(SelectionMutationResult::Stale)
    );
    assert_eq!(
        backend
            .apply_selection_mutation(
                spec(session.id),
                SelectionMutation::MergeTags { tags: Vec::new() }
            )
            .unwrap_err()
            .text(),
        "Enter at least one tag."
    );
    let summaries = backend
        .get_asset_summaries_by_ids(ids(&session.items))
        .unwrap();
    assert!(summaries.iter().all(|summary| !summary.is_favorite));
}

// Deleting and duplicates.

#[test]
fn delete_asset_removes_the_source_file_and_the_asset() {
    let core = library(2);
    let backend = core.backend();
    let session = everything(backend, "gallery");
    let victim = &session.items[0];
    let source = core.media_dir().join(&victim.file_name);
    assert!(source.is_file());

    let summary = backend.delete_asset(victim.id).unwrap();

    assert_eq!(summary.removed_assets, 1);
    assert!(matches!(summary.source_status, DeleteSourceStatus::Deleted));
    assert_eq!(summary.recovery_path, None);
    assert!(!source.exists());
    assert!(backend.get_asset_details(victim.id).unwrap().is_none());
    assert_eq!(everything(backend, "after").total, 1);
    // The other file of the temporary root is untouched.
    assert!(core.media_dir().join(&session.items[1].file_name).is_file());
}

#[test]
fn find_duplicate_assets_groups_equal_file_names_and_reports_progress() {
    let core = TempCore::start();
    core.write_image("a/same.png");
    core.write_image("b/same.png");
    core.write_image("a/unique.png");
    let backend = core.backend();
    backend
        .scan_folder(TempCore::text(&core.media_dir()))
        .unwrap();
    let (seen, subscription) = phases(backend);

    let summary = backend.find_duplicate_assets().unwrap();
    drop(subscription);

    assert_eq!((summary.duplicate_groups, summary.duplicate_assets), (1, 2));
    assert_eq!(summary.groups[0].file_name, "same.png");
    assert_eq!(summary.groups[0].assets.len(), 2);
    assert_eq!(
        *seen.lock().unwrap(),
        ["duplicates-scan", "duplicates-scan-done"]
    );
}

#[test]
fn apply_duplicate_resolution_batch_renames_and_deletes_on_temporary_roots() {
    let core = TempCore::start();
    let kept = core.write_image("a/same.png");
    let renamed = core.write_image("b/same.png");
    let deleted = core.write_image("c/same.png");
    let backend = core.backend();
    backend
        .scan_folder(TempCore::text(&core.media_dir()))
        .unwrap();
    let scan = backend.find_duplicate_assets().unwrap();
    let asset = |path: &Path| {
        scan.groups[0]
            .assets
            .iter()
            .find(|asset| Path::new(&asset.path) == path)
            .expect("the duplicate is listed")
            .clone()
    };
    let (rename, delete) = (asset(&renamed), asset(&deleted));

    let summary = backend
        .apply_duplicate_resolution_batch(DuplicateResolutionBatchInput {
            scan_revision: scan.revision,
            changes: vec![
                DuplicateResolutionChangeInput::Rename {
                    asset_id: rename.id,
                    expected_path: rename.path.clone(),
                    expected_record_version: rename.record_version,
                    new_file_name: "renamed.png".into(),
                },
                DuplicateResolutionChangeInput::Delete {
                    asset_id: delete.id,
                    expected_path: delete.path.clone(),
                    expected_record_version: delete.record_version,
                },
            ],
        })
        .unwrap();

    assert!(matches!(
        summary.status,
        DuplicateResolutionBatchStatus::Committed
    ));
    assert!(matches!(
        summary.results[0].status,
        DuplicateResolutionItemStatus::Renamed
    ));
    assert!(matches!(
        summary.results[1].status,
        DuplicateResolutionItemStatus::Deleted
    ));
    assert!(kept.is_file());
    assert!(!renamed.exists());
    assert!(core.media_dir().join("b/renamed.png").is_file());
    assert!(!deleted.exists());
    assert_eq!(backend.find_duplicate_assets().unwrap().duplicate_groups, 0);
}

// Scanning.

#[test]
fn scan_folder_adds_the_root_indexes_it_and_reports_progress() {
    let core = TempCore::start();
    core.write_image("one.png");
    core.write_image("nested/two.png");
    fs::write(core.media_dir().join("notes.txt"), b"not media").unwrap();
    let backend = core.backend();
    let (seen, subscription) = phases(backend);

    let summary = backend
        .scan_folder(TempCore::text(&core.media_dir()))
        .unwrap();
    drop(subscription);

    assert_eq!(
        (summary.indexed, summary.removed, summary.failed),
        (2, 0, 0)
    );
    let roots = backend.list_scan_roots().unwrap();
    assert_eq!(roots.len(), 1);
    assert_eq!(Path::new(&roots[0].path), core.media_dir());
    assert_eq!(
        names(&everything(backend, "gallery").items),
        ["one.png", "two.png"]
    );
    let seen = seen.lock().unwrap();
    // `cleanup` ends the scan of a root; `scanning` is only published when the
    // scan had to wait for an indexing worker.
    assert_eq!(seen.last().map(String::as_str), Some("cleanup"), "{seen:?}");
}

#[test]
fn scan_folder_refuses_a_missing_folder_with_the_core_text() {
    let core = TempCore::start();
    let backend = core.backend();
    let missing = TempCore::text(&core.media_dir().join("missing"));

    let error = backend.scan_folder(missing.clone()).unwrap_err();

    assert_eq!(
        error.text(),
        "Folder path does not exist or is not a directory"
    );
    assert_eq!(backend.add_scan_root(missing).unwrap_err(), error);
    assert!(backend.list_scan_roots().unwrap().is_empty());
}

#[test]
fn add_scan_root_and_list_scan_roots_keep_roots_without_scanning() {
    let core = TempCore::start();
    core.write_image("one.png");
    let backend = core.backend();

    backend
        .add_scan_root(format!("  {}/", TempCore::text(&core.media_dir())))
        .unwrap();

    let roots = backend.list_scan_roots().unwrap();
    assert_eq!(roots.len(), 1);
    assert_eq!(Path::new(&roots[0].path), core.media_dir());
    assert!(!roots[0].auto_scan_on_startup);
    assert_eq!(everything(backend, "gallery").total, 0);
}

#[test]
fn set_scan_root_auto_scan_changes_the_setting_of_one_root() {
    let core = TempCore::start();
    let backend = core.backend();
    let root = TempCore::text(&core.media_dir());
    backend.add_scan_root(root.clone()).unwrap();

    backend.set_scan_root_auto_scan(root.clone(), true).unwrap();
    assert!(backend.list_scan_roots().unwrap()[0].auto_scan_on_startup);
    backend.set_scan_root_auto_scan(root, false).unwrap();
    assert!(!backend.list_scan_roots().unwrap()[0].auto_scan_on_startup);
}

#[test]
fn rescan_all_roots_picks_up_new_files_and_drops_removed_ones() {
    let core = library(2);
    let backend = core.backend();
    core.write_image("new.png");
    fs::remove_file(core.media_dir().join("image-00.png")).unwrap();
    let (seen, subscription) = phases(backend);

    let summary = backend.rescan_all_roots().unwrap();
    drop(subscription);

    assert_eq!(summary.removed, 1);
    assert_eq!(
        names(&everything(backend, "gallery").items),
        ["image-01.png", "new.png"]
    );
    assert!(!seen.lock().unwrap().is_empty());
}

#[test]
fn remove_scan_root_removes_the_root_and_its_assets_but_no_source_file() {
    let core = library(2);
    let backend = core.backend();

    let summary = backend
        .remove_scan_root(TempCore::text(&core.media_dir()))
        .unwrap();

    assert_eq!(summary.removed_assets, 2);
    assert!(backend.list_scan_roots().unwrap().is_empty());
    assert_eq!(everything(backend, "gallery").total, 0);
    assert!(core.media_dir().join("image-00.png").is_file());
    assert!(core.media_dir().join("image-01.png").is_file());
}

#[test]
fn scan_startup_roots_without_an_enabled_root_answers_none() {
    let core = library(1);
    let backend = core.backend();

    assert!(backend.scan_startup_roots().unwrap().is_none());
    assert!(backend.scan_startup_roots().unwrap().is_none());
}

#[test]
fn scan_startup_roots_scans_the_launch_snapshot_once_and_replays_the_result() {
    let core = TempCore::start();
    core.write_image("one.png");
    let root = TempCore::text(&core.media_dir());
    core.backend().add_scan_root(root.clone()).unwrap();
    core.backend()
        .set_scan_root_auto_scan(root.clone(), true)
        .unwrap();
    // The setting belongs to the next launch.
    assert!(core.backend().scan_startup_roots().unwrap().is_none());

    let core = core.relaunch();
    let backend = core.backend();
    // Turning the setting off now does not change this launch's snapshot.
    backend.set_scan_root_auto_scan(root, false).unwrap();

    let (seen, subscription) = phases(backend);
    let results: Vec<_> = std::thread::scope(|scope| {
        let callers: Vec<_> = (0..4)
            .map(|_| scope.spawn(|| backend.scan_startup_roots()))
            .collect();
        callers
            .into_iter()
            .map(|caller| caller.join().unwrap().unwrap().expect("a summary"))
            .collect()
    });
    assert!(results.iter().all(|summary| summary.indexed == 1));
    assert_eq!(everything(backend, "gallery").total, 1);

    // A file added afterwards is not picked up: the result is replayed.
    core.write_image("two.png");
    let replay = backend.scan_startup_roots().unwrap().unwrap();
    assert_eq!(replay.indexed, 1);
    assert_eq!(everything(backend, "after").total, 1);
    drop(subscription);

    // Four callers and a replay published the progress of exactly one scan.
    let startup = seen.lock().unwrap().clone();
    fs::remove_file(core.media_dir().join("two.png")).unwrap();
    let (one_scan, subscription) = phases(backend);
    backend.rescan_all_roots().unwrap();
    drop(subscription);
    let ends = |phases: &[String]| phases.iter().filter(|phase| *phase == "cleanup").count();
    // A scan publishes `cleanup` once per root; `scanning` only when it had to
    // wait for an indexing worker.
    assert_eq!(ends(&one_scan.lock().unwrap()), 1);
    assert_eq!(
        ends(&startup),
        ends(&one_scan.lock().unwrap()),
        "{startup:?}"
    );
}

// Thumbnails.

#[test]
fn get_video_tool_status_reports_missing_tools() {
    let core = TempCore::start();

    let status = core.backend().get_video_tool_status().unwrap();

    // The temporary core has no ffmpeg; ffprobe is looked up on the machine.
    assert!(!status.ffmpeg_available);
}

#[test]
fn render_all_thumbnails_renders_every_image_and_reports_progress() {
    let core = library(3);
    let backend = core.backend();
    let (seen, subscription) = phases(backend);

    let summary = backend.render_all_thumbnails().unwrap();
    drop(subscription);

    assert_eq!((summary.generated, summary.failed), (3, 0));
    assert_eq!(summary.total, 3);
    assert!(!summary.cancelled);
    assert_eq!(
        seen.lock().unwrap().first().map(String::as_str),
        Some("thumbs-start")
    );
    assert_eq!(
        seen.lock().unwrap().last().map(String::as_str),
        Some("thumbs-done")
    );
    let details = backend
        .get_asset_details(everything(backend, "gallery").items[0].id)
        .unwrap()
        .unwrap();
    let thumb = details.thumb_path.expect("the thumbnail is recorded");
    assert!(Path::new(&thumb).starts_with(core.thumbs_dir()));
    assert!(is_jpeg(&fs::read(thumb).unwrap()));
}

#[test]
fn render_failed_thumbnails_retries_only_failed_ones() {
    let core = TempCore::start();
    core.write_image("good.png");
    fs::write(core.media_dir().join("broken.png"), b"not a png").unwrap();
    let backend = core.backend();
    backend
        .scan_folder(TempCore::text(&core.media_dir()))
        .unwrap();
    let first = backend.render_all_thumbnails().unwrap();
    assert_eq!((first.generated, first.failed), (1, 1));
    let (seen, subscription) = phases(backend);

    let retry = backend.render_failed_thumbnails().unwrap();
    drop(subscription);

    assert_eq!(retry.total, 1);
    assert_eq!((retry.generated, retry.failed), (0, 1));
    assert!(seen
        .lock()
        .unwrap()
        .iter()
        .any(|phase| phase == "thumbs-failed-done"));
}

#[test]
fn cancel_render_all_thumbnails_answers_false_when_no_render_runs() {
    let core = library(1);

    assert_eq!(core.backend().cancel_render_all_thumbnails(), Ok(false));
}

#[test]
fn clear_all_thumbnails_removes_the_files_and_reports_progress() {
    let core = library(2);
    let backend = core.backend();
    backend.render_all_thumbnails().unwrap();
    let (seen, subscription) = phases(backend);

    let removed = backend.clear_all_thumbnails().unwrap();
    drop(subscription);

    assert_eq!(removed, 2);
    assert!(seen
        .lock()
        .unwrap()
        .iter()
        .all(|phase| phase == "thumbs-clear"));
    assert!(!seen.lock().unwrap().is_empty());
    let asset = &everything(backend, "gallery").items[0];
    assert_eq!(
        backend
            .get_asset_details(asset.id)
            .unwrap()
            .unwrap()
            .thumb_path,
        None
    );
    // The source files of the temporary root are untouched.
    assert!(core.media_dir().join("image-00.png").is_file());
}

#[test]
fn request_thumbnail_renders_on_demand_and_then_serves_the_file() {
    let core = library(1);
    let backend = core.backend();
    let asset = everything(backend, "gallery").items.remove(0);

    let ThumbnailReply::Ready(rendered) = thumbnail(backend, visible(&asset)) else {
        panic!("the first request renders the thumbnail");
    };
    assert!(is_jpeg(&rendered.bytes));
    assert_eq!(rendered.version, asset.thumb_version);

    let ThumbnailReply::Ready(served) = thumbnail(backend, visible(&asset)) else {
        panic!("the second request serves the file");
    };
    assert_eq!(served, rendered);

    // A cached-only request and a prefetch are served from the same file.
    for (cached_only, prefetch) in [(true, false), (false, true)] {
        let request = ThumbnailRequest {
            cached_only,
            prefetch,
            ..visible(&asset)
        };
        assert_eq!(
            thumbnail(backend, request),
            ThumbnailReply::Ready(rendered.clone())
        );
    }
}

#[test]
fn request_thumbnail_serves_the_current_picture_for_a_stale_version() {
    let core = library(1);
    let backend = core.backend();
    let asset = everything(backend, "gallery").items.remove(0);
    let stale = ThumbnailRequest {
        version: "0000000000000000".into(),
        ..visible(&asset)
    };

    let ThumbnailReply::Ready(image) = thumbnail(backend, stale) else {
        panic!("a stale version still gets the current picture");
    };

    assert_eq!(image.version, asset.thumb_version);
}

#[test]
fn request_thumbnail_answers_not_found_without_rendering_when_cached_only() {
    let core = library(1);
    let backend = core.backend();
    let asset = everything(backend, "gallery").items.remove(0);
    let cached_only = ThumbnailRequest {
        cached_only: true,
        ..visible(&asset)
    };

    assert_eq!(thumbnail(backend, cached_only), ThumbnailReply::NotFound);
    assert_eq!(
        backend
            .get_asset_details(asset.id)
            .unwrap()
            .unwrap()
            .thumb_path,
        None,
        "a cached-only miss must not render"
    );
}

#[test]
fn request_thumbnail_answers_not_found_for_an_unknown_asset_and_a_broken_source() {
    let core = TempCore::start();
    fs::write(core.media_dir().join("broken.png"), b"not a png").unwrap();
    let backend = core.backend();
    backend
        .scan_folder(TempCore::text(&core.media_dir()))
        .unwrap();
    let broken = everything(backend, "gallery").items.remove(0);

    assert_eq!(
        thumbnail(backend, visible(&broken)),
        ThumbnailReply::NotFound
    );
    let unknown = ThumbnailRequest {
        asset_id: 9_999_999,
        ..visible(&broken)
    };
    assert_eq!(thumbnail(backend, unknown), ThumbnailReply::NotFound);
}

#[test]
fn request_thumbnail_refuses_a_malformed_request() {
    let core = library(1);
    let backend = core.backend();
    let asset = everything(backend, "gallery").items.remove(0);

    for request in [
        ThumbnailRequest {
            asset_id: 0,
            ..visible(&asset)
        },
        ThumbnailRequest {
            version: "../thumbs/x.jpg".into(),
            ..visible(&asset)
        },
    ] {
        assert_eq!(thumbnail(backend, request), ThumbnailReply::BadRequest);
    }
}

#[test]
fn many_thumbnail_requests_at_once_are_all_answered() {
    let core = library(24);
    let backend = core.backend();
    let assets = everything(backend, "gallery").items;
    let (sender, receiver) = mpsc::channel();

    for asset in &assets {
        let sender = sender.clone();
        let asset_id = asset.id;
        backend.request_thumbnail(
            visible(asset),
            ThumbnailResponder::new(move |reply| {
                let _ = sender.send((asset_id, reply));
            }),
        );
    }

    let mut answered = Vec::new();
    for _ in &assets {
        let (asset_id, reply) = receiver
            .recv_timeout(WAIT)
            .expect("every request is answered");
        assert!(
            matches!(&reply, ThumbnailReply::Ready(image) if is_jpeg(&image.bytes)),
            "asset {asset_id}: {reply:?}"
        );
        answered.push(asset_id);
    }
    answered.sort_unstable();
    let mut expected = ids(&assets);
    expected.sort_unstable();
    assert_eq!(answered, expected);
}

// The not-ready window.

#[test]
fn a_backend_without_its_core_answers_unavailable_and_not_ready() {
    let backend = CoreBackend::starting();
    assert!(!backend.is_ready());

    let request = ThumbnailRequest {
        asset_id: 1,
        version: "0123456789abcdef".into(),
        cached_only: false,
        prefetch: false,
        retry: 0,
    };
    assert_eq!(thumbnail(&backend, request), ThumbnailReply::Unavailable);

    let error = backend.list_scan_roots().unwrap_err();
    assert_eq!(error.kind(), BackendErrorKind::NotReady);
    assert_eq!(error.i18n_key(), Some("common.loading"));
    assert_eq!(
        backend.scan_startup_roots().unwrap_err().kind(),
        BackendErrorKind::NotReady
    );
    assert_eq!(
        backend
            .start_asset_query(AssetQueryRequest::new("owner", 1))
            .unwrap_err()
            .kind(),
        BackendErrorKind::NotReady
    );
    assert_eq!(
        backend.cancel_render_all_thumbnails().unwrap_err().kind(),
        BackendErrorKind::NotReady
    );
}

#[test]
fn a_tile_requested_during_startup_gets_its_picture_once_the_core_is_attached() {
    let core = library(1);
    let asset = everything(core.backend(), "gallery").items.remove(0);
    // A second backend over the same profile that has not been attached yet
    // stands for the application before its startup has finished.
    let starting = CoreBackend::starting();
    let executor = Arc::new(ThreadExecutor::new());
    let mut driver = Driver::new(
        ThumbnailPipeline::new(),
        ThumbnailFetcher::new(Arc::new(starting.clone())),
        Arc::clone(&executor) as Arc<dyn Executor>,
    );
    let key = ThumbnailKey::new(asset.id, asset.thumb_version.clone());

    driver.handle(ThumbnailInput::Request {
        key: key.clone(),
        mode: ThumbnailMode::Visible,
    });
    // The first answer is Unavailable; the model schedules its quick retry.
    assert!(driver.pump_one(WAIT));
    assert!(driver.model_mut().take_events().is_empty());

    core.attach_to(&starting).expect("attach the started core");
    assert!(starting.is_ready());
    assert!(matches!(
        core.attach_to(&starting),
        Err(AttachError::AlreadyAttached)
    ));

    let mut events = Vec::new();
    while events.is_empty() {
        assert!(driver.pump_one(WAIT), "the retry is answered");
        events = driver.model_mut().take_events();
    }
    assert!(matches!(
        &events[..],
        [ThumbnailEvent::Ready { key: ready, image, current: true }]
            if *ready == key && is_jpeg(&image.bytes)
    ));
}

// Library transfer and maintenance.

#[test]
fn export_tags_csv_and_import_tags_csv_carry_tags_between_libraries() {
    let core = library(2);
    let backend = core.backend();
    let session = everything(backend, "gallery");
    backend
        .set_asset_tags(session.items[0].id, vec!["cat".into(), "trip".into()])
        .unwrap();
    let csv = out_file(&core, "tags.csv");

    let exported = backend.export_tags_csv(csv.clone()).unwrap();
    assert!(exported.rows >= 1);
    assert!(Path::new(&csv).is_file());

    backend
        .set_asset_tags(session.items[0].id, Vec::new())
        .unwrap();
    assert!(tags_of(backend, session.items[0].id).is_empty());
    let imported = backend.import_tags_csv(csv).unwrap();

    assert!(imported.assets_updated >= 1);
    let mut restored = tags_of(backend, session.items[0].id);
    restored.sort_unstable();
    assert_eq!(restored, ["cat", "trip"]);
    assert!(backend
        .import_tags_csv(out_file(&core, "missing.csv"))
        .is_err());
}

#[test]
fn clear_library_data_empties_the_library_and_keeps_the_source_files() {
    let core = library(2);
    let backend = core.backend();
    backend.render_all_thumbnails().unwrap();
    let (seen, subscription) = phases(backend);

    let summary = backend.clear_library_data().unwrap();
    drop(subscription);

    assert_eq!(
        (
            summary.removed_assets,
            summary.removed_roots,
            summary.removed_thumbnails
        ),
        (2, 1, 2)
    );
    assert!(backend.list_scan_roots().unwrap().is_empty());
    assert_eq!(everything(backend, "gallery").total, 0);
    assert!(core.media_dir().join("image-00.png").is_file());
    let seen = seen.lock().unwrap();
    assert_eq!(seen.first().map(String::as_str), Some("library-clear"));
    assert_eq!(seen.last().map(String::as_str), Some("library-clear-done"));
}

#[test]
fn a_database_bundle_is_exported_inspected_and_restored() {
    let core = library(2);
    let backend = core.backend();
    let asset = everything(backend, "gallery").items[0].id;
    backend.set_asset_tags(asset, vec!["kept".into()]).unwrap();
    backend.render_all_thumbnails().unwrap();
    let bundle = out_file(&core, "backup.zip");

    let exported = backend.export_db_bundle(bundle.clone()).unwrap();
    assert!(exported.copied_files >= 1);
    assert_eq!(exported.copied_thumbnails, 2);

    let inspection = backend.inspect_db_bundle(bundle.clone()).unwrap();
    assert_eq!(inspection.roots.len(), 1);
    assert_eq!(Path::new(&inspection.roots[0]), core.media_dir());
    assert!(!inspection.requires_mapping);
    assert!(inspection.format_version.is_some());

    // Change the library, then restore the bundle over it.
    backend
        .set_asset_tags(asset, vec!["changed".into()])
        .unwrap();
    let restored = backend
        .import_db_bundle(bundle, Vec::<DbRootMapping>::new())
        .unwrap();

    assert!(restored.restored_files >= 1);
    assert_eq!(restored.restored_thumbnails, 2);
    let session = everything(backend, "restored");
    assert_eq!(session.total, 2);
    let tagged: Vec<_> = session
        .items
        .iter()
        .map(|item| tags_of(backend, item.id))
        .filter(|tags| !tags.is_empty())
        .collect();
    assert_eq!(tagged, [["kept"]]);
    assert!(backend
        .inspect_db_bundle(out_file(&core, "missing.zip"))
        .is_err());
}

// Progress.

#[test]
fn subscribe_progress_delivers_only_while_the_subscription_lives() {
    let core = TempCore::start();
    core.write_image("one.png");
    let backend = core.backend();
    let root = TempCore::text(&core.media_dir());

    // Subscribed before the call: the operation's first event is seen.
    let (seen, subscription) = phases(backend);
    backend.scan_folder(root).unwrap();
    let during = seen.lock().unwrap().clone();
    drop(subscription);
    backend.rescan_all_roots().unwrap();

    assert_eq!(during.last().map(String::as_str), Some("cleanup"));
    assert_eq!(*seen.lock().unwrap(), during);
}

// The executor boundary.

#[test]
fn no_workflow_lock_outlives_the_call_that_took_it() {
    let core = library(3);
    let backend = core.backend();
    let session = everything(backend, "gallery");

    // Each of these takes the scan lock, the thumbnail lock or the maintenance
    // gate inside the call. A delete fails at once with `libraryBusy` while the
    // scan lock is held, and a bundle export waits for every other operation, so
    // both succeeding right after proves that nothing was kept.
    backend.rescan_all_roots().unwrap();
    backend.render_all_thumbnails().unwrap();
    backend.clear_all_thumbnails().unwrap();
    assert!(matches!(
        thumbnail(backend, visible(&session.items[1])),
        ThumbnailReply::Ready(_)
    ));
    backend
        .export_db_bundle(out_file(&core, "after.zip"))
        .unwrap();
    backend.delete_asset(session.items[0].id).unwrap();
    backend.rescan_all_roots().unwrap();
    backend.clear_library_data().unwrap();
}

#[test]
fn calls_from_many_executor_threads_all_finish() {
    let core = library(6);
    let backend = core.backend().clone();
    let session = everything(&backend, "gallery");
    let executor = ThreadExecutor::new();
    let (sender, receiver) = mpsc::channel();
    let mut expected = 0;

    for round in 0..3 {
        for asset in &session.items {
            let (backend, sender, asset) = (backend.clone(), sender.clone(), asset.clone());
            expected += 1;
            executor.spawn(Box::new(move || {
                let outcome = match round {
                    0 => backend
                        .set_asset_tags(asset.id, vec![format!("tag-{}", asset.id)])
                        .map(|_| ()),
                    1 => backend.set_asset_favorite(asset.id, true),
                    _ => backend.get_asset_details(asset.id).map(|_| ()),
                };
                let _ = sender.send(outcome);
            }));
        }
        let (backend, sender) = (backend.clone(), sender.clone());
        expected += 1;
        executor.spawn(Box::new(move || {
            let outcome = match round {
                0 => backend.rescan_all_roots().map(|_| ()),
                1 => backend.render_all_thumbnails().map(|_| ()),
                _ => backend.find_duplicate_assets().map(|_| ()),
            };
            let _ = sender.send(outcome);
        }));
    }

    for _ in 0..expected {
        receiver
            .recv_timeout(WAIT)
            .expect("no call waits for a lock another task kept")
            .expect("the call succeeds");
    }
    assert!(executor.wait_until_quiet(WAIT));
}

/// `TempCore::seed_synthetic_library` gives the library the rows of the grid
/// baseline's seed: the running core sees them with its next query (the seed moves
/// the library revision), with the kinds, favourites and tags the measurements
/// filter by.
#[test]
fn the_synthetic_library_is_queried_like_an_indexed_one() {
    let core = TempCore::start();
    let backend = core.backend();
    assert_eq!(query(backend, AssetQueryRequest::new("seed", 1)).total, 0);

    core.seed_synthetic_library(2_000);
    let all = query(backend, AssetQueryRequest::new("seed", 2));
    assert_eq!(all.total, 2_000);
    assert_eq!(all.items.len(), 128);

    let mut generation = 2;
    let mut total = |change: &dyn Fn(&mut AssetQueryRequest)| {
        generation += 1;
        let mut request = AssetQueryRequest::new("seed", generation);
        change(&mut request);
        query(backend, request).total
    };
    // One video in 20, one favourite in 20, `common` on every fifth asset and
    // `rare` on every 200th.
    assert_eq!(total(&|request| request.kind = Some(MediaKind::Video)), 100);
    assert_eq!(total(&|request| request.favorites_only = true), 100);
    assert_eq!(
        total(&|request| request.tags_and = vec!["common".to_owned()]),
        400
    );
    assert_eq!(
        total(&|request| request.tags_and = vec!["rare".to_owned()]),
        10
    );
    assert_eq!(
        total(&|request| request.tags_not = vec!["common".to_owned()]),
        1_600
    );

    let roots = backend.list_scan_roots().expect("list the roots");
    assert_eq!(
        roots
            .iter()
            .map(|root| root.path.as_str())
            .collect::<Vec<_>>(),
        ["/synthetic/library"]
    );
    // The known tags: 2,000 of the vocabulary and the two the filters use.
    let tags = backend
        .list_tags(String::new(), 0, 100)
        .expect("list the tags");
    assert_eq!(tags.total, 2_002);
}
