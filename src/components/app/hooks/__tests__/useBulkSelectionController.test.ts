import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AssetDetails, AssetSummary } from "../../../../types";
import { useBulkSelectionController } from "../useBulkSelectionController";
import { useAssetTagState } from "../useAssetTagState";

const apiMocks = vi.hoisted(() => ({ getAssetDetails: vi.fn(), setAssetTags: vi.fn() }));
const actionMocks = vi.hoisted(() => ({ applyBulkTagsAction: vi.fn() }));

vi.mock("../../../../api", async () => {
  const actual = await vi.importActual<typeof import("../../../../api")>("../../../../api");
  return { ...actual, ...apiMocks };
});
vi.mock("../../../bulk/tagging/services/applyBulkTagsAction", () => actionMocks);

function createAsset(id: number): AssetSummary {
  return {
    id, file_name: `${id}.jpg`, preview_path: null, kind: "image", modified_at: 1,
    width: 100, height: 100, duration_ms: null, thumb_path: null, is_favorite: false,
    media_group_key: null, media_group_order: null
  };
}

function createDetails(summary: AssetSummary, tags: string[]): AssetDetails {
  return {
    ...summary,
    path: `C:/media/${summary.id}.jpg`,
    size_bytes: 1,
    tags
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((next) => { resolve = next; });
  return { promise, resolve };
}

function options(assets: AssetSummary[], refresh = vi.fn(async () => {})) {
  return {
    assets,
    knownTags: [],
    settingsViewOpen: false,
    queueThumbnailsByIds: vi.fn(),
    setAssets: vi.fn(),
    refresh,
    refreshKnownTags: vi.fn(async () => [])
  };
}

function select(result: { current: ReturnType<typeof useBulkSelectionController> }, id: number, index: number, ctrlLike = false) {
  act(() => result.current.onBulkSelectionInteraction({
    assetId: id, assetIndex: index, ctrlLike, shift: false, viaDrag: false
  }));
}

describe("useBulkSelectionController", () => {
  beforeEach(() => {
    apiMocks.getAssetDetails.mockReset().mockResolvedValue(null);
    apiMocks.setAssetTags.mockReset();
    actionMocks.applyBulkTagsAction.mockReset();
  });

  it("ignores stale tag details after the single selection changes", async () => {
    const assets = [createAsset(1), createAsset(2)];
    const firstDetails = deferred<AssetDetails>();
    apiMocks.getAssetDetails.mockImplementation((assetId: number) =>
      assetId === 1 ? firstDetails.promise : Promise.resolve(createDetails(assets[1]!, ["dog"]))
    );
    const { result } = renderHook(() => useBulkSelectionController(options(assets)));
    act(() => result.current.onToggleSelectionMode());
    select(result, 1, 0);
    await waitFor(() => expect(apiMocks.getAssetDetails).toHaveBeenCalledWith(1));
    select(result, 2, 1);
    await waitFor(() => expect(result.current.singleAssetTags).toEqual(["dog"]));
    await act(async () => {
      firstDetails.resolve(createDetails(assets[0]!, ["cat"]));
      await firstDetails.promise;
    });
    expect(result.current.singleAssetTags).toEqual(["dog"]);
  });

  it("does not start bulk IPC while lightbox owns the asset lock and retries from canonical tags", async () => {
    const asset = createAsset(1);
    apiMocks.getAssetDetails.mockResolvedValueOnce({ ...asset, tags: ["cat"] });
    apiMocks.setAssetTags.mockImplementation(async (assetId: number, tags: string[]) => ({
      asset_id: assetId, changed: true, tags, revision: 3
    }));
    const { result } = renderHook(() => {
      const assetTagState = useAssetTagState();
      const controller = useBulkSelectionController({ ...options([asset]), assetTagState });
      return { assetTagState, controller };
    });
    act(() => result.current.controller.onToggleSelectionMode());
    act(() => result.current.controller.onBulkSelectionInteraction({
      assetId: 1, assetIndex: 0, ctrlLike: false, shift: false, viaDrag: false
    }));
    await waitFor(() => expect(result.current.controller.singleAssetTags).toEqual(["cat"]));
    const lightboxToken = result.current.assetTagState.beginMutation(1);
    expect(lightboxToken).not.toBeNull();

    let added = true;
    await act(async () => { added = await result.current.controller.onAddTag("dog"); });
    expect(added).toBe(false);
    expect(apiMocks.setAssetTags).not.toHaveBeenCalled();
    expect(result.current.controller.tagSaveFailed).toBe(true);

    act(() => {
      result.current.assetTagState.settleMutation(lightboxToken!, ["cat", "lightbox"]);
    });
    await waitFor(() => expect(result.current.controller.singleAssetTags).toEqual(["cat", "lightbox"]));
    await act(async () => { added = await result.current.controller.onAddTag("dog"); });
    expect(added).toBe(true);
    expect(apiMocks.setAssetTags).toHaveBeenCalledTimes(1);
    expect(apiMocks.setAssetTags).toHaveBeenCalledWith(1, ["cat", "lightbox", "dog"]);
  });

  it("blocks single-asset replacement after details failure and retries details", async () => {
    const asset = createAsset(1);
    apiMocks.getAssetDetails
      .mockRejectedValueOnce(new Error("load failed"))
      .mockResolvedValueOnce({ ...asset, tags: ["cat"] });
    apiMocks.setAssetTags.mockResolvedValue({ asset_id: 1, changed: true, tags: ["cat", "dog"], revision: 2 });
    const { result } = renderHook(() => useBulkSelectionController(options([asset])));
    act(() => result.current.onToggleSelectionMode());
    select(result, 1, 0);
    await waitFor(() => expect(result.current.tagDetailsFailed).toBe(true));

    let added = true;
    await act(async () => { added = await result.current.onAddTag("dog"); });
    expect(added).toBe(false);
    expect(apiMocks.setAssetTags).not.toHaveBeenCalled();

    act(() => result.current.onRetryTagDetails());
    await waitFor(() => expect(result.current.singleAssetTags).toEqual(["cat"]));
    await act(async () => { added = await result.current.onAddTag("dog"); });
    expect(added).toBe(true);
    expect(apiMocks.setAssetTags).toHaveBeenCalledWith(1, ["cat", "dog"]);
  });

  it("treats a null details response as an unavailable base", async () => {
    const asset = createAsset(1);
    apiMocks.getAssetDetails.mockResolvedValueOnce(null);
    const { result } = renderHook(() => useBulkSelectionController(options([asset])));
    act(() => result.current.onToggleSelectionMode());
    select(result, 1, 0);
    await waitFor(() => expect(result.current.tagDetailsFailed).toBe(true));
    await act(async () => expect(result.current.onAddTag("dog")).resolves.toBe(false));
    expect(apiMocks.setAssetTags).not.toHaveBeenCalled();
  });

  it("does not cache details that finish after the asset was deleted", async () => {
    const asset = createAsset(1);
    const pendingDetails = deferred<AssetDetails | null>();
    apiMocks.getAssetDetails.mockReturnValueOnce(pendingDetails.promise);
    const { result } = renderHook(() => {
      const assetTagState = useAssetTagState();
      const controller = useBulkSelectionController({ ...options([asset]), assetTagState });
      return { assetTagState, controller };
    });
    act(() => result.current.controller.onToggleSelectionMode());
    act(() => result.current.controller.onBulkSelectionInteraction({
      assetId: 1, assetIndex: 0, ctrlLike: false, shift: false, viaDrag: false
    }));
    await waitFor(() => expect(apiMocks.getAssetDetails).toHaveBeenCalledWith(1));
    act(() => result.current.assetTagState.remove(1));
    await act(async () => {
      pendingDetails.resolve({ ...createDetails(asset, ["stale"]) });
      await pendingDetails.promise;
    });

    expect(result.current.assetTagState.get(1)).toBeNull();
    expect(result.current.controller.singleAssetTags).toEqual([]);
    expect(result.current.controller.tagDetailsFailed).toBe(true);
  });

  it("clears its details cache and selection when the library identity resets", async () => {
    const asset = createAsset(1);
    apiMocks.getAssetDetails
      .mockResolvedValueOnce({ ...asset, tags: ["old"] })
      .mockResolvedValueOnce({ ...asset, path: "C:/media/reused.jpg", tags: ["new"] });
    const { result } = renderHook(() => {
      const assetTagState = useAssetTagState();
      const controller = useBulkSelectionController({ ...options([asset]), assetTagState });
      return { assetTagState, controller };
    });
    act(() => result.current.controller.onToggleSelectionMode());
    act(() => result.current.controller.onBulkSelectionInteraction({
      assetId: 1, assetIndex: 0, ctrlLike: false, shift: false, viaDrag: false
    }));
    await waitFor(() => expect(result.current.controller.singleAssetTags).toEqual(["old"]));

    act(() => result.current.assetTagState.reset());
    await waitFor(() => expect(result.current.controller.selectedAssets).toEqual([]));
    act(() => result.current.controller.onBulkSelectionInteraction({
      assetId: 1, assetIndex: 0, ctrlLike: false, shift: false, viaDrag: false
    }));
    await waitFor(() => expect(result.current.controller.singleAssetTags).toEqual(["new"]));
    expect(apiMocks.getAssetDetails).toHaveBeenCalledTimes(2);
  });

  it("does not send a multi-asset command unless every selected ID lock is acquired", async () => {
    const assets = [createAsset(1), createAsset(2)];
    actionMocks.applyBulkTagsAction.mockResolvedValue({
      processed_assets: 2, updated_assets: 2, processed_asset_ids: [1, 2], updated_asset_ids: [1, 2],
      results: [
        { asset_id: 1, changed: true, tags: ["travel"] },
        { asset_id: 2, changed: true, tags: ["travel"] }
      ],
      revision: 2
    });
    const { result } = renderHook(() => {
      const assetTagState = useAssetTagState();
      const controller = useBulkSelectionController({ ...options(assets), assetTagState });
      return { assetTagState, controller };
    });
    act(() => result.current.controller.onToggleSelectionMode());
    act(() => result.current.controller.onBulkSelectionInteraction({
      assetId: 1, assetIndex: 0, ctrlLike: false, shift: false, viaDrag: false
    }));
    act(() => result.current.controller.onBulkSelectionInteraction({
      assetId: 2, assetIndex: 1, ctrlLike: true, shift: false, viaDrag: false
    }));
    const otherOwner = result.current.assetTagState.beginMutation(2);
    expect(otherOwner).not.toBeNull();

    await act(async () => expect(result.current.controller.onAddTag("travel")).resolves.toBe(false));
    expect(actionMocks.applyBulkTagsAction).not.toHaveBeenCalled();
    const releasedFirstId = result.current.assetTagState.beginMutation(1);
    expect(releasedFirstId).not.toBeNull();
    act(() => {
      result.current.assetTagState.settleMutation(releasedFirstId!);
      result.current.assetTagState.settleMutation(otherOwner!);
    });

    await act(async () => expect(result.current.controller.onAddTag("travel")).resolves.toBe(true));
    expect(actionMocks.applyBulkTagsAction).toHaveBeenCalledTimes(1);
    expect(actionMocks.applyBulkTagsAction).toHaveBeenCalledWith({
      assetIds: [1, 2], tags: ["travel"], refreshKnownTags: expect.any(Function)
    });
  });

  it("does not confirm or clear a bulk draft when no assets were processed", async () => {
    const assets = [createAsset(1), createAsset(2)];
    const refresh = vi.fn(async () => {});
    actionMocks.applyBulkTagsAction.mockResolvedValue({
      processed_assets: 0, updated_assets: 0, processed_asset_ids: [], updated_asset_ids: [],
      results: [], revision: 1
    });
    const { result } = renderHook(() => useBulkSelectionController(options(assets, refresh)));
    act(() => result.current.onToggleSelectionMode());
    select(result, 1, 0);
    select(result, 2, 1, true);
    let added = true;
    await act(async () => { added = await result.current.onAddTag("travel"); });
    expect(added).toBe(false);
    expect(result.current.appliedBulkTags).toEqual([]);
    expect(refresh).toHaveBeenCalledTimes(1);
  });

  it("publishes canonical partial bulk results only for returned asset IDs", async () => {
    const assets = [createAsset(1), createAsset(2)];
    actionMocks.applyBulkTagsAction.mockResolvedValue({
      processed_assets: 1, updated_assets: 0, processed_asset_ids: [1], updated_asset_ids: [],
      results: [{ asset_id: 1, changed: false, tags: ["travel"] }], revision: 1
    });
    const { result } = renderHook(() => {
      const assetTagState = useAssetTagState();
      const controller = useBulkSelectionController({ ...options(assets), assetTagState });
      return { assetTagState, controller };
    });
    act(() => result.current.controller.onToggleSelectionMode());
    act(() => result.current.controller.onBulkSelectionInteraction({
      assetId: 1, assetIndex: 0, ctrlLike: false, shift: false, viaDrag: false
    }));
    act(() => result.current.controller.onBulkSelectionInteraction({
      assetId: 2, assetIndex: 1, ctrlLike: true, shift: false, viaDrag: false
    }));
    let added = false;
    await act(async () => { added = await result.current.controller.onAddTag("travel"); });
    expect(added).toBe(true);
    expect(result.current.assetTagState.get(1)?.tags).toEqual(["travel"]);
    expect(result.current.assetTagState.get(2)).toBeNull();
  });

  it("starts barriers for every bulk ID before the API and settles partial results safely", async () => {
    const assets = [createAsset(1), createAsset(2)];
    const pending = deferred<{
      processed_assets: number; updated_assets: number; processed_asset_ids: number[];
      updated_asset_ids: number[]; results: { asset_id: number; changed: boolean; tags: string[] }[];
      revision: number;
    }>();
    actionMocks.applyBulkTagsAction.mockReturnValueOnce(pending.promise);
    const { result } = renderHook(() => {
      const assetTagState = useAssetTagState();
      const controller = useBulkSelectionController({ ...options(assets), assetTagState });
      return { assetTagState, controller };
    });
    act(() => result.current.controller.onToggleSelectionMode());
    act(() => result.current.controller.onBulkSelectionInteraction({
      assetId: 1, assetIndex: 0, ctrlLike: false, shift: false, viaDrag: false
    }));
    act(() => result.current.controller.onBulkSelectionInteraction({
      assetId: 2, assetIndex: 1, ctrlLike: true, shift: false, viaDrag: false
    }));
    const oldRead1 = result.current.assetTagState.captureGeneration(1);
    const oldRead2 = result.current.assetTagState.captureGeneration(2);
    let addPromise!: Promise<boolean>;
    act(() => { addPromise = result.current.controller.onAddTag("travel"); });

    expect(result.current.assetTagState.publishDetails(1, ["stale-1"], oldRead1)).toBe(false);
    expect(result.current.assetTagState.publishDetails(2, ["stale-2"], oldRead2)).toBe(false);

    await act(async () => {
      pending.resolve({
        processed_assets: 1, updated_assets: 1, processed_asset_ids: [1], updated_asset_ids: [1],
        results: [{ asset_id: 1, changed: true, tags: ["travel"] }], revision: 2
      });
      await addPromise;
    });
    expect(result.current.assetTagState.get(1)?.tags).toEqual(["travel"]);
    expect(result.current.assetTagState.get(2)).toBeNull();
    expect(result.current.assetTagState.publishDetails(2, ["still-stale"], oldRead2)).toBe(false);
  });

  it("keeps durable bulk success feedback when partial-result refresh fails", async () => {
    const assets = [createAsset(1), createAsset(2)];
    const refresh = vi.fn(async () => { throw new Error("secondary refresh failed"); });
    actionMocks.applyBulkTagsAction.mockResolvedValue({
      processed_assets: 1,
      updated_assets: 1,
      processed_asset_ids: [1],
      updated_asset_ids: [1],
      results: [{ asset_id: 1, changed: true, tags: ["travel"] }],
      revision: 2
    });
    const { result } = renderHook(() => useBulkSelectionController(options(assets, refresh)));
    act(() => result.current.onToggleSelectionMode());
    select(result, 1, 0);
    select(result, 2, 1, true);
    await waitFor(() => expect(result.current.selectedAssets).toHaveLength(2));
    let added = false;
    await act(async () => { added = await result.current.onAddTag("travel"); });
    expect(refresh).toHaveBeenCalledTimes(1);
    expect(added).toBe(true);
    expect(result.current.appliedBulkTags).toEqual(["travel"]);
    expect(result.current.tagSaveFailed).toBe(false);
  });

  it("selects the exact global Shift range through the ID resolver", async () => {
    const assets = [createAsset(10), createAsset(11)];
    const ranges: Array<[number, number]> = [];
    const getIdsRangeAsync = vi.fn(async (from: number, to: number) => {
      ranges.push([from, to]);
      return [30, 31, 32];
    });
    const { result } = renderHook(() =>
      useBulkSelectionController({ ...options(assets), getIdsRangeAsync })
    );
    act(() => result.current.onToggleSelectionMode());
    act(() => result.current.onBulkSelectionInteraction({
      assetId: 10, assetIndex: 0, ctrlLike: false, shift: false, viaDrag: false
    }));
    await act(async () => {
      result.current.onBulkSelectionInteraction({
        assetId: 99, assetIndex: 2, ctrlLike: false, shift: true, viaDrag: false
      });
    });
    expect(ranges).toEqual([[0, 2]]);
    expect([...result.current.selectedAssetIds].sort()).toEqual([30, 31, 32]);
  });

  it("ignores a Shift range that resolves after the query epoch changes", async () => {
    const range = deferred<number[]>();
    const assets = [createAsset(1), createAsset(2)];
    const { result, rerender } = renderHook(
      ({ queryEpoch }: { queryEpoch: number }) =>
        useBulkSelectionController({
          ...options(assets),
          queryEpoch,
          getIdsRangeAsync: () => range.promise
        }),
      { initialProps: { queryEpoch: 1 } }
    );
    act(() => result.current.onToggleSelectionMode());
    select(result, 1, 0);
    act(() => result.current.onBulkSelectionInteraction({
      assetId: 2, assetIndex: 1, ctrlLike: false, shift: true, viaDrag: false
    }));
    rerender({ queryEpoch: 2 });
    await act(async () => {
      range.resolve([1, 2]);
      await range.promise;
    });
    expect(result.current.selectedAssetIds).toEqual(new Set([1]));
  });

  it("ignores a Shift range that resolves after a newer plain selection", async () => {
    const range = deferred<number[]>();
    const assets = [createAsset(1), createAsset(2), createAsset(3)];
    const { result } = renderHook(() =>
      useBulkSelectionController({
        ...options(assets),
        getIdsRangeAsync: () => range.promise
      })
    );
    act(() => result.current.onToggleSelectionMode());
    select(result, 1, 0);
    act(() => result.current.onBulkSelectionInteraction({
      assetId: 2, assetIndex: 1, ctrlLike: false, shift: true, viaDrag: false
    }));
    select(result, 3, 2);
    await act(async () => {
      range.resolve([1, 2]);
      await range.promise;
    });
    expect(result.current.selectedAssetIds).toEqual(new Set([3]));
  });

  it("ignores a Shift range that resolves after selection mode is disabled", async () => {
    const range = deferred<number[]>();
    const assets = [createAsset(1), createAsset(2)];
    const { result } = renderHook(() =>
      useBulkSelectionController({
        ...options(assets),
        getIdsRangeAsync: () => range.promise
      })
    );
    act(() => result.current.onToggleSelectionMode());
    select(result, 1, 0);
    act(() => result.current.onBulkSelectionInteraction({
      assetId: 2, assetIndex: 1, ctrlLike: false, shift: true, viaDrag: false
    }));
    act(() => result.current.onToggleSelectionMode());
    await act(async () => {
      range.resolve([1, 2]);
      await range.promise;
    });
    expect(result.current.selectionModeEnabled).toBe(false);
    expect(result.current.selectedAssetIds).toEqual(new Set());
  });

  it("refreshes a filtered query after removing its tag", async () => {
    const asset = createAsset(1);
    const refresh = vi.fn(async () => {});
    apiMocks.getAssetDetails.mockResolvedValueOnce(createDetails(asset, ["cat"]));
    apiMocks.setAssetTags.mockResolvedValueOnce({
      asset_id: 1, changed: true, tags: [], revision: 2
    });
    const { result } = renderHook(() =>
      useBulkSelectionController({ ...options([asset], refresh), appliedFilterTags: ["cat"] })
    );
    act(() => result.current.onToggleSelectionMode());
    select(result, 1, 0);
    await waitFor(() => expect(result.current.singleAssetTags).toEqual(["cat"]));
    await act(async () => {
      await result.current.onRemoveTag("cat");
    });
    expect(refresh).toHaveBeenCalledTimes(1);
  });

  it("keeps the selection when render-cache pages evict assets", () => {
    const initialAssets = [createAsset(1), createAsset(2), createAsset(3)];
    const { result, rerender } = renderHook(
      ({ assets }: { assets: AssetSummary[] }) =>
        useBulkSelectionController(options(assets)),
      { initialProps: { assets: initialAssets } }
    );
    act(() => result.current.onToggleSelectionMode());
    select(result, 1, 0);
    select(result, 3, 2, true);
    expect(result.current.selectedAssetIds.size).toBe(2);

    // Eviction: only one summary remains available for rendering.
    rerender({ assets: [createAsset(3)] });
    expect(result.current.selectedAssetIds).toEqual(new Set([1, 3]));
    // Renderable selection is the intersection with loaded summaries.
    expect(result.current.selectedAssets.map((asset) => asset.id)).toEqual([3]);
  });

  it("degrades a Shift press without a valid anchor to a plain selection after a new query epoch", async () => {
    const assets = [createAsset(1), createAsset(2)];
    const getIdsRangeAsync = vi.fn(async () => [1, 2]);
    const { result, rerender } = renderHook(
      ({ assets, queryEpoch }: { assets: AssetSummary[]; queryEpoch: number }) =>
        useBulkSelectionController({ ...options(assets), queryEpoch, getIdsRangeAsync }),
      { initialProps: { assets, queryEpoch: 1 } }
    );
    act(() => result.current.onToggleSelectionMode());
    select(result, 1, 0);
    rerender({ assets: [createAsset(9), createAsset(1)], queryEpoch: 2 });
    await act(async () => {
      result.current.onBulkSelectionInteraction({
        assetId: 9, assetIndex: 2, ctrlLike: false, shift: true, viaDrag: false
      });
    });
    expect(getIdsRangeAsync).not.toHaveBeenCalled();
    expect([...result.current.selectedAssetIds]).toEqual([9]);
  });
});
