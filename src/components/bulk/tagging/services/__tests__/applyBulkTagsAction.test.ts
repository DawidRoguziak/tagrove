import { beforeEach, describe, expect, it, vi } from "vitest";
import { applyBulkTagsAction } from "../applyBulkTagsAction";

const apiMocks = vi.hoisted(() => ({ mergeAssetTagsBulk: vi.fn() }));
vi.mock("../../../../../api", async () => ({
  ...(await vi.importActual<typeof import("../../../../../api")>("../../../../../api")),
  ...apiMocks
}));

describe("applyBulkTagsAction", () => {
  beforeEach(() => apiMocks.mergeAssetTagsBulk.mockReset());

  it("normalizes payload, returns canonical partial results, and does not fabricate summary tags", async () => {
    const refreshKnownTags = vi.fn().mockRejectedValue(new Error("secondary failure"));
    apiMocks.mergeAssetTagsBulk.mockResolvedValue({
      processed_assets: 1, updated_assets: 1, processed_asset_ids: [1], updated_asset_ids: [1],
      results: [{ asset_id: 1, changed: true, tags: ["cat", "travel"] }], revision: 2
    });

    const result = await applyBulkTagsAction({
      assetIds: [2, 2, 1, -3, 0, 1.5], tags: [" Travel ", "travel", "", "CAT"],
      refreshKnownTags
    });
    expect(apiMocks.mergeAssetTagsBulk).toHaveBeenCalledWith([2, 1], ["travel", "cat"]);
    expect(result?.processed_asset_ids).toEqual([1]);
  });

  it("does nothing when no valid asset ids are provided", async () => {
    const result = await applyBulkTagsAction({ assetIds: [0, -1], tags: ["cat"], refreshKnownTags: vi.fn() });
    expect(result).toBeNull();
    expect(apiMocks.mergeAssetTagsBulk).not.toHaveBeenCalled();
  });

  it("does nothing when no valid tags are provided", async () => {
    const result = await applyBulkTagsAction({ assetIds: [1, 2], tags: ["   ", ""], refreshKnownTags: vi.fn() });
    expect(result).toBeNull();
    expect(apiMocks.mergeAssetTagsBulk).not.toHaveBeenCalled();
  });
});
