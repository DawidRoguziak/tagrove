import { invoke } from "@tauri-apps/api/core";
import type { SetAssetTagsSummary, BulkTagMergeSummary, TagListPage } from "./contracts/tags";

export async function listTags(params: {
  query: string;
  offset: number;
  limit: number;
}): Promise<TagListPage> {
  return await invoke<TagListPage>("list_tags", {
    query: params.query,
    offset: params.offset,
    limit: params.limit
  });
}

export async function setAssetTags(assetId: number, tags: string[]): Promise<SetAssetTagsSummary> {
  return await invoke<SetAssetTagsSummary>("set_asset_tags", { assetId, tags });
}

export async function mergeAssetTagsBulk(
  assetIds: number[],
  tags: string[]
): Promise<BulkTagMergeSummary> {
  return await invoke<BulkTagMergeSummary>("merge_asset_tags_bulk", { assetIds, tags });
}
