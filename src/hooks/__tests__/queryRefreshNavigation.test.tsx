import { useState } from "react";
import { act, renderHook, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { useSelectionState } from "../useSelectionState";
import type { AssetDetails, AssetQueryPositionResult, AssetSummary } from "../../types";

function asset(id: number): AssetDetails {
  return { id, path: `/photos/${id}.jpg`, file_name: `${id}.jpg`, preview_path: null,
    kind: "image", size_bytes: 10, modified_at: id, width: 10, height: 10,
    duration_ms: null, thumb_path: null, is_favorite: false,
    media_group_key: "existing-group", media_group_order: id, tags: [] };
}
vi.mock("../../api", async () => ({
  ...(await vi.importActual<typeof import("../../api")>("../../api")),
  getAssetDetails: vi.fn(async (id: number) => asset(id)),
  setAssetMediaGroup: vi.fn(async () => {})
}));

it("group save restores next/previous beyond the refreshed first page in the new order", async () => {
  const firstPage = Array.from({ length: 128 }, (_, index) => asset(index + 1));
  const all = Array.from({ length: 400 }, (_, index) => asset(index + 1));
  const reordered = [...all];
  [reordered[201], reordered[202]] = [reordered[202], reordered[201]];
  const position = vi.fn(async () => ({ status: "resolved" as const, index: 200 }));
  const lookup = vi.fn(async (index: number) => reordered[index]);
  const { result } = renderHook(() => {
    const [assets, setAssets] = useState<AssetSummary[]>(all.slice(128, 256));
    const [queryEpoch, setQueryEpoch] = useState(1);
    return useSelectionState({
      assets, setAssets, queryEpoch, appliedFavoritesOnly: false, assetCount: 400,
      refreshKnownTags: async () => [], getAssetAtAsync: lookup, getAssetPosition: position,
      getAssetIndex: (id) => assets.some(item => item.id === id) ? id - 1 : null,
      refresh: async () => { setAssets(firstPage); setQueryEpoch(value => value + 1); }
    });
  });
  act(() => result.current.setSelected(asset(201)));
  await waitFor(() => expect(result.current.selected?.size_bytes).toBe(10));
  // Change order inside the same group; photo 201 remains beyond the first page.
  await act(async () => { await result.current.saveMediaGroup({ key: "existing-group", order: 201.5 }); });
  expect(result.current.selected?.id).toBe(201);
  lookup.mockClear();
  await act(async () => { result.current.handleSelectNext(); });
  expect(result.current.selected?.id).toBe(203);
  expect(position).toHaveBeenCalledWith(201);
  expect(lookup).toHaveBeenCalledWith(201);
  await act(async () => { result.current.handleSelectPrevious(); });
  expect(result.current.selected?.id).toBe(201);
});

function deferred() {
  let resolve!: (value: AssetQueryPositionResult) => void;
  const promise = new Promise<AssetQueryPositionResult>(done => { resolve = done; });
  return { promise, resolve };
}

it.each(["refresh", "selection", "close"])("ignores late position after another %s", async change => {
  const pending = deferred();
  const position = vi.fn().mockReturnValueOnce(pending.promise).mockResolvedValue({ status: "resolved", index: 250 });
  const lookup = vi.fn(async (index: number) => asset(index + 1));
  const { result, rerender } = renderHook(({ epoch, pendingQuery }) => useSelectionState({
    assets: [asset(1)], setAssets: vi.fn(), appliedFavoritesOnly: false, refresh: async () => {},
    refreshKnownTags: async () => [], assetCount: 400, queryEpoch: epoch, queryPending: pendingQuery,
    getAssetIndex: () => null, getAssetPosition: position, getAssetAtAsync: lookup
  }), { initialProps: { epoch: 1, pendingQuery: false } });
  act(() => result.current.selectAsset(asset(201), 200));
  rerender({ epoch: 2, pendingQuery: false });
  expect(result.current.navigationStatus).toBe("resolving");
  lookup.mockClear();
  act(() => result.current.handleSelectNext());
  expect(lookup).not.toHaveBeenCalled();
  if (change === "refresh") {
    rerender({ epoch: 3, pendingQuery: true });
  } else act(() => result.current.selectAsset(change === "close" ? null : asset(10), 9));
  await act(async () => pending.resolve({ status: "resolved", index: 200 }));
  if (change === "refresh") {
    expect(result.current.navigationStatus).toBe("resolving");
    rerender({ epoch: 4, pendingQuery: false });
    await waitFor(() => expect(result.current.navigationStatus).toBe("ready"));
    await act(async () => result.current.handleSelectNext());
    expect(result.current.selected?.id).toBe(252);
  } else if (change === "close") expect(result.current.selected).toBeNull();
  else {
    await act(async () => result.current.handleSelectNext());
    expect(result.current.selected?.id).toBe(11);
  }
});

it.each([0, 399])("keeps a filtered-out photo open with %s results and retries a failed lookup", async assetCount => {
  const position = vi.fn().mockRejectedValueOnce(new Error("busy")).mockResolvedValueOnce({ status: "missing" });
  const lookup = vi.fn(async (index: number) => asset(index + 1));
  const { result, rerender } = renderHook(({ epoch }) => useSelectionState({
    assets: assetCount ? [asset(1)] : [], setAssets: vi.fn(), appliedFavoritesOnly: false, refresh: async () => {},
    refreshKnownTags: async () => [], assetCount, queryEpoch: epoch,
    getAssetIndex: () => null, getAssetPosition: position, getAssetAtAsync: lookup
  }), { initialProps: { epoch: 1 } });
  act(() => result.current.selectAsset(asset(201), 200));
  rerender({ epoch: 2 });
  await waitFor(() => expect(result.current.navigationStatus).toBe("failed"));
  act(() => result.current.retryNavigation());
  await waitFor(() => expect(result.current.navigationStatus).toBe("missing"));
  lookup.mockClear();
  act(() => result.current.handleSelectPrevious());
  expect(lookup).not.toHaveBeenCalled();
  expect(result.current.selected?.id).toBe(201);
});
