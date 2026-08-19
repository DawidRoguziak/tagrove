import { describe, expect, it } from "vitest";
import type { Asset } from "../../../../../types";
import {
  mergeBulkTagsInAssets,
  mergeTagLists,
  normalizeBulkTagList
} from "../bulkTagMergeService";

function createAsset(id: number, tags: string[] = []): Asset {
  return {
    id,
    path: `C:/media/${id}.jpg`,
    kind: "image",
    size_bytes: 123,
    modified_at: 1700000000,
    width: 100,
    height: 100,
    duration_ms: null,
    thumb_path: null,
    is_favorite: false,
    media_group_key: null,
    media_group_order: null,
    tags
  };
}

describe("bulkTagMergeService", () => {
  it("normalizes list to lowercase and removes duplicates", () => {
    expect(normalizeBulkTagList([" Cat ", "cat", "DOG", ""])).toEqual(["cat", "dog"]);
  });

  it("merges incoming tags to existing list without duplicates", () => {
    expect(mergeTagLists(["cat", "dog"], ["dog", "travel", " Travel "])).toEqual([
      "cat",
      "dog",
      "travel"
    ]);
  });

  it("applies merged tags only to selected assets", () => {
    const assets = [createAsset(1, ["cat"]), createAsset(2, ["dog"]), createAsset(3, [])];
    const nextAssets = mergeBulkTagsInAssets(assets, new Set([1, 3]), ["travel", "cat"]);

    expect(nextAssets[0]?.tags).toEqual(["cat", "travel"]);
    expect(nextAssets[1]?.tags).toEqual(["dog"]);
    expect(nextAssets[2]?.tags).toEqual(["travel", "cat"]);
  });
});
