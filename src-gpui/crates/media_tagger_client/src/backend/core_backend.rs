//! `CoreBackend`: the `Backend` port over the real core.
//!
//! This is the only file of the client crate that names the core's use cases,
//! services and state (`architecture_boundaries`,
//! `client_reaches_the_core_only_through_its_backend_adapter`). Every method is the
//! body of the Tauri command of the same name without the IPC: arguments → use
//! case → result, with the core's error text passed on unchanged. Validation, SQL
//! and the workflow locks (maintenance gate → `scan_lock` → `thumb_lock`) stay in
//! the use cases, which take and release them inside the one call; nothing here
//! takes a lock or holds one for a caller. The one exception is not part of a
//! normal build: `test_support::TempCore::hold_scan_lock` holds the scan lock for a
//! test, as a running scan would.
//!
//! What the Tauri adapter owns outside the core is rebuilt here without a
//! framework runtime: the `process-progress` event ([`ProgressHub`]), the
//! single-flight startup scan ([`StartupScan`]), the binary ID range (decoded to
//! typed IDs) and the `thumb://` handler ([`CoreBackend::request_thumbnail`]).

use std::{
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{mpsc, Arc, Mutex, OnceLock},
    thread,
};

use media_tagger_core::{
    app::state::AppState,
    error::AppResult,
    models::{
        AssetDetails, AssetQueryPageResult, AssetQueryPositionResult, AssetQuerySectionsResult,
        AssetSummary, BulkFavoriteSummary, BulkMediaGroupSummary, BulkTagMergeSummary,
        ClearLibrarySummary, CsvExportSummary, CsvImportSummary, DbBundleExportSummary,
        DbBundleImportSummary, DbBundleInspection, DbRootMapping, DeleteAssetSummary,
        DuplicateResolutionBatchInput, DuplicateResolutionBatchSummary, DuplicateScanSummary,
        RemoveRootSummary, ScanProgress, ScanRoot, ScanSummary, SelectionMutationResult,
        SetAssetTagsSummary, StartAssetQueryResult, TagListPage, ThumbnailRenderSummary,
        VideoToolStatus,
    },
    services::{
        asset_query_service, selection_service,
        thumb_service::{self, PendingThumbnail, ThumbnailRequestMode, ThumbnailRequestOutcome},
    },
    usecases::{assets, import_export, scan, thumbs},
};

use super::{
    id_range::decode_id_range, AssetIdRange, AssetQueryRequest, Backend, BackendError,
    BackendResult, MediaGroupUpdate, MetaFilter, SelectionMutation, SelectionSpec, ThumbnailImage,
    ThumbnailReply, ThumbnailRequest, ThumbnailResponder,
};
use crate::{
    progress::{ProgressHub, ProgressListener, ProgressSubscription},
    startup_scan::StartupScan,
};

const _: () = assert!(super::THUMB_VERSION_LEN == media_tagger_core::thumbs::THUMB_VERSION_LEN);

/// The core's tag rule (`media_tagger_core::utils::tags`) for
/// [`crate::library::tags`], so the client has no second implementation of it:
/// what is trimmed, how a tag is lowercased, which characters a tag cannot hold.
/// Pure functions; they touch no state and take no lock.
pub(crate) mod tag_rules {
    use media_tagger_core::utils::tags;

    /// Trimmed, lowercased and deduplicated in first-seen order; blank tags go.
    /// Tags with a character the rule forbids are kept.
    pub(crate) fn normalize(tags: Vec<String>) -> Vec<String> {
        tags::normalize_tags(tags)
    }

    /// [`normalize`], or `None` when a tag holds whitespace, a comma, a semicolon
    /// or a control character.
    pub(crate) fn normalize_valid(tags: Vec<String>) -> Option<Vec<String>> {
        tags::normalize_and_validate_tags(tags).ok()
    }
}

/// Most finished renders published in one transaction, as in the `thumb://`
/// publisher of the core.
const PUBLISH_BATCH: usize = 64;

/// The backend of one opened profile. Clones share the same state.
#[derive(Clone)]
pub struct CoreBackend {
    shared: Arc<Shared>,
}

struct Shared {
    /// Empty until startup has built `AppState`: the counterpart of the window in
    /// which the Tauri `thumb://` scheme answers 503.
    ready: OnceLock<Ready>,
    progress: ProgressHub,
}

