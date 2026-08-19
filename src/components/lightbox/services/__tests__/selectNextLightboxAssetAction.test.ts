import { describe, expect, it } from "vitest";
import type { Asset } from "../../../../types";
import { selectNextLightboxAssetAction } from "../selectNextLightboxAssetAction";

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

describe("selectNextLightboxAssetAction", () => {
  it("returns next asset when current one is not last", () => {
    const assets = [createAsset(1), createAsset(2), createAsset(3)];
    const result = selectNextLightboxAssetAction(assets, assets[1] ?? null);
    expect(result?.id).toBe(3);
  });

  it("keeps selection when already on last asset", () => {
    const assets = [createAsset(1), createAsset(2)];
    const result = selectNextLightboxAssetAction(assets, assets[1] ?? null);
    expect(result?.id).toBe(2);
  });

  it("returns null when there is no selected asset", () => {
    const assets = [createAsset(1), createAsset(2)];
    const result = selectNextLightboxAssetAction(assets, null);
    expect(result).toBeNull();
  });

  it("keeps selection when selected asset is not present in list", () => {
    const assets = [createAsset(1), createAsset(2)];
    const detached = createAsset(99);
    const result = selectNextLightboxAssetAction(assets, detached);
    expect(result).toBe(detached);
  });
});
