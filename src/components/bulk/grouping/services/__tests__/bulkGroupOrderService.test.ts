import { describe, expect, it } from "vitest";
import {
  buildBulkMediaGroupAssignments,
  deriveBulkGroupSelectionState,
  normalizeGroupKey,
  reorderAssetIdsByDrop,
  uniqueOrderedAssetIds
} from "../bulkGroupOrderService";
import type { AssetSummary } from "../../../../../types";

function createAsset(id: number, groupKey: string | null, groupOrder: number | null): AssetSummary {
  return {
    id,
    file_name: `${id}.jpg`,
    preview_path: null,
    kind: "image",
    modified_at: 1,
    width: null,
    height: null,
    duration_ms: null,
    thumb_path: null,
    is_favorite: false,
    media_group_key: groupKey,
    media_group_order: groupOrder
  };
}

describe("bulkGroupOrderService", () => {
  it("trims group key", () => {
    expect(normalizeGroupKey("  trip-2026  ")).toBe("trip-2026");
  });

  it("keeps positive unique asset ids in order", () => {
    expect(uniqueOrderedAssetIds([3, -1, 3, 2, 0, 1, 2])).toEqual([3, 2, 1]);
  });

  it("builds 1-based media group order assignments", () => {
    expect(buildBulkMediaGroupAssignments([7, 9, 4])).toEqual([
      { assetId: 7, mediaGroupOrder: 1 },
      { assetId: 9, mediaGroupOrder: 2 },
      { assetId: 4, mediaGroupOrder: 3 }
    ]);
  });

  it("reorders ids by drag and drop target", () => {
    expect(reorderAssetIdsByDrop([11, 12, 13, 14], 11, 13)).toEqual([12, 13, 11, 14]);
  });

  it("derives a shared group and sorts its assets by saved order", () => {
    expect(
      deriveBulkGroupSelectionState([
        createAsset(1, " Trip ", 3),
        createAsset(2, "trip", 1),
        createAsset(3, "TRIP", null)
      ])
    ).toEqual({
      groupKey: "Trip",
      hasConflictingGroups: false,
      orderedAssetIds: [2, 1, 3]
    });
  });

  it("reports mixed grouped and ungrouped selections as conflicting", () => {
    expect(
      deriveBulkGroupSelectionState([
        createAsset(1, "trip", 1),
        createAsset(2, null, null)
      ])
    ).toEqual({
      groupKey: "",
      hasConflictingGroups: true,
      orderedAssetIds: [1, 2]
    });
  });

  it("builds nullable assignments for clearing", () => {
    expect(buildBulkMediaGroupAssignments([4, 2], false)).toEqual([
      { assetId: 4, mediaGroupOrder: null },
      { assetId: 2, mediaGroupOrder: null }
    ]);
  });
});
