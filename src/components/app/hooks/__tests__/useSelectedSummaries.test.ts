import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useSelectedSummaries } from "../useSelectedSummaries";
import type { AssetSummary } from "../../../../types";
const api = vi.hoisted(() => ({ getAssetSummariesByIds: vi.fn() }));
vi.mock("../../../../api", () => api);
function summary(id: number): AssetSummary {
  return {
    id,
    file_name: `${id}.jpg`,
    preview_path: null,
    kind: "image",
    modified_at: id,
    width: 10,
    height: 10,
    duration_ms: null,
    thumb_path: null,
    is_favorite: false,
    media_group_key: null,
    media_group_order: null
  };
}
describe("selected summary hydration", () => {
  beforeEach(() => {
    api.getAssetSummariesByIds.mockReset();
  });
  it("retains selected metadata after eviction and removes it on deselection", async () => {
    const ids = new Set([1, 2]);
    api.getAssetSummariesByIds.mockResolvedValue([summary(2)]);
    const { result, rerender } = renderHook(
      ({ ids, cached }) => useSelectedSummaries(ids, cached, 0),
      { initialProps: { ids, cached: [summary(1)] } }
    );
    await waitFor(() => expect(result.current.ready).toBe(true));
    expect(api.getAssetSummariesByIds).toHaveBeenCalledWith([2]);
    rerender({ ids, cached: [] });
    expect(result.current.selected.map((item) => item.id)).toEqual([1, 2]);
    rerender({ ids: new Set([2]), cached: [] });
    expect(result.current.selected.map((item) => item.id)).toEqual([2]);
  });
  it("reports missing records, retries explicitly, and ignores obsolete hydration", async () => {
    let complete!: (items: AssetSummary[]) => void;
    api.getAssetSummariesByIds
      .mockReturnValueOnce(
        new Promise((resolve) => {
          complete = resolve;
        })
      )
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([summary(2)]);
    const { result, rerender } = renderHook(({ ids }) => useSelectedSummaries(ids, [], 0), {
      initialProps: { ids: new Set([1]) }
    });
    rerender({ ids: new Set([2]) });
    await waitFor(() => expect(result.current.failed).toBe(true));
    await act(async () => complete([summary(1)]));
    expect(result.current.selected).toEqual([]);
    expect(result.current.ready).toBe(false);
    act(() => result.current.retry());
    await waitFor(() => expect(result.current.ready).toBe(true));
    expect(result.current.selected.map((item) => item.id)).toEqual([2]);
  });
  it("limits IPC to four concurrent batches of at most 256 IDs and stops after unmount", async () => {
    const completions: Array<() => void> = [];
    api.getAssetSummariesByIds.mockImplementation(
      (ids: number[]) =>
        new Promise((resolve) => {
          completions.push(() => resolve(ids.map(summary)));
        })
    );
    const ids = new Set(Array.from({ length: 1025 }, (_, index) => index + 1));
    const { result, unmount } = renderHook(() => useSelectedSummaries(ids, [], 0));
    expect(api.getAssetSummariesByIds).toHaveBeenCalledTimes(4);
    for (const [batch] of api.getAssetSummariesByIds.mock.calls)
      expect(batch.length).toBeLessThanOrEqual(256);
    await act(async () => completions[0]!());
    expect(api.getAssetSummariesByIds).toHaveBeenCalledTimes(5);
    expect(result.current.selected).toHaveLength(256);
    unmount();
    await act(async () => completions.slice(1).forEach((complete) => complete()));
    expect(api.getAssetSummariesByIds).toHaveBeenCalledTimes(5);
  });
});
