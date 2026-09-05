import { useState } from "react";
import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AssetSummary, BulkFavoriteSummary } from "../../../../types";
import { useBulkSelectionController } from "../useBulkSelectionController";
import { useAssetTagState } from "../useAssetTagState";

const api = vi.hoisted(() => ({ toggleAssetsFavoriteBulk: vi.fn(), getAssetDetails: vi.fn() }));
vi.mock("../../../../api", async (importOriginal) => ({
  ...await importOriginal<typeof import("../../../../api")>(), ...api
}));

function asset(id: number, is_favorite = false): AssetSummary {
  return {
    id, is_favorite, file_name: `${id}.png`, kind: "image", preview_path: null,
    modified_at: 1, width: 10, height: 10, duration_ms: null, thumb_path: null,
    media_group_key: null, media_group_order: null
  };
}

function setup(initial = [asset(1), asset(2, true)], favoritesOnly = false) {
  const refresh = vi.fn(async () => {});
  const onFavoritesChanged = vi.fn();
  const hook = renderHook(() => {
    const [assets, setAssets] = useState(initial);
    const assetTagState = useAssetTagState();
    const bulk = useBulkSelectionController({
      assets, setAssets, assetTagState, refresh, onFavoritesChanged,
      appliedFavoritesOnly: favoritesOnly, knownTags: [], settingsViewOpen: false,
      queueThumbnailsByIds: vi.fn(), refreshKnownTags: vi.fn(async () => [])
    });
    return { bulk, assets, setAssets, assetTagState };
  });
  act(() => hook.result.current.bulk.onToggleSelectionMode());
  const select = (id: number, ctrlLike = true) => act(() => {
    hook.result.current.bulk.onBulkSelectionInteraction({
      assetId: id, assetIndex: id - 1, ctrlLike, shift: false, viaDrag: false
    });
  });
  return { ...hook, refresh, onFavoritesChanged, select };
}

