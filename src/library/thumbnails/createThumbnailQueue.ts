import { createSignal, onCleanup } from "solid-js";
import { ensureThumbnailsStream } from "../../platform/tauri/thumbnails";
import { ThumbnailStore } from "./thumbnailStore";

const UI_FLUSH_DELAY_MS = 24;
const QUEUE_PROCESS_DELAY_MS = 36;
const THUMBNAIL_BATCH_SIZE = 64;

export function createThumbnailQueue() {
  const thumbnailStore = new ThumbnailStore();
  const failed: Set<number> = new Set();
  let currentGeneration = 0;

  let pendingThumbUpdates: Record<number, string> = {};
  let pendingRenderedDone: Set<number> = new Set();

  const queuedIds: Set<number> = new Set();
  let galleryVisible: Set<number> = new Set();
  let galleryPrefetch: Set<number> = new Set();
  let mounted = true;
  const inFlightIds: Set<number> = new Set();
  let processRunning = false;

  let uiFlushTimer: ReturnType<typeof globalThis.setTimeout> | null = null;
  let processTimer: ReturnType<typeof globalThis.setTimeout> | null = null;

  const [isGeneratingPage, setIsGeneratingPage] = createSignal(false);
  const [pendingPageSize, setPendingPageSize] = createSignal(0);

  const updateQueueState = () => {
    const pendingCount = new Set([
      ...queuedIds,
      ...galleryVisible,
      ...galleryPrefetch,
      ...inFlightIds
    ]).size;
    setPendingPageSize(pendingCount);
    setIsGeneratingPage(pendingCount > 0);
  };

  const flushQueuedUpdates = () => {
    if (uiFlushTimer !== null) {
      globalThis.clearTimeout(uiFlushTimer);
      uiFlushTimer = null;
    }

    const paths = pendingThumbUpdates;
    const finishedRenderingIds = pendingRenderedDone;
    pendingThumbUpdates = {};
    pendingRenderedDone = new Set();

    thumbnailStore.complete(paths, finishedRenderingIds);
  };

  const scheduleQueuedFlush = () => {
    if (uiFlushTimer !== null) {
      return;
    }

    uiFlushTimer = globalThis.setTimeout(() => {
      flushQueuedUpdates();
    }, UI_FLUSH_DELAY_MS);
  };

  const scheduleQueueProcessing = () => {
    if (processTimer !== null) {
      return;
    }

    processTimer = globalThis.setTimeout(() => {
      processTimer = null;
      void processQueuedBatches();
    }, QUEUE_PROCESS_DELAY_MS);
  };

  const processQueuedBatches = async () => {
    if (processRunning) {
      return;
    }

    processRunning = true;

    try {
      while (
        mounted &&
        (queuedIds.size > 0 || galleryVisible.size > 0 || galleryPrefetch.size > 0)
      ) {
        const generation = currentGeneration;
        const visible: number[] = [];
        const prefetch: number[] = [];
        const take = (source: Set<number>, destination: number[]) => {
          for (const id of source) {
            if (visible.length + prefetch.length >= THUMBNAIL_BATCH_SIZE) break;
            queuedIds.delete(id);
            galleryVisible.delete(id);
            galleryPrefetch.delete(id);
            if (!thumbnailStore.getPath(id) && !failed.has(id)) destination.push(id);
          }
        };
        take(galleryVisible, visible);
        take(queuedIds, visible);
        take(galleryPrefetch, prefetch);
        const batch = [...visible, ...prefetch];
        if (!batch.length) {
          break;
        }

        for (const assetId of batch) {
          inFlightIds.add(assetId);
        }

        updateQueueState();
        thumbnailStore.markRendering(batch, true);

        try {
          await ensureThumbnailsStream(generation, visible, prefetch, (message) => {
            if (!mounted || generation !== currentGeneration) return;
            if (message.event === "ready") {
              pendingThumbUpdates[message.data.asset_id] = message.data.thumb_path;
              pendingRenderedDone.add(message.data.asset_id);
              scheduleQueuedFlush();
            } else if (message.event === "stale") {
              pendingRenderedDone.add(message.data.asset_id);
              scheduleQueuedFlush();
            } else if (message.event === "failed") {
              failed.add(message.data.asset_id);
              pendingRenderedDone.add(message.data.asset_id);
              scheduleQueuedFlush();
            }
          });
        } catch {
          if (generation !== currentGeneration) {
            continue;
          }

          for (const assetId of batch) {
            failed.add(assetId);
            pendingRenderedDone.add(assetId);
          }

          scheduleQueuedFlush();
        } finally {
          if (generation === currentGeneration && mounted) {
            for (const assetId of batch) {
              pendingRenderedDone.add(assetId);
              inFlightIds.delete(assetId);
            }

            updateQueueState();
            flushQueuedUpdates();
          }
        }
      }
    } finally {
      processRunning = false;
      if (mounted) {
        updateQueueState();
        if (queuedIds.size > 0 || galleryVisible.size > 0 || galleryPrefetch.size > 0) {
          scheduleQueueProcessing();
        }
      }
    }
  };

  onCleanup(() => {
    mounted = false;
    resetThumbnailQueue();
  });

  const queueThumbnailsByIds = (assetIds: number[]) => {
    if (!mounted || !assetIds.length) {
      return;
    }

    let added = false;

    for (const assetId of assetIds) {
      if (assetId <= 0) {
        continue;
      }

      if (thumbnailStore.getPath(assetId)) {
        continue;
      }

      if (pendingThumbUpdates[assetId]) {
        continue;
      }

      if (failed.has(assetId)) {
        continue;
      }

      if (queuedIds.has(assetId) || inFlightIds.has(assetId)) {
        continue;
      }

      queuedIds.add(assetId);
      added = true;
    }

    if (!added) {
      return;
    }

    updateQueueState();
    scheduleQueueProcessing();
  };

  const setGalleryThumbnailDemand = (visibleIds: number[], prefetchIds: number[]) => {
    if (!mounted) return;
    const needsThumbnail = (id: number) =>
      id > 0 &&
      !thumbnailStore.getPath(id) &&
      !pendingThumbUpdates[id] &&
      !failed.has(id) &&
      !inFlightIds.has(id);
    galleryVisible = new Set(visibleIds.filter(needsThumbnail));
    galleryPrefetch = new Set(
      prefetchIds.filter((id) => !galleryVisible.has(id) && needsThumbnail(id))
    );
    updateQueueState();
    if (galleryVisible.size || galleryPrefetch.size) scheduleQueueProcessing();
  };

  const resetThumbnailQueue = () => {
    if (uiFlushTimer !== null) {
      globalThis.clearTimeout(uiFlushTimer);
      uiFlushTimer = null;
    }

    if (processTimer !== null) {
      globalThis.clearTimeout(processTimer);
      processTimer = null;
    }

    currentGeneration += 1;
    failed.clear();

    pendingThumbUpdates = {};
    pendingRenderedDone = new Set();
    queuedIds.clear();
    galleryVisible.clear();
    galleryPrefetch.clear();
    inFlightIds.clear();

    setIsGeneratingPage(false);
    setPendingPageSize(0);

    thumbnailStore.clear();
  };

  return {
    queueThumbnailsByIds,
    setGalleryThumbnailDemand,
    resetThumbnailQueue,
    get isGeneratingPage() {
      return isGeneratingPage();
    },
    get pendingPageSize() {
      return pendingPageSize();
    },

    thumbnailStore
  };
}
