import { describe, expect, it } from "vitest";
import type { Asset } from "../../../../types";
import { selectPreviousLightboxAssetAction } from "../selectPreviousLightboxAssetAction";

function createAsset(id: number): Asset {
  return {
    id,
    path: `C:/media/${id}.jpg`,
    kind: "image",
    size_bytes: 100,
    modified_at: 100,
    width: 320,
    height: 240,
    duration_ms: null,
    thumb_path: null,
    is_favorite: false,
    media_group_key: null,
    media_group_order: null,
    tags: []
  };
}

describe("selectPreviousLightboxAssetAction", () => {
  it("returns previous asset when current one is not first", () => {
    const assets = [createAsset(1), createAsset(2), createAsset(3)];
    const result = selectPreviousLightboxAssetAction(assets, assets[1] ?? null);
    expect(result?.id).toBe(1);
  });

  it("keeps selection when already on first asset", () => {
    const assets = [createAsset(1), createAsset(2)];
    const result = selectPreviousLightboxAssetAction(assets, assets[0] ?? null);
    expect(result?.id).toBe(1);
  });

  it("returns null when there is no selected asset", () => {
    const assets = [createAsset(1), createAsset(2)];
    const result = selectPreviousLightboxAssetAction(assets, null);
    expect(result).toBeNull();
  });

  it("keeps selection when selected asset is not present in list", () => {
    const assets = [createAsset(1), createAsset(2)];
    const detached = createAsset(99);
    const result = selectPreviousLightboxAssetAction(assets, detached);
    expect(result).toBe(detached);
  });
});
