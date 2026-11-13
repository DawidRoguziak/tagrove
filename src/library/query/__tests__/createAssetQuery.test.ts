import { settle } from "../../../test/settle";
import { renderHook, waitFor } from "@solidjs/testing-library";
import { createEffect, flush } from "solid-js";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AssetSummary } from "../../../platform/tauri/contracts/assets";
import { idRangeResult } from "../../../test/idRange";
import {
  createAssetQuery,
  ID_RANGE_CHUNK,
  MAX_CACHED_PAGES,
  STALE_REFRESH_DELAY_MS,
  STALE_REFRESH_MIN_INTERVAL_MS
} from "../createAssetQuery";

const apiMocks = vi.hoisted(() => ({
  startAssetQuery: vi.fn(),
  getAssetQueryPage: vi.fn(),
  getAssetQueryIdRange: vi.fn(),
  getAssetQuerySections: vi.fn()
}));

vi.mock("../../../platform/tauri/assets", () => apiMocks);

function createSummary(id: number): AssetSummary {
  return {
    id,
    file_name: `photo-${id}.jpg`,
    preview_path: null,
    kind: "image",
    modified_at: 100 - id,
    width: 10,
    height: 10,
    duration_ms: null,
    thumb_version: "0123456789abcdef",
    is_favorite: false,
    media_group_key: null,
    media_group_order: null
  };
}

function readyStart(items: AssetSummary[], total = items.length, sessionId = 7) {
  return {
    status: "ready" as const,
    session_id: sessionId,
    revision: 3,
    total,
    offset: 0,
    items
  };
}

const PAGE_SIZE = 2;

async function renderAssets() {
  return renderHook(() =>
    createAssetQuery({
      pageSize: PAGE_SIZE,
      filters: () => ({
        filterInclude: [],
        filterExclude: [],
        metaFilter: null,
        appliedMediaKind: "all",
        appliedFavoritesOnly: false
      })
    })
  );
}

