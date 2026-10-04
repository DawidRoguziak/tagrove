import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import type {
  SearchMetaFilter,
  AssetSummary,
  AssetDetails,
  StartAssetQueryResult,
  AssetQueryPageResult,
  DeleteAssetSummary,
  BulkFavoriteSummary,
  BulkMediaGroupSummary,
  AssetPage,
  AssetQueryPositionResult
} from "./contracts/assets";

export async function startAssetQuery(params: {
  tagsAnd: string[];
  tagsNot: string[];
  mediaKind: "all" | "image" | "gif" | "video";
  favoritesOnly: boolean;
  metaFilter?: SearchMetaFilter | null;
  generation: number;
  pageSize?: number;
}): Promise<StartAssetQueryResult> {
  return await invoke<StartAssetQueryResult>("start_asset_query", {
    tagsAnd: params.tagsAnd,
    tagsNot: params.tagsNot,
    kind: params.mediaKind === "all" ? null : params.mediaKind,
    favoritesOnly: params.favoritesOnly,
    metaFilter: params.metaFilter ?? null,
    generation: params.generation,
    pageSize: params.pageSize ?? 128
  });
}

export async function getAssetQueryPage(
  sessionId: number,
  offset: number,
  limit = 128
): Promise<AssetQueryPageResult> {
  return await invoke<AssetQueryPageResult>("get_asset_query_page", {
    sessionId,
    offset,
    limit
  });
}

export async function getAssetDetails(assetId: number): Promise<AssetDetails | null> {
  return await invoke<AssetDetails | null>("get_asset_details", { assetId });
}

export async function listAssets(params: {
  offset: number;
  limit: number;
  tagsAnd: string[];
  tagsNot: string[];
  mediaKind: "all" | "image" | "gif" | "video";
  favoritesOnly: boolean;
  metaFilter?: SearchMetaFilter | null;
}): Promise<AssetPage> {
  return await invoke<AssetPage>("list_assets", {
    offset: params.offset,
    limit: params.limit,
    tagsAnd: params.tagsAnd,
    tagsNot: params.tagsNot,
    kind: params.mediaKind === "all" ? null : params.mediaKind,
    favoritesOnly: params.favoritesOnly,
    metaFilter: params.metaFilter ?? null
  });
}

export async function toggleAssetsFavoriteBulk(assetIds: number[]): Promise<BulkFavoriteSummary> {
  return await invoke<BulkFavoriteSummary>("toggle_assets_favorite_bulk", { assetIds });
}

export async function setAssetFavorite(assetId: number, isFavorite: boolean): Promise<void> {
  await invoke("set_asset_favorite", { assetId, isFavorite });
}

export async function setAssetMediaGroup(
  assetId: number,
  mediaGroupKey: string | null,
  mediaGroupOrder: number | null
): Promise<void> {
  await invoke("set_asset_media_group", { assetId, mediaGroupKey, mediaGroupOrder });
}

export async function setAssetsMediaGroupBulk(
  updates: Array<{ assetId: number; mediaGroupOrder: number | null }>,
  mediaGroupKey: string | null
): Promise<BulkMediaGroupSummary> {
  return await invoke<BulkMediaGroupSummary>("set_assets_media_group_bulk", {
    updates,
    mediaGroupKey
  });
}

export async function deleteAsset(assetId: number): Promise<DeleteAssetSummary> {
  return await invoke<DeleteAssetSummary>("delete_asset", { assetId });
}

export function toMediaSrc(path: string): string {
  return convertFileSrc(path);
}

export async function getAssetSummariesByIds(assetIds: number[]): Promise<AssetSummary[]> {
  return await invoke<AssetSummary[]>("get_asset_summaries_by_ids", { assetIds });
}

export async function getAssetQueryPosition(
  sessionId: number,
  assetId: number
): Promise<AssetQueryPositionResult> {
  return await invoke<AssetQueryPositionResult>("get_asset_query_position", { sessionId, assetId });
}
