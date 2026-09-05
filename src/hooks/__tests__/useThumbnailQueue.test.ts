import { act, renderHook } from "@testing-library/react";
import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ThumbnailStreamEvent } from "../../types";
import { useThumbnailQueue } from "../useThumbnailQueue";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

const apiMocks = vi.hoisted(() => ({
  ensureThumbnailsStream: vi.fn()
}));

vi.mock("../../api", async () => {
  const actual = await vi.importActual<typeof import("../../api")>("../../api");
  return {
    ...actual,
    ...apiMocks
  };
});

function createQueueHarness(initialThumbs: Record<number, string> = {}) {
  return renderHook(() => {
    const [thumbs, setThumbs] = useState<Record<number, string>>(initialThumbs);
    const queue = useThumbnailQueue({ thumbs, setThumbs });
    return {
      thumbs,
      ...queue
    };
  });
}

async function flushTimers(ms = 300) {
  await act(async () => {
    vi.advanceTimersByTime(ms);
    await Promise.resolve();
  });
}

describe("useThumbnailQueue", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    apiMocks.ensureThumbnailsStream.mockReset();
    apiMocks.ensureThumbnailsStream.mockImplementation(async (
      _requestId: number,
      _visibleIds: number[],
      _prefetchIds: number[],
      onEvent: (event: ThumbnailStreamEvent) => void
    ) => onEvent({ event: "done", data: { ready: 0, failed: 0 } }));
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("deduplicates queued ids and ignores already-resolved or failed items", async () => {
    apiMocks.ensureThumbnailsStream.mockImplementationOnce(async (_requestId, _visible, _prefetch, onEvent) => {
      onEvent({ event: "failed", data: { asset_id: 2 } });
      onEvent({ event: "ready", data: { asset_id: 3, thumb_path: "thumb-3.jpg" } });
      onEvent({ event: "done", data: { ready: 1, failed: 1 } });
    });

    const { result } = createQueueHarness({ 1: "thumb-1.jpg" });

    act(() => {
      result.current.queueThumbnailsByIds([0, 1, 2, 2, 3]);
    });

    expect(result.current.pendingPageSize).toBe(2);
    await flushTimers(500);

    expect(apiMocks.ensureThumbnailsStream).toHaveBeenCalledTimes(1);
    expect(apiMocks.ensureThumbnailsStream.mock.calls[0]?.[1]).toEqual([2, 3]);
    expect(result.current.thumbs).toEqual({
      1: "thumb-1.jpg",
      3: "thumb-3.jpg"
    });
    expect(result.current.isGeneratingPage).toBe(false);
    expect(result.current.pendingPageSize).toBe(0);
    expect(result.current.renderingAssetIds).toEqual({});

    act(() => {
      result.current.queueThumbnailsByIds([2, 3]);
    });
    await flushTimers(200);

    expect(apiMocks.ensureThumbnailsStream).toHaveBeenCalledTimes(1);
  });

  it("merges ready thumbnails from event stream and command result", async () => {
    const pending = deferred<void>();
    let onEvent: ((event: ThumbnailStreamEvent) => void) | undefined;
    apiMocks.ensureThumbnailsStream.mockImplementationOnce(async (_requestId, _visible, _prefetch, handler) => {
      onEvent = handler;
      await pending.promise;
    });

    const { result } = createQueueHarness();

    act(() => {
      result.current.queueThumbnailsByIds([10, 11]);
    });
    await flushTimers(60);

    expect(result.current.isGeneratingPage).toBe(true);
    expect(result.current.pendingPageSize).toBe(2);
    expect(result.current.renderingAssetIds).toEqual({
      10: true,
      11: true
    });

    act(() => {
      onEvent?.({ event: "ready", data: { asset_id: 10, thumb_path: "event-10.jpg" } });
      onEvent?.({ event: "ready", data: { asset_id: 11, thumb_path: "ready-11.jpg" } });
    });
    pending.resolve();

    await flushTimers(300);

    expect(result.current.thumbs).toEqual({
      10: "event-10.jpg",
      11: "ready-11.jpg"
    });
    expect(result.current.isGeneratingPage).toBe(false);
    expect(result.current.pendingPageSize).toBe(0);
    expect(result.current.renderingAssetIds).toEqual({});
  });

  it("ignores stale results after queue reset bumps generation", async () => {
    const pending = deferred<void>();
    let onEvent: ((event: ThumbnailStreamEvent) => void) | undefined;
    apiMocks.ensureThumbnailsStream.mockImplementationOnce(async (_requestId, _visible, _prefetch, handler) => {
      onEvent = handler;
      await pending.promise;
    });

    const { result } = createQueueHarness();

    act(() => {
      result.current.queueThumbnailsByIds([33]);
    });
    await flushTimers(60);

    act(() => {
      result.current.resetThumbnailQueue();
    });

    act(() => {
      onEvent?.({ event: "ready", data: { asset_id: 33, thumb_path: "stale-event.jpg" } });
    });
    pending.resolve();
    await flushTimers(400);

    expect(result.current.thumbs).toEqual({});
    expect(result.current.isGeneratingPage).toBe(false);
    expect(result.current.pendingPageSize).toBe(0);
    expect(result.current.renderingAssetIds).toEqual({});
  });

  it("splits large queues into configured batches", async () => {
    apiMocks.ensureThumbnailsStream.mockImplementation(async (_requestId, assetIds, _prefetch, onEvent) => {
      for (const assetId of assetIds) {
        onEvent({ event: "ready", data: { asset_id: assetId, thumb_path: `thumb-${assetId}.jpg` } });
      }
      onEvent({ event: "done", data: { ready: assetIds.length, failed: 0 } });
    });

    const { result } = createQueueHarness();
    const allIds = Array.from({ length: 181 }, (_, index) => index + 1);

    act(() => {
      result.current.queueThumbnailsByIds(allIds);
    });
    await flushTimers(1000);

    expect(apiMocks.ensureThumbnailsStream).toHaveBeenCalledTimes(3);
    expect(apiMocks.ensureThumbnailsStream.mock.calls[0]?.[1]).toHaveLength(64);
    expect(apiMocks.ensureThumbnailsStream.mock.calls[1]?.[1]).toHaveLength(64);
    expect(apiMocks.ensureThumbnailsStream.mock.calls[2]?.[1]).toHaveLength(53);
    expect(Object.keys(result.current.thumbs)).toHaveLength(181);
  });
  it("replaces abandoned gallery demand while preserving additive consumers", async () => {
    const { result } = createQueueHarness();
    act(() => {
      result.current.setGalleryThumbnailDemand([1, 2], [3, 4]);
      result.current.queueThumbnailsByIds([4, 9]);
      result.current.setGalleryThumbnailDemand([5, 6], [7, 8]);
    });
    expect(result.current.pendingPageSize).toBe(6);
    await flushTimers();
    expect(apiMocks.ensureThumbnailsStream.mock.calls[0]?.slice(1, 3)).toEqual([[5, 6, 4, 9], [7, 8]]);
  });

  it("prioritizes the latest viewport after the current batch finishes", async () => {
    const pending = deferred<void>();
    apiMocks.ensureThumbnailsStream.mockImplementationOnce(() => pending.promise);
    const { result } = createQueueHarness();
    act(() => result.current.setGalleryThumbnailDemand([1], [2]));
    await flushTimers(60);
    act(() => {
      result.current.setGalleryThumbnailDemand([3], [4]);
      result.current.setGalleryThumbnailDemand([5], [6]);
    });
    pending.resolve();
    await flushTimers();
    expect(apiMocks.ensureThumbnailsStream.mock.calls.map((call) => call.slice(1, 3)))
      .toEqual([[[1], [2]], [[5], [6]]]);
  });

  it("publishes a completed path and spinner state in a single tile notification", async () => {
    const pending = deferred<void>();
    let emit: ((event: ThumbnailStreamEvent) => void) | undefined;
    apiMocks.ensureThumbnailsStream.mockImplementationOnce((_id, _visible, _prefetch, callback) => {
      emit = callback;
      return pending.promise;
    });
    const { result } = createQueueHarness();
    act(() => result.current.setGalleryThumbnailDemand([1], []));
    await flushTimers(60);
    const observed: unknown[] = [];
    const unsubscribe = result.current.thumbnailStore.subscribe(1, () => observed.push({
      path: result.current.thumbnailStore.getPath(1),
      rendering: result.current.thumbnailStore.isRendering(1)
    }));
    act(() => emit?.({ event: "ready", data: { asset_id: 1, thumb_path: "1.jpg" } }));
    await flushTimers(30);
    expect(observed).toEqual([{ path: "1.jpg", rendering: false }]);
    unsubscribe();
    pending.resolve();
    await flushTimers();
  });

  it("keeps new demand after reset while an obsolete invocation settles", async () => {
    const pending = deferred<void>();
    apiMocks.ensureThumbnailsStream.mockImplementationOnce(() => pending.promise);
    const { result } = createQueueHarness();
    act(() => result.current.setGalleryThumbnailDemand([1], []));
    await flushTimers(60);
    act(() => {
      result.current.resetThumbnailQueue();
      result.current.setGalleryThumbnailDemand([1], [2]);
    });
    pending.resolve();
    await flushTimers();
    expect(apiMocks.ensureThumbnailsStream.mock.calls[1]?.slice(0, 3)).toEqual([1, [1], [2]]);
    expect(result.current.renderingAssetIds).toEqual({});
  });

  it("does not dispatch queued work after unmount", async () => {
    const pending = deferred<void>();
    apiMocks.ensureThumbnailsStream.mockImplementationOnce(() => pending.promise);
    const { result, unmount } = createQueueHarness();
    act(() => result.current.setGalleryThumbnailDemand([1], []));
    await flushTimers(60);
    act(() => result.current.setGalleryThumbnailDemand([2], []));
    unmount();
    pending.resolve();
    await flushTimers();
    expect(apiMocks.ensureThumbnailsStream).toHaveBeenCalledTimes(1);
  });

});