struct Ready {
    state: Arc<AppState>,
    startup_scan: StartupScan,
    /// Finished on-demand renders on their way to the publisher thread.
    rendered: mpsc::Sender<Rendered>,
}

/// A finished on-demand render and the request waiting for it.
struct Rendered {
    pending: PendingThumbnail,
    responder: ThumbnailResponder,
}

/// The backend cannot be attached to this `AppState`.
#[derive(Debug)]
pub enum AttachError {
    /// The backend already has its state; the first one stays.
    AlreadyAttached,
    /// The thread that publishes finished on-demand renders did not start.
    Publisher(std::io::Error),
}

impl std::fmt::Display for AttachError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyAttached => f.write_str("the backend is already attached to its state"),
            Self::Publisher(error) => {
                write!(f, "failed to start the thumbnail publisher thread: {error}")
            }
        }
    }
}

impl std::error::Error for AttachError {}

impl CoreBackend {
    /// The backend of a started core. `startup_scan_roots` is the scan-root
    /// snapshot taken when the profile was opened
    /// (`bootstrap::StartupSnapshots::scan_roots`).
    pub fn new(
        state: Arc<AppState>,
        startup_scan_roots: Vec<ScanRoot>,
    ) -> Result<Self, AttachError> {
        let backend = Self::starting();
        backend.attach(state, startup_scan_roots)?;
        Ok(backend)
    }

    /// A backend whose core has not started yet. Until [`attach`](Self::attach),
    /// thumbnail requests are answered [`ThumbnailReply::Unavailable`] and every
    /// other call fails with a `NotReady` error; progress can already be
    /// subscribed to.
    pub fn starting() -> Self {
        Self {
            shared: Arc::new(Shared {
                ready: OnceLock::new(),
                progress: ProgressHub::new(),
            }),
        }
    }

    /// Hands the started core to the backend and ends the not-ready window. As
    /// in the Tauri adapter this is the last step of startup, after the workers
    /// have started.
    pub fn attach(
        &self,
        state: Arc<AppState>,
        startup_scan_roots: Vec<ScanRoot>,
    ) -> Result<(), AttachError> {
        if self.shared.ready.get().is_some() {
            return Err(AttachError::AlreadyAttached);
        }
        let (rendered, finished) = mpsc::channel();
        let publisher_state = Arc::clone(&state);
        // The publisher ends when every sender is gone: this backend and the
        // render callbacks still held by the scheduler.
        thread::Builder::new()
            .name("tagrove-thumb-publish".into())
            .spawn(move || publish_rendered(&publisher_state, &finished))
            .map_err(AttachError::Publisher)?;
        self.shared
            .ready
            .set(Ready {
                state,
                startup_scan: StartupScan::new(startup_scan_roots),
                rendered,
            })
            .map_err(|_| AttachError::AlreadyAttached)
    }

    pub fn is_ready(&self) -> bool {
        self.shared.ready.get().is_some()
    }

    fn ready(&self) -> BackendResult<&Ready> {
        self.shared.ready.get().ok_or_else(BackendError::not_ready)
    }

    fn state(&self) -> BackendResult<&AppState> {
        Ok(&self.ready()?.state)
    }

    /// The core's `ProgressSink` over the hub. Publishing cannot fail, so a
    /// listener never aborts the operation that reports to it.
    fn progress_sink(&self) -> impl Fn(ScanProgress) -> AppResult<()> + '_ {
        |progress| {
            self.shared.progress.publish(&progress);
            Ok(())
        }
    }
}

fn rejected<T>(result: Result<T, String>) -> BackendResult<T> {
    result.map_err(BackendError::rejected)
}

impl From<MetaFilter> for assets::AssetMetaFilterInput {
    fn from(filter: MetaFilter) -> Self {
        match filter {
            MetaFilter::HasNoTags { tag_count } => Self::HasNoTags { tag_count },
            MetaFilter::GroupName { group_name } => Self::GroupName { group_name },
        }
    }
}

impl From<SelectionSpec> for asset_query_service::SelectionSpec {
    fn from(spec: SelectionSpec) -> Self {
        Self {
            session_id: spec.session_id,
            ranges: spec.ranges,
            excluded: spec.excluded,
            included: spec.included,
        }
    }
}

impl From<SelectionMutation> for selection_service::SelectionMutation {
    fn from(mutation: SelectionMutation) -> Self {
        match mutation {
            SelectionMutation::MergeTags { tags } => Self::MergeTags { tags },
            SelectionMutation::ToggleFavorite => Self::ToggleFavorite,
            SelectionMutation::SetMediaGroup { media_group_key } => {
                Self::SetMediaGroup { media_group_key }
            }
        }
    }
}

