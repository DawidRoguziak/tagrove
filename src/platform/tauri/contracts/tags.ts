export interface AssetTagResult {
  asset_id: number;
  changed: boolean;
  tags: string[];
}

export type TagQueryImpact =
  | { type: "none" }
  | { type: "tags"; changed_tags: string[]; tag_count_changed: boolean }
  | { type: "all" };

export interface SetAssetTagsSummary extends AssetTagResult {
  vocabulary_changed: boolean;
  query_impact: TagQueryImpact;
  revision: number;
}

export interface BulkTagMergeSummary {
  vocabulary_changed: boolean;
  query_impact: TagQueryImpact;
  processed_assets: number;
  updated_assets: number;
  processed_asset_ids: number[];
  updated_asset_ids: number[];
  results: AssetTagResult[];
  revision: number;
}

export interface TagListPage {
  items: string[];
  total: number;
}
