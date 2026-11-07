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
  AssetQueryIdRangeResult,
  AssetQueryPositionResult
} from "./contracts/assets";

export async function startAssetQuery(params: {
  tagsAnd: string[];
  tagsNot: string[];
  mediaKind: "all" | "image" | "gif" | "video";
  favoritesOnly: boolean;
  metaFilter?: SearchMetaFilter | null;
  ownerId: string;
  generation: number;
  pageSize?: number;
  /** Global index whose page the `ready` result carries instead of page zero. */
  anchorOffset?: number | null;
}): Promise<StartAssetQueryResult> {
  return await invoke<StartAssetQueryResult>("start_asset_query", {
    tagsAnd: params.tagsAnd,
    tagsNot: params.tagsNot,
    kind: params.mediaKind === "all" ? null : params.mediaKind,
    favoritesOnly: params.favoritesOnly,
    metaFilter: params.metaFilter ?? null,
    ownerId: params.ownerId,
    generation: params.generation,
    pageSize: params.pageSize ?? 128,
    anchorOffset: params.anchorOffset ?? null
  });
}

const LITTLE_ENDIAN_HOST = new Uint8Array(new Float64Array([1]).buffer)[7] === 0x3f;

function rangeBytes(body: unknown): Uint8Array {
  if (body instanceof ArrayBuffer) return new Uint8Array(body);
  if (ArrayBuffer.isView(body)) return new Uint8Array(body.buffer, body.byteOffset, body.byteLength);
  // Some transports deliver raw responses as a plain byte array.
  if (Array.isArray(body)) return Uint8Array.from(body);
  throw new Error("Asset ID range response is not binary");
}

/**
 * Decodes a binary ID range: little-endian f64 `[count, ...ids]`; an empty body
 * means `stale`. IDs are exact because the backend rejects IDs above 2^53.
 */
export function decodeAssetIdRange(body: unknown): AssetQueryIdRangeResult {
  const bytes = rangeBytes(body);
  if (bytes.byteLength === 0) return { status: "stale" };
  if (bytes.byteLength % 8 !== 0) throw new Error("Asset ID range response is malformed");
  let values: Float64Array;
  if (LITTLE_ENDIAN_HOST) {
    // Float64Array needs 8-byte alignment; copy only when the view is unaligned.
    const aligned = bytes.byteOffset % 8 === 0 ? bytes : bytes.slice();
    values = new Float64Array(aligned.buffer, aligned.byteOffset, aligned.byteLength / 8);
  } else {
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    values = Float64Array.from({ length: bytes.byteLength / 8 }, (_, i) =>
      view.getFloat64(i * 8, true)
    );
  }
  if (values[0] !== values.length - 1) throw new Error("Asset ID range response is incomplete");
  return { status: "ready", ids: values.subarray(1) };
}

/** Ordered snapshot IDs for the inclusive range `start..=end`, clamped by the backend. */
export async function getAssetQueryIdRange(
  sessionId: number,
  start: number,
  end: number
): Promise<AssetQueryIdRangeResult> {
  return decodeAssetIdRange(
    await invoke<ArrayBuffer>("get_asset_query_id_range", { sessionId, start, end })
  );
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