impl From<MediaGroupUpdate> for assets::BulkMediaGroupUpdateInput {
    fn from(update: MediaGroupUpdate) -> Self {
        Self {
            asset_id: update.asset_id,
            media_group_order: update.media_group_order,
        }
    }
}

impl Backend for CoreBackend {
    fn start_asset_query(
        &self,
        request: AssetQueryRequest,
    ) -> BackendResult<StartAssetQueryResult> {
        let database = &self.state()?.database;
        // Calls of one owner may run in any order on the executor; the
        // generation check of `begin` answers the older one `Superseded`.
        let (filters, request_id) = rejected(assets::begin_asset_query_service(
            database,
            request.tags_and,
            request.tags_not,
            request.kind.map(|kind| kind.as_str().to_owned()),
            request.favorites_only,
            request.meta_filter.map(Into::into),
            &request.owner_id,
            request.generation,
        ))?;
        Ok(assets::start_asset_query_service(
            database,
            filters,
            request.page_size,
            request_id,
            request.generation,
            request.anchor_offset,
        )?)
    }

    fn get_asset_query_page(
        &self,
        session_id: u64,
        offset: usize,
        limit: usize,
    ) -> BackendResult<AssetQueryPageResult> {
        let database = &self.state()?.database;
        Ok(assets::get_asset_query_page_service(
            database, session_id, offset, limit,
        )?)
    }

    fn get_asset_query_position(
        &self,
        session_id: u64,
        asset_id: i64,
    ) -> BackendResult<AssetQueryPositionResult> {
        let database = &self.state()?.database;
        Ok(assets::get_asset_query_position_service(
            database, session_id, asset_id,
        )?)
    }

    fn get_asset_query_id_range(
        &self,
        session_id: u64,
        start: usize,
        end: usize,
    ) -> BackendResult<AssetIdRange> {
        let database = &self.state()?.database;
        let bytes = assets::get_asset_query_id_range_service(database, session_id, start, end)?;
        decode_id_range(&bytes).map_err(|error| BackendError::rejected(error.to_string()))
    }

    fn get_asset_query_sections(&self, session_id: u64) -> BackendResult<AssetQuerySectionsResult> {
        let database = &self.state()?.database;
        Ok(assets::get_asset_query_sections_service(
            database, session_id,
        )?)
    }

    fn get_asset_summaries_by_ids(&self, asset_ids: Vec<i64>) -> BackendResult<Vec<AssetSummary>> {
        let database = &self.state()?.database;
        rejected(assets::validate_summary_batch(&asset_ids))?;
        Ok(assets::get_asset_summaries_by_ids_service(
            database, &asset_ids,
        )?)
    }

    fn get_asset_details(&self, asset_id: i64) -> BackendResult<Option<AssetDetails>> {
        let database = &self.state()?.database;
        Ok(assets::get_asset_details_service(database, asset_id)?)
    }

    fn list_tags(&self, query: String, offset: i64, limit: i64) -> BackendResult<TagListPage> {
        rejected(assets::list_tags_service(
            query,
            offset,
            limit,
            self.state()?,
        ))
    }

    fn set_asset_tags(
        &self,
        asset_id: i64,
        tags: Vec<String>,
    ) -> BackendResult<SetAssetTagsSummary> {
        rejected(assets::set_asset_tags_service(
            asset_id,
            tags,
            self.state()?,
        ))
    }

    fn merge_asset_tags_bulk(
        &self,
        asset_ids: Vec<i64>,
        tags: Vec<String>,
    ) -> BackendResult<BulkTagMergeSummary> {
        rejected(assets::merge_asset_tags_bulk_service(
            asset_ids,
            tags,
            self.state()?,
        ))
    }

    fn set_asset_favorite(&self, asset_id: i64, is_favorite: bool) -> BackendResult<()> {
        rejected(assets::set_asset_favorite_service(
            asset_id,
            is_favorite,
            self.state()?,
        ))
    }

    fn toggle_assets_favorite_bulk(
        &self,
        asset_ids: Vec<i64>,
    ) -> BackendResult<BulkFavoriteSummary> {
        let database = &self.state()?.database;
        Ok(assets::toggle_assets_favorite_bulk_service(
            database, &asset_ids,
        )?)
    }

