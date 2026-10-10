import { renderHook, waitFor } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createLibrary } from "../../../library/createLibrary";
import { createLightbox } from "../createLightbox";
import { settle } from "../../../test/settle";
import { asset, summary, deferred, saved } from "./fixtures";
import type { AssetDetails, AssetSummary, AssetQueryPositionResult } from "../../../platform/tauri/contracts/assets";
const api = vi.hoisted(() => ({
  startAssetQuery: vi.fn(),
  getAssetQueryPage: vi.fn(),
  getAssetQueryPosition: vi.fn(),
  getAssetDetails: vi.fn(),
  setAssetTags: vi.fn(),
  setAssetFavorite: vi.fn(),
  setAssetMediaGroup: vi.fn(),
  deleteAsset: vi.fn(),
  listTags: vi.fn()
}));
vi.mock("../../../platform/tauri/assets", () => api);
vi.mock("../../../platform/tauri/tags", () => api);
const ready = (ids = [1, 2, 3], total = ids.length) => ({
  status: "ready",
  session_id: 1,
  revision: 1,
  offset: 0,
  total,
  items: ids.map((id) => summary(id))
});
beforeEach(() => {
  vi.resetAllMocks();
  api.startAssetQuery.mockResolvedValue(ready());
  api.getAssetDetails.mockImplementation(async (id: number) => asset(id));
  api.setAssetTags.mockImplementation(async (id: number, tags: string[]) => saved(id, tags));
  api.listTags.mockResolvedValue({ items: ["cat", "dog"], total: 2 });
  api.getAssetQueryPosition.mockResolvedValue({ status: "resolved", index: 0 });
  api.deleteAsset.mockResolvedValue({
    removed_assets: 1,
    removed_thumbnails: 0,
    source_status: "deleted",
    revision: 2,
    recovery_path: null
  });
});
async function setup(open = true) {
  const view = renderHook(() => {
    const library = createLibrary();
    return { library, box: createLightbox(library) };
  });
  await settle(() => view.result.library.query.refresh());
  if (open) {
    settle(() => view.result.box.select(summary(1)));
    await waitFor(() => expect(view.result.box.selected?.path).toBe("/synthetic/1.png"));
  }
  return { ...view, ...view.result };
}
const settled = () =>
  settle(async () => {
    for (let i = 0; i < 12; i++) await Promise.resolve();
  });
