import { buildFilterDescriptor } from "../filterService";
import { describe, expect, it } from "vitest";
import {
  bulkTagMutationRequiresRefresh,
  collectChangedTags,
  favoriteMutationRequiresRefresh,
  tagMutationTouchesFilters
} from "../libraryInvalidationService";

describe("collectChangedTags", () => {
  it("reports symmetric differences case-insensitively", () => {
    expect(collectChangedTags(["Travel", "beach"], ["travel", "sunset"]).map((tag) => tag.toLowerCase())).toEqual([
      "beach",
      "sunset"
    ]);
  });

  it("returns nothing when the tag sets are equivalent", () => {
    expect(collectChangedTags([" Trip "], ["trip"])).toEqual([]);
  });
});

describe("tagMutationTouchesFilters", () => {
  it("detects changed tags referenced by applied include/exclude filters", () => {
    expect(tagMutationTouchesFilters(["travel"], ["travel", "city"])).toBe(true);
    expect(tagMutationTouchesFilters(["sunset"], ["TRAVEL"])).toBe(false);
  });

  it("is false without changed tags or without active filters", () => {
    expect(tagMutationTouchesFilters([], ["travel"])).toBe(false);
    expect(tagMutationTouchesFilters(["travel"], [])).toBe(false);
  });
});

describe("favoriteMutationRequiresRefresh", () => {
  it("requires refresh only when an un-favorite happens under favorites-only", () => {
    expect(favoriteMutationRequiresRefresh(true, false)).toBe(true);
    expect(favoriteMutationRequiresRefresh(true, true)).toBe(false);
    expect(favoriteMutationRequiresRefresh(false, false)).toBe(false);
  });
});

describe("bulkTagMutationRequiresRefresh", () => {
  it("requires refresh for updated assets while tag filters are active", () => {
    expect(bulkTagMutationRequiresRefresh(2, ["travel"])).toBe(true);
  });

  it("is false without updates or without active filters", () => {
    expect(bulkTagMutationRequiresRefresh(0, ["travel"])).toBe(false);
    expect(bulkTagMutationRequiresRefresh(3, [])).toBe(false);
  });
});

it.each(["tags", "tags:2"])("invalidates exact-count filter %s after tags change", filterInput => {
  const descriptor = buildFilterDescriptor({ filterInput, mediaKind: "all", favoritesOnly: false });
  expect(tagMutationTouchesFilters(["cat"], descriptor)).toBe(true);
  expect(bulkTagMutationRequiresRefresh(1, descriptor)).toBe(true);
  expect(tagMutationTouchesFilters([], descriptor)).toBe(false);
  expect(bulkTagMutationRequiresRefresh(0, descriptor)).toBe(false);
});