    fn set_asset_media_group(
        &self,
        asset_id: i64,
        media_group_key: Option<String>,
        media_group_order: Option<f64>,
    ) -> BackendResult<()> {
        rejected(assets::set_asset_media_group_service(
            asset_id,
            media_group_key,
            media_group_order,
            self.state()?,
        ))
    }

    fn set_assets_media_group_bulk(
        &self,
        updates: Vec<MediaGroupUpdate>,
        media_group_key: Option<String>,
    ) -> BackendResult<BulkMediaGroupSummary> {
        rejected(assets::set_assets_media_group_bulk_service(
            updates.into_iter().map(Into::into).collect(),
            media_group_key,
            self.state()?,
        ))
    }

    fn apply_selection_mutation(
        &self,
        selection: SelectionSpec,
        mutation: SelectionMutation,
    ) -> BackendResult<SelectionMutationResult> {
        let database = &self.state()?.database;
        Ok(selection_service::apply_selection_mutation(
            database,
            &selection.into(),
            mutation.into(),
        )?)
    }

    fn delete_asset(&self, asset_id: i64) -> BackendResult<DeleteAssetSummary> {
        rejected(assets::delete_asset_service(asset_id, self.state()?))
    }

    fn find_duplicate_assets(&self) -> BackendResult<DuplicateScanSummary> {
        rejected(assets::find_duplicate_assets_service(
            self.state()?,
            &self.progress_sink(),
        ))
    }

    fn apply_duplicate_resolution_batch(
        &self,
        input: DuplicateResolutionBatchInput,
    ) -> BackendResult<DuplicateResolutionBatchSummary> {
        rejected(assets::apply_duplicate_resolution_batch_service(
            input,
            self.state()?,
        ))
    }

    fn scan_folder(&self, path: String) -> BackendResult<ScanSummary> {
        rejected(scan::scan_folder_service(
            path,
            self.state()?,
            &self.progress_sink(),
        ))
    }

    fn list_scan_roots(&self) -> BackendResult<Vec<ScanRoot>> {
        rejected(scan::list_scan_roots_service(self.state()?))
    }

    fn add_scan_root(&self, path: String) -> BackendResult<()> {
        rejected(scan::add_scan_root_service(path, self.state()?))
    }

    fn remove_scan_root(&self, path: String) -> BackendResult<RemoveRootSummary> {
        rejected(scan::remove_scan_root_service(path, self.state()?))
    }

    fn rescan_all_roots(&self) -> BackendResult<ScanSummary> {
        rejected(scan::rescan_all_roots_service(
            self.state()?,
            &self.progress_sink(),
        ))
    }

    fn set_scan_root_auto_scan(&self, path: String, enabled: bool) -> BackendResult<()> {
        rejected(scan::set_scan_root_auto_scan_service(
            path,
            enabled,
            self.state()?,
        ))
    }

    fn scan_startup_roots(&self) -> BackendResult<Option<ScanSummary>> {
        let ready = self.ready()?;
        ready.startup_scan.run(|roots| {
            rejected(scan::scan_startup_roots_service(
                &roots,
                &ready.state,
                &self.progress_sink(),
            ))
        })
    }

    fn get_video_tool_status(&self) -> BackendResult<VideoToolStatus> {
        rejected(thumbs::get_video_tool_status_service(self.state()?))
    }

    fn render_all_thumbnails(&self) -> BackendResult<ThumbnailRenderSummary> {
        rejected(thumbs::render_all_thumbnails_service(
            self.state()?,
            &self.progress_sink(),
        ))
    }

    fn render_failed_thumbnails(&self) -> BackendResult<ThumbnailRenderSummary> {
        rejected(thumbs::render_failed_thumbnails_service(
            self.state()?,
            &self.progress_sink(),
        ))
    }

    fn cancel_render_all_thumbnails(&self) -> BackendResult<bool> {
        Ok(thumb_service::cancel_render_all_thumbnails(self.state()?)?)
    }

    fn clear_all_thumbnails(&self) -> BackendResult<usize> {
        rejected(thumbs::clear_all_thumbnails_service(
            self.state()?,
            &self.progress_sink(),
        ))
    }

