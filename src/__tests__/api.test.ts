import { beforeEach, describe, expect, it, vi } from "vitest";
import { listAssets, mergeAssetTagsBulk, setAssetTags, toMediaSrc } from "../api";

const coreMocks = vi.hoisted(() => ({
  invoke: vi.fn(),
  convertFileSrc: vi.fn((path: string) => `tauri://${path}`)
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: coreMocks.invoke,
  convertFileSrc: coreMocks.convertFileSrc
}));

describe("api contract", () => {
  beforeEach(() => {
    coreMocks.invoke.mockReset();
    coreMocks.convertFileSrc.mockClear();
  });

  it("maps mediaKind=all to null kind in listAssets invoke payload", async () => {
    coreMocks.invoke.mockResolvedValueOnce({ items: [], total: 0 });

    await listAssets({
      offset: 10,
      limit: 20,
      tagsAnd: ["cat"],
      tagsNot: ["dog"],
      mediaKind: "all",
      favoritesOnly: true
    });

    expect(coreMocks.invoke).toHaveBeenCalledWith("list_assets", {
      offset: 10,
      limit: 20,
      tagsAnd: ["cat"],
      tagsNot: ["dog"],
      kind: null,
      favoritesOnly: true,
      metaFilter: null
    });
  });

  it("maps tag mutation payloads and returns canonical backend summaries", async () => {
    const single = { asset_id: 7, changed: true, tags: ["cat"], revision: 4 };
    const bulk = {
      processed_assets: 1,
      updated_assets: 1,
      processed_asset_ids: [7],
      updated_asset_ids: [7],
      results: [{ asset_id: 7, changed: true, tags: ["cat", "travel"] }],
      revision: 5
    };
    coreMocks.invoke.mockResolvedValueOnce(single).mockResolvedValueOnce(bulk);

    await expect(setAssetTags(7, ["cat"])).resolves.toEqual(single);
    expect(coreMocks.invoke).toHaveBeenNthCalledWith(1, "set_asset_tags", {
      assetId: 7,
      tags: ["cat"]
    });
    await expect(mergeAssetTagsBulk([7, 9], ["travel"])).resolves.toEqual(bulk);
    expect(coreMocks.invoke).toHaveBeenNthCalledWith(2, "merge_asset_tags_bulk", {
      assetIds: [7, 9],
      tags: ["travel"]
    });
  });

  it("normalizes windows slashes before converting media source", () => {
    const result = toMediaSrc("C:\\library\\cats\\photo.jpg");

    expect(coreMocks.convertFileSrc).toHaveBeenCalledWith("C:/library/cats/photo.jpg");
    expect(result).toBe("tauri://C:/library/cats/photo.jpg");
  });
});
