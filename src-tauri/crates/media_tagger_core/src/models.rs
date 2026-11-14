#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ScanRoot {
    pub path: String,
    pub auto_scan_on_startup: bool,
}

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AssetSummary {
    pub id: i64,
    pub file_name: String,
    pub preview_path: Option<String>,
    pub kind: String,
    pub modified_at: i64,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub duration_ms: Option<i64>,
    /// Short token of the source version; the `v` parameter of `thumb://` URLs.
    pub thumb_version: String,
    pub is_favorite: bool,
    pub media_group_key: Option<String>,
    pub media_group_order: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssetDetails {
    #[serde(flatten)]
    pub summary: AssetSummary,
    pub path: String,
    pub size_bytes: i64,
    pub thumb_path: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum StartAssetQueryResult {
    Ready {
        session_id: u64,
        revision: i64,
        total: usize,
        offset: usize,
        items: Vec<AssetSummary>,
    },
    Superseded,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum AssetQueryPageResult {
    Ready {
        session_id: u64,
        revision: i64,
        total: usize,
        offset: usize,
        items: Vec<AssetSummary>,
    },
    Stale,
}

/// One timeline section of a query session: `count` rows from `start` whose
/// bucket month is `t` (`yyyymm` in UTC, `0` for no date). Adjacent sections
/// may share a month when a large month is split.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct AssetQuerySection {
    pub t: u32,
    pub start: usize,
    pub count: usize,
}

/// `Ready` with no sections while `total > 0` means the sections are
/// unavailable for this snapshot and the session stays valid (ADR 0027).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum AssetQuerySectionsResult {
    Ready {
        session_id: u64,
        total: usize,
        sections: Vec<AssetQuerySection>,
    },
    Stale,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AssetQueryPositionResult {
    Resolved { index: usize },
    Missing,
    Stale,
}

#[derive(Debug, Serialize)]
pub struct TagListPage {
    pub items: Vec<String>,
    pub total: i64,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ScanCompletion {
    Complete,
    Partial,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanSummary {
    pub completion: ScanCompletion,
    pub indexed: usize,
    pub removed: usize,
    pub failed: usize,
}

#[derive(Debug, Serialize)]
pub struct RemoveRootSummary {
    pub removed_assets: usize,
    pub removed_thumbnails: usize,
}

#[derive(Debug, Serialize)]
pub struct DeleteAssetSummary {
    pub removed_assets: usize,
    pub removed_thumbnails: usize,
    pub source_status: DeleteSourceStatus,
    pub revision: i64,
    pub recovery_path: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeleteSourceStatus {
    Deleted,
    Missing,
    CleanupPending,
}

#[derive(Debug, Clone, Serialize)]
pub struct DuplicateAsset {
    pub id: i64,
    pub path: String,
    pub record_version: i64,
    pub size_bytes: i64,
    pub thumb_version: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DuplicateGroup {
    pub file_name: String,
    pub assets: Vec<DuplicateAsset>,
}

#[derive(Debug, Serialize)]
pub struct DuplicateScanSummary {
    pub groups: Vec<DuplicateGroup>,
    pub duplicate_groups: usize,
    pub duplicate_assets: usize,
    pub revision: i64,
}

#[derive(Debug, Serialize)]
pub struct RenameAssetSummary {
    pub asset_id: i64,
    pub old_path: String,
    pub new_path: String,
    pub removed_thumbnails: usize,
    pub revision: i64,
    pub status: RenameAssetStatus,
    pub recovery_path: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RenameAssetStatus {
    Renamed,
    CleanupPending,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DuplicateResolutionChangeInput {
    Rename {
        asset_id: i64,
        expected_path: String,
        expected_record_version: i64,
        new_file_name: String,
    },
    Delete {
        asset_id: i64,
        expected_path: String,
        expected_record_version: i64,
    },
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateResolutionBatchInput {
    pub scan_revision: i64,
    pub changes: Vec<DuplicateResolutionChangeInput>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DuplicateResolutionBatchStatus {
    Committed,
    RolledBack,
    RecoveryRequired,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DuplicateResolutionItemStatus {
    Renamed,
    Deleted,
    SourceMissing,
    RolledBack,
    RollbackFailed,
    CleanupPending,
}

#[derive(Debug, Serialize)]
pub struct DuplicateResolutionItemResult {
    pub asset_id: i64,
    pub status: DuplicateResolutionItemStatus,
    pub old_path: String,
    pub new_path: Option<String>,
    pub recovery_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DuplicateResolutionBatchSummary {
    pub status: DuplicateResolutionBatchStatus,
    pub revision: i64,
    pub results: Vec<DuplicateResolutionItemResult>,
    pub removed_thumbnails: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct AssetTagResult {
    pub asset_id: i64,
    pub changed: bool,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TagQueryImpact {
    None,
    Tags {
        changed_tags: Vec<String>,
        tag_count_changed: bool,
    },
    All,
}

impl TagQueryImpact {
    pub(crate) fn union(&mut self, other: Self) {
        match (&mut *self, other) {
            (Self::All, _) | (_, Self::None) => {}
            (_, Self::All) => *self = Self::All,
            (Self::None, impact) => *self = impact,
            (
                Self::Tags {
                    changed_tags,
                    tag_count_changed,
                },
                Self::Tags {
                    changed_tags: names,
                    tag_count_changed: count,
                },
            ) => {
                changed_tags.extend(names);
                changed_tags.sort();
                changed_tags.dedup();
                *tag_count_changed |= count;
            }
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SetAssetTagsSummary {
    pub vocabulary_changed: bool,
    pub query_impact: TagQueryImpact,
    pub asset_id: i64,
    pub changed: bool,
    pub tags: Vec<String>,
    pub revision: i64,
}

#[derive(Debug, Serialize)]
pub struct BulkTagMergeSummary {
    pub vocabulary_changed: bool,
    pub query_impact: TagQueryImpact,
    pub processed_assets: usize,
    pub updated_assets: usize,
    pub processed_asset_ids: Vec<i64>,
    pub updated_asset_ids: Vec<i64>,
    pub results: Vec<AssetTagResult>,
    pub revision: i64,
}

/// Result of `apply_selection_mutation` (stage 09). `stale` means the
/// selection's query session changed or is gone and nothing was written.
/// `query_impact` is the tag impact of a tag merge (`none` for the other
/// mutations); `is_favorite` and `media_group_key` are set only by their
/// mutation.
#[derive(Debug, Serialize, PartialEq)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum SelectionMutationResult {
    Ready {
        processed_assets: usize,
        updated_assets: usize,
        revision: i64,
        query_impact: TagQueryImpact,
        vocabulary_changed: bool,
        is_favorite: Option<bool>,
        media_group_key: Option<String>,
    },
    Stale,
}

#[derive(Debug, Serialize)]
pub struct BulkFavoriteSummary {
    pub processed_asset_ids: Vec<i64>,
    pub is_favorite: bool,
    pub revision: i64,
}

#[derive(Debug, Serialize)]
pub struct BulkMediaGroupSummary {
    pub processed_asset_ids: Vec<i64>,
    pub processed_assets: usize,
    pub updated_assets: usize,
    pub media_group_key: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ThumbnailRenderSummary {
    pub stale: usize,
    pub generated: usize,
    pub failed: usize,
    pub skipped_failed: usize,
    pub processed: usize,
    pub total: usize,
    pub cancelled: bool,
}

#[derive(Debug, Serialize)]
pub struct ClearLibrarySummary {
    pub removed_assets: usize,
    pub removed_roots: usize,
    pub removed_thumbnails: usize,
}

#[derive(Debug, Serialize)]
pub struct CsvExportSummary {
    pub rows: usize,
}

#[derive(Debug, Serialize)]
pub struct CsvImportSummary {
    pub rows_read: usize,
    pub rows_applied: usize,
    pub assets_matched: usize,
    pub assets_updated: usize,
}

#[derive(Debug, Serialize)]
pub struct DbBundleExportSummary {
    pub copied_files: usize,
    pub copied_thumbnails: usize,
}

#[derive(Debug, Serialize)]
pub struct DbBundleImportSummary {
    pub restored_files: usize,
    pub restored_thumbnails: usize,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DbRootMapping {
    pub source_root: String,
    pub target_root: String,
}

#[derive(Debug, Serialize)]
pub struct DbBundleInspection {
    pub format_version: Option<u32>,
    pub source_platform: Option<String>,
    pub source_thumbs_dir: Option<String>,
    pub roots: Vec<String>,
    pub requires_mapping: bool,
}

#[derive(Debug, Serialize)]
pub struct VideoToolStatus {
    pub ffmpeg_available: bool,
    pub ffprobe_available: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ScanProgress {
    pub phase: String,
    pub processed: usize,
    pub total: usize,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct NewAsset {
    pub path: String,
    pub kind: String,
    pub size_bytes: i64,
    pub modified_at: i64,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub duration_ms: Option<i64>,
    pub thumb_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ThumbnailAsset {
    pub id: i64,
    pub path: String,
    pub kind: String,
    pub modified_at: i64,
    pub duration_ms: Option<i64>,
    pub thumb_path: Option<String>,
    pub size_bytes: i64,
    pub fingerprint_mtime_ns: i64,
    pub record_version: i64,
}
