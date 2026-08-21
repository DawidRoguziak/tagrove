import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Asset } from "../../types";
import { useSelectionState } from "../useSelectionState";
import { useAssetTagState } from "../../components/app/hooks/useAssetTagState";

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: Error) => void;
  const promise = new Promise<T>((onResolve, onReject) => {
    resolve = onResolve;
    reject = onReject;
  });
  return { promise, resolve, reject };
}

function createAsset(partial: Partial<Asset> & { id: number }): Asset {
  return {
    id: partial.id,
    path: partial.path ?? `C:/library/${partial.id}.jpg`,
    kind: partial.kind ?? "image",
    size_bytes: partial.size_bytes ?? 100,
    modified_at: partial.modified_at ?? partial.id,
    width: partial.width ?? 100,
    height: partial.height ?? 100,
    duration_ms: partial.duration_ms ?? null,
    thumb_path: partial.thumb_path ?? null,
    is_favorite: partial.is_favorite ?? false,
    media_group_key: partial.media_group_key ?? null,
    media_group_order: partial.media_group_order ?? null,
    tags: partial.tags ?? []
  };
}

const serviceMocks = vi.hoisted(() => ({
  saveLightboxTagsAction: vi.fn(),
  toggleLightboxFavoriteAction: vi.fn(),
  saveLightboxMediaGroupAction: vi.fn(),
  deleteLightboxAssetAction: vi.fn(),
  selectNextLightboxAssetAction: vi.fn(),
  selectPreviousLightboxAssetAction: vi.fn()
}));

const apiMocks = vi.hoisted(() => ({
  getAssetDetails: vi.fn()
}));

vi.mock("../../api", async () => ({
  ...(await vi.importActual<typeof import("../../api")>("../../api")),
  ...apiMocks
}));

vi.mock("../../components/lightbox/services/saveLightboxTagsAction", () => ({
  saveLightboxTagsAction: serviceMocks.saveLightboxTagsAction
}));

vi.mock("../../components/lightbox/services/toggleLightboxFavoriteAction", () => ({
  toggleLightboxFavoriteAction: serviceMocks.toggleLightboxFavoriteAction
}));

vi.mock("../../components/lightbox/services/saveLightboxMediaGroupAction", () => ({
  saveLightboxMediaGroupAction: serviceMocks.saveLightboxMediaGroupAction
}));

vi.mock("../../components/lightbox/services/deleteLightboxAssetAction", () => ({
  deleteLightboxAssetAction: serviceMocks.deleteLightboxAssetAction
}));

vi.mock("../../components/lightbox/services/selectNextLightboxAssetAction", () => ({
  selectNextLightboxAssetAction: serviceMocks.selectNextLightboxAssetAction
}));

vi.mock("../../components/lightbox/services/selectPreviousLightboxAssetAction", () => ({
  selectPreviousLightboxAssetAction: serviceMocks.selectPreviousLightboxAssetAction
}));

