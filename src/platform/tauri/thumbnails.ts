import { invoke } from "@tauri-apps/api/core";
import type { ThumbnailRenderSummary } from "./contracts/thumbnails";

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

/**
 * URL of an asset's gallery thumbnail, served by the `thumb://` scheme. The
 * backend resolves the file from the ID; `version` makes the URL change exactly
 * when the thumbnail does, so WebKit may cache each URL forever. `cachedOnly`
 * serves an existing file but never starts a render (404 instead), for tiles
 * shown during a fast-scroll pause. `prefetch` (overscan tiles) renders behind
 * every visible request. `retry` busts the cache of a failed load and is
 * otherwise ignored by the backend.
 */
export function thumbnailUrl(
  assetId: number,
  version: string,
  options: { retry?: number; cachedOnly?: boolean; prefetch?: boolean } = {}
): string {
  let url = `thumb://localhost/${assetId}?v=${version}`;
  if (options.cachedOnly) url += "&c=1";
  if (options.prefetch) url += "&p=1";
  if (options.retry) url += `&r=${options.retry}`;
  return url;
}
