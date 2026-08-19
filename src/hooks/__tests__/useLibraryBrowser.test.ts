import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Asset, AssetSummary } from "../../types";
import { useLibraryBrowser } from "../useLibraryBrowser";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

function createAsset(id: number, thumbPath: string | null = null): Asset {
  return {
    id,
    path: `C:/library/${id}.jpg`,
    kind: "image",
    size_bytes: 100,
    modified_at: id,
    width: 100,
    height: 100,
    duration_ms: null,
    thumb_path: thumbPath,
    is_favorite: false,
    media_group_key: null,
    media_group_order: null,
    tags: []
  };
}

function createTagListPage(items: string[] = [], total: number = items.length) {
  return { items, total };
}

const apiMocks = vi.hoisted(() => ({
  startAssetQuery: vi.fn(),
  getAssetQueryPage: vi.fn(),
  listTags: vi.fn()
}));

function toSummary(asset: Asset): AssetSummary {
  return {
    id: asset.id,
    file_name: asset.path.split("/").at(-1) ?? asset.path,
    preview_path: asset.path,
    kind: asset.kind,
    modified_at: asset.modified_at,
    width: asset.width,
    height: asset.height,
    duration_ms: asset.duration_ms,
    thumb_path: asset.thumb_path,
    is_favorite: asset.is_favorite,
    media_group_key: asset.media_group_key,
    media_group_order: asset.media_group_order
  };
}

function ready(items: Asset[], total: number, offset = 0) {
  return {
    status: "ready" as const,
    session_id: 1,
    revision: 1,
    total,
    offset,
    items: items.map(toSummary)
  };
}

const queueMocks = vi.hoisted(() => ({
  queueThumbnailsByIds: vi.fn(),
  resetThumbnailQueue: vi.fn()
}));

vi.mock("../../api", async () => {
  const actual = await vi.importActual<typeof import("../../api")>("../../api");
  return {
    ...actual,
    ...apiMocks
  };
});

vi.mock("../useThumbnailQueue", () => ({
  useThumbnailQueue: vi.fn(() => ({
    queueThumbnailsByIds: queueMocks.queueThumbnailsByIds,
    resetThumbnailQueue: queueMocks.resetThumbnailQueue,
    isGeneratingPage: false,
    pendingPageSize: 0,
    renderingAssetIds: {}
  }))
}));

