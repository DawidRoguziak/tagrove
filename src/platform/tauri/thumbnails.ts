import { Channel, invoke } from "@tauri-apps/api/core";
import type { ThumbnailStreamEvent, ThumbnailRenderSummary } from "./contracts/thumbnails";

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
