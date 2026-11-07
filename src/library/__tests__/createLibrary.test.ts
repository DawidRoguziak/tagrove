import { settle } from "../../test/settle";
import { ThumbnailStore } from "../thumbnails/thumbnailStore";
import { renderHook, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AssetSummary } from "../../platform/tauri/contracts/assets";
import { createLibrary } from "../createLibrary";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

function createAsset(id: number, thumbPath: string | null = null): AssetSummary {
  return {
    id,
    file_name: `${id}.jpg`,
    preview_path: null,
    kind: "image",
    modified_at: id,
    width: 100,
    height: 100,
    duration_ms: null,
    thumb_path: thumbPath,
    is_favorite: false,
    media_group_key: null,
    media_group_order: null
  };
}

function createTagListPage(items: string[] = [], total: number = items.length) {
  return { items, total };
}

const apiMocks = vi.hoisted(() => ({
  startAssetQuery: vi.fn(),
  getAssetQueryPage: vi.fn(),
  getAssetQueryPosition: vi.fn(),
  setAssetFavorite: vi.fn(),
  listTags: vi.fn()
}));

function ready(items: AssetSummary[], total: number, offset = 0) {
  return {
    status: "ready" as const,
    session_id: 1,
    revision: 1,
    total,
    offset,
    items: items
  };
}

const queueMocks = vi.hoisted(() => ({
  queueThumbnailsByIds: vi.fn(),
  setGalleryThumbnailDemand: vi.fn(),
  resetThumbnailQueue: vi.fn()
}));

vi.mock("../../platform/tauri/assets", async () => ({
  ...(await vi.importActual<typeof import("../../platform/tauri/assets")>(
    "../../platform/tauri/assets"
  )),
  ...apiMocks
}));
vi.mock("../../platform/tauri/tags", () => ({ listTags: apiMocks.listTags }));

vi.mock("../thumbnails/createThumbnailQueue", () => ({
  createThumbnailQueue: vi.fn(() => ({
    queueThumbnailsByIds: queueMocks.queueThumbnailsByIds,
    setGalleryThumbnailDemand: queueMocks.setGalleryThumbnailDemand,
    resetThumbnailQueue: queueMocks.resetThumbnailQueue,
    isGeneratingPage: false,
    pendingPageSize: 0,
    thumbnailStore: new ThumbnailStore(),
    renderingAssetIds: {}
  }))
}));

