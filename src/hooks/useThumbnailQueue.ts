import { useCallback, useEffect, useRef, useState } from "react";
import type { Dispatch, SetStateAction } from "react";
import { ensureThumbnailsStream } from "../api";
import {
  mergeThumbnailUpdates,
  QUEUE_PROCESS_DELAY_MS,
  removeRenderingAssetIds,
  takeThumbnailBatch,
  THUMBNAIL_BATCH_SIZE,
  UI_FLUSH_DELAY_MS
} from "./services/thumbnailQueueService";
import { ThumbnailStore } from "./services/thumbnailStore";

interface UseThumbnailQueueArgs {
  thumbs: Record<number, string>;
  setThumbs: Dispatch<SetStateAction<Record<number, string>>>;
}

export function useThumbnailQueue({ thumbs, setThumbs }: UseThumbnailQueueArgs) {
  const thumbnailStoreRef = useRef<ThumbnailStore | null>(null);
  if (!thumbnailStoreRef.current) thumbnailStoreRef.current = new ThumbnailStore();
  const thumbnailStore = thumbnailStoreRef.current;
  const thumbsRef = useRef<Record<number, string>>({});
  const failedRef = useRef<Set<number>>(new Set());
  const generationRef = useRef(0);

  const pendingThumbUpdatesRef = useRef<Record<number, string>>({});
  const pendingRenderedDoneRef = useRef<Set<number>>(new Set());

  const queuedIdsRef = useRef<Set<number>>(new Set());
  const inFlightIdsRef = useRef<Set<number>>(new Set());
  const processRunningRef = useRef(false);

  const uiFlushTimerRef = useRef<ReturnType<typeof globalThis.setTimeout> | null>(null);
  const processTimerRef = useRef<ReturnType<typeof globalThis.setTimeout> | null>(null);
  const processQueueRef = useRef<() => void>(() => {});

  const [isGeneratingPage, setIsGeneratingPage] = useState(false);
  const [pendingPageSize, setPendingPageSize] = useState(0);
  const [renderingAssetIds, setRenderingAssetIds] = useState<Record<number, true>>({});

  useEffect(() => {
    thumbsRef.current = thumbs;
    thumbnailStore.sync(thumbs);
  }, [thumbnailStore, thumbs]);

  const updateQueueState = useCallback(() => {
    const pendingCount = queuedIdsRef.current.size + inFlightIdsRef.current.size;
    setPendingPageSize(pendingCount);
    setIsGeneratingPage(pendingCount > 0);
  }, []);

  const flushQueuedUpdates = useCallback(() => {
    if (uiFlushTimerRef.current !== null) {
      globalThis.clearTimeout(uiFlushTimerRef.current);
      uiFlushTimerRef.current = null;
    }

    const pendingThumbUpdates = pendingThumbUpdatesRef.current;
    const finishedRenderingIds = pendingRenderedDoneRef.current;
    pendingThumbUpdatesRef.current = {};
    pendingRenderedDoneRef.current = new Set();

    if (Object.keys(pendingThumbUpdates).length > 0) {
      setThumbs((prev) => mergeThumbnailUpdates(prev, pendingThumbUpdates));
    }

    if (finishedRenderingIds.size > 0) {
      thumbnailStore.markRendering(finishedRenderingIds, false);
      setRenderingAssetIds((prev) => removeRenderingAssetIds(prev, finishedRenderingIds));
    }
  }, [setThumbs, thumbnailStore]);

  const scheduleQueuedFlush = useCallback(() => {
    if (uiFlushTimerRef.current !== null) {
      return;
    }

    uiFlushTimerRef.current = globalThis.setTimeout(() => {
      flushQueuedUpdates();
    }, UI_FLUSH_DELAY_MS);
  }, [flushQueuedUpdates]);

  const scheduleQueueProcessing = useCallback(() => {
    if (processTimerRef.current !== null) {
      return;
    }

    processTimerRef.current = globalThis.setTimeout(() => {
      processTimerRef.current = null;
      processQueueRef.current();
    }, QUEUE_PROCESS_DELAY_MS);
  }, []);

  const processQueuedBatches = useCallback(async () => {
    if (processRunningRef.current) {
      return;
    }

    processRunningRef.current = true;

    try {
      while (queuedIdsRef.current.size > 0) {
        const generation = generationRef.current;
        const batch = takeThumbnailBatch(queuedIdsRef.current, THUMBNAIL_BATCH_SIZE);
        if (!batch.length) {
          break;
        }

        for (const assetId of batch) {
          inFlightIdsRef.current.add(assetId);
        }

        updateQueueState();
        setRenderingAssetIds((prev) => {
          const next = { ...prev };
          let changed = false;

          for (const assetId of batch) {
            if (next[assetId]) {
              continue;
            }

            next[assetId] = true;
            changed = true;
          }

          return changed ? next : prev;
        });
        thumbnailStore.markRendering(batch, true);

        try {
          await ensureThumbnailsStream(generation, batch, [], (message) => {
            if (generation !== generationRef.current) return;
            if (message.event === "ready") {
              pendingThumbUpdatesRef.current[message.data.asset_id] = message.data.thumb_path;
              pendingRenderedDoneRef.current.add(message.data.asset_id);
              scheduleQueuedFlush();
            } else if (message.event === "failed") {
              failedRef.current.add(message.data.asset_id);
              pendingRenderedDoneRef.current.add(message.data.asset_id);
              scheduleQueuedFlush();
            }
          });
        } catch {
          if (generation !== generationRef.current) {
            continue;
          }

          for (const assetId of batch) {
            failedRef.current.add(assetId);
            pendingRenderedDoneRef.current.add(assetId);
          }

          scheduleQueuedFlush();
        } finally {
          for (const assetId of batch) {
            inFlightIdsRef.current.delete(assetId);
          }

          updateQueueState();
          flushQueuedUpdates();
        }
      }
    } finally {
      processRunningRef.current = false;
      updateQueueState();

      if (queuedIdsRef.current.size > 0) {
        scheduleQueueProcessing();
      }
    }
  }, [flushQueuedUpdates, scheduleQueueProcessing, scheduleQueuedFlush, thumbnailStore, updateQueueState]);

  useEffect(() => {
    processQueueRef.current = () => {
      void processQueuedBatches();
    };
  }, [processQueuedBatches]);

  useEffect(() => {
    return () => {
      if (uiFlushTimerRef.current !== null) {
        globalThis.clearTimeout(uiFlushTimerRef.current);
        uiFlushTimerRef.current = null;
      }

      if (processTimerRef.current !== null) {
        globalThis.clearTimeout(processTimerRef.current);
        processTimerRef.current = null;
      }
    };
  }, []);

  const queueThumbnailsByIds = useCallback(
    (assetIds: number[]) => {
      if (!assetIds.length) {
        return;
      }

      let added = false;

      for (const assetId of assetIds) {
        if (assetId <= 0) {
          continue;
        }

        if (thumbsRef.current[assetId]) {
          continue;
        }

        if (pendingThumbUpdatesRef.current[assetId]) {
          continue;
        }

        if (failedRef.current.has(assetId)) {
          continue;
        }

        if (queuedIdsRef.current.has(assetId) || inFlightIdsRef.current.has(assetId)) {
          continue;
        }

        queuedIdsRef.current.add(assetId);
        added = true;
      }

      if (!added) {
        return;
      }

      updateQueueState();
      scheduleQueueProcessing();
    },
    [scheduleQueueProcessing, updateQueueState]
  );

  const resetThumbnailQueue = useCallback(() => {
    if (uiFlushTimerRef.current !== null) {
      globalThis.clearTimeout(uiFlushTimerRef.current);
      uiFlushTimerRef.current = null;
    }

    if (processTimerRef.current !== null) {
      globalThis.clearTimeout(processTimerRef.current);
      processTimerRef.current = null;
    }

    generationRef.current += 1;
    failedRef.current.clear();

    pendingThumbUpdatesRef.current = {};
    pendingRenderedDoneRef.current = new Set();
    queuedIdsRef.current.clear();
    inFlightIdsRef.current.clear();

    setIsGeneratingPage(false);
    setPendingPageSize(0);
    setRenderingAssetIds({});
    thumbnailStore.clear();
  }, [thumbnailStore]);

  return {
    queueThumbnailsByIds,
    resetThumbnailQueue,
    isGeneratingPage,
    pendingPageSize,
    renderingAssetIds,
    thumbnailStore
  };
}
