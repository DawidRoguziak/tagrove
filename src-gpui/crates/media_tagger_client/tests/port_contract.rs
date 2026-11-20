//! Behaviour tests of the \`Backend\` port and its model plumbing, ported from the
//! wrapper contract tests of the Tauri frontend
//! (\`src/platform/tauri/__tests__/api.test.ts\`) and from the acceptance criteria of
//! epic E02 (\`docs/plans/gpui-port/E02-client-backend-port.md\`).
//!
//! The Tauri tests prove payload shapes against a mocked \`invoke\`; here the same
//! behaviour is checked where it is decided: on the real core in a temporary
//! profile (\`TempCore\`), through the port only, or on the deterministic executor
//! with the \`FakeBackend\`. Destructive calls only ever see the temporary media
//! directory of their own \`TempCore\`.

use std::{
    fs,
    sync::{mpsc, Arc, Mutex},
    time::Duration,
};

use media_tagger_client::{
    backend::{
        core_backend::test_support::TempCore,
        id_range::{decode_id_range, AssetIdRange},
        AssetQueryRequest, Backend, BackendError, BackendResult, CoreBackend, FakeBackend,
        MediaGroupUpdate, MediaKind, SelectionMutation, SelectionSpec, ThumbnailReply,
    },
    exec::{Executor, ManualExecutor},
    model::{Dispatch, Driver, Model, Runner},
    progress::forward,
    thumbs::{ThumbnailFetcher, ThumbnailInput, ThumbnailKey, ThumbnailMode, ThumbnailPipeline},
};
use media_tagger_core::models::{
    AssetQuerySectionsResult, AssetSummary, ScanCompletion, ScanProgress, ScanSummary,
    SelectionMutationResult, StartAssetQueryResult, TagQueryImpact, ThumbnailRenderSummary,
};

const WAIT: Duration = Duration::from_secs(60);
const VERSION: &str = "0123456789abcdef";

/// A core with \`count\` indexed images \`image-00.png\`, \`image-01.png\`, ...
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

/// The first page and session ID of an unfiltered query by \`owner\`.
fn everything(backend: &CoreBackend, owner: &str) -> (u64, Vec<AssetSummary>) {
    match backend
        .start_asset_query(AssetQueryRequest::new(owner, 1))
        .expect("start the query")
    {
        StartAssetQueryResult::Ready {
            session_id, items, ..
        } => (session_id, items),
        StartAssetQueryResult::Superseded => panic!("the query was superseded"),
    }
}

fn ids(items: &[AssetSummary]) -> Vec<i64> {
    items.iter().map(|item| item.id).collect()
}

fn group_of(backend: &CoreBackend, asset_id: i64) -> Option<String> {
    backend
        .get_asset_details(asset_id)
        .expect("read the details")
        .expect("the asset exists")
        .summary
        .media_group_key
}

// Ported from api.test.ts.

/// "sends a selection spec and mutation with camelCase keys": the second call
/// clears the group (\`mediaGroupKey: null\`) and answers \`stale\`.
#[test]
fn a_selection_mutation_without_a_group_key_clears_the_group_and_a_stale_one_writes_nothing() {
    let core = library(3);
    let backend = core.backend();
    let (session_id, items) = everything(backend, "gallery");
    let all = ids(&items);
    let spec = |session_id| SelectionSpec {
        session_id,
        ranges: vec![[0, 1]],
        excluded: Vec::new(),
        included: vec![all[2]],
    };
    backend
        .apply_selection_mutation(
            spec(session_id),
            SelectionMutation::SetMediaGroup {
                media_group_key: Some("trip".into()),
            },
        )
        .expect("group the selection");
    assert!(all
        .iter()
        .all(|id| group_of(backend, *id).as_deref() == Some("trip")));

    // The write changed the library revision: the session it was resolved on is
    // stale now, and a selection on it is refused without a write.
    assert_eq!(
        backend.apply_selection_mutation(
            spec(session_id),
            SelectionMutation::SetMediaGroup {
                media_group_key: None
            },
        ),
        Ok(SelectionMutationResult::Stale)
    );
    assert!(
        all.iter()
            .all(|id| group_of(backend, *id).as_deref() == Some("trip")),
        "a stale selection writes nothing"
    );

    let (current, _) = everything(backend, "gallery-after");
    let cleared = backend
        .apply_selection_mutation(
            spec(current),
            SelectionMutation::SetMediaGroup {
                media_group_key: None,
            },
        )
        .expect("clear the group");
    let SelectionMutationResult::Ready {
        processed_assets,
        media_group_key,
        is_favorite,
        ..
    } = cleared
    else {
        panic!("the selection is current");
    };
    assert_eq!(processed_assets, 3);
    assert_eq!(media_group_key, None);
    assert_eq!(is_favorite, None);
    assert!(all.iter().all(|id| group_of(backend, *id).is_none()));
}

