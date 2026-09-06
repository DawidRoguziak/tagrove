import { useState } from "react";
import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AssetDetails, AssetSummary, SelectedAsset } from "../../types";
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

function createAsset(partial: Partial<SelectedAsset> & { id: number }): SelectedAsset {
  return {
    id: partial.id,
    file_name: partial.file_name ?? `${partial.id}.jpg`,
    preview_path: partial.preview_path ?? null,
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

function createDetails(asset: SelectedAsset, tags: string[]): AssetDetails {
  return asDetails(asset, tags);
}

function asDetails(asset: SelectedAsset, tags: string[]): AssetDetails {
  if (asset.path === null || asset.size_bytes === null) {
    throw new Error("not detail-complete");
  }
  return { ...asset, path: asset.path, size_bytes: asset.size_bytes, tags };
}

const serviceMocks = vi.hoisted(() => ({
  saveLightboxTagsAction: vi.fn(),
  toggleLightboxFavoriteAction: vi.fn(),
  saveLightboxMediaGroupAction: vi.fn(),
  deleteLightboxAssetAction: vi.fn()
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

describe("useSelectionState", () => {
  it("does not let an older details response undo a bulk favorite change", async () => {
    const asset = createAsset({ id: 1, tags: ["cat"] });
    const details = deferred<AssetDetails>();
    apiMocks.getAssetDetails.mockReturnValueOnce(details.promise).mockResolvedValueOnce(createDetails({ ...asset, is_favorite: true }, ["cat"]));
    const { result } = renderHook(() => {
      const [assets, setAssets] = useState<AssetSummary[]>([asset]);
      const assetTagState = useAssetTagState();
      const selection = useSelectionState({
        assets, setAssets, assetTagState, appliedFavoritesOnly: false,
        refresh: vi.fn(async () => {}), refreshKnownTags: vi.fn(async () => []), assetCount: 1
      });
      return { ...selection, setAssets, assetTagState };
    });
    act(() => result.current.setSelected(asset));
    const token = result.current.assetTagState.beginMutation(1);
    expect(token).not.toBeNull();
    act(() => {
      if (token) result.current.assetTagState.settleMutation(token);
      result.current.setAssets([{ ...asset, is_favorite: true }]);
      result.current.applyFavoriteChanges(new Set([1]), true);
    });
    await act(async () => {
      details.resolve(createDetails(asset, ["cat"]));
      await details.promise;
    });
    expect(result.current.selected?.is_favorite).toBe(true);
    expect(result.current.assetDetailsFailed).toBe(false);
  });

  it("keeps bulk favorite changes in the lightbox details cache after reopening", async () => {
    const asset = createAsset({ id: 1 });
    apiMocks.getAssetDetails.mockResolvedValue(createDetails(asset, []));
    const { result } = renderHook(() => {
      const [assets, setAssets] = useState<AssetSummary[]>([asset]);
      const selection = useSelectionState({
        assets, setAssets, appliedFavoritesOnly: false,
        refresh: vi.fn(async () => {}), refreshKnownTags: vi.fn(async () => []), assetCount: 1
      });
      return { ...selection, setAssets };
    });
    act(() => result.current.setSelected(asset));
    await waitFor(() => expect(result.current.assetDetailsFailed).toBe(false));
    await waitFor(() => expect(result.current.tagDetailsLoading).toBe(false));
    act(() => {
      result.current.setAssets([{ ...asset, is_favorite: true }]);
      result.current.applyFavoriteChanges(new Set([1]), true);
    });
    expect(result.current.selected?.is_favorite).toBe(true);
    act(() => result.current.setSelected(null));
    act(() => result.current.setSelected({ ...asset, is_favorite: true }));
    expect(result.current.selected?.is_favorite).toBe(true);
    expect(apiMocks.getAssetDetails).toHaveBeenCalledTimes(1);
  });

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
      ({ assets }: { assets: AssetSummary[] }) =>
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
    const lateDetails = deferred<AssetDetails>();
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
    act(() => lateDetails.resolve(asDetails(asset, ["old"])));
    await act(async () => { await lateDetails.promise; });
    act(() => result.current.setSelected(null));
    act(() => result.current.setSelected(asset));
    await waitFor(() => expect(result.current.tagEditor).toEqual([]));
  });

  it("blocks tag replacement until details establish an authoritative base", async () => {
    const asset = createAsset({ id: 20, tags: [] });
    const details = deferred<AssetDetails | null>();
    apiMocks.getAssetDetails.mockReturnValueOnce(details.promise);
    const { result } = renderHook(() => useSelectionState({
      assets: [asset], setAssets, appliedFavoritesOnly: false, refresh, refreshKnownTags
    }));

    act(() => result.current.setSelected(asset));
    expect(result.current.tagDetailsLoading).toBe(true);
    act(() => result.current.saveTags(["new"]));
    expect(serviceMocks.saveLightboxTagsAction).not.toHaveBeenCalled();

    act(() => details.resolve(asDetails(asset, ["existing"])));
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
    expect(result.current.assetDetailsFailed).toBe(true);
    act(() => result.current.saveTags(["new"]));
    expect(serviceMocks.saveLightboxTagsAction).not.toHaveBeenCalled();

    act(() => result.current.retryTagDetails());
    await waitFor(() => expect(result.current.tagEditor).toEqual(["existing"]));
    expect(result.current.tagDetailsFailed).toBe(false);
    expect(result.current.assetDetailsFailed).toBe(false);
  });

  it("keeps a details failure visible after canonical tags arrive and allows retry", async () => {
    const fullAsset = createAsset({ id: 23, tags: ["canonical"] });
    const { path: _path, size_bytes: _sizeBytes, tags: _tags, ...summary } = fullAsset;
    const firstDetails = deferred<AssetDetails | null>();
    apiMocks.getAssetDetails
      .mockReturnValueOnce(firstDetails.promise)
      .mockResolvedValueOnce(fullAsset);
    const { result } = renderHook(() => {
      const assetTagState = useAssetTagState();
      const selection = useSelectionState({
        assets: [summary], setAssets, appliedFavoritesOnly: false, refresh, refreshKnownTags,
        assetTagState
      });
      return { assetTagState, selection };
    });

    act(() => result.current.selection.setSelected(summary));
    act(() => {
      const token = result.current.assetTagState.beginMutation(23);
      expect(token).not.toBeNull();
      result.current.assetTagState.settleMutation(token!, ["canonical"]);
    });
    act(() => firstDetails.reject(new Error("load failed")));

    await waitFor(() => expect(result.current.selection.assetDetailsFailed).toBe(true));
    expect(result.current.selection.tagDetailsFailed).toBe(false);
    expect(result.current.selection.tagEditor).toEqual(["canonical"]);
    expect(result.current.selection.selected?.path).toBeNull();

    act(() => result.current.selection.retryTagDetails());
    await waitFor(() => expect(result.current.selection.selected?.path).toBe(fullAsset.path));
    expect(result.current.selection.assetDetailsFailed).toBe(false);
  });

  it("applies shared canonical tags and ignores details that predate a bulk mutation", async () => {
    const asset = createAsset({ id: 22, tags: [] });
    const details = deferred<AssetDetails | null>();
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

    act(() => details.resolve(asDetails(asset, ["stale"])));
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
    const lateDetails = deferred<AssetDetails | null>();
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
    act(() => lateDetails.resolve(asDetails(asset, ["stale"])));
    await act(async () => { await lateDetails.promise; });

    expect(result.current.selection.tagEditor).toEqual(["cat", "dog"]);
    expect(result.current.assetTagState.get(23)?.tags).toEqual(["cat"]);

    act(() => pendingSave.resolve());
    await waitFor(() => expect(result.current.selection.selected?.tags).toEqual(["cat", "dog"]));
  });

  it("clears local caches on identity reset and reloads a reused asset ID", async () => {
    const asset = createAsset({ id: 24, tags: [] });
    const oldDetails = deferred<AssetDetails | null>();
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

    act(() => result.current.selection.setSelected(createAsset({ id: 24, path: "C:/library/reused.jpg", tags: [] })));
    await waitFor(() => expect(result.current.selection.tagEditor).toEqual(["new"]));
    act(() => oldDetails.resolve({ ...createDetails(asset, ["old"]) }));
    await act(async () => { await oldDetails.promise; });

    expect(result.current.selection.selected?.path).toBe("C:/library/reused.jpg");
    expect(result.current.selection.tagEditor).toEqual(["new"]);
  });

  it("invalidates pending details as soon as deletion succeeds", async () => {
    const asset = createAsset({ id: 25, tags: [] });
    const pendingDetails = deferred<AssetDetails | null>();
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
    act(() => pendingDetails.resolve({ ...createDetails(asset, ["stale"]) }));
    await act(async () => { await pendingDetails.promise; });

    expect(result.current.selection.selected).toBeNull();
    expect(result.current.assetTagState.get(25)).toBeNull();
  });

  it("accumulates rapid Right presses and only commits the latest response", async () => {
    const assets = [createAsset({ id: 1 }), createAsset({ id: 2 }), createAsset({ id: 3 })];
    const secondAsset = deferred<AssetSummary | undefined>();
    const thirdAsset = deferred<AssetSummary | undefined>();
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
    const originalAsset = deferred<AssetSummary | undefined>();
    const secondAsset = deferred<AssetSummary | undefined>();
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

  it("rolls the navigation target back when the target record is unavailable", async () => {
    const assets = [createAsset({ id: 1 }), createAsset({ id: 2 })];
    let failNext = true;
    const getAssetAtAsync = vi.fn((index: number) =>
      index === 1 && failNext ? Promise.resolve(undefined) : Promise.resolve(assets[index])
    );
    const { result } = renderHook(() => useSelectionState({
      assets, setAssets, appliedFavoritesOnly: false, refresh, refreshKnownTags,
      assetCount: assets.length, getAssetAtAsync
    }));
    act(() => result.current.setSelected(assets[0]!, 0));

    act(() => result.current.handleSelectNext());
    await waitFor(() => expect(getAssetAtAsync).toHaveBeenCalledTimes(1));

    // The record becomes available; the retry must target the same index.
    failNext = false;
    act(() => result.current.handleSelectNext());
    await waitFor(() => expect(result.current.selected?.id).toBe(2));
    expect(getAssetAtAsync).toHaveBeenLastCalledWith(1);
  });

  it("recomputes the lightbox position from a new query session", async () => {
    const initialAssets = [createAsset({ id: 1 }), createAsset({ id: 2 }), createAsset({ id: 3 })];
    const getAssetAtAsync = vi.fn((index: number) => Promise.resolve(initialAssets[index]));
    const { result, rerender } = renderHook(
      ({ assets, queryEpoch }: { assets: AssetSummary[]; queryEpoch: number }) =>
        useSelectionState({
          assets, setAssets, appliedFavoritesOnly: false, refresh, refreshKnownTags,
          assetCount: 3, queryEpoch, getAssetAtAsync
        }),
      { initialProps: { assets: initialAssets, queryEpoch: 1 } }
    );
    act(() => result.current.setSelected(initialAssets[0]!, 0));

    // The fresh session reordered the snapshot: id 1 now sits at global index 2.
    const reordered = [initialAssets[1]!, initialAssets[2]!, initialAssets[0]!];
    rerender({ assets: reordered, queryEpoch: 2 });

    act(() => result.current.handleSelectNext());
    // From the recomputed index 2 the next record is the wrapped index 0,
    // not index 1 from the stale session ordering.
    expect(getAssetAtAsync).toHaveBeenLastCalledWith(0);
  });

  it("re-fetches details after a new query epoch instead of trusting the cache", async () => {
    const asset = createAsset({ id: 40, tags: [] });
    apiMocks.getAssetDetails.mockResolvedValue({ ...asset, tags: ["loaded"] });
    const { result, rerender } = renderHook(
      ({ queryEpoch }: { queryEpoch: number }) =>
        useSelectionState({
          assets: [asset], setAssets, appliedFavoritesOnly: false, refresh, refreshKnownTags, queryEpoch
        }),
      { initialProps: { queryEpoch: 1 } }
    );
    act(() => result.current.setSelected(asset));
    await waitFor(() => expect(apiMocks.getAssetDetails).toHaveBeenCalledTimes(1));

    act(() => result.current.setSelected(null));
    rerender({ queryEpoch: 2 });
    act(() => result.current.setSelected(asset));
    await waitFor(() => expect(apiMocks.getAssetDetails).toHaveBeenCalledTimes(2));
  });
  it("preserves dirty group fields across unrelated tag publications and detail arrival", async () => {
    const asset = createAsset({ id: 1 });
    const pending = deferred<AssetDetails>();
    apiMocks.getAssetDetails.mockReturnValueOnce(pending.promise);
    const { result } = renderHook(() => {
      const coordinator = useAssetTagState();
      return { coordinator, selection: useSelectionState({ assets: [asset], setAssets: vi.fn(),
        appliedFavoritesOnly: false, refresh: vi.fn(async () => {}), refreshKnownTags: vi.fn(async () => []),
        assetTagState: coordinator, assetCount: 1 }) };
    });
    act(() => result.current.selection.selectAsset(asset));
    act(() => result.current.selection.setMediaGroupKeyEditor("unsaved"));
    act(() => result.current.coordinator.publishDetails(2, ["other"], result.current.coordinator.captureGeneration(2)));
    await act(async () => { pending.resolve(createDetails({ ...asset, media_group_key: "canonical", media_group_order: 7 }, [])); });
    expect(result.current.selection.mediaGroupKeyEditor).toBe("unsaved");
    expect(result.current.selection.mediaGroupOrderEditor).toBe("7");
  });

  it("rejects complete details started before a favorite save and refetches", async () => {
    const asset = createAsset({ id: 1, tags: ["cat"] });
    const stale = deferred<AssetDetails>();
    const fresh = deferred<AssetDetails>();
    apiMocks.getAssetDetails.mockReturnValueOnce(stale.promise).mockReturnValueOnce(fresh.promise);
    serviceMocks.toggleLightboxFavoriteAction.mockImplementation(async ({ setSelected, setAssets }) => {
      setSelected((current: SelectedAsset) => ({ ...current, is_favorite: true }));
      setAssets((items: AssetSummary[]) => items.map(item => ({ ...item, is_favorite: true })));
    });
    const { result } = renderHook(() => {
      const [assets, setAssets] = useState<AssetSummary[]>([asset]);
      return useSelectionState({ assets, setAssets, appliedFavoritesOnly: false, assetCount: 1,
        refresh: vi.fn(async () => {}), refreshKnownTags: vi.fn(async () => []) });
    });
    act(() => result.current.selectAsset(asset));
    await act(() => result.current.toggleSelectedFavorite());
    await act(async () => { stale.resolve(createDetails(asset, ["cat"])); });
    expect(result.current.selected?.is_favorite).toBe(true);
    expect(apiMocks.getAssetDetails).toHaveBeenCalledTimes(2);
    await act(async () => { fresh.resolve(createDetails({ ...asset, is_favorite: true }, ["cat"])); });
    expect(result.current.selected?.is_favorite).toBe(true);
  });
});
