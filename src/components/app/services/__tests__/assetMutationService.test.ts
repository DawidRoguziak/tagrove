import { describe, expect, it } from "vitest";
import type { Asset } from "../../../../types";
import {
  applyBulkMediaGroupToAssets,
  updateAssetFavorite,
  updateAssetMediaGroup,
  updateAssetTags,
  updateSelectedFavorite,
  updateSelectedMediaGroup,
  updateSelectedTags
} from "../assetMutationService";

function createAsset(id: number): Asset {
  return {
    id,
    path: `C:/media/${id}.jpg`,
    kind: "image",
    size_bytes: 10,
    modified_at: 1,
    width: 100,
    height: 100,
    duration_ms: null,
    thumb_path: null,
    is_favorite: false,
    media_group_key: null,
    media_group_order: null,
    tags: []
  };
}

describe("assetMutationService", () => {
  it("updates tags in list and selected asset", () => {
    const assets = [createAsset(1), createAsset(2)];
    const nextAssets = updateAssetTags(assets, 2, ["cat"]);

    expect(nextAssets[0]?.tags).toEqual([]);
    expect(nextAssets[1]?.tags).toEqual(["cat"]);
    expect(updateSelectedTags(assets[1] ?? null, ["cat"])?.tags).toEqual(["cat"]);
  });

  it("updates favorite in list and selected asset", () => {
    const assets = [createAsset(1), createAsset(2)];
    const nextAssets = updateAssetFavorite(assets, 1, true);

    expect(nextAssets[0]?.is_favorite).toBe(true);
    expect(nextAssets[1]?.is_favorite).toBe(false);
    expect(updateSelectedFavorite(assets[0] ?? null, true)?.is_favorite).toBe(true);
  });

  it("updates media group fields in list and selected asset", () => {
    const assets = [createAsset(1), createAsset(2)];
    const nextAssets = updateAssetMediaGroup(assets, 2, "group-a", 4.5);

    expect(nextAssets[0]?.media_group_key).toBeNull();
    expect(nextAssets[1]?.media_group_key).toBe("group-a");
    expect(nextAssets[1]?.media_group_order).toBe(4.5);

    const selected = updateSelectedMediaGroup(assets[1] ?? null, "group-a", 4.5);
    expect(selected?.media_group_key).toBe("group-a");
    expect(selected?.media_group_order).toBe(4.5);
  });

  it("applies media group to ordered bulk assignments", () => {
    const assets = [createAsset(1), createAsset(2), createAsset(3)];
    const nextAssets = applyBulkMediaGroupToAssets(assets, "trip-2026", [
      { assetId: 3, mediaGroupOrder: 1 },
      { assetId: 1, mediaGroupOrder: 2 }
    ]);

    expect(nextAssets[0]?.media_group_key).toBe("trip-2026");
    expect(nextAssets[0]?.media_group_order).toBe(2);
    expect(nextAssets[1]?.media_group_key).toBeNull();
    expect(nextAssets[1]?.media_group_order).toBeNull();
    expect(nextAssets[2]?.media_group_key).toBe("trip-2026");
    expect(nextAssets[2]?.media_group_order).toBe(1);
  });
});
