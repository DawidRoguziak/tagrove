//! `FakeBackend`: a scriptable `Backend` for model tests.
//!
//! Every call is recorded ([`FakeBackend::calls`]) and answered by the handler
//! scripted for its method (`on_<method>`); a method without a handler answers
//! with an error that names it. A handler sees the arguments, so a test can answer
//! by request (by generation, by asset ID) instead of by call order.
//!
//! The order in which answers reach a model is scripted with the executor: on a
//! `ManualExecutor` each backend call is a waiting task, and `run_nth` decides
//! which call answers first. Thumbnail requests answer through a responder
//! instead; without a handler the fake holds them, and the test answers them in
//! any order with [`FakeBackend::answer_thumbnail`]. Progress events are emitted by
//! the test ([`FakeBackend::emit_progress`]).

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, MutexGuard},
};

use media_tagger_core::models::{
    AssetDetails, AssetQueryPageResult, AssetQueryPositionResult, AssetQuerySectionsResult,
    AssetSummary, BulkFavoriteSummary, BulkMediaGroupSummary, BulkTagMergeSummary,
    ClearLibrarySummary, CsvExportSummary, CsvImportSummary, DbBundleExportSummary,
    DbBundleImportSummary, DbBundleInspection, DbRootMapping, DeleteAssetSummary,
    DuplicateResolutionBatchInput, DuplicateResolutionBatchSummary, DuplicateScanSummary,
    RemoveRootSummary, ScanProgress, ScanRoot, ScanSummary, SelectionMutationResult,
    SetAssetTagsSummary, StartAssetQueryResult, TagListPage, ThumbnailRenderSummary,
    VideoToolStatus,
};

use super::{
    AssetIdRange, AssetQueryRequest, Backend, BackendError, BackendResult, MediaGroupUpdate,
    SelectionMutation, SelectionSpec, ThumbnailReply, ThumbnailRequest, ThumbnailResponder,
};
use crate::progress::{ProgressHub, ProgressListener, ProgressSubscription};

type ThumbnailHandler = Box<dyn FnMut(ThumbnailRequest, ThumbnailResponder) + Send>;

fn lock<T: ?Sized>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Answers of a handler taken from a list, in call order; a call beyond the list
/// is answered with an error. It is the handler of a method without arguments
/// (`fake.on_list_scan_roots(sequence([..]))`); a handler with arguments calls it.
pub fn sequence<T: Send + 'static>(
    answers: impl IntoIterator<Item = BackendResult<T>>,
) -> impl FnMut() -> BackendResult<T> + Send + 'static {
    let mut answers: VecDeque<_> = answers.into_iter().collect();
    move || {
        answers.pop_front().unwrap_or_else(|| {
            Err(BackendError::rejected(
                "FakeBackend: the scripted answers are used up",
            ))
        })
    }
}

