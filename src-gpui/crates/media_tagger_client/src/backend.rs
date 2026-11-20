//! The `Backend` port: the one way presentation logic reaches the core (plan
//! decision D3).
//!
//! The trait has one method per use case the interface needs, with the names of
//! the Tauri commands they replace (`docs/architecture/ipc-contract.md`) and the
//! result types of `media_tagger_core::models`. Native video and window commands
//! are not part of it.
//!
//! Every method except [`Backend::subscribe_progress`] does blocking work (SQLite,
//! files, waiting for a workflow lock). A model never calls them on its own thread:
//! it returns an effect, and the effect's runner calls the port on an
//! [`Executor`](crate::exec::Executor) and sends the result back as an input.
//! Workflow locks are taken and released inside a single call, by the use case.
//!
//! Implementations: [`CoreBackend`] over the real core (the only file of this crate
//! that names the core's use cases, services and state) and `FakeBackend` for
//! tests (feature `test-support`).

use std::fmt::{Debug, Formatter};

use media_tagger_core::models::{
    AssetDetails, AssetQueryPageResult, AssetQueryPositionResult, AssetQuerySectionsResult,
    AssetSummary, BulkFavoriteSummary, BulkMediaGroupSummary, BulkTagMergeSummary,
    ClearLibrarySummary, CsvExportSummary, CsvImportSummary, DbBundleExportSummary,
    DbBundleImportSummary, DbBundleInspection, DbRootMapping, DeleteAssetSummary,
    DuplicateResolutionBatchInput, DuplicateResolutionBatchSummary, DuplicateScanSummary,
    RemoveRootSummary, ScanRoot, ScanSummary, SelectionMutationResult, SetAssetTagsSummary,
    StartAssetQueryResult, TagListPage, ThumbnailRenderSummary, VideoToolStatus,
};

use crate::progress::{ProgressListener, ProgressSubscription};

pub mod core_backend;
#[cfg(test)]
mod core_tests;
pub mod error;
#[cfg(any(test, feature = "test-support"))]
pub mod fake;
pub mod id_range;

pub use core_backend::CoreBackend;
pub use error::{BackendError, BackendErrorKind, BackendResult, BackendText};
#[cfg(any(test, feature = "test-support"))]
pub use fake::FakeBackend;
pub use id_range::AssetIdRange;

/// Page size of a query session when the caller has no reason for another one.
pub const DEFAULT_QUERY_PAGE_SIZE: usize = 128;
/// Most IDs one [`Backend::get_asset_summaries_by_ids`] call accepts.
pub const MAX_SUMMARY_BATCH: usize = 256;
/// Length of a thumbnail version token (`AssetSummary::thumb_version`).
pub const THUMB_VERSION_LEN: usize = 16;

/// The media kinds a query can be narrowed to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaKind {
    Image,
    Gif,
    Video,
}

impl MediaKind {
    /// The value of `AssetSummary::kind` and of the query filter.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Gif => "gif",
            Self::Video => "video",
        }
    }

    pub fn parse(kind: &str) -> Option<Self> {
        match kind {
            "image" => Some(Self::Image),
            "gif" => Some(Self::Gif),
            "video" => Some(Self::Video),
            _ => None,
        }
    }
}

/// The meta filters of the search grammar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetaFilter {
    /// Assets with exactly `tag_count` tags (`0`: untagged).
    HasNoTags { tag_count: i64 },
    /// Assets of the media group named `group_name`.
    GroupName { group_name: String },
}

/// Arguments of `start_asset_query`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetQueryRequest {
    pub tags_and: Vec<String>,
    pub tags_not: Vec<String>,
    /// `None`: every kind.
    pub kind: Option<MediaKind>,
    pub favorites_only: bool,
    pub meta_filter: Option<MetaFilter>,
    /// The model instance that owns the query (1 to 128 bytes). Generations are
    /// compared within one owner.
    pub owner_id: String,
    /// Increases with every query of the owner; an older generation that
    /// arrives late is answered `Superseded`.
    pub generation: u64,
    pub page_size: usize,
    /// Global index whose page the `Ready` result carries instead of page zero.
    pub anchor_offset: Option<usize>,
}

impl AssetQueryRequest {
    /// An unfiltered query of `owner_id` with the default page size.
    pub fn new(owner_id: impl Into<String>, generation: u64) -> Self {
        Self {
            tags_and: Vec::new(),
            tags_not: Vec::new(),
            kind: None,
            favorites_only: false,
            meta_filter: None,
            owner_id: owner_id.into(),
            generation,
            page_size: DEFAULT_QUERY_PAGE_SIZE,
            anchor_offset: None,
        }
    }
}