describe("createAssetQuery", () => {
  beforeEach(() => {
    apiMocks.startAssetQuery.mockReset();
    apiMocks.getAssetQueryPage.mockReset();
    apiMocks.getAssetQueryIdRange.mockReset();
    apiMocks.getAssetQuerySections.mockReset();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("replaces the cache with the ready first page of the current generation", async () => {
    apiMocks.startAssetQuery
      .mockResolvedValueOnce(readyStart([createSummary(1), createSummary(2)]))
      .mockResolvedValueOnce(readyStart([createSummary(9)], 1, 8));
    const { result } = await renderAssets();

    await settle(async () => {
      await result.refresh();
    });

    expect(result.total).toBe(2);
    expect(result.loadError).toBeNull();
    expect(result.getAssetAt(0)?.id).toBe(1);
    expect(apiMocks.startAssetQuery).toHaveBeenCalledWith(
      expect.objectContaining({ generation: 1, pageSize: PAGE_SIZE })
    );

    await settle(async () => {
      await result.refresh();
    });
    expect(result.assets.map((asset) => asset.id)).toEqual([9]);
  });

  it("loads timeline sections once per session and drops them for a new session", async () => {
    const sections = [
      { t: 202403, start: 0, count: 1 },
      { t: 0, start: 1, count: 1 }
    ];
    apiMocks.startAssetQuery
      .mockResolvedValueOnce(readyStart([createSummary(1), createSummary(2)]))
      .mockResolvedValueOnce(readyStart([createSummary(1), createSummary(2)]))
      .mockResolvedValueOnce(readyStart([createSummary(9)], 1, 8));
    apiMocks.getAssetQuerySections
      .mockResolvedValueOnce({ status: "ready", session_id: 7, total: 2, sections })
      .mockResolvedValueOnce({ status: "stale" });
    const { result } = await renderAssets();
    await settle(async () => {
      await result.refresh();
    });
    await waitFor(() => expect(result.sections).toEqual(sections));
    expect(apiMocks.getAssetQuerySections).toHaveBeenCalledWith(7);
    // The same session (a cache hit) keeps its sections without another read.
    await settle(async () => {
      await result.refresh();
    });
    expect(apiMocks.getAssetQuerySections).toHaveBeenCalledTimes(1);
    expect(result.sections).toEqual(sections);
    expect(result.sessionId()).toBe(7);
    // A new session clears them; a stale read leaves the timeline empty.
    await settle(async () => {
      await result.refresh();
    });
    await waitFor(() => expect(apiMocks.getAssetQuerySections).toHaveBeenCalledTimes(2));
    expect(apiMocks.getAssetQuerySections).toHaveBeenLastCalledWith(8);
    expect(result.sections).toEqual([]);
    expect(result.sessionId()).toBe(8);
  });

  it("keeps the session and hides the timeline when ready sections are unavailable", async () => {
    // A scan batch committed without a revision bump (ADR 0027): the backend
    // keeps the session and returns `ready` with no sections.
    vi.useFakeTimers();
    apiMocks.startAssetQuery.mockResolvedValueOnce(readyStart([createSummary(1), createSummary(2)], 4, 7));
    apiMocks.getAssetQuerySections.mockResolvedValueOnce({
      status: "ready",
      session_id: 7,
      total: 4,
      sections: []
    });
    apiMocks.getAssetQueryPage.mockResolvedValueOnce({
      ...readyStart([createSummary(3), createSummary(4)], 4, 7),
      offset: 2
    });
    const { result } = await renderAssets();
    await settle(async () => {
      await result.refresh();
    });
    await vi.advanceTimersByTimeAsync(STALE_REFRESH_DELAY_MS + STALE_REFRESH_MIN_INTERVAL_MS);
    expect(apiMocks.getAssetQuerySections).toHaveBeenCalledTimes(1);
    expect(apiMocks.startAssetQuery).toHaveBeenCalledTimes(1);
    expect(result.sections).toEqual([]);
    expect(result.sessionId()).toBe(7);
    await settle(async () => {
      expect((await result.getAssetAtAsync(3))?.id).toBe(4);
    });
    expect(apiMocks.getAssetQueryPage).toHaveBeenCalledWith(7, 2, PAGE_SIZE);
  });

  it("exposes current supersession for retry while keeping the previous cache visible", async () => {
    apiMocks.startAssetQuery.mockResolvedValueOnce(readyStart([createSummary(1)], 1, 7));
    const { result } = await renderAssets();
    await settle(async () => {
      await result.refresh();
    });

    apiMocks.startAssetQuery.mockResolvedValueOnce({ status: "superseded" });
    await settle(async () => {
      await expect(result.refresh()).rejects.toThrow("superseded");
    });

    expect(result.total).toBe(1);
    expect(result.getAssetAt(0)?.id).toBe(1);
    expect(result.loading).toBe(false);
    expect(result.loadError).toContain("superseded");
    apiMocks.startAssetQuery.mockResolvedValueOnce(readyStart([createSummary(9)]));
    await settle(() => result.retryLoad());
    expect(result.loadError).toBeNull();
    expect(result.getAssetAt(0)?.id).toBe(9);
  });

  it.each([true, false])("keeps the newest filter with reversed admission, newest reply first: %s", async (newestFirst) => {
    let completeOld!: (value: object) => void;
    let completeNew!: (value: object) => void;
    apiMocks.startAssetQuery
      .mockImplementationOnce(() => new Promise(resolve => { completeOld = resolve; }))
      .mockImplementationOnce(() => new Promise(resolve => { completeNew = resolve; }));
    let included = ["old"];
    const { result } = renderHook(() => createAssetQuery({
      pageSize: PAGE_SIZE,
      filters: () => ({ filterInclude: included, filterExclude: [], appliedMediaKind: "all", appliedFavoritesOnly: false })
    }));
    const old = result.refresh();
    included = ["new"];
    const newest = result.refresh();
    const finishOld = async () => { completeOld({ status: "superseded" }); await old; };
    const finishNew = async () => { completeNew(readyStart([createSummary(9)])); await newest; };
    await settle(newestFirst ? finishNew : finishOld);
    await settle(newestFirst ? finishOld : finishNew);
    expect(result.assets.map(asset => asset.id)).toEqual([9]);
    expect(result.loadError).toBeNull();
    const [first, second] = apiMocks.startAssetQuery.mock.calls.map(([params]) => params);
    expect(first.ownerId).toBe(second.ownerId);
    expect(first.generation).toBeLessThan(second.generation);
    expect(second.tagsAnd).toEqual(["new"]);
    expect(result.getAssetIndex(9)).toBe(0);
  });

  it("gives a remounted query a new owner even though generations restart", async () => {
    apiMocks.startAssetQuery.mockResolvedValue(readyStart([]));
    const first = await renderAssets();
    await settle(() => first.result.refresh());
    await settle(() => first.result.refresh());
    first.cleanup();
    const replacement = await renderAssets();
    await settle(() => replacement.result.refresh());
    const calls = apiMocks.startAssetQuery.mock.calls.map(([params]) => params);
    expect(calls[2].generation).toBe(1);
    expect(calls[2].ownerId).toEqual(expect.any(String));
    expect(calls[2].ownerId).not.toBe(calls[0].ownerId);
  });

  it("records a start failure in loadError and recovers through retryLoad", async () => {
    apiMocks.startAssetQuery
      .mockRejectedValueOnce(new Error("database is locked"))
      .mockResolvedValueOnce(
        readyStart([createSummary(1), createSummary(2), createSummary(3)], 3, 9)
      );
    const { result } = await renderAssets();

    await settle(async () => {
      await expect(result.refresh()).rejects.toThrow("database is locked");
    });
    expect(result.loadError).toBe("database is locked");
    // The selection reads this pair as a lost session (no session after a failure).
    expect(result.sessionId()).toBeNull();
    expect(result.pageFailureEpoch).toBe(1);

    await settle(async () => {
      await result.retryLoad();
    });
    expect(result.loadError).toBeNull();
    expect(result.total).toBe(3);
    expect(result.getAssetAt(0)?.id).toBe(1);
  });

  it("keeps a failed page retryable and clears loadError once it loads", async () => {
    apiMocks.startAssetQuery.mockResolvedValue(
      readyStart([createSummary(1), createSummary(2)], 4, 5)
    );
    const { result } = await renderAssets();
    await settle(async () => {
      await result.refresh();
    });

    apiMocks.getAssetQueryPage.mockRejectedValueOnce(new Error("page worker failed"));
    await settle(async () => {
      await expect(result.getAssetAtAsync(PAGE_SIZE)).rejects.toThrow("page worker failed");
    });
    expect(result.loadError).toBe("page worker failed");
    expect(result.pageFailureEpoch).toBeGreaterThan(0);
    // A page failure keeps the session.
    expect(result.sessionId()).toBe(5);
    expect(result.getAssetAt(PAGE_SIZE)).toBeUndefined();

    apiMocks.getAssetQueryPage.mockResolvedValueOnce({
      status: "ready",
      session_id: 5,
      revision: 3,
      total: 4,
      offset: PAGE_SIZE,
      items: [createSummary(3), createSummary(4)]
    });
    await settle(async () => {
      result.ensureRange(PAGE_SIZE, PAGE_SIZE + 1);
      await result.retryLoad();
    });
    expect(result.loadError).toBeNull();
    expect(apiMocks.getAssetQueryPage).toHaveBeenNthCalledWith(2, 5, PAGE_SIZE, PAGE_SIZE);
  });

  it("restarts the query after the stale debounce when a page reports stale", async () => {
    apiMocks.startAssetQuery
      .mockResolvedValueOnce(readyStart([createSummary(1)], 4, 5))
      .mockResolvedValueOnce(readyStart([createSummary(9)], 1, 6));
    const { result } = await renderAssets();
    await settle(async () => {
      await result.refresh();
    });

    apiMocks.getAssetQueryPage.mockResolvedValueOnce({ status: "stale" });
    await settle(async () => {
      // A stale page resolves as a hole while a fresh query replaces the view.
      const asset = await result.getAssetAtAsync(PAGE_SIZE);
      expect(asset).toBeUndefined();
    });

    await waitFor(() => expect(result.total).toBe(1));
    expect(apiMocks.startAssetQuery).toHaveBeenCalledTimes(2);
  });

  it("stores summaries verbatim without fabricating path, size, or tags", async () => {
    apiMocks.startAssetQuery.mockResolvedValue(readyStart([createSummary(1)]));
    const { result } = await renderAssets();
    await settle(async () => {
      await result.refresh();
    });

    const stored = result.getAssetAt(0);
    expect(stored).toEqual(createSummary(1));
    expect(stored).not.toHaveProperty("tags");
    expect(stored?.preview_path).toBeNull();
  });

  it("invalidates queryEpoch when refresh starts even if the backend supersedes it", async () => {
    apiMocks.startAssetQuery.mockResolvedValue(readyStart([createSummary(1)], 1, 7));
    const { result } = await renderAssets();
    expect(result.queryEpoch).toBe(0);

    await settle(async () => {
      await result.refresh();
    });
    const afterFirst = result.queryEpoch;
    expect(afterFirst).toBeGreaterThan(0);

    apiMocks.startAssetQuery.mockResolvedValueOnce({ status: "superseded" });
    await settle(async () => {
      await expect(result.refresh()).rejects.toThrow("superseded");
    });
    expect(result.queryEpoch).toBe(afterFirst + 1);
    expect(result.queryPending).toBe(false);
    expect(result.getAssetIndex(1)).toBeNull();
  });

  it("reads an ID range from the session vector in one call without touching pages", async () => {
    apiMocks.startAssetQuery.mockResolvedValue(
      readyStart([createSummary(1), createSummary(2)], 4, 5)
    );
    const { result } = await renderAssets();
    await settle(async () => {
      await result.refresh();
    });
    apiMocks.getAssetQueryIdRange.mockResolvedValueOnce(idRangeResult([2, 3, 4]));

    let ids: ArrayLike<number> = [];
    await settle(async () => {
      // Reversed and over-long bounds are normalized to the snapshot.
      ids = await result.getIdsRangeAsync(9, 1);
    });
    expect(Array.from(ids)).toEqual([2, 3, 4]);
    expect(apiMocks.getAssetQueryIdRange).toHaveBeenCalledExactlyOnceWith(5, 1, 3);
    expect(apiMocks.getAssetQueryPage).not.toHaveBeenCalled();
    expect(result.getAssetAt(2)).toBeUndefined();
    expect(result.assets.map((asset) => asset.id)).toEqual([1, 2]);
  });

  it("returns an empty range without IPC when the bounds miss the snapshot", async () => {
    apiMocks.startAssetQuery.mockResolvedValue(readyStart([createSummary(1)], 1, 5));
    const { result } = await renderAssets();
    await settle(async () => {
      await result.refresh();
    });
    expect(Array.from(await result.getIdsRangeAsync(3, 8))).toEqual([]);
    expect(apiMocks.getAssetQueryIdRange).not.toHaveBeenCalled();
  });

  it("rejects the whole ID range when the range read fails or is incomplete", async () => {
    apiMocks.startAssetQuery.mockResolvedValue(
      readyStart([createSummary(1), createSummary(2)], 4, 5)
    );
    const { result } = await renderAssets();
    await settle(async () => {
      await result.refresh();
    });

    apiMocks.getAssetQueryIdRange
      .mockRejectedValueOnce(new Error("range worker failed"))
      .mockResolvedValueOnce(idRangeResult([2]));
    await settle(async () => {
      await expect(result.getIdsRangeAsync(1, 2)).rejects.toThrow("range worker failed");
      await expect(result.getIdsRangeAsync(1, 2)).rejects.toThrow("incomplete");
    });
    // A range failure belongs to its caller, not to the gallery page state.
    expect(result.loadError).toBeNull();
  });

  it("rejects an ID range of a stale session and schedules a refresh", async () => {
    vi.useFakeTimers();
    apiMocks.startAssetQuery
      .mockResolvedValueOnce(readyStart([createSummary(1), createSummary(2)], 4, 5))
      .mockResolvedValueOnce(readyStart([createSummary(9)], 1, 6));
    const { result } = await renderAssets();
    await settle(async () => {
      await result.refresh();
    });
    apiMocks.getAssetQueryIdRange.mockResolvedValueOnce({ status: "stale" });

    await settle(async () => {
      await expect(result.getIdsRangeAsync(1, 2)).rejects.toThrow("cancelled");
    });
    await vi.advanceTimersByTimeAsync(STALE_REFRESH_DELAY_MS);
    expect(apiMocks.startAssetQuery).toHaveBeenCalledTimes(2);
  });

  it("keeps the current session alive with a one-ID read and refreshes when it is stale", async () => {
    vi.useFakeTimers();
    apiMocks.startAssetQuery
      .mockResolvedValueOnce(readyStart([createSummary(1), createSummary(2)], 4, 5))
      .mockResolvedValueOnce(readyStart([createSummary(9)], 1, 6));
    apiMocks.getAssetQuerySections.mockResolvedValue({
      status: "ready",
      session_id: 5,
      total: 4,
      sections: []
    });
    const { result } = await renderAssets();
    await settle(async () => {
      await result.refresh();
    });
    apiMocks.getAssetQueryIdRange.mockResolvedValueOnce(idRangeResult([1]));
    await expect(result.keepSessionAlive(5)).resolves.toBe("ready");
    expect(apiMocks.getAssetQueryIdRange).toHaveBeenCalledExactlyOnceWith(5, 0, 0);
    // A session that is not the current one is left to expire.
    await expect(result.keepSessionAlive(4)).resolves.toBe("skipped");
    expect(apiMocks.getAssetQueryIdRange).toHaveBeenCalledOnce();
    await vi.advanceTimersByTimeAsync(STALE_REFRESH_DELAY_MS * 2);
    expect(apiMocks.startAssetQuery).toHaveBeenCalledOnce();
    expect(result.loadError).toBeNull();

    apiMocks.getAssetQueryIdRange.mockResolvedValueOnce({ status: "stale" });
    await expect(result.keepSessionAlive(5)).resolves.toBe("stale");
    await vi.advanceTimersByTimeAsync(STALE_REFRESH_DELAY_MS);
    flush();
    expect(apiMocks.startAssetQuery).toHaveBeenCalledTimes(2);
    expect(result.sessionId()).toBe(6);
  });

  it("splits huge ranges into chunks and assembles them in order", async () => {
    const total = ID_RANGE_CHUNK + 5;
    apiMocks.startAssetQuery.mockResolvedValue(readyStart([createSummary(1)], total, 5));
    apiMocks.getAssetQueryIdRange.mockImplementation(async (_session, start: number, end: number) =>
      idRangeResult(Float64Array.from({ length: end - start + 1 }, (_, i) => start + i + 1))
    );
    const { result } = await renderAssets();
    await settle(async () => {
      await result.refresh();
    });
    let ids: ArrayLike<number> = [];
    await settle(async () => {
      ids = await result.getIdsRangeAsync(0, total - 1);
    });
    expect(apiMocks.getAssetQueryIdRange.mock.calls.map(([, start, end]) => [start, end])).toEqual([
      [0, ID_RANGE_CHUNK - 1],
      [ID_RANGE_CHUNK, total - 1]
    ]);
    expect(ids.length).toBe(total);
    expect([ids[0], ids[ID_RANGE_CHUNK], ids[total - 1]]).toEqual([1, ID_RANGE_CHUNK + 1, total]);
  });

  it("drops an obsolete page response captured before a refresh", async () => {
    let resolvePage!: (value: unknown) => void;
    apiMocks.startAssetQuery.mockResolvedValue(readyStart([createSummary(1)], 4, 5));
    const { result } = await renderAssets();
    await settle(async () => {
      await result.refresh();
    });

    apiMocks.getAssetQueryPage.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolvePage = resolve;
        })
    );
    let pagePromise!: Promise<unknown>;
    settle(() => {
      pagePromise = result.getAssetAtAsync(PAGE_SIZE);
    });

    await settle(async () => {
      await result.refresh();
    });
    await settle(async () => {
      resolvePage({
        status: "ready",
        session_id: 5,
        revision: 3,
        total: 4,
        offset: PAGE_SIZE,
        items: [createSummary(8)]
      });
      await pagePromise;
    });

    // The late response belongs to the previous generation and must not merge.
    expect(result.getAssetAt(PAGE_SIZE)?.id).not.toBe(8);
  });
  it("retains recently accessed and active pages while evicting beyond the page bound", async () => {
    const cachedRows = MAX_CACHED_PAGES * PAGE_SIZE;
    const total = cachedRows + 8 * PAGE_SIZE;
    apiMocks.startAssetQuery.mockResolvedValue(
      readyStart([createSummary(1), createSummary(2)], total)
    );
    apiMocks.getAssetQueryPage.mockImplementation(async (_session, offset) => ({
      ...readyStart([createSummary(offset + 1), createSummary(offset + 2)], total),
      offset
    }));
    const { result } = await renderAssets();
    await settle(async () => {
      await result.refresh();
    });
    for (let offset = 2; offset < cachedRows; offset += 2) {
      await settle(async () => {
        await result.getAssetAtAsync(offset);
      });
    }
    expect(result.assets).toHaveLength(cachedRows);
    // Explicit access promotes the first page; render-time lookup does not.
    await settle(async () => {
      await result.getAssetAtAsync(0);
    });
    await settle(async () => {
      await result.getAssetAtAsync(cachedRows);
    });
    expect(result.getAssetAt(0)?.id).toBe(1);
    expect(result.getAssetAt(2)).toBeUndefined();
    settle(() => result.ensureRange(0, 1));
    for (let offset = 4; offset < total; offset += 2) {
      await settle(async () => {
        await result.getAssetAtAsync(offset);
      });
      expect(result.assets.length).toBeLessThanOrEqual(cachedRows);
    }
    expect(result.getAssetAt(0)?.id).toBe(1);
    await settle(async () => {
      await result.getAssetAtAsync(2);
    });
    expect(result.getAssetAt(2)?.id).toBe(3);
    expect(result.assets).toHaveLength(cachedRows);
  });

  it("does not retry failed demanded pages until Retry, even across repeated reactive flushes", async () => {
    apiMocks.startAssetQuery.mockResolvedValue(readyStart([createSummary(1), createSummary(2)], 6));
    apiMocks.getAssetQueryPage.mockRejectedValue(new Error("offline"));
    const { result } = await renderAssets();
    await settle(() => result.refresh());
    for (let i = 0; i < 6; i++) {
      await settle(async () => result.ensureRange(2, 3));
      settle(() => {});
    }
    expect(apiMocks.getAssetQueryPage).toHaveBeenCalledTimes(1);
    await settle(async () => {
      await expect(result.retryLoad()).rejects.toThrow("offline");
    });
    expect(apiMocks.getAssetQueryPage).toHaveBeenCalledTimes(2);
    expect(apiMocks.startAssetQuery).toHaveBeenCalledOnce();
  });

  it("retries only the requested asset page and keeps other failed pages blocked", async () => {
    apiMocks.startAssetQuery.mockResolvedValue(readyStart([createSummary(1), createSummary(2)], 6));
    apiMocks.getAssetQueryPage.mockRejectedValue(new Error("offline"));
    const { result } = await renderAssets();
    await settle(() => result.refresh());
    await settle(async () => {
      await expect(result.getAssetAtAsync(2)).rejects.toThrow("offline");
      await expect(result.getAssetAtAsync(4)).rejects.toThrow("offline");
    });
    apiMocks.getAssetQueryPage.mockResolvedValue({
      ...readyStart([createSummary(5), createSummary(6)], 6), offset: 4
    });
    await settle(async () => expect((await result.retryAssetAt(4))?.id).toBe(5));
    expect(result.loadError).toBe("offline");
    await settle(async () => {
      await expect(result.getAssetAtAsync(2)).rejects.toThrow("offline");
    });
    expect(apiMocks.getAssetQueryPage).toHaveBeenCalledTimes(3);
    expect(apiMocks.getAssetQueryPage).toHaveBeenLastCalledWith(7, 4, 2);
    expect(apiMocks.startAssetQuery).toHaveBeenCalledOnce();
  });

  it.each([
    "refresh",
    "reset",
    "unmount"
  ] as const)("rejects an entire range interrupted by %s", async (action) => {
    apiMocks.startAssetQuery.mockResolvedValue(readyStart([createSummary(1), createSummary(2)], 6));
    let resolve!: (range: object) => void;
    apiMocks.getAssetQueryIdRange.mockReturnValue(
      new Promise((done) => {
        resolve = done;
      })
    );
    const { result, cleanup: unmount } = await renderAssets();
    await settle(() => result.refresh());
    let range!: Promise<ArrayLike<number>>;
    await settle(async () => {
      range = result.getIdsRangeAsync(0, 5);
      await Promise.resolve();
    });
    const rejection = expect(range).rejects.toThrow("cancelled");
    await settle(async () => {
      if (action === "unmount") unmount();
      else await result[action]();
      resolve(idRangeResult([1, 2, 3, 4, 5, 6]));
      await rejection;
    });
    expect(apiMocks.getAssetQueryIdRange).toHaveBeenCalledOnce();
  });

  it("anchors a refresh at the first visible index and merges that page in one IPC", async () => {
    let visible: number | null = null;
    apiMocks.startAssetQuery
      .mockResolvedValueOnce(readyStart([createSummary(1), createSummary(2)], 40, 5))
      .mockResolvedValueOnce({
        ...readyStart([createSummary(31), createSummary(32)], 40, 6),
        offset: 30
      });
    const { result } = renderHook(() =>
      createAssetQuery({
        pageSize: PAGE_SIZE,
        anchorIndex: () => visible,
        filters: () => ({
          filterInclude: [],
          filterExclude: [],
          appliedMediaKind: "all",
          appliedFavoritesOnly: false
        })
      })
    );
    await settle(async () => {
      await result.refresh();
    });
    expect(apiMocks.startAssetQuery).toHaveBeenLastCalledWith(
      expect.objectContaining({ anchorOffset: null })
    );
    visible = 31;
    await settle(async () => {
      await result.refresh();
    });
    expect(apiMocks.startAssetQuery).toHaveBeenLastCalledWith(
      expect.objectContaining({ anchorOffset: 31 })
    );
    expect(result.getAssetAt(30)?.id).toBe(31);
    expect(result.getAssetAt(0)).toBeUndefined();
    expect(result.getAssetIndex(32)).toBe(31);
    expect(apiMocks.getAssetQueryPage).not.toHaveBeenCalled();
  });

  it("notifies render-time readers through version when a page merges in place", async () => {
    apiMocks.startAssetQuery.mockResolvedValue(readyStart([createSummary(1), createSummary(2)], 6));
    apiMocks.getAssetQueryPage.mockImplementation(async (_session, offset) => ({
      ...readyStart([createSummary(offset + 1), createSummary(offset + 2)], 6),
      offset
    }));
    const seen: Array<number | undefined> = [];
    const { result } = renderHook(() => {
      const query = createAssetQuery({
        pageSize: PAGE_SIZE,
        filters: () => ({
          filterInclude: [],
          filterExclude: [],
          appliedMediaKind: "all",
          appliedFavoritesOnly: false
        })
      });
      createEffect(
        () => query.getAssetAt(PAGE_SIZE)?.id,
        (id) => {
          seen.push(id);
        }
      );
      return query;
    });
    await settle(async () => {
      await result.refresh();
    });
    expect(seen[seen.length - 1]).toBeUndefined();
    await settle(async () => {
      await result.getAssetAtAsync(PAGE_SIZE);
    });
    expect(seen[seen.length - 1]).toBe(PAGE_SIZE + 1);
  });

  it("keeps old viewport rows as render-only fallback until their page reloads", async () => {
    apiMocks.startAssetQuery
      .mockResolvedValueOnce(readyStart([createSummary(1), createSummary(2)], 6, 5))
      .mockResolvedValueOnce(readyStart([createSummary(11), createSummary(12)], 6, 6));
    apiMocks.getAssetQueryPage.mockResolvedValueOnce({
      ...readyStart([createSummary(3), createSummary(4)], 6, 5),
      offset: PAGE_SIZE
    });
    const { result } = renderHook(() =>
      createAssetQuery({
        pageSize: PAGE_SIZE,
        filters: () => ({
          filterInclude: [],
          filterExclude: [],
          appliedMediaKind: "all",
          appliedFavoritesOnly: false
        })
      })
    );
    await settle(async () => {
      await result.refresh();
    });
    await settle(async () => {
      result.ensureRange(PAGE_SIZE, PAGE_SIZE + 1);
      await result.getAssetAtAsync(PAGE_SIZE);
    });
    expect(result.getAssetAt(PAGE_SIZE)?.id).toBe(3);

    let reload!: (value: unknown) => void;
    apiMocks.getAssetQueryPage.mockReturnValueOnce(
      new Promise((resolve) => {
        reload = resolve;
      })
    );
    await settle(async () => {
      await result.refresh();
    });
    // The new query's page replaced page 0; the deep viewport page stays on screen.
    expect(result.getAssetAt(0)?.id).toBe(11);
    expect(result.getAssetAt(PAGE_SIZE)?.id).toBe(3);
    expect(result.getAssetIndex(3)).toBeNull();
    expect(result.assets.map((asset) => asset.id)).toEqual([11, 12]);

    await settle(async () => {
      result.ensureRange(PAGE_SIZE, PAGE_SIZE + 1);
      reload({ ...readyStart([createSummary(13), createSummary(14)], 6, 6), offset: PAGE_SIZE });
    });
    await waitFor(() => expect(result.getAssetAt(PAGE_SIZE)?.id).toBe(13));
  });

  it("drops fallback rows that leave the viewport", async () => {
    apiMocks.startAssetQuery
      .mockResolvedValueOnce(readyStart([createSummary(1), createSummary(2)], 6, 5))
      .mockResolvedValueOnce(readyStart([createSummary(11), createSummary(12)], 6, 6));
    apiMocks.getAssetQueryPage
      .mockResolvedValueOnce({ ...readyStart([createSummary(3), createSummary(4)], 6, 5), offset: 2 })
      .mockReturnValue(new Promise(() => {}));
    const { result } = await renderAssets();
    await settle(async () => {
      await result.refresh();
    });
    await settle(async () => {
      result.ensureRange(2, 3);
      await result.getAssetAtAsync(2);
    });
    await settle(async () => {
      await result.refresh();
    });
    expect(result.getAssetAt(2)?.id).toBe(3);
    settle(() => result.ensureRange(0, 1));
    expect(result.getAssetAt(2)).toBeUndefined();
  });

  it("debounces stale-driven refreshes and lets an explicit refresh cancel the pending one", async () => {
    vi.useFakeTimers();
    apiMocks.startAssetQuery.mockResolvedValue(readyStart([createSummary(1)], 8, 5));
    apiMocks.getAssetQueryPage.mockResolvedValue({ status: "stale" });
    const { result } = await renderAssets();
    await settle(async () => {
      await result.refresh();
    });
    for (const offset of [2, 4, 6]) {
      await settle(async () => {
        expect(await result.getAssetAtAsync(offset)).toBeUndefined();
      });
    }
    expect(apiMocks.startAssetQuery).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(STALE_REFRESH_DELAY_MS);
    flush();
    expect(apiMocks.startAssetQuery).toHaveBeenCalledTimes(2);

    // A second burst right after a stale refresh waits for the minimum interval.
    await settle(async () => {
      await result.getAssetAtAsync(2);
    });
    await vi.advanceTimersByTimeAsync(STALE_REFRESH_DELAY_MS);
    expect(apiMocks.startAssetQuery).toHaveBeenCalledTimes(2);
    await vi.advanceTimersByTimeAsync(STALE_REFRESH_MIN_INTERVAL_MS);
    expect(apiMocks.startAssetQuery).toHaveBeenCalledTimes(3);

    // An explicit refresh runs at once and cancels a scheduled stale refresh.
    await vi.advanceTimersByTimeAsync(STALE_REFRESH_MIN_INTERVAL_MS);
    await settle(async () => {
      await result.getAssetAtAsync(2);
    });
    await settle(async () => {
      await result.refresh();
    });
    expect(apiMocks.startAssetQuery).toHaveBeenCalledTimes(4);
    await vi.advanceTimersByTimeAsync(STALE_REFRESH_MIN_INTERVAL_MS * 2);
    expect(apiMocks.startAssetQuery).toHaveBeenCalledTimes(4);
  });
});
