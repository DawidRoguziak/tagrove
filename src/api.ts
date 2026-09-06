import { Channel, invoke, convertFileSrc } from "@tauri-apps/api/core";
import type {
  AssetDetails,
  AssetSummary,
  AssetQueryPageResult,
  AssetPage,
  BulkFavoriteSummary,
  BulkMediaGroupSummary,
  BulkTagMergeSummary,
  ClearLibrarySummary,
  CsvExportSummary,
  CsvImportSummary,
  DeleteAssetSummary,
  DbBundleExportSummary,
  DbBundleImportSummary,
  DbBundleInspection,
  DbRootMapping,
  DuplicateResolutionBatchChange,
  DuplicateResolutionBatchSummary,
  DuplicateScanSummary,
  RemoveRootSummary,
  ScanSummary,
  ScanRoot,
  SearchMetaFilter,
  SetAssetTagsSummary,
  StartAssetQueryResult,
  TagListPage,
  ThumbnailStreamEvent,
  ThumbnailRenderSummary,
  VideoToolStatus
} from "./types";
import type {
  MpvVideoEvent,
  NativeVideoControlLabels,
  VideoBounds,
  VideoControl
} from "./components/lightbox/mpvVideoTypes";

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

export async function beginVideoOpen(): Promise<number> {
  return await invoke<number>("begin_video_open");
}

export async function cancelVideoOpen(requestId: number): Promise<void> {
  await invoke("cancel_video_open", { requestId });
}

export async function setVideoControlLabels(sessionId: number, controlLabels: NativeVideoControlLabels): Promise<void> {
  await invoke("set_video_control_labels", { sessionId, controlLabels });
}

export async function openVideo(
  assetId: number,
  requestId: number,
  bounds: VideoBounds,
  controlLabels: NativeVideoControlLabels,
  onEvent: (event: MpvVideoEvent) => void
): Promise<number> {
  const channel = new Channel<MpvVideoEvent>();
  channel.onmessage = onEvent;
  return await invoke<number>("open_video", {
    assetId,
    requestId,
    bounds,
    controlLabels,
    onEvent: channel
  });
}

export async function setVideoBounds(sessionId: number, bounds: VideoBounds): Promise<void> {
  await invoke("set_video_bounds", { sessionId, bounds });
}

export async function controlVideo(sessionId: number, command: VideoControl): Promise<void> {
  await invoke("control_video", { sessionId, command });
}

export async function closeVideo(sessionId: number): Promise<void> {
  await invoke("close_video", { sessionId });
}

export async function scanFolder(path: string): Promise<ScanSummary> {
  return await invoke<ScanSummary>("scan_folder", { path });
}

export async function listScanRoots(): Promise<ScanRoot[]> {
  return await invoke<ScanRoot[]>("list_scan_roots");
}

export async function setScanRootAutoScan(path: string, enabled: boolean): Promise<void> {
  return await invoke<void>("set_scan_root_auto_scan", { path, enabled });
}

export async function scanStartupRoots(): Promise<ScanSummary | null> {
  return await invoke<ScanSummary | null>("scan_startup_roots");
}

export async function addScanRoot(path: string): Promise<void> {
  await invoke("add_scan_root", { path });
}

export async function removeScanRoot(path: string): Promise<RemoveRootSummary> {
  return await invoke<RemoveRootSummary>("remove_scan_root", { path });
}

export async function rescanAllRoots(): Promise<ScanSummary> {
  return await invoke<ScanSummary>("rescan_all_roots");
}

export async function renderAllThumbnails(): Promise<ThumbnailRenderSummary> {
  return await invoke<ThumbnailRenderSummary>("render_all_thumbnails");
}

export async function renderFailedThumbnails(): Promise<ThumbnailRenderSummary> {
  return await invoke<ThumbnailRenderSummary>("render_failed_thumbnails");
}

export async function cancelRenderAllThumbnails(): Promise<boolean> {
  return await invoke<boolean>("cancel_render_all_thumbnails");
}

export async function clearAllThumbnails(): Promise<number> {
  return await invoke<number>("clear_all_thumbnails");
}

export async function clearLibraryData(): Promise<ClearLibrarySummary> {
  return await invoke<ClearLibrarySummary>("clear_library_data");
}

export async function exportTagsCsv(path: string): Promise<CsvExportSummary> {
  return await invoke<CsvExportSummary>("export_tags_csv", { path });
}

export async function importTagsCsv(path: string): Promise<CsvImportSummary> {
  return await invoke<CsvImportSummary>("import_tags_csv", { path });
}

export async function exportDbBundle(path: string): Promise<DbBundleExportSummary> {
  return await invoke<DbBundleExportSummary>("export_db_bundle", { path });
}

export async function inspectDbBundle(path: string): Promise<DbBundleInspection> {
  return await invoke<DbBundleInspection>("inspect_db_bundle", { path });
}

export async function getVideoToolStatus(): Promise<VideoToolStatus> {
  return await invoke<VideoToolStatus>("get_video_tool_status");
}

export async function importDbBundle(
  path: string,
  rootMappings: DbRootMapping[] = []
): Promise<DbBundleImportSummary> {
  return await invoke<DbBundleImportSummary>("import_db_bundle", { path, rootMappings });
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

export async function findDuplicateAssets(): Promise<DuplicateScanSummary> {
  return await invoke<DuplicateScanSummary>("find_duplicate_assets");
}

export async function applyDuplicateResolutionBatch(
  scanRevision: number,
  changes: DuplicateResolutionBatchChange[]
): Promise<DuplicateResolutionBatchSummary> {
  return await invoke<DuplicateResolutionBatchSummary>("apply_duplicate_resolution_batch", {
    input: { scanRevision, changes }
  });
}

export async function ensureThumbnailsStream(
  requestId: number,
  visibleIds: number[],
  prefetchIds: number[],
  onEvent: (event: ThumbnailStreamEvent) => void
): Promise<void> {
  const channel = new Channel<ThumbnailStreamEvent>();
  channel.onmessage = onEvent;
  await invoke("ensure_thumbnails", {
    requestId,
    visibleIds,
    prefetchIds,
    onEvent: channel
  });
}

export function toMediaSrc(path: string): string {
  return convertFileSrc(path);
}
export async function syncNativeWindowTheme(theme: "light" | "dark"): Promise<void> {
  await invoke("sync_window_theme", { theme });
}

export async function getAssetSummariesByIds(assetIds: number[]): Promise<AssetSummary[]> {
  return await invoke<AssetSummary[]>("get_asset_summaries_by_ids", { assetIds });
}