describe("useLibraryBrowser", () => {
  beforeEach(() => {
    apiMocks.startAssetQuery.mockReset();
    apiMocks.getAssetQueryPage.mockReset();
    apiMocks.listTags.mockReset();
    queueMocks.queueThumbnailsByIds.mockReset();
    queueMocks.resetThumbnailQueue.mockReset();
  });

  it("loads first page with replace mode and resets thumbnail queue", async () => {
    apiMocks.startAssetQuery.mockResolvedValueOnce(
      ready([createAsset(1, "thumb-1.jpg"), createAsset(2, null)], 5)
    );

    const { result } = renderHook(() =>
      useLibraryBrowser({
        pageSize: 2,
        filterInclude: ["cat"],
        filterExclude: ["dog"],
        appliedMediaKind: "image",
        appliedFavoritesOnly: true
      })
    );

    await act(async () => {
      await result.current.refresh();
    });

    expect(apiMocks.startAssetQuery).toHaveBeenCalledWith({
      tagsAnd: ["cat"],
      tagsNot: ["dog"],
      mediaKind: "image",
      favoritesOnly: true,
      metaFilter: null,
      generation: 1,
      pageSize: 2
    });
    expect(result.current.assets.map((asset) => asset.id)).toEqual([1, 2]);
    expect(result.current.total).toBe(5);
    expect(result.current.thumbs).toEqual({ 1: "thumb-1.jpg" });
    expect(result.current.loading).toBe(false);
    expect(queueMocks.resetThumbnailQueue).toHaveBeenCalledTimes(1);
  });

  it("forwards meta filters to asset loading", async () => {
    apiMocks.startAssetQuery.mockResolvedValueOnce(ready([createAsset(3)], 1));

    const { result } = renderHook(() =>
      useLibraryBrowser({
        pageSize: 10,
        filterInclude: [],
        filterExclude: [],
        metaFilter: {
          type: "groupName",
          groupName: "Trip-2026"
        },
        appliedMediaKind: "all",
        appliedFavoritesOnly: false
      })
    );

    await act(async () => {
      await result.current.refresh();
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
      generation: 1,
      pageSize: 10
    });
  });

  it("guards against duplicate page loads during rapid reach-end events", async () => {
    const nextPage = deferred<ReturnType<typeof ready>>();
    apiMocks.startAssetQuery.mockResolvedValueOnce(ready([createAsset(1), createAsset(2)], 4));
    apiMocks.getAssetQueryPage.mockReturnValueOnce(nextPage.promise);

    const { result } = renderHook(() =>
      useLibraryBrowser({
        pageSize: 2,
        filterInclude: [],
        filterExclude: [],
        appliedMediaKind: "all",
        appliedFavoritesOnly: false
      })
    );

    await act(async () => {
      await result.current.refresh();
    });

    act(() => {
      result.current.handleReachEnd();
      result.current.handleReachEnd();
    });

    expect(apiMocks.startAssetQuery).toHaveBeenCalledTimes(1);
    expect(apiMocks.getAssetQueryPage).toHaveBeenCalledTimes(1);

    nextPage.resolve(ready([createAsset(3), createAsset(4)], 4, 2));

    await waitFor(() => {
      expect(result.current.assets.map((asset) => asset.id)).toEqual([1, 2, 3, 4]);
    });

    act(() => {
      result.current.handleReachEnd();
    });
    expect(apiMocks.getAssetQueryPage).toHaveBeenCalledTimes(1);
  });

  it("queues visible ids from clamped virtual range", async () => {
    apiMocks.startAssetQuery.mockResolvedValueOnce(
      ready([createAsset(11), createAsset(12), createAsset(13)], 3)
    );

    const { result } = renderHook(() =>
      useLibraryBrowser({
        pageSize: 5,
        filterInclude: [],
        filterExclude: [],
        appliedMediaKind: "all",
        appliedFavoritesOnly: false
      })
    );

    await act(async () => {
      await result.current.refresh();
    });

    act(() => {
      result.current.handleVirtualRangeChange(-5, 50);
    });

    expect(queueMocks.queueThumbnailsByIds).toHaveBeenCalledWith([11, 12, 13]);

    act(() => {
      result.current.handleVirtualRangeChange(3, 1);
    });

    expect(queueMocks.queueThumbnailsByIds).toHaveBeenCalledTimes(1);
  });

  it("clears assets, tags and thumbs on library reset", async () => {
    apiMocks.startAssetQuery.mockResolvedValueOnce(ready([createAsset(1, "thumb-1.jpg")], 1));
    apiMocks.listTags.mockResolvedValueOnce(createTagListPage(["cat", "travel"]));

    const { result } = renderHook(() =>
      useLibraryBrowser({
        pageSize: 10,
        filterInclude: [],
        filterExclude: [],
        appliedMediaKind: "all",
        appliedFavoritesOnly: false
      })
    );

    await act(async () => {
      await result.current.refresh();
      await result.current.refreshKnownTags();
    });

    expect(apiMocks.listTags).toHaveBeenCalledWith({
      query: "",
      offset: 0,
      limit: 200
    });
    expect(result.current.assets).toHaveLength(1);
    expect(result.current.knownTags).toEqual(["cat", "travel"]);
    expect(result.current.thumbs).toEqual({ 1: "thumb-1.jpg" });

    act(() => {
      result.current.handleLibraryCleared();
    });

    expect(result.current.assets).toEqual([]);
    expect(result.current.total).toBe(0);
    expect(result.current.knownTags).toEqual([]);
    expect(result.current.thumbs).toEqual({});
    expect(queueMocks.resetThumbnailQueue).toHaveBeenCalledTimes(2);
  });
});
