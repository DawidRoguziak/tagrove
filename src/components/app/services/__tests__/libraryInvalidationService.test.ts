import { buildFilterDescriptor } from "../filterService";
import { describe, expect, it } from "vitest";
import { favoriteMutationRequiresRefresh, tagMutationRequiresRefresh } from "../libraryInvalidationService";

describe("tag query impact", () => {
  it.each(["", "cat", "-dog", "cat -dog", "tags:2", "gN:album"])("handles none/all under %s", filterInput => {
    const filter = buildFilterDescriptor({ filterInput, mediaKind: "all", favoritesOnly: false });
    expect(tagMutationRequiresRefresh({ type: "none" }, filter)).toBe(false);
    expect(tagMutationRequiresRefresh({ type: "all" }, filter)).toBe(true);
  });
  it.each([
    ["cat", ["cat"], false, true],
    ["-dog", ["dog"], false, true],
    ["cat -dog", ["unrelated"], true, false],
    ["tags:2", ["cat", "dog"], false, false],
    ["tags:2", ["cat"], true, true],
    ["", ["cat"], true, false],
    ["gN:album", ["cat"], true, false],
    ["żółć|%_:'新", ["żółć|%_:'新"], false, true],
  ] as const)("checks actual membership/count changes for %s", (filterInput, names, count, expected) => {
    const filter = buildFilterDescriptor({ filterInput, mediaKind: "image", favoritesOnly: true });
    expect(tagMutationRequiresRefresh({ type: "tags", changed_tags: [...names], tag_count_changed: count }, filter)).toBe(expected);
  });
});

it("preserves the lightbox Favorites refresh rule", () => {
  expect(favoriteMutationRequiresRefresh(true, false)).toBe(true);
  expect(favoriteMutationRequiresRefresh(true, true)).toBe(false);
  expect(favoriteMutationRequiresRefresh(false, false)).toBe(false);
});
