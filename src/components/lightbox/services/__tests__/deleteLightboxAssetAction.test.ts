import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AssetSummary, SelectedAsset } from "../../../../types";
import { deleteLightboxAssetAction } from "../deleteLightboxAssetAction";

const apiMocks = vi.hoisted(() => ({
  deleteAsset: vi.fn()
}));

vi.mock("../../../../api", async () => {
  const actual = await vi.importActual<typeof import("../../../../api")>("../../../../api");
  return {
    ...actual,
    ...apiMocks
  };
});

function createSummary(id: number): AssetSummary {
  return {
    id,
    file_name: `${id}.jpg`,
    preview_path: null,
    kind: "image",
    modified_at: 22,
    width: 640,
    height: 480,
    duration_ms: null,
    thumb_path: `C:/thumbs/${id}.jpg`,
    is_favorite: false,
    media_group_key: null,
    media_group_order: null
  };
}

function createAsset(id: number): SelectedAsset {
  return {
    ...createSummary(id),
    path: `C:/media/${id}.jpg`,
    size_bytes: 222,
    tags: ["cat"]
  };
}

describe("deleteLightboxAssetAction", () => {
  beforeEach(() => {
    apiMocks.deleteAsset.mockReset();
  });

  it("deletes selected asset and refreshes library + tags", async () => {
    apiMocks.deleteAsset.mockResolvedValue({
      removed_assets: 1,
      removed_thumbnails: 1,
      removed_media_file: true
    });

    let assets = [createSummary(1), createSummary(2)];
    let selected: SelectedAsset | null = createAsset(2);

    const setAssets = vi.fn((updater: (previous: AssetSummary[]) => AssetSummary[]) => {
      assets = updater(assets);
    });
    const setSelected = vi.fn((updater: (previous: SelectedAsset | null) => SelectedAsset | null) => {
      selected = updater(selected);
    });
    const refresh = vi.fn().mockResolvedValue(undefined);
    const refreshKnownTags = vi.fn().mockResolvedValue([]);
    const onDeleted = vi.fn();

    await deleteLightboxAssetAction({
      selected,
      setAssets,
      setSelected,
      refresh,
      refreshKnownTags,
      onDeleted
    });

    expect(apiMocks.deleteAsset).toHaveBeenCalledWith(2);
    expect(onDeleted).toHaveBeenCalledWith(2);
    expect(assets.map((asset) => asset.id)).toEqual([1]);
    expect(selected).toBeNull();
    expect(refreshKnownTags).toHaveBeenCalledTimes(1);
    expect(refresh).toHaveBeenCalledTimes(1);
  });

  it("does not patch a reused identity when deletion invalidation is rejected", async () => {
    apiMocks.deleteAsset.mockResolvedValue({
      removed_assets: 1, removed_thumbnails: 0, removed_media_file: true
    });
    const setAssets = vi.fn();
    const setSelected = vi.fn();
    const refresh = vi.fn().mockResolvedValue(undefined);
    const refreshKnownTags = vi.fn().mockResolvedValue([]);

    await deleteLightboxAssetAction({
      selected: createAsset(7),
      setAssets,
      setSelected,
      refresh,
      refreshKnownTags,
      onDeleted: () => false
    });

    expect(setAssets).not.toHaveBeenCalled();
    expect(setSelected).not.toHaveBeenCalled();
    expect(refreshKnownTags).not.toHaveBeenCalled();
    expect(refresh).not.toHaveBeenCalled();
  });

  it("does nothing when nothing is selected", async () => {
    const setAssets = vi.fn();
    const setSelected = vi.fn();
    const refresh = vi.fn().mockResolvedValue(undefined);
    const refreshKnownTags = vi.fn().mockResolvedValue([]);

    await deleteLightboxAssetAction({
      selected: null,
      setAssets,
      setSelected,
      refresh,
      refreshKnownTags
    });

    expect(apiMocks.deleteAsset).not.toHaveBeenCalled();
    expect(setAssets).not.toHaveBeenCalled();
    expect(setSelected).not.toHaveBeenCalled();
    expect(refreshKnownTags).not.toHaveBeenCalled();
    expect(refresh).not.toHaveBeenCalled();
  });
});