    /// The typed counterpart of `thumb_protocol::serve`: the same two phases of
    /// `thumb_service` and the same answers, without HTTP in between. The call
    /// serves an existing file, or enqueues a render whose result reaches the
    /// publisher thread; it never waits for a render. Every request is answered:
    /// the scheduler cannot take a queued job back, so a caller that lost
    /// interest drops the answer instead.
    fn request_thumbnail(&self, request: ThumbnailRequest, responder: ThumbnailResponder) {
        if !request.is_well_formed() {
            responder.respond(ThumbnailReply::BadRequest);
            return;
        }
        // Not attached yet: asked again later, like a 503 during Tauri's setup.
        let Some(ready) = self.shared.ready.get() else {
            responder.respond(ThumbnailReply::Unavailable);
            return;
        };
        let state: &AppState = &ready.state;
        let Ok(permit) = state.database.admit() else {
            responder.respond(ThumbnailReply::Unavailable);
            return;
        };

        // The responder moves into the render callback only when a render is
        // enqueued; the shared slot lets the immediate path still answer.
        let slot = Arc::new(Mutex::new(Some(responder)));
        let callback_slot = Arc::clone(&slot);
        let rendered = ready.rendered.clone();
        let outcome = thumb_service::request_thumbnail(
            request.asset_id,
            ThumbnailRequestMode {
                cached_only: request.cached_only,
                prefetch: request.prefetch,
            },
            state,
            &permit,
            move |pending| {
                if let Some(responder) = take(&callback_slot) {
                    // A publisher that is gone drops the message, and the
                    // dropped responder answers `Unavailable`.
                    let _ = rendered.send(Rendered { pending, responder });
                }
            },
        );

        let reply = match outcome {
            Ok(ThumbnailRequestOutcome::Ready(served)) => ThumbnailReply::Ready(ThumbnailImage {
                bytes: served.bytes,
                version: served.version,
            }),
            Ok(ThumbnailRequestOutcome::NotFound) => ThumbnailReply::NotFound,
            Ok(ThumbnailRequestOutcome::Pending) => return,
            // Full scheduler queue, busy database pool or another request
            // error: transient.
            Err(_) => ThumbnailReply::Unavailable,
        };
        if let Some(responder) = take(&slot) {
            responder.respond(reply);
        }
    }

    fn export_tags_csv(&self, path: String) -> BackendResult<CsvExportSummary> {
        rejected(import_export::export_tags_csv_service(path, self.state()?))
    }

    fn import_tags_csv(&self, path: String) -> BackendResult<CsvImportSummary> {
        rejected(import_export::import_tags_csv_service(path, self.state()?))
    }

    fn clear_library_data(&self) -> BackendResult<ClearLibrarySummary> {
        rejected(import_export::clear_library_data_service(
            self.state()?,
            &self.progress_sink(),
        ))
    }

    fn export_db_bundle(&self, path: String) -> BackendResult<DbBundleExportSummary> {
        rejected(import_export::export_db_bundle_service(path, self.state()?))
    }

    fn import_db_bundle(
        &self,
        path: String,
        root_mappings: Vec<DbRootMapping>,
    ) -> BackendResult<DbBundleImportSummary> {
        rejected(import_export::import_db_bundle_service(
            path,
            root_mappings,
            self.state()?,
        ))
    }

    fn inspect_db_bundle(&self, path: String) -> BackendResult<DbBundleInspection> {
        rejected(import_export::inspect_db_bundle_service(
            path,
            self.state()?,
        ))
    }

    fn subscribe_progress(&self, listener: ProgressListener) -> ProgressSubscription {
        self.shared.progress.subscribe(listener)
    }
}

fn take(slot: &Mutex<Option<ThumbnailResponder>>) -> Option<ThumbnailResponder> {
    slot.lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take()
}

/// The publisher thread: publishes whatever finished renders are already waiting
/// in one transaction and answers their requests, off the scheduler's workers.
/// Grouping adds no delay, because the first result is published at once.
fn publish_rendered(state: &AppState, finished: &mpsc::Receiver<Rendered>) {
    while let Ok(first) = finished.recv() {
        let mut batch = vec![first];
        while batch.len() < PUBLISH_BATCH {
            match finished.try_recv() {
                Ok(next) => batch.push(next),
                Err(_) => break,
            }
        }
        // A panic while publishing drops the batch's responders, which answer
        // `Unavailable`; the thread goes on with the next batch.
        let _ = catch_unwind(AssertUnwindSafe(|| publish_batch(state, batch)));
    }
}

