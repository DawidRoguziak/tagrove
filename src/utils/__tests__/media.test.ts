import { describe, expect, it } from "vitest";
import {
  formatBytes,
  formatDuration,
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
    expect(parseSearchFilter("tags:").validationError).toBe("hasNoTagsInvalidCount");
    expect(parseSearchFilter("gN:").validationError).toBe("groupNameMissingValue");
    expect(parseSearchFilter("cat tags").validationError).toBe("metaTagRequiresSolo");
    expect(parseSearchFilter("dog gN:trip-2026").validationError).toBe("metaTagRequiresSolo");
    expect(parseSearchFilter("cat,dog").validationError).toBe("tagInvalidCharacters");
  });

  it("accepts tags:N up to the largest safe integer, including leading zeroes", () => {
    expect(parseSearchFilter("tags:0003").metaFilter).toEqual({ type: "hasNoTags", tagCount: 3 });
    expect(parseSearchFilter(`tags:${Number.MAX_SAFE_INTEGER}`).metaFilter).toEqual({
      type: "hasNoTags",
      tagCount: Number.MAX_SAFE_INTEGER
    });
    expect(parseSearchFilter(`tags:000000000000000000000${Number.MAX_SAFE_INTEGER}`)).toMatchObject({
      metaFilter: { type: "hasNoTags", tagCount: Number.MAX_SAFE_INTEGER },
      validationError: null
    });
  });

  it("rejects tags:N beyond the safe integer range with the count validation message", () => {
    for (const count of [
      "9007199254740992", // Number.MAX_SAFE_INTEGER + 1
      "9223372036854775808", // i64::MAX + 1
      "18446744073709551616", // 2^64
      "99999999999999999999",
      "9".repeat(400) // parses to Infinity
    ]) {
      expect(parseSearchFilter(`tags:${count}`)).toEqual({
        mode: "tags",
        include: [],
        exclude: [],
        metaFilter: null,
        validationError: "hasNoTagsInvalidCount"
      });
    }
  });

  it("formats bytes and duration", () => {
    expect(formatBytes(1024)).toBe("1.0 KB");
    expect(formatDuration(65000)).toBe("1:05");
    expect(formatDuration(0)).toBe("-");
  });
});
