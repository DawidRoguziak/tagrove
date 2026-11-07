import type { TagQueryImpact } from "./tags";

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
  /** Source-version token; the `v` parameter of the `thumb://` URL. */
  thumb_version: string;
  is_favorite: boolean;
  media_group_key: string | null;
  media_group_order: number | null;
}

export interface AssetDetails extends AssetSummary {
  path: string;
  size_bytes: number;
  thumb_path: string | null;
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

/**
 * One timeline section: `count` rows from `start` whose bucket month is `t`
 * (`yyyymm` in UTC; `0` means no date). Adjacent sections may share a month
 * when a large month is split.
 */
export interface AssetQuerySection {
  t: number;
  start: number;
  count: number;
}

export type AssetQuerySectionsResult =
  | { status: "ready"; session_id: number; total: number; sections: AssetQuerySection[] }
  | AssetQueryStale;

/**
 * Decoded `get_asset_query_id_range` body. The wire format is binary, not JSON:
 * little-endian f64 `[count, ...ids]`, or zero bytes for `stale`.
 */
export type AssetQueryIdRangeResult = { status: "ready"; ids: Float64Array } | AssetQueryStale;

export interface DeleteAssetSummary {
  removed_assets: number;
  removed_thumbnails: number;
  source_status: "deleted" | "missing" | "cleanup_pending";
  revision: number;
  recovery_path: string | null;
}

export interface BulkFavoriteSummary {
  processed_asset_ids: number[];
  is_favorite: boolean;
  revision: number;
}

/**
 * A bulk selection against one query session (stage 09). `ranges` are inclusive
 * `[start, end]` session positions, ascending and non-overlapping; `excluded` are IDs
 * inside them that are not selected, `included` selected IDs outside them.
 */
export interface SelectionSpec {
  sessionId: number;
  ranges: Array<[number, number]>;
  excluded: number[];
  included: number[];
}

export type SelectionMutation =
  | { type: "mergeTags"; tags: string[] }
  | { type: "toggleFavorite" }
  | { type: "setMediaGroup"; mediaGroupKey: string | null };

/** `stale`: the session changed or is gone, and nothing was written. */
export type SelectionMutationResult =
  | {
      status: "ready";
      processed_assets: number;
      updated_assets: number;
      revision: number;
      query_impact: TagQueryImpact;
      vocabulary_changed: boolean;
      is_favorite: boolean | null;
      media_group_key: string | null;
    }
  | AssetQueryStale;

export interface BulkMediaGroupSummary {
  processed_asset_ids: number[];
  processed_assets: number;
  updated_assets: number;
  media_group_key: string | null;
}

export type AssetQueryPositionResult =
  | { status: "resolved"; index: number }
  | { status: "missing" }
  | { status: "stale" };
