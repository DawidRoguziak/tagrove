import { describe, expect, it } from "vitest";
import {
  formatBytes,
  formatDuration,
  mapThumbs,
  normalizeTags,
  parseFilterTags,
  parseSearchFilter
} from "../media";

describe("media utils", () => {
  it("normalizes tags to lowercase and deduplicates", () => {
    expect(normalizeTags(" Cat  DOG cat  ")).toEqual(["cat", "dog"]);
    expect(normalizeTags(["cat", "new york", "dog;bird"])).toEqual(["cat"]);
  });

  it("parses include and exclude filter tags", () => {
    expect(parseFilterTags("cat  -dog car -bird")).toEqual({
      include: ["cat", "car"],
      exclude: ["dog", "bird"]
    });
    expect(parseFilterTags("-")).toEqual({ include: ["-"], exclude: [] });
  });

  it("parses tags metatags as exact tag counts", () => {
    expect(parseSearchFilter("tags")).toEqual({
      mode: "meta",
      include: [],
      exclude: [],
      metaFilter: {
        type: "hasNoTags",
        tagCount: 0
      },
      validationError: null
    });

    expect(parseSearchFilter("tags:2")).toEqual({
      mode: "meta",
      include: [],
      exclude: [],
      metaFilter: {
        type: "hasNoTags",
        tagCount: 2
      },
      validationError: null
    });
  });

  it("parses gN metatags as exact group filters", () => {
    expect(parseSearchFilter("gN:Trip 2026")).toEqual({
      mode: "meta",
      include: [],
      exclude: [],
      metaFilter: {
        type: "groupName",
        groupName: "Trip 2026"
      },
      validationError: null
    });
  });

  it("rejects invalid metatag payloads and mixing metatags with normal tags", () => {
    expect(parseSearchFilter("tags:abc").validationError).toBe("hasNoTagsInvalidCount");
    expect(parseSearchFilter("tags:-1").validationError).toBe("hasNoTagsInvalidCount");
    expect(parseSearchFilter("gN:").validationError).toBe("groupNameMissingValue");
    expect(parseSearchFilter("cat tags").validationError).toBe("metaTagRequiresSolo");
    expect(parseSearchFilter("dog gN:trip-2026").validationError).toBe("metaTagRequiresSolo");
    expect(parseSearchFilter("cat,dog").validationError).toBe("tagInvalidCharacters");
  });

  it("maps only available thumbnail paths", () => {
    expect(
      mapThumbs([
        {
          id: 1,
          file_name: "a.jpg",
          preview_path: null,
          kind: "image",
          modified_at: 1,
          width: null,
          height: null,
          duration_ms: null,
          thumb_path: "thumb-a",
          is_favorite: false,
          media_group_key: null,
          media_group_order: null
        },
        {
          id: 2,
          file_name: "b.mp4",
          preview_path: "b.mp4",
          kind: "video",
          modified_at: 1,
          width: null,
          height: null,
          duration_ms: null,
          thumb_path: null,
          is_favorite: false,
          media_group_key: null,
          media_group_order: null
        }
      ])
    ).toEqual({ 1: "thumb-a" });
  });

  it("formats bytes and duration", () => {
    expect(formatBytes(1024)).toBe("1.0 KB");
    expect(formatDuration(65000)).toBe("1:05");
    expect(formatDuration(0)).toBe("-");
  });
});