/// "maps tag mutation summaries with vocabulary_changed=false|true".
#[test]
fn tag_mutation_summaries_report_whether_the_vocabulary_changed() {
    let core = library(3);
    let backend = core.backend();
    let (_, items) = everything(backend, "gallery");
    let (first, second, third) = (items[0].id, items[1].id, items[2].id);

    // A new tag enters the vocabulary.
    let created = backend.set_asset_tags(first, vec!["cat".into()]).unwrap();
    assert!(created.changed);
    assert!(created.vocabulary_changed);
    assert_eq!(created.tags, ["cat"]);
    assert!(matches!(
        &created.query_impact,
        TagQueryImpact::Tags { changed_tags, tag_count_changed: true } if changed_tags == &["cat"]
    ));

    // A tag already in the vocabulary changes the asset only.
    let reused = backend.set_asset_tags(second, vec!["cat".into()]).unwrap();
    assert!(reused.changed);
    assert!(!reused.vocabulary_changed);
    assert!(reused.revision > created.revision);

    // The same for a bulk merge.
    let merged_new = backend
        .merge_asset_tags_bulk(vec![first, third], vec!["travel".into()])
        .unwrap();
    assert!(merged_new.vocabulary_changed);
    assert_eq!(merged_new.processed_asset_ids, [first, third]);
    assert_eq!(merged_new.updated_asset_ids, [first, third]);
    assert!(matches!(
        &merged_new.query_impact,
        TagQueryImpact::Tags { changed_tags, .. } if changed_tags == &["travel"]
    ));
    let merged_known = backend
        .merge_asset_tags_bulk(vec![second], vec!["travel".into()])
        .unwrap();
    assert!(!merged_known.vocabulary_changed);
    assert_eq!(merged_known.updated_asset_ids, [second]);
    assert!(merged_known.revision > merged_new.revision);
    let result = &merged_known.results[0];
    assert_eq!(result.asset_id, second);
    assert!(result.changed);
    let mut tags = result.tags.clone();
    tags.sort_unstable();
    assert_eq!(tags, ["cat", "travel"]);

    // Removing the last use of a tag changes the vocabulary again.
    let removed = backend.set_asset_tags(third, Vec::new()).unwrap();
    assert!(!removed.vocabulary_changed, "\"travel\" is still in use");
    backend.set_asset_tags(first, vec!["cat".into()]).unwrap();
    let last = backend.set_asset_tags(second, vec!["cat".into()]).unwrap();
    assert!(
        last.vocabulary_changed,
        "the last use of \"travel\" is gone"
    );
}

/// "sends the refresh anchor offset": an image-only favorites query with page
/// size 64 and an anchor offset returns the anchored page.
#[test]
fn an_image_query_of_favorites_returns_the_page_of_its_anchor_offset() {
    let core = library(5);
    let backend = core.backend();
    let (_, items) = everything(backend, "seed");
    for item in &items[1..] {
        backend.set_asset_favorite(item.id, true).unwrap();
    }
    let favorites: Vec<i64> = ids(&items[1..]);

    let mut request = AssetQueryRequest::new("owner-test", 3);
    request.kind = Some(MediaKind::Image);
    request.favorites_only = true;
    request.page_size = 64;
    request.anchor_offset = Some(3);
    let StartAssetQueryResult::Ready {
        total,
        offset,
        items: page,
        ..
    } = backend.start_asset_query(request.clone()).unwrap()
    else {
        panic!("the query is ready");
    };

    assert_eq!(total, 4);
    // The page that holds the anchor starts at a multiple of the page size.
    assert_eq!(offset, 0);
    assert_eq!(ids(&page), favorites);
    assert!(page
        .iter()
        .all(|item| item.kind == "image" && item.is_favorite));

    // An older generation of the same owner is superseded.
    request.generation = 2;
    assert_eq!(
        backend.start_asset_query(request),
        Ok(StartAssetQueryResult::Superseded)
    );
}

