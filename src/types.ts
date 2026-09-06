export interface ScanRoot {
  path: string;
  auto_scan_on_startup: boolean;
}

export type MediaKind = "image" | "gif" | "video";

export type SearchMetaFilter =
  | {
      type: "hasNoTags";
      tagCount: number;
    }
  | {
      type: "groupName";
      groupName: string;
    };

export interface AssetSummary {
  id: number;
  file_name: string;
  preview_path: string | null;
  kind: MediaKind;
  modified_at: number;
  width: number | null;
  height: number | null;
  duration_ms: number | null;
  thumb_path: string | null;
  is_favorite: boolean;
  media_group_key: string | null;
  media_group_order: number | null;
}

export interface AssetDetails extends AssetSummary {
  path: string;
  size_bytes: number;
  tags: string[];
}

export interface SelectedAsset extends AssetSummary {
  path: string | null;
  size_bytes: number | null;
  tags: string[];
}

export interface LegacyAsset {
  id: number;
  path: string;
  kind: MediaKind;
  size_bytes: number;
  modified_at: number;
  width: number | null;
  height: number | null;
  duration_ms: number | null;
  thumb_path: string | null;
  is_favorite: boolean;
  media_group_key: string | null;
  media_group_order: number | null;
  tags: string[];
}

export interface AssetQueryFilters {
  tags_and: string[];
  tags_not: string[];
  kind: MediaKind | null;
  favorites_only: boolean;
  meta_filter: SearchMetaFilter | null;
}

export interface AssetQueryReady {
  status: "ready";
  session_id: number;
  revision: number;
  total: number;
  offset: number;
  items: AssetSummary[];
}

export interface AssetQueryStale {
  status: "stale";
}

export interface AssetQuerySuperseded {
  status: "superseded";
}

export type StartAssetQueryResult = AssetQueryReady | AssetQuerySuperseded;
export type AssetQueryPageResult = AssetQueryReady | AssetQueryStale;

export type ThumbnailStreamEvent =
  | { event: "ready"; data: ThumbnailBatchItem }
  | { event: "failed"; data: { asset_id: number } }
  | { event: "done"; data: { ready: number; failed: number } };

export interface ScanSummary {
  completion: "complete" | "partial";
  indexed: number;
  removed: number;
  failed: number;
}

export interface RemoveRootSummary {
  removed_assets: number;
  removed_thumbnails: number;
}

export interface DeleteAssetSummary {
  removed_assets: number;
  removed_thumbnails: number;
  source_status: "deleted" | "missing" | "cleanup_pending";
  revision: number;
  recovery_path: string | null;
}

export interface DuplicateAsset {
  id: number;
  path: string;
  record_version: number;
  size_bytes: number;
}

export interface DuplicateGroup {
  file_name: string;
  assets: DuplicateAsset[];
}

export interface DuplicateScanSummary {
  groups: DuplicateGroup[];
  duplicate_groups: number;
  duplicate_assets: number;
  revision: number;
}

export type DuplicateResolutionBatchChange =
  | {
      type: "rename";
      assetId: number;
      expectedPath: string;
      expectedRecordVersion: number;
      newFileName: string;
    }
  | {
      type: "delete";
      assetId: number;
      expectedPath: string;
      expectedRecordVersion: number;
    };

export interface DuplicateResolutionItemResult {
  asset_id: number;
  status: "renamed" | "deleted" | "source_missing" | "rolled_back" | "rollback_failed" | "cleanup_pending";
  old_path: string;
  new_path: string | null;
  recovery_path: string | null;
}

export interface DuplicateResolutionBatchSummary {
  status: "committed" | "rolled_back" | "recovery_required";
  revision: number;
  results: DuplicateResolutionItemResult[];
  removed_thumbnails: number;
}

export interface AssetTagResult {
  asset_id: number;
  changed: boolean;
  tags: string[];
}

export interface SetAssetTagsSummary extends AssetTagResult {
  revision: number;
}

export interface BulkTagMergeSummary {
  processed_assets: number;
  updated_assets: number;
  processed_asset_ids: number[];
  updated_asset_ids: number[];
  results: AssetTagResult[];
  revision: number;
}

export interface BulkFavoriteSummary {
  processed_asset_ids: number[];
  is_favorite: boolean;
  revision: number;
}

export interface BulkMediaGroupSummary {
  processed_assets: number;
  updated_assets: number;
  media_group_key: string | null;
}

export interface ThumbnailRenderSummary {
  generated: number;
  failed: number;
  skipped_failed: number;
  processed: number;
  total: number;
  cancelled: boolean;
}

export interface ThumbnailBatchItem {
  asset_id: number;
  thumb_path: string;
}

export interface ClearLibrarySummary {
  removed_assets: number;
  removed_roots: number;
  removed_thumbnails: number;
}

export interface CsvExportSummary {
  rows: number;
}

export interface CsvImportSummary {
  rows_read: number;
  rows_applied: number;
  assets_matched: number;
  assets_updated: number;
}

export interface DbBundleExportSummary {
  copied_files: number;
  copied_thumbnails: number;
}

export interface DbBundleImportSummary {
  restored_files: number;
  restored_thumbnails: number;
}

export interface DbRootMapping {
  sourceRoot: string;
  targetRoot: string;
}

export interface DbBundleInspection {
  format_version: number | null;
  source_platform: string | null;
  source_thumbs_dir: string | null;
  roots: string[];
  requires_mapping: boolean;
}

export interface VideoToolStatus {
  ffmpeg_available: boolean;
  ffprobe_available: boolean;
}

export interface AssetPage {
  items: LegacyAsset[];
  total: number;
}

export interface TagListPage {
  items: string[];
  total: number;
}

export interface ScanProgress {
  phase: "counting" | "scanning" | "cleanup" | string;
  processed: number;
  total: number;
  message: string;
}
