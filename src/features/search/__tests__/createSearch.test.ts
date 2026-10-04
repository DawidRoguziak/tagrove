import { renderHook } from "@solidjs/testing-library";
import { flush } from "solid-js";
import { describe, expect, it, vi } from "vitest";
import { createSearch } from "../createSearch";

describe("Solid search commands", () => {
  it("separates draft from applied state and explicitly resubmits an unchanged normalized filter", async () => {
    const apply = vi.fn().mockResolvedValue(undefined),
      { result } = renderHook(() => createSearch(apply));
    result.change("Cat dog cat");
    flush();
    expect(apply).not.toHaveBeenCalled();
    expect(result.applied.include).toEqual([]);
    await result.submit();
    flush();
    expect(result.applied.include).toEqual(["cat", "dog"]);
    const descriptor = result.applied;
    await result.applyInput("DOG CAT");
    flush();
    expect(result.applied).toBe(descriptor);
    expect(apply).toHaveBeenCalledTimes(2);
  });
  it.each([
    [
      '"tags" -"-holiday"',
      { filterInclude: ["tags"], filterExclude: ["-holiday"], metaFilter: null }
    ],
    [
      "tags:2",
      { filterInclude: [], filterExclude: [], metaFilter: { type: "hasNoTags", tagCount: 2 } }
    ],
    [
      "gN:Trip 2026",
      {
        filterInclude: [],
        filterExclude: [],
        metaFilter: { type: "groupName", groupName: "trip 2026" }
      }
    ]
  ])("uses the existing parser for %s", async (input, expected) => {
    const apply = vi.fn().mockResolvedValue(undefined),
      { result } = renderHook(() => createSearch(apply));
    await result.applyInput(input);
    expect(apply).toHaveBeenLastCalledWith(expect.objectContaining(expected));
  });
  it("composes input, kind and favorites in the same batch", async () => {
    const apply = vi.fn().mockResolvedValue(undefined),
      { result } = renderHook(() => createSearch(apply));
    result.change("tags:3");
    const kind = result.applyKind("video"),
      favorite = result.applyFavorites(true);
    await Promise.all([kind, favorite]);
    flush();
    expect(apply).toHaveBeenLastCalledWith(
      expect.objectContaining({
        metaFilter: { type: "hasNoTags", tagCount: 3 },
        appliedMediaKind: "video",
        appliedFavoritesOnly: true
      })
    );
    expect(result.mediaKind).toBe("video");
    expect(result.favoritesOnly).toBe(true);
  });
  it.each([
    "gN:",
    "tags:-1",
    '"unfinished'
  ])("rejects %s, retains applied values and resets through clear", async (input) => {
    const apply = vi.fn().mockResolvedValue(undefined),
      { result } = renderHook(() => createSearch(apply));
    await result.applyInput("cat");
    const valid = apply.mock.lastCall?.[0];
    result.change(input);
    await result.applyFavorites(true);
    flush();
    expect(result.validationError).not.toBeNull();
    expect(apply).toHaveBeenCalledOnce();
    expect(valid.filterInclude).toEqual(["cat"]);
    await result.clear();
    flush();
    expect(result.filterInput).toBe("");
    expect(result.mediaKind).toBe("all");
    expect(result.favoritesOnly).toBe(false);
    expect(result.validationError).toBeNull();
  });
});
