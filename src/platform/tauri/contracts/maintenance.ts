export interface DuplicateAsset {
  id: number;
  path: string;
  record_version: number;
  size_bytes: number;
  thumb_version: string;
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
  status:
    | "renamed"
    | "deleted"
    | "source_missing"
    | "rolled_back"
    | "rollback_failed"
    | "cleanup_pending";
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
