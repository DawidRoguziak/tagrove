import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AssetSummary } from "../../../../../types";
import {
  applyBulkMediaGroupAction,
  buildApplyBulkMediaGroupPayload
} from "../applyBulkMediaGroupAction";

const apiMocks = vi.hoisted(() => ({
  setAssetsMediaGroupBulk: vi.fn()
}));

vi.mock("../../../../../api", async () => {
  const actual = await vi.importActual<typeof import("../../../../../api")>("../../../../../api");
  return {
    ...actual,
    ...apiMocks
  };
});

function createAsset(id: number): AssetSummary {
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
  }
}

describe("applyBulkMediaGroupAction", () => {
  beforeEach(() => {
    apiMocks.setAssetsMediaGroupBulk.mockReset();
    apiMocks.setAssetsMediaGroupBulk.mockResolvedValue({
      processed_assets: 0,
      updated_assets: 0,
      media_group_key: null
    });
  });

  it("builds normalized payload", () => {
    const payload = buildApplyBulkMediaGroupPayload([4, 4, 2], "  trip-2026 ");
    expect(payload).toEqual({
      normalizedGroupKey: "trip-2026",
      assignments: [
        { assetId: 4, mediaGroupOrder: 1 },
        { assetId: 2, mediaGroupOrder: 2 }
      ]
    });
  });

  it("applies bulk media group and patches assets in memory", async () => {
    let assets = [createAsset(1), createAsset(2), createAsset(3)];
    const setAssets = vi.fn((updater: (previous: AssetSummary[]) => AssetSummary[]) => {
      assets = updater(assets);
    });

    await applyBulkMediaGroupAction({
      assetIdsInOrder: [3, 1],
      groupKey: "trip-2026",
      setAssets
    });

    expect(apiMocks.setAssetsMediaGroupBulk).toHaveBeenCalledWith(
      [
        { assetId: 3, mediaGroupOrder: 1 },
        { assetId: 1, mediaGroupOrder: 2 }
      ],
      "trip-2026"
    );
    expect(assets[0]?.media_group_key).toBe("trip-2026");
    expect(assets[0]?.media_group_order).toBe(2);
    expect(assets[1]?.media_group_key).toBeNull();
    expect(assets[2]?.media_group_key).toBe("trip-2026");
    expect(assets[2]?.media_group_order).toBe(1);
  });

  it("clears group keys and orders with a nullable bulk payload", async () => {
    let assets: AssetSummary[] = [
      { ...createAsset(1), media_group_key: "legacy", media_group_order: 4 },
      { ...createAsset(2), media_group_key: "legacy", media_group_order: 5 }
    ];
    const setAssets = vi.fn((updater: (previous: AssetSummary[]) => AssetSummary[]) => {
      assets = updater(assets);
    });

    await applyBulkMediaGroupAction({
      assetIdsInOrder: [1, 2],
      groupKey: "   ",
      setAssets
    });

    expect(apiMocks.setAssetsMediaGroupBulk).toHaveBeenCalledWith(
      [
        { assetId: 1, mediaGroupOrder: null },
        { assetId: 2, mediaGroupOrder: null }
      ],
      null
    );
    expect(assets.every((asset) => asset.media_group_key === null && asset.media_group_order === null)).toBe(true);
  });
});