fn publish_batch(state: &AppState, batch: Vec<Rendered>) {
    let (pending, responders): (Vec<_>, Vec<_>) = batch
        .into_iter()
        .map(|rendered| (rendered.pending, rendered.responder))
        .unzip();
    let served = state
        .database
        .admit()
        .and_then(|permit| thumb_service::finish_thumbnail_requests(pending, state, &permit));
    match served {
        Ok(served) => {
            for (responder, served) in responders.into_iter().zip(served) {
                responder.respond(match served {
                    Some(served) => ThumbnailReply::Ready(ThumbnailImage {
                        bytes: served.bytes,
                        version: served.version,
                    }),
                    // Stale (record changed or thumbnails cleared) or failed:
                    // a later request renders again.
                    None => ThumbnailReply::NotFound,
                });
            }
        }
        Err(_) => {
            for responder in responders {
                responder.respond(ThumbnailReply::Unavailable);
            }
        }
    }
}

/// The real core on a temporary profile, for tests of this crate and of the GPUI
/// adapter.
#[cfg(any(test, feature = "test-support"))]
pub mod test_support {
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::{mpsc, Arc},
        thread,
    };

    use media_tagger_core::app::{bootstrap, locks, state::AppState};

    use super::{AttachError, CoreBackend};
    use crate::backend::Backend;

    /// An 8×6 RGB PNG: the smallest fixture the indexer measures and the
    /// thumbnail renderer decodes.
    pub const PNG_FIXTURE: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x06, 0x08, 0x02, 0x00, 0x00, 0x00, 0x71,
        0x67, 0x48, 0xac, 0x00, 0x00, 0x00, 0x63, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x0d, 0xc9,
        0x21, 0x01, 0x00, 0x41, 0x08, 0x45, 0x41, 0x92, 0xa0, 0x09, 0xb1, 0x21, 0xd0, 0x9b, 0xe4,
        0xcb, 0x97, 0x02, 0x4d, 0x88, 0x0d, 0x81, 0x26, 0xd1, 0xdd, 0xd8, 0x31, 0x33, 0xdc, 0x38,
        0xc6, 0x35, 0x64, 0x94, 0xf1, 0x8c, 0x35, 0xcc, 0x02, 0x0f, 0x4e, 0x70, 0x03, 0x05, 0x15,
        0xbc, 0x60, 0xe3, 0x8f, 0xc4, 0x93, 0x93, 0xdc, 0x44, 0x49, 0x25, 0x2f, 0xd9, 0xfc, 0x43,
        0xb8, 0x38, 0xe2, 0x0a, 0x89, 0x12, 0x4f, 0xac, 0xfe, 0x68, 0xbc, 0x39, 0xcd, 0x6d, 0xd4,
        0x54, 0xf3, 0x9a, 0xed, 0x3f, 0x06, 0x1f, 0xce, 0x70, 0x07, 0x0d, 0x35, 0xbc, 0x61, 0x87,
        0x0f, 0xcc, 0x7f, 0x3e, 0x71, 0x8f, 0xb9, 0xa0, 0x71, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
        0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    /// A started core in a fresh temporary directory:
    ///
    /// ```text
    /// <tmp>/data/    the profile: media.db, thumbs/
    /// <tmp>/media/   an empty directory for synthetic media
    /// ```
    ///
    /// The profile is opened the way the application opens it
    /// (`bootstrap::open_storage`, `bootstrap::start_workers`), with an ffmpeg path
    /// that does not exist: ffmpeg reports as unavailable on every machine, while
    /// the core still looks for ffprobe on the machine. The directory is removed
    /// when the value is dropped; destructive tests therefore never see anything
    /// but their own files.
    pub struct TempCore {
        backend: CoreBackend,
        state: Arc<AppState>,
        root: tempfile::TempDir,
    }

    impl Default for TempCore {
        fn default() -> Self {
            Self::start()
        }
    }

    impl TempCore {
        /// # Panics
        ///
        /// When the temporary profile cannot be created or opened.
        pub fn start() -> Self {
            let root = tempfile::Builder::new()
                .prefix("media-tagger-client-test-")
                .tempdir()
                .expect("create a temporary directory");
            fs::create_dir_all(root.path().join("media")).expect("create the media directory");
            let (backend, state) = open(&root.path().join("data"));
            Self {
                backend,
                state,
                root,
            }
        }

        /// Opens the same profile again, as the next launch of the application
        /// does: a new `AppState` and a new startup scan snapshot.
        ///
        /// # Panics
        ///
        /// When the profile cannot be opened again.
        #[must_use]
        pub fn relaunch(self) -> Self {
            let Self {
                backend,
                state,
                root,
            } = self;
            drop((backend, state));
            let (backend, state) = open(&root.path().join("data"));
            Self {
                backend,
                state,
                root,
            }
        }

        /// Attaches this core to `starting`, a backend made with
        /// [`CoreBackend::starting`]: the end of the not-ready window of an
        /// application whose views exist before its core has started. The
        /// startup scan snapshot of that backend is empty.
        pub fn attach_to(&self, starting: &CoreBackend) -> Result<(), AttachError> {
            starting.attach(Arc::clone(&self.state), Vec::new())
        }

        pub fn backend(&self) -> &CoreBackend {
            &self.backend
        }

        /// The backend as models hold it.
        pub fn shared(&self) -> Arc<dyn Backend> {
            Arc::new(self.backend.clone())
        }

        /// The temporary directory that holds `data/` and `media/`.
        pub fn root(&self) -> &Path {
            self.root.path()
        }

        pub fn data_dir(&self) -> PathBuf {
            self.root.path().join("data")
        }

        pub fn thumbs_dir(&self) -> PathBuf {
            self.data_dir().join("thumbs")
        }

        /// The directory for synthetic media; scan it or a directory below it.
        pub fn media_dir(&self) -> PathBuf {
            self.root.path().join("media")
        }

        /// Writes [`PNG_FIXTURE`] to `relative` below the media directory,
        /// creating parent directories, and returns the file's path.
        ///
        /// # Panics
        ///
        /// When the file cannot be written.
        pub fn write_image(&self, relative: &str) -> PathBuf {
            let path = self.media_dir().join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("create the image's directory");
            }
            fs::write(&path, PNG_FIXTURE).expect("write the image");
            path
        }

        /// `path` as the text the port takes.
        pub fn text(path: &Path) -> String {
            path.to_string_lossy().into_owned()
        }

        /// Holds the core's scan lock as a running scan does, until the returned
        /// value is dropped: on a thread of its own, through the helper a scan
        /// takes it with. Returns once the lock is held. Calls that must not wait
        /// for a scan (a delete, ADR 0022) are refused meanwhile; a call that
        /// waits for the scan lock waits until the drop, so a test makes none on
        /// the thread that keeps the value.
        ///
        /// [`CoreBackend`] itself never holds a lock for a caller; this is the
        /// test's stand-in for the scan that would.
        ///
        /// # Panics
        ///
        /// When the lock cannot be taken.
        #[must_use = "the scan lock is released when the value is dropped"]
        pub fn hold_scan_lock(&self) -> HeldScanLock {
            let state = Arc::clone(&self.state);
            let (held, is_held) = mpsc::channel();
            let (release, released) = mpsc::channel::<()>();
            let holder = thread::spawn(move || {
                locks::with_scan_lock(&state, |_| {
                    let _ = held.send(());
                    // Ends when the sender is dropped.
                    let _ = released.recv();
                    Ok(())
                })
                .expect("take the scan lock");
            });
            is_held.recv().expect("the scan lock is held");
            HeldScanLock {
                release: Some(release),
                holder: Some(holder),
            }
        }
    }

    /// The scan lock of a [`TempCore`], held until this value is dropped
    /// ([`TempCore::hold_scan_lock`]).
    pub struct HeldScanLock {
        release: Option<mpsc::Sender<()>>,
        holder: Option<thread::JoinHandle<()>>,
    }

    impl Drop for HeldScanLock {
        fn drop(&mut self) {
            drop(self.release.take());
            if let Some(holder) = self.holder.take() {
                // The lock is free again before the next call of the test.
                let _ = holder.join();
            }
        }
    }

    fn open(data_dir: &Path) -> (CoreBackend, Arc<AppState>) {
        fs::create_dir_all(data_dir).expect("create the data directory");
        let (storage, snapshots) = bootstrap::open_storage(data_dir).expect("open the profile");
        let state = bootstrap::start_workers(&storage, data_dir.join("no-ffmpeg"))
            .expect("start the thumbnail workers");
        // The startup connection lives until the workers have started.
        drop(storage);
        let state = Arc::new(state);
        let backend =
            CoreBackend::new(Arc::clone(&state), snapshots.scan_roots).expect("attach the core");
        (backend, state)
    }
}