describe("createLibrary", () => {
  it("does not automatically retry a failed query when resolving selection", async () => {
    apiMocks.startAssetQuery.mockRejectedValue(new Error("busy"));
    const { result } = renderHook(() =>
      createLibrary({
        pageSize: 128,
        filters: {
          filterInclude: [],
          filterExclude: [],
          appliedMediaKind: "all",
          appliedFavoritesOnly: false
        }
      })
    );
    await settle(async () => {
      await expect(result.query.refresh()).rejects.toThrow("busy");
    });
    await expect(result.query.getAssetPosition(201)).rejects.toThrow("Asset query is unavailable");
    expect(apiMocks.startAssetQuery).toHaveBeenCalledTimes(1);
    expect(apiMocks.getAssetQueryPosition).not.toHaveBeenCalled();
    expect(result.query.queryPending).toBe(false);
  });

  beforeEach(() => {
    apiMocks.startAssetQuery.mockReset();
    apiMocks.getAssetQueryPage.mockReset();
    apiMocks.getAssetQueryPosition.mockReset();
    apiMocks.setAssetFavorite.mockReset().mockResolvedValue(undefined);
    apiMocks.listTags.mockReset();
    queueMocks.queueThumbnailsByIds.mockReset();
    queueMocks.setGalleryThumbnailDemand.mockReset();
    queueMocks.resetThumbnailQueue.mockReset();
  });

  it("looks up uncached positions and refreshes stale sessions without loading every page", async () => {
    apiMocks.startAssetQuery.mockResolvedValue(ready([createAsset(1)], 400));
    apiMocks.getAssetQueryPosition
      .mockResolvedValueOnce({ status: "resolved", index: 200 })
      .mockResolvedValueOnce({ status: "missing" })
      .mockResolvedValueOnce({ status: "stale" });
    const { result } = renderHook(() =>
      createLibrary({
        pageSize: 128,
        filters: {
          filterInclude: [],
          filterExclude: [],
          appliedMediaKind: "all",
          appliedFavoritesOnly: false
        }
      })
    );
    await settle(async () => result.query.refresh());
    await expect(result.query.getAssetPosition(201)).resolves.toEqual({
      status: "resolved",
      index: 200
    });
    expect(apiMocks.getAssetQueryPosition).toHaveBeenCalledWith(1, 201);
    await expect(result.query.getAssetPosition(401)).resolves.toEqual({ status: "missing" });
    await settle(async () => {
      expect(await result.query.getAssetPosition(201)).toEqual({ status: "stale" });
    });
    // The stale reply schedules one debounced refresh.
    await waitFor(() => expect(apiMocks.startAssetQuery).toHaveBeenCalledTimes(2));
    expect(apiMocks.getAssetQueryPage).not.toHaveBeenCalled();
    expect(result.query.assets).toHaveLength(1);
  });

  it("discards a late position reply after a refresh starts", async () => {
    apiMocks.startAssetQuery.mockResolvedValue(ready([createAsset(1)], 400));
    const reply = deferred<{ status: "resolved"; index: number }>();
    apiMocks.getAssetQueryPosition.mockReturnValue(reply.promise);
    const { result } = renderHook(() =>
      createLibrary({
        pageSize: 128,
        filters: {
          filterInclude: [],
          filterExclude: [],
          appliedMediaKind: "all",
          appliedFavoritesOnly: false
        }
      })
    );
    await settle(async () => result.query.refresh());
    const position = result.query.getAssetPosition(201);
    await settle(async () => result.query.refresh());
    reply.resolve({ status: "resolved", index: 200 });
    await expect(position).resolves.toEqual({ status: "stale" });
  });

  it.each([
    "initial",
    "page"
  ])("rereads delayed %s summaries across every metadata patch without restarting", async (kind) => {
    const delayed = deferred<ReturnType<typeof ready>>();
    const reread = deferred<ReturnType<typeof ready>>();
    const original = createAsset(2);
    const saved = { ...original, is_favorite: true };
    const final = { ...saved, media_group_key: "saved" };
    apiMocks.startAssetQuery.mockReturnValue(
      kind === "initial" ? delayed.promise : Promise.resolve(ready([createAsset(1)], 2))
    );
    apiMocks.getAssetQueryPage.mockReturnValueOnce(
      kind === "initial" ? reread.promise : delayed.promise
    );
    if (kind === "page") apiMocks.getAssetQueryPage.mockReturnValueOnce(reread.promise);
    apiMocks.getAssetQueryPage.mockResolvedValue(ready([final], 2, kind === "initial" ? 0 : 1));
    const { result } = renderHook(() =>
      createLibrary({
        pageSize: 1,
        filters: {
          filterInclude: [],
          filterExclude: [],
          appliedMediaKind: "all",
          appliedFavoritesOnly: false
        }
      })
    );
    let pending: Promise<void | AssetSummary | undefined>;
    if (kind === "initial")
      settle(() => {
        pending = result.query.refresh();
      });
    else {
      await settle(() => result.query.refresh());
      settle(() => {
        pending = result.query.getAssetAtAsync(1);
      });
    }
    await settle(() => result.mutations.setFavorite(2, true));
    expect(apiMocks.setAssetFavorite).toHaveBeenCalledExactlyOnceWith(2, true);
    await settle(async () => delayed.resolve(ready([original], 2, kind === "initial" ? 0 : 1)));
    expect(apiMocks.getAssetQueryPage).toHaveBeenCalledTimes(kind === "initial" ? 1 : 2);
    settle(() =>
      result.query.patchAssets((previous) =>
        previous.map((asset) => ({ ...asset, media_group_key: "saved" }))
      )
    );
    await settle(async () => {
      reread.resolve(ready([saved], 2, kind === "initial" ? 0 : 1));
      const returned = await pending;
      if (kind === "page") expect(returned).toEqual(final);
    });
    expect(result.query.assets).toContainEqual(final);
    expect(apiMocks.startAssetQuery).toHaveBeenCalledTimes(1);
    expect(queueMocks.resetThumbnailQueue).not.toHaveBeenCalled();
    expect(apiMocks.getAssetQueryPage).toHaveBeenLastCalledWith(1, kind === "initial" ? 0 : 1, 1);
  });

  it("loads first page with replace mode and keeps the thumbnail queue", async () => {
    apiMocks.startAssetQuery.mockResolvedValueOnce(
      ready([createAsset(1, "thumb-1.jpg"), createAsset(2, null)], 5)
    );

    const { result } = renderHook(() =>
      createLibrary({
        pageSize: 2,
        filters: {
          filterInclude: ["cat"],
          filterExclude: ["dog"],
          appliedMediaKind: "image",
          appliedFavoritesOnly: true
        }
      })
    );

    await settle(async () => {
      await result.query.refresh();
    });

    expect(apiMocks.startAssetQuery).toHaveBeenCalledWith({
      tagsAnd: ["cat"],
      tagsNot: ["dog"],
      mediaKind: "image",
      favoritesOnly: true,
      metaFilter: null,
      ownerId: expect.any(String),
      generation: 1,
      pageSize: 2
    });
    expect(result.query.assets.map((asset) => asset.id)).toEqual([1, 2]);
    expect(result.query.total).toBe(5);
    expect(result.thumbnails.thumbnailStore.getPath(1)).toBe("thumb-1.jpg");
    expect(result.query.loading).toBe(false);
    // A refresh keeps thumbnail paths; the new page's `thumb_path` confirms them.
    expect(queueMocks.resetThumbnailQueue).not.toHaveBeenCalled();
  });

  it("uses the full preview path for a provisional video asset", async () => {
    const video: AssetSummary = {
      ...createAsset(7),
      file_name: "clip.mp4",
      preview_path: "C:/library/clip.mp4",
      kind: "video",
      duration_ms: 10_000
    };
    apiMocks.startAssetQuery.mockResolvedValueOnce(ready([video], 1));

    const { result } = renderHook(() =>
      createLibrary({
        pageSize: 10,
        filters: {
          filterInclude: [],
          filterExclude: [],
          appliedMediaKind: "all",
          appliedFavoritesOnly: false
        }
      })
    );

    await settle(async () => {
      await result.query.refresh();
    });

    expect(result.query.assets[0].preview_path).toBe("C:/library/clip.mp4");
    expect(result.query.assets[0].preview_path).not.toBe("clip.mp4");
  });

  it("forwards meta filters to asset loading", async () => {
    apiMocks.startAssetQuery.mockResolvedValueOnce(ready([createAsset(3)], 1));

    const { result } = renderHook(() =>
      createLibrary({
        pageSize: 10,
        filters: {
          filterInclude: [],
          filterExclude: [],
          metaFilter: {
            type: "groupName",
            groupName: "Trip-2026"
          },
          appliedMediaKind: "all",
          appliedFavoritesOnly: false
        }
      })
    );

    await settle(async () => {
      await result.query.refresh();
    });

    expect(apiMocks.startAssetQuery).toHaveBeenCalledWith({
      tagsAnd: [],
      tagsNot: [],
      mediaKind: "all",
      favoritesOnly: false,
      metaFilter: {
        type: "groupName",
        groupName: "Trip-2026"
      },
      ownerId: expect.any(String),
      generation: 1,
      pageSize: 10
    });
  });

  it("guards against duplicate page loads during rapid range demand", async () => {
    const nextPage = deferred<ReturnType<typeof ready>>();
    apiMocks.startAssetQuery.mockResolvedValueOnce(ready([createAsset(1), createAsset(2)], 4));
    apiMocks.getAssetQueryPage.mockReturnValueOnce(nextPage.promise);

    const { result } = renderHook(() =>
      createLibrary({
        pageSize: 2,
        filters: {
          filterInclude: [],
          filterExclude: [],
          appliedMediaKind: "all",
          appliedFavoritesOnly: false
        }
      })
    );

    await settle(async () => {
      await result.query.refresh();
    });

    settle(() => {
      result.query.ensureRange(2, 3);
      result.query.ensureRange(2, 3);
    });

    expect(apiMocks.startAssetQuery).toHaveBeenCalledTimes(1);
    expect(apiMocks.getAssetQueryPage).toHaveBeenCalledTimes(1);

    nextPage.resolve(ready([createAsset(3), createAsset(4)], 4, 2));

    await waitFor(() => {
      expect(result.query.assets.map((asset) => asset.id)).toEqual([1, 2, 3, 4]);
    });

    settle(() => {
      result.query.ensureRange(2, 3);
    });
    expect(apiMocks.getAssetQueryPage).toHaveBeenCalledTimes(1);
  });

  it("shares an in-flight page result between concurrent indexed lookups", async () => {
    const nextPage = deferred<ReturnType<typeof ready>>();
    apiMocks.startAssetQuery.mockResolvedValueOnce(ready([createAsset(1), createAsset(2)], 4));
    apiMocks.getAssetQueryPage.mockReturnValueOnce(nextPage.promise);

    const { result } = renderHook(() =>
      createLibrary({
        pageSize: 2,
        filters: {
          filterInclude: [],
          filterExclude: [],
          appliedMediaKind: "all",
          appliedFavoritesOnly: false
        }
      })
    );
    await settle(async () => {
      await result.query.refresh();
    });

    let firstLookup!: Promise<AssetSummary | undefined>;
    let secondLookup!: Promise<AssetSummary | undefined>;
    settle(() => {
      firstLookup = result.query.getAssetAtAsync(2);
      secondLookup = result.query.getAssetAtAsync(2);
    });
    expect(apiMocks.getAssetQueryPage).toHaveBeenCalledTimes(1);

    nextPage.resolve(ready([createAsset(3), createAsset(4)], 4, 2));
    let resolved!: Array<AssetSummary | undefined>;
    await settle(async () => {
      resolved = await Promise.all([firstLookup, secondLookup]);
    });
    expect(resolved.map((asset) => asset?.id)).toEqual([3, 3]);
  });

  it("queues visible ids from clamped virtual range", async () => {
    apiMocks.startAssetQuery.mockResolvedValueOnce(
      ready([createAsset(11), createAsset(12), createAsset(13)], 3)
    );

    const { result } = renderHook(() =>
      createLibrary({
        pageSize: 5,
        filters: {
          filterInclude: [],
          filterExclude: [],
          appliedMediaKind: "all",
          appliedFavoritesOnly: false
        }
      })
    );

    await settle(async () => {
      await result.query.refresh();
    });

    settle(() => {
      result.handleVirtualRangeChange({
        paused: false,
        startIndex: -5,
        endIndex: 50,
        visibleStartIndex: 1,
        visibleEndIndex: 1
      });
    });

    expect(queueMocks.setGalleryThumbnailDemand).toHaveBeenCalledWith([12], [11, 13]);

    settle(() => {
      result.handleVirtualRangeChange({
        paused: false,
        startIndex: 3,
        endIndex: 1,
        visibleStartIndex: 3,
        visibleEndIndex: 1
      });
    });

    expect(queueMocks.setGalleryThumbnailDemand).toHaveBeenLastCalledWith([], []);
  });

  it("clears assets, tags and thumbs on library reset", async () => {
    apiMocks.startAssetQuery.mockResolvedValueOnce(ready([createAsset(1, "thumb-1.jpg")], 1));
    apiMocks.listTags.mockResolvedValueOnce(createTagListPage(["cat", "travel"]));

    const { result } = renderHook(() =>
      createLibrary({
        pageSize: 10,
        filters: {
          filterInclude: [],
          filterExclude: [],
          appliedMediaKind: "all",
          appliedFavoritesOnly: false
        }
      })
    );

    await settle(async () => {
      await result.query.refresh();
      await result.vocabulary.refreshKnownTags();
    });

    expect(apiMocks.listTags).toHaveBeenCalledWith({
      query: "",
      offset: 0,
      limit: 200
    });
    expect(result.query.assets).toHaveLength(1);
    expect(result.vocabulary.knownTags).toEqual(["cat", "travel"]);
    expect(result.thumbnails.thumbnailStore.getPath(1)).toBe("thumb-1.jpg");

    settle(() => {
      result.handleLibraryCleared();
    });

    expect(result.query.assets).toEqual([]);
    expect(result.query.total).toBe(0);
    expect(result.vocabulary.knownTags).toEqual([]);
    expect(result.thumbnails.thumbnailStore.getPath(1)).toBeUndefined();
    expect(queueMocks.resetThumbnailQueue).toHaveBeenCalledTimes(1);
  });
  it("holds only the latest paused range through late pages and filter refresh, then resumes it", async () => {
    apiMocks.startAssetQuery.mockResolvedValue(ready([createAsset(1), createAsset(2)], 100));
    const pending = deferred<ReturnType<typeof ready>>();
    apiMocks.getAssetQueryPage.mockReturnValueOnce(pending.promise)
      .mockImplementation(async (_session, offset) => ready([createAsset(offset + 1), createAsset(offset + 2)], 100, offset));
    const { result } = renderHook(() => createLibrary({ pageSize: 2 }));
    await settle(async () => result.query.refresh());
    const viewport = (start: number, paused: boolean) => ({
      startIndex: start, endIndex: start + 1, visibleStartIndex: start,
      visibleEndIndex: start + 1, paused
    });
    settle(() => result.handleVirtualRangeChange(viewport(2, false)));
    expect(apiMocks.getAssetQueryPage).toHaveBeenCalledTimes(1);
    settle(() => result.handleVirtualRangeChange(viewport(20, true)));
    settle(() => result.handleVirtualRangeChange(viewport(40, true)));
    await settle(async () => pending.resolve(ready([createAsset(3), createAsset(4)], 100, 2)));
    expect(result.query.getAssetAt(2)?.id).toBe(3);
    expect(apiMocks.getAssetQueryPage).toHaveBeenCalledTimes(1);
    expect(queueMocks.setGalleryThumbnailDemand).toHaveBeenLastCalledWith([], []);
    expect(queueMocks.resetThumbnailQueue).not.toHaveBeenCalled();
    await settle(async () => result.applyFilters({ filterInclude: ["new"], filterExclude: [],
      appliedMediaKind: "all", appliedFavoritesOnly: false }));
    expect(apiMocks.getAssetQueryPage).toHaveBeenCalledTimes(1);
    expect(queueMocks.setGalleryThumbnailDemand).toHaveBeenLastCalledWith([], []);
    // Explicit reads for lightbox and selection still work during the gallery pause.
    await settle(async () => {
      expect((await result.query.getAssetAtAsync(60))?.id).toBe(61);
      expect(await result.query.getIdsRangeAsync(62, 63)).toEqual([63, 64]);
    });
    expect(apiMocks.getAssetQueryPage).toHaveBeenCalledTimes(3);
    settle(() => result.handleVirtualRangeChange(viewport(40, false)));
    await waitFor(() => expect(result.query.getAssetAt(40)?.id).toBe(41));
    expect(apiMocks.getAssetQueryPage.mock.calls.map(([, offset]) => offset)).toEqual([2, 60, 62, 40]);
    expect(queueMocks.setGalleryThumbnailDemand).toHaveBeenLastCalledWith([41, 42], []);
    settle(() => result.handleVirtualRangeChange(null));
    expect(queueMocks.setGalleryThumbnailDemand).toHaveBeenLastCalledWith([], []);
    expect(result.query.getAssetAt(40)?.id).toBe(41);
  });

});
