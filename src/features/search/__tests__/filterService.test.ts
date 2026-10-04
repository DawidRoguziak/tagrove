import { describe, expect, it } from "vitest";
import {
  buildMetaFilterKey,
  DEFAULT_SEARCH_FILTERS,
  areSearchFiltersEqual
} from "../filterService";

describe("filterService", () => {
  it("compares search filters by value", () => {
    expect(
      areSearchFiltersEqual(
        { filterInput: "cat", mediaKind: "video", favoritesOnly: true },
        { filterInput: "cat", mediaKind: "video", favoritesOnly: true }
      )
    ).toBe(true);

    expect(
      areSearchFiltersEqual(
        { filterInput: "cat", mediaKind: "video", favoritesOnly: true },
        { filterInput: "dog", mediaKind: "video", favoritesOnly: true }
      )
    ).toBe(false);
  });

  it("defines empty defaults", () => {
    expect(DEFAULT_SEARCH_FILTERS).toEqual({
      filterInput: "",
      mediaKind: "all",
      favoritesOnly: false
    });
  });

  it("builds stable meta filter keys", () => {
    expect(buildMetaFilterKey(null)).toBe("");
    expect(buildMetaFilterKey({ type: "hasNoTags", tagCount: 2 })).toBe("hasNoTags:2");
    expect(buildMetaFilterKey({ type: "groupName", groupName: "  Trip-2026  " })).toBe(
      "groupName:trip-2026"
    );
  });
});
