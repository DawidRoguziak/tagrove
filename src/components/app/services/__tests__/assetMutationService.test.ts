import { describe, expect, it } from "vitest";
import type { AssetSummary, SelectedAsset } from "../../../../types";
import {
  applyBulkMediaGroupToAssets,
  updateAssetFavorite,
  updateAssetMediaGroup,
  updateSelectedFavorite,
  updateSelectedMediaGroup,
  updateSelectedTags
} from "../assetMutationService";

function createSummary(id: number): AssetSummary {
  return {
    id,
    file_name: `${id}.jpg`,
    preview_path: null,
    kind: "image",
    modified_at: 1,
    width: 100,
    height: 100,
    duration_ms: null,
    thumb_path: null,
    is_favorite: false,
    media_group_key: null,
    media_group_order: null
  };
}

function createSelected(id: number): SelectedAsset {
  return {
    ...createSummary(id),
    path: `C:/media/${id}.jpg`,
    size_bytes: 10,
    tags: []
  };
}

describe("assetMutationService", () => {
  it("updates tags on the selected asset view", () => {
    expect(updateSelectedTags(createSelected(1), ["cat"])?.tags).toEqual(["cat"]);
  });

  it("updates favorite in list and selected asset", () => {
    const assets = [createSummary(1), createSummary(2)];
    const nextAssets = updateAssetFavorite(assets, 1, true);

    expect(nextAssets[0]?.is_favorite).toBe(true);
    expect(nextAssets[1]?.is_favorite).toBe(false);
    expect(updateSelectedFavorite(createSelected(1), true)?.is_favorite).toBe(true);
  });

  it("updates media group fields in list and selected asset", () => {
    const assets = [createSummary(1), createSummary(2)];
    const nextAssets = updateAssetMediaGroup(assets, 2, "group-a", 4.5);

    expect(nextAssets[0]?.media_group_key).toBeNull();
    expect(nextAssets[1]?.media_group_key).toBe("group-a");
    expect(nextAssets[1]?.media_group_order).toBe(4.5);

    const selected = updateSelectedMediaGroup(createSelected(2), "group-a", 4.5);
    expect(selected?.media_group_key).toBe("group-a");
    expect(selected?.media_group_order).toBe(4.5);
  });

  it("applies media group to ordered bulk assignments", () => {
    const assets = [createSummary(1), createSummary(2), createSummary(3)];
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