/// A bulk selection against one query session. `ranges` are inclusive
/// `[start, end]` session positions, ascending and non-overlapping; `excluded`
/// are IDs inside them that are not selected, `included` selected IDs outside
/// them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SelectionSpec {
    pub session_id: u64,
    pub ranges: Vec<[usize; 2]>,
    pub excluded: Vec<i64>,
    pub included: Vec<i64>,
}

/// The metadata change applied to every selected asset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionMutation {
    /// Adds tags; never removes any.
    MergeTags { tags: Vec<String> },
    /// Favorites all selected assets unless all already are, then clears all.
    ToggleFavorite,
    /// Assigns the group key with orders 1..n in selection order, or clears it.
    SetMediaGroup { media_group_key: Option<String> },
}

/// One asset of `set_assets_media_group_bulk` with its place in the group.
#[derive(Debug, Clone, PartialEq)]
pub struct MediaGroupUpdate {
    pub asset_id: i64,
    pub media_group_order: Option<f64>,
}

/// One on-demand thumbnail request: the typed form of a
/// `thumb://localhost/<id>?v=<version>[&c=1][&p=1][&r=<retry>]` URL.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ThumbnailRequest {
    pub asset_id: i64,
    /// `AssetSummary::thumb_version` of the summary the tile shows.
    pub version: String,
    /// Serve an existing file only and never start a render (fast-scroll pause).
    pub cached_only: bool,
    /// Overscan tile: render behind every visible request.
    pub prefetch: bool,
    /// Which retry of the tile this is (0: the first load). The backend ignores
    /// it, as it ignores `r`.
    pub retry: u32,
}

impl ThumbnailRequest {
    /// Whether the request names a positive asset ID and a version token of 16
    /// lowercase hex digits. Anything else is answered [`ThumbnailReply::BadRequest`].
    pub fn is_well_formed(&self) -> bool {
        self.asset_id > 0
            && self.version.len() == THUMB_VERSION_LEN
            && self
                .version
                .bytes()
                .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    }
}

/// A JPEG thumbnail and the source version it was rendered from.
#[derive(Clone, PartialEq, Eq)]
pub struct ThumbnailImage {
    pub bytes: Vec<u8>,
    /// Version token of the bytes. It differs from the requested version when
    /// the source changed after the summary was read.
    pub version: String,
}

impl Debug for ThumbnailImage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThumbnailImage")
            .field("bytes", &self.bytes.len())
            .field("version", &self.version)
            .finish()
    }
}

/// The answer to a thumbnail request, with the meaning of the `thumb://` status
/// codes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThumbnailReply {
    /// `200`.
    Ready(ThumbnailImage),
    /// `404`: no asset, no source, a failed render, a cached-only miss, or a
    /// render that lost against a newer record. A later request may succeed.
    NotFound,
    /// `503`: the backend is not ready yet, the render queue is full or the
    /// database is busy. Ask again later.
    Unavailable,
    /// `400`: the request is not well formed.
    BadRequest,
}

/// Answers one thumbnail request exactly once, on whichever thread has the
/// answer. A responder dropped without an answer answers
/// [`ThumbnailReply::Unavailable`], so a request is never left open.
pub struct ThumbnailResponder(Option<Box<dyn FnOnce(ThumbnailReply) + Send + 'static>>);

impl ThumbnailResponder {
    pub fn new(respond: impl FnOnce(ThumbnailReply) + Send + 'static) -> Self {
        Self(Some(Box::new(respond)))
    }

    pub fn respond(mut self, reply: ThumbnailReply) {
        if let Some(respond) = self.0.take() {
            respond(reply);
        }
    }
}

impl Drop for ThumbnailResponder {
    fn drop(&mut self) {
        if let Some(respond) = self.0.take() {
            respond(ThumbnailReply::Unavailable);
        }
    }
}

impl Debug for ThumbnailResponder {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("ThumbnailResponder")
    }
}