macro_rules! fake_backend {
    ($(
        $(#[$doc:meta])*
        $method:ident / $on:ident / $variant:ident ( $( $arg:ident : $ty:ty ),* ) -> $ret:ty;
    )*) => {
        /// One recorded call with its arguments.
        #[derive(Debug, Clone, PartialEq)]
        pub enum Call {
            $( $variant { $( $arg: $ty ),* }, )*
            RequestThumbnail { request: ThumbnailRequest },
        }

        impl Call {
            /// The name of the port method that was called.
            pub fn method(&self) -> &'static str {
                match self {
                    $( Self::$variant { .. } => stringify!($method), )*
                    Self::RequestThumbnail { .. } => "request_thumbnail",
                }
            }
        }

        #[derive(Default)]
        struct Handlers {
            $( $method: Option<Arc<Mutex<dyn FnMut($($ty),*) -> BackendResult<$ret> + Send>>>, )*
        }

        impl FakeBackend {
            $(
                $(#[$doc])*
                pub fn $on(
                    &self,
                    handler: impl FnMut($($ty),*) -> BackendResult<$ret> + Send + 'static,
                ) -> &Self {
                    lock(&self.handlers).$method = Some(Arc::new(Mutex::new(handler)));
                    self
                }
            )*
        }

        impl Backend for FakeBackend {
            $(
                fn $method(&self, $( $arg: $ty ),*) -> BackendResult<$ret> {
                    self.record(Call::$variant { $( $arg: Clone::clone(&$arg) ),* });
                    // The handler runs outside the fake's own locks, so it may
                    // block (a test gate) or call the fake again.
                    let handler = lock(&self.handlers).$method.clone();
                    match handler {
                        Some(handler) => {
                            let mut handler = lock(&handler);
                            (*handler)($($arg),*)
                        }
                        None => Err(unscripted(stringify!($method))),
                    }
                }
            )*

            fn request_thumbnail(&self, request: ThumbnailRequest, responder: ThumbnailResponder) {
                self.record(Call::RequestThumbnail { request: request.clone() });
                let handler = lock(&self.thumbnail_handler).clone();
                match handler {
                    Some(handler) => {
                        let mut handler = lock(&handler);
                        (*handler)(request, responder);
                    }
                    None => lock(&self.held_thumbnails).push((request, responder)),
                }
            }

            fn subscribe_progress(&self, listener: ProgressListener) -> ProgressSubscription {
                self.progress.subscribe(listener)
            }
        }
    };
}

fn unscripted(method: &str) -> BackendError {
    BackendError::rejected(format!("FakeBackend: no answer is scripted for {method}"))
}

#[derive(Default)]
pub struct FakeBackend {
    calls: Mutex<Vec<Call>>,
    handlers: Mutex<Handlers>,
    thumbnail_handler: Mutex<Option<Arc<Mutex<ThumbnailHandler>>>>,
    held_thumbnails: Mutex<Vec<(ThumbnailRequest, ThumbnailResponder)>>,
    progress: ProgressHub,
}

fake_backend! {
    start_asset_query / on_start_asset_query / StartAssetQuery(request: AssetQueryRequest) -> StartAssetQueryResult;
    get_asset_query_page / on_get_asset_query_page / GetAssetQueryPage(session_id: u64, offset: usize, limit: usize) -> AssetQueryPageResult;
    get_asset_query_position / on_get_asset_query_position / GetAssetQueryPosition(session_id: u64, asset_id: i64) -> AssetQueryPositionResult;
    get_asset_query_id_range / on_get_asset_query_id_range / GetAssetQueryIdRange(session_id: u64, start: usize, end: usize) -> AssetIdRange;
    get_asset_query_sections / on_get_asset_query_sections / GetAssetQuerySections(session_id: u64) -> AssetQuerySectionsResult;
    get_asset_summaries_by_ids / on_get_asset_summaries_by_ids / GetAssetSummariesByIds(asset_ids: Vec<i64>) -> Vec<AssetSummary>;
    get_asset_details / on_get_asset_details / GetAssetDetails(asset_id: i64) -> Option<AssetDetails>;
    list_tags / on_list_tags / ListTags(query: String, offset: i64, limit: i64) -> TagListPage;
    set_asset_tags / on_set_asset_tags / SetAssetTags(asset_id: i64, tags: Vec<String>) -> SetAssetTagsSummary;
    merge_asset_tags_bulk / on_merge_asset_tags_bulk / MergeAssetTagsBulk(asset_ids: Vec<i64>, tags: Vec<String>) -> BulkTagMergeSummary;
    set_asset_favorite / on_set_asset_favorite / SetAssetFavorite(asset_id: i64, is_favorite: bool) -> ();
    toggle_assets_favorite_bulk / on_toggle_assets_favorite_bulk / ToggleAssetsFavoriteBulk(asset_ids: Vec<i64>) -> BulkFavoriteSummary;
    set_asset_media_group / on_set_asset_media_group / SetAssetMediaGroup(asset_id: i64, media_group_key: Option<String>, media_group_order: Option<f64>) -> ();
    set_assets_media_group_bulk / on_set_assets_media_group_bulk / SetAssetsMediaGroupBulk(updates: Vec<MediaGroupUpdate>, media_group_key: Option<String>) -> BulkMediaGroupSummary;
    apply_selection_mutation / on_apply_selection_mutation / ApplySelectionMutation(selection: SelectionSpec, mutation: SelectionMutation) -> SelectionMutationResult;
    delete_asset / on_delete_asset / DeleteAsset(asset_id: i64) -> DeleteAssetSummary;
    find_duplicate_assets / on_find_duplicate_assets / FindDuplicateAssets() -> DuplicateScanSummary;
    apply_duplicate_resolution_batch / on_apply_duplicate_resolution_batch / ApplyDuplicateResolutionBatch(input: DuplicateResolutionBatchInput) -> DuplicateResolutionBatchSummary;
    scan_folder / on_scan_folder / ScanFolder(path: String) -> ScanSummary;
    list_scan_roots / on_list_scan_roots / ListScanRoots() -> Vec<ScanRoot>;
    add_scan_root / on_add_scan_root / AddScanRoot(path: String) -> ();
    remove_scan_root / on_remove_scan_root / RemoveScanRoot(path: String) -> RemoveRootSummary;
    rescan_all_roots / on_rescan_all_roots / RescanAllRoots() -> ScanSummary;
    set_scan_root_auto_scan / on_set_scan_root_auto_scan / SetScanRootAutoScan(path: String, enabled: bool) -> ();
    scan_startup_roots / on_scan_startup_roots / ScanStartupRoots() -> Option<ScanSummary>;
    get_video_tool_status / on_get_video_tool_status / GetVideoToolStatus() -> VideoToolStatus;
    render_all_thumbnails / on_render_all_thumbnails / RenderAllThumbnails() -> ThumbnailRenderSummary;
    render_failed_thumbnails / on_render_failed_thumbnails / RenderFailedThumbnails() -> ThumbnailRenderSummary;
    cancel_render_all_thumbnails / on_cancel_render_all_thumbnails / CancelRenderAllThumbnails() -> bool;
    clear_all_thumbnails / on_clear_all_thumbnails / ClearAllThumbnails() -> usize;
    export_tags_csv / on_export_tags_csv / ExportTagsCsv(path: String) -> CsvExportSummary;
    import_tags_csv / on_import_tags_csv / ImportTagsCsv(path: String) -> CsvImportSummary;
    clear_library_data / on_clear_library_data / ClearLibraryData() -> ClearLibrarySummary;
    export_db_bundle / on_export_db_bundle / ExportDbBundle(path: String) -> DbBundleExportSummary;
    import_db_bundle / on_import_db_bundle / ImportDbBundle(path: String, root_mappings: Vec<DbRootMapping>) -> DbBundleImportSummary;
    inspect_db_bundle / on_inspect_db_bundle / InspectDbBundle(path: String) -> DbBundleInspection;
}

impl FakeBackend {
    pub fn new() -> Self {
        Self::default()
    }

    fn record(&self, call: Call) {
        lock(&self.calls).push(call);
    }

    /// Every call so far, in the order the fake received them.
    pub fn calls(&self) -> Vec<Call> {
        lock(&self.calls).clone()
    }

    /// The names of the methods called so far, in order.
    pub fn called_methods(&self) -> Vec<&'static str> {
        lock(&self.calls).iter().map(Call::method).collect()
    }

    /// Returns the recorded calls and forgets them.
    pub fn take_calls(&self) -> Vec<Call> {
        std::mem::take(&mut *lock(&self.calls))
    }

    /// Answers thumbnail requests with `handler` instead of holding them. The
    /// handler may answer at once, keep the responder, or drop it (which answers
    /// `Unavailable`).
    pub fn on_request_thumbnail(
        &self,
        handler: impl FnMut(ThumbnailRequest, ThumbnailResponder) + Send + 'static,
    ) -> &Self {
        *lock(&self.thumbnail_handler) = Some(Arc::new(Mutex::new(Box::new(handler))));
        self
    }

    /// The thumbnail requests that are held without an answer, oldest first.
    pub fn held_thumbnails(&self) -> Vec<ThumbnailRequest> {
        lock(&self.held_thumbnails)
            .iter()
            .map(|(request, _)| request.clone())
            .collect()
    }

    /// Answers the held thumbnail request at `index` (0 is the oldest) and
    /// returns it, or `None` when there is none.
    pub fn answer_thumbnail(
        &self,
        index: usize,
        reply: ThumbnailReply,
    ) -> Option<ThumbnailRequest> {
        let (request, responder) = {
            let mut held = lock(&self.held_thumbnails);
            (index < held.len()).then(|| held.remove(index))?
        };
        responder.respond(reply);
        Some(request)
    }

    /// Publishes a progress event to the fake's subscribers, on this thread.
    pub fn emit_progress(&self, progress: ScanProgress) {
        self.progress.publish(&progress);
    }

    /// How many progress listeners are subscribed right now.
    pub fn progress_listeners(&self) -> usize {
        self.progress.listener_count()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use media_tagger_core::models::{AssetQueryPositionResult, ScanProgress};

    use super::{sequence, Call, FakeBackend};
    use crate::backend::{
        Backend, BackendError, ThumbnailImage, ThumbnailReply, ThumbnailRequest, ThumbnailResponder,
    };

    fn thumbnail(asset_id: i64) -> ThumbnailRequest {
        ThumbnailRequest {
            asset_id,
            version: "0123456789abcdef".into(),
            cached_only: false,
            prefetch: false,
            retry: 0,
        }
    }

    fn collecting() -> (
        Arc<Mutex<Vec<ThumbnailReply>>>,
        impl Fn() -> ThumbnailResponder,
    ) {
        let replies = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&replies);
        (replies, move || {
            let sink = Arc::clone(&sink);
            ThumbnailResponder::new(move |reply| sink.lock().unwrap().push(reply))
        })
    }

    #[test]
    fn an_unscripted_call_fails_with_the_method_name_and_is_still_recorded() {
        let fake = FakeBackend::new();

        let error = fake.get_asset_details(7).unwrap_err();

        assert_eq!(
            error.text(),
            "FakeBackend: no answer is scripted for get_asset_details"
        );
        assert_eq!(fake.calls(), [Call::GetAssetDetails { asset_id: 7 }]);
    }

    #[test]
    fn a_handler_answers_from_the_arguments_and_calls_are_recorded_in_order() {
        let fake = FakeBackend::new();
        fake.on_get_asset_query_position(|session_id, asset_id| {
            Ok(if session_id == 4 {
                AssetQueryPositionResult::Resolved {
                    index: asset_id as usize * 2,
                }
            } else {
                AssetQueryPositionResult::Stale
            })
        })
        .on_set_asset_favorite(|_, _| Err(BackendError::rejected("i18n:common.libraryBusy")));

        assert_eq!(
            fake.get_asset_query_position(4, 21),
            Ok(AssetQueryPositionResult::Resolved { index: 42 })
        );
        assert_eq!(
            fake.get_asset_query_position(5, 21),
            Ok(AssetQueryPositionResult::Stale)
        );
        assert_eq!(
            fake.set_asset_favorite(9, true).unwrap_err().i18n_key(),
            Some("common.libraryBusy")
        );

        assert_eq!(
            fake.called_methods(),
            [
                "get_asset_query_position",
                "get_asset_query_position",
                "set_asset_favorite"
            ]
        );
        assert_eq!(
            fake.take_calls()[2],
            Call::SetAssetFavorite {
                asset_id: 9,
                is_favorite: true
            }
        );
        assert!(fake.calls().is_empty());
    }

    #[test]
    fn a_sequence_answers_in_call_order_and_then_fails() {
        let fake = FakeBackend::new();
        fake.on_clear_all_thumbnails(sequence([
            Ok(3_usize),
            Err(BackendError::rejected("second")),
        ]));

        assert_eq!(fake.clear_all_thumbnails(), Ok(3));
        assert_eq!(fake.clear_all_thumbnails().unwrap_err().text(), "second");
        assert_eq!(
            fake.clear_all_thumbnails().unwrap_err().text(),
            "FakeBackend: the scripted answers are used up"
        );
    }

    #[test]
    fn a_handler_may_call_the_fake_again() {
        let fake = Arc::new(FakeBackend::new());
        let inner = Arc::clone(&fake);
        fake.on_cancel_render_all_thumbnails(|| Ok(true));
        fake.on_clear_all_thumbnails(move || {
            Ok(usize::from(inner.cancel_render_all_thumbnails()?))
        });

        assert_eq!(fake.clear_all_thumbnails(), Ok(1));
    }

    #[test]
    fn held_thumbnail_requests_are_answered_in_the_order_the_test_chooses() {
        let fake = FakeBackend::new();
        let (replies, responder) = collecting();
        fake.request_thumbnail(thumbnail(1), responder());
        fake.request_thumbnail(thumbnail(2), responder());
        assert_eq!(
            fake.held_thumbnails()
                .iter()
                .map(|request| request.asset_id)
                .collect::<Vec<_>>(),
            [1, 2]
        );

        let image = ThumbnailImage {
            bytes: vec![1, 2, 3],
            version: "0123456789abcdef".into(),
        };
        let second = fake.answer_thumbnail(1, ThumbnailReply::Ready(image.clone()));
        let first = fake.answer_thumbnail(0, ThumbnailReply::NotFound);

        assert_eq!(second.map(|request| request.asset_id), Some(2));
        assert_eq!(first.map(|request| request.asset_id), Some(1));
        assert_eq!(
            *replies.lock().unwrap(),
            [ThumbnailReply::Ready(image), ThumbnailReply::NotFound]
        );
        assert!(fake.answer_thumbnail(0, ThumbnailReply::NotFound).is_none());
        assert_eq!(fake.called_methods(), ["request_thumbnail"; 2]);
    }

    #[test]
    fn a_thumbnail_handler_replaces_holding() {
        let fake = FakeBackend::new();
        let (replies, responder) = collecting();
        fake.on_request_thumbnail(|request, responder| {
            if request.cached_only {
                responder.respond(ThumbnailReply::NotFound);
            }
            // Otherwise the responder is dropped: Unavailable.
        });

        fake.request_thumbnail(
            ThumbnailRequest {
                cached_only: true,
                ..thumbnail(1)
            },
            responder(),
        );
        fake.request_thumbnail(thumbnail(2), responder());

        assert_eq!(
            *replies.lock().unwrap(),
            [ThumbnailReply::NotFound, ThumbnailReply::Unavailable]
        );
        assert!(fake.held_thumbnails().is_empty());
    }

    #[test]
    fn progress_reaches_subscribers_only_while_they_are_subscribed() {
        let fake = FakeBackend::new();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        let event = |phase: &str| ScanProgress {
            phase: phase.to_owned(),
            processed: 1,
            total: 2,
            message: String::new(),
        };

        fake.emit_progress(event("counting"));
        let subscription = fake.subscribe_progress(Arc::new(move |progress: &ScanProgress| {
            sink.lock().unwrap().push(progress.phase.clone());
        }));
        assert_eq!(fake.progress_listeners(), 1);
        fake.emit_progress(event("scanning"));
        drop(subscription);
        fake.emit_progress(event("cleanup"));

        assert_eq!(*seen.lock().unwrap(), ["scanning"]);
    }
}
