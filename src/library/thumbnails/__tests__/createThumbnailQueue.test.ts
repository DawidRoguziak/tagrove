import { settle } from "../../../test/settle";
import { renderHook } from "@solidjs/testing-library";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ThumbnailStreamEvent } from "../../../platform/tauri/contracts/thumbnails";
import { createThumbnailQueue } from "../createThumbnailQueue";

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

vi.mock("../../../platform/tauri/thumbnails", async () => {
  const actual = await vi.importActual<typeof import("../../../platform/tauri/thumbnails")>(
    "../../../platform/tauri/thumbnails"
  );
  return {
    ...actual,
    ...apiMocks
  };
});

function createQueueHarness(initialThumbs: Record<number, string> = {}) {
  return renderHook(() => {
    const queue = createThumbnailQueue();
    queue.thumbnailStore.sync(initialThumbs);
    return queue;
  });
}

async function flushTimers(ms = 300) {
  await settle(async () => {
    vi.advanceTimersByTime(ms);
    await Promise.resolve();
  });
}

describe("createThumbnailQueue", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    apiMocks.ensureThumbnailsStream.mockReset();
    apiMocks.ensureThumbnailsStream.mockImplementation(
      async (
        _requestId: number,
        _visibleIds: number[],
        _prefetchIds: number[],
        onEvent: (event: ThumbnailStreamEvent) => void
      ) => onEvent({ event: "done", data: { ready: 0, failed: 0, stale: 0 } })
    );
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("deduplicates queued ids and ignores already-resolved or failed items", async () => {
    apiMocks.ensureThumbnailsStream.mockImplementationOnce(
      async (_requestId, _visible, _prefetch, onEvent) => {
        onEvent({ event: "failed", data: { asset_id: 2 } });
        onEvent({ event: "ready", data: { asset_id: 3, thumb_path: "thumb-3.jpg" } });
        onEvent({ event: "done", data: { ready: 1, failed: 1, stale: 0 } });
      }
    );

    const { result } = createQueueHarness({ 1: "thumb-1.jpg" });

    settle(() => {
      result.queueThumbnailsByIds([0, 1, 2, 2, 3]);
    });

    expect(result.pendingPageSize).toBe(2);
    await flushTimers(500);

    expect(apiMocks.ensureThumbnailsStream).toHaveBeenCalledTimes(1);
    expect(apiMocks.ensureThumbnailsStream.mock.calls[0]?.[1]).toEqual([2, 3]);
    expect(
      Object.fromEntries(
        Array.from({ length: 200 }, (_, index) => index + 1)
          .filter((id) => result.thumbnailStore.getPath(id))
          .map((id) => [id, result.thumbnailStore.getPath(id)])
      )
    ).toEqual({
      1: "thumb-1.jpg",
      3: "thumb-3.jpg"
    });
    expect(result.isGeneratingPage).toBe(false);
    expect(result.pendingPageSize).toBe(0);
    expect(
      Object.fromEntries(
        Array.from({ length: 200 }, (_, index) => index + 1)
          .filter((id) => result.thumbnailStore.isRendering(id))
          .map((id) => [id, true])
      )
    ).toEqual({});

    settle(() => {
      result.queueThumbnailsByIds([2, 3]);
    });
    await flushTimers(200);

    expect(apiMocks.ensureThumbnailsStream).toHaveBeenCalledTimes(1);
  });

  it("merges ready thumbnails from event stream and command result", async () => {
    const pending = deferred<void>();
    let onEvent: ((event: ThumbnailStreamEvent) => void) | undefined;
    apiMocks.ensureThumbnailsStream.mockImplementationOnce(
      async (_requestId, _visible, _prefetch, handler) => {
        onEvent = handler;
        await pending.promise;
      }
    );

    const { result } = createQueueHarness();

    settle(() => {
      result.queueThumbnailsByIds([10, 11]);
    });
    await flushTimers(60);

    expect(result.isGeneratingPage).toBe(true);
    expect(result.pendingPageSize).toBe(2);
    expect(
      Object.fromEntries(
        Array.from({ length: 200 }, (_, index) => index + 1)
          .filter((id) => result.thumbnailStore.isRendering(id))
          .map((id) => [id, true])
      )
    ).toEqual({
      10: true,
      11: true
    });

    settle(() => {
      onEvent?.({ event: "ready", data: { asset_id: 10, thumb_path: "event-10.jpg" } });
      onEvent?.({ event: "ready", data: { asset_id: 11, thumb_path: "ready-11.jpg" } });
    });
    pending.resolve();

    await flushTimers(300);

    expect(
      Object.fromEntries(
        Array.from({ length: 200 }, (_, index) => index + 1)
          .filter((id) => result.thumbnailStore.getPath(id))
          .map((id) => [id, result.thumbnailStore.getPath(id)])
      )
    ).toEqual({
      10: "event-10.jpg",
      11: "ready-11.jpg"
    });
    expect(result.isGeneratingPage).toBe(false);
    expect(result.pendingPageSize).toBe(0);
    expect(
      Object.fromEntries(
        Array.from({ length: 200 }, (_, index) => index + 1)
          .filter((id) => result.thumbnailStore.isRendering(id))
          .map((id) => [id, true])
      )
    ).toEqual({});
  });

  it("ignores stale results after queue reset bumps generation", async () => {
    const pending = deferred<void>();
    let onEvent: ((event: ThumbnailStreamEvent) => void) | undefined;
    apiMocks.ensureThumbnailsStream.mockImplementationOnce(
      async (_requestId, _visible, _prefetch, handler) => {
        onEvent = handler;
        await pending.promise;
      }
    );

    const { result } = createQueueHarness();

    settle(() => {
      result.queueThumbnailsByIds([33]);
    });
    await flushTimers(60);

    settle(() => {
      result.resetThumbnailQueue();
    });

    settle(() => {
      onEvent?.({ event: "ready", data: { asset_id: 33, thumb_path: "stale-event.jpg" } });
    });
    pending.resolve();
    await flushTimers(400);

    expect(
      Object.fromEntries(
        Array.from({ length: 200 }, (_, index) => index + 1)
          .filter((id) => result.thumbnailStore.getPath(id))
          .map((id) => [id, result.thumbnailStore.getPath(id)])
      )
    ).toEqual({});
    expect(result.isGeneratingPage).toBe(false);
    expect(result.pendingPageSize).toBe(0);
    expect(
      Object.fromEntries(
        Array.from({ length: 200 }, (_, index) => index + 1)
          .filter((id) => result.thumbnailStore.isRendering(id))
          .map((id) => [id, true])
      )
    ).toEqual({});
  });

  it("splits large queues into configured batches", async () => {
    apiMocks.ensureThumbnailsStream.mockImplementation(
      async (_requestId, assetIds, _prefetch, onEvent) => {
        for (const assetId of assetIds) {
          onEvent({
            event: "ready",
            data: { asset_id: assetId, thumb_path: `thumb-${assetId}.jpg` }
          });
        }
        onEvent({ event: "done", data: { ready: assetIds.length, failed: 0 } });
      }
    );

    const { result } = createQueueHarness();
    const allIds = Array.from({ length: 181 }, (_, index) => index + 1);

    settle(() => {
      result.queueThumbnailsByIds(allIds);
    });
    await flushTimers(1000);

    expect(apiMocks.ensureThumbnailsStream).toHaveBeenCalledTimes(3);
    expect(apiMocks.ensureThumbnailsStream.mock.calls[0]?.[1]).toHaveLength(64);
    expect(apiMocks.ensureThumbnailsStream.mock.calls[1]?.[1]).toHaveLength(64);
    expect(apiMocks.ensureThumbnailsStream.mock.calls[2]?.[1]).toHaveLength(53);
    expect(
      Object.keys(
        Object.fromEntries(
          Array.from({ length: 200 }, (_, index) => index + 1)
            .filter((id) => result.thumbnailStore.getPath(id))
            .map((id) => [id, result.thumbnailStore.getPath(id)])
        )
      )
    ).toHaveLength(181);
  });
  it("replaces abandoned gallery demand while preserving additive consumers", async () => {
    const { result } = createQueueHarness();
    settle(() => {
      result.setGalleryThumbnailDemand([1, 2], [3, 4]);
      result.queueThumbnailsByIds([4, 9]);
      result.setGalleryThumbnailDemand([5, 6], [7, 8]);
    });
    expect(result.pendingPageSize).toBe(6);
    await flushTimers();
    expect(apiMocks.ensureThumbnailsStream.mock.calls[0]?.slice(1, 3)).toEqual([
      [5, 6, 4, 9],
      [7, 8]
    ]);
  });

  it("prioritizes the latest viewport after the current batch finishes", async () => {
    const pending = deferred<void>();
    apiMocks.ensureThumbnailsStream.mockImplementationOnce(() => pending.promise);
    const { result } = createQueueHarness();
    settle(() => result.setGalleryThumbnailDemand([1], [2]));
    await flushTimers(60);
    settle(() => {
      result.setGalleryThumbnailDemand([3], [4]);
      result.setGalleryThumbnailDemand([5], [6]);
    });
    pending.resolve();
    await flushTimers();
    expect(apiMocks.ensureThumbnailsStream.mock.calls.map((call) => call.slice(1, 3))).toEqual([
      [[1], [2]],
      [[5], [6]]
    ]);
  });

  it("publishes a completed path and spinner state in a single tile notification", async () => {
    const pending = deferred<void>();
    let emit: ((event: ThumbnailStreamEvent) => void) | undefined;
    apiMocks.ensureThumbnailsStream.mockImplementationOnce((_id, _visible, _prefetch, callback) => {
      emit = callback;
      return pending.promise;
    });
    const { result } = createQueueHarness();
    settle(() => result.setGalleryThumbnailDemand([1], []));
    await flushTimers(60);
    const observed: unknown[] = [];
    const unsubscribe = result.thumbnailStore.subscribe(1, () =>
      observed.push({
        path: result.thumbnailStore.getPath(1),
        rendering: result.thumbnailStore.isRendering(1)
      })
    );
    settle(() => emit?.({ event: "ready", data: { asset_id: 1, thumb_path: "1.jpg" } }));
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
    settle(() => result.setGalleryThumbnailDemand([1], []));
    await flushTimers(60);
    settle(() => {
      result.resetThumbnailQueue();
      result.setGalleryThumbnailDemand([1], [2]);
    });
    pending.resolve();
    await flushTimers();
    expect(apiMocks.ensureThumbnailsStream.mock.calls[1]?.slice(0, 3)).toEqual([1, [1], [2]]);
    expect(
      Object.fromEntries(
        Array.from({ length: 200 }, (_, index) => index + 1)
          .filter((id) => result.thumbnailStore.isRendering(id))
          .map((id) => [id, true])
      )
    ).toEqual({});
  });

  it("does not dispatch queued work after unmount", async () => {
    const pending = deferred<void>();
    apiMocks.ensureThumbnailsStream.mockImplementationOnce(() => pending.promise);
    const { result, cleanup: unmount } = createQueueHarness();
    settle(() => result.setGalleryThumbnailDemand([1], []));
    await flushTimers(60);
    settle(() => result.setGalleryThumbnailDemand([2], []));
    unmount();
    pending.resolve();
    await flushTimers();
    expect(apiMocks.ensureThumbnailsStream).toHaveBeenCalledTimes(1);
  });

  it("allows stale source results to be requested again", async () => {
    apiMocks.ensureThumbnailsStream.mockImplementationOnce(
      async (_generation, _visible, _prefetch, send) => {
        send({ event: "stale", data: { asset_id: 7 } });
        send({ event: "done", data: { ready: 0, failed: 0, stale: 1 } });
      }
    );
    const { result } = createQueueHarness();
    settle(() => result.queueThumbnailsByIds([7]));
    await flushTimers(500);
    expect(
      Object.fromEntries(
        Array.from({ length: 200 }, (_, index) => index + 1)
          .filter((id) => result.thumbnailStore.getPath(id))
          .map((id) => [id, result.thumbnailStore.getPath(id)])
      )[7]
    ).toBeUndefined();
    settle(() => result.queueThumbnailsByIds([7]));
    await flushTimers(500);
    expect(apiMocks.ensureThumbnailsStream).toHaveBeenCalledTimes(2);
  });
});
