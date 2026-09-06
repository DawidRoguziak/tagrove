import { act, renderHook } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { useAppSearchFilters } from "../useAppSearchFilters";

describe("useAppSearchFilters", () => {
  it("refreshes when submitting unchanged filters", async () => {
    const refresh = vi.fn();
    const { result } = renderHook(() => useAppSearchFilters());

    await act(async () => {
      await result.current.handleSearchSubmit(refresh);
    });

    expect(refresh).toHaveBeenCalledTimes(1);
    expect(result.current.appliedParsedFilter.metaFilter).toBeNull();
  });

  it("applies exact tag-count and group-name metatags", async () => {
    const { result } = renderHook(() => useAppSearchFilters());

    act(() => {
      result.current.handleFilterChange("tags:2");
      result.current.setMediaKind("video");
      result.current.setFavoritesOnly(true);
    });

    await act(async () => {
      await result.current.handleSearchSubmit();
    });

    expect(result.current.appliedParsedFilter.metaFilter).toEqual({
      type: "hasNoTags",
      tagCount: 2
    });
    expect(result.current.appliedMediaKind).toBe("video");
    expect(result.current.appliedFavoritesOnly).toBe(true);
    expect(result.current.appliedMetaFilterKey).toBe("hasNoTags:2");

    act(() => {
      result.current.handleFilterChange("gN:Trip-2026");
    });

    await act(async () => {
      await result.current.handleSearchSubmit();
    });

    expect(result.current.appliedParsedFilter.metaFilter).toEqual({
      type: "groupName",
      groupName: "Trip-2026"
    });
    expect(result.current.appliedMetaFilterKey).toBe("groupName:trip-2026");
  });

  it("keeps invalid filters unapplied and clear resets state", async () => {
    const refresh = vi.fn();
    const { result } = renderHook(() => useAppSearchFilters());

    act(() => {
      result.current.handleFilterChange("gN:");
      result.current.setMediaKind("gif");
      result.current.setFavoritesOnly(true);
    });

    await act(async () => {
      await result.current.handleSearchSubmit(refresh);
    });

    expect(result.current.filterValidationError).toBe("groupNameMissingValue");
    expect(result.current.appliedParsedFilter.metaFilter).toBeNull();
    expect(refresh).not.toHaveBeenCalled();

    await act(async () => {
      await result.current.handleClearSearch(refresh);
    });

    expect(result.current.filterInput).toBe("");
    expect(result.current.mediaKind).toBe("all");
    expect(result.current.favoritesOnly).toBe(false);
    expect(result.current.filterValidationError).toBeNull();
    expect(refresh).toHaveBeenCalledTimes(1);
  });

  it("atomically applies filter input and parsed search state", async () => {
    const refresh = vi.fn();
    const { result } = renderHook(() => useAppSearchFilters());

    await act(async () => {
      await result.current.applyFilterInputAndSubmit("banana Apple -cherry", refresh);
    });

    expect(result.current.filterInput).toBe("banana Apple -cherry");
    expect(result.current.appliedParsedFilter.include).toEqual(["banana", "apple"]);
    expect(result.current.appliedParsedFilter.exclude).toEqual(["cherry"]);
    expect(result.current.filterValidationError).toBeNull();
    expect(refresh).not.toHaveBeenCalled();
  });

  it("refreshes when atomically applying the same filter input again", async () => {
    const refresh = vi.fn();
    const { result } = renderHook(() => useAppSearchFilters());

    await act(async () => {
      await result.current.applyFilterInputAndSubmit("cat -dog");
    });

    await act(async () => {
      await result.current.applyFilterInputAndSubmit("cat -dog", refresh);
    });

    expect(result.current.filterInput).toBe("cat -dog");
    expect(result.current.appliedParsedFilter.include).toEqual(["cat"]);
    expect(result.current.appliedParsedFilter.exclude).toEqual(["dog"]);
    expect(refresh).toHaveBeenCalledTimes(1);
  });

  it("does not apply invalid filter input through atomic submit", async () => {
    const refresh = vi.fn();
    const { result } = renderHook(() => useAppSearchFilters());

    act(() => {
      result.current.handleFilterChange("cat");
    });

    await act(async () => {
      await result.current.handleSearchSubmit();
      await result.current.applyFilterInputAndSubmit("gN:", refresh);
    });

    expect(result.current.filterInput).toBe("gN:");
    expect(result.current.appliedParsedFilter.include).toEqual(["cat"]);
    expect(result.current.appliedParsedFilter.metaFilter).toBeNull();
    expect(result.current.filterValidationError).toBe("groupNameMissingValue");
    expect(refresh).not.toHaveBeenCalled();
  });
  it("validates toolbar controls without replacing the applied query", async () => {
    const { result } = renderHook(() => useAppSearchFilters());
    await act(() => result.current.applyFilterInputAndSubmit("cat"));
    const previous = result.current.appliedQueryKey;
    act(() => result.current.handleFilterChange("cat tags:2"));
    const refresh = vi.fn();
    await act(() => result.current.applyMediaKindAndSubmit("video", refresh));
    await act(() => result.current.applyFavoritesOnlyAndSubmit(true, refresh));
    expect(result.current.appliedQueryKey).toBe(previous);
    expect(result.current.filterValidationError).not.toBeNull();
    expect(refresh).not.toHaveBeenCalled();
  });

  it("distinguishes pipe-containing tags and refreshes normalized unchanged queries", async () => {
    const { result } = renderHook(() => useAppSearchFilters());
    await act(() => result.current.applyFilterInputAndSubmit("a|b c"));
    const first = result.current.appliedQueryKey;
    await act(() => result.current.applyFilterInputAndSubmit("a b|c"));
    expect(result.current.appliedQueryKey).not.toBe(first);
    const refresh = vi.fn();
    await act(() => result.current.applyFilterInputAndSubmit("B|C A", refresh));
    expect(refresh).toHaveBeenCalledOnce();
  });
});