describe("useSelectionState", () => {
  const setAssets = vi.fn();
  const refresh = vi.fn(async () => {});
  const refreshKnownTags = vi.fn(async () => ["tag"]);

  beforeEach(() => {
    setAssets.mockReset();
    refresh.mockClear();
    refreshKnownTags.mockClear();
    serviceMocks.saveLightboxTagsAction.mockReset().mockResolvedValue(undefined);
    serviceMocks.toggleLightboxFavoriteAction.mockReset().mockResolvedValue(undefined);
    serviceMocks.saveLightboxMediaGroupAction.mockReset().mockResolvedValue(undefined);
    serviceMocks.deleteLightboxAssetAction.mockReset().mockResolvedValue(undefined);
    serviceMocks.selectNextLightboxAssetAction.mockReset();
    serviceMocks.selectPreviousLightboxAssetAction.mockReset();
    apiMocks.getAssetDetails.mockReset().mockResolvedValue(null);
  });

  it("updates editors based on currently selected asset", async () => {
    const selectedAsset = createAsset({
      id: 1,
      tags: ["cat", "travel"],
      media_group_key: "group-a",
      media_group_order: 2
    });

    const { result } = renderHook(() =>
      useSelectionState({
        assets: [selectedAsset],
        setAssets,
        appliedFavoritesOnly: false,
        refresh,
        refreshKnownTags
      })
    );

    act(() => {
      result.current.setSelected(selectedAsset);
    });

    await waitFor(() => {
      expect(result.current.tagEditor).toEqual(["cat", "travel"]);
    });
    expect(result.current.mediaGroupKeyEditor).toBe("group-a");
    expect(result.current.mediaGroupOrderEditor).toBe("2");

    act(() => {
      result.current.setSelected(null);
    });

    await waitFor(() => {
      expect(result.current.tagEditor).toEqual([]);
      expect(result.current.mediaGroupKeyEditor).toBe("");
      expect(result.current.mediaGroupOrderEditor).toBe("");
    });
  });

  it("syncs selected object with latest assets and clears when selected asset disappears", async () => {
    const initialAsset = createAsset({ id: 7, tags: ["old"], media_group_key: "legacy" });
    const updatedAsset = createAsset({
      id: 7,
      tags: [],
      media_group_key: "fresh",
      media_group_order: 9
    });

    const { result, rerender } = renderHook(
      ({ assets }: { assets: Asset[] }) =>
        useSelectionState({
          assets,
          setAssets,
          appliedFavoritesOnly: false,
          refresh,
          refreshKnownTags
        }),
      {
        initialProps: {
          assets: [initialAsset]
        }
      }
    );

    act(() => {
      result.current.setSelected(initialAsset);
    });

    rerender({ assets: [updatedAsset] });

    await waitFor(() => {
      expect(result.current.selected).toEqual({ ...updatedAsset, tags: ["old"] });
      expect(result.current.tagEditor).toEqual(["old"]);
      expect(result.current.mediaGroupKeyEditor).toBe("fresh");
      expect(result.current.mediaGroupOrderEditor).toBe("9");
    });

    rerender({ assets: [] });

    await waitFor(() => {
      expect(result.current.selected).toBeNull();
      expect(result.current.tagEditor).toEqual([]);
    });
  });

  it("serializes per-asset saves across selection changes and coalesces to the latest replacement", async () => {
    const first = createAsset({ id: 1, tags: ["one"] });
    const second = createAsset({ id: 2, tags: ["two"] });
    const firstSave = deferred<void>();
    serviceMocks.saveLightboxTagsAction.mockImplementation(({ onSaved }, tags: string[]) => {
      if (serviceMocks.saveLightboxTagsAction.mock.calls.length === 1) {
        return firstSave.promise.then(() => {
          onSaved?.(1, tags);
          return { asset_id: 1, changed: true, tags, revision: 2 };
        });
      }
      return Promise.resolve({ asset_id: 1, changed: true, tags, revision: 3 });
    });

    const { result } = renderHook(() => useSelectionState({
      assets: [first, second], setAssets, appliedFavoritesOnly: false, refresh, refreshKnownTags
    }));
    act(() => result.current.setSelected(first));
    act(() => result.current.saveTags(["one", "a"]));
    act(() => result.current.saveTags(["one", "a", "b"]));
    act(() => result.current.setSelected(second));
    act(() => firstSave.resolve());

    await waitFor(() => expect(serviceMocks.saveLightboxTagsAction).toHaveBeenCalledTimes(2));
    expect(serviceMocks.saveLightboxTagsAction.mock.calls[1]?.[1]).toEqual(["one", "a", "b"]);
    expect(result.current.selected?.id).toBe(2);
  });

  it("retries the latest queued replacement after failure across close and reopen without overlap", async () => {
    const asset = createAsset({ id: 4, tags: ["cat"] });
    const firstSave = deferred<void>();
    const retrySave = deferred<void>();
    let activeSaves = 0;
    let maxActiveSaves = 0;
    serviceMocks.saveLightboxTagsAction.mockImplementation((_args, tags: string[]) => {
      activeSaves += 1;
      maxActiveSaves = Math.max(maxActiveSaves, activeSaves);
      const pending = serviceMocks.saveLightboxTagsAction.mock.calls.length === 1
        ? firstSave.promise
        : retrySave.promise;
      return pending.finally(() => { activeSaves -= 1; }).then(() => ({
        asset_id: 4, changed: true, tags, revision: 2
      }));
    });

    const { result } = renderHook(() => useSelectionState({
      assets: [asset], setAssets, appliedFavoritesOnly: false, refresh, refreshKnownTags
    }));
    act(() => result.current.setSelected(asset));
    act(() => result.current.saveTags(["cat", "dog"]));
    act(() => result.current.saveTags(["cat", "dog", "travel"]));
    act(() => result.current.setSelected(null));
    act(() => result.current.setSelected(asset));
    expect(serviceMocks.saveLightboxTagsAction).toHaveBeenCalledTimes(1);

    act(() => firstSave.reject(new Error("save failed")));
    await waitFor(() => expect(result.current.tagFailed).toBe(true));
    expect(result.current.tagEditor).toEqual(["cat", "dog", "travel"]);
    act(() => result.current.retryTags());
    await waitFor(() => expect(serviceMocks.saveLightboxTagsAction).toHaveBeenCalledTimes(2));
    expect(serviceMocks.saveLightboxTagsAction.mock.calls[1]?.[1]).toEqual(["cat", "dog", "travel"]);
    expect(maxActiveSaves).toBe(1);
    act(() => retrySave.resolve());
    await waitFor(() => expect(result.current.tagFailed).toBe(false));
  });

  it("keeps a successful final-tag removal authoritative over a late detail response and cached reopen", async () => {
    const asset = createAsset({ id: 5, tags: ["old"] });
    const lateDetails = deferred<Asset>();
    apiMocks.getAssetDetails.mockReturnValueOnce(lateDetails.promise);
    serviceMocks.saveLightboxTagsAction.mockImplementation(async ({ onSaved }, tags: string[]) => {
      onSaved?.(5, tags);
      return { asset_id: 5, changed: true, tags, revision: 2 };
    });

    const { result } = renderHook(() => useSelectionState({
      assets: [asset], setAssets, appliedFavoritesOnly: false, refresh, refreshKnownTags
    }));
    act(() => result.current.setSelected(asset));
    await waitFor(() => expect(apiMocks.getAssetDetails).toHaveBeenCalledWith(5));
    act(() => result.current.saveTags([]));
    await waitFor(() => expect(serviceMocks.saveLightboxTagsAction).toHaveBeenCalledTimes(1));
    act(() => lateDetails.resolve({ ...asset, tags: ["old"] }));
    await act(async () => { await lateDetails.promise; });
    act(() => result.current.setSelected(null));
    act(() => result.current.setSelected(asset));
    await waitFor(() => expect(result.current.tagEditor).toEqual([]));
  });

  it("blocks tag replacement until details establish an authoritative base", async () => {
    const asset = createAsset({ id: 20, tags: [] });
    const details = deferred<Asset | null>();
    apiMocks.getAssetDetails.mockReturnValueOnce(details.promise);
    const { result } = renderHook(() => useSelectionState({
      assets: [asset], setAssets, appliedFavoritesOnly: false, refresh, refreshKnownTags
    }));

    act(() => result.current.setSelected(asset));
    expect(result.current.tagDetailsLoading).toBe(true);
    act(() => result.current.saveTags(["new"]));
    expect(serviceMocks.saveLightboxTagsAction).not.toHaveBeenCalled();

    act(() => details.resolve({ ...asset, tags: ["existing"] }));
    await waitFor(() => expect(result.current.tagDetailsLoading).toBe(false));
    act(() => result.current.saveTags(["existing", "new"]));
    expect(serviceMocks.saveLightboxTagsAction).toHaveBeenCalledTimes(1);
  });

  it("keeps editing blocked after details failure and retry unlocks it", async () => {
    const asset = createAsset({ id: 21, tags: [] });
    apiMocks.getAssetDetails
      .mockRejectedValueOnce(new Error("load failed"))
      .mockResolvedValueOnce({ ...asset, tags: ["existing"] });
    const { result } = renderHook(() => useSelectionState({
      assets: [asset], setAssets, appliedFavoritesOnly: false, refresh, refreshKnownTags
    }));

    act(() => result.current.setSelected(asset));
    await waitFor(() => expect(result.current.tagDetailsFailed).toBe(true));
    act(() => result.current.saveTags(["new"]));
    expect(serviceMocks.saveLightboxTagsAction).not.toHaveBeenCalled();

    act(() => result.current.retryTagDetails());
    await waitFor(() => expect(result.current.tagEditor).toEqual(["existing"]));
    expect(result.current.tagDetailsFailed).toBe(false);
  });

  it("applies shared canonical tags and ignores details that predate a bulk mutation", async () => {
    const asset = createAsset({ id: 22, tags: [] });
    const details = deferred<Asset | null>();
    apiMocks.getAssetDetails.mockReturnValueOnce(details.promise);
    const { result } = renderHook(() => {
      const assetTagState = useAssetTagState();
      const selection = useSelectionState({
        assets: [asset], setAssets, appliedFavoritesOnly: false, refresh, refreshKnownTags,
        assetTagState
      });
      return { assetTagState, selection };
    });

    act(() => result.current.selection.setSelected(asset));
    act(() => {
      const token = result.current.assetTagState.beginMutation(22);
      expect(token).not.toBeNull();
      result.current.assetTagState.settleMutation(token!, ["bulk"]);
    });
    await waitFor(() => expect(result.current.selection.tagEditor).toEqual(["bulk"]));
    expect(result.current.selection.tagDetailsLoading).toBe(false);

    act(() => details.resolve({ ...asset, tags: ["stale"] }));
    await act(async () => { await details.promise; });
    expect(result.current.selection.tagEditor).toEqual(["bulk"]);
    expect(result.current.selection.selected?.tags).toEqual(["bulk"]);
  });

  it("does not start lightbox IPC while bulk owns the asset lock and safely rebases Retry", async () => {
    const asset = createAsset({ id: 26, tags: ["cat"] });
    const { result } = renderHook(() => {
      const assetTagState = useAssetTagState();
      const selection = useSelectionState({
        assets: [asset], setAssets, appliedFavoritesOnly: false, refresh, refreshKnownTags,
        assetTagState
      });
      return { assetTagState, selection };
    });
    act(() => result.current.selection.setSelected(asset));
    const bulkToken = result.current.assetTagState.beginMutation(26);
    expect(bulkToken).not.toBeNull();

    act(() => result.current.selection.saveTags(["cat", "dog"]));
    await waitFor(() => expect(result.current.selection.tagFailed).toBe(true));
    expect(serviceMocks.saveLightboxTagsAction).not.toHaveBeenCalled();

    act(() => {
      result.current.assetTagState.settleMutation(bulkToken!, ["cat", "bulk"]);
    });
    await waitFor(() => {
      expect(result.current.selection.tagEditor).toEqual(["cat", "bulk", "dog"]);
    });
    act(() => result.current.selection.retryTags());
    expect(serviceMocks.saveLightboxTagsAction).toHaveBeenCalledTimes(1);
    expect(serviceMocks.saveLightboxTagsAction.mock.calls[0]?.[1]).toEqual(["cat", "bulk", "dog"]);
  });

  it("retains the lightbox draft for Retry when maintenance denies mutation ownership", async () => {
    const asset = createAsset({ id: 27, tags: ["cat"] });
    const maintenanceWork = deferred<void>();
    const { result } = renderHook(() => {
      const assetTagState = useAssetTagState();
      const selection = useSelectionState({
        assets: [asset], setAssets, appliedFavoritesOnly: false, refresh, refreshKnownTags,
        assetTagState
      });
      return { assetTagState, selection };
    });
    act(() => result.current.selection.setSelected(asset));
    const maintenance = result.current.assetTagState.runWithMutationBarrier(() => maintenanceWork.promise);

    act(() => result.current.selection.saveTags(["cat", "dog"]));
    await waitFor(() => expect(result.current.selection.tagFailed).toBe(true));
    expect(result.current.selection.tagEditor).toEqual(["cat", "dog"]);
    expect(serviceMocks.saveLightboxTagsAction).not.toHaveBeenCalled();

    await act(async () => {
      maintenanceWork.resolve();
      await maintenance;
    });
    act(() => result.current.selection.retryTags());
    expect(serviceMocks.saveLightboxTagsAction).toHaveBeenCalledTimes(1);
    expect(serviceMocks.saveLightboxTagsAction.mock.calls[0]?.[1]).toEqual(["cat", "dog"]);
  });

  it("does not let details publish while a lightbox mutation is pending", async () => {
    const asset = createAsset({ id: 23, tags: ["cat"] });
    const lateDetails = deferred<Asset | null>();
    const pendingSave = deferred<void>();
    apiMocks.getAssetDetails.mockReturnValueOnce(lateDetails.promise);
    serviceMocks.saveLightboxTagsAction.mockImplementation(({ onSaved }, tags: string[]) =>
      pendingSave.promise.then(() => {
        onSaved?.(23, tags);
        return { asset_id: 23, changed: true, tags, revision: 2 };
      })
    );
    const { result } = renderHook(() => {
      const assetTagState = useAssetTagState();
      const selection = useSelectionState({
        assets: [asset], setAssets, appliedFavoritesOnly: false, refresh, refreshKnownTags,
        assetTagState
      });
      return { assetTagState, selection };
    });

    act(() => result.current.selection.setSelected(asset));
    act(() => result.current.selection.saveTags(["cat", "dog"]));
    act(() => lateDetails.resolve({ ...asset, tags: ["stale"] }));
    await act(async () => { await lateDetails.promise; });

    expect(result.current.selection.tagEditor).toEqual(["cat", "dog"]);
    expect(result.current.assetTagState.get(23)?.tags).toEqual(["cat"]);

    act(() => pendingSave.resolve());
    await waitFor(() => expect(result.current.selection.selected?.tags).toEqual(["cat", "dog"]));
  });

  it("clears local caches on identity reset and reloads a reused asset ID", async () => {
    const asset = createAsset({ id: 24, tags: [] });
    const oldDetails = deferred<Asset | null>();
    apiMocks.getAssetDetails
      .mockReturnValueOnce(oldDetails.promise)
      .mockResolvedValueOnce({ ...asset, path: "C:/library/reused.jpg", tags: ["new"] });
    const { result } = renderHook(() => {
      const assetTagState = useAssetTagState();
      const selection = useSelectionState({
        assets: [asset], setAssets, appliedFavoritesOnly: false, refresh, refreshKnownTags,
        assetTagState
      });
      return { assetTagState, selection };
    });

    act(() => result.current.selection.setSelected(asset));
    await waitFor(() => expect(apiMocks.getAssetDetails).toHaveBeenCalledTimes(1));
    act(() => result.current.assetTagState.reset());
    await waitFor(() => expect(result.current.selection.selected).toBeNull());

    act(() => result.current.selection.setSelected({ ...asset, path: "C:/library/reused.jpg" }));
    await waitFor(() => expect(result.current.selection.tagEditor).toEqual(["new"]));
    act(() => oldDetails.resolve({ ...asset, tags: ["old"] }));
    await act(async () => { await oldDetails.promise; });

    expect(result.current.selection.selected?.path).toBe("C:/library/reused.jpg");
    expect(result.current.selection.tagEditor).toEqual(["new"]);
  });

  it("invalidates pending details as soon as deletion succeeds", async () => {
    const asset = createAsset({ id: 25, tags: [] });
    const pendingDetails = deferred<Asset | null>();
    apiMocks.getAssetDetails.mockReturnValueOnce(pendingDetails.promise);
    serviceMocks.deleteLightboxAssetAction.mockImplementation(async ({ onDeleted }) => {
      onDeleted?.(25);
    });
    const { result } = renderHook(() => {
      const assetTagState = useAssetTagState();
      const selection = useSelectionState({
        assets: [asset], setAssets, appliedFavoritesOnly: false, refresh, refreshKnownTags,
        assetTagState
      });
      return { assetTagState, selection };
    });

    act(() => result.current.selection.setSelected(asset));
    await act(async () => { await result.current.selection.deleteSelectedAsset(); });
    act(() => pendingDetails.resolve({ ...asset, tags: ["stale"] }));
    await act(async () => { await pendingDetails.promise; });

    expect(result.current.selection.selected).toBeNull();
    expect(result.current.assetTagState.get(25)).toBeNull();
  });

  it("accumulates rapid Right presses and only commits the latest response", async () => {
    const assets = [createAsset({ id: 1 }), createAsset({ id: 2 }), createAsset({ id: 3 })];
    const secondAsset = deferred<Asset | undefined>();
    const thirdAsset = deferred<Asset | undefined>();
    const getAssetAtAsync = vi.fn((index: number) => {
      if (index === 1) return secondAsset.promise;
      if (index === 2) return thirdAsset.promise;
      return Promise.resolve(assets[index]);
    });
    const { result } = renderHook(() => useSelectionState({
      assets,
      setAssets,
      appliedFavoritesOnly: false,
      refresh,
      refreshKnownTags,
      assetCount: assets.length,
      getAssetAtAsync
    }));
    act(() => result.current.setSelected(assets[0]));

    act(() => {
      result.current.handleSelectNext();
      result.current.handleSelectNext();
    });
    expect(getAssetAtAsync).toHaveBeenNthCalledWith(1, 1);
    expect(getAssetAtAsync).toHaveBeenNthCalledWith(2, 2);

    act(() => thirdAsset.resolve(assets[2]));
    await waitFor(() => expect(result.current.selected?.id).toBe(3));
    act(() => secondAsset.resolve(assets[1]));
    await act(async () => { await secondAsset.promise; });
    expect(result.current.selected?.id).toBe(3);
  });

  it("lets rapid Right then Left return to the original target despite reverse responses", async () => {
    const assets = [createAsset({ id: 1 }), createAsset({ id: 2 }), createAsset({ id: 3 })];
    const originalAsset = deferred<Asset | undefined>();
    const secondAsset = deferred<Asset | undefined>();
    const getAssetAtAsync = vi.fn((index: number) => {
      if (index === 0) return originalAsset.promise;
      if (index === 1) return secondAsset.promise;
      return Promise.resolve(assets[index]);
    });
    const { result } = renderHook(() => useSelectionState({
      assets,
      setAssets,
      appliedFavoritesOnly: false,
      refresh,
      refreshKnownTags,
      assetCount: assets.length,
      getAssetAtAsync
    }));
    act(() => result.current.setSelected(assets[0]));

    act(() => {
      result.current.handleSelectNext();
      result.current.handleSelectPrevious();
    });
    expect(getAssetAtAsync).toHaveBeenNthCalledWith(1, 1);
    expect(getAssetAtAsync).toHaveBeenNthCalledWith(2, 0);

    act(() => secondAsset.resolve(assets[1]));
    await act(async () => { await secondAsset.promise; });
    expect(result.current.selected?.id).toBe(1);
    act(() => originalAsset.resolve(assets[0]));
    await waitFor(() => expect(result.current.selected?.id).toBe(1));
  });

  it("delegates save, favorite, group and delete actions with current selection context", async () => {
    const first = createAsset({ id: 1, tags: ["one"] });
    const second = createAsset({ id: 2, tags: ["two"] });

    const { result } = renderHook(() =>
      useSelectionState({
        assets: [first, second],
        setAssets,
        appliedFavoritesOnly: true,
        refresh,
        refreshKnownTags
      })
    );

    act(() => {
      result.current.setSelected(first);
    });

    await act(async () => {
      await result.current.saveTags(["new"]);
      await result.current.toggleSelectedFavorite();
      await result.current.saveMediaGroup({ key: "group-1", order: 2 });
      await result.current.deleteSelectedAsset();
    });

    expect(serviceMocks.saveLightboxTagsAction).toHaveBeenCalledWith(
      expect.objectContaining({
        selected: first,
        refreshKnownTags
      }),
      ["new"]
    );

    expect(serviceMocks.toggleLightboxFavoriteAction).toHaveBeenCalledWith(
      expect.objectContaining({
        selected: first,
        appliedFavoritesOnly: true,
        refresh
      })
    );

    expect(serviceMocks.saveLightboxMediaGroupAction).toHaveBeenCalledWith(
      expect.objectContaining({
        selected: first
      }),
      { key: "group-1", order: 2 }
    );

    expect(serviceMocks.deleteLightboxAssetAction).toHaveBeenCalledWith(
      expect.objectContaining({
        selected: first,
        refresh,
        refreshKnownTags
      })
    );

    act(() => {
      result.current.handleSelectNext();
    });
    await waitFor(() => expect(result.current.selected?.id).toBe(second.id));

    act(() => {
      result.current.handleSelectPrevious();
    });
    await waitFor(() => expect(result.current.selected?.id).toBe(first.id));
  });
});
