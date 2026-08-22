import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AssetSummary, SelectedAsset } from "../../../../types";
import { toggleLightboxFavoriteAction } from "../toggleLightboxFavoriteAction";

const apiMocks = vi.hoisted(() => ({
  setAssetFavorite: vi.fn()
}));

vi.mock("../../../../api", async () => {
  const actual = await vi.importActual<typeof import("../../../../api")>("../../../../api");
  return {
    ...actual,
    ...apiMocks
  };
});

function createSummary(id: number, isFavorite: boolean): AssetSummary {
  return {
    id,
    file_name: `${id}.jpg`,
    preview_path: null,
    kind: "image",
    modified_at: 100,
    width: 400,
    height: 300,
    duration_ms: null,
    thumb_path: null,
    is_favorite: isFavorite,
    media_group_key: null,
    media_group_order: null
  };
}

function createAsset(id: number, isFavorite: boolean): SelectedAsset {
  return {
    ...createSummary(id, isFavorite),
    path: `C:/media/${id}.jpg`,
    size_bytes: 200,
    tags: []
  };
}

describe("toggleLightboxFavoriteAction", () => {
  beforeEach(() => {
    apiMocks.setAssetFavorite.mockReset();
  });

  it("toggles favorite and refreshes when favorites-only view would hide asset", async () => {
    apiMocks.setAssetFavorite.mockResolvedValue(undefined);
    const refresh = vi.fn().mockResolvedValue(undefined);

    let assets = [createSummary(1, true), createSummary(2, false)];
    let selected: SelectedAsset | null = createAsset(1, true);

    const setAssets = vi.fn((updater: (previous: AssetSummary[]) => AssetSummary[]) => {
      assets = updater(assets);
    });
    const setSelected = vi.fn((updater: (previous: SelectedAsset | null) => SelectedAsset | null) => {
      selected = updater(selected);
    });

    await toggleLightboxFavoriteAction({
      selected,
      appliedFavoritesOnly: true,
      setAssets,
      setSelected,
      refresh
    });

    expect(apiMocks.setAssetFavorite).toHaveBeenCalledWith(1, false);
    expect(assets[0]?.is_favorite).toBe(false);
    expect(selected?.is_favorite).toBe(false);
    expect(refresh).toHaveBeenCalledTimes(1);
  });

  it("toggles favorite without refresh outside favorites-only view", async () => {
    apiMocks.setAssetFavorite.mockResolvedValue(undefined);
    const refresh = vi.fn().mockResolvedValue(undefined);

    let assets = [createSummary(1, false), createSummary(2, false)];
    let selected: SelectedAsset | null = createAsset(1, false);

    const setAssets = vi.fn((updater: (previous: AssetSummary[]) => AssetSummary[]) => {
      assets = updater(assets);
    });
    const setSelected = vi.fn((updater: (previous: SelectedAsset | null) => SelectedAsset | null) => {
      selected = updater(selected);
    });

    await toggleLightboxFavoriteAction({
      selected,
      appliedFavoritesOnly: false,
      setAssets,
      setSelected,
      refresh
    });

    expect(apiMocks.setAssetFavorite).toHaveBeenCalledWith(1, true);
    expect(assets[0]?.is_favorite).toBe(true);
    expect(selected?.is_favorite).toBe(true);
    expect(refresh).not.toHaveBeenCalled();
  });

  it("does nothing when nothing is selected", async () => {
    const refresh = vi.fn().mockResolvedValue(undefined);
    const setAssets = vi.fn();
    const setSelected = vi.fn();

    await toggleLightboxFavoriteAction({
      selected: null,
      appliedFavoritesOnly: false,
      setAssets,
      setSelected,
      refresh
    });

    expect(apiMocks.setAssetFavorite).not.toHaveBeenCalled();
    expect(setAssets).not.toHaveBeenCalled();
    expect(setSelected).not.toHaveBeenCalled();
    expect(refresh).not.toHaveBeenCalled();
  });
});
