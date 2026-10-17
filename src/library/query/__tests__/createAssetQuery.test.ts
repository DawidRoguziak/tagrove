import { settle } from "../../../test/settle";
import { renderHook, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AssetSummary } from "../../../platform/tauri/contracts/assets";
import { createAssetQuery } from "../createAssetQuery";

const apiMocks = vi.hoisted(() => ({
  startAssetQuery: vi.fn(),
  getAssetQueryPage: vi.fn()
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
    thumb_path: null,
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
      }),
      resetThumbnailQueue: vi.fn()
    })
  );
}

describe("createAssetQuery", () => {
  beforeEach(() => {
    apiMocks.startAssetQuery.mockReset();
    apiMocks.getAssetQueryPage.mockReset();
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
    expect(result.offset).toBe(PAGE_SIZE);
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
      filters: () => ({ filterInclude: included, filterExclude: [], appliedMediaKind: "all", appliedFavoritesOnly: false }),
      resetThumbnailQueue: vi.fn()
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

  it("restarts the query automatically when a page reports stale", async () => {
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

  it("resolves ordered snapshot IDs across unloaded pages via getIdsRangeAsync", async () => {
    apiMocks.startAssetQuery.mockResolvedValue(
      readyStart([createSummary(1), createSummary(2)], 4, 5)
    );
    const { result } = await renderAssets();
    await settle(async () => {
      await result.refresh();
    });

    apiMocks.getAssetQueryPage.mockResolvedValueOnce({
      status: "ready",
      session_id: 5,
      revision: 3,
      total: 4,
      offset: PAGE_SIZE,
      items: [createSummary(3), createSummary(4)]
    });

    let ids: number[] = [];
    await settle(async () => {
      ids = await result.getIdsRangeAsync(1, 2);
    });
    expect(ids).toEqual([2, 3]);
    expect(apiMocks.getAssetQueryPage).toHaveBeenCalledWith(5, PAGE_SIZE, PAGE_SIZE);
  });

  it("rejects the whole ID range when a required page fails", async () => {
    apiMocks.startAssetQuery.mockResolvedValue(
      readyStart([createSummary(1), createSummary(2)], 4, 5)
    );
    const { result } = await renderAssets();
    await settle(async () => {
      await result.refresh();
    });

    apiMocks.getAssetQueryPage.mockRejectedValueOnce(new Error("page worker failed"));
    await settle(async () => {
      // Index 1 is cached, index 2 needs the failing second page.
      await expect(result.getIdsRangeAsync(1, 2)).rejects.toThrow("page worker failed");
    });
  });

  it("rejects an ID range cancelled by a stale session", async () => {
    apiMocks.startAssetQuery
      .mockResolvedValueOnce(readyStart([createSummary(1), createSummary(2)], 4, 5))
      .mockResolvedValueOnce(readyStart([createSummary(9)], 1, 6));
    const { result } = await renderAssets();
    await settle(async () => {
      await result.refresh();
    });
    apiMocks.getAssetQueryPage.mockResolvedValueOnce({ status: "stale" });

    await settle(async () => {
      await expect(result.getIdsRangeAsync(1, 2)).rejects.toThrow("cancelled");
    });
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
  it("retains recently accessed and active pages while evicting beyond twelve pages", async () => {
    apiMocks.startAssetQuery.mockResolvedValue(
      readyStart([createSummary(1), createSummary(2)], 40)
    );
    apiMocks.getAssetQueryPage.mockImplementation(async (_session, offset) => ({
      ...readyStart([createSummary(offset + 1), createSummary(offset + 2)], 40),
      offset
    }));
    const { result } = await renderAssets();
    await settle(async () => {
      await result.refresh();
    });
    for (let offset = 2; offset < 24; offset += 2) {
      await settle(async () => {
        await result.getAssetAtAsync(offset);
      });
    }
    expect(result.assets).toHaveLength(24);
    // Explicit access promotes the first page; render-time lookup does not.
    await settle(async () => {
      await result.getAssetAtAsync(0);
    });
    await settle(async () => {
      await result.getAssetAtAsync(24);
    });
    expect(result.getAssetAt(0)?.id).toBe(1);
    expect(result.getAssetAt(2)).toBeUndefined();
    settle(() => result.ensureRange(0, 1));
    for (let offset = 4; offset < 40; offset += 2) {
      await settle(async () => {
        await result.getAssetAtAsync(offset);
      });
      expect(result.assets.length).toBeLessThanOrEqual(24);
    }
    expect(result.getAssetAt(0)?.id).toBe(1);
    await settle(async () => {
      await result.getAssetAtAsync(2);
    });
    expect(result.getAssetAt(2)?.id).toBe(3);
    expect(result.assets).toHaveLength(24);
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

  it("Retry unblocks failed offscreen selection pages without fetching undemanded pages", async () => {
    apiMocks.startAssetQuery.mockResolvedValue(readyStart([createSummary(1), createSummary(2)], 6));
    apiMocks.getAssetQueryPage.mockRejectedValueOnce(new Error("offline"));
    const { result } = await renderAssets();
    await settle(() => result.refresh());
    settle(() => result.ensureRange(0, 1));
    await settle(async () => {
      await expect(result.getIdsRangeAsync(4, 5)).rejects.toThrow("offline");
    });
    apiMocks.getAssetQueryPage.mockResolvedValue({
      ...readyStart([createSummary(5), createSummary(6)], 6),
      offset: 4
    });
    await settle(() => result.retryLoad());
    expect(apiMocks.getAssetQueryPage).toHaveBeenCalledOnce();
    await settle(async () => {
      expect(await result.getIdsRangeAsync(4, 5)).toEqual([5, 6]);
    });
    expect(apiMocks.getAssetQueryPage).toHaveBeenCalledTimes(2);
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
    let resolve!: (page: object) => void;
    apiMocks.getAssetQueryPage.mockReturnValue(
      new Promise((done) => {
        resolve = done;
      })
    );
    const { result, cleanup: unmount } = await renderAssets();
    await settle(() => result.refresh());
    let range!: Promise<number[]>;
    await settle(async () => {
      range = result.getIdsRangeAsync(0, 5);
      await Promise.resolve();
    });
    const rejection = expect(range).rejects.toThrow("cancelled");
    await settle(async () => {
      if (action === "unmount") unmount();
      else await result[action]();
      resolve({ ...readyStart([createSummary(3), createSummary(4)], 6), offset: 2 });
      await rejection;
    });
    expect(apiMocks.getAssetQueryPage).toHaveBeenCalledOnce();
  });
});
