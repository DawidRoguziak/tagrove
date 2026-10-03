import type { mergeAssetTagsBulk } from "../../../../../api";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { applyBulkTagsAction } from "../applyBulkTagsAction";

const apiMocks = vi.hoisted(() => ({ mergeAssetTagsBulk: vi.fn<typeof mergeAssetTagsBulk>() }));
vi.mock("../../../../../api", async () => ({
  ...(await vi.importActual<typeof import("../../../../../api")>("../../../../../api")),
  ...apiMocks
}));

describe("applyBulkTagsAction", () => {
  beforeEach(() => apiMocks.mergeAssetTagsBulk.mockReset());

  it.each([false, true])("normalizes partial bulk results with vocabulary_changed=%s", async (vocabularyChanged) => {
    const refreshKnownTagsAfterMutation = vi.fn().mockRejectedValue(new Error("secondary failure"));
    apiMocks.mergeAssetTagsBulk.mockResolvedValue({
      vocabulary_changed: vocabularyChanged, query_impact: { type: "all" }, processed_assets: 1, updated_assets: 1, processed_asset_ids: [1], updated_asset_ids: [1],
      results: [{ asset_id: 1, changed: true, tags: ["cat", "travel"] }], revision: 2
    });

    const result = await applyBulkTagsAction({
      assetIds: [2, 2, 1, -3, 0, 1.5], tags: [" Travel ", "travel", "", "CAT"],
      refreshKnownTagsAfterMutation
    });
    expect(apiMocks.mergeAssetTagsBulk).toHaveBeenCalledWith([2, 1], ["travel", "cat"]);
    expect(result?.processed_asset_ids).toEqual([1]);
    expect(refreshKnownTagsAfterMutation).toHaveBeenCalledWith(vocabularyChanged);
  });

  it("does nothing when no valid asset ids are provided", async () => {
    const result = await applyBulkTagsAction({ assetIds: [0, -1], tags: ["cat"], refreshKnownTagsAfterMutation: vi.fn() });
    expect(result).toBeNull();
    expect(apiMocks.mergeAssetTagsBulk).not.toHaveBeenCalled();
  });

  it("does nothing when no valid tags are provided", async () => {
    const result = await applyBulkTagsAction({ assetIds: [1, 2], tags: ["   ", ""], refreshKnownTagsAfterMutation: vi.fn() });
    expect(result).toBeNull();
    expect(apiMocks.mergeAssetTagsBulk).not.toHaveBeenCalled();
  });
});
