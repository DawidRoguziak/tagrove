import { renderHook, waitFor } from "@solidjs/testing-library";
import { createEffect } from "solid-js";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createLibrary } from "../../createLibrary";
import { createAssetQuery } from "../createAssetQuery";
import { settle } from "../../../test/settle";
import type { AssetQueryReady, AssetSummary } from "../../../platform/tauri/contracts/assets";

const api = vi.hoisted(() => ({ startAssetQuery: vi.fn(), getAssetQueryPage: vi.fn() }));
vi.mock("../../../platform/tauri/assets", () => api);
vi.mock("../../../platform/tauri/tags", () => ({
  listTags: vi.fn(async () => ({ items: [], total: 0 }))
}));
function row(id: number): AssetSummary {
  return {
    id,
    file_name: `${id}.jpg`,
    preview_path: null,
    kind: "image",
    modified_at: 0,
    width: 10,
    height: 10,
    duration_ms: null,
    thumb_path: null,
    is_favorite: false,
    media_group_key: null,
    media_group_order: null
  };
}
function ready(offset = 0, session_id = 1): AssetQueryReady {
  return { status: "ready", session_id, revision: 1, offset, total: 30, items: [row(offset + 1)] };
}
function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((ok) => {
    resolve = ok;
  });
  return { promise, resolve };
}
function model() {
  return createAssetQuery({
    pageSize: 1,
    resetThumbnailQueue: vi.fn(),
    filters: () => ({
      filterInclude: [],
      filterExclude: [],
      appliedMediaKind: "all",
      appliedFavoritesOnly: false
    })
  });
}
beforeEach(() => {
  vi.resetAllMocks();
  api.startAssetQuery.mockResolvedValue(ready());
  api.getAssetQueryPage.mockImplementation(async (id, offset) => ready(offset, id));
});

describe("query lifetime and synchronous coordination", () => {
  it("keeps the exact twelve-page bound even when demand protects more pages", async () => {
    const { result: query } = renderHook(model);
    await settle(() => query.refresh());
    query.ensureRange(0, 29);
    await settle(() => query.getIdsRangeAsync(0, 29));
    expect(query.assets).toHaveLength(12);
    const missing = Array.from({ length: 30 }, (_, i) => i).find((i) => !query.getAssetAt(i))!;
    expect((await query.getAssetAtAsync(missing))?.id).toBe(missing + 1);
    settle(() => {});
    expect(query.assets).toHaveLength(12);
  });

  it("does not reload evicted pages repeatedly when the library range exceeds its cache", async () => {
    api.getAssetQueryPage.mockImplementation(async (id, offset) => ({
      ...ready(offset, id),
      items: [{ ...row(offset + 1), thumb_path: "synthetic.jpg" }]
    }));
    const { result: library } = renderHook(() => createLibrary({ pageSize: 1 }));
    await library.query.refresh();
    settle(() =>
      library.handleVirtualRangeChange({
        startIndex: 0,
        endIndex: 29,
        visibleStartIndex: 0,
        visibleEndIndex: 29
      })
    );
    await waitFor(() => expect(library.query.loading).toBe(false));
    expect(library.query.assets).toHaveLength(12);
    expect(api.getAssetQueryPage).toHaveBeenCalledTimes(29);
    settle(() => {});
    await Promise.resolve();
    expect(api.getAssetQueryPage).toHaveBeenCalledTimes(29);
  });

  it("updates thumbnail demand when an unchanged viewport's missing page arrives", async () => {
    const pending = deferred<AssetQueryReady>();
    api.getAssetQueryPage.mockReturnValueOnce(pending.promise);
    const { result: library } = renderHook(() => createLibrary({ pageSize: 1 }));
    const demand = vi.spyOn(library.thumbnails, "setGalleryThumbnailDemand");
    await library.query.refresh();
    settle(() =>
      library.handleVirtualRangeChange({
        startIndex: 1,
        endIndex: 1,
        visibleStartIndex: 1,
        visibleEndIndex: 1
      })
    );
    expect(demand).toHaveBeenLastCalledWith([], []);
    pending.resolve(ready(1));
    await waitFor(() => expect(demand).toHaveBeenLastCalledWith([2], []));
    expect(api.getAssetQueryPage).toHaveBeenCalledOnce();
  });

  it("an obsolete request's finalizer cannot remove the replacement request for the same offset", async () => {
    const old = deferred<AssetQueryReady>();
    const current = deferred<AssetQueryReady>();
    api.getAssetQueryPage.mockReturnValueOnce(old.promise).mockReturnValueOnce(current.promise);
    const { result: query } = renderHook(model);
    await query.refresh();
    const first = query.getAssetAtAsync(1);
    api.startAssetQuery.mockResolvedValueOnce(ready(0, 2));
    await query.refresh();
    const second = query.getAssetAtAsync(1);
    old.resolve(ready(1));
    expect(await first).toBeUndefined();
    const third = query.getAssetAtAsync(1);
    expect(api.getAssetQueryPage).toHaveBeenCalledTimes(2);
    current.resolve(ready(1, 2));
    expect((await second)?.id).toBe(2);
    expect((await third)?.id).toBe(2);
  });

  it("disposal discards pending pages, prevents new IPC and cancels complete range reads", async () => {
    const pending = deferred<AssetQueryReady>();
    api.getAssetQueryPage.mockReturnValueOnce(pending.promise);
    const { result: query, cleanup } = renderHook(model);
    await query.refresh();
    const range = query.getIdsRangeAsync(0, 1);
    const rejection = expect(range).rejects.toThrow("cancelled");
    await Promise.resolve();
    cleanup();
    pending.resolve(ready(1));
    await rejection;
    await query.refresh();
    expect(api.startAssetQuery).toHaveBeenCalledOnce();
    expect(query.getAssetAt(1)).toBeUndefined();
  });

  it("does not shift global positions after deletion and rejects incomplete cached ranges", async () => {
    const { result: query } = renderHook(model);
    await query.refresh();
    await query.getAssetAtAsync(1);
    query.patchAssets((rows) => rows.filter((asset) => asset.id !== 1));
    expect(query.getAssetAt(0)).toBeUndefined();
    expect(query.getAssetIndex(1)).toBeNull();
    expect(query.getAssetAt(1)?.id).toBe(2);
    await expect(query.getIdsRangeAsync(0, 1)).rejects.toThrow("cancelled");
  });

  it("reactive slots update after page arrival while loading alone preserves the assets array", async () => {
    const pending = deferred<AssetQueryReady>();
    api.getAssetQueryPage.mockReturnValueOnce(pending.promise);
    const slots = vi.fn();
    const arrays = vi.fn();
    const { result: query } = renderHook(() => {
      const query = model();
      createEffect(() => query.getAssetAt(1), slots);
      createEffect(() => query.assets, arrays);
      return query;
    });
    await settle(() => query.refresh());
    const count = arrays.mock.calls.length;
    const page = query.getAssetAtAsync(1);
    settle(() => {});
    expect(arrays).toHaveBeenCalledTimes(count);
    pending.resolve(ready(1));
    await settle(() => page);
    expect(slots.mock.calls[slots.mock.calls.length - 1]?.[0]?.id).toBe(2);
    expect(arrays).toHaveBeenCalledTimes(count + 1);
  });
});