describe("bulk favorites", () => {
  beforeEach(() => {
    api.toggleAssetsFavoriteBulk.mockReset();
    api.getAssetDetails.mockReset().mockResolvedValue(null);
  });

  it("toggles mixed and all-favorite selections using the canonical backend result", async () => {
    const { result, select, onFavoritesChanged, refresh } = setup();
    await act(() => result.current.bulk.onToggleFavorite());
    expect(api.toggleAssetsFavoriteBulk).not.toHaveBeenCalled();
    select(1); select(2);
    expect(result.current.bulk.allSelectedFavorites).toBe(false);
    api.toggleAssetsFavoriteBulk.mockResolvedValueOnce({ processed_asset_ids: [1, 2], is_favorite: true, revision: 2 });
    await act(() => result.current.bulk.onToggleFavorite());
    expect(api.toggleAssetsFavoriteBulk).toHaveBeenCalledWith([1, 2]);
    expect(result.current.assets.every((item) => item.is_favorite)).toBe(true);
    expect(result.current.bulk.allSelectedFavorites).toBe(true);
    expect(onFavoritesChanged).toHaveBeenCalledWith(new Set([1, 2]), true);
    api.toggleAssetsFavoriteBulk.mockResolvedValueOnce({ processed_asset_ids: [1, 2], is_favorite: false, revision: 3 });
    await act(() => result.current.bulk.onToggleFavorite());
    expect(result.current.assets.every((item) => !item.is_favorite)).toBe(true);
    expect(result.current.bulk.selectedAssetIds).toEqual(new Set([1, 2]));
    expect(refresh).not.toHaveBeenCalled();
  });

  it("includes evicted IDs and refreshes favorites-only removal", async () => {
    const { result, select, refresh } = setup([asset(1, true), asset(2, true)], true);
    select(1); select(2);
    act(() => result.current.setAssets([]));
    expect(result.current.bulk.allSelectedFavorites).toBe(false);
    api.toggleAssetsFavoriteBulk.mockResolvedValue({ processed_asset_ids: [1, 2], is_favorite: false, revision: 2 });
    await act(() => result.current.bulk.onToggleFavorite());
    expect(api.toggleAssetsFavoriteBulk).toHaveBeenCalledWith([1, 2]);
    expect(refresh).toHaveBeenCalledOnce();
    expect(result.current.bulk.selectedAssetIds.size).toBe(2);
    api.toggleAssetsFavoriteBulk.mockResolvedValueOnce({ processed_asset_ids: [1, 2], is_favorite: true, revision: 3 });
    await act(() => result.current.bulk.onToggleFavorite());
    expect(refresh).toHaveBeenCalledTimes(2);
  });

  it("prunes only missing IDs and refreshes when no rows exist", async () => {
    const { result, select, refresh } = setup();
    select(1); select(2);
    api.toggleAssetsFavoriteBulk.mockResolvedValueOnce({ processed_asset_ids: [1], is_favorite: true, revision: 2 });
    await act(() => result.current.bulk.onToggleFavorite());
    expect(result.current.bulk.selectedAssetIds).toEqual(new Set([1]));
    expect(result.current.assets[0]?.is_favorite).toBe(true);
    api.toggleAssetsFavoriteBulk.mockResolvedValueOnce({ processed_asset_ids: [], is_favorite: false, revision: 2 });
    await act(() => result.current.bulk.onToggleFavorite());
    expect(result.current.bulk.selectedAssetIds.size).toBe(0);
    expect(refresh).toHaveBeenCalledTimes(2);
  });

  it("keeps persisted state on failure and permits retry", async () => {
    const { result, select, onFavoritesChanged } = setup();
    select(1);
    api.toggleAssetsFavoriteBulk.mockRejectedValueOnce(new Error("database failure"));
    await act(() => result.current.bulk.onToggleFavorite());
    expect(result.current.assets[0]?.is_favorite).toBe(false);
    expect(onFavoritesChanged).not.toHaveBeenCalled();
    expect(result.current.bulk.favoriteFailed).toBe(true);
    expect(result.current.bulk.favoriteApplying).toBe(false);
    api.toggleAssetsFavoriteBulk.mockResolvedValueOnce({ processed_asset_ids: [1], is_favorite: true, revision: 2 });
    await act(() => result.current.bulk.onToggleFavorite());
    expect(result.current.bulk.favoriteFailed).toBe(false);
    expect(result.current.assets[0]?.is_favorite).toBe(true);
  });

  it("acquires every metadata lock before IPC and releases acquired locks on contention", async () => {
    const { result, select } = setup();
    select(1); select(2);
    const held = result.current.assetTagState.beginMutation(2);
    expect(held).not.toBeNull();
    await act(() => result.current.bulk.onToggleFavorite());
    expect(api.toggleAssetsFavoriteBulk).not.toHaveBeenCalled();
    const released = result.current.assetTagState.beginMutation(1);
    expect(released).not.toBeNull();
    if (held) result.current.assetTagState.settleMutation(held);
    if (released) result.current.assetTagState.settleMutation(released);
  });

  it("blocks duplicate clicks, captures the selection, and drains before maintenance", async () => {
    const { result, select } = setup();
    select(1);
    let complete!: (value: BulkFavoriteSummary) => void;
    api.toggleAssetsFavoriteBulk.mockReturnValueOnce(new Promise<BulkFavoriteSummary>((resolve) => { complete = resolve; }));
    let pending!: Promise<void>;
    act(() => { pending = result.current.bulk.onToggleFavorite(); });
    expect(result.current.bulk.favoriteApplying).toBe(true);
    select(2, false);
    await act(() => result.current.bulk.onToggleFavorite());
    const maintenance = vi.fn(async () => {});
    const barrier = result.current.assetTagState.runWithMutationBarrier(maintenance);
    expect(maintenance).not.toHaveBeenCalled();
    await act(async () => {
      complete({ processed_asset_ids: [1], is_favorite: true, revision: 2 });
      await pending;
      await barrier;
    });
    expect(api.toggleAssetsFavoriteBulk).toHaveBeenCalledTimes(1);
    expect(result.current.bulk.selectedAssetIds).toEqual(new Set([2]));
    expect(result.current.assets[0]?.is_favorite).toBe(true);
    expect(maintenance).toHaveBeenCalledOnce();
  });

  it("does not patch a new library identity after a stale completion", async () => {
    const { result, select, onFavoritesChanged } = setup();
    select(1);
    let complete!: (value: BulkFavoriteSummary) => void;
    api.toggleAssetsFavoriteBulk.mockReturnValueOnce(new Promise<BulkFavoriteSummary>((resolve) => { complete = resolve; }));
    let pending!: Promise<void>;
    act(() => { pending = result.current.bulk.onToggleFavorite(); });
    act(() => result.current.assetTagState.reset());
    await act(async () => {
      complete({ processed_asset_ids: [1], is_favorite: true, revision: 2 });
      await pending;
    });
    expect(onFavoritesChanged).not.toHaveBeenCalled();
    expect(result.current.assets[0]?.is_favorite).toBe(false);
    expect(result.current.bulk.selectedAssetIds.size).toBe(0);
    expect(result.current.bulk.favoriteApplying).toBe(false);
  });
});
