import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AssetSummary } from "../../types";
import { useLibraryAssets } from "../useLibraryAssets";

const apiMocks = vi.hoisted(() => ({
  startAssetQuery: vi.fn(),
  getAssetQueryPage: vi.fn()
}));

vi.mock("../../api", () => apiMocks);

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
    useLibraryAssets({
      pageSize: PAGE_SIZE,
      filterInclude: [],
      filterExclude: [],
      metaFilter: null,
      appliedMediaKind: "all",
      appliedFavoritesOnly: false,
      setThumbs: vi.fn(),
      resetThumbnailQueue: vi.fn()
    })
  );
}

describe("useLibraryAssets", () => {
  beforeEach(() => {
    apiMocks.startAssetQuery.mockReset();
    apiMocks.getAssetQueryPage.mockReset();
  });

  it("replaces the cache with the ready first page of the current generation", async () => {
    apiMocks.startAssetQuery.mockResolvedValue(readyStart([createSummary(1), createSummary(2)]));
    const { result } = await renderAssets();

    await act(async () => {
      await result.current.refresh();
    });

    expect(result.current.total).toBe(2);
    expect(result.current.offset).toBe(PAGE_SIZE);
    expect(result.current.loadError).toBeNull();
    expect(result.current.getAssetAt(0)?.id).toBe(1);
    expect(apiMocks.startAssetQuery).toHaveBeenCalledWith(
      expect.objectContaining({ generation: 1, pageSize: PAGE_SIZE })
    );
  });

  it("ignores a superseded start and keeps the previous cache visible", async () => {
    apiMocks.startAssetQuery.mockResolvedValueOnce(readyStart([createSummary(1)], 1, 7));
    const { result } = await renderAssets();
    await act(async () => {
      await result.current.refresh();
    });

    apiMocks.startAssetQuery.mockResolvedValueOnce({ status: "superseded" });
    await act(async () => {
      await result.current.refresh();
    });

    expect(result.current.total).toBe(1);
    expect(result.current.getAssetAt(0)?.id).toBe(1);
    expect(result.current.loading).toBe(false);
  });

  it("records a start failure in loadError and recovers through retryLoad", async () => {
    apiMocks.startAssetQuery
      .mockRejectedValueOnce(new Error("database is locked"))
      .mockResolvedValueOnce(readyStart([createSummary(1), createSummary(2), createSummary(3)], 3, 9));
    const { result } = await renderAssets();

    await act(async () => {
      await expect(result.current.refresh()).rejects.toThrow("database is locked");
    });
    expect(result.current.loadError).toBe("database is locked");

    await act(async () => {
      await result.current.retryLoad();
    });
    expect(result.current.loadError).toBeNull();
    expect(result.current.total).toBe(3);
    expect(result.current.getAssetAt(0)?.id).toBe(1);
  });

  it("keeps a failed page retryable and clears loadError once it loads", async () => {
    apiMocks.startAssetQuery.mockResolvedValue(readyStart([createSummary(1), createSummary(2)], 4, 5));
    const { result } = await renderAssets();
    await act(async () => {
      await result.current.refresh();
    });

    apiMocks.getAssetQueryPage.mockRejectedValueOnce(new Error("page worker failed"));
    await act(async () => {
      await expect(result.current.getAssetAtAsync(PAGE_SIZE)).rejects.toThrow("page worker failed");
    });
    expect(result.current.loadError).toBe("page worker failed");
    expect(result.current.pageFailureEpoch).toBeGreaterThan(0);
    expect(result.current.getAssetAt(PAGE_SIZE)).toBeUndefined();

    apiMocks.getAssetQueryPage.mockResolvedValueOnce({
      status: "ready",
      session_id: 5,
      revision: 3,
      total: 4,
      offset: PAGE_SIZE,
      items: [createSummary(3), createSummary(4)]
    });
    await act(async () => {
      const asset = await result.current.getAssetAtAsync(PAGE_SIZE);
      expect(asset?.id).toBe(3);
    });
    expect(result.current.loadError).toBeNull();
    expect(apiMocks.getAssetQueryPage).toHaveBeenNthCalledWith(2, 5, PAGE_SIZE, PAGE_SIZE);
  });

  it("restarts the query automatically when a page reports stale", async () => {
    apiMocks.startAssetQuery
      .mockResolvedValueOnce(readyStart([createSummary(1)], 4, 5))
      .mockResolvedValueOnce(readyStart([createSummary(9)], 1, 6));
    const { result } = await renderAssets();
    await act(async () => {
      await result.current.refresh();
    });

    apiMocks.getAssetQueryPage.mockResolvedValueOnce({ status: "stale" });
    await act(async () => {
      // A stale page resolves as a hole while a fresh query replaces the view.
      const asset = await result.current.getAssetAtAsync(PAGE_SIZE);
      expect(asset).toBeUndefined();
    });

    await waitFor(() => expect(result.current.total).toBe(1));
    expect(apiMocks.startAssetQuery).toHaveBeenCalledTimes(2);
  });

  it("drops an obsolete page response captured before a refresh", async () => {
    let resolvePage!: (value: unknown) => void;
    apiMocks.startAssetQuery.mockResolvedValue(readyStart([createSummary(1)], 4, 5));
    const { result } = await renderAssets();
    await act(async () => {
      await result.current.refresh();
    });

    apiMocks.getAssetQueryPage.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolvePage = resolve;
        })
    );
    let pagePromise!: Promise<unknown>;
    act(() => {
      pagePromise = result.current.getAssetAtAsync(PAGE_SIZE);
    });

    await act(async () => {
      await result.current.refresh();
    });
    await act(async () => {
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
    expect(result.current.getAssetAt(PAGE_SIZE)?.id).not.toBe(8);
  });
});