describe("Solid lightbox selection and metadata", () => {
  it("opens a summary immediately and blocks replacements until full tags arrive", async () => {
    const detail = deferred<AssetDetails>();
    api.getAssetDetails.mockReturnValue(detail.promise);
    const { box } = await setup(false);
    settle(() => {
      box.select(summary(1));
      box.saveTags(["unsafe"]);
    });
    expect(box.selected?.path).toBeNull();
    expect(box.tagDetailsLoading).toBe(true);
    expect(api.setAssetTags).not.toHaveBeenCalled();
    await settle(async () => detail.resolve(asset(1)));
    await settled();
    expect(box.tagEditor).toEqual(["cat"]);
    expect(box.tagEditingDisabled).toBe(false);
  });
  it.each([false, true])("sends no mutation without a selection, previously opened=%s", async (opened) => {
    const { box } = await setup(opened);
    settle(box.close);
    await settle(async () => {
      box.saveTags(["ignored"]);
      await box.toggleFavorite();
      await box.saveMediaGroup();
      expect(await box.deleteSelected()).toBeNull();
    });
    expect(api.setAssetTags).not.toHaveBeenCalled();
    expect(api.setAssetFavorite).not.toHaveBeenCalled();
    expect(api.setAssetMediaGroup).not.toHaveBeenCalled();
    expect(api.deleteAsset).not.toHaveBeenCalled();
  });
  it("ignores details after selection changes and close", async () => {
    const first = deferred<AssetDetails>(),
      second = deferred<AssetDetails>();
    api.getAssetDetails.mockImplementation((id) => (id === 1 ? first.promise : second.promise));
    const { box } = await setup(false);
    settle(() => {
      box.select(summary(1));
      box.select(summary(2));
    });
    await settle(async () => first.resolve(asset(1)));
    expect(box.selected?.id).toBe(2);
    settle(box.close);
    await settle(async () => second.resolve(asset(2)));
    expect(box.selected).toBeNull();
  });
  it("prefetches only adjacent details and reuses complete cached records", async () => {
    const { box } = await setup();
    await settled();
    expect(api.getAssetDetails.mock.calls.map((c) => c[0]).sort()).toEqual([1, 2, 3]);
    settle(() => {
      box.close();
      box.select(summary(2));
    });
    expect(box.selected?.path).toBe("/synthetic/2.png");
    expect(api.getAssetDetails).toHaveBeenCalledTimes(3);
  });
  it("does not prefetch in a single-result query", async () => {
    api.startAssetQuery.mockResolvedValue(ready([1]));
    await setup();
    await settled();
    expect(api.getAssetDetails).toHaveBeenCalledTimes(1);
  });
  it("keeps prefetching neighbors when navigation selects cached details", async () => {
    api.startAssetQuery.mockResolvedValue(ready([1, 2, 3, 4, 5]));
    const { box, library } = await setup();
    await settled();
    expect(api.getAssetDetails.mock.calls.map(([id]) => id).sort()).toEqual([1, 2, 5]);

    settle(box.next);
    await settled();
    expect(box.selected?.id).toBe(2);
    expect(library.details.get(3)?.id).toBe(3);
    expect(api.getAssetDetails.mock.calls.map(([id]) => id).sort()).toEqual([1, 2, 3, 5]);

    settle(box.next);
    await settled();
    expect(box.selected?.id).toBe(3);
    expect(library.details.get(4)?.id).toBe(4);
    expect(api.getAssetDetails.mock.calls.map(([id]) => id).sort()).toEqual([1, 2, 3, 4, 5]);
  });
  it("keeps failed details and failed saves independent", async () => {
    api.getAssetDetails.mockRejectedValue(new Error("read failed"));
    const { box, library } = await setup(false);
    settle(() => box.select(summary(1)));
    await settled();
    expect(box.assetDetailsFailed).toBe(true);
    expect(box.tagDetailsFailed).toBe(true);
    await settle(() => library.mutations.saveTags(1, ["bulk"]));
    expect(box.assetDetailsFailed).toBe(true);
    expect(box.tagDetailsFailed).toBe(false);
    expect(box.tagEditingDisabled).toBe(false);
    api.setAssetTags.mockRejectedValueOnce(new Error("write failed"));
    settle(() => box.saveTags(["bulk", "draft"]));
    await settled();
    expect(box.tagFailed).toBe(true);
    expect(box.tagEditor).toEqual(["bulk", "draft"]);
    api.getAssetDetails.mockResolvedValue(asset(1, { tags: ["bulk"] }));
    settle(box.retryDetails);
    await settled();
    expect(box.assetDetailsFailed).toBe(false);
    expect(box.tagFailed).toBe(true);
    settle(box.retryTags);
    await settled();
    expect(box.tagFailed).toBe(false);
    expect(box.tagEditor).toEqual(["bulk", "draft"]);
  });
  it("coalesces same-turn saves and serializes across close and reopen", async () => {
    const { box } = await setup();
    const write = deferred<ReturnType<typeof saved>>();
    api.setAssetTags.mockReturnValueOnce(write.promise);
    settle(() => {
      box.saveTags(["cat", "a"]);
      box.saveTags(["cat", "a", "b"]);
      box.saveTags(["cat", "final"]);
      box.close();
      box.select(summary(1));
    });
    expect(api.setAssetTags).toHaveBeenCalledTimes(1);
    await settle(async () => write.resolve(saved(1, ["cat", "a"])));
    await settled();
    expect(api.setAssetTags).toHaveBeenCalledTimes(2);
    expect(api.setAssetTags).toHaveBeenLastCalledWith(1, ["cat", "final"]);
    expect(box.tagEditor).toEqual(["cat", "final"]);
  });
  it("retains the newest queued replacement after failure", async () => {
    const { box } = await setup();
    const write = deferred<ReturnType<typeof saved>>();
    api.setAssetTags.mockReturnValueOnce(write.promise);
    settle(() => {
      box.saveTags(["a"]);
      box.saveTags(["b"]);
    });
    await settle(async () => write.reject(new Error("failed")));
    await settled();
    expect(box.tagFailed).toBe(true);
    expect(box.tagEditor).toEqual(["b"]);
    expect(api.setAssetTags).toHaveBeenCalledTimes(1);
    settle(() => {
      box.close();
      box.select(summary(1));
      box.retryTags();
    });
    await settled();
    expect(api.setAssetTags).toHaveBeenLastCalledWith(1, ["b"]);
  });
  it("rebases failed intent onto an external winner before Retry", async () => {
    const { box, library } = await setup();
    const token = library.tags.beginMutation(1)!;
    settle(() => box.saveTags(["mine"]));
    await settled();
    expect(box.tagFailed).toBe(true);
    expect(api.setAssetTags).not.toHaveBeenCalled();
    settle(() => library.tags.settleMutation(token, ["cat", "external"]));
    expect(box.tagEditor).toEqual(["external", "mine"]);
    settle(box.retryTags);
    await settled();
    expect(api.setAssetTags).toHaveBeenLastCalledWith(1, ["external", "mine"]);
  });
  it("retains retry intent while maintenance denies writes", async () => {
    const { box, library } = await setup();
    const hold = deferred<void>();
    const maintenance = library.tags.runWithMutationBarrier(() => hold.promise);
    settle(() => box.saveTags(["draft"]));
    await settled();
    expect(box.tagFailed).toBe(true);
    expect(api.setAssetTags).not.toHaveBeenCalled();
    hold.resolve();
    await maintenance;
    settle(box.retryTags);
    await settled();
    expect(box.tagEditor).toEqual(["draft"]);
  });
  it("re-reads complete details that predate a favorite mutation", async () => {
    const stale = deferred<AssetDetails>();
    api.getAssetDetails.mockReturnValueOnce(stale.promise);
    const { box, library } = await setup(false);
    settle(() => box.select(summary(1)));
    await settle(() => library.mutations.setFavorite(1, true));
    api.getAssetDetails.mockResolvedValue(asset(1, { is_favorite: true }));
    await settle(async () => stale.resolve(asset(1)));
    await settled();
    expect(box.selected?.is_favorite).toBe(true);
    expect(box.selected?.path).toBe("/synthetic/1.png");
    expect(api.getAssetDetails.mock.calls.filter((c) => c[0] === 1)).toHaveLength(2);
  });
  it("waits for a pending write before refetching details", async () => {
    const first = deferred<AssetDetails>(),
      write = deferred<void>();
    api.getAssetDetails.mockReturnValueOnce(first.promise);
    api.setAssetFavorite.mockReturnValueOnce(write.promise);
    const { box, library } = await setup(false);
    settle(() => box.select(summary(1)));
    const saving = library.mutations.setFavorite(1, true);
    await settle(async () => first.resolve(asset(1)));
    await settled();
    expect(api.getAssetDetails).toHaveBeenCalledTimes(1);
    api.getAssetDetails.mockResolvedValue(asset(1, { is_favorite: true }));
    write.resolve();
    await settle(() => saving);
    await settled();
    expect(box.selected?.is_favorite).toBe(true);
  });
  it("preserves empty canonical tags and cached favorite changes on reopen", async () => {
    const { box, library } = await setup();
    await settled();
    await settle(() => library.mutations.setFavorite(1, true));
    settle(() => box.saveTags([]));
    await settled();
    settle(() => {
      box.close();
      box.select(summary(1));
    });
    expect(box.tagEditor).toEqual([]);
    expect(box.selected?.is_favorite).toBe(true);
  });
  it("closes on identity reset and never publishes an obsolete save", async () => {
    const { box, library } = await setup();
    const write = deferred<ReturnType<typeof saved>>();
    api.setAssetTags.mockReturnValueOnce(write.promise);
    settle(() => box.saveTags(["old"]));
    settle(library.resetIdentity);
    expect(box.selected).toBeNull();
    api.getAssetDetails.mockResolvedValue(asset(1, { tags: ["new"] }));
    settle(() => box.select(summary(1)));
    await settled();
    await settle(async () => write.resolve(saved(1, ["old"])));
    await settled();
    expect(box.tagEditor).toEqual(["new"]);
  });
  it("preserves independently dirty group fields through hydration and tag changes", async () => {
    const detail = deferred<AssetDetails>();
    api.getAssetDetails.mockReturnValueOnce(detail.promise);
    const { box } = await setup(false);
    settle(() => {
      box.select(summary(1));
      box.setMediaGroupKeyEditor("draft");
    });
    await settle(async () =>
      detail.resolve(asset(1, { media_group_key: "canonical", media_group_order: 4 }))
    );
    await settled();
    expect(box.mediaGroupKeyEditor).toBe("draft");
    expect(box.mediaGroupOrderEditor).toBe("4");
    settle(() => {
      box.setMediaGroupOrderEditor("07");
      box.saveTags(["new"]);
    });
    await settled();
    expect(box.mediaGroupKeyEditor).toBe("draft");
    expect(box.mediaGroupOrderEditor).toBe("07");
    await settle(box.saveMediaGroup);
    expect(api.setAssetMediaGroup).toHaveBeenLastCalledWith(1, "draft", 7);
  });
  it("excludes duplicate metadata commands synchronously and reports contention", async () => {
    const { box, library } = await setup();
    const write = deferred<void>();
    api.setAssetFavorite.mockReturnValueOnce(write.promise);
    const one = box.toggleFavorite(),
      two = box.toggleFavorite();
    expect(api.setAssetFavorite).toHaveBeenCalledTimes(1);
    write.resolve();
    await settle(() => Promise.all([one, two]));
    const token = library.tags.beginMutation(1)!;
    await settle(box.saveMediaGroup);
    expect(box.groupFailed).toBe(true);
    expect(api.setAssetMediaGroup).not.toHaveBeenCalled();
    library.tags.settleMutation(token);
  });
  it("closes only the deleted selection and preserves a committed delete on refresh failure", async () => {
    const { box } = await setup();
    api.startAssetQuery.mockRejectedValue(new Error("refresh"));
    const result = await settle(box.deleteSelected);
    expect(result?.source_status).toBe("deleted");
    expect(box.selected).toBeNull();
  });
  it("keeps failed deletion selected", async () => {
    const { box } = await setup();
    api.deleteAsset.mockRejectedValue(new Error("preserved"));
    await expect(box.deleteSelected()).rejects.toThrow("preserved");
    expect(box.selected?.id).toBe(1);
  });
  it("accumulates rapid navigation and rejects reverse-order responses", async () => {
    const { box, library } = await setup();
    await settled();
    const a = deferred<AssetSummary | undefined>(),
      b = deferred<AssetSummary | undefined>();
    vi.spyOn(library.query, "getAssetAtAsync")
      .mockReturnValueOnce(a.promise)
      .mockReturnValueOnce(b.promise);
    settle(() => {
      box.next();
      box.next();
    });
    await settle(async () => b.resolve(summary(3)));
    await settle(async () => a.resolve(summary(2)));
    await settled();
    expect(box.selected?.id).toBe(3);
  });
  it("rapid Right then Left returns to the original target", async () => {
    const { box, library } = await setup();
    await settled();
    const a = deferred<AssetSummary | undefined>(),
      b = deferred<AssetSummary | undefined>();
    vi.spyOn(library.query, "getAssetAtAsync")
      .mockReturnValueOnce(a.promise)
      .mockReturnValueOnce(b.promise);
    settle(() => {
      box.next();
      box.previous();
    });
    await settle(async () => b.resolve(summary(1)));
    await settle(async () => a.resolve(summary(2)));
    expect(box.selected?.id).toBe(1);
  });
  it("wraps and rolls back failed navigation so the next key retries", async () => {
    const { box, library } = await setup();
    await settled();
    settle(box.previous);
    await settled();
    expect(box.selected?.id).toBe(3);
    const lookup = vi
      .spyOn(library.query, "getAssetAtAsync")
      .mockRejectedValueOnce(new Error("page"));
    settle(box.next);
    await settled();
    expect(box.selected?.id).toBe(3);
    settle(box.next);
    await settled();
    expect(box.selected?.id).toBe(1);
    expect(lookup).toHaveBeenCalledWith(0);
  });
  it("resolves a selected item beyond page one after query reorder", async () => {
    const { box, library } = await setup();
    await settled();
    api.startAssetQuery.mockResolvedValue(ready([9], 500));
    api.getAssetQueryPosition.mockResolvedValue({ status: "resolved", index: 300 });
    await settle(() => library.query.refresh());
    await settled();
    expect(box.navigationStatus).toBe("ready");
    const lookup = vi.spyOn(library.query, "getAssetAtAsync").mockResolvedValue(summary(7));
    settle(box.next);
    await settled();
    expect(lookup).toHaveBeenCalledWith(301);
    expect(box.selected?.id).toBe(7);
  });
  it("keeps missing selection open and retries failed position lookup", async () => {
    const { box, library } = await setup();
    api.startAssetQuery.mockResolvedValue(ready([], 0));
    api.getAssetQueryPosition.mockResolvedValue({ status: "missing" });
    await settle(() => library.query.refresh());
    await settled();
    expect(box.navigationStatus).toBe("missing");
    expect(box.selected?.id).toBe(1);
    api.getAssetQueryPosition.mockRejectedValue(new Error("position"));
    await settle(box.retryNavigation);
    await settled();
    expect(box.navigationStatus).toBe("failed");
    api.startAssetQuery.mockResolvedValue(ready());
    await settle(box.retryNavigation);
    expect(box.navigationStatus).toBe("ready");
  });
  it("rejects retired position and navigation reads after close", async () => {
    const { box, library } = await setup();
    await settled();
    const position = deferred<AssetQueryPositionResult>();
    api.startAssetQuery.mockResolvedValue(ready([9], 100));
    api.getAssetQueryPosition.mockReturnValue(position.promise);
    await settle(() => library.query.refresh());
    settle(box.close);
    await settle(async () => position.resolve({ status: "resolved", index: 99 }));
    settle(box.next);
    expect(box.selected).toBeNull();
  });
  it("drops query-scoped cached details and refetches on reselect", async () => {
    const { box, library } = await setup();
    await settled();
    const count = api.getAssetDetails.mock.calls.filter((c) => c[0] === 1).length;
    await settle(() => library.query.refresh());
    settle(() => box.select(summary(1)));
    await settled();
    expect(api.getAssetDetails.mock.calls.filter((c) => c[0] === 1)).toHaveLength(count + 1);
  });
  it("does not publish details after owner disposal", async () => {
    const detail = deferred<AssetDetails>();
    api.getAssetDetails.mockReturnValueOnce(detail.promise);
    const { box, cleanup } = await setup(false);
    settle(() => box.select(summary(1)));
    cleanup();
    detail.resolve(asset(1));
    await settled();
    expect(box.selected?.path).toBeNull();
  });
  it("crosses a real page boundary and wraps using the query total", async () => {
    api.startAssetQuery.mockResolvedValue(
      ready(
        Array.from({ length: 128 }, (_, index) => index + 1),
        260
      )
    );
    api.getAssetQueryPage.mockImplementation(async (_session, offset: number) => ({
      ...ready(
        Array.from({ length: Math.min(128, 260 - offset) }, (_, index) => offset + index + 1),
        260
      ),
      offset
    }));
    const { box, library } = await setup(false);
    settle(() => box.select(summary(128), 127));
    await settled();
    settle(box.next);
    await settled();
    expect(box.selected?.id).toBe(129);
    settle(() => box.select(summary(1), 0));
    settle(box.previous);
    await settled();
    expect(box.selected?.id).toBe(260);
    expect(library.query.assets.length).toBeLessThanOrEqual(260);
  });
  it("invalidates a navigation target immediately when refresh starts in the same turn", async () => {
    const { box, library } = await setup();
    await settled();
    const lookup = vi.spyOn(library.query, "getAssetAtAsync");
    const refresh = deferred<ReturnType<typeof ready>>();
    api.startAssetQuery.mockReturnValue(refresh.promise);
    const pending = library.query.refresh();
    box.next();
    expect(lookup).not.toHaveBeenCalled();
    refresh.resolve(ready([3, 2, 1]));
    await settle(() => pending);
    await settled();
    settle(box.next);
    await settled();
    expect(box.selected?.id).toBe(3);
  });
  it("synchronizes loaded summary changes without losing complete path or tags", async () => {
    const { box, library } = await setup();
    settle(() =>
      library.query.patchAssets((rows) =>
        rows.map((row) =>
          row.id === 1 ? { ...row, file_name: "renamed.png", media_group_key: "new-group" } : row
        )
      )
    );
    expect(box.selected?.file_name).toBe("renamed.png");
    expect(box.selected?.path).toBe("/synthetic/1.png");
    expect(box.tagEditor).toEqual(["cat"]);
  });
});