pub trait Backend: Send + Sync + 'static {
    // Query sessions and reads.

    /// Starts (or reuses) the query session of the request's filters and returns
    /// its first page, or `Superseded` when a newer generation of the same owner
    /// was admitted first.
    fn start_asset_query(&self, request: AssetQueryRequest)
        -> BackendResult<StartAssetQueryResult>;
    fn get_asset_query_page(
        &self,
        session_id: u64,
        offset: usize,
        limit: usize,
    ) -> BackendResult<AssetQueryPageResult>;
    fn get_asset_query_position(
        &self,
        session_id: u64,
        asset_id: i64,
    ) -> BackendResult<AssetQueryPositionResult>;
    /// Ordered snapshot IDs for the inclusive range `start..=end`, clamped by
    /// the backend.
    fn get_asset_query_id_range(
        &self,
        session_id: u64,
        start: usize,
        end: usize,
    ) -> BackendResult<AssetIdRange>;
    /// Timeline sections (months) of a query session.
    fn get_asset_query_sections(&self, session_id: u64) -> BackendResult<AssetQuerySectionsResult>;
    /// Summaries of at most [`MAX_SUMMARY_BATCH`] assets; unknown IDs are left out.
    fn get_asset_summaries_by_ids(&self, asset_ids: Vec<i64>) -> BackendResult<Vec<AssetSummary>>;
    fn get_asset_details(&self, asset_id: i64) -> BackendResult<Option<AssetDetails>>;
    fn list_tags(&self, query: String, offset: i64, limit: i64) -> BackendResult<TagListPage>;

    // Tags, favorites and media groups.

    fn set_asset_tags(
        &self,
        asset_id: i64,
        tags: Vec<String>,
    ) -> BackendResult<SetAssetTagsSummary>;
    fn merge_asset_tags_bulk(
        &self,
        asset_ids: Vec<i64>,
        tags: Vec<String>,
    ) -> BackendResult<BulkTagMergeSummary>;
    fn set_asset_favorite(&self, asset_id: i64, is_favorite: bool) -> BackendResult<()>;
    fn toggle_assets_favorite_bulk(
        &self,
        asset_ids: Vec<i64>,
    ) -> BackendResult<BulkFavoriteSummary>;
    fn set_asset_media_group(
        &self,
        asset_id: i64,
        media_group_key: Option<String>,
        media_group_order: Option<f64>,
    ) -> BackendResult<()>;
    fn set_assets_media_group_bulk(
        &self,
        updates: Vec<MediaGroupUpdate>,
        media_group_key: Option<String>,
    ) -> BackendResult<BulkMediaGroupSummary>;

    // Selection.

    /// Bulk tag, favorite or media-group write resolved on the spec's query
    /// session; `Stale` writes nothing.
    fn apply_selection_mutation(
        &self,
        selection: SelectionSpec,
        mutation: SelectionMutation,
    ) -> BackendResult<SelectionMutationResult>;

    // Deleting and duplicates.

    /// Deletes the source file and the asset. Fails at once with
    /// `i18n:common.libraryBusy` while a scan or another library workflow runs.
    fn delete_asset(&self, asset_id: i64) -> BackendResult<DeleteAssetSummary>;
    /// Progress phases `duplicates-scan` and `duplicates-scan-done`.
    fn find_duplicate_assets(&self) -> BackendResult<DuplicateScanSummary>;
    fn apply_duplicate_resolution_batch(
        &self,
        input: DuplicateResolutionBatchInput,
    ) -> BackendResult<DuplicateResolutionBatchSummary>;

    // Scanning.

    /// Adds `path` as a scan root and scans it, with scan progress.
    fn scan_folder(&self, path: String) -> BackendResult<ScanSummary>;
    fn list_scan_roots(&self) -> BackendResult<Vec<ScanRoot>>;
    fn add_scan_root(&self, path: String) -> BackendResult<()>;
    fn remove_scan_root(&self, path: String) -> BackendResult<RemoveRootSummary>;
    /// Scans every root, with scan progress.
    fn rescan_all_roots(&self) -> BackendResult<ScanSummary>;
    fn set_scan_root_auto_scan(&self, path: String, enabled: bool) -> BackendResult<()>;
    /// Joins the process-owned startup scan, including its retained success or
    /// failure ([`crate::startup_scan`]). `None`: the launch snapshot had no
    /// enabled root.
    fn scan_startup_roots(&self) -> BackendResult<Option<ScanSummary>>;

    // Thumbnails.

    fn get_video_tool_status(&self) -> BackendResult<VideoToolStatus>;
    /// Renders every missing thumbnail, with progress.
    fn render_all_thumbnails(&self) -> BackendResult<ThumbnailRenderSummary>;
    /// Renders the thumbnails that failed before, with progress.
    fn render_failed_thumbnails(&self) -> BackendResult<ThumbnailRenderSummary>;
    /// Asks a running bulk render to stop. `false`: none is running. Takes no
    /// lock and returns at once.
    fn cancel_render_all_thumbnails(&self) -> BackendResult<bool>;
    /// Removes every thumbnail, with progress; returns how many files went.
    fn clear_all_thumbnails(&self) -> BackendResult<usize>;
    /// Serves or renders one thumbnail and answers through `responder`. A
    /// thumbnail that has to be rendered is answered later, from another thread;
    /// the call itself only reads the database and the thumbnail file, and never
    /// waits for a render.
    fn request_thumbnail(&self, request: ThumbnailRequest, responder: ThumbnailResponder);

    // Library transfer and maintenance.

    fn export_tags_csv(&self, path: String) -> BackendResult<CsvExportSummary>;
    fn import_tags_csv(&self, path: String) -> BackendResult<CsvImportSummary>;
    /// Progress phases `library-clear` and `library-clear-done`.
    fn clear_library_data(&self) -> BackendResult<ClearLibrarySummary>;
    fn export_db_bundle(&self, path: String) -> BackendResult<DbBundleExportSummary>;
    fn import_db_bundle(
        &self,
        path: String,
        root_mappings: Vec<DbRootMapping>,
    ) -> BackendResult<DbBundleImportSummary>;
    fn inspect_db_bundle(&self, path: String) -> BackendResult<DbBundleInspection>;

    // Progress.

    /// Registers a listener for the progress of every long operation of this
    /// backend ([`crate::progress`]). Subscribe before starting the operation.
    /// Does no blocking work.
    fn subscribe_progress(&self, listener: ProgressListener) -> ProgressSubscription;
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::{
        AssetQueryRequest, MediaKind, ThumbnailImage, ThumbnailReply, ThumbnailRequest,
        ThumbnailResponder, DEFAULT_QUERY_PAGE_SIZE,
    };

    fn request(asset_id: i64, version: &str) -> ThumbnailRequest {
        ThumbnailRequest {
            asset_id,
            version: version.to_owned(),
            cached_only: false,
            prefetch: false,
            retry: 0,
        }
    }

    #[test]
    fn a_thumbnail_request_needs_a_positive_id_and_a_hex_version_token() {
        assert!(request(42, "0123456789abcdef").is_well_formed());
        assert!(!request(0, "0123456789abcdef").is_well_formed());
        assert!(!request(-1, "0123456789abcdef").is_well_formed());
        assert!(!request(42, "0123456789abcde").is_well_formed());
        assert!(!request(42, "0123456789abcdef0").is_well_formed());
        assert!(!request(42, "0123456789ABCDEF").is_well_formed());
        assert!(!request(42, "../../etc/passwd").is_well_formed());
        assert!(!request(42, "").is_well_formed());
    }

    #[test]
    fn a_responder_answers_once_and_a_dropped_one_answers_unavailable() {
        let replies = Arc::new(Mutex::new(Vec::new()));
        let responder = |replies: &Arc<Mutex<Vec<ThumbnailReply>>>| {
            let replies = Arc::clone(replies);
            ThumbnailResponder::new(move |reply| replies.lock().unwrap().push(reply))
        };

        responder(&replies).respond(ThumbnailReply::NotFound);
        drop(responder(&replies));

        assert_eq!(
            *replies.lock().unwrap(),
            [ThumbnailReply::NotFound, ThumbnailReply::Unavailable]
        );
    }

    #[test]
    fn thumbnail_bytes_stay_out_of_debug_output() {
        let image = ThumbnailImage {
            bytes: vec![0xff; 4096],
            version: "0123456789abcdef".into(),
        };

        assert_eq!(
            format!("{image:?}"),
            "ThumbnailImage { bytes: 4096, version: \"0123456789abcdef\" }"
        );
    }

    #[test]
    fn media_kinds_use_the_values_of_the_summaries() {
        for kind in [MediaKind::Image, MediaKind::Gif, MediaKind::Video] {
            assert_eq!(MediaKind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(MediaKind::parse("all"), None);
    }

    #[test]
    fn a_new_query_request_is_unfiltered_with_the_default_page_size() {
        let request = AssetQueryRequest::new("owner", 3);

        assert_eq!(request.page_size, DEFAULT_QUERY_PAGE_SIZE);
        assert_eq!(
            (request.owner_id.as_str(), request.generation),
            ("owner", 3)
        );
        assert!(request.tags_and.is_empty() && request.tags_not.is_empty());
        assert_eq!(request.kind, None);
        assert!(!request.favorites_only);
        assert_eq!(request.meta_filter, None);
        assert_eq!(request.anchor_offset, None);
    }
}