/// "reads timeline sections of a session": a ready session with no sections
/// stays usable.
#[test]
fn the_sections_of_an_empty_session_are_ready_and_empty_and_the_session_stays_usable() {
    let core = TempCore::start();
    let backend = core.backend();
    let (session_id, items) = everything(backend, "gallery");
    assert!(items.is_empty());

    assert_eq!(
        backend.get_asset_query_sections(session_id),
        Ok(AssetQuerySectionsResult::Ready {
            session_id,
            total: 0,
            sections: Vec::new(),
        })
    );
    assert_eq!(
        backend.get_asset_query_id_range(session_id, 0, 10),
        Ok(AssetIdRange::Ready(Vec::new()))
    );
}

fn id_body(values: &[f64]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

/// "decodes stale, empty, unaligned and byte-array ID ranges": the decoder takes
/// a byte slice at any offset of a larger buffer.
#[test]
fn an_id_range_decodes_from_an_unaligned_slice_of_a_larger_buffer() {
    let mut padded = vec![0xaa];
    padded.extend(id_body(&[2.0, 9.0, 11.0]));
    padded.push(0xbb);

    assert_eq!(
        decode_id_range(&padded[1..padded.len() - 1]),
        Ok(AssetIdRange::Ready(vec![9, 11]))
    );
    assert_eq!(
        decode_id_range(id_body(&[1.0, 5.0]).as_slice()),
        Ok(AssetIdRange::Ready(vec![5]))
    );
    // "round-trips the shared binary test body": [4, 5, 6] and the empty body.
    assert_eq!(
        decode_id_range(&id_body(&[3.0, 4.0, 5.0, 6.0])),
        Ok(AssetIdRange::Ready(vec![4, 5, 6]))
    );
    assert_eq!(decode_id_range(&[]), Ok(AssetIdRange::Stale));
}

fn pipeline_driver(
    fake: &Arc<FakeBackend>,
    executor: &Arc<ManualExecutor>,
) -> Driver<ThumbnailPipeline, ThumbnailFetcher> {
    Driver::new(
        ThumbnailPipeline::new(),
        ThumbnailFetcher::new(Arc::clone(fake) as Arc<dyn Backend>),
        Arc::clone(executor) as Arc<dyn Executor>,
    )
}

/// "builds thumb:// URLs from the asset ID and version token only": \`r=<n>\`
/// counts retries and \`p=1\` stays on the retries of a prefetch.
#[test]
fn the_retries_of_a_prefetch_keep_its_flag_and_count_up() {
    let fake = Arc::new(FakeBackend::new());
    let executor = Arc::new(ManualExecutor::new());
    let mut driver = pipeline_driver(&fake, &executor);
    let key = ThumbnailKey::new(42, VERSION);

    driver.handle(ThumbnailInput::Request {
        key: key.clone(),
        mode: ThumbnailMode::Prefetch,
    });
    for _ in 0..2 {
        executor.run_all();
        fake.answer_thumbnail(0, ThumbnailReply::NotFound);
        assert_eq!(driver.pump(), 1);
        let delay = executor.next_timer_in().expect("a retry is due");
        executor.advance_and_run(delay);
        assert_eq!(driver.pump(), 1);
    }
    executor.run_all();

    let sent: Vec<_> = fake
        .calls()
        .into_iter()
        .map(|call| match call {
            media_tagger_client::backend::fake::Call::RequestThumbnail { request } => (
                request.asset_id,
                request.version,
                request.cached_only,
                request.prefetch,
                request.retry,
            ),
            other => panic!("unexpected call {other:?}"),
        })
        .collect();
    assert_eq!(
        sent,
        [
            (42, VERSION.to_owned(), false, true, 0),
            (42, VERSION.to_owned(), false, true, 1),
            (42, VERSION.to_owned(), false, true, 2),
        ]
    );
}

/// "preserves shared startup results and failures across repeated
/// observations", on the real core: a launch whose enabled root has gone is
/// scanned once, partially, and every later caller sees the same result.
#[test]
fn a_startup_scan_of_a_missing_root_is_partial_and_replayed_without_a_rescan() {
    let core = TempCore::start();
    let gone = core.media_dir().join("gone");
    fs::create_dir_all(&gone).unwrap();
    let gone_text = TempCore::text(&gone);
    core.backend().add_scan_root(gone_text.clone()).unwrap();
    core.backend()
        .set_scan_root_auto_scan(gone_text, true)
        .unwrap();
    fs::remove_dir(&gone).unwrap();

    let core = core.relaunch();
    let backend = core.backend();
    let phases = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&phases);
    let subscription = backend.subscribe_progress(Arc::new(move |progress: &ScanProgress| {
        sink.lock().unwrap().push(progress.phase.clone());
    }));

    let first = backend.scan_startup_roots().unwrap().expect("a summary");
    let second = backend.scan_startup_roots().unwrap().expect("a summary");
    drop(subscription);

    for summary in [&first, &second] {
        assert!(
            matches!(
                summary,
                ScanSummary {
                    completion: ScanCompletion::Partial,
                    indexed: 0,
                    removed: 0,
                    failed: 1,
                }
            ),
            "{summary:?}"
        );
    }
    assert_eq!(
        *phases.lock().unwrap(),
        ["scan-skip"],
        "one scan, no replay progress"
    );
}

// Acceptance criteria.

/// The core's bulk-render cancel is reachable through the port while a render
/// runs; the progress listener runs inside the render, as a model's would.
#[test]
fn cancel_render_all_thumbnails_stops_a_running_render_and_answers_true() {
    let core = library(3);
    let backend = core.backend().clone();
    let answers = Arc::new(Mutex::new(Vec::new()));
    let (listener_backend, sink) = (backend.clone(), Arc::clone(&answers));
    let subscription = backend.subscribe_progress(Arc::new(move |progress: &ScanProgress| {
        if progress.phase == "thumbs-start" {
            sink.lock()
                .unwrap()
                .push(listener_backend.cancel_render_all_thumbnails());
        }
    }));

    let cancelled: ThumbnailRenderSummary = backend.render_all_thumbnails().unwrap();
    drop(subscription);

    assert_eq!(*answers.lock().unwrap(), [Ok(true)]);
    assert!(cancelled.cancelled);
    assert_eq!(cancelled.generated, 0);
    assert_eq!(backend.cancel_render_all_thumbnails(), Ok(false));

    // The cancel belonged to that run: the next one renders everything.
    let next = backend.render_all_thumbnails().unwrap();
    assert!(!next.cancelled);
    assert_eq!((next.generated, next.failed), (3, 0));
}

/// \`delete_asset\` fails at once with \`i18n:common.libraryBusy\` while a scan
/// holds the scan lock (lock order and fail-fast stay in the use case).
#[test]
fn delete_asset_fails_at_once_with_library_busy_while_a_scan_runs() {
    let core = library(2);
    let backend = core.backend().clone();
    let (_, items) = everything(&backend, "gallery");
    let victim = items[0].clone();
    let source = core.media_dir().join(&victim.file_name);
    let attempts = Arc::new(Mutex::new(Vec::new()));
    let (listener_backend, sink) = (backend.clone(), Arc::clone(&attempts));
    let subscription = backend.subscribe_progress(Arc::new(move |progress: &ScanProgress| {
        if progress.phase != "cleanup" || !sink.lock().unwrap().is_empty() {
            return;
        }
        // Another thread, as another model's executor task would be.
        let (sender, receiver) = mpsc::channel();
        let deleting = listener_backend.clone();
        std::thread::spawn(move || {
            let _ = sender.send(deleting.delete_asset(victim.id));
        });
        let outcome = receiver
            .recv_timeout(WAIT)
            .expect("the delete does not wait for the scan");
        sink.lock().unwrap().push(outcome);
    }));

    backend.rescan_all_roots().unwrap();
    drop(subscription);

    let attempts = attempts.lock().unwrap();
    let [Err(error)] = &attempts[..] else {
        panic!("expected one refused delete, got {attempts:?}");
    };
    assert_eq!(error.i18n_key(), Some("common.libraryBusy"));
    assert!(source.is_file(), "a refused delete touches no file");
    assert!(backend.get_asset_details(items[0].id).unwrap().is_some());
    // Once the scan is over the same delete goes through.
    backend.delete_asset(items[0].id).unwrap();
    assert!(!source.exists());
}

/// A minimal model that runs one backend call per command and records the
/// replies and the progress it is sent, in arrival order.
#[derive(Default)]
struct Recorder {
    seen: Vec<String>,
}

enum Input {
    Scan(String),
    Delete(i64),
    Progress(ScanProgress),
    Scanned(BackendResult<ScanSummary>),
    Deleted(Result<(), BackendError>),
}

enum Effect {
    Scan(String),
    Delete(i64),
}

impl Model for Recorder {
    type Input = Input;
    type Effect = Effect;

    fn handle(&mut self, input: Input) -> Vec<Effect> {
        match input {
            Input::Scan(path) => vec![Effect::Scan(path)],
            Input::Delete(id) => vec![Effect::Delete(id)],
            Input::Progress(progress) => {
                self.seen.push(format!("progress:{}", progress.phase));
                Vec::new()
            }
            Input::Scanned(result) => {
                self.seen.push(format!(
                    "scanned:{}",
                    result.map(|summary| summary.indexed).unwrap_or(usize::MAX)
                ));
                Vec::new()
            }
            Input::Deleted(result) => {
                self.seen.push(match result {
                    Ok(()) => "deleted".to_owned(),
                    Err(error) => format!("refused:{}", error.text()),
                });
                Vec::new()
            }
        }
    }
}

struct RecorderRunner(Arc<dyn Backend>);

impl Runner<Recorder> for RecorderRunner {
    fn run(&mut self, effect: Effect, dispatch: &Dispatch<Input>) {
        let backend = Arc::clone(&self.0);
        match effect {
            Effect::Scan(path) => dispatch.call(
                "scan_folder",
                move || backend.scan_folder(path),
                Input::Scanned,
            ),
            Effect::Delete(id) => dispatch.call(
                "delete_asset",
                move || backend.delete_asset(id).map(|_| ()),
                Input::Deleted,
            ),
        }
    }
}

/// Progress subscribed before the call reaches the model as inputs, ahead of the
/// call's own reply; a reply that waits in the inbox holds no core lock.
#[test]
fn progress_precedes_the_reply_and_a_queued_reply_holds_no_lock() {
    let core = TempCore::start();
    core.write_image("one.png");
    core.write_image("two.png");
    let backend = core.shared();
    let executor = Arc::new(ManualExecutor::new());
    let mut driver = Driver::new(
        Recorder::default(),
        RecorderRunner(Arc::clone(&backend)),
        Arc::clone(&executor) as Arc<dyn Executor>,
    );
    let subscription = forward(&*backend, driver.sender(), Input::Progress);

    driver.handle(Input::Scan(TempCore::text(&core.media_dir())));
    assert_eq!(executor.run_all(), 1);
    // The scan's reply now waits in the inbox, unapplied: its use case has
    // already released the scan lock, so a delete on another task succeeds.
    let (_, items) = everything(core.backend(), "gallery");
    assert_eq!(items.len(), 2);
    driver.handle(Input::Delete(items[0].id));
    assert_eq!(executor.run_all(), 1);
    driver.pump();
    drop(subscription);

    let seen = &driver.model().seen;
    let reply = seen
        .iter()
        .position(|entry| entry == "scanned:2")
        .unwrap_or_else(|| panic!("the scan is answered: {seen:?}"));
    assert!(seen[..reply]
        .iter()
        .all(|entry| entry.starts_with("progress:")));
    assert!(
        seen[..reply].contains(&"progress:cleanup".to_owned()),
        "{seen:?}"
    );
    assert_eq!(seen.last().map(String::as_str), Some("deleted"), "{seen:?}");
}

/// Bulk media-group updates keep the order given per asset
/// ("uses the selection summary and processed group ID contracts").
#[test]
fn a_bulk_group_update_reports_only_the_assets_that_exist() {
    let core = library(2);
    let backend = core.backend();
    let (_, items) = everything(backend, "gallery");

    let summary = backend
        .set_assets_media_group_bulk(
            vec![
                MediaGroupUpdate {
                    asset_id: items[0].id,
                    media_group_order: Some(1.0),
                },
                MediaGroupUpdate {
                    asset_id: 9_999_999,
                    media_group_order: Some(2.0),
                },
            ],
            Some("trip".into()),
        )
        .unwrap();

    assert_eq!(summary.processed_asset_ids, [items[0].id]);
    assert_eq!(summary.processed_assets, 1);
    assert_eq!(summary.media_group_key.as_deref(), Some("trip"));
    assert_eq!(group_of(backend, items[1].id), None);
}
